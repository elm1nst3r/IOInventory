mod cleanup;
mod commands;
mod export;
mod graph;
mod manage;
mod model;
mod settings;
mod snapshot;

// Public so the `ioinv-mcp` binary can share the ledger, the scan engine, and
// the MCP server itself with the desktop app.
pub mod db;
pub mod mcp;
pub mod scan;

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
            let settings = db.settings().sanitized();

            app.manage(AppState {
                db: Mutex::new(db),
                settings: Mutex::new(settings),
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
            commands::list_scan_sources,
            commands::get_settings,
            commands::set_settings,
            commands::mcp_info,
            commands::export_agent_map,
            commands::save_snapshot,
            commands::list_snapshots,
            commands::get_snapshot_inventory,
            commands::get_snapshot_graph,
            commands::delete_snapshot,
            commands::rename_snapshot,
            commands::diff_snapshot,
            commands::export_snapshot,
            commands::import_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// The version lives in four manifests and a release tags whatever they say.
/// They drifted once already (the MCP crate was left a minor behind), which
/// ships an app whose sidecar reports the wrong version — so assert it.
#[cfg(test)]
mod version_consistency {
    /// First `"version": "x"` / `version = "x"` in a manifest.
    fn version_in(manifest: &str) -> &str {
        for key in ["\"version\":", "version ="] {
            if let Some(rest) = manifest.split_once(key).map(|(_, r)| r) {
                if let Some(start) = rest.find('"') {
                    let rest = &rest[start + 1..];
                    if let Some(end) = rest.find('"') {
                        return &rest[..end];
                    }
                }
            }
        }
        panic!("no version found in manifest");
    }

    #[test]
    fn all_manifests_agree() {
        let crate_version = env!("CARGO_PKG_VERSION");
        for (label, manifest) in [
            ("package.json", include_str!("../../package.json")),
            ("tauri.conf.json", include_str!("../tauri.conf.json")),
            ("mcp-server/Cargo.toml", include_str!("../mcp-server/Cargo.toml")),
        ] {
            assert_eq!(
                version_in(manifest),
                crate_version,
                "{label} is out of step with src-tauri/Cargo.toml ({crate_version}); \
                 bump all four before cutting a release"
            );
        }
        println!("all_manifests_agree OK — v{crate_version}");
    }
}
