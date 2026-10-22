//! Minecraft version metadata parsing, rule evaluation, and library classpath calculation.

use crate::env::paths::{libraries_dir, os_name, version_jar_path, version_json_path, versions_dir};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VersionError {
    #[error("I/O error reading version metadata: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error parsing version metadata: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Version '{0}' directory or json does not exist")]
    NotFound(String),
}

/// Simplified view of a version definition from `{id}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type", default)]
    pub version_type: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(rename = "minecraftArguments")]
    pub legacy_arguments: Option<String>,
    pub arguments: Option<ModernArguments>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub path: Option<String>,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub version: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModernArguments {
    #[serde(default)]
    pub game: Vec<ArgumentItem>,
    #[serde(default)]
    pub jvm: Vec<ArgumentItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentItem {
    Simple(String),
    Conditional {
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    Single(String),
    Multiple(Vec<String>),
}

/// Evaluates a list of rules against the current OS.
pub fn evaluate_rules(rules: &[Rule], current_os: &str) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        let os_match = match &rule.os {
            Some(os_rule) => os_rule.name.as_deref().map(|n| n == current_os).unwrap_or(true),
            None => true,
        };

        if os_match {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

/// Converts a Maven coordinate string ("group:artifact:version[:classifier]") to relative file path.
pub fn maven_to_path(name: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3);

    let filename = if let Some(cls) = classifier {
        format!("{artifact}-{version}-{cls}.jar")
    } else {
        format!("{artifact}-{version}.jar")
    };

    let mut path = PathBuf::from(group);
    path.push(artifact);
    path.push(version);
    path.push(filename);
    Some(path)
}

/// Scans the versions directory and returns a sorted list of local version IDs.
pub fn scan_local_versions(game_dir: &Path) -> Vec<String> {
    let v_dir = versions_dir(game_dir);
    let mut versions = Vec::new();

    if let Ok(entries) = fs::read_dir(v_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let id = entry.file_name().to_string_lossy().to_string();
                let json_path = version_json_path(game_dir, &id);
                if json_path.is_file() {
                    versions.push(id);
                }
            }
        }
    }

    versions.sort();
    versions
}

/// Loads and parses a version's JSON file.
pub fn load_version_info(game_dir: &Path, version_id: &str) -> Result<VersionInfo, VersionError> {
    let json_path = version_json_path(game_dir, version_id);

    if !json_path.is_file() {
        return Err(VersionError::NotFound(version_id.to_string()));
    }

    let content = fs::read_to_string(json_path)?;
    let info: VersionInfo = serde_json::from_str(&content)?;
    Ok(info)
}

/// Resolves the list of classpath jar paths for the given version.
pub fn collect_classpaths(game_dir: &Path, info: &VersionInfo) -> Vec<PathBuf> {
    let lib_dir = libraries_dir(game_dir);
    let current_os = os_name();
    let mut paths = Vec::new();

    for lib in &info.libraries {
        if let Some(rules) = &lib.rules {
            if !evaluate_rules(rules, current_os) {
                continue;
            }
        }

        let rel_path = lib
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .and_then(|a| a.path.as_deref().map(PathBuf::from))
            .or_else(|| maven_to_path(&lib.name));

        if let Some(rel) = rel_path {
            paths.push(lib_dir.join(rel));
        }
    }

    paths.push(version_jar_path(game_dir, &info.id));
    paths
}
