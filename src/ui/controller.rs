//! UI controller: owns the launcher configuration and reacts to host actions and background results.

use super::model::{self, instance_summary};
use super::{Shell, dark_theme, light_theme};
use crate::diag::{AnalysisReport, analyze_crash_text, find_latest_crash_report};
use crate::env::java::select_java;
use crate::env::paths::version_dir;
use crate::game::account::{Account, AccountStore};
use crate::game::config::{LauncherConfig, ThemePreference};
use crate::game::launch::{prepare_launch_command, required_java_for};
use crate::game::version::{load_version_info, scan_local_versions};
use crate::net::auth::{DeviceCode, ensure_fresh};
use crate::net::download::{DownloadSource, ManifestVersion};
use crate::net::fabric::LoaderVersion;
use crate::net::mods::ModSearchResult;
use crate::net::profile::McProfile;
use crate::net::task::{TaskManager, TaskStatus};
use aegle::loader::{State, StateValue};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

/// Results delivered from background threads to the UI thread.
pub enum Msg {
    Manifest(Result<Vec<ManifestVersion>, String>),
    LoaderVersions { key: (i64, String), result: Result<Vec<LoaderVersion>, String> },
    Hits(Result<Vec<ModSearchResult>, String>),
    Javas(Vec<(String, String)>),
    Launched(Result<String, String>),
    Exited { instance: String, code: Option<i32>, report: Option<AnalysisReport> },
    Tick,
    /// A device-code sign-in started; the user now enters the code in the browser.
    DeviceCode(Result<DeviceCode, String>),
    /// A device-code sign-in finished.
    SignedIn(Result<Account, String>),
    /// A saved account was signed in again (refreshed token, possibly a new name).
    AccountUpdated(Result<Account, String>),
    /// The Minecraft profile of the account with this UUID, read or just changed.
    Profile { uuid: String, result: Result<McProfile, String> },
    /// The PNG texture of that account's active skin.
    SkinTexture { uuid: String, result: Result<Vec<u8>, String> },
}

/// Thread-safe sender of [`Msg`] to the UI thread.
pub type Post = Arc<dyn Fn(Msg) + Send + Sync>;

/// What a finished task should refresh.
pub(super) enum TaskKind {
    Install(String),
    Mod(String),
    Backup(String),
}

/// A destructive action waiting for the confirmation dialog.
pub(super) enum Deletion {
    Mod(String),
    Shader(String),
    Save(String),
    Account(String),
}

pub(super) struct Model {
    pub config: LauncherConfig,
    pub accounts: AccountStore,
    /// Set to stop the device-code sign-in in progress.
    pub login_cancel: Arc<AtomicBool>,
    /// UUID of the account whose profile the 账户 page shows.
    pub profile_of: String,
    pub instance_ids: Vec<String>,
    pub manifest: Vec<ManifestVersion>,
    pub snapshots: bool,
    pub filter: String,
    pub loader_versions: Vec<LoaderVersion>,
    pub loader_key: (i64, String),
    pub hits: Vec<ModSearchResult>,
    pub pending: Option<Deletion>,
    pub task_kinds: HashMap<u64, TaskKind>,
    pub finished: HashSet<u64>,
}

pub struct Controller {
    pub(super) shell: Shell,
    pub(super) model: RefCell<Model>,
    pub(super) manager: Arc<TaskManager>,
    pub(super) post: Post,
    /// Whether network requests and the browser may be used (false in headless renders).
    pub(super) online: bool,
    ticking: Arc<AtomicBool>,
}

/// Sets a markup state; a rejected value is a programming error worth reporting, not a crash.
pub(super) fn put<T: StateValue>(state: &State<T>, value: T) {
    if let Err(e) = state.set(value) {
        eprintln!("fxlauncher ui: {e}");
    }
}

impl Controller {
    pub(super) fn new(shell: Shell, config: LauncherConfig, accounts: AccountStore, manager: Arc<TaskManager>, post: Post, online: bool) -> Self {
        let model = Model {
            config,
            accounts,
            login_cancel: Arc::default(),
            profile_of: String::new(),
            instance_ids: Vec::new(),
            manifest: Vec::new(),
            snapshots: false,
            filter: String::new(),
            loader_versions: Vec::new(),
            loader_key: (0, String::new()),
            hits: Vec::new(),
            pending: None,
            task_kinds: HashMap::new(),
            finished: HashSet::new(),
        };
        Self { shell, model: RefCell::new(model), manager, post, online, ticking: Arc::default() }
    }

    /// The mounted view's states and named controls.
    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    pub(super) fn config(&self) -> LauncherConfig {
        self.model.borrow().config.clone()
    }

    /// Fills controls from the configuration and starts the initial fetches.
    pub(super) fn init(&self) {
        let cfg = self.config();
        let sh = &self.shell;
        sh.game_dir.set_path(&cfg.game_dir.to_string_lossy());
        sh.java_path.set_path(&cfg.java_path);
        sh.jvm_args.set_text(&cfg.extra_jvm_args.join(" "));
        sh.memory_slider.set_value(cfg.max_memory_mb as f64);
        put(&sh.memory, cfg.max_memory_mb as f32);
        sh.window_width.set_value(cfg.window_width as f64);
        sh.window_height.set_value(cfg.window_height as f64);
        put(&sh.isolation, cfg.version_isolation);
        put(&sh.source, if cfg.download_source == DownloadSource::Bmclapi { 0 } else { 1 });
        put(&sh.java_current, cfg.java_path.clone());
        self.apply_theme(cfg.theme);
        self.reload_instances(Some(&cfg.selected_version));
        self.reload_accounts();
        if self.online {
            self.fetch_manifest();
        }
    }

    pub(super) fn save_config(&self) {
        let cfg = self.config();
        match cfg.save_to_path(&LauncherConfig::default_path()) {
            Ok(()) => put(&self.shell.saved_hint, String::new()),
            Err(e) => put(&self.shell.saved_hint, format!("保存设置失败：{e}")),
        }
    }

    pub(super) fn update_config(&self, edit: impl FnOnce(&mut LauncherConfig)) {
        edit(&mut self.model.borrow_mut().config);
        self.save_config();
    }

    pub fn handle(&self, msg: Msg) {
        match msg {
            Msg::Manifest(result) => self.on_manifest(result),
            Msg::LoaderVersions { key, result } => self.on_loader_versions(key, result),
            Msg::Hits(result) => self.on_hits(result),
            Msg::Javas(list) => {
                let count = list.len();
                put(&self.shell.javas, model::java_records(&list));
                put(&self.shell.java_status, if count == 0 { "没有找到 Java。请安装 Java 21（新版本）或 Java 8（1.16 及更早版本）。".into() } else { format!("找到 {count} 个 Java 运行时") });
            }
            Msg::Launched(result) => match result {
                Ok(java) => put(&self.shell.play_status, format!("游戏运行中 · {java}")),
                Err(e) => {
                    put(&self.shell.busy, false);
                    put(&self.shell.play_status, "启动失败".into());
                    put(&self.shell.crash_title, "无法启动游戏".into());
                    put(&self.shell.crash_body, e);
                }
            },
            Msg::Exited { instance, code, report } => self.on_exited(instance, code, report),
            Msg::Tick => self.refresh_tasks(),
            Msg::DeviceCode(result) => self.on_device_code(result),
            Msg::SignedIn(result) => self.on_signed_in(result),
            Msg::AccountUpdated(result) => self.on_account_updated(result),
            Msg::Profile { uuid, result } => self.on_profile(&uuid, result),
            Msg::SkinTexture { uuid, result } => self.on_skin_texture(&uuid, result),
        }
    }

    // ---------------- 启动 ----------------

    /// Rescans `versions/`, rebuilds the instance list and the play dropdown.
    pub(super) fn reload_instances(&self, prefer: Option<&str>) {
        let game_dir = self.config().game_dir;
        let ids = scan_local_versions(&game_dir);
        put(&self.shell.instances, model::instance_records(&game_dir, &ids));

        let current = self.shell.current.get();
        let selected = prefer
            .filter(|p| ids.iter().any(|i| i == p))
            .map(str::to_string)
            .or_else(|| ids.iter().find(|i| **i == current).cloned())
            .or_else(|| ids.first().cloned());

        let mut titles: Vec<String> = ids.iter().map(|id| instance_summary(&game_dir, id).0).collect();
        if titles.is_empty() {
            titles.push("无实例".to_string());
        }
        put(&self.shell.instance_titles, titles);
        self.model.borrow_mut().instance_ids = ids;
        self.set_current(selected.as_deref().unwrap_or(""));
    }

    /// Makes `id` the instance that the launch button starts.
    pub(super) fn set_current(&self, id: &str) {
        let sh = &self.shell;
        put(&sh.current, id.to_string());
        put(&sh.current_stats, String::new());
        if id.is_empty() {
            put(&sh.current_title, "还没有游戏实例".into());
            put(&sh.current_detail, "在「下载」页选择一个 Minecraft 版本，几分钟后就能开始游戏。".into());
            return;
        }
        let game_dir = self.config().game_dir;
        let (title, detail) = instance_summary(&game_dir, id);
        put(&sh.current_title, title);
        put(&sh.current_detail, detail);
        put(&sh.current_stats, model::instance_stats(&game_dir, id));
        let index = self.model.borrow().instance_ids.iter().position(|x| x == id).unwrap_or(0);
        put(&sh.current_index, index as i64);
        if self.config().selected_version != id {
            self.update_config(|c| c.selected_version = id.to_string());
        }
        if sh.picked.get().is_empty() {
            self.pick_instance(id);
        }
    }

    /// Called by the play page's instance dropdown.
    pub(super) fn pick_current(&self, index: usize) {
        let id = self.model.borrow().instance_ids.get(index).cloned();
        if let Some(id) = id {
            self.set_current(&id);
        }
    }

    pub(super) fn launch(&self) {
        let sh = &self.shell;
        let Some(account) = self.model.borrow().accounts.selected().cloned() else {
            put(&sh.play_status, "请先添加一个账户".into());
            self.goto(1);
            return;
        };
        let instance = sh.current.get();
        self.update_config(|c| c.selected_version = instance.clone());
        put(&sh.busy, true);
        put(&sh.crash_title, String::new());
        put(&sh.play_status, if account.is_microsoft() { "正在验证账户…" } else { "正在准备…" }.into());

        let mut cfg = self.config();
        let post = self.post.clone();
        thread::spawn(move || {
            let started = SystemTime::now();
            let account = match ensure_fresh(&account) {
                Ok(Some(fresh)) => {
                    post(Msg::AccountUpdated(Ok(fresh.clone())));
                    fresh
                }
                Ok(None) => account,
                Err(e) => return post(Msg::Launched(Err(format!("正版验证失败：{e}。可在「账户」页重新验证或重新登录。")))),
            };
            let prepared = (|| -> Result<(std::process::Child, std::path::PathBuf, String), String> {
                let info = load_version_info(&cfg.game_dir, &instance).map_err(|e| format!("无法读取实例 {instance}：{e}"))?;
                let java = select_java(&cfg.java_path, required_java_for(&info))?;
                cfg.java_path = java.path.to_string_lossy().to_string();
                let java_label = format!("Java {}", java.major_version);

                let logs = version_dir(&cfg.game_dir, &instance).join("logs");
                fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
                let log_path = logs.join("fxlauncher-output.log");
                let log = File::create(&log_path).map_err(|e| e.to_string())?;
                let err_log = log.try_clone().map_err(|e| e.to_string())?;
                let mut cmd = prepare_launch_command(&cfg, &account)?;
                cmd.stdout(log).stderr(err_log);
                let child = cmd.spawn().map_err(|e| format!("无法启动 Java（{}）：{e}", cfg.java_path))?;
                Ok((child, log_path, java_label))
            })();
            let (mut child, log_path, java_label) = match prepared {
                Ok(p) => p,
                Err(e) => return post(Msg::Launched(Err(e))),
            };
            post(Msg::Launched(Ok(java_label)));
            let code = child.wait().ok().and_then(|s| s.code());
            let report = if code == Some(0) {
                None
            } else {
                diagnose(&cfg.game_dir, &instance, &log_path, started)
            };
            post(Msg::Exited { instance, code, report });
        });
    }

    fn on_exited(&self, instance: String, code: Option<i32>, report: Option<AnalysisReport>) {
        let sh = &self.shell;
        put(&sh.busy, false);
        if code == Some(0) {
            put(&sh.play_status, "游戏已退出".into());
            return;
        }
        let code_text = code.map_or("被系统终止".to_string(), |c| format!("退出码 {c}"));
        put(&sh.play_status, format!("游戏异常退出（{code_text}）"));
        let (title, body) = match &report {
            Some(r) => (format!("游戏崩溃：{}", r.summary), r.suggestion.clone()),
            None => ("游戏异常退出".to_string(), format!("{instance} {code_text}，没有找到崩溃报告。")),
        };
        put(&sh.crash_title, title);
        put(&sh.crash_body, body);
        if let Some(r) = report {
            put(&sh.diagnosis, super::content::format_report(&r));
        }
    }

    // ---------------- 任务 ----------------

    /// Starts the progress ticker; it stops by itself once no task is active.
    pub(super) fn ensure_ticker(&self) {
        self.refresh_tasks();
        if self.ticking.swap(true, Ordering::SeqCst) {
            return;
        }
        let (post, manager, ticking) = (self.post.clone(), self.manager.clone(), self.ticking.clone());
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(400));
                post(Msg::Tick);
                if !manager.has_active() {
                    ticking.store(false, Ordering::SeqCst);
                    post(Msg::Tick);
                    break;
                }
            }
        });
    }

    pub(super) fn refresh_tasks(&self) {
        let snapshots = self.manager.list_snapshots();
        put(&self.shell.tasks, model::task_records(&snapshots));
        let active: Vec<_> = snapshots.iter().filter(|t| !t.status.is_finished()).collect();
        let badge = match active.len() {
            0 => String::new(),
            1 => format!("{} · {:.0}%", active[0].title, active[0].progress_percent),
            n => format!("{n} 个任务进行中"),
        };
        put(&self.shell.task_badge, badge);

        for t in snapshots.iter().filter(|t| t.status.is_finished()) {
            if !self.model.borrow_mut().finished.insert(t.id) {
                continue;
            }
            let kind = self.model.borrow_mut().task_kinds.remove(&t.id);
            let done = t.status == TaskStatus::Completed;
            match kind {
                Some(TaskKind::Install(id)) if done => {
                    put(&self.shell.install_status, format!("{id} 安装完成"));
                    self.reload_instances(Some(&id));
                }
                Some(TaskKind::Install(id)) => put(&self.shell.install_status, format!("{id} 安装未完成，详见「任务」")),
                Some(TaskKind::Mod(instance) | TaskKind::Backup(instance)) if self.shell.picked.get() == instance => {
                    self.load_instance_content(&instance);
                }
                _ => {}
            }
        }
    }

    // ---------------- 设置 ----------------

    pub fn apply_theme(&self, pref: ThemePreference) {
        let root = &self.shell.root;
        match pref {
            ThemePreference::System => root.set_theme(None),
            ThemePreference::Light => root.set_theme(light_theme()),
            ThemePreference::Dark => root.set_theme(dark_theme()),
        }
        let mode = match pref {
            ThemePreference::System => 0,
            ThemePreference::Light => 1,
            ThemePreference::Dark => 2,
        };
        put(&self.shell.theme_mode, mode);
    }

    pub(super) fn detect_java(&self) {
        put(&self.shell.java_status, "正在检测…".into());
        let post = self.post.clone();
        thread::spawn(move || {
            let mut list = Vec::new();
            for path in crate::env::java::find_system_java_candidates() {
                if let Ok(info) = crate::env::java::probe_java_binary(&path) {
                    let bits = if info.is_64_bit { "64 位" } else { "32 位" };
                    let entry = (path.to_string_lossy().to_string(), format!("Java {} · {} · {bits}", info.major_version, info.version_string));
                    if !list.iter().any(|(p, _): &(String, String)| *p == entry.0) {
                        list.push(entry);
                    }
                }
            }
            post(Msg::Javas(list));
        });
    }
}

/// Finds why the game exited: a crash report written during this run, else the captured output.
fn diagnose(game_dir: &std::path::Path, instance: &str, log_path: &std::path::Path, since: SystemTime) -> Option<AnalysisReport> {
    if let Some(report) = find_latest_crash_report(game_dir, Some(instance))
        && fs::metadata(&report).and_then(|m| m.modified()).is_ok_and(|t| t >= since)
        && let Ok(text) = fs::read_to_string(&report)
    {
        return Some(analyze_crash_text(&text));
    }
    let text = fs::read_to_string(log_path).ok()?;
    (!text.trim().is_empty()).then(|| analyze_crash_text(&text))
}
