//! The TunnelMux daemon the desktop app hosts inside its own process.
//!
//! The app links the daemon as a library and runs it on the Tauri async
//! runtime. Nothing is spawned, so there is no child process to leak, no PID to
//! track, and no second owner able to bind the control port. The GUI keeps
//! talking to it over loopback HTTP exactly as before, which means the whole
//! command layer stays unchanged.

use std::fmt;

use anyhow::{Context, anyhow};
use tunnelmuxd::{DaemonArgs, DaemonHandle};

use crate::{
    daemon_manager,
    settings::{DEFAULT_GUI_GATEWAY_TARGET_URL, GuiSettings},
};

/// Absolute path to a provider binary, or the bare name when nothing was found.
///
/// The GUI can be launched from Finder, where `PATH` is a minimal system list,
/// so resolving here (PATH + Homebrew + `/usr/local/bin`) and handing the daemon
/// absolute paths keeps provider discovery identical to when the daemon was a
/// separate child process.
fn provider_binary(name: &str) -> String {
    daemon_manager::resolve_provider_binary(name)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| name.to_string())
}

/// A daemon owning the control port on behalf of this app.
pub struct EmbeddedDaemon {
    handle: DaemonHandle,
    control_url: String,
    token: Option<String>,
}

impl EmbeddedDaemon {
    /// Loopback URL the GUI should use for control-plane calls.
    pub fn control_url(&self) -> &str {
        &self.control_url
    }

    /// Bearer token the control plane expects.
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Stop the tunnels this daemon owns, then stop serving.
    pub async fn shutdown(self) {
        self.handle.shutdown().await;
    }
}

impl fmt::Debug for EmbeddedDaemon {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EmbeddedDaemon")
            .field("control_url", &self.control_url)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Result of starting an embedded daemon.
pub struct EmbeddedStart {
    pub daemon: EmbeddedDaemon,
    /// Control-plane URL that is actually bound.
    pub base_url: String,
    /// Token that URL expects.
    pub token: Option<String>,
}

/// Start the daemon in this process and wait until it is actually serving.
///
/// Returns as soon as both the control-plane and gateway listeners are bound,
/// so callers never have to poll for readiness.
pub async fn start(settings: &GuiSettings) -> anyhow::Result<EmbeddedStart> {
    let listen = daemon_manager::listen_addr_from_url(&settings.base_url)?;
    let gateway_target_url = settings
        .current_tunnel()
        .map(|tunnel| tunnel.gateway_target_url.as_str())
        .unwrap_or(DEFAULT_GUI_GATEWAY_TARGET_URL);
    let gateway_listen = daemon_manager::listen_addr_from_url(gateway_target_url)?;

    let args = DaemonArgs {
        listen,
        gateway_listen,
        cloudflared_bin: provider_binary("cloudflared"),
        ngrok_bin: provider_binary("ngrok"),
        api_token: settings
            .token
            .clone()
            .filter(|token| !token.trim().is_empty()),
        ..DaemonArgs::default()
    };

    let handle = tunnelmuxd::start(args)
        .await
        .map_err(describe_start_failure)
        .context("the embedded TunnelMux daemon could not start")?;

    let base_url = format!("http://{}", handle.control_addr());
    let token = handle.api_token().map(str::to_string);

    Ok(EmbeddedStart {
        daemon: EmbeddedDaemon {
            handle,
            control_url: base_url.clone(),
            token: token.clone(),
        },
        base_url,
        token,
    })
}

/// Turn a raw bind failure into something a user can act on.
fn describe_start_failure(error: anyhow::Error) -> anyhow::Error {
    let text = format!("{error:#}");
    if text.contains("Address already in use") || text.contains("os error 48") {
        return anyhow!(
            "another process is already using the TunnelMux control port, so the embedded daemon could not start: {text}"
        );
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::TunnelProfileSettings;

    #[test]
    fn provider_binary_falls_back_to_the_bare_name() {
        // A name that cannot exist anywhere on the machine.
        assert_eq!(
            provider_binary("tunnelmux-nonexistent-provider-binary"),
            "tunnelmux-nonexistent-provider-binary"
        );
    }

    #[test]
    fn describe_start_failure_explains_addr_in_use() {
        let error = describe_start_failure(anyhow!(
            "failed to bind control plane on 127.0.0.1:4765: Address already in use (os error 48)"
        ));

        assert!(
            error
                .to_string()
                .contains("already using the TunnelMux control port"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn describe_start_failure_passes_other_errors_through() {
        let error = describe_start_failure(anyhow!("invalid control listen address: nope"));

        assert_eq!(error.to_string(), "invalid control listen address: nope");
    }

    #[test]
    fn settings_gateway_target_url_drives_the_gateway_listener() {
        let settings = GuiSettings {
            current_tunnel_id: Some("primary".to_string()),
            tunnels: vec![TunnelProfileSettings {
                id: "primary".to_string(),
                gateway_target_url: "http://127.0.0.1:49123".to_string(),
                ..TunnelProfileSettings::default()
            }],
            ..GuiSettings::default()
        };

        let tunnel = settings.current_tunnel().expect("tunnel should be present");
        assert_eq!(
            daemon_manager::listen_addr_from_url(&tunnel.gateway_target_url)
                .expect("gateway URL should parse"),
            "127.0.0.1:49123"
        );
    }
}
