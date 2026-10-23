//! Desktop UI built with aegle. The view is declared in `main.aegle` / `parts.aegle`; Rust
//! implements the `host.*` actions it calls and pushes data back through its states.

mod actions;
mod content;
mod controller;
mod model;

pub use controller::{Controller, Msg, Post};

use crate::game::config::LauncherConfig;
use crate::net::task::TaskManager;
use aegle::loader::{Data, State};
use aegle::prelude::*;
use std::cell::OnceCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// Minecraft grass green, the launcher's single accent.
pub const ACCENT: Color = Color::rgb(0x3C, 0x85, 0x27);

pub fn light_theme() -> Theme {
    Theme { accent: ACCENT, on_accent: Color::WHITE, ..Theme::light() }
}

pub fn dark_theme() -> Theme {
    Theme { accent: ACCENT, on_accent: Color::WHITE, ..Theme::dark() }
}

/// Handles to the states and named controls of the mounted view.
pub struct Shell {
    pub root: Container,
    // 启动
    pub instances: State<Data>,
    pub instance_titles: State<Vec<String>>,
    pub current_index: State<i64>,
    pub current: State<String>,
    pub current_title: State<String>,
    pub current_detail: State<String>,
    pub current_stats: State<String>,
    pub play_status: State<String>,
    pub busy: State<bool>,
    pub crash_title: State<String>,
    pub crash_body: State<String>,
    pub task_badge: State<String>,
    pub player: TextField,
    pub launch: Button,
    pub tabs: Tabs,
    // 实例
    pub picked: State<String>,
    pub picked_title: State<String>,
    pub picked_detail: State<String>,
    pub picked_vanilla: State<bool>,
    pub mods: State<Data>,
    pub mods_summary: State<String>,
    pub mods_warning: State<String>,
    pub shaders: State<Data>,
    pub shader_off: State<bool>,
    pub saves: State<Data>,
    pub diagnosis: State<String>,
    pub instance_tabs: Tabs,
    // 下载
    pub versions: State<Data>,
    pub version_status: State<String>,
    pub chosen: State<String>,
    pub install_title: State<String>,
    pub loader: State<i64>,
    pub loader_status: State<String>,
    pub loader_items: State<Vec<String>>,
    pub loader_index: State<i64>,
    pub install_status: State<String>,
    pub hits: State<Data>,
    pub search_status: State<String>,
    pub mod_target: State<String>,
    pub download_tabs: Tabs,
    pub instance_name: TextField,
    pub install: Button,
    pub mod_query: TextField,
    // 任务
    pub tasks: State<Data>,
    // 设置
    pub memory: State<f32>,
    pub source: State<i64>,
    pub theme_mode: State<i64>,
    pub isolation: State<bool>,
    pub javas: State<Data>,
    pub java_current: State<String>,
    pub java_status: State<String>,
    pub saved_hint: State<String>,
    pub game_dir: TextField,
    pub java_path: TextField,
    pub jvm_args: TextField,
    pub memory_slider: Slider,
    pub window_width: NumberField,
    pub window_height: NumberField,
    // 确认
    pub confirm_open: State<bool>,
    pub confirm_text: State<String>,
}

/// Builds the launcher view into `root` and wires it to a controller.
/// `post` delivers background results back to the UI thread; `start` kicks off the initial
/// network fetches (headless renders pass `false`).
pub fn mount(root: &Container, config: LauncherConfig, manager: Arc<TaskManager>, post: Post, start: bool) -> Result<Rc<Controller>> {
    let cell: Rc<OnceCell<Rc<Controller>>> = Rc::default();
    actions::register(&cell);
    root.set_padding(0.0);
    root.set_gap(0.0, 0.0);
    let v = aegle::ui!(root, "src/ui/main.aegle")?;
    let shell = Shell {
        root: root.clone(),
        instances: v.instances, instance_titles: v.instance_titles, current_index: v.current_index, current: v.current, current_title: v.current_title,
        current_detail: v.current_detail, current_stats: v.current_stats, play_status: v.play_status, busy: v.busy,
        crash_title: v.crash_title, crash_body: v.crash_body, task_badge: v.task_badge,
        player: v.player, launch: v.launch, tabs: v.tabs,
        picked: v.picked, picked_title: v.picked_title, picked_detail: v.picked_detail,
        picked_vanilla: v.picked_vanilla, mods: v.mods, mods_summary: v.mods_summary,
        mods_warning: v.mods_warning, shaders: v.shaders, shader_off: v.shader_off, saves: v.saves,
        diagnosis: v.diagnosis, instance_tabs: v.instance_tabs,
        versions: v.versions, version_status: v.version_status, chosen: v.chosen,
        install_title: v.install_title, loader: v.loader, loader_status: v.loader_status,
        loader_items: v.loader_items, loader_index: v.loader_index,
        install_status: v.install_status, hits: v.hits, search_status: v.search_status,
        mod_target: v.mod_target, download_tabs: v.download_tabs,
        instance_name: v.instance_name, install: v.install, mod_query: v.mod_query,
        tasks: v.tasks,
        memory: v.memory, source: v.source, theme_mode: v.theme_mode, isolation: v.isolation,
        javas: v.javas, java_current: v.java_current, java_status: v.java_status,
        saved_hint: v.saved_hint, game_dir: v.game_dir, java_path: v.java_path, jvm_args: v.jvm_args,
        memory_slider: v.memory_slider, window_width: v.window_width, window_height: v.window_height,
        confirm_open: v.confirm_open, confirm_text: v.confirm_text,
    };
    shell.launch.set_accent(true);
    shell.install.set_accent(true);
    v.confirm_ok.set_accent(true);

    let controller = Rc::new(Controller::new(shell, config, manager, post));
    let _ = cell.set(controller.clone());
    controller.init(start);
    Ok(controller)
}

/// Opens the launcher window and runs the event loop.
pub fn run() -> Result<()> {
    let app = App::with_options(AppOptions {
        app_id: "io.github.fxlauncher".into(),
        theme: light_theme(),
        dark_theme: Some(dark_theme()),
        ..Default::default()
    })?;
    let window = app.window_with_options("FXLauncher", WindowOptions { width: 960, height: 600, ..Default::default() })?;

    let cell: Rc<OnceCell<Rc<Controller>>> = Rc::default();
    let receiver = cell.clone();
    let proxy = app.proxy(move |msg: Msg| {
        if let Some(c) = receiver.get() {
            c.handle(msg);
        }
    })?;
    let proxy = Mutex::new(proxy);
    let post: Post = Arc::new(move |msg| {
        let _ = proxy.lock().unwrap().send(msg);
    });

    let controller = mount(&window, LauncherConfig::load_or_default(), Arc::new(TaskManager::new()), post, true)?;
    let _ = cell.set(controller);
    app.run()
}
