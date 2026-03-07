use anyhow::{Context, anyhow};
use reqwest::{Client, RequestBuilder, Response, StatusCode as ReqwestStatusCode};
use serde::de::DeserializeOwned;
use tunnelmux_core::{
    ApplyRoutesRequest, ApplyRoutesResponse, CreateRouteRequest, DashboardResponse,
    DeleteRouteResponse, DeleteTunnelResponse, DiagnosticsResponse, ErrorResponse,
    HealthCheckSettingsResponse, HealthResponse, MetricsResponse, ReloadSettingsResponse,
    RouteMatchResponse, RouteRule, RoutesResponse, TunnelDeleteRequest, TunnelLogsResponse,
    TunnelStartRequest, TunnelStatusResponse, TunnelWorkspaceResponse,
    UpdateHealthCheckSettingsRequest, UpstreamsHealthResponse,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlClientConfig {
    pub base_url: String,
    pub token: Option<String>,
}

impl ControlClientConfig {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base_url: normalize_base_url(&base_url.into()),
            token: normalize_token(token),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TunnelmuxControlClient {
    client: Client,
    config: ControlClientConfig,
}

impl TunnelmuxControlClient {
    pub fn new(config: ControlClientConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    pub fn with_http_client(client: Client, config: ControlClientConfig) -> Self {
        Self { client, config }
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    pub fn token(&self) -> Option<&str> {
        self.config.token.as_deref()
    }

    pub async fn health(&self) -> anyhow::Result<HealthResponse> {
        self.get("/v1/health").await
    }

    pub async fn tunnel_status(&self, tunnel_id: &str) -> anyhow::Result<TunnelStatusResponse> {
        let url = self.url("/v1/tunnel/status");
        let response = self
            .request_with_token(self.client.get(&url))
            .query(&[("tunnel_id", tunnel_id)])
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_response(response).await
    }

    pub async fn tunnel_workspace(&self) -> anyhow::Result<TunnelWorkspaceResponse> {
        self.get("/v1/tunnels/workspace").await
    }

    pub async fn start_tunnel(
        &self,
        payload: &TunnelStartRequest,
    ) -> anyhow::Result<TunnelStatusResponse> {
        let url = self.url("/v1/tunnel/start");
        let response = self
            .request_with_token(self.client.post(&url).json(payload))
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_tunnel_status_response(response, &payload.tunnel_id).await
    }

    pub async fn stop_tunnel(&self, tunnel_id: &str) -> anyhow::Result<TunnelStatusResponse> {
        self.post(
            "/v1/tunnel/stop",
            &tunnelmux_core::TunnelStopRequest {
                tunnel_id: tunnel_id.to_string(),
            },
        )
        .await
    }

    pub async fn delete_tunnel(&self, tunnel_id: &str) -> anyhow::Result<DeleteTunnelResponse> {
        self.post(
            "/v1/tunnel/delete",
            &TunnelDeleteRequest {
                tunnel_id: tunnel_id.to_string(),
            },
        )
        .await
    }

    pub async fn diagnostics(&self) -> anyhow::Result<DiagnosticsResponse> {
        self.get("/v1/diagnostics").await
    }

    pub async fn diagnostics_for_tunnel(
        &self,
        tunnel_id: &str,
    ) -> anyhow::Result<DiagnosticsResponse> {
        let url = self.url("/v1/diagnostics");
        let response = self
            .request_with_token(self.client.get(&url))
            .query(&[("tunnel_id", tunnel_id)])
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_response(response).await
    }

    pub async fn tunnel_logs(&self, lines: usize) -> anyhow::Result<TunnelLogsResponse> {
        let url = self.url("/v1/tunnel/logs");
        let response = self
            .request_with_token(self.client.get(&url))
            .query(&[("lines", lines)])
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_response(response).await
    }

    pub async fn tunnel_logs_for_tunnel(
        &self,
        tunnel_id: &str,
        lines: usize,
    ) -> anyhow::Result<TunnelLogsResponse> {
        let url = self.url("/v1/tunnel/logs");
        let response = self
            .request_with_token(self.client.get(&url))
            .query(&[("tunnel_id", tunnel_id)])
            .query(&[("lines", lines)])
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_response(response).await
    }

    pub async fn dashboard(&self) -> anyhow::Result<DashboardResponse> {
        self.get("/v1/dashboard").await
    }

    pub async fn metrics(&self) -> anyhow::Result<MetricsResponse> {
        self.get("/v1/metrics").await
    }

    pub async fn list_routes(&self, tunnel_id: &str) -> anyhow::Result<RoutesResponse> {
        let url = self.url("/v1/routes");
        let response = self
            .request_with_token(self.client.get(&url))
            .query(&[("tunnel_id", tunnel_id)])
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_response(response).await
    }

    pub async fn create_route(&self, payload: &CreateRouteRequest) -> anyhow::Result<RouteRule> {
        self.post("/v1/routes", payload).await
    }

    pub async fn update_route(
        &self,
        id: &str,
        payload: &CreateRouteRequest,
    ) -> anyhow::Result<RouteRule> {
        self.update_route_with_options(id, payload, false).await
    }

    pub async fn update_route_with_options(
        &self,
        id: &str,
        payload: &CreateRouteRequest,
        upsert: bool,
    ) -> anyhow::Result<RouteRule> {
        let path = build_route_update_endpoint(id, upsert);
        self.put(&path, payload).await
    }

    pub async fn delete_route(
        &self,
        id: &str,
        tunnel_id: &str,
        ignore_missing: bool,
    ) -> anyhow::Result<DeleteRouteResponse> {
        let path = format!("/v1/routes/{id}");
        let url = self.url(&path);
        let response = self
            .request_with_token(self.client.delete(&url))
            .query(&[("tunnel_id", tunnel_id)])
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read response body")?;
        if status == ReqwestStatusCode::NOT_FOUND && ignore_missing {
            return Ok(DeleteRouteResponse { removed: false });
        }
        if !status.is_success() {
            return Err(anyhow!("HTTP {}: {}", status, extract_error_message(&body)));
        }
        serde_json::from_str::<DeleteRouteResponse>(&body)
            .with_context(|| format!("failed to parse delete route response: {body}"))
    }

    pub async fn match_route(
        &self,
        tunnel_id: &str,
        path: &str,
        host: Option<&str>,
    ) -> anyhow::Result<RouteMatchResponse> {
        let url = self.url("/v1/routes/match");
        let mut request = self.request_with_token(self.client.get(&url));
        request = request.query(&[("tunnel_id", tunnel_id)]);
        request = request.query(&[("path", path)]);
        if let Some(host) = host.map(str::trim).filter(|value| !value.is_empty()) {
            request = request.query(&[("host", host)]);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_response(response).await
    }

    pub async fn apply_routes(
        &self,
        payload: &ApplyRoutesRequest,
    ) -> anyhow::Result<ApplyRoutesResponse> {
        self.post("/v1/routes/apply", payload).await
    }

    pub async fn upstreams_health(&self) -> anyhow::Result<UpstreamsHealthResponse> {
        self.get("/v1/upstreams/health").await
    }

    pub async fn upstreams_health_for_tunnel(
        &self,
        tunnel_id: &str,
    ) -> anyhow::Result<UpstreamsHealthResponse> {
        let url = self.url("/v1/upstreams/health");
        let response = self
            .request_with_token(self.client.get(&url))
            .query(&[("tunnel_id", tunnel_id)])
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_response(response).await
    }

    pub async fn get_health_check_settings(&self) -> anyhow::Result<HealthCheckSettingsResponse> {
        self.get("/v1/settings/health-check").await
    }

    pub async fn update_health_check_settings(
        &self,
        payload: &UpdateHealthCheckSettingsRequest,
    ) -> anyhow::Result<HealthCheckSettingsResponse> {
        self.put("/v1/settings/health-check", payload).await
    }

    pub async fn reload_settings(&self) -> anyhow::Result<ReloadSettingsResponse> {
        self.post("/v1/settings/reload", &serde_json::json!({}))
            .await
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        self.send(self.request_with_token(self.client.get(self.url(path))))
            .await
    }

    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &impl serde::Serialize,
    ) -> anyhow::Result<T> {
        self.send(
            self.request_with_token(self.client.post(self.url(path)))
                .json(payload),
        )
        .await
    }

    pub async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        payload: &impl serde::Serialize,
    ) -> anyhow::Result<T> {
        self.send(
            self.request_with_token(self.client.put(self.url(path)))
                .json(payload),
        )
        .await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        self.send(self.request_with_token(self.client.delete(self.url(path))))
            .await
    }

    fn request_with_token(&self, builder: RequestBuilder) -> RequestBuilder {
        match self.token() {
            Some(token) => builder.bearer_auth(token),
            None => builder,
        }
    }

    async fn send<T: DeserializeOwned>(&self, builder: RequestBuilder) -> anyhow::Result<T> {
        let request = builder.build().context("failed to build request")?;
        let url = request.url().to_string();
        let response = self
            .client
            .execute(request)
            .await
            .with_context(|| format!("request failed: {url}"))?;
        decode_response(response).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }
}

pub async fn decode_response<T: DeserializeOwned>(response: Response) -> anyhow::Result<T> {
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;

    if !status.is_success() {
        return Err(anyhow!("HTTP {}: {}", status, extract_error_message(&body)));
    }

    serde_json::from_str::<T>(&body)
        .with_context(|| format!("failed to parse successful response body: {body}"))
}

async fn decode_tunnel_status_response(
    response: Response,
    fallback_tunnel_id: &str,
) -> anyhow::Result<TunnelStatusResponse> {
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;

    if !status.is_success() {
        return Err(anyhow!("HTTP {}: {}", status, extract_error_message(&body)));
    }

    if let Ok(decoded) = serde_json::from_str::<TunnelStatusResponse>(&body) {
        return Ok(decoded);
    }

    let legacy = serde_json::from_str::<serde_json::Value>(&body)
        .with_context(|| format!("failed to parse successful response body: {body}"))?;
    let tunnel = legacy
        .get("tunnel")
        .cloned()
        .ok_or_else(|| anyhow!("failed to parse successful response body: {body}"))?;
    let tunnel = serde_json::from_value(tunnel)
        .with_context(|| format!("failed to parse successful response body: {body}"))?;

    Ok(TunnelStatusResponse {
        tunnel_id: fallback_tunnel_id.to_string(),
        tunnel,
    })
}

pub fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<ErrorResponse>(body)
        .map(|error| error.error)
        .unwrap_or_else(|_| body.trim().to_string())
}

fn normalize_base_url(server: &str) -> String {
    let trimmed = server.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    format!("http://{trimmed}")
}

fn normalize_token(token: Option<String>) -> Option<String> {
    token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_route_update_endpoint(id: &str, upsert: bool) -> String {
    if upsert {
        format!("/v1/routes/{id}?upsert=true")
    } else {
        format!("/v1/routes/{id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::{Query, State},
        http::{HeaderMap, StatusCode},
        routing::{get, post},
    };
    use std::{collections::HashMap, net::SocketAddr, sync::Arc};
    use tokio::{net::TcpListener, sync::Mutex};
    use tunnelmux_core::{TunnelProvider, TunnelState};

    #[derive(Debug, Default)]
    struct TestState {
        auth_headers: Mutex<Vec<Option<String>>>,
        log_line_queries: Mutex<Vec<Option<String>>>,
        tunnel_queries: Mutex<Vec<Option<String>>>,
    }

    #[tokio::test]
    async fn tunnel_status_decodes_success_payload() {
        let state = Arc::new(TestState::default());
        let app = Router::new()
            .route("/v1/tunnel/status", get(tunnel_status_handler))
            .with_state(state.clone());
        let base_url = spawn_test_server(app).await;
        let client = TunnelmuxControlClient::new(ControlClientConfig::new(
            base_url,
            Some("dev-token".to_string()),
        ));

        let response = client
            .tunnel_status("primary")
            .await
            .expect("status request should succeed");

        assert_eq!(response.tunnel.state, TunnelState::Running);
        assert_eq!(response.tunnel.provider, Some(TunnelProvider::Cloudflared));
        assert_eq!(
            response.tunnel.public_base_url.as_deref(),
            Some("https://demo.trycloudflare.com")
        );

        let auth_headers = state.auth_headers.lock().await;
        assert_eq!(
            auth_headers.as_slice(),
            &[Some("Bearer dev-token".to_string())]
        );
    }

    #[tokio::test]
    async fn tunnel_logs_decodes_success_payload() {
        let state = Arc::new(TestState::default());
        let app = Router::new()
            .route("/v1/tunnel/logs", get(tunnel_logs_handler))
            .with_state(state.clone());
        let base_url = spawn_test_server(app).await;
        let client = TunnelmuxControlClient::new(ControlClientConfig::new(
            base_url,
            Some("dev-token".to_string()),
        ));

        let response = client
            .tunnel_logs_for_tunnel("primary", 50)
            .await
            .expect("logs request should succeed");

        assert_eq!(
            response.lines,
            vec![
                "provider booted".to_string(),
                "tunnel connected".to_string()
            ]
        );

        let auth_headers = state.auth_headers.lock().await;
        assert_eq!(
            auth_headers.as_slice(),
            &[Some("Bearer dev-token".to_string())]
        );
        drop(auth_headers);

        let log_line_queries = state.log_line_queries.lock().await;
        assert_eq!(log_line_queries.as_slice(), &[Some("50".to_string())]);
        drop(log_line_queries);

        let tunnel_queries = state.tunnel_queries.lock().await;
        assert_eq!(tunnel_queries.as_slice(), &[Some("primary".to_string())]);
    }

    #[tokio::test]
    async fn tunnel_logs_surfaces_structured_error_message() {
        let app = Router::new().route("/v1/tunnel/logs", get(logs_error_handler));
        let base_url = spawn_test_server(app).await;
        let client = TunnelmuxControlClient::new(ControlClientConfig::new(base_url, None));

        let err = client
            .tunnel_logs_for_tunnel("primary", 100)
            .await
            .expect_err("logs request should fail");

        assert!(
            err.to_string().contains("provider log unavailable"),
            "unexpected error: {err:#}"
        );
    }

    #[tokio::test]
    async fn diagnostics_includes_tunnel_id_query() {
        let state = Arc::new(TestState::default());
        let app = Router::new()
            .route("/v1/diagnostics", get(diagnostics_handler))
            .with_state(state.clone());
        let base_url = spawn_test_server(app).await;
        let client = TunnelmuxControlClient::new(ControlClientConfig::new(base_url, None));

        let response = client
            .diagnostics_for_tunnel("tunnel-2")
            .await
            .expect("diagnostics request should succeed");

        assert_eq!(response.route_count, 2);

        let tunnel_queries = state.tunnel_queries.lock().await;
        assert_eq!(tunnel_queries.as_slice(), &[Some("tunnel-2".to_string())]);
    }

    #[tokio::test]
    async fn upstreams_health_includes_tunnel_id_query() {
        let state = Arc::new(TestState::default());
        let app = Router::new()
            .route("/v1/upstreams/health", get(upstreams_health_handler))
            .with_state(state.clone());
        let base_url = spawn_test_server(app).await;
        let client = TunnelmuxControlClient::new(ControlClientConfig::new(base_url, None));

        let response = client
            .upstreams_health_for_tunnel("tunnel-2")
            .await
            .expect("upstreams health request should succeed");

        assert_eq!(response.upstreams.len(), 1);

        let tunnel_queries = state.tunnel_queries.lock().await;
        assert_eq!(tunnel_queries.as_slice(), &[Some("tunnel-2".to_string())]);
    }

    #[tokio::test]
    async fn delete_tunnel_decodes_success_payload() {
        let app = Router::new().route("/v1/tunnel/delete", post(delete_tunnel_handler));
        let base_url = spawn_test_server(app).await;
        let client = TunnelmuxControlClient::new(ControlClientConfig::new(base_url, None));

        let response = client
            .delete_tunnel("tunnel-2")
            .await
            .expect("delete tunnel request should succeed");

        assert!(response.removed);
    }

    #[tokio::test]
    async fn start_tunnel_decodes_legacy_payload_without_tunnel_id() {
        let app = Router::new().route("/v1/tunnel/start", post(legacy_start_tunnel_handler));
        let base_url = spawn_test_server(app).await;
        let client = TunnelmuxControlClient::new(ControlClientConfig::new(base_url, None));

        let response = client
            .start_tunnel(&TunnelStartRequest {
                tunnel_id: "primary".to_string(),
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:48080".to_string(),
                auto_restart: Some(true),
                metadata: None,
            })
            .await
            .expect("legacy start tunnel payload should decode");

        assert_eq!(response.tunnel_id, "primary");
        assert_eq!(response.tunnel.state, TunnelState::Running);
        assert_eq!(
            response.tunnel.public_base_url.as_deref(),
            Some("https://demo.trycloudflare.com")
        );
    }

    #[tokio::test]
    async fn create_route_surfaces_structured_error_message() {
        let app = Router::new().route("/v1/routes", post(route_error_handler));
        let base_url = spawn_test_server(app).await;
        let client = TunnelmuxControlClient::new(ControlClientConfig::new(base_url, None));

        let err = client
            .create_route(&CreateRouteRequest {
                tunnel_id: "primary".to_string(),
                id: "app-web".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: Some(true),
            })
            .await
            .expect_err("create route should fail");

        assert!(
            err.to_string().contains("duplicate route id"),
            "unexpected error: {err:#}"
        );
    }

    async fn tunnel_status_handler(
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
            tunnel_id: "primary".to_string(),
            tunnel: tunnelmux_core::TunnelStatus {
                state: TunnelState::Running,
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

    async fn tunnel_logs_handler(
        State(state): State<Arc<TestState>>,
        headers: HeaderMap,
        Query(query): Query<HashMap<String, String>>,
    ) -> Json<TunnelLogsResponse> {
        state.auth_headers.lock().await.push(
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string),
        );
        state
            .log_line_queries
            .lock()
            .await
            .push(query.get("lines").cloned());
        state
            .tunnel_queries
            .lock()
            .await
            .push(query.get("tunnel_id").cloned());

        Json(TunnelLogsResponse {
            lines: vec![
                "provider booted".to_string(),
                "tunnel connected".to_string(),
            ],
        })
    }

    async fn diagnostics_handler(
        State(state): State<Arc<TestState>>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Json<DiagnosticsResponse> {
        state
            .tunnel_queries
            .lock()
            .await
            .push(query.get("tunnel_id").cloned());

        Json(DiagnosticsResponse {
            data_file: "/tmp/state.json".to_string(),
            config_file: "/tmp/config.json".to_string(),
            provider_log_file: "/tmp/provider.log".to_string(),
            route_count: 2,
            enabled_route_count: 1,
            tunnel_state: TunnelState::Running,
            pending_restart: false,
            config_reload_enabled: true,
            config_reload_interval_ms: 1_000,
            last_config_reload_at: None,
            last_config_reload_error: None,
        })
    }

    async fn upstreams_health_handler(
        State(state): State<Arc<TestState>>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Json<UpstreamsHealthResponse> {
        state
            .tunnel_queries
            .lock()
            .await
            .push(query.get("tunnel_id").cloned());

        Json(UpstreamsHealthResponse {
            upstreams: vec![tunnelmux_core::UpstreamHealthEntry {
                upstream_url: "http://127.0.0.1:4000".to_string(),
                health_check_path: "/ready".to_string(),
                healthy: Some(true),
                last_checked_at: Some("2026-03-06T00:00:00Z".to_string()),
                last_error: None,
            }],
        })
    }

    async fn logs_error_handler() -> (StatusCode, Json<ErrorResponse>) {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "provider log unavailable".to_string(),
            }),
        )
    }

    async fn route_error_handler() -> (StatusCode, Json<ErrorResponse>) {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "duplicate route id".to_string(),
            }),
        )
    }

    async fn delete_tunnel_handler() -> Json<DeleteTunnelResponse> {
        Json(DeleteTunnelResponse { removed: true })
    }

    async fn legacy_start_tunnel_handler() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "tunnel": {
                "state": "running",
                "provider": "cloudflared",
                "target_url": "http://127.0.0.1:48080",
                "public_base_url": "https://demo.trycloudflare.com",
                "started_at": "2026-03-07T00:00:00Z",
                "updated_at": "2026-03-07T00:00:01Z",
                "process_id": 12345,
                "auto_restart": true,
                "restart_count": 0,
                "last_error": null
            }
        }))
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
}
