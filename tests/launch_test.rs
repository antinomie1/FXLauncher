use fxlauncher::account::Account;
use fxlauncher::config::LauncherConfig;
use fxlauncher::launch::{
    LaunchContext, build_game_args, build_jvm_args, build_launch_command, build_token_map,
    replace_tokens,
};
use fxlauncher::version::VersionInfo;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_replace_tokens() {
    let mut tokens = HashMap::new();
    tokens.insert("auth_player_name", "Steve".to_string());
    tokens.insert("version_name", "1.21.4".to_string());

    let template = "Hello, ${auth_player_name}! Welcome to Minecraft ${version_name}.";
    let replaced = replace_tokens(template, &tokens);
    assert_eq!(replaced, "Hello, Steve! Welcome to Minecraft 1.21.4.");
}

#[test]
fn test_build_launch_command_modern() {
    let json = r#"{
        "id": "1.21.4",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "libraries": [],
        "arguments": {
            "game": [
                "--username", "${auth_player_name}",
                "--version", "${version_name}",
                "--gameDir", "${game_directory}"
            ],
            "jvm": [
                "-Dminecraft.launcher.brand=${launcher_name}",
                "-Dminecraft.launcher.version=${launcher_version}",
                "-cp", "${classpath}"
            ]
        }
    }"#;

    let version: VersionInfo = serde_json::from_str(json).unwrap();
    let config = LauncherConfig {
        game_dir: PathBuf::from("/home/user/.minecraft"),
        selected_version: "1.21.4".to_string(),
        java_path: "/usr/bin/java".to_string(),
        min_memory_mb: 2048,
        max_memory_mb: 4096,
        extra_jvm_args: vec!["-XX:+UseG1GC".to_string()],
        window_width: 854,
        window_height: 480,
        version_isolation: false,
        download_source: fxlauncher::downloader::DownloadSource::Official,
        ..Default::default()
    };
    let account = Account::new_offline("Steve");

    let ctx = LaunchContext {
        config: &config,
        version: &version,
        account: &account,
    };

    let cmd = build_launch_command(&ctx).unwrap();
    assert_eq!(cmd.get_program(), "/usr/bin/java");

    let args: Vec<String> = cmd
        .get_args()
        .map(|s| s.to_string_lossy().to_string())
        .collect();

    // Verify memory flags
    assert!(args.contains(&"-Xms2048M".to_string()));
    assert!(args.contains(&"-Xmx4096M".to_string()));
    assert!(args.contains(&"-XX:+UseG1GC".to_string()));

    // Verify main class
    assert!(args.contains(&"net.minecraft.client.main.Main".to_string()));

    // Verify game arguments
    assert!(args.contains(&"--username".to_string()));
    assert!(args.contains(&"Steve".to_string()));
    assert!(args.contains(&"--version".to_string()));
    assert!(args.contains(&"1.21.4".to_string()));
    assert!(args.contains(&"--gameDir".to_string()));
    assert!(args.contains(&"/home/user/.minecraft".to_string()));
}

#[test]
fn test_build_launch_command_legacy() {
    let json = r#"{
        "id": "1.12.2",
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "libraries": [],
        "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory}"
    }"#;

    let version: VersionInfo = serde_json::from_str(json).unwrap();
    let config = LauncherConfig::default();
    let account = Account::new_offline("Alex");

    let ctx = LaunchContext {
        config: &config,
        version: &version,
        account: &account,
    };

    let tokens = build_token_map(&ctx, "dummy.jar");
    let game_args = build_game_args(&ctx, &tokens);

    let expected_dir = config.game_dir.join("versions").join("1.12.2");
    assert_eq!(
        game_args,
        vec![
            "--username",
            "Alex",
            "--version",
            "1.12.2",
            "--gameDir",
            expected_dir.to_string_lossy().as_ref()
        ]
    );

    let jvm_args = build_jvm_args(&ctx, &tokens, "dummy.jar");
    assert!(jvm_args.contains(&"-Xms1024M".to_string()));
    assert!(jvm_args.contains(&"-Xmx4096M".to_string()));
    assert!(jvm_args.contains(&"-cp".to_string()));
    assert!(jvm_args.contains(&"dummy.jar".to_string()));
}
