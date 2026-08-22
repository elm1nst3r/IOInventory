use agent_ledger_core::model::{CleanupAction, CleanupPreview, CleanupResult, Graph, Inventory};
use agent_ledger_core::settings::{ScanSourceInfo, Settings};
use agent_ledger_core::state::AppState;
use agent_ledger_core::{cleanup, export, graph, scan};
use std::time::Instant;
use tauri::State;

use agent_ledger_core::scan::util::{host_name, os_name};

/// Run a full on-demand scan, persist it, and return the fresh inventory.
#[tauri::command]
pub async fn scan(state: State<'_, AppState>) -> Result<Inventory, String> {
    // Clone out of the guard before awaiting — never hold a std Mutex across .await.
    let settings = { state.settings.lock().unwrap().clone() };
    let started_at = chrono::Utc::now().to_rfc3339();
    let timer = Instant::now();

    let outcome = scan::run_all(settings.roots(), &settings).await;

    let finished_at = chrono::Utc::now().to_rfc3339();
    let duration_ms = timer.elapsed().as_millis() as i64;
    let host = host_name();
    let os = os_name();

    let inventory = {
        let mut db = state.db.lock().unwrap();
        if outcome.items.is_empty()
            && !outcome.warnings.is_empty()
            && db.latest_inventory().map_err(|e| e.to_string())?.is_some()
        {
            return Err(format!(
                "scan returned no items after {} collector error(s); the previous inventory was preserved",
                outcome.warnings.len()
            ));
        }
        db.save_scan(
            &host,
            &os,
            &started_at,
            &finished_at,
            duration_ms,
            &outcome.items,
            &outcome.warnings,
        )
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

/// What update/uninstall actions are available for an item.
///
/// `source_path` and `cask` come from the item itself: applications aren't
/// managed by a package manager, so what can be done to one depends on where
/// its bundle lives and whether a Homebrew cask owns it.
#[tauri::command]
pub fn item_actions(
    collector: String,
    name: String,
    source_path: Option<String>,
    cask: Option<String>,
) -> agent_ledger_core::manage::ActionInfo {
    agent_ledger_core::manage::info(&collector, &name, source_path.as_deref(), cask.as_deref())
}

/// Run an update or delete action for a single item. `action` is "update" or "delete".
#[tauri::command]
pub async fn run_item_action(
    collector: String,
    name: String,
    action: String,
    source_path: Option<String>,
    cask: Option<String>,
) -> agent_ledger_core::manage::ActionResult {
    agent_ledger_core::manage::run(&collector, &name, &action, source_path.as_deref(), cask.as_deref()).await
}

/// Replace the tags on an item (persisted across re-scans).
#[tauri::command]
pub async fn set_item_tags(
    state: State<'_, AppState>,
    item_key: String,
    tags: Vec<String>,
) -> Result<(), String> {
    let mut db = state.db.lock().unwrap();
    db.set_item_tags(&item_key, &tags).map_err(|e| e.to_string())
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

/// Workspace roots currently searched for git repos (resolved: an empty
/// setting means the auto-detected defaults, and that's what's returned).
#[tauri::command]
pub fn get_roots(state: State<'_, AppState>) -> Vec<String> {
    state
        .settings
        .lock()
        .unwrap()
        .roots()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[tauri::command]
pub fn set_roots(state: State<'_, AppState>, roots: Vec<String>) -> Result<(), String> {
    // Drop the settings guard before taking the db lock — `set_settings` takes
    // them the other way round, and holding both would invert the lock order.
    let roots: Vec<String> = roots
        .into_iter()
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .collect();
    let next = {
        let mut guard = state.settings.lock().unwrap();
        guard.roots = roots;
        guard.clone()
    };
    state
        .db
        .lock()
        .unwrap()
        .save_settings(&next)
        .map_err(|e| e.to_string())
}

// ---- Settings ----

/// The parts of the machine that can be scanned, for the settings UI.
#[tauri::command]
pub fn list_scan_sources() -> Vec<ScanSourceInfo> {
    agent_ledger_core::settings::catalog()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

/// Persist settings and return them as stored (unknown source ids dropped).
#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    let clean = settings.sanitized();
    state
        .db
        .lock()
        .unwrap()
        .save_settings(&clean)
        .map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = clean.clone();
    Ok(clean)
}

/// Where the bundled MCP server binary lives and how to point an agent at it.
#[tauri::command]
pub fn mcp_info() -> agent_ledger_core::mcp::McpInfo {
    agent_ledger_core::mcp::info()
}

// ---- Snapshots ----

use agent_ledger_core::model::{Diff, SnapshotMeta};
use agent_ledger_core::snapshot;

/// Save the current inventory as a named snapshot.
#[tauri::command]
pub async fn save_snapshot(state: State<'_, AppState>, name: String) -> Result<SnapshotMeta, String> {
    let inv = {
        let db = state.db.lock().unwrap();
        db.latest_inventory().map_err(|e| e.to_string())?
    }
    .ok_or("no inventory to snapshot — run a scan first")?;
    let name = if name.trim().is_empty() {
        format!("Snapshot {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))
    } else {
        name.trim().to_string()
    };
    let created_at = chrono::Utc::now().to_rfc3339();
    let db = state.db.lock().unwrap();
    db.save_snapshot(&name, &created_at, &inv.scan.host, &inv.scan.os, "scan", &inv.items)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_snapshots(state: State<'_, AppState>) -> Result<Vec<SnapshotMeta>, String> {
    let db = state.db.lock().unwrap();
    db.list_snapshots().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_snapshot_inventory(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Inventory, String> {
    let (meta, items) = {
        let db = state.db.lock().unwrap();
        db.get_snapshot(id).map_err(|e| e.to_string())?
    }
    .ok_or("snapshot not found")?;
    Ok(snapshot::to_inventory(&meta, items))
}

#[tauri::command]
pub async fn get_snapshot_graph(state: State<'_, AppState>, id: i64) -> Result<Graph, String> {
    let (meta, items) = {
        let db = state.db.lock().unwrap();
        db.get_snapshot(id).map_err(|e| e.to_string())?
    }
    .ok_or("snapshot not found")?;
    Ok(graph::build(&snapshot::to_inventory(&meta, items)))
}

#[tauri::command]
pub async fn delete_snapshot(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let db = state.db.lock().unwrap();
    db.delete_snapshot(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_snapshot(state: State<'_, AppState>, id: i64, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("snapshot name can't be empty".into());
    }
    let db = state.db.lock().unwrap();
    db.rename_snapshot(id, name).map_err(|e| e.to_string())
}

/// Diff a snapshot (base) against another snapshot, or against the current
/// inventory when `target_id` is omitted.
#[tauri::command]
pub async fn diff_snapshot(
    state: State<'_, AppState>,
    id: i64,
    target_id: Option<i64>,
) -> Result<Diff, String> {
    if Some(id) == target_id {
        return Err("pick two different snapshots to compare".into());
    }
    let db = state.db.lock().unwrap();
    let (base_meta, base_items) = db
        .get_snapshot(id)
        .map_err(|e| e.to_string())?
        .ok_or("snapshot not found")?;
    let (target_label, target_items) = match target_id {
        Some(target_id) => {
            let (meta, items) = db
                .get_snapshot(target_id)
                .map_err(|e| e.to_string())?
                .ok_or("comparison snapshot not found")?;
            (snapshot::label(&meta), items)
        }
        None => {
            let current = db
                .latest_inventory()
                .map_err(|e| e.to_string())?
                .ok_or("no current scan to compare")?;
            ("Current scan".to_string(), current.items)
        }
    };
    Ok(snapshot::diff(
        &base_items,
        &target_items,
        &snapshot::label(&base_meta),
        &target_label,
    ))
}

/// Export a snapshot (by id) or the current inventory (id = None) as a
/// portable `.ioinv.json` file in the user's documents folder.
#[tauri::command]
pub async fn export_snapshot(
    state: State<'_, AppState>,
    id: Option<i64>,
) -> Result<serde_json::Value, String> {
    let file = {
        let db = state.db.lock().unwrap();
        match id {
            Some(id) => {
                let (meta, items) = db.get_snapshot(id).map_err(|e| e.to_string())?.ok_or("snapshot not found")?;
                snapshot::SnapshotFile::new(&meta.name, &meta.created_at, &meta.host, &meta.os, items)
            }
            None => {
                let inv = db.latest_inventory().map_err(|e| e.to_string())?.ok_or("no inventory to export")?;
                let name = format!("Current {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
                snapshot::SnapshotFile::new(&name, &chrono::Utc::now().to_rfc3339(), &inv.scan.host, &inv.scan.os, inv.items)
            }
        }
    };
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dir = dirs::document_dir().unwrap_or_else(scan::util::home);
    let path = dir.join(format!("IOInventory-{stamp}.ioinv.json"));
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, &json).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "path": path.to_string_lossy() }))
}

/// Import a snapshot from raw `.ioinv.json` content and store it.
#[tauri::command]
pub async fn import_snapshot(
    state: State<'_, AppState>,
    content: String,
    name: Option<String>,
) -> Result<SnapshotMeta, String> {
    let file = snapshot::parse_import(&content)?;
    let name = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| file.name.clone());
    let created_at = if file.created_at.is_empty() {
        chrono::Utc::now().to_rfc3339()
    } else {
        file.created_at.clone()
    };
    let db = state.db.lock().unwrap();
    db.save_snapshot(&name, &created_at, &file.host, &file.os, "import", &file.items)
        .map_err(|e| e.to_string())
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
