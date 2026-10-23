//! Registration of the `host.*` actions called from the markup.

use super::controller::{Controller, Deletion, put};
use crate::game::config::ThemePreference;
use aegle::loader::markup::Type;
use aegle::loader::{Data, action};
use std::cell::OnceCell;
use std::path::PathBuf;
use std::rc::Rc;

fn text(args: &[Data]) -> String {
    match args.first() {
        Some(Data::String(s)) => s.to_string(),
        _ => String::new(),
    }
}

fn int(args: &[Data]) -> i64 {
    match args.first() {
        Some(Data::Int(i)) => *i,
        _ => 0,
    }
}

fn float(args: &[Data]) -> f32 {
    match args.first() {
        Some(Data::Float(f)) => *f,
        Some(Data::Int(i)) => *i as f32,
        _ => 0.0,
    }
}

fn flag(args: &[Data]) -> bool {
    matches!(args.first(), Some(Data::Bool(true)))
}

/// Registers every action before the view is built (building checks that each call exists).
pub(super) fn register(cell: &Rc<OnceCell<Rc<Controller>>>) {
    let on = |name: &str, params: &[Type], run: fn(&Controller, &[Data])| {
        let cell = cell.clone();
        action(name, params, move |args: &[Data]| {
            if let Some(c) = cell.get() {
                run(c, args);
            }
        });
    };
    use Type::{Bool, Float, Int, String as Str};

    // 启动 / navigation
    on("goto", &[Int], |c, a| c.goto(int(a).clamp(0, 4) as usize));
    on("launch", &[], |c, _| c.launch());
    on("pick_current", &[Int], |c, a| c.pick_current(int(a).max(0) as usize));
    on("set_player", &[Str], |c, a| {
        let name = text(a).trim().to_string();
        if crate::game::account::Account::validate_username(&name) {
            c.update_config(|cfg| cfg.player_name = name);
        }
    });
    on("show_diagnosis", &[], |c, _| c.show_diagnosis());

    // 实例
    on("pick_instance", &[Str], |c, a| c.pick_instance(&text(a)));
    on("use_picked", &[], |c, _| c.use_picked());
    on("refresh_instance", &[], |c, _| c.refresh_instance());
    on("toggle_mod", &[Str], |c, a| c.toggle_mod(&text(a)));
    on("delete_mod", &[Str], |c, a| c.request(Deletion::Mod(text(a))));
    on("set_shader", &[Str], |c, a| c.set_shader(&text(a)));
    on("delete_shader", &[Str], |c, a| c.request(Deletion::Shader(text(a))));
    on("backup_save", &[Str], |c, a| c.backup_save(&text(a)));
    on("delete_save", &[Str], |c, a| c.request(Deletion::Save(text(a))));
    on("analyze", &[], |c, _| c.analyze());
    on("goto_mod_search", &[], |c, _| c.goto_mod_search());
    on("confirm", &[], |c, _| c.confirm());

    // 下载
    on("filter_versions", &[Str], |c, a| c.filter_versions(&text(a)));
    on("set_snapshots", &[Bool], |c, a| c.set_snapshots(flag(a)));
    on("pick_version", &[Str], |c, a| c.pick_version(&text(a)));
    on("set_loader", &[Int], |c, a| c.set_loader(int(a)));
    on("install", &[], |c, _| c.install());
    on("search_mods", &[Str], |c, a| c.search_mods(&text(a)));
    on("search_mods_button", &[], |c, _| c.search_mods(&c.shell.mod_query.text()));
    on("install_mod", &[Str], |c, a| c.install_mod(&text(a)));

    // 任务
    on("pause_task", &[Int], |c, a| {
        c.manager.pause_task(int(a) as u64);
        c.refresh_tasks();
    });
    on("resume_task", &[Int], |c, a| {
        c.manager.resume_task(int(a) as u64);
        c.refresh_tasks();
    });
    on("cancel_task", &[Int], |c, a| {
        c.manager.cancel_task(int(a) as u64);
        c.refresh_tasks();
    });
    on("clear_tasks", &[], |c, _| {
        c.manager.clear_finished();
        c.refresh_tasks();
    });

    // 设置
    on("apply_game_dir", &[], |c, _| {
        let dir = PathBuf::from(c.shell.game_dir.text().trim());
        if dir.as_os_str().is_empty() {
            return;
        }
        c.update_config(|cfg| cfg.game_dir = dir);
        if let Err(e) = c.config().ensure_game_directories() {
            put(&c.shell.saved_hint, format!("无法创建游戏目录：{e}"));
        }
        c.reload_instances(None);
    });
    on("set_isolation", &[Bool], |c, a| c.update_config(|cfg| cfg.version_isolation = flag(a)));
    on("apply_java", &[], |c, _| {
        let path = c.shell.java_path.text().trim().to_string();
        put(&c.shell.java_current, path.clone());
        c.update_config(|cfg| cfg.java_path = path);
    });
    on("detect_java", &[], |c, _| c.detect_java());
    on("pick_java", &[Str], |c, a| {
        let path = text(a);
        c.shell.java_path.set_text(&path);
        put(&c.shell.java_current, path.clone());
        c.update_config(|cfg| cfg.java_path = path);
    });
    on("set_memory", &[Float], |c, a| {
        let mb = float(a).round() as u32;
        c.update_config(|cfg| {
            cfg.max_memory_mb = mb;
            cfg.min_memory_mb = cfg.min_memory_mb.min(mb);
        });
    });
    on("apply_jvm_args", &[], |c, _| {
        let args = c.shell.jvm_args.text().split_whitespace().map(str::to_string).collect();
        c.update_config(|cfg| cfg.extra_jvm_args = args);
    });
    on("set_window_width", &[Float], |c, a| c.update_config(|cfg| cfg.window_width = float(a) as u32));
    on("set_window_height", &[Float], |c, a| c.update_config(|cfg| cfg.window_height = float(a) as u32));
    on("set_source", &[Int], |c, a| c.set_source(int(a)));
    on("set_theme", &[Int], |c, a| {
        let pref = match int(a) {
            1 => ThemePreference::Light,
            2 => ThemePreference::Dark,
            _ => ThemePreference::System,
        };
        c.apply_theme(pref);
        c.update_config(|cfg| cfg.theme = pref);
    });
}
