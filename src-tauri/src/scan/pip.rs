use super::util;
use crate::model::{Domain, Item};

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
        if let Some(out) = util::run(pip, &["list", "--format=json"]).await {
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
    items
}
