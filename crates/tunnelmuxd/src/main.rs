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
    CreateRouteRequest, DEFAULT_CONTROL_ADDR, DEFAULT_GATEWAY_TARGET_URL, DeleteRouteResponse,
    ErrorResponse, HealthCheckSettings, HealthCheckSettingsResponse, HealthResponse, RouteRule,
    RoutesResponse, TunnelLogsResponse, TunnelProvider, TunnelStartRequest, TunnelState,
    TunnelStatus, TunnelStatusResponse, UpdateHealthCheckSettingsRequest, UpstreamHealthEntry,
    UpstreamsHealthResponse,
};
use url::Url;

#[derive(Debug, Parser)]
#[command(name = "tunnelmuxd", version, about = "TunnelMux daemon")]
struct Args {
    #[arg(long, default_value = DEFAULT_CONTROL_ADDR)]
    listen: String,

    #[arg(long)]
    data_file: Option<PathBuf>,

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
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            tunnel: default_tunnel_status(TunnelState::Idle),
            routes: Vec::new(),
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
    public_base_url: String,
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

#[derive(Debug)]
struct AppState {
    runtime: Mutex<RuntimeState>,
    upstream_health: Mutex<HashMap<UpstreamHealthKey, UpstreamHealth>>,
    health_check_settings: RwLock<HealthCheckSettings>,
    data_file: PathBuf,
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
    public_url: String,
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
    let provider_log_file = args
        .provider_log_file
        .unwrap_or_else(default_provider_log_file);
    let api_token = resolve_api_token(args.api_token);
    let health_check_settings = HealthCheckSettings {
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
    let persisted = load_persisted_state(&data_file).await?;
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

    let protected_control_app = Router::new()
        .route("/v1/tunnel/status", get(get_tunnel_status))
        .route("/v1/tunnel/logs", get(get_tunnel_logs))
        .route("/v1/tunnel/logs/stream", get(stream_tunnel_logs))
        .route("/v1/tunnel/start", post(start_tunnel))
        .route("/v1/tunnel/stop", post(stop_tunnel))
        .route(
            "/v1/settings/health-check",
            get(get_health_check_settings).put(update_health_check_settings),
        )
        .route("/v1/upstreams/health", get(get_upstreams_health))
        .route("/v1/routes", get(list_routes).post(add_route))
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

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "tunnelmuxd".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

fn resolve_api_token(arg_token: Option<String>) -> Option<String> {
    arg_token
        .or_else(|| std::env::var("TUNNELMUX_API_TOKEN").ok())
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

async fn control_auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    if is_authorized_request(request.headers(), state.api_token.as_deref()) {
        return next.run(request).await;
    }

    ApiError {
        status: StatusCode::UNAUTHORIZED,
        message: "unauthorized: missing or invalid bearer token".to_string(),
    }
    .into_response()
}

fn is_authorized_request(headers: &HeaderMap, expected_token: Option<&str>) -> bool {
    match expected_token {
        None => true,
        Some(token) => extract_bearer_token(headers)
            .map(|candidate| candidate == token)
            .unwrap_or(false),
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            if scheme.eq_ignore_ascii_case("bearer") {
                Some(token)
            } else {
                None
            }
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

async fn get_tunnel_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TunnelStatusResponse>, ApiError> {
    reconcile_runtime_and_maybe_restart(&state).await?;
    let maybe_snapshot = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.clone()
    };
    Ok(Json(TunnelStatusResponse {
        tunnel: maybe_snapshot.tunnel,
    }))
}

async fn get_health_check_settings(
    State(state): State<Arc<AppState>>,
) -> Json<HealthCheckSettingsResponse> {
    let settings = {
        let current = state.health_check_settings.read().await;
        current.clone()
    };
    Json(HealthCheckSettingsResponse {
        health_check: settings,
    })
}

async fn update_health_check_settings(
    State(state): State<Arc<AppState>>,
    Json(request): Json<UpdateHealthCheckSettingsRequest>,
) -> Result<Json<HealthCheckSettingsResponse>, ApiError> {
    let updated = {
        let mut current = state.health_check_settings.write().await;
        let next = apply_health_check_settings_update(&current, request)?;
        *current = next.clone();
        next
    };

    Ok(Json(HealthCheckSettingsResponse {
        health_check: updated,
    }))
}

async fn get_tunnel_logs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TunnelLogsQuery>,
) -> Result<Json<TunnelLogsResponse>, ApiError> {
    let lines = normalize_log_tail_lines(query.lines)?;
    let source = match fs::read_to_string(&state.provider_log_file).await {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(ApiError::internal(format!(
                "failed to read provider logs from {}: {err}",
                state.provider_log_file.display()
            )));
        }
    };

    Ok(Json(TunnelLogsResponse {
        lines: tail_lines(&source, lines),
    }))
}

async fn stream_tunnel_logs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TunnelLogsStreamQuery>,
) -> Result<Response, ApiError> {
    let lines = normalize_log_tail_lines(query.lines)?;
    let poll_ms = normalize_log_stream_poll_ms(query.poll_ms)?;
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(128);
    let log_file = state.provider_log_file.clone();

    tokio::spawn(async move {
        let mut last_offset = 0usize;
        if let Ok(source) = fs::read_to_string(&log_file).await {
            last_offset = source.len();
            for line in tail_lines(&source, lines) {
                if tx
                    .send(Ok(Event::default().event("line").data(line)))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }

        loop {
            match fs::read_to_string(&log_file).await {
                Ok(source) => {
                    if source.len() < last_offset {
                        last_offset = 0;
                    }
                    if source.len() > last_offset {
                        let chunk = &source[last_offset..];
                        for line in chunk.lines() {
                            if tx
                                .send(Ok(Event::default().event("line").data(line.to_string())))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                        last_offset = source.len();
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    last_offset = 0;
                }
                Err(err) => {
                    if tx
                        .send(Ok(Event::default()
                            .event("error")
                            .data(format!("failed reading provider logs: {err}"))))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }

            sleep(Duration::from_millis(poll_ms)).await;
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
        .into_response())
}

async fn start_tunnel(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<TunnelStartRequest>,
) -> Result<Json<TunnelStatusResponse>, ApiError> {
    validate_target_url(&request.target_url)?;
    request.target_url = request.target_url.trim().to_string();
    let request_metadata = request.metadata.clone();

    stop_running_process(&state)
        .await
        .map_err(|err| ApiError::internal(format!("failed to stop existing tunnel: {err}")))?;

    {
        let mut runtime = state.runtime.lock().await;
        runtime.persisted.tunnel = default_tunnel_status(TunnelState::Starting);
        runtime.persisted.tunnel.provider = Some(request.provider.clone());
        runtime.persisted.tunnel.target_url = Some(request.target_url.clone());
        runtime.persisted.tunnel.auto_restart = request.auto_restart.unwrap_or(true);
        runtime.pending_restart = None;
    }
    persist_from_runtime(&state).await?;

    let spawned = match spawn_provider_process(&state, &request).await {
        Ok(spawned) => spawned,
        Err(err) => {
            {
                let mut runtime = state.runtime.lock().await;
                runtime.persisted.tunnel = default_tunnel_status(TunnelState::Error);
                runtime.persisted.tunnel.provider = Some(request.provider);
                runtime.persisted.tunnel.target_url = Some(request.target_url);
                runtime.persisted.tunnel.last_error = Some(err.to_string());
            }
            persist_from_runtime(&state).await?;
            return Err(ApiError::internal(err.to_string()));
        }
    };

    let status = {
        let mut runtime = state.runtime.lock().await;
        let started_at = now_iso();
        let auto_restart = request.auto_restart.unwrap_or(true);
        let status = TunnelStatus {
            state: TunnelState::Running,
            provider: Some(request.provider.clone()),
            target_url: Some(request.target_url.clone()),
            public_base_url: Some(spawned.public_url.clone()),
            started_at: Some(started_at.clone()),
            updated_at: now_iso(),
            process_id: spawned.process_id,
            auto_restart,
            restart_count: 0,
            last_error: None,
        };
        runtime.running_tunnel = Some(RunningTunnel {
            child: spawned.child,
            provider: request.provider,
            target_url: request.target_url,
            metadata: request_metadata,
            auto_restart,
            restart_count: 0,
            started_at,
            public_base_url: spawned.public_url,
            process_id: spawned.process_id,
        });
        runtime.pending_restart = None;
        runtime.persisted.tunnel = status.clone();
        status
    };
    persist_from_runtime(&state).await?;

    Ok(Json(TunnelStatusResponse { tunnel: status }))
}

async fn stop_tunnel(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TunnelStatusResponse>, ApiError> {
    stop_running_process(&state)
        .await
        .map_err(|err| ApiError::internal(format!("failed to stop tunnel: {err}")))?;

    let status = {
        let mut runtime = state.runtime.lock().await;
        runtime.persisted.tunnel = default_tunnel_status(TunnelState::Stopped);
        runtime.pending_restart = None;
        runtime.persisted.tunnel.clone()
    };
    persist_from_runtime(&state).await?;

    Ok(Json(TunnelStatusResponse { tunnel: status }))
}

async fn list_routes(State(state): State<Arc<AppState>>) -> Json<RoutesResponse> {
    let routes = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.routes.clone()
    };
    Json(RoutesResponse { routes })
}

async fn get_upstreams_health(State(state): State<Arc<AppState>>) -> Json<UpstreamsHealthResponse> {
    let routes = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.routes.clone()
    };
    let health_map = {
        let upstream_health = state.upstream_health.lock().await;
        upstream_health.clone()
    };
    let default_health_check_path = {
        let settings = state.health_check_settings.read().await;
        settings.path.clone()
    };
    Json(UpstreamsHealthResponse {
        upstreams: collect_upstream_health_entries(
            &routes,
            &default_health_check_path,
            &health_map,
        ),
    })
}

async fn add_route(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateRouteRequest>,
) -> Result<(StatusCode, Json<RouteRule>), ApiError> {
    let route = normalize_route_request(request)?;

    {
        let mut runtime = state.runtime.lock().await;
        if runtime
            .persisted
            .routes
            .iter()
            .any(|item| item.id == route.id)
        {
            return Err(ApiError::conflict(format!(
                "route '{}' already exists",
                route.id
            )));
        }
        runtime.persisted.routes.push(route.clone());
    }

    persist_from_runtime(&state).await?;
    Ok((StatusCode::CREATED, Json(route)))
}

async fn update_route(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<CreateRouteRequest>,
) -> Result<Json<RouteRule>, ApiError> {
    let route = normalize_route_request(request)?;
    ensure_route_id_matches(&id, &route.id)?;

    let updated = {
        let mut runtime = state.runtime.lock().await;
        replace_route(&mut runtime.persisted.routes, route.clone())
    };
    if !updated {
        return Err(ApiError::not_found(format!("route '{}' not found", id)));
    }

    persist_from_runtime(&state).await?;
    Ok(Json(route))
}

async fn delete_route(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<DeleteRouteResponse>, ApiError> {
    let removed = {
        let mut runtime = state.runtime.lock().await;
        let before = runtime.persisted.routes.len();
        runtime.persisted.routes.retain(|item| item.id != id);
        before != runtime.persisted.routes.len()
    };

    if !removed {
        return Err(ApiError::not_found(format!("route '{}' not found", id)));
    }

    persist_from_runtime(&state).await?;
    Ok(Json(DeleteRouteResponse { removed: true }))
}

async fn proxy_request(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Result<Response, ApiError> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    let path = uri.path().to_string();
    let query = uri.query().map(|value| value.to_string());
    let host = extract_host_from_headers(&headers);

    let route = {
        let runtime = state.runtime.lock().await;
        select_route(&runtime.persisted.routes, host.as_deref(), &path).cloned()
    };

    let route = match route {
        Some(route) => route,
        None => {
            return Err(ApiError {
                status: StatusCode::NOT_FOUND,
                message: format!("no route matched host={host:?} path={path}"),
            });
        }
    };

    if is_websocket_upgrade_request(&method, &headers) {
        return proxy_websocket_request(&state, request, route, &path, query.as_deref()).await;
    }

    let body = to_bytes(request.into_body(), 16 * 1024 * 1024)
        .await
        .map_err(|err| ApiError::internal(format!("failed to read request body: {err}")))?;
    let default_health_check_path = {
        let settings = state.health_check_settings.read().await;
        settings.path.clone()
    };
    let route_health_check_path =
        effective_route_health_check_path(&route, &default_health_check_path);
    let targets = {
        let health_map = state.upstream_health.lock().await;
        ordered_upstream_targets(&route, &route_health_check_path, &health_map)
    };

    let mut last_response = None::<reqwest::Response>;
    let mut last_error = None::<ApiError>;
    for (index, target) in targets.iter().enumerate() {
        let has_more_target = index + 1 < targets.len();
        match send_http_upstream(
            &state,
            &route.id,
            &route,
            target,
            &method,
            &headers,
            &body,
            &path,
            query.as_deref(),
        )
        .await
        {
            Ok(response) => {
                if has_more_target && should_failover_status(response.status()) {
                    warn!(
                        "upstream returned {}, trying next upstream: route={}, upstream={}",
                        response.status(),
                        route.id,
                        target
                    );
                    last_response = Some(response);
                    continue;
                }

                return build_http_proxy_response(response).await;
            }
            Err(err) => {
                if has_more_target {
                    warn!(
                        "upstream request failed, trying next upstream: route={}, upstream={}, error={}",
                        route.id, target, err.message
                    );
                    last_error = Some(err);
                    continue;
                }

                if let Some(response) = last_response {
                    return build_http_proxy_response(response).await;
                }
                return Err(err);
            }
        }
    }

    if let Some(response) = last_response {
        return build_http_proxy_response(response).await;
    }
    if let Some(err) = last_error {
        return Err(err);
    }

    Err(ApiError::internal(format!(
        "no upstream available for route '{}'",
        route.id
    )))
}

fn extract_host_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .split(':')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        })
}

fn is_websocket_upgrade_request(method: &Method, headers: &HeaderMap) -> bool {
    if method != Method::GET {
        return false;
    }

    let has_connection_upgrade = headers
        .get("connection")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
        })
        .unwrap_or(false);

    let has_websocket_upgrade = headers
        .get("upgrade")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    has_connection_upgrade && has_websocket_upgrade
}

async fn proxy_websocket_request(
    state: &Arc<AppState>,
    mut request: Request,
    route: RouteRule,
    path: &str,
    query: Option<&str>,
) -> Result<Response, ApiError> {
    let method = request.method().clone();
    let version = request.version();
    let headers = request.headers().clone();

    let on_client_upgrade = hyper::upgrade::on(&mut request);
    let default_health_check_path = {
        let settings = state.health_check_settings.read().await;
        settings.path.clone()
    };
    let route_health_check_path =
        effective_route_health_check_path(&route, &default_health_check_path);
    let targets = {
        let health_map = state.upstream_health.lock().await;
        ordered_upstream_targets(&route, &route_health_check_path, &health_map)
    };

    let mut upstream_response = None;
    let mut last_request_error = None::<String>;
    for (index, target) in targets.iter().enumerate() {
        let upstream_uri = build_upstream_uri(target, &route, path, query)?;
        let mut upstream_builder = axum::http::Request::builder()
            .method(method.clone())
            .uri(upstream_uri)
            .version(version);
        if let Some(upstream_headers) = upstream_builder.headers_mut() {
            copy_headers_for_websocket_upstream(upstream_headers, &headers);
        }
        let upstream_request = upstream_builder.body(Body::empty()).map_err(|err| {
            ApiError::internal(format!("failed to build websocket upstream request: {err}"))
        })?;

        match state.ws_proxy_client.request(upstream_request).await {
            Ok(response) => {
                let status = response.status();
                if status == StatusCode::SWITCHING_PROTOCOLS {
                    upstream_response = Some(response);
                    break;
                }

                let has_more_target = index + 1 < targets.len();
                if has_more_target && should_failover_status(status) {
                    warn!(
                        "websocket handshake got {}, trying next upstream: route={}, upstream={}",
                        status, route.id, target
                    );
                    continue;
                }

                return build_ws_handshake_failure_response(response).await;
            }
            Err(err) => {
                let has_more_target = index + 1 < targets.len();
                if has_more_target {
                    warn!(
                        "websocket handshake failed, trying next upstream: route={}, upstream={}, error={err}",
                        route.id, target
                    );
                    last_request_error = Some(err.to_string());
                    continue;
                }

                return Err(ApiError::internal(format!(
                    "upstream websocket handshake failed for route '{}': {err}",
                    route.id
                )));
            }
        }
    }

    let mut upstream_response = upstream_response.ok_or_else(|| {
        ApiError::internal(format!(
            "upstream websocket handshake failed for route '{}': {}",
            route.id,
            last_request_error.unwrap_or_else(|| "no upstream available".to_string())
        ))
    })?;

    let upstream_status = upstream_response.status();
    let upstream_headers = upstream_response.headers().clone();
    let on_upstream_upgrade = hyper::upgrade::on(&mut upstream_response);

    let mut response_builder = Response::builder()
        .status(upstream_status)
        .version(Version::HTTP_11);
    if let Some(headers_map) = response_builder.headers_mut() {
        copy_headers_unfiltered(headers_map, &upstream_headers);
    }
    let client_response = response_builder.body(Body::empty()).map_err(|err| {
        ApiError::internal(format!("failed to build websocket upgrade response: {err}"))
    })?;

    tokio::spawn(async move {
        let client_upgraded = match on_client_upgrade.await {
            Ok(stream) => stream,
            Err(err) => {
                warn!("client upgrade failed: {err}");
                return;
            }
        };

        let upstream_upgraded = match on_upstream_upgrade.await {
            Ok(stream) => stream,
            Err(err) => {
                warn!("upstream upgrade failed: {err}");
                return;
            }
        };

        let mut client_io = TokioIo::new(client_upgraded);
        let mut upstream_io = TokioIo::new(upstream_upgraded);
        if let Err(err) = tokio::io::copy_bidirectional(&mut client_io, &mut upstream_io).await {
            debug!("websocket proxy stream closed with error: {err}");
        }
    });

    Ok(client_response)
}

async fn send_http_upstream(
    state: &Arc<AppState>,
    route_id: &str,
    route: &RouteRule,
    upstream_base_url: &str,
    method: &Method,
    headers: &HeaderMap,
    body: &axum::body::Bytes,
    path: &str,
    query: Option<&str>,
) -> Result<reqwest::Response, ApiError> {
    let upstream_url = build_upstream_url(upstream_base_url, route, path, query)?;
    let mut upstream_request = state.proxy_client.request(method.clone(), upstream_url);
    upstream_request = copy_headers_to_upstream(upstream_request, headers);
    upstream_request = upstream_request.body(body.clone());

    upstream_request.send().await.map_err(|err| {
        ApiError::internal(format!("upstream request failed for '{}': {err}", route_id))
    })
}

async fn build_http_proxy_response(
    upstream_response: reqwest::Response,
) -> Result<Response, ApiError> {
    let status = upstream_response.status();
    let upstream_headers = upstream_response.headers().clone();
    let upstream_body = upstream_response.bytes().await.map_err(|err| {
        ApiError::internal(format!("failed reading upstream response body: {err}"))
    })?;

    let mut response_builder = Response::builder().status(status);
    if let Some(headers_map) = response_builder.headers_mut() {
        copy_headers_from_upstream(headers_map, &upstream_headers);
    }
    response_builder
        .body(Body::from(upstream_body))
        .map_err(|err| ApiError::internal(format!("failed to build proxy response: {err}")))
}

async fn build_ws_handshake_failure_response(
    upstream_response: hyper::Response<hyper::body::Incoming>,
) -> Result<Response, ApiError> {
    let status = upstream_response.status();
    let upstream_headers = upstream_response.headers().clone();
    let upstream_body = upstream_response
        .into_body()
        .collect()
        .await
        .map_err(|err| {
            ApiError::internal(format!("failed reading websocket handshake body: {err}"))
        })?
        .to_bytes();
    let mut response_builder = Response::builder().status(status);
    if let Some(headers_map) = response_builder.headers_mut() {
        for (name, value) in &upstream_headers {
            if is_hop_by_hop_header(name) {
                continue;
            }
            headers_map.insert(name, value.clone());
        }
    }
    response_builder
        .body(Body::from(upstream_body))
        .map_err(|err| {
            ApiError::internal(format!("failed to build handshake failure response: {err}"))
        })
}

fn build_upstream_url(
    upstream_base_url: &str,
    route: &RouteRule,
    path: &str,
    query: Option<&str>,
) -> Result<Url, ApiError> {
    let mut base = Url::parse(upstream_base_url)
        .map_err(|_| ApiError::internal(format!("invalid upstream URL in route '{}'", route.id)))?;
    let forwarded_path = rewrite_path(path, route);
    let joined_path = join_upstream_path(base.path(), &forwarded_path);
    base.set_path(&joined_path);
    base.set_query(query);
    Ok(base)
}

fn build_upstream_uri(
    upstream_base_url: &str,
    route: &RouteRule,
    path: &str,
    query: Option<&str>,
) -> Result<Uri, ApiError> {
    let upstream_url = build_upstream_url(upstream_base_url, route, path, query)?;
    upstream_url.as_str().parse::<Uri>().map_err(|err| {
        ApiError::internal(format!(
            "failed to convert upstream URL to URI for route '{}': {err}",
            route.id
        ))
    })
}

fn should_failover_status(status: StatusCode) -> bool {
    status.is_server_error()
}

fn effective_route_health_check_path(route: &RouteRule, default_health_check_path: &str) -> String {
    route
        .health_check_path
        .clone()
        .unwrap_or_else(|| default_health_check_path.to_string())
}

fn upstream_health_key(upstream_url: &str, health_check_path: &str) -> UpstreamHealthKey {
    UpstreamHealthKey {
        upstream_url: upstream_url.to_string(),
        health_check_path: health_check_path.to_string(),
    }
}

fn ordered_upstream_targets(
    route: &RouteRule,
    route_health_check_path: &str,
    health_map: &HashMap<UpstreamHealthKey, UpstreamHealth>,
) -> Vec<String> {
    let primary = route.upstream_url.clone();
    let fallback = route
        .fallback_upstream_url
        .as_deref()
        .filter(|value| *value != route.upstream_url)
        .map(ToString::to_string);

    let Some(fallback) = fallback else {
        return vec![primary];
    };

    let primary_health = health_map
        .get(&upstream_health_key(&primary, route_health_check_path))
        .map(|item| item.healthy);
    let fallback_health = health_map
        .get(&upstream_health_key(&fallback, route_health_check_path))
        .map(|item| item.healthy);
    if matches!(primary_health, Some(false)) && matches!(fallback_health, Some(true)) {
        return vec![fallback, primary];
    }

    vec![primary, fallback]
}

fn collect_upstream_health_entries(
    routes: &[RouteRule],
    default_health_check_path: &str,
    health_map: &HashMap<UpstreamHealthKey, UpstreamHealth>,
) -> Vec<UpstreamHealthEntry> {
    let mut upstreams = HashSet::new();
    for route in routes {
        let route_health_check_path =
            effective_route_health_check_path(route, default_health_check_path);
        upstreams.insert(upstream_health_key(
            &route.upstream_url,
            &route_health_check_path,
        ));
        if let Some(fallback) = route.fallback_upstream_url.as_ref() {
            upstreams.insert(upstream_health_key(fallback, &route_health_check_path));
        }
    }

    let mut upstream_keys = upstreams.into_iter().collect::<Vec<_>>();
    upstream_keys.sort_by(|left, right| {
        left.upstream_url
            .cmp(&right.upstream_url)
            .then_with(|| left.health_check_path.cmp(&right.health_check_path))
    });

    upstream_keys
        .into_iter()
        .map(|key| match health_map.get(&key) {
            Some(health) => UpstreamHealthEntry {
                upstream_url: key.upstream_url,
                health_check_path: key.health_check_path,
                healthy: Some(health.healthy),
                last_checked_at: Some(health.last_checked_at.clone()),
                last_error: health.last_error.clone(),
            },
            None => UpstreamHealthEntry {
                upstream_url: key.upstream_url,
                health_check_path: key.health_check_path,
                healthy: None,
                last_checked_at: None,
                last_error: None,
            },
        })
        .collect()
}

fn rewrite_path(path: &str, route: &RouteRule) -> String {
    let mut rewritten = path.to_string();
    if let Some(prefix) = route.strip_path_prefix.as_deref() {
        if rewritten == prefix {
            rewritten = "/".to_string();
        } else if rewritten.starts_with(prefix) {
            let rest = &rewritten[prefix.len()..];
            rewritten = if rest.starts_with('/') {
                rest.to_string()
            } else {
                format!("/{rest}")
            };
        }
    }

    if rewritten.is_empty() || !rewritten.starts_with('/') {
        return format!("/{rewritten}");
    }
    rewritten
}

fn join_upstream_path(base_path: &str, forwarded_path: &str) -> String {
    if forwarded_path == "/" {
        if base_path.is_empty() {
            return "/".to_string();
        }
        return base_path.to_string();
    }

    let mut base = base_path.to_string();
    if base.is_empty() {
        base.push('/');
    }

    if base.ends_with('/') {
        base.pop();
    }

    if base.is_empty() {
        return forwarded_path.to_string();
    }
    format!("{base}{forwarded_path}")
}

fn select_route<'a>(
    routes: &'a [RouteRule],
    host: Option<&str>,
    path: &str,
) -> Option<&'a RouteRule> {
    let host_lc = host.map(|item| item.to_ascii_lowercase());
    routes
        .iter()
        .filter(|route| route.enabled)
        .filter(|route| match route.match_host.as_deref() {
            Some(route_host) => host_lc
                .as_deref()
                .map(|value| value.eq_ignore_ascii_case(route_host))
                .unwrap_or(false),
            None => true,
        })
        .filter(|route| match route.match_path_prefix.as_deref() {
            Some(prefix) => path.starts_with(prefix),
            None => true,
        })
        .max_by_key(|route| {
            let host_weight = if route.match_host.is_some() { 2 } else { 0 };
            let path_weight = route
                .match_path_prefix
                .as_ref()
                .map(|value| value.len())
                .unwrap_or(0);
            (host_weight, path_weight)
        })
}

fn copy_headers_to_upstream(
    mut builder: reqwest::RequestBuilder,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        if is_hop_by_hop_header(name) || name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder
}

fn copy_headers_for_websocket_upstream(target: &mut HeaderMap, source: &HeaderMap) {
    for (name, value) in source {
        if name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        target.insert(name, value.clone());
    }
}

fn copy_headers_from_upstream(target: &mut HeaderMap, headers: &reqwest::header::HeaderMap) {
    for (name, value) in headers {
        if is_hop_by_hop_header(name) {
            continue;
        }
        target.insert(name, value.clone());
    }
}

fn copy_headers_unfiltered(target: &mut HeaderMap, headers: &HeaderMap) {
    for (name, value) in headers {
        target.insert(name, value.clone());
    }
}

fn is_hop_by_hop_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
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

    Ok(RouteRule {
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
    if let Some(index) = routes.iter().position(|item| item.id == route.id) {
        routes[index] = route;
        return true;
    }
    false
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

async fn persist_from_runtime(state: &Arc<AppState>) -> Result<(), ApiError> {
    let snapshot = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.clone()
    };

    save_state_file(&state.data_file, &snapshot)
        .await
        .map_err(|err| ApiError::internal(format!("failed to persist state: {err}")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExitAction {
    NoRestart,
    Restart {
        next_restart_count: u32,
        backoff: Duration,
    },
    Exhausted,
}

fn determine_exit_action(
    auto_restart: bool,
    restart_count: u32,
    max_auto_restarts: u32,
) -> ExitAction {
    if !auto_restart {
        return ExitAction::NoRestart;
    }

    if max_auto_restarts == 0 || restart_count >= max_auto_restarts {
        return ExitAction::Exhausted;
    }

    let next_restart_count = restart_count.saturating_add(1);
    ExitAction::Restart {
        next_restart_count,
        backoff: restart_backoff(next_restart_count),
    }
}

fn restart_backoff(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    Duration::from_secs(1_u64 << exponent)
}

async fn stop_running_process(state: &Arc<AppState>) -> anyhow::Result<bool> {
    let (running, pending_cleared) = {
        let mut runtime = state.runtime.lock().await;
        (
            runtime.running_tunnel.take(),
            runtime.pending_restart.take().is_some(),
        )
    };

    if let Some(mut running) = running {
        terminate_child(&mut running.child).await?;
        return Ok(true);
    }

    Ok(pending_cleared)
}

async fn monitor_runtime_state(state: Arc<AppState>) {
    loop {
        if let Err(err) = reconcile_runtime_and_maybe_restart(&state).await {
            warn!("runtime reconcile failed: {}", err.message);
        }
        sleep(Duration::from_secs(1)).await;
    }
}

async fn monitor_upstream_health(state: Arc<AppState>) {
    loop {
        let settings = {
            let current = state.health_check_settings.read().await;
            current.clone()
        };

        if let Err(err) = refresh_upstream_health(&state, &settings).await {
            warn!("upstream health check failed: {err}");
        }
        sleep(Duration::from_millis(settings.interval_ms)).await;
    }
}

async fn refresh_upstream_health(
    state: &Arc<AppState>,
    settings: &HealthCheckSettings,
) -> anyhow::Result<()> {
    let routes = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.routes.clone()
    };

    let mut upstreams = HashSet::new();
    for route in &routes {
        let route_health_check_path = effective_route_health_check_path(route, &settings.path);
        upstreams.insert(upstream_health_key(
            &route.upstream_url,
            &route_health_check_path,
        ));
        if let Some(fallback) = route.fallback_upstream_url.as_ref() {
            upstreams.insert(upstream_health_key(fallback, &route_health_check_path));
        }
    }

    let mut latest = HashMap::new();
    for upstream_key in upstreams {
        let checked_at = now_iso();
        let check_url = match build_health_check_url(
            &upstream_key.upstream_url,
            &upstream_key.health_check_path,
        ) {
            Ok(url) => url,
            Err(err) => {
                latest.insert(
                    upstream_key,
                    UpstreamHealth {
                        healthy: false,
                        last_checked_at: checked_at,
                        last_error: Some(err.to_string()),
                    },
                );
                continue;
            }
        };
        let check_result = state
            .proxy_client
            .get(check_url)
            .timeout(Duration::from_millis(settings.timeout_ms))
            .send()
            .await;

        let health = match check_result {
            Ok(response) if response.status().is_success() => UpstreamHealth {
                healthy: true,
                last_checked_at: checked_at.clone(),
                last_error: None,
            },
            Ok(response) => UpstreamHealth {
                healthy: false,
                last_checked_at: checked_at.clone(),
                last_error: Some(format!("status {}", response.status())),
            },
            Err(err) => UpstreamHealth {
                healthy: false,
                last_checked_at: checked_at,
                last_error: Some(err.to_string()),
            },
        };
        latest.insert(upstream_key, health);
    }

    let mut health_map = state.upstream_health.lock().await;
    *health_map = latest;
    Ok(())
}

async fn reconcile_runtime_and_maybe_restart(state: &Arc<AppState>) -> Result<(), ApiError> {
    let mut changed = {
        let mut runtime = state.runtime.lock().await;
        reconcile_runtime_tunnel_state(&mut runtime, state.max_auto_restarts)?
    };

    changed |= process_pending_restart(state).await?;

    if changed {
        persist_from_runtime(state).await?;
    }
    Ok(())
}

fn reconcile_runtime_tunnel_state(
    runtime: &mut RuntimeState,
    max_auto_restarts: u32,
) -> Result<bool, ApiError> {
    if let Some(running) = runtime.running_tunnel.as_mut() {
        match running.child.try_wait() {
            Ok(Some(status)) => {
                let provider = running.provider.clone();
                let target_url = running.target_url.clone();
                let metadata = running.metadata.clone();
                let public_base_url = running.public_base_url.clone();
                let started_at = running.started_at.clone();
                let auto_restart = running.auto_restart;
                let restart_count = running.restart_count;
                let exit_reason =
                    format!("provider process exited unexpectedly with status: {status}");
                warn!(
                    "provider process exited unexpectedly: provider={:?}, status={status}",
                    provider
                );
                runtime.running_tunnel = None;
                match determine_exit_action(auto_restart, restart_count, max_auto_restarts) {
                    ExitAction::NoRestart => {
                        runtime.pending_restart = None;
                        runtime.persisted.tunnel = default_tunnel_status(TunnelState::Stopped);
                        runtime.persisted.tunnel.provider = Some(provider);
                        runtime.persisted.tunnel.target_url = Some(target_url);
                        runtime.persisted.tunnel.public_base_url = Some(public_base_url);
                        runtime.persisted.tunnel.started_at = Some(started_at);
                        runtime.persisted.tunnel.auto_restart = auto_restart;
                        runtime.persisted.tunnel.restart_count = restart_count;
                        runtime.persisted.tunnel.last_error = Some(exit_reason);
                    }
                    ExitAction::Restart {
                        next_restart_count,
                        backoff,
                    } => {
                        runtime.pending_restart = Some(PendingRestart {
                            provider: provider.clone(),
                            target_url: target_url.clone(),
                            metadata,
                            auto_restart,
                            restart_count: next_restart_count,
                            started_at: started_at.clone(),
                            next_attempt_at: Instant::now() + backoff,
                            reason: exit_reason.clone(),
                        });
                        runtime.persisted.tunnel = TunnelStatus {
                            state: TunnelState::Starting,
                            provider: Some(provider),
                            target_url: Some(target_url),
                            public_base_url: None,
                            started_at: Some(started_at),
                            updated_at: now_iso(),
                            process_id: None,
                            auto_restart,
                            restart_count: next_restart_count,
                            last_error: Some(format!(
                                "{exit_reason}; scheduling auto restart attempt {} in {}s",
                                next_restart_count,
                                backoff.as_secs()
                            )),
                        };
                    }
                    ExitAction::Exhausted => {
                        runtime.pending_restart = None;
                        runtime.persisted.tunnel = default_tunnel_status(TunnelState::Error);
                        runtime.persisted.tunnel.provider = Some(provider);
                        runtime.persisted.tunnel.target_url = Some(target_url);
                        runtime.persisted.tunnel.public_base_url = Some(public_base_url);
                        runtime.persisted.tunnel.started_at = Some(started_at);
                        runtime.persisted.tunnel.auto_restart = auto_restart;
                        runtime.persisted.tunnel.restart_count = restart_count;
                        runtime.persisted.tunnel.last_error = Some(format!(
                            "{exit_reason}; auto restart limit reached ({max_auto_restarts})"
                        ));
                    }
                }
                return Ok(true);
            }
            Ok(None) => {
                let should_update = runtime.persisted.tunnel.state != TunnelState::Running
                    || runtime.persisted.tunnel.provider != Some(running.provider.clone())
                    || runtime.persisted.tunnel.target_url != Some(running.target_url.clone())
                    || runtime.persisted.tunnel.public_base_url
                        != Some(running.public_base_url.clone())
                    || runtime.persisted.tunnel.started_at != Some(running.started_at.clone())
                    || runtime.persisted.tunnel.process_id != running.process_id
                    || runtime.persisted.tunnel.auto_restart != running.auto_restart
                    || runtime.persisted.tunnel.restart_count != running.restart_count
                    || runtime.persisted.tunnel.last_error.is_some();
                if should_update {
                    runtime.persisted.tunnel = TunnelStatus {
                        state: TunnelState::Running,
                        provider: Some(running.provider.clone()),
                        target_url: Some(running.target_url.clone()),
                        public_base_url: Some(running.public_base_url.clone()),
                        started_at: Some(running.started_at.clone()),
                        updated_at: now_iso(),
                        process_id: running.process_id,
                        auto_restart: running.auto_restart,
                        restart_count: running.restart_count,
                        last_error: None,
                    };
                    return Ok(true);
                }
            }
            Err(err) => {
                let provider = running.provider.clone();
                let target_url = running.target_url.clone();
                runtime.running_tunnel = None;
                runtime.pending_restart = None;
                runtime.persisted.tunnel = default_tunnel_status(TunnelState::Error);
                runtime.persisted.tunnel.provider = Some(provider);
                runtime.persisted.tunnel.target_url = Some(target_url);
                runtime.persisted.tunnel.last_error =
                    Some(format!("failed to inspect provider process state: {err}"));
                return Ok(true);
            }
        }
    }

    Ok(false)
}

async fn process_pending_restart(state: &Arc<AppState>) -> Result<bool, ApiError> {
    let maybe_pending = {
        let runtime = state.runtime.lock().await;
        runtime.pending_restart.clone()
    };

    let Some(pending) = maybe_pending else {
        return Ok(false);
    };

    if Instant::now() < pending.next_attempt_at {
        return Ok(false);
    }

    {
        let mut runtime = state.runtime.lock().await;
        runtime.pending_restart = None;
        runtime.persisted.tunnel.state = TunnelState::Starting;
        runtime.persisted.tunnel.updated_at = now_iso();
    }

    let request = TunnelStartRequest {
        provider: pending.provider.clone(),
        target_url: pending.target_url.clone(),
        auto_restart: Some(pending.auto_restart),
        metadata: pending.metadata.clone(),
    };
    let attempt_no = pending.restart_count;

    match spawn_provider_process(state, &request).await {
        Ok(spawned) => {
            let status = TunnelStatus {
                state: TunnelState::Running,
                provider: Some(pending.provider.clone()),
                target_url: Some(pending.target_url.clone()),
                public_base_url: Some(spawned.public_url.clone()),
                started_at: Some(pending.started_at.clone()),
                updated_at: now_iso(),
                process_id: spawned.process_id,
                auto_restart: pending.auto_restart,
                restart_count: pending.restart_count,
                last_error: None,
            };
            let mut runtime = state.runtime.lock().await;
            runtime.running_tunnel = Some(RunningTunnel {
                child: spawned.child,
                provider: pending.provider,
                target_url: pending.target_url,
                metadata: pending.metadata,
                auto_restart: pending.auto_restart,
                restart_count: pending.restart_count,
                started_at: pending.started_at,
                public_base_url: spawned.public_url,
                process_id: spawned.process_id,
            });
            runtime.pending_restart = None;
            runtime.persisted.tunnel = status;
            Ok(true)
        }
        Err(err) => {
            let action = determine_exit_action(
                pending.auto_restart,
                pending.restart_count,
                state.max_auto_restarts,
            );
            let mut runtime = state.runtime.lock().await;
            match action {
                ExitAction::Restart {
                    next_restart_count,
                    backoff,
                } => {
                    runtime.pending_restart = Some(PendingRestart {
                        provider: pending.provider.clone(),
                        target_url: pending.target_url.clone(),
                        metadata: pending.metadata.clone(),
                        auto_restart: pending.auto_restart,
                        restart_count: next_restart_count,
                        started_at: pending.started_at.clone(),
                        next_attempt_at: Instant::now() + backoff,
                        reason: pending.reason.clone(),
                    });
                    runtime.persisted.tunnel = TunnelStatus {
                        state: TunnelState::Starting,
                        provider: Some(pending.provider),
                        target_url: Some(pending.target_url),
                        public_base_url: None,
                        started_at: Some(pending.started_at),
                        updated_at: now_iso(),
                        process_id: None,
                        auto_restart: pending.auto_restart,
                        restart_count: next_restart_count,
                        last_error: Some(format!(
                            "auto restart attempt {} failed: {err}; retrying in {}s",
                            attempt_no,
                            backoff.as_secs()
                        )),
                    };
                }
                ExitAction::NoRestart | ExitAction::Exhausted => {
                    runtime.pending_restart = None;
                    runtime.persisted.tunnel = default_tunnel_status(TunnelState::Error);
                    runtime.persisted.tunnel.provider = Some(pending.provider);
                    runtime.persisted.tunnel.target_url = Some(pending.target_url);
                    runtime.persisted.tunnel.started_at = Some(pending.started_at);
                    runtime.persisted.tunnel.auto_restart = pending.auto_restart;
                    runtime.persisted.tunnel.restart_count = pending.restart_count;
                    runtime.persisted.tunnel.last_error = Some(format!(
                        "auto restart attempt {} failed and no more retries are available: {err}",
                        attempt_no
                    ));
                }
            }
            Ok(true)
        }
    }
}

async fn spawn_provider_process(
    state: &Arc<AppState>,
    request: &TunnelStartRequest,
) -> anyhow::Result<SpawnedTunnel> {
    let mut command = match request.provider {
        TunnelProvider::Cloudflared => {
            let mut cmd = Command::new(&state.cloudflared_bin);
            cmd.args([
                "tunnel",
                "--no-autoupdate",
                "--url",
                request.target_url.as_str(),
            ]);
            cmd
        }
        TunnelProvider::Ngrok => {
            let mut cmd = Command::new(&state.ngrok_bin);
            cmd.args([
                "http",
                request.target_url.as_str(),
                "--log",
                "stdout",
                "--log-format",
                "json",
            ]);

            if let Some(metadata) = request.metadata.as_ref() {
                if let Some(domain) = metadata
                    .get("ngrokDomain")
                    .or_else(|| metadata.get("domain"))
                    .map(|item| item.trim())
                    .filter(|item| !item.is_empty())
                {
                    cmd.arg("--domain").arg(domain);
                }

                if let Some(authtoken) = metadata
                    .get("ngrokAuthtoken")
                    .or_else(|| metadata.get("authtoken"))
                    .map(|item| item.trim())
                    .filter(|item| !item.is_empty())
                {
                    cmd.env("NGROK_AUTHTOKEN", authtoken);
                }
            }

            cmd
        }
    };

    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn provider command: {:?}", request.provider))?;
    let process_id = child.id();
    let public_url = wait_for_public_url(
        &mut child,
        request.provider.clone(),
        Duration::from_millis(state.ready_timeout_ms),
        state.provider_log_file.clone(),
    )
    .await
    .inspect_err(|err| warn!("provider startup failed: {err}"))?;

    Ok(SpawnedTunnel {
        child,
        public_url,
        process_id,
    })
}

async fn wait_for_public_url(
    child: &mut Child,
    provider: TunnelProvider,
    timeout_duration: Duration,
    provider_log_file: PathBuf,
) -> anyhow::Result<String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to capture provider stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("failed to capture provider stderr"))?;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(pipe_reader_to_channel(
        stdout,
        tx.clone(),
        provider.clone(),
        "stdout",
        provider_log_file.clone(),
    ));
    tokio::spawn(pipe_reader_to_channel(
        stderr,
        tx,
        provider.clone(),
        "stderr",
        provider_log_file,
    ));

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(anyhow!(
                "provider exited before publishing public URL: {status}"
            ));
        }

        let elapsed = start.elapsed();
        if elapsed >= timeout_duration {
            let _ = terminate_child(child).await;
            return Err(anyhow!(
                "provider did not report public URL within {} ms",
                timeout_duration.as_millis()
            ));
        }

        let remaining = timeout_duration.saturating_sub(elapsed);
        match timeout(remaining, rx.recv()).await {
            Ok(Some(line)) => {
                if let Some(url) = extract_public_url(&provider, &line) {
                    return Ok(url);
                }
            }
            Ok(None) => {
                let _ = terminate_child(child).await;
                return Err(anyhow!(
                    "provider log stream closed before URL was discovered"
                ));
            }
            Err(_) => {
                let _ = terminate_child(child).await;
                return Err(anyhow!(
                    "provider did not report public URL within {} ms",
                    timeout_duration.as_millis()
                ));
            }
        }
    }
}

async fn pipe_reader_to_channel<R>(
    reader: R,
    tx: mpsc::UnboundedSender<String>,
    provider: TunnelProvider,
    stream_name: &'static str,
    provider_log_file: PathBuf,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut lines = BufReader::new(reader).lines();
    let mut log_file = match open_provider_log_file(&provider_log_file).await {
        Ok(file) => Some(file),
        Err(err) => {
            warn!(
                "failed to open provider log file {}: {err}",
                provider_log_file.display()
            );
            None
        }
    };

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if let Some(file) = log_file.as_mut() {
                    let formatted = format_provider_log_line(&provider, stream_name, &line);
                    if let Err(err) = file.write_all(formatted.as_bytes()).await {
                        warn!("failed to write provider logs: {err}");
                        log_file = None;
                    }
                }
                if tx.send(line.clone()).is_err() {
                    debug!(line = line, "provider-log");
                }
            }
            Ok(None) => break,
            Err(err) => {
                debug!("failed to read provider output: {err}");
                break;
            }
        }
    }
}

async fn open_provider_log_file(path: &Path) -> anyhow::Result<fs::File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create provider log dir: {}", parent.display()))?;
    }

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("failed to open provider log file: {}", path.display()))
}

fn format_provider_log_line(provider: &TunnelProvider, stream_name: &str, line: &str) -> String {
    let provider_name = match provider {
        TunnelProvider::Cloudflared => "cloudflared",
        TunnelProvider::Ngrok => "ngrok",
    };
    format!(
        "{} [{}:{}] {}\n",
        now_iso(),
        provider_name,
        stream_name,
        line
    )
}

fn extract_public_url(provider: &TunnelProvider, line: &str) -> Option<String> {
    match provider {
        TunnelProvider::Cloudflared => cloudflared_url_regex()
            .find(line)
            .map(|matched| matched.as_str().to_string()),
        TunnelProvider::Ngrok => extract_ngrok_url(line),
    }
}

fn extract_ngrok_url(line: &str) -> Option<String> {
    if let Ok(parsed) = serde_json::from_str::<HashMap<String, serde_json::Value>>(line) {
        if let Some(url) = parsed
            .get("url")
            .and_then(|value| value.as_str())
            .filter(|value| value.starts_with("https://"))
        {
            return Some(url.to_string());
        }
    }

    ngrok_url_regex()
        .find(line)
        .map(|matched| matched.as_str().to_string())
}

fn cloudflared_url_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"https://[a-z0-9-]+\.trycloudflare\.com\b")
            .expect("valid cloudflared URL regex")
    })
}

fn ngrok_url_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"https://[a-z0-9.-]*ngrok(?:-free)?\.app\b").expect("valid ngrok URL regex")
    })
}

async fn terminate_child(child: &mut Child) -> anyhow::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }

    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(5), child.wait()).await;
    Ok(())
}

fn default_data_file() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".tunnelmux").join("state.json");
    }
    PathBuf::from("./data/state.json")
}

fn default_provider_log_file() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".tunnelmux").join("provider.log");
    }
    PathBuf::from("./data/provider.log")
}

async fn load_persisted_state(path: &Path) -> anyhow::Result<PersistedState> {
    if !path.exists() {
        return Ok(PersistedState::default());
    }

    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read state file: {}", path.display()))?;
    let mut parsed: PersistedState = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse state file: {}", path.display()))?;

    if matches!(
        parsed.tunnel.state,
        TunnelState::Running | TunnelState::Starting
    ) {
        parsed.tunnel.state = TunnelState::Stopped;
        parsed.tunnel.process_id = None;
        parsed.tunnel.last_error =
            Some("daemon restarted; previous tunnel process was detached".to_string());
        parsed.tunnel.updated_at = now_iso();
    }

    Ok(parsed)
}

async fn save_state_file(path: &Path, state: &PersistedState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create state dir: {}", parent.display()))?;
    }

    let raw = serde_json::to_string_pretty(state)?;
    let tmp_path = path.with_extension("json.tmp");

    fs::write(&tmp_path, format!("{raw}\n"))
        .await
        .with_context(|| format!("failed to write state temp file: {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path).await.with_context(|| {
        format!(
            "failed to move state temp file {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
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

    #[test]
    fn select_route_prefers_host_specific() {
        let routes = vec![
            route("fallback", None, Some("/"), None, true),
            route(
                "host-specific",
                Some("app.local"),
                Some("/"),
                None,
                true,
            ),
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

        let state = Arc::new(AppState {
            runtime: Mutex::new(RuntimeState {
                persisted: PersistedState {
                    tunnel: default_tunnel_status(TunnelState::Idle),
                    routes: vec![RouteRule {
                        id: "wss".to_string(),
                        match_host: None,
                        match_path_prefix: Some("/".to_string()),
                        strip_path_prefix: None,
                        upstream_url: format!("https://localhost:{}", upstream_addr.port()),
                        fallback_upstream_url: None,
                        health_check_path: None,
                        enabled: true,
                    }],
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
            data_file: PathBuf::from("/tmp/tunnelmux-test-state.json"),
            provider_log_file: PathBuf::from("/tmp/tunnelmux-test-provider.log"),
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
                public_base_url: "https://demo.trycloudflare.com".to_string(),
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
                public_base_url: "https://demo.trycloudflare.com".to_string(),
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
    fn should_failover_status_only_for_server_errors() {
        assert!(should_failover_status(StatusCode::BAD_GATEWAY));
        assert!(should_failover_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(!should_failover_status(StatusCode::BAD_REQUEST));
        assert!(!should_failover_status(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn ordered_upstream_targets_prefers_primary_when_healthy() {
        let route = RouteRule {
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
