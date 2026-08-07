pub mod ai_libs;
pub mod ai_tools;
pub mod cargo;
pub mod claude;
pub mod docker;
pub mod enrich;
pub mod gem;
pub mod hf_cache;
pub mod homebrew;
pub mod npm;
pub mod ollama;
pub mod outdated;
pub mod pip;
pub mod repos;
pub mod runtimes;
pub mod util;

use crate::model::Item;
use crate::settings::Settings;
use std::future::Future;
use std::path::PathBuf;

/// Default workspace roots to search for git repositories.
pub fn default_roots() -> Vec<PathBuf> {
    let home = util::home();
    ["Dev", "Projects", "Code", "src", "repos", "work", "Sites"]
        .iter()
        .map(|d| home.join(d))
        .filter(|p| p.exists())
        .collect()
}

/// Await `f` only when the source is enabled. Async fns are lazy, so a
/// disabled collector's future is constructed but never polled — no process is
/// spawned and no filesystem is walked.
async fn when<F: Future<Output = Vec<Item>>>(enabled: bool, f: F) -> Vec<Item> {
    if enabled {
        f.await
    } else {
        Vec::new()
    }
}

/// Run every enabled collector concurrently and return the merged item list.
/// Each collector internally guards its own subprocesses with a timeout, so a
/// hung tool can't stall the overall scan.
pub async fn run_all(roots: Vec<PathBuf>, settings: &Settings) -> Vec<Item> {
    let on = |id: &str| settings.is_enabled(id);

    let (brew, npm_, pip_, cargo_, gem_, runtimes_, docker_, claude_, ollama_, hf, ai, tools_, repos_) = tokio::join!(
        when(on("homebrew"), homebrew::collect()),
        when(on("npm"), npm::collect()),
        when(on("pip"), pip::collect()),
        when(on("cargo"), cargo::collect()),
        when(on("gem"), gem::collect()),
        when(on("runtimes"), runtimes::collect()),
        when(on("docker"), docker::collect()),
        when(on("claude"), claude::collect()),
        when(on("ollama"), ollama::collect()),
        when(on("hf_cache"), hf_cache::collect()),
        when(on("ai_libs"), ai_libs::collect()),
        when(on("ai_tools"), ai_tools::collect()),
        when(on("repos"), repos::collect(&roots)),
    );

    let mut items = Vec::new();
    for group in [
        brew, npm_, pip_, cargo_, gem_, runtimes_, docker_, claude_, ollama_, hf, ai, tools_, repos_,
    ] {
        items.extend(group);
    }

    // Annotate items that have newer versions available (bulk brew/npm checks).
    // Both queries are slow, so skip them when nothing they'd annotate is here.
    if items
        .iter()
        .any(|i| matches!(i.collector.as_str(), "homebrew" | "homebrew-cask" | "npm"))
    {
        outdated::mark(&mut items).await;
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::Instant;

    /// Disabling a source must skip its collector outright, not filter after
    /// the fact — with everything off nothing runs, so this returns instantly.
    #[tokio::test]
    async fn disabled_sources_are_not_collected() {
        let all_off = Settings {
            disabled_sources: crate::settings::SOURCES.iter().map(|s| s.id.into()).collect(),
            ..Default::default()
        };
        let t = Instant::now();
        let items = run_all(default_roots(), &all_off).await;
        let elapsed = t.elapsed();
        assert!(items.is_empty(), "expected no items, got {}", items.len());
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "disabled collectors still did work: {elapsed:?}"
        );

        // With only Homebrew on, nothing else may appear.
        let only_brew = Settings {
            disabled_sources: crate::settings::SOURCES
                .iter()
                .map(|s| s.id.to_string())
                .filter(|id| id != "homebrew")
                .collect(),
            ..Default::default()
        };
        let items = run_all(vec![], &only_brew).await;
        let foreign: Vec<&str> = items
            .iter()
            .map(|i| i.collector.as_str())
            .filter(|c| !matches!(*c, "homebrew" | "homebrew-cask"))
            .collect();
        assert!(foreign.is_empty(), "unexpected collectors leaked through: {foreign:?}");
        println!(
            "disabled_sources_are_not_collected OK — all-off in {elapsed:?}, brew-only gave {} items",
            items.len()
        );
    }

    /// Smoke test: runs the whole scan against the real machine and prints a
    /// per-collector breakdown. Run with: `cargo test -- --nocapture`.
    #[tokio::test]
    async fn scan_smoke() {
        let t = Instant::now();
        let items = run_all(default_roots(), &Settings::default()).await;
        let elapsed = t.elapsed();

        let mut by: BTreeMap<String, usize> = BTreeMap::new();
        for it in &items {
            *by.entry(format!("{}/{}", it.domain.as_str(), it.collector))
                .or_default() += 1;
        }
        println!("\n=== scan_smoke: {} items in {:?} ===", items.len(), elapsed);
        for (k, n) in &by {
            println!("{:>5}  {}", n, k);
        }

        let outdated = items
            .iter()
            .filter(|i| i.metadata.get("outdated").and_then(|o| o.as_bool()).unwrap_or(false))
            .count();
        let deprecated = items
            .iter()
            .filter(|i| i.metadata.get("deprecated").and_then(|o| o.as_bool()).unwrap_or(false))
            .count();
        println!("outdated (brew/npm): {outdated} · deprecated (brew): {deprecated}");

        // Homebrew size coverage + top 3 largest formulae.
        let mut brew_sized: Vec<(&str, i64)> = items
            .iter()
            .filter(|i| i.collector == "homebrew")
            .filter_map(|i| i.size_bytes.map(|b| (i.name.as_str(), b)))
            .collect();
        brew_sized.sort_by(|a, b| b.1.cmp(&a.1));
        println!(
            "brew formulae with size: {} · top: {:?}",
            brew_sized.len(),
            brew_sized.iter().take(3).map(|(n, b)| format!("{n}={}MB", b / 1_000_000)).collect::<Vec<_>>()
        );

        // Per-item management info for a Homebrew formula (no destructive run).
        if let Some(f) = items.iter().find(|i| i.collector == "homebrew") {
            let a = crate::manage::info("homebrew", &f.name);
            println!("manage {} -> update={:?} delete={:?} available={}", f.name, a.update, a.delete, a.available);
        }

        // Enrich one Homebrew formula end-to-end.
        if let Some(f) = items.iter().find(|i| i.collector == "homebrew") {
            let e = enrich::enrich("homebrew", &f.name, None).await;
            println!(
                "enrich {} -> desc={:?} latest={:?} installed_at={:?} outdated={:?}",
                f.name, e.description, e.latest_version, e.installed_at, e.outdated
            );
        }
        // Enrich one Claude skill (frontmatter description).
        if let Some(s) = items.iter().find(|i| i.collector == "claude-skill") {
            let e = enrich::enrich("claude-skill", &s.name, s.source_path.clone()).await;
            println!("enrich skill {} -> desc={:?}", s.name, e.description.map(|d| d.chars().take(60).collect::<String>()));
        }

        assert!(!items.is_empty(), "scan returned no items");
    }
}
