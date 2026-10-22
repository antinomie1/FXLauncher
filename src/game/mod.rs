//! Game models, configurations, account authentication, version metadata, mod & shader management, and world saves.

pub mod account;
pub mod config;
pub mod launch;
pub mod loader;
pub mod mods;
pub mod saves;
pub mod shaders;
pub mod version;

pub use account::{Account, offline_uuid};
pub use config::{ConfigError, LauncherConfig};
pub use launch::{LaunchContext, LaunchError, build_game_args, build_jvm_args, build_launch_command, build_token_map, join_classpaths, prepare_and_launch, replace_tokens, spawn_game};
pub use loader::{InstanceLoaderInfo, detect_instance_loader, detect_loader_from_info};
pub use mods::{DuplicateModAlert, ModAnalysisReport, ModEntry, analyze_instance_mods, deduce_mod_info_from_filename, delete_mod, disable_mod, enable_mod, list_instance_mods, toggle_mod};
pub use saves::{WorldSaveEntry, backup_world_save, compute_save_size, delete_world_save, list_world_saves, rename_world_save};
pub use shaders::{ShaderPackEntry, delete_shaderpack, get_active_shaderpack, list_shaderpacks, options_shaders_path, set_active_shaderpack};
pub use version::{ArgumentItem, ArgumentValue, Artifact, AssetIndexRef, Library, LibraryDownloads, ModernArguments, OsRule, Rule, VersionError, VersionInfo, collect_classpaths, evaluate_rules, load_version_info, maven_to_path, scan_local_versions};
