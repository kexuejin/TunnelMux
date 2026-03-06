use super::*;

pub(super) async fn proxy_request(
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

pub(super) fn extract_host_from_headers(headers: &HeaderMap) -> Option<String> {
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

pub(super) fn is_websocket_upgrade_request(method: &Method, headers: &HeaderMap) -> bool {
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

pub(super) async fn proxy_websocket_request(
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

pub(super) async fn send_http_upstream(
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

pub(super) async fn build_http_proxy_response(
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

pub(super) async fn build_ws_handshake_failure_response(
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

pub(super) fn build_upstream_url(
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

pub(super) fn build_upstream_uri(
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

pub(super) fn should_failover_status(status: StatusCode) -> bool {
    status.is_server_error()
}

pub(super) fn effective_route_health_check_path(
    route: &RouteRule,
    default_health_check_path: &str,
) -> String {
    route
        .health_check_path
        .clone()
        .unwrap_or_else(|| default_health_check_path.to_string())
}

pub(super) fn upstream_health_key(
    upstream_url: &str,
    health_check_path: &str,
) -> UpstreamHealthKey {
    UpstreamHealthKey {
        upstream_url: upstream_url.to_string(),
        health_check_path: health_check_path.to_string(),
    }
}

pub(super) fn ordered_upstream_targets(
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

pub(super) fn collect_upstream_health_entries(
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

pub(super) fn rewrite_path(path: &str, route: &RouteRule) -> String {
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

pub(super) fn join_upstream_path(base_path: &str, forwarded_path: &str) -> String {
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

pub(super) fn select_route<'a>(
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

pub(super) fn copy_headers_to_upstream(
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

pub(super) fn copy_headers_for_websocket_upstream(target: &mut HeaderMap, source: &HeaderMap) {
    for (name, value) in source {
        if name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        target.insert(name, value.clone());
    }
}

pub(super) fn copy_headers_from_upstream(
    target: &mut HeaderMap,
    headers: &reqwest::header::HeaderMap,
) {
    for (name, value) in headers {
        if is_hop_by_hop_header(name) {
            continue;
        }
        target.insert(name, value.clone());
    }
}

pub(super) fn copy_headers_unfiltered(target: &mut HeaderMap, headers: &HeaderMap) {
    for (name, value) in headers {
        target.insert(name, value.clone());
    }
}

pub(super) fn is_hop_by_hop_header(name: &HeaderName) -> bool {
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
