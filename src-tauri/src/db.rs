use crate::model::{Domain, Inventory, Item, ScanInfo};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
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
            "#,
        )?;
        Ok(())
    }

    /// Persist a completed scan and its items. Returns the new scan id.
    pub fn save_scan(
        &mut self,
        host: &str,
        os: &str,
        started_at: &str,
        finished_at: &str,
        duration_ms: i64,
        items: &[Item],
    ) -> Result<i64> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO scans (started_at, finished_at, host, os, duration_ms, item_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![started_at, finished_at, host, os, duration_ms, items.len() as i64],
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
            "SELECT id, started_at, finished_at, host, os, duration_ms, item_count
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
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
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
        _ => Domain::AiAgent,
    }
}
