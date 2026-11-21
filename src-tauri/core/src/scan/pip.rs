use super::util;
use crate::model::{Domain, Item};
use std::collections::HashSet;

/// Packages no other installed package depends on — what the user asked for.
/// `None` when the query is unavailable, which is not the same as "none".
/// Names are lowercased because pip normalises inconsistently across versions.
async fn top_level_packages(pip: &str) -> Option<HashSet<String>> {
    let out = util::run_with(
        pip,
        &["list", "--not-required", "--format=json"],
        std::time::Duration::from_secs(10),
    )
    .await?;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&out).ok()?;
    let set: HashSet<String> = arr
        .iter()
        .filter_map(|p| p.get("name").and_then(|v| v.as_str()))
        .map(|n| n.to_ascii_lowercase())
        .collect();
    if set.is_empty() {
        None
    } else {
        Some(set)
    }
}

/// Global pip packages and pipx-managed applications.
pub async fn collect() -> Vec<Item> {
    let mut items = Vec::new();

    let pip = if util::is_available("pip3") {
        Some("pip3")
    } else if util::is_available("pip") {
        Some("pip")
    } else {
        None
    };
    if let Some(pip) = pip {
        // The full list and the top-level-only list, concurrently — the second
        // is what separates "I installed this" from "something needed it".
        let (all, top_level) = tokio::join!(
            util::run(pip, &["list", "--format=json"]),
            top_level_packages(pip),
        );
        if let Some(out) = all {
            if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str(&out) {
                for pkg in arr {
                    let name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if name.is_empty() || crate::scan::ai_libs::is_ai_lib(name) {
                        // AI libs are surfaced under the AI & Agents domain instead.
                        continue;
                    }
                    let mut item = Item::new(Domain::PackageManager, "pip", name);
                    if let Some(v) = pkg.get("version").and_then(|v| v.as_str()) {
                        item = item.version(v);
                    }
                    if let Some(top) = &top_level {
                        let dependency = !top.contains(&name.to_ascii_lowercase());
                        item = item.meta(serde_json::json!({ "dependency": dependency }));
                    }
                    items.push(item);
                }
            }
        }
    }

    if util::is_available("pipx") {
        if let Some(out) = util::run("pipx", &["list", "--json"]).await {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) {
                if let Some(venvs) = v.get("venvs").and_then(|d| d.as_object()) {
                    for (name, info) in venvs {
                        let main = info.pointer("/metadata/main_package");
                        let ver = main
                            .and_then(|m| m.get("package_version"))
                            .and_then(|v| v.as_str());
                        let mut item = Item::new(Domain::PackageManager, "pipx", name.clone());
                        if let Some(v) = ver {
                            item = item.version(v);
                        }
                        items.push(item.meta(serde_json::json!({ "kind": "application" })));
                    }
                }
            }
        }
    }
    if util::is_available("uv") {
        if let Some(out) = util::run("uv", &["tool", "list"]).await {
            items.extend(parse_uv_tools(&out));
        }
    }
    items
}

/// `uv tool list`: a `name vX.Y.Z` line per tool, each followed by `- exe`
/// lines for the executables it provides (and possibly a "No tools installed"
/// notice, which has no version and is skipped).
fn parse_uv_tools(out: &str) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for line in out.lines() {
        let line = line.trim_end();
        if let Some(exe) = line.trim_start().strip_prefix("- ") {
            if let Some(last) = items.last_mut() {
                if let Some(arr) = last.metadata["executables"].as_array_mut() {
                    arr.push(serde_json::json!(exe.trim()));
                }
            }
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(name), Some(ver)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(ver) = ver.strip_prefix('v').filter(|v| v.starts_with(|c: char| c.is_ascii_digit())) else {
            continue;
        };
        items.push(
            Item::new(Domain::PackageManager, "uv-tool", name)
                .version(ver)
                .meta(serde_json::json!({ "kind": "application", "executables": [] })),
        );
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_tool_list_parses() {
        let items = parse_uv_tools("ruff v0.6.9\n- ruff\nllm v0.19\n- llm\n- llm-x\n");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "ruff");
        assert_eq!(items[0].version.as_deref(), Some("0.6.9"));
        assert_eq!(items[1].metadata["executables"].as_array().map(|a| a.len()), Some(2));
        assert!(parse_uv_tools("No tools installed\n").is_empty());
        println!("uv_tool_list_parses OK");
    }
}
