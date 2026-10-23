//! Intelligent error, crash, and mod failure analysis.

use crate::diag::crash::parse_crash_report;
use crate::env::paths::version_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrashSeverity {
    Fatal,
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrashCause {
    OutOfMemory {
        required_mb: Option<u32>,
    },
    JavaVersionMismatch {
        current_major: Option<u32>,
        required_major: Option<u32>,
    },
    MissingModDependency {
        mod_name: String,
        required_dependency: String,
        version_req: Option<String>,
    },
    ModConflict {
        mod_a: String,
        mod_b: String,
        reason: String,
    },
    IncompatibleMod {
        mod_name: String,
        error_detail: String,
    },
    GraphicsDriverError {
        detail: String,
    },
    Unknown {
        summary: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub severity: CrashSeverity,
    pub cause: CrashCause,
    pub summary: String,
    pub suggestion: String,
    pub suspected_mods: Vec<String>,
    pub stacktrace: Option<String>,
}

/// Converts Java class file format version number to Java major version.
/// e.g. 65.0 -> Java 21, 61.0 -> Java 17, 52.0 -> Java 8.
pub fn class_file_version_to_java(version: u32) -> u32 {
    if version >= 45 {
        version - 44
    } else {
        version
    }
}

/// Analyzes arbitrary crash text (crash report or game console log).
pub fn analyze_crash_text(text: &str) -> AnalysisReport {
    let parsed_report = parse_crash_report(text);
    let mut suspected = parsed_report.suspected_mods.clone();

    // 1. Out Of Memory Check
    if text.contains("java.lang.OutOfMemoryError")
        || text.contains("Java heap space")
        || text.contains("Metaspace")
    {
        return AnalysisReport {
            severity: CrashSeverity::Fatal,
            cause: CrashCause::OutOfMemory { required_mb: None },
            summary: "游戏内存不足 (OutOfMemoryError)".to_string(),
            suggestion: "请在启动器设置中调大最大分配内存（例如增加至 4096 MB 或更高），并关闭不必要的后台程序。".to_string(),
            suspected_mods: suspected,
            stacktrace: if parsed_report.stacktrace.is_empty() { None } else { Some(parsed_report.stacktrace) },
        };
    }

    // 2. Java Version Incompatibility (UnsupportedClassVersionError)
    if text.contains("UnsupportedClassVersionError") {
        let (current, required) = parse_class_version_numbers(text);
        let req_text = required.map(|r| format!("Java {r}")).unwrap_or_else(|| "更高版本的 Java".to_string());
        return AnalysisReport {
            severity: CrashSeverity::Fatal,
            cause: CrashCause::JavaVersionMismatch {
                current_major: current,
                required_major: required,
            },
            summary: format!("Java 运行时版本过低，需要 {req_text}"),
            suggestion: format!("当前使用的 Java 无法运行该版本或模组，请在设置中将 Java 运行时切换为 {req_text} 或更高版本。"),
            suspected_mods: suspected,
            stacktrace: if parsed_report.stacktrace.is_empty() { None } else { Some(parsed_report.stacktrace) },
        };
    }

    // 3. Fabric Missing Dependencies
    if (text.contains("net.fabricmc.loader.impl.FormattedException")
        || text.contains("incompatible with the game or each other")
        || text.contains("requires version")
        || text.contains("that is not installed"))
        && let Some((mod_name, dep_name, ver_req)) = parse_fabric_missing_dep(text) {
            if !suspected.contains(&mod_name) {
                suspected.push(mod_name.clone());
            }
            let req_str = ver_req.as_deref().unwrap_or("任何兼容版本").to_string();
            return AnalysisReport {
                severity: CrashSeverity::Fatal,
                cause: CrashCause::MissingModDependency {
                    mod_name: mod_name.clone(),
                    required_dependency: dep_name.clone(),
                    version_req: ver_req,
                },
                summary: format!("模组 '{mod_name}' 缺少必要前置依赖: '{dep_name}'"),
                suggestion: format!("模组 '{mod_name}' 需要安装前置 '{dep_name}' ({req_str})，请前往模组平台下载安装该前置。"),
                suspected_mods: suspected,
                stacktrace: if parsed_report.stacktrace.is_empty() { None } else { Some(parsed_report.stacktrace) },
            };
        }

    // 4. Forge Missing / Duplicate Mod Dependencies
    if text.contains("ModLoadingException")
        || text.contains("Missing or unsupported mandatory dependencies")
        || text.contains("DuplicateModsFoundException")
    {
        if text.contains("DuplicateModsFoundException") {
            let mod_name = extract_duplicate_mod(text).unwrap_or_else(|| "重复模组".to_string());
            if !suspected.contains(&mod_name) {
                suspected.push(mod_name.clone());
            }
            return AnalysisReport {
                severity: CrashSeverity::Fatal,
                cause: CrashCause::ModConflict {
                    mod_a: mod_name.clone(),
                    mod_b: mod_name.clone(),
                    reason: "检测到重复安装的模组文件".to_string(),
                },
                summary: format!("模组重复安装冲突: '{mod_name}'"),
                suggestion: format!("检测到版本专属 mods/ 目录中存在重复的 '{mod_name}' 文件，请删除其中旧版本文件后重试。"),
                suspected_mods: suspected,
                stacktrace: if parsed_report.stacktrace.is_empty() { None } else { Some(parsed_report.stacktrace) },
            };
        }

        if let Some((mod_name, dep)) = parse_forge_missing_dep(text) {
            if !suspected.contains(&mod_name) {
                suspected.push(mod_name.clone());
            }
            return AnalysisReport {
                severity: CrashSeverity::Fatal,
                cause: CrashCause::MissingModDependency {
                    mod_name: mod_name.clone(),
                    required_dependency: dep.clone(),
                    version_req: None,
                },
                summary: format!("Forge 模组 '{mod_name}' 缺少依赖: '{dep}'"),
                suggestion: format!("请在 mods/ 目录中添加缺失的依赖模组 '{dep}'。"),
                suspected_mods: suspected,
                stacktrace: if parsed_report.stacktrace.is_empty() { None } else { Some(parsed_report.stacktrace) },
            };
        }
    }

    // 5. Graphics / OpenGL Driver Crash
    if text.contains("ig9icd64.dll")
        || text.contains("ig4icd64.dll")
        || text.contains("nvoglv64.dll")
        || text.contains("atio6axx.dll")
        || text.contains("org.lwjgl.opengl.OpenGLException")
        || text.contains("GLFW error 65542")
    {
        let driver_detail = if text.contains("nvoglv64.dll") {
            "NVIDIA 显卡驱动"
        } else if text.contains("ig9icd") || text.contains("ig4icd") {
            "Intel 核显驱动"
        } else if text.contains("atio6axx") {
            "AMD 显卡驱动"
        } else {
            "OpenGL 显卡驱动"
        };

        return AnalysisReport {
            severity: CrashSeverity::Fatal,
            cause: CrashCause::GraphicsDriverError {
                detail: driver_detail.to_string(),
            },
            summary: format!("{driver_detail}崩溃或不支持当前图形特性"),
            suggestion: "建议更新显卡驱动程序至最新版本，并在笔记本电脑上确认已指定高性能独立显卡运行游戏。".to_string(),
            suspected_mods: suspected,
            stacktrace: if parsed_report.stacktrace.is_empty() { None } else { Some(parsed_report.stacktrace) },
        };
    }

    // 6. Mixin / Incompatible Mod Exception
    if text.contains("org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError")
        || text.contains("org.spongepowered.asm.mixin.injection.throwables.InjectionError")
    {
        let target_mod = extract_mod_from_mixin(text).unwrap_or_else(|| "未知模组".to_string());
        if !suspected.contains(&target_mod) && target_mod != "未知模组" {
            suspected.push(target_mod.clone());
        }
        return AnalysisReport {
            severity: CrashSeverity::Fatal,
            cause: CrashCause::IncompatibleMod {
                mod_name: target_mod.clone(),
                error_detail: "Mixin 字节码注入失败".to_string(),
            },
            summary: format!("模组 '{target_mod}' 代码注入冲突 (Mixin Error)"),
            suggestion: format!("模组 '{target_mod}' 与当前游戏版本或其它模组存在兼容性冲突，建议暂时移出 mods/ 目录排查。"),
            suspected_mods: suspected,
            stacktrace: if parsed_report.stacktrace.is_empty() { None } else { Some(parsed_report.stacktrace) },
        };
    }

    // Default Unknown Crash
    let summary = if !parsed_report.description.is_empty() {
        parsed_report.description.clone()
    } else {
        "游戏异常终止".to_string()
    };

    AnalysisReport {
        severity: CrashSeverity::Error,
        cause: CrashCause::Unknown {
            summary: summary.clone(),
        },
        summary: format!("发生未识别异常: {summary}"),
        suggestion: "建议检查游戏控制台日志最后输出的报错堆栈，排查最近安装或更新的模组文件。".to_string(),
        suspected_mods: suspected,
        stacktrace: if parsed_report.stacktrace.is_empty() { None } else { Some(parsed_report.stacktrace) },
    }
}

/// Helper to parse `class file version XX.0 ... up to YY.0`.
fn parse_class_version_numbers(text: &str) -> (Option<u32>, Option<u32>) {
    let mut req = None;
    let mut cur = None;

    if let Some(pos) = text.find("class file version ") {
        let after = &text[pos + "class file version ".len()..];
        if let Some(dot) = after.find('.')
            && let Ok(ver) = after[..dot].parse::<u32>() {
                req = Some(class_file_version_to_java(ver));
            }
    }

    if let Some(pos) = text.find("only recognizes class file versions up to ") {
        let after = &text[pos + "only recognizes class file versions up to ".len()..];
        if let Some(dot) = after.find('.')
            && let Ok(ver) = after[..dot].parse::<u32>() {
                cur = Some(class_file_version_to_java(ver));
            }
    }

    (cur, req)
}

/// Helper to extract missing dependency details from Fabric formatted logs.
fn parse_fabric_missing_dep(text: &str) -> Option<(String, String, Option<String>)> {
    for line in text.lines() {
        let trimmed = line.trim();
        let clean = trimmed.trim_start_matches(['-', '•', '*', '\t', ' ']).trim();

        // Pattern 1: Mod 'foo' (foo) requires version >=1.0 of 'bar' (bar)
        if clean.starts_with("Mod '")
            && let Some(mod_end) = clean[5..].find('\'') {
                let mod_name = clean[5..5 + mod_end].to_string();
                if let Some(req_pos) = clean.find("requires") {
                    let after_req = &clean[req_pos + "requires".len()..];
                    if let Some(dep_start) = after_req.rfind('\'') {
                        let before_dep = &after_req[..dep_start];
                        if let Some(dep_first) = before_dep.rfind('\'') {
                            let dep_name = before_dep[dep_first + 1..].to_string();
                            let ver_req = if before_dep.contains("version") {
                                Some(before_dep.trim().to_string())
                            } else {
                                None
                            };
                            return Some((mod_name, dep_name, ver_req));
                        }
                    }
                }
            }

        // Pattern 2: A mod requires a version of 'foo' that is not installed
        if trimmed.contains("requires a version of '") {
            let start = trimmed.find("requires a version of '").unwrap() + "requires a version of '".len();
            if let Some(end) = trimmed[start..].find('\'') {
                let dep = trimmed[start..start + end].to_string();
                return Some(("已安装模组".to_string(), dep, None));
            }
        }
    }
    None
}

/// Helper to extract missing dependency details from Forge logs.
fn parse_forge_missing_dep(text: &str) -> Option<(String, String)> {
    for line in text.lines() {
        if line.contains("Missing or unsupported mandatory dependencies:") {
            let after = line.split("dependencies:").nth(1).unwrap_or("").trim();
            if !after.is_empty() {
                return Some(("Forge模组".to_string(), after.to_string()));
            }
        }
    }
    None
}

/// Helper to extract duplicated mod name.
fn extract_duplicate_mod(text: &str) -> Option<String> {
    for line in text.lines() {
        if line.contains("Duplicate mod ID was found:") || line.contains("duplicate mod:") {
            let after = line.split(':').next_back().unwrap_or("").trim();
            if !after.is_empty() {
                return Some(after.to_string());
            }
        }
    }
    None
}

/// Helper to extract mod name from mixin error lines.
fn extract_mod_from_mixin(text: &str) -> Option<String> {
    for line in text.lines() {
        if (line.contains("in classmap") || line.contains("in target") || line.contains("mixin:"))
            && let Some(pos) = line.find("mixin:") {
                let after = &line[pos + 6..];
                if let Some(end) = after.find('.') {
                    let m = after[..end].trim();
                    if !m.is_empty() {
                        return Some(m.to_string());
                    }
                }
            }
    }
    None
}

/// Scans the version directory and root directory for the most recent crash report file.
pub fn find_latest_crash_report(game_dir: &Path, version_id: Option<&str>) -> Option<PathBuf> {
    let mut candidate_dirs = Vec::new();

    if let Some(ver) = version_id {
        candidate_dirs.push(version_dir(game_dir, ver).join("crash-reports"));
    }
    candidate_dirs.push(game_dir.join("crash-reports"));

    let mut newest_file = None;
    let mut newest_time = std::time::SystemTime::UNIX_EPOCH;

    for dir in candidate_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("txt")
                    && let Ok(meta) = entry.metadata()
                        && let Ok(modified) = meta.modified()
                            && modified > newest_time {
                                newest_time = modified;
                                newest_file = Some(path);
                            }
            }
        }
    }

    newest_file
}

/// Locates and analyzes the most recent crash report or latest.log in the specified game directory.
pub fn analyze_latest_crash(
    game_dir: &Path,
    version_id: Option<&str>,
) -> std::io::Result<Option<AnalysisReport>> {
    // 1. Try finding crash report first
    if let Some(crash_file) = find_latest_crash_report(game_dir, version_id) {
        let content = fs::read_to_string(crash_file)?;
        return Ok(Some(analyze_crash_text(&content)));
    }

    // 2. Fallback to latest.log in version dir or game dir
    let mut log_candidates = Vec::new();
    if let Some(ver) = version_id {
        log_candidates.push(version_dir(game_dir, ver).join("logs").join("latest.log"));
    }
    log_candidates.push(game_dir.join("logs").join("latest.log"));

    for log_path in log_candidates {
        if log_path.is_file() {
            let content = fs::read_to_string(log_path)?;
            return Ok(Some(analyze_crash_text(&content)));
        }
    }

    Ok(None)
}
