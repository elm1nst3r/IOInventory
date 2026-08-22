use super::util;
use crate::model::{Domain, Item};

/// Cargo-installed binaries, read from ~/.cargo/.crates2.json (no subprocess).
pub async fn collect() -> Vec<Item> {
    let path = util::home().join(".cargo/.crates2.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if let Some(installs) = v.get("installs").and_then(|d| d.as_object()) {
        for (key, info) in installs {
            // key looks like: "ripgrep 14.1.0 (registry+https://...)"
            let mut parts = key.split_whitespace();
            let name = parts.next().unwrap_or("").to_string();
            let version = parts.next().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let bins = info
                .get("bins")
                .and_then(|b| b.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            let mut item = Item::new(Domain::PackageManager, "cargo", name);
            if !version.is_empty() {
                item = item.version(version);
            }
            items.push(item.meta(serde_json::json!({ "bins": bins })));
        }
    }
    items
}
