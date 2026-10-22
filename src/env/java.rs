//! Java runtime discovery, version parsing, and compatibility checks.

use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JavaError {
    #[error("Failed to execute Java binary at '{0}': {1}")]
    Execution(PathBuf, std::io::Error),
    #[error("Could not parse Java version output: '{0}'")]
    Parse(String),
}

/// Information about a detected Java runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInfo {
    pub path: PathBuf,
    pub version_string: String,
    pub major_version: u32,
    pub is_64_bit: bool,
}

/// Parses the output of `java -version` (typically written to stderr).
pub fn parse_java_version(output: &str) -> Option<(String, u32, bool)> {
    let mut version_str = None;
    let mut is_64_bit = output.contains("64-Bit") || output.contains("x86_64") || output.contains("amd64");

    for line in output.lines() {
        if line.contains("version \"") {
            if let Some(start) = line.find('"') {
                if let Some(end) = line[start + 1..].find('"') {
                    let v = &line[start + 1..start + 1 + end];
                    version_str = Some(v.to_string());
                    break;
                }
            }
        }
    }

    let version_str = version_str?;
    let major = extract_major_version(&version_str)?;

    if !is_64_bit && (output.contains("32-Bit") || output.contains("i386") || output.contains("x86")) {
        is_64_bit = false;
    } else if !is_64_bit {
        is_64_bit = cfg!(target_pointer_width = "64");
    }

    Some((version_str, major, is_64_bit))
}

/// Extracts the major version number from a Java version string (e.g. "1.8.0_312" -> 8, "21.0.2" -> 21).
pub fn extract_major_version(version: &str) -> Option<u32> {
    if version.starts_with("1.") {
        let mut parts = version.split('.');
        parts.next()?; // skip "1"
        let second = parts.next()?;
        second.split('_').next()?.parse::<u32>().ok()
    } else {
        let first = version.split('.').next()?;
        first.split('-').next()?.parse::<u32>().ok()
    }
}

/// Determines the minimum recommended Java major version for a given Minecraft release.
pub fn required_java_major(mc_version: &str) -> u32 {
    let clean = mc_version.trim_start_matches('v');
    let mut parts = clean.split('.');
    let _major = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
    let minor = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);

    if minor >= 21 || (minor == 20 && patch >= 5) {
        21
    } else if minor >= 18 {
        17
    } else if minor == 17 {
        16
    } else {
        8
    }
}

/// Probes a specific Java executable path by running `java -version`.
pub fn probe_java_binary(path: &Path) -> Result<JavaInfo, JavaError> {
    let output = Command::new(path)
        .arg("-version")
        .output()
        .map_err(|e| JavaError::Execution(path.to_path_buf(), e))?;

    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let (version_string, major_version, is_64_bit) =
        parse_java_version(&text).ok_or_else(|| JavaError::Parse(text.trim().to_string()))?;

    Ok(JavaInfo {
        path: path.to_path_buf(),
        version_string,
        major_version,
        is_64_bit,
    })
}

/// Scans standard platform locations for Java runtime installations.
pub fn find_system_java_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Check JAVA_HOME
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let bin = PathBuf::from(java_home).join("bin");
        #[cfg(target_os = "windows")]
        let exe = bin.join("java.exe");
        #[cfg(not(target_os = "windows"))]
        let exe = bin.join("java");

        if exe.is_file() {
            candidates.push(exe);
        }
    }

    #[cfg(target_os = "windows")]
    {
        for program_files in &["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(dir) = std::env::var_os(program_files) {
                let java_base = PathBuf::from(dir).join("Java");
                if let Ok(entries) = std::fs::read_dir(java_base) {
                    for entry in entries.flatten() {
                        let candidate = entry.path().join("bin").join("java.exe");
                        if candidate.is_file() {
                            candidates.push(candidate);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let jvm_dir = Path::new("/Library/Java/JavaVirtualMachines");
        if let Ok(entries) = std::fs::read_dir(jvm_dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("Contents").join("Home").join("bin").join("java");
                if candidate.is_file() {
                    candidates.push(candidate);
                }
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let common = [
            "/usr/bin/java",
            "/usr/local/bin/java",
            "/etc/alternatives/java",
        ];
        for path_str in common {
            let path = PathBuf::from(path_str);
            if path.is_file() {
                candidates.push(path);
            }
        }

        let jvm_dir = Path::new("/usr/lib/jvm");
        if let Ok(entries) = std::fs::read_dir(jvm_dir) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("bin").join("java");
                if candidate.is_file() {
                    candidates.push(candidate);
                }
            }
        }
    }

    // PATH fallback
    candidates.push(PathBuf::from(if cfg!(target_os = "windows") { "java.exe" } else { "java" }));
    candidates.dedup();
    candidates
}
