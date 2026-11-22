//! Vanilla Minecraft: the version list, a version's description, its libraries, assets and
//! client jar, read from Mojang's launcher metadata.

use crate::net::{self, Download};
use crate::paths;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// The list of every game version.
const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
/// Where the game's assets are downloaded from, by hash.
const RESOURCES: &str = "https://resources.download.minecraft.net";

/// One game version in Mojang's list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
}

/// Mojang's version list, as far as the launcher reads it.
#[derive(Debug, Deserialize)]
struct Manifest {
    versions: Vec<ManifestEntry>,
}

/// Every game version; the last list fetched is kept and used when offline.
pub async fn manifest() -> Result<Vec<ManifestEntry>> {
    let cache = paths::versions().join("version_manifest_v2.json");
    match net::json::<Manifest>(MANIFEST).await {
        Ok(m) => {
            if let Ok(text) = serde_json::to_string(&serde_json::json!({ "versions": m.versions })) {
                let _ = std::fs::create_dir_all(paths::versions());
                let _ = std::fs::write(&cache, text);
            }
            Ok(m.versions)
        }
        // offline: fall back to the last list we saw
        Err(e) => {
            let text = std::fs::read_to_string(&cache).map_err(|_| e)?;
            Ok(serde_json::from_str::<Manifest>(&text)?.versions)
        }
    }
}

/// The version description (libraries, arguments, assets), cached under versions/<id>/<id>.json.
pub async fn version_json(id: &str) -> Result<Value> {
    let path = paths::versions().join(id).join(format!("{id}.json"));
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str(&text) {
            return Ok(value);
        }
    }
    let entry = manifest()
        .await?
        .into_iter()
        .find(|v| v.id == id)
        .ok_or_else(|| anyhow!("unknown version {id}"))?;
    net::fetch_all(
        vec![Download {
            url: entry.url,
            path: path.clone(),
            sha1: Some(entry.sha1),
        }],
        |_, _| {},
    )
    .await?;
    Ok(serde_json::from_str(&std::fs::read_to_string(&path)?)?)
}

/// Whether a library or argument applies on this machine (Windows x64, no optional features).
pub fn rules_allow(rules: Option<&Value>) -> bool {
    let Some(rules) = rules.and_then(|r| r.as_array()) else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        let allow = rule.get("action").and_then(|a| a.as_str()) == Some("allow");
        let mut matches = true;
        if let Some(os) = rule.get("os") {
            if let Some(name) = os.get("name").and_then(|n| n.as_str()) {
                matches &= name == current_os();
            }
            if let Some(arch) = os.get("arch").and_then(|a| a.as_str()) {
                matches &= arch == "x86_64" || (arch == "x86" && cfg!(target_arch = "x86"));
            }
        }
        if rule.get("features").is_some() {
            // demo user, custom resolution, quick play: none of them are requested
            matches = false;
        }
        if matches {
            allowed = allow;
        }
    }
    allowed
}

/// This machine's operating system, in the words of a version's library rules.
pub fn current_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

/// A version's libraries, sorted for this machine.
pub struct Libraries {
    pub downloads: Vec<Download>,
    pub classpath: Vec<PathBuf>,
    /// Old-style native jars to unpack into the natives folder.
    pub natives: Vec<PathBuf>,
}

/// Libraries for this machine: what to download, what goes on the classpath, what to unpack.
pub fn libraries(version: &Value) -> Libraries {
    let mut out = Libraries {
        downloads: vec![],
        classpath: vec![],
        natives: vec![],
    };
    let os = current_os();
    for lib in version.get("libraries").and_then(|l| l.as_array()).into_iter().flatten() {
        if !rules_allow(lib.get("rules")) {
            continue;
        }
        let name = lib.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if cfg!(target_arch = "x86_64") && name.ends_with("-arm64") {
            continue;
        }
        if let Some(artifact) = lib.pointer("/downloads/artifact") {
            if let Some(d) = artifact_download(artifact) {
                out.classpath.push(d.path.clone());
                out.downloads.push(d);
            }
        }
        // before 1.19, natives were separate classifier jars named in a "natives" map
        if let Some(classifier) = lib.pointer(&format!("/natives/{os}")).and_then(|c| c.as_str()) {
            let classifier = classifier.replace("${arch}", "64");
            if let Some(native) = lib.pointer(&format!("/downloads/classifiers/{classifier}")) {
                if let Some(d) = artifact_download(native) {
                    out.natives.push(d.path.clone());
                    out.downloads.push(d);
                }
            }
        }
    }
    out
}

/// A library artifact as a download into the libraries folder; `None` without a URL.
fn artifact_download(artifact: &Value) -> Option<Download> {
    let path = artifact.get("path")?.as_str()?;
    let url = artifact.get("url")?.as_str()?;
    if url.is_empty() {
        return None;
    }
    Some(Download {
        url: url.to_string(),
        path: paths::libraries().join(path),
        sha1: artifact.get("sha1").and_then(|s| s.as_str()).map(String::from),
    })
}

/// The game's own jar for a version.
pub fn client_jar(version: &Value, id: &str) -> Result<Download> {
    let client = version.pointer("/downloads/client").context("version has no client download")?;
    Ok(Download {
        url: client["url"].as_str().unwrap_or_default().to_string(),
        path: paths::versions().join(id).join(format!("{id}.jar")),
        sha1: client.get("sha1").and_then(|s| s.as_str()).map(String::from),
    })
}

/// The asset index and every asset object it lists.
pub async fn assets(version: &Value) -> Result<(String, Vec<Download>)> {
    let index = version.get("assetIndex").context("version has no asset index")?;
    let index_id = index["id"].as_str().unwrap_or("legacy").to_string();
    let index_path = paths::assets().join("indexes").join(format!("{index_id}.json"));
    net::fetch_all(
        vec![Download {
            url: index["url"].as_str().unwrap_or_default().to_string(),
            path: index_path.clone(),
            sha1: index.get("sha1").and_then(|s| s.as_str()).map(String::from),
        }],
        |_, _| {},
    )
    .await?;
    let parsed: Value = serde_json::from_str(&std::fs::read_to_string(&index_path)?)?;
    let mut downloads = vec![];
    if let Some(objects) = parsed.get("objects").and_then(|o| o.as_object()) {
        for object in objects.values() {
            let Some(hash) = object.get("hash").and_then(|h| h.as_str()) else {
                continue;
            };
            let prefix = &hash[..2];
            downloads.push(Download {
                url: format!("{RESOURCES}/{prefix}/{hash}"),
                path: paths::assets().join("objects").join(prefix).join(hash),
                sha1: Some(hash.to_string()),
            });
        }
    }
    Ok((index_id, downloads))
}

/// The log4j configuration file and the JVM argument that points at it.
pub fn logging(version: &Value) -> Option<(Download, String)> {
    let client = version.pointer("/logging/client")?;
    let file = client.get("file")?;
    let id = file.get("id")?.as_str()?;
    let path = paths::assets().join("log_configs").join(id);
    let argument = client.get("argument")?.as_str()?.replace("${path}", &path.to_string_lossy());
    Some((
        Download {
            url: file.get("url")?.as_str()?.to_string(),
            path,
            sha1: file.get("sha1").and_then(|s| s.as_str()).map(String::from),
        },
        argument,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rules() {
        assert!(rules_allow(None));
        let windows_only = json!([{ "action": "allow", "os": { "name": "windows" } }]);
        assert_eq!(rules_allow(Some(&windows_only)), cfg!(windows));
        let not_osx = json!([{ "action": "allow" }, { "action": "disallow", "os": { "name": "osx" } }]);
        assert!(rules_allow(Some(&not_osx)) || cfg!(target_os = "macos"));
        let feature = json!([{ "action": "allow", "features": { "is_demo_user": true } }]);
        assert!(!rules_allow(Some(&feature)));
    }
}
