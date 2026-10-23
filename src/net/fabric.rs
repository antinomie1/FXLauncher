//! Fabric and Quilt installation through their metadata servers (profile JSON + Maven libraries).

use crate::env::paths::{ensure_dir, version_dir, version_json_path};
use crate::net::download::{DownloadSource, DownloaderError, fetch_json};
use crate::net::mods::ModLoader;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// A selectable loader build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Debug, Deserialize)]
struct LoaderInfo {
    version: String,
    #[serde(default)]
    stable: Option<bool>,
}

fn meta_base(loader: ModLoader) -> &'static str {
    match loader {
        ModLoader::Quilt => "https://meta.quiltmc.org/v3",
        _ => "https://meta.fabricmc.net/v2",
    }
}

/// Lists loader builds compatible with a Minecraft version, newest first.
pub fn list_loader_versions(loader: ModLoader, game_version: &str, source: DownloadSource) -> Result<Vec<LoaderVersion>, DownloaderError> {
    let url = format!("{}/versions/loader/{game_version}", meta_base(loader));
    let entries: Vec<LoaderEntry> = fetch_json(&url, source)?;
    Ok(entries
        .into_iter()
        .map(|e| {
            let stable = e.loader.stable.unwrap_or_else(|| !e.loader.version.contains('-'));
            LoaderVersion { version: e.loader.version, stable }
        })
        .collect())
}

/// Default instance id used by the official installers.
pub fn default_profile_id(loader: ModLoader, game_version: &str, loader_version: &str) -> String {
    match loader {
        ModLoader::Quilt => format!("quilt-loader-{loader_version}-{game_version}"),
        _ => format!("fabric-loader-{loader_version}-{game_version}"),
    }
}

/// Downloads the launcher profile for the loader and writes it as `versions/{id}/{id}.json`
/// (inheriting from the vanilla version). Returns the instance id.
pub fn install_profile(
    game_dir: &Path,
    loader: ModLoader,
    game_version: &str,
    loader_version: Option<&str>,
    name: Option<&str>,
    source: DownloadSource,
) -> Result<String, DownloaderError> {
    let loader_version = match loader_version {
        Some(v) => v.to_string(),
        None => {
            let versions = list_loader_versions(loader, game_version, source)?;
            versions
                .iter()
                .find(|v| v.stable)
                .or(versions.first())
                .map(|v| v.version.clone())
                .ok_or_else(|| DownloaderError::VersionNotFound(format!("{} for {game_version}", loader.display_name())))?
        }
    };

    let url = format!(
        "{}/versions/loader/{game_version}/{loader_version}/profile/json",
        meta_base(loader)
    );
    let mut profile: serde_json::Value = fetch_json(&url, source)?;
    let id = name
        .map(str::to_string)
        .unwrap_or_else(|| default_profile_id(loader, game_version, &loader_version));
    profile["id"] = serde_json::Value::String(id.clone());
    profile["inheritsFrom"] = serde_json::Value::String(game_version.to_string());

    ensure_dir(&version_dir(game_dir, &id))?;
    fs::write(version_json_path(game_dir, &id), serde_json::to_string_pretty(&profile)?)?;
    Ok(id)
}
