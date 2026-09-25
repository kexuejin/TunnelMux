use super::*;
use tokio::io::AsyncWriteExt;

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

/// Default on-disk location of the auto-generated control-plane API token.
///
/// Derived from the state file rather than hardcoded to `~/.tunnelmux`, so a
/// daemon started with `--data-file /tmp/scratch/state.json` keeps its token at
/// `/tmp/scratch/api-token` instead of overwriting — and invalidating — the
/// token a production daemon on the default path handed out. For the default
/// state file the result is still `~/.tunnelmux/api-token`.
pub(super) fn default_api_token_file(data_file: &Path) -> PathBuf {
    match data_file.parent() {
        Some(directory) if !directory.as_os_str().is_empty() => directory.join("api-token"),
        _ => PathBuf::from("api-token"),
    }
}

fn route_rule_to_create_request(route: RouteRule) -> CreateRouteRequest {
    let health_check_enabled = route_health_check_enabled(&route);
    CreateRouteRequest {
        tunnel_id: route.tunnel_id,
        id: route.id,
        match_host: route.match_host,
        match_path_prefix: route.match_path_prefix,
        strip_path_prefix: route.strip_path_prefix,
        upstream_url: route.upstream_url,
        fallback_upstream_url: route.fallback_upstream_url,
        health_check_path: route.health_check_path,
        health_check_enabled: Some(health_check_enabled),
        enabled: Some(route.enabled),
        forward_host_header: Some(route.forward_host_header),
        rewrite_response_paths: Some(route.rewrite_response_paths),
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

/// Write a declarative config file the way a user would.
///
/// The daemon only ever *reads* `config.json`, so this exists purely to set up
/// fixtures in tests; gating it keeps the production build warning-free.
#[cfg(test)]
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

/// Read the state file for a reader that **cannot** act on a running claim.
///
/// A status that still says `running` was written by a process that is no
/// longer here, so this process cannot honour it: the claim is normalised to
/// `stopped` and the pid is dropped. Anything that merely *reads* state must
/// come through here — adopting a pid this process does not own would make the
/// daemon report a tunnel it has no child for.
///
/// Startup does **not** use this: it can act on the claim, and dropping the pid
/// here is what used to make a leftover provider impossible to reclaim. See
/// [`load_persisted_state_for_reclaim`].
pub(super) async fn load_persisted_state(path: &Path) -> anyhow::Result<PersistedState> {
    let mut parsed = load_persisted_state_for_reclaim(path).await?;
    detach_running_tunnels(&mut parsed);
    Ok(parsed)
}

/// Read the state file exactly as it was written, keeping any claim that a
/// tunnel is running — and the pid that claim names.
///
/// Only startup should use this, and only because it hands the result to
/// `reclaim_leftover_providers`, which decides what happens to that pid. A
/// reader that cannot act on it must use [`load_persisted_state`].
pub(super) async fn load_persisted_state_for_reclaim(
    path: &Path,
) -> anyhow::Result<PersistedState> {
    if !path.exists() {
        return Ok(PersistedState::default());
    }

    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read state file: {}", path.display()))?;
    let mut parsed = parse_persisted_state(&raw)
        .with_context(|| format!("failed to parse state file: {}", path.display()))?;

    migrate_route_access_keys(&mut parsed);
    if parsed.current_tunnel_id.is_none() {
        parsed.current_tunnel_id = Some("primary".to_string());
    }

    Ok(parsed)
}

/// Forget a running claim this process cannot honour, keeping the fact on
/// record so the status is not silently rewritten to look uneventful.
fn detach_running_tunnels(parsed: &mut PersistedState) {
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
        default_route_access: RouteAccessConfig::default(),
        route_access: HashMap::new(),
    })
}

static STATE_TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(super) async fn save_state_file(path: &Path, state: &PersistedState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create state dir: {}", parent.display()))?;
    }

    let raw = serde_json::to_string_pretty(state)?;
    let sequence = STATE_TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_path = path.with_extension(format!("json.tmp-{sequence}"));

    let mut file = fs::File::create(&tmp_path)
        .await
        .with_context(|| format!("failed to create state temp file: {}", tmp_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .await
            .with_context(|| format!("failed to secure state temp file: {}", tmp_path.display()))?;
    }
    file.write_all(format!("{raw}\n").as_bytes())
        .await
        .with_context(|| format!("failed to write state temp file: {}", tmp_path.display()))?;
    file.sync_all()
        .await
        .with_context(|| format!("failed to sync state temp file: {}", tmp_path.display()))?;
    drop(file);

    if let Err(error) = fs::rename(&tmp_path, path).await {
        let _ = fs::remove_file(&tmp_path).await;
        return Err(error).with_context(|| {
            format!(
                "failed to move state temp file {} -> {}",
                tmp_path.display(),
                path.display()
            )
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[tokio::test]
    async fn load_persisted_state_migrates_legacy_route_access_keys() {
        let path = unique_temp_path("legacy-route-access.json");
        fs::write(
            &path,
            r#"{
  "current_tunnel_id": "primary",
  "tunnels": [],
  "routes": [{
    "tunnel_id": "primary",
    "id": "svc-a",
    "match_path_prefix": "/",
    "upstream_url": "http://127.0.0.1:3000",
    "enabled": true
  }],
  "default_route_access": {},
  "route_access": {
    "svc-a": { "require_access_code": "legacy-code" }
  }
}
"#,
        )
        .await
        .expect("state fixture should write");

        let state = load_persisted_state(&path)
            .await
            .expect("state should load");
        assert_eq!(
            state
                .route_access
                .get(&RouteAccessKey::scoped("primary", "svc-a"))
                .and_then(|config| config.require_access_code.as_deref()),
            Some("legacy-code")
        );
        let _ = fs::remove_file(&path).await;
    }

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

    #[tokio::test]
    async fn the_reclaim_loader_keeps_the_pid_a_previous_run_recorded() {
        // The entire point of the split: startup has to see the pid to reclaim
        // the process, and the normalising reader has to not see it. Dropping
        // the pid at load time is what used to make a leftover provider
        // impossible to reclaim — nothing was left to act on.
        let path = unique_temp_path("reclaim-state.json");
        fs::write(
            &path,
            r#"{
  "current_tunnel_id": "primary",
  "tunnels": [
    {
      "id": "primary",
      "status": {
        "state": "running",
        "provider": "cloudflared",
        "target_url": "http://127.0.0.1:48081",
        "public_base_url": "https://leftover.invalid",
        "started_at": "2026-09-17T00:00:00+00:00",
        "updated_at": "2026-09-17T00:00:00+00:00",
        "process_id": 99364,
        "auto_restart": true,
        "restart_count": 0,
        "last_error": null
      }
    }
  ],
  "routes": [],
  "health_check": {"interval_ms": 5000, "timeout_ms": 2000, "path": "/"},
  "default_route_access": {},
  "route_access": {}
}
"#,
        )
        .await
        .expect("state fixture should write");

        let as_written = load_persisted_state_for_reclaim(&path)
            .await
            .expect("state should load");
        assert_eq!(as_written.tunnels[0].status.state, TunnelState::Running);
        assert_eq!(as_written.tunnels[0].status.process_id, Some(99364));

        let for_readers = load_persisted_state(&path)
            .await
            .expect("state should load");
        assert_eq!(for_readers.tunnels[0].status.state, TunnelState::Stopped);
        assert_eq!(for_readers.tunnels[0].status.process_id, None);
        assert_eq!(
            for_readers.tunnels[0].status.last_error.as_deref(),
            Some("daemon restarted; previous tunnel process was detached")
        );

        let _ = fs::remove_file(&path).await;
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("tunnelmuxd-{unique}-{name}"))
    }

    #[test]
    fn api_token_file_sits_next_to_the_state_file() {
        // The default layout must stay exactly where local clients look for it,
        // otherwise auto-discovery breaks.
        assert_eq!(
            default_api_token_file(&default_data_file()),
            default_data_file().with_file_name("api-token")
        );
        // A daemon pointed elsewhere gets its own token file, so starting it
        // cannot rotate the token a production daemon handed out.
        assert_eq!(
            default_api_token_file(Path::new("/tmp/scratch/state.json")),
            PathBuf::from("/tmp/scratch/api-token")
        );
    }

    #[test]
    fn api_token_file_without_a_parent_directory_stays_relative() {
        assert_eq!(
            default_api_token_file(Path::new("state.json")),
            PathBuf::from("api-token")
        );
    }
}
