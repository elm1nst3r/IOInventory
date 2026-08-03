use crate::model::{Domain, Item};
use std::path::{Path, PathBuf};

/// Walk the configured workspace roots for git repositories and infer each
/// repo's tech stack and launch command from its manifest files. Everything
/// here is filesystem reads (no `git` subprocess) to keep the scan fast.
pub async fn collect(roots: &[PathBuf]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for root in roots {
        if !root.exists() {
            continue;
        }
        // Depth-bounded walk: repos live directly in a root or one level down.
        let walker = walkdir::WalkDir::new(root)
            .min_depth(1)
            .max_depth(3)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                // Don't descend into dependency/build dirs or nested .git internals.
                !matches!(
                    name.as_ref(),
                    "node_modules" | ".git" | "target" | "dist" | "build" | ".venv" | "venv"
                )
            });

        for entry in walker.filter_map(|e| e.ok()) {
            if !entry.file_type().is_dir() {
                continue;
            }
            let repo = entry.path();
            if !repo.join(".git").exists() {
                continue;
            }
            if !seen.insert(repo.to_path_buf()) {
                continue;
            }
            items.push(build_repo_item(repo));
        }
    }
    items
}

fn build_repo_item(repo: &Path) -> Item {
    let name = repo
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo.to_string_lossy().into_owned());

    let stacks = detect_stacks(repo);
    let launch_cmd = detect_launch(repo);
    let remote = read_remote(repo);
    let last_commit = read_last_commit(repo);

    let path_str = repo.to_string_lossy().into_owned();
    Item::new(Domain::Project, "git", name)
        .keyed(&path_str)
        .path(path_str.clone())
        .meta(serde_json::json!({
            "stacks": stacks,
            "launch_cmd": launch_cmd,
            "remote": remote,
            "last_commit": last_commit,
        }))
}

/// Detect tech stacks from the presence of manifest files.
fn detect_stacks(repo: &Path) -> Vec<String> {
    let mut stacks = Vec::new();
    let has = |f: &str| repo.join(f).exists();

    if has("package.json") {
        stacks.push(node_stack(repo));
    }
    if has("Cargo.toml") {
        stacks.push("Rust".into());
    }
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") || has("Pipfile") {
        stacks.push("Python".into());
    }
    if has("go.mod") {
        stacks.push("Go".into());
    }
    if has("Gemfile") {
        stacks.push("Ruby".into());
    }
    if has("pom.xml") || has("build.gradle") || has("build.gradle.kts") {
        stacks.push("Java/Kotlin".into());
    }
    if has("composer.json") {
        stacks.push("PHP".into());
    }
    if has("pubspec.yaml") {
        stacks.push("Flutter/Dart".into());
    }
    if has("Package.swift") || has_ext(repo, "xcodeproj") {
        stacks.push("Swift".into());
    }
    if has("Dockerfile") || has("docker-compose.yml") || has("compose.yaml") {
        stacks.push("Docker".into());
    }
    if stacks.is_empty() {
        stacks.push("Unknown".into());
    }
    stacks
}

/// Refine a Node project into a framework label when we can spot one.
fn node_stack(repo: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(repo.join("package.json")) else {
        return "Node.js".into();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return "Node.js".into();
    };
    let deps_has = |k: &str| {
        v.get("dependencies").and_then(|d| d.get(k)).is_some()
            || v.get("devDependencies").and_then(|d| d.get(k)).is_some()
    };
    if deps_has("next") {
        "Next.js".into()
    } else if deps_has("@tauri-apps/api") {
        "Tauri".into()
    } else if deps_has("react-native") || deps_has("expo") {
        "React Native".into()
    } else if deps_has("vue") {
        "Vue".into()
    } else if deps_has("svelte") {
        "Svelte".into()
    } else if deps_has("react") {
        "React".into()
    } else {
        "Node.js".into()
    }
}

fn has_ext(repo: &Path, ext: &str) -> bool {
    std::fs::read_dir(repo)
        .map(|entries| {
            entries.flatten().any(|e| {
                e.path()
                    .extension()
                    .map(|x| x == ext)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Infer the command to launch/open the project.
fn detect_launch(repo: &Path) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(repo.join("package.json")) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(scripts) = v.get("scripts").and_then(|s| s.as_object()) {
                for key in ["dev", "start", "serve"] {
                    if scripts.contains_key(key) {
                        return Some(format!("npm run {key}"));
                    }
                }
            }
        }
    }
    if repo.join("Cargo.toml").exists() {
        return Some("cargo run".into());
    }
    if repo.join("Makefile").exists() {
        return Some("make".into());
    }
    if repo.join("docker-compose.yml").exists() || repo.join("compose.yaml").exists() {
        return Some("docker compose up".into());
    }
    None
}

/// Read the origin remote URL straight from .git/config.
fn read_remote(repo: &Path) -> Option<String> {
    let cfg = std::fs::read_to_string(repo.join(".git/config")).ok()?;
    let mut in_origin = false;
    for line in cfg.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_origin = l.contains("remote \"origin\"");
        } else if in_origin && l.starts_with("url") {
            return l.split_once('=').map(|(_, u)| u.trim().to_string());
        }
    }
    None
}

/// Read the most recent commit timestamp from .git/logs/HEAD (last field pair
/// is `<unix_ts> <tz>`), returned as an ISO date.
fn read_last_commit(repo: &Path) -> Option<String> {
    let log = std::fs::read_to_string(repo.join(".git/logs/HEAD")).ok()?;
    let last = log.lines().filter(|l| !l.trim().is_empty()).last()?;
    // Format: "<old> <new> <name> <email> <ts> <tz>\t<message>"
    let head = last.split('\t').next().unwrap_or(last);
    let tokens: Vec<&str> = head.split_whitespace().collect();
    // Timestamp is the second-to-last token before the timezone.
    if tokens.len() >= 2 {
        if let Ok(ts) = tokens[tokens.len() - 2].parse::<i64>() {
            let dt = chrono::DateTime::from_timestamp(ts, 0)?;
            return Some(dt.format("%Y-%m-%d").to_string());
        }
    }
    None
}
