use super::*;

pub(super) fn default_data_file() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".tunnelmux").join("state.json");
    }
    PathBuf::from("./data/state.json")
}

pub(super) fn default_provider_log_file() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".tunnelmux").join("provider.log");
    }
    PathBuf::from("./data/provider.log")
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
