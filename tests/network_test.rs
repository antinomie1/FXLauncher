//! Real-network install checks. Opt-in: `cargo test --test network_test -- --ignored --nocapture`.
//! Set `FX_TEST_GAME_DIR` to reuse a game directory between runs.

use fxlauncher::downloader::DownloadSource;
use fxlauncher::net::fabric::list_loader_versions;
use fxlauncher::net::forge::list_versions;
use fxlauncher::net::install::{InstanceRequest, install_instance};
use fxlauncher::net::mods::ModLoader;
use fxlauncher::task::{TaskManager, TaskStatus};
use fxlauncher::version::{collect_classpaths, load_version_info};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn game_dir() -> (PathBuf, Option<tempfile::TempDir>) {
    match std::env::var_os("FX_TEST_GAME_DIR") {
        Some(dir) => (PathBuf::from(dir), None),
        None => {
            let tmp = tempfile::tempdir().unwrap();
            (tmp.path().to_path_buf(), Some(tmp))
        }
    }
}

fn install_and_wait(req: InstanceRequest) -> (PathBuf, String) {
    let (dir, keep) = game_dir();
    std::mem::forget(keep);
    let manager = TaskManager::new();
    let name = req.name.clone().unwrap();
    let task = install_instance(&dir, req, DownloadSource::Bmclapi, &manager);
    loop {
        let s = task.snapshot();
        eprintln!(
            "[{:?}] {} {}/{} files {:.1}% {} KiB/s",
            s.status, s.stage, s.done_items, s.total_items, s.progress_percent, s.speed_bps / 1024
        );
        if s.status.is_finished() {
            assert_eq!(s.status, TaskStatus::Completed, "{:?}", s.error_msg);
            return (dir, name);
        }
        thread::sleep(Duration::from_secs(2));
    }
}

#[test]
#[ignore]
fn network_list_loader_versions() {
    let fabric = list_loader_versions(ModLoader::Fabric, "1.21.1", DownloadSource::Bmclapi).unwrap();
    assert!(fabric.iter().any(|v| v.stable));
    let forge = list_versions(ModLoader::Forge, "1.20.1", DownloadSource::Bmclapi).unwrap();
    assert!(forge.iter().all(|v| v.version.starts_with("1.20.1-")));
    let neo = list_versions(ModLoader::NeoForge, "1.21.1", DownloadSource::Bmclapi).unwrap();
    assert!(neo.iter().all(|v| v.version.starts_with("21.1.")));
    eprintln!("fabric {:?}\nforge {:?}\nneo {:?}", fabric.first(), forge.first(), neo.first());
}

#[test]
#[ignore]
fn network_install_fabric_instance() {
    let (dir, id) = install_and_wait(InstanceRequest {
        game_version: "1.21.1".into(),
        loader: ModLoader::Fabric,
        loader_version: None,
        name: Some("Fabric 1.21.1".into()),
        java: "java".into(),
    });
    let info = load_version_info(&dir, &id).unwrap();
    for path in collect_classpaths(&dir, &info) {
        assert!(path.is_file(), "missing {}", path.display());
    }
    assert!(dir.join("versions").join(&id).join("natives").read_dir().unwrap().next().is_some());
}

/// Without a usable Java the install must get through installer download, profile parsing,
/// Maven extraction and library downloads, then stop exactly at the processor step.
#[test]
#[ignore]
fn network_forge_and_neoforge_until_processors() {
    use fxlauncher::net::forge;
    let (dir, keep) = game_dir();
    std::mem::forget(keep);
    let manager = TaskManager::new();
    for (loader, mc) in [(ModLoader::Forge, "1.20.1"), (ModLoader::NeoForge, "1.21.1")] {
        let req = InstanceRequest {
            game_version: mc.into(),
            loader,
            loader_version: None,
            name: Some(format!("{mc}-{}", loader.display_name())),
            java: "/nonexistent/java".into(),
        };
        let task = manager.create_task("forge");
        let err = forge::install(&dir, &req, DownloadSource::Bmclapi, &task).unwrap_err().to_string();
        eprintln!("{loader:?}: stage={:?} err={err}", task.snapshot().stage);
        assert!(err.contains("无法运行 Java"), "{err}");
        assert!(task.snapshot().stage.starts_with("运行安装处理器 1/"));
    }
}
