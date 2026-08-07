//! User settings: which parts of the machine to scan, and where to look for
//! git repositories. Persisted in the `settings` table as a single JSON blob so
//! the shape can grow without a migration.
//!
//! Everything is opt-*out*: a fresh install scans all sources, and disabling
//! one both hides its items and skips the collector entirely, which makes the
//! scan proportionally faster.

use crate::model::Domain;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Key under which [`Settings`] is stored in the `settings` table.
pub const SETTINGS_KEY: &str = "settings";

/// One toggleable unit of scanning. `id` matches a collector module in
/// `scan/`, which is the granularity we can actually skip work at — a single
/// module may emit items under several `collector` names (the Claude source
/// emits skills, commands, agents, plugins and MCP servers).
pub struct ScanSource {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub domain: Domain,
}

pub const SOURCES: &[ScanSource] = &[
    ScanSource {
        id: "homebrew",
        label: "Homebrew",
        description: "Installed formulae and casks, with their disk usage.",
        domain: Domain::PackageManager,
    },
    ScanSource {
        id: "npm",
        label: "npm & pnpm",
        description: "Globally installed Node packages.",
        domain: Domain::PackageManager,
    },
    ScanSource {
        id: "pip",
        label: "pip & pipx",
        description: "Python packages installed for the user, and pipx apps.",
        domain: Domain::PackageManager,
    },
    ScanSource {
        id: "cargo",
        label: "cargo",
        description: "Binaries installed with `cargo install`.",
        domain: Domain::PackageManager,
    },
    ScanSource {
        id: "gem",
        label: "RubyGems",
        description: "Gems installed for the current Ruby.",
        domain: Domain::PackageManager,
    },
    ScanSource {
        id: "ai_libs",
        label: "Python AI libraries",
        description: "Installed ML/AI packages such as torch, transformers, langchain.",
        domain: Domain::PackageManager,
    },
    ScanSource {
        id: "runtimes",
        label: "Language runtimes",
        description: "Node, Python, Ruby, Go, Rust toolchains and version managers.",
        domain: Domain::Runtime,
    },
    ScanSource {
        id: "repos",
        label: "Git repositories",
        description: "Projects under your workspace roots, with detected stacks.",
        domain: Domain::Project,
    },
    ScanSource {
        id: "docker",
        label: "Docker",
        description: "Local images and containers.",
        domain: Domain::Container,
    },
    ScanSource {
        id: "claude",
        label: "Claude Code",
        description: "Skills, commands, agents, plugins and configured MCP servers.",
        domain: Domain::AiAgent,
    },
    ScanSource {
        id: "ai_tools",
        label: "AI tools & apps",
        description: "AI CLIs and desktop apps installed on this machine.",
        domain: Domain::AiAgent,
    },
    ScanSource {
        id: "ollama",
        label: "Ollama models",
        description: "Local models pulled with Ollama.",
        domain: Domain::AiAgent,
    },
    ScanSource {
        id: "hf_cache",
        label: "Hugging Face cache",
        description: "Models and datasets cached by the Hugging Face libraries.",
        domain: Domain::AiAgent,
    },
];

/// Catalog entry as sent to the UI.
#[derive(Serialize)]
pub struct ScanSourceInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    pub domain: Domain,
}

pub fn catalog() -> Vec<ScanSourceInfo> {
    SOURCES
        .iter()
        .map(|s| ScanSourceInfo {
            id: s.id.into(),
            label: s.label.into(),
            description: s.description.into(),
            domain: s.domain,
        })
        .collect()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Source ids the user has switched off. Empty means "scan everything".
    #[serde(default)]
    pub disabled_sources: Vec<String>,
    /// Directories searched for git repositories. Empty means "use the
    /// auto-detected defaults", so the default keeps working if the user later
    /// creates a `~/Projects`.
    #[serde(default)]
    pub roots: Vec<String>,
    /// Whether the MCP server may expose the tools that change this machine
    /// (install / update / uninstall / cleanups). Off by default — agents get
    /// read-only access until the user deliberately turns this on.
    ///
    /// The server re-reads this on every request, so switching it off takes
    /// effect immediately, without restarting the agent's client.
    #[serde(default)]
    pub mcp_allow_write: bool,
}

impl Settings {
    pub fn is_enabled(&self, id: &str) -> bool {
        !self.disabled_sources.iter().any(|d| d == id)
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        if self.roots.is_empty() {
            crate::scan::default_roots()
        } else {
            self.roots.iter().map(PathBuf::from).collect()
        }
    }

    /// Drop unknown ids so a stale setting can't silently disable nothing (or,
    /// after a rename, everything).
    pub fn sanitized(mut self) -> Settings {
        self.disabled_sources
            .retain(|id| SOURCES.iter().any(|s| s.id == id));
        self.disabled_sources.sort();
        self.disabled_sources.dedup();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every source id must name a real collector, since `run_all` gates on
    /// these strings — a typo here silently disables nothing.
    #[test]
    fn source_ids_are_unique_and_complete() {
        let mut ids: Vec<&str> = SOURCES.iter().map(|s| s.id).collect();
        let count = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate source id in SOURCES");
        // Mirrors the collectors joined in scan::run_all.
        let expected = [
            "ai_libs", "ai_tools", "cargo", "claude", "docker", "gem", "hf_cache", "homebrew",
            "npm", "ollama", "pip", "repos", "runtimes",
        ];
        assert_eq!(ids, expected, "SOURCES has drifted from scan::run_all");
    }

    #[test]
    fn settings_round_trip_and_sanitize() {
        let s = Settings {
            disabled_sources: vec!["docker".into(), "nope".into(), "docker".into()],
            roots: vec!["/tmp/work".into()],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(s.disabled_sources, vec!["docker".to_string()]);
        assert!(!s.is_enabled("docker"));
        assert!(s.is_enabled("homebrew"));
        assert_eq!(s.roots(), vec![PathBuf::from("/tmp/work")]);

        // Defaults scan everything, and fall back to auto-detected roots.
        let d = Settings::default();
        assert!(SOURCES.iter().all(|src| d.is_enabled(src.id)));
        assert_eq!(d.roots(), crate::scan::default_roots());

        // Survives a JSON round trip (that's how it's stored).
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.disabled_sources, s.disabled_sources);

        // Older rows without these fields still load.
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert!(old.disabled_sources.is_empty());
        println!("settings_round_trip_and_sanitize OK");
    }
}
