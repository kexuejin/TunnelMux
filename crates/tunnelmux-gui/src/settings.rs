use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tunnelmux_core::TunnelProvider;

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4765";
pub const DEFAULT_GUI_GATEWAY_TARGET_URL: &str = "http://127.0.0.1:48080";
pub const DEFAULT_TUNNEL_NAME: &str = "Main Tunnel";
pub const DEFAULT_TUNNEL_ID: &str = "primary";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TunnelProfileSettings {
    pub id: String,
    pub name: String,
    pub provider: TunnelProvider,
    pub gateway_target_url: String,
    pub auto_restart: bool,
    pub cloudflared_tunnel_token: Option<String>,
    pub ngrok_authtoken: Option<String>,
    pub ngrok_domain: Option<String>,
}

impl Default for TunnelProfileSettings {
    fn default() -> Self {
        Self {
            id: DEFAULT_TUNNEL_ID.to_string(),
            name: DEFAULT_TUNNEL_NAME.to_string(),
            provider: TunnelProvider::Cloudflared,
            gateway_target_url: DEFAULT_GUI_GATEWAY_TARGET_URL.to_string(),
            auto_restart: true,
            cloudflared_tunnel_token: None,
            ngrok_authtoken: None,
            ngrok_domain: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct GuiSettings {
    pub base_url: String,
    pub token: Option<String>,
    pub current_tunnel_id: Option<String>,
    pub tunnels: Vec<TunnelProfileSettings>,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            token: None,
            current_tunnel_id: None,
            tunnels: Vec::new(),
        }
    }
}

impl GuiSettings {
    pub fn current_tunnel(&self) -> Option<&TunnelProfileSettings> {
        self.current_tunnel_id
            .as_deref()
            .and_then(|id| self.tunnels.iter().find(|tunnel| tunnel.id == id))
            .or_else(|| self.tunnels.first())
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
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        anyhow::anyhow!("failed to parse settings file {}: {error}", path.display())
    })?;
    let mut settings: GuiSettings = serde_json::from_value(value.clone()).map_err(|error| {
        anyhow::anyhow!("failed to parse settings file {}: {error}", path.display())
    })?;

    settings.base_url = normalize_base_url(&settings.base_url);
    settings.token = normalize_token(settings.token);
    settings.current_tunnel_id = normalize_token(settings.current_tunnel_id);
    settings.tunnels = normalize_tunnel_profiles(settings.tunnels);
    if settings.tunnels.is_empty() {
        if let Some(legacy) = migrate_legacy_tunnel_profile(&value) {
            settings.current_tunnel_id = Some(legacy.id.clone());
            settings.tunnels = vec![legacy];
        }
    } else if settings
        .current_tunnel_id
        .as_deref()
        .map(|id| settings.tunnels.iter().any(|tunnel| tunnel.id == id))
        != Some(true)
    {
        settings.current_tunnel_id = settings.tunnels.first().map(|tunnel| tunnel.id.clone());
    }
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
    normalized.token = normalize_token(normalized.token);
    normalized.current_tunnel_id = normalize_token(normalized.current_tunnel_id);
    normalized.tunnels = normalize_tunnel_profiles(normalized.tunnels);
    if normalized
        .current_tunnel_id
        .as_deref()
        .map(|id| normalized.tunnels.iter().any(|tunnel| tunnel.id == id))
        != Some(true)
    {
        normalized.current_tunnel_id = normalized.tunnels.first().map(|tunnel| tunnel.id.clone());
    }

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

fn normalize_tunnel_profiles(profiles: Vec<TunnelProfileSettings>) -> Vec<TunnelProfileSettings> {
    profiles
        .into_iter()
        .filter_map(|profile| {
            let id = normalize_token(Some(profile.id))?;
            let name = normalize_token(Some(profile.name))
                .unwrap_or_else(|| DEFAULT_TUNNEL_NAME.to_string());
            Some(TunnelProfileSettings {
                id,
                name,
                provider: profile.provider,
                gateway_target_url: normalize_base_url(&profile.gateway_target_url),
                auto_restart: profile.auto_restart,
                cloudflared_tunnel_token: normalize_token(profile.cloudflared_tunnel_token),
                ngrok_authtoken: normalize_token(profile.ngrok_authtoken),
                ngrok_domain: normalize_token(profile.ngrok_domain),
            })
        })
        .collect()
}

fn migrate_legacy_tunnel_profile(value: &serde_json::Value) -> Option<TunnelProfileSettings> {
    let object = value.as_object()?;
    let has_legacy_tunnel_fields = object.contains_key("tunnel_name")
        || object.contains_key("default_provider")
        || object.contains_key("gateway_target_url")
        || object.contains_key("auto_restart")
        || object.contains_key("cloudflared_tunnel_token")
        || object.contains_key("ngrok_authtoken")
        || object.contains_key("ngrok_domain")
        || object.contains_key("token");

    if !has_legacy_tunnel_fields {
        return None;
    }

    let provider = object
        .get("default_provider")
        .cloned()
        .and_then(|item| serde_json::from_value(item).ok())
        .unwrap_or(TunnelProvider::Cloudflared);

    Some(TunnelProfileSettings {
        id: DEFAULT_TUNNEL_ID.to_string(),
        name: object
            .get("tunnel_name")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .unwrap_or(DEFAULT_TUNNEL_NAME)
            .to_string(),
        provider,
        gateway_target_url: normalize_base_url(
            object
                .get("gateway_target_url")
                .and_then(|item| item.as_str())
                .unwrap_or(DEFAULT_GUI_GATEWAY_TARGET_URL),
        ),
        auto_restart: object
            .get("auto_restart")
            .and_then(|item| item.as_bool())
            .unwrap_or(true),
        cloudflared_tunnel_token: normalize_token(
            object
                .get("cloudflared_tunnel_token")
                .and_then(|item| item.as_str())
                .map(ToString::to_string),
        ),
        ngrok_authtoken: normalize_token(
            object
                .get("ngrok_authtoken")
                .and_then(|item| item.as_str())
                .map(ToString::to_string),
        ),
        ngrok_domain: normalize_token(
            object
                .get("ngrok_domain")
                .and_then(|item| item.as_str())
                .map(ToString::to_string),
        ),
    })
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
        assert_eq!(settings.current_tunnel_id, None);
        assert!(settings.tunnels.is_empty());
    }

    #[test]
    fn save_and_reload_settings_round_trips_token() {
        let temp_dir = prepare_temp_dir();
        let expected = GuiSettings {
            base_url: "http://127.0.0.1:9999".to_string(),
            token: Some("dev-token".to_string()),
            current_tunnel_id: Some("main".to_string()),
            tunnels: vec![TunnelProfileSettings {
                id: "main".to_string(),
                name: "Main Tunnel".to_string(),
                provider: TunnelProvider::Ngrok,
                gateway_target_url: "127.0.0.1:28080".to_string(),
                auto_restart: false,
                cloudflared_tunnel_token: Some("cf-token".to_string()),
                ngrok_authtoken: Some("ngrok-token".to_string()),
                ngrok_domain: Some("demo.ngrok.app".to_string()),
            }],
        };

        save_settings_to_dir(&temp_dir, &expected).expect("settings should save");
        let loaded = load_settings_from_dir(&temp_dir).expect("saved settings should reload");

        assert_eq!(
            loaded,
            GuiSettings {
                tunnels: vec![TunnelProfileSettings {
                    gateway_target_url: "http://127.0.0.1:28080".to_string(),
                    ..expected.tunnels[0].clone()
                }],
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
        assert_eq!(loaded.current_tunnel_id.as_deref(), Some(DEFAULT_TUNNEL_ID));
        assert_eq!(loaded.tunnels.len(), 1);
        assert_eq!(loaded.tunnels[0].name, DEFAULT_TUNNEL_NAME);
        assert_eq!(loaded.tunnels[0].provider, TunnelProvider::Cloudflared);
        assert_eq!(
            loaded.tunnels[0].gateway_target_url,
            DEFAULT_GUI_GATEWAY_TARGET_URL
        );
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
