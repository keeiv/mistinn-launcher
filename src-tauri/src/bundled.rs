//! The 霧宿 mod itself, built into the launcher binary.
//!
//! The portable exe has no resource folder beside it, so the jar lives inside the binary and is
//! written out under the launcher's own folder before each launch.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// The version of the bundled mod, from `resources/mods/mistinn.version` (see build.rs).
pub const MISTINN_MOD_VERSION: &str = env!("MISTINN_MOD_VERSION");
/// File name the bundled mod is written out under.
pub const MISTINN_JAR_NAME: &str = concat!("mistinn-", env!("MISTINN_MOD_VERSION"), ".jar");
const MISTINN_JAR: &[u8] = include_bytes!("../resources/mods/mistinn.jar");

/// Writes the bundled mod into `dir` (only when missing or different), drops the jars of earlier
/// versions left there, and returns its path.
pub fn extract(dir: &Path) -> Result<PathBuf> {
    let path = write_if_changed(dir, MISTINN_JAR_NAME, MISTINN_JAR)?;
    remove_other_versions(dir, MISTINN_JAR_NAME);
    Ok(path)
}

/// Removes every `mistinn-*.jar` in `dir` but `keep`: an update leaves no stale copy behind.
fn remove_other_versions(dir: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name != keep && name.starts_with("mistinn-") && name.ends_with(".jar") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn write_if_changed(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(name);
    let same = std::fs::read(&path).is_ok_and(|existing| existing == bytes);
    if !same {
        // write beside it and rename, so a half-written jar never reaches the mods folder
        let partial = dir.join(format!("{name}.part"));
        std::fs::write(&partial, bytes)?;
        std::fs::rename(&partial, &path)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_jar_is_a_zip() {
        assert!(MISTINN_JAR.len() > 1024);
        assert_eq!(&MISTINN_JAR[..2], b"PK");
    }

    #[test]
    fn embedded_name_carries_the_mod_version() {
        assert_eq!(MISTINN_JAR_NAME, format!("mistinn-{MISTINN_MOD_VERSION}.jar"));
        assert_eq!(MISTINN_MOD_VERSION.split('.').count(), 3);
    }

    #[test]
    fn an_update_removes_earlier_versions_only() {
        let dir = std::env::temp_dir().join(format!("mistinn-bundled-versions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["mistinn-0.1.0.jar", "mistinn-0.1.1.jar", "other-mod.jar", "mistinn-notes.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        remove_other_versions(&dir, "mistinn-0.1.1.jar");
        assert!(!dir.join("mistinn-0.1.0.jar").exists());
        assert!(dir.join("mistinn-0.1.1.jar").exists());
        assert!(dir.join("other-mod.jar").exists());
        assert!(dir.join("mistinn-notes.txt").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn writes_and_replaces_stale_copies() {
        let dir = std::env::temp_dir().join(format!("mistinn-bundled-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let path = write_if_changed(&dir, "a.jar", b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");

        std::fs::write(&path, b"old").unwrap();
        write_if_changed(&dir, "a.jar", b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert!(!dir.join("a.jar.part").exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
