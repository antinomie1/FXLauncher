//! Mod management: listing, enabling, disabling, deleting, and dependency/conflict analysis.

use crate::env::paths::version_mods_dir;
use crate::net::mods::ModLoader;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Metadata about a mod found in the instance's mods folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModEntry {
    pub file_name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub size_bytes: u64,
    pub modified_time: Option<u64>,
    pub deduced_id: String,
    pub deduced_version: Option<String>,
    pub deduced_mc_version: Option<String>,
}

/// Diagnostic report generated from analyzing the instance's mods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModAnalysisReport {
    pub total_mods: usize,
    pub enabled_mods: usize,
    pub disabled_mods: usize,
    pub duplicate_mods: Vec<DuplicateModAlert>,
    pub compatibility_warnings: Vec<String>,
    pub missing_recommendations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateModAlert {
    pub mod_id: String,
    pub files: Vec<String>,
}

/// Parses mod id, mod version, and mc version from typical mod jar filenames.
/// Examples:
/// - `sodium-fabric-0.5.8+mc1.21.jar` -> id: `sodium`, ver: `0.5.8`, mc: `1.21`
/// - `iris-mc1.20.1-1.6.4.jar` -> id: `iris`, ver: `1.6.4`, mc: `1.20.1`
/// - `jei-1.20.1-forge-15.3.0.4.jar` -> id: `jei`, ver: `15.3.0.4`, mc: `1.20.1`
pub fn deduce_mod_info_from_filename(filename: &str) -> (String, Option<String>, Option<String>) {
    let clean = filename
        .trim_end_matches(".disabled")
        .trim_end_matches(".jar");

    let parts: Vec<&str> = clean.split(&['-', '_', '+'][..]).collect();
    let mod_id = parts.first().unwrap_or(&clean).to_lowercase();

    let rest = parts.get(1..).unwrap_or_default();
    let starts_numeric = |s: &str| s.chars().next().is_some_and(|c| c.is_ascii_digit());
    // An explicit `mc1.21.1` tag wins; otherwise the first unprefixed `1.x` part is taken as the game version.
    let mc_index = rest
        .iter()
        .position(|p| p.strip_prefix("mc").is_some_and(|v| starts_numeric(v) && v.contains('.')))
        .or_else(|| rest.iter().position(|p| p.starts_with("1.") && p.len() > 2));
    let mc_ver = mc_index.map(|i| rest[i].trim_start_matches("mc").to_string());
    let mod_ver = rest
        .iter()
        .enumerate()
        .find(|(i, p)| Some(*i) != mc_index && starts_numeric(p))
        .map(|(_, p)| p.to_string());

    (mod_id, mod_ver, mc_ver)
}

/// Scans the instance's mods folder and returns all installed mods (both enabled and disabled).
pub fn list_instance_mods(game_dir: &Path, version_id: &str) -> Vec<ModEntry> {
    let mods_dir = version_mods_dir(game_dir, version_id);
    let mut list = Vec::new();

    if let Ok(entries) = fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let is_jar = file_name.ends_with(".jar");
            let is_disabled = file_name.ends_with(".jar.disabled") || file_name.ends_with(".disabled");

            if !is_jar && !is_disabled {
                continue;
            }

            let enabled = is_jar;
            let meta = entry.metadata().ok();
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified_time = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());

            let (deduced_id, deduced_version, deduced_mc_version) =
                deduce_mod_info_from_filename(&file_name);

            list.push(ModEntry {
                file_name,
                path,
                enabled,
                size_bytes,
                modified_time,
                deduced_id,
                deduced_version,
                deduced_mc_version,
            });
        }
    }

    list.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    list
}

/// Enables a disabled mod by renaming it to end with `.jar`.
pub fn enable_mod(game_dir: &Path, version_id: &str, file_name: &str) -> std::io::Result<PathBuf> {
    let mods_dir = version_mods_dir(game_dir, version_id);
    let current_path = mods_dir.join(file_name);

    let new_name = if file_name.ends_with(".disabled") {
        file_name.trim_end_matches(".disabled").to_string()
    } else {
        file_name.to_string()
    };

    let new_path = mods_dir.join(&new_name);
    if current_path != new_path {
        fs::rename(&current_path, &new_path)?;
    }
    Ok(new_path)
}

/// Disables an enabled mod by renaming it to end with `.jar.disabled`.
pub fn disable_mod(game_dir: &Path, version_id: &str, file_name: &str) -> std::io::Result<PathBuf> {
    let mods_dir = version_mods_dir(game_dir, version_id);
    let current_path = mods_dir.join(file_name);

    let new_name = if file_name.ends_with(".disabled") {
        file_name.to_string()
    } else {
        format!("{file_name}.disabled")
    };

    let new_path = mods_dir.join(&new_name);
    if current_path != new_path {
        fs::rename(&current_path, &new_path)?;
    }
    Ok(new_path)
}

/// Toggles a mod between enabled and disabled.
pub fn toggle_mod(game_dir: &Path, version_id: &str, file_name: &str) -> std::io::Result<PathBuf> {
    if file_name.ends_with(".disabled") {
        enable_mod(game_dir, version_id, file_name)
    } else {
        disable_mod(game_dir, version_id, file_name)
    }
}

/// Permanently removes a mod file from the instance's mods folder.
pub fn delete_mod(game_dir: &Path, version_id: &str, file_name: &str) -> std::io::Result<()> {
    let mods_dir = version_mods_dir(game_dir, version_id);
    let path = mods_dir.join(file_name);
    if path.is_file() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Analyzes installed mods for duplicates, version mismatches, and common dependency gaps.
pub fn analyze_instance_mods(
    game_dir: &Path,
    version_id: &str,
    target_mc_version: Option<&str>,
    target_loader: Option<ModLoader>,
) -> ModAnalysisReport {
    let mods = list_instance_mods(game_dir, version_id);
    let mut report = ModAnalysisReport {
        total_mods: mods.len(),
        enabled_mods: mods.iter().filter(|m| m.enabled).count(),
        disabled_mods: mods.iter().filter(|m| !m.enabled).count(),
        duplicate_mods: Vec::new(),
        compatibility_warnings: Vec::new(),
        missing_recommendations: Vec::new(),
    };

    // 1. Duplicate detection
    let mut id_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for m in &mods {
        if m.enabled {
            id_map
                .entry(m.deduced_id.clone())
                .or_default()
                .push(m.file_name.clone());
        }
    }

    for (id, files) in id_map {
        if files.len() > 1 {
            report.duplicate_mods.push(DuplicateModAlert {
                mod_id: id,
                files,
            });
        }
    }

    // 2. Minecraft version compatibility checks
    if let Some(target_ver) = target_mc_version {
        for m in &mods {
            if let Some(mc) = &m.deduced_mc_version
                && !target_ver.starts_with(mc) && !mc.starts_with(target_ver) {
                    report.compatibility_warnings.push(format!(
                        "模组 '{}' 标注支持 Minecraft {}，可能与当前版本 {} 不匹配。",
                        m.file_name, mc, target_ver
                    ));
                }
        }
    }

    // 3. Mod loader compatibility checks
    if let Some(loader) = target_loader {
        for m in &mods {
            let lower_name = m.file_name.to_lowercase();
            match loader {
                ModLoader::Fabric => {
                    if lower_name.contains("-forge-") || lower_name.ends_with("-forge.jar") {
                        report.compatibility_warnings.push(format!(
                            "模组 '{}' 疑似为 Forge 专属版本，无法在 Fabric 实例中加载。",
                            m.file_name
                        ));
                    }
                }
                ModLoader::Forge
                    if (lower_name.contains("-fabric-") || lower_name.ends_with("-fabric.jar")) => {
                        report.compatibility_warnings.push(format!(
                            "模组 '{}' 疑似为 Fabric 专属版本，无法在 Forge 实例中加载。",
                            m.file_name
                        ));
                    }
                _ => {}
            }
        }
    }

    // 4. Common dependency recommendations
    let enabled_ids: Vec<&str> = mods.iter().filter(|m| m.enabled).map(|m| m.deduced_id.as_str()).collect();
    if enabled_ids.contains(&"iris") && !enabled_ids.contains(&"sodium") {
        report.missing_recommendations.push("已启用 Iris Shaders，但未检测到前置模组 Sodium。".to_string());
    }
    if enabled_ids.contains(&"indium") && !enabled_ids.contains(&"sodium") {
        report.missing_recommendations.push("已启用 Indium，但未检测到核心模组 Sodium。".to_string());
    }

    report
}
