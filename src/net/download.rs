//! Minecraft game downloader supporting Official Mojang and BMCLAPI mirrors.

use crate::env::paths::{ensure_dir, version_dir, version_jar_path, version_json_path};
use crate::net::client::http_get_json;
use crate::net::task::{TaskHandle, TaskManager, spawn_download_task};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DownloaderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Network error: {0}")]
    Network(#[from] crate::net::client::HttpError),
    #[error("Failed to serialize/deserialize JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Version '{0}' not found in manifest")]
    VersionNotFound(String),
    #[error("Version metadata missing client download entry")]
    MissingClientDownload,
}

/// Download source selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DownloadSource {
    #[default]
    Official,
    Bmclapi,
}

impl DownloadSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Official => "Official (Mojang)",
            Self::Bmclapi => "BMCLAPI (Mirror)",
        }
    }
}

/// Transforms Mojang download URLs to BMCLAPI mirror URLs when requested.
pub fn transform_url(original_url: &str, source: DownloadSource) -> String {
    match source {
        DownloadSource::Official => original_url.to_string(),
        DownloadSource::Bmclapi => original_url
            .replace("https://piston-meta.mojang.com", "https://bmclapi2.bangbang93.com")
            .replace("https://launchermeta.mojang.com", "https://bmclapi2.bangbang93.com")
            .replace("https://piston-data.mojang.com", "https://bmclapi2.bangbang93.com")
            .replace(
                "https://libraries.minecraft.net/",
                "https://bmclapi2.bangbang93.com/maven/",
            )
            .replace(
                "https://resources.download.minecraft.net/",
                "https://bmclapi2.bangbang93.com/assets/",
            ),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersion,
    pub versions: Vec<ManifestVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestVersion {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    #[serde(default)]
    pub sha1: String,
}

/// Manifest endpoint for official Mojang vs BMCLAPI.
pub fn manifest_url(source: DownloadSource) -> &'static str {
    match source {
        DownloadSource::Official => "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        DownloadSource::Bmclapi => {
            "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
        }
    }
}

/// Fetches the version manifest from the selected mirror.
pub fn fetch_version_manifest(source: DownloadSource) -> Result<VersionManifest, DownloaderError> {
    let url = manifest_url(source);
    let manifest: VersionManifest = http_get_json(url)?;
    Ok(manifest)
}

/// Fetches detailed version info JSON from the version's manifest entry.
pub fn fetch_version_json(
    manifest_entry: &ManifestVersion,
    source: DownloadSource,
) -> Result<serde_json::Value, DownloaderError> {
    let url = transform_url(&manifest_entry.url, source);
    let val: serde_json::Value = http_get_json(&url)?;
    Ok(val)
}

/// Sets up version directory structure (mods, config, saves).
pub fn setup_isolated_version_dir(game_dir: &Path, version_id: &str) -> std::io::Result<PathBuf> {
    let v_dir = version_dir(game_dir, version_id);
    ensure_dir(&v_dir)?;
    ensure_dir(&crate::env::paths::version_mods_dir(game_dir, version_id))?;
    ensure_dir(&crate::env::paths::version_config_dir(game_dir, version_id))?;
    ensure_dir(&crate::env::paths::version_saves_dir(game_dir, version_id))?;
    Ok(v_dir)
}

/// Schedules a background download task for client.jar and saves the version JSON in the target version directory.
pub fn install_version(
    game_dir: &Path,
    version_id: &str,
    source: DownloadSource,
    task_manager: &TaskManager,
) -> Result<Arc<TaskHandle>, DownloaderError> {
    let manifest = fetch_version_manifest(source)?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| DownloaderError::VersionNotFound(version_id.to_string()))?;

    let ver_json_value = fetch_version_json(entry, source)?;
    setup_isolated_version_dir(game_dir, version_id)?;

    // 1. Write the version json: .minecraft/versions/{version_id}/{version_id}.json
    let json_dest = version_json_path(game_dir, version_id);
    let json_pretty = serde_json::to_string_pretty(&ver_json_value)?;
    fs::write(&json_dest, json_pretty)?;

    // 2. Extract client.jar download URL & SHA-1
    let client_dl = ver_json_value
        .get("downloads")
        .and_then(|d| d.get("client"))
        .ok_or(DownloaderError::MissingClientDownload)?;

    let client_url = client_dl
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or(DownloaderError::MissingClientDownload)?;
    let client_sha1 = client_dl
        .get("sha1")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    let client_jar_dest = version_jar_path(game_dir, version_id);
    let final_url = transform_url(client_url, source);

    // 3. Create managed task and spawn background worker
    let task = task_manager.create_task(&format!("下载 Minecraft {version_id} 核心客户端"));
    spawn_download_task(final_url, client_jar_dest, client_sha1, task.clone());

    Ok(task)
}
