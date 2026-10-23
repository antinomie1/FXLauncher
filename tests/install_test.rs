use fxlauncher::account::Account;
use fxlauncher::config::LauncherConfig;
use fxlauncher::downloader::{DownloadSource, candidate_urls, transform_url};
use fxlauncher::game::natives::{classifier_matches_arch, extract_natives};
use fxlauncher::launch::{LaunchContext, build_game_args, build_token_map};
use fxlauncher::net::batch::{DownloadItem, download_all};
use fxlauncher::net::forge::neoforge_prefix;
use fxlauncher::net::install::{AssetIndex, asset_items, library_items};
use fxlauncher::task::TaskManager;
use fxlauncher::version::{VersionInfo, collect_classpaths, evaluate_rules, evaluate_rules_with, load_version_info};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

fn write_version(game_dir: &Path, id: &str, json: &str) {
    let dir = game_dir.join("versions").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{id}.json")), json).unwrap();
}

#[test]
fn test_feature_rules_skip_demo_and_keep_resolution() {
    let json = r#"{
        "id": "1.21.4", "mainClass": "net.minecraft.client.main.Main",
        "arguments": { "game": [
            "--username", "${auth_player_name}",
            { "rules": [{ "action": "allow", "features": { "is_demo_user": true } }], "value": "--demo" },
            { "rules": [{ "action": "allow", "features": { "has_custom_resolution": true } }],
              "value": ["--width", "${resolution_width}"] }
        ] }
    }"#;
    let version: VersionInfo = serde_json::from_str(json).unwrap();
    let config = LauncherConfig::default();
    let account = Account::new_offline("Steve");
    let ctx = LaunchContext { config: &config, version: &version, account: &account };
    let args = build_game_args(&ctx, &build_token_map(&ctx, ""));
    assert!(!args.contains(&"--demo".to_string()));
    assert!(args.contains(&"--width".to_string()));

    let fxlauncher::version::ArgumentItem::Conditional { rules, .. } = &version.arguments.as_ref().unwrap().game[2] else {
        unreachable!()
    };
    assert!(!evaluate_rules(rules, "linux"));
    assert!(evaluate_rules_with(rules, "linux", &["is_demo_user"]));
}

#[test]
fn test_inherited_profile_merges_onto_parent() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    write_version(game_dir, "1.20.1", r#"{
        "id": "1.20.1", "type": "release", "mainClass": "net.minecraft.client.main.Main",
        "assetIndex": { "id": "5", "url": "https://piston-meta.mojang.com/5.json" },
        "libraries": [
            { "name": "org.ow2.asm:asm:9.3" },
            { "name": "com.mojang:logging:1.1.1" }
        ],
        "arguments": { "game": ["--username", "${auth_player_name}"], "jvm": ["-cp", "${classpath}"] }
    }"#);
    write_version(game_dir, "fabric-1.20.1", r#"{
        "id": "fabric-1.20.1", "inheritsFrom": "1.20.1",
        "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
        "libraries": [
            { "name": "org.ow2.asm:asm:9.6", "url": "https://maven.fabricmc.net/" },
            { "name": "net.fabricmc:fabric-loader:0.16.9", "url": "https://maven.fabricmc.net/" }
        ],
        "arguments": { "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] }
    }"#);

    let info = load_version_info(game_dir, "fabric-1.20.1").unwrap();
    assert_eq!(info.id, "fabric-1.20.1");
    assert_eq!(info.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
    assert_eq!(info.client_jar_id(), "1.20.1");
    assert_eq!(info.asset_index_id(), Some("5"));
    assert!(info.inherits_from.is_none());

    let names: Vec<&str> = info.libraries.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["org.ow2.asm:asm:9.6", "net.fabricmc:fabric-loader:0.16.9", "com.mojang:logging:1.1.1"]);
    let args = info.arguments.as_ref().unwrap();
    assert_eq!(args.jvm.len(), 3);
    assert_eq!(args.game.len(), 2);

    // Without its own jar copy, the classpath uses the parent's client jar.
    let cp = collect_classpaths(game_dir, &info);
    assert!(cp.last().unwrap().ends_with("versions/1.20.1/1.20.1.jar"));

    // Maven-style libraries resolve against their repository, with the mirror tried first.
    let items = library_items(game_dir, &info, DownloadSource::Bmclapi);
    let loader = items.iter().find(|i| i.dest.ends_with("fabric-loader-0.16.9.jar")).unwrap();
    assert_eq!(loader.urls, [
        "https://bmclapi2.bangbang93.com/maven/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar",
        "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar",
    ]);
}

#[test]
fn test_mirror_urls_cover_loader_hosts() {
    let meta = "https://meta.fabricmc.net/v2/versions/loader/1.21.1";
    assert_eq!(transform_url(meta, DownloadSource::Bmclapi), "https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/1.21.1");
    let neo = "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.77/x.jar";
    assert_eq!(transform_url(neo, DownloadSource::Bmclapi), "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge/21.1.77/x.jar");
    let urls = candidate_urls("https://example.com/a.jar", DownloadSource::Bmclapi);
    assert_eq!(urls, ["https://example.com/a.jar"]);
}

#[test]
fn test_neoforge_prefix() {
    assert_eq!(neoforge_prefix("1.21.1").as_deref(), Some("21.1."));
    assert_eq!(neoforge_prefix("1.21").as_deref(), Some("21.0."));
    assert_eq!(neoforge_prefix("1.20.4").as_deref(), Some("20.4."));
    assert_eq!(neoforge_prefix("26.1").as_deref(), Some("26.1.0."));
    assert_eq!(neoforge_prefix("26.1.2").as_deref(), Some("26.1.2."));
    assert_eq!(neoforge_prefix("b1.7.3"), None);
}

#[test]
fn test_asset_items_layout() {
    let dir = tempdir().unwrap();
    let index: AssetIndex = serde_json::from_str(r#"{ "objects": {
        "icons/icon_16x16.png": { "hash": "bdf48ef6b5d0d23bbb02e17d04865216179f510a", "size": 3665 },
        "dup.png": { "hash": "bdf48ef6b5d0d23bbb02e17d04865216179f510a", "size": 3665 }
    } }"#).unwrap();
    let items = asset_items(dir.path(), &index, DownloadSource::Official);
    assert_eq!(items.len(), 1);
    assert!(items[0].dest.ends_with("assets/objects/bd/bdf48ef6b5d0d23bbb02e17d04865216179f510a"));
    assert_eq!(items[0].urls[0], "https://resources.download.minecraft.net/bd/bdf48ef6b5d0d23bbb02e17d04865216179f510a");
    assert_eq!(items[0].urls[1], "https://bmclapi2.bangbang93.com/assets/bd/bdf48ef6b5d0d23bbb02e17d04865216179f510a");
}

#[test]
fn test_download_all_skips_valid_files() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("hello.txt");
    fs::write(&file, b"hello world\n").unwrap();
    let item = DownloadItem::new(
        vec!["http://127.0.0.1:9/unreachable".to_string()],
        file,
        Some("22596363b3de40b06f981fb85d82312e8c0ed511".to_string()),
        12,
    );
    assert!(item.is_satisfied());
    let manager = TaskManager::new();
    let task = manager.create_task("noop");
    download_all(vec![item], &task, 4).unwrap();
    assert_eq!(task.snapshot().total_items, 0);
}

#[test]
fn test_extract_natives_flattens_and_skips_meta_inf() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let rel = "org/lwjgl/lwjgl/3.2.2/lwjgl-3.2.2-natives.jar";
    let jar = game_dir.join("libraries").join(rel);
    fs::create_dir_all(jar.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar).unwrap());
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in ["linux/x64/org/lwjgl/liblwjgl.so", "lwjgl.dll", "META-INF/evil.so", "README.txt"] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(b"native").unwrap();
    }
    zip.finish().unwrap();

    let json = format!(r#"{{
        "id": "1.14", "mainClass": "x",
        "libraries": [{{
            "name": "org.lwjgl:lwjgl:3.2.2",
            "natives": {{ "linux": "natives", "windows": "natives", "osx": "natives" }},
            "extract": {{ "exclude": ["META-INF/"] }},
            "downloads": {{ "classifiers": {{ "natives": {{ "path": "{rel}", "url": "https://x/{rel}" }} }} }}
        }}]
    }}"#);
    let info: VersionInfo = serde_json::from_str(&json).unwrap();
    // Native-only libraries never land on the classpath.
    assert_eq!(collect_classpaths(game_dir, &info).len(), 1);

    assert_eq!(extract_natives(game_dir, &info).unwrap(), 2);
    let natives = game_dir.join("versions/1.14/natives");
    assert!(natives.join("liblwjgl.so").is_file());
    assert!(natives.join("lwjgl.dll").is_file());
    assert!(!natives.join("evil.so").exists());
    assert!(!natives.join("README.txt").exists());
    // Second run is a no-op.
    assert_eq!(extract_natives(game_dir, &info).unwrap(), 0);

    assert!(classifier_matches_arch("natives-windows-arm64") == cfg!(target_arch = "aarch64"));
}
