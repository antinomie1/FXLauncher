//! Launcher configuration and persistence.

use crate::env::paths::{default_minecraft_dir, ensure_dir, libraries_dir, versions_dir};
use crate::net::download::DownloadSource;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialize/deserialize error: {0}")]
    Json(#[from] serde_json::Error),
}

/// UI color scheme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

/// Launcher configuration options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LauncherConfig {
    /// Minecraft game root directory.
    pub game_dir: PathBuf,
    /// Currently selected version ID (e.g., "1.21.4").
    pub selected_version: String,
    /// Player display name.
    pub player_name: String,
    /// Whether to use offline mode.
    pub offline_mode: bool,
    /// Path or command name for the Java binary.
    pub java_path: String,
    /// Minimum JVM memory in megabytes (e.g. 1024).
    pub min_memory_mb: u32,
    /// Maximum JVM memory in megabytes (e.g. 4096).
    pub max_memory_mb: u32,
    /// Additional JVM arguments (e.g. GC flags).
    pub extra_jvm_args: Vec<String>,
    /// Game window default width.
    pub window_width: u32,
    /// Game window default height.
    pub window_height: u32,
    /// Whether game directory points to the version directory.
    #[serde(default = "default_true")]
    pub version_isolation: bool,
    /// Download source mirror (Official or BMCLAPI).
    #[serde(default)]
    pub download_source: DownloadSource,
    /// UI color scheme.
    #[serde(default)]
    pub theme: ThemePreference,
}

fn default_true() -> bool {
    true
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            game_dir: default_minecraft_dir(),
            selected_version: "1.21.4".to_string(),
            player_name: "Steve".to_string(),
            offline_mode: true,
            java_path: "java".to_string(),
            min_memory_mb: 1024,
            max_memory_mb: 4096,
            extra_jvm_args: Vec::new(),
            window_width: 854,
            window_height: 480,
            version_isolation: true,
            download_source: DownloadSource::Bmclapi,
            theme: ThemePreference::System,
        }
    }
}

impl LauncherConfig {
    /// Path to default configuration file inside the game directory.
    pub fn config_file_path(game_dir: &Path) -> PathBuf {
        game_dir.join("fxlauncher.json")
    }

    /// Fixed location of the launcher's own settings: `fxlauncher.json` in the default `.minecraft`.
    pub fn default_path() -> PathBuf {
        Self::config_file_path(&default_minecraft_dir())
    }

    /// Loads the settings at [`default_path`](Self::default_path), or defaults when absent or unreadable.
    pub fn load_or_default() -> Self {
        Self::load_from_path(&Self::default_path()).unwrap_or_default()
    }

    /// Loads launcher configuration from a JSON file.
    pub fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Saves launcher configuration to a JSON file (creates parent dirs if needed).
    pub fn save_to_path(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            ensure_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Ensures common game directories exist on disk.
    pub fn ensure_game_directories(&self) -> std::io::Result<()> {
        ensure_dir(&self.game_dir)?;
        ensure_dir(&versions_dir(&self.game_dir))?;
        ensure_dir(&libraries_dir(&self.game_dir))?;
        Ok(())
    }
}
