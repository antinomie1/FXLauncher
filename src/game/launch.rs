//! Minecraft launch command assembly and process management.

use crate::env::paths::{assets_dir, classpath_separator, libraries_dir, natives_dir, os_name, version_dir};
use crate::game::account::Account;
use crate::game::config::LauncherConfig;
use crate::game::natives::extract_natives;
use crate::game::version::{
    ArgumentItem, ArgumentValue, VersionInfo, collect_classpaths, evaluate_rules_with, load_version_info,
};
use crate::net::install::legacy_assets_dir;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("I/O error spawning game process: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("Missing main class in version metadata")]
    MissingMainClass,
}

/// Parameters required to construct a game launch invocation.
#[derive(Debug, Clone)]
pub struct LaunchContext<'a> {
    pub config: &'a LauncherConfig,
    pub version: &'a VersionInfo,
    pub account: &'a Account,
}

/// Launcher features reported to argument rules; everything else (demo, quick play) stays off.
pub const LAUNCH_FEATURES: &[&str] = &["has_custom_resolution"];

/// Token replacer for Minecraft argument templates (e.g. `${auth_player_name}`).
pub fn replace_tokens(template: &str, tokens: &HashMap<&str, String>) -> String {
    let mut result = template.to_string();
    for (k, v) in tokens {
        let pattern = format!("${{{k}}}");
        result = result.replace(&pattern, v);
    }
    result
}

/// Assembles token map from launch context.
pub fn build_token_map(ctx: &LaunchContext<'_>, classpath_str: &str) -> HashMap<&'static str, String> {
    let effective_game_dir = if ctx.config.version_isolation {
        version_dir(&ctx.config.game_dir, &ctx.version.id)
    } else {
        ctx.config.game_dir.clone()
    };
    let game_dir_str = effective_game_dir.to_string_lossy().to_string();
    let assets_dir_str = assets_dir(&ctx.config.game_dir).to_string_lossy().to_string();
    let natives_dir_str = natives_dir(&ctx.config.game_dir, &ctx.version.id)
        .to_string_lossy()
        .to_string();
    let asset_index = ctx
        .version
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .or_else(|| ctx.version.assets.clone())
        .unwrap_or_else(|| ctx.version.id.clone());
    let game_assets = if matches!(asset_index.as_str(), "legacy" | "pre-1.6") {
        legacy_assets_dir(&ctx.config.game_dir).to_string_lossy().to_string()
    } else {
        assets_dir_str.clone()
    };

    let mut map = HashMap::new();
    map.insert("auth_player_name", ctx.account.username().to_string());
    map.insert("version_name", ctx.version.id.clone());
    map.insert("game_directory", game_dir_str);
    map.insert("assets_root", assets_dir_str.clone());
    map.insert("game_assets", game_assets);
    map.insert("assets_index_name", asset_index);
    map.insert("auth_uuid", ctx.account.uuid().to_string());
    map.insert("auth_access_token", ctx.account.access_token().to_string());
    map.insert("auth_session", ctx.account.access_token().to_string());
    map.insert("auth_xuid", "0".to_string());
    map.insert("clientid", String::new());
    map.insert("user_properties", "{}".to_string());
    map.insert("user_type", ctx.account.user_type().to_string());
    map.insert("version_type", ctx.version.version_type.clone());
    map.insert("resolution_width", ctx.config.window_width.to_string());
    map.insert("resolution_height", ctx.config.window_height.to_string());
    map.insert("natives_directory", natives_dir_str);
    map.insert("launcher_name", "FXLauncher".to_string());
    map.insert("launcher_version", env!("CARGO_PKG_VERSION").to_string());
    map.insert("classpath", classpath_str.to_string());
    map.insert("classpath_separator", classpath_separator().to_string());
    map.insert(
        "library_directory",
        libraries_dir(&ctx.config.game_dir).to_string_lossy().to_string(),
    );
    map
}

/// Formats classpath list into a single delimited string.
pub fn join_classpaths(paths: &[PathBuf]) -> String {
    let sep = classpath_separator();
    paths
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join(&sep.to_string())
}

/// Builds JVM argument list.
pub fn build_jvm_args(ctx: &LaunchContext<'_>, tokens: &HashMap<&str, String>, classpath_str: &str) -> Vec<String> {
    let mut args = Vec::new();

    // Standard memory bounds
    args.push(format!("-Xms{}M", ctx.config.min_memory_mb));
    args.push(format!("-Xmx{}M", ctx.config.max_memory_mb));

    // Custom user flags
    args.extend(ctx.config.extra_jvm_args.clone());

    let current_os = os_name();
    let mut has_cp_in_modern = false;

    if let Some(modern) = &ctx.version.arguments {
        for item in &modern.jvm {
            match item {
                ArgumentItem::Simple(s) => {
                    let replaced = replace_tokens(s, tokens);
                    if replaced.contains("-cp") || replaced.contains("-classpath") {
                        has_cp_in_modern = true;
                    }
                    args.push(replaced);
                }
                ArgumentItem::Conditional { rules, value } => {
                    if evaluate_rules_with(rules, current_os, LAUNCH_FEATURES) {
                        match value {
                            ArgumentValue::Single(s) => args.push(replace_tokens(s, tokens)),
                            ArgumentValue::Multiple(list) => {
                                for s in list {
                                    args.push(replace_tokens(s, tokens));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !has_cp_in_modern {
        let natives = natives_dir(&ctx.config.game_dir, &ctx.version.id);
        args.push(format!("-Djava.library.path={}", natives.to_string_lossy()));
        args.push("-cp".to_string());
        args.push(classpath_str.to_string());
    }

    args
}

/// Builds Minecraft game argument list.
pub fn build_game_args(ctx: &LaunchContext<'_>, tokens: &HashMap<&str, String>) -> Vec<String> {
    let mut args = Vec::new();
    let current_os = os_name();

    if let Some(modern) = &ctx.version.arguments {
        for item in &modern.game {
            match item {
                ArgumentItem::Simple(s) => args.push(replace_tokens(s, tokens)),
                ArgumentItem::Conditional { rules, value } => {
                    if evaluate_rules_with(rules, current_os, LAUNCH_FEATURES) {
                        match value {
                            ArgumentValue::Single(s) => args.push(replace_tokens(s, tokens)),
                            ArgumentValue::Multiple(list) => {
                                for s in list {
                                    args.push(replace_tokens(s, tokens));
                                }
                            }
                        }
                    }
                }
            }
        }
    } else if let Some(legacy) = &ctx.version.legacy_arguments {
        for part in legacy.split_whitespace() {
            args.push(replace_tokens(part, tokens));
        }
    }

    args
}

/// Constructs the complete `std::process::Command` ready to launch Minecraft.
pub fn build_launch_command(ctx: &LaunchContext<'_>) -> Result<Command, LaunchError> {
    if ctx.version.main_class.trim().is_empty() {
        return Err(LaunchError::MissingMainClass);
    }

    let classpaths = collect_classpaths(&ctx.config.game_dir, ctx.version);
    let cp_str = join_classpaths(&classpaths);
    let tokens = build_token_map(ctx, &cp_str);

    let jvm_args = build_jvm_args(ctx, &tokens, &cp_str);
    let game_args = build_game_args(ctx, &tokens);

    let mut cmd = Command::new(&ctx.config.java_path);
    let effective_game_dir = if ctx.config.version_isolation {
        version_dir(&ctx.config.game_dir, &ctx.version.id)
    } else {
        ctx.config.game_dir.clone()
    };
    cmd.current_dir(&effective_game_dir);
    cmd.args(&jvm_args);
    cmd.arg(&ctx.version.main_class);
    cmd.args(&game_args);

    Ok(cmd)
}

/// Spawns the Minecraft process.
pub fn spawn_game(ctx: &LaunchContext<'_>) -> Result<Child, LaunchError> {
    let mut cmd = build_launch_command(ctx)?;
    let child = cmd.spawn()?;
    Ok(child)
}

/// Prepares launch context and spawns Minecraft, returning either the Child process or an error message.
pub fn prepare_and_launch(config: &LauncherConfig, account: &Account) -> Result<Child, String> {
    prepare_launch_command(config, account)?
        .spawn()
        .map_err(|e| format!("启动游戏子进程失败: {e}"))
}

/// Java major version a version needs: its `javaVersion` metadata, else derived from the id.
pub fn required_java_for(info: &VersionInfo) -> u32 {
    info.java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or_else(|| crate::env::java::required_java_major(info.client_jar_id()))
}

/// Validates the configuration, extracts natives and builds the launch command without spawning it.
/// A Microsoft account's token must already be fresh (see [`crate::net::auth::ensure_fresh`]).
pub fn prepare_launch_command(config: &LauncherConfig, account: &Account) -> Result<Command, String> {
    if !account.is_microsoft() && !Account::validate_username(account.username()) {
        return Err("玩家用户名不合法：必须为 1-16 位字母、数字或下划线。".to_string());
    }
    if config.selected_version.trim().is_empty() {
        return Err("未指定 Minecraft 版本号。".to_string());
    }

    let version_info = load_version_info(&config.game_dir, &config.selected_version)
        .map_err(|e| format!("无法读取版本 '{}' 的 JSON 元数据: {e}", config.selected_version))?;

    // Natives may be missing when files were copied from another launcher; extraction is idempotent.
    extract_natives(&config.game_dir, &version_info)
        .map_err(|e| format!("解压本地库失败: {e}"))?;

    let ctx = LaunchContext {
        config,
        version: &version_info,
        account,
    };

    build_launch_command(&ctx).map_err(|e| format!("无法组装启动命令: {e}"))
}
