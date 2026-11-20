//! Prepares everything a version needs and starts the game.

use crate::auth::Account;
use crate::hardware::Hardware;
use crate::net::{self, Download};
use crate::presets::{self, Preset};
use crate::{fabric, java, mojang, pack, paths};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

/// The game version 霧宿：追放 is made for.
pub const MISTINN_VERSION: &str = "26.3";

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: String,
    pub done: usize,
    pub total: usize,
}

pub struct Request {
    pub version: String,
    pub preset: Preset,
    pub account: Account,
    /// The 霧宿 mod jars shipped with the launcher.
    pub bundled: Vec<PathBuf>,
}

pub fn instance_id(version: &str) -> String {
    if version == MISTINN_VERSION {
        "mistinn".to_string()
    } else {
        format!("play-{version}")
    }
}

/// Everything from download to the running game. `report` receives progress; `log` game output.
pub async fn run(
    request: Request,
    hw: &Hardware,
    report: impl Fn(Progress) + Send + Sync + Clone + 'static,
    log: impl Fn(String) + Send + Sync + Clone + 'static,
) -> Result<()> {
    let id = &request.version;
    let mistinn = id == MISTINN_VERSION;
    let stage = |name: &str| {
        let report = report.clone();
        let name = name.to_string();
        move |done: usize, total: usize| report(Progress { stage: name.clone(), done, total })
    };

    stage("version")(0, 1);
    let version = mojang::version_json(id).await?;

    let component = java::component_of(&version);
    let java = java::ensure(&component, stage("java")).await?;

    let fabric_profile = if fabric::supports(id).await.unwrap_or(false) {
        let loader = fabric::latest_loader(id).await?;
        Some(fabric::profile(id, &loader).await?)
    } else {
        None
    };

    let libs = mojang::libraries(&version);
    let client = mojang::client_jar(&version, id)?;
    let logging = mojang::logging(&version);
    let mut downloads: Vec<Download> = libs.downloads.clone();
    downloads.push(client.clone());
    if let Some((file, _)) = &logging {
        downloads.push(file.clone());
    }
    if let Some(profile) = &fabric_profile {
        downloads.extend(profile.libraries.iter().cloned());
    }
    net::fetch_all(downloads, stage("libraries")).await?;

    let (asset_index, asset_downloads) = mojang::assets(&version).await?;
    net::fetch_all(asset_downloads, stage("assets")).await?;

    let game_dir = paths::instance(&instance_id(id));
    std::fs::create_dir_all(&game_dir)?;
    let settings = presets::settings(request.preset, hw);
    if fabric_profile.is_some() {
        stage("mods")(0, 1);
        let plan = pack::resolve(id, mistinn, &settings).await?;
        let bundled: Vec<PathBuf> = if mistinn { request.bundled.clone() } else { vec![] };
        pack::install(&plan, &game_dir, &bundled, stage("mods")).await?;
        if !plan.missing.is_empty() {
            log(format!("[launcher] no build for this version, skipped: {}", plan.missing.join(", ")));
        }
    }
    let options = game_dir.join("options.txt");
    let first_time = !options.exists();
    presets::write_options(&options, &settings, first_time)?;
    // an offline account's own skin, as a resource pack in place of the default skins
    if crate::skin::apply(&game_dir, &request.account)? {
        log("[launcher] offline skin pack on".to_string());
    }

    let natives = paths::versions().join(id).join("natives");
    unpack_natives(&libs.natives, &natives)?;

    // classpath: Fabric's libraries win over vanilla ones of the same artifact
    let mut classpath: Vec<PathBuf> = vec![];
    let mut taken: HashSet<String> = HashSet::new();
    if let Some(profile) = &fabric_profile {
        for lib in &profile.libraries {
            taken.insert(artifact_key(&lib.path));
            classpath.push(lib.path.clone());
        }
    }
    for path in &libs.classpath {
        if taken.insert(artifact_key(path)) {
            classpath.push(path.clone());
        }
    }
    classpath.push(client.path.clone());
    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath_text = classpath.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(separator);

    let (name, uuid, token, user_type) = match &request.account {
        Account::Microsoft { name, uuid, access_token, .. } => (name.clone(), uuid.clone(), access_token.clone(), "msa"),
        Account::Offline { name, uuid } => (name.clone(), uuid.clone(), "0".to_string(), "legacy"),
    };
    let vars = [
        ("auth_player_name", name),
        ("version_name", id.clone()),
        ("game_directory", game_dir.to_string_lossy().to_string()),
        ("assets_root", paths::assets().to_string_lossy().to_string()),
        ("game_assets", paths::assets().to_string_lossy().to_string()),
        ("assets_index_name", asset_index),
        ("auth_uuid", uuid),
        ("auth_access_token", token.clone()),
        ("auth_session", token),
        ("clientid", String::new()),
        ("auth_xuid", String::new()),
        ("user_type", user_type.to_string()),
        ("version_type", "Mistinn Launcher".to_string()),
        ("user_properties", "{}".to_string()),
        ("natives_directory", natives.to_string_lossy().to_string()),
        ("launcher_name", "mistinn-launcher".to_string()),
        ("launcher_version", env!("CARGO_PKG_VERSION").to_string()),
        ("classpath", classpath_text),
        ("classpath_separator", separator.to_string()),
        ("library_directory", paths::libraries().to_string_lossy().to_string()),
    ];
    let substitute = |arg: &str| -> String {
        let mut out = arg.to_string();
        for (k, v) in &vars {
            out = out.replace(&format!("${{{k}}}"), v);
        }
        out
    };

    let mut jvm: Vec<String> = vec![
        format!("-Xmx{}M", settings.memory_mb),
        format!("-Xms{}M", (settings.memory_mb / 2).max(1024)),
        "-XX:+UseG1GC".into(),
        "-XX:+ParallelRefProcEnabled".into(),
        "-XX:MaxGCPauseMillis=200".into(),
        "-Dfile.encoding=UTF-8".into(),
    ];
    if let Some((_, argument)) = &logging {
        jvm.push(argument.clone());
    }
    let mut game: Vec<String> = vec![];
    if version.pointer("/arguments").is_some() {
        jvm.extend(arguments(&version, "jvm").iter().map(|a| substitute(a)));
        game.extend(arguments(&version, "game").iter().map(|a| substitute(a)));
    } else {
        // before 1.13: one argument string, and the classpath set by hand
        jvm.push(format!("-Djava.library.path={}", natives.display()));
        jvm.push("-cp".into());
        jvm.push(substitute("${classpath}"));
        if let Some(legacy) = version.get("minecraftArguments").and_then(|a| a.as_str()) {
            game.extend(legacy.split_whitespace().map(|a| substitute(a)));
        }
    }
    let mut main_class = version.get("mainClass").and_then(|m| m.as_str()).unwrap_or_default().to_string();
    if let Some(profile) = &fabric_profile {
        jvm.extend(profile.jvm_args.iter().map(|a| substitute(a)));
        game.extend(profile.game_args.iter().map(|a| substitute(a)));
        main_class = profile.main_class.clone();
    }

    stage("start")(1, 1);
    let mut command = tokio::process::Command::new(&java);
    command.args(&jvm).arg(&main_class).args(&game).current_dir(&game_dir).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().with_context(|| format!("starting {}", java.display()))?;
    for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>)].into_iter().flatten() {
        let log = log.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log(line);
            }
        });
    }
    let status = child.wait().await?;
    log(format!("[launcher] the game exited ({status})"));
    Ok(())
}

/// The modern argument list of one kind ("jvm" or "game"), with rule-gated entries resolved.
fn arguments(version: &Value, kind: &str) -> Vec<String> {
    let mut out = vec![];
    for arg in version.pointer(&format!("/arguments/{kind}")).and_then(|a| a.as_array()).into_iter().flatten() {
        match arg {
            Value::String(s) => out.push(s.clone()),
            Value::Object(o) => {
                if mojang::rules_allow(o.get("rules")) {
                    match o.get("value") {
                        Some(Value::String(s)) => out.push(s.clone()),
                        Some(Value::Array(a)) => out.extend(a.iter().filter_map(|v| v.as_str().map(String::from))),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// "group/name" of a library jar under the libraries folder, to spot two versions of one library.
fn artifact_key(path: &Path) -> String {
    let relative = path.strip_prefix(paths::libraries()).unwrap_or(path);
    let parts: Vec<String> = relative.iter().map(|p| p.to_string_lossy().to_string()).collect();
    if parts.len() >= 4 {
        let file = &parts[parts.len() - 1];
        let name = &parts[parts.len() - 3];
        let version = &parts[parts.len() - 2];
        // keep classifiers apart: natives jars are separate artifacts
        let classifier = file
            .strip_prefix(&format!("{name}-{version}"))
            .and_then(|rest| rest.strip_suffix(".jar"))
            .unwrap_or("");
        format!("{}/{name}{classifier}", parts[..parts.len() - 3].join("/"))
    } else {
        relative.to_string_lossy().to_string()
    }
}

fn unpack_natives(jars: &[PathBuf], into: &Path) -> Result<()> {
    if jars.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(into)?;
    for jar in jars {
        let file = std::fs::File::open(jar)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name()?.to_string();
            if entry.is_dir() || name.starts_with("META-INF") {
                continue;
            }
            // only plain file names: never let an archive write outside the folder
            let Some(file_name) = Path::new(&name).file_name() else { continue };
            let mut out = std::fs::File::create(into.join(file_name))?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_keys_ignore_versions_but_not_classifiers() {
        let lib = paths::libraries();
        let a = artifact_key(&lib.join("org/ow2/asm/asm/9.8/asm-9.8.jar"));
        let b = artifact_key(&lib.join("org/ow2/asm/asm/9.9/asm-9.9.jar"));
        let n = artifact_key(&lib.join("org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-windows.jar"));
        let j = artifact_key(&lib.join("org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3.jar"));
        assert_eq!(a, b);
        assert_ne!(n, j);
    }
}
