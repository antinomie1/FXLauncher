use fxlauncher::version::{
    OsRule, Rule, VersionInfo, collect_classpaths, evaluate_rules, maven_to_path,
    scan_local_versions,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn test_maven_to_path() {
    let p1 = maven_to_path("com.mojang:patchy:1.3.9").unwrap();
    assert_eq!(p1, PathBuf::from("com/mojang/patchy/1.3.9/patchy-1.3.9.jar"));

    let p2 = maven_to_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap();
    assert_eq!(
        p2,
        PathBuf::from("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar")
    );

    assert_eq!(maven_to_path("invalid_coordinate"), None);
}

#[test]
fn test_evaluate_rules() {
    // Empty rules -> always allowed
    assert!(evaluate_rules(&[], "linux"));
    assert!(evaluate_rules(&[], "windows"));

    // Allow only on windows
    let win_only = vec![Rule {
        action: "allow".to_string(),
        os: Some(OsRule {
            name: Some("windows".to_string()),
            version: None,
            arch: None,
        }),
    }];
    assert!(evaluate_rules(&win_only, "windows"));
    assert!(!evaluate_rules(&win_only, "linux"));
    assert!(!evaluate_rules(&win_only, "osx"));

    // Allow all, but disallow osx
    let disallow_osx = vec![
        Rule {
            action: "allow".to_string(),
            os: None,
        },
        Rule {
            action: "disallow".to_string(),
            os: Some(OsRule {
                name: Some("osx".to_string()),
                version: None,
                arch: None,
            }),
        },
    ];
    assert!(evaluate_rules(&disallow_osx, "linux"));
    assert!(evaluate_rules(&disallow_osx, "windows"));
    assert!(!evaluate_rules(&disallow_osx, "osx"));
}

#[test]
fn test_parse_modern_version_json() {
    let json = r#"{
        "id": "1.21.4",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "assets": "17",
        "libraries": [
            {
                "name": "com.mojang:logging:1.2.7",
                "downloads": {
                    "artifact": {
                        "path": "com/mojang/logging/1.2.7/logging-1.2.7.jar",
                        "url": "https://libraries.minecraft.net/com/mojang/logging/1.2.7/logging-1.2.7.jar"
                    }
                }
            }
        ],
        "arguments": {
            "game": [
                "--username", "${auth_player_name}",
                "--version", "${version_name}"
            ],
            "jvm": [
                "-Djava.library.path=${natives_directory}",
                "-Dminecraft.launcher.brand=${launcher_name}",
                "-cp", "${classpath}"
            ]
        }
    }"#;

    let info: VersionInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.id, "1.21.4");
    assert_eq!(info.version_type, "release");
    assert_eq!(info.main_class, "net.minecraft.client.main.Main");
    assert_eq!(info.libraries.len(), 1);
    assert!(info.arguments.is_some());
}

#[test]
fn test_collect_classpaths_and_scan() {
    let dir = tempdir().unwrap();
    let game_dir = dir.path();
    let v_dir = game_dir.join("versions").join("1.21.4");
    fs::create_dir_all(&v_dir).unwrap();

    let json = r#"{
        "id": "1.21.4",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "libraries": [
            {
                "name": "com.mojang:patchy:1.3.9"
            }
        ]
    }"#;
    fs::write(v_dir.join("1.21.4.json"), json).unwrap();

    // Test scan_local_versions
    let scanned = scan_local_versions(game_dir);
    assert_eq!(scanned, vec!["1.21.4".to_string()]);

    let info: VersionInfo = serde_json::from_str(json).unwrap();
    let cps = collect_classpaths(game_dir, &info);
    assert_eq!(cps.len(), 2); // 1 library + 1 client jar
    assert!(cps[0].ends_with(Path::new("com/mojang/patchy/1.3.9/patchy-1.3.9.jar")));
    assert!(cps[1].ends_with(Path::new("versions/1.21.4/1.21.4.jar")));
}
