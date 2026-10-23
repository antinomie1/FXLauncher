//! Extraction of platform native libraries (LWJGL, OpenAL, ...) into a version's `natives/` directory.

use crate::env::paths::{arch_name, ensure_dir, libraries_dir, natives_dir};
use crate::game::version::{Library, VersionInfo, maven_to_path};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

const NATIVE_EXTENSIONS: &[&str] = &["so", "dll", "dylib", "jnilib"];

/// Whether a modern natives classifier (`natives-windows-arm64`) targets the running CPU.
pub fn classifier_matches_arch(classifier: &str) -> bool {
    let suffix_arch = if classifier.ends_with("-arm64") || classifier.ends_with("-aarch_64") {
        "aarch64"
    } else if classifier.ends_with("-x86") {
        "x86"
    } else if classifier.ends_with("-arm32") {
        "arm"
    } else {
        "x86_64"
    };
    suffix_arch == arch_name() || (suffix_arch == "x86_64" && arch_name() == "unknown")
}

/// Native jars (with their exclude prefixes) that apply to the current platform.
pub fn native_jars(game_dir: &Path, info: &VersionInfo) -> Vec<(PathBuf, Vec<String>)> {
    let lib_dir = libraries_dir(game_dir);
    let mut jars = Vec::new();
    for lib in info.libraries.iter().filter(|l| l.applies()) {
        let exclude = lib.extract.as_ref().map(|e| e.exclude.clone()).unwrap_or_default();
        if let Some(artifact) = lib.native_artifact() {
            if let Some(rel) = artifact.path.as_deref().map(PathBuf::from).or_else(|| legacy_native_path(lib)) {
                jars.push((lib_dir.join(rel), exclude));
            }
        } else if lib.is_native_jar() {
            let classifier = lib.name.split(':').nth(3).unwrap_or_default();
            if classifier_matches_arch(classifier)
                && let Some(rel) = lib.artifact_path()
            {
                jars.push((lib_dir.join(rel), exclude));
            }
        }
    }
    jars
}

fn legacy_native_path(lib: &Library) -> Option<PathBuf> {
    let classifier = lib.natives.as_ref()?.get(crate::env::paths::os_name())?;
    let bits = if cfg!(target_pointer_width = "64") { "64" } else { "32" };
    maven_to_path(&format!("{}:{}", lib.name, classifier.replace("${arch}", bits)))
}

/// Extracts native binaries from every applicable natives jar into `versions/{id}/natives`,
/// flattening directories. Missing jars are skipped. Returns the number of files written.
pub fn extract_natives(game_dir: &Path, info: &VersionInfo) -> io::Result<usize> {
    let out_dir = natives_dir(game_dir, &info.id);
    ensure_dir(&out_dir)?;
    let mut written = 0;
    for (jar, exclude) in native_jars(game_dir, info) {
        if jar.is_file() {
            written += extract_jar(&jar, &out_dir, &exclude)?;
        }
    }
    Ok(written)
}

fn extract_jar(jar: &Path, out_dir: &Path, exclude: &[String]) -> io::Result<usize> {
    let mut archive = zip::ZipArchive::new(File::open(jar)?).map_err(io::Error::other)?;
    let mut written = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(io::Error::other)?;
        let name = entry.name().map_err(io::Error::other)?.to_string();
        if entry.is_dir() || name.starts_with("META-INF/") || exclude.iter().any(|e| name.starts_with(e.as_str())) {
            continue;
        }
        let file_name = match Path::new(&name).file_name() {
            Some(f) => f.to_owned(),
            None => continue,
        };
        let is_native = Path::new(&file_name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| NATIVE_EXTENSIONS.contains(&e));
        if !is_native {
            continue;
        }
        let dest = out_dir.join(&file_name);
        if fs::metadata(&dest).is_ok_and(|m| m.len() == entry.size()) {
            continue;
        }
        io::copy(&mut entry, &mut File::create(&dest)?)?;
        written += 1;
    }
    Ok(written)
}
