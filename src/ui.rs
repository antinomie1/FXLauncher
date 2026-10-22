//! UI controller and Aegle event wiring for FXLauncher.

use crate::account::Account;
use crate::config::LauncherConfig;
use crate::java::{find_system_java_candidates, probe_java_binary};
use crate::launch::{LaunchContext, build_launch_command, spawn_game};
use crate::paths::default_minecraft_dir;
use crate::version::{load_version_info, scan_local_versions};
use aegle::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Helper to append a line to the launcher's log view.
fn append_log(view: &TextField, msg: &str) {
    let current = view.text();
    if current.is_empty() {
        view.set_text(msg);
    } else {
        view.set_text(&format!("{current}\n{msg}"));
    }
}

/// Sets up the complete launcher user interface with event listeners.
pub fn setup_launcher_ui(app: &App) -> Result<()> {
    let default_dir = default_minecraft_dir();
    let config_path = LauncherConfig::config_file_path(&default_dir);
    let initial_config = LauncherConfig::load_from_path(&config_path).unwrap_or_else(|_| {
        let mut cfg = LauncherConfig::default();
        cfg.game_dir = default_dir;
        cfg
    });

    let view = aegle::ui!(app, "ui/main.aegle")?;

    // Populate initial UI state from config
    view.player_name.set_text(&initial_config.player_name);
    view.offline_mode.set_checked(initial_config.offline_mode);
    view.version_input.set_text(&initial_config.selected_version);
    view.java_input.set_text(&initial_config.java_path);
    view.memory_slider.set_value(initial_config.max_memory_mb as f64);
    view.memory_display
        .set_text(&format!("{} MB", initial_config.max_memory_mb));
    view.game_dir_input
        .set_text(&initial_config.game_dir.to_string_lossy());

    let is_dark_theme = Rc::new(RefCell::new(false));
    let current_config = Rc::new(RefCell::new(initial_config));

    // 1. Theme toggle
    {
        let window = view.root.clone();
        let dark_flag = is_dark_theme.clone();
        view.theme_toggle.on_click(move |_| {
            let mut is_dark = dark_flag.borrow_mut();
            *is_dark = !*is_dark;
            if *is_dark {
                window.set_theme(Theme::dark());
            } else {
                window.set_theme(Theme::light());
            }
        });
    }

    // 2. Memory slider change
    {
        let display = view.memory_display.clone();
        view.memory_slider.on_change(move |slider| {
            let val = slider.value() as u32;
            display.set_text(&format!("{val} MB"));
        });
    }

    // 3. Java detection button
    {
        let java_field = view.java_input.clone();
        let log = view.log_view.clone();
        let status = view.status_label.clone();
        view.detect_java.on_click(move |_| {
            status.set_text("正在检测 Java 运行时...");
            append_log(&log, "[Java] 扫描系统 Java 候选路径...");
            let candidates = find_system_java_candidates();
            let mut detected = None;

            for candidate in candidates {
                if let Ok(info) = probe_java_binary(&candidate) {
                    append_log(
                        &log,
                        &format!(
                            "[Java] 发现有效运行时: {} (版本 {}, {} 位)",
                            info.path.display(),
                            info.version_string,
                            if info.is_64_bit { 64 } else { 32 }
                        ),
                    );
                    if detected.is_none() {
                        detected = Some(info);
                    }
                }
            }

            if let Some(info) = detected {
                java_field.set_text(&info.path.to_string_lossy());
                status.set_text("Java 检测完成");
                append_log(
                    &log,
                    &format!("[Java] 已选用默认运行时: {}", info.path.display()),
                );
            } else {
                status.set_text("未检测到有效 Java");
                append_log(&log, "[Java 警告] 未在系统标准路径中找到 Java。");
            }
        });
    }

    // 4. Scan versions button
    {
        let dir_field = view.game_dir_input.clone();
        let ver_field = view.version_input.clone();
        let log = view.log_view.clone();
        let status = view.status_label.clone();
        view.refresh_versions.on_click(move |_| {
            let dir_str = dir_field.text();
            let game_dir = PathBuf::from(&dir_str);
            append_log(&log, &format!("[版本] 扫描目录: {}", game_dir.display()));

            let versions = scan_local_versions(&game_dir);
            if versions.is_empty() {
                status.set_text("未找到已安装版本");
                append_log(
                    &log,
                    "[版本] 该目录下暂无已安装版本，请确认 .minecraft/versions 路径。",
                );
            } else {
                status.set_text(&format!("找到 {} 个可用版本", versions.len()));
                append_log(
                    &log,
                    &format!("[版本] 已安装列表: {}", versions.join(", ")),
                );
                if let Some(latest) = versions.last() {
                    ver_field.set_text(latest);
                }
            }
        });
    }

    // 5. Save configuration button
    {
        let p_name = view.player_name.clone();
        let off_mode = view.offline_mode.clone();
        let ver_field = view.version_input.clone();
        let j_field = view.java_input.clone();
        let mem_slider = view.memory_slider.clone();
        let dir_field = view.game_dir_input.clone();
        let log = view.log_view.clone();
        let status = view.status_label.clone();
        let cfg_ref = current_config.clone();

        view.save_btn.on_click(move |_| {
            let mut cfg = cfg_ref.borrow_mut();
            cfg.player_name = p_name.text().trim().to_string();
            cfg.offline_mode = off_mode.is_checked();
            cfg.selected_version = ver_field.text().trim().to_string();
            cfg.java_path = j_field.text().trim().to_string();
            cfg.max_memory_mb = mem_slider.value() as u32;
            cfg.game_dir = PathBuf::from(dir_field.text().trim());

            let save_path = LauncherConfig::config_file_path(&cfg.game_dir);
            match cfg.save_to_path(&save_path) {
                Ok(()) => {
                    status.set_text("配置已保存");
                    append_log(&log, &format!("[配置] 成功保存配置至 {}", save_path.display()));
                }
                Err(e) => {
                    status.set_text("配置保存失败");
                    append_log(&log, &format!("[错误] 保存配置失败: {e}"));
                }
            }
        });
    }

    // 6. Launch Minecraft button
    {
        let p_name = view.player_name.clone();
        let off_mode = view.offline_mode.clone();
        let ver_field = view.version_input.clone();
        let j_field = view.java_input.clone();
        let mem_slider = view.memory_slider.clone();
        let dir_field = view.game_dir_input.clone();
        let log = view.log_view.clone();
        let status = view.status_label.clone();
        let cfg_ref = current_config.clone();

        view.launch_btn.on_click(move |_| {
            let username = p_name.text().trim().to_string();
            if !Account::validate_username(&username) {
                status.set_text("玩家用户名不合法");
                append_log(
                    &log,
                    "[启动失败] 玩家名称必须为 1-16 位字母、数字或下划线。",
                );
                return;
            }

            let version_id = ver_field.text().trim().to_string();
            if version_id.is_empty() {
                status.set_text("未指定版本");
                append_log(&log, "[启动失败] 请输入或选择需要启动的 Minecraft 版本。");
                return;
            }

            let mut cfg = cfg_ref.borrow_mut();
            cfg.player_name = username.clone();
            cfg.offline_mode = off_mode.is_checked();
            cfg.selected_version = version_id.clone();
            cfg.java_path = j_field.text().trim().to_string();
            cfg.max_memory_mb = mem_slider.value() as u32;
            cfg.game_dir = PathBuf::from(dir_field.text().trim());

            append_log(
                &log,
                &format!(
                    "[启动] 正在准备启动 Minecraft {} (玩家: {})...",
                    version_id, username
                ),
            );

            // Attempt to load version definition
            let version_info = match load_version_info(&cfg.game_dir, &version_id) {
                Ok(info) => info,
                Err(e) => {
                    status.set_text("加载版本元数据失败");
                    append_log(
                        &log,
                        &format!(
                            "[启动错误] 无法读取版本 '{version_id}' 的 JSON 元数据: {e}。\n提示：请确认 {}/versions/{version_id}/{version_id}.json 存在。",
                            cfg.game_dir.display()
                        ),
                    );
                    return;
                }
            };

            let account = if cfg.offline_mode {
                Account::new_offline(&username)
            } else {
                Account::Offline {
                    username: username.clone(),
                    uuid: "00000000-0000-0000-0000-000000000000".to_string(),
                }
            };

            let ctx = LaunchContext {
                config: &cfg,
                version: &version_info,
                account: &account,
            };

            match build_launch_command(&ctx) {
                Ok(cmd) => {
                    append_log(
                        &log,
                        &format!("[启动命令] {:?} {:?}", cmd.get_program(), cmd.get_args().collect::<Vec<_>>()),
                    );
                    match spawn_game(&ctx) {
                        Ok(child) => {
                            status.set_text("游戏进程已启动");
                            append_log(
                                &log,
                                &format!("[启动成功] 游戏进程已启动，PID: {}", child.id()),
                            );
                        }
                        Err(e) => {
                            status.set_text("子进程创建失败");
                            append_log(&log, &format!("[进程错误] 无法启动 Java 进程: {e}"));
                        }
                    }
                }
                Err(e) => {
                    status.set_text("组装启动参数失败");
                    append_log(&log, &format!("[参数错误] 构建启动命令出错: {e}"));
                }
            }
        });
    }

    Ok(())
}
