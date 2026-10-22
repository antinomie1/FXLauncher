//! Game models, configurations, account authentication, version metadata, and launching.

pub mod account;
pub mod config;
pub mod launch;
pub mod version;

pub use account::{Account, offline_uuid};
pub use config::{ConfigError, LauncherConfig};
pub use launch::{LaunchContext, LaunchError, build_game_args, build_jvm_args, build_launch_command, build_token_map, join_classpaths, prepare_and_launch, replace_tokens, spawn_game};
pub use version::{ArgumentItem, ArgumentValue, Artifact, AssetIndexRef, Library, LibraryDownloads, ModernArguments, OsRule, Rule, VersionError, VersionInfo, collect_classpaths, evaluate_rules, load_version_info, maven_to_path, scan_local_versions};
