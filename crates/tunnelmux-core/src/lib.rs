use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_CONTROL_ADDR: &str = "127.0.0.1:4765";
pub const DEFAULT_GATEWAY_TARGET_URL: &str = "http://127.0.0.1:18080";
pub const DISABLED_HEALTH_CHECK_SENTINEL: &str = "/__disabled__";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TunnelProvider {
    Cloudflared,
    Ngrok,
}

/// Control-plane authentication mode.
///
/// - `require` (default): fail closed — requests are rejected unless they
///   carry a valid bearer token, even when the daemon has no token configured
///   (it then auto-generates one and records it on disk).
/// - `optional`: backward compatible — a configured token is enforced, but a
///   tokenless daemon stays open.
/// - `off`: never enforce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlAuthMode {
    Require,
    Optional,
    Off,
}

impl std::str::FromStr for ControlAuthMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "require" => Ok(ControlAuthMode::Require),
            "optional" => Ok(ControlAuthMode::Optional),
            "off" => Ok(ControlAuthMode::Off),
            other => Err(format!(
                "invalid control-auth mode '{other}' (expected require | optional | off)"
            )),
        }
    }
}

impl ControlAuthMode {
    pub const DEFAULT: ControlAuthMode = ControlAuthMode::Require;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TunnelState {
    Idle,
    Starting,
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelStatus {
    pub state: TunnelState,
    pub provider: Option<TunnelProvider>,
    pub target_url: Option<String>,
    pub public_base_url: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub process_id: Option<u32>,
    pub auto_restart: bool,
    pub restart_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelStartRequest {
    pub tunnel_id: String,
    pub provider: TunnelProvider,
    pub target_url: String,
    pub auto_restart: Option<bool>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelStatusResponse {
    pub tunnel_id: String,
    pub tunnel: TunnelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelStopRequest {
    pub tunnel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelDeleteRequest {
    pub tunnel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelProfileSummary {
    pub id: String,
    pub name: Option<String>,
    pub provider: Option<TunnelProvider>,
    pub state: TunnelState,
    pub target_url: Option<String>,
    pub public_base_url: Option<String>,
    pub route_count: usize,
    pub enabled_route_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelWorkspaceResponse {
    pub tunnels: Vec<TunnelProfileSummary>,
    pub current_tunnel_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelLogsResponse {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteRule {
    pub tunnel_id: String,
    pub id: String,
    pub match_host: Option<String>,
    pub match_path_prefix: Option<String>,
    pub strip_path_prefix: Option<String>,
    pub upstream_url: String,
    pub fallback_upstream_url: Option<String>,
    pub health_check_path: Option<String>,
    pub enabled: bool,
    /// Forward the request's original Host header to the upstream instead of
    /// the upstream's own authority. Needed by upstreams that validate Host
    /// against a public tunnel domain (same-origin / DNS-rebinding fences).
    #[serde(default)]
    pub forward_host_header: bool,
    /// Rewrite upstream response bodies so root-relative URLs carry the
    /// `match_path_prefix` mount (subpath hosting): `src`/`href` and the
    /// boot-manifest `url` in HTML, and `/api` references in JavaScript. A
    /// no-op when `match_path_prefix` is unset; already-prefixed references
    /// are left alone.
    #[serde(default)]
    pub rewrite_response_paths: bool,
}

pub fn route_health_check_enabled(route: &RouteRule) -> bool {
    route.health_check_path.as_deref() != Some(DISABLED_HEALTH_CHECK_SENTINEL)
}

/// Synthetic route id used by the access API to read/write the global default
/// service gate. Real service route ids should not use this value.
pub const DEFAULT_ROUTE_ACCESS_ID: &str = "__default__";

/// Gateway access gate configuration. Stored as a side table keyed by route
/// `id` for service overrides, plus one daemon-wide default. Routes inherit the
/// default unless they define their own code or set `public` to true.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteAccessConfig {
    /// Access code required to reach the route through the gateway. Empty/None
    /// means either inherit the default (route override) or no default gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_access_code: Option<String>,
    /// True means this route is explicitly public and does not inherit the
    /// default access code. Ignored for the daemon-wide default config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public: Option<bool>,
    /// How long (ms) a verified access cookie stays valid for this route.
    /// Defaults to the daemon-wide window when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_ttl_ms: Option<u64>,
}

/// Request to set/clear the gateway access config for one route, or the global
/// default when `route_id` is `__default__`, `default`, or `*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRouteAccessRequest {
    pub route_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_access_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie_ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRouteAccessResponse {
    pub route_id: String,
    pub require_access_code: Option<String>,
    pub public: Option<bool>,
    pub cookie_ttl_ms: Option<u64>,
}
/// A non-secret summary of one route access gate, safe for list endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteAccessSummary {
    pub route_id: String,
    /// True when this route effectively requires an access code.
    pub gated: bool,
    /// One of: route, inherited, public, open.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode: String,
    /// True when the route has an explicit override entry.
    #[serde(default)]
    pub explicit: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteAccessSummaryResponse {
    /// True when a daemon-wide default access code is configured.
    #[serde(default)]
    pub default_gated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_cookie_ttl_ms: Option<u64>,
    #[serde(default)]
    pub routes: Vec<RouteAccessSummary>,
}

pub fn effective_route_health_check_path(
    route: &RouteRule,
    default_health_check_path: &str,
) -> String {
    route
        .health_check_path
        .clone()
        .unwrap_or_else(|| default_health_check_path.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateRouteRequest {
    pub tunnel_id: String,
    pub id: String,
    pub match_host: Option<String>,
    pub match_path_prefix: Option<String>,
    pub strip_path_prefix: Option<String>,
    pub upstream_url: String,
    pub fallback_upstream_url: Option<String>,
    pub health_check_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check_enabled: Option<bool>,
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forward_host_header: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewrite_response_paths: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutesResponse {
    pub routes: Vec<RouteRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteMatchTarget {
    pub upstream_url: String,
    pub healthy: Option<bool>,
    pub last_checked_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteMatchResponse {
    pub host: Option<String>,
    pub path: String,
    pub matched: bool,
    pub route: Option<RouteRule>,
    pub forwarded_path: Option<String>,
    pub health_check_path: Option<String>,
    pub targets: Vec<RouteMatchTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyRoutesRequest {
    pub routes: Vec<CreateRouteRequest>,
    pub replace: Option<bool>,
    pub dry_run: Option<bool>,
    pub allow_empty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyRoutesResponse {
    pub applied: usize,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
    pub removed: Vec<String>,
    pub replace: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamHealthEntry {
    pub upstream_url: String,
    pub health_check_path: String,
    pub healthy: Option<bool>,
    pub last_checked_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamsHealthResponse {
    pub upstreams: Vec<UpstreamHealthEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthCheckSettings {
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthCheckSettingsResponse {
    pub health_check: HealthCheckSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReloadSettingsResponse {
    pub reloaded: bool,
    pub route_count: usize,
    pub tunnel_state: TunnelState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateHealthCheckSettingsRequest {
    pub interval_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetricsResponse {
    pub tunnel_state: TunnelState,
    pub running_tunnel: bool,
    pub pending_restart: bool,
    pub route_count: usize,
    pub enabled_route_count: usize,
    pub upstream_health_entries: usize,
    pub health_check: HealthCheckSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticsResponse {
    pub data_file: String,
    pub config_file: String,
    pub provider_log_file: String,
    pub route_count: usize,
    pub enabled_route_count: usize,
    pub tunnel_state: TunnelState,
    pub pending_restart: bool,
    pub config_reload_enabled: bool,
    pub config_reload_interval_ms: u64,
    pub last_config_reload_at: Option<String>,
    pub last_config_reload_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardResponse {
    pub tunnel: TunnelStatus,
    pub metrics: MetricsResponse,
    pub routes: Vec<RouteRule>,
    pub upstreams: Vec<UpstreamHealthEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteRouteResponse {
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteTunnelResponse {
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Control-plane access-code auth status.
pub struct AuthStatusResponse {
    /// Whether loopback is currently unlocked.
    pub unlocked: bool,
    /// Epoch millis when the current unlock expires, if unlocked.
    pub unlock_expires_at: Option<i64>,
    /// The current access code (only surfaced for loopback callers).
    pub code: Option<String>,
    /// Whether the access code is fixed (true) or rotates on each relock.
    pub fixed_code: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthUnlockRequest {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorResponse {
    pub error: String,
}
