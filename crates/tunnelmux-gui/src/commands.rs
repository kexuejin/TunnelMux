use crate::daemon_manager::{self, DaemonStatusSnapshot};
use crate::settings::{GuiSettings, load_settings_from_dir, save_settings_to_dir};
use crate::state::GuiAppState;
use crate::view_models::{
    DiagnosticsSummaryVm, LogTailVm, RouteFormData, RouteWorkspaceSnapshot, UpstreamHealthVm,
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
    match client.health().await {
        Ok(health) => match client.tunnel_status().await {
            Ok(tunnel) => {
                let message = if tunnel.tunnel.state == tunnelmux_core::TunnelState::Running
                    && tunnel.tunnel.provider == Some(TunnelProvider::Cloudflared)
                    && tunnel.tunnel.public_base_url.is_none()
                    && settings.cloudflared_tunnel_token.is_some()
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
    let metadata = build_tunnel_metadata(&settings, &input.provider);
    let response = client
        .start_tunnel(&TunnelStartRequest {
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
    let response = client.stop_tunnel().await.map_err(command_error)?;

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
    let (_, client) = load_client(settings_dir)?;
    let response = client.list_routes().await.map_err(command_error)?;
    let message = if response.routes.is_empty() {
        Some(
            "No services yet. Add your first local service to replace the default welcome page."
                .to_string(),
        )
    } else {
        None
    };
    Ok(RouteWorkspaceSnapshot::from_routes(response.routes, message))
}

pub async fn save_route_from_settings_dir(
    settings_dir: &Path,
    form: RouteFormData,
) -> Result<RouteWorkspaceSnapshot, String> {
    let (_, client) = load_client(settings_dir)?;
    let request = form.into_create_request();
    if let Some(original_id) = form.original_id.as_deref() {
        client
            .update_route_with_options(original_id, &request, true)
            .await
            .map_err(command_error)?;
    } else {
        client.create_route(&request).await.map_err(command_error)?;
    }

    let routes = client.list_routes().await.map_err(command_error)?;
    Ok(RouteWorkspaceSnapshot::from_routes(
        routes.routes,
        Some("Route saved.".to_string()),
    ))
}

pub async fn delete_route_from_settings_dir(
    settings_dir: &Path,
    id: String,
) -> Result<RouteWorkspaceSnapshot, String> {
    let (_, client) = load_client(settings_dir)?;
    client
        .delete_route(&id, false)
        .await
        .map_err(command_error)?;
    let routes = client.list_routes().await.map_err(command_error)?;
    Ok(RouteWorkspaceSnapshot::from_routes(
        routes.routes,
        Some("Route deleted.".to_string()),
    ))
}

pub async fn load_diagnostics_summary_from_settings_dir(
    settings_dir: &Path,
) -> Result<DiagnosticsSummaryVm, String> {
    let (_, client) = load_client(settings_dir)?;
    let response = client.diagnostics().await.map_err(command_error)?;
    Ok(DiagnosticsSummaryVm::from(response))
}

pub async fn load_upstreams_health_from_settings_dir(
    settings_dir: &Path,
) -> Result<Vec<UpstreamHealthVm>, String> {
    let (_, client) = load_client(settings_dir)?;
    let response = client.upstreams_health().await.map_err(command_error)?;
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

    let (_, client) = load_client(settings_dir)?;
    let response = client.tunnel_logs(lines).await.map_err(command_error)?;
    Ok(LogTailVm::from_response(lines, response))
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
    settings: &GuiSettings,
    provider: &TunnelProvider,
) -> Option<HashMap<String, String>> {
    let mut metadata = HashMap::new();

    match provider {
        TunnelProvider::Cloudflared => {
            if let Some(value) = settings.cloudflared_tunnel_token.as_deref() {
                metadata.insert("cloudflaredTunnelToken".to_string(), value.to_string());
            }
        }
        TunnelProvider::Ngrok => {
            if let Some(value) = settings.ngrok_authtoken.as_deref() {
                metadata.insert("ngrokAuthtoken".to_string(), value.to_string());
            }
            if let Some(value) = settings.ngrok_domain.as_deref() {
                metadata.insert("ngrokDomain".to_string(), value.to_string());
            }
        }
    }

    (!metadata.is_empty()).then_some(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::get};
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
                default_provider: TunnelProvider::Ngrok,
                gateway_target_url: "http://127.0.0.1:28080".to_string(),
                auto_restart: false,
                cloudflared_tunnel_token: None,
                ngrok_authtoken: Some("ngrok-token".to_string()),
                ngrok_domain: Some("demo.ngrok.app".to_string()),
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
                default_provider: TunnelProvider::Cloudflared,
                gateway_target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
                cloudflared_tunnel_token: Some("cf-token".to_string()),
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
                default_provider: TunnelProvider::Cloudflared,
                gateway_target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: true,
                cloudflared_tunnel_token: Some("cf-token".to_string()),
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
        let base_url =
            spawn_test_server(Router::new().route("/v1/diagnostics", get(diagnostics_handler)))
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

        let summary = load_diagnostics_summary_from_settings_dir(&temp_dir)
            .await
            .expect("diagnostics summary should load");

        assert_eq!(summary.route_count, 2);
        assert_eq!(summary.enabled_route_count, 1);
        assert_eq!(summary.tunnel_state, "running");
        assert!(summary.pending_restart);
    }

    #[tokio::test]
    async fn load_upstreams_health_maps_mixed_health_states() {
        let temp_dir = prepare_temp_dir();
        let base_url = spawn_test_server(
            Router::new().route("/v1/upstreams/health", get(upstreams_health_handler)),
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

        let upstreams = load_upstreams_health_from_settings_dir(&temp_dir)
            .await
            .expect("upstreams health should load");

        assert_eq!(upstreams.len(), 3);
        assert_eq!(upstreams[0].health_label, "healthy");
        assert_eq!(upstreams[1].health_label, "unhealthy");
        assert_eq!(upstreams[2].health_label, "unknown");
    }

    #[tokio::test]
    async fn load_recent_logs_returns_requested_tail_lines() {
        let temp_dir = prepare_temp_dir();
        let base_url =
            spawn_test_server(Router::new().route("/v1/tunnel/logs", get(recent_logs_handler)))
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

        let log_tail = load_recent_logs_from_settings_dir(&temp_dir, 25)
            .await
            .expect("recent logs should load");

        assert_eq!(log_tail.requested_lines, 25);
        assert_eq!(
            log_tail.lines,
            vec!["first log line".to_string(), "second log line".to_string()]
        );
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

    async fn diagnostics_handler() -> Json<tunnelmux_core::DiagnosticsResponse> {
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

    async fn upstreams_health_handler() -> Json<tunnelmux_core::UpstreamsHealthResponse> {
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

    async fn recent_logs_handler() -> Json<tunnelmux_core::TunnelLogsResponse> {
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
