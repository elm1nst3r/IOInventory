//! Duplicate installs: the same tool present more than once on PATH.
//!
//! Only the first copy in PATH order ever runs, so updating any other copy —
//! the Homebrew one that `brew upgrade` dutifully refreshes, say — changes
//! nothing you actually use. That's invisible from inside any one package
//! manager, which is why it gets its own pass over the finished scan.
//!
//! Order comes from the user's interactive shell PATH (see
//! [`util::login_shell_path`]), falling back to the app's augmented PATH.

use super::util;
use crate::model::{Domain, Item};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

/// Commands worth checking: things people install more than one way.
const COMMANDS: &[&str] = &[
    "claude", "codex", "gemini", "copilot", "aider", "node", "npm", "pnpm", "bun", "deno",
    "python3", "pip3", "uv", "ruby", "gem", "go", "rustc", "cargo",
];

/// One installed copy of a command.
struct Copy {
    path: PathBuf,
    version: Option<String>,
}

/// Annotate the items for a conflicted command with its `copies`, and add a
/// `path-conflict` item for each one so it shows up even when nothing else in
/// the inventory represents that command (npm, pip3, cargo…).
pub async fn mark(items: &mut Vec<Item>, shell_path: Option<OsString>) {
    let from_shell = shell_path.is_some();
    let path = shell_path.unwrap_or_else(|| util::augmented_path().to_os_string());

    let mut found: Vec<(&str, Vec<PathBuf>)> = COMMANDS
        .iter()
        .map(|cmd| (*cmd, without_hidden_system_copies(util::which_all_in(cmd, &path))))
        .filter(|(_, copies)| copies.len() > 1)
        .collect();
    if found.is_empty() {
        return;
    }

    // Versions only for the conflicted ones, all probed at once.
    let mut probes = tokio::task::JoinSet::new();
    for (ci, (cmd, copies)) in found.iter().enumerate() {
        for (pi, p) in copies.iter().enumerate() {
            let p = p.to_string_lossy().into_owned();
            let arg = if *cmd == "go" { "version" } else { "--version" };
            probes.spawn(async move {
                let out = util::run_with(&p, &[arg], Duration::from_secs(3)).await;
                (ci, pi, out.and_then(|o| first_version(&o)))
            });
        }
    }
    let mut versions = std::collections::HashMap::new();
    while let Some(Ok((ci, pi, v))) = probes.join_next().await {
        versions.insert((ci, pi), v);
    }

    for (ci, (cmd, copies)) in found.drain(..).enumerate() {
        let copies: Vec<Copy> = copies
            .into_iter()
            .enumerate()
            .map(|(pi, path)| Copy { version: versions.remove(&(ci, pi)).flatten(), path })
            .collect();
        let json: Vec<serde_json::Value> = copies
            .iter()
            .enumerate()
            .map(|(i, c)| {
                serde_json::json!({
                    "path": c.path.to_string_lossy(),
                    "version": c.version,
                    "active": i == 0,
                    "homebrew": util::is_homebrew_owned(&c.path),
                })
            })
            .collect();

        // The existing item for this tool, if any: an AI CLI by agent slug, a
        // runtime by its binary.
        for item in items.iter_mut() {
            let by_agent = item.collector == "ai-cli"
                && item.metadata.get("agent").and_then(|a| a.as_str()) == Some(cmd);
            let by_binary = item.metadata.get("binary").and_then(|b| b.as_str()) == Some(cmd);
            if (by_agent || by_binary) && item.metadata.is_object() {
                item.metadata["copies"] = serde_json::Value::Array(json.clone());
            }
        }

        let active = &copies[0];
        let mut item = Item::new(Domain::Runtime, "path-conflict", cmd)
            .path(active.path.to_string_lossy().into_owned())
            .meta(serde_json::json!({
                "binary": cmd,
                "copies": json,
                "order_from_shell": from_shell,
                "description": summary(cmd, &copies),
            }));
        if let Some(v) = &active.version {
            item = item.version(v.clone());
        }
        items.push(item);
    }
}

/// macOS ships its own `python3`, `pip3`, `ruby`, `gem`… in `/usr/bin`. A newer
/// install shadowing those is the intended setup, not a conflict, so system
/// copies only count when one of them is the copy that *runs* — that's when a
/// newer install is being hidden.
fn without_hidden_system_copies(copies: Vec<PathBuf>) -> Vec<PathBuf> {
    let system = |p: &PathBuf| {
        ["/usr/bin/", "/bin/", "/usr/sbin/", "/System/"].iter().any(|d| p.starts_with(d))
    };
    copies
        .into_iter()
        .enumerate()
        .filter(|(i, p)| *i == 0 || !system(p))
        .map(|(_, p)| p)
        .collect()
}

/// "codex: 2 copies. ~/.local/bin/codex (0.167.0) runs; /opt/homebrew/bin/codex
/// (0.171.0) is never used." — plus the hint that matters when the hidden
/// copy is the newer one.
fn summary(cmd: &str, copies: &[Copy]) -> String {
    let home = util::home().to_string_lossy().into_owned();
    let show = |c: &Copy| {
        let p = c.path.to_string_lossy().replacen(&home, "~", 1);
        match &c.version {
            Some(v) => format!("{p} ({v})"),
            None => p,
        }
    };
    let hidden: Vec<String> = copies[1..].iter().map(show).collect();
    let mut s = format!(
        "{} copies of `{cmd}` on PATH. {} runs; {} {} never used.",
        copies.len(),
        show(&copies[0]),
        hidden.join(", "),
        if hidden.len() == 1 { "is" } else { "are" },
    );
    let newer_hidden = copies[1..].iter().any(|c| newer(c.version.as_deref(), copies[0].version.as_deref()));
    if newer_hidden {
        s.push_str(" A hidden copy is newer than the one that runs.");
    }
    s
}

/// Whether version `a` is strictly newer than `b`, comparing numeric parts.
fn newer(a: Option<&str>, b: Option<&str>) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
    };
    match (a, b) {
        (Some(a), Some(b)) => parts(a) > parts(b),
        _ => false,
    }
}

/// First dotted version number in a `--version` line ("codex-cli 0.171.0").
fn first_version(s: &str) -> Option<String> {
    s.split(|c: char| c.is_whitespace() || c == '(' || c == ',')
        .map(|t| t.trim_start_matches(['v', 'V']).trim_start_matches("go"))
        .find(|t| t.contains('.') && t.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(|t| t.trim_end_matches(|c: char| !c.is_ascii_alphanumeric()).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_copies_only_count_when_active() {
        let p = |s: &str| PathBuf::from(s);
        // Homebrew python first, macOS's behind it: intended, not a conflict.
        let kept = without_hidden_system_copies(vec![p("/opt/homebrew/bin/python3"), p("/usr/bin/python3")]);
        assert_eq!(kept.len(), 1);
        // The system copy runs and hides a newer install: that's the conflict.
        let kept = without_hidden_system_copies(vec![p("/usr/bin/python3"), p("/opt/homebrew/bin/python3")]);
        assert_eq!(kept.len(), 2);
        println!("system_copies_only_count_when_active OK");
    }

    #[test]
    fn versions_parse_and_compare() {
        assert_eq!(first_version("codex-cli 0.171.0").as_deref(), Some("0.171.0"));
        assert_eq!(first_version("2.1.330 (Claude Code)").as_deref(), Some("2.1.330"));
        assert_eq!(first_version("go version go1.23.4 darwin/arm64").as_deref(), Some("1.23.4"));
        assert_eq!(first_version("v22.11.0").as_deref(), Some("22.11.0"));
        assert!(newer(Some("0.171.0"), Some("0.167.0")));
        assert!(!newer(Some("0.9.0"), Some("0.10.0")));
        println!("versions_parse_and_compare OK");
    }

    /// Two copies of one command become a conflict item that names which runs,
    /// and the existing item for that tool is annotated with both.
    #[tokio::test]
    async fn duplicate_copies_are_reported() {
        let base = std::env::temp_dir().join(format!("ioinv-shadow-{}", std::process::id()));
        let (a, b) = (base.join("a"), base.join("b"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            let f = d.join("codex");
            std::fs::write(&f, "#!/bin/sh\necho x\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let path = std::env::join_paths([&a, &b]).unwrap();
        let mut items = vec![Item::new(Domain::AiAgent, "ai-cli", "OpenAI Codex")
            .meta(serde_json::json!({ "agent": "codex" }))];
        mark(&mut items, Some(path)).await;
        let _ = std::fs::remove_dir_all(&base);

        let conflict = items.iter().find(|i| i.collector == "path-conflict").expect("conflict item");
        assert_eq!(conflict.name, "codex");
        let copies = conflict.metadata["copies"].as_array().unwrap();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0]["active"], true);
        assert!(copies[0]["path"].as_str().unwrap().contains("/a/codex"));
        assert_eq!(items[0].metadata["copies"].as_array().map(|c| c.len()), Some(2));
        println!("duplicate_copies_are_reported OK — {}", conflict.metadata["description"]);
    }
}
