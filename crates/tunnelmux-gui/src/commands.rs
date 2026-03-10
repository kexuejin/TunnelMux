use crate::daemon_manager::{self, DaemonStatusSnapshot};
use crate::provider_installer::{
    ProviderInstallManifestEntry, ProviderInstallSource, ProviderInstallStatus,
    download_provider_archive_bytes, install_provider_from_bytes, load_provider_install_statuses,
    provider_manifest_entry_for_current_platform, save_provider_install_statuses,
    tools_root_from_base_dir,
};
use crate::settings::{GuiSettings, load_settings_from_dir, save_settings_to_dir};
use crate::state::GuiAppState;
use crate::view_models::{
    DiagnosticsSummaryVm, LogTailVm, ProviderAvailabilitySnapshotVm, ProviderAvailabilityVm,
    ProviderStatusVm, RouteFormData, RouteWorkspaceSnapshot, TunnelProfileVm, TunnelWorkspaceVm,
    UpstreamHealthVm,
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::Manager;
use tunnelmux_control_client::{ControlClientConfig, TunnelmuxControlClient};
use tunnelmux_core::{HealthResponse, TunnelProvider, TunnelStartRequest, TunnelStatus};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaveSettingsResult {
    pub settings: GuiSettings,
    pub daemon_status: DaemonStatusSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsSaveReconnectMode {
    EnsureLocalDaemon,
    ProbeConnection,
}

#[tauri::command]
pub async fn ensure_local_daemon(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<DaemonStatusSnapshot, String> {
    bootstrap_local_daemon_with_state(&app, state.inner()).await
}

pub async fn bootstrap_local_daemon<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<DaemonStatusSnapshot, String> {
    let state = app.state::<GuiAppState>();
    bootstrap_local_daemon_with_state(app, state.inner()).await
}

async fn bootstrap_local_daemon_with_state<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &GuiAppState,
) -> Result<DaemonStatusSnapshot, String> {
    let settings_dir = resolve_settings_dir(app, state)?;
    let settings = load_settings_from_dir(&settings_dir).map_err(command_error)?;

    match startup_reconnect_mode(&settings) {
        SettingsSaveReconnectMode::EnsureLocalDaemon => {
            daemon_manager::ensure_local_daemon(app, &state.daemon_runtime, &settings)
                .await
                .map_err(command_error)
        }
        SettingsSaveReconnectMode::ProbeConnection => {
            probe_connection_from_settings_dir(&settings_dir)
                .await
                .map(daemon_status_snapshot_from_connection)
        }
    }
}

#[tauri::command]
pub fn daemon_connection_state(
    state: tauri::State<'_, GuiAppState>,
) -> Result<DaemonStatusSnapshot, String> {
    Ok(daemon_manager::read_daemon_status(&state.daemon_runtime))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardSnapshot {
    pub connected: bool,
    pub settings: GuiSettings,
    pub health: Option<HealthResponse>,
    pub tunnel: Option<TunnelStatus>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StartTunnelInput {
    pub provider: TunnelProvider,
    pub target_url: String,
    pub auto_restart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelProfileInput {
    pub id: Option<String>,
    pub name: String,
    pub provider: TunnelProvider,
    pub gateway_target_url: String,
    pub auto_restart: bool,
    pub cloudflared_tunnel_token: Option<String>,
    pub ngrok_authtoken: Option<String>,
    pub ngrok_domain: Option<String>,
}

#[tauri::command]
pub fn load_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<GuiSettings, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    load_settings_from_dir(&settings_dir).map_err(command_error)
}

#[tauri::command]
pub async fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    settings: GuiSettings,
) -> Result<SaveSettingsResult, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    save_settings_to_dir(&settings_dir, &settings).map_err(command_error)?;
    let settings = load_settings_from_dir(&settings_dir).map_err(command_error)?;
    let daemon_status =
        reconnect_after_settings_save(&app, state.inner(), &settings_dir, &settings).await;
    Ok(SaveSettingsResult {
        settings,
        daemon_status,
    })
}

#[tauri::command]
pub async fn probe_connection(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<ConnectionStatus, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    probe_connection_from_settings_dir(&settings_dir).await
}

#[tauri::command]
pub async fn refresh_dashboard(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<DashboardSnapshot, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    refresh_dashboard_from_settings_dir(&settings_dir).await
}

#[tauri::command]
pub async fn load_tunnel_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<TunnelWorkspaceVm, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    load_tunnel_workspace_from_settings_dir(&settings_dir).await
}

#[tauri::command]
pub fn load_provider_availability_snapshot(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<ProviderAvailabilitySnapshotVm, String> {
    let live_install_statuses = state
        .provider_install_statuses
        .lock()
        .expect("provider install statuses should lock")
        .clone();
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    let base_snapshot = build_provider_availability_snapshot(
        ProviderAvailabilityProbe::detect_for_settings_dir(&settings_dir),
    );
    let persisted_install_statuses =
        load_provider_install_statuses(&settings_dir).unwrap_or_default();
    let normalized_persisted_install_statuses =
        normalize_persisted_provider_install_statuses(&base_snapshot, &persisted_install_statuses);
    if normalized_persisted_install_statuses != persisted_install_statuses {
        let _ =
            save_provider_install_statuses(&settings_dir, &normalized_persisted_install_statuses);
    }

    Ok(merge_provider_install_statuses(
        merge_provider_install_statuses(base_snapshot, &normalized_persisted_install_statuses),
        &live_install_statuses,
    ))
}

#[tauri::command]
pub async fn install_provider(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    provider: TunnelProvider,
) -> Result<ProviderInstallStatus, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    let tools_root = tools_root_from_base_dir(&settings_dir);
    let provider_key = provider_binary_name(&provider).to_string();

    if let Some(manifest) = provider_manifest_entry_for_current_platform(&provider) {
        let manifest_version = manifest.version.clone();
        let mut persisted_statuses =
            load_provider_install_statuses(&settings_dir).unwrap_or_default();
        {
            let mut statuses = state
                .provider_install_statuses
                .lock()
                .expect("provider install statuses should lock");
            if let Some(existing) = statuses.get(&provider_key) {
                if existing.state == crate::provider_installer::ProviderInstallState::Downloading {
                    return Ok(existing.clone());
                }
            }
            statuses.insert(
                provider_key.clone(),
                ProviderInstallStatus {
                    state: crate::provider_installer::ProviderInstallState::Downloading,
                    source: ProviderInstallSource::Missing,
                    resolved_path: None,
                    version: Some(manifest_version.clone()),
                    last_error: None,
                },
            );
        }
        persisted_statuses.insert(
            provider_key.clone(),
            ProviderInstallStatus {
                state: crate::provider_installer::ProviderInstallState::Downloading,
                source: ProviderInstallSource::Missing,
                resolved_path: None,
                version: Some(manifest_version.clone()),
                last_error: None,
            },
        );
        save_provider_install_statuses(&settings_dir, &persisted_statuses)
            .map_err(command_error)?;

        let install_statuses = state.provider_install_statuses.clone();
        let settings_dir = settings_dir.clone();
        tauri::async_runtime::spawn(async move {
            let next_status = match install_manifest_to_tools_root_with_downloader(
                manifest.clone(),
                &tools_root,
                |manifest| async move { download_provider_archive_bytes(&manifest).await },
            )
            .await
            {
                Ok(status) => status,
                Err(error) => ProviderInstallStatus {
                    state: crate::provider_installer::ProviderInstallState::Failed,
                    source: ProviderInstallSource::Missing,
                    resolved_path: None,
                    version: Some(manifest.version.clone()),
                    last_error: Some(error),
                },
            };

            install_statuses
                .lock()
                .expect("provider install statuses should lock")
                .insert(provider_key.clone(), next_status.clone());

            let mut persisted_statuses =
                load_provider_install_statuses(&settings_dir).unwrap_or_default();
            match next_status.state {
                crate::provider_installer::ProviderInstallState::Installed
                | crate::provider_installer::ProviderInstallState::Idle => {
                    persisted_statuses.remove(&provider_key);
                }
                _ => {
                    persisted_statuses.insert(provider_key, next_status);
                }
            }
            let _ = save_provider_install_statuses(&settings_dir, &persisted_statuses);
        });

        return Ok(ProviderInstallStatus {
            state: crate::provider_installer::ProviderInstallState::Downloading,
            source: ProviderInstallSource::Missing,
            resolved_path: None,
            version: Some(manifest_version),
            last_error: None,
        });
    }

    launch_system_provider_installer(&provider)?;
    Ok(ProviderInstallStatus {
        state: crate::provider_installer::ProviderInstallState::Idle,
        source: ProviderInstallSource::SystemPath,
        resolved_path: None,
        version: None,
        last_error: None,
    })
}

#[tauri::command]
pub fn save_tunnel_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    profile: TunnelProfileInput,
) -> Result<TunnelWorkspaceVm, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    save_tunnel_profile_to_settings_dir(&settings_dir, profile)
}

#[tauri::command]
pub fn select_tunnel_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    id: String,
) -> Result<TunnelWorkspaceVm, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    select_tunnel_profile_from_settings_dir(&settings_dir, &id)
}

#[tauri::command]
pub async fn delete_tunnel_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    id: String,
) -> Result<TunnelWorkspaceVm, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    delete_tunnel_profile_from_settings_dir(&settings_dir, &id).await
}

#[tauri::command]
pub async fn start_tunnel(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    input: StartTunnelInput,
) -> Result<DashboardSnapshot, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    start_tunnel_from_settings_dir(&settings_dir, input).await
}

#[tauri::command]
pub async fn stop_tunnel(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<DashboardSnapshot, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    stop_tunnel_from_settings_dir(&settings_dir).await
}

#[tauri::command]
pub async fn list_routes(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<RouteWorkspaceSnapshot, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    list_routes_from_settings_dir(&settings_dir).await
}

#[tauri::command]
pub async fn save_route(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    form: RouteFormData,
) -> Result<RouteWorkspaceSnapshot, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    save_route_from_settings_dir(&settings_dir, form).await
}

#[tauri::command]
pub async fn delete_route(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    id: String,
) -> Result<RouteWorkspaceSnapshot, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    delete_route_from_settings_dir(&settings_dir, id).await
}

#[tauri::command]
pub async fn load_diagnostics_summary(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<DiagnosticsSummaryVm, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    load_diagnostics_summary_from_settings_dir(&settings_dir).await
}

#[tauri::command]
pub async fn load_upstreams_health(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<Vec<UpstreamHealthVm>, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    load_upstreams_health_from_settings_dir(&settings_dir).await
}

#[tauri::command]
pub async fn load_recent_logs(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    lines: usize,
) -> Result<LogTailVm, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    load_recent_logs_from_settings_dir(&settings_dir, lines).await
}

#[tauri::command]
pub async fn load_provider_status_summary(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<Option<ProviderStatusVm>, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    load_provider_status_summary_from_settings_dir(&settings_dir).await
}

pub async fn probe_connection_from_settings_dir(
    settings_dir: &Path,
) -> Result<ConnectionStatus, String> {
    let (_, client) = load_client(settings_dir)?;
    match client.health().await {
        Ok(_) => Ok(ConnectionStatus {
            connected: true,
            message: None,
        }),
        Err(error) => Ok(ConnectionStatus {
            connected: false,
            message: Some(command_error(error)),
        }),
    }
}

fn settings_save_reconnect_mode(settings: &GuiSettings) -> SettingsSaveReconnectMode {
    if settings.base_url == crate::settings::DEFAULT_BASE_URL {
        SettingsSaveReconnectMode::EnsureLocalDaemon
    } else {
        SettingsSaveReconnectMode::ProbeConnection
    }
}

fn startup_reconnect_mode(settings: &GuiSettings) -> SettingsSaveReconnectMode {
    settings_save_reconnect_mode(settings)
}

fn daemon_status_snapshot_from_connection(connection: ConnectionStatus) -> DaemonStatusSnapshot {
    if connection.connected {
        DaemonStatusSnapshot {
            ownership: daemon_manager::DaemonOwnership::External,
            bootstrapping: false,
            connected: true,
            message: Some("Connected to the configured TunnelMux daemon.".to_string()),
        }
    } else {
        DaemonStatusSnapshot {
            ownership: daemon_manager::DaemonOwnership::Unavailable,
            bootstrapping: false,
            connected: false,
            message: connection
                .message
                .or_else(|| Some("Configured TunnelMux daemon is unavailable.".to_string())),
        }
    }
}

async fn reconnect_after_settings_save<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &GuiAppState,
    settings_dir: &Path,
    settings: &GuiSettings,
) -> DaemonStatusSnapshot {
    match settings_save_reconnect_mode(settings) {
        SettingsSaveReconnectMode::EnsureLocalDaemon => {
            match daemon_manager::ensure_local_daemon(app, &state.daemon_runtime, settings).await {
                Ok(snapshot) => snapshot,
                Err(error) => DaemonStatusSnapshot {
                    ownership: daemon_manager::DaemonOwnership::Unavailable,
                    bootstrapping: false,
                    connected: false,
                    message: {
                        let error = command_error(error);
                        let friendly = daemon_manager::friendly_daemon_unavailable_message(&error);
                        Some(if friendly == error {
                            format!("Could not start local TunnelMux: {error}")
                        } else {
                            friendly
                        })
                    },
                },
            }
        }
        SettingsSaveReconnectMode::ProbeConnection => {
            match probe_connection_from_settings_dir(settings_dir).await {
                Ok(connection) => daemon_status_snapshot_from_connection(connection),
                Err(error) => DaemonStatusSnapshot {
                    ownership: daemon_manager::DaemonOwnership::Unavailable,
                    bootstrapping: false,
                    connected: false,
                    message: Some(error),
                },
            }
        }
    }
}

pub async fn refresh_dashboard_from_settings_dir(
    settings_dir: &Path,
) -> Result<DashboardSnapshot, String> {
    let (settings, client) = load_client(settings_dir)?;
    let current_tunnel = settings.current_tunnel();
    let tunnel_id = current_tunnel
        .map(|tunnel| tunnel.id.as_str())
        .unwrap_or("primary");
    match client.health().await {
        Ok(health) => match client.tunnel_status(tunnel_id).await {
            Ok(tunnel) => {
                let message = if tunnel.tunnel.state == tunnelmux_core::TunnelState::Running
                    && tunnel.tunnel.provider == Some(TunnelProvider::Cloudflared)
                    && tunnel.tunnel.public_base_url.is_none()
                    && current_tunnel
                        .and_then(|tunnel| tunnel.cloudflared_tunnel_token.as_ref())
                        .is_some()
                {
                    Some(
                        "Cloudflared named tunnel is running. Public hostname is managed in Cloudflare."
                            .to_string(),
                    )
                } else {
                    tunnel.tunnel.last_error.clone().filter(|value| {
                        matches!(
                            tunnel.tunnel.state,
                            tunnelmux_core::TunnelState::Stopped
                                | tunnelmux_core::TunnelState::Error
                        ) && !value.trim().is_empty()
                    })
                };
                Ok(DashboardSnapshot {
                    connected: true,
                    settings,
                    health: Some(health),
                    tunnel: Some(tunnel.tunnel),
                    message,
                })
            }
            Err(error) => Ok(DashboardSnapshot {
                connected: false,
                settings,
                health: Some(health),
                tunnel: None,
                message: Some(command_error(error)),
            }),
        },
        Err(error) => Ok(DashboardSnapshot {
            connected: false,
            settings,
            health: None,
            tunnel: None,
            message: Some(command_error(error)),
        }),
    }
}

pub async fn start_tunnel_from_settings_dir(
    settings_dir: &Path,
    input: StartTunnelInput,
) -> Result<DashboardSnapshot, String> {
    start_tunnel_from_settings_dir_with_provider_probe(
        settings_dir,
        input,
        ProviderAvailabilityProbe::detect_for_settings_dir(settings_dir),
    )
    .await
}

async fn start_tunnel_from_settings_dir_with_provider_probe(
    settings_dir: &Path,
    input: StartTunnelInput,
    provider_probe: ProviderAvailabilityProbe,
) -> Result<DashboardSnapshot, String> {
    ensure_provider_is_available(&input.provider, &provider_probe)?;
    let (settings, client) = load_client(settings_dir)?;
    let current_tunnel = settings.current_tunnel();
    let tunnel_id = current_tunnel
        .map(|tunnel| tunnel.id.clone())
        .ok_or_else(|| "no tunnel selected".to_string())?;
    ensure_tunnel_start_is_configured(current_tunnel, &input.provider)?;
    let metadata = build_tunnel_metadata(
        current_tunnel,
        &input.provider,
        provider_probe.availability(&input.provider),
        settings.base_url == crate::settings::DEFAULT_BASE_URL,
    );
    let request = TunnelStartRequest {
        tunnel_id,
        provider: input.provider.clone(),
        target_url: input.target_url.clone(),
        auto_restart: Some(input.auto_restart),
        metadata,
    };
    let response = client
        .start_tunnel(&request)
        .await
        .map_err(|error| friendly_start_error(command_error(error), current_tunnel, &input))?;

    Ok(DashboardSnapshot {
        connected: true,
        settings,
        health: None,
        tunnel: Some(response.tunnel),
        message: None,
    })
}

pub async fn stop_tunnel_from_settings_dir(
    settings_dir: &Path,
) -> Result<DashboardSnapshot, String> {
    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.clone())
        .ok_or_else(|| "no tunnel selected".to_string())?;
    let response = client
        .stop_tunnel(&tunnel_id)
        .await
        .map_err(command_error)?;

    Ok(DashboardSnapshot {
        connected: true,
        settings,
        health: None,
        tunnel: Some(response.tunnel),
        message: None,
    })
}

pub async fn list_routes_from_settings_dir(
    settings_dir: &Path,
) -> Result<RouteWorkspaceSnapshot, String> {
    let (settings, client) = load_client(settings_dir)?;
    let current_tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.as_str())
        .ok_or_else(|| "no tunnel selected".to_string())?;
    let response = client
        .list_routes(current_tunnel_id)
        .await
        .map_err(command_error)?;
    let routes = response.routes;
    let message = if routes.is_empty() {
        Some(
            "No services yet. Add your first local service to replace the default welcome page."
                .to_string(),
        )
    } else {
        None
    };
    Ok(RouteWorkspaceSnapshot::from_routes(routes, message))
}

pub async fn save_route_from_settings_dir(
    settings_dir: &Path,
    form: RouteFormData,
) -> Result<RouteWorkspaceSnapshot, String> {
    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.as_str())
        .ok_or_else(|| "no tunnel selected".to_string())?;
    let request = form.into_create_request(tunnel_id);
    if let Some(original_id) = form.original_id.as_deref() {
        client
            .update_route_with_options(original_id, &request, true)
            .await
            .map_err(command_error)
            .map_err(|error| friendly_route_save_error(error, &request))?;
    } else {
        client
            .create_route(&request)
            .await
            .map_err(command_error)
            .map_err(|error| friendly_route_save_error(error, &request))?;
    }

    let routes = client.list_routes(tunnel_id).await.map_err(command_error)?;
    let filtered = routes.routes;
    Ok(RouteWorkspaceSnapshot::from_routes(
        filtered,
        Some("Route saved.".to_string()),
    ))
}

pub async fn delete_route_from_settings_dir(
    settings_dir: &Path,
    id: String,
) -> Result<RouteWorkspaceSnapshot, String> {
    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.as_str())
        .ok_or_else(|| "no tunnel selected".to_string())?;
    client
        .delete_route(&id, tunnel_id, false)
        .await
        .map_err(command_error)?;
    let routes = client.list_routes(tunnel_id).await.map_err(command_error)?;
    let filtered = routes.routes;
    Ok(RouteWorkspaceSnapshot::from_routes(
        filtered,
        Some("Route deleted.".to_string()),
    ))
}

pub async fn load_diagnostics_summary_from_settings_dir(
    settings_dir: &Path,
) -> Result<DiagnosticsSummaryVm, String> {
    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.as_str())
        .unwrap_or("primary");
    let response = client
        .diagnostics_for_tunnel(tunnel_id)
        .await
        .map_err(command_error)?;
    Ok(DiagnosticsSummaryVm::from(response))
}

pub async fn load_upstreams_health_from_settings_dir(
    settings_dir: &Path,
) -> Result<Vec<UpstreamHealthVm>, String> {
    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.as_str())
        .unwrap_or("primary");
    let response = client
        .upstreams_health_for_tunnel(tunnel_id)
        .await
        .map_err(command_error)?;
    Ok(response
        .upstreams
        .into_iter()
        .map(UpstreamHealthVm::from)
        .collect())
}

pub async fn load_recent_logs_from_settings_dir(
    settings_dir: &Path,
    lines: usize,
) -> Result<LogTailVm, String> {
    if lines == 0 {
        return Err("lines must be greater than zero".to_string());
    }

    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.as_str())
        .unwrap_or("primary");
    let response = client
        .tunnel_logs_for_tunnel(tunnel_id, lines)
        .await
        .map_err(command_error)?;
    Ok(LogTailVm::from_response(lines, response))
}

pub async fn load_provider_status_summary_from_settings_dir(
    settings_dir: &Path,
) -> Result<Option<ProviderStatusVm>, String> {
    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.clone())
        .unwrap_or_else(|| "primary".to_string());
    let tunnel = client
        .tunnel_status(&tunnel_id)
        .await
        .ok()
        .map(|response| response.tunnel);
    let log_lines = client
        .tunnel_logs_for_tunnel(&tunnel_id, 40)
        .await
        .map(|response| response.lines)
        .unwrap_or_default();
    let routes = client
        .list_routes(&tunnel_id)
        .await
        .map(|response| response.routes)
        .unwrap_or_default();
    Ok(derive_provider_status_summary(
        &settings,
        tunnel.as_ref(),
        &log_lines,
        &routes,
    ))
}

pub async fn load_tunnel_workspace_from_settings_dir(
    settings_dir: &Path,
) -> Result<TunnelWorkspaceVm, String> {
    let settings = load_settings_from_dir(settings_dir).map_err(command_error)?;
    let provider_probe = ProviderAvailabilityProbe::detect_for_settings_dir(settings_dir);

    let daemon_workspace = if let Ok((_, client)) = load_client(settings_dir) {
        if client.health().await.is_ok() {
            client.tunnel_workspace().await.ok()
        } else {
            None
        }
    } else {
        None
    };

    Ok(build_tunnel_workspace(
        &settings,
        daemon_workspace.as_ref(),
        provider_probe,
    ))
}

pub fn save_tunnel_profile_to_settings_dir(
    settings_dir: &Path,
    profile: TunnelProfileInput,
) -> Result<TunnelWorkspaceVm, String> {
    validate_tunnel_profile_input(&profile)?;
    let mut settings = load_settings_from_dir(settings_dir).map_err(command_error)?;
    let tunnel_id = profile
        .id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| next_tunnel_profile_id(&settings.tunnels));
    let next_profile = crate::settings::TunnelProfileSettings {
        id: tunnel_id.clone(),
        name: profile.name,
        provider: profile.provider,
        gateway_target_url: profile.gateway_target_url,
        auto_restart: profile.auto_restart,
        cloudflared_tunnel_token: profile.cloudflared_tunnel_token,
        ngrok_authtoken: profile.ngrok_authtoken,
        ngrok_domain: profile.ngrok_domain,
    };

    if let Some(index) = settings
        .tunnels
        .iter()
        .position(|item| item.id == tunnel_id)
    {
        settings.tunnels[index] = next_profile;
    } else {
        settings.tunnels.push(next_profile);
    }
    settings.current_tunnel_id = Some(tunnel_id);
    save_settings_to_dir(settings_dir, &settings).map_err(command_error)?;
    load_tunnel_workspace_from_settings_dir_without_daemon(settings_dir)
}

pub fn select_tunnel_profile_from_settings_dir(
    settings_dir: &Path,
    id: &str,
) -> Result<TunnelWorkspaceVm, String> {
    let mut settings = load_settings_from_dir(settings_dir).map_err(command_error)?;
    if !settings.tunnels.iter().any(|item| item.id == id) {
        return Err(format!("tunnel profile not found: {id}"));
    }
    settings.current_tunnel_id = Some(id.to_string());
    save_settings_to_dir(settings_dir, &settings).map_err(command_error)?;
    load_tunnel_workspace_from_settings_dir_without_daemon(settings_dir)
}

pub async fn delete_tunnel_profile_from_settings_dir(
    settings_dir: &Path,
    id: &str,
) -> Result<TunnelWorkspaceVm, String> {
    if let Ok((_, client)) = load_client(settings_dir) {
        if client.health().await.is_ok() {
            client.delete_tunnel(id).await.map_err(command_error)?;
        }
    }
    delete_tunnel_profile_from_settings_dir_without_daemon(settings_dir, id)
}

fn delete_tunnel_profile_from_settings_dir_without_daemon(
    settings_dir: &Path,
    id: &str,
) -> Result<TunnelWorkspaceVm, String> {
    let mut settings = load_settings_from_dir(settings_dir).map_err(command_error)?;
    let before = settings.tunnels.len();
    settings.tunnels.retain(|item| item.id != id);
    if settings.tunnels.len() == before {
        return Err(format!("tunnel profile not found: {id}"));
    }
    settings.current_tunnel_id = settings.tunnels.first().map(|item| item.id.clone());
    save_settings_to_dir(settings_dir, &settings).map_err(command_error)?;
    load_tunnel_workspace_from_settings_dir_without_daemon(settings_dir)
}

fn load_tunnel_workspace_from_settings_dir_without_daemon(
    settings_dir: &Path,
) -> Result<TunnelWorkspaceVm, String> {
    load_tunnel_workspace_from_settings_dir_without_daemon_with_provider_probe(
        settings_dir,
        ProviderAvailabilityProbe::detect_for_settings_dir(settings_dir),
    )
}

fn load_tunnel_workspace_from_settings_dir_without_daemon_with_provider_probe(
    settings_dir: &Path,
    provider_probe: ProviderAvailabilityProbe,
) -> Result<TunnelWorkspaceVm, String> {
    let settings = load_settings_from_dir(settings_dir).map_err(command_error)?;
    Ok(build_tunnel_workspace(&settings, None, provider_probe))
}

fn build_tunnel_workspace(
    settings: &GuiSettings,
    daemon_workspace: Option<&tunnelmux_core::TunnelWorkspaceResponse>,
    provider_probe: ProviderAvailabilityProbe,
) -> TunnelWorkspaceVm {
    if settings.tunnels.is_empty() {
        return TunnelWorkspaceVm {
            tunnels: Vec::new(),
            current_tunnel_id: None,
        };
    }

    let tunnels = settings
        .tunnels
        .iter()
        .map(|tunnel| {
            let summary = daemon_workspace
                .and_then(|workspace| workspace.tunnels.iter().find(|item| item.id == tunnel.id));
            let availability = provider_probe.availability(&tunnel.provider);
            TunnelProfileVm {
                id: tunnel.id.clone(),
                name: tunnel.name.clone(),
                provider: provider_name(&tunnel.provider).to_string(),
                provider_availability: ProviderAvailabilityVm {
                    binary_name: provider_binary_name(&tunnel.provider).to_string(),
                    installed: availability.installed(),
                    source: provider_install_source_label(availability.source).to_string(),
                    resolved_path: availability
                        .resolved_path
                        .as_ref()
                        .map(|path| path.display().to_string()),
                    install_state: None,
                    install_error: None,
                    install_version: None,
                },
                state: summary
                    .map(|item| format!("{:?}", item.state).to_lowercase())
                    .unwrap_or_else(|| "idle".to_string()),
                route_count: summary.map(|item| item.route_count).unwrap_or(0),
                enabled_route_count: summary.map(|item| item.enabled_route_count).unwrap_or(0),
                public_base_url: summary.and_then(|item| item.public_base_url.clone()),
            }
        })
        .collect();

    TunnelWorkspaceVm {
        tunnels,
        current_tunnel_id: daemon_workspace
            .and_then(|workspace| workspace.current_tunnel_id.clone())
            .or(settings.current_tunnel_id.clone()),
    }
}

fn build_provider_availability_snapshot(
    provider_probe: ProviderAvailabilityProbe,
) -> ProviderAvailabilitySnapshotVm {
    ProviderAvailabilitySnapshotVm {
        cloudflared: ProviderAvailabilityVm {
            binary_name: provider_binary_name(&TunnelProvider::Cloudflared).to_string(),
            installed: provider_probe.cloudflared.installed(),
            source: provider_install_source_label(provider_probe.cloudflared.source).to_string(),
            resolved_path: provider_probe
                .cloudflared
                .resolved_path
                .as_ref()
                .map(|path| path.display().to_string()),
            install_state: None,
            install_error: None,
            install_version: None,
        },
        ngrok: ProviderAvailabilityVm {
            binary_name: provider_binary_name(&TunnelProvider::Ngrok).to_string(),
            installed: provider_probe.ngrok.installed(),
            source: provider_install_source_label(provider_probe.ngrok.source).to_string(),
            resolved_path: provider_probe
                .ngrok
                .resolved_path
                .as_ref()
                .map(|path| path.display().to_string()),
            install_state: None,
            install_error: None,
            install_version: None,
        },
    }
}

fn merge_provider_install_statuses(
    mut snapshot: ProviderAvailabilitySnapshotVm,
    install_statuses: &HashMap<String, ProviderInstallStatus>,
) -> ProviderAvailabilitySnapshotVm {
    if let Some(status) = install_statuses.get("cloudflared") {
        apply_install_status_to_availability(&mut snapshot.cloudflared, status);
    }
    if let Some(status) = install_statuses.get("ngrok") {
        apply_install_status_to_availability(&mut snapshot.ngrok, status);
    }
    snapshot
}

fn normalize_persisted_provider_install_statuses(
    snapshot: &ProviderAvailabilitySnapshotVm,
    persisted_statuses: &HashMap<String, ProviderInstallStatus>,
) -> HashMap<String, ProviderInstallStatus> {
    let mut normalized = HashMap::new();

    if let Some(status) = normalize_persisted_provider_install_status(
        &snapshot.cloudflared,
        persisted_statuses.get("cloudflared"),
    ) {
        normalized.insert("cloudflared".to_string(), status);
    }

    if let Some(status) = normalize_persisted_provider_install_status(
        &snapshot.ngrok,
        persisted_statuses.get("ngrok"),
    ) {
        normalized.insert("ngrok".to_string(), status);
    }

    normalized
}

fn normalize_persisted_provider_install_status(
    availability: &ProviderAvailabilityVm,
    status: Option<&ProviderInstallStatus>,
) -> Option<ProviderInstallStatus> {
    let status = status?;

    if availability.installed {
        return None;
    }

    match status.state {
        crate::provider_installer::ProviderInstallState::Downloading => {
            Some(ProviderInstallStatus {
                state: crate::provider_installer::ProviderInstallState::Failed,
                source: ProviderInstallSource::Missing,
                resolved_path: None,
                version: status.version.clone(),
                last_error: Some(status.last_error.clone().unwrap_or_else(|| {
                    "Installation was interrupted before completion. Retry install.".to_string()
                })),
            })
        }
        crate::provider_installer::ProviderInstallState::Failed => Some(status.clone()),
        crate::provider_installer::ProviderInstallState::Installed
        | crate::provider_installer::ProviderInstallState::Idle => None,
    }
}

fn apply_install_status_to_availability(
    availability: &mut ProviderAvailabilityVm,
    status: &ProviderInstallStatus,
) {
    availability.install_state = Some(provider_install_state_label(status.state).to_string());
    availability.install_error = status.last_error.clone();
    availability.install_version = status.version.clone();
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderBinaryAvailability {
    source: ProviderInstallSource,
    resolved_path: Option<PathBuf>,
}

impl ProviderBinaryAvailability {
    fn from_path(source: ProviderInstallSource, path: PathBuf) -> Self {
        Self {
            source,
            resolved_path: Some(path),
        }
    }

    fn missing() -> Self {
        Self {
            source: ProviderInstallSource::Missing,
            resolved_path: None,
        }
    }

    fn installed(&self) -> bool {
        self.resolved_path.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderAvailabilityProbe {
    cloudflared: ProviderBinaryAvailability,
    ngrok: ProviderBinaryAvailability,
}

impl ProviderAvailabilityProbe {
    fn detect_for_settings_dir(settings_dir: &Path) -> Self {
        Self::from_search_dirs_with_local_tools(
            daemon_manager::provider_binary_search_dirs(
                std::env::var_os("PATH").as_deref(),
                std::iter::empty::<PathBuf>(),
            ),
            tools_root_from_base_dir(settings_dir),
            std::env::var_os("PATHEXT").as_deref(),
        )
    }

    #[cfg(test)]
    fn from_env(path: Option<&OsStr>, pathext: Option<&OsStr>) -> Self {
        let search_dirs = path
            .map(std::env::split_paths)
            .into_iter()
            .flatten()
            .collect();
        Self::from_search_dirs(search_dirs, pathext)
    }

    #[cfg(test)]
    fn from_search_dirs(search_dirs: Vec<PathBuf>, pathext: Option<&OsStr>) -> Self {
        Self::from_search_dirs_with_local_tools(
            search_dirs,
            PathBuf::from("__missing_local_tools__"),
            pathext,
        )
    }

    fn from_search_dirs_with_local_tools(
        search_dirs: Vec<PathBuf>,
        local_tools_root: PathBuf,
        pathext: Option<&OsStr>,
    ) -> Self {
        let local_tools_bin = local_tools_root.join("bin");
        Self {
            cloudflared: resolve_provider_availability(
                "cloudflared",
                local_tools_bin.clone(),
                search_dirs.clone(),
                pathext,
            ),
            ngrok: resolve_provider_availability("ngrok", local_tools_bin, search_dirs, pathext),
        }
    }

    fn installed(&self, provider: &TunnelProvider) -> bool {
        self.availability(provider).installed()
    }

    fn availability(&self, provider: &TunnelProvider) -> &ProviderBinaryAvailability {
        match provider {
            TunnelProvider::Cloudflared => &self.cloudflared,
            TunnelProvider::Ngrok => &self.ngrok,
        }
    }

    #[cfg(test)]
    fn from_install_flags(cloudflared_installed: bool, ngrok_installed: bool) -> Self {
        Self {
            cloudflared: if cloudflared_installed {
                ProviderBinaryAvailability::from_path(
                    ProviderInstallSource::SystemPath,
                    PathBuf::from("/usr/bin/cloudflared"),
                )
            } else {
                ProviderBinaryAvailability::missing()
            },
            ngrok: if ngrok_installed {
                ProviderBinaryAvailability::from_path(
                    ProviderInstallSource::SystemPath,
                    PathBuf::from("/usr/bin/ngrok"),
                )
            } else {
                ProviderBinaryAvailability::missing()
            },
        }
    }
}

fn resolve_provider_availability(
    binary_name: &str,
    local_tools_bin: PathBuf,
    search_dirs: Vec<PathBuf>,
    pathext: Option<&OsStr>,
) -> ProviderBinaryAvailability {
    if let Some(path) =
        daemon_manager::resolve_binary_in_dirs(binary_name, vec![local_tools_bin], pathext)
    {
        return ProviderBinaryAvailability::from_path(ProviderInstallSource::LocalTools, path);
    }

    if let Some(path) = daemon_manager::resolve_binary_in_dirs(binary_name, search_dirs, pathext) {
        return ProviderBinaryAvailability::from_path(ProviderInstallSource::SystemPath, path);
    }

    ProviderBinaryAvailability::missing()
}

fn provider_install_source_label(source: ProviderInstallSource) -> &'static str {
    match source {
        ProviderInstallSource::LocalTools => "local_tools",
        ProviderInstallSource::SystemPath => "system_path",
        ProviderInstallSource::Missing => "missing",
    }
}

fn provider_install_state_label(
    state: crate::provider_installer::ProviderInstallState,
) -> &'static str {
    match state {
        crate::provider_installer::ProviderInstallState::Idle => "idle",
        crate::provider_installer::ProviderInstallState::Downloading => "downloading",
        crate::provider_installer::ProviderInstallState::Installed => "installed",
        crate::provider_installer::ProviderInstallState::Failed => "failed",
    }
}

fn ensure_provider_is_available(
    provider: &TunnelProvider,
    provider_probe: &ProviderAvailabilityProbe,
) -> Result<(), String> {
    if provider_probe.installed(provider) {
        return Ok(());
    }

    Err(missing_provider_install_message(provider))
}

fn ensure_tunnel_start_is_configured(
    current_tunnel: Option<&crate::settings::TunnelProfileSettings>,
    provider: &TunnelProvider,
) -> Result<(), String> {
    if *provider == TunnelProvider::Ngrok
        && current_tunnel
            .and_then(|tunnel| tunnel.ngrok_authtoken.as_deref())
            .is_none()
    {
        return Err("Add the ngrok authtoken on this tunnel, then retry.".to_string());
    }

    if *provider == TunnelProvider::Ngrok
        && current_tunnel
            .and_then(|tunnel| tunnel.ngrok_domain.as_deref())
            .is_some_and(|value| !is_supported_ngrok_reserved_domain(value))
    {
        return Err(ngrok_reserved_domain_recovery_message());
    }

    Ok(())
}

fn provider_name(provider: &TunnelProvider) -> &'static str {
    match provider {
        TunnelProvider::Cloudflared => "cloudflared",
        TunnelProvider::Ngrok => "ngrok",
    }
}

fn provider_binary_name(provider: &TunnelProvider) -> &'static str {
    provider_name(provider)
}

fn missing_provider_install_message(provider: &TunnelProvider) -> String {
    let binary_name = provider_binary_name(provider);
    format!(
        "Install {binary_name} to start this tunnel. TunnelMux could not find the {binary_name} command in your PATH."
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderInstallInvocation {
    program: String,
    args: Vec<String>,
}

fn provider_install_command(provider: &TunnelProvider) -> &'static str {
    match provider {
        TunnelProvider::Cloudflared => "brew install cloudflared",
        TunnelProvider::Ngrok => "brew install ngrok/ngrok/ngrok",
    }
}

fn launch_system_provider_installer(provider: &TunnelProvider) -> Result<(), String> {
    let invocation = provider_install_invocation(provider)?;
    Command::new(&invocation.program)
        .args(&invocation.args)
        .spawn()
        .map_err(|error| {
            format!(
                "failed to launch {} installer: {error}",
                provider_binary_name(provider)
            )
        })?;
    Ok(())
}

async fn install_manifest_to_tools_root_with_downloader<F, Fut>(
    manifest: ProviderInstallManifestEntry,
    tools_root: &Path,
    downloader: F,
) -> Result<ProviderInstallStatus, String>
where
    F: FnOnce(ProviderInstallManifestEntry) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Vec<u8>>>,
{
    let archive = downloader(manifest.clone()).await.map_err(command_error)?;
    install_provider_from_bytes(tools_root, &manifest, &archive).map_err(command_error)
}

#[cfg(target_os = "macos")]
fn provider_install_invocation(
    provider: &TunnelProvider,
) -> Result<ProviderInstallInvocation, String> {
    let command = provider_install_command(provider);
    let quoted = format!("\"{}\"", command.replace('\\', "\\\\").replace('"', "\\\""));

    Ok(ProviderInstallInvocation {
        program: "osascript".to_string(),
        args: vec![
            "-e".to_string(),
            format!("tell application \"Terminal\" to do script {quoted}"),
            "-e".to_string(),
            "activate".to_string(),
        ],
    })
}

#[cfg(target_os = "windows")]
fn provider_install_invocation(
    provider: &TunnelProvider,
) -> Result<ProviderInstallInvocation, String> {
    Ok(ProviderInstallInvocation {
        program: "cmd".to_string(),
        args: vec![
            "/C".to_string(),
            "start".to_string(),
            String::new(),
            "powershell".to_string(),
            "-NoExit".to_string(),
            "-Command".to_string(),
            match provider {
                TunnelProvider::Cloudflared => {
                    "winget install --id Cloudflare.cloudflared".to_string()
                }
                TunnelProvider::Ngrok => "winget install --id ngrok.ngrok".to_string(),
            },
        ],
    })
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn provider_install_invocation(
    provider: &TunnelProvider,
) -> Result<ProviderInstallInvocation, String> {
    let url = match provider {
        TunnelProvider::Cloudflared => {
            "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/"
        }
        TunnelProvider::Ngrok => "https://dashboard.ngrok.com/get-started/setup",
    };

    Ok(ProviderInstallInvocation {
        program: "xdg-open".to_string(),
        args: vec![url.to_string()],
    })
}

fn next_tunnel_profile_id(profiles: &[crate::settings::TunnelProfileSettings]) -> String {
    let mut index = 1;
    loop {
        let candidate = format!("tunnel-{index}");
        if profiles.iter().all(|profile| profile.id != candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn derive_provider_status_summary(
    settings: &GuiSettings,
    tunnel: Option<&TunnelStatus>,
    log_lines: &[String],
    routes: &[tunnelmux_core::RouteRule],
) -> Option<ProviderStatusVm> {
    let tunnel = tunnel?;
    let current_tunnel = settings.current_tunnel();
    let provider = tunnel
        .provider
        .clone()
        .or_else(|| current_tunnel.map(|tunnel| tunnel.provider.clone()))
        .unwrap_or(TunnelProvider::Cloudflared);

    if provider == TunnelProvider::Cloudflared
        && tunnel.state == tunnelmux_core::TunnelState::Running
        && tunnel.public_base_url.is_none()
        && current_tunnel
            .and_then(|tunnel| tunnel.cloudflared_tunnel_token.as_ref())
            .is_some()
    {
        return Some(
            ProviderStatusVm::new(
                "warning",
                "Cloudflare Setup",
                "Named tunnel connected. Configure hostname and Access in Cloudflare.",
            )
            .with_action("open_cloudflare", "Open Cloudflare")
            .with_follow_up_action("open_cloudflare_docs", "Setup Hostname"),
        );
    }

    for line in log_lines.iter().rev() {
        if let Some(summary) = classify_provider_log_line(&provider, current_tunnel, line, routes) {
            return Some(summary);
        }
    }

    if tunnel.state == tunnelmux_core::TunnelState::Running {
        return Some(ProviderStatusVm::new(
            "success",
            "Provider Ready",
            "Tunnel provider is connected and ready.",
        ));
    }

    None
}

fn classify_provider_log_line(
    provider: &TunnelProvider,
    current_tunnel: Option<&crate::settings::TunnelProfileSettings>,
    line: &str,
    routes: &[tunnelmux_core::RouteRule],
) -> Option<ProviderStatusVm> {
    let lower = line.to_ascii_lowercase();

    if lower.contains("unable to reach the origin service")
        || lower.contains("connection refused")
        || lower.contains("upstream request failed")
    {
        let summary = ProviderStatusVm::new(
            "warning",
            "Local Service Unreachable",
            "Tunnel is up, but the local service did not respond. Check the target URL and make sure your app is running.",
        );

        return Some(if routes.len() == 1 {
            summary
                .with_action("edit_service", "Edit Service")
                .with_action_payload(&routes[0].id)
        } else {
            summary.with_action("review_services", "Review Services")
        });
    }

    if *provider == TunnelProvider::Ngrok {
        if let Some(code) = line
            .split(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .find(|token| token.starts_with("ERR_NGROK_"))
        {
            return Some(classify_ngrok_error(current_tunnel, &lower, code));
        }
    }

    if *provider == TunnelProvider::Cloudflared
        && is_named_cloudflared_tunnel(current_tunnel)
        && is_cloudflared_named_tunnel_setup_error(&lower)
    {
        return Some(
            ProviderStatusVm::new(
                "error",
                "Cloudflare Setup",
                cloudflared_named_tunnel_setup_status_message(),
            )
            .with_action("open_cloudflare", "Open Cloudflare")
            .with_follow_up_action("open_cloudflare_docs", "Setup Hostname"),
        );
    }

    if *provider == TunnelProvider::Cloudflared && line.contains("trycloudflare.com") {
        return Some(ProviderStatusVm::new(
            "success",
            "Quick Tunnel Active",
            "Cloudflare quick tunnel published a public URL.",
        ));
    }

    None
}

fn classify_ngrok_error(
    current_tunnel: Option<&crate::settings::TunnelProfileSettings>,
    lower: &str,
    code: &str,
) -> ProviderStatusVm {
    let has_authtoken = current_tunnel
        .and_then(|tunnel| tunnel.ngrok_authtoken.as_ref())
        .is_some();
    let has_domain = current_tunnel
        .and_then(|tunnel| tunnel.ngrok_domain.as_ref())
        .is_some();

    if lower.contains("authtoken") || code == "ERR_NGROK_4018" || !has_authtoken {
        return ProviderStatusVm::new(
            "error",
            "ngrok Authtoken Needed",
            &format!("{code}. Add the ngrok authtoken on this tunnel, then retry."),
        )
        .with_action("edit_tunnel", "Add ngrok Authtoken")
        .with_action_payload("ngrok_authtoken");
    }

    if lower.contains("domain") || lower.contains("reserved") || has_domain {
        return ProviderStatusVm::new(
            "error",
            "ngrok Domain Check",
            &format!("{code}. Review the reserved domain on this tunnel and make sure it matches your ngrok account."),
        )
        .with_action("edit_tunnel", "Review ngrok Domain")
        .with_action_payload("ngrok_domain");
    }

    ProviderStatusVm::new(
        "error",
        "ngrok Setup Check",
        &format!("{code}. Review the ngrok settings on this tunnel, then retry."),
    )
    .with_action("edit_tunnel", "Review ngrok Settings")
    .with_action_payload("ngrok_authtoken")
}

fn load_client(settings_dir: &Path) -> Result<(GuiSettings, TunnelmuxControlClient), String> {
    let settings = load_settings_from_dir(settings_dir).map_err(command_error)?;
    let client = TunnelmuxControlClient::new(ControlClientConfig::new(
        settings.base_url.clone(),
        settings.token.clone(),
    ));
    Ok((settings, client))
}

fn resolve_settings_dir<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &GuiAppState,
) -> Result<PathBuf, String> {
    if let Some(path) = &state.settings_dir_override {
        return Ok(path.clone());
    }

    app.path()
        .app_config_dir()
        .context("failed to resolve app config dir")
        .map_err(command_error)
}

fn command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn is_supported_local_url(value: &str) -> bool {
    Url::parse(value.trim())
        .map(|parsed| matches!(parsed.scheme(), "http" | "https"))
        .unwrap_or(false)
}

fn tunnel_local_service_url_recovery_message() -> String {
    "Invalid Local Service URL. Review it on this tunnel, then retry.".to_string()
}

fn ngrok_reserved_domain_recovery_message() -> String {
    "Review the reserved domain on this tunnel. Use only the hostname, like demo.ngrok.app, without https://, paths, query strings, or ports.".to_string()
}

fn is_supported_ngrok_reserved_domain(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }

    if trimmed.contains("://") || trimmed.chars().any(char::is_whitespace) {
        return false;
    }

    Url::parse(&format!("https://{trimmed}"))
        .map(|parsed| {
            parsed.host_str().is_some()
                && parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.port().is_none()
                && parsed.path() == "/"
                && parsed.query().is_none()
                && parsed.fragment().is_none()
        })
        .unwrap_or(false)
}

fn validate_tunnel_profile_input(profile: &TunnelProfileInput) -> Result<(), String> {
    if !is_supported_local_url(&profile.gateway_target_url) {
        return Err(tunnel_local_service_url_recovery_message());
    }

    if profile.provider == TunnelProvider::Ngrok
        && profile
            .ngrok_domain
            .as_deref()
            .is_some_and(|value| !is_supported_ngrok_reserved_domain(value))
    {
        return Err(ngrok_reserved_domain_recovery_message());
    }

    Ok(())
}

fn is_named_cloudflared_tunnel(
    current_tunnel: Option<&crate::settings::TunnelProfileSettings>,
) -> bool {
    current_tunnel
        .and_then(|tunnel| tunnel.cloudflared_tunnel_token.as_deref())
        .is_some()
}

fn is_cloudflared_named_tunnel_setup_error(lower: &str) -> bool {
    lower.contains("cloudflare tunnel token")
        || lower.contains("tunnel token")
        || lower.contains("provided tunnel token")
        || lower.contains("token is not valid")
        || lower.contains("invalid token")
        || lower.contains("failed to get tunnel")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("tunnel credentials")
}

fn cloudflared_tunnel_token_recovery_message() -> String {
    "Review the Cloudflare Tunnel Token on this tunnel, then retry.".to_string()
}

fn cloudflared_named_tunnel_setup_status_message() -> &'static str {
    "Cloudflare rejected the named tunnel setup. Review the tunnel token on this tunnel and make sure a hostname is configured in Cloudflare."
}

fn friendly_start_error(
    message: String,
    current_tunnel: Option<&crate::settings::TunnelProfileSettings>,
    input: &StartTunnelInput,
) -> String {
    let lower = message.to_ascii_lowercase();

    if lower.contains("invalid gateway target url")
        || (lower.contains("invalid") && lower.contains("target url"))
    {
        return tunnel_local_service_url_recovery_message();
    }

    if lower.contains("provider executable not found: /")
        || (lower.contains("provider executable not found: ") && lower.contains(":\\"))
    {
        return format!(
            "TunnelMux found {}, but the connected daemon could not use that binary path. Retry the local daemon or check whether another TunnelMux daemon is already using this port.",
            provider_binary_name(&input.provider)
        );
    }

    if lower.contains("provider executable not found") {
        return missing_provider_install_message(&input.provider);
    }

    if input.provider == TunnelProvider::Cloudflared
        && is_named_cloudflared_tunnel(current_tunnel)
        && is_cloudflared_named_tunnel_setup_error(&lower)
    {
        return cloudflared_tunnel_token_recovery_message();
    }

    if input.provider == TunnelProvider::Ngrok
        && (lower.contains("ngrok authtoken")
            || lower.contains("authtoken")
            || lower.contains("err_ngrok_4018"))
    {
        return "Add the ngrok authtoken on this tunnel, then retry.".to_string();
    }

    if input.provider == TunnelProvider::Ngrok
        && (lower.contains("reserved domain") || lower.contains("domain"))
    {
        return "Review the reserved domain on this tunnel and make sure it matches your ngrok account.".to_string();
    }

    message
}

fn friendly_route_save_error(
    message: String,
    request: &tunnelmux_core::CreateRouteRequest,
) -> String {
    let lower = message.to_ascii_lowercase();

    if lower.contains("invalid url") {
        if !is_supported_local_url(&request.upstream_url) {
            return "Invalid Local Service URL. Review it in this service, then save again."
                .to_string();
        }

        if request
            .fallback_upstream_url
            .as_deref()
            .is_some_and(|value| !is_supported_local_url(value))
        {
            return "Invalid Fallback Local URL. Review it in this service, then save again."
                .to_string();
        }
    }

    if lower.contains("invalid health_check_path") {
        return "Health Check Path must be a slash path like /healthz. Remove any ?query or #fragment, then save again.".to_string();
    }

    if lower.contains("route id is required") {
        return "Add a Local Service URL or enter a Service Name before saving.".to_string();
    }

    if lower.contains("already exists in tunnel") || lower.contains("duplicate route id") {
        let route_name = request.id.trim();
        if route_name.is_empty() {
            return "Service Name is already in use for this tunnel. Rename it and save again."
                .to_string();
        }

        return format!(
            "Service Name \"{}\" is already in use for this tunnel. Rename it and save again.",
            route_name
        );
    }

    message
}

fn build_tunnel_metadata(
    tunnel: Option<&crate::settings::TunnelProfileSettings>,
    provider: &TunnelProvider,
    availability: &ProviderBinaryAvailability,
    include_system_provider_path: bool,
) -> Option<HashMap<String, String>> {
    let mut metadata = HashMap::new();

    if availability.source == ProviderInstallSource::LocalTools
        || (include_system_provider_path
            && availability.source == ProviderInstallSource::SystemPath)
    {
        if let Some(path) = availability.resolved_path.as_ref() {
            metadata.insert("providerBinaryPath".to_string(), path.display().to_string());
        }
    }

    match provider {
        TunnelProvider::Cloudflared => {
            if let Some(value) =
                tunnel.and_then(|tunnel| tunnel.cloudflared_tunnel_token.as_deref())
            {
                metadata.insert("cloudflaredTunnelToken".to_string(), value.to_string());
            }
        }
        TunnelProvider::Ngrok => {
            if let Some(value) = tunnel.and_then(|tunnel| tunnel.ngrok_authtoken.as_deref()) {
                metadata.insert("ngrokAuthtoken".to_string(), value.to_string());
            }
            if let Some(value) = tunnel.and_then(|tunnel| tunnel.ngrok_domain.as_deref()) {
                metadata.insert("ngrokDomain".to_string(), value.to_string());
            }
        }
    }

    (!metadata.is_empty()).then_some(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::Query,
        routing::{get, post},
    };
    use flate2::{Compression, write::GzEncoder};
    use std::{
        net::SocketAddr,
        path::Path,
        sync::atomic::{AtomicU64, Ordering},
    };
    use tar::{Builder, Header};
    use tokio::net::TcpListener;
    use tunnelmux_core::{TunnelStartRequest, TunnelStatus, TunnelStatusResponse};

    #[tokio::test]
    async fn refresh_dashboard_returns_tunnel_snapshot_for_connected_daemon() {
        let temp_dir = prepare_temp_dir();
        let base_url = spawn_test_server(
            Router::new()
                .route("/v1/health", get(health_handler))
                .route("/v1/tunnel/status", get(tunnel_status_handler)),
        )
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: Some("dev-token".to_string()),
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let snapshot = refresh_dashboard_from_settings_dir(&temp_dir)
            .await
            .expect("dashboard refresh should succeed");

        assert!(snapshot.connected);
        assert_eq!(snapshot.settings.token.as_deref(), Some("dev-token"));
        assert_eq!(
            snapshot.tunnel.as_ref().map(|item| item.state.clone()),
            Some(tunnelmux_core::TunnelState::Running)
        );
        assert_eq!(
            snapshot
                .tunnel
                .as_ref()
                .and_then(|item| item.public_base_url.as_deref()),
            Some("https://demo.trycloudflare.com")
        );
    }

    #[test]
    fn settings_save_reconnect_mode_prefers_local_daemon_for_default_url() {
        assert_eq!(
            settings_save_reconnect_mode(&GuiSettings::default()),
            SettingsSaveReconnectMode::EnsureLocalDaemon
        );
    }

    #[test]
    fn settings_save_reconnect_mode_uses_probe_for_custom_url() {
        assert_eq!(
            settings_save_reconnect_mode(&GuiSettings {
                base_url: "http://127.0.0.1:9900".to_string(),
                ..GuiSettings::default()
            }),
            SettingsSaveReconnectMode::ProbeConnection
        );
    }

    #[test]
    fn startup_reconnect_mode_prefers_local_daemon_for_default_url() {
        assert_eq!(
            startup_reconnect_mode(&GuiSettings::default()),
            SettingsSaveReconnectMode::EnsureLocalDaemon
        );
    }

    #[test]
    fn startup_reconnect_mode_uses_probe_for_custom_url() {
        assert_eq!(
            startup_reconnect_mode(&GuiSettings {
                base_url: "http://127.0.0.1:9900".to_string(),
                ..GuiSettings::default()
            }),
            SettingsSaveReconnectMode::ProbeConnection
        );
    }

    #[test]
    fn daemon_status_snapshot_from_connection_marks_connected_custom_daemon_as_external() {
        let snapshot = daemon_status_snapshot_from_connection(ConnectionStatus {
            connected: true,
            message: None,
        });

        assert!(snapshot.connected);
        assert_eq!(
            snapshot.ownership,
            daemon_manager::DaemonOwnership::External
        );
        assert_eq!(
            snapshot.message.as_deref(),
            Some("Connected to the configured TunnelMux daemon.")
        );
    }

    #[test]
    fn daemon_status_snapshot_from_connection_preserves_custom_probe_errors() {
        let snapshot = daemon_status_snapshot_from_connection(ConnectionStatus {
            connected: false,
            message: Some("connection refused".to_string()),
        });

        assert!(!snapshot.connected);
        assert_eq!(
            snapshot.ownership,
            daemon_manager::DaemonOwnership::Unavailable
        );
        assert_eq!(snapshot.message.as_deref(), Some("connection refused"));
    }

    #[tokio::test]
    async fn probe_connection_reports_connected_for_reachable_custom_daemon() {
        let temp_dir = prepare_temp_dir();
        let base_url =
            spawn_test_server(Router::new().route("/v1/health", get(health_handler))).await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let status = probe_connection_from_settings_dir(&temp_dir)
            .await
            .expect("probe should succeed");

        assert!(status.connected);
        assert_eq!(status.message, None);
    }

    #[tokio::test]
    async fn probe_connection_reports_error_for_unreachable_custom_daemon() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url: "http://127.0.0.1:9".to_string(),
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let status = probe_connection_from_settings_dir(&temp_dir)
            .await
            .expect("probe should return a disconnected status");

        assert!(!status.connected);
        assert!(
            status
                .message
                .as_deref()
                .map(|message| message.contains("request failed")
                    || message.contains("error sending request"))
                .unwrap_or(false),
            "unexpected status: {:?}",
            status
        );
    }

    #[test]
    fn friendly_start_error_maps_invalid_gateway_target_url() {
        let tunnel = crate::settings::TunnelProfileSettings {
            id: "primary".to_string(),
            name: "Main Tunnel".to_string(),
            provider: TunnelProvider::Cloudflared,
            gateway_target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: true,
            cloudflared_tunnel_token: None,
            ngrok_authtoken: None,
            ngrok_domain: None,
        };

        assert_eq!(
            friendly_start_error(
                "invalid gateway target URL: not-a-url".to_string(),
                Some(&tunnel),
                &StartTunnelInput {
                    provider: TunnelProvider::Cloudflared,
                    target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                },
            ),
            "Invalid Local Service URL. Review it on this tunnel, then retry."
        );
    }

    #[test]
    fn friendly_start_error_when_provider_executable_is_missing() {
        let tunnel = crate::settings::TunnelProfileSettings {
            id: "primary".to_string(),
            name: "Main Tunnel".to_string(),
            provider: TunnelProvider::Cloudflared,
            gateway_target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: true,
            cloudflared_tunnel_token: None,
            ngrok_authtoken: None,
            ngrok_domain: None,
        };

        assert_eq!(
            friendly_start_error(
                "HTTP 500 Internal Server Error: provider executable not found: cloudflared (Cloudflared); install it or configure the daemon binary path".to_string(),
                Some(&tunnel),
                &StartTunnelInput {
                    provider: TunnelProvider::Cloudflared,
                    target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                },
            ),
            "Install cloudflared to start this tunnel. TunnelMux could not find the cloudflared command in your PATH."
        );
    }

    #[test]
    fn friendly_start_error_reports_daemon_path_mismatch_when_provider_path_was_resolved() {
        let tunnel = crate::settings::TunnelProfileSettings {
            id: "primary".to_string(),
            name: "Main Tunnel".to_string(),
            provider: TunnelProvider::Cloudflared,
            gateway_target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: true,
            cloudflared_tunnel_token: None,
            ngrok_authtoken: None,
            ngrok_domain: None,
        };

        assert_eq!(
            friendly_start_error(
                "HTTP 500 Internal Server Error: provider executable not found: /opt/homebrew/bin/cloudflared (Cloudflared); install it or configure the daemon binary path".to_string(),
                Some(&tunnel),
                &StartTunnelInput {
                    provider: TunnelProvider::Cloudflared,
                    target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                },
            ),
            "TunnelMux found cloudflared, but the connected daemon could not use that binary path. Retry the local daemon or check whether another TunnelMux daemon is already using this port."
        );
    }

    #[test]
    fn friendly_route_save_error_maps_invalid_local_service_url() {
        assert_eq!(
            friendly_route_save_error(
                "invalid URL: not-a-url".to_string(),
                &crate::view_models::RouteFormData {
                    upstream_url: "not-a-url".to_string(),
                    ..crate::view_models::RouteFormData::default()
                }
                .into_create_request("primary"),
            ),
            "Invalid Local Service URL. Review it in this service, then save again."
        );
    }

    #[test]
    fn friendly_route_save_error_maps_invalid_fallback_local_service_url() {
        assert_eq!(
            friendly_route_save_error(
                "invalid URL: tcp://127.0.0.1:3001".to_string(),
                &crate::view_models::RouteFormData {
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    fallback_upstream_url: "tcp://127.0.0.1:3001".to_string(),
                    ..crate::view_models::RouteFormData::default()
                }
                .into_create_request("primary"),
            ),
            "Invalid Fallback Local URL. Review it in this service, then save again."
        );
    }

    #[test]
    fn friendly_route_save_error_maps_invalid_health_check_path() {
        assert_eq!(
            friendly_route_save_error(
                "invalid health_check_path: health check path must not include query or fragment"
                    .to_string(),
                &crate::view_models::RouteFormData {
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    health_check_path: "/healthz?bad=1".to_string(),
                    ..crate::view_models::RouteFormData::default()
                }
                .into_create_request("primary"),
            ),
            "Health Check Path must be a slash path like /healthz. Remove any ?query or #fragment, then save again."
        );
    }

    #[test]
    fn friendly_route_save_error_maps_duplicate_service_name() {
        assert_eq!(
            friendly_route_save_error(
                "route 'local-3000' already exists in tunnel 'primary'".to_string(),
                &crate::view_models::RouteFormData {
                    id: "local-3000".to_string(),
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    ..crate::view_models::RouteFormData::default()
                }
                .into_create_request("primary"),
            ),
            "Service Name \"local-3000\" is already in use for this tunnel. Rename it and save again."
        );
    }

    #[test]
    fn friendly_route_save_error_maps_missing_service_name_or_url() {
        assert_eq!(
            friendly_route_save_error(
                "route id is required".to_string(),
                &crate::view_models::RouteFormData::default().into_create_request("primary"),
            ),
            "Add a Local Service URL or enter a Service Name before saving."
        );
    }

    #[test]
    fn friendly_start_error_maps_ngrok_config_errors() {
        let tunnel = crate::settings::TunnelProfileSettings {
            id: "primary".to_string(),
            name: "Main Tunnel".to_string(),
            provider: TunnelProvider::Ngrok,
            gateway_target_url: "http://127.0.0.1:58080".to_string(),
            auto_restart: true,
            cloudflared_tunnel_token: None,
            ngrok_authtoken: Some("token".to_string()),
            ngrok_domain: Some("demo.ngrok.app".to_string()),
        };

        assert_eq!(
            friendly_start_error(
                "ERR_NGROK_4018: authtoken missing".to_string(),
                Some(&tunnel),
                &StartTunnelInput {
                    provider: TunnelProvider::Ngrok,
                    target_url: "http://127.0.0.1:58080".to_string(),
                    auto_restart: true,
                },
            ),
            "Add the ngrok authtoken on this tunnel, then retry."
        );

        assert_eq!(
            friendly_start_error(
                "reserved domain mismatch".to_string(),
                Some(&tunnel),
                &StartTunnelInput {
                    provider: TunnelProvider::Ngrok,
                    target_url: "http://127.0.0.1:58080".to_string(),
                    auto_restart: true,
                },
            ),
            "Review the reserved domain on this tunnel and make sure it matches your ngrok account."
        );
    }

    #[test]
    fn friendly_start_error_maps_cloudflared_named_tunnel_setup_errors() {
        let tunnel = crate::settings::TunnelProfileSettings {
            id: "primary".to_string(),
            name: "Main Tunnel".to_string(),
            provider: TunnelProvider::Cloudflared,
            gateway_target_url: "http://127.0.0.1:58080".to_string(),
            auto_restart: true,
            cloudflared_tunnel_token: Some("cf-token".to_string()),
            ngrok_authtoken: None,
            ngrok_domain: None,
        };

        assert_eq!(
            friendly_start_error(
                "Provided Tunnel token is not valid".to_string(),
                Some(&tunnel),
                &StartTunnelInput {
                    provider: TunnelProvider::Cloudflared,
                    target_url: "http://127.0.0.1:58080".to_string(),
                    auto_restart: true,
                },
            ),
            "Review the Cloudflare Tunnel Token on this tunnel, then retry."
        );

        assert_eq!(
            friendly_start_error(
                "Unauthorized: failed to get tunnel".to_string(),
                Some(&tunnel),
                &StartTunnelInput {
                    provider: TunnelProvider::Cloudflared,
                    target_url: "http://127.0.0.1:58080".to_string(),
                    auto_restart: true,
                },
            ),
            "Review the Cloudflare Tunnel Token on this tunnel, then retry."
        );
    }

    #[tokio::test]
    async fn start_tunnel_command_maps_connection_errors_cleanly() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url: "http://127.0.0.1:9".to_string(),
                token: None,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error = start_tunnel_from_settings_dir_with_provider_probe(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:18080".to_string(),
                auto_restart: true,
            },
            ProviderAvailabilityProbe::from_install_flags(true, false),
        )
        .await
        .expect_err("start should fail against unreachable daemon");

        assert!(
            error.contains("request failed") || error.contains("error sending request"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn start_tunnel_returns_friendly_error_when_provider_is_missing() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url: "http://127.0.0.1:9".to_string(),
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Cloudflared,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: None,
                    ngrok_domain: None,
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error = start_tunnel_from_settings_dir_with_provider_probe(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
            },
            ProviderAvailabilityProbe::from_install_flags(false, true),
        )
        .await
        .expect_err("missing provider should be caught before the daemon call");

        assert_eq!(
            error,
            "Install cloudflared to start this tunnel. TunnelMux could not find the cloudflared command in your PATH."
        );
    }

    #[tokio::test]
    async fn install_provider_to_local_tools_reports_status() {
        let temp_dir = prepare_temp_dir();
        let tools_root = crate::provider_installer::tools_root_from_base_dir(&temp_dir);
        let archive = build_test_provider_archive_bytes("cloudflared", b"#!/bin/sh\nexit 0\n");
        let manifest = crate::provider_installer::ProviderInstallManifestEntry {
            provider: TunnelProvider::Cloudflared,
            version: "test-version".to_string(),
            binary_name: "cloudflared".to_string(),
            archive_name: "cloudflared-darwin-arm64.tgz".to_string(),
            download_url: "https://example.invalid/cloudflared.tgz".to_string(),
            sha256: crate::provider_installer::sha256_hex(&archive),
        };

        let status = install_manifest_to_tools_root_with_downloader(
            manifest.clone(),
            &tools_root,
            move |_| {
                let archive = archive.clone();
                async move { Ok(archive) }
            },
        )
        .await
        .expect("install should succeed");

        assert_eq!(
            status.state,
            crate::provider_installer::ProviderInstallState::Installed
        );
        assert_eq!(
            status.source,
            crate::provider_installer::ProviderInstallSource::LocalTools
        );
        assert!(Path::new(status.resolved_path.as_deref().expect("path")).exists());
    }

    #[tokio::test]
    async fn install_provider_to_local_tools_surfaces_download_failures() {
        let temp_dir = prepare_temp_dir();
        let tools_root = crate::provider_installer::tools_root_from_base_dir(&temp_dir);
        let manifest = crate::provider_installer::ProviderInstallManifestEntry {
            provider: TunnelProvider::Ngrok,
            version: "test-version".to_string(),
            binary_name: "ngrok".to_string(),
            archive_name: "ngrok-v3-3.37.1-darwin-arm64.tar.gz".to_string(),
            download_url: "https://example.invalid/ngrok.tgz".to_string(),
            sha256: "unused".to_string(),
        };

        let error =
            install_manifest_to_tools_root_with_downloader(manifest, &tools_root, |_| async {
                Err(anyhow::anyhow!("download exploded"))
            })
            .await
            .expect_err("download failure should surface");

        assert!(error.contains("download exploded"));
    }

    #[test]
    fn persisted_downloading_install_status_becomes_retryable_failure_after_restart() {
        let persisted = HashMap::from([(
            "cloudflared".to_string(),
            ProviderInstallStatus {
                state: crate::provider_installer::ProviderInstallState::Downloading,
                source: ProviderInstallSource::Missing,
                resolved_path: None,
                version: Some("2026.2.0".to_string()),
                last_error: None,
            },
        )]);

        let snapshot = normalize_persisted_provider_install_statuses(
            &build_provider_availability_snapshot(ProviderAvailabilityProbe::from_install_flags(
                false, false,
            )),
            &persisted,
        );

        assert_eq!(
            snapshot.get("cloudflared").map(|status| status.state),
            Some(crate::provider_installer::ProviderInstallState::Failed)
        );
        assert_eq!(
            snapshot
                .get("cloudflared")
                .and_then(|status| status.last_error.as_deref()),
            Some("Installation was interrupted before completion. Retry install.")
        );
    }

    #[tokio::test]
    async fn start_tunnel_returns_friendly_error_when_ngrok_authtoken_is_missing() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url: "http://127.0.0.1:9".to_string(),
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: None,
                    ngrok_domain: None,
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error = start_tunnel_from_settings_dir_with_provider_probe(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Ngrok,
                target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
            },
            ProviderAvailabilityProbe::from_install_flags(true, true),
        )
        .await
        .expect_err("missing ngrok authtoken should be caught before the daemon call");

        assert_eq!(error, "Add the ngrok authtoken on this tunnel, then retry.");
    }

    #[tokio::test]
    async fn start_tunnel_returns_friendly_error_when_ngrok_reserved_domain_is_invalid() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url: "http://127.0.0.1:9".to_string(),
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: Some("ngrok-token".to_string()),
                    ngrok_domain: Some("https://demo.ngrok.app/path".to_string()),
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error = start_tunnel_from_settings_dir_with_provider_probe(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Ngrok,
                target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
            },
            ProviderAvailabilityProbe::from_install_flags(true, true),
        )
        .await
        .expect_err("invalid reserved domain should be caught before the daemon call");

        assert_eq!(
            error,
            "Review the reserved domain on this tunnel. Use only the hostname, like demo.ngrok.app, without https://, paths, query strings, or ports."
        );
    }

    #[tokio::test]
    async fn start_tunnel_uses_saved_ngrok_tunnel_settings() {
        let temp_dir = prepare_temp_dir();
        let captured = std::sync::Arc::new(tokio::sync::Mutex::new(None::<TunnelStartRequest>));
        let base_url = spawn_test_server(Router::new().route(
            "/v1/tunnel/start",
            axum::routing::post({
                let captured = captured.clone();
                move |payload| start_tunnel_handler(captured.clone(), payload)
            }),
        ))
        .await;

        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    gateway_target_url: "http://127.0.0.1:28080".to_string(),
                    auto_restart: false,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: Some("ngrok-token".to_string()),
                    ngrok_domain: Some("demo.ngrok.app".to_string()),
                }],
            },
        )
        .expect("settings should save");

        let snapshot = start_tunnel_from_settings_dir_with_provider_probe(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Ngrok,
                target_url: "http://127.0.0.1:28080".to_string(),
                auto_restart: false,
            },
            ProviderAvailabilityProbe::from_install_flags(false, true),
        )
        .await
        .expect("start tunnel should succeed");

        assert!(snapshot.connected);

        let payload = loop {
            if let Some(payload) = captured.lock().await.clone() {
                break payload;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        };
        assert_eq!(payload.provider, TunnelProvider::Ngrok);
        assert_eq!(payload.target_url, "http://127.0.0.1:28080");
        assert_eq!(payload.auto_restart, Some(false));
        assert_eq!(
            payload
                .metadata
                .as_ref()
                .and_then(|value| value.get("ngrokAuthtoken"))
                .map(String::as_str),
            Some("ngrok-token")
        );
        assert_eq!(
            payload
                .metadata
                .as_ref()
                .and_then(|value| value.get("ngrokDomain"))
                .map(String::as_str),
            Some("demo.ngrok.app")
        );
    }

    #[tokio::test]
    async fn start_tunnel_uses_saved_cloudflared_tunnel_token() {
        let temp_dir = prepare_temp_dir();
        let captured = std::sync::Arc::new(tokio::sync::Mutex::new(None::<TunnelStartRequest>));
        let base_url = spawn_test_server(Router::new().route(
            "/v1/tunnel/start",
            axum::routing::post({
                let captured = captured.clone();
                move |payload| start_tunnel_handler(captured.clone(), payload)
            }),
        ))
        .await;

        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Cloudflared,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: Some("cf-token".to_string()),
                    ngrok_authtoken: None,
                    ngrok_domain: None,
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let snapshot = start_tunnel_from_settings_dir_with_provider_probe(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
            },
            ProviderAvailabilityProbe::from_install_flags(true, false),
        )
        .await
        .expect("start tunnel should succeed");

        assert!(snapshot.connected);

        let payload = captured
            .lock()
            .await
            .clone()
            .expect("start request should be captured");
        assert_eq!(
            payload
                .metadata
                .as_ref()
                .and_then(|value| value.get("cloudflaredTunnelToken"))
                .map(String::as_str),
            Some("cf-token")
        );
    }

    #[tokio::test]
    async fn start_tunnel_uses_local_provider_binary_path_when_available() {
        let temp_dir = prepare_temp_dir();
        let tools_bin = temp_dir.join("tools").join("bin");
        std::fs::create_dir_all(&tools_bin).expect("tools bin should be created");
        let local_cloudflared = tools_bin.join("cloudflared");
        write_fake_binary(&local_cloudflared);

        let captured = std::sync::Arc::new(tokio::sync::Mutex::new(None::<TunnelStartRequest>));
        let base_url = spawn_test_server(Router::new().route(
            "/v1/tunnel/start",
            axum::routing::post({
                let captured = captured.clone();
                move |payload| start_tunnel_handler(captured.clone(), payload)
            }),
        ))
        .await;

        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Cloudflared,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: None,
                    ngrok_domain: None,
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let probe = ProviderAvailabilityProbe::from_search_dirs_with_local_tools(
            Vec::new(),
            temp_dir.join("tools"),
            None,
        );

        let snapshot = start_tunnel_from_settings_dir_with_provider_probe(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
            },
            probe,
        )
        .await
        .expect("start tunnel should succeed");

        assert!(snapshot.connected);

        let payload = captured
            .lock()
            .await
            .clone()
            .expect("start request should be captured");
        assert_eq!(
            payload
                .metadata
                .as_ref()
                .and_then(|value| value.get("providerBinaryPath"))
                .map(String::as_str),
            local_cloudflared.to_str()
        );
    }

    #[test]
    fn build_tunnel_metadata_includes_system_provider_binary_path_for_default_local_daemon() {
        let settings = GuiSettings::default();
        let availability = ProviderBinaryAvailability::from_path(
            ProviderInstallSource::SystemPath,
            PathBuf::from("/opt/homebrew/bin/cloudflared"),
        );

        let metadata = build_tunnel_metadata(
            settings.current_tunnel(),
            &TunnelProvider::Cloudflared,
            &availability,
            true,
        )
        .expect("metadata should be built");

        assert_eq!(
            metadata.get("providerBinaryPath").map(String::as_str),
            Some("/opt/homebrew/bin/cloudflared")
        );
    }

    #[tokio::test]
    async fn refresh_dashboard_surfaces_stopped_tunnel_reason() {
        let temp_dir = prepare_temp_dir();
        let base_url = spawn_test_server(
            Router::new()
                .route("/v1/health", get(health_handler))
                .route("/v1/tunnel/status", get(stopped_tunnel_status_handler)),
        )
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let snapshot = refresh_dashboard_from_settings_dir(&temp_dir)
            .await
            .expect("dashboard refresh should succeed");

        assert!(snapshot.connected);
        assert_eq!(
            snapshot.message.as_deref(),
            Some("daemon restarted; previous tunnel process was detached")
        );
    }

    #[tokio::test]
    async fn refresh_dashboard_reports_running_cloudflared_named_tunnel_without_public_url() {
        let temp_dir = prepare_temp_dir();
        let base_url = spawn_test_server(
            Router::new()
                .route("/v1/health", get(health_handler))
                .route(
                    "/v1/tunnel/status",
                    get(running_named_tunnel_status_handler),
                ),
        )
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Cloudflared,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: Some("cf-token".to_string()),
                    ngrok_authtoken: None,
                    ngrok_domain: None,
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let snapshot = refresh_dashboard_from_settings_dir(&temp_dir)
            .await
            .expect("dashboard refresh should succeed");

        assert!(snapshot.connected);
        assert_eq!(
            snapshot.message.as_deref(),
            Some("Cloudflared named tunnel is running. Public hostname is managed in Cloudflare.")
        );
    }

    #[tokio::test]
    async fn list_routes_returns_onboarding_message_when_empty() {
        let temp_dir = prepare_temp_dir();
        let routes = std::sync::Arc::new(tokio::sync::Mutex::new(
            Vec::<tunnelmux_core::RouteRule>::new(),
        ));
        let base_url = spawn_routes_server(routes).await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let snapshot = list_routes_from_settings_dir(&temp_dir)
            .await
            .expect("list routes should succeed");

        assert!(snapshot.routes.is_empty());
        assert_eq!(
            snapshot.message.as_deref(),
            Some(
                "No services yet. Add your first local service to replace the default welcome page."
            )
        );
    }

    async fn health_handler() -> Json<HealthResponse> {
        Json(HealthResponse {
            ok: true,
            service: "tunnelmuxd".to_string(),
            version: "0.2.0".to_string(),
        })
    }

    async fn tunnel_status_handler() -> Json<TunnelStatusResponse> {
        Json(TunnelStatusResponse {
            tunnel_id: "primary".to_string(),
            tunnel: TunnelStatus {
                state: tunnelmux_core::TunnelState::Running,
                provider: Some(TunnelProvider::Cloudflared),
                target_url: Some("http://127.0.0.1:18080".to_string()),
                public_base_url: Some("https://demo.trycloudflare.com".to_string()),
                started_at: Some("2026-03-06T00:00:00Z".to_string()),
                updated_at: "2026-03-06T00:00:01Z".to_string(),
                process_id: Some(12345),
                auto_restart: true,
                restart_count: 0,
                last_error: None,
            },
        })
    }

    async fn stopped_tunnel_status_handler() -> Json<TunnelStatusResponse> {
        Json(TunnelStatusResponse {
            tunnel_id: "primary".to_string(),
            tunnel: TunnelStatus {
                state: tunnelmux_core::TunnelState::Stopped,
                provider: Some(TunnelProvider::Cloudflared),
                target_url: Some("http://127.0.0.1:48080".to_string()),
                public_base_url: None,
                started_at: Some("2026-03-06T00:00:00Z".to_string()),
                updated_at: "2026-03-07T00:00:01Z".to_string(),
                process_id: None,
                auto_restart: true,
                restart_count: 0,
                last_error: Some(
                    "daemon restarted; previous tunnel process was detached".to_string(),
                ),
            },
        })
    }

    async fn running_named_tunnel_status_handler() -> Json<TunnelStatusResponse> {
        Json(TunnelStatusResponse {
            tunnel_id: "primary".to_string(),
            tunnel: TunnelStatus {
                state: tunnelmux_core::TunnelState::Running,
                provider: Some(TunnelProvider::Cloudflared),
                target_url: Some("http://127.0.0.1:48080".to_string()),
                public_base_url: None,
                started_at: Some("2026-03-07T00:00:00Z".to_string()),
                updated_at: "2026-03-07T00:00:01Z".to_string(),
                process_id: Some(12345),
                auto_restart: true,
                restart_count: 0,
                last_error: None,
            },
        })
    }

    #[tokio::test]
    async fn save_route_creates_enabled_route_and_returns_fresh_list() {
        let temp_dir = prepare_temp_dir();
        let routes = std::sync::Arc::new(tokio::sync::Mutex::new(
            Vec::<tunnelmux_core::RouteRule>::new(),
        ));
        let base_url = spawn_routes_server(routes.clone()).await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let snapshot = save_route_from_settings_dir(
            &temp_dir,
            crate::view_models::RouteFormData {
                original_id: None,
                id: "svc-a".to_string(),
                match_host: "demo.local".to_string(),
                match_path_prefix: "/".to_string(),
                strip_path_prefix: String::new(),
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: String::new(),
                health_check_path: String::new(),
                enabled: true,
            },
        )
        .await
        .expect("save route should succeed");

        assert_eq!(snapshot.routes.len(), 1);
        assert_eq!(snapshot.routes[0].id, "svc-a");
        assert!(snapshot.routes[0].enabled);
        assert_eq!(routes.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn save_route_returns_friendly_duplicate_name_guidance() {
        let temp_dir = prepare_temp_dir();
        let base_url = spawn_test_server(
            Router::new().route("/v1/routes", post(create_route_conflict_handler)),
        )
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error = save_route_from_settings_dir(
            &temp_dir,
            crate::view_models::RouteFormData {
                upstream_url: "http://127.0.0.1:3000".to_string(),
                ..crate::view_models::RouteFormData::default()
            },
        )
        .await
        .expect_err("duplicate name should surface as guidance");

        assert_eq!(
            error,
            "Service Name \"local-3000\" is already in use for this tunnel. Rename it and save again."
        );
    }

    #[tokio::test]
    async fn save_route_returns_friendly_missing_name_guidance() {
        let temp_dir = prepare_temp_dir();
        let base_url = spawn_test_server(
            Router::new().route("/v1/routes", post(create_route_missing_id_handler)),
        )
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error =
            save_route_from_settings_dir(&temp_dir, crate::view_models::RouteFormData::default())
                .await
                .expect_err("missing name should surface as guidance");

        assert_eq!(
            error,
            "Add a Local Service URL or enter a Service Name before saving."
        );
    }

    #[tokio::test]
    async fn delete_route_returns_updated_route_list() {
        let temp_dir = prepare_temp_dir();
        let routes = std::sync::Arc::new(tokio::sync::Mutex::new(vec![
            tunnelmux_core::RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
            tunnelmux_core::RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/api".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:4000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: false,
            },
        ]));
        let base_url = spawn_routes_server(routes.clone()).await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let snapshot = delete_route_from_settings_dir(&temp_dir, "svc-a".to_string())
            .await
            .expect("delete route should succeed");

        assert_eq!(snapshot.routes.len(), 1);
        assert_eq!(snapshot.routes[0].id, "svc-b");
        assert_eq!(routes.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn load_diagnostics_summary_returns_connected_snapshot() {
        let temp_dir = prepare_temp_dir();
        let captured_query = std::sync::Arc::new(tokio::sync::Mutex::new(None::<String>));
        let base_url = spawn_test_server(Router::new().route(
            "/v1/diagnostics",
            get({
                let captured_query = captured_query.clone();
                move |query| diagnostics_handler(captured_query.clone(), query)
            }),
        ))
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                current_tunnel_id: Some("tunnel-2".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "tunnel-2".to_string(),
                    name: "API Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    gateway_target_url: "http://127.0.0.1:58080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: None,
                    ngrok_domain: None,
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let summary = load_diagnostics_summary_from_settings_dir(&temp_dir)
            .await
            .expect("diagnostics summary should load");

        assert_eq!(summary.route_count, 2);
        assert_eq!(summary.enabled_route_count, 1);
        assert_eq!(summary.tunnel_state, "running");
        assert!(summary.pending_restart);
        assert_eq!(captured_query.lock().await.as_deref(), Some("tunnel-2"));
    }

    #[tokio::test]
    async fn load_upstreams_health_maps_mixed_health_states() {
        let temp_dir = prepare_temp_dir();
        let captured_query = std::sync::Arc::new(tokio::sync::Mutex::new(None::<String>));
        let base_url = spawn_test_server(Router::new().route(
            "/v1/upstreams/health",
            get({
                let captured_query = captured_query.clone();
                move |query| upstreams_health_handler(captured_query.clone(), query)
            }),
        ))
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                current_tunnel_id: Some("tunnel-2".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "tunnel-2".to_string(),
                    name: "API Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    gateway_target_url: "http://127.0.0.1:58080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: None,
                    ngrok_domain: None,
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let upstreams = load_upstreams_health_from_settings_dir(&temp_dir)
            .await
            .expect("upstreams health should load");

        assert_eq!(upstreams.len(), 3);
        assert_eq!(upstreams[0].health_label, "healthy");
        assert_eq!(upstreams[1].health_label, "unhealthy");
        assert_eq!(upstreams[2].health_label, "unknown");
        assert_eq!(captured_query.lock().await.as_deref(), Some("tunnel-2"));
    }

    #[tokio::test]
    async fn load_recent_logs_returns_requested_tail_lines() {
        let temp_dir = prepare_temp_dir();
        let captured_query = std::sync::Arc::new(tokio::sync::Mutex::new(None::<String>));
        let base_url = spawn_test_server(Router::new().route(
            "/v1/tunnel/logs",
            get({
                let captured_query = captured_query.clone();
                move |query| recent_logs_handler(captured_query.clone(), query)
            }),
        ))
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                token: None,
                current_tunnel_id: Some("tunnel-2".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "tunnel-2".to_string(),
                    name: "API Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    gateway_target_url: "http://127.0.0.1:58080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: None,
                    ngrok_domain: None,
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let log_tail = load_recent_logs_from_settings_dir(&temp_dir, 25)
            .await
            .expect("recent logs should load");

        assert_eq!(log_tail.requested_lines, 25);
        assert_eq!(
            log_tail.lines,
            vec!["first log line".to_string(), "second log line".to_string()]
        );
        assert_eq!(captured_query.lock().await.as_deref(), Some("tunnel-2"));
    }

    #[tokio::test]
    async fn load_tunnel_workspace_returns_empty_when_tunnel_not_configured() {
        let temp_dir = prepare_temp_dir();

        let workspace = load_tunnel_workspace_from_settings_dir_without_daemon(&temp_dir)
            .expect("workspace should load");

        assert!(workspace.tunnels.is_empty());
        assert_eq!(workspace.current_tunnel_id, None);
    }

    #[tokio::test]
    async fn load_tunnel_workspace_returns_single_current_tunnel_when_configured() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Cloudflared,
                    ..crate::settings::TunnelProfileSettings::default()
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let workspace = load_tunnel_workspace_from_settings_dir_without_daemon(&temp_dir)
            .expect("workspace should load");

        assert_eq!(workspace.tunnels.len(), 1);
        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("primary"));
        assert_eq!(workspace.tunnels[0].name, "Main Tunnel");
        assert_eq!(workspace.tunnels[0].provider, "cloudflared");
        assert_eq!(workspace.tunnels[0].state, "idle");
        assert_eq!(workspace.tunnels[0].route_count, 0);
        assert_eq!(workspace.tunnels[0].enabled_route_count, 0);
    }

    #[test]
    fn load_tunnel_workspace_includes_provider_availability_for_each_tunnel() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                current_tunnel_id: Some("tunnel-2".to_string()),
                tunnels: vec![
                    crate::settings::TunnelProfileSettings {
                        id: "primary".to_string(),
                        name: "Main Tunnel".to_string(),
                        provider: TunnelProvider::Cloudflared,
                        ..crate::settings::TunnelProfileSettings::default()
                    },
                    crate::settings::TunnelProfileSettings {
                        id: "tunnel-2".to_string(),
                        name: "API Tunnel".to_string(),
                        provider: TunnelProvider::Ngrok,
                        ..crate::settings::TunnelProfileSettings::default()
                    },
                ],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let workspace = load_tunnel_workspace_from_settings_dir_without_daemon_with_provider_probe(
            &temp_dir,
            ProviderAvailabilityProbe::from_install_flags(true, false),
        )
        .expect("workspace should load");

        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("tunnel-2"));
        assert_eq!(workspace.tunnels.len(), 2);
        assert!(workspace.tunnels[0].provider_availability.installed);
        assert_eq!(
            workspace.tunnels[0].provider_availability.binary_name,
            "cloudflared"
        );
        assert_eq!(
            workspace.tunnels[0].provider_availability.source,
            "system_path"
        );
        assert!(!workspace.tunnels[1].provider_availability.installed);
        assert_eq!(
            workspace.tunnels[1].provider_availability.binary_name,
            "ngrok"
        );
        assert_eq!(workspace.tunnels[1].provider_availability.source, "missing");
    }

    #[test]
    fn provider_availability_probe_detects_binaries_from_explicit_path() {
        let temp_dir = prepare_temp_dir();
        write_fake_binary(&temp_dir.join("cloudflared"));

        let probe = ProviderAvailabilityProbe::from_env(Some(temp_dir.as_os_str()), None);

        assert!(probe.cloudflared.installed());
        assert!(!probe.ngrok.installed());
    }

    #[test]
    fn provider_availability_probe_prefers_local_tools_over_system_path() {
        let temp_dir = prepare_temp_dir();
        let tools_dir = temp_dir.join("tools").join("bin");
        let common_dir = temp_dir.join("common-bin");
        std::fs::create_dir_all(&tools_dir).expect("tools dir should be created");
        std::fs::create_dir_all(&common_dir).expect("common dir should be created");
        let local_binary = tools_dir.join("cloudflared");
        let system_binary = common_dir.join("cloudflared");
        write_fake_binary(&local_binary);
        write_fake_binary(&system_binary);

        let probe = ProviderAvailabilityProbe::from_search_dirs_with_local_tools(
            vec![common_dir],
            temp_dir.join("tools"),
            None,
        );
        let snapshot = build_provider_availability_snapshot(probe);

        assert!(snapshot.cloudflared.installed);
        assert_eq!(snapshot.cloudflared.source, "local_tools");
        assert_eq!(
            snapshot.cloudflared.resolved_path.as_deref(),
            local_binary.to_str()
        );
    }

    #[test]
    fn provider_availability_probe_detects_windows_pathext_binaries() {
        let temp_dir = prepare_temp_dir();
        write_fake_binary(&temp_dir.join("ngrok.CMD"));

        let probe = ProviderAvailabilityProbe::from_env(
            Some(temp_dir.as_os_str()),
            Some(OsStr::new(".EXE;.CMD")),
        );

        assert!(probe.ngrok.installed());
    }

    #[test]
    fn provider_availability_probe_detects_binaries_from_common_search_dirs() {
        let temp_dir = prepare_temp_dir();
        let common_dir = temp_dir.join("common-bin");
        std::fs::create_dir_all(&common_dir).expect("common dir should be created");
        write_fake_binary(&common_dir.join("cloudflared"));

        let probe = ProviderAvailabilityProbe::from_search_dirs(vec![common_dir], None);

        assert!(probe.cloudflared.installed());
        assert!(!probe.ngrok.installed());
    }

    #[test]
    fn provider_availability_probe_refreshes_when_path_contents_change() {
        let temp_dir = prepare_temp_dir();

        let before = ProviderAvailabilityProbe::from_env(Some(temp_dir.as_os_str()), None);
        assert!(!before.cloudflared.installed());
        assert!(!before.ngrok.installed());

        write_fake_binary(&temp_dir.join("cloudflared"));

        let after = ProviderAvailabilityProbe::from_env(Some(temp_dir.as_os_str()), None);
        let snapshot = build_provider_availability_snapshot(after);

        assert!(snapshot.cloudflared.installed);
        assert!(!snapshot.ngrok.installed);
    }

    #[test]
    fn provider_availability_snapshot_reports_both_supported_providers() {
        let snapshot = build_provider_availability_snapshot(
            ProviderAvailabilityProbe::from_install_flags(false, true),
        );

        assert_eq!(snapshot.cloudflared.binary_name, "cloudflared");
        assert!(!snapshot.cloudflared.installed);
        assert_eq!(snapshot.cloudflared.source, "missing");
        assert_eq!(snapshot.ngrok.binary_name, "ngrok");
        assert!(snapshot.ngrok.installed);
        assert_eq!(snapshot.ngrok.source, "system_path");
    }

    #[test]
    fn provider_availability_snapshot_surfaces_downloading_install_status() {
        let snapshot = merge_provider_install_statuses(
            build_provider_availability_snapshot(ProviderAvailabilityProbe::from_install_flags(
                false, false,
            )),
            &HashMap::from([(
                "cloudflared".to_string(),
                ProviderInstallStatus {
                    state: crate::provider_installer::ProviderInstallState::Downloading,
                    source: ProviderInstallSource::Missing,
                    resolved_path: None,
                    version: Some("2026.2.0".to_string()),
                    last_error: None,
                },
            )]),
        );

        assert_eq!(
            snapshot.cloudflared.install_state.as_deref(),
            Some("downloading")
        );
        assert_eq!(
            snapshot.cloudflared.install_version.as_deref(),
            Some("2026.2.0")
        );
        assert_eq!(snapshot.cloudflared.install_error, None);
    }

    #[test]
    fn provider_availability_snapshot_surfaces_failed_install_status() {
        let snapshot = merge_provider_install_statuses(
            build_provider_availability_snapshot(ProviderAvailabilityProbe::from_install_flags(
                false, false,
            )),
            &HashMap::from([(
                "cloudflared".to_string(),
                ProviderInstallStatus {
                    state: crate::provider_installer::ProviderInstallState::Failed,
                    source: ProviderInstallSource::Missing,
                    resolved_path: None,
                    version: Some("2026.2.0".to_string()),
                    last_error: Some("download exploded".to_string()),
                },
            )]),
        );

        assert_eq!(
            snapshot.cloudflared.install_state.as_deref(),
            Some("failed")
        );
        assert_eq!(
            snapshot.cloudflared.install_error.as_deref(),
            Some("download exploded")
        );
        assert_eq!(
            snapshot.cloudflared.install_version.as_deref(),
            Some("2026.2.0")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn provider_install_invocation_builds_macos_cloudflared_terminal_command() {
        let invocation = provider_install_invocation(&TunnelProvider::Cloudflared)
            .expect("cloudflared install invocation should exist");

        assert_eq!(invocation.program, "osascript");
        assert!(
            invocation
                .args
                .iter()
                .any(|arg| arg.contains("brew install cloudflared")),
            "unexpected args: {:?}",
            invocation.args
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn provider_install_invocation_builds_macos_ngrok_terminal_command() {
        let invocation = provider_install_invocation(&TunnelProvider::Ngrok)
            .expect("ngrok install invocation should exist");

        assert_eq!(invocation.program, "osascript");
        assert!(
            invocation
                .args
                .iter()
                .any(|arg| arg.contains("brew install ngrok/ngrok/ngrok")),
            "unexpected args: {:?}",
            invocation.args
        );
    }

    #[tokio::test]
    async fn load_tunnel_workspace_merges_daemon_summary_when_available() {
        let temp_dir = prepare_temp_dir();
        let base_url = spawn_test_server(
            Router::new()
                .route("/v1/health", get(health_handler))
                .route("/v1/tunnels/workspace", get(tunnel_workspace_handler)),
        )
        .await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                current_tunnel_id: Some("tunnel-2".to_string()),
                tunnels: vec![
                    crate::settings::TunnelProfileSettings {
                        id: "primary".to_string(),
                        name: "Main Tunnel".to_string(),
                        provider: TunnelProvider::Cloudflared,
                        ..crate::settings::TunnelProfileSettings::default()
                    },
                    crate::settings::TunnelProfileSettings {
                        id: "tunnel-2".to_string(),
                        name: "API Tunnel".to_string(),
                        provider: TunnelProvider::Ngrok,
                        ..crate::settings::TunnelProfileSettings::default()
                    },
                ],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let workspace = load_tunnel_workspace_from_settings_dir(&temp_dir)
            .await
            .expect("workspace should load");

        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("tunnel-2"));
        assert_eq!(workspace.tunnels.len(), 2);
        assert_eq!(workspace.tunnels[0].state, "running");
        assert_eq!(workspace.tunnels[0].route_count, 3);
        assert_eq!(
            workspace.tunnels[0].public_base_url.as_deref(),
            Some("https://demo.trycloudflare.com")
        );
        assert_eq!(workspace.tunnels[1].state, "stopped");
        assert_eq!(workspace.tunnels[1].public_base_url, None);
        assert_eq!(workspace.tunnels[1].enabled_route_count, 1);
    }

    #[test]
    fn save_tunnel_profile_adds_new_profile_and_selects_it() {
        let temp_dir = prepare_temp_dir();

        let workspace = save_tunnel_profile_to_settings_dir(
            &temp_dir,
            TunnelProfileInput {
                id: None,
                name: "Second Tunnel".to_string(),
                provider: TunnelProvider::Ngrok,
                gateway_target_url: "http://127.0.0.1:58080".to_string(),
                auto_restart: false,
                cloudflared_tunnel_token: None,
                ngrok_authtoken: Some("ngrok-token".to_string()),
                ngrok_domain: None,
            },
        )
        .expect("profile should save");

        assert_eq!(workspace.tunnels.len(), 1);
        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("tunnel-1"));
        assert_eq!(workspace.tunnels[0].name, "Second Tunnel");
        assert_eq!(workspace.tunnels[0].provider, "ngrok");
    }

    #[test]
    fn save_tunnel_profile_rejects_invalid_local_service_url_without_overwriting_profile() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Primary Tunnel".to_string(),
                    provider: TunnelProvider::Cloudflared,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    ..crate::settings::TunnelProfileSettings::default()
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error = save_tunnel_profile_to_settings_dir(
            &temp_dir,
            TunnelProfileInput {
                id: Some("primary".to_string()),
                name: "Primary Tunnel".to_string(),
                provider: TunnelProvider::Cloudflared,
                gateway_target_url: "not-a-url".to_string(),
                auto_restart: true,
                cloudflared_tunnel_token: None,
                ngrok_authtoken: None,
                ngrok_domain: None,
            },
        )
        .expect_err("invalid tunnel target should be rejected");

        assert_eq!(
            error,
            "Invalid Local Service URL. Review it on this tunnel, then retry."
        );

        let settings = load_settings_from_dir(&temp_dir).expect("settings should still load");
        assert_eq!(settings.current_tunnel_id.as_deref(), Some("primary"));
        assert_eq!(settings.tunnels.len(), 1);
        assert_eq!(
            settings.tunnels[0].gateway_target_url,
            "http://127.0.0.1:48080"
        );
    }

    #[test]
    fn save_tunnel_profile_rejects_invalid_ngrok_reserved_domain_without_overwriting_profile() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Primary Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: true,
                    cloudflared_tunnel_token: None,
                    ngrok_authtoken: Some("ngrok-token".to_string()),
                    ngrok_domain: Some("demo.ngrok.app".to_string()),
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error = save_tunnel_profile_to_settings_dir(
            &temp_dir,
            TunnelProfileInput {
                id: Some("primary".to_string()),
                name: "Primary Tunnel".to_string(),
                provider: TunnelProvider::Ngrok,
                gateway_target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
                cloudflared_tunnel_token: None,
                ngrok_authtoken: Some("ngrok-token".to_string()),
                ngrok_domain: Some("https://demo.ngrok.app/path".to_string()),
            },
        )
        .expect_err("invalid reserved domain should be rejected");

        assert_eq!(
            error,
            "Review the reserved domain on this tunnel. Use only the hostname, like demo.ngrok.app, without https://, paths, query strings, or ports."
        );

        let settings = load_settings_from_dir(&temp_dir).expect("settings should still load");
        assert_eq!(settings.current_tunnel_id.as_deref(), Some("primary"));
        assert_eq!(
            settings.tunnels[0].ngrok_domain.as_deref(),
            Some("demo.ngrok.app")
        );
    }

    #[test]
    fn save_tunnel_profile_updates_provider_without_dropping_easy_path_fields() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Cloudflared,
                    gateway_target_url: "http://127.0.0.1:48080".to_string(),
                    auto_restart: false,
                    cloudflared_tunnel_token: Some("cf-token".to_string()),
                    ..crate::settings::TunnelProfileSettings::default()
                }],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let workspace = save_tunnel_profile_to_settings_dir(
            &temp_dir,
            TunnelProfileInput {
                id: Some("primary".to_string()),
                name: "Main Tunnel".to_string(),
                provider: TunnelProvider::Ngrok,
                gateway_target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: false,
                cloudflared_tunnel_token: Some("cf-token".to_string()),
                ngrok_authtoken: None,
                ngrok_domain: None,
            },
        )
        .expect("profile should save");

        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("primary"));
        assert_eq!(workspace.tunnels.len(), 1);
        assert_eq!(workspace.tunnels[0].provider, "ngrok");
        assert_eq!(workspace.tunnels[0].name, "Main Tunnel");

        let settings = load_settings_from_dir(&temp_dir).expect("settings should load");
        assert_eq!(settings.tunnels[0].provider, TunnelProvider::Ngrok);
        assert_eq!(settings.tunnels[0].name, "Main Tunnel");
        assert_eq!(
            settings.tunnels[0].gateway_target_url,
            "http://127.0.0.1:48080"
        );
        assert!(!settings.tunnels[0].auto_restart);
    }

    #[test]
    fn select_tunnel_profile_switches_current_profile() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![
                    crate::settings::TunnelProfileSettings {
                        id: "primary".to_string(),
                        name: "Main Tunnel".to_string(),
                        provider: TunnelProvider::Cloudflared,
                        ..crate::settings::TunnelProfileSettings::default()
                    },
                    crate::settings::TunnelProfileSettings {
                        id: "tunnel-2".to_string(),
                        name: "API Tunnel".to_string(),
                        provider: TunnelProvider::Ngrok,
                        ..crate::settings::TunnelProfileSettings::default()
                    },
                ],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let workspace = select_tunnel_profile_from_settings_dir(&temp_dir, "tunnel-2")
            .expect("profile should switch");

        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("tunnel-2"));
    }

    #[tokio::test]
    async fn delete_tunnel_profile_removes_profile_and_cleans_daemon_state() {
        let temp_dir = prepare_temp_dir();
        let routes = std::sync::Arc::new(tokio::sync::Mutex::new(vec![
            tunnelmux_core::RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
            tunnelmux_core::RouteRule {
                tunnel_id: "tunnel-2".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/api".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:4000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        ]));
        let deleted_tunnels = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let base_url = spawn_tunnel_delete_server(routes.clone(), deleted_tunnels.clone()).await;
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url,
                current_tunnel_id: Some("tunnel-2".to_string()),
                tunnels: vec![
                    crate::settings::TunnelProfileSettings {
                        id: "primary".to_string(),
                        name: "Main Tunnel".to_string(),
                        provider: TunnelProvider::Cloudflared,
                        ..crate::settings::TunnelProfileSettings::default()
                    },
                    crate::settings::TunnelProfileSettings {
                        id: "tunnel-2".to_string(),
                        name: "API Tunnel".to_string(),
                        provider: TunnelProvider::Ngrok,
                        ..crate::settings::TunnelProfileSettings::default()
                    },
                ],
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let workspace = delete_tunnel_profile_from_settings_dir(&temp_dir, "tunnel-2")
            .await
            .expect("profile should delete");

        assert_eq!(workspace.tunnels.len(), 1);
        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("primary"));
        assert_eq!(
            deleted_tunnels.lock().await.as_slice(),
            &["tunnel-2".to_string()]
        );
        let remaining_routes = routes.lock().await.clone();
        assert_eq!(remaining_routes.len(), 1);
        assert_eq!(remaining_routes[0].tunnel_id, "primary");
    }

    #[test]
    fn provider_status_summary_identifies_named_cloudflared_setup() {
        let settings = GuiSettings {
            current_tunnel_id: Some("primary".to_string()),
            tunnels: vec![crate::settings::TunnelProfileSettings {
                id: "primary".to_string(),
                name: "Main Tunnel".to_string(),
                provider: TunnelProvider::Cloudflared,
                cloudflared_tunnel_token: Some("cf-token".to_string()),
                ..crate::settings::TunnelProfileSettings::default()
            }],
            ..GuiSettings::default()
        };
        let tunnel = TunnelStatus {
            state: tunnelmux_core::TunnelState::Running,
            provider: Some(TunnelProvider::Cloudflared),
            target_url: Some("http://127.0.0.1:48080".to_string()),
            public_base_url: None,
            started_at: Some("2026-03-07T00:00:00Z".to_string()),
            updated_at: "2026-03-07T00:00:01Z".to_string(),
            process_id: Some(12345),
            auto_restart: true,
            restart_count: 0,
            last_error: None,
        };

        let summary = derive_provider_status_summary(&settings, Some(&tunnel), &[], &[])
            .expect("summary should be derived");

        assert_eq!(summary.level, "warning");
        assert_eq!(summary.title, "Cloudflare Setup");
        assert_eq!(summary.action_kind.as_deref(), Some("open_cloudflare"));
        assert_eq!(summary.action_label.as_deref(), Some("Open Cloudflare"));
        assert_eq!(
            summary.follow_up_action_kind.as_deref(),
            Some("open_cloudflare_docs")
        );
        assert_eq!(
            summary.follow_up_action_label.as_deref(),
            Some("Setup Hostname")
        );
    }

    #[test]
    fn provider_status_summary_classifies_named_cloudflared_setup_errors() {
        let settings = GuiSettings {
            current_tunnel_id: Some("primary".to_string()),
            tunnels: vec![crate::settings::TunnelProfileSettings {
                id: "primary".to_string(),
                name: "Main Tunnel".to_string(),
                provider: TunnelProvider::Cloudflared,
                cloudflared_tunnel_token: Some("cf-token".to_string()),
                ..crate::settings::TunnelProfileSettings::default()
            }],
            ..GuiSettings::default()
        };
        let tunnel = TunnelStatus {
            state: tunnelmux_core::TunnelState::Starting,
            provider: Some(TunnelProvider::Cloudflared),
            target_url: Some("http://127.0.0.1:48080".to_string()),
            public_base_url: None,
            started_at: None,
            updated_at: "2026-03-07T00:00:01Z".to_string(),
            process_id: None,
            auto_restart: true,
            restart_count: 0,
            last_error: None,
        };

        let summary = derive_provider_status_summary(
            &settings,
            Some(&tunnel),
            &[String::from(
                "Unauthorized: failed to get tunnel credentials",
            )],
            &[],
        )
        .expect("summary should be derived");

        assert_eq!(summary.level, "error");
        assert_eq!(summary.title, "Cloudflare Setup");
        assert_eq!(
            summary.message,
            "Cloudflare rejected the named tunnel setup. Review the tunnel token on this tunnel and make sure a hostname is configured in Cloudflare."
        );
        assert_eq!(summary.action_kind.as_deref(), Some("open_cloudflare"));
        assert_eq!(summary.action_label.as_deref(), Some("Open Cloudflare"));
        assert_eq!(
            summary.follow_up_action_kind.as_deref(),
            Some("open_cloudflare_docs")
        );
        assert_eq!(
            summary.follow_up_action_label.as_deref(),
            Some("Setup Hostname")
        );
    }

    #[test]
    fn provider_status_summary_focuses_ngrok_authtoken_recovery_on_tunnel_editor() {
        let tunnel = TunnelStatus {
            state: tunnelmux_core::TunnelState::Starting,
            provider: Some(TunnelProvider::Ngrok),
            target_url: Some("http://127.0.0.1:48080".to_string()),
            public_base_url: None,
            started_at: None,
            updated_at: "2026-03-07T00:00:01Z".to_string(),
            process_id: None,
            auto_restart: true,
            restart_count: 0,
            last_error: None,
        };

        let summary = derive_provider_status_summary(
            &GuiSettings {
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    ..crate::settings::TunnelProfileSettings::default()
                }],
                ..GuiSettings::default()
            },
            Some(&tunnel),
            &[String::from(
                "t=2026-03-07 lvl=eror msg=\"failed\" err=\"ERR_NGROK_4018\"",
            )],
            &[],
        )
        .expect("summary should be derived");

        assert_eq!(summary.level, "error");
        assert_eq!(summary.title, "ngrok Authtoken Needed");
        assert!(summary.message.contains("ERR_NGROK_4018"));
        assert_eq!(summary.action_kind.as_deref(), Some("edit_tunnel"));
        assert_eq!(summary.action_label.as_deref(), Some("Add ngrok Authtoken"));
        assert_eq!(summary.action_payload.as_deref(), Some("ngrok_authtoken"));
    }

    #[test]
    fn provider_status_summary_focuses_ngrok_domain_recovery_on_tunnel_editor() {
        let tunnel = TunnelStatus {
            state: tunnelmux_core::TunnelState::Starting,
            provider: Some(TunnelProvider::Ngrok),
            target_url: Some("http://127.0.0.1:48080".to_string()),
            public_base_url: None,
            started_at: None,
            updated_at: "2026-03-07T00:00:01Z".to_string(),
            process_id: None,
            auto_restart: true,
            restart_count: 0,
            last_error: None,
        };

        let summary = derive_provider_status_summary(
            &GuiSettings {
                current_tunnel_id: Some("primary".to_string()),
                tunnels: vec![crate::settings::TunnelProfileSettings {
                    id: "primary".to_string(),
                    name: "Main Tunnel".to_string(),
                    provider: TunnelProvider::Ngrok,
                    ngrok_authtoken: Some("ngrok-token".to_string()),
                    ngrok_domain: Some("demo.ngrok.app".to_string()),
                    ..crate::settings::TunnelProfileSettings::default()
                }],
                ..GuiSettings::default()
            },
            Some(&tunnel),
            &[String::from(
                "t=2026-03-07 lvl=eror msg=\"failed\" err=\"domain already reserved ERR_NGROK_3004\"",
            )],
            &[],
        )
        .expect("summary should be derived");

        assert_eq!(summary.level, "error");
        assert_eq!(summary.title, "ngrok Domain Check");
        assert_eq!(summary.action_kind.as_deref(), Some("edit_tunnel"));
        assert_eq!(summary.action_label.as_deref(), Some("Review ngrok Domain"));
        assert_eq!(summary.action_payload.as_deref(), Some("ngrok_domain"));
    }

    #[test]
    fn provider_status_summary_identifies_unreachable_origin() {
        let tunnel = TunnelStatus {
            state: tunnelmux_core::TunnelState::Running,
            provider: Some(TunnelProvider::Cloudflared),
            target_url: Some("http://127.0.0.1:48080".to_string()),
            public_base_url: Some("https://demo.trycloudflare.com".to_string()),
            started_at: Some("2026-03-07T00:00:00Z".to_string()),
            updated_at: "2026-03-07T00:00:01Z".to_string(),
            process_id: Some(12345),
            auto_restart: true,
            restart_count: 0,
            last_error: None,
        };

        let summary = derive_provider_status_summary(
            &GuiSettings::default(),
            Some(&tunnel),
            &[String::from("Unable to reach the origin service. The service may be down or it may not be responding to traffic from cloudflared")],
            &[],
        )
        .expect("summary should be derived");

        assert_eq!(summary.level, "warning");
        assert_eq!(summary.title, "Local Service Unreachable");
        assert_eq!(summary.action_kind.as_deref(), Some("review_services"));
        assert_eq!(summary.action_label.as_deref(), Some("Review Services"));
    }

    #[test]
    fn provider_status_summary_edits_single_unreachable_service_directly() {
        let tunnel = TunnelStatus {
            state: tunnelmux_core::TunnelState::Running,
            provider: Some(TunnelProvider::Cloudflared),
            target_url: Some("http://127.0.0.1:48080".to_string()),
            public_base_url: Some("https://demo.trycloudflare.com".to_string()),
            started_at: Some("2026-03-07T00:00:00Z".to_string()),
            updated_at: "2026-03-07T00:00:01Z".to_string(),
            process_id: Some(12345),
            auto_restart: true,
            restart_count: 0,
            last_error: None,
        };
        let routes = vec![tunnelmux_core::RouteRule {
            tunnel_id: "primary".to_string(),
            id: "demo-web".to_string(),
            match_host: None,
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: None,
            health_check_path: None,
            enabled: true,
        }];

        let summary = derive_provider_status_summary(
            &GuiSettings::default(),
            Some(&tunnel),
            &[String::from("Unable to reach the origin service. The service may be down or it may not be responding to traffic from cloudflared")],
            &routes,
        )
        .expect("summary should be derived");

        assert_eq!(summary.action_kind.as_deref(), Some("edit_service"));
        assert_eq!(summary.action_label.as_deref(), Some("Edit Service"));
        assert_eq!(summary.action_payload.as_deref(), Some("demo-web"));
    }

    #[test]
    fn provider_status_summary_keeps_multi_service_unreachable_recovery_lightweight() {
        let tunnel = TunnelStatus {
            state: tunnelmux_core::TunnelState::Running,
            provider: Some(TunnelProvider::Cloudflared),
            target_url: Some("http://127.0.0.1:48080".to_string()),
            public_base_url: Some("https://demo.trycloudflare.com".to_string()),
            started_at: Some("2026-03-07T00:00:00Z".to_string()),
            updated_at: "2026-03-07T00:00:01Z".to_string(),
            process_id: Some(12345),
            auto_restart: true,
            restart_count: 0,
            last_error: None,
        };
        let routes = vec![
            tunnelmux_core::RouteRule {
                tunnel_id: "primary".to_string(),
                id: "demo-web".to_string(),
                match_host: None,
                match_path_prefix: Some("/".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
            tunnelmux_core::RouteRule {
                tunnel_id: "primary".to_string(),
                id: "demo-api".to_string(),
                match_host: None,
                match_path_prefix: Some("/api".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3001".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        ];

        let summary = derive_provider_status_summary(
            &GuiSettings::default(),
            Some(&tunnel),
            &[String::from("Unable to reach the origin service. The service may be down or it may not be responding to traffic from cloudflared")],
            &routes,
        )
        .expect("summary should be derived");

        assert_eq!(summary.action_kind.as_deref(), Some("review_services"));
        assert_eq!(summary.action_label.as_deref(), Some("Review Services"));
        assert_eq!(summary.action_payload, None);
    }

    #[tokio::test]
    async fn diagnostics_commands_surface_connection_errors_cleanly() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url: "http://127.0.0.1:9".to_string(),
                token: None,
                ..GuiSettings::default()
            },
        )
        .expect("settings should save");

        let error = load_diagnostics_summary_from_settings_dir(&temp_dir)
            .await
            .expect_err("diagnostics summary should fail against unreachable daemon");

        assert!(
            error.contains("request failed") || error.contains("error sending request"),
            "unexpected error: {error}"
        );
    }

    async fn diagnostics_handler(
        captured_query: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<tunnelmux_core::DiagnosticsResponse> {
        *captured_query.lock().await = query.get("tunnel_id").cloned();
        Json(tunnelmux_core::DiagnosticsResponse {
            data_file: "/tmp/state.json".to_string(),
            config_file: "/tmp/config.json".to_string(),
            provider_log_file: "/tmp/provider.log".to_string(),
            route_count: 2,
            enabled_route_count: 1,
            tunnel_state: tunnelmux_core::TunnelState::Running,
            pending_restart: true,
            config_reload_enabled: true,
            config_reload_interval_ms: 1000,
            last_config_reload_at: Some("2026-03-06T10:00:00Z".to_string()),
            last_config_reload_error: None,
        })
    }

    async fn tunnel_workspace_handler() -> Json<tunnelmux_core::TunnelWorkspaceResponse> {
        Json(tunnelmux_core::TunnelWorkspaceResponse {
            current_tunnel_id: Some("tunnel-2".to_string()),
            tunnels: vec![
                tunnelmux_core::TunnelProfileSummary {
                    id: "primary".to_string(),
                    name: Some("Main Tunnel".to_string()),
                    provider: Some(TunnelProvider::Cloudflared),
                    state: tunnelmux_core::TunnelState::Running,
                    target_url: Some("http://127.0.0.1:48080".to_string()),
                    public_base_url: Some("https://demo.trycloudflare.com".to_string()),
                    route_count: 3,
                    enabled_route_count: 2,
                },
                tunnelmux_core::TunnelProfileSummary {
                    id: "tunnel-2".to_string(),
                    name: Some("API Tunnel".to_string()),
                    provider: Some(TunnelProvider::Ngrok),
                    state: tunnelmux_core::TunnelState::Stopped,
                    target_url: Some("http://127.0.0.1:58080".to_string()),
                    public_base_url: None,
                    route_count: 2,
                    enabled_route_count: 1,
                },
            ],
        })
    }

    async fn upstreams_health_handler(
        captured_query: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<tunnelmux_core::UpstreamsHealthResponse> {
        *captured_query.lock().await = query.get("tunnel_id").cloned();
        Json(tunnelmux_core::UpstreamsHealthResponse {
            upstreams: vec![
                tunnelmux_core::UpstreamHealthEntry {
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    health_check_path: "/healthz".to_string(),
                    healthy: Some(true),
                    last_checked_at: Some("2026-03-06T10:00:00Z".to_string()),
                    last_error: None,
                },
                tunnelmux_core::UpstreamHealthEntry {
                    upstream_url: "http://127.0.0.1:3001".to_string(),
                    health_check_path: "/healthz".to_string(),
                    healthy: Some(false),
                    last_checked_at: Some("2026-03-06T10:00:01Z".to_string()),
                    last_error: Some("status 503".to_string()),
                },
                tunnelmux_core::UpstreamHealthEntry {
                    upstream_url: "http://127.0.0.1:3002".to_string(),
                    health_check_path: "/healthz".to_string(),
                    healthy: None,
                    last_checked_at: None,
                    last_error: None,
                },
            ],
        })
    }

    async fn recent_logs_handler(
        captured_query: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
        Query(query): Query<std::collections::HashMap<String, String>>,
    ) -> Json<tunnelmux_core::TunnelLogsResponse> {
        *captured_query.lock().await = query.get("tunnel_id").cloned();
        Json(tunnelmux_core::TunnelLogsResponse {
            lines: vec!["first log line".to_string(), "second log line".to_string()],
        })
    }

    async fn start_tunnel_handler(
        captured: std::sync::Arc<tokio::sync::Mutex<Option<TunnelStartRequest>>>,
        Json(request): Json<TunnelStartRequest>,
    ) -> Json<TunnelStatusResponse> {
        *captured.lock().await = Some(request.clone());
        Json(TunnelStatusResponse {
            tunnel_id: request.tunnel_id.clone(),
            tunnel: TunnelStatus {
                state: tunnelmux_core::TunnelState::Running,
                provider: Some(request.provider),
                target_url: Some(request.target_url),
                public_base_url: Some("https://demo.ngrok.app".to_string()),
                started_at: Some("2026-03-06T00:00:00Z".to_string()),
                updated_at: "2026-03-06T00:00:01Z".to_string(),
                process_id: Some(12345),
                auto_restart: request.auto_restart.unwrap_or(true),
                restart_count: 0,
                last_error: None,
            },
        })
    }

    async fn spawn_test_server(app: Router) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address: SocketAddr = listener.local_addr().expect("local addr should resolve");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });
        format!("http://{}", address)
    }

    async fn spawn_routes_server(
        routes: std::sync::Arc<tokio::sync::Mutex<Vec<tunnelmux_core::RouteRule>>>,
    ) -> String {
        let app = Router::new()
            .route(
                "/v1/routes",
                get(list_routes_handler).post(create_route_handler),
            )
            .route(
                "/v1/routes/{id}",
                axum::routing::delete(delete_route_handler).put(update_route_handler),
            )
            .with_state(routes);
        spawn_test_server(app).await
    }

    async fn spawn_tunnel_delete_server(
        routes: std::sync::Arc<tokio::sync::Mutex<Vec<tunnelmux_core::RouteRule>>>,
        deleted_tunnels: std::sync::Arc<tokio::sync::Mutex<Vec<String>>>,
    ) -> String {
        let app = Router::new()
            .route("/v1/health", get(health_handler))
            .route(
                "/v1/tunnel/delete",
                axum::routing::post({
                    let deleted_tunnels = deleted_tunnels.clone();
                    let routes = routes.clone();
                    move |payload| {
                        delete_tunnel_handler(deleted_tunnels.clone(), routes.clone(), payload)
                    }
                }),
            )
            .with_state(routes);
        spawn_test_server(app).await
    }

    async fn list_routes_handler(
        tauri_state: axum::extract::State<
            std::sync::Arc<tokio::sync::Mutex<Vec<tunnelmux_core::RouteRule>>>,
        >,
    ) -> Json<tunnelmux_core::RoutesResponse> {
        let routes = tauri_state.0.lock().await.clone();
        Json(tunnelmux_core::RoutesResponse { routes })
    }

    async fn create_route_handler(
        tauri_state: axum::extract::State<
            std::sync::Arc<tokio::sync::Mutex<Vec<tunnelmux_core::RouteRule>>>,
        >,
        Json(request): Json<tunnelmux_core::CreateRouteRequest>,
    ) -> Json<tunnelmux_core::RouteRule> {
        let route = tunnelmux_core::RouteRule {
            tunnel_id: request.tunnel_id,
            id: request.id,
            match_host: request.match_host,
            match_path_prefix: request.match_path_prefix,
            strip_path_prefix: request.strip_path_prefix,
            upstream_url: request.upstream_url,
            fallback_upstream_url: request.fallback_upstream_url,
            health_check_path: request.health_check_path,
            enabled: request.enabled.unwrap_or(true),
        };
        tauri_state.0.lock().await.push(route.clone());
        Json(route)
    }

    async fn create_route_conflict_handler(
        Json(request): Json<tunnelmux_core::CreateRouteRequest>,
    ) -> Result<
        Json<tunnelmux_core::RouteRule>,
        (axum::http::StatusCode, Json<tunnelmux_core::ErrorResponse>),
    > {
        Err((
            axum::http::StatusCode::CONFLICT,
            Json(tunnelmux_core::ErrorResponse {
                error: format!(
                    "route '{}' already exists in tunnel '{}'",
                    request.id, request.tunnel_id
                ),
            }),
        ))
    }

    async fn create_route_missing_id_handler(
        _request: Json<tunnelmux_core::CreateRouteRequest>,
    ) -> Result<
        Json<tunnelmux_core::RouteRule>,
        (axum::http::StatusCode, Json<tunnelmux_core::ErrorResponse>),
    > {
        Err((
            axum::http::StatusCode::BAD_REQUEST,
            Json(tunnelmux_core::ErrorResponse {
                error: "route id is required".to_string(),
            }),
        ))
    }

    async fn update_route_handler(
        axum::extract::Path(id): axum::extract::Path<String>,
        tauri_state: axum::extract::State<
            std::sync::Arc<tokio::sync::Mutex<Vec<tunnelmux_core::RouteRule>>>,
        >,
        Json(request): Json<tunnelmux_core::CreateRouteRequest>,
    ) -> Result<
        Json<tunnelmux_core::RouteRule>,
        (axum::http::StatusCode, Json<tunnelmux_core::ErrorResponse>),
    > {
        let route = tunnelmux_core::RouteRule {
            tunnel_id: request.tunnel_id,
            id: request.id,
            match_host: request.match_host,
            match_path_prefix: request.match_path_prefix,
            strip_path_prefix: request.strip_path_prefix,
            upstream_url: request.upstream_url,
            fallback_upstream_url: request.fallback_upstream_url,
            health_check_path: request.health_check_path,
            enabled: request.enabled.unwrap_or(true),
        };
        let mut routes = tauri_state.0.lock().await;
        if let Some(existing) = routes.iter_mut().find(|item| item.id == id) {
            *existing = route.clone();
            return Ok(Json(route));
        }
        routes.push(route.clone());
        Ok(Json(route))
    }

    async fn delete_route_handler(
        axum::extract::Path(id): axum::extract::Path<String>,
        tauri_state: axum::extract::State<
            std::sync::Arc<tokio::sync::Mutex<Vec<tunnelmux_core::RouteRule>>>,
        >,
    ) -> Result<
        Json<tunnelmux_core::DeleteRouteResponse>,
        (axum::http::StatusCode, Json<tunnelmux_core::ErrorResponse>),
    > {
        let mut routes = tauri_state.0.lock().await;
        let before = routes.len();
        routes.retain(|item| item.id != id);
        if routes.len() == before {
            return Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(tunnelmux_core::ErrorResponse {
                    error: "route not found".to_string(),
                }),
            ));
        }
        Ok(Json(tunnelmux_core::DeleteRouteResponse { removed: true }))
    }

    async fn delete_tunnel_handler(
        deleted_tunnels: std::sync::Arc<tokio::sync::Mutex<Vec<String>>>,
        routes: std::sync::Arc<tokio::sync::Mutex<Vec<tunnelmux_core::RouteRule>>>,
        Json(request): Json<tunnelmux_core::TunnelDeleteRequest>,
    ) -> Json<tunnelmux_core::DeleteTunnelResponse> {
        deleted_tunnels.lock().await.push(request.tunnel_id.clone());
        routes
            .lock()
            .await
            .retain(|route| route.tunnel_id != request.tunnel_id);
        Json(tunnelmux_core::DeleteTunnelResponse { removed: true })
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
        std::fs::write(path, "#!/bin/sh\nexit 0\n").expect("fake binary should be written");

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

    fn build_test_provider_archive_bytes(binary_name: &str, binary_bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = Builder::new(&mut encoder);
            let mut header = Header::new_gnu();
            header.set_size(binary_bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, binary_name, binary_bytes)
                .expect("archive entry should append");
            builder.finish().expect("archive should finish");
        }
        encoder.finish().expect("gzip encoder should finish")
    }

    fn next_temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "tunnelmux-gui-commands-{}",
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
