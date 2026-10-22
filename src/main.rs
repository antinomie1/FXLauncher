//! Main executable entry point for FXLauncher.

use aegle::prelude::*;
use fxlauncher::ui::setup_launcher_ui;
use std::env;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("FXLauncher — A lightweight cross-platform Minecraft launcher");
        println!();
        println!("Usage: fxlauncher [OPTIONS]");
        println!();
        println!("Options:");
        println!("  -h, --help       Print help information");
        println!("  -v, --version    Print launcher version");
        println!("  --headless       Run in CLI headless mode (verifies core setup without GUI)");
        return Ok(());
    }

    if args.iter().any(|arg| arg == "--version" || arg == "-v") {
        println!("FXLauncher v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if args.iter().any(|arg| arg == "--headless") {
        println!("FXLauncher v{} (Headless Mode)", env!("CARGO_PKG_VERSION"));
        let default_dir = fxlauncher::paths::default_minecraft_dir();
        println!("Default Minecraft directory: {}", default_dir.display());
        let javas = fxlauncher::java::find_system_java_candidates();
        println!("Found {} Java candidate(s):", javas.len());
        for j in javas {
            if let Ok(info) = fxlauncher::java::probe_java_binary(&j) {
                println!(
                    "  - {} (Java {}, {} bit)",
                    info.path.display(),
                    info.major_version,
                    if info.is_64_bit { 64 } else { 32 }
                );
            }
        }
        return Ok(());
    }

    // Launch GUI with Aegle
    let app = App::new()?;
    setup_launcher_ui(&app)?;
    app.run()
}
