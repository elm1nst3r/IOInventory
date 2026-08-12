use super::util;
use crate::model::Item;
use std::collections::{HashMap, HashSet};

/// Mark items that have a newer version available (bulk "outdated" queries for
/// Homebrew + npm) and flag deprecated Homebrew formulae. Annotates items with
/// `outdated`/`latest`/`deprecated` in metadata.
pub async fn mark(items: &mut [Item]) {
    let (brew, npm, installed) =
        tokio::join!(brew_outdated(), npm_outdated(), brew_installed_facts());

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
        if it.collector == "homebrew" {
            if installed.deprecated.contains(&it.name) {
                if let Some(obj) = it.metadata.as_object_mut() {
                    obj.insert("deprecated".into(), serde_json::json!(true));
                }
            }
            // Only claim a formula is/isn't a dependency when the graph was
            // readable and actually covered it; an empty set would mark
            // everything top-level, and an unlisted formula gets no verdict.
            if let Some(depended_on) = &installed.depended_on {
                if installed.known.contains(&it.name) {
                    if let Some(obj) = it.metadata.as_object_mut() {
                        obj.insert(
                            "dependency".into(),
                            serde_json::json!(depended_on.contains(&it.name)),
                        );
                    }
                }
            }
        }
    }
}

/// What one `brew info --json=v2 --installed` payload tells us about the
/// installed formulae.
#[derive(Default)]
struct InstalledFacts {
    /// Formulae flagged deprecated or disabled upstream.
    deprecated: HashSet<String>,
    /// Formulae that some other installed formula pulls in. The complement is
    /// what `brew leaves` reports — the set the user actually asked for — so
    /// this saves a second ~1s brew invocation. `None` if the payload was
    /// unreadable, which must not be confused with "nothing is a dependency".
    depended_on: Option<HashSet<String>>,
    /// Formulae the payload actually described. `brew info` silently omits ones
    /// it can't resolve (a formula from a tap that's since gone, say), and
    /// those get no verdict rather than a wrong one.
    known: HashSet<String>,
}

/// Deprecation flags and the runtime-dependency graph for every installed
/// formula, from a single `brew info --json=v2 --installed` call. Bounded by a
/// timeout so it can't dominate the scan.
async fn brew_installed_facts() -> InstalledFacts {
    let mut facts = InstalledFacts::default();
    if !util::is_available("brew") {
        return facts;
    }
    let out = match util::run_with(
        "brew",
        &["info", "--json=v2", "--installed"],
        std::time::Duration::from_secs(8),
    )
    .await
    {
        Some(o) => o,
        None => return facts,
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) else {
        return facts;
    };
    if let Some(arr) = v.get("formulae").and_then(|f| f.as_array()) {
        let mut depended_on = HashSet::new();
        for f in arr {
            let deprecated = f.get("deprecated").and_then(|d| d.as_bool()).unwrap_or(false);
            let disabled = f.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false);
            if let Some(name) = f.get("name").and_then(|n| n.as_str()) {
                facts.known.insert(name.to_string());
                if deprecated || disabled {
                    facts.deprecated.insert(name.to_string());
                }
            }
            // Each installed keg lists what it actually needs at runtime; the
            // union is every formula that's here because something else wanted
            // it. Tapped formulae appear as "tap/name", so key on the last part.
            for inst in f.get("installed").and_then(|i| i.as_array()).into_iter().flatten() {
                for rd in inst
                    .get("runtime_dependencies")
                    .and_then(|r| r.as_array())
                    .into_iter()
                    .flatten()
                {
                    if let Some(full) = rd.get("full_name").and_then(|n| n.as_str()) {
                        depended_on.insert(full.rsplit('/').next().unwrap_or(full).to_string());
                    }
                }
            }
        }
        if !arr.is_empty() {
            facts.depended_on = Some(depended_on);
        }
    }
    facts
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
    // npm outdated exits non-zero when packages are outdated, so read the output
    // regardless of exit status — and read stdout only, because npm's warnings
    // go to stderr and would otherwise be spliced into the JSON.
    let Some(out) =
        util::run_stdout_untracked("npm", &["outdated", "-g", "--json"], std::time::Duration::from_secs(8))
            .await
    else {
        return map;
    };
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
