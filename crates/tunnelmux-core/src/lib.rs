use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_CONTROL_ADDR: &str = "127.0.0.1:4765";
pub const DEFAULT_GATEWAY_TARGET_URL: &str = "http://127.0.0.1:18080";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TunnelProvider {
    Cloudflared,
    Ngrok,
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
    pub provider: TunnelProvider,
    pub target_url: String,
    pub auto_restart: Option<bool>,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelStatusResponse {
    pub tunnel: TunnelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelLogsResponse {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteRule {
    pub id: String,
    pub match_host: Option<String>,
    pub match_path_prefix: Option<String>,
    pub strip_path_prefix: Option<String>,
    pub upstream_url: String,
    pub fallback_upstream_url: Option<String>,
    pub health_check_path: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateRouteRequest {
    pub id: String,
    pub match_host: Option<String>,
    pub match_path_prefix: Option<String>,
    pub strip_path_prefix: Option<String>,
    pub upstream_url: String,
    pub fallback_upstream_url: Option<String>,
    pub health_check_path: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutesResponse {
    pub routes: Vec<RouteRule>,
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
pub struct DeleteRouteResponse {
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorResponse {
    pub error: String,
}
