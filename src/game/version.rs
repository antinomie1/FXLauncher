//! Minecraft version metadata parsing, rule evaluation, inheritance and library classpath calculation.

use crate::env::paths::{arch_name, libraries_dir, os_name, version_jar_path, version_json_path, versions_dir};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Default Maven repository for libraries that only carry a coordinate.
pub const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

#[derive(Debug, Error)]
pub enum VersionError {
    #[error("I/O error reading version metadata: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error parsing version metadata: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Version '{0}' directory or json does not exist")]
    NotFound(String),
    #[error("Version inheritance chain is too deep or cyclic at '{0}'")]
    InheritanceLoop(String),
}

/// Simplified view of a version definition from `{id}.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type", default)]
    pub version_type: String,
    #[serde(rename = "mainClass", default)]
    pub main_class: String,
    #[serde(rename = "assetIndex")]
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(rename = "minecraftArguments")]
    pub legacy_arguments: Option<String>,
    pub arguments: Option<ModernArguments>,
    /// Parent version this profile extends (Fabric, Forge, ...).
    #[serde(rename = "inheritsFrom", default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    /// Version whose client jar should be used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jar: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<VersionDownloads>,
    #[serde(rename = "javaVersion", default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionDownloads {
    pub client: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaVersionRef {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<HashMap<String, String>>,
    /// Maven repository base URL (Fabric / Forge style libraries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<ExtractRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractRule {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Artifact {
    pub path: Option<String>,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Evaluates a list of rules against the current OS with every launcher feature disabled.
pub fn evaluate_rules(rules: &[Rule], current_os: &str) -> bool {
    evaluate_rules_with(rules, current_os, &[])
}

/// Evaluates rules against the OS, CPU architecture and the enabled launcher features
/// (e.g. `has_custom_resolution`). Unlisted features count as disabled.
pub fn evaluate_rules_with(rules: &[Rule], current_os: &str, features: &[&str]) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        let os_match = match &rule.os {
            Some(os_rule) => {
                os_rule.name.as_deref().is_none_or(|n| n == current_os)
                    && os_rule.arch.as_deref().is_none_or(|a| a == arch_name())
            }
            None => true,
        };
        let features_match = rule.features.as_ref().is_none_or(|map| {
            map.iter().all(|(name, wanted)| features.contains(&name.as_str()) == *wanted)
        });

        if os_match && features_match {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

/// Converts a Maven coordinate string ("group:artifact:version[:classifier][@ext]") to relative file path.
pub fn maven_to_path(name: &str) -> Option<PathBuf> {
    let (coord, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<&str> = coord.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3);

    let filename = if let Some(cls) = classifier {
        format!("{artifact}-{version}-{cls}.{ext}")
    } else {
        format!("{artifact}-{version}.{ext}")
    };

    let mut path = PathBuf::from(group);
    path.push(artifact);
    path.push(version);
    path.push(filename);
    Some(path)
}

impl Library {
    /// Whether this library applies to the current platform.
    pub fn applies(&self) -> bool {
        self.rules.as_deref().is_none_or(|r| evaluate_rules(r, os_name()))
    }

    /// Legacy native-only libraries (with a `natives` map and no main artifact) are not put on the classpath.
    pub fn is_legacy_native_only(&self) -> bool {
        self.natives.is_some() && self.downloads.as_ref().is_none_or(|d| d.artifact.is_none())
    }

    /// Whether the main artifact is a modern platform natives jar (`...:natives-linux`).
    pub fn is_native_jar(&self) -> bool {
        self.name.split(':').nth(3).is_some_and(|c| c.starts_with("natives-"))
    }

    /// Key identifying the same library across versions: `group:artifact[:classifier]`.
    pub fn dedup_key(&self) -> String {
        let parts: Vec<&str> = self.name.split('@').next().unwrap_or("").split(':').collect();
        match parts.as_slice() {
            [g, a, _, c, ..] => format!("{g}:{a}:{c}"),
            [g, a, ..] => format!("{g}:{a}"),
            _ => self.name.clone(),
        }
    }

    /// Relative path of the main artifact inside `libraries/`.
    pub fn artifact_path(&self) -> Option<PathBuf> {
        self.downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .and_then(|a| a.path.as_deref().map(PathBuf::from))
            .or_else(|| maven_to_path(&self.name))
    }

    /// Main artifact download as `(relative path, url, sha1, size)`.
    /// The URL is empty when the file must come from elsewhere (e.g. a Forge installer).
    pub fn artifact_download(&self) -> Option<(PathBuf, String, Option<String>, u64)> {
        if self.is_legacy_native_only() {
            return None;
        }
        let rel = self.artifact_path()?;
        if let Some(a) = self.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            let url = a.url.clone().unwrap_or_default();
            return Some((rel, url, a.sha1.clone(), a.size.unwrap_or(0)));
        }
        let base = self.url.as_deref().unwrap_or(MOJANG_LIBRARIES);
        let base = base.trim_end_matches('/');
        let url = format!("{base}/{}", rel.to_string_lossy().replace('\\', "/"));
        Some((rel, url, self.sha1.clone(), self.size.unwrap_or(0)))
    }

    /// Legacy natives classifier artifact for the current OS (`natives-linux`, `natives-windows-64`, ...).
    pub fn native_artifact(&self) -> Option<&Artifact> {
        let classifier = self.natives.as_ref()?.get(os_name())?;
        let bits = if cfg!(target_pointer_width = "64") { "64" } else { "32" };
        let classifier = classifier.replace("${arch}", bits);
        self.downloads.as_ref()?.classifiers.as_ref()?.get(&classifier)
    }
}

impl VersionInfo {
    /// Id of the version whose client jar is used.
    pub fn client_jar_id(&self) -> &str {
        self.jar.as_deref().unwrap_or(&self.id)
    }

    /// Asset index id (`assetIndex.id`, falling back to `assets`).
    pub fn asset_index_id(&self) -> Option<&str> {
        self.asset_index.as_ref().map(|a| a.id.as_str()).or(self.assets.as_deref())
    }

    /// Merges a child profile on top of its parent: child libraries win, arguments are appended.
    pub fn merge_onto(self, parent: VersionInfo) -> VersionInfo {
        let mut seen = HashSet::new();
        let mut libraries = Vec::new();
        for lib in self.libraries.into_iter().chain(parent.libraries) {
            if seen.insert(lib.dedup_key()) {
                libraries.push(lib);
            }
        }

        let arguments = match (parent.arguments, self.arguments) {
            (Some(mut p), Some(c)) => {
                p.game.extend(c.game);
                p.jvm.extend(c.jvm);
                Some(p)
            }
            (p, c) => c.or(p),
        };
        let jar = self.jar.or(parent.jar);

        VersionInfo {
            id: self.id,
            version_type: if self.version_type.is_empty() { parent.version_type } else { self.version_type },
            main_class: if self.main_class.is_empty() { parent.main_class } else { self.main_class },
            asset_index: self.asset_index.or(parent.asset_index),
            assets: self.assets.or(parent.assets),
            libraries,
            legacy_arguments: self.legacy_arguments.or(parent.legacy_arguments),
            arguments,
            inherits_from: None,
            jar,
            downloads: self.downloads.or(parent.downloads),
            java_version: self.java_version.or(parent.java_version),
        }
    }
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

/// Loads a version's JSON file as-is, without resolving `inheritsFrom`.
pub fn load_raw_version_info(game_dir: &Path, version_id: &str) -> Result<VersionInfo, VersionError> {
    let json_path = version_json_path(game_dir, version_id);

    if !json_path.is_file() {
        return Err(VersionError::NotFound(version_id.to_string()));
    }

    let content = fs::read_to_string(json_path)?;
    let info: VersionInfo = serde_json::from_str(&content)?;
    Ok(info)
}

/// Loads a version's JSON file and resolves its `inheritsFrom` chain into one profile.
pub fn load_version_info(game_dir: &Path, version_id: &str) -> Result<VersionInfo, VersionError> {
    let mut info = load_raw_version_info(game_dir, version_id)?;
    let mut depth = 0;
    let mut root_id = None;
    while let Some(parent_id) = info.inherits_from.clone() {
        depth += 1;
        if depth > 8 || parent_id == info.id {
            return Err(VersionError::InheritanceLoop(parent_id));
        }
        let parent = load_raw_version_info(game_dir, &parent_id)?;
        let next_parent = parent.inherits_from.clone();
        info = info.merge_onto(parent);
        info.inherits_from = next_parent;
        root_id = Some(parent_id);
    }
    // The client jar comes from the root vanilla version unless a profile names one.
    info.jar = info.jar.or(root_id);
    Ok(info)
}

/// Resolves the list of classpath jar paths for the given version.
pub fn collect_classpaths(game_dir: &Path, info: &VersionInfo) -> Vec<PathBuf> {
    let lib_dir = libraries_dir(game_dir);
    let mut paths = Vec::new();

    for lib in &info.libraries {
        if !lib.applies() || lib.is_legacy_native_only() {
            continue;
        }
        if let Some(rel) = lib.artifact_path() {
            paths.push(lib_dir.join(rel));
        }
    }

    // Prefer the instance's own jar copy, fall back to the inherited one.
    let own_jar = version_jar_path(game_dir, &info.id);
    if own_jar.is_file() || info.jar.is_none() {
        paths.push(own_jar);
    } else {
        paths.push(version_jar_path(game_dir, info.client_jar_id()));
    }
    paths
}
