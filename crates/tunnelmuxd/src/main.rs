use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::{Context, anyhow};
use axum::{
    Json, Router,
    body::Body,
    body::to_bytes,
    extract::Request,
    extract::{Path as AxumPath, Query, State},
    http::HeaderMap,
    http::HeaderName,
    http::Method,
    http::StatusCode,
    http::Uri,
    http::Version,
    middleware::{self, Next},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{delete, get, post},
};
use chrono::Utc;
use clap::Parser;
use http_body_util::BodyExt;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::{
    client::legacy::{Client as HyperClient, connect::HttpConnector},
    rt::{TokioExecutor, TokioIo},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader},
    net::TcpListener,
    process::{Child, Command},
    sync::{Mutex, RwLock, mpsc},
    time::{Instant, sleep, timeout},
};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, warn};
use tunnelmux_core::{
    ApplyRoutesRequest, ApplyRoutesResponse, CreateRouteRequest, DEFAULT_CONTROL_ADDR,
    DEFAULT_GATEWAY_TARGET_URL, DashboardResponse, DeleteRouteResponse, DiagnosticsResponse,
    ErrorResponse, HealthCheckSettings, HealthCheckSettingsResponse, HealthResponse,
    MetricsResponse, ReloadSettingsResponse, RouteMatchResponse, RouteMatchTarget, RouteRule,
    RoutesResponse, TunnelLogsResponse, TunnelProvider, TunnelStartRequest, TunnelState,
    TunnelStatus, TunnelStatusResponse, UpdateHealthCheckSettingsRequest, UpstreamHealthEntry,
    UpstreamsHealthResponse,
};
use url::Url;

mod api;
mod gateway;
mod persistence;
mod runtime;

use api::*;
use gateway::*;
use persistence::*;
use runtime::*;

#[derive(Debug, Parser)]
#[command(name = "tunnelmuxd", version, about = "TunnelMux daemon")]
struct Args {
    #[arg(long, default_value = DEFAULT_CONTROL_ADDR)]
    listen: String,

    #[arg(long)]
    data_file: Option<PathBuf>,

    #[arg(long)]
    config_file: Option<PathBuf>,

    #[arg(long, default_value_t = 1_000)]
    config_reload_interval_ms: u64,

    #[arg(long, default_value = "cloudflared")]
    cloudflared_bin: String,

    #[arg(long, default_value = "ngrok")]
    ngrok_bin: String,

    #[arg(long, default_value_t = 15_000)]
    ready_timeout_ms: u64,

    #[arg(long, default_value = "127.0.0.1:18080")]
    gateway_listen: String,

    #[arg(long, default_value_t = 10)]
    max_auto_restarts: u32,

    #[arg(long, default_value_t = 5_000)]
    health_check_interval_ms: u64,

    #[arg(long, default_value_t = 2_000)]
    health_check_timeout_ms: u64,

    #[arg(long, default_value = "/")]
    health_check_path: String,

    #[arg(long)]
    provider_log_file: Option<PathBuf>,

    #[arg(long)]
    api_token: Option<String>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    tunnel: TunnelStatus,
    routes: Vec<RouteRule>,
    health_check: Option<HealthCheckSettings>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            tunnel: default_tunnel_status(TunnelState::Idle),
            routes: Vec::new(),
            health_check: None,
        }
    }
}

#[derive(Debug)]
struct RunningTunnel {
    child: Child,
    provider: TunnelProvider,
    target_url: String,
    metadata: Option<HashMap<String, String>>,
    auto_restart: bool,
    restart_count: u32,
    started_at: String,
    public_base_url: Option<String>,
    process_id: Option<u32>,
}

#[derive(Debug, Clone)]
struct PendingRestart {
    provider: TunnelProvider,
    target_url: String,
    metadata: Option<HashMap<String, String>>,
    auto_restart: bool,
    restart_count: u32,
    started_at: String,
    next_attempt_at: Instant,
    reason: String,
}

#[derive(Debug)]
struct RuntimeState {
    persisted: PersistedState,
    running_tunnel: Option<RunningTunnel>,
    pending_restart: Option<PendingRestart>,
}

#[derive(Debug, Default)]
struct ConfigReloadStatus {
    enabled: bool,
    interval_ms: u64,
    last_digest: Option<u64>,
    last_config_reload_at: Option<String>,
    last_config_reload_error: Option<String>,
}

#[derive(Debug)]
struct AppState {
    runtime: Mutex<RuntimeState>,
    upstream_health: Mutex<HashMap<UpstreamHealthKey, UpstreamHealth>>,
    health_check_settings: RwLock<HealthCheckSettings>,
    data_file: PathBuf,
    config_file: PathBuf,
    config_reload_status: Mutex<ConfigReloadStatus>,
    provider_log_file: PathBuf,
    api_token: Option<String>,
    cloudflared_bin: String,
    ngrok_bin: String,
    ready_timeout_ms: u64,
    max_auto_restarts: u32,
    proxy_client: reqwest::Client,
    ws_proxy_client: HyperClient<HttpsConnector<HttpConnector>, Body>,
}

#[derive(Debug, Clone)]
struct UpstreamHealth {
    healthy: bool,
    last_checked_at: String,
    last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UpstreamHealthKey {
    upstream_url: String,
    health_check_path: String,
}

#[derive(Debug)]
struct SpawnedTunnel {
    child: Child,
    public_url: Option<String>,
    process_id: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TunnelLogsQuery {
    lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TunnelLogsStreamQuery {
    lines: Option<usize>,
    poll_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct StreamIntervalQuery {
    interval_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RouteMatchQuery {
    tunnel_id: Option<String>,
    host: Option<String>,
    path: String,
}

#[derive(Debug, Deserialize)]
struct TunnelRouteQuery {
    tunnel_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateRouteQuery {
    upsert: Option<bool>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let addr: SocketAddr = args
        .listen
        .parse()
        .with_context(|| format!("invalid --listen address: {}", args.listen))?;
    let gateway_addr: SocketAddr = args
        .gateway_listen
        .parse()
        .with_context(|| format!("invalid --gateway-listen address: {}", args.gateway_listen))?;

    let data_file = args.data_file.unwrap_or_else(default_data_file);
    let config_file = args.config_file.unwrap_or_else(default_config_file);
    let config_reload_interval_ms =
        normalize_config_reload_interval_ms(args.config_reload_interval_ms).with_context(|| {
            format!(
                "invalid --config-reload-interval-ms: {}",
                args.config_reload_interval_ms
            )
        })?;
    let provider_log_file = args
        .provider_log_file
        .unwrap_or_else(default_provider_log_file);
    let api_token = resolve_api_token(args.api_token);
    let startup_health_check_settings = HealthCheckSettings {
        interval_ms: normalize_health_check_interval_ms(args.health_check_interval_ms)
            .with_context(|| {
                format!(
                    "invalid --health-check-interval-ms: {}",
                    args.health_check_interval_ms
                )
            })?,
        timeout_ms: normalize_health_check_timeout_ms(args.health_check_timeout_ms).with_context(
            || {
                format!(
                    "invalid --health-check-timeout-ms: {}",
                    args.health_check_timeout_ms
                )
            },
        )?,
        path: normalize_health_check_path(&args.health_check_path)
            .with_context(|| format!("invalid --health-check-path: {}", args.health_check_path))?,
    };
    let mut persisted = load_persisted_state(&data_file).await?;
    let mut health_check_settings = resolve_initial_health_check_settings(
        startup_health_check_settings,
        persisted.health_check.clone(),
    );
    let mut config_reload_status = ConfigReloadStatus {
        enabled: true,
        interval_ms: config_reload_interval_ms,
        ..ConfigReloadStatus::default()
    };
    if let Some(config) = load_config_file(&config_file).await? {
        persisted.routes = config.routes.clone();
        if let Some(config_health_check) = config.health_check.clone() {
            health_check_settings = config_health_check;
        }
        config_reload_status.last_digest = Some(hash_declarative_config(&config)?);
        config_reload_status.last_config_reload_at = Some(now_iso());
    }
    persisted.health_check = Some(health_check_settings.clone());
    let mut http_connector = HttpConnector::new();
    http_connector.enforce_http(false);
    let https_connector = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .wrap_connector(http_connector);
    let ws_proxy_client = HyperClient::builder(TokioExecutor::new()).build(https_connector);

    let shared = Arc::new(AppState {
        runtime: Mutex::new(RuntimeState {
            persisted,
            running_tunnel: None,
            pending_restart: None,
        }),
        upstream_health: Mutex::new(HashMap::new()),
        health_check_settings: RwLock::new(health_check_settings),
        data_file,
        config_file,
        config_reload_status: Mutex::new(config_reload_status),
        provider_log_file,
        api_token,
        cloudflared_bin: args.cloudflared_bin,
        ngrok_bin: args.ngrok_bin,
        ready_timeout_ms: args.ready_timeout_ms,
        max_auto_restarts: args.max_auto_restarts,
        proxy_client: reqwest::Client::new(),
        ws_proxy_client,
    });

    tokio::spawn(monitor_runtime_state(shared.clone()));
    tokio::spawn(monitor_upstream_health(shared.clone()));
    tokio::spawn(monitor_config_file(shared.clone()));

    let protected_control_app = Router::new()
        .route("/v1/tunnel/status", get(get_tunnel_status))
        .route("/v1/tunnel/status/stream", get(stream_tunnel_status))
        .route("/v1/tunnels/workspace", get(get_tunnel_workspace))
        .route("/v1/tunnel/logs", get(get_tunnel_logs))
        .route("/v1/tunnel/logs/stream", get(stream_tunnel_logs))
        .route("/v1/tunnel/start", post(start_tunnel))
        .route("/v1/tunnel/stop", post(stop_tunnel))
        .route(
            "/v1/settings/health-check",
            get(get_health_check_settings).put(update_health_check_settings),
        )
        .route("/v1/dashboard", get(get_dashboard))
        .route("/v1/dashboard/stream", get(stream_dashboard))
        .route("/v1/metrics", get(get_metrics))
        .route("/v1/metrics/stream", get(stream_metrics))
        .route("/v1/diagnostics", get(get_diagnostics))
        .route("/v1/upstreams/health", get(get_upstreams_health))
        .route("/v1/upstreams/health/stream", get(stream_upstreams_health))
        .route("/v1/routes", get(list_routes).post(add_route))
        .route("/v1/routes/match", get(match_route))
        .route("/v1/routes/stream", get(stream_routes))
        .route("/v1/routes/apply", post(apply_routes))
        .route("/v1/routes/{id}", delete(delete_route).put(update_route))
        .layer(middleware::from_fn_with_state(
            shared.clone(),
            control_auth_middleware,
        ));
    let control_app = Router::new()
        .route("/v1/health", get(health))
        .merge(protected_control_app)
        .with_state(shared.clone());
    let gateway_app = Router::new()
        .fallback(proxy_request)
        .with_state(shared.clone());

    let listener = TcpListener::bind(addr).await?;
    let gateway_listener = TcpListener::bind(gateway_addr).await?;
    info!(
        "tunnelmuxd listening on {}, state file {}",
        listener.local_addr()?,
        shared.data_file.display()
    );
    if shared.api_token.is_some() {
        info!("control api token auth: enabled");
    } else {
        info!("control api token auth: disabled");
    }
    info!("provider log file {}", shared.provider_log_file.display());
    info!("gateway listening on {}", gateway_listener.local_addr()?);

    let control_server = axum::serve(listener, control_app);
    let gateway_server = axum::serve(gateway_listener, gateway_app);
    tokio::try_join!(control_server, gateway_server)?;
    Ok(())
}

fn normalize_route_request(request: CreateRouteRequest) -> Result<RouteRule, ApiError> {
    let id = request.id.trim().to_string();
    if id.is_empty() {
        return Err(ApiError::bad_request("route id is required"));
    }

    let match_host = normalize_optional(request.match_host);
    let match_path_prefix = normalize_optional(request.match_path_prefix);
    let strip_path_prefix = normalize_optional(request.strip_path_prefix);
    let fallback_upstream_url = normalize_optional(request.fallback_upstream_url);
    let health_check_path = normalize_optional(request.health_check_path)
        .map(|value| normalize_health_check_path(&value))
        .transpose()
        .map_err(|err| ApiError::bad_request(format!("invalid health_check_path: {err}")))?;

    if match_host.is_none() && match_path_prefix.is_none() {
        return Err(ApiError::bad_request(
            "at least one of match_host or match_path_prefix is required",
        ));
    }

    if let Some(prefix) = match_path_prefix.as_ref() {
        if !prefix.starts_with('/') {
            return Err(ApiError::bad_request(
                "match_path_prefix must start with '/'",
            ));
        }
    }

    if let Some(prefix) = strip_path_prefix.as_ref() {
        if !prefix.starts_with('/') {
            return Err(ApiError::bad_request(
                "strip_path_prefix must start with '/'",
            ));
        }
    }

    let upstream_url = request.upstream_url.trim().to_string();
    validate_target_url(&upstream_url)?;
    if let Some(fallback) = fallback_upstream_url.as_ref() {
        validate_target_url(fallback)?;
    }
    let tunnel_id = request.tunnel_id.trim().to_string();
    if tunnel_id.is_empty() {
        return Err(ApiError::bad_request("tunnel_id is required"));
    }

    Ok(RouteRule {
        tunnel_id,
        id,
        match_host,
        match_path_prefix,
        strip_path_prefix,
        upstream_url,
        fallback_upstream_url,
        health_check_path,
        enabled: request.enabled.unwrap_or(true),
    })
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn ensure_route_id_matches(path_id: &str, body_id: &str) -> Result<(), ApiError> {
    if path_id == body_id {
        return Ok(());
    }
    Err(ApiError::bad_request(format!(
        "route id mismatch: path '{}' != body '{}'",
        path_id, body_id
    )))
}

fn replace_route(routes: &mut [RouteRule], route: RouteRule) -> bool {
    if let Some(index) = routes
        .iter()
        .position(|item| item.id == route.id && item.tunnel_id == route.tunnel_id)
    {
        routes[index] = route;
        return true;
    }
    false
}

#[derive(Debug, PartialEq, Eq)]
struct RouteApplyPlan {
    created: Vec<String>,
    updated: Vec<String>,
    unchanged: Vec<String>,
    removed: Vec<String>,
}

fn ensure_unique_route_ids(routes: &[RouteRule]) -> Result<(), ApiError> {
    let mut ids = HashSet::new();
    for route in routes {
        if !ids.insert((route.tunnel_id.as_str(), route.id.as_str())) {
            return Err(ApiError::bad_request(format!(
                "duplicate route id in payload: {} ({})",
                route.id,
                route.tunnel_id
            )));
        }
    }
    Ok(())
}

fn ensure_apply_payload_safe(
    routes: &[RouteRule],
    replace: bool,
    allow_empty: bool,
) -> Result<(), ApiError> {
    if replace && routes.is_empty() && !allow_empty {
        return Err(ApiError::bad_request(
            "refusing empty payload with replace=true; set allow_empty=true to confirm full cleanup",
        ));
    }
    Ok(())
}

fn build_route_apply_plan(
    existing: &[RouteRule],
    incoming: &[RouteRule],
    replace: bool,
) -> RouteApplyPlan {
    let existing_by_id = existing
        .iter()
        .map(|route| ((route.tunnel_id.as_str(), route.id.as_str()), route))
        .collect::<HashMap<_, _>>();
    let mut incoming_ids = HashSet::new();
    let mut created = Vec::new();
    let mut updated = Vec::new();
    let mut unchanged = Vec::new();

    for route in incoming {
        incoming_ids.insert((route.tunnel_id.as_str(), route.id.as_str()));
        match existing_by_id.get(&(route.tunnel_id.as_str(), route.id.as_str())) {
            Some(current) if *current == route => {
                unchanged.push(format!("{}:{}", route.tunnel_id, route.id));
            }
            Some(_) => {
                updated.push(format!("{}:{}", route.tunnel_id, route.id));
            }
            None => {
                created.push(format!("{}:{}", route.tunnel_id, route.id));
            }
        }
    }

    let removed = if replace {
        existing
            .iter()
            .filter(|route| !incoming_ids.contains(&(route.tunnel_id.as_str(), route.id.as_str())))
            .map(|route| format!("{}:{}", route.tunnel_id, route.id))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    RouteApplyPlan {
        created,
        updated,
        unchanged,
        removed,
    }
}

fn apply_route_rules(
    existing: &[RouteRule],
    incoming: Vec<RouteRule>,
    replace: bool,
) -> Vec<RouteRule> {
    if replace {
        return incoming;
    }

    let mut next = existing.to_vec();
    for route in incoming {
        if let Some(index) = next
            .iter()
            .position(|item| item.id == route.id && item.tunnel_id == route.tunnel_id)
        {
            if next[index] != route {
                next[index] = route;
            }
        } else {
            next.push(route);
        }
    }
    next
}

fn normalize_health_check_interval_ms(value: u64) -> anyhow::Result<u64> {
    if !(200..=60_000).contains(&value) {
        return Err(anyhow!(
            "health check interval_ms out of range: expected 200..=60000, got {}",
            value
        ));
    }
    Ok(value)
}

fn normalize_config_reload_interval_ms(value: u64) -> anyhow::Result<u64> {
    if !(200..=60_000).contains(&value) {
        return Err(anyhow!(
            "config reload interval_ms out of range: expected 200..=60000, got {}",
            value
        ));
    }
    Ok(value)
}

fn normalize_health_check_timeout_ms(value: u64) -> anyhow::Result<u64> {
    if !(100..=30_000).contains(&value) {
        return Err(anyhow!(
            "health check timeout_ms out of range: expected 100..=30000, got {}",
            value
        ));
    }
    Ok(value)
}

fn normalize_health_check_path(value: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("health check path cannot be empty"));
    }

    let normalized = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };

    if normalized.contains('?') || normalized.contains('#') {
        return Err(anyhow!(
            "health check path must not include query or fragment"
        ));
    }

    Ok(normalized)
}

fn apply_health_check_settings_update(
    current: &HealthCheckSettings,
    request: UpdateHealthCheckSettingsRequest,
) -> Result<HealthCheckSettings, ApiError> {
    let interval_ms = match request.interval_ms {
        Some(value) => normalize_health_check_interval_ms(value)
            .map_err(|err| ApiError::bad_request(format!("invalid interval_ms: {err}")))?,
        None => current.interval_ms,
    };
    let timeout_ms = match request.timeout_ms {
        Some(value) => normalize_health_check_timeout_ms(value)
            .map_err(|err| ApiError::bad_request(format!("invalid timeout_ms: {err}")))?,
        None => current.timeout_ms,
    };
    let path = match request.path {
        Some(value) => normalize_health_check_path(&value)
            .map_err(|err| ApiError::bad_request(format!("invalid path: {err}")))?,
        None => current.path.clone(),
    };

    Ok(HealthCheckSettings {
        interval_ms,
        timeout_ms,
        path,
    })
}

fn build_health_check_url(upstream_base_url: &str, health_check_path: &str) -> anyhow::Result<Url> {
    let mut url = Url::parse(upstream_base_url)
        .with_context(|| format!("invalid upstream URL: {upstream_base_url}"))?;
    url.set_path(health_check_path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn validate_target_url(value: &str) -> Result<(), ApiError> {
    let parsed = Url::parse(value.trim())
        .map_err(|_| ApiError::bad_request(format!("invalid URL: {}", value.trim())))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        _ => Err(ApiError::bad_request(
            "target URL must use http:// or https://",
        )),
    }
}

fn normalize_log_tail_lines(lines: Option<usize>) -> Result<usize, ApiError> {
    let lines = lines.unwrap_or(200);
    if lines == 0 {
        return Err(ApiError::bad_request("lines must be >= 1"));
    }
    if lines > 5_000 {
        return Err(ApiError::bad_request("lines must be <= 5000"));
    }
    Ok(lines)
}

fn normalize_log_stream_poll_ms(poll_ms: Option<u64>) -> Result<u64, ApiError> {
    let poll_ms = poll_ms.unwrap_or(1000);
    if poll_ms < 100 {
        return Err(ApiError::bad_request("poll_ms must be >= 100"));
    }
    if poll_ms > 10_000 {
        return Err(ApiError::bad_request("poll_ms must be <= 10000"));
    }
    Ok(poll_ms)
}

fn normalize_stream_interval_ms(interval_ms: Option<u64>) -> Result<u64, ApiError> {
    let interval_ms = interval_ms.unwrap_or(2_000);
    if interval_ms < 200 {
        return Err(ApiError::bad_request("interval_ms must be >= 200"));
    }
    if interval_ms > 60_000 {
        return Err(ApiError::bad_request("interval_ms must be <= 60000"));
    }
    Ok(interval_ms)
}

fn normalize_match_route_path(value: &str) -> Result<String, ApiError> {
    let path = value.trim();
    if path.is_empty() {
        return Err(ApiError::bad_request("path is required"));
    }
    if !path.starts_with('/') {
        return Err(ApiError::bad_request("path must start with '/'"));
    }
    Ok(path.to_string())
}

fn tail_lines(source: &str, count: usize) -> Vec<String> {
    source
        .lines()
        .rev()
        .take(count)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn default_tunnel_status(state: TunnelState) -> TunnelStatus {
    TunnelStatus {
        state,
        provider: None,
        target_url: Some(DEFAULT_GATEWAY_TARGET_URL.to_string()),
        public_base_url: None,
        started_at: None,
        updated_at: now_iso(),
        process_id: None,
        auto_restart: true,
        restart_count: 0,
        last_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::HeaderMap;
    use axum::http::Method;
    use axum::http::header::CONNECTION;
    use axum::http::header::UPGRADE;
    use futures_util::{SinkExt, StreamExt};
    use reqwest::Client as ReqwestClient;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::net::TcpListener;
    use tokio::process::Command as TokioCommand;
    use tokio_rustls::TlsAcceptor;
    use tokio_rustls::rustls;
    use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use tokio_tungstenite::tungstenite::Message;

    fn route(
        id: &str,
        host: Option<&str>,
        prefix: Option<&str>,
        strip: Option<&str>,
        enabled: bool,
    ) -> RouteRule {
        RouteRule {
            tunnel_id: "primary".to_string(),
            id: id.to_string(),
            match_host: host.map(ToString::to_string),
            match_path_prefix: prefix.map(ToString::to_string),
            strip_path_prefix: strip.map(ToString::to_string),
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: None,
            health_check_path: None,
            enabled,
        }
    }

    fn test_health(healthy: bool) -> UpstreamHealth {
        UpstreamHealth {
            healthy,
            last_checked_at: "2026-03-05T00:00:00Z".to_string(),
            last_error: if healthy {
                None
            } else {
                Some("status 503".to_string())
            },
        }
    }

    fn test_health_key(upstream_url: &str, health_check_path: &str) -> UpstreamHealthKey {
        upstream_health_key(upstream_url, health_check_path)
    }

    fn next_test_id() -> u64 {
        static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);
        NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
    }

    fn test_state_with_routes(routes: Vec<RouteRule>, api_token: Option<&str>) -> Arc<AppState> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let test_id = next_test_id();

        let mut http_connector = HttpConnector::new();
        http_connector.enforce_http(false);
        let https_connector = HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .wrap_connector(http_connector);

        let data_file = PathBuf::from(format!("/tmp/tunnelmux-test-state-{test_id}.json"));
        let config_file = PathBuf::from(format!("/tmp/tunnelmux-test-config-{test_id}.json"));
        let provider_log_file =
            PathBuf::from(format!("/tmp/tunnelmux-test-provider-{test_id}.log"));
        let _ = std::fs::remove_file(&data_file);
        let _ = std::fs::remove_file(&config_file);
        let _ = std::fs::remove_file(&provider_log_file);

        Arc::new(AppState {
            runtime: Mutex::new(RuntimeState {
                persisted: PersistedState {
                    tunnel: default_tunnel_status(TunnelState::Idle),
                    routes,
                    health_check: Some(HealthCheckSettings {
                        interval_ms: 5_000,
                        timeout_ms: 2_000,
                        path: "/".to_string(),
                    }),
                },
                running_tunnel: None,
                pending_restart: None,
            }),
            upstream_health: Mutex::new(HashMap::new()),
            health_check_settings: RwLock::new(HealthCheckSettings {
                interval_ms: 5_000,
                timeout_ms: 2_000,
                path: "/".to_string(),
            }),
            data_file,
            config_file,
            config_reload_status: Mutex::new(ConfigReloadStatus {
                enabled: true,
                interval_ms: 1_000,
                ..ConfigReloadStatus::default()
            }),
            provider_log_file,
            api_token: api_token.map(ToString::to_string),
            cloudflared_bin: "cloudflared".to_string(),
            ngrok_bin: "ngrok".to_string(),
            ready_timeout_ms: 15_000,
            max_auto_restarts: 10,
            proxy_client: reqwest::Client::new(),
            ws_proxy_client: HyperClient::builder(TokioExecutor::new()).build(https_connector),
        })
    }

    async fn spawn_control_test_server(
        state: Arc<AppState>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let protected_control_app = Router::new()
            .route("/v1/tunnel/status", get(get_tunnel_status))
            .route("/v1/tunnel/status/stream", get(stream_tunnel_status))
            .route("/v1/tunnels/workspace", get(get_tunnel_workspace))
            .route(
                "/v1/settings/health-check",
                get(get_health_check_settings).put(update_health_check_settings),
            )
            .route("/v1/settings/reload", post(reload_settings))
            .route("/v1/dashboard", get(get_dashboard))
            .route("/v1/dashboard/stream", get(stream_dashboard))
            .route("/v1/metrics", get(get_metrics))
            .route("/v1/metrics/stream", get(stream_metrics))
            .route("/v1/diagnostics", get(get_diagnostics))
            .route("/v1/upstreams/health", get(get_upstreams_health))
            .route("/v1/upstreams/health/stream", get(stream_upstreams_health))
            .route("/v1/routes", get(list_routes).post(add_route))
            .route("/v1/routes/match", get(match_route))
            .route("/v1/routes/stream", get(stream_routes))
            .route("/v1/routes/apply", post(apply_routes))
            .route(
                "/v1/routes/{id}",
                axum::routing::delete(delete_route).put(update_route),
            )
            .layer(middleware::from_fn_with_state(
                state.clone(),
                control_auth_middleware,
            ));
        let control_app = Router::new()
            .route("/v1/health", get(health))
            .merge(protected_control_app)
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind control listener");
        let addr = listener.local_addr().expect("control listener addr");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, control_app).await;
        });
        (format!("http://{}", addr), task)
    }

    #[test]
    fn select_route_prefers_host_specific() {
        let routes = vec![
            route("fallback", None, Some("/"), None, true),
            route("host-specific", Some("app.local"), Some("/"), None, true),
        ];
        let selected =
            select_route(&routes, Some("app.local"), "/chat").expect("route should exist");
        assert_eq!(selected.id, "host-specific");
    }

    #[test]
    fn select_route_prefers_longest_prefix() {
        let routes = vec![
            route("root", None, Some("/"), None, true),
            route("app", None, Some("/app"), None, true),
            route("app-deep", None, Some("/app/v1"), None, true),
        ];
        let selected =
            select_route(&routes, Some("any.local"), "/app/v1/health").expect("route should exist");
        assert_eq!(selected.id, "app-deep");
    }

    #[test]
    fn rewrite_path_applies_strip_prefix() {
        let route = route("app", None, Some("/app"), Some("/app"), true);
        assert_eq!(rewrite_path("/app", &route), "/");
        assert_eq!(rewrite_path("/app/metrics", &route), "/metrics");
    }

    #[test]
    fn join_upstream_path_joins_base_with_forwarded_path() {
        assert_eq!(join_upstream_path("/base", "/metrics"), "/base/metrics");
        assert_eq!(join_upstream_path("/base/", "/metrics"), "/base/metrics");
        assert_eq!(join_upstream_path("/", "/metrics"), "/metrics");
        assert_eq!(join_upstream_path("/base", "/"), "/base");
    }

    #[test]
    fn websocket_upgrade_detection_requires_get_and_upgrade_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, "Upgrade".parse().expect("valid header value"));
        headers.insert(UPGRADE, "websocket".parse().expect("valid header value"));

        assert!(is_websocket_upgrade_request(&Method::GET, &headers));
        assert!(!is_websocket_upgrade_request(&Method::POST, &headers));
    }

    #[test]
    fn websocket_upgrade_detection_is_case_insensitive() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONNECTION,
            "keep-alive, UPGRADE".parse().expect("valid header value"),
        );
        headers.insert(UPGRADE, "WebSocket".parse().expect("valid header value"));

        assert!(is_websocket_upgrade_request(&Method::GET, &headers));
    }

    #[tokio::test]
    async fn websocket_proxy_supports_wss_upstream() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let test_id = next_test_id();

        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("create self-signed cert");
        let cert_der: CertificateDer<'static> = certified.cert.der().clone();
        let key_der =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));

        let server_tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der.clone()], key_der)
            .expect("build server tls config");
        let tls_acceptor = TlsAcceptor::from(Arc::new(server_tls));

        let upstream_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind upstream listener");
        let upstream_addr = upstream_listener.local_addr().expect("upstream addr");
        let upstream_task = tokio::spawn(async move {
            let (stream, _) = upstream_listener.accept().await.expect("accept upstream");
            let tls_stream = tls_acceptor.accept(stream).await.expect("tls accept");
            let mut ws_stream = tokio_tungstenite::accept_async(tls_stream)
                .await
                .expect("ws accept");
            if let Some(Ok(message)) = ws_stream.next().await {
                ws_stream.send(message).await.expect("echo back");
            }
        });

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(cert_der)
            .expect("add self-signed cert to trust store");
        let client_tls = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let mut http_connector = HttpConnector::new();
        http_connector.enforce_http(false);
        let https_connector = HttpsConnectorBuilder::new()
            .with_tls_config(client_tls)
            .https_or_http()
            .enable_http1()
            .wrap_connector(http_connector);

        let data_file = PathBuf::from(format!("/tmp/tunnelmux-test-state-{test_id}.json"));
        let config_file = PathBuf::from(format!("/tmp/tunnelmux-test-config-{test_id}.json"));
        let provider_log_file =
            PathBuf::from(format!("/tmp/tunnelmux-test-provider-{test_id}.log"));
        let _ = std::fs::remove_file(&data_file);
        let _ = std::fs::remove_file(&config_file);
        let _ = std::fs::remove_file(&provider_log_file);

        let state = Arc::new(AppState {
            runtime: Mutex::new(RuntimeState {
                persisted: PersistedState {
                    tunnel: default_tunnel_status(TunnelState::Idle),
                    routes: vec![RouteRule {
                        tunnel_id: "primary".to_string(),
                        id: "wss".to_string(),
                        match_host: None,
                        match_path_prefix: Some("/".to_string()),
                        strip_path_prefix: None,
                        upstream_url: format!("https://localhost:{}", upstream_addr.port()),
                        fallback_upstream_url: None,
                        health_check_path: None,
                        enabled: true,
                    }],
                    health_check: Some(HealthCheckSettings {
                        interval_ms: 5_000,
                        timeout_ms: 2_000,
                        path: "/".to_string(),
                    }),
                },
                running_tunnel: None,
                pending_restart: None,
            }),
            upstream_health: Mutex::new(HashMap::new()),
            health_check_settings: RwLock::new(HealthCheckSettings {
                interval_ms: 5_000,
                timeout_ms: 2_000,
                path: "/".to_string(),
            }),
            data_file,
            config_file,
            config_reload_status: Mutex::new(ConfigReloadStatus {
                enabled: true,
                interval_ms: 1_000,
                ..ConfigReloadStatus::default()
            }),
            provider_log_file,
            api_token: None,
            cloudflared_bin: "cloudflared".to_string(),
            ngrok_bin: "ngrok".to_string(),
            ready_timeout_ms: 15_000,
            max_auto_restarts: 10,
            proxy_client: reqwest::Client::new(),
            ws_proxy_client: HyperClient::builder(TokioExecutor::new()).build(https_connector),
        });

        let gateway_app = Router::new().fallback(proxy_request).with_state(state);
        let gateway_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway listener");
        let gateway_addr = gateway_listener.local_addr().expect("gateway addr");
        let gateway_task = tokio::spawn(async move {
            let _ = axum::serve(gateway_listener, gateway_app).await;
        });

        let ws_url = format!("ws://127.0.0.1:{}/echo", gateway_addr.port());
        let (mut client_ws, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .expect("connect to gateway websocket");
        client_ws
            .send(Message::Text("hello-wss".into()))
            .await
            .expect("send websocket frame");

        let reply = client_ws
            .next()
            .await
            .expect("receive websocket reply")
            .expect("valid websocket frame");
        match reply {
            Message::Text(text) => assert_eq!(text.as_str(), "hello-wss"),
            other => panic!("unexpected websocket reply: {other:?}"),
        }

        let _ = client_ws.close(None).await;
        upstream_task.await.expect("upstream task completed");
        gateway_task.abort();
    }

    #[tokio::test]
    async fn gateway_returns_welcome_page_when_no_routes_configured() {
        let state = test_state_with_routes(vec![], None);
        let gateway_app = Router::new().fallback(proxy_request).with_state(state);
        let gateway_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway listener");
        let gateway_addr = gateway_listener.local_addr().expect("gateway addr");
        let gateway_task = tokio::spawn(async move {
            let _ = axum::serve(gateway_listener, gateway_app).await;
        });

        let client = ReqwestClient::new();
        let response = client
            .get(format!("http://{}/", gateway_addr))
            .send()
            .await
            .expect("request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/html"));

        let body = response.text().await.expect("body should decode");
        assert!(body.contains("TunnelMux is live"));
        assert!(body.contains("Add your first service"));

        gateway_task.abort();
    }

    #[tokio::test]
    async fn control_endpoint_rejects_unauthorized_requests() {
        let state = test_state_with_routes(vec![], Some("secret-token"));
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .get(format!("{base_url}/v1/settings/health-check"))
            .send()
            .await
            .expect("request should complete");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = client
            .get(format!("{base_url}/v1/settings/health-check"))
            .bearer_auth("secret-token")
            .send()
            .await
            .expect("authorized request should complete");
        assert_eq!(response.status(), StatusCode::OK);

        server_task.abort();
    }

    #[tokio::test]
    async fn health_check_settings_endpoint_updates_runtime_config() {
        let state = test_state_with_routes(vec![], None);
        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let initial: HealthCheckSettingsResponse = client
            .get(format!("{base_url}/v1/settings/health-check"))
            .send()
            .await
            .expect("initial request should complete")
            .json()
            .await
            .expect("initial payload should decode");
        assert_eq!(
            initial.health_check,
            HealthCheckSettings {
                interval_ms: 5_000,
                timeout_ms: 2_000,
                path: "/".to_string(),
            }
        );

        let updated: HealthCheckSettingsResponse = client
            .put(format!("{base_url}/v1/settings/health-check"))
            .json(&UpdateHealthCheckSettingsRequest {
                interval_ms: Some(1_000),
                timeout_ms: Some(1_500),
                path: Some("/ready".to_string()),
            })
            .send()
            .await
            .expect("update request should complete")
            .json()
            .await
            .expect("updated payload should decode");
        assert_eq!(
            updated.health_check,
            HealthCheckSettings {
                interval_ms: 1_000,
                timeout_ms: 1_500,
                path: "/ready".to_string(),
            }
        );

        let current: HealthCheckSettingsResponse = client
            .get(format!("{base_url}/v1/settings/health-check"))
            .send()
            .await
            .expect("readback request should complete")
            .json()
            .await
            .expect("readback payload should decode");
        assert_eq!(current.health_check, updated.health_check);
        let runtime = state.runtime.lock().await;
        assert_eq!(
            runtime.persisted.health_check,
            Some(updated.health_check.clone())
        );
        drop(runtime);

        server_task.abort();
    }

    #[tokio::test]
    async fn settings_reload_endpoint_prefers_config_file_when_present() {
        let state = test_state_with_routes(
            vec![route("svc-a", Some("old.local"), Some("/"), None, true)],
            None,
        );

        save_state_file(
            &state.data_file,
            &PersistedState {
                tunnel: default_tunnel_status(TunnelState::Stopped),
                routes: vec![route(
                    "svc-state",
                    Some("state.local"),
                    Some("/state"),
                    None,
                    true,
                )],
                health_check: Some(HealthCheckSettings {
                    interval_ms: 6_000,
                    timeout_ms: 2_500,
                    path: "/state".to_string(),
                }),
            },
        )
        .await
        .expect("state fixture should persist");

        save_config_file(
            &state.config_file,
            &DeclarativeConfigFile {
                routes: vec![RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-config".to_string(),
                    match_host: Some("config.local".to_string()),
                    match_path_prefix: Some("/config".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:4100".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: Some("/readyz".to_string()),
                    enabled: true,
                }],
                health_check: Some(HealthCheckSettings {
                    interval_ms: 7_500,
                    timeout_ms: 1_500,
                    path: "/readyz".to_string(),
                }),
            },
        )
        .await
        .expect("config fixture should persist");

        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let response = client
            .post(format!("{base_url}/v1/settings/reload"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("reload request should complete");
        assert_eq!(response.status(), StatusCode::OK);

        let routes: RoutesResponse = client
            .get(format!("{base_url}/v1/routes"))
            .send()
            .await
            .expect("routes request should complete")
            .json()
            .await
            .expect("routes response should decode");
        assert_eq!(routes.routes.len(), 1);
        assert_eq!(routes.routes[0].id, "svc-config");

        let health: HealthCheckSettingsResponse = client
            .get(format!("{base_url}/v1/settings/health-check"))
            .send()
            .await
            .expect("health settings request should complete")
            .json()
            .await
            .expect("health settings response should decode");
        assert_eq!(health.health_check.interval_ms, 7_500);
        assert_eq!(health.health_check.path, "/readyz");

        server_task.abort();
    }

    #[tokio::test]
    async fn settings_reload_endpoint_refreshes_routes_from_disk() {
        let state = test_state_with_routes(
            vec![route("svc-a", Some("old.local"), Some("/"), None, true)],
            None,
        );
        {
            let mut runtime = state.runtime.lock().await;
            runtime.persisted.tunnel = default_tunnel_status(TunnelState::Error);
            runtime.persisted.tunnel.last_error = Some("keep current runtime state".to_string());
        }

        save_state_file(
            &state.data_file,
            &PersistedState {
                tunnel: default_tunnel_status(TunnelState::Stopped),
                routes: vec![RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-b".to_string(),
                    match_host: Some("new.local".to_string()),
                    match_path_prefix: Some("/new".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:4000".to_string(),
                    fallback_upstream_url: Some("http://127.0.0.1:4001".to_string()),
                    health_check_path: Some("/readyz".to_string()),
                    enabled: true,
                }],
                health_check: Some(HealthCheckSettings {
                    interval_ms: 7_500,
                    timeout_ms: 1_500,
                    path: "/readyz".to_string(),
                }),
            },
        )
        .await
        .expect("reload fixture should persist");

        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let response = client
            .post(format!("{base_url}/v1/settings/reload"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("reload request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value = response
            .json()
            .await
            .expect("reload response should decode");
        assert_eq!(payload["reloaded"], true);
        assert_eq!(payload["route_count"], 1);
        assert_eq!(payload["tunnel_state"], "error");

        let routes: RoutesResponse = client
            .get(format!("{base_url}/v1/routes"))
            .send()
            .await
            .expect("routes request should complete")
            .json()
            .await
            .expect("routes response should decode");
        assert_eq!(routes.routes.len(), 1);
        assert_eq!(routes.routes[0].id, "svc-b");
        assert_eq!(routes.routes[0].match_host.as_deref(), Some("new.local"));

        let health: HealthCheckSettingsResponse = client
            .get(format!("{base_url}/v1/settings/health-check"))
            .send()
            .await
            .expect("health settings request should complete")
            .json()
            .await
            .expect("health settings response should decode");
        assert_eq!(health.health_check.interval_ms, 7_500);
        assert_eq!(health.health_check.timeout_ms, 1_500);
        assert_eq!(health.health_check.path, "/readyz");

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 1);
        assert_eq!(runtime.persisted.routes[0].id, "svc-b");
        assert_eq!(runtime.persisted.tunnel.state, TunnelState::Error);
        assert_eq!(
            runtime.persisted.tunnel.last_error.as_deref(),
            Some("keep current runtime state")
        );
        drop(runtime);

        let live_health = state.health_check_settings.read().await;
        assert_eq!(live_health.interval_ms, 7_500);
        assert_eq!(live_health.timeout_ms, 1_500);
        assert_eq!(live_health.path, "/readyz");
        drop(live_health);

        server_task.abort();
    }

    #[tokio::test]
    async fn update_route_endpoint_updates_existing_route() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let response = client
            .put(format!("{base_url}/v1/routes/svc-a"))
            .json(&CreateRouteRequest {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3010".to_string(),
                fallback_upstream_url: Some("http://127.0.0.1:3011".to_string()),
                health_check_path: Some("/healthz".to_string()),
                enabled: Some(false),
            })
            .send()
            .await
            .expect("update route request should complete");
        assert_eq!(response.status(), StatusCode::OK);

        let payload: RouteRule = response.json().await.expect("route response should decode");
        assert_eq!(payload.id, "svc-a");
        assert_eq!(payload.match_host.as_deref(), Some("demo.local"));
        assert_eq!(payload.upstream_url, "http://127.0.0.1:3010");
        assert!(!payload.enabled);

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 1);
        assert_eq!(
            runtime.persisted.routes[0].upstream_url,
            "http://127.0.0.1:3010"
        );
        assert_eq!(
            runtime.persisted.routes[0].health_check_path.as_deref(),
            Some("/healthz")
        );

        drop(runtime);
        server_task.abort();
    }

    #[tokio::test]
    async fn update_route_endpoint_rejects_id_mismatch() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .put(format!("{base_url}/v1/routes/svc-a"))
            .json(&CreateRouteRequest {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: Some(true),
            })
            .send()
            .await
            .expect("mismatch request should complete");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        server_task.abort();
    }

    #[tokio::test]
    async fn update_route_endpoint_returns_not_found_when_missing_without_upsert() {
        let state = test_state_with_routes(vec![], None);
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .put(format!("{base_url}/v1/routes/svc-a"))
            .json(&CreateRouteRequest {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: Some(true),
            })
            .send()
            .await
            .expect("update route request should complete");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        server_task.abort();
    }

    #[tokio::test]
    async fn update_route_endpoint_upsert_creates_missing_route() {
        let state = test_state_with_routes(vec![], None);
        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let response = client
            .put(format!("{base_url}/v1/routes/svc-a?upsert=true"))
            .json(&CreateRouteRequest {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
                health_check_path: Some("/healthz".to_string()),
                enabled: Some(true),
            })
            .send()
            .await
            .expect("upsert route request should complete");
        assert_eq!(response.status(), StatusCode::CREATED);

        let payload: RouteRule = response.json().await.expect("route response should decode");
        assert_eq!(payload.id, "svc-a");
        assert_eq!(payload.match_host.as_deref(), Some("demo.local"));
        assert_eq!(payload.upstream_url, "http://127.0.0.1:3000");
        assert_eq!(
            payload.fallback_upstream_url.as_deref(),
            Some("http://127.0.0.1:3001")
        );

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 1);
        assert_eq!(runtime.persisted.routes[0], payload);
        drop(runtime);

        server_task.abort();
    }

    #[tokio::test]
    async fn create_route_allows_same_id_in_different_tunnels() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let response = client
            .post(format!("{base_url}/v1/routes"))
            .json(&CreateRouteRequest {
                tunnel_id: "tunnel-2".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("api.local".to_string()),
                match_path_prefix: Some("/".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:4000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: Some(true),
            })
            .send()
            .await
            .expect("create route request should complete");

        assert_eq!(response.status(), StatusCode::CREATED);

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 2);
        drop(runtime);
        server_task.abort();
    }

    #[tokio::test]
    async fn list_routes_endpoint_filters_by_tunnel_id() {
        let state = test_state_with_routes(
            vec![
                RouteRule {
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
                RouteRule {
                    tunnel_id: "tunnel-2".to_string(),
                    id: "svc-a".to_string(),
                    match_host: Some("api.local".to_string()),
                    match_path_prefix: Some("/".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:4000".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: true,
                },
            ],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let payload: RoutesResponse = client
            .get(format!("{base_url}/v1/routes"))
            .query(&[("tunnel_id", "tunnel-2")])
            .send()
            .await
            .expect("routes request should complete")
            .json()
            .await
            .expect("routes response should decode");

        assert_eq!(payload.routes.len(), 1);
        assert_eq!(payload.routes[0].tunnel_id, "tunnel-2");
        assert_eq!(payload.routes[0].upstream_url, "http://127.0.0.1:4000");

        server_task.abort();
    }

    #[tokio::test]
    async fn delete_route_endpoint_uses_tunnel_scope() {
        let state = test_state_with_routes(
            vec![
                RouteRule {
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
                RouteRule {
                    tunnel_id: "tunnel-2".to_string(),
                    id: "svc-a".to_string(),
                    match_host: Some("api.local".to_string()),
                    match_path_prefix: Some("/".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:4000".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: true,
                },
            ],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let response = client
            .delete(format!("{base_url}/v1/routes/svc-a"))
            .query(&[("tunnel_id", "tunnel-2")])
            .send()
            .await
            .expect("delete request should complete");

        assert_eq!(response.status(), StatusCode::OK);

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 1);
        assert_eq!(runtime.persisted.routes[0].tunnel_id, "primary");

        server_task.abort();
    }

    #[tokio::test]
    async fn routes_match_endpoint_returns_selected_route_and_targets() {
        let state = test_state_with_routes(
            vec![
                RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-root".to_string(),
                    match_host: None,
                    match_path_prefix: Some("/".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:3009".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: true,
                },
                RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-api".to_string(),
                    match_host: Some("demo.local".to_string()),
                    match_path_prefix: Some("/api".to_string()),
                    strip_path_prefix: Some("/api".to_string()),
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
                    health_check_path: None,
                    enabled: true,
                },
            ],
            None,
        );
        {
            let mut map = state.upstream_health.lock().await;
            map.insert(
                test_health_key("http://127.0.0.1:3000", "/"),
                test_health(false),
            );
            map.insert(
                test_health_key("http://127.0.0.1:3001", "/"),
                test_health(true),
            );
        }
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let payload: RouteMatchResponse = client
            .get(format!("{base_url}/v1/routes/match"))
            .query(&[("host", "demo.local"), ("path", "/api/v1/ping")])
            .send()
            .await
            .expect("match request should complete")
            .json()
            .await
            .expect("match response should decode");

        assert!(payload.matched);
        assert_eq!(payload.host.as_deref(), Some("demo.local"));
        assert_eq!(payload.path, "/api/v1/ping");
        assert_eq!(payload.forwarded_path.as_deref(), Some("/v1/ping"));
        assert_eq!(payload.health_check_path.as_deref(), Some("/"));
        let route = payload.route.expect("route should exist");
        assert_eq!(route.id, "svc-api");
        assert_eq!(payload.targets.len(), 2);
        assert_eq!(payload.targets[0].upstream_url, "http://127.0.0.1:3001");
        assert_eq!(payload.targets[0].healthy, Some(true));
        assert_eq!(payload.targets[1].upstream_url, "http://127.0.0.1:3000");
        assert_eq!(payload.targets[1].healthy, Some(false));

        server_task.abort();
    }

    #[tokio::test]
    async fn routes_match_endpoint_reports_unmatched_route() {
        let state = test_state_with_routes(vec![], None);
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let payload: RouteMatchResponse = client
            .get(format!("{base_url}/v1/routes/match"))
            .query(&[("host", "demo.local"), ("path", "/missing")])
            .send()
            .await
            .expect("match request should complete")
            .json()
            .await
            .expect("match response should decode");

        assert!(!payload.matched);
        assert_eq!(payload.host.as_deref(), Some("demo.local"));
        assert_eq!(payload.path, "/missing");
        assert!(payload.route.is_none());
        assert!(payload.forwarded_path.is_none());
        assert!(payload.health_check_path.is_none());
        assert!(payload.targets.is_empty());

        server_task.abort();
    }

    #[tokio::test]
    async fn apply_routes_endpoint_dry_run_does_not_mutate_runtime() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let response = client
            .post(format!("{base_url}/v1/routes/apply"))
            .json(&ApplyRoutesRequest {
                routes: vec![CreateRouteRequest {
                    tunnel_id: "primary".to_string(),
                    id: "svc-b".to_string(),
                    match_host: Some("demo.local".to_string()),
                    match_path_prefix: Some("/b".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:3001".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: Some(true),
                }],
                replace: Some(true),
                dry_run: Some(true),
                allow_empty: Some(false),
            })
            .send()
            .await
            .expect("apply request should complete");
        assert_eq!(response.status(), StatusCode::OK);

        let payload: ApplyRoutesResponse =
            response.json().await.expect("apply response should decode");
        assert_eq!(payload.applied, 1);
        assert_eq!(payload.created, vec!["primary:svc-b".to_string()]);
        assert_eq!(payload.updated, Vec::<String>::new());
        assert_eq!(payload.unchanged, Vec::<String>::new());
        assert_eq!(payload.removed, vec!["primary:svc-a".to_string()]);
        assert!(payload.replace);
        assert!(payload.dry_run);

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 1);
        assert_eq!(runtime.persisted.routes[0].id, "svc-a");
        drop(runtime);

        server_task.abort();
    }

    #[tokio::test]
    async fn apply_routes_endpoint_replaces_routes_when_enabled() {
        let state = test_state_with_routes(
            vec![
                RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-a".to_string(),
                    match_host: Some("demo.local".to_string()),
                    match_path_prefix: Some("/a".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: true,
                },
                RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-b".to_string(),
                    match_host: Some("demo.local".to_string()),
                    match_path_prefix: Some("/b".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:3001".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: true,
                },
            ],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state.clone()).await;
        let client = ReqwestClient::new();

        let response = client
            .post(format!("{base_url}/v1/routes/apply"))
            .json(&ApplyRoutesRequest {
                routes: vec![
                    CreateRouteRequest {
                        tunnel_id: "primary".to_string(),
                        id: "svc-b".to_string(),
                        match_host: Some("demo.local".to_string()),
                        match_path_prefix: Some("/b".to_string()),
                        strip_path_prefix: None,
                        upstream_url: "http://127.0.0.1:3011".to_string(),
                        fallback_upstream_url: None,
                        health_check_path: None,
                        enabled: Some(false),
                    },
                    CreateRouteRequest {
                        tunnel_id: "primary".to_string(),
                        id: "svc-c".to_string(),
                        match_host: Some("demo.local".to_string()),
                        match_path_prefix: Some("/c".to_string()),
                        strip_path_prefix: None,
                        upstream_url: "http://127.0.0.1:3002".to_string(),
                        fallback_upstream_url: None,
                        health_check_path: Some("/ready".to_string()),
                        enabled: Some(true),
                    },
                ],
                replace: Some(true),
                dry_run: Some(false),
                allow_empty: Some(false),
            })
            .send()
            .await
            .expect("apply request should complete");
        assert_eq!(response.status(), StatusCode::OK);

        let payload: ApplyRoutesResponse =
            response.json().await.expect("apply response should decode");
        assert_eq!(payload.applied, 2);
        assert_eq!(payload.created, vec!["primary:svc-c".to_string()]);
        assert_eq!(payload.updated, vec!["primary:svc-b".to_string()]);
        assert_eq!(payload.unchanged, Vec::<String>::new());
        assert_eq!(payload.removed, vec!["primary:svc-a".to_string()]);
        assert!(payload.replace);
        assert!(!payload.dry_run);

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 2);
        assert_eq!(runtime.persisted.routes[0].id, "svc-b");
        assert_eq!(
            runtime.persisted.routes[0].upstream_url,
            "http://127.0.0.1:3011"
        );
        assert!(!runtime.persisted.routes[0].enabled);
        assert_eq!(runtime.persisted.routes[1].id, "svc-c");
        assert_eq!(
            runtime.persisted.routes[1].health_check_path.as_deref(),
            Some("/ready")
        );
        drop(runtime);

        server_task.abort();
    }

    #[tokio::test]
    async fn apply_routes_endpoint_rejects_empty_replace_without_allow_empty() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .post(format!("{base_url}/v1/routes/apply"))
            .json(&ApplyRoutesRequest {
                routes: Vec::new(),
                replace: Some(true),
                dry_run: Some(false),
                allow_empty: Some(false),
            })
            .send()
            .await
            .expect("apply request should complete");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        server_task.abort();
    }

    #[tokio::test]
    async fn apply_routes_endpoint_reports_unchanged_routes() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .post(format!("{base_url}/v1/routes/apply"))
            .json(&ApplyRoutesRequest {
                routes: vec![CreateRouteRequest {
                    tunnel_id: "primary".to_string(),
                    id: "svc-a".to_string(),
                    match_host: Some("demo.local".to_string()),
                    match_path_prefix: Some("/a".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: Some(true),
                }],
                replace: Some(false),
                dry_run: Some(true),
                allow_empty: Some(false),
            })
            .send()
            .await
            .expect("apply request should complete");
        assert_eq!(response.status(), StatusCode::OK);

        let payload: ApplyRoutesResponse =
            response.json().await.expect("apply response should decode");
        assert_eq!(payload.applied, 1);
        assert_eq!(payload.created, Vec::<String>::new());
        assert_eq!(payload.updated, Vec::<String>::new());
        assert_eq!(payload.unchanged, vec!["primary:svc-a".to_string()]);
        assert_eq!(payload.removed, Vec::<String>::new());

        server_task.abort();
    }

    #[tokio::test]
    async fn dashboard_endpoint_returns_composite_snapshot() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
                health_check_path: Some("/ready".to_string()),
                enabled: true,
            }],
            None,
        );
        {
            let mut health_map = state.upstream_health.lock().await;
            health_map.insert(
                test_health_key("http://127.0.0.1:3000", "/ready"),
                test_health(true),
            );
            health_map.insert(
                test_health_key("http://127.0.0.1:3001", "/ready"),
                test_health(false),
            );
        }
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let dashboard: DashboardResponse = client
            .get(format!("{base_url}/v1/dashboard"))
            .send()
            .await
            .expect("dashboard request should complete")
            .json()
            .await
            .expect("dashboard payload should decode");

        assert_eq!(dashboard.metrics.route_count, 1);
        assert_eq!(dashboard.metrics.enabled_route_count, 1);
        assert_eq!(dashboard.metrics.upstream_health_entries, 2);
        assert_eq!(dashboard.routes.len(), 1);
        assert_eq!(dashboard.routes[0].id, "svc-a");
        assert_eq!(dashboard.upstreams.len(), 2);

        server_task.abort();
    }

    #[tokio::test]
    async fn tunnel_workspace_endpoint_returns_empty_when_unconfigured() {
        let state = test_state_with_routes(vec![], None);
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let workspace: tunnelmux_core::TunnelWorkspaceResponse = client
            .get(format!("{base_url}/v1/tunnels/workspace"))
            .send()
            .await
            .expect("workspace request should complete")
            .json()
            .await
            .expect("workspace payload should decode");

        assert!(workspace.tunnels.is_empty());
        assert_eq!(workspace.current_tunnel_id, None);

        server_task.abort();
    }

    #[tokio::test]
    async fn tunnel_workspace_endpoint_returns_single_primary_tunnel() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        {
            let mut runtime = state.runtime.lock().await;
            runtime.persisted.tunnel = TunnelStatus {
                state: TunnelState::Running,
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
        }
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let workspace: tunnelmux_core::TunnelWorkspaceResponse = client
            .get(format!("{base_url}/v1/tunnels/workspace"))
            .send()
            .await
            .expect("workspace request should complete")
            .json()
            .await
            .expect("workspace payload should decode");

        assert_eq!(workspace.current_tunnel_id.as_deref(), Some("primary"));
        assert_eq!(workspace.tunnels.len(), 1);
        assert_eq!(workspace.tunnels[0].id, "primary");
        assert_eq!(workspace.tunnels[0].route_count, 1);
        assert_eq!(workspace.tunnels[0].enabled_route_count, 1);
        assert_eq!(workspace.tunnels[0].provider, Some(TunnelProvider::Cloudflared));

        server_task.abort();
    }

    #[tokio::test]
    async fn dashboard_stream_endpoint_emits_snapshot_event() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .get(format!("{base_url}/v1/dashboard/stream?interval_ms=200"))
            .send()
            .await
            .expect("dashboard stream request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/event-stream"));

        let mut stream = response.bytes_stream();
        let first_chunk = timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("stream should emit within timeout")
            .expect("stream should produce chunk")
            .expect("chunk should decode");
        let body = String::from_utf8_lossy(&first_chunk);
        assert!(body.contains("event: snapshot"));
        assert!(body.contains("data: "));

        server_task.abort();
    }

    #[tokio::test]
    async fn tunnel_status_stream_endpoint_emits_snapshot_event() {
        let state = test_state_with_routes(vec![], None);
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .get(format!(
                "{base_url}/v1/tunnel/status/stream?interval_ms=200"
            ))
            .send()
            .await
            .expect("tunnel status stream request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/event-stream"));

        let mut stream = response.bytes_stream();
        let first_chunk = timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("stream should emit within timeout")
            .expect("stream should produce chunk")
            .expect("chunk should decode");
        let body = String::from_utf8_lossy(&first_chunk);
        assert!(body.contains("event: snapshot"));
        assert!(body.contains("data: "));

        server_task.abort();
    }

    #[tokio::test]
    async fn upstreams_health_stream_endpoint_emits_snapshot_event() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .get(format!(
                "{base_url}/v1/upstreams/health/stream?interval_ms=200"
            ))
            .send()
            .await
            .expect("upstreams health stream request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/event-stream"));

        let mut stream = response.bytes_stream();
        let first_chunk = timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("stream should emit within timeout")
            .expect("stream should produce chunk")
            .expect("chunk should decode");
        let body = String::from_utf8_lossy(&first_chunk);
        assert!(body.contains("event: snapshot"));
        assert!(body.contains("data: "));

        server_task.abort();
    }

    #[tokio::test]
    async fn routes_stream_endpoint_emits_snapshot_event() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .get(format!("{base_url}/v1/routes/stream?interval_ms=200"))
            .send()
            .await
            .expect("routes stream request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/event-stream"));

        let mut stream = response.bytes_stream();
        let first_chunk = timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("stream should emit within timeout")
            .expect("stream should produce chunk")
            .expect("chunk should decode");
        let body = String::from_utf8_lossy(&first_chunk);
        assert!(body.contains("event: snapshot"));
        assert!(body.contains("data: "));

        server_task.abort();
    }

    #[tokio::test]
    async fn metrics_stream_endpoint_emits_snapshot_event() {
        let state = test_state_with_routes(
            vec![RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            }],
            None,
        );
        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .get(format!("{base_url}/v1/metrics/stream?interval_ms=200"))
            .send()
            .await
            .expect("metrics stream request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.starts_with("text/event-stream"));

        let mut stream = response.bytes_stream();
        let first_chunk = timeout(Duration::from_secs(3), stream.next())
            .await
            .expect("stream should emit within timeout")
            .expect("stream should produce chunk")
            .expect("chunk should decode");
        let body = String::from_utf8_lossy(&first_chunk);
        assert!(body.contains("event: snapshot"));
        assert!(body.contains("data: "));

        server_task.abort();
    }

    #[tokio::test]
    async fn config_reload_poll_applies_changed_routes() {
        let state = test_state_with_routes(
            vec![route("svc-a", Some("old.local"), Some("/"), None, true)],
            None,
        );

        save_config_file(
            &state.config_file,
            &DeclarativeConfigFile {
                routes: vec![RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-b".to_string(),
                    match_host: Some("new.local".to_string()),
                    match_path_prefix: Some("/new".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:4200".to_string(),
                    fallback_upstream_url: Some("http://127.0.0.1:4201".to_string()),
                    health_check_path: Some("/healthz".to_string()),
                    enabled: true,
                }],
                health_check: Some(HealthCheckSettings {
                    interval_ms: 9_000,
                    timeout_ms: 1_250,
                    path: "/healthz".to_string(),
                }),
            },
        )
        .await
        .expect("config fixture should persist");

        let changed = reload_config_file(&state, false)
            .await
            .expect("reload should succeed");
        assert!(changed);

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 1);
        assert_eq!(runtime.persisted.routes[0].id, "svc-b");
        drop(runtime);

        let settings = state.health_check_settings.read().await;
        assert_eq!(settings.interval_ms, 9_000);
        assert_eq!(settings.timeout_ms, 1_250);
        assert_eq!(settings.path, "/healthz");
        drop(settings);

        let status = state.config_reload_status.lock().await;
        assert!(status.last_config_reload_at.is_some());
        assert_eq!(status.last_config_reload_error, None);
    }

    #[tokio::test]
    async fn config_reload_poll_keeps_last_good_config_on_parse_error() {
        let state = test_state_with_routes(
            vec![route("svc-a", Some("old.local"), Some("/"), None, true)],
            None,
        );

        save_config_file(
            &state.config_file,
            &DeclarativeConfigFile {
                routes: vec![route(
                    "svc-good",
                    Some("good.local"),
                    Some("/good"),
                    None,
                    true,
                )],
                health_check: Some(HealthCheckSettings {
                    interval_ms: 8_000,
                    timeout_ms: 2_000,
                    path: "/good".to_string(),
                }),
            },
        )
        .await
        .expect("good config should persist");
        assert!(
            reload_config_file(&state, true)
                .await
                .expect("initial reload should succeed")
        );

        fs::write(&state.config_file, "{ invalid json")
            .await
            .expect("invalid config should write");

        let result = reload_config_file(&state, false).await;
        assert!(result.is_err());

        let runtime = state.runtime.lock().await;
        assert_eq!(runtime.persisted.routes.len(), 1);
        assert_eq!(runtime.persisted.routes[0].id, "svc-good");
        drop(runtime);

        let status = state.config_reload_status.lock().await;
        assert!(status.last_config_reload_error.is_some());
    }

    #[tokio::test]
    async fn diagnostics_endpoint_returns_local_runtime_context() {
        let state = test_state_with_routes(
            vec![
                route("svc-a", None, Some("/a"), None, true),
                route("svc-b", None, Some("/b"), None, false),
            ],
            None,
        );
        let expected_data_file = state.data_file.display().to_string();
        let expected_config_file = state.config_file.display().to_string();
        let expected_provider_log_file = state.provider_log_file.display().to_string();
        {
            let mut runtime = state.runtime.lock().await;
            runtime.persisted.tunnel = default_tunnel_status(TunnelState::Running);
            runtime.pending_restart = Some(PendingRestart {
                provider: TunnelProvider::Cloudflared,
                target_url: DEFAULT_GATEWAY_TARGET_URL.to_string(),
                metadata: None,
                auto_restart: true,
                restart_count: 2,
                started_at: "2026-03-05T00:00:00Z".to_string(),
                next_attempt_at: Instant::now() + Duration::from_secs(5),
                reason: "provider exited".to_string(),
            });
        }

        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();

        let response = client
            .get(format!("{base_url}/v1/diagnostics"))
            .send()
            .await
            .expect("diagnostics request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value = response
            .json()
            .await
            .expect("diagnostics response should decode");

        assert_eq!(payload["data_file"], expected_data_file);
        assert_eq!(payload["provider_log_file"], expected_provider_log_file);
        assert_eq!(payload["route_count"], 2);
        assert_eq!(payload["enabled_route_count"], 1);
        assert_eq!(payload["tunnel_state"], "running");
        assert_eq!(payload["pending_restart"], true);
        assert_eq!(payload["config_file"], expected_config_file);
        assert_eq!(payload["config_reload_enabled"], true);
        assert_eq!(payload["config_reload_interval_ms"], 1_000);
        assert_eq!(payload["last_config_reload_at"], serde_json::Value::Null);
        assert_eq!(payload["last_config_reload_error"], serde_json::Value::Null);

        server_task.abort();
    }

    #[tokio::test]
    async fn metrics_endpoint_returns_runtime_snapshot() {
        let state = test_state_with_routes(
            vec![
                RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-a".to_string(),
                    match_host: None,
                    match_path_prefix: Some("/a".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: true,
                },
                RouteRule {
                    tunnel_id: "primary".to_string(),
                    id: "svc-b".to_string(),
                    match_host: None,
                    match_path_prefix: Some("/b".to_string()),
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:3001".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: false,
                },
            ],
            None,
        );
        {
            let mut health_map = state.upstream_health.lock().await;
            health_map.insert(
                test_health_key("http://127.0.0.1:3000", "/"),
                test_health(true),
            );
        }
        {
            let mut settings = state.health_check_settings.write().await;
            settings.interval_ms = 1_000;
            settings.timeout_ms = 1_500;
            settings.path = "/ready".to_string();
        }

        let (base_url, server_task) = spawn_control_test_server(state).await;
        let client = ReqwestClient::new();
        let metrics: MetricsResponse = client
            .get(format!("{base_url}/v1/metrics"))
            .send()
            .await
            .expect("metrics request should complete")
            .json()
            .await
            .expect("metrics payload should decode");

        assert_eq!(metrics.route_count, 2);
        assert_eq!(metrics.enabled_route_count, 1);
        assert_eq!(metrics.upstream_health_entries, 1);
        assert_eq!(metrics.tunnel_state, TunnelState::Idle);
        assert!(!metrics.running_tunnel);
        assert!(!metrics.pending_restart);
        assert_eq!(
            metrics.health_check,
            HealthCheckSettings {
                interval_ms: 1_000,
                timeout_ms: 1_500,
                path: "/ready".to_string(),
            }
        );

        server_task.abort();
    }

    #[test]
    fn restart_backoff_grows_and_caps() {
        assert_eq!(restart_backoff(1), Duration::from_secs(1));
        assert_eq!(restart_backoff(2), Duration::from_secs(2));
        assert_eq!(restart_backoff(3), Duration::from_secs(4));
        assert_eq!(restart_backoff(4), Duration::from_secs(8));
        assert_eq!(restart_backoff(5), Duration::from_secs(16));
        assert_eq!(restart_backoff(6), Duration::from_secs(32));
        assert_eq!(restart_backoff(7), Duration::from_secs(32));
        assert_eq!(restart_backoff(0), Duration::from_secs(1));
    }

    #[test]
    fn exit_action_respects_auto_restart_and_limit() {
        assert_eq!(determine_exit_action(false, 0, 5), ExitAction::NoRestart,);
        assert_eq!(
            determine_exit_action(true, 0, 5),
            ExitAction::Restart {
                next_restart_count: 1,
                backoff: Duration::from_secs(1),
            },
        );
        assert_eq!(
            determine_exit_action(true, 4, 5),
            ExitAction::Restart {
                next_restart_count: 5,
                backoff: Duration::from_secs(16),
            },
        );
        assert_eq!(determine_exit_action(true, 5, 5), ExitAction::Exhausted,);
    }

    #[tokio::test]
    async fn reconcile_schedules_pending_restart_on_unexpected_exit() {
        let mut child = TokioCommand::new("sh")
            .arg("-c")
            .arg("exit 7")
            .spawn()
            .expect("spawned test child");
        let _ = child.wait().await.expect("waited test child");

        let mut runtime = RuntimeState {
            persisted: PersistedState::default(),
            running_tunnel: Some(RunningTunnel {
                child,
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:18080".to_string(),
                metadata: None,
                auto_restart: true,
                restart_count: 0,
                started_at: "2026-03-05T00:00:00Z".to_string(),
                public_base_url: Some("https://demo.trycloudflare.com".to_string()),
                process_id: None,
            }),
            pending_restart: None,
        };

        let changed = reconcile_runtime_tunnel_state(&mut runtime, 5).expect("reconcile succeeds");
        assert!(changed);
        assert!(runtime.running_tunnel.is_none());
        assert!(runtime.pending_restart.is_some());
        assert_eq!(runtime.persisted.tunnel.state, TunnelState::Starting);
        assert_eq!(runtime.persisted.tunnel.restart_count, 1);
    }

    #[tokio::test]
    async fn reconcile_marks_error_when_auto_restart_exhausted() {
        let mut child = TokioCommand::new("sh")
            .arg("-c")
            .arg("exit 9")
            .spawn()
            .expect("spawned test child");
        let _ = child.wait().await.expect("waited test child");

        let mut runtime = RuntimeState {
            persisted: PersistedState::default(),
            running_tunnel: Some(RunningTunnel {
                child,
                provider: TunnelProvider::Cloudflared,
                target_url: "http://127.0.0.1:18080".to_string(),
                metadata: None,
                auto_restart: true,
                restart_count: 2,
                started_at: "2026-03-05T00:00:00Z".to_string(),
                public_base_url: Some("https://demo.trycloudflare.com".to_string()),
                process_id: None,
            }),
            pending_restart: None,
        };

        let changed = reconcile_runtime_tunnel_state(&mut runtime, 2).expect("reconcile succeeds");
        assert!(changed);
        assert!(runtime.running_tunnel.is_none());
        assert!(runtime.pending_restart.is_none());
        assert_eq!(runtime.persisted.tunnel.state, TunnelState::Error);
    }

    #[test]
    fn normalize_log_tail_lines_applies_default_and_bounds() {
        assert_eq!(normalize_log_tail_lines(None).expect("default lines"), 200);
        assert_eq!(normalize_log_tail_lines(Some(1)).expect("one line"), 1);
        assert!(normalize_log_tail_lines(Some(0)).is_err());
        assert!(normalize_log_tail_lines(Some(5_001)).is_err());
    }

    #[test]
    fn tail_lines_returns_last_n_lines_in_order() {
        let source = "a\nb\nc\nd\n";
        assert_eq!(
            tail_lines(source, 2),
            vec!["c".to_string(), "d".to_string()]
        );
        assert_eq!(
            tail_lines(source, 10),
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]
        );
    }

    #[test]
    fn extract_bearer_token_parses_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer secret-token".parse().expect("valid header"),
        );
        assert_eq!(extract_bearer_token(&headers), Some("secret-token"));
    }

    #[test]
    fn extract_bearer_token_rejects_invalid_schemes() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Basic abc".parse().expect("valid header"),
        );
        assert_eq!(extract_bearer_token(&headers), None);
    }

    #[test]
    fn is_authorized_request_accepts_when_token_disabled_or_matches() {
        let mut headers = HeaderMap::new();
        assert!(is_authorized_request(&headers, None));

        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer expected".parse().expect("valid header"),
        );
        assert!(is_authorized_request(&headers, Some("expected")));
        assert!(!is_authorized_request(&headers, Some("other")));
    }

    #[test]
    fn normalize_log_stream_poll_ms_applies_bounds() {
        assert_eq!(normalize_log_stream_poll_ms(None).expect("default"), 1000);
        assert_eq!(normalize_log_stream_poll_ms(Some(100)).expect("min"), 100);
        assert_eq!(
            normalize_log_stream_poll_ms(Some(10_000)).expect("max"),
            10_000
        );
        assert!(normalize_log_stream_poll_ms(Some(99)).is_err());
        assert!(normalize_log_stream_poll_ms(Some(10_001)).is_err());
    }

    #[test]
    fn normalize_stream_interval_ms_applies_bounds() {
        assert_eq!(normalize_stream_interval_ms(None).expect("default"), 2_000);
        assert_eq!(normalize_stream_interval_ms(Some(200)).expect("min"), 200);
        assert_eq!(
            normalize_stream_interval_ms(Some(60_000)).expect("max"),
            60_000
        );
        assert!(normalize_stream_interval_ms(Some(199)).is_err());
        assert!(normalize_stream_interval_ms(Some(60_001)).is_err());
    }

    #[test]
    fn normalize_match_route_path_requires_leading_slash() {
        assert_eq!(
            normalize_match_route_path("/api/v1").expect("valid path"),
            "/api/v1"
        );
        assert!(normalize_match_route_path("").is_err());
        assert!(normalize_match_route_path("api/v1").is_err());
    }

    #[test]
    fn should_failover_status_only_for_server_errors() {
        assert!(should_failover_status(StatusCode::BAD_GATEWAY));
        assert!(should_failover_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(!should_failover_status(StatusCode::BAD_REQUEST));
        assert!(!should_failover_status(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn ordered_upstream_targets_prefers_primary_when_healthy() {
        let route = RouteRule {
            tunnel_id: "primary".to_string(),
            id: "svc".to_string(),
            match_host: None,
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
            health_check_path: None,
            enabled: true,
        };
        let mut health = HashMap::new();
        health.insert(test_health_key(&route.upstream_url, "/"), test_health(true));
        health.insert(
            test_health_key(
                route
                    .fallback_upstream_url
                    .as_deref()
                    .expect("fallback should exist"),
                "/",
            ),
            test_health(false),
        );

        let targets = ordered_upstream_targets(&route, "/", &health);
        assert_eq!(
            targets,
            vec![
                "http://127.0.0.1:3000".to_string(),
                "http://127.0.0.1:3001".to_string()
            ]
        );
    }

    #[test]
    fn ordered_upstream_targets_prefers_fallback_when_primary_unhealthy() {
        let route = RouteRule {
            tunnel_id: "primary".to_string(),
            id: "svc".to_string(),
            match_host: None,
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
            health_check_path: None,
            enabled: true,
        };
        let mut health = HashMap::new();
        health.insert(
            test_health_key(&route.upstream_url, "/"),
            test_health(false),
        );
        health.insert(
            test_health_key(
                route
                    .fallback_upstream_url
                    .as_deref()
                    .expect("fallback should exist"),
                "/",
            ),
            test_health(true),
        );

        let targets = ordered_upstream_targets(&route, "/", &health);
        assert_eq!(
            targets,
            vec![
                "http://127.0.0.1:3001".to_string(),
                "http://127.0.0.1:3000".to_string()
            ]
        );
    }

    #[test]
    fn ordered_upstream_targets_keeps_primary_first_when_health_unknown() {
        let route = RouteRule {
            tunnel_id: "primary".to_string(),
            id: "svc".to_string(),
            match_host: None,
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
            health_check_path: None,
            enabled: true,
        };
        let health = HashMap::new();

        let targets = ordered_upstream_targets(&route, "/", &health);
        assert_eq!(
            targets,
            vec![
                "http://127.0.0.1:3000".to_string(),
                "http://127.0.0.1:3001".to_string()
            ]
        );
    }

    #[test]
    fn ordered_upstream_targets_uses_route_health_check_path() {
        let route = RouteRule {
            tunnel_id: "primary".to_string(),
            id: "svc".to_string(),
            match_host: None,
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
            health_check_path: Some("/ready".to_string()),
            enabled: true,
        };
        let mut health = HashMap::new();
        health.insert(test_health_key(&route.upstream_url, "/"), test_health(true));
        health.insert(
            test_health_key(
                route
                    .fallback_upstream_url
                    .as_deref()
                    .expect("fallback should exist"),
                "/",
            ),
            test_health(false),
        );
        health.insert(
            test_health_key(&route.upstream_url, "/ready"),
            test_health(false),
        );
        health.insert(
            test_health_key(
                route
                    .fallback_upstream_url
                    .as_deref()
                    .expect("fallback should exist"),
                "/ready",
            ),
            test_health(true),
        );

        let targets = ordered_upstream_targets(&route, "/ready", &health);
        assert_eq!(
            targets,
            vec![
                "http://127.0.0.1:3001".to_string(),
                "http://127.0.0.1:3000".to_string()
            ]
        );
    }

    #[test]
    fn collect_upstream_health_entries_dedupes_and_sorts_urls() {
        let routes = vec![
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
                health_check_path: None,
                enabled: true,
            },
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3001".to_string(),
                fallback_upstream_url: Some("http://127.0.0.1:3002".to_string()),
                health_check_path: None,
                enabled: true,
            },
        ];
        let mut health = HashMap::new();
        health.insert(
            test_health_key("http://127.0.0.1:3000", "/"),
            UpstreamHealth {
                healthy: true,
                last_checked_at: "2026-03-05T00:00:00Z".to_string(),
                last_error: None,
            },
        );
        health.insert(
            test_health_key("http://127.0.0.1:3001", "/"),
            UpstreamHealth {
                healthy: false,
                last_checked_at: "2026-03-05T00:00:01Z".to_string(),
                last_error: Some("status 503".to_string()),
            },
        );

        let entries = collect_upstream_health_entries(&routes, "/", &health);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].upstream_url, "http://127.0.0.1:3000");
        assert_eq!(entries[0].health_check_path, "/");
        assert_eq!(entries[0].healthy, Some(true));
        assert_eq!(
            entries[0].last_checked_at.as_deref(),
            Some("2026-03-05T00:00:00Z")
        );
        assert_eq!(entries[1].upstream_url, "http://127.0.0.1:3001");
        assert_eq!(entries[1].health_check_path, "/");
        assert_eq!(entries[1].healthy, Some(false));
        assert_eq!(entries[1].last_error.as_deref(), Some("status 503"));
        assert_eq!(entries[2].upstream_url, "http://127.0.0.1:3002");
        assert_eq!(entries[2].health_check_path, "/");
        assert_eq!(entries[2].healthy, None);
        assert!(entries[2].last_checked_at.is_none());
        assert!(entries[2].last_error.is_none());
    }

    #[test]
    fn collect_upstream_health_entries_ignores_duplicate_urls() {
        let routes = vec![
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: Some("http://127.0.0.1:3000".to_string()),
                health_check_path: None,
                enabled: true,
            },
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        ];
        let health = HashMap::new();

        let entries = collect_upstream_health_entries(&routes, "/", &health);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].upstream_url, "http://127.0.0.1:3000");
        assert_eq!(entries[0].health_check_path, "/");
    }

    #[test]
    fn collect_upstream_health_entries_keeps_same_upstream_with_different_paths() {
        let routes = vec![
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: Some("/ready".to_string()),
                enabled: true,
            },
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        ];
        let health = HashMap::new();

        let entries = collect_upstream_health_entries(&routes, "/", &health);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].upstream_url, "http://127.0.0.1:3000");
        assert_eq!(entries[0].health_check_path, "/");
        assert_eq!(entries[1].upstream_url, "http://127.0.0.1:3000");
        assert_eq!(entries[1].health_check_path, "/ready");
    }

    #[test]
    fn normalize_route_request_accepts_fallback_upstream_url() {
        let route = normalize_route_request(CreateRouteRequest {
            tunnel_id: "primary".to_string(),
            id: "service-a".to_string(),
            match_host: Some("demo.local".to_string()),
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
            health_check_path: None,
            enabled: Some(true),
        })
        .expect("route should be accepted");
        assert_eq!(
            route.fallback_upstream_url.as_deref(),
            Some("http://127.0.0.1:3001")
        );
    }

    #[test]
    fn normalize_route_request_rejects_invalid_fallback_upstream_url() {
        let err = normalize_route_request(CreateRouteRequest {
            tunnel_id: "primary".to_string(),
            id: "service-a".to_string(),
            match_host: Some("demo.local".to_string()),
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("tcp://127.0.0.1:3001".to_string()),
            health_check_path: None,
            enabled: Some(true),
        })
        .expect_err("route should be rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn normalize_route_request_accepts_health_check_path() {
        let route = normalize_route_request(CreateRouteRequest {
            tunnel_id: "primary".to_string(),
            id: "service-a".to_string(),
            match_host: Some("demo.local".to_string()),
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: None,
            health_check_path: Some("healthz".to_string()),
            enabled: Some(true),
        })
        .expect("route should be accepted");
        assert_eq!(route.health_check_path.as_deref(), Some("/healthz"));
    }

    #[test]
    fn normalize_route_request_rejects_invalid_health_check_path() {
        let err = normalize_route_request(CreateRouteRequest {
            tunnel_id: "primary".to_string(),
            id: "service-a".to_string(),
            match_host: Some("demo.local".to_string()),
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: None,
            health_check_path: Some("/healthz?bad=1".to_string()),
            enabled: Some(true),
        })
        .expect_err("route should be rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn ensure_route_id_matches_rejects_mismatch() {
        let err = ensure_route_id_matches("svc-a", "svc-b").expect_err("should reject mismatch");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn replace_route_updates_existing_item() {
        let mut routes = vec![
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3001".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        ];

        let updated = replace_route(
            &mut routes,
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3010".to_string(),
                fallback_upstream_url: Some("http://127.0.0.1:3011".to_string()),
                health_check_path: Some("/healthz".to_string()),
                enabled: false,
            },
        );

        assert!(updated);
        assert_eq!(routes[1].upstream_url, "http://127.0.0.1:3010");
        assert_eq!(routes[1].match_host.as_deref(), Some("demo.local"));
        assert_eq!(routes[1].enabled, false);
    }

    #[test]
    fn replace_route_returns_false_when_missing() {
        let mut routes = vec![RouteRule {
            tunnel_id: "primary".to_string(),
            id: "svc-a".to_string(),
            match_host: None,
            match_path_prefix: Some("/a".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: None,
            health_check_path: None,
            enabled: true,
        }];

        let updated = replace_route(
            &mut routes,
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3001".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        );
        assert!(!updated);
    }

    #[test]
    fn ensure_unique_route_ids_rejects_duplicates() {
        let routes = vec![
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3001".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        ];
        assert!(ensure_unique_route_ids(&routes).is_err());
    }

    #[test]
    fn build_route_apply_plan_classifies_operations() {
        let existing = vec![
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: Some("/a".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3001".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        ];
        let incoming = vec![
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-b".to_string(),
                match_host: None,
                match_path_prefix: Some("/b".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3011".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: false,
            },
            RouteRule {
                tunnel_id: "primary".to_string(),
                id: "svc-c".to_string(),
                match_host: None,
                match_path_prefix: Some("/c".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3002".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: true,
            },
        ];

        let plan = build_route_apply_plan(&existing, &incoming, true);
        assert_eq!(plan.created, vec!["primary:svc-c".to_string()]);
        assert_eq!(plan.updated, vec!["primary:svc-b".to_string()]);
        assert_eq!(plan.unchanged, Vec::<String>::new());
        assert_eq!(plan.removed, vec!["primary:svc-a".to_string()]);
    }

    #[test]
    fn apply_route_rules_replaces_when_enabled() {
        let existing = vec![RouteRule {
            tunnel_id: "primary".to_string(),
            id: "svc-a".to_string(),
            match_host: None,
            match_path_prefix: Some("/a".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: None,
            health_check_path: None,
            enabled: true,
        }];
        let incoming = vec![RouteRule {
            tunnel_id: "primary".to_string(),
            id: "svc-b".to_string(),
            match_host: None,
            match_path_prefix: Some("/b".to_string()),
            strip_path_prefix: None,
            upstream_url: "http://127.0.0.1:3001".to_string(),
            fallback_upstream_url: None,
            health_check_path: None,
            enabled: true,
        }];

        let applied = apply_route_rules(&existing, incoming, true);
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].id, "svc-b");
    }

    #[test]
    fn normalize_health_check_path_enforces_leading_slash() {
        assert_eq!(
            normalize_health_check_path(" /healthz ").expect("valid path"),
            "/healthz"
        );
        assert_eq!(
            normalize_health_check_path("ready").expect("valid path"),
            "/ready"
        );
        assert_eq!(normalize_health_check_path("/").expect("root path"), "/");
    }

    #[test]
    fn normalize_health_check_path_rejects_empty() {
        assert!(normalize_health_check_path("   ").is_err());
    }

    #[test]
    fn normalize_health_check_interval_ms_applies_bounds() {
        assert_eq!(
            normalize_health_check_interval_ms(200).expect("min interval"),
            200
        );
        assert_eq!(
            normalize_health_check_interval_ms(60_000).expect("max interval"),
            60_000
        );
        assert!(normalize_health_check_interval_ms(199).is_err());
        assert!(normalize_health_check_interval_ms(60_001).is_err());
    }

    #[test]
    fn normalize_health_check_timeout_ms_applies_bounds() {
        assert_eq!(
            normalize_health_check_timeout_ms(100).expect("min timeout"),
            100
        );
        assert_eq!(
            normalize_health_check_timeout_ms(30_000).expect("max timeout"),
            30_000
        );
        assert!(normalize_health_check_timeout_ms(99).is_err());
        assert!(normalize_health_check_timeout_ms(30_001).is_err());
    }

    #[test]
    fn resolve_initial_health_check_settings_prefers_persisted_value() {
        let startup = HealthCheckSettings {
            interval_ms: 5_000,
            timeout_ms: 2_000,
            path: "/".to_string(),
        };
        let persisted = HealthCheckSettings {
            interval_ms: 1_000,
            timeout_ms: 1_500,
            path: "/ready".to_string(),
        };

        let resolved = resolve_initial_health_check_settings(startup, Some(persisted.clone()));
        assert_eq!(resolved, persisted);
    }

    #[test]
    fn resolve_initial_health_check_settings_falls_back_to_startup_default() {
        let startup = HealthCheckSettings {
            interval_ms: 5_000,
            timeout_ms: 2_000,
            path: "/".to_string(),
        };
        let resolved = resolve_initial_health_check_settings(startup.clone(), None);
        assert_eq!(resolved, startup);
    }

    #[test]
    fn apply_health_check_settings_update_applies_partial_overrides() {
        let current = HealthCheckSettings {
            interval_ms: 5_000,
            timeout_ms: 2_000,
            path: "/".to_string(),
        };
        let updated = apply_health_check_settings_update(
            &current,
            UpdateHealthCheckSettingsRequest {
                interval_ms: Some(1_000),
                timeout_ms: None,
                path: Some("ready".to_string()),
            },
        )
        .expect("update should succeed");

        assert_eq!(
            updated,
            HealthCheckSettings {
                interval_ms: 1_000,
                timeout_ms: 2_000,
                path: "/ready".to_string(),
            }
        );
    }

    #[test]
    fn apply_health_check_settings_update_rejects_invalid_values() {
        let current = HealthCheckSettings {
            interval_ms: 5_000,
            timeout_ms: 2_000,
            path: "/".to_string(),
        };
        let err = apply_health_check_settings_update(
            &current,
            UpdateHealthCheckSettingsRequest {
                interval_ms: Some(100),
                timeout_ms: None,
                path: None,
            },
        )
        .expect_err("interval should be rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);

        let err = apply_health_check_settings_update(
            &current,
            UpdateHealthCheckSettingsRequest {
                interval_ms: None,
                timeout_ms: Some(50),
                path: None,
            },
        )
        .expect_err("timeout should be rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);

        let err = apply_health_check_settings_update(
            &current,
            UpdateHealthCheckSettingsRequest {
                interval_ms: None,
                timeout_ms: None,
                path: Some("/ok?bad=1".to_string()),
            },
        )
        .expect_err("path should be rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn build_health_check_url_overrides_path_and_clears_query() {
        let url = build_health_check_url("http://127.0.0.1:3000/base?x=1", "/healthz")
            .expect("health check url");
        assert_eq!(url.as_str(), "http://127.0.0.1:3000/healthz");
    }
}
