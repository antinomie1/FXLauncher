//! FXLauncher: A lightweight cross-platform Minecraft launcher backend.

pub mod account;
pub mod config;
pub mod downloader;
pub mod java;
pub mod launch;
pub mod mod_manager;
pub mod paths;
pub mod task;
pub mod version;

pub use account::Account;
pub use config::LauncherConfig;
pub use downloader::{DownloadSource, fetch_version_manifest, install_version};
pub use java::JavaInfo;
pub use launch::{LaunchContext, prepare_and_launch};
pub use mod_manager::{
    ModLoader, download_mod_to_isolated_version, list_installed_mods, resolve_modrinth_file,
    search_curseforge, search_modrinth, version_mods_dir,
};
pub use task::{TaskManager, TaskSnapshot, TaskStatus};
pub use version::VersionInfo;
