use crate::db::Db;
use crate::model::{CleanupAction, CleanupPreview, CleanupResult, Graph, Inventory};
use crate::{cleanup, export, graph, scan};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
use tauri::State;

/// Shared application state: the SQLite handle and the configured workspace roots.
pub struct AppState {
    pub db: Mutex<Db>,
    pub roots: Mutex<Vec<PathBuf>>,
}

fn host_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "this-machine".into())
}

fn os_string() -> String {
    #[cfg(target_os = "macos")]
    {
        "macOS".to_string()
    }
    #[cfg(target_os = "windows")]
    {
        "Windows".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::consts::OS.to_string()
    }
}

/// Run a full on-demand scan, persist it, and return the fresh inventory.
#[tauri::command]
pub async fn scan(state: State<'_, AppState>) -> Result<Inventory, String> {
    let roots = { state.roots.lock().unwrap().clone() };
    let started_at = chrono::Utc::now().to_rfc3339();
    let timer = Instant::now();

    let items = scan::run_all(roots).await;

    let finished_at = chrono::Utc::now().to_rfc3339();
    let duration_ms = timer.elapsed().as_millis() as i64;
    let host = host_name();
    let os = os_string();

    let inventory = {
        let mut db = state.db.lock().unwrap();
        db.save_scan(&host, &os, &started_at, &finished_at, duration_ms, &items)
            .map_err(|e| e.to_string())?;
        db.latest_inventory().map_err(|e| e.to_string())?
    };
    inventory.ok_or_else(|| "scan produced no inventory".into())
}

/// Return the most recent inventory without re-scanning (fast app open).
#[tauri::command]
pub async fn get_inventory(state: State<'_, AppState>) -> Result<Option<Inventory>, String> {
    let db = state.db.lock().unwrap();
    db.latest_inventory().map_err(|e| e.to_string())
}

/// Return the architecture graph derived from the latest inventory.
#[tauri::command]
pub async fn get_graph(state: State<'_, AppState>) -> Result<Option<Graph>, String> {
    let db = state.db.lock().unwrap();
    let inv = db.latest_inventory().map_err(|e| e.to_string())?;
    Ok(inv.map(|i| graph::build(&i)))
}

/// Attach or update a note ("why used") on an item; persists across re-scans.
#[tauri::command]
pub async fn set_note(
    state: State<'_, AppState>,
    item_key: String,
    note: String,
    why: String,
) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.set_note(&item_key, &note, &why).map_err(|e| e.to_string())
}

/// Fetch on-demand extra context (description, homepage, latest version,
/// install date) for a single item. Called lazily when an item is selected.
#[tauri::command]
pub async fn enrich_item(
    collector: String,
    name: String,
    source_path: Option<String>,
) -> serde_json::Value {
    let e = scan::enrich::enrich(&collector, &name, source_path).await;
    serde_json::to_value(e).unwrap_or_else(|_| serde_json::json!({}))
}

#[tauri::command]
pub fn list_cleanups() -> Vec<CleanupAction> {
    cleanup::list()
}

#[tauri::command]
pub async fn preview_cleanup(id: String) -> CleanupPreview {
    cleanup::preview(&id).await
}

#[tauri::command]
pub async fn run_cleanup(id: String) -> CleanupResult {
    cleanup::run(&id).await
}

/// Workspace roots currently searched for git repos.
#[tauri::command]
pub fn get_roots(state: State<'_, AppState>) -> Vec<String> {
    state
        .roots
        .lock()
        .unwrap()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
pub fn set_roots(state: State<'_, AppState>, roots: Vec<String>) {
    let mut guard = state.roots.lock().unwrap();
    *guard = roots.into_iter().map(PathBuf::from).collect();
}

/// Generate AGENT_MAP.md from the latest inventory, write it to the user's
/// documents folder, and return { path, content }.
#[tauri::command]
pub async fn export_agent_map(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let inv = {
        let db = state.db.lock().unwrap();
        db.latest_inventory().map_err(|e| e.to_string())?
    };
    let inv = inv.ok_or("no inventory to export — run a scan first")?;
    let content = export::to_agent_map(&inv);
    let dir = dirs::document_dir().unwrap_or_else(scan::util::home);
    let path = dir.join("AGENT_MAP.md");
    std::fs::write(&path, &content).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "path": path.to_string_lossy(),
        "content": content,
    }))
}
