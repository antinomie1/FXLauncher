//! Mod loader detection and environment inspection for Minecraft instances.

use crate::game::version::{VersionError, VersionInfo, load_version_info};
use crate::net::mods::ModLoader;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Detected mod loader and game version information for an instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceLoaderInfo {
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub game_version: Option<String>,
    pub has_optifine: bool,
}

/// Detects the mod loader and environment directly from a loaded `VersionInfo`.
pub fn detect_loader_from_info(info: &VersionInfo) -> InstanceLoaderInfo {
    let mut loader = ModLoader::Vanilla;
    let mut loader_version = None;
    let mut game_version = None;
    let mut has_optifine = false;

    // 1. Inspect mainClass
    let main_class = &info.main_class;
    if main_class.contains("net.fabricmc.loader") {
        loader = ModLoader::Fabric;
    } else if main_class.contains("org.quiltmc.loader") {
        loader = ModLoader::Quilt;
    } else if main_class.contains("net.neoforged") {
        loader = ModLoader::NeoForge;
    } else if main_class.contains("net.minecraftforge") || main_class.contains("cpw.mods.fml") {
        loader = ModLoader::Forge;
    }

    // 2. Inspect libraries for specific maven coordinates
    for lib in &info.libraries {
        let name = &lib.name;

        if name.starts_with("net.fabricmc:fabric-loader:") {
            loader = ModLoader::Fabric;
            loader_version = extract_library_version(name);
        } else if name.starts_with("org.quiltmc:quilt-loader:") {
            loader = ModLoader::Quilt;
            loader_version = extract_library_version(name);
        } else if name.starts_with("net.neoforged:neoforge:") {
            loader = ModLoader::NeoForge;
            loader_version = extract_library_version(name);
        } else if name.starts_with("net.minecraftforge:forge:") {
            loader = ModLoader::Forge;
            loader_version = extract_library_version(name);
        } else if name.starts_with("optifine:OptiFine:") {
            has_optifine = true;
        }

        // Try to identify base minecraft version from library versions
        if name.starts_with("com.mojang:minecraft:") || name.starts_with("net.minecraft:client:") {
            if game_version.is_none() {
                game_version = extract_library_version(name);
            }
        }
    }

    // Fallback: if game_version was not in libraries, check assets / id
    if game_version.is_none() {
        game_version = Some(info.id.clone());
    }

    InstanceLoaderInfo {
        loader,
        loader_version,
        game_version,
        has_optifine,
    }
}

/// Detects the mod loader of an installed instance by reading its version JSON.
pub fn detect_instance_loader(
    game_dir: &Path,
    version_id: &str,
) -> Result<InstanceLoaderInfo, VersionError> {
    let info = load_version_info(game_dir, version_id)?;
    Ok(detect_loader_from_info(&info))
}

fn extract_library_version(maven_name: &str) -> Option<String> {
    let parts: Vec<&str> = maven_name.split(':').collect();
    if parts.len() >= 3 {
        Some(parts[2].to_string())
    } else {
        None
    }
}
