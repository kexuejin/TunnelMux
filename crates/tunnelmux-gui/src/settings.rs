use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tunnelmux_core::TunnelProvider;

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4765";
pub const DEFAULT_GUI_GATEWAY_TARGET_URL: &str = "http://127.0.0.1:48080";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct GuiSettings {
    pub base_url: String,
    pub token: Option<String>,
    pub default_provider: TunnelProvider,
    pub gateway_target_url: String,
    pub auto_restart: bool,
    pub ngrok_authtoken: Option<String>,
    pub ngrok_domain: Option<String>,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            token: None,
            default_provider: TunnelProvider::Cloudflared,
            gateway_target_url: DEFAULT_GUI_GATEWAY_TARGET_URL.to_string(),
            auto_restart: true,
            ngrok_authtoken: None,
            ngrok_domain: None,
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
        .map_err(|error| {
            anyhow::anyhow!("failed to read settings file {}: {error}", path.display())
        })?;
    let mut settings: GuiSettings = serde_json::from_str(&raw).map_err(|error| {
        anyhow::anyhow!("failed to parse settings file {}: {error}", path.display())
    })?;

    settings.base_url = normalize_base_url(&settings.base_url);
    settings.gateway_target_url = normalize_base_url(&settings.gateway_target_url);
    settings.token = normalize_token(settings.token);
    settings.ngrok_authtoken = normalize_token(settings.ngrok_authtoken);
    settings.ngrok_domain = normalize_token(settings.ngrok_domain);
    Ok(settings)
}

pub fn save_settings_to_dir(config_dir: &Path, settings: &GuiSettings) -> anyhow::Result<()> {
    std::fs::create_dir_all(config_dir).map_err(|error| {
        anyhow::anyhow!(
            "failed to create settings directory {}: {error}",
            config_dir.display()
        )
    })?;
    let path = settings_path(config_dir);
    let mut normalized = settings.clone();
    normalized.base_url = normalize_base_url(&normalized.base_url);
    normalized.gateway_target_url = normalize_base_url(&normalized.gateway_target_url);
    normalized.token = normalize_token(normalized.token);
    normalized.ngrok_authtoken = normalize_token(normalized.ngrok_authtoken);
    normalized.ngrok_domain = normalize_token(normalized.ngrok_domain);

    let raw = serde_json::to_string_pretty(&normalized)
        .map_err(|error| anyhow::anyhow!("failed to serialize settings: {error}"))?;
    std::fs::write(&path, format!("{raw}\n")).map_err(|error| {
        anyhow::anyhow!("failed to write settings file {}: {error}", path.display())
    })?;
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
        assert_eq!(settings.default_provider, TunnelProvider::Cloudflared);
        assert_eq!(settings.gateway_target_url, DEFAULT_GUI_GATEWAY_TARGET_URL);
        assert!(settings.auto_restart);
        assert_eq!(settings.ngrok_authtoken, None);
        assert_eq!(settings.ngrok_domain, None);
    }

    #[test]
    fn save_and_reload_settings_round_trips_token() {
        let temp_dir = prepare_temp_dir();
        let expected = GuiSettings {
            base_url: "http://127.0.0.1:9999".to_string(),
            token: Some("dev-token".to_string()),
            default_provider: TunnelProvider::Ngrok,
            gateway_target_url: "127.0.0.1:28080".to_string(),
            auto_restart: false,
            ngrok_authtoken: Some("ngrok-token".to_string()),
            ngrok_domain: Some("demo.ngrok.app".to_string()),
        };

        save_settings_to_dir(&temp_dir, &expected).expect("settings should save");
        let loaded = load_settings_from_dir(&temp_dir).expect("saved settings should reload");

        assert_eq!(
            loaded,
            GuiSettings {
                gateway_target_url: "http://127.0.0.1:28080".to_string(),
                ..expected
            }
        );
    }

    #[test]
    fn load_settings_backfills_new_fields_from_old_schema() {
        let temp_dir = prepare_temp_dir();
        let path = settings_path(&temp_dir);
        std::fs::write(
            &path,
            "{\n  \"base_url\": \"127.0.0.1:8765\",\n  \"token\": \"legacy-token\"\n}\n",
        )
        .expect("legacy settings should write");

        let loaded = load_settings_from_dir(&temp_dir).expect("legacy settings should load");

        assert_eq!(loaded.base_url, "http://127.0.0.1:8765");
        assert_eq!(loaded.token.as_deref(), Some("legacy-token"));
        assert_eq!(loaded.default_provider, TunnelProvider::Cloudflared);
        assert_eq!(loaded.gateway_target_url, DEFAULT_GUI_GATEWAY_TARGET_URL);
        assert!(loaded.auto_restart);
        assert_eq!(loaded.ngrok_authtoken, None);
        assert_eq!(loaded.ngrok_domain, None);
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
