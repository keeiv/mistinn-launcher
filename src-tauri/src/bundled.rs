//! The 霧宿 mod itself, built into the launcher binary.
//!
//! The portable exe has no resource folder beside it, so the jar lives inside the binary and is
//! written out under the launcher's own folder before each launch.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// File name of the bundled mod jar; change it together with the file in `resources/mods`.
pub const MISTINN_JAR_NAME: &str = "mistinn-0.1.0.jar";
const MISTINN_JAR: &[u8] = include_bytes!("../resources/mods/mistinn-0.1.0.jar");

/// Writes the bundled mod into `dir` (only when missing or different) and returns its path.
pub fn extract(dir: &Path) -> Result<PathBuf> {
    write_if_changed(dir, MISTINN_JAR_NAME, MISTINN_JAR)
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
