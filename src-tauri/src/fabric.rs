//! Fabric Loader: which game versions it supports, and the launch profile that adds the loader's
//! libraries and main class on top of vanilla.

use crate::net::{self, Download};
use crate::paths;
use anyhow::{anyhow, Result};
use serde_json::Value;

/// Fabric's metadata service.
const META: &str = "https://meta.fabricmc.net/v2";

/// Whether Fabric supports this game version at all.
pub async fn supports(game_version: &str) -> Result<bool> {
    let versions: Value = net::json(&format!("{META}/versions/game")).await?;
    Ok(versions
        .as_array()
        .is_some_and(|list| list.iter().any(|v| v.get("version").and_then(|s| s.as_str()) == Some(game_version))))
}

/// The newest stable loader for the game version.
pub async fn latest_loader(game_version: &str) -> Result<String> {
    let loaders: Value = net::json(&format!("{META}/versions/loader/{game_version}")).await?;
    let list = loaders.as_array().ok_or_else(|| anyhow!("no Fabric loader for {game_version}"))?;
    let pick = list
        .iter()
        .find(|l| l.pointer("/loader/stable").and_then(serde_json::Value::as_bool).unwrap_or(false))
        .or_else(|| list.first())
        .ok_or_else(|| anyhow!("no Fabric loader for {game_version}"))?;
    Ok(pick
        .pointer("/loader/version")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string())
}

/// What Fabric adds to a game version: its main class, its libraries and its extra arguments.
pub struct Profile {
    pub main_class: String,
    pub libraries: Vec<Download>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

/// Fabric's launch profile for a game version and loader version.
pub async fn profile(game_version: &str, loader: &str) -> Result<Profile> {
    let json: Value = net::json(&format!("{META}/versions/loader/{game_version}/{loader}/profile/json")).await?;
    let mut libraries = vec![];
    for lib in json.get("libraries").and_then(|l| l.as_array()).into_iter().flatten() {
        let Some(name) = lib.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let Some(relative) = paths::maven_path(name) else { continue };
        let base = lib.get("url").and_then(|u| u.as_str()).unwrap_or("https://maven.fabricmc.net/");
        let base = if base.ends_with('/') {
            base.to_string()
        } else {
            format!("{base}/")
        };
        libraries.push(Download {
            url: format!("{base}{}", relative.to_string_lossy().replace('\\', "/")),
            path: paths::libraries().join(&relative),
            sha1: lib.get("sha1").and_then(|s| s.as_str()).map(String::from),
        });
    }
    let strings = |pointer: &str| -> Vec<String> {
        json.pointer(pointer)
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    };
    Ok(Profile {
        main_class: json.get("mainClass").and_then(|m| m.as_str()).unwrap_or_default().to_string(),
        libraries,
        jvm_args: strings("/arguments/jvm"),
        game_args: strings("/arguments/game"),
    })
}
