//! World save management: listing, backup, renaming, and deletion.

use crate::env::paths::{ensure_dir, version_saves_dir};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Discovered world save directory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldSaveEntry {
    pub folder_name: String,
    pub path: PathBuf,
    pub display_name: String,
    pub size_bytes: u64,
    pub modified_time: Option<u64>,
    pub has_icon: bool,
    pub icon_path: Option<PathBuf>,
}

/// Recursively computes the total byte size of a directory.
pub fn compute_save_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += compute_save_size(&path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Lists all world saves in the instance's saves folder.
pub fn list_world_saves(game_dir: &Path, version_id: &str) -> Vec<WorldSaveEntry> {
    let saves_dir = version_saves_dir(game_dir, version_id);
    let mut list = Vec::new();

    if let Ok(entries) = fs::read_dir(&saves_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // A valid world save directory contains level.dat or sub-chunks
            let folder_name = path.file_name().unwrap().to_string_lossy().to_string();
            let icon_file = path.join("icon.png");
            let has_icon = icon_file.is_file();
            let icon_path = if has_icon { Some(icon_file) } else { None };

            let meta = entry.metadata().ok();
            let modified_time = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());

            let size_bytes = compute_save_size(&path);

            list.push(WorldSaveEntry {
                folder_name: folder_name.clone(),
                path,
                display_name: folder_name,
                size_bytes,
                modified_time,
                has_icon,
                icon_path,
            });
        }
    }

    list.sort_by_key(|a| std::cmp::Reverse(a.modified_time));
    list
}

/// Deletes a world save folder permanently.
pub fn delete_world_save(game_dir: &Path, version_id: &str, folder_name: &str) -> std::io::Result<()> {
    let target = version_saves_dir(game_dir, version_id).join(folder_name);
    if target.is_dir() {
        fs::remove_dir_all(target)?;
    }
    Ok(())
}

/// Renames a world save folder.
pub fn rename_world_save(
    game_dir: &Path,
    version_id: &str,
    old_name: &str,
    new_name: &str,
) -> std::io::Result<PathBuf> {
    let saves_dir = version_saves_dir(game_dir, version_id);
    let src = saves_dir.join(old_name);
    let dst = saves_dir.join(new_name);

    if src.is_dir() {
        fs::rename(&src, &dst)?;
    }
    Ok(dst)
}

/// Creates a full backup copy of a world save: `{folder_name}_backup_{timestamp}`.
pub fn backup_world_save(
    game_dir: &Path,
    version_id: &str,
    folder_name: &str,
) -> std::io::Result<PathBuf> {
    let saves_dir = version_saves_dir(game_dir, version_id);
    let src = saves_dir.join(folder_name);

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_name = format!("{folder_name}_backup_{ts}");
    let dst = saves_dir.join(&backup_name);

    copy_dir_all(&src, &dst)?;
    Ok(dst)
}

/// Recursively copies a directory to a new location.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    ensure_dir(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
