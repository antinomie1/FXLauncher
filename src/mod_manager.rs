//! Mod management and version-specific mods directory support (Modrinth & CurseForge).

use crate::task::{TaskHandle, TaskManager, spawn_download_task};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Network error: {0}")]
    Network(#[from] Box<ureq::Error>),
    #[error("Failed to parse mod metadata JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No compatible file found for version {version} and loader {loader}")]
    NoCompatibleFile { version: String, loader: String },
}

/// Target mod loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModLoader {
    #[default]
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

impl ModLoader {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::Quilt => "quilt",
        }
    }
}

/// Returns the `mods/` directory for a given version.
/// Example: `.minecraft/versions/{version_id}/mods/`.
pub fn version_mods_dir(game_dir: &Path, version_id: &str) -> PathBuf {
    game_dir.join("versions").join(version_id).join("mods")
}

/// Metadata about an installed mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledMod {
    pub file_name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// Scans and lists all mod `.jar` files in the version's mods directory.
pub fn list_installed_mods(game_dir: &Path, version_id: &str) -> Vec<InstalledMod> {
    let mods_dir = version_mods_dir(game_dir, version_id);
    let mut list = Vec::new();

    if let Ok(entries) = fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jar") {
                let file_name = path.file_name().unwrap().to_string_lossy().to_string();
                let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                list.push(InstalledMod {
                    file_name,
                    path,
                    size_bytes,
                });
            }
        }
    }

    list.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    list
}

/// Unified mod search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSearchResult {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub source_platform: String, // "modrinth" or "curseforge"
    pub downloads: u64,
}

/// Mod file information ready for download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModFileInfo {
    pub file_name: String,
    pub download_url: String,
    pub sha1: Option<String>,
    pub size: u64,
}

// ---------------- Modrinth API ----------------

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthHit>,
}

#[derive(Debug, Deserialize)]
struct ModrinthHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    icon_url: Option<String>,
    downloads: u64,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    size: u64,
    hashes: ModrinthHashes,
}

#[derive(Debug, Deserialize)]
struct ModrinthHashes {
    sha1: Option<String>,
}

/// Searches Modrinth for mods matching query and target Minecraft version.
pub fn search_modrinth(
    query: &str,
    mc_version: &str,
    loader: ModLoader,
) -> Result<Vec<ModSearchResult>, ModError> {
    let mut facets = format!(r#"[["project_type:mod"],["versions:{mc_version}"]"#);
    if loader != ModLoader::Vanilla {
        facets.push_str(&format!(r#",["categories:{}"]"#, loader.as_str()));
    }
    facets.push(']');

    let url = format!(
        "https://api.modrinth.com/v2/search?query={}&facets={}",
        urlencoding(query),
        urlencoding(&facets)
    );

    let resp = ureq::get(&url)
        .set("User-Agent", "fxlauncher/0.1.0")
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| ModError::Network(Box::new(e)))?;

    let data: ModrinthSearchResponse = resp.into_json()?;
    let results = data
        .hits
        .into_iter()
        .map(|h| ModSearchResult {
            id: h.project_id,
            slug: h.slug,
            title: h.title,
            description: h.description,
            icon_url: h.icon_url,
            source_platform: "modrinth".to_string(),
            downloads: h.downloads,
        })
        .collect();

    Ok(results)
}

/// Resolves the downloadable file for a Modrinth project matching version & loader.
pub fn resolve_modrinth_file(
    project_id_or_slug: &str,
    mc_version: &str,
    loader: ModLoader,
) -> Result<ModFileInfo, ModError> {
    let url = format!(
        "https://api.modrinth.com/v2/project/{}/version?game_versions={}&loaders={}",
        project_id_or_slug,
        urlencoding(&format!(r#"["{mc_version}"]"#)),
        urlencoding(&format!(r#"["{}"]"#, loader.as_str()))
    );

    let resp = ureq::get(&url)
        .set("User-Agent", "fxlauncher/0.1.0")
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| ModError::Network(Box::new(e)))?;

    let versions: Vec<ModrinthVersion> = resp.into_json()?;
    for ver in versions {
        if let Some(file) = ver.files.into_iter().next() {
            return Ok(ModFileInfo {
                file_name: file.filename,
                download_url: file.url,
                sha1: file.hashes.sha1,
                size: file.size,
            });
        }
    }

    Err(ModError::NoCompatibleFile {
        version: mc_version.to_string(),
        loader: loader.as_str().to_string(),
    })
}

// ---------------- CurseForge API (via BMCLAPI Mirror) ----------------

#[derive(Debug, Deserialize)]
struct CurseForgeSearchResponse {
    data: Vec<CurseForgeMod>,
}

#[derive(Debug, Deserialize)]
struct CurseForgeMod {
    id: u64,
    name: String,
    summary: String,
    slug: String,
    #[serde(rename = "downloadCount", default)]
    download_count: u64,
}

/// Searches CurseForge for mods using BMCLAPI's CurseForge mirror.
pub fn search_curseforge(
    query: &str,
    mc_version: &str,
) -> Result<Vec<ModSearchResult>, ModError> {
    let url = format!(
        "https://bmclapi2.bangbang93.com/curseforge/v1/mods/search?gameId=432&gameVersion={}&searchFilter={}",
        urlencoding(mc_version),
        urlencoding(query)
    );

    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| ModError::Network(Box::new(e)))?;

    let data: CurseForgeSearchResponse = resp.into_json()?;
    let results = data
        .data
        .into_iter()
        .map(|m| ModSearchResult {
            id: m.id.to_string(),
            slug: m.slug,
            title: m.name,
            description: m.summary,
            icon_url: None,
            source_platform: "curseforge".to_string(),
            downloads: m.download_count,
        })
        .collect();

    Ok(results)
}

/// Downloads a mod file directly into the `mods/` directory of the target version.
pub fn download_mod_to_isolated_version(
    game_dir: &Path,
    version_id: &str,
    file_info: &ModFileInfo,
    task_manager: &TaskManager,
) -> std::io::Result<Arc<TaskHandle>> {
    let mods_dir = version_mods_dir(game_dir, version_id);
    fs::create_dir_all(&mods_dir)?;

    let dest = mods_dir.join(&file_info.file_name);
    let task = task_manager.create_task(&format!("下载模组: {}", file_info.file_name));

    spawn_download_task(
        file_info.download_url.clone(),
        dest,
        file_info.sha1.clone(),
        task.clone(),
    );

    Ok(task)
}

/// Simple percent encoder for query parameters without bulky external crates.
fn urlencoding(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}
