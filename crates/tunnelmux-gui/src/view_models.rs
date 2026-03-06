use serde::{Deserialize, Serialize};
use tunnelmux_core::{CreateRouteRequest, RouteRule};

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
