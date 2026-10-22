//! Networking, non-blocking task management, game downloader, and mod managers.

pub mod client;
pub mod download;
pub mod mods;
pub mod task;

pub use client::{DEFAULT_TIMEOUT, DEFAULT_USER_AGENT, HttpError, http_get, http_get_json, url_encode};
pub use download::{DownloadSource, DownloaderError, LatestVersion, ManifestVersion, VersionManifest, fetch_version_json, fetch_version_manifest, install_version, manifest_url, setup_isolated_version_dir, transform_url};
pub use mods::{InstalledMod, ModError, ModFileInfo, ModLoader, ModSearchResult, download_mod_to_isolated_version, list_installed_mods, resolve_modrinth_file, search_curseforge, search_modrinth};
pub use task::{TaskError, TaskHandle, TaskManager, TaskSnapshot, TaskStatus, compute_file_sha1, download_file_with_task, spawn_download_task};
