use crate::settings::GuiSettings;
use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Manager, Runtime};
use tunnelmux_control_client::{ControlClientConfig, TunnelmuxControlClient};
use url::Url;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DaemonOwnership {
    External,
    Managed,
    #[default]
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonBinarySource {
    Bundled,
    Path,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedDaemonBinary {
    pub path: PathBuf,
    pub source: DaemonBinarySource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DaemonConnectionState {
    pub ownership: DaemonOwnership,
    pub managed_pid: Option<u32>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DaemonStartupAction {
    UseExistingConnection,
    StartManagedDaemon(ResolvedDaemonBinary),
    Unavailable,
}

pub trait ManagedDaemonHandle {
    fn id(&self) -> u32;
    fn kill(&mut self) -> std::io::Result<()>;
}

#[derive(Debug)]
pub struct ManagedDaemonProcess<H = std::process::Child> {
    pub binary: ResolvedDaemonBinary,
    pub handle: H,
}

#[derive(Debug, Default)]
pub struct DaemonRuntimeState {
    pub connection: DaemonConnectionState,
    pub managed: Option<ManagedDaemonProcess>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonStatusSnapshot {
    pub ownership: DaemonOwnership,
    pub connected: bool,
    pub message: Option<String>,
}

pub fn resolve_daemon_binary_paths(
    bundled_binary: Option<&Path>,
    path_binary: Option<&Path>,
) -> anyhow::Result<ResolvedDaemonBinary> {
    if let Some(path) = bundled_binary.filter(|candidate| candidate.exists()) {
        return Ok(ResolvedDaemonBinary {
            path: path.to_path_buf(),
            source: DaemonBinarySource::Bundled,
        });
    }

    if let Some(path) = path_binary.filter(|candidate| candidate.exists()) {
        return Ok(ResolvedDaemonBinary {
            path: path.to_path_buf(),
            source: DaemonBinarySource::Path,
        });
    }

    Err(anyhow!(
        "tunnelmuxd binary could not be found in bundled resources or PATH"
    ))
}

pub fn determine_daemon_startup_action(
    probe_connected: bool,
    resolved_binary: Option<ResolvedDaemonBinary>,
) -> DaemonStartupAction {
    if probe_connected {
        DaemonStartupAction::UseExistingConnection
    } else if let Some(binary) = resolved_binary {
        DaemonStartupAction::StartManagedDaemon(binary)
    } else {
        DaemonStartupAction::Unavailable
    }
}

pub fn mark_external_daemon() -> DaemonConnectionState {
    DaemonConnectionState {
        ownership: DaemonOwnership::External,
        managed_pid: None,
        last_error: None,
    }
}

pub fn mark_managed_daemon(pid: u32) -> DaemonConnectionState {
    DaemonConnectionState {
        ownership: DaemonOwnership::Managed,
        managed_pid: Some(pid),
        last_error: None,
    }
}

pub fn mark_unavailable_daemon(error: Option<String>) -> DaemonConnectionState {
    DaemonConnectionState {
        ownership: DaemonOwnership::Unavailable,
        managed_pid: None,
        last_error: error,
    }
}

pub fn stop_managed_daemon<H: ManagedDaemonHandle>(
    connection: &DaemonConnectionState,
    managed: Option<&mut ManagedDaemonProcess<H>>,
) -> anyhow::Result<bool> {
    if connection.ownership != DaemonOwnership::Managed {
        return Ok(false);
    }

    let Some(process) = managed else {
        return Ok(false);
    };

    process
        .handle
        .kill()
        .map_err(|error| anyhow!("failed to stop managed tunnelmuxd: {error}"))?;
    Ok(true)
}

impl ManagedDaemonHandle for std::process::Child {
    fn id(&self) -> u32 {
        self.id()
    }

    fn kill(&mut self) -> std::io::Result<()> {
        std::process::Child::kill(self)
    }
}

pub fn daemon_status_snapshot(connection: &DaemonConnectionState) -> DaemonStatusSnapshot {
    let message = match connection.ownership {
        DaemonOwnership::Managed => {
            Some("Connected to a GUI-managed local TunnelMux daemon.".to_string())
        }
        DaemonOwnership::External => {
            Some("Using an existing local TunnelMux daemon.".to_string())
        }
        DaemonOwnership::Unavailable => connection.last_error.clone(),
    };

    DaemonStatusSnapshot {
        ownership: connection.ownership,
        connected: connection.ownership != DaemonOwnership::Unavailable,
        message,
    }
}

pub fn read_daemon_status(state: &Arc<Mutex<DaemonRuntimeState>>) -> DaemonStatusSnapshot {
    let runtime = state.lock().expect("daemon runtime state should lock");
    daemon_status_snapshot(&runtime.connection)
}

pub fn stop_managed_daemon_in_state(
    state: &Arc<Mutex<DaemonRuntimeState>>,
) -> anyhow::Result<bool> {
    let mut runtime = state.lock().expect("daemon runtime state should lock");
    let connection = runtime.connection.clone();
    let stopped = stop_managed_daemon(&connection, runtime.managed.as_mut())?;
    if let Some(process) = runtime.managed.as_mut() {
        let _ = process.handle.wait();
    }
    if stopped {
        runtime.managed = None;
        runtime.connection = DaemonConnectionState::default();
    }
    Ok(stopped)
}

pub async fn ensure_local_daemon<R: Runtime>(
    app: &AppHandle<R>,
    runtime_state: &Arc<Mutex<DaemonRuntimeState>>,
    settings: &GuiSettings,
) -> anyhow::Result<DaemonStatusSnapshot> {
    let client = TunnelmuxControlClient::new(ControlClientConfig::new(
        settings.base_url.clone(),
        settings.token.clone(),
    ));

    if client.health().await.is_ok() {
        let mut runtime = runtime_state
            .lock()
            .expect("daemon runtime state should lock");
        runtime.managed = None;
        runtime.connection = mark_external_daemon();
        return Ok(daemon_status_snapshot(&runtime.connection));
    }

    let existing_managed_pid = {
        let mut runtime = runtime_state
            .lock()
            .expect("daemon runtime state should lock");
        if let Some(process) = runtime.managed.as_mut() {
            if process.handle.try_wait()?.is_none() {
                Some(process.handle.id())
            } else {
                runtime.managed = None;
                None
            }
        } else {
            None
        }
    };

    if let Some(pid) = existing_managed_pid {
        wait_for_daemon_ready(settings).await?;
        let mut runtime = runtime_state
            .lock()
            .expect("daemon runtime state should lock");
        runtime.connection = mark_managed_daemon(pid);
        return Ok(daemon_status_snapshot(&runtime.connection));
    }

    let bundled_binary = resolve_bundled_daemon_binary(app);
    let path_binary = find_binary_on_path("tunnelmuxd");
    let resolved =
        resolve_daemon_binary_paths(bundled_binary.as_deref(), path_binary.as_deref())?;
    let mut managed = spawn_managed_daemon(&resolved, settings)?;

    if let Err(error) = wait_for_daemon_ready(settings).await {
        let _ = managed.handle.kill();
        let _ = managed.handle.wait();
        let mut runtime = runtime_state
            .lock()
            .expect("daemon runtime state should lock");
        runtime.managed = None;
        runtime.connection = mark_unavailable_daemon(Some(error.to_string()));
        return Err(error);
    }

    let pid = managed.handle.id();
    let mut runtime = runtime_state
        .lock()
        .expect("daemon runtime state should lock");
    runtime.connection = mark_managed_daemon(pid);
    runtime.managed = Some(managed);
    Ok(daemon_status_snapshot(&runtime.connection))
}

pub fn resolve_bundled_daemon_binary<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("tunnelmuxd"));
        candidates.push(resource_dir.join("tunnelmuxd-x86_64-apple-darwin"));
        candidates.push(resource_dir.join("tunnelmuxd-aarch64-apple-darwin"));
        candidates.push(resource_dir.join("tunnelmuxd-x86_64-unknown-linux-gnu"));
        candidates.push(resource_dir.join("tunnelmuxd-x86_64-pc-windows-msvc.exe"));
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join("tunnelmuxd"));
            candidates.push(parent.join("tunnelmuxd.exe"));
        }
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    candidates.push(workspace_root.join("target/debug/tunnelmuxd"));
    candidates.push(workspace_root.join("target/debug/tunnelmuxd.exe"));
    candidates.push(workspace_root.join("target/release/tunnelmuxd"));
    candidates.push(workspace_root.join("target/release/tunnelmuxd.exe"));

    candidates.into_iter().find(|path| path.exists())
}

pub fn find_binary_on_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .flat_map(|path| [path.join(binary_name), path.join(format!("{binary_name}.exe"))])
        .find(|candidate| candidate.exists())
}

pub fn resolve_provider_binary(binary_name: &str) -> Option<PathBuf> {
    resolve_binary_in_dirs(binary_name, common_provider_search_dirs())
}

fn resolve_binary_in_dirs(
    binary_name: &str,
    search_dirs: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    search_dirs
        .into_iter()
        .flat_map(|path| [path.join(binary_name), path.join(format!("{binary_name}.exe"))])
        .find(|candidate| candidate.exists())
}

fn common_provider_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(path_var) = env::var_os("PATH") {
        dirs.extend(env::split_paths(&path_var));
    }

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

pub fn spawn_managed_daemon(
    binary: &ResolvedDaemonBinary,
    settings: &GuiSettings,
) -> anyhow::Result<ManagedDaemonProcess> {
    let mut command = build_managed_daemon_command(
        binary,
        settings,
        resolve_provider_binary("cloudflared"),
        resolve_provider_binary("ngrok"),
    )?;

    let child = command
        .spawn()
        .with_context(|| format!("failed to spawn tunnelmuxd from {}", binary.path.display()))?;

    Ok(ManagedDaemonProcess {
        binary: binary.clone(),
        handle: child,
    })
}

fn build_managed_daemon_command(
    binary: &ResolvedDaemonBinary,
    settings: &GuiSettings,
    cloudflared_binary: Option<PathBuf>,
    ngrok_binary: Option<PathBuf>,
) -> anyhow::Result<Command> {
    let listen = control_listen_arg(&settings.base_url)?;
    let gateway = gateway_listen_arg(&settings.gateway_target_url)?;

    let mut command = Command::new(&binary.path);
    command
        .arg("--listen")
        .arg(listen)
        .arg("--gateway-listen")
        .arg(gateway)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Some(token) = settings.token.as_deref() {
        command.arg("--api-token").arg(token);
    }

    if let Some(path) = cloudflared_binary {
        command.arg("--cloudflared-bin").arg(path);
    }
    if let Some(path) = ngrok_binary {
        command.arg("--ngrok-bin").arg(path);
    }

    Ok(command)
}

pub async fn wait_for_daemon_ready(settings: &GuiSettings) -> anyhow::Result<()> {
    let client = TunnelmuxControlClient::new(ControlClientConfig::new(
        settings.base_url.clone(),
        settings.token.clone(),
    ));

    for _ in 0..40 {
        if client.health().await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    Err(anyhow!(
        "local TunnelMux daemon did not become ready at {}",
        settings.base_url
    ))
}

fn control_listen_arg(base_url: &str) -> anyhow::Result<String> {
    let url = Url::parse(base_url)
        .with_context(|| format!("invalid GUI base URL for daemon startup: {base_url}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("base URL is missing a host: {base_url}"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("base URL is missing a port: {base_url}"))?;
    Ok(format!("{host}:{port}"))
}

fn gateway_listen_arg(target_url: &str) -> anyhow::Result<String> {
    let url = Url::parse(target_url)
        .with_context(|| format!("invalid gateway target URL for daemon startup: {target_url}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("gateway target URL is missing a host: {target_url}"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("gateway target URL is missing a port: {target_url}"))?;
    Ok(format!("{host}:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn daemon_manager_prefers_bundled_binary_over_path() {
        let temp_dir = prepare_temp_dir();
        let bundled = temp_dir.join("bundled-tunnelmuxd");
        let path_binary = temp_dir.join("path-tunnelmuxd");
        std::fs::write(&bundled, "").expect("bundled binary fixture should write");
        std::fs::write(&path_binary, "").expect("path binary fixture should write");

        let resolved = resolve_daemon_binary_paths(Some(&bundled), Some(&path_binary))
            .expect("bundled binary should resolve first");

        assert_eq!(resolved.path, bundled);
        assert_eq!(resolved.source, DaemonBinarySource::Bundled);
    }

    #[test]
    fn daemon_manager_uses_path_when_bundled_binary_missing() {
        let temp_dir = prepare_temp_dir();
        let bundled = temp_dir.join("missing-bundled-tunnelmuxd");
        let path_binary = temp_dir.join("path-tunnelmuxd");
        std::fs::write(&path_binary, "").expect("path binary fixture should write");

        let resolved = resolve_daemon_binary_paths(Some(&bundled), Some(&path_binary))
            .expect("path binary should resolve when bundled is absent");

        assert_eq!(resolved.path, path_binary);
        assert_eq!(resolved.source, DaemonBinarySource::Path);
    }

    #[test]
    fn provider_binary_resolution_uses_common_directories() {
        let temp_dir = prepare_temp_dir();
        let tools_dir = temp_dir.join("tools");
        std::fs::create_dir_all(&tools_dir).expect("tool dir should be created");
        let provider = tools_dir.join("cloudflared");
        std::fs::write(&provider, "").expect("provider fixture should write");

        let resolved = resolve_binary_in_dirs("cloudflared", vec![tools_dir.clone()])
            .expect("provider should resolve from common search dir");

        assert_eq!(resolved, provider);
    }

    #[test]
    fn managed_daemon_command_passes_resolved_provider_paths() {
        let binary = ResolvedDaemonBinary {
            path: PathBuf::from("/tmp/tunnelmuxd"),
            source: DaemonBinarySource::Path,
        };
        let settings = GuiSettings::default();
        let command = build_managed_daemon_command(
            &binary,
            &settings,
            Some(PathBuf::from("/opt/homebrew/bin/cloudflared")),
            Some(PathBuf::from("/opt/homebrew/bin/ngrok")),
        )
        .expect("command should build");

        let args: Vec<String> = command
            .get_args()
            .map(|value| value.to_string_lossy().to_string())
            .collect();

        assert!(args.windows(2).any(|items| {
            items == ["--cloudflared-bin", "/opt/homebrew/bin/cloudflared"]
        }));
        assert!(args.windows(2).any(|items| items == ["--ngrok-bin", "/opt/homebrew/bin/ngrok"]));
    }

    #[test]
    fn daemon_manager_reports_missing_binary_clearly() {
        let error = resolve_daemon_binary_paths(None::<&Path>, None::<&Path>)
            .expect_err("missing daemon binary should fail");

        assert!(
            error.to_string().contains("tunnelmuxd binary"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn daemon_manager_detects_external_daemon_without_spawn() {
        let action = determine_daemon_startup_action(true, None);

        assert_eq!(action, DaemonStartupAction::UseExistingConnection);
    }

    #[test]
    fn daemon_manager_marks_gui_spawned_daemon_as_managed() {
        let state = mark_managed_daemon(4242);

        assert_eq!(state.ownership, DaemonOwnership::Managed);
        assert_eq!(state.managed_pid, Some(4242));
    }

    #[test]
    fn daemon_manager_shutdown_stops_managed_daemon() {
        let connection = mark_managed_daemon(4242);
        let binary = ResolvedDaemonBinary {
            path: PathBuf::from("/tmp/tunnelmuxd"),
            source: DaemonBinarySource::Path,
        };
        let mut process = ManagedDaemonProcess {
            binary,
            handle: FakeManagedChild::default(),
        };

        let stopped =
            stop_managed_daemon(&connection, Some(&mut process)).expect("shutdown should work");

        assert!(stopped);
        assert!(process.handle.killed);
    }

    #[test]
    fn daemon_manager_shutdown_skips_external_daemon() {
        let connection = mark_external_daemon();
        let binary = ResolvedDaemonBinary {
            path: PathBuf::from("/tmp/tunnelmuxd"),
            source: DaemonBinarySource::Path,
        };
        let mut process = ManagedDaemonProcess {
            binary,
            handle: FakeManagedChild::default(),
        };

        let stopped =
            stop_managed_daemon(&connection, Some(&mut process)).expect("shutdown should work");

        assert!(!stopped);
        assert!(!process.handle.killed);
    }

    #[derive(Debug, Default)]
    struct FakeManagedChild {
        killed: bool,
    }

    impl ManagedDaemonHandle for FakeManagedChild {
        fn id(&self) -> u32 {
            4242
        }

        fn kill(&mut self) -> std::io::Result<()> {
            self.killed = true;
            Ok(())
        }
    }

    fn prepare_temp_dir() -> PathBuf {
        let path = next_temp_dir();
        if path.exists() {
            std::fs::remove_dir_all(&path).expect("stale temp dir should be removed");
        }
        std::fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn next_temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "tunnelmux-gui-daemon-manager-{}",
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
