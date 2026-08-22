use super::util;
use crate::model::{Domain, Item};

/// Globally-installed JS packages from npm and (if present) pnpm.
pub async fn collect() -> Vec<Item> {
    let mut items = Vec::new();

    if util::is_available("npm") {
        if let Some(out) = util::run("npm", &["ls", "-g", "--depth=0", "--json"]).await {
            parse_npm(&out, "npm", &mut items);
        }
    }
    if util::is_available("pnpm") {
        if let Some(out) = util::run("pnpm", &["ls", "-g", "--depth=0", "--json"]).await {
            // pnpm returns an array of project objects.
            if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&out) {
                for proj in arr {
                    if let Some(deps) = proj.get("dependencies").and_then(|d| d.as_object()) {
                        for (name, info) in deps {
                            push(name, info.get("version"), "pnpm", &mut items);
                        }
                    }
                }
            }
        }
    }
    items
}

fn parse_npm(out: &str, collector: &str, items: &mut Vec<Item>) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(out) {
        if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
            for (name, info) in deps {
                push(name, info.get("version"), collector, items);
            }
        }
    }
}

fn push(name: &str, version: Option<&serde_json::Value>, collector: &str, items: &mut Vec<Item>) {
    if name == "npm" || name == "pnpm" || name == "corepack" {
        return;
    }
    let mut item = Item::new(Domain::PackageManager, collector, name);
    if let Some(v) = version.and_then(|v| v.as_str()) {
        item = item.version(v);
    }
    items.push(item);
}
