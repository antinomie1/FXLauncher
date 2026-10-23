//! Conversion of backend data into the markup records declared in `parts.aegle`.

use crate::game::loader::detect_instance_loader;
use crate::game::mods::ModEntry;
use crate::game::saves::WorldSaveEntry;
use crate::game::shaders::ShaderPackEntry;
use crate::game::version::VersionError;
use crate::net::download::ManifestVersion;
use crate::net::mods::{ModLoader, ModSearchResult};
use crate::net::task::{TaskSnapshot, TaskStatus};
use aegle::loader::Data;
use std::path::Path;

fn s(text: impl AsRef<str>) -> Data {
    Data::String(text.as_ref().into())
}

/// A list of records, each given as its field values in declaration order.
pub fn records(rows: impl IntoIterator<Item = Vec<Data>>) -> Data {
    Data::List(rows.into_iter().map(|r| Data::Record(r.into())).collect())
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

pub fn format_count(n: u64) -> String {
    match n {
        0..=9_999 => n.to_string(),
        10_000..=99_999_999 => format!("{:.1} 万", n as f64 / 10_000.0),
        _ => format!("{:.1} 亿", n as f64 / 100_000_000.0),
    }
}

/// Seconds since the Unix epoch → `YYYY-MM-DD` (UTC).
pub fn format_date(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y}-{m:02}-{d:02}")
}

/// Title and detail line describing an installed instance.
pub fn instance_summary(game_dir: &Path, id: &str) -> (String, String) {
    match detect_instance_loader(game_dir, id) {
        Ok(info) => {
            let mc = info.game_version.unwrap_or_else(|| id.to_string());
            let mut detail = match (info.loader, info.loader_version) {
                (ModLoader::Vanilla, _) => format!("原版 · Minecraft {mc}"),
                (loader, Some(v)) => format!("{} {v} · Minecraft {mc}", loader.display_name()),
                (loader, None) => format!("{} · Minecraft {mc}", loader.display_name()),
            };
            if info.has_optifine {
                detail.push_str(" · OptiFine");
            }
            (id.to_string(), detail)
        }
        Err(VersionError::NotFound(missing)) if missing != id => (id.to_string(), format!("缺少依赖的版本 {missing}，请重新安装")),
        Err(_) => (id.to_string(), "版本文件损坏或缺失".to_string()),
    }
}

/// Short facts shown under the current instance: mods, saves and the Java it needs.
pub fn instance_stats(game_dir: &Path, id: &str) -> String {
    let mut parts = Vec::new();
    let mods = crate::game::mods::list_instance_mods(game_dir, id);
    let enabled = mods.iter().filter(|m| m.enabled).count();
    if !mods.is_empty() {
        parts.push(format!("{enabled} 个模组"));
    }
    let saves = crate::game::saves::list_world_saves(game_dir, id).len();
    if saves > 0 {
        parts.push(format!("{saves} 个存档"));
    }
    if let Ok(info) = crate::game::version::load_version_info(game_dir, id) {
        parts.push(format!("需要 Java {}", crate::game::launch::required_java_for(&info)));
    }
    parts.join(" · ")
}

pub fn instance_records(game_dir: &Path, ids: &[String]) -> Data {
    records(ids.iter().map(|id| {
        let (title, detail) = instance_summary(game_dir, id);
        vec![s(id), s(title), s(detail)]
    }))
}

pub fn version_records(versions: &[&ManifestVersion]) -> Data {
    records(versions.iter().map(|v| {
        let kind = match v.version_type.as_str() {
            "release" => "正式版",
            "snapshot" => "快照",
            "old_beta" => "Beta",
            "old_alpha" => "Alpha",
            other => other,
        };
        let date = v.release_time.get(..10).unwrap_or("");
        vec![s(&v.id), s(format!("{kind} · {date}"))]
    }))
}

pub fn task_state(status: TaskStatus) -> i64 {
    match status {
        TaskStatus::Queued | TaskStatus::Running => 0,
        TaskStatus::Paused => 1,
        TaskStatus::Completed => 2,
        TaskStatus::Failed => 3,
        TaskStatus::Cancelled => 4,
    }
}

pub fn task_detail(t: &TaskSnapshot) -> String {
    match t.status {
        TaskStatus::Completed => return "已完成".to_string(),
        TaskStatus::Cancelled => return "已取消".to_string(),
        TaskStatus::Failed => return format!("失败 · {}", t.stage),
        TaskStatus::Queued => return "等待中".to_string(),
        _ => {}
    }
    let mut parts = vec![if t.stage.is_empty() { "准备中".to_string() } else { t.stage.clone() }];
    if t.total_items > 1 {
        parts.push(format!("{}/{} 个文件", t.done_items, t.total_items));
    }
    if t.total_bytes > 0 {
        parts.push(format!("{} / {}", format_bytes(t.transferred_bytes), format_bytes(t.total_bytes)));
    }
    if t.status == TaskStatus::Paused {
        parts.push("已暂停".to_string());
    } else if t.speed_bps > 0 {
        parts.push(format!("{}/s", format_bytes(t.speed_bps)));
    }
    parts.join(" · ")
}

pub fn task_records(tasks: &[TaskSnapshot]) -> Data {
    records(tasks.iter().rev().map(|t| {
        vec![
            Data::Int(t.id as i64),
            s(&t.title),
            s(task_detail(t)),
            Data::Float(t.progress_percent.clamp(0.0, 100.0)),
            Data::Int(task_state(t.status)),
            s(t.error_msg.as_deref().filter(|_| t.status == TaskStatus::Failed).unwrap_or("")),
        ]
    }))
}

pub fn mod_records(mods: &[ModEntry]) -> Data {
    records(mods.iter().map(|m| {
        let mut detail = Vec::new();
        if let Some(v) = &m.deduced_version {
            detail.push(v.clone());
        }
        detail.push(format_bytes(m.size_bytes));
        vec![s(&m.file_name), s(&m.deduced_id), s(detail.join(" · ")), Data::Bool(m.enabled)]
    }))
}

pub fn shader_records(packs: &[ShaderPackEntry]) -> Data {
    records(packs.iter().map(|p| {
        let kind = if p.is_directory { "文件夹".to_string() } else { format_bytes(p.size_bytes) };
        vec![s(&p.file_name), s(kind), Data::Bool(p.is_active)]
    }))
}

pub fn save_records(saves: &[WorldSaveEntry]) -> Data {
    records(saves.iter().map(|w| {
        let mut detail = vec![format_bytes(w.size_bytes)];
        if let Some(t) = w.modified_time {
            detail.push(format!("修改于 {}", format_date(t)));
        }
        if w.display_name != w.folder_name {
            detail.push(w.folder_name.clone());
        }
        vec![s(&w.folder_name), s(&w.display_name), s(detail.join(" · "))]
    }))
}

pub fn hit_records(hits: &[ModSearchResult]) -> Data {
    records(hits.iter().map(|h| {
        let detail = format!("{} 次下载 · Modrinth · {}", format_count(h.downloads), h.slug);
        vec![s(&h.id), s(&h.title), s(detail), s(&h.description)]
    }))
}

pub fn java_records(javas: &[(String, String)]) -> Data {
    records(javas.iter().map(|(path, detail)| vec![s(path), s(detail)]))
}
