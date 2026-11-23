//! 霧宿啟動器 (Mistinn Launcher): starts Minecraft ready for 霧宿：追放, with a performance pack
//! and settings fitted to the player's computer.
// Tauri hands a command its state and arguments by value: that is how it calls them
#![allow(clippy::needless_pass_by_value)]
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
mod update;

use auth::Account;
use presets::Preset;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// What the window's commands share: the hardware found at start, the accounts, and whether a
/// launch is under way.
struct AppState {
    hardware: hardware::Hardware,
    accounts: Mutex<Vec<Account>>,
    launching: Mutex<bool>,
    /// The newer launcher found at start, if any.
    update: Mutex<Option<update::Available>>,
}

/// An account as the window shows it, without its tokens.
#[derive(Serialize)]
struct AccountView {
    name: String,
    kind: &'static str,
    /// An offline account's own skin, as a PNG data URL for the preview; none for the default.
    skin: Option<String>,
    slim_arms: bool,
}

/// Everything the window needs when it opens.
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

/// The accounts as the window shows them.
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
                    format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(if s.slim_arms { &s.slim } else { &s.wide })
                    )
                }),
            }
        })
        .collect()
}

/// Sets an offline account's skin: the wide-armed and slim-armed versions (64x64 PNGs, made in
/// the window from the player's image) and which arms it was drawn for.
#[tauri::command]
fn set_skin(
    app: tauri::State<'_, AppState>,
    index: usize,
    wide: Vec<u8>,
    slim: Vec<u8>,
    slim_arms: bool,
) -> Result<Vec<AccountView>, String> {
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

/// Everything the window needs when it opens: hardware, the three ways to play, accounts and
/// versions.
#[tauri::command]
fn state(app: tauri::State<'_, AppState>) -> StateView {
    let config = auth::config();
    let hw = app.hardware.clone();
    StateView {
        recommended: presets::recommended(&hw),
        presets: [Preset::Smooth, Preset::Normal, Preset::Ultra]
            .iter()
            .map(|p| presets::settings(*p, &hw))
            .collect(),
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

/// Adds an offline account (or moves it to the top) and saves the list.
#[tauri::command]
fn add_offline(app: tauri::State<'_, AppState>, name: String) -> Result<Vec<AccountView>, String> {
    let account = auth::offline(&name).map_err(|e| format!("{e:#}"))?;
    let mut accounts = app.accounts.lock().unwrap();
    accounts.retain(|a| a.name() != account.name());
    accounts.insert(0, account);
    auth::save(&accounts).map_err(|e| format!("{e:#}"))?;
    Ok(account_views(&accounts))
}

/// Removes an account, and an offline account's skin with it.
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

/// Starts a Microsoft sign-in and returns the code the player enters.
#[tauri::command]
async fn microsoft_start() -> Result<auth::DeviceCode, String> {
    auth::start_device_code().await.map_err(|e| format!("{e:#}"))
}

/// Finishes a Microsoft sign-in, saves the account and returns the list.
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

/// Writes out the mod built into the launcher and returns where it is.
fn bundled_mods() -> Result<Vec<PathBuf>, String> {
    let jar = bundled::extract(&paths::root().join("bundled")).map_err(|e| format!("cannot unpack the 霧宿 mod: {e:#}"))?;
    Ok(vec![jar])
}

/// Prepares and starts the game; only one launch at a time.
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
        let mut chosen = state
            .accounts
            .lock()
            .unwrap()
            .get(account)
            .cloned()
            .ok_or("choose an account first")?;
        if matches!(chosen, Account::Microsoft { .. }) {
            chosen = auth::refresh(&chosen).await.map_err(|e| format!("{e:#}"))?;
            let mut accounts = state.accounts.lock().unwrap();
            if let Some(slot) = accounts.get_mut(account) {
                *slot = chosen.clone();
            }
            let _ = auth::save(&accounts);
        }
        let request = launch::Request {
            version,
            preset,
            account: chosen,
            bundled: bundled_mods()?,
        };
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

/// Opens a version's game folder in the file manager.
#[tauri::command]
fn open_instance(version: String) -> Result<(), String> {
    let dir = paths::instance(&launch::instance_id(&version));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new(if cfg!(windows) { "explorer" } else { "xdg-open" })
        .arg(dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// `--selftest-update`: the update without the window, for automated checks. Asks for a newer
/// release, downloads and checks it, and swaps it in for this exe (without starting it), printing
/// each step and the new build's SHA-256.
fn selftest_update() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        println!("this build: {:?} at {:?}", update::BUILD_TAG, update::BUILD_COMMIT);
        let Some(found) = update::check().await else {
            println!("no update offered");
            return Ok(());
        };
        println!("offered: {} at {}", found.tag, found.commit);
        let current = std::env::current_exe()?;
        let new = update::incoming(&current);
        update::download(&found.portable, &new, |done, total| {
            if done == total {
                println!("downloaded {done} bytes");
            }
        })
        .await?;
        update::swap(&current, &new)?;
        println!("swapped in; expected sha256 {}", found.portable.sha256);
        Ok(())
    })
}

/// `--selftest <version> <smooth|normal|ultra> <offline name> [mistinn jar|-] [skin png]`: install
/// and start without the window, printing progress, for automated checks. A skin given here is
/// used as both its wide and slim version.
fn selftest(args: &[String]) -> anyhow::Result<()> {
    let version = args.get(2).cloned().unwrap_or_else(|| launch::MISTINN_VERSION.to_string());
    let preset: Preset = serde_json::from_value(serde_json::json!(args.get(3).cloned().unwrap_or_else(|| "normal".into())))?;
    let account = auth::offline(args.get(4).map_or("Hunter", String::as_str))?;
    // without an explicit jar (or with "-"), use the one built into this binary, as the window does
    let bundled: Vec<PathBuf> = match args.get(5).filter(|a| a.as_str() != "-") {
        Some(jar) => vec![PathBuf::from(jar)],
        None => vec![bundled::extract(&paths::root().join("bundled"))?],
    };
    if let (Some(png), Account::Offline { uuid, .. }) = (args.get(6), &account) {
        let bytes = std::fs::read(png)?;
        skin::save(
            uuid,
            &skin::Skin {
                wide: bytes.clone(),
                slim: bytes,
                slim_arms: false,
            },
        )?;
    }
    let hw = hardware::detect();
    println!("hardware: {hw:?}");
    println!("settings: {:?}", presets::settings(preset, &hw));
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(launch::run(
        launch::Request {
            version,
            preset,
            account,
            bundled,
        },
        &hw,
        |p| {
            if p.done == p.total || p.done % 500 == 0 {
                println!("[{}] {}/{}", p.stage, p.done, p.total);
            }
        },
        |line| println!("{line}"),
    ))
}

/// Asks once, at start, whether a newer launcher is out, and keeps the answer for the update window.
#[tauri::command]
async fn update_check(app: tauri::AppHandle) -> Option<update::Available> {
    let found = update::check().await;
    app.state::<AppState>().update.lock().unwrap().clone_from(&found);
    found
}

/// The newer launcher found at start, for the update window.
#[tauri::command]
fn update_info(app: tauri::State<'_, AppState>) -> Option<update::Available> {
    app.update.lock().unwrap().clone()
}

/// Opens the update window, or brings it forward when it is already open.
#[tauri::command]
fn update_open(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("update") {
        let _ = window.set_focus();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, "update", tauri::WebviewUrl::App("update.html".into()))
        .title("更新霧宿啟動器")
        .inner_size(460.0, 380.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// How far an update has got, sent to the update window.
#[derive(Clone, Serialize)]
struct UpdateProgress {
    /// `download`, `install`, `installer` or `restart`.
    stage: &'static str,
    done: u64,
    total: u64,
}

/// Downloads and installs the newer launcher found at start, then starts it. Not while a game is
/// being started: the two would fight over the launcher.
#[tauri::command]
async fn update_apply(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let available = state.update.lock().unwrap().clone().ok_or("no update is available")?;
    {
        let mut busy = state.launching.lock().unwrap();
        if *busy {
            return Err("the game is starting: update after it has closed".into());
        }
        *busy = true;
    }
    let result = install_update(&app, &available).await;
    if result.is_err() {
        *state.launching.lock().unwrap() = false;
    }
    result.map_err(|e| format!("{e:#}"))
}

/// Puts the new build in place of the running one and starts it. Where the launcher's folder cannot
/// be written (an install for every user), the release's installer is started instead.
async fn install_update(app: &tauri::AppHandle, available: &update::Available) -> anyhow::Result<()> {
    let send = |stage: &'static str, done: u64, total: u64| {
        let _ = app.emit_to("update", "update-progress", UpdateProgress { stage, done, total });
    };
    let current = std::env::current_exe()?;
    let new = update::incoming(&current);
    let writable = std::fs::File::create(&new).is_ok();
    let _ = std::fs::remove_file(&new);
    let next = if writable {
        update::download(&available.portable, &new, |done, total| send("download", done, total)).await?;
        send("install", 0, 0);
        update::swap(&current, &new)?;
        send("restart", 0, 0);
        current
    } else {
        let installer = available
            .installer
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the launcher's folder cannot be written, and the release has no installer"))?;
        let path = std::env::temp_dir().join(&installer.name);
        update::download(installer, &path, |done, total| send("download", done, total)).await?;
        send("installer", 0, 0);
        path
    };
    std::process::Command::new(&next).spawn()?;
    // let the window show the last step before this launcher closes
    tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    app.exit(0);
    Ok(())
}

/// Starts the window, or with `--selftest` installs and starts one game without it.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--selftest") {
        if let Err(e) = selftest(&args) {
            eprintln!("selftest failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--selftest-update") {
        if let Err(e) = selftest_update() {
            eprintln!("update selftest failed: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    // what the last update left behind: the build it replaced, or a download that never finished
    if let Ok(exe) = std::env::current_exe() {
        update::clean_up(&exe);
    }
    tauri::Builder::default()
        .manage(AppState {
            hardware: hardware::detect(),
            accounts: Mutex::new(auth::load()),
            launching: Mutex::new(false),
            update: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            state,
            versions,
            add_offline,
            remove_account,
            set_skin,
            clear_skin,
            microsoft_start,
            microsoft_finish,
            play,
            open_instance,
            update_check,
            update_info,
            update_open,
            update_apply
        ])
        .run(tauri::generate_context!())
        .expect("error while running the launcher");
}
