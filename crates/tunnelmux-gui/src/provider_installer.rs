use anyhow::{Context, anyhow};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tar::Archive;
use tunnelmux_core::TunnelProvider;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderInstallSource {
    LocalTools,
    SystemPath,
    Missing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderInstallState {
    Idle,
    Downloading,
    Installed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderInstallStatus {
    pub state: ProviderInstallState,
    pub source: ProviderInstallSource,
    pub resolved_path: Option<String>,
    pub version: Option<String>,
    pub last_error: Option<String>,
}

impl Default for ProviderInstallStatus {
    fn default() -> Self {
        Self {
            state: ProviderInstallState::Idle,
            source: ProviderInstallSource::Missing,
            resolved_path: None,
            version: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderInstallManifestEntry {
    pub provider: TunnelProvider,
    pub version: String,
    pub binary_name: String,
    pub archive_name: String,
    pub download_url: String,
    pub sha256: String,
}

pub fn tools_root_from_base_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("tools")
}

pub fn provider_install_statuses_path(base_dir: &Path) -> PathBuf {
    base_dir.join("provider-install-statuses.json")
}

pub fn load_provider_install_statuses(
    base_dir: &Path,
) -> anyhow::Result<HashMap<String, ProviderInstallStatus>> {
    let path = provider_install_statuses_path(base_dir);
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn save_provider_install_statuses(
    base_dir: &Path,
    statuses: &HashMap<String, ProviderInstallStatus>,
) -> anyhow::Result<()> {
    let path = provider_install_statuses_path(base_dir);

    if statuses.is_empty() {
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
        return Ok(());
    }

    fs::create_dir_all(base_dir)
        .with_context(|| format!("failed to create {}", base_dir.display()))?;
    let raw = serde_json::to_string_pretty(statuses)
        .context("failed to serialize provider install statuses")?;
    fs::write(&path, format!("{raw}\n"))
        .with_context(|| format!("failed to write {}", path.display()))
}

pub fn provider_manifest_entry_for_current_platform(
    provider: &TunnelProvider,
) -> Option<ProviderInstallManifestEntry> {
    current_platform_manifest_entries()
        .into_iter()
        .find(|entry| entry.provider == *provider)
}

pub fn provider_binary_name(provider: &TunnelProvider) -> &'static str {
    match provider {
        TunnelProvider::Cloudflared => "cloudflared",
        TunnelProvider::Ngrok => "ngrok",
    }
}

fn current_cloudflared_version() -> &'static str {
    "2026.2.0"
}

fn current_ngrok_version() -> &'static str {
    "3.37.1"
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn current_platform_manifest_entries() -> Vec<ProviderInstallManifestEntry> {
    vec![
        ProviderInstallManifestEntry {
            provider: TunnelProvider::Cloudflared,
            version: current_cloudflared_version().to_string(),
            binary_name: "cloudflared".to_string(),
            archive_name: "cloudflared-darwin-arm64.tgz".to_string(),
            download_url: "https://github.com/cloudflare/cloudflared/releases/download/2026.2.0/cloudflared-darwin-arm64.tgz".to_string(),
            sha256: "ba99c6f87320236b9f842c3ba4b9526f687560125b7b43a581201579543ca4ff".to_string(),
        },
        ProviderInstallManifestEntry {
            provider: TunnelProvider::Ngrok,
            version: current_ngrok_version().to_string(),
            binary_name: "ngrok".to_string(),
            archive_name: "ngrok-v3-3.37.1-darwin-arm64.tar.gz".to_string(),
            download_url: "https://bin.equinox.io/a/6Z3aazwsicH/ngrok-v3-3.37.1-darwin-arm64.tar.gz".to_string(),
            sha256: "bf9e2967846156851e51c8e66460914161a08edb4f3eda8de90e00758f9d5876".to_string(),
        },
    ]
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn current_platform_manifest_entries() -> Vec<ProviderInstallManifestEntry> {
    vec![
        ProviderInstallManifestEntry {
            provider: TunnelProvider::Cloudflared,
            version: current_cloudflared_version().to_string(),
            binary_name: "cloudflared".to_string(),
            archive_name: "cloudflared-darwin-amd64.tgz".to_string(),
            download_url: "https://github.com/cloudflare/cloudflared/releases/download/2026.2.0/cloudflared-darwin-amd64.tgz".to_string(),
            sha256: "685688a260c324eb8d9c9434ca22f0ce4f504fd6acd0706787c4833de8d6eb17".to_string(),
        },
        ProviderInstallManifestEntry {
            provider: TunnelProvider::Ngrok,
            version: current_ngrok_version().to_string(),
            binary_name: "ngrok".to_string(),
            archive_name: "ngrok-v3-3.37.1-darwin-amd64.tar.gz".to_string(),
            download_url: "https://bin.equinox.io/a/7Td44DVhb6L/ngrok-v3-3.37.1-darwin-amd64.tar.gz".to_string(),
            sha256: "37b905e1e29a2e89b6fdb3f720d1f5c8bfb0a9f12479f0937cda23dbbd6b6548".to_string(),
        },
    ]
}

#[cfg(not(target_os = "macos"))]
fn current_platform_manifest_entries() -> Vec<ProviderInstallManifestEntry> {
    Vec::new()
}

pub async fn download_provider_archive_bytes(
    manifest: &ProviderInstallManifestEntry,
) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .build()
        .context("failed to build provider download client")?;

    let response = client
        .get(&manifest.download_url)
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .with_context(|| format!("failed to download {}", manifest.download_url))?
        .error_for_status()
        .with_context(|| {
            format!(
                "provider download returned an error for {}",
                manifest.download_url
            )
        })?;

    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .with_context(|| format!("failed to read {}", manifest.download_url))
}

pub fn stable_provider_binary_path(tools_root: &Path, provider: &TunnelProvider) -> PathBuf {
    tools_root.join("bin").join(provider_binary_name(provider))
}

pub fn versioned_provider_binary_path(
    tools_root: &Path,
    manifest: &ProviderInstallManifestEntry,
) -> PathBuf {
    tools_root
        .join(provider_binary_name(&manifest.provider))
        .join(&manifest.version)
        .join(&manifest.binary_name)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

pub fn install_provider_from_bytes(
    tools_root: &Path,
    manifest: &ProviderInstallManifestEntry,
    archive_bytes: &[u8],
) -> anyhow::Result<ProviderInstallStatus> {
    let actual_sha256 = sha256_hex(archive_bytes);
    if actual_sha256 != manifest.sha256.to_ascii_lowercase() {
        return Err(anyhow!(
            "download checksum mismatch for {}",
            manifest.binary_name
        ));
    }

    let temp_root = tools_root.join(".tmp");
    fs::create_dir_all(&temp_root)
        .with_context(|| format!("failed to create {}", temp_root.display()))?;

    let temp_version_dir = temp_root.join(format!(
        "{}-{}-{}",
        provider_binary_name(&manifest.provider),
        manifest.version,
        unique_suffix()
    ));
    if temp_version_dir.exists() {
        fs::remove_dir_all(&temp_version_dir)
            .with_context(|| format!("failed to remove {}", temp_version_dir.display()))?;
    }
    fs::create_dir_all(&temp_version_dir)
        .with_context(|| format!("failed to create {}", temp_version_dir.display()))?;

    let extracted_binary = extract_provider_binary(manifest, archive_bytes, &temp_version_dir)
        .with_context(|| {
            format!(
                "failed to extract {} from {}",
                manifest.binary_name, manifest.archive_name
            )
        })?;

    let versioned_binary_path = versioned_provider_binary_path(tools_root, manifest);
    let versioned_dir = versioned_binary_path
        .parent()
        .expect("versioned binary path should have a parent")
        .to_path_buf();
    if versioned_dir.exists() {
        fs::remove_dir_all(&versioned_dir)
            .with_context(|| format!("failed to remove {}", versioned_dir.display()))?;
    }
    if let Some(parent) = versioned_dir.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::rename(&temp_version_dir, &versioned_dir)
        .with_context(|| format!("failed to promote {}", versioned_dir.display()))?;

    let stable_path = stable_provider_binary_path(tools_root, &manifest.provider);
    if let Some(parent) = stable_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let stable_temp_path = stable_path.with_extension("tmp");
    if stable_temp_path.exists() {
        let _ = fs::remove_file(&stable_temp_path);
    }
    fs::copy(&versioned_binary_path, &stable_temp_path).with_context(|| {
        format!(
            "failed to copy {} to {}",
            versioned_binary_path.display(),
            stable_temp_path.display()
        )
    })?;
    ensure_executable(&stable_temp_path)?;
    if stable_path.exists() {
        fs::remove_file(&stable_path)
            .with_context(|| format!("failed to replace {}", stable_path.display()))?;
    }
    fs::rename(&stable_temp_path, &stable_path)
        .with_context(|| format!("failed to promote {}", stable_path.display()))?;

    let _ = extracted_binary;

    Ok(ProviderInstallStatus {
        state: ProviderInstallState::Installed,
        source: ProviderInstallSource::LocalTools,
        resolved_path: Some(stable_path.display().to_string()),
        version: Some(manifest.version.clone()),
        last_error: None,
    })
}

fn extract_provider_binary(
    manifest: &ProviderInstallManifestEntry,
    archive_bytes: &[u8],
    destination_dir: &Path,
) -> anyhow::Result<PathBuf> {
    let target_path = destination_dir.join(&manifest.binary_name);

    if manifest.archive_name.ends_with(".tgz") || manifest.archive_name.ends_with(".tar.gz") {
        let cursor = Cursor::new(archive_bytes);
        let decoder = GzDecoder::new(cursor);
        let mut archive = Archive::new(decoder);

        for entry in archive
            .entries()
            .context("failed to read archive entries")?
        {
            let mut entry = entry.context("failed to read archive entry")?;
            let path = entry.path().context("failed to read archive path")?;
            if path.file_name() == Some(OsStr::new(&manifest.binary_name)) {
                entry
                    .unpack(&target_path)
                    .with_context(|| format!("failed to unpack {}", target_path.display()))?;
                ensure_executable(&target_path)?;
                return Ok(target_path);
            }
        }

        return Err(anyhow!(
            "archive did not contain expected binary {}",
            manifest.binary_name
        ));
    }

    fs::write(&target_path, archive_bytes)
        .with_context(|| format!("failed to write {}", target_path.display()))?;
    ensure_executable(&target_path)?;
    Ok(target_path)
}

fn ensure_executable(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .with_context(|| format!("failed to stat {}", path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("failed to chmod {}", path.display()))?;
    }

    Ok(())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current time should be after epoch")
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::path::Path;
    use tar::{Builder, Header};

    #[test]
    fn tools_root_from_base_dir_appends_tools() {
        let root = tools_root_from_base_dir(Path::new("/tmp/tunnelmux"));

        assert_eq!(root, Path::new("/tmp/tunnelmux").join("tools"));
    }

    #[test]
    fn manifest_lookup_for_supported_providers() {
        let cloudflared =
            provider_manifest_entry_for_current_platform(&TunnelProvider::Cloudflared);
        let ngrok = provider_manifest_entry_for_current_platform(&TunnelProvider::Ngrok);

        if cfg!(target_os = "macos") {
            let cloudflared = cloudflared.expect("cloudflared manifest entry should exist");
            let ngrok = ngrok.expect("ngrok manifest entry should exist");

            assert_eq!(cloudflared.provider, TunnelProvider::Cloudflared);
            assert_eq!(ngrok.provider, TunnelProvider::Ngrok);
            assert!(!cloudflared.version.is_empty());
            assert!(!ngrok.version.is_empty());
        } else {
            assert_eq!(cloudflared, None);
            assert_eq!(ngrok, None);
        }
    }

    #[test]
    fn default_install_status_is_idle() {
        let status = ProviderInstallStatus::default();

        assert_eq!(status.state, ProviderInstallState::Idle);
        assert_eq!(status.last_error, None);
    }

    #[test]
    fn checksum_mismatch_does_not_promote_binary() {
        let temp_dir = tempfile::tempdir().expect("temp dir should exist");
        let tools_root = tools_root_from_base_dir(temp_dir.path());
        let archive = build_test_provider_archive("cloudflared", b"#!/bin/sh\nexit 0\n");
        let manifest = ProviderInstallManifestEntry {
            provider: TunnelProvider::Cloudflared,
            version: "test-version".to_string(),
            binary_name: "cloudflared".to_string(),
            archive_name: "cloudflared-darwin-arm64.tgz".to_string(),
            download_url: "https://example.invalid/cloudflared.tgz".to_string(),
            sha256: "bad-checksum".to_string(),
        };

        let error = install_provider_from_bytes(&tools_root, &manifest, &archive)
            .expect_err("checksum mismatch should fail");

        assert!(error.to_string().contains("checksum"));
        assert!(!stable_provider_binary_path(&tools_root, &TunnelProvider::Cloudflared).exists());
        assert!(!versioned_provider_binary_path(&tools_root, &manifest).exists());
    }

    #[test]
    fn successful_install_promotes_versioned_binary_to_stable_path() {
        let temp_dir = tempfile::tempdir().expect("temp dir should exist");
        let tools_root = tools_root_from_base_dir(temp_dir.path());
        let archive = build_test_provider_archive("ngrok", b"#!/bin/sh\nexit 0\n");
        let manifest = ProviderInstallManifestEntry {
            provider: TunnelProvider::Ngrok,
            version: "test-version".to_string(),
            binary_name: "ngrok".to_string(),
            archive_name: "ngrok-v3-stable-darwin-arm64.tar.gz".to_string(),
            download_url: "https://example.invalid/ngrok.tar.gz".to_string(),
            sha256: sha256_hex(&archive),
        };

        let status = install_provider_from_bytes(&tools_root, &manifest, &archive)
            .expect("install should succeed");

        assert_eq!(status.state, ProviderInstallState::Installed);
        assert_eq!(status.source, ProviderInstallSource::LocalTools);
        assert!(stable_provider_binary_path(&tools_root, &TunnelProvider::Ngrok).exists());
        assert!(versioned_provider_binary_path(&tools_root, &manifest).exists());
    }

    #[test]
    fn provider_install_statuses_round_trip_to_disk() {
        let temp_dir = tempfile::tempdir().expect("temp dir should exist");
        let statuses = std::collections::HashMap::from([(
            "cloudflared".to_string(),
            ProviderInstallStatus {
                state: ProviderInstallState::Failed,
                source: ProviderInstallSource::Missing,
                resolved_path: None,
                version: Some("2026.2.0".to_string()),
                last_error: Some("download exploded".to_string()),
            },
        )]);

        save_provider_install_statuses(temp_dir.path(), &statuses)
            .expect("provider install statuses should save");
        let loaded = load_provider_install_statuses(temp_dir.path())
            .expect("provider install statuses should load");

        assert_eq!(loaded, statuses);
    }

    #[test]
    fn provider_install_statuses_file_is_removed_when_snapshot_is_empty() {
        let temp_dir = tempfile::tempdir().expect("temp dir should exist");
        let path = provider_install_statuses_path(temp_dir.path());
        std::fs::write(&path, "{}\n").expect("fixture file should write");

        save_provider_install_statuses(temp_dir.path(), &std::collections::HashMap::new())
            .expect("empty provider install statuses should clear the file");

        assert!(!path.exists());
    }

    fn build_test_provider_archive(binary_name: &str, binary_bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut builder = Builder::new(&mut encoder);
            let mut header = Header::new_gnu();
            header.set_size(binary_bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, binary_name, binary_bytes)
                .expect("archive entry should append");
            builder.finish().expect("archive should finish");
        }
        encoder.finish().expect("gzip encoder should finish")
    }
}
