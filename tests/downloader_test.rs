use fxlauncher::downloader::{DownloadSource, setup_isolated_version_dir, transform_url};
use tempfile::tempdir;

#[test]
fn test_transform_url_official_vs_bmclapi() {
    let original = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
    assert_eq!(transform_url(original, DownloadSource::Official), original);

    let bmclapi = transform_url(original, DownloadSource::Bmclapi);
    assert_eq!(
        bmclapi,
        "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json"
    );

    let lib_url = "https://libraries.minecraft.net/com/mojang/patchy/1.3.9/patchy-1.3.9.jar";
    let bmclapi_lib = transform_url(lib_url, DownloadSource::Bmclapi);
    assert_eq!(
        bmclapi_lib,
        "https://bmclapi2.bangbang93.com/maven/com/mojang/patchy/1.3.9/patchy-1.3.9.jar"
    );
}

#[test]
fn test_setup_isolated_version_dir() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let ver_dir = setup_isolated_version_dir(game_dir, "1.21.4").unwrap();

    assert!(ver_dir.is_dir());
    assert!(ver_dir.join("mods").is_dir());
    assert!(ver_dir.join("config").is_dir());
    assert!(ver_dir.join("saves").is_dir());
}
