use crate::settings::{GuiSettings, load_settings_from_dir, save_settings_to_dir};
use crate::state::GuiAppState;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::Manager;
use tunnelmux_control_client::{ControlClientConfig, TunnelmuxControlClient};
use tunnelmux_core::{HealthResponse, TunnelProvider, TunnelStartRequest, TunnelStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub message: Option<String>,
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

pub async fn probe_connection_from_settings_dir(settings_dir: &Path) -> Result<ConnectionStatus, String> {
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
            Ok(tunnel) => Ok(DashboardSnapshot {
                connected: true,
                settings,
                health: Some(health),
                tunnel: Some(tunnel.tunnel),
                message: None,
            }),
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
    let response = client
        .start_tunnel(&TunnelStartRequest {
            provider: input.provider,
            target_url: input.target_url,
            auto_restart: Some(input.auto_restart),
            metadata: None,
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

pub async fn stop_tunnel_from_settings_dir(settings_dir: &Path) -> Result<DashboardSnapshot, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::get};
    use std::{net::SocketAddr, sync::atomic::{AtomicU64, Ordering}};
    use tokio::net::TcpListener;
    use tunnelmux_core::{TunnelStatus, TunnelStatusResponse};

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
            },
        )
        .expect("settings should save");

        let snapshot = refresh_dashboard_from_settings_dir(&temp_dir)
            .await
            .expect("dashboard refresh should succeed");

        assert!(snapshot.connected);
        assert_eq!(snapshot.settings.token.as_deref(), Some("dev-token"));
        assert_eq!(snapshot.tunnel.as_ref().map(|item| item.state.clone()), Some(tunnelmux_core::TunnelState::Running));
        assert_eq!(snapshot.tunnel.as_ref().and_then(|item| item.public_base_url.as_deref()), Some("https://demo.trycloudflare.com"));
    }

    #[tokio::test]
    async fn start_tunnel_command_maps_connection_errors_cleanly() {
        let temp_dir = prepare_temp_dir();
        save_settings_to_dir(
            &temp_dir,
            &GuiSettings {
                base_url: "http://127.0.0.1:9".to_string(),
                token: None,
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

        assert!(error.contains("request failed") || error.contains("error sending request"), "unexpected error: {error}");
    }

    async fn health_handler() -> Json<HealthResponse> {
        Json(HealthResponse {
            ok: true,
            service: "tunnelmuxd".to_string(),
            version: "0.1.3".to_string(),
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
