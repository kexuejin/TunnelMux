use super::*;
use tunnelmux_core::{TunnelProfileSummary, TunnelWorkspaceResponse};

#[derive(Debug, Deserialize)]
pub(super) struct TunnelQuery {
    pub tunnel_id: Option<String>,
}

pub(super) async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "tunnelmuxd".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

pub(super) fn resolve_api_token(arg_token: Option<String>) -> Option<String> {
    arg_token
        .or_else(|| std::env::var("TUNNELMUX_API_TOKEN").ok())
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

pub(super) fn resolve_initial_health_check_settings(
    startup_default: HealthCheckSettings,
    persisted: Option<HealthCheckSettings>,
) -> HealthCheckSettings {
    persisted.unwrap_or(startup_default)
}

pub(super) async fn control_auth_middleware(
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

pub(super) fn is_authorized_request(headers: &HeaderMap, expected_token: Option<&str>) -> bool {
    match expected_token {
        None => true,
        Some(token) => extract_bearer_token(headers)
            .map(|candidate| candidate == token)
            .unwrap_or(false),
    }
}

pub(super) fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
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

pub(super) async fn get_tunnel_status(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TunnelQuery>,
) -> Result<Json<TunnelStatusResponse>, ApiError> {
    Ok(Json(
        build_tunnel_status_snapshot(&state, query.tunnel_id.as_deref().unwrap_or("primary"))
            .await?,
    ))
}

pub(super) async fn get_tunnel_workspace(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TunnelWorkspaceResponse>, ApiError> {
    Ok(Json(build_tunnel_workspace_snapshot(&state).await?))
}

pub(super) async fn stream_tunnel_status(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StreamIntervalQuery>,
) -> Result<Response, ApiError> {
    let interval_ms = normalize_stream_interval_ms(query.interval_ms)?;
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let state_for_task = state.clone();
    let tunnel_id = "primary".to_string();

    tokio::spawn(async move {
        loop {
            match build_tunnel_status_snapshot(&state_for_task, &tunnel_id).await {
                Ok(snapshot) => {
                    let payload = match serde_json::to_string(&snapshot) {
                        Ok(value) => value,
                        Err(err) => {
                            let _ = tx
                                .send(Ok(Event::default().event("error").data(format!(
                                    "failed to serialize tunnel status snapshot: {err}"
                                ))))
                                .await;
                            return;
                        }
                    };
                    if tx
                        .send(Ok(Event::default().event("snapshot").data(payload)))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(err) => {
                    if tx
                        .send(Ok(Event::default().event("error").data(err.message)))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
            sleep(Duration::from_millis(interval_ms)).await;
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

pub(super) async fn get_health_check_settings(
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

pub(super) async fn update_health_check_settings(
    State(state): State<Arc<AppState>>,
    Json(request): Json<UpdateHealthCheckSettingsRequest>,
) -> Result<Json<HealthCheckSettingsResponse>, ApiError> {
    let updated = {
        let mut current = state.health_check_settings.write().await;
        let next = apply_health_check_settings_update(&current, request)?;
        *current = next.clone();
        next
    };
    {
        let mut runtime = state.runtime.lock().await;
        runtime.persisted.health_check = Some(updated.clone());
    }
    persist_from_runtime(&state).await?;

    Ok(Json(HealthCheckSettingsResponse {
        health_check: updated,
    }))
}

pub(super) async fn reload_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ReloadSettingsResponse>, ApiError> {
    if state.config_file.exists() {
        reload_config_file(&state, true)
            .await
            .map_err(|err| ApiError::internal(format!("failed to reload config file: {err}")))?;
        let (route_count, tunnel_state) = {
            let runtime = state.runtime.lock().await;
            (
                runtime.persisted.routes.len(),
                runtime.persisted.tunnel.state.clone(),
            )
        };
        return Ok(Json(ReloadSettingsResponse {
            reloaded: true,
            route_count,
            tunnel_state,
        }));
    }

    let reloaded = load_persisted_state(&state.data_file)
        .await
        .map_err(|err| ApiError::internal(format!("failed to reload state file: {err}")))?;
    let updated_health_check = {
        let current = state.health_check_settings.read().await;
        resolve_initial_health_check_settings(current.clone(), reloaded.health_check.clone())
    };

    {
        let mut current = state.health_check_settings.write().await;
        *current = updated_health_check.clone();
    }

    let (route_count, tunnel_state) = {
        let mut runtime = state.runtime.lock().await;
        runtime.persisted.routes = reloaded.routes;
        runtime.persisted.health_check = Some(updated_health_check.clone());
        (
            runtime.persisted.routes.len(),
            runtime.persisted.tunnel.state.clone(),
        )
    };

    Ok(Json(ReloadSettingsResponse {
        reloaded: true,
        route_count,
        tunnel_state,
    }))
}

pub(super) async fn get_tunnel_logs(
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

pub(super) async fn stream_tunnel_logs(
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

pub(super) async fn start_tunnel(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<TunnelStartRequest>,
) -> Result<Json<TunnelStatusResponse>, ApiError> {
    if request.tunnel_id.trim().is_empty() {
        return Err(ApiError::bad_request("tunnel_id is required"));
    }
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
            public_base_url: spawned.public_url.clone(),
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

    Ok(Json(TunnelStatusResponse {
        tunnel_id: request.tunnel_id,
        tunnel: status,
    }))
}

pub(super) async fn stop_tunnel(
    State(state): State<Arc<AppState>>,
    Json(request): Json<tunnelmux_core::TunnelStopRequest>,
) -> Result<Json<TunnelStatusResponse>, ApiError> {
    if request.tunnel_id.trim().is_empty() {
        return Err(ApiError::bad_request("tunnel_id is required"));
    }
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

    Ok(Json(TunnelStatusResponse {
        tunnel_id: request.tunnel_id,
        tunnel: status,
    }))
}

pub(super) async fn list_routes(State(state): State<Arc<AppState>>) -> Json<RoutesResponse> {
    Json(build_routes_snapshot(&state).await)
}

pub(super) async fn match_route(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RouteMatchQuery>,
) -> Result<Json<RouteMatchResponse>, ApiError> {
    let path = normalize_match_route_path(&query.path)?;
    let host = query
        .host
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    let route = {
        let runtime = state.runtime.lock().await;
        select_route(&runtime.persisted.routes, host.as_deref(), &path).cloned()
    };

    let Some(route) = route else {
        return Ok(Json(RouteMatchResponse {
            host,
            path,
            matched: false,
            route: None,
            forwarded_path: None,
            health_check_path: None,
            targets: Vec::new(),
        }));
    };

    let default_health_check_path = {
        let settings = state.health_check_settings.read().await;
        settings.path.clone()
    };
    let route_health_check_path =
        effective_route_health_check_path(&route, &default_health_check_path);
    let forwarded_path = rewrite_path(&path, &route);
    let targets = {
        let health_map = state.upstream_health.lock().await;
        ordered_upstream_targets(&route, &route_health_check_path, &health_map)
            .into_iter()
            .map(|upstream_url| {
                let health = health_map.get(&upstream_health_key(
                    &upstream_url,
                    &route_health_check_path,
                ));
                RouteMatchTarget {
                    upstream_url,
                    healthy: health.map(|item| item.healthy),
                    last_checked_at: health.map(|item| item.last_checked_at.clone()),
                    last_error: health.and_then(|item| item.last_error.clone()),
                }
            })
            .collect::<Vec<_>>()
    };

    Ok(Json(RouteMatchResponse {
        host,
        path,
        matched: true,
        route: Some(route),
        forwarded_path: Some(forwarded_path),
        health_check_path: Some(route_health_check_path),
        targets,
    }))
}

pub(super) async fn stream_routes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StreamIntervalQuery>,
) -> Result<Response, ApiError> {
    let interval_ms = normalize_stream_interval_ms(query.interval_ms)?;
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let state_for_task = state.clone();

    tokio::spawn(async move {
        loop {
            let snapshot = build_routes_snapshot(&state_for_task).await;
            let payload = match serde_json::to_string(&snapshot) {
                Ok(value) => value,
                Err(err) => {
                    let _ = tx
                        .send(Ok(Event::default()
                            .event("error")
                            .data(format!("failed to serialize routes snapshot: {err}"))))
                        .await;
                    return;
                }
            };
            if tx
                .send(Ok(Event::default().event("snapshot").data(payload)))
                .await
                .is_err()
            {
                return;
            }
            sleep(Duration::from_millis(interval_ms)).await;
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

pub(super) async fn get_upstreams_health(
    State(state): State<Arc<AppState>>,
) -> Json<UpstreamsHealthResponse> {
    Json(build_upstreams_health_snapshot(&state).await)
}

pub(super) async fn stream_upstreams_health(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StreamIntervalQuery>,
) -> Result<Response, ApiError> {
    let interval_ms = normalize_stream_interval_ms(query.interval_ms)?;
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let state_for_task = state.clone();

    tokio::spawn(async move {
        loop {
            let snapshot = build_upstreams_health_snapshot(&state_for_task).await;
            let payload = match serde_json::to_string(&snapshot) {
                Ok(value) => value,
                Err(err) => {
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(format!(
                            "failed to serialize upstream health snapshot: {err}"
                        ))))
                        .await;
                    return;
                }
            };
            if tx
                .send(Ok(Event::default().event("snapshot").data(payload)))
                .await
                .is_err()
            {
                return;
            }
            sleep(Duration::from_millis(interval_ms)).await;
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

pub(super) async fn get_metrics(State(state): State<Arc<AppState>>) -> Json<MetricsResponse> {
    Json(build_metrics_snapshot(&state).await)
}

pub(super) async fn stream_metrics(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StreamIntervalQuery>,
) -> Result<Response, ApiError> {
    let interval_ms = normalize_stream_interval_ms(query.interval_ms)?;
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let state_for_task = state.clone();

    tokio::spawn(async move {
        loop {
            let snapshot = build_metrics_snapshot(&state_for_task).await;
            let payload = match serde_json::to_string(&snapshot) {
                Ok(value) => value,
                Err(err) => {
                    let _ = tx
                        .send(Ok(Event::default()
                            .event("error")
                            .data(format!("failed to serialize metrics snapshot: {err}"))))
                        .await;
                    return;
                }
            };
            if tx
                .send(Ok(Event::default().event("snapshot").data(payload)))
                .await
                .is_err()
            {
                return;
            }
            sleep(Duration::from_millis(interval_ms)).await;
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

pub(super) async fn get_diagnostics(
    State(state): State<Arc<AppState>>,
) -> Json<DiagnosticsResponse> {
    Json(build_diagnostics_snapshot(&state).await)
}

pub(super) async fn get_dashboard(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DashboardResponse>, ApiError> {
    let snapshot = build_dashboard_snapshot(&state).await?;
    Ok(Json(snapshot))
}

pub(super) async fn stream_dashboard(
    State(state): State<Arc<AppState>>,
    Query(query): Query<StreamIntervalQuery>,
) -> Result<Response, ApiError> {
    let interval_ms = normalize_stream_interval_ms(query.interval_ms)?;
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);
    let state_for_task = state.clone();

    tokio::spawn(async move {
        loop {
            match build_dashboard_snapshot(&state_for_task).await {
                Ok(snapshot) => {
                    let payload = match serde_json::to_string(&snapshot) {
                        Ok(value) => value,
                        Err(err) => {
                            let _ = tx
                                .send(Ok(Event::default().event("error").data(format!(
                                    "failed to serialize dashboard snapshot: {err}"
                                ))))
                                .await;
                            return;
                        }
                    };
                    if tx
                        .send(Ok(Event::default().event("snapshot").data(payload)))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Err(err) => {
                    if tx
                        .send(Ok(Event::default().event("error").data(err.message)))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }

            sleep(Duration::from_millis(interval_ms)).await;
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

pub(super) async fn build_diagnostics_snapshot(state: &Arc<AppState>) -> DiagnosticsResponse {
    let (routes, tunnel_state, pending_restart) = {
        let runtime = state.runtime.lock().await;
        (
            runtime.persisted.routes.clone(),
            runtime.persisted.tunnel.state.clone(),
            runtime.pending_restart.is_some(),
        )
    };
    let (
        config_reload_enabled,
        config_reload_interval_ms,
        last_config_reload_at,
        last_config_reload_error,
    ) = {
        let status = state.config_reload_status.lock().await;
        (
            status.enabled,
            status.interval_ms,
            status.last_config_reload_at.clone(),
            status.last_config_reload_error.clone(),
        )
    };

    DiagnosticsResponse {
        data_file: state.data_file.display().to_string(),
        config_file: state.config_file.display().to_string(),
        provider_log_file: state.provider_log_file.display().to_string(),
        route_count: routes.len(),
        enabled_route_count: routes.iter().filter(|route| route.enabled).count(),
        tunnel_state,
        pending_restart,
        config_reload_enabled,
        config_reload_interval_ms,
        last_config_reload_at,
        last_config_reload_error,
    }
}

pub(super) async fn build_dashboard_snapshot(
    state: &Arc<AppState>,
) -> Result<DashboardResponse, ApiError> {
    reconcile_runtime_and_maybe_restart(state).await?;

    let (tunnel, routes) = {
        let runtime = state.runtime.lock().await;
        (
            runtime.persisted.tunnel.clone(),
            runtime.persisted.routes.clone(),
        )
    };
    let metrics = build_metrics_snapshot(state).await;
    let health_check = metrics.health_check.clone();
    let health_map = {
        let map = state.upstream_health.lock().await;
        map.clone()
    };

    let upstreams = collect_upstream_health_entries(&routes, &health_check.path, &health_map);

    Ok(DashboardResponse {
        tunnel,
        metrics,
        routes,
        upstreams,
    })
}

pub(super) async fn build_upstreams_health_snapshot(
    state: &Arc<AppState>,
) -> UpstreamsHealthResponse {
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
    UpstreamsHealthResponse {
        upstreams: collect_upstream_health_entries(
            &routes,
            &default_health_check_path,
            &health_map,
        ),
    }
}

pub(super) async fn build_routes_snapshot(state: &Arc<AppState>) -> RoutesResponse {
    let routes = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.routes.clone()
    };
    RoutesResponse { routes }
}

pub(super) async fn build_tunnel_status_snapshot(
    state: &Arc<AppState>,
    tunnel_id: &str,
) -> Result<TunnelStatusResponse, ApiError> {
    reconcile_runtime_and_maybe_restart(state).await?;
    let snapshot = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.tunnel.clone()
    };
    Ok(TunnelStatusResponse {
        tunnel_id: tunnel_id.to_string(),
        tunnel: snapshot,
    })
}

pub(super) async fn build_tunnel_workspace_snapshot(
    state: &Arc<AppState>,
) -> Result<TunnelWorkspaceResponse, ApiError> {
    reconcile_runtime_and_maybe_restart(state).await?;

    let (tunnel, routes) = {
        let runtime = state.runtime.lock().await;
        (
            runtime.persisted.tunnel.clone(),
            runtime.persisted.routes.clone(),
        )
    };

    if tunnel.provider.is_none() && routes.is_empty() {
        return Ok(TunnelWorkspaceResponse {
            tunnels: Vec::new(),
            current_tunnel_id: None,
        });
    }

    let route_count = routes.len();
    let enabled_route_count = routes.iter().filter(|route| route.enabled).count();
    let summary = TunnelProfileSummary {
        id: "primary".to_string(),
        name: None,
        provider: tunnel.provider,
        state: tunnel.state,
        target_url: tunnel.target_url,
        public_base_url: tunnel.public_base_url,
        route_count,
        enabled_route_count,
    };

    Ok(TunnelWorkspaceResponse {
        tunnels: vec![summary],
        current_tunnel_id: Some("primary".to_string()),
    })
}

pub(super) async fn build_metrics_snapshot(state: &Arc<AppState>) -> MetricsResponse {
    let (tunnel_state, running_tunnel, pending_restart, routes) = {
        let runtime = state.runtime.lock().await;
        (
            runtime.persisted.tunnel.state.clone(),
            runtime.running_tunnel.is_some(),
            runtime.pending_restart.is_some(),
            runtime.persisted.routes.clone(),
        )
    };
    let upstream_health_entries = {
        let health_map = state.upstream_health.lock().await;
        health_map.len()
    };
    let health_check = {
        let settings = state.health_check_settings.read().await;
        settings.clone()
    };

    MetricsResponse {
        tunnel_state,
        running_tunnel,
        pending_restart,
        route_count: routes.len(),
        enabled_route_count: routes.iter().filter(|item| item.enabled).count(),
        upstream_health_entries,
        health_check,
    }
}

pub(super) async fn add_route(
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

pub(super) async fn update_route(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<UpdateRouteQuery>,
    Json(request): Json<CreateRouteRequest>,
) -> Result<(StatusCode, Json<RouteRule>), ApiError> {
    let route = normalize_route_request(request)?;
    ensure_route_id_matches(&id, &route.id)?;

    let upsert = query.upsert.unwrap_or(false);
    let updated = {
        let mut runtime = state.runtime.lock().await;
        if replace_route(&mut runtime.persisted.routes, route.clone()) {
            true
        } else if upsert {
            runtime.persisted.routes.push(route.clone());
            false
        } else {
            return Err(ApiError::not_found(format!("route '{}' not found", id)));
        }
    };

    persist_from_runtime(&state).await?;
    let status = if updated {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(route)))
}

pub(super) async fn delete_route(
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

pub(super) async fn apply_routes(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ApplyRoutesRequest>,
) -> Result<Json<ApplyRoutesResponse>, ApiError> {
    let replace = request.replace.unwrap_or(false);
    let dry_run = request.dry_run.unwrap_or(false);
    let allow_empty = request.allow_empty.unwrap_or(false);

    let normalized = request
        .routes
        .into_iter()
        .map(normalize_route_request)
        .collect::<Result<Vec<_>, _>>()?;
    ensure_unique_route_ids(&normalized)?;
    ensure_apply_payload_safe(&normalized, replace, allow_empty)?;

    let applied = normalized.len();
    let plan = if dry_run {
        let runtime = state.runtime.lock().await;
        build_route_apply_plan(&runtime.persisted.routes, &normalized, replace)
    } else {
        let mut runtime = state.runtime.lock().await;
        let plan = build_route_apply_plan(&runtime.persisted.routes, &normalized, replace);
        runtime.persisted.routes =
            apply_route_rules(&runtime.persisted.routes, normalized, replace);
        plan
    };

    if !dry_run {
        persist_from_runtime(&state).await?;
    }

    Ok(Json(ApplyRoutesResponse {
        applied,
        created: plan.created,
        updated: plan.updated,
        unchanged: plan.unchanged,
        removed: plan.removed,
        replace,
        dry_run,
    }))
}
