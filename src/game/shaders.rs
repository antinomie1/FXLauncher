//! Shader pack management and active shader configuration (`optionsshaders.txt`).

use crate::env::paths::{ensure_dir, version_dir, version_shaderpacks_dir};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// A shader pack entry discovered in the shaderpacks folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderPackEntry {
    pub file_name: String,
    pub path: PathBuf,
    pub is_directory: bool,
    pub is_active: bool,
    pub size_bytes: u64,
}

/// Returns the path to the shader options configuration file: `{version_dir}/optionsshaders.txt`.
pub fn options_shaders_path(game_dir: &Path, version_id: &str) -> PathBuf {
    version_dir(game_dir, version_id).join("optionsshaders.txt")
}

/// Reads the currently active shader pack name from `optionsshaders.txt`.
pub fn get_active_shaderpack(game_dir: &Path, version_id: &str) -> Option<String> {
    let path = options_shaders_path(game_dir, version_id);
    if !path.is_file() {
        return None;
    }

    let content = fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("shaderPack=") {
            let val = trimmed["shaderPack=".len()..].trim();
            if val.is_empty() || val.eq_ignore_ascii_case("OFF") || val.eq_ignore_ascii_case("(internal)") {
                return None;
            }
            return Some(val.to_string());
        }
    }

    None
}

/// Sets the active shader pack by updating `optionsshaders.txt`.
pub fn set_active_shaderpack(
    game_dir: &Path,
    version_id: &str,
    pack_name: Option<&str>,
) -> std::io::Result<()> {
    let path = options_shaders_path(game_dir, version_id);
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }

    let pack_val = pack_name.unwrap_or("OFF");
    let mut lines = Vec::new();
    let mut replaced = false;

    if path.is_file() {
        if let Ok(content) = fs::read_to_string(&path) {
            for line in content.lines() {
                if line.trim().starts_with("shaderPack=") {
                    lines.push(format!("shaderPack={pack_val}"));
                    replaced = true;
                } else {
                    lines.push(line.to_string());
                }
            }
        }
    }

    if !replaced {
        lines.push(format!("shaderPack={pack_val}"));
    }

    fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Lists all shader packs in the instance's shaderpacks folder.
pub fn list_shaderpacks(game_dir: &Path, version_id: &str) -> Vec<ShaderPackEntry> {
    let dir = version_shaderpacks_dir(game_dir, version_id);
    let active_name = get_active_shaderpack(game_dir, version_id);
    let mut list = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_dir = path.is_dir();
            let is_zip = path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("zip");

            if !is_dir && !is_zip {
                continue;
            }

            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let is_active = active_name.as_deref() == Some(&file_name);
            let size_bytes = if is_dir {
                compute_dir_size(&path)
            } else {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            };

            list.push(ShaderPackEntry {
                file_name,
                path,
                is_directory: is_dir,
                is_active,
                size_bytes,
            });
        }
    }

    list.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    list
}

/// Deletes a shader pack file or folder from the shaderpacks directory.
pub fn delete_shaderpack(game_dir: &Path, version_id: &str, file_name: &str) -> std::io::Result<()> {
    let dir = version_shaderpacks_dir(game_dir, version_id);
    let target = dir.join(file_name);

    if target.is_dir() {
        fs::remove_dir_all(target)?;
    } else if target.is_file() {
        fs::remove_file(target)?;
    }
    Ok(())
}

fn compute_dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += compute_dir_size(&path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}
