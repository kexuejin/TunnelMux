use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct DeclarativeConfigFile {
    pub routes: Vec<RouteRule>,
    pub health_check: Option<HealthCheckSettings>,
}

pub(super) fn default_data_file() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".tunnelmux").join("state.json");
    }
    PathBuf::from("./data/state.json")
}

pub(super) fn default_config_file() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".tunnelmux").join("config.json");
    }
    PathBuf::from("./data/config.json")
}

pub(super) fn default_provider_log_file() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".tunnelmux").join("provider.log");
    }
    PathBuf::from("./data/provider.log")
}

fn route_rule_to_create_request(route: RouteRule) -> CreateRouteRequest {
    CreateRouteRequest {
        tunnel_id: route.tunnel_id,
        id: route.id,
        match_host: route.match_host,
        match_path_prefix: route.match_path_prefix,
        strip_path_prefix: route.strip_path_prefix,
        upstream_url: route.upstream_url,
        fallback_upstream_url: route.fallback_upstream_url,
        health_check_path: route.health_check_path,
        enabled: Some(route.enabled),
    }
}

fn normalize_declarative_config(
    config: DeclarativeConfigFile,
) -> anyhow::Result<DeclarativeConfigFile> {
    let mut routes = Vec::with_capacity(config.routes.len());
    for route in config.routes {
        let normalized = normalize_route_request(route_rule_to_create_request(route))
            .map_err(|err| anyhow!(err.message))?;
        routes.push(normalized);
    }
    ensure_unique_route_ids(&routes).map_err(|err| anyhow!(err.message))?;

    let health_check = match config.health_check {
        Some(settings) => Some(HealthCheckSettings {
            interval_ms: normalize_health_check_interval_ms(settings.interval_ms)?,
            timeout_ms: normalize_health_check_timeout_ms(settings.timeout_ms)?,
            path: normalize_health_check_path(&settings.path)?,
        }),
        None => None,
    };

    Ok(DeclarativeConfigFile {
        routes,
        health_check,
    })
}

pub(super) async fn load_config_file(path: &Path) -> anyhow::Result<Option<DeclarativeConfigFile>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    let parsed: DeclarativeConfigFile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse config file: {}", path.display()))?;
    Ok(Some(normalize_declarative_config(parsed)?))
}

pub(super) async fn save_config_file(
    path: &Path,
    config: &DeclarativeConfigFile,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
    }

    let raw = serde_json::to_string_pretty(config)?;
    let tmp_path = path.with_extension("json.tmp");

    fs::write(&tmp_path, format!("{raw}\n"))
        .await
        .with_context(|| format!("failed to write config temp file: {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path).await.with_context(|| {
        format!(
            "failed to move config temp file {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

pub(super) async fn load_persisted_state(path: &Path) -> anyhow::Result<PersistedState> {
    if !path.exists() {
        return Ok(PersistedState::default());
    }

    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read state file: {}", path.display()))?;
    let mut parsed = parse_persisted_state(&raw)
        .with_context(|| format!("failed to parse state file: {}", path.display()))?;

    if parsed.current_tunnel_id.is_none() {
        parsed.current_tunnel_id = Some("primary".to_string());
    }

    for tunnel in &mut parsed.tunnels {
        if matches!(
            tunnel.status.state,
            TunnelState::Running | TunnelState::Starting
        ) {
            tunnel.status.state = TunnelState::Stopped;
            tunnel.status.process_id = None;
            tunnel.status.last_error =
                Some("daemon restarted; previous tunnel process was detached".to_string());
            tunnel.status.updated_at = now_iso();
        }
    }

    Ok(parsed)
}

fn parse_persisted_state(raw: &str) -> anyhow::Result<PersistedState> {
    match serde_json::from_str::<PersistedState>(raw) {
        Ok(parsed) => Ok(parsed),
        Err(primary_error) => {
            legacy_persisted_state_to_current(raw).ok_or_else(|| primary_error.into())
        }
    }
}

fn legacy_persisted_state_to_current(raw: &str) -> Option<PersistedState> {
    #[derive(Debug, Deserialize)]
    struct LegacyPersistedState {
        tunnel: Option<TunnelStatus>,
        #[serde(default)]
        routes: Vec<RouteRule>,
        health_check: Option<HealthCheckSettings>,
    }

    let legacy = serde_json::from_str::<LegacyPersistedState>(raw).ok()?;
    Some(PersistedState {
        current_tunnel_id: Some("primary".to_string()),
        tunnels: vec![PersistedTunnelState {
            id: "primary".to_string(),
            status: legacy
                .tunnel
                .unwrap_or_else(|| default_tunnel_status(TunnelState::Idle)),
        }],
        routes: legacy.routes,
        health_check: legacy.health_check,
    })
}

pub(super) async fn save_state_file(path: &Path, state: &PersistedState) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[tokio::test]
    async fn load_persisted_state_migrates_legacy_single_tunnel_shape() {
        let path = unique_temp_path("legacy-state.json");
        fs::write(
            &path,
            r#"{
  "tunnel": {
    "state": "running",
    "provider": "cloudflared",
    "target_url": "http://127.0.0.1:48080",
    "public_base_url": "https://example.trycloudflare.com",
    "started_at": "2026-03-07T11:56:10.037636+00:00",
    "updated_at": "2026-03-07T11:56:10.037641+00:00",
    "process_id": 99364,
    "auto_restart": true,
    "restart_count": 0,
    "last_error": null
  },
  "routes": [],
  "health_check": {
    "interval_ms": 5000,
    "timeout_ms": 2000,
    "path": "/"
  }
}
"#,
        )
        .await
        .expect("legacy state fixture should write");

        let persisted = load_persisted_state(&path)
            .await
            .expect("legacy state should load");

        assert_eq!(persisted.current_tunnel_id.as_deref(), Some("primary"));
        assert_eq!(persisted.tunnels.len(), 1);
        assert_eq!(persisted.tunnels[0].id, "primary");
        assert_eq!(
            persisted.tunnels[0].status.provider,
            Some(TunnelProvider::Cloudflared)
        );
        assert_eq!(
            persisted.tunnels[0].status.target_url.as_deref(),
            Some("http://127.0.0.1:48080")
        );
        assert_eq!(persisted.tunnels[0].status.state, TunnelState::Stopped);
        assert_eq!(persisted.tunnels[0].status.process_id, None);
        assert_eq!(
            persisted.tunnels[0].status.last_error.as_deref(),
            Some("daemon restarted; previous tunnel process was detached")
        );
        assert_eq!(
            persisted.health_check,
            Some(HealthCheckSettings {
                interval_ms: 5000,
                timeout_ms: 2000,
                path: "/".to_string(),
            })
        );

        let _ = fs::remove_file(&path).await;
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("tunnelmuxd-{unique}-{name}"))
    }
}
