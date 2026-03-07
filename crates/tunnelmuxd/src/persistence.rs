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
