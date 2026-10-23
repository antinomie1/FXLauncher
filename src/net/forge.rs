//! Forge and NeoForge installation by reading the official installer jar: extracting its Maven
//! payload, downloading libraries and running the client-side install processors with Java.

use crate::env::paths::{classpath_separator, ensure_dir, libraries_dir, version_dir, version_jar_path, version_json_path};
use crate::game::version::{Library, VersionInfo, maven_to_path};
use crate::net::batch::{DEFAULT_WORKERS, DownloadItem, download_all};
use crate::net::download::{DownloadSource, DownloaderError, candidate_urls, fetch_json};
use crate::net::fabric::LoaderVersion;
use crate::net::install::{InstanceRequest, library_items};
use crate::net::mods::ModLoader;
use crate::net::task::{TaskError, TaskHandle, compute_file_sha1};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";
const NEOFORGE_VERSIONS: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";
const BMCLAPI_FORGE: &str = "https://bmclapi2.bangbang93.com/forge";
const FORGE_PROMOTIONS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";

#[derive(Debug, Deserialize)]
struct NeoVersions {
    versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Promotions {
    promos: HashMap<String, String>,
}

/// Entry of BMCLAPI's `/forge/minecraft/{mc}` list.
#[derive(Debug, Clone, Deserialize)]
struct BmclForge {
    version: String,
    mcversion: String,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    build: u64,
}

impl BmclForge {
    fn full_version(&self) -> String {
        match &self.branch {
            Some(b) if !b.is_empty() => format!("{}-{}-{b}", self.mcversion, self.version),
            _ => format!("{}-{}", self.mcversion, self.version),
        }
    }
}

#[derive(Debug, Deserialize)]
struct BmclPromo {
    name: String,
    #[serde(default)]
    build: Option<BmclForge>,
}

/// A loader version source, tried in download-source order.
type Lister = fn(&str) -> Result<Vec<LoaderVersion>, DownloaderError>;

fn with_fallback(source: DownloadSource, mirror: Lister, official: Lister, game_version: &str) -> Result<Vec<LoaderVersion>, DownloaderError> {
    let (first, second) = match source {
        DownloadSource::Bmclapi => (mirror, official),
        DownloadSource::Official => (official, mirror),
    };
    match first(game_version) {
        Ok(list) if !list.is_empty() => Ok(list),
        _ => second(game_version),
    }
}

/// Sort key comparing dotted versions numerically (`21.1.10` > `21.1.9`).
fn version_key(v: &str) -> Vec<u64> {
    v.split(['.', '-']).map(|p| p.parse().unwrap_or(0)).collect()
}

fn neo_entry(version: String) -> LoaderVersion {
    LoaderVersion { stable: !version.contains("beta") && !version.contains("alpha"), version }
}

fn neoforge_versions_official(game_version: &str) -> Result<Vec<LoaderVersion>, DownloaderError> {
    let prefix = neoforge_prefix(game_version).ok_or_else(|| DownloaderError::VersionNotFound(game_version.to_string()))?;
    let list: NeoVersions = fetch_json(NEOFORGE_VERSIONS, DownloadSource::Official)?;
    let mut versions: Vec<String> = list.versions.into_iter().filter(|v| v.starts_with(&prefix)).collect();
    versions.sort_by_key(|v| std::cmp::Reverse(version_key(v)));
    Ok(versions.into_iter().map(neo_entry).collect())
}

#[derive(Debug, Deserialize)]
struct BmclNeo {
    version: String,
}

fn neoforge_versions_bmclapi(game_version: &str) -> Result<Vec<LoaderVersion>, DownloaderError> {
    let list: Vec<BmclNeo> = crate::net::client::http_get_json(&format!("{BMCLAPI}/neoforge/list/{game_version}"))?;
    let mut versions: Vec<String> = list.into_iter().map(|n| n.version).collect();
    versions.sort_by_key(|v| std::cmp::Reverse(version_key(v)));
    Ok(versions.into_iter().map(neo_entry).collect())
}

fn forge_versions_bmclapi(game_version: &str) -> Result<Vec<LoaderVersion>, DownloaderError> {
    let mut builds: Vec<BmclForge> = crate::net::client::http_get_json(&format!("{BMCLAPI_FORGE}/minecraft/{game_version}"))?;
    builds.sort_by_key(|b| std::cmp::Reverse(b.build));
    let promos: Vec<BmclPromo> = crate::net::client::http_get_json(&format!("{BMCLAPI_FORGE}/promos")).unwrap_or_default();
    let recommended = promos
        .into_iter()
        .find(|p| p.name == format!("{game_version}-recommended"))
        .and_then(|p| p.build)
        .map(|b| b.version);
    Ok(builds
        .iter()
        .map(|b| LoaderVersion { version: b.full_version(), stable: recommended.as_deref() == Some(b.version.as_str()) })
        .collect())
}

fn forge_versions_official(game_version: &str) -> Result<Vec<LoaderVersion>, DownloaderError> {
    let meta: HashMap<String, Vec<String>> = fetch_json(&format!("{FORGE_MAVEN}/maven-metadata.json"), DownloadSource::Official)?;
    let recommended = fetch_json::<Promotions>(FORGE_PROMOTIONS, DownloadSource::Official)
        .ok()
        .and_then(|p| p.promos.get(&format!("{game_version}-recommended")).cloned());
    let builds = meta.get(game_version).cloned().unwrap_or_default();
    Ok(builds
        .into_iter()
        .rev()
        .map(|v| {
            let stable = recommended.as_deref().is_some_and(|r| v.contains(&format!("-{r}")));
            LoaderVersion { version: v, stable }
        })
        .collect())
}

/// NeoForge version prefix for a Minecraft version: `1.21.1` → `21.1.`, `1.21` → `21.0.`, `26.1` → `26.1.0.`.
pub fn neoforge_prefix(game_version: &str) -> Option<String> {
    let parts: Vec<&str> = game_version.split('.').collect();
    match parts.as_slice() {
        ["1", minor] => Some(format!("{minor}.0.")),
        ["1", minor, patch] => Some(format!("{minor}.{patch}.")),
        [year, drop] if year.parse::<u32>().is_ok_and(|y| y >= 25) => Some(format!("{year}.{drop}.0.")),
        [year, drop, hotfix] if year.parse::<u32>().is_ok_and(|y| y >= 25) => Some(format!("{year}.{drop}.{hotfix}.")),
        _ => None,
    }
}

/// Lists Forge or NeoForge builds for a Minecraft version, newest first.
/// Forge entries are full maven versions (`1.20.1-47.3.0`); `stable` marks the recommended build.
pub fn list_versions(loader: ModLoader, game_version: &str, source: DownloadSource) -> Result<Vec<LoaderVersion>, DownloaderError> {
    match loader {
        ModLoader::NeoForge => with_fallback(source, neoforge_versions_bmclapi, neoforge_versions_official, game_version),
        _ => with_fallback(source, forge_versions_bmclapi, forge_versions_official, game_version),
    }
}

fn installer_coord(loader: ModLoader, version: &str) -> (String, String) {
    match loader {
        ModLoader::NeoForge => (
            format!("net.neoforged:neoforge:{version}:installer"),
            format!("{NEOFORGE_MAVEN}/{version}/neoforge-{version}-installer.jar"),
        ),
        _ => (
            format!("net.minecraftforge:forge:{version}:installer"),
            format!("{FORGE_MAVEN}/{version}/forge-{version}-installer.jar"),
        ),
    }
}

#[derive(Debug, Deserialize)]
struct LegacyProfile {
    install: LegacyInstall,
    #[serde(rename = "versionInfo")]
    version_info: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct LegacyInstall {
    path: String,
    #[serde(rename = "filePath")]
    file_path: String,
}

#[derive(Debug, Deserialize)]
struct ModernProfile {
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    data: HashMap<String, SidedValue>,
    #[serde(default)]
    processors: Vec<Processor>,
    #[serde(default)]
    libraries: Vec<Library>,
}

#[derive(Debug, Deserialize)]
struct SidedValue {
    client: String,
}

#[derive(Debug, Deserialize)]
struct Processor {
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
    #[serde(default)]
    sides: Option<Vec<String>>,
}

fn zip_read(archive: &mut zip::ZipArchive<File>, name: &str) -> Result<Vec<u8>, TaskError> {
    let mut entry = archive.by_name(name.trim_start_matches('/')).map_err(TaskError::other)?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

fn zip_extract(archive: &mut zip::ZipArchive<File>, name: &str, dest: &Path) -> Result<(), TaskError> {
    let bytes = zip_read(archive, name)?;
    if let Some(parent) = dest.parent() {
        ensure_dir(parent)?;
    }
    fs::write(dest, bytes)?;
    Ok(())
}

fn library_path(game_dir: &Path, coord: &str) -> Result<PathBuf, TaskError> {
    maven_to_path(coord)
        .map(|rel| libraries_dir(game_dir).join(rel))
        .ok_or_else(|| TaskError::other(format!("无效的 Maven 坐标: {coord}")))
}

/// Writes the profile JSON under its instance id and returns the id.
fn write_profile(game_dir: &Path, mut json: serde_json::Value, name: Option<&str>, game_version: &str) -> Result<String, TaskError> {
    let id = name
        .map(str::to_string)
        .or_else(|| json["id"].as_str().map(str::to_string))
        .ok_or_else(|| TaskError::other("安装器中的版本 JSON 缺少 id"))?;
    json["id"] = serde_json::Value::String(id.clone());
    json["inheritsFrom"] = serde_json::Value::String(game_version.to_string());
    ensure_dir(&version_dir(game_dir, &id))?;
    fs::write(version_json_path(game_dir, &id), serde_json::to_string_pretty(&json).map_err(TaskError::other)?)?;
    Ok(id)
}

/// Installs Forge or NeoForge (version picked automatically when not given). Returns the instance id.
/// The vanilla version must already be installed.
pub fn install(game_dir: &Path, req: &InstanceRequest, source: DownloadSource, task: &TaskHandle) -> Result<String, TaskError> {
    let mc = req.game_version.as_str();
    task.set_stage("获取加载器版本");
    let version = match &req.loader_version {
        Some(v) => v.clone(),
        None => {
            let list = list_versions(req.loader, mc, source).map_err(TaskError::other)?;
            list.iter()
                .find(|v| v.stable)
                .or(list.first())
                .map(|v| v.version.clone())
                .ok_or_else(|| TaskError::other(format!("没有适用于 {mc} 的 {}", req.loader.display_name())))?
        }
    };

    task.set_stage("下载安装器");
    let (coord, url) = installer_coord(req.loader, &version);
    let installer = library_path(game_dir, &coord)?;
    download_all(vec![DownloadItem::new(candidate_urls(&url, source), installer.clone(), None, 0)], task, 1)?;
    let mut archive = zip::ZipArchive::new(File::open(&installer)?).map_err(TaskError::other)?;
    let profile: serde_json::Value = serde_json::from_slice(&zip_read(&mut archive, "install_profile.json")?).map_err(TaskError::other)?;

    if profile.get("install").is_some() {
        let legacy: LegacyProfile = serde_json::from_value(profile).map_err(TaskError::other)?;
        task.set_stage("解压 Forge");
        zip_extract(&mut archive, &legacy.install.file_path, &library_path(game_dir, &legacy.install.path)?)?;
        return write_profile(game_dir, legacy.version_info, req.name.as_deref(), mc);
    }

    let modern: ModernProfile = serde_json::from_value(profile).map_err(TaskError::other)?;
    let json_name = modern.json.clone().unwrap_or_else(|| "version.json".to_string());
    let version_json: serde_json::Value = serde_json::from_slice(&zip_read(&mut archive, &json_name)?).map_err(TaskError::other)?;
    let version_info: VersionInfo = serde_json::from_value(version_json.clone()).map_err(TaskError::other)?;

    task.set_stage("解压安装器文件");
    extract_maven_dir(&mut archive, &libraries_dir(game_dir))?;

    task.set_stage("下载加载器依赖");
    let mut libs = VersionInfo { libraries: modern.libraries.clone(), ..Default::default() };
    libs.libraries.extend(version_info.libraries);
    download_all(library_items(game_dir, &libs, source), task, DEFAULT_WORKERS)?;

    let work_dir = game_dir.join(".fxlauncher").join("forge-install");
    let data = build_data(game_dir, &modern, &mut archive, &installer, mc, &work_dir)?;
    run_processors(game_dir, &modern.processors, &data, &req.java, task)?;
    let _ = fs::remove_dir_all(&work_dir);

    write_profile(game_dir, version_json, req.name.as_deref(), mc)
}

fn extract_maven_dir(archive: &mut zip::ZipArchive<File>, lib_dir: &Path) -> Result<(), TaskError> {
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(TaskError::other)?;
        let name = entry.name().map_err(TaskError::other)?.to_string();
        let Some(rel) = name.strip_prefix("maven/") else { continue };
        if entry.is_dir() || rel.is_empty() || rel.contains("..") {
            continue;
        }
        let dest = lib_dir.join(rel);
        if dest.is_file() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            ensure_dir(parent)?;
        }
        io::copy(&mut entry, &mut File::create(&dest)?)?;
    }
    Ok(())
}

fn build_data(
    game_dir: &Path,
    profile: &ModernProfile,
    archive: &mut zip::ZipArchive<File>,
    installer: &Path,
    mc: &str,
    work_dir: &Path,
) -> Result<HashMap<String, String>, TaskError> {
    let mut data = HashMap::new();
    for (key, value) in &profile.data {
        let v = &value.client;
        let resolved = if let Some(coord) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            library_path(game_dir, coord)?.to_string_lossy().to_string()
        } else if let Some(lit) = v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            lit.to_string()
        } else if v.starts_with('/') {
            let dest = work_dir.join(v.trim_start_matches('/'));
            zip_extract(archive, v, &dest)?;
            dest.to_string_lossy().to_string()
        } else {
            v.clone()
        };
        data.insert(key.clone(), resolved);
    }
    let path_str = |p: PathBuf| p.to_string_lossy().to_string();
    data.insert("SIDE".into(), "client".into());
    data.insert("MINECRAFT_JAR".into(), path_str(version_jar_path(game_dir, mc)));
    data.insert("MINECRAFT_VERSION".into(), mc.to_string());
    data.insert("ROOT".into(), path_str(game_dir.to_path_buf()));
    data.insert("INSTALLER".into(), path_str(installer.to_path_buf()));
    data.insert("LIBRARY_DIR".into(), path_str(libraries_dir(game_dir)));
    Ok(data)
}

/// Substitutes `{KEY}` data references and `[maven:coord]` artifact references in a processor argument.
fn resolve_arg(game_dir: &Path, arg: &str, data: &HashMap<String, String>) -> Result<String, TaskError> {
    if let Some(coord) = arg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return Ok(library_path(game_dir, coord)?.to_string_lossy().to_string());
    }
    if let Some(lit) = arg.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ok(lit.to_string());
    }
    let mut out = arg.to_string();
    for (k, v) in data {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    Ok(out)
}

fn jar_main_class(jar: &Path) -> Result<String, TaskError> {
    let mut archive = zip::ZipArchive::new(File::open(jar)?).map_err(TaskError::other)?;
    let manifest = String::from_utf8_lossy(&zip_read(&mut archive, "META-INF/MANIFEST.MF")?).to_string();
    manifest
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class:").map(|c| c.trim().to_string()))
        .ok_or_else(|| TaskError::other(format!("{} 没有 Main-Class", jar.display())))
}

fn outputs_ok(game_dir: &Path, outputs: &HashMap<String, String>, data: &HashMap<String, String>) -> Result<bool, TaskError> {
    if outputs.is_empty() {
        return Ok(false);
    }
    for (path, sha1) in outputs {
        let path = PathBuf::from(resolve_arg(game_dir, path, data)?);
        let sha1 = resolve_arg(game_dir, sha1, data)?;
        if !compute_file_sha1(&path).is_ok_and(|h| h.eq_ignore_ascii_case(&sha1)) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn run_processors(game_dir: &Path, processors: &[Processor], data: &HashMap<String, String>, java: &str, task: &TaskHandle) -> Result<(), TaskError> {
    let client: Vec<&Processor> = processors
        .iter()
        .filter(|p| p.sides.as_ref().is_none_or(|s| s.iter().any(|x| x == "client")))
        .collect();
    task.reset_progress(0, client.len() as u64);
    for (i, proc) in client.iter().enumerate() {
        task.checkpoint()?;
        task.set_stage(&format!("运行安装处理器 {}/{}", i + 1, client.len()));
        if !outputs_ok(game_dir, &proc.outputs, data)? {
            let jar = library_path(game_dir, &proc.jar)?;
            let mut cp = vec![jar.clone()];
            for c in &proc.classpath {
                cp.push(library_path(game_dir, c)?);
            }
            let cp = cp.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(&classpath_separator().to_string());
            let args = proc.args.iter().map(|a| resolve_arg(game_dir, a, data)).collect::<Result<Vec<_>, _>>()?;

            let output = Command::new(java)
                .current_dir(game_dir)
                .arg("-cp")
                .arg(&cp)
                .arg(jar_main_class(&jar)?)
                .args(&args)
                .output()
                .map_err(|e| TaskError::other(format!("无法运行 Java ({java}): {e}")))?;
            if !output.status.success() {
                let log = String::from_utf8_lossy(&output.stderr);
                let tail: Vec<&str> = log.lines().rev().take(8).collect();
                let tail: Vec<&str> = tail.into_iter().rev().collect();
                return Err(TaskError::other(format!("安装处理器 {} 失败:\n{}", proc.jar, tail.join("\n"))));
            }
            if !proc.outputs.is_empty() && !outputs_ok(game_dir, &proc.outputs, data)? {
                return Err(TaskError::other(format!("安装处理器 {} 的输出校验失败", proc.jar)));
            }
        }
        task.done_items.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}
