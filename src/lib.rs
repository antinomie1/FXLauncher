//! FXLauncher: A lightweight cross-platform Minecraft launcher backend.

pub mod diag;
pub mod env;
pub mod game;
pub mod net;

// Top-level aliases for backwards compatibility with tests and callers
pub use diag as diagnostics;
pub use env::paths;
pub use env::java;
pub use game::account;
pub use game::config;
pub use game::launch;
pub use game::loader;
pub use game::mods as instance_mods;
pub use game::saves as instance_saves;
pub use game::shaders as instance_shaders;
pub use game::version;
pub use net::download as downloader;
pub use net::mods as mod_manager;
pub use net::task;

pub use diag::{AnalysisReport, CrashCause, CrashReport, CrashSeverity, LogEntry, LogLevel, analyze_crash_text, analyze_latest_crash, extract_error_logs, find_latest_crash_report, parse_crash_report, parse_log_line};
pub use env::java::{JavaError, JavaInfo, extract_major_version, find_system_java_candidates, parse_java_version, probe_java_binary, required_java_major};
pub use env::paths::{arch_name, assets_dir, classpath_separator, default_minecraft_dir, libraries_dir, natives_dir, os_name, version_config_dir, version_dir, version_jar_path, version_json_path, version_mods_dir, version_saves_dir, version_shaderpacks_dir, versions_dir};
pub use game::account::{Account, offline_uuid};
pub use game::config::{ConfigError, LauncherConfig};
pub use game::launch::{LaunchContext, LaunchError, build_game_args, build_jvm_args, build_launch_command, build_token_map, join_classpaths, prepare_and_launch, replace_tokens, spawn_game};
pub use game::loader::{InstanceLoaderInfo, detect_instance_loader, detect_loader_from_info};
pub use game::mods::{DuplicateModAlert, ModAnalysisReport, ModEntry, analyze_instance_mods, deduce_mod_info_from_filename, delete_mod, disable_mod, enable_mod, list_instance_mods, toggle_mod};
pub use game::saves::{WorldSaveEntry, backup_world_save, compute_save_size, delete_world_save, list_world_saves, rename_world_save};
pub use game::shaders::{ShaderPackEntry, delete_shaderpack, get_active_shaderpack, list_shaderpacks, options_shaders_path, set_active_shaderpack};
pub use game::version::{ArgumentItem, ArgumentValue, Artifact, AssetIndexRef, Library, LibraryDownloads, ModernArguments, OsRule, Rule, VersionError, VersionInfo, collect_classpaths, evaluate_rules, load_version_info, maven_to_path, scan_local_versions};
pub use net::download::{DownloadSource, DownloaderError, LatestVersion, ManifestVersion, VersionManifest, fetch_version_json, fetch_version_manifest, install_version, manifest_url, setup_isolated_version_dir, transform_url};
pub use net::mods::{InstalledMod, ModError, ModFileInfo, ModLoader, ModSearchResult, download_mod_to_isolated_version, list_installed_mods, resolve_modrinth_file, search_curseforge, search_modrinth};
pub use net::task::{TaskError, TaskHandle, TaskManager, TaskSnapshot, TaskStatus, compute_file_sha1, download_file_with_task, spawn_download_task};
