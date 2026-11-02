//! Tauri commands (the bridge to the Svelte UI) + the play task.

use crate::{
    auth::{self, AccountInfo, AuthInfo},
    db::{Db, Instance, NewInstance, Settings},
    gpu::{self, Gpu},
    java::{self, JavaInfo},
    launcher::{self, Reporter, VersionEntry},
    loaders::{self, LoaderKind},
    AppState,
};
use anyhow::{anyhow, bail};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;
use tracing::{info, warn};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn client_id(s: &Settings) -> String {
    std::env::var("SUNRISE_MS_CLIENT_ID")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| s.ms_client_id.clone())
        .trim()
        .to_string()
}

// ───────────────────────── events ─────────────────────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GameState {
    id: i64,
    /// installing | running | idle | error
    state: String,
    message: String,
    need_java: Option<u32>,
}

pub(crate) fn emit_state(
    app: &AppHandle,
    id: i64,
    state: &str,
    message: &str,
    need_java: Option<u32>,
) {
    let _ = app.emit(
        "game-state",
        GameState {
            id,
            state: state.into(),
            message: message.into(),
            need_java,
        },
    );
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LoginEvent {
    /// waiting | done | error
    status: String,
    message: String,
    account: Option<AccountInfo>,
}

fn emit_login(app: &AppHandle, status: &str, message: &str, account: Option<AccountInfo>) {
    let _ = app.emit(
        "login",
        LoginEvent {
            status: status.into(),
            message: message.into(),
            account,
        },
    );
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsSnapshot {
    accounts: Vec<AccountInfo>,
    active_account_id: Option<i64>,
}

fn accounts_snapshot(db: &Db) -> Result<AccountsSnapshot, String> {
    let accounts = db.accounts().map_err(err)?;
    let active_account_id = db.account().map(|a| a.id);
    Ok(AccountsSnapshot {
        accounts: accounts.iter().map(AccountInfo::from).collect(),
        active_account_id,
    })
}

// ───────────────────────── app / settings ─────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    root: String,
    version: String,
    /// default parent folder for instances
    instances_dir: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        root: state.root.to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").into(),
        instances_dir: state.root.join("instances").to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.db.settings()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    state.db.save_settings(&settings).map_err(err)
}

// ───────────────────────── instances ─────────────────────────

#[tauri::command]
pub fn list_instances(state: State<'_, AppState>) -> Result<Vec<Instance>, String> {
    state.db.instances().map_err(err)
}

/// `parent/<name>`, or `parent/<name>-2`, … if that folder already has files in it.
pub(crate) fn unique_dir(parent: &Path, name: &str) -> PathBuf {
    let mut slug: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    slug = slug
        .trim_matches(|c| c == ' ' || c == '.')
        .chars()
        .take(48)
        .collect();
    if slug.is_empty() {
        slug = "instance".into();
    }
    let mut p = parent.join(&slug);
    let mut n = 2;
    while p.exists()
        && std::fs::read_dir(&p)
            .map(|mut d| d.next().is_some())
            .unwrap_or(true)
    {
        p = parent.join(format!("{slug}-{n}"));
        n += 1;
    }
    p
}

/// Shared by "New instance" and the modpack installer.
pub(crate) fn create_instance_inner(
    state: &AppState,
    input: &NewInstance,
) -> Result<Instance, String> {
    if input.name.trim().is_empty() {
        return Err("Give the instance a name".into());
    }
    if input.mc_version.trim().is_empty() {
        return Err("Pick a Minecraft version".into());
    }
    // where to keep it: folder chosen in the dialog > Settings default > built-in <data>/instances/<id>
    let parent = if !input.dir_parent.trim().is_empty() {
        input.dir_parent.trim().to_string()
    } else {
        state.db.settings().instances_dir.trim().to_string()
    };
    let dir = if parent.is_empty() {
        String::new()
    } else {
        let d = unique_dir(Path::new(&parent), input.name.trim());
        std::fs::create_dir_all(&d).map_err(|e| format!("Can't create {}: {e}", d.display()))?;
        d.to_string_lossy().to_string()
    };
    let inst = state.db.insert_instance(input, &dir).map_err(err)?;
    info!(id = inst.id, name = %inst.name, dir = %inst.dir, "instance created");
    Ok(inst)
}

#[tauri::command]
pub fn create_instance(state: State<'_, AppState>, input: NewInstance) -> Result<Instance, String> {
    create_instance_inner(&state, &input)
}

#[tauri::command]
pub fn update_instance(state: State<'_, AppState>, instance: Instance) -> Result<(), String> {
    if instance.name.trim().is_empty() {
        return Err("The name can't be empty".into());
    }
    state.db.update_instance(&instance).map_err(err)
}

#[tauri::command]
pub fn delete_instance(
    state: State<'_, AppState>,
    id: i64,
    delete_files: bool,
) -> Result<(), String> {
    if state.running.lock().unwrap().contains_key(&id) {
        return Err("Stop the game first".into());
    }
    let inst = state.db.instance(id).map_err(err)?;
    state.db.delete_instance(id).map_err(err)?;
    if delete_files {
        let dir = launcher::game_dir(&state.root, &inst);
        // never remove a filesystem root
        if dir.parent().is_some() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    Ok(())
}

// ───────────────────────── versions / java / gpu ─────────────────────────

async fn manifest(state: &AppState) -> anyhow::Result<Vec<VersionEntry>> {
    let mut g = state.versions.lock().await;
    if g.is_none() {
        *g = Some(launcher::fetch_versions(&state.http).await?);
    }
    Ok(g.clone().unwrap_or_default())
}

#[tauri::command]
pub async fn list_mc_versions(state: State<'_, AppState>) -> Result<Vec<VersionEntry>, String> {
    manifest(&state).await.map_err(err)
}

#[tauri::command]
pub async fn list_loader_versions(
    state: State<'_, AppState>,
    kind: String,
    mc: String,
) -> Result<Vec<String>, String> {
    loaders::versions(&state.http, LoaderKind::from_key(&kind), &mc)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn detect_java(state: State<'_, AppState>) -> Result<Vec<JavaInfo>, String> {
    let s = state.db.settings();
    Ok(java::find_all(&s.custom_java).await)
}

#[tauri::command]
pub async fn add_custom_java(state: State<'_, AppState>, path: String) -> Result<JavaInfo, String> {
    let info = java::probe_custom(&path)
        .await
        .ok_or("That path isn't a working Java")?;
    let mut s = state.db.settings();
    let raw = path.trim().to_string();
    if !s.custom_java.contains(&raw) {
        s.custom_java.push(raw);
        state.db.save_settings(&s).map_err(err)?;
    }
    Ok(info)
}

#[tauri::command]
pub fn gpu_info() -> Vec<Gpu> {
    gpu::detect()
}

// ───────────────────────── play / stop ─────────────────────────

#[tauri::command]
pub async fn play(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let inst = state.db.instance(id).map_err(err)?;
    let (tx, rx) = oneshot::channel::<()>();
    {
        let mut run = state.running.lock().unwrap();
        if run.contains_key(&id) {
            return Err("This instance is already running".into());
        }
        run.insert(id, tx);
    }
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        run_game(app2, inst, rx).await;
    });
    Ok(())
}

#[tauri::command]
pub fn stop(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    if let Some(tx) = state.running.lock().unwrap().remove(&id) {
        let _ = tx.send(());
    }
    Ok(())
}

async fn run_game(app: AppHandle, inst: Instance, mut rx: oneshot::Receiver<()>) {
    let id = inst.id;
    let state = app.state::<AppState>();
    emit_state(&app, id, "installing", "", None);

    let result = run_inner(&app, state.inner(), &inst, &mut rx).await;
    state.running.lock().unwrap().remove(&id);

    match result {
        Ok(()) => emit_state(&app, id, "idle", "Game closed", None),
        Err(e) => {
            let need = e.downcast_ref::<launcher::NeedJava>().map(|n| n.major);
            warn!(instance = id, error = %e, "play failed");
            emit_state(&app, id, "error", &e.to_string(), need);
        }
    }
}

async fn run_inner(
    app: &AppHandle,
    state: &AppState,
    inst: &Instance,
    rx: &mut oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let settings = state.db.settings();
    let rep = Reporter::new(app.clone(), inst.id);
    rep.set("Preparing…", 0.0);

    // Signed-in account if present, otherwise offline with the chosen username.
    let auth_info: AuthInfo = match state.db.account() {
        Some(acc) => {
            if acc.provider == "microsoft" {
                let cid = client_id(&settings);
                if cid.is_empty() {
                    bail!("Enter your Azure client ID in Settings to use your Microsoft account");
                }
                rep.set("Signing in with Microsoft…", 0.01);
                let (info, updated) = auth::refresh(&state.http, &cid, &acc).await?;
                state.db.save_account(&updated)?;
                info
            } else {
                rep.set(format!("Signing in with {}…", acc.provider), 0.01);
                let (info, updated) = auth::refresh_yggdrasil(&state.http, &acc).await?;
                state.db.save_account(&updated)?;
                info
            }
        }
        None => AuthInfo::offline(&settings.username),
    };

    let entry = manifest(state)
        .await?
        .into_iter()
        .find(|v| v.id == inst.mc_version)
        .ok_or_else(|| anyhow!("Minecraft {} is not in the version list", inst.mc_version))?;

    let kind = LoaderKind::from_key(&inst.loader);
    if kind != LoaderKind::Vanilla {
        rep.set(format!("Looking up {} builds…", kind.label()), 0.02);
    }
    let loader =
        loaders::resolve(&state.http, kind, &inst.mc_version, &inst.loader_version).await?;

    let javas = java::find_all(&settings.custom_java).await;
    let pref = if inst.java_path.is_empty() {
        settings.java_path.clone()
    } else {
        inst.java_path.clone()
    };

    let ins = launcher::install(
        &state.http,
        &entry,
        &loader,
        &state.root,
        &rep,
        &javas,
        &pref,
    )
    .await?;
    let authlib = if let Some(api_root) = auth_info.api_root.as_deref() {
        rep.set("Preparing external authentication…", 0.98);
        Some(launcher::authlib_config(&state.http, &state.root, api_root).await?)
    } else {
        None
    };
    rep.set(format!("Starting Minecraft {}…", ins.display), 1.0);

    let mut child = launcher::launch(
        &ins,
        inst,
        &settings,
        &state.root,
        &auth_info,
        authlib.as_ref(),
    )?;
    state.db.touch_played(inst.id)?;
    emit_state(app, inst.id, "running", "", None);

    let mut stopped = false;
    let status = tokio::select! {
        s = child.wait() => Some(s),
        _ = &mut *rx => {
            stopped = true;
            let _ = child.start_kill();
            let _ = child.wait().await;
            None
        }
    };
    if let Some(Ok(s)) = status {
        info!(instance = inst.id, status = %s, "Minecraft exited");
        if !s.success() && !stopped {
            bail!("Minecraft exited with {s} – see the Logs tab");
        }
    }
    Ok(())
}

// ───────────────────────── Microsoft account ─────────────────────────

#[tauri::command]
pub fn account_status(state: State<'_, AppState>) -> Result<AccountsSnapshot, String> {
    accounts_snapshot(&state.db)
}

#[tauri::command]
pub async fn account_skin(state: State<'_, AppState>, id: i64) -> Result<Option<String>, String> {
    let Some(mut account) = state
        .db
        .accounts()
        .map_err(err)?
        .into_iter()
        .find(|a| a.id == id)
    else {
        return Ok(None);
    };
    if account.skin_url.is_empty() {
        if let Ok(Some(url)) =
            auth::profile_skin(&state.http, &account.provider, &account.uuid).await
        {
            account.skin_url = url;
            state.db.save_account(&account).map_err(err)?;
        }
    }
    Ok((!account.skin_url.is_empty()).then_some(account.skin_url))
}

#[tauri::command]
pub fn select_account(state: State<'_, AppState>, id: i64) -> Result<AccountsSnapshot, String> {
    state.db.set_active_account(Some(id)).map_err(err)?;
    accounts_snapshot(&state.db)
}

#[tauri::command]
pub fn remove_account(state: State<'_, AppState>, id: i64) -> Result<AccountsSnapshot, String> {
    state.db.delete_account(id).map_err(err)?;
    accounts_snapshot(&state.db)
}

#[tauri::command]
pub fn login_start(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
    username: String,
    password: String,
    totp: String,
) -> Result<(), String> {
    let cid = if provider == "microsoft" {
        let cid = client_id(&state.db.settings());
        if cid.is_empty() {
            return Err("Enter your Azure client ID in Settings first (see the README)".into());
        }
        Some(cid)
    } else {
        if !["elyby", "littleskin"].contains(&provider.as_str()) {
            return Err("Unknown sign-in provider".into());
        }
        if username.trim().is_empty() || password.is_empty() {
            return Err("Enter your account name and password".into());
        }
        None
    };
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(old) = state.login_cancel.lock().unwrap().replace(cancel.clone()) {
        old.store(true, Ordering::Relaxed); // stop any previous attempt
    }
    emit_login(&app, "waiting", "", None);

    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app2.state::<AppState>();
        let opener_app = app2.clone();
        let open = move |url: String| -> anyhow::Result<()> {
            opener_app
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|e| anyhow!(e.to_string()))
        };
        let result = if provider == "microsoft" {
            auth::interactive_login(
                &state.http,
                cid.as_deref().unwrap_or_default(),
                open,
                cancel.clone(),
            )
            .await
        } else {
            auth::yggdrasil_login(&state.http, &provider, &username, &password, &totp).await
        };
        if cancel.load(Ordering::Relaxed) {
            emit_login(&app2, "idle", "", None);
            return;
        }
        match result {
            Ok(acc) => {
                match state.db.save_account(&acc).and_then(|saved| {
                    state.db.set_active_account(Some(saved.id))?;
                    Ok(saved)
                }) {
                    Ok(saved) => emit_login(&app2, "done", "", Some(AccountInfo::from(&saved))),
                    Err(e) => emit_login(&app2, "error", &e.to_string(), None),
                }
            }
            Err(e) => {
                if cancel.load(Ordering::Relaxed) {
                    emit_login(&app2, "idle", "", None);
                } else {
                    warn!(provider = %provider, error = %e, "account sign-in failed");
                    emit_login(&app2, "error", &e.to_string(), None);
                }
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn login_cancel(state: State<'_, AppState>) {
    if let Some(c) = state.login_cancel.lock().unwrap().as_ref() {
        c.store(true, Ordering::Relaxed);
    }
}

// ───────────────────────── files: folders, logs, mods ─────────────────────────

fn safe_name(name: &str) -> Result<&str, String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        Err("Invalid file name".into())
    } else {
        Ok(name)
    }
}

fn inst_dir(state: &AppState, id: i64) -> Result<PathBuf, String> {
    let inst = state.db.instance(id).map_err(err)?;
    Ok(launcher::game_dir(&state.root, &inst))
}

fn mods_dir(state: &AppState, id: i64) -> Result<PathBuf, String> {
    Ok(inst_dir(state, id)?.join("mods"))
}

/// kind: "game" | "mods"
#[tauri::command]
pub fn open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    kind: String,
) -> Result<(), String> {
    let dir = if kind == "mods" {
        mods_dir(&state, id)?
    } else if kind == "resourcepacks" {
        inst_dir(&state, id)?.join("resourcepacks")
    } else {
        inst_dir(&state, id)?
    };
    std::fs::create_dir_all(&dir).map_err(err)?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Only https links can be opened".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}

#[tauri::command]
pub fn read_log(state: State<'_, AppState>, id: i64) -> String {
    let Ok(dir) = inst_dir(&state, id) else {
        return String::new();
    };
    let path = dir.join("sunrise-latest.log");
    let txt = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = txt.lines().collect();
    lines[lines.len().saturating_sub(300)..].join("\n")
}

#[derive(Serialize)]
pub struct ModFile {
    name: String,
    size: u64,
    enabled: bool,
}

#[tauri::command]
pub fn list_mods(state: State<'_, AppState>, id: i64) -> Vec<ModFile> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(mods_dir(&state, id).unwrap_or_default()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !(name.ends_with(".jar") || name.ends_with(".jar.disabled")) {
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(ModFile {
                enabled: !name.ends_with(".disabled"),
                name,
                size,
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Enable/disable a mod by renaming `x.jar` <-> `x.jar.disabled`.
#[tauri::command]
pub fn toggle_mod(state: State<'_, AppState>, id: i64, name: String) -> Result<(), String> {
    let name = safe_name(&name)?;
    let dir = mods_dir(&state, id)?;
    let new = match name.strip_suffix(".disabled") {
        Some(base) => base.to_string(),
        None => format!("{name}.disabled"),
    };
    std::fs::rename(dir.join(name), dir.join(new)).map_err(err)
}

#[tauri::command]
pub fn delete_mod(state: State<'_, AppState>, id: i64, name: String) -> Result<(), String> {
    let name = safe_name(&name)?;
    let path: &Path = &mods_dir(&state, id)?.join(name);
    std::fs::remove_file(path).map_err(err)
}

/// Native folder picker. Runs on a blocking thread (the dialog must not block the UI thread).
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose a folder")
            .blocking_pick_folder()
    })
    .await
    .ok()
    .flatten()
    .and_then(|p| p.into_path().ok())
    .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn pick_image(app: AppHandle) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Choose an image")
            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
            .blocking_pick_file()
    }).await.ok().flatten().and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn pick_music_files(app: AppHandle) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title("Add music")
            .add_filter("Audio", &["mp3", "ogg", "wav", "flac", "m4a", "aac"])
            .blocking_pick_files()
    }).await.ok().flatten().unwrap_or_default().into_iter()
        .filter_map(|p| p.into_path().ok().map(|p| p.to_string_lossy().to_string())).collect()
}
