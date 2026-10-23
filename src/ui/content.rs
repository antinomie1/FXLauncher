//! Instance management (mods, shaders, saves, diagnosis) and the download pages.

use super::controller::{Controller, Deletion, Msg, TaskKind, put};
use super::model;
use crate::diag::{AnalysisReport, CrashCause, analyze_latest_crash};
use crate::env::paths::{version_dir, version_mods_dir};
use crate::game::loader::detect_instance_loader;
use crate::game::{mods, saves, shaders};
use crate::net::batch::{DownloadItem, download_all};
use crate::net::download::{DownloadSource, fetch_version_manifest};
use crate::net::install::{InstanceRequest, install_instance_blocking};
use crate::net::mods::{ModLoader, resolve_modrinth_file, search_modrinth};
use crate::net::{fabric, forge};
use std::thread;

const LOADERS: [ModLoader; 5] = [ModLoader::Vanilla, ModLoader::Fabric, ModLoader::Quilt, ModLoader::Forge, ModLoader::NeoForge];

/// Multi-line text for the 诊断 tab.
pub(super) fn format_report(r: &AnalysisReport) -> String {
    let cause = match &r.cause {
        CrashCause::OutOfMemory { .. } => "内存不足".to_string(),
        CrashCause::JavaVersionMismatch { current_major, required_major } => format!(
            "Java 版本不匹配（当前 {}，需要 {}）",
            current_major.map_or("未知".into(), |v| v.to_string()),
            required_major.map_or("未知".into(), |v| v.to_string())
        ),
        CrashCause::MissingModDependency { mod_name, required_dependency, .. } => format!("{mod_name} 缺少前置 {required_dependency}"),
        CrashCause::ModConflict { mod_a, mod_b, .. } => format!("{mod_a} 与 {mod_b} 冲突"),
        CrashCause::IncompatibleMod { mod_name, .. } => format!("{mod_name} 不兼容"),
        CrashCause::GraphicsDriverError { .. } => "显卡驱动异常".to_string(),
        CrashCause::Unknown { .. } => "未能确定".to_string(),
    };
    let mut text = format!("原因：{cause}\n\n{}\n\n建议：{}", r.summary, r.suggestion);
    if !r.suspected_mods.is_empty() {
        text.push_str(&format!("\n\n可疑模组：{}", r.suspected_mods.join("、")));
    }
    if let Some(trace) = &r.stacktrace {
        text.push_str("\n\n");
        text.push_str(&trace.lines().take(24).collect::<Vec<_>>().join("\n"));
    }
    text
}

impl Controller {
    /// Shows a top-level page: 0 启动, 1 实例, 2 下载, 3 任务, 4 设置.
    pub fn goto(&self, page: usize) {
        self.shell.tabs.select(page);
    }

    // ---------------- 实例 ----------------

    pub(super) fn pick_instance(&self, id: &str) {
        let sh = &self.shell;
        put(&sh.picked, id.to_string());
        let (title, detail) = model::instance_summary(&self.config().game_dir, id);
        put(&sh.picked_title, title);
        put(&sh.picked_detail, detail);
        self.load_instance_content(id);
    }

    pub(super) fn load_instance_content(&self, id: &str) {
        let sh = &self.shell;
        let game_dir = self.config().game_dir;
        let info = detect_instance_loader(&game_dir, id).ok();
        let loader = info.as_ref().map_or(ModLoader::Vanilla, |i| i.loader);
        put(&sh.picked_vanilla, loader == ModLoader::Vanilla && !info.as_ref().is_some_and(|i| i.has_optifine));

        let list = mods::list_instance_mods(&game_dir, id);
        let report = mods::analyze_instance_mods(&game_dir, id, info.as_ref().and_then(|i| i.game_version.as_deref()), Some(loader));
        put(&sh.mods, model::mod_records(&list));
        put(&sh.mods_summary, if list.is_empty() {
            "还没有模组。可以在「下载 › 模组」中搜索安装。".into()
        } else {
            format!("{} 个模组 · {} 个已启用 · {} 个已禁用", report.total_mods, report.enabled_mods, report.disabled_mods)
        });
        let mut warnings: Vec<String> = report
            .duplicate_mods
            .iter()
            .map(|d| format!("重复模组 {}：{}", d.mod_id, d.files.join("、")))
            .collect();
        warnings.extend(report.compatibility_warnings);
        put(&sh.mods_warning, warnings.join("\n"));

        let packs = shaders::list_shaderpacks(&game_dir, id);
        put(&sh.shader_off, !packs.iter().any(|p| p.is_active));
        put(&sh.shaders, model::shader_records(&packs));
        put(&sh.saves, model::save_records(&saves::list_world_saves(&game_dir, id)));
    }

    pub(super) fn use_picked(&self) {
        let id = self.shell.picked.get();
        self.set_current(&id);
    }

    pub(super) fn refresh_instance(&self) {
        let picked = self.shell.picked.get();
        self.reload_instances(None);
        if !picked.is_empty() {
            self.pick_instance(&picked);
        }
    }

    pub(super) fn toggle_mod(&self, file: &str) {
        let id = self.shell.picked.get();
        if let Err(e) = mods::toggle_mod(&self.config().game_dir, &id, file) {
            put(&self.shell.mods_warning, format!("无法切换 {file}：{e}"));
        }
        self.load_instance_content(&id);
    }

    pub(super) fn request(&self, pending: Deletion) {
        let text = match &pending {
            Deletion::Mod(f) => format!("删除模组 {f}？文件会被永久删除。"),
            Deletion::Shader(f) => format!("删除光影包 {f}？文件会被永久删除。"),
            Deletion::Save(f) => format!("删除存档 {f}？世界会被永久删除，建议先备份。"),
        };
        self.model.borrow_mut().pending = Some(pending);
        put(&self.shell.confirm_text, text);
        put(&self.shell.confirm_open, true);
    }

    pub(super) fn confirm(&self) {
        let Some(pending) = self.model.borrow_mut().pending.take() else { return };
        let id = self.shell.picked.get();
        let game_dir = self.config().game_dir;
        let result = match &pending {
            Deletion::Mod(f) => mods::delete_mod(&game_dir, &id, f),
            Deletion::Shader(f) => shaders::delete_shaderpack(&game_dir, &id, f),
            Deletion::Save(f) => saves::delete_world_save(&game_dir, &id, f),
        };
        if let Err(e) = result {
            put(&self.shell.mods_warning, format!("删除失败：{e}"));
        }
        self.load_instance_content(&id);
    }

    pub(super) fn set_shader(&self, name: &str) {
        let id = self.shell.picked.get();
        let pack = (!name.is_empty()).then_some(name);
        if let Err(e) = shaders::set_active_shaderpack(&self.config().game_dir, &id, pack) {
            put(&self.shell.mods_warning, format!("无法切换光影：{e}"));
        }
        self.load_instance_content(&id);
    }

    pub(super) fn backup_save(&self, folder: &str) {
        let id = self.shell.picked.get();
        let (game_dir, folder) = (self.config().game_dir, folder.to_string());
        let (inst, f) = (id.clone(), folder.clone());
        let task = self.manager.run_task(&format!("备份存档 {folder}"), move |task| {
            task.set_stage("复制世界文件");
            saves::backup_world_save(&game_dir, &inst, &f).map(|_| ()).map_err(Into::into)
        });
        self.model.borrow_mut().task_kinds.insert(task.id, TaskKind::Backup(id));
        self.ensure_ticker();
    }

    pub(super) fn analyze(&self) {
        let id = self.shell.picked.get();
        let text = match analyze_latest_crash(&self.config().game_dir, Some(&id)) {
            Ok(Some(report)) => format_report(&report),
            Ok(None) => "没有找到崩溃报告或日志。".to_string(),
            Err(e) => format!("读取日志失败：{e}"),
        };
        put(&self.shell.diagnosis, text);
    }

    pub(super) fn show_diagnosis(&self) {
        let current = self.shell.current.get();
        if !current.is_empty() && self.shell.picked.get() != current {
            self.pick_instance(&current);
        }
        self.goto(1);
        self.shell.instance_tabs.select(3);
    }

    // ---------------- 下载 › 游戏 ----------------

    pub(super) fn fetch_manifest(&self) {
        put(&self.shell.version_status, "正在获取版本列表…".into());
        let (post, source) = (self.post.clone(), self.config().download_source);
        thread::spawn(move || {
            let result = fetch_version_manifest(source).map(|m| m.versions).map_err(|e| e.to_string());
            post(Msg::Manifest(result));
        });
    }

    pub(super) fn on_manifest(&self, result: Result<Vec<crate::net::download::ManifestVersion>, String>) {
        match result {
            Ok(list) => {
                self.model.borrow_mut().manifest = list;
                self.apply_version_filter();
            }
            Err(e) => put(&self.shell.version_status, format!("获取版本列表失败：{e}")),
        }
    }

    pub(super) fn apply_version_filter(&self) {
        let m = self.model.borrow();
        let shown: Vec<_> = m
            .manifest
            .iter()
            .filter(|v| m.snapshots || v.version_type == "release")
            .filter(|v| m.filter.is_empty() || v.id.contains(&m.filter))
            .collect();
        put(&self.shell.versions, model::version_records(&shown));
        put(&self.shell.version_status, if shown.is_empty() { "没有匹配的版本".into() } else { String::new() });
        let first = shown.first().map(|v| v.id.clone());
        drop(m);
        if self.shell.chosen.get().is_empty()
            && let Some(first) = first
        {
            self.pick_version(&first);
        }
    }

    pub(super) fn set_snapshots(&self, on: bool) {
        self.model.borrow_mut().snapshots = on;
        self.apply_version_filter();
    }

    pub(super) fn filter_versions(&self, text: &str) {
        self.model.borrow_mut().filter = text.trim().to_string();
        self.apply_version_filter();
    }

    pub(super) fn pick_version(&self, id: &str) {
        put(&self.shell.chosen, id.to_string());
        put(&self.shell.install_title, format!("安装 Minecraft {id}"));
        self.after_install_choice();
    }

    pub(super) fn set_loader(&self, index: i64) {
        put(&self.shell.loader, index);
        self.after_install_choice();
    }

    fn selected_loader(&self) -> ModLoader {
        LOADERS[self.shell.loader.get().clamp(0, 4) as usize]
    }

    /// Suggests an instance name and refreshes the loader version list.
    fn after_install_choice(&self) {
        let (mc, loader) = (self.shell.chosen.get(), self.selected_loader());
        let name = match loader {
            ModLoader::Vanilla => mc.clone(),
            l => format!("{mc}-{}", l.display_name()),
        };
        self.shell.instance_name.set_text(&name);
        put(&self.shell.install_status, String::new());
        if loader == ModLoader::Vanilla || mc.is_empty() {
            return;
        }
        let key = (self.shell.loader.get(), mc.clone());
        if self.model.borrow().loader_key == key {
            return;
        }
        self.model.borrow_mut().loader_key = key.clone();
        self.model.borrow_mut().loader_versions.clear();
        self.show_loader_versions(vec!["正在获取…".into()], 0);
        put(&self.shell.loader_status, format!("正在获取 {} 版本…", loader.display_name()));
        let (post, source) = (self.post.clone(), self.config().download_source);
        thread::spawn(move || {
            let result = match loader {
                ModLoader::Fabric | ModLoader::Quilt => fabric::list_loader_versions(loader, &mc, source),
                _ => forge::list_versions(loader, &mc, source),
            };
            post(Msg::LoaderVersions { key, result: result.map_err(|e| e.to_string()) });
        });
    }

    fn show_loader_versions(&self, items: Vec<String>, selected: usize) {
        // Items first: the dropdown rejects an index outside the current list.
        put(&self.shell.loader_items, items);
        put(&self.shell.loader_index, selected as i64);
    }

    pub(super) fn on_loader_versions(&self, key: (i64, String), result: Result<Vec<fabric::LoaderVersion>, String>) {
        if self.model.borrow().loader_key != key {
            return;
        }
        let name = LOADERS[key.0.clamp(0, 4) as usize].display_name();
        match result {
            Ok(list) if !list.is_empty() => {
                let labels: Vec<String> = list
                    .iter()
                    .map(|v| if v.stable { format!("{}（推荐）", v.version) } else { v.version.clone() })
                    .collect();
                let best = list.iter().position(|v| v.stable).unwrap_or(0);
                self.show_loader_versions(labels, best);
                put(&self.shell.loader_status, format!("共 {} 个 {name} 版本", list.len()));
                self.model.borrow_mut().loader_versions = list;
            }
            Ok(_) => {
                self.show_loader_versions(vec!["不可用".into()], 0);
                put(&self.shell.loader_status, format!("{name} 尚不支持 Minecraft {}", key.1));
            }
            Err(e) => {
                self.model.borrow_mut().loader_key = (0, String::new());
                self.show_loader_versions(vec!["获取失败".into()], 0);
                put(&self.shell.loader_status, format!("获取 {name} 版本失败：{e}"));
            }
        }
    }

    pub(super) fn install(&self) {
        let sh = &self.shell;
        let mc = sh.chosen.get();
        let loader = self.selected_loader();
        let loader_version = if loader == ModLoader::Vanilla {
            None
        } else {
            let index = sh.loader_index.get().max(0) as usize;
            let version = self.model.borrow().loader_versions.get(index).map(|v| v.version.clone());
            match version {
                Some(v) => Some(v),
                None => {
                    put(&sh.install_status, "请等待加载器版本列表，或换一个游戏版本".into());
                    return;
                }
            }
        };
        let name = sh.instance_name.text().trim().to_string();
        if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || name.starts_with('.') {
            put(&sh.install_status, "实例名称不能为空，也不能包含 / \\ : * ? \" < > |".into());
            return;
        }
        let cfg = self.config();
        let exists = version_dir(&cfg.game_dir, &name).join(format!("{name}.json")).is_file();
        if exists && name != mc {
            put(&sh.install_status, format!("已存在名为 {name} 的实例"));
            return;
        }

        let req = InstanceRequest { game_version: mc, loader, loader_version, name: Some(name.clone()), java: cfg.java_path.clone() };
        let (game_dir, source) = (cfg.game_dir.clone(), cfg.download_source);
        let task = self.manager.run_task(&format!("安装实例 {name}"), move |task| {
            let mut req = req;
            if matches!(req.loader, ModLoader::Forge | ModLoader::NeoForge) {
                task.set_stage("查找 Java");
                let major = crate::env::java::required_java_major(&req.game_version);
                req.java = crate::env::java::select_java(&req.java, major).map_err(crate::net::task::TaskError::Other)?.path.to_string_lossy().to_string();
            }
            install_instance_blocking(&game_dir, &req, source, task).map(|_| ())
        });
        self.model.borrow_mut().task_kinds.insert(task.id, TaskKind::Install(name.clone()));
        put(&sh.install_status, format!("已开始安装 {name}，进度见「任务」"));
        self.ensure_ticker();
    }

    // ---------------- 下载 › 模组 ----------------

    /// Minecraft version and loader of the current instance, when it can load mods.
    fn mod_context(&self) -> Option<(String, String, ModLoader)> {
        let id = self.shell.current.get();
        let info = detect_instance_loader(&self.config().game_dir, &id).ok()?;
        (info.loader != ModLoader::Vanilla).then(|| (id, info.game_version.unwrap_or_default(), info.loader))
    }

    pub(super) fn goto_mod_search(&self) {
        let picked = self.shell.picked.get();
        if !picked.is_empty() {
            self.set_current(&picked);
        }
        self.goto(2);
        self.shell.download_tabs.select(1);
        self.shell.mod_query.focus();
    }

    pub(super) fn search_mods(&self, query: &str) {
        let Some((id, mc, loader)) = self.mod_context() else {
            put(&self.shell.mod_target, String::new());
            put(&self.shell.search_status, "当前实例是原版。请先在「启动」页选择一个装有模组加载器的实例。".into());
            return;
        };
        put(&self.shell.mod_target, id.clone());
        put(&self.shell.search_status, format!("正在搜索 {} {mc} 可用的模组…", loader.display_name()));
        let (post, query) = (self.post.clone(), query.trim().to_string());
        thread::spawn(move || post(Msg::Hits(search_modrinth(&query, &mc, loader).map_err(|e| e.to_string()))));
    }

    pub(super) fn on_hits(&self, result: Result<Vec<crate::net::mods::ModSearchResult>, String>) {
        match result {
            Ok(hits) => {
                let target = self.shell.mod_target.get();
                put(&self.shell.search_status, if hits.is_empty() { "没有找到结果".into() } else { format!("{} 个结果，将安装到 {target}", hits.len()) });
                put(&self.shell.hits, model::hit_records(&hits));
                self.model.borrow_mut().hits = hits;
            }
            Err(e) => put(&self.shell.search_status, format!("搜索失败：{e}")),
        }
    }

    pub(super) fn install_mod(&self, project: &str) {
        let Some((id, mc, loader)) = self.mod_context() else { return };
        let title = self.model.borrow().hits.iter().find(|h| h.id == project).map_or(project.to_string(), |h| h.title.clone());
        let (game_dir, project, inst) = (self.config().game_dir, project.to_string(), id.clone());
        let task = self.manager.run_task(&format!("安装模组 {title}"), move |task| {
            task.set_stage("查找兼容版本");
            let file = resolve_modrinth_file(&project, &mc, loader).map_err(crate::net::task::TaskError::other)?;
            task.set_stage(&format!("下载 {}", file.file_name));
            let dest = version_mods_dir(&game_dir, &inst).join(&file.file_name);
            download_all(vec![DownloadItem::new(vec![file.download_url], dest, file.sha1, file.size)], task, 1)
        });
        self.model.borrow_mut().task_kinds.insert(task.id, TaskKind::Mod(id));
        put(&self.shell.search_status, format!("正在安装 {title}，进度见「任务」"));
        self.ensure_ticker();
    }

    pub(super) fn set_source(&self, index: i64) {
        put(&self.shell.source, index);
        self.update_config(|c| c.download_source = if index == 0 { DownloadSource::Bmclapi } else { DownloadSource::Official });
        if self.model.borrow().manifest.is_empty() {
            self.fetch_manifest();
        }
    }
}
