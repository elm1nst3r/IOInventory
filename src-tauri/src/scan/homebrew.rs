use super::util;
use crate::model::{Domain, Item};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// Homebrew formulae and casks. We use `--versions` so we get name+version in
/// one call per kind, then attribute on-disk sizes to formulae from a single
/// `du` over the Cellar (so the list can be sorted by size).
pub async fn collect() -> Vec<Item> {
    if !util::is_available("brew") {
        return Vec::new();
    }

    // Homebrew prefix → Cellar / Caskroom locations.
    let prefix = util::run("brew", &["--prefix"])
        .await
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/opt/homebrew".to_string());
    let cellar = PathBuf::from(&prefix).join("Cellar");
    let caskroom = PathBuf::from(&prefix).join("Caskroom");

    // Run both listings and the size probe concurrently.
    let (formula_out, cask_out, sizes) = tokio::join!(
        util::run("brew", &["list", "--formula", "--versions"]),
        util::run("brew", &["list", "--cask", "--versions"]),
        cellar_sizes(&cellar),
    );

    let mut items = Vec::new();
    if let Some(out) = formula_out {
        for line in out.lines() {
            if let Some(item) = parse_line(line, "formula", &cellar) {
                items.push(item);
            }
        }
    }
    if let Some(out) = cask_out {
        for line in out.lines() {
            if let Some(item) = parse_line(line, "cask", &caskroom) {
                items.push(item);
            }
        }
    }

    if !sizes.is_empty() {
        for it in items.iter_mut() {
            if it.collector == "homebrew" {
                if let Some(bytes) = sizes.get(&it.name) {
                    it.size_bytes = Some(*bytes);
                }
            }
        }
    }

    // Which formulae are transitive rather than chosen is worked out in
    // `outdated::mark`, which already has the `brew info --json=v2 --installed`
    // payload that answers it — a separate `brew leaves` call costs ~1s and
    // tells us the same thing.

    items
}

fn parse_line(line: &str, kind: &str, base: &std::path::Path) -> Option<Item> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.split_whitespace();
    let name = parts.next()?.to_string();
    let version = parts.collect::<Vec<_>>().join(", ");
    let collector = if kind == "cask" { "homebrew-cask" } else { "homebrew" };
    let mut item = Item::new(Domain::PackageManager, collector, name.clone());
    if !version.is_empty() {
        item = item.version(version);
    }
    let path = base.join(&name);
    if path.exists() {
        item = item.path(path.to_string_lossy().into_owned());
    }
    Some(item.meta(serde_json::json!({ "kind": kind })))
}

/// Map of formula name → installed size in bytes, from one `du` over the Cellar.
async fn cellar_sizes(cellar: &std::path::Path) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    if !cellar.exists() {
        return map;
    }
    let cellar_str = cellar.to_string_lossy();
    // `-d 1` gives one line per immediate child (each formula) plus the total.
    let (_, out) = util::run_capture(
        "du",
        &["-d", "1", "-k", cellar_str.as_ref()],
        Duration::from_secs(8),
    )
    .await;
    for line in out.lines() {
        let mut it = line.splitn(2, '\t');
        let (Some(kb), Some(path)) = (it.next(), it.next()) else {
            continue;
        };
        let path = path.trim();
        // Skip the Cellar total line itself.
        if path == cellar_str.as_ref() {
            continue;
        }
        if let (Ok(kb), Some(name)) = (
            kb.trim().parse::<i64>(),
            std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().into_owned()),
        ) {
            map.insert(name, kb * 1024);
        }
    }
    map
}
