//! Installation pipelines: client jar, libraries, natives and assets for vanilla versions,
//! plus complete instance installs with an optional mod loader.

use crate::env::paths::{assets_dir, ensure_dir, libraries_dir, version_dir, version_jar_path, version_json_path};
use crate::game::natives::extract_natives;
use crate::game::version::{VersionInfo, load_version_info};
use crate::net::batch::{DEFAULT_WORKERS, DownloadItem, download_all};
use crate::net::download::{DownloadSource, candidate_urls, ensure_version_json, setup_isolated_version_dir};
use crate::net::mods::ModLoader;
use crate::net::task::{TaskError, TaskHandle, TaskManager, compute_file_sha1};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

/// Parsed `assets/indexes/{id}.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
    #[serde(rename = "virtual", default)]
    pub is_virtual: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// `assets/objects/{hh}/{hash}`.
pub fn asset_object_path(game_dir: &Path, hash: &str) -> PathBuf {
    assets_dir(game_dir).join("objects").join(&hash[..2]).join(hash)
}

/// Directory used as `${game_assets}` by pre-1.7 versions with a virtual asset index.
pub fn legacy_assets_dir(game_dir: &Path) -> PathBuf {
    assets_dir(game_dir).join("virtual").join("legacy")
}

/// Reads the version's asset index from disk, downloading it first when missing or outdated.
pub fn load_or_fetch_asset_index(game_dir: &Path, info: &VersionInfo, source: DownloadSource, task: &TaskHandle) -> Result<Option<AssetIndex>, TaskError> {
    let Some(index_ref) = info.asset_index.as_ref() else {
        return Ok(None);
    };
    let path = assets_dir(game_dir).join("indexes").join(format!("{}.json", index_ref.id));
    let item = DownloadItem::new(
        candidate_urls(&index_ref.url, source),
        path.clone(),
        index_ref.sha1.clone(),
        index_ref.size.unwrap_or(0),
    );
    if !item.is_satisfied() {
        download_all(vec![item], task, 1)?;
    }
    let index = serde_json::from_slice(&fs::read(&path)?).map_err(TaskError::other)?;
    Ok(Some(index))
}

/// Download list for every asset object of an index.
pub fn asset_items(game_dir: &Path, index: &AssetIndex, source: DownloadSource) -> Vec<DownloadItem> {
    let mut seen = std::collections::HashSet::new();
    index
        .objects
        .values()
        .filter(|o| o.hash.len() > 2 && seen.insert(o.hash.clone()))
        .map(|o| {
            let url = format!("{RESOURCES_URL}/{}/{}", &o.hash[..2], o.hash);
            DownloadItem::new(
                candidate_urls(&url, source),
                asset_object_path(game_dir, &o.hash),
                Some(o.hash.clone()),
                o.size,
            )
        })
        .collect()
}

/// Download list for the libraries (and legacy native classifiers) that apply to this platform.
/// Libraries without a URL are expected to be provided by an installer and are skipped.
pub fn library_items(game_dir: &Path, info: &VersionInfo, source: DownloadSource) -> Vec<DownloadItem> {
    let lib_dir = libraries_dir(game_dir);
    let mut items = Vec::new();
    for lib in info.libraries.iter().filter(|l| l.applies()) {
        if let Some((rel, url, sha1, size)) = lib.artifact_download()
            && !url.is_empty()
        {
            items.push(DownloadItem::new(candidate_urls(&url, source), lib_dir.join(rel), sha1, size));
        }
        if let Some(native) = lib.native_artifact()
            && let (Some(path), Some(url)) = (&native.path, &native.url)
        {
            items.push(DownloadItem::new(
                candidate_urls(url, source),
                lib_dir.join(path),
                native.sha1.clone(),
                native.size.unwrap_or(0),
            ));
        }
    }
    items
}

/// Downloads the client jar of the version's root (`jar` / inherited) version.
pub fn download_client(game_dir: &Path, info: &VersionInfo, source: DownloadSource, task: &TaskHandle) -> Result<(), TaskError> {
    task.set_stage("下载游戏核心");
    let client = info
        .downloads
        .as_ref()
        .and_then(|d| d.client.as_ref())
        .and_then(|c| c.url.as_ref().map(|u| (u, c)))
        .ok_or_else(|| TaskError::other("版本元数据缺少 client 下载项"))?;
    let item = DownloadItem::new(
        candidate_urls(client.0, source),
        version_jar_path(game_dir, info.client_jar_id()),
        client.1.sha1.clone(),
        client.1.size.unwrap_or(0),
    );
    download_all(vec![item], task, 1)
}

pub fn download_libraries(game_dir: &Path, info: &VersionInfo, source: DownloadSource, task: &TaskHandle) -> Result<(), TaskError> {
    task.set_stage("下载依赖库");
    download_all(library_items(game_dir, info, source), task, DEFAULT_WORKERS)?;
    task.set_stage("解压本地库");
    extract_natives(game_dir, info)?;
    Ok(())
}

pub fn download_assets(game_dir: &Path, info: &VersionInfo, source: DownloadSource, task: &TaskHandle) -> Result<(), TaskError> {
    task.set_stage("下载资源索引");
    let Some(index) = load_or_fetch_asset_index(game_dir, info, source, task)? else {
        return Ok(());
    };
    task.set_stage("下载资源文件");
    download_all(asset_items(game_dir, &index, source), task, DEFAULT_WORKERS)?;
    if index.is_virtual || index.map_to_resources {
        task.set_stage("整理旧版资源");
        let target = if index.map_to_resources {
            version_dir(game_dir, &info.id).join("resources")
        } else {
            legacy_assets_dir(game_dir)
        };
        for (name, obj) in &index.objects {
            let dest = target.join(name);
            if !dest.is_file() {
                if let Some(parent) = dest.parent() {
                    ensure_dir(parent)?;
                }
                fs::copy(asset_object_path(game_dir, &obj.hash), dest)?;
            }
        }
    }
    Ok(())
}

/// Installs every file a vanilla version needs to launch.
pub fn install_vanilla(game_dir: &Path, version_id: &str, source: DownloadSource, task: &TaskHandle) -> Result<(), TaskError> {
    task.set_stage("获取版本信息");
    ensure_version_json(game_dir, version_id, source).map_err(TaskError::other)?;
    setup_isolated_version_dir(game_dir, version_id)?;
    let info = load_version_info(game_dir, version_id).map_err(TaskError::other)?;
    download_client(game_dir, &info, source, task)?;
    download_libraries(game_dir, &info, source, task)?;
    download_assets(game_dir, &info, source, task)?;
    task.set_stage("完成");
    Ok(())
}

/// What to install as a new instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceRequest {
    /// Minecraft version (e.g. "1.21.1").
    pub game_version: String,
    pub loader: ModLoader,
    /// Loader version; `None` picks the latest stable one.
    pub loader_version: Option<String>,
    /// Instance (version directory) name; `None` uses the loader's default id.
    pub name: Option<String>,
    /// Java binary used to run Forge/NeoForge installer processors.
    pub java: String,
}

/// Writes a child profile `{id: name, inheritsFrom: parent}` so a vanilla version can get its own instance name.
fn write_alias_profile(game_dir: &Path, name: &str, parent: &str) -> Result<(), TaskError> {
    let json = serde_json::json!({ "id": name, "inheritsFrom": parent, "type": "release" });
    ensure_dir(&version_dir(game_dir, name))?;
    fs::write(version_json_path(game_dir, name), serde_json::to_string_pretty(&json).map_err(TaskError::other)?)?;
    Ok(())
}

/// Copies the root client jar to `versions/{id}/{id}.jar` so the instance is self-contained.
fn copy_client_jar(game_dir: &Path, info: &VersionInfo) -> Result<(), TaskError> {
    let own = version_jar_path(game_dir, &info.id);
    let root = version_jar_path(game_dir, info.client_jar_id());
    if own != root {
        let up_to_date = own.is_file() && compute_file_sha1(&own).ok() == compute_file_sha1(&root).ok();
        if !up_to_date {
            fs::copy(&root, &own)?;
        }
    }
    Ok(())
}

/// Installs vanilla files, then the requested loader, as one task step. Returns the instance id.
pub fn install_instance_blocking(game_dir: &Path, req: &InstanceRequest, source: DownloadSource, task: &TaskHandle) -> Result<String, TaskError> {
    install_vanilla(game_dir, &req.game_version, source, task)?;
    let mc = req.game_version.as_str();
    let id = match req.loader {
        ModLoader::Vanilla => match req.name.as_deref().filter(|n| *n != mc) {
            Some(name) => {
                write_alias_profile(game_dir, name, mc)?;
                name.to_string()
            }
            None => return Ok(mc.to_string()),
        },
        ModLoader::Fabric | ModLoader::Quilt => {
            task.set_stage("安装加载器");
            crate::net::fabric::install_profile(game_dir, req.loader, mc, req.loader_version.as_deref(), req.name.as_deref(), source)
                .map_err(TaskError::other)?
        }
        ModLoader::Forge | ModLoader::NeoForge => crate::net::forge::install(game_dir, req, source, task)?,
    };

    let info = load_version_info(game_dir, &id).map_err(TaskError::other)?;
    setup_isolated_version_dir(game_dir, &id)?;
    download_libraries(game_dir, &info, source, task)?;
    copy_client_jar(game_dir, &info)?;
    task.set_stage("完成");
    Ok(id)
}

/// Schedules a background task that installs a complete instance.
pub fn install_instance(game_dir: &Path, req: InstanceRequest, source: DownloadSource, manager: &TaskManager) -> Arc<TaskHandle> {
    let game_dir = game_dir.to_path_buf();
    let title = match (&req.loader, &req.name) {
        (_, Some(name)) => format!("安装实例 {name}"),
        (ModLoader::Vanilla, None) => format!("安装 Minecraft {}", req.game_version),
        (loader, None) => format!("安装 {} {}", loader.display_name(), req.game_version),
    };
    manager.run_task(&title, move |task| install_instance_blocking(&game_dir, &req, source, task).map(|_| ()))
}
