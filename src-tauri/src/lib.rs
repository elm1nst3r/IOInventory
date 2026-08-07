mod cleanup;
mod commands;
mod db;
mod export;
mod graph;
mod manage;
mod model;
mod scan;
mod snapshot;

use commands::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    // Auto-updater (desktop only).
    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init());
    }

    builder
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
            commands::set_item_tags,
            commands::enrich_item,
            commands::item_actions,
            commands::run_item_action,
            commands::list_cleanups,
            commands::preview_cleanup,
            commands::run_cleanup,
            commands::get_roots,
            commands::set_roots,
            commands::export_agent_map,
            commands::save_snapshot,
            commands::list_snapshots,
            commands::get_snapshot_inventory,
            commands::get_snapshot_graph,
            commands::delete_snapshot,
            commands::diff_snapshot,
            commands::export_snapshot,
            commands::import_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
