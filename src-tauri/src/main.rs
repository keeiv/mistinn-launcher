//! 霧宿啟動器 (Mistinn Launcher): starts Minecraft ready for 霧宿：追放, with a performance pack
//! and settings fitted to the player's computer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod bundled;
mod fabric;
mod hardware;
mod java;
mod launch;
mod mojang;
mod net;
mod pack;
mod paths;
mod presets;
mod skin;

use auth::Account;
use presets::Preset;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

struct AppState {
    hardware: hardware::Hardware,
    accounts: Mutex<Vec<Account>>,
    launching: Mutex<bool>,
}

#[derive(Serialize)]
struct AccountView {
    name: String,
    kind: &'static str,
    /// An offline account's own skin, as a PNG data URL for the preview; none for the default.
    skin: Option<String>,
    slim_arms: bool,
}

#[derive(Serialize)]
struct StateView {
    hardware: hardware::Hardware,
    recommended: Preset,
    presets: Vec<presets::Settings>,
    accounts: Vec<AccountView>,
    microsoft_ready: bool,
    mistinn_version: &'static str,
    launcher_version: &'static str,
}

fn account_views(accounts: &[Account]) -> Vec<AccountView> {
    accounts
        .iter()
        .map(|a| {
            let skin = match a {
                Account::Offline { uuid, .. } => skin::load(uuid),
                Account::Microsoft { .. } => None,
            };
            AccountView {
                name: a.name().to_string(),
                kind: match a {
                    Account::Microsoft { .. } => "microsoft",
                    Account::Offline { .. } => "offline",
                },
                slim_arms: skin.as_ref().is_some_and(|s| s.slim_arms),
                skin: skin.map(|s| {
                    use base64::Engine;
                    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(if s.slim_arms { &s.slim } else { &s.wide }))
                }),
            }
        })
        .collect()
}

/// Sets an offline account's skin: the wide-armed and slim-armed versions (64x64 PNGs, made in
/// the window from the player's image) and which arms it was drawn for.
#[tauri::command]
fn set_skin(app: tauri::State<'_, AppState>, index: usize, wide: Vec<u8>, slim: Vec<u8>, slim_arms: bool) -> Result<Vec<AccountView>, String> {
    let accounts = app.accounts.lock().unwrap();
    match accounts.get(index) {
        Some(Account::Offline { uuid, .. }) => skin::save(uuid, &skin::Skin { wide, slim, slim_arms }).map_err(|e| format!("{e:#}"))?,
        _ => return Err("only an offline account takes a skin from here".into()),
    }
    Ok(account_views(&accounts))
}

/// Goes back to the default skin.
#[tauri::command]
fn clear_skin(app: tauri::State<'_, AppState>, index: usize) -> Result<Vec<AccountView>, String> {
    let accounts = app.accounts.lock().unwrap();
    if let Some(Account::Offline { uuid, .. }) = accounts.get(index) {
        skin::remove(uuid).map_err(|e| format!("{e:#}"))?;
    }
    Ok(account_views(&accounts))
}

#[tauri::command]
fn state(app: tauri::State<'_, AppState>) -> StateView {
    let config = auth::config();
    let hw = app.hardware.clone();
    StateView {
        recommended: presets::recommended(&hw),
        presets: [Preset::Smooth, Preset::Normal, Preset::Ultra].iter().map(|p| presets::settings(*p, &hw)).collect(),
        hardware: hw,
        accounts: account_views(&app.accounts.lock().unwrap()),
        microsoft_ready: !config.msa_client_id.trim().is_empty(),
        mistinn_version: launch::MISTINN_VERSION,
        launcher_version: env!("CARGO_PKG_VERSION"),
    }
}

/// Release versions, newest first, for the version picker.
#[tauri::command]
async fn versions() -> Result<Vec<String>, String> {
    let list = mojang::manifest().await.map_err(|e| format!("{e:#}"))?;
    Ok(list.into_iter().filter(|v| v.kind == "release").map(|v| v.id).collect())
}

#[tauri::command]
fn add_offline(app: tauri::State<'_, AppState>, name: String) -> Result<Vec<AccountView>, String> {
    let account = auth::offline(&name).map_err(|e| format!("{e:#}"))?;
    let mut accounts = app.accounts.lock().unwrap();
    accounts.retain(|a| a.name() != account.name());
    accounts.insert(0, account);
    auth::save(&accounts).map_err(|e| format!("{e:#}"))?;
    Ok(account_views(&accounts))
}

#[tauri::command]
fn remove_account(app: tauri::State<'_, AppState>, index: usize) -> Result<Vec<AccountView>, String> {
    let mut accounts = app.accounts.lock().unwrap();
    if index < accounts.len() {
        if let Account::Offline { uuid, .. } = accounts.remove(index) {
            let _ = skin::remove(&uuid);
        }
        auth::save(&accounts).map_err(|e| format!("{e:#}"))?;
    }
    Ok(account_views(&accounts))
}

#[tauri::command]
async fn microsoft_start() -> Result<auth::DeviceCode, String> {
    auth::start_device_code().await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn microsoft_finish(app: tauri::AppHandle, code: auth::DeviceCode) -> Result<Vec<AccountView>, String> {
    let account = auth::finish_device_code(&code).await.map_err(|e| format!("{e:#}"))?;
    let state = app.state::<AppState>();
    let mut accounts = state.accounts.lock().unwrap();
    accounts.retain(|a| a.name() != account.name());
    accounts.insert(0, account);
    auth::save(&accounts).map_err(|e| format!("{e:#}"))?;
    Ok(account_views(&accounts))
}

fn bundled_mods() -> Result<Vec<PathBuf>, String> {
    let jar = bundled::extract(&paths::root().join("bundled")).map_err(|e| format!("cannot unpack the 霧宿 mod: {e:#}"))?;
    Ok(vec![jar])
}

#[tauri::command]
async fn play(app: tauri::AppHandle, version: String, preset: Preset, account: usize) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut busy = state.launching.lock().unwrap();
        if *busy {
            return Err("the game is already starting".into());
        }
        *busy = true;
    }
    let result = async {
        let mut chosen = state.accounts.lock().unwrap().get(account).cloned().ok_or("choose an account first")?;
        if matches!(chosen, Account::Microsoft { .. }) {
            chosen = auth::refresh(&chosen).await.map_err(|e| format!("{e:#}"))?;
            let mut accounts = state.accounts.lock().unwrap();
            if let Some(slot) = accounts.get_mut(account) {
                *slot = chosen.clone();
            }
            let _ = auth::save(&accounts);
        }
        let request = launch::Request { version, preset, account: chosen, bundled: bundled_mods()? };
        let progress_app = app.clone();
        let log_app = app.clone();
        launch::run(
            request,
            &state.hardware,
            move |p| {
                let _ = progress_app.emit("progress", p);
            },
            move |line| {
                let _ = log_app.emit("game-log", line);
            },
        )
        .await
        .map_err(|e| format!("{e:#}"))
    }
    .await;
    *state.launching.lock().unwrap() = false;
    result
}

#[tauri::command]
fn open_instance(version: String) -> Result<(), String> {
    let dir = paths::instance(&launch::instance_id(&version));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new(if cfg!(windows) { "explorer" } else { "xdg-open" }).arg(dir).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// `--selftest <version> <smooth|normal|ultra> <offline name> [mistinn jar|-] [skin png]`: install
/// and start without the window, printing progress, for automated checks. A skin given here is
/// used as both its wide and slim version.
fn selftest(args: &[String]) -> anyhow::Result<()> {
    let version = args.get(2).cloned().unwrap_or_else(|| launch::MISTINN_VERSION.to_string());
    let preset: Preset = serde_json::from_value(serde_json::json!(args.get(3).cloned().unwrap_or_else(|| "normal".into())))?;
    let account = auth::offline(args.get(4).map(String::as_str).unwrap_or("Hunter"))?;
    // without an explicit jar (or with "-"), use the one built into this binary, as the window does
    let bundled: Vec<PathBuf> = match args.get(5).filter(|a| a.as_str() != "-") {
        Some(jar) => vec![PathBuf::from(jar)],
        None => vec![bundled::extract(&paths::root().join("bundled"))?],
    };
    if let (Some(png), Account::Offline { uuid, .. }) = (args.get(6), &account) {
        let bytes = std::fs::read(png)?;
        skin::save(uuid, &skin::Skin { wide: bytes.clone(), slim: bytes, slim_arms: false })?;
    }
    let hw = hardware::detect();
    println!("hardware: {hw:?}");
    println!("settings: {:?}", presets::settings(preset, &hw));
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(launch::run(
        launch::Request { version, preset, account, bundled },
        &hw,
        |p| {
            if p.done == p.total || p.done % 500 == 0 {
                println!("[{}] {}/{}", p.stage, p.done, p.total);
            }
        },
        |line| println!("{line}"),
    ))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--selftest") {
        if let Err(e) = selftest(&args) {
            eprintln!("selftest failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    tauri::Builder::default()
        .manage(AppState { hardware: hardware::detect(), accounts: Mutex::new(auth::load()), launching: Mutex::new(false) })
        .invoke_handler(tauri::generate_handler![state, versions, add_offline, remove_account, set_skin, clear_skin, microsoft_start, microsoft_finish, play, open_instance])
        .run(tauri::generate_context!())
        .expect("error while running the launcher");
}
