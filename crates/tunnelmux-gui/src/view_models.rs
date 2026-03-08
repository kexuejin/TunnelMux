use serde::{Deserialize, Serialize};
use tunnelmux_core::{
    CreateRouteRequest, DiagnosticsResponse, RouteRule, TunnelLogsResponse, TunnelState,
    UpstreamHealthEntry,
};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteViewModel {
    pub tunnel_id: String,
    pub id: String,
    pub match_host: Option<String>,
    pub match_path_prefix: Option<String>,
    pub strip_path_prefix: Option<String>,
    pub upstream_url: String,
    pub fallback_upstream_url: Option<String>,
    pub health_check_path: Option<String>,
    pub enabled: bool,
    pub display_match: String,
}

impl From<RouteRule> for RouteViewModel {
    fn from(route: RouteRule) -> Self {
        let host = route.match_host.as_deref().unwrap_or("*");
        let path = route.match_path_prefix.as_deref().unwrap_or("/");
        Self {
            display_match: format!("{host}{path}"),
            tunnel_id: route.tunnel_id,
            id: route.id,
            match_host: route.match_host,
            match_path_prefix: route.match_path_prefix,
            strip_path_prefix: route.strip_path_prefix,
            upstream_url: route.upstream_url,
            fallback_upstream_url: route.fallback_upstream_url,
            health_check_path: route.health_check_path,
            enabled: route.enabled,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteWorkspaceSnapshot {
    pub routes: Vec<RouteViewModel>,
    pub message: Option<String>,
}

impl RouteWorkspaceSnapshot {
    pub fn from_routes(routes: Vec<RouteRule>, message: Option<String>) -> Self {
        Self {
            routes: routes.into_iter().map(RouteViewModel::from).collect(),
            message,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticsSummaryVm {
    pub data_file: String,
    pub config_file: String,
    pub provider_log_file: String,
    pub route_count: usize,
    pub enabled_route_count: usize,
    pub tunnel_state: String,
    pub pending_restart: bool,
    pub config_reload_enabled: bool,
    pub config_reload_interval_ms: u64,
    pub last_config_reload_at: Option<String>,
    pub last_config_reload_error: Option<String>,
}

impl From<DiagnosticsResponse> for DiagnosticsSummaryVm {
    fn from(response: DiagnosticsResponse) -> Self {
        Self {
            data_file: response.data_file,
            config_file: response.config_file,
            provider_log_file: response.provider_log_file,
            route_count: response.route_count,
            enabled_route_count: response.enabled_route_count,
            tunnel_state: tunnel_state_label(&response.tunnel_state),
            pending_restart: response.pending_restart,
            config_reload_enabled: response.config_reload_enabled,
            config_reload_interval_ms: response.config_reload_interval_ms,
            last_config_reload_at: response.last_config_reload_at,
            last_config_reload_error: response.last_config_reload_error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamHealthVm {
    pub upstream_url: String,
    pub health_check_path: String,
    pub health_label: String,
    pub last_checked_at: Option<String>,
    pub last_error: Option<String>,
}

impl From<UpstreamHealthEntry> for UpstreamHealthVm {
    fn from(entry: UpstreamHealthEntry) -> Self {
        Self {
            upstream_url: entry.upstream_url,
            health_check_path: entry.health_check_path,
            health_label: match entry.healthy {
                Some(true) => "healthy".to_string(),
                Some(false) => "unhealthy".to_string(),
                None => "unknown".to_string(),
            },
            last_checked_at: entry.last_checked_at,
            last_error: entry.last_error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogTailVm {
    pub requested_lines: usize,
    pub lines: Vec<String>,
}

impl LogTailVm {
    pub fn from_response(requested_lines: usize, response: TunnelLogsResponse) -> Self {
        Self {
            requested_lines,
            lines: response.lines,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderStatusVm {
    pub level: String,
    pub title: String,
    pub message: String,
    pub action_kind: Option<String>,
    pub action_label: Option<String>,
    pub action_payload: Option<String>,
    pub follow_up_action_kind: Option<String>,
    pub follow_up_action_label: Option<String>,
    pub follow_up_action_payload: Option<String>,
}

impl ProviderStatusVm {
    pub fn new(level: &str, title: &str, message: &str) -> Self {
        Self {
            level: level.to_string(),
            title: title.to_string(),
            message: message.to_string(),
            action_kind: None,
            action_label: None,
            action_payload: None,
            follow_up_action_kind: None,
            follow_up_action_label: None,
            follow_up_action_payload: None,
        }
    }

    pub fn with_action(mut self, action_kind: &str, action_label: &str) -> Self {
        self.action_kind = Some(action_kind.to_string());
        self.action_label = Some(action_label.to_string());
        self
    }

    pub fn with_action_payload(mut self, action_payload: &str) -> Self {
        self.action_payload = Some(action_payload.to_string());
        self
    }

    pub fn with_follow_up_action(mut self, action_kind: &str, action_label: &str) -> Self {
        self.follow_up_action_kind = Some(action_kind.to_string());
        self.follow_up_action_label = Some(action_label.to_string());
        self
    }

    pub fn with_follow_up_action_payload(mut self, action_payload: &str) -> Self {
        self.follow_up_action_payload = Some(action_payload.to_string());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelProfileVm {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub provider_availability: ProviderAvailabilityVm,
    pub state: String,
    pub route_count: usize,
    pub enabled_route_count: usize,
    pub public_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderAvailabilityVm {
    pub binary_name: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderAvailabilitySnapshotVm {
    pub cloudflared: ProviderAvailabilityVm,
    pub ngrok: ProviderAvailabilityVm,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelWorkspaceVm {
    pub tunnels: Vec<TunnelProfileVm>,
    pub current_tunnel_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteFormData {
    pub original_id: Option<String>,
    pub id: String,
    pub match_host: String,
    pub match_path_prefix: String,
    pub strip_path_prefix: String,
    pub upstream_url: String,
    pub fallback_upstream_url: String,
    pub health_check_path: String,
    pub enabled: bool,
}

impl Default for RouteFormData {
    fn default() -> Self {
        Self {
            original_id: None,
            id: String::new(),
            match_host: String::new(),
            match_path_prefix: "/".to_string(),
            strip_path_prefix: String::new(),
            upstream_url: String::new(),
            fallback_upstream_url: String::new(),
            health_check_path: String::new(),
            enabled: true,
        }
    }
}

impl RouteFormData {
    pub fn into_create_request(&self, tunnel_id: &str) -> CreateRouteRequest {
        CreateRouteRequest {
            tunnel_id: tunnel_id.to_string(),
            id: resolve_route_id(&self.id, &self.upstream_url),
            match_host: empty_to_none(&self.match_host),
            match_path_prefix: empty_to_none(&self.match_path_prefix),
            strip_path_prefix: empty_to_none(&self.strip_path_prefix),
            upstream_url: self.upstream_url.trim().to_string(),
            fallback_upstream_url: empty_to_none(&self.fallback_upstream_url),
            health_check_path: empty_to_none(&self.health_check_path),
            enabled: Some(self.enabled),
        }
    }
}

fn resolve_route_id(id: &str, upstream_url: &str) -> String {
    empty_to_none(id)
        .or_else(|| derive_route_id_from_upstream_url(upstream_url))
        .unwrap_or_default()
}

fn derive_route_id_from_upstream_url(upstream_url: &str) -> Option<String> {
    let url = Url::parse(upstream_url.trim()).ok()?;
    let host = url.host_str()?;
    let host_label = match host {
        "localhost" | "127.0.0.1" => "local".to_string(),
        _ => sanitize_route_id_fragment(host),
    };
    let path_label = url
        .path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.trim().is_empty()).last())
        .map(sanitize_route_id_fragment)
        .filter(|segment| !segment.is_empty());

    let mut parts = Vec::new();
    if !host_label.is_empty() {
        parts.push(host_label);
    }
    if let Some(path) = path_label {
        parts.push(path);
    }
    if let Some(port) = url.port() {
        parts.push(port.to_string());
    }

    let route_id = sanitize_route_id_fragment(&parts.join("-"));
    (!route_id.is_empty()).then_some(route_id)
}

fn sanitize_route_id_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn empty_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn tunnel_state_label(state: &TunnelState) -> String {
    match state {
        TunnelState::Idle => "idle",
        TunnelState::Starting => "starting",
        TunnelState::Running => "running",
        TunnelState::Stopped => "stopped",
        TunnelState::Error => "error",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunnelmux_core::{
        CreateRouteRequest, DiagnosticsResponse, TunnelLogsResponse, TunnelState,
        UpstreamHealthEntry,
    };

    #[test]
    fn route_form_data_derives_route_id_from_upstream_url_when_blank() {
        let request = RouteFormData {
            id: String::new(),
            upstream_url: "http://127.0.0.1:3000".to_string(),
            ..RouteFormData::default()
        }
        .into_create_request("primary");

        assert_eq!(
            request,
            CreateRouteRequest {
                tunnel_id: "primary".to_string(),
                id: "local-3000".to_string(),
                match_host: None,
                match_path_prefix: Some("/".to_string()),
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: Some(true),
            }
        );
    }

    #[test]
    fn route_form_data_preserves_user_entered_route_id() {
        let request = RouteFormData {
            id: "docs".to_string(),
            upstream_url: "http://127.0.0.1:4000/docs".to_string(),
            ..RouteFormData::default()
        }
        .into_create_request("primary");

        assert_eq!(request.id, "docs");
    }

    #[test]
    fn diagnostics_summary_vm_preserves_counts_and_reload_state() {
        let summary = DiagnosticsSummaryVm::from(DiagnosticsResponse {
            data_file: "/tmp/state.json".to_string(),
            config_file: "/tmp/config.json".to_string(),
            provider_log_file: "/tmp/provider.log".to_string(),
            route_count: 4,
            enabled_route_count: 3,
            tunnel_state: TunnelState::Running,
            pending_restart: true,
            config_reload_enabled: true,
            config_reload_interval_ms: 1000,
            last_config_reload_at: Some("2026-03-06T10:00:00Z".to_string()),
            last_config_reload_error: Some("failed to parse config".to_string()),
        });

        assert_eq!(summary.route_count, 4);
        assert_eq!(summary.enabled_route_count, 3);
        assert_eq!(summary.tunnel_state, "running");
        assert!(summary.pending_restart);
        assert!(summary.config_reload_enabled);
        assert_eq!(summary.config_reload_interval_ms, 1000);
        assert_eq!(
            summary.last_config_reload_error.as_deref(),
            Some("failed to parse config")
        );
    }

    #[test]
    fn upstream_health_vm_maps_unknown_health_to_neutral_label() {
        let upstream = UpstreamHealthVm::from(UpstreamHealthEntry {
            upstream_url: "http://127.0.0.1:3000".to_string(),
            health_check_path: "/healthz".to_string(),
            healthy: None,
            last_checked_at: None,
            last_error: None,
        });

        assert_eq!(upstream.health_label, "unknown");
    }

    #[test]
    fn log_tail_vm_preserves_requested_lines_and_order() {
        let log_tail = LogTailVm::from_response(
            200,
            TunnelLogsResponse {
                lines: vec!["first log line".to_string(), "second log line".to_string()],
            },
        );

        assert_eq!(log_tail.requested_lines, 200);
        assert_eq!(
            log_tail.lines,
            vec!["first log line".to_string(), "second log line".to_string(),]
        );
    }
}
