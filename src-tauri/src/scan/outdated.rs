use super::util;
use crate::model::Item;
use std::collections::{HashMap, HashSet};

/// Mark items that have a newer version available (bulk "outdated" queries for
/// Homebrew + npm) and flag deprecated Homebrew formulae. Annotates items with
/// `outdated`/`latest`/`deprecated` in metadata.
pub async fn mark(items: &mut [Item]) {
    let (brew, npm, deprecated) =
        tokio::join!(brew_outdated(), npm_outdated(), brew_deprecated());

    for it in items.iter_mut() {
        let latest = match it.collector.as_str() {
            "homebrew" | "homebrew-cask" => brew.get(&it.name),
            "npm" => npm.get(&it.name),
            _ => None,
        };
        if let Some(latest) = latest {
            if let Some(obj) = it.metadata.as_object_mut() {
                obj.insert("outdated".into(), serde_json::json!(true));
                obj.insert("latest".into(), serde_json::json!(latest));
            }
        }
        if it.collector == "homebrew" && deprecated.contains(&it.name) {
            if let Some(obj) = it.metadata.as_object_mut() {
                obj.insert("deprecated".into(), serde_json::json!(true));
            }
        }
    }
}

/// Names of installed Homebrew formulae that are deprecated or disabled.
/// A single `brew info --json=v2 --installed` call carries the flags for all
/// installed formulae. Bounded by a timeout so it can't dominate the scan.
async fn brew_deprecated() -> HashSet<String> {
    let mut set = HashSet::new();
    if !util::is_available("brew") {
        return set;
    }
    let out = match util::run_with(
        "brew",
        &["info", "--json=v2", "--installed"],
        std::time::Duration::from_secs(8),
    )
    .await
    {
        Some(o) => o,
        None => return set,
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) else {
        return set;
    };
    if let Some(arr) = v.get("formulae").and_then(|f| f.as_array()) {
        for f in arr {
            let deprecated = f.get("deprecated").and_then(|d| d.as_bool()).unwrap_or(false);
            let disabled = f.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false);
            if deprecated || disabled {
                if let Some(name) = f.get("name").and_then(|n| n.as_str()) {
                    set.insert(name.to_string());
                }
            }
        }
    }
    set
}

/// name -> latest version, for outdated brew formulae and casks.
async fn brew_outdated() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if !util::is_available("brew") {
        return map;
    }
    // --greedy so pinned/auto-update casks are still reported.
    let Some(out) = util::run("brew", &["outdated", "--json=v2", "--greedy"]).await else {
        return map;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) else {
        return map;
    };
    for key in ["formulae", "casks"] {
        if let Some(arr) = v.get(key).and_then(|x| x.as_array()) {
            for entry in arr {
                let name = entry.get("name").and_then(|n| n.as_str());
                let latest = entry
                    .get("current_version")
                    .or_else(|| entry.get("current_versions"))
                    .and_then(|c| c.as_str())
                    .or_else(|| {
                        entry
                            .get("current_version")
                            .and_then(|c| c.as_array())
                            .and_then(|a| a.first())
                            .and_then(|s| s.as_str())
                    });
                if let (Some(name), Some(latest)) = (name, latest) {
                    map.insert(name.to_string(), latest.to_string());
                }
            }
        }
    }
    map
}

/// name -> latest version for outdated global npm packages.
async fn npm_outdated() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if !util::is_available("npm") {
        return map;
    }
    // npm outdated exits non-zero when packages are outdated, so capture output
    // regardless of exit status.
    let (_, out) =
        util::run_capture("npm", &["outdated", "-g", "--json"], std::time::Duration::from_secs(8))
            .await;
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) else {
        return map;
    };
    if let Some(obj) = v.as_object() {
        for (name, info) in obj {
            if let Some(latest) = info.get("latest").and_then(|l| l.as_str()) {
                map.insert(name.clone(), latest.to_string());
            }
        }
    }
    map
}
