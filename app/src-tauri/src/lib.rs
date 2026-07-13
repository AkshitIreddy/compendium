mod commands;
pub mod engine;
pub mod error;

use std::path::PathBuf;

use parking_lot::{Mutex, RwLock};
use tauri::{Emitter, Manager};

use commands::AppState;
use engine::cohere::CohereClient;
use engine::{appdb, keys, pack};

/// Resolve the packs directory: dev override -> bundled resources.
fn packs_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("COMPENDIUM_PACKS_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    app.path()
        .resolve("packs", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.is_dir())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let conn = appdb::open(&data_dir)?;

            let cohere = CohereClient::new();
            if let Ok(Some(key)) = keys::read_key() {
                cohere.set_key(Some(key));
            }

            app.manage(AppState {
                packs: RwLock::new(Vec::new()),
                cohere,
                appdb: Mutex::new(conn),
            });

            // Pack loading happens off the main thread so the window shows
            // immediately; the UI listens for `packs-loaded`.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let state = handle.state::<AppState>();
                let mut loaded = Vec::new();
                if let Some(dir) = packs_dir(&handle) {
                    for path in pack::discover_packs(&dir) {
                        match pack::load_pack(&path) {
                            Ok(p) => {
                                let _ = appdb::register_pack(
                                    &state.appdb.lock(),
                                    &p.manifest.pack_id,
                                    &p.manifest.pack_version,
                                    &path.to_string_lossy(),
                                );
                                loaded.push(p);
                            }
                            Err(e) => {
                                eprintln!("failed to load pack {}: {e}", path.display());
                                let _ = handle.emit("pack-load-error", format!("{}: {e}", path.display()));
                            }
                        }
                    }
                }
                let infos: Vec<_> = loaded
                    .iter()
                    .map(|p| serde_json::json!({
                        "pack_id": p.manifest.pack_id,
                        "name": p.manifest.name,
                        "healed": p.healed,
                    }))
                    .collect();
                *state.packs.write() = loaded;
                let _ = handle.emit("packs-loaded", infos);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::packs_list,
            commands::key_set,
            commands::key_status,
            commands::key_delete,
            commands::search_query,
            commands::document_get,
            commands::technique_get,
            commands::settings_get_all,
            commands::settings_set,
            commands::quota_get,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
