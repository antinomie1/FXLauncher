//! FXLauncher: A lightweight cross-platform Minecraft launcher backend.

pub mod env;
pub mod game;
pub mod net;

// Top-level aliases for backwards compatibility with tests and callers
pub use env::paths;
pub use env::java;
pub use game::account;
pub use game::config;
pub use game::launch;
pub use game::version;
pub use net::download as downloader;
pub use net::mods as mod_manager;
pub use net::task;

pub use env::java::{JavaError, JavaInfo, extract_major_version, find_system_java_candidates, parse_java_version, probe_java_binary, required_java_major};
pub use env::paths::{arch_name, assets_dir, classpath_separator, default_minecraft_dir, libraries_dir, natives_dir, os_name, version_dir, version_mods_dir, versions_dir};
pub use game::account::{Account, offline_uuid};
pub use game::config::{ConfigError, LauncherConfig};
pub use game::launch::{LaunchContext, LaunchError, build_game_args, build_jvm_args, build_launch_command, build_token_map, join_classpaths, prepare_and_launch, replace_tokens, spawn_game};
pub use game::version::{ArgumentItem, ArgumentValue, Artifact, AssetIndexRef, Library, LibraryDownloads, ModernArguments, OsRule, Rule, VersionError, VersionInfo, collect_classpaths, evaluate_rules, load_version_info, maven_to_path, scan_local_versions};
pub use net::download::{DownloadSource, DownloaderError, LatestVersion, ManifestVersion, VersionManifest, fetch_version_json, fetch_version_manifest, install_version, manifest_url, setup_isolated_version_dir, transform_url};
pub use net::mods::{InstalledMod, ModError, ModFileInfo, ModLoader, ModSearchResult, download_mod_to_isolated_version, list_installed_mods, resolve_modrinth_file, search_curseforge, search_modrinth};
pub use net::task::{TaskError, TaskHandle, TaskManager, TaskSnapshot, TaskStatus, compute_file_sha1, download_file_with_task, spawn_download_task};
