//! Daemon lifecycle for the desktop app.
//!
//! The daemon runs **inside this process** (see [`crate::embedded_daemon`]), so
//! the app is the single owner of the control port, the persisted state, and the
//! provider child processes. The failure modes that come from having two owners
//! are structurally gone: no second daemon racing for `127.0.0.1:4765`, no
//! shared api token rotated out from under a running client, and no "UI says
//! stopped while the public URL still answers" split brain.
//!
//! A daemon that is already answering on the configured address is never fought
//! over — it is adopted as [`DaemonOwnership::External`]. That is the path for a
//! headless `tunnelmuxd` a user runs explicitly for the CLI.

use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use tunnelmux_control_client::{ControlClientConfig, TunnelmuxControlClient};

use crate::{embedded_daemon, embedded_daemon::EmbeddedDaemon, settings::GuiSettings};

/// Who owns the daemon the GUI is currently talking to.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DaemonOwnership {
    /// A daemon started by somebody else: a headless `tunnelmuxd`, a launch
    /// agent, or a remote host. The app only talks to it, never stops it.
    External,
    /// The daemon hosted in this very process. The app stops it on exit.
    Embedded,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DaemonConnectionState {
    pub ownership: DaemonOwnership,
    pub last_error: Option<String>,
}

#[derive(Default)]
pub struct DaemonRuntimeState {
    pub connection: DaemonConnectionState,
    /// The in-process daemon. Holding it here is what ties the daemon's lifetime
    /// to the app's lifetime.
    pub embedded: Option<EmbeddedDaemon>,
    pub bootstrapping: bool,
}

impl std::fmt::Debug for DaemonRuntimeState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DaemonRuntimeState")
            .field("connection", &self.connection)
            .field("embedded", &self.embedded.is_some())
            .field("bootstrapping", &self.bootstrapping)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonStatusSnapshot {
    pub ownership: DaemonOwnership,
    pub bootstrapping: bool,
    pub connected: bool,
    pub message: Option<String>,
}

const BOOTSTRAPPING_MESSAGE: &str = "Starting local TunnelMux…";
const LOCK_POISONED: &str = "daemon runtime state should lock";

pub fn mark_external_daemon() -> DaemonConnectionState {
    DaemonConnectionState {
        ownership: DaemonOwnership::External,
        last_error: None,
    }
}

pub fn mark_embedded_daemon() -> DaemonConnectionState {
    DaemonConnectionState {
        ownership: DaemonOwnership::Embedded,
        last_error: None,
    }
}

pub fn mark_unavailable_daemon(error: Option<String>) -> DaemonConnectionState {
    DaemonConnectionState {
        ownership: DaemonOwnership::Unavailable,
        last_error: error,
    }
}

pub fn daemon_status_snapshot(
    connection: &DaemonConnectionState,
    bootstrapping: bool,
) -> DaemonStatusSnapshot {
    let message = if bootstrapping && connection.ownership == DaemonOwnership::Unavailable {
        Some(BOOTSTRAPPING_MESSAGE.to_string())
    } else {
        match connection.ownership {
            DaemonOwnership::Embedded => Some("TunnelMux is running inside this app.".to_string()),
            DaemonOwnership::External => Some(
                "Using a TunnelMux daemon that is already running on this machine.".to_string(),
            ),
            DaemonOwnership::Unavailable => connection.last_error.clone(),
        }
    };

    DaemonStatusSnapshot {
        ownership: connection.ownership,
        bootstrapping,
        connected: connection.ownership != DaemonOwnership::Unavailable,
        message,
    }
}

pub fn read_daemon_status(state: &Arc<Mutex<DaemonRuntimeState>>) -> DaemonStatusSnapshot {
    let runtime = state.lock().expect(LOCK_POISONED);
    daemon_status_snapshot(&runtime.connection, runtime.bootstrapping)
}

/// Stop the daemon this app started.
///
/// A daemon we merely adopted is deliberately left running: it belongs to
/// whoever started it, and killing it would break their CLI or their machine's
/// launch configuration.
pub async fn shutdown_embedded_daemon(state: &Arc<Mutex<DaemonRuntimeState>>) {
    let daemon = {
        let mut runtime = state.lock().expect(LOCK_POISONED);
        let daemon = runtime.embedded.take();
        runtime.connection = DaemonConnectionState::default();
        runtime.bootstrapping = false;
        daemon
    };

    if let Some(daemon) = daemon {
        daemon.shutdown().await;
    }
}

/// Make sure the GUI has a daemon to talk to, and point `settings` at it.
///
/// Adopts an already-answering daemon when there is one; otherwise starts the
/// embedded daemon. On success `settings.base_url` and `settings.token` are
/// aligned with the daemon that is actually listening, so every other command
/// keeps using its plain HTTP client unchanged.
pub async fn ensure_local_daemon(
    runtime_state: &Arc<Mutex<DaemonRuntimeState>>,
    settings: &mut GuiSettings,
) -> anyhow::Result<DaemonStatusSnapshot> {
    let client = TunnelmuxControlClient::new(ControlClientConfig::new(
        settings.base_url.clone(),
        settings.token.clone(),
    ));
    if !client.is_loopback() && client.token().is_none() {
        return Err(anyhow!(
            "remote TunnelMux connections require an explicit bearer token"
        ));
    }

    // An already-answering daemon always wins.
    if client.health().await.is_ok() {
        let mut runtime = runtime_state.lock().expect(LOCK_POISONED);
        runtime.connection = mark_external_daemon();
        runtime.bootstrapping = false;
        return Ok(daemon_status_snapshot(&runtime.connection, false));
    }

    // Single-flight: a concurrent second caller waits for the first attempt
    // instead of racing it for the same port.
    let already_starting = {
        let mut runtime = runtime_state.lock().expect(LOCK_POISONED);
        if runtime.bootstrapping {
            true
        } else {
            runtime.bootstrapping = true;
            runtime.connection = mark_unavailable_daemon(None);
            false
        }
    };
    if already_starting {
        return wait_for_bootstrap_completion(runtime_state).await;
    }

    let started = embedded_daemon::start(settings).await;

    let mut runtime = runtime_state.lock().expect(LOCK_POISONED);
    runtime.bootstrapping = false;

    match started {
        Ok(started) => {
            settings.base_url = started.base_url.clone();
            settings.token = started.token.clone();
            runtime.embedded = Some(started.daemon);
            runtime.connection = mark_embedded_daemon();
            Ok(daemon_status_snapshot(&runtime.connection, false))
        }
        Err(error) => {
            runtime.embedded = None;
            runtime.connection = mark_unavailable_daemon(Some(error.to_string()));
            Err(error)
        }
    }
}

async fn wait_for_bootstrap_completion(
    state: &Arc<Mutex<DaemonRuntimeState>>,
) -> anyhow::Result<DaemonStatusSnapshot> {
    for _ in 0..40 {
        let snapshot = {
            let runtime = state.lock().expect(LOCK_POISONED);
            if runtime.bootstrapping {
                None
            } else {
                Some(daemon_status_snapshot(&runtime.connection, false))
            }
        };

        if let Some(snapshot) = snapshot {
            return if snapshot.connected {
                Ok(snapshot)
            } else {
                Err(anyhow!(
                    "{}",
                    snapshot
                        .message
                        .unwrap_or_else(|| "local TunnelMux is unavailable".to_string())
                ))
            };
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    Err(anyhow!("local TunnelMux daemon is still starting"))
}

pub fn find_binary_on_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .flat_map(|path| {
            [
                path.join(binary_name),
                path.join(format!("{binary_name}.exe")),
            ]
        })
        .find(|candidate| candidate.exists())
}

pub fn resolve_provider_binary(binary_name: &str) -> Option<PathBuf> {
    resolve_binary_in_dirs(
        binary_name,
        provider_binary_search_dirs(
            env::var_os("PATH").as_deref(),
            std::iter::empty::<PathBuf>(),
        ),
        env::var_os("PATHEXT").as_deref(),
    )
}

pub(crate) fn resolve_binary_in_dirs(
    binary_name: &str,
    search_dirs: impl IntoIterator<Item = PathBuf>,
    pathext: Option<&OsStr>,
) -> Option<PathBuf> {
    if Path::new(binary_name).components().count() > 1 {
        return binary_is_executable(Path::new(binary_name)).then(|| PathBuf::from(binary_name));
    }

    let candidates = executable_candidate_names(binary_name, pathext);
    search_dirs
        .into_iter()
        .flat_map(|path| candidates.iter().map(move |candidate| path.join(candidate)))
        .find(|candidate| binary_is_executable(candidate))
}

pub(crate) fn provider_binary_search_dirs(
    path_var: Option<&OsStr>,
    extra_search_dirs: impl IntoIterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(path_var) = path_var {
        dirs.extend(env::split_paths(path_var));
    }

    dirs.extend(extra_search_dirs);

    #[cfg(unix)]
    {
        dirs.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            PathBuf::from("/snap/bin"),
        ]);
    }

    dirs.sort();
    dirs.dedup();
    dirs
}

fn executable_candidate_names(binary_name: &str, pathext: Option<&OsStr>) -> Vec<String> {
    let mut candidates = vec![binary_name.to_string()];

    if Path::new(binary_name).extension().is_none() {
        let pathext = pathext
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .or_else(|| cfg!(windows).then_some(".COM;.EXE;.BAT;.CMD"));

        if let Some(pathext) = pathext {
            for extension in pathext.split(';').filter(|value| !value.is_empty()) {
                candidates.push(format!("{binary_name}{extension}"));
            }
        }
    }

    candidates
}

fn binary_is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

/// Derive a `host:port` listen address from a URL the GUI stores.
///
/// Used for both the control-plane base URL and a tunnel's gateway target URL so
/// the embedded daemon binds exactly where the rest of the app expects it.
pub(crate) fn listen_addr_from_url(url: &str) -> anyhow::Result<String> {
    let parsed = url::Url::parse(url)
        .with_context(|| format!("invalid URL for embedded daemon startup: {url}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("URL is missing a host: {url}"))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| anyhow!("URL is missing a port: {url}"))?;
    Ok(format!("{host}:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn provider_binary_resolution_uses_common_directories() {
        let temp_dir = prepare_temp_dir();
        let tools_dir = temp_dir.join("tools");
        std::fs::create_dir_all(&tools_dir).expect("tool dir should be created");
        let provider = tools_dir.join("cloudflared");
        write_fake_binary(&provider);

        let resolved = resolve_binary_in_dirs("cloudflared", vec![tools_dir.clone()], None)
            .expect("provider should resolve from common search dir");

        assert_eq!(resolved, provider);
    }

    #[test]
    fn provider_binary_resolution_uses_pathext_candidates() {
        let temp_dir = prepare_temp_dir();
        let tools_dir = temp_dir.join("tools");
        std::fs::create_dir_all(&tools_dir).expect("tool dir should be created");
        let provider = tools_dir.join("cloudflared.CMD");
        write_fake_binary(&provider);

        let resolved = resolve_binary_in_dirs(
            "cloudflared",
            vec![tools_dir.clone()],
            Some(OsStr::new(".EXE;.CMD")),
        )
        .expect("provider should resolve from PATHEXT candidate");

        assert_eq!(resolved, provider);
    }

    #[test]
    fn listen_addr_is_derived_from_control_base_url() {
        assert_eq!(
            listen_addr_from_url("http://127.0.0.1:4765").expect("default base URL should parse"),
            "127.0.0.1:4765"
        );
    }

    #[test]
    fn listen_addr_is_derived_from_gateway_target_url() {
        assert_eq!(
            listen_addr_from_url("http://127.0.0.1:48080").expect("gateway URL should parse"),
            "127.0.0.1:48080"
        );
    }

    #[test]
    fn daemon_manager_marks_embedded_daemon_ownership() {
        let state = mark_embedded_daemon();

        assert_eq!(state.ownership, DaemonOwnership::Embedded);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn daemon_manager_marks_external_daemon_ownership() {
        let state = mark_external_daemon();

        assert_eq!(state.ownership, DaemonOwnership::External);
    }

    #[test]
    fn daemon_status_snapshot_distinguishes_embedded_and_external() {
        let embedded = daemon_status_snapshot(&mark_embedded_daemon(), false);
        let external = daemon_status_snapshot(&mark_external_daemon(), false);

        assert!(embedded.connected);
        assert_eq!(embedded.ownership, DaemonOwnership::Embedded);
        assert!(external.connected);
        assert_eq!(external.ownership, DaemonOwnership::External);
        assert_ne!(embedded.message, external.message);
    }

    #[test]
    fn daemon_status_snapshot_reports_bootstrapping_state() {
        let snapshot = daemon_status_snapshot(&DaemonConnectionState::default(), true);

        assert_eq!(snapshot.ownership, DaemonOwnership::Unavailable);
        assert!(snapshot.bootstrapping);
        assert!(!snapshot.connected);
        assert_eq!(snapshot.message.as_deref(), Some(BOOTSTRAPPING_MESSAGE));
    }

    #[test]
    fn daemon_status_snapshot_surfaces_start_failure_message() {
        let connection = mark_unavailable_daemon(Some("port is taken".to_string()));

        let snapshot = daemon_status_snapshot(&connection, false);

        assert!(!snapshot.connected);
        assert_eq!(snapshot.message.as_deref(), Some("port is taken"));
    }

    #[tokio::test]
    async fn shutdown_leaves_an_adopted_daemon_alone() {
        let state = Arc::new(Mutex::new(DaemonRuntimeState {
            connection: mark_external_daemon(),
            embedded: None,
            bootstrapping: false,
        }));

        shutdown_embedded_daemon(&state).await;

        let runtime = state.lock().expect(LOCK_POISONED);
        assert!(runtime.embedded.is_none());
    }

    fn prepare_temp_dir() -> PathBuf {
        let path = next_temp_dir();
        if path.exists() {
            std::fs::remove_dir_all(&path).expect("stale temp dir should be removed");
        }
        std::fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn write_fake_binary(path: &Path) {
        std::fs::write(
            path,
            "#!/bin/sh
exit 0
",
        )
        .expect("fake binary should be written");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = std::fs::metadata(path)
                .expect("fake binary metadata should load")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions)
                .expect("fake binary permissions should update");
        }
    }

    fn next_temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "tunnelmux-gui-daemon-manager-{}",
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
