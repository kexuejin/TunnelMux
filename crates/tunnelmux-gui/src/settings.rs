use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4765";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuiSettings {
    pub base_url: String,
    pub token: Option<String>,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            token: None,
        }
    }
}

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load_settings_from_dir(config_dir: &Path) -> anyhow::Result<GuiSettings> {
    let path = settings_path(config_dir);
    if !path.exists() {
        return Ok(GuiSettings::default());
    }

    let raw = std::fs::read_to_string(&path)
        .map_err(|error| anyhow::anyhow!(error))
        .map_err(|error| anyhow::anyhow!("failed to read settings file {}: {error}", path.display()))?;
    let mut settings: GuiSettings = serde_json::from_str(&raw)
        .map_err(|error| anyhow::anyhow!("failed to parse settings file {}: {error}", path.display()))?;

    settings.base_url = normalize_base_url(&settings.base_url);
    settings.token = normalize_token(settings.token);
    Ok(settings)
}

pub fn save_settings_to_dir(config_dir: &Path, settings: &GuiSettings) -> anyhow::Result<()> {
    std::fs::create_dir_all(config_dir)
        .map_err(|error| anyhow::anyhow!("failed to create settings directory {}: {error}", config_dir.display()))?;
    let path = settings_path(config_dir);
    let mut normalized = settings.clone();
    normalized.base_url = normalize_base_url(&normalized.base_url);
    normalized.token = normalize_token(normalized.token);

    let raw = serde_json::to_string_pretty(&normalized)
        .map_err(|error| anyhow::anyhow!("failed to serialize settings: {error}"))?;
    std::fs::write(&path, format!("{raw}\n"))
        .map_err(|error| anyhow::anyhow!("failed to write settings file {}: {error}", path.display()))?;
    Ok(())
}

fn normalize_base_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return DEFAULT_BASE_URL.to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    format!("http://{trimmed}")
}

fn normalize_token(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn load_settings_returns_default_base_url_when_missing() {
        let temp_dir = prepare_temp_dir();

        let settings =
            load_settings_from_dir(&temp_dir).expect("missing settings should load defaults");

        assert_eq!(settings.base_url, DEFAULT_BASE_URL);
        assert_eq!(settings.token, None);
    }

    #[test]
    fn save_and_reload_settings_round_trips_token() {
        let temp_dir = prepare_temp_dir();
        let expected = GuiSettings {
            base_url: "http://127.0.0.1:9999".to_string(),
            token: Some("dev-token".to_string()),
        };

        save_settings_to_dir(&temp_dir, &expected).expect("settings should save");
        let loaded = load_settings_from_dir(&temp_dir).expect("saved settings should reload");

        assert_eq!(loaded, expected);
    }

    fn prepare_temp_dir() -> PathBuf {
        let path = next_temp_dir();
        if path.exists() {
            std::fs::remove_dir_all(&path).expect("stale temp dir should be removed");
        }
        std::fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn next_temp_dir() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "tunnelmux-gui-settings-{}",
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
