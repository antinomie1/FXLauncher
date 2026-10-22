use fxlauncher::mod_manager::{ModLoader, list_installed_mods, version_mods_dir};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_version_mods_dir_isolation() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();

    let mods_1214 = version_mods_dir(game_dir, "1.21.4");
    let mods_1201 = version_mods_dir(game_dir, "1.20.1");

    assert_ne!(mods_1214, mods_1201);
    assert!(mods_1214.ends_with("versions/1.21.4/mods"));
    assert!(mods_1201.ends_with("versions/1.20.1/mods"));
}

#[test]
fn test_list_installed_mods() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let mods_dir = version_mods_dir(game_dir, "1.21.4");
    fs::create_dir_all(&mods_dir).unwrap();

    fs::write(mods_dir.join("fabric-api-0.92.0.jar"), b"dummy jar 1").unwrap();
    fs::write(mods_dir.join("sodium-0.5.8.jar"), b"dummy jar 2").unwrap();
    fs::write(mods_dir.join("readme.txt"), b"not a mod").unwrap();

    let installed = list_installed_mods(game_dir, "1.21.4");
    assert_eq!(installed.len(), 2);
    assert_eq!(installed[0].file_name, "fabric-api-0.92.0.jar");
    assert_eq!(installed[1].file_name, "sodium-0.5.8.jar");

    // Other version has zero mods
    let installed_other = list_installed_mods(game_dir, "1.20.1");
    assert_eq!(installed_other.len(), 0);
}

#[test]
fn test_mod_loader_strings() {
    assert_eq!(ModLoader::Vanilla.as_str(), "vanilla");
    assert_eq!(ModLoader::Fabric.as_str(), "fabric");
    assert_eq!(ModLoader::Forge.as_str(), "forge");
    assert_eq!(ModLoader::NeoForge.as_str(), "neoforge");
    assert_eq!(ModLoader::Quilt.as_str(), "quilt");
}
