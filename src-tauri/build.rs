//! Build script: reads which éœ§å®¿ mod to embed, and compiles the icons in.

use std::path::Path;

/// The éœ§å®¿ mod is built into the launcher. `scripts/fetch-mod.ps1` puts it in `resources/mods`:
/// the jar of one of the mod's GitHub Releases as `mistinn.jar`, and its version in `mistinn.version`.
fn main() {
    let dir = Path::new("resources/mods");
    println!("cargo:rerun-if-changed=resources/mods/mistinn.jar");
    println!("cargo:rerun-if-changed=resources/mods/mistinn.version");
    // the window and installer icons are compiled into the binary: rebuild when they change
    println!("cargo:rerun-if-changed=icons");
    let version = std::fs::read_to_string(dir.join("mistinn.version"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let parts: Vec<&str> = version.split('.').collect();
    let valid = parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    assert!(
        valid && dir.join("mistinn.jar").is_file(),
        "the éœ§å®¿ mod is missing: run scripts/fetch-mod.ps1 first (it writes resources/mods/mistinn.jar and mistinn.version)"
    );
    println!("cargo:rustc-env=MISTINN_MOD_VERSION={version}");
    tauri_build::build();
}
