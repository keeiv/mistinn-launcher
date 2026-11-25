//! The mods that come with each way to play, fetched from Modrinth for the exact game version.
//!
//! The list follows the mainstream Fabric performance packs (Fabulously Optimized and friends):
//! Sodium, Lithium, `FerriteCore`, `ImmediatelyFast`, `EntityCulling`, Dynamic FPS and More Culling,
//! plus `AppleSkin` and Mod Menu. 霧宿：追放 brings its own minimap and grid inventory, so with it
//! no minimap or inventory mods are added; other versions get Xaero's maps, Jade and Mouse Tweaks.
//!
//! The newest build of each is taken unless the set would not load: a mod can refuse another's
//! build (Sodium 0.9.3 refused every Iris out for 26.3 the day it came out). The jars say so in
//! their `fabric.mod.json`; [`settle`] reads them and steps the mod in conflict back a build.

#[path = "compat.rs"]
mod compat;

use crate::net::{self, Download};
use crate::presets::Settings;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

/// Modrinth's API.
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
/// The relay that opens a 霧宿 room to friends anywhere (e4mc, MIT): the room's invite carries the
/// address it hands out, so nobody has to forward a port.
const RELAY_MOD: &str = "e4mc";
/// The mod that runs shader packs.
const SHADER_MOD: &str = "iris";
/// The shader pack used on the strongest graphics cards.
const SHADER_PACK: &str = "complementary-reimagined";

/// A mod or pack chosen for a version: which build, and where to get it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolved {
    pub slug: String,
    pub title: String,
    pub version: String,
    pub file: String,
    pub url: String,
    pub sha1: Option<String>,
}

/// Everything an instance gets for one way to play.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub mods: Vec<Resolved>,
    pub shader: Option<Resolved>,
    /// Mods in the list that have no build for this version.
    pub missing: Vec<String>,
    /// Mods stepped back to an older build so the set loads, and why.
    pub notes: Vec<String>,
}

/// One mod and every build of it for the game version, newest release first; `at` is the one taken.
struct Choice {
    slug: String,
    options: Vec<(Value, Value)>,
    at: usize,
}

/// The Modrinth projects an instance wants, by slug, for this way to play.
pub fn wanted(mistinn: bool, settings: &Settings) -> Vec<&'static str> {
    let mut list: Vec<&'static str> = CORE.to_vec();
    if mistinn {
        list.push(RELAY_MOD);
    } else {
        list.extend_from_slice(PLAIN_EXTRAS);
    }
    if settings.shaders {
        list.push(SHADER_MOD);
    }
    list
}

/// Every build of a project for the game version with its main file: releases newest first, then
/// betas, then alphas.
async fn candidates(project: &str, game_version: &str, loader: &str) -> Vec<(Value, Value)> {
    let url = format!("{API}/project/{project}/version?loaders=%5B%22{loader}%22%5D&game_versions=%5B%22{game_version}%22%5D");
    let Ok(versions) = net::json::<Value>(&url).await else {
        return vec![];
    };
    let list = versions.as_array().cloned().unwrap_or_default();
    let mut out = vec![];
    for kind in ["release", "beta", "alpha"] {
        for version in list.iter().filter(|v| v.get("version_type").and_then(|t| t.as_str()) == Some(kind)) {
            let file = version
                .get("files")
                .and_then(|f| f.as_array())
                .and_then(|files| {
                    files
                        .iter()
                        .find(|f| f.get("primary").and_then(serde_json::Value::as_bool).unwrap_or(false))
                        .or_else(|| files.first())
                })
                .cloned();
            if let Some(file) = file.filter(|f| f.get("filename").and_then(|n| n.as_str()).is_some_and(safe_file_name)) {
                out.push((version.clone(), file));
            }
        }
    }
    out
}

/// A project's newest build for a game version and loader, preferring releases; `None` when there
/// is none.
async fn latest(project: &str, game_version: &str, loader: &str) -> Result<Option<(Value, Value)>> {
    let url = format!("{API}/project/{project}/version?loaders=%5B%22{loader}%22%5D&game_versions=%5B%22{game_version}%22%5D");
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
        .and_then(|files| {
            files
                .iter()
                .find(|f| f.get("primary").and_then(serde_json::Value::as_bool).unwrap_or(false))
                .or_else(|| files.first())
        })
        .cloned();
    Ok(file.map(|f| (version, f)))
}

/// A file name we will write or delete inside an instance folder: one plain name ending in .jar
/// or .zip, no folders, no "..". Names come from Modrinth and from our own lock file, and neither
/// should be able to reach outside the folder.
pub fn safe_file_name(name: &str) -> bool {
    let plain = Path::new(name).file_name().is_some_and(|n| n == std::ffi::OsStr::new(name));
    let archive = Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jar") || e.eq_ignore_ascii_case("zip"));
    plain && !name.contains(['/', '\\', ':']) && !name.starts_with('.') && archive
}

/// A Modrinth version and its file, as a [`Resolved`] download.
fn resolved(slug: &str, version: &Value, file: &Value) -> Resolved {
    Resolved {
        slug: slug.to_string(),
        title: version.get("name").and_then(|n| n.as_str()).unwrap_or(slug).to_string(),
        version: version
            .get("version_number")
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string(),
        file: file.get("filename").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
        url: file.get("url").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
        sha1: file.pointer("/hashes/sha1").and_then(|n| n.as_str()).map(String::from),
    }
}

/// Finds a build of every wanted mod, and of each mod they require, for the game version: the
/// newest, stepped back where the set would not load together (the bundled jars included). Jars
/// read for the check are kept in `cache` for [`install`].
pub async fn resolve(game_version: &str, mistinn: bool, settings: &Settings, cache: &Path, bundled: &[std::path::PathBuf]) -> Result<Plan> {
    let mut queue: Vec<String> = wanted(mistinn, settings).iter().map(std::string::ToString::to_string).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut choices: Vec<Choice> = vec![];
    let mut missing = vec![];
    while let Some(project) = queue.pop() {
        if !seen.insert(project.clone()) {
            continue;
        }
        let options = candidates(&project, game_version, "fabric").await;
        let Some((version, _)) = options.first() else {
            missing.push(project);
            continue;
        };
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
        choices.push(Choice {
            slug: project,
            options,
            at: 0,
        });
    }
    let notes = settle(&mut choices, cache, bundled).await;
    let mut mods: Vec<Resolved> = choices
        .iter()
        .map(|c| resolved(&c.slug, &c.options[c.at].0, &c.options[c.at].1))
        .collect();
    let shader = if settings.shaders {
        latest(SHADER_PACK, game_version, "iris")
            .await?
            .map(|(v, f)| resolved(SHADER_PACK, &v, &f))
            .filter(|r| safe_file_name(&r.file))
    } else {
        None
    };
    mods.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(Plan {
        mods,
        shader,
        missing,
        notes,
    })
}

/// Where a mod's jar is kept between the check and the install.
fn cached(cache: &Path, file: &Value) -> Option<std::path::PathBuf> {
    let name = file.get("filename")?.as_str()?;
    safe_file_name(name).then(|| cache.join(name))
}

/// The jar of one build, from the cache or downloaded into it.
async fn jar(cache: &Path, file: &Value) -> Option<Vec<u8>> {
    let path = cached(cache, file)?;
    let sha1 = file.pointer("/hashes/sha1").and_then(|n| n.as_str()).map(String::from);
    let item = Download {
        url: file.get("url")?.as_str()?.to_string(),
        path: path.clone(),
        sha1,
    };
    net::fetch_all(vec![item], |_, _| {}).await.ok()?;
    std::fs::read(&path).ok()
}

/// Steps mods back a build at a time until no mod in the set breaks another's build or needs
/// another build than the one taken. The mod doing the breaking (or the needing) goes back
/// first; if it has no older build, the other does. Gives up after a few rounds and leaves a note.
async fn settle(choices: &mut [Choice], cache: &Path, bundled: &[std::path::PathBuf]) -> Vec<String> {
    let mut notes = vec![];
    let fixed: Vec<compat::Manifest> = bundled
        .iter()
        .filter_map(|p| std::fs::read(p).ok())
        .filter_map(|b| compat::manifest(&b))
        .collect();
    for _ in 0..12 {
        let mut set: Vec<compat::Manifest> = vec![];
        let mut owner: Vec<Option<usize>> = vec![];
        for (i, choice) in choices.iter().enumerate() {
            if let Some(manifest) = jar(cache, &choice.options[choice.at].1).await.and_then(|b| compat::manifest(&b)) {
                set.push(manifest);
                owner.push(Some(i));
            }
        }
        for manifest in &fixed {
            set.push(manifest.clone());
            owner.push(None);
        }
        let found = compat::conflicts(&set);
        if found.is_empty() {
            return notes;
        }
        let mut stepped = false;
        for conflict in &found {
            let (first, second) = match *conflict {
                compat::Conflict::Breaks { r#mod, other } | compat::Conflict::Needs { r#mod, other } => (r#mod, other),
            };
            for who in [first, second] {
                if let Some(i) = owner[who] {
                    let choice = &mut choices[i];
                    if choice.at + 1 < choice.options.len() {
                        let from = set[who].version.clone();
                        choice.at += 1;
                        let to = choice.options[choice.at]
                            .0
                            .get("version_number")
                            .and_then(|n| n.as_str())
                            .unwrap_or_default()
                            .to_string();
                        notes.push(format!(
                            "{} {from} does not load with {} {}: using {to}",
                            set[who].id,
                            set[if who == first { second } else { first }].id,
                            set[if who == first { second } else { first }].version
                        ));
                        stepped = true;
                        break;
                    }
                }
            }
            if stepped {
                break;
            }
        }
        if !stepped {
            notes.push(format!("could not settle a conflict: {found:?}"));
            return notes;
        }
    }
    notes
}

/// The file in an instance that lists the mods the launcher put there.
const LOCK: &str = "mistinn-managed.json";

/// Puts the plan's mods into the instance: downloads what is new, removes mods this launcher put
/// there earlier that are no longer wanted, and leaves the player's own mods alone.
pub async fn install(
    plan: &Plan,
    game_dir: &Path,
    cache: &Path,
    bundled: &[std::path::PathBuf],
    progress: impl Fn(usize, usize) + Send + Sync + 'static,
) -> Result<()> {
    let mods_dir = game_dir.join("mods");
    std::fs::create_dir_all(&mods_dir)?;
    // the jars the compatibility check already fetched need not be fetched again
    for m in &plan.mods {
        let target = mods_dir.join(&m.file);
        let kept = cache.join(&m.file);
        if !target.exists() && kept.exists() && safe_file_name(&m.file) {
            let _ = std::fs::copy(&kept, &target);
        }
    }
    let lock = game_dir.join(LOCK);
    let previous: Vec<String> = std::fs::read_to_string(&lock)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();

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
        .map(|m| Download {
            url: m.url.clone(),
            path: mods_dir.join(&m.file),
            sha1: m.sha1.clone(),
        })
        .collect();
    if let Some(shader) = &plan.shader {
        downloads.push(Download {
            url: shader.url.clone(),
            path: game_dir.join("shaderpacks").join(&shader.file),
            sha1: shader.sha1.clone(),
        });
    }
    net::fetch_all(downloads, progress).await?;
    std::fs::write(&lock, serde_json::to_string_pretty(&now)?)?;
    // the check's cache keeps only the builds now in use
    if let Ok(entries) = std::fs::read_dir(cache) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if safe_file_name(&name) && !plan.mods.iter().any(|m| m.file == name) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    // Iris: shaders on with the downloaded pack, or off
    if plan.mods.iter().any(|m| m.slug == SHADER_MOD) || game_dir.join("config/iris.properties").exists() {
        let mut text = String::from("# managed by Mistinn Launcher\n");
        match &plan.shader {
            Some(pack) => {
                let _ = write!(text, "enableShaders=true\nshaderPack={}\n", pack.file);
            }
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

    /// Against Modrinth itself (run with `cargo test -- --ignored`): the 26.3 set with shaders
    /// must be one Fabric Loader will start, whatever was released today.
    #[tokio::test]
    #[ignore = "reaches Modrinth over the network"]
    async fn the_live_set_for_26_3_loads_together() {
        let settings = crate::presets::Settings {
            preset: crate::presets::Preset::Ultra,
            memory_mb: 8192,
            render_distance: 12,
            simulation_distance: 8,
            graphics: "fancy",
            particles: 0,
            clouds: "fancy",
            biome_blend: 2,
            mipmaps: 4,
            entity_distance: 1.0,
            max_fps: 120,
            shaders: true,
        };
        let cache = std::env::temp_dir().join("mistinn-compat-check");
        std::fs::create_dir_all(&cache).unwrap();
        let plan = resolve("26.3", true, &settings, &cache, &[]).await.unwrap();
        for note in &plan.notes {
            println!("note: {note}");
        }
        for m in &plan.mods {
            println!("{} {}", m.slug, m.version);
        }
        assert!(!plan.notes.iter().any(|n| n.starts_with("could not settle")), "{:?}", plan.notes);
        let set: Vec<compat::Manifest> = plan
            .mods
            .iter()
            .filter_map(|m| std::fs::read(cache.join(&m.file)).ok())
            .filter_map(|b| compat::manifest(&b))
            .collect();
        assert_eq!(set.len(), plan.mods.len());
        assert_eq!(compat::conflicts(&set), vec![]);
    }

    #[test]
    fn only_plain_jar_and_zip_names_are_safe() {
        assert!(safe_file_name("sodium-fabric-0.9.2+mc26.3.jar"));
        assert!(safe_file_name("ComplementaryReimagined_r5.9.3.zip"));
        for bad in [
            "../evil.jar",
            "..\\evil.jar",
            "sub/mod.jar",
            "C:evil.jar",
            ".hidden.jar",
            "mod.exe",
            "",
        ] {
            assert!(!safe_file_name(bad), "{bad}");
        }
    }
}
