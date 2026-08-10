use crate::model::{Diff, DiffChange, DiffItem, Inventory, Item, ScanInfo, SnapshotMeta};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const FORMAT: &str = "io-inventory-snapshot";
pub const FORMAT_VERSION: u32 = 1;

/// On-disk snapshot format (`.ioinv.json`). A superset of an inventory plus a
/// format header so imports can be validated.
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub created_at: String,
    pub host: String,
    pub os: String,
    pub item_count: i64,
    pub items: Vec<Item>,
}

impl SnapshotFile {
    pub fn new(name: &str, created_at: &str, host: &str, os: &str, items: Vec<Item>) -> Self {
        SnapshotFile {
            format: FORMAT.into(),
            version: FORMAT_VERSION,
            name: name.into(),
            created_at: created_at.into(),
            host: host.into(),
            os: os.into(),
            item_count: items.len() as i64,
            items,
        }
    }
}

/// Parse a `.ioinv.json` file. Lenient: accepts anything with an `items` array,
/// but rejects a clearly-wrong format string when one is present.
pub fn parse_import(content: &str) -> Result<SnapshotFile, String> {
    let v: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("not valid JSON: {e}"))?;
    if let Some(fmt) = v.get("format").and_then(|f| f.as_str()) {
        if fmt != FORMAT {
            return Err(format!("unrecognized snapshot format: {fmt}"));
        }
    }
    let items: Vec<Item> = serde_json::from_value(
        v.get("items").cloned().unwrap_or(serde_json::Value::Null),
    )
    .map_err(|e| format!("could not read items: {e}"))?;
    if items.is_empty() {
        return Err("file contains no items".into());
    }
    let name = v.get("name").and_then(|s| s.as_str()).unwrap_or("Imported snapshot").to_string();
    let created_at = v
        .get("created_at")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let host = v.get("host").and_then(|s| s.as_str()).unwrap_or("imported").to_string();
    let os = v.get("os").and_then(|s| s.as_str()).unwrap_or("").to_string();
    Ok(SnapshotFile {
        format: FORMAT.into(),
        version: FORMAT_VERSION,
        name,
        created_at,
        host,
        os,
        item_count: items.len() as i64,
        items,
    })
}

/// Reconstruct an Inventory from a stored snapshot so it can feed the graph/list.
pub fn to_inventory(meta: &SnapshotMeta, items: Vec<Item>) -> Inventory {
    Inventory {
        scan: ScanInfo {
            id: meta.id,
            started_at: meta.created_at.clone(),
            finished_at: meta.created_at.clone(),
            host: meta.host.clone(),
            os: meta.os.clone(),
            duration_ms: 0,
            item_count: meta.item_count,
            warnings: Vec::new(),
        },
        items,
    }
}

/// Diff two item sets, keyed by stable `item_key`. `base` is the older set
/// (e.g. a snapshot), `target` is the newer (e.g. the current scan).
pub fn diff(base: &[Item], target: &[Item], base_label: &str, target_label: &str) -> Diff {
    let base_map: HashMap<&str, &Item> = base.iter().map(|i| (i.item_key.as_str(), i)).collect();
    let target_map: HashMap<&str, &Item> =
        target.iter().map(|i| (i.item_key.as_str(), i)).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged = 0i64;

    for it in target {
        match base_map.get(it.item_key.as_str()) {
            None => added.push(diff_item(it)),
            Some(old) => {
                if old.version != it.version {
                    changed.push(DiffChange {
                        name: it.name.clone(),
                        collector: it.collector.clone(),
                        domain: it.domain,
                        old_version: old.version.clone(),
                        new_version: it.version.clone(),
                    });
                } else {
                    unchanged += 1;
                }
            }
        }
    }
    for it in base {
        if !target_map.contains_key(it.item_key.as_str()) {
            removed.push(diff_item(it));
        }
    }

    let by_name = |a: &DiffItem, b: &DiffItem| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    added.sort_by(by_name);
    removed.sort_by(by_name);
    changed.sort_by_key(|a| a.name.to_lowercase());

    Diff {
        base_label: base_label.into(),
        target_label: target_label.into(),
        added,
        removed,
        changed,
        unchanged,
    }
}

fn diff_item(it: &Item) -> DiffItem {
    DiffItem {
        name: it.name.clone(),
        collector: it.collector.clone(),
        domain: it.domain,
        version: it.version.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Domain;

    fn brew(name: &str, ver: &str) -> Item {
        Item::new(Domain::PackageManager, "homebrew", name).version(ver)
    }

    #[test]
    fn diff_detects_changes() {
        let base = vec![brew("ripgrep", "14.0.0"), brew("jq", "1.7"), brew("fd", "9.0")];
        let target = vec![brew("ripgrep", "14.1.0"), brew("jq", "1.7"), brew("bat", "0.24")];
        let d = diff(&base, &target, "snap", "current");
        // bat added; fd removed; ripgrep changed; jq unchanged
        assert_eq!(d.added.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["bat"]);
        assert_eq!(d.removed.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["fd"]);
        assert_eq!(d.changed.len(), 1);
        assert_eq!(d.changed[0].name, "ripgrep");
        assert_eq!(d.changed[0].old_version.as_deref(), Some("14.0.0"));
        assert_eq!(d.changed[0].new_version.as_deref(), Some("14.1.0"));
        assert_eq!(d.unchanged, 1);
    }

    #[test]
    fn export_import_roundtrip() {
        let items = vec![brew("ripgrep", "14.1.0"), brew("jq", "1.7")];
        let file = SnapshotFile::new("My setup", "2026-01-01T00:00:00Z", "host", "macOS", items);
        let json = serde_json::to_string(&file).unwrap();
        let parsed = parse_import(&json).unwrap();
        assert_eq!(parsed.name, "My setup");
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.items[0].name, "ripgrep");
        // A wrong format string is rejected.
        assert!(parse_import(r#"{"format":"nope","items":[]}"#).is_err());
        // Missing items is rejected.
        assert!(parse_import(r#"{"name":"x"}"#).is_err());
        println!("export_import_roundtrip OK");
    }
}
