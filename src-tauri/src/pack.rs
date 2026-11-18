//! The mods that come with each way to play, fetched from Modrinth for the exact game version.
//!
//! The list follows the mainstream Fabric performance packs (Fabulously Optimized and friends):
//! Sodium, Lithium, FerriteCore, ImmediatelyFast, EntityCulling, Dynamic FPS and More Culling,
//! plus AppleSkin and Mod Menu. 霧宿：追放 brings its own minimap and grid inventory, so with it
//! no minimap or inventory mods are added; other versions get Xaero's maps, Jade and Mouse Tweaks.

use crate::net::{self, Download};
use crate::presets::Settings;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

const API: &str = "https://api.modrinth.com/v2";

/// Mods every instance gets (when Modrinth has a build for the version).
const CORE: &[&str] = &[
    "fabric-api",
    "sodium",
    "sodium-extra",
    "reeses-sodium-options",
    "lithium",
    "ferrite-core",
    "immediatelyfast",
    "entityculling",
    "dynamic-fps",
    "moreculling",
    "appleskin",
    "modmenu",
];
/// For ordinary play on other versions: maps and quality-of-life the Mistinn mod replaces.
const PLAIN_EXTRAS: &[&str] = &["xaeros-minimap", "xaeros-world-map", "jade", "mouse-tweaks"];
const SHADER_MOD: &str = "iris";
const SHADER_PACK: &str = "complementary-reimagined";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolved {
    pub slug: String,
    pub title: String,
    pub version: String,
    pub file: String,
    pub url: String,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub mods: Vec<Resolved>,
    pub shader: Option<Resolved>,
    /// Mods in the list that have no build for this version.
    pub missing: Vec<String>,
}

pub fn wanted(mistinn: bool, settings: &Settings) -> Vec<&'static str> {
    let mut list: Vec<&'static str> = CORE.to_vec();
    if !mistinn {
        list.extend_from_slice(PLAIN_EXTRAS);
    }
    if settings.shaders {
        list.push(SHADER_MOD);
    }
    list
}

async fn latest(project: &str, game_version: &str, loader: &str) -> Result<Option<(Value, Value)>> {
    let url = format!(
        "{API}/project/{project}/version?loaders=%5B%22{loader}%22%5D&game_versions=%5B%22{game_version}%22%5D"
    );
    let versions: Value = match net::json(&url).await {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let list = versions.as_array().cloned().unwrap_or_default();
    // prefer a release, then a beta, then anything
    let pick = ["release", "beta", "alpha"]
        .iter()
        .find_map(|kind| list.iter().find(|v| v.get("version_type").and_then(|t| t.as_str()) == Some(kind)))
        .cloned();
    let Some(version) = pick else { return Ok(None) };
    let file = version
        .get("files")
        .and_then(|f| f.as_array())
        .and_then(|files| files.iter().find(|f| f.get("primary").and_then(|p| p.as_bool()).unwrap_or(false)).or_else(|| files.first()))
        .cloned();
    Ok(file.map(|f| (version, f)))
}

/// A file name we will write or delete inside an instance folder: one plain name ending in .jar
/// or .zip, no folders, no "..". Names come from Modrinth and from our own lock file, and neither
/// should be able to reach outside the folder.
pub fn safe_file_name(name: &str) -> bool {
    let plain = Path::new(name).file_name().map(|n| n == std::ffi::OsStr::new(name)).unwrap_or(false);
    let lower = name.to_ascii_lowercase();
    plain
        && !name.contains(['/', '\\', ':'])
        && !name.starts_with('.')
        && (lower.ends_with(".jar") || lower.ends_with(".zip"))
}

fn resolved(slug: &str, version: &Value, file: &Value) -> Resolved {
    Resolved {
        slug: slug.to_string(),
        title: version.get("name").and_then(|n| n.as_str()).unwrap_or(slug).to_string(),
        version: version.get("version_number").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
        file: file.get("filename").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
        url: file.get("url").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
        sha1: file.pointer("/hashes/sha1").and_then(|n| n.as_str()).map(String::from),
    }
}

/// Finds a build of every wanted mod, and of each mod they require, for the game version.
pub async fn resolve(game_version: &str, mistinn: bool, settings: &Settings) -> Result<Plan> {
    let mut queue: Vec<String> = wanted(mistinn, settings).iter().map(|s| s.to_string()).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut mods = vec![];
    let mut missing = vec![];
    while let Some(project) = queue.pop() {
        if !seen.insert(project.clone()) {
            continue;
        }
        match latest(&project, game_version, "fabric").await?.filter(|(_, f)| f.get("filename").and_then(|n| n.as_str()).is_some_and(safe_file_name)) {
            Some((version, file)) => {
                let id = version.get("project_id").and_then(|p| p.as_str()).unwrap_or_default().to_string();
                if !seen_ids.insert(id) {
                    continue;
                }
                for dep in version.get("dependencies").and_then(|d| d.as_array()).into_iter().flatten() {
                    if dep.get("dependency_type").and_then(|t| t.as_str()) == Some("required") {
                        if let Some(pid) = dep.get("project_id").and_then(|p| p.as_str()) {
                            queue.push(pid.to_string());
                        }
                    }
                }
                mods.push(resolved(&project, &version, &file));
            }
            None => missing.push(project),
        }
    }
    let shader = if settings.shaders {
        latest(SHADER_PACK, game_version, "iris")
            .await?
            .map(|(v, f)| resolved(SHADER_PACK, &v, &f))
            .filter(|r| safe_file_name(&r.file))
    } else {
        None
    };
    mods.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(Plan { mods, shader, missing })
}

const LOCK: &str = "mistinn-managed.json";

/// Puts the plan's mods into the instance: downloads what is new, removes mods this launcher put
/// there earlier that are no longer wanted, and leaves the player's own mods alone.
pub async fn install(plan: &Plan, game_dir: &Path, bundled: &[std::path::PathBuf], progress: impl Fn(usize, usize) + Send + Sync + 'static) -> Result<()> {
    let mods_dir = game_dir.join("mods");
    std::fs::create_dir_all(&mods_dir)?;
    let lock = game_dir.join(LOCK);
    let previous: Vec<String> = std::fs::read_to_string(&lock).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();

    let mut now: Vec<String> = plan.mods.iter().map(|m| m.file.clone()).collect();
    for jar in bundled {
        if let Some(name) = jar.file_name() {
            std::fs::copy(jar, mods_dir.join(name))?;
            now.push(name.to_string_lossy().to_string());
        }
    }
    for old in previous.iter().filter(|f| !now.contains(f) && safe_file_name(f)) {
        let _ = std::fs::remove_file(mods_dir.join(old));
    }
    let mut downloads: Vec<Download> = plan
        .mods
        .iter()
        .map(|m| Download { url: m.url.clone(), path: mods_dir.join(&m.file), sha1: m.sha1.clone() })
        .collect();
    if let Some(shader) = &plan.shader {
        downloads.push(Download { url: shader.url.clone(), path: game_dir.join("shaderpacks").join(&shader.file), sha1: shader.sha1.clone() });
    }
    net::fetch_all(downloads, progress).await?;
    std::fs::write(&lock, serde_json::to_string_pretty(&now)?)?;

    // Iris: shaders on with the downloaded pack, or off
    if plan.mods.iter().any(|m| m.slug == SHADER_MOD) || game_dir.join("config/iris.properties").exists() {
        let mut text = String::from("# managed by Mistinn Launcher\n");
        match &plan.shader {
            Some(pack) => text.push_str(&format!("enableShaders=true\nshaderPack={}\n", pack.file)),
            None => text.push_str("enableShaders=false\n"),
        }
        std::fs::create_dir_all(game_dir.join("config"))?;
        std::fs::write(game_dir.join("config/iris.properties"), text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_jar_and_zip_names_are_safe() {
        assert!(safe_file_name("sodium-fabric-0.9.2+mc26.3.jar"));
        assert!(safe_file_name("ComplementaryReimagined_r5.9.3.zip"));
        for bad in ["../evil.jar", "..\\evil.jar", "sub/mod.jar", "C:evil.jar", ".hidden.jar", "mod.exe", ""] {
            assert!(!safe_file_name(bad), "{bad}");
        }
    }
}
