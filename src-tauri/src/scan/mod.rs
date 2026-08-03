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

/// Run every collector concurrently and return the merged item list. Each
/// collector internally guards its own subprocesses with a timeout, so a hung
/// tool can't stall the overall scan.
pub async fn run_all(roots: Vec<PathBuf>) -> Vec<Item> {
    let (brew, npm_, pip_, cargo_, gem_, runtimes_, docker_, claude_, ollama_, hf, ai, tools_, repos_) = tokio::join!(
        homebrew::collect(),
        npm::collect(),
        pip::collect(),
        cargo::collect(),
        gem::collect(),
        runtimes::collect(),
        docker::collect(),
        claude::collect(),
        ollama::collect(),
        hf_cache::collect(),
        ai_libs::collect(),
        ai_tools::collect(),
        repos::collect(&roots),
    );

    let mut items = Vec::new();
    for group in [
        brew, npm_, pip_, cargo_, gem_, runtimes_, docker_, claude_, ollama_, hf, ai, tools_, repos_,
    ] {
        items.extend(group);
    }

    // Annotate items that have newer versions available (bulk brew/npm checks).
    outdated::mark(&mut items).await;

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::Instant;

    /// Smoke test: runs the whole scan against the real machine and prints a
    /// per-collector breakdown. Run with: `cargo test -- --nocapture`.
    #[tokio::test]
    async fn scan_smoke() {
        let t = Instant::now();
        let items = run_all(default_roots()).await;
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
        println!("outdated (brew/npm): {outdated}");

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
