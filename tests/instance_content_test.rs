use fxlauncher::game::loader::detect_loader_from_info;
use fxlauncher::game::mods::{
    analyze_instance_mods, deduce_mod_info_from_filename, delete_mod, disable_mod,
    list_instance_mods, toggle_mod,
};
use fxlauncher::game::saves::{
    backup_world_save, delete_world_save, list_world_saves, rename_world_save,
};
use fxlauncher::game::shaders::{
    delete_shaderpack, get_active_shaderpack, list_shaderpacks, set_active_shaderpack,
};
use fxlauncher::game::version::{Library, VersionInfo};
use fxlauncher::net::mods::ModLoader;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_detect_instance_loader() {
    let mut info = VersionInfo {
        id: "1.21.4-fabric".to_string(),
        version_type: "release".to_string(),
        main_class: "net.fabricmc.loader.impl.launch.knot.KnotClient".to_string(),
        asset_index: None,
        assets: None,
        libraries: vec![
            Library {
                name: "net.fabricmc:fabric-loader:0.16.9".to_string(),
                downloads: None,
                rules: None,
                natives: None,
            },
            Library {
                name: "optifine:OptiFine:1.21.4_HD_U_I1".to_string(),
                downloads: None,
                rules: None,
                natives: None,
            },
        ],
        legacy_arguments: None,
        arguments: None,
    };

    let detected = detect_loader_from_info(&info);
    assert_eq!(detected.loader, ModLoader::Fabric);
    assert_eq!(detected.loader_version.as_deref(), Some("0.16.9"));
    assert!(detected.has_optifine);

    // Test Forge
    info.main_class = "net.minecraftforge.bootstrap.ForgeBootstrap".to_string();
    info.libraries = vec![Library {
        name: "net.minecraftforge:forge:1.20.1-47.3.0".to_string(),
        downloads: None,
        rules: None,
        natives: None,
    }];
    let forge_detected = detect_loader_from_info(&info);
    assert_eq!(forge_detected.loader, ModLoader::Forge);
    assert_eq!(forge_detected.loader_version.as_deref(), Some("1.20.1-47.3.0"));

    // Test NeoForge
    info.main_class = "net.neoforged.neoforge.client.ClientModLoader".to_string();
    info.libraries = vec![Library {
        name: "net.neoforged:neoforge:20.4.80-beta".to_string(),
        downloads: None,
        rules: None,
        natives: None,
    }];
    let neoforge_detected = detect_loader_from_info(&info);
    assert_eq!(neoforge_detected.loader, ModLoader::NeoForge);
    assert_eq!(neoforge_detected.loader_version.as_deref(), Some("20.4.80-beta"));
}

#[test]
fn test_deduce_mod_info_from_filename() {
    let (id1, ver1, mc1) = deduce_mod_info_from_filename("sodium-fabric-0.5.8+mc1.21.jar");
    assert_eq!(id1, "sodium");
    assert_eq!(ver1.as_deref(), Some("0.5.8"));
    assert_eq!(mc1.as_deref(), Some("1.21"));

    let (id2, ver2, mc2) = deduce_mod_info_from_filename("iris-mc1.20.1-1.6.4.jar.disabled");
    assert_eq!(id2, "iris");
    assert_eq!(ver2.as_deref(), Some("1.6.4"));
    assert_eq!(mc2.as_deref(), Some("1.20.1"));
}

#[test]
fn test_mod_lifecycle_enable_disable_delete() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let ver_id = "1.21.4";
    let mods_dir = fxlauncher::paths::version_mods_dir(game_dir, ver_id);
    fs::create_dir_all(&mods_dir).unwrap();

    let mod_file = mods_dir.join("fabric-api-0.92.0.jar");
    fs::write(&mod_file, b"dummy mod content").unwrap();

    // 1. Initial list
    let mods = list_instance_mods(game_dir, ver_id);
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].file_name, "fabric-api-0.92.0.jar");
    assert!(mods[0].enabled);

    // 2. Disable mod
    let disabled_path = disable_mod(game_dir, ver_id, "fabric-api-0.92.0.jar").unwrap();
    assert!(disabled_path.ends_with("fabric-api-0.92.0.jar.disabled"));
    assert!(disabled_path.is_file());
    assert!(!mod_file.is_file());

    let mods_after_disable = list_instance_mods(game_dir, ver_id);
    assert_eq!(mods_after_disable.len(), 1);
    assert_eq!(mods_after_disable[0].file_name, "fabric-api-0.92.0.jar.disabled");
    assert!(!mods_after_disable[0].enabled);

    // 3. Toggle mod (disabled -> enabled)
    let toggled_path = toggle_mod(game_dir, ver_id, "fabric-api-0.92.0.jar.disabled").unwrap();
    assert!(toggled_path.ends_with("fabric-api-0.92.0.jar"));
    assert!(toggled_path.is_file());

    // 4. Delete mod
    delete_mod(game_dir, ver_id, "fabric-api-0.92.0.jar").unwrap();
    assert_eq!(list_instance_mods(game_dir, ver_id).len(), 0);
}

#[test]
fn test_mod_analysis() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let ver_id = "1.21.4";
    let mods_dir = fxlauncher::paths::version_mods_dir(game_dir, ver_id);
    fs::create_dir_all(&mods_dir).unwrap();

    // Create duplicate sodium mods
    fs::write(mods_dir.join("sodium-fabric-0.5.8+mc1.21.jar"), b"a").unwrap();
    fs::write(mods_dir.join("sodium-fabric-0.5.7+mc1.21.jar"), b"b").unwrap();
    // Create iris without sodium dependency satisfaction test
    fs::write(mods_dir.join("iris-mc1.20.1-1.6.4.jar"), b"c").unwrap();
    // Create forge mod in fabric test
    fs::write(mods_dir.join("jei-1.20.1-forge-15.3.0.jar"), b"d").unwrap();

    let report = analyze_instance_mods(
        game_dir,
        ver_id,
        Some("1.21.4"),
        Some(ModLoader::Fabric),
    );

    assert_eq!(report.total_mods, 4);
    assert_eq!(report.enabled_mods, 4);

    // Duplicate alert for sodium
    assert_eq!(report.duplicate_mods.len(), 1);
    assert_eq!(report.duplicate_mods[0].mod_id, "sodium");
    assert_eq!(report.duplicate_mods[0].files.len(), 2);

    // Version warning for iris (marked 1.20.1 in 1.21.4)
    assert!(report.compatibility_warnings.iter().any(|w| w.contains("1.20.1")));

    // Loader warning for jei (marked forge in fabric)
    assert!(report.compatibility_warnings.iter().any(|w| w.contains("Forge 专属版本")));
}

#[test]
fn test_shaderpacks_management() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let ver_id = "1.21.4";
    let shader_dir = fxlauncher::paths::version_shaderpacks_dir(game_dir, ver_id);
    fs::create_dir_all(&shader_dir).unwrap();

    fs::write(shader_dir.join("ComplementaryReimagined.zip"), b"dummy shader").unwrap();
    fs::write(shader_dir.join("BSL_Shaders.zip"), b"dummy shader 2").unwrap();

    // 1. Initial list (active is None)
    let list = list_shaderpacks(game_dir, ver_id);
    assert_eq!(list.len(), 2);
    assert_eq!(get_active_shaderpack(game_dir, ver_id), None);

    // 2. Set active shaderpack
    set_active_shaderpack(game_dir, ver_id, Some("ComplementaryReimagined.zip")).unwrap();
    assert_eq!(
        get_active_shaderpack(game_dir, ver_id).as_deref(),
        Some("ComplementaryReimagined.zip")
    );

    let updated_list = list_shaderpacks(game_dir, ver_id);
    let comp = updated_list.iter().find(|s| s.file_name == "ComplementaryReimagined.zip").unwrap();
    assert!(comp.is_active);

    // 3. Delete shaderpack
    delete_shaderpack(game_dir, ver_id, "BSL_Shaders.zip").unwrap();
    assert_eq!(list_shaderpacks(game_dir, ver_id).len(), 1);
}

#[test]
fn test_world_saves_management() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let ver_id = "1.21.4";
    let saves_dir = fxlauncher::paths::version_saves_dir(game_dir, ver_id);

    let world_dir = saves_dir.join("Survival_World");
    fs::create_dir_all(&world_dir).unwrap();
    fs::write(world_dir.join("level.dat"), b"dummy nbt level data").unwrap();
    fs::write(world_dir.join("icon.png"), b"dummy png").unwrap();

    // 1. List world saves
    let saves = list_world_saves(game_dir, ver_id);
    assert_eq!(saves.len(), 1);
    assert_eq!(saves[0].folder_name, "Survival_World");
    assert!(saves[0].has_icon);
    assert!(saves[0].size_bytes > 0);

    // 2. Backup world save
    let backup_path = backup_world_save(game_dir, ver_id, "Survival_World").unwrap();
    assert!(backup_path.is_dir());
    assert!(backup_path.join("level.dat").is_file());
    assert_eq!(list_world_saves(game_dir, ver_id).len(), 2);

    // 3. Rename world save
    let renamed = rename_world_save(game_dir, ver_id, "Survival_World", "My_World").unwrap();
    assert!(renamed.is_dir());
    assert!(!world_dir.is_dir());

    // 4. Delete world save
    delete_world_save(game_dir, ver_id, "My_World").unwrap();
    assert!(!renamed.is_dir());
}
