use crate::model::{Domain, Inventory, Item, ScanInfo, ScanWarning, SnapshotMeta};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

/// Bundle identifier — also the app-data folder name, matching what Tauri's
/// `app_data_dir()` resolves to (`tauri.conf.json` → `identifier`).
const APP_IDENTIFIER: &str = "com.ioinventory.app";

/// Where the ledger lives for processes that aren't the Tauri app itself
/// (currently the `ioinv-mcp` server). Mirrors the path the app derives from
/// Tauri's `app_data_dir()`, with the same `~/.agent-ledger` fallback, so both
/// processes read and write one database.
///
/// `IOINV_DB` overrides it, which is mainly useful for tests.
pub fn default_path() -> PathBuf {
    if let Ok(p) = std::env::var("IOINV_DB") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    let fallback = crate::scan::util::home().join(".agent-ledger").join("ledger.sqlite");
    match dirs::data_dir() {
        Some(dir) => {
            let primary = dir.join(APP_IDENTIFIER).join("ledger.sqlite");
            // Only prefer the legacy location if it's the one that actually has data.
            if !primary.exists() && fallback.exists() {
                fallback
            } else {
                primary
            }
        }
        None => fallback,
    }
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        // The desktop app and the MCP server can both hold the ledger open, so
        // run in WAL mode (readers don't block the writer) and wait instead of
        // failing outright if the other process is mid-write.
        let _: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS scans (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                started_at   TEXT NOT NULL,
                finished_at  TEXT NOT NULL,
                host         TEXT NOT NULL,
                os           TEXT NOT NULL,
                duration_ms  INTEGER NOT NULL,
                item_count   INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS items (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_id       INTEGER NOT NULL,
                item_key      TEXT NOT NULL,
                domain        TEXT NOT NULL,
                collector     TEXT NOT NULL,
                name          TEXT NOT NULL,
                version       TEXT,
                source_path   TEXT,
                size_bytes    INTEGER,
                metadata_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS idx_items_scan ON items(scan_id);
            CREATE TABLE IF NOT EXISTS notes (
                item_key   TEXT PRIMARY KEY,
                note       TEXT,
                why        TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tags (
                item_key TEXT NOT NULL,
                tag      TEXT NOT NULL,
                PRIMARY KEY (item_key, tag)
            );
            CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS snapshots (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL,
                created_at TEXT NOT NULL,
                host       TEXT NOT NULL,
                os         TEXT NOT NULL,
                item_count INTEGER NOT NULL,
                source     TEXT NOT NULL,
                data_json  TEXT NOT NULL
            );
            "#,
        )?;
        // Added in v0.10.x. SQLite has no `ADD COLUMN IF NOT EXISTS`, so an
        // already-migrated database legitimately returns a duplicate-column error.
        let _ = self.conn.execute(
            "ALTER TABLE scans ADD COLUMN warnings_json TEXT NOT NULL DEFAULT '[]'",
            [],
        );
        Ok(())
    }

    /// Persist a completed scan and its items. Returns the new scan id.
    #[allow(clippy::too_many_arguments)]
    pub fn save_scan(
        &mut self,
        host: &str,
        os: &str,
        started_at: &str,
        finished_at: &str,
        duration_ms: i64,
        items: &[Item],
        warnings: &[ScanWarning],
    ) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let warnings_json = serde_json::to_string(warnings)?;
        tx.execute(
            "INSERT INTO scans
             (started_at, finished_at, host, os, duration_ms, item_count, warnings_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                started_at,
                finished_at,
                host,
                os,
                duration_ms,
                items.len() as i64,
                warnings_json
            ],
        )?;
        let scan_id = tx.last_insert_rowid();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO items
                 (scan_id, item_key, domain, collector, name, version, source_path, size_bytes, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for it in items {
                stmt.execute(params![
                    scan_id,
                    it.item_key,
                    it.domain.as_str(),
                    it.collector,
                    it.name,
                    it.version,
                    it.source_path,
                    it.size_bytes,
                    it.metadata.to_string(),
                ])?;
            }
        }
        tx.commit()?;
        // Keep the history small: retain the 10 most recent scans.
        self.prune_old_scans(10).ok();
        Ok(scan_id)
    }

    fn prune_old_scans(&self, keep: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM items WHERE scan_id NOT IN
                (SELECT id FROM scans ORDER BY id DESC LIMIT ?1)",
            params![keep],
        )?;
        self.conn.execute(
            "DELETE FROM scans WHERE id NOT IN
                (SELECT id FROM scans ORDER BY id DESC LIMIT ?1)",
            params![keep],
        )?;
        Ok(())
    }

    pub fn latest_scan_id(&self) -> Option<i64> {
        self.conn
            .query_row("SELECT id FROM scans ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .ok()
    }

    pub fn latest_inventory(&self) -> Result<Option<Inventory>> {
        let Some(scan_id) = self.latest_scan_id() else {
            return Ok(None);
        };
        let scan = self.conn.query_row(
            "SELECT id, started_at, finished_at, host, os, duration_ms, item_count, warnings_json
             FROM scans WHERE id = ?1",
            params![scan_id],
            |r| {
                Ok(ScanInfo {
                    id: r.get(0)?,
                    started_at: r.get(1)?,
                    finished_at: r.get(2)?,
                    host: r.get(3)?,
                    os: r.get(4)?,
                    duration_ms: r.get(5)?,
                    item_count: r.get(6)?,
                    warnings: r
                        .get::<_, String>(7)
                        .ok()
                        .and_then(|json| serde_json::from_str(&json).ok())
                        .unwrap_or_default(),
                })
            },
        )?;
        let items = self.items_for_scan(scan_id)?;
        Ok(Some(Inventory { scan, items }))
    }

    fn items_for_scan(&self, scan_id: i64) -> Result<Vec<Item>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.item_key, i.domain, i.collector, i.name, i.version, i.source_path,
                    i.size_bytes, i.metadata_json, n.note, n.why
             FROM items i
             LEFT JOIN notes n ON n.item_key = i.item_key
             WHERE i.scan_id = ?1
             ORDER BY i.domain, i.collector, i.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![scan_id], |r| {
            let domain_str: String = r.get(1)?;
            let meta_str: String = r.get(7)?;
            Ok(Item {
                item_key: r.get(0)?,
                domain: parse_domain(&domain_str),
                collector: r.get(2)?,
                name: r.get(3)?,
                version: r.get(4)?,
                source_path: r.get(5)?,
                size_bytes: r.get(6)?,
                metadata: serde_json::from_str(&meta_str).unwrap_or_else(|_| serde_json::json!({})),
                note: r.get(8)?,
                why: r.get(9)?,
                tags: Vec::new(),
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        // Attach tags from the tags table.
        let tag_map = self.all_tags()?;
        for it in items.iter_mut() {
            if let Some(tags) = tag_map.get(&it.item_key) {
                it.tags = tags.clone();
            }
        }
        Ok(items)
    }

    /// item_key -> sorted list of tags.
    fn all_tags(&self) -> Result<std::collections::HashMap<String, Vec<String>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT item_key, tag FROM tags ORDER BY tag COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for row in rows {
            let (key, tag) = row?;
            map.entry(key).or_default().push(tag);
        }
        Ok(map)
    }

    /// Replace all tags for an item with the given set.
    ///
    /// Wrapped in a transaction: this clears before it inserts, so without one
    /// a failure part-way through would drop the old tags and not write the new
    /// ones. Tags are the only data here a re-scan can't reproduce.
    pub fn set_item_tags(&mut self, item_key: &str, tags: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM tags WHERE item_key = ?1", params![item_key])?;
        let mut seen = std::collections::HashSet::new();
        for tag in tags {
            let tag = tag.trim();
            if tag.is_empty() || !seen.insert(tag.to_lowercase()) {
                continue;
            }
            tx.execute(
                "INSERT OR IGNORE INTO tags (item_key, tag) VALUES (?1, ?2)",
                params![item_key, tag],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // ---- Snapshots ----

    pub fn save_snapshot(
        &self,
        name: &str,
        created_at: &str,
        host: &str,
        os: &str,
        source: &str,
        items: &[Item],
    ) -> Result<SnapshotMeta> {
        let data = serde_json::to_string(items)?;
        self.conn.execute(
            "INSERT INTO snapshots (name, created_at, host, os, item_count, source, data_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![name, created_at, host, os, items.len() as i64, source, data],
        )?;
        Ok(SnapshotMeta {
            id: self.conn.last_insert_rowid(),
            name: name.into(),
            created_at: created_at.into(),
            host: host.into(),
            os: os.into(),
            item_count: items.len() as i64,
            source: source.into(),
        })
    }

    pub fn list_snapshots(&self) -> Result<Vec<SnapshotMeta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, created_at, host, os, item_count, source
             FROM snapshots ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SnapshotMeta {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
                host: r.get(3)?,
                os: r.get(4)?,
                item_count: r.get(5)?,
                source: r.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_snapshot(&self, id: i64) -> Result<Option<(SnapshotMeta, Vec<Item>)>> {
        let row = self.conn.query_row(
            "SELECT id, name, created_at, host, os, item_count, source, data_json
             FROM snapshots WHERE id = ?1",
            params![id],
            |r| {
                let meta = SnapshotMeta {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    host: r.get(3)?,
                    os: r.get(4)?,
                    item_count: r.get(5)?,
                    source: r.get(6)?,
                };
                let data: String = r.get(7)?;
                Ok((meta, data))
            },
        );
        match row {
            Ok((meta, data)) => {
                let items: Vec<Item> = serde_json::from_str(&data).unwrap_or_default();
                Ok(Some((meta, items)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete_snapshot(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM snapshots WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- Settings ----

    /// Read the persisted settings. A missing or unreadable row falls back to
    /// defaults (scan everything) rather than failing the app's startup.
    pub fn settings(&self) -> crate::settings::Settings {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![crate::settings::SETTINGS_KEY],
                |r| r.get(0),
            )
            .ok();
        raw.and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &crate::settings::Settings) -> Result<()> {
        let json = serde_json::to_string(s)?;
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![crate::settings::SETTINGS_KEY, json],
        )?;
        Ok(())
    }

    pub fn set_note(&self, item_key: &str, note: &str, why: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO notes (item_key, note, why, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(item_key) DO UPDATE SET note=?2, why=?3, updated_at=?4",
            params![item_key, note, why, now],
        )?;
        Ok(())
    }
}

fn parse_domain(s: &str) -> Domain {
    match s {
        "package_manager" => Domain::PackageManager,
        "runtime" => Domain::Runtime,
        "project" => Domain::Project,
        "container" => Domain::Container,
        "application" => Domain::Application,
        _ => Domain::AiAgent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Domain, Item};

    #[test]
    fn tags_persist_and_export() {
        let path = std::env::temp_dir().join(format!("io-inv-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut db = Db::open(&path).unwrap();

        let ripgrep = Item::new(Domain::PackageManager, "homebrew", "ripgrep").version("14.1.0");
        let jq = Item::new(Domain::PackageManager, "homebrew", "jq");
        let key = ripgrep.item_key.clone();
        let warnings = vec![ScanWarning {
            source: "docker".into(),
            message: "daemon unavailable".into(),
        }];
        db.save_scan(
            "host",
            "macOS",
            "t0",
            "t1",
            100,
            &[ripgrep, jq],
            &warnings,
        )
        .unwrap();

        // Assign tags, then read back.
        db.set_item_tags(&key, &["favorite".into(), "cli".into()]).unwrap();
        let inv = db.latest_inventory().unwrap().unwrap();
        assert_eq!(inv.scan.warnings.len(), 1);
        assert_eq!(inv.scan.warnings[0].source, "docker");
        let tagged = inv.items.iter().find(|i| i.item_key == key).unwrap();
        assert_eq!(tagged.tags, vec!["cli".to_string(), "favorite".to_string()]);

        // Export includes a Tagged Views section.
        let md = crate::export::to_agent_map(&inv);
        assert!(md.contains("## Tagged Views"), "export missing Tagged Views:\n{md}");
        assert!(md.contains("#favorite"), "export missing #favorite tag");
        assert!(md.contains("**ripgrep**"), "export missing item");

        // Replacing tags removes old ones.
        db.set_item_tags(&key, &["favorite".into()]).unwrap();
        let inv2 = db.latest_inventory().unwrap().unwrap();
        let t2 = inv2.items.iter().find(|i| i.item_key == key).unwrap();
        assert_eq!(t2.tags, vec!["favorite".to_string()]);

        let _ = std::fs::remove_file(&path);
        println!("tags_persist_and_export OK");
    }
}
