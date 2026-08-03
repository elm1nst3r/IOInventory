use super::util;
use crate::model::{Domain, Item};

/// Homebrew formulae and casks. We use `--versions` so we get name+version in
/// one call per kind; per-package size is intentionally skipped to stay fast.
pub async fn collect() -> Vec<Item> {
    if !util::is_available("brew") {
        return Vec::new();
    }
    let mut items = Vec::new();

    if let Some(out) = util::run("brew", &["list", "--formula", "--versions"]).await {
        for line in out.lines() {
            if let Some(item) = parse_line(line, "formula") {
                items.push(item);
            }
        }
    }
    if let Some(out) = util::run("brew", &["list", "--cask", "--versions"]).await {
        for line in out.lines() {
            if let Some(item) = parse_line(line, "cask") {
                items.push(item);
            }
        }
    }
    items
}

fn parse_line(line: &str, kind: &str) -> Option<Item> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.split_whitespace();
    let name = parts.next()?.to_string();
    let version = parts.collect::<Vec<_>>().join(", ");
    let collector = if kind == "cask" { "homebrew-cask" } else { "homebrew" };
    let mut item = Item::new(Domain::PackageManager, collector, name);
    if !version.is_empty() {
        item = item.version(version);
    }
    Some(item.meta(serde_json::json!({ "kind": kind })))
}
