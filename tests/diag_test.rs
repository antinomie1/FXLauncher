use fxlauncher::diag::{
    CrashCause, CrashSeverity, LogLevel, analyze_crash_text, class_file_version_to_java,
    extract_error_logs, parse_crash_report, parse_log_line,
};

#[test]
fn test_parse_log_line() {
    let line = "[12:34:56] [main/INFO]: Setting user: Steve";
    let entry = parse_log_line(line).unwrap();
    assert_eq!(entry.timestamp.as_deref(), Some("12:34:56"));
    assert_eq!(entry.thread.as_deref(), Some("main"));
    assert_eq!(entry.level, LogLevel::Info);
    assert_eq!(entry.message, "Setting user: Steve");

    let err_line = "[12:35:00] [Render thread/ERROR]: Failed to load texture";
    let err_entry = parse_log_line(err_line).unwrap();
    assert_eq!(err_entry.thread.as_deref(), Some("Render thread"));
    assert_eq!(err_entry.level, LogLevel::Error);
    assert_eq!(err_entry.message, "Failed to load texture");

    assert!(parse_log_line("Non standard line without brackets").is_none());
}

#[test]
fn test_extract_error_logs() {
    let log_content = r#"
[10:00:00] [main/INFO]: Starting Minecraft
[10:00:01] [main/WARN]: Deprecated feature used
[10:00:02] [Render thread/ERROR]: OpenGL error 1282
[10:00:03] [main/FATAL]: Crashed unrecoverably
[10:00:04] [main/INFO]: Stopping
"#;
    let errors = extract_error_logs(log_content);
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0].level, LogLevel::Error);
    assert_eq!(errors[0].message, "OpenGL error 1282");
    assert_eq!(errors[1].level, LogLevel::Fatal);
    assert_eq!(errors[1].message, "Crashed unrecoverably");
}

#[test]
fn test_class_file_version_to_java() {
    assert_eq!(class_file_version_to_java(52), 8);
    assert_eq!(class_file_version_to_java(61), 17);
    assert_eq!(class_file_version_to_java(65), 21);
}

#[test]
fn test_parse_standard_crash_report() {
    let report_text = r#"
---- Minecraft Crash Report ----
// You're mean.

Time: 2026-10-23 02:00:00
Description: Initializing game
Suspected Mods: sodium, iris

java.lang.NullPointerException: Cannot invoke method
	at net.minecraft.client.main.Main.main(Main.java:100)

-- System Details --
Details:
	Minecraft Version: 1.21.4
	Operating System: Linux (amd64) version 6.6.0
	Java Version: 21.0.2, Oracle Corporation
"#;

    let parsed = parse_crash_report(report_text);
    assert_eq!(parsed.description, "Initializing game");
    assert_eq!(parsed.time.as_deref(), Some("2026-10-23 02:00:00"));
    assert_eq!(parsed.suspected_mods, vec!["sodium", "iris"]);
    assert_eq!(
        parsed.system_details.get("Minecraft Version").map(|s| s.as_str()),
        Some("1.21.4")
    );
    assert_eq!(
        parsed.system_details.get("Operating System").map(|s| s.as_str()),
        Some("Linux (amd64) version 6.6.0")
    );
    assert!(parsed.stacktrace.contains("java.lang.NullPointerException"));
}

#[test]
fn test_analyze_out_of_memory() {
    let log = "Exception in thread \"main\" java.lang.OutOfMemoryError: Java heap space";
    let analysis = analyze_crash_text(log);

    assert_eq!(analysis.severity, CrashSeverity::Fatal);
    match analysis.cause {
        CrashCause::OutOfMemory { .. } => {}
        other => panic!("Expected OutOfMemory, got {:?}", other),
    }
    assert!(analysis.summary.contains("内存不足"));
    assert!(analysis.suggestion.contains("调大最大分配内存"));
}

#[test]
fn test_analyze_java_version_mismatch() {
    let log = r#"
java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 61.0
"#;
    let analysis = analyze_crash_text(log);

    assert_eq!(analysis.severity, CrashSeverity::Fatal);
    match analysis.cause {
        CrashCause::JavaVersionMismatch {
            current_major,
            required_major,
        } => {
            assert_eq!(current_major, Some(17));
            assert_eq!(required_major, Some(21));
        }
        other => panic!("Expected JavaVersionMismatch, got {:?}", other),
    }
    assert!(analysis.summary.contains("Java 21"));
    assert!(analysis.suggestion.contains("Java 21"));
}

#[test]
fn test_analyze_fabric_missing_dependency() {
    let log = r#"
net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!
A potential solution has been determined, this may resolve your issue:
	 - Mod 'sodium' (sodium) requires version >=0.92.0 of 'fabric-api' (fabric-api), which is missing!
"#;
    let analysis = analyze_crash_text(log);

    assert_eq!(analysis.severity, CrashSeverity::Fatal);
    match analysis.cause {
        CrashCause::MissingModDependency {
            mod_name,
            required_dependency,
            ..
        } => {
            assert_eq!(mod_name, "sodium");
            assert_eq!(required_dependency, "fabric-api");
        }
        other => panic!("Expected MissingModDependency, got {:?}", other),
    }
    assert!(analysis.suspected_mods.contains(&"sodium".to_string()));
    assert!(analysis.summary.contains("fabric-api"));
    assert!(analysis.suggestion.contains("fabric-api"));
}

#[test]
fn test_analyze_forge_duplicate_mod() {
    let log = r#"
net.minecraftforge.fml.loading.EarlyLoadingException: DuplicateModsFoundException: Duplicate mod ID was found: jei
"#;
    let analysis = analyze_crash_text(log);

    assert_eq!(analysis.severity, CrashSeverity::Fatal);
    match analysis.cause {
        CrashCause::ModConflict { mod_a, .. } => {
            assert_eq!(mod_a, "jei");
        }
        other => panic!("Expected ModConflict, got {:?}", other),
    }
    assert!(analysis.summary.contains("jei"));
    assert!(analysis.suggestion.contains("删除其中旧版本文件"));
}

#[test]
fn test_analyze_graphics_driver_crash() {
    let log = r#"
# A fatal error has been detected by the Java Runtime Environment:
#  EXCEPTION_ACCESS_VIOLATION (0xc0000005) at pc=0x00007ff812345678, pid=1234
# Problematic frame:
# C  [nvoglv64.dll+0x9ab123]
"#;
    let analysis = analyze_crash_text(log);

    assert_eq!(analysis.severity, CrashSeverity::Fatal);
    match analysis.cause {
        CrashCause::GraphicsDriverError { detail } => {
            assert!(detail.contains("NVIDIA"));
        }
        other => panic!("Expected GraphicsDriverError, got {:?}", other),
    }
    assert!(analysis.suggestion.contains("更新显卡驱动程序"));
}

#[test]
fn test_analyze_mixin_conflict() {
    let log = r#"
org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError: An unexpected critical error was encountered in mixin: sodium.mixins.core.json:MixinWorldRenderer
"#;
    let analysis = analyze_crash_text(log);

    assert_eq!(analysis.severity, CrashSeverity::Fatal);
    match analysis.cause {
        CrashCause::IncompatibleMod { mod_name, .. } => {
            assert_eq!(mod_name, "sodium");
        }
        other => panic!("Expected IncompatibleMod, got {:?}", other),
    }
    assert!(analysis.suspected_mods.contains(&"sodium".to_string()));
    assert!(analysis.summary.contains("sodium"));
}
