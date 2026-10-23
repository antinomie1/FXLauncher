//! Networking, non-blocking task management, game downloader, and mod managers.

pub mod batch;
pub mod client;
pub mod download;
pub mod fabric;
pub mod forge;
pub mod install;
pub mod mods;
pub mod task;

pub use batch::{DEFAULT_WORKERS, DownloadItem, download_all};
pub use client::{DEFAULT_TIMEOUT, DEFAULT_USER_AGENT, HttpError, http_get, http_get_json, http_get_json_any, url_encode};
pub use download::{DownloadSource, DownloaderError, LatestVersion, ManifestVersion, VersionManifest, candidate_urls, ensure_version_json, fetch_json, fetch_version_json, fetch_version_manifest, install_version, manifest_url, setup_isolated_version_dir, transform_url};
pub use fabric::{LoaderVersion, list_loader_versions};
pub use install::{AssetIndex, AssetObject, InstanceRequest, asset_items, asset_object_path, install_instance, install_instance_blocking, install_vanilla, legacy_assets_dir, library_items};
pub use mods::{InstalledMod, ModError, ModFileInfo, ModLoader, ModSearchResult, download_mod_to_isolated_version, list_installed_mods, resolve_modrinth_file, search_curseforge, search_modrinth};
pub use task::{TaskError, TaskHandle, TaskManager, TaskSnapshot, TaskStatus, compute_file_sha1, download_file_with_task, fetch_file, spawn_download_task};
