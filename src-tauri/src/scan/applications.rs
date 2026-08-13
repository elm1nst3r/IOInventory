use crate::model::Item;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use super::util;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::model::Domain;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::path::Path;

/// Installed applications from the OS's own app folders — macOS's
/// `/Applications` (+ the per-user one) and Windows's `Program Files` (+
/// per-user installs under `%LOCALAPPDATA%\Programs`). Unlike the
/// package-manager collectors this catches everything a user drags in or
/// double-click-installs, which is most of what you'd need to know about to
/// rebuild a machine from scratch.
pub async fn collect() -> Vec<Item> {
    #[cfg(target_os = "macos")]
    {
        collect_macos().await
    }
    #[cfg(target_os = "windows")]
    {
        collect_windows().await
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
async fn collect_macos() -> Vec<Item> {
    let roots: Vec<PathBuf> = [PathBuf::from("/Applications"), util::home().join("Applications")]
        .into_iter()
        .filter(|p| p.exists())
        .collect();

    let mut bundles: Vec<PathBuf> = Vec::new();
    for root in &roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("app") {
                bundles.push(path);
            }
        }
    }

    // No sizes here, deliberately. The `du -d 1` trick this collector borrowed
    // from `homebrew::cellar_sizes` doesn't transfer: the Cellar is ~36k files,
    // while /Applications is ~380k across ~32 GB, and walking it cost 7.7s of a
    // scan budgeted at 2s. Reading each bundle's Info.plist, by contrast, is
    // ~0.07s for all of them in parallel, so that stays.
    //
    // If per-app size is wanted later it belongs in `enrich.rs`, which exists
    // precisely to keep this kind of cost off the scan path.
    let mut items: Vec<Item> = Vec::new();
    let mut jobs: tokio::task::JoinSet<(usize, Option<serde_json::Value>)> =
        tokio::task::JoinSet::new();

    for bundle in &bundles {
        let name = bundle.file_stem().map(|s| s.to_string_lossy().into_owned());
        let Some(name) = name.filter(|n| !n.is_empty()) else {
            continue;
        };

        let item = Item::new(Domain::Application, "app", name)
            .path(bundle.to_string_lossy().into_owned());

        let idx = items.len();
        items.push(item);

        let plist = bundle.join("Contents/Info.plist");
        if plist.exists() {
            jobs.spawn(async move { (idx, read_info_plist(&plist).await) });
        }
    }

    while let Some(joined) = jobs.join_next().await {
        let Ok((idx, Some(info))) = joined else {
            continue;
        };
        if let Some(v) = info.get("CFBundleShortVersionString").and_then(|v| v.as_str()) {
            items[idx].version = Some(v.to_string());
        }
        items[idx].metadata = serde_json::json!({
            "bundle_id": info.get("CFBundleIdentifier").and_then(|v| v.as_str()),
            "display_name": info
                .get("CFBundleDisplayName")
                .or_else(|| info.get("CFBundleName"))
                .and_then(|v| v.as_str()),
        });
    }

    items
}

#[cfg(target_os = "macos")]
async fn read_info_plist(path: &Path) -> Option<serde_json::Value> {
    let path_str = path.to_string_lossy().into_owned();
    let out = util::run("plutil", &["-convert", "json", "-o", "-", &path_str]).await?;
    serde_json::from_str(&out).ok()
}


#[cfg(target_os = "windows")]
async fn collect_windows() -> Vec<Item> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(p) = std::env::var(var) {
            let p = PathBuf::from(p);
            if p.exists() && !roots.contains(&p) {
                roots.push(p);
            }
        }
    }
    if roots.is_empty() {
        for p in ["C:\\Program Files", "C:\\Program Files (x86)"] {
            let p = PathBuf::from(p);
            if p.exists() {
                roots.push(p);
            }
        }
    }
    // Many modern installers (VS Code, Discord, …) install per-user here
    // instead of the machine-wide Program Files.
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local).join("Programs");
        if p.exists() {
            roots.push(p);
        }
    }

    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in &roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.is_empty() || !seen.insert(name.clone()) {
                continue;
            }
            items.push(
                Item::new(Domain::Application, "app", name)
                    .path(entry.path().to_string_lossy().into_owned()),
            );
        }
    }
    items
}
