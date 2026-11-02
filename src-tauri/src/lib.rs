mod auth;
mod commands;
mod db;
mod gpu;
mod java;
mod launcher;
mod loaders;
mod modpacks;
mod modrinth_mods;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// Everything the commands share.
pub struct AppState {
    pub db: db::Db,
    pub root: PathBuf,
    pub http: reqwest::Client,
    /// cached Mojang version manifest
    pub versions: tokio::sync::Mutex<Option<Vec<launcher::VersionEntry>>>,
    /// instance id -> stop signal for the running game
    pub running: Mutex<HashMap<i64, tokio::sync::oneshot::Sender<()>>>,
    pub login_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("dev", "sunrise", "Sunrise")
        .map(|p| p.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,sunrise_lib=debug")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let root = data_dir();
            std::fs::create_dir_all(&root)?;
            let db = db::Db::open(&root.join("sunrise.db")).map_err(|e| e.to_string())?;
            let http = reqwest::Client::builder()
                .user_agent("Sunrise-Launcher/0.2")
                .build()
                .map_err(|e| e.to_string())?;
            tracing::info!(path = %root.display(), "data directory");
            app.manage(AppState {
                db,
                root,
                http,
                versions: tokio::sync::Mutex::new(None),
                running: Mutex::new(HashMap::new()),
                login_cancel: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_settings,
            commands::save_settings,
            commands::list_instances,
            commands::create_instance,
            commands::update_instance,
            commands::delete_instance,
            commands::list_mc_versions,
            commands::list_loader_versions,
            commands::detect_java,
            commands::add_custom_java,
            commands::gpu_info,
            commands::play,
            commands::stop,
            commands::account_status,
            commands::account_skin,
            commands::select_account,
            commands::remove_account,
            commands::login_start,
            commands::login_cancel,
            commands::open_folder,
            commands::open_url,
            commands::pick_folder,
            commands::pick_image,
            commands::pick_music_files,
            modpacks::search_modpacks,
            modpacks::modpack_versions,
            modpacks::install_modpack,
            modpacks::pick_modpack_file,
            modpacks::import_modpack_file,
            modrinth_mods::search_modrinth_mods,
            modrinth_mods::install_modrinth_mod,
            modrinth_mods::search_modrinth_resourcepacks,
            modrinth_mods::install_modrinth_resourcepack,
            modrinth_mods::list_resource_packs,
            modrinth_mods::delete_resource_pack,
            commands::read_log,
            commands::list_mods,
            commands::toggle_mod,
            commands::delete_mod,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Sunrise");
}
