use crate::daemon_manager::{self, DaemonStatusSnapshot};
use crate::settings::{GuiSettings, load_settings_from_dir, save_settings_to_dir};
use crate::state::GuiAppState;
use crate::view_models::{
    DiagnosticsSummaryVm, LogTailVm, ProviderStatusVm, RouteFormData, RouteWorkspaceSnapshot,
    TunnelProfileVm, TunnelWorkspaceVm, UpstreamHealthVm,
};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::Manager;
use tunnelmux_control_client::{ControlClientConfig, TunnelmuxControlClient};
use tunnelmux_core::{HealthResponse, TunnelProvider, TunnelStartRequest, TunnelStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub message: Option<String>,
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
    daemon_manager::ensure_local_daemon(app, &state.daemon_runtime, &settings)
        .await
        .map_err(command_error)
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
pub fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    settings: GuiSettings,
) -> Result<GuiSettings, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    save_settings_to_dir(&settings_dir, &settings).map_err(command_error)?;
    load_settings_from_dir(&settings_dir).map_err(command_error)
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
pub fn load_tunnel_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
) -> Result<TunnelWorkspaceVm, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    load_tunnel_workspace_from_settings_dir(&settings_dir)
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
pub fn delete_tunnel_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, GuiAppState>,
    id: String,
) -> Result<TunnelWorkspaceVm, String> {
    let settings_dir = resolve_settings_dir(&app, state.inner())?;
    delete_tunnel_profile_from_settings_dir(&settings_dir, &id)
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
    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.clone())
        .ok_or_else(|| "no tunnel selected".to_string())?;
    let metadata = build_tunnel_metadata(settings.current_tunnel(), &input.provider);
    let response = client
        .start_tunnel(&TunnelStartRequest {
            tunnel_id,
            provider: input.provider,
            target_url: input.target_url,
            auto_restart: Some(input.auto_restart),
            metadata,
        })
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

pub async fn stop_tunnel_from_settings_dir(
    settings_dir: &Path,
) -> Result<DashboardSnapshot, String> {
    let (settings, client) = load_client(settings_dir)?;
    let tunnel_id = settings
        .current_tunnel()
        .map(|tunnel| tunnel.id.clone())
        .ok_or_else(|| "no tunnel selected".to_string())?;
    let response = client.stop_tunnel(&tunnel_id).await.map_err(command_error)?;

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
            .map_err(command_error)?;
    } else {
        client.create_route(&request).await.map_err(command_error)?;
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
    Ok(derive_provider_status_summary(
        &settings,
        tunnel.as_ref(),
        &log_lines,
    ))
}

pub fn load_tunnel_workspace_from_settings_dir(
    settings_dir: &Path,
) -> Result<TunnelWorkspaceVm, String> {
    let settings = load_settings_from_dir(settings_dir).map_err(command_error)?;
    if settings.tunnels.is_empty() {
        return Ok(TunnelWorkspaceVm {
            tunnels: Vec::new(),
            current_tunnel_id: None,
        });
    }

    let tunnels = settings
        .tunnels
        .iter()
        .map(|tunnel| TunnelProfileVm {
            id: tunnel.id.clone(),
            name: tunnel.name.clone(),
            provider: match tunnel.provider {
                TunnelProvider::Cloudflared => "cloudflared".to_string(),
                TunnelProvider::Ngrok => "ngrok".to_string(),
            },
        })
        .collect();

    Ok(TunnelWorkspaceVm {
        tunnels,
        current_tunnel_id: settings.current_tunnel_id.clone(),
    })
}

pub fn save_tunnel_profile_to_settings_dir(
    settings_dir: &Path,
    profile: TunnelProfileInput,
) -> Result<TunnelWorkspaceVm, String> {
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

    if let Some(index) = settings.tunnels.iter().position(|item| item.id == tunnel_id) {
        settings.tunnels[index] = next_profile;
    } else {
        settings.tunnels.push(next_profile);
    }
    settings.current_tunnel_id = Some(tunnel_id);
    save_settings_to_dir(settings_dir, &settings).map_err(command_error)?;
    load_tunnel_workspace_from_settings_dir(settings_dir)
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
    load_tunnel_workspace_from_settings_dir(settings_dir)
}

pub fn delete_tunnel_profile_from_settings_dir(
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
    load_tunnel_workspace_from_settings_dir(settings_dir)
}

fn next_tunnel_profile_id(
    profiles: &[crate::settings::TunnelProfileSettings],
) -> String {
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
        return Some(ProviderStatusVm::new(
            "warning",
            "Cloudflare Setup",
            "Named tunnel connected. Configure hostname and Access in Cloudflare.",
        )
        .with_action("open_cloudflare", "Open Cloudflare"));
    }

    for line in log_lines.iter().rev() {
        if let Some(summary) = classify_provider_log_line(&provider, line) {
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
    line: &str,
) -> Option<ProviderStatusVm> {
    let lower = line.to_ascii_lowercase();

    if lower.contains("unable to reach the origin service")
        || lower.contains("connection refused")
        || lower.contains("upstream request failed")
    {
        return Some(ProviderStatusVm::new(
            "warning",
            "Local Service Unreachable",
            "Tunnel is up, but the local service did not respond. Check the target URL and make sure your app is running.",
        )
        .with_action("review_services", "Review Services"));
    }

    if *provider == TunnelProvider::Ngrok {
        if let Some(code) = line
            .split(|value: char| !value.is_ascii_alphanumeric() && value != '_')
            .find(|token| token.starts_with("ERR_NGROK_"))
        {
            return Some(ProviderStatusVm::new(
                "error",
                "ngrok Error",
                &format!("{code}. Check your authtoken, domain, or ngrok account settings."),
            )
            .with_action("open_settings", "Open Settings"));
        }
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

fn build_tunnel_metadata(
    tunnel: Option<&crate::settings::TunnelProfileSettings>,
    provider: &TunnelProvider,
) -> Option<HashMap<String, String>> {
    let mut metadata = HashMap::new();

    match provider {
        TunnelProvider::Cloudflared => {
            if let Some(value) = tunnel.and_then(|tunnel| tunnel.cloudflared_tunnel_token.as_deref()) {
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
    use axum::{Json, Router, extract::Query, routing::get};
    use std::{
        net::SocketAddr,
        sync::atomic::{AtomicU64, Ordering},
    };
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

        let error = start_tunnel_from_settings_dir(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:18080".to_string(),
                auto_restart: true,
            },
        )
        .await
        .expect_err("start should fail against unreachable daemon");

        assert!(
            error.contains("request failed") || error.contains("error sending request"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn start_tunnel_uses_saved_ngrok_tunnel_settings() {
        let temp_dir = prepare_temp_dir();
        let captured = std::sync::Arc::new(tokio::sync::Mutex::new(None::<TunnelStartRequest>));
        let base_url = spawn_test_server(
            Router::new().route(
                "/v1/tunnel/start",
                axum::routing::post({
                    let captured = captured.clone();
                    move |payload| start_tunnel_handler(captured.clone(), payload)
                }),
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

        let snapshot = start_tunnel_from_settings_dir(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Ngrok,
                target_url: "http://127.0.0.1:28080".to_string(),
                auto_restart: false,
            },
        )
        .await
        .expect("start tunnel should succeed");

        assert!(snapshot.connected);

        let payload = captured
            .lock()
            .await
            .clone()
            .expect("start request should be captured");
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
        let base_url = spawn_test_server(
            Router::new().route(
                "/v1/tunnel/start",
                axum::routing::post({
                    let captured = captured.clone();
                    move |payload| start_tunnel_handler(captured.clone(), payload)
                }),
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

        let snapshot = start_tunnel_from_settings_dir(
            &temp_dir,
            StartTunnelInput {
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
            },
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
                .route("/v1/tunnel/status", get(running_named_tunnel_status_handler)),
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
            Some("No services yet. Add your first local service to replace the default welcome page.")
        );
    }

    async fn health_handler() -> Json<HealthResponse> {
        Json(HealthResponse {
            ok: true,
            service: "tunnelmuxd".to_string(),
            version: "0.1.5".to_string(),
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
                last_error: Some("daemon restarted; previous tunnel process was detached".to_string()),
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
        assert_eq!(
            captured_query.lock().await.as_deref(),
            Some("tunnel-2")
        );
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
        assert_eq!(
            captured_query.lock().await.as_deref(),
            Some("tunnel-2")
        );
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
        assert_eq!(
            captured_query.lock().await.as_deref(),
            Some("tunnel-2")
        );
    }

    #[test]
    fn load_tunnel_workspace_returns_empty_when_tunnel_not_configured() {
        let temp_dir = prepare_temp_dir();

        let workspace = load_tunnel_workspace_from_settings_dir(&temp_dir)
            .expect("workspace should load");

        assert!(workspace.tunnels.is_empty());
        assert_eq!(workspace.current_tunnel_id, None);
    }

    #[test]
    fn load_tunnel_workspace_returns_single_current_tunnel_when_configured() {
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

        let workspace = load_tunnel_workspace_from_settings_dir(&temp_dir)
            .expect("workspace should load");

        assert_eq!(workspace.tunnels.len(), 1);
        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("primary"));
        assert_eq!(workspace.tunnels[0].name, "Main Tunnel");
        assert_eq!(workspace.tunnels[0].provider, "cloudflared");
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

    #[test]
    fn delete_tunnel_profile_removes_profile_and_falls_back_to_first() {
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

        let workspace = delete_tunnel_profile_from_settings_dir(&temp_dir, "tunnel-2")
            .expect("profile should delete");

        assert_eq!(workspace.tunnels.len(), 1);
        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("primary"));
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

        let summary = derive_provider_status_summary(&settings, Some(&tunnel), &[])
            .expect("summary should be derived");

        assert_eq!(summary.level, "warning");
        assert_eq!(summary.title, "Cloudflare Setup");
        assert_eq!(summary.action_kind.as_deref(), Some("open_cloudflare"));
        assert_eq!(summary.action_label.as_deref(), Some("Open Cloudflare"));
    }

    #[test]
    fn provider_status_summary_identifies_ngrok_error_code() {
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
            &[String::from("t=2026-03-07 lvl=eror msg=\"failed\" err=\"ERR_NGROK_4018\"")],
        )
        .expect("summary should be derived");

        assert_eq!(summary.level, "error");
        assert!(summary.message.contains("ERR_NGROK_4018"));
        assert_eq!(summary.action_kind.as_deref(), Some("open_settings"));
        assert_eq!(summary.action_label.as_deref(), Some("Open Settings"));
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
        )
        .expect("summary should be derived");

        assert_eq!(summary.level, "warning");
        assert_eq!(summary.title, "Local Service Unreachable");
        assert_eq!(summary.action_kind.as_deref(), Some("review_services"));
        assert_eq!(summary.action_label.as_deref(), Some("Review Services"));
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
            "tunnelmux-gui-commands-{}",
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
