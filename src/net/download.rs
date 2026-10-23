//! Minecraft game downloader supporting Official Mojang and BMCLAPI mirrors.

use crate::env::paths::{ensure_dir, version_dir, version_json_path};
use crate::net::client::{http_get_json, http_get_json_any};
use crate::net::task::{TaskHandle, TaskManager};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";

/// Official host prefix → BMCLAPI mirror prefix.
const MIRROR_MAP: &[(&str, &str)] = &[
    ("https://piston-meta.mojang.com", BMCLAPI),
    ("https://launchermeta.mojang.com", BMCLAPI),
    ("https://piston-data.mojang.com", BMCLAPI),
    ("https://launcher.mojang.com", BMCLAPI),
    ("https://libraries.minecraft.net/", "https://bmclapi2.bangbang93.com/maven/"),
    ("https://resources.download.minecraft.net/", "https://bmclapi2.bangbang93.com/assets/"),
    ("https://maven.minecraftforge.net/", "https://bmclapi2.bangbang93.com/maven/"),
    ("https://files.minecraftforge.net/maven/", "https://bmclapi2.bangbang93.com/maven/"),
    ("https://maven.neoforged.net/releases/", "https://bmclapi2.bangbang93.com/maven/"),
    ("https://maven.fabricmc.net/", "https://bmclapi2.bangbang93.com/maven/"),
    ("https://meta.fabricmc.net/", "https://bmclapi2.bangbang93.com/fabric-meta/"),
    ("https://maven.quiltmc.org/repository/release/", "https://bmclapi2.bangbang93.com/maven/"),
    ("https://meta.quiltmc.org/", "https://bmclapi2.bangbang93.com/quilt-meta/"),
];

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
        DownloadSource::Bmclapi => MIRROR_MAP
            .iter()
            .find_map(|(official, mirror)| {
                original_url
                    .strip_prefix(official)
                    .map(|rest| format!("{mirror}{rest}"))
            })
            .unwrap_or_else(|| original_url.to_string()),
    }
}

/// Candidate URLs for a download: the preferred source first, the other one as fallback.
pub fn candidate_urls(original_url: &str, source: DownloadSource) -> Vec<String> {
    let mirror = transform_url(original_url, DownloadSource::Bmclapi);
    let mut urls = match source {
        DownloadSource::Bmclapi => vec![mirror, original_url.to_string()],
        DownloadSource::Official => vec![original_url.to_string(), mirror],
    };
    urls.dedup();
    urls
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
    #[serde(rename = "releaseTime", default)]
    pub release_time: String,
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

/// Fetches the version manifest from the selected source, falling back to the other one.
pub fn fetch_version_manifest(source: DownloadSource) -> Result<VersionManifest, DownloaderError> {
    let urls = candidate_urls(manifest_url(DownloadSource::Official), source);
    Ok(http_get_json_any(&urls)?)
}

/// Fetches detailed version info JSON from the version's manifest entry.
pub fn fetch_version_json(
    manifest_entry: &ManifestVersion,
    source: DownloadSource,
) -> Result<serde_json::Value, DownloaderError> {
    let urls = candidate_urls(&manifest_entry.url, source);
    let val: serde_json::Value = http_get_json_any(&urls)?;
    Ok(val)
}

/// Fetches a single JSON document from the preferred source (used by loader metadata APIs).
pub fn fetch_json<T: serde::de::DeserializeOwned>(official_url: &str, source: DownloadSource) -> Result<T, DownloaderError> {
    match source {
        DownloadSource::Official => Ok(http_get_json(official_url)?),
        DownloadSource::Bmclapi => Ok(http_get_json_any(&candidate_urls(official_url, source))?),
    }
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

/// Downloads and stores `versions/{id}/{id}.json` unless it already exists.
pub fn ensure_version_json(game_dir: &Path, version_id: &str, source: DownloadSource) -> Result<(), DownloaderError> {
    let json_dest = version_json_path(game_dir, version_id);
    if json_dest.is_file() {
        return Ok(());
    }
    let manifest = fetch_version_manifest(source)?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| DownloaderError::VersionNotFound(version_id.to_string()))?;

    let ver_json_value = fetch_version_json(entry, source)?;
    setup_isolated_version_dir(game_dir, version_id)?;
    fs::write(&json_dest, serde_json::to_string_pretty(&ver_json_value)?)?;
    Ok(())
}

/// Schedules a background task installing a vanilla version completely:
/// version JSON, client jar, libraries, natives and assets.
pub fn install_version(
    game_dir: &Path,
    version_id: &str,
    source: DownloadSource,
    task_manager: &TaskManager,
) -> Result<Arc<TaskHandle>, DownloaderError> {
    let game_dir = game_dir.to_path_buf();
    let id = version_id.to_string();
    let title = format!("安装 Minecraft {version_id}");
    Ok(task_manager.run_task(&title, move |task| {
        crate::net::install::install_vanilla(&game_dir, &id, source, task)
    }))
}
