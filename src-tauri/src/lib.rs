mod cleanup;
mod commands;
mod db;
mod export;
mod graph;
mod model;
mod scan;

use commands::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Store the SQLite ledger in the app's data directory.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| scan::util::home().join(".agent-ledger"));
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("ledger.sqlite");
            let db = db::Db::open(&db_path).expect("failed to open ledger database");

            app.manage(AppState {
                db: Mutex::new(db),
                roots: Mutex::new(scan::default_roots()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::get_inventory,
            commands::get_graph,
            commands::set_note,
            commands::enrich_item,
            commands::list_cleanups,
            commands::preview_cleanup,
            commands::run_cleanup,
            commands::get_roots,
            commands::set_roots,
            commands::export_agent_map,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
