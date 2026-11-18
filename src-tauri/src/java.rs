//! The Java runtime each version asks for, downloaded from Mojang's own runtime list, so players
//! never need to install Java themselves.

use crate::net::{self, Download};
use crate::paths;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::path::PathBuf;

const RUNTIMES: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

fn platform() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        "windows-arm64"
    } else if cfg!(target_os = "macos") {
        "mac-os"
    } else {
        "linux"
    }
}

/// The runtime component a version wants ("java-runtime-epsilon" for 26.x), default jre-legacy.
pub fn component_of(version: &Value) -> String {
    version
        .pointer("/javaVersion/component")
        .and_then(|c| c.as_str())
        .unwrap_or("jre-legacy")
        .to_string()
}

pub fn java_exe(component: &str) -> PathBuf {
    let dir = paths::runtimes().join(component);
    if cfg!(windows) {
        dir.join("bin").join("javaw.exe")
    } else {
        dir.join("bin").join("java")
    }
}

/// Downloads (or checks) the runtime and returns the java executable.
pub async fn ensure(component: &str, progress: impl Fn(usize, usize) + Send + Sync + 'static) -> Result<PathBuf> {
    let all: Value = net::json(RUNTIMES).await?;
    let entry = all
        .pointer(&format!("/{}/{component}/0", platform()))
        .ok_or_else(|| anyhow!("no {component} runtime for {}", platform()))?;
    let manifest_url = entry.pointer("/manifest/url").and_then(|u| u.as_str()).context("runtime manifest url")?;
    let manifest: Value = net::json(manifest_url).await?;
    let root = paths::runtimes().join(component);
    let mut downloads = vec![];
    for (relative, file) in manifest.get("files").and_then(|f| f.as_object()).into_iter().flatten() {
        let target = root.join(relative);
        match file.get("type").and_then(|t| t.as_str()) {
            Some("directory") => {
                std::fs::create_dir_all(&target)?;
            }
            Some("file") => {
                let raw = file.pointer("/downloads/raw").context("runtime file download")?;
                downloads.push(Download {
                    url: raw["url"].as_str().unwrap_or_default().to_string(),
                    path: target,
                    sha1: raw.get("sha1").and_then(|s| s.as_str()).map(String::from),
                });
            }
            _ => {}
        }
    }
    net::fetch_all(downloads, progress).await?;
    let exe = java_exe(component);
    if !exe.exists() {
        return Err(anyhow!("java was not found at {} after installing {component}", exe.display()));
    }
    Ok(exe)
}
