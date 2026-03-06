use assert_cmd::prelude::*;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use std::{net::SocketAddr, process::Command, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex};
use tunnelmux_core::{
    DiagnosticsResponse, ErrorResponse, TunnelProvider, TunnelState, TunnelStatus,
    TunnelStatusResponse,
};

#[derive(Debug, Default)]
struct TestState {
    auth_headers: Mutex<Vec<Option<String>>>,
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnostics_command_uses_shared_control_client() {
    let state = Arc::new(TestState::default());
    let app = Router::new()
        .route("/v1/diagnostics", get(diagnostics_handler))
        .with_state(state.clone());
    let base_url = spawn_test_server(app).await;

    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("tunnelmux-cli"));
    command
        .arg("--server")
        .arg(&base_url)
        .arg("--token")
        .arg("dev-token")
        .arg("diagnostics");

    let assert = command.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout utf8");
    assert!(stdout.contains("\"config_reload_enabled\": true"));
    assert!(stdout.contains("\"config_file\": \"/tmp/config.json\""));

    let auth_headers = state.auth_headers.lock().await;
    assert_eq!(
        auth_headers.as_slice(),
        &[Some("Bearer dev-token".to_string())]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tunnel_stop_command_still_prints_json_payload() {
    let state = Arc::new(TestState::default());
    let app = Router::new()
        .route("/v1/tunnel/stop", post(tunnel_stop_handler))
        .with_state(state.clone());
    let base_url = spawn_test_server(app).await;

    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("tunnelmux-cli"));
    command
        .arg("--server")
        .arg(&base_url)
        .arg("tunnel")
        .arg("stop");

    let assert = command.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout utf8");
    assert!(
        stdout.contains("\"state\": \"stopped\""),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("\"restart_count\": 1"), "stdout: {stdout}");

    let auth_headers = state.auth_headers.lock().await;
    assert_eq!(auth_headers.as_slice(), &[None]);
}

async fn diagnostics_handler(
    State(state): State<Arc<TestState>>,
    headers: HeaderMap,
) -> Result<Json<DiagnosticsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    state.auth_headers.lock().await.push(auth.clone());

    if auth.as_deref() != Some("Bearer dev-token") {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "unauthorized".to_string(),
            }),
        ));
    }

    Ok(Json(DiagnosticsResponse {
        data_file: "/tmp/state.json".to_string(),
        config_file: "/tmp/config.json".to_string(),
        provider_log_file: "/tmp/provider.log".to_string(),
        route_count: 2,
        enabled_route_count: 1,
        tunnel_state: TunnelState::Running,
        pending_restart: false,
        config_reload_enabled: true,
        config_reload_interval_ms: 1_000,
        last_config_reload_at: Some("2026-03-06T00:00:00Z".to_string()),
        last_config_reload_error: None,
    }))
}

async fn tunnel_stop_handler(
    State(state): State<Arc<TestState>>,
    headers: HeaderMap,
) -> Json<TunnelStatusResponse> {
    state.auth_headers.lock().await.push(
        headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
    );

    Json(TunnelStatusResponse {
        tunnel: TunnelStatus {
            state: TunnelState::Stopped,
            provider: Some(TunnelProvider::Cloudflared),
            target_url: Some("http://127.0.0.1:18080".to_string()),
            public_base_url: None,
            started_at: Some("2026-03-06T00:00:00Z".to_string()),
            updated_at: "2026-03-06T00:00:05Z".to_string(),
            process_id: None,
            auto_restart: true,
            restart_count: 1,
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
