use fxlauncher::paths::{
    arch_name, assets_dir, classpath_separator, default_minecraft_dir, libraries_dir, natives_dir,
    os_name, versions_dir,
};
use std::path::Path;

#[test]
fn test_default_minecraft_dir_not_empty() {
    let dir = default_minecraft_dir();
    assert!(!dir.as_os_str().is_empty());
    assert!(dir.to_string_lossy().contains(".minecraft") || dir.to_string_lossy().contains("minecraft"));
}

#[test]
fn test_subdirectories_paths() {
    let game = Path::new("/custom/mc");
    assert_eq!(versions_dir(game), Path::new("/custom/mc/versions"));
    assert_eq!(libraries_dir(game), Path::new("/custom/mc/libraries"));
    assert_eq!(assets_dir(game), Path::new("/custom/mc/assets"));
    assert_eq!(
        natives_dir(game, "1.21.4"),
        Path::new("/custom/mc/versions/1.21.4/natives")
    );
}

#[test]
fn test_classpath_separator() {
    let sep = classpath_separator();
    if cfg!(target_os = "windows") {
        assert_eq!(sep, ';');
    } else {
        assert_eq!(sep, ':');
    }
}

#[test]
fn test_os_and_arch_names() {
    let os = os_name();
    assert!(["windows", "osx", "linux"].contains(&os));

    let arch = arch_name();
    assert!(["x86", "x86_64", "arm", "aarch64", "unknown"].contains(&arch));
}
