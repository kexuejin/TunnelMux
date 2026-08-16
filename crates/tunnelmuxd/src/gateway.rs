use super::*;
use tokio_stream::StreamExt;

pub(super) async fn proxy_request_for_tunnel(
    State(gateway_state): State<Arc<TunnelGatewayState>>,
    request: Request,
) -> Result<Response, ApiError> {
    let state = &gateway_state.app_state;
    let tunnel_id = gateway_state.tunnel_id.as_str();
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    let path = uri.path().to_string();
    let query = uri.query().map(|value| value.to_string());
    let host = extract_host_from_headers(&headers);

    let (has_enabled_routes, route) = {
        let runtime = state.runtime.lock().await;
        let routes = runtime
            .persisted
            .routes
            .iter()
            .filter(|route| route.tunnel_id == tunnel_id)
            .cloned()
            .collect::<Vec<_>>();
        let has_enabled_routes = routes.iter().any(|route| route.enabled);
        let route = select_route(&routes, host.as_deref(), &path).cloned();
        (has_enabled_routes, route)
    };

    let route = match route {
        Some(route) => route,
        None => {
            if !has_enabled_routes
                && method == Method::GET
                && !is_websocket_upgrade_request(&method, &headers)
            {
                return Ok(build_welcome_response());
            }
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

                return build_http_proxy_response(response, rewrite_prefix_for(&route).as_deref()).await;
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
                    return build_http_proxy_response(response, rewrite_prefix_for(&route).as_deref()).await;
                }
                return Err(err);
            }
        }
    }

    if let Some(response) = last_response {
        return build_http_proxy_response(response, rewrite_prefix_for(&route).as_deref()).await;
    }
    if let Some(err) = last_error {
        return Err(err);
    }

    Err(ApiError::internal(format!(
        "no upstream available for route '{}'",
        route.id
    )))
}

fn build_welcome_response() -> Response {
    const WELCOME_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>TunnelMux</title>
    <style>
      :root { color-scheme: dark; }
      body {
        margin: 0;
        min-height: 100vh;
        display: grid;
        place-items: center;
        padding: 24px;
        font-family: Inter, ui-sans-serif, system-ui, sans-serif;
        background: linear-gradient(180deg, #0b1324 0%, #08101d 100%);
        color: #eef2ff;
      }
      main {
        max-width: 640px;
        padding: 28px;
        border-radius: 24px;
        background: rgba(18, 24, 34, 0.88);
        border: 1px solid rgba(148, 163, 184, 0.16);
        box-shadow: 0 18px 44px rgba(15, 23, 42, 0.34);
      }
      p { color: #cbd5e1; line-height: 1.6; }
      code {
        padding: 2px 6px;
        border-radius: 8px;
        background: rgba(15, 23, 42, 0.92);
      }
    </style>
  </head>
  <body>
    <main>
      <h1>TunnelMux is live</h1>
      <p>Add your first service in the TunnelMux app to route this public URL to a local upstream.</p>
      <p>Typical first target: <code>http://127.0.0.1:3000</code></p>
    </main>
  </body>
</html>"#;

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html; charset=utf-8")
        .body(Body::from(WELCOME_HTML))
        .expect("welcome response should build")
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
            copy_headers_for_websocket_upstream(upstream_headers, &headers, route.forward_host_header);
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
    upstream_request = copy_headers_to_upstream(upstream_request, headers, route.forward_host_header);
    upstream_request = upstream_request.body(body.clone());

    upstream_request.send().await.map_err(|err| {
        ApiError::internal(format!("upstream request failed for '{}': {err}", route_id))
    })
}

pub(super) async fn build_http_proxy_response(
    upstream_response: reqwest::Response,
    rewrite_prefix: Option<&str>,
) -> Result<Response, ApiError> {
    let status = upstream_response.status();
    let upstream_headers = upstream_response.headers().clone();

    if let Some(prefix) = rewrite_prefix {
        let is_rewritable = upstream_headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|content_type| is_rewritable_content_type(content_type));
        let encoded = upstream_headers.contains_key(reqwest::header::CONTENT_ENCODING);
        if is_rewritable && !encoded {
            // The whole body must be visible to rewrite root-relative URLs;
            // text/html and JavaScript responses are bounded by the client
            // bundle sizes DSH serves (a few hundred KB).
            let bytes = upstream_response
                .bytes()
                .await
                .map_err(|err| ApiError::internal(format!("failed reading upstream response: {err}")))?;
            let rewritten = rewrite_root_paths(&String::from_utf8_lossy(&bytes), prefix);
            let mut response_builder = Response::builder().status(status);
            if let Some(headers_map) = response_builder.headers_mut() {
                copy_headers_from_upstream(headers_map, &upstream_headers);
                headers_map.insert(
                    reqwest::header::CONTENT_LENGTH,
                    rewritten.len().to_string().parse().expect("byte length fits header"),
                );
            }
            return response_builder
                .body(Body::from(rewritten))
                .map_err(|err| ApiError::internal(format!("failed to build proxy response: {err}")));
        }
    }

    let upstream_body = upstream_response.bytes_stream().map(|chunk| {
        chunk
            .map_err(|err| std::io::Error::other(format!("upstream response stream failed: {err}")))
    });

    let mut response_builder = Response::builder().status(status);
    if let Some(headers_map) = response_builder.headers_mut() {
        copy_headers_from_upstream(headers_map, &upstream_headers);
    }
    response_builder
        .body(Body::from_stream(upstream_body))
        .map_err(|err| ApiError::internal(format!("failed to build proxy response: {err}")))
}

/** Whether a response content type carries text a root-URL rewrite may touch. */
fn is_rewritable_content_type(content_type: &str) -> bool {
    let normalized = content_type.to_ascii_lowercase();
    normalized.starts_with("text/html")
        || normalized.starts_with("text/javascript")
        || normalized.starts_with("application/javascript")
}

/**
 * Prefix root-relative URL references with a mount path: `src`/`href` and
 * `"url":` JSON values in HTML, and `/api` references in JavaScript (fetch,
 * SSE, and WebSocket paths). Protocol-relative (`//host`), scheme-absolute
 * (`https://…`), and already-prefixed references are left alone.
 * @param body - the upstream response body.
 * @param prefix - the mount prefix (leading slash, no trailing slash).
 */
pub(super) fn rewrite_root_paths(body: &str, prefix: &str) -> String {
    let prefix_inner = prefix.strip_prefix('/').unwrap_or(prefix);
    let mut out = guarded_replace(body, &["src=\"/", "href=\"/"], prefix_inner, |b, at, len| {
        // Guard: not protocol-relative (`//`) and not already prefixed.
        !b[at + len..].starts_with(b"/") && !b[at + len..].starts_with(prefix_inner.as_bytes())
    });
    out = guarded_replace(&out, &["\"url\":\"/"], prefix_inner, |b, at, len| {
        !b[at + len..].starts_with(b"/") && !b[at + len..].starts_with(prefix_inner.as_bytes())
    });
    out = guarded_replace(&out, &["\"/", "'/", "`/"], prefix_inner, |b, at, len| {
        // A quoted root slash whose path is `api` (fetch, SSE, WebSocket, or the
        // bare `/api` RPC channel); the prefix lands between the slash and `api`.
        // An already-prefixed `/api` (`"/deepseek/api`) has `deepseek` right
        // after the slash, so `api` never follows — no double-prefix.
        let rest = &b[at + len..];
        rest.starts_with(b"api")
            && matches!(b.get(at + len + 3), Some(b'"') | Some(b'\'') | Some(b'/') | Some(b'`'))
    });
    out
}

/**
 * One guarded replacement pass: at every occurrence of any marker, append the
 * marker verbatim plus `prefix_inner/` when `valid` allows it, else append the
 * marker unchanged. The marker includes the leading quote (for `"/api`) or the
 * trailing slash (for `src="/`), so the inserted prefix keeps the URL well-formed.
 */
fn guarded_replace(
    body: &str,
    markers: &[&str],
    prefix_inner: &str,
    valid: impl Fn(&[u8], usize, usize) -> bool,
) -> String {
    let b = body.as_bytes();
    let mut out = Vec::with_capacity(b.len() + 32);
    let mut i = 0;
    'outer: while i < b.len() {
        for marker in markers {
            let m = marker.as_bytes();
            if b[i..].starts_with(m) {
                out.extend_from_slice(m);
                if valid(b, i, m.len()) {
                    out.extend_from_slice(prefix_inner.as_bytes());
                    out.push(b'/');
                }
                i += m.len();
                continue 'outer;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).expect("guarded replacement preserves UTF-8")
}

/** The response-rewrite mount prefix for a route, when enabled and mounted. */
pub(super) fn rewrite_prefix_for(route: &RouteRule) -> Option<String> {
    if !route.rewrite_response_paths {
        return None;
    }
    route
        .match_path_prefix
        .as_ref()
        .filter(|prefix| !prefix.is_empty())
        .cloned()
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

    if !route_health_check_enabled(route) {
        return vec![primary, fallback];
    }

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
        if !route_health_check_enabled(route) {
            continue;
        }
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
    forward_host_header: bool,
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        if is_hop_by_hop_header(name) {
            continue;
        }
        if name.as_str().eq_ignore_ascii_case("host") && !forward_host_header {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder
}

pub(super) fn copy_headers_for_websocket_upstream(
    target: &mut HeaderMap,
    source: &HeaderMap,
    forward_host_header: bool,
) {
    for (name, value) in source {
        if name.as_str().eq_ignore_ascii_case("host") && !forward_host_header {
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

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;

    use super::{build_http_proxy_response, rewrite_root_paths};

    #[test]
    fn rewrite_prefixes_html_refs_and_manifest_urls() {
        let html = concat!(
            "<link href=\"/manifest.webmanifest\">",
            "<script src=\"/assets/index.js\"></script>",
            "<script>window.__DSH_BOOT__ = {\"rev\":\"x\",\"entries\":[{\"url\":\"/plugins/a/client.js?rev=x\"}]}</script>",
        );
        let out = rewrite_root_paths(html, "/deepseek");
        assert!(out.contains("href=\"/deepseek/manifest.webmanifest\""));
        assert!(out.contains("src=\"/deepseek/assets/index.js\""));
        assert!(out.contains("\"url\":\"/deepseek/plugins/a/client.js?rev=x\""));
    }

    #[test]
    fn rewrite_prefixes_js_api_references_in_all_quote_forms() {
        let js = r#"fetch("/api/session.list")"#;
        let js = format!(
            r#"{js}; const a = '/api/events.mux'; const b = `/api/${{method}}`; const c = rpc.call("/api", "goals/create");"#
        );
        let out = rewrite_root_paths(&js, "/deepseek");
        assert!(out.contains(r#""/deepseek/api/session.list""#));
        assert!(out.contains(r#"'/deepseek/api/events.mux'"#));
        assert!(out.contains("`/deepseek/api/${method}`"));
        assert!(out.contains(r#""/deepseek/api""#));
    }

    #[test]
    fn rewrite_leaves_protocol_relative_scheme_absolute_and_prefixed_urls_alone() {
        let html = concat!(
            "<script src=\"//cdn.example/lib.js\"></script>",
            "<link href=\"https://cdn.example/style.css\">",
            "<script src=\"/deepseek/assets/keep.js\"></script>",
        );
        let out = rewrite_root_paths(html, "/deepseek");
        assert!(out.contains("src=\"//cdn.example/lib.js\""));
        assert!(out.contains("href=\"https://cdn.example/style.css\""));
        assert!(out.contains("src=\"/deepseek/assets/keep.js\""));
        assert_eq!(out.matches("/deepseek/assets").count(), 1);
    }

    #[test]
    fn rewrite_is_a_noop_for_plain_text() {
        let body = "the /api path and /assets path are words here";
        assert_eq!(rewrite_root_paths(body, "/deepseek"), body);
    }

    #[tokio::test]
    async fn build_http_proxy_response_rewrites_uncompressed_html_when_route_enables_it() {
        let upstream = reqwest::Response::from(
            axum::http::Response::builder()
                .status(200)
                .header("content-type", "text/html; charset=utf-8")
                .body("<script src=\"/assets/index.js\"></script>".to_string())
                .expect("build upstream response"),
        );
        let response = build_http_proxy_response(upstream, Some("/deepseek"))
            .await
            .expect("proxy response");
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("src=\"/deepseek/assets/index.js\""), "rewritten body: {body}");
    }

    #[tokio::test]
    async fn build_http_proxy_response_skips_rewrite_for_content_encoded_bodies() {
        // A compressed upstream body must not be rewritten as plain text: the
        // encoded bytes are forwarded untouched so the client can decode them.
        let upstream = reqwest::Response::from(
            axum::http::Response::builder()
                .status(200)
                .header("content-type", "text/html; charset=utf-8")
                .header("content-encoding", "gzip")
                .body("<script src=\"/assets/index.js\"></script>".to_string())
                .expect("build upstream response"),
        );
        let response = build_http_proxy_response(upstream, Some("/deepseek"))
            .await
            .expect("proxy response");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response.headers().get("content-encoding").and_then(|v| v.to_str().ok()),
            Some("gzip")
        );
        let body = response
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes();
        let body = String::from_utf8_lossy(&body);
        assert!(!body.contains("/deepseek"), "encoded body must be untouched: {body}");
        assert!(body.contains("<script src=\"/assets/index.js\"></script>"));
    }
}
