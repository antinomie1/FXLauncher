use fxlauncher::config::LauncherConfig;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_default_config_values() {
    let cfg = LauncherConfig::default();
    assert_eq!(cfg.player_name, "Steve");
    assert!(cfg.offline_mode);
    assert_eq!(cfg.selected_version, "1.21.4");
    assert_eq!(cfg.java_path, "java");
    assert_eq!(cfg.min_memory_mb, 1024);
    assert_eq!(cfg.max_memory_mb, 4096);
    assert_eq!(cfg.window_width, 854);
    assert_eq!(cfg.window_height, 480);
}

#[test]
fn test_save_and_load_roundtrip() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("config.json");

    let mut original = LauncherConfig::default();
    original.player_name = "Alex".to_string();
    original.selected_version = "1.20.1".to_string();
    original.max_memory_mb = 8192;
    original.game_dir = PathBuf::from("/test/minecraft");

    original.save_to_path(&file_path).unwrap();
    assert!(file_path.is_file());

    let loaded = LauncherConfig::load_from_path(&file_path).unwrap();
    assert_eq!(original, loaded);
}

#[test]
fn test_ensure_directories() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path().join(".minecraft");

    let mut cfg = LauncherConfig::default();
    cfg.game_dir = game_dir.clone();

    cfg.ensure_game_directories().unwrap();
    assert!(game_dir.is_dir());
    assert!(game_dir.join("versions").is_dir());
    assert!(game_dir.join("libraries").is_dir());
}
