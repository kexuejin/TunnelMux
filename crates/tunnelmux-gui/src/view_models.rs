use serde::{Deserialize, Serialize};
use tunnelmux_core::{
    CreateRouteRequest, DiagnosticsResponse, RouteRule, TunnelLogsResponse, TunnelState,
    UpstreamHealthEntry,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteViewModel {
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
    pub fn into_create_request(&self) -> CreateRouteRequest {
        CreateRouteRequest {
            id: self.id.clone(),
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
    use tunnelmux_core::{DiagnosticsResponse, TunnelLogsResponse, TunnelState, UpstreamHealthEntry};

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
                lines: vec![
                    "first log line".to_string(),
                    "second log line".to_string(),
                ],
            },
        );

        assert_eq!(log_tail.requested_lines, 200);
        assert_eq!(
            log_tail.lines,
            vec![
                "first log line".to_string(),
                "second log line".to_string(),
            ]
        );
    }
}
