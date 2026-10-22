//! Cross-platform path resolution and operating system utilities.

use std::path::{Path, PathBuf};

/// Returns the default `.minecraft` directory for the current operating system.
pub fn default_minecraft_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(".minecraft");
        }
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(userprofile)
                .join("AppData")
                .join("Roaming")
                .join(".minecraft");
        }
        PathBuf::from(".minecraft")
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("minecraft");
        }
        PathBuf::from(".minecraft")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".minecraft");
        }
        PathBuf::from(".minecraft")
    }
}

/// Returns the `versions` directory inside the given game directory.
pub fn versions_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("versions")
}

/// Returns the `libraries` directory inside the given game directory.
pub fn libraries_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("libraries")
}

/// Returns the `assets` directory inside the given game directory.
pub fn assets_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("assets")
}

/// Returns the directory where extracted native binaries reside for a version.
pub fn natives_dir(game_dir: &Path, version_id: &str) -> PathBuf {
    versions_dir(game_dir).join(version_id).join("natives")
}

/// Classpath separator character: `;` on Windows, `:` on Unix/macOS.
#[inline]
pub fn classpath_separator() -> char {
    if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    }
}

/// Returns the Minecraft OS identifier (`windows`, `osx`, or `linux`).
#[inline]
pub fn os_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "osx"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "linux"
    }
}

/// Returns the CPU architecture string for rule matching.
#[inline]
pub fn arch_name() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "x86")]
    {
        "x86"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "x86",
        target_arch = "aarch64"
    )))]
    {
        "unknown"
    }
}
