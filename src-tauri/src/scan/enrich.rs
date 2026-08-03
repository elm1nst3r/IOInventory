use super::util;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// On-demand extra context for a single item, fetched only when the user
/// selects it (keeps scans fast). All fields are best-effort/optional.
#[derive(Default, Serialize)]
pub struct Enrichment {
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub latest_version: Option<String>,
    pub installed_version: Option<String>,
    pub outdated: Option<bool>,
    pub installed_at: Option<String>,
}

pub async fn enrich(collector: &str, name: &str, source_path: Option<String>) -> Enrichment {
    let mut e = match collector {
        "homebrew" => brew(name, false).await,
        "homebrew-cask" => brew(name, true).await,
        "pip" | "python-ai-lib" => pip(name).await,
        "npm" | "pnpm" => npm(name).await,
        "ollama" => ollama(name).await,
        "claude-skill" | "claude-command" | "claude-agent" => skill(&source_path).await,
        _ => Enrichment::default(),
    };
    // Universal fallback: derive an install/updated date from the path mtime.
    if e.installed_at.is_none() {
        if let Some(p) = &source_path {
            e.installed_at = mtime_date(Path::new(p));
        }
    }
    e
}

async fn brew(name: &str, cask: bool) -> Enrichment {
    let mut e = Enrichment::default();
    let mut args = vec!["info", "--json=v2"];
    if cask {
        args.push("--cask");
    }
    args.push(name);
    let Some(out) = util::run_with("brew", &args, Duration::from_secs(12)).await else {
        e.installed_at = brew_install_date(name, cask);
        return e;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) else {
        return e;
    };
    let arr_key = if cask { "casks" } else { "formulae" };
    if let Some(item) = v.get(arr_key).and_then(|a| a.as_array()).and_then(|a| a.first()) {
        e.description = item
            .get("desc")
            .and_then(|d| d.as_str())
            .map(str::to_string);
        e.homepage = item
            .get("homepage")
            .and_then(|h| h.as_str())
            .map(str::to_string);
        e.outdated = item.get("outdated").and_then(|o| o.as_bool());
        if cask {
            e.latest_version = item.get("version").and_then(|s| s.as_str()).map(str::to_string);
            e.installed_version = item
                .get("installed")
                .and_then(|s| s.as_str())
                .map(str::to_string);
        } else {
            e.latest_version = item
                .pointer("/versions/stable")
                .and_then(|s| s.as_str())
                .map(str::to_string);
            e.installed_version = item
                .get("installed")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.get("version"))
                .and_then(|s| s.as_str())
                .map(str::to_string);
        }
    }
    e.installed_at = brew_install_date(name, cask);
    e
}

/// Install date from the Cellar/Caskroom directory mtime (no subprocess).
fn brew_install_date(name: &str, cask: bool) -> Option<String> {
    let sub = if cask { "Caskroom" } else { "Cellar" };
    for prefix in ["/opt/homebrew", "/usr/local"] {
        let p = PathBuf::from(prefix).join(sub).join(name);
        if p.exists() {
            return mtime_date(&p);
        }
    }
    None
}

async fn pip(name: &str) -> Enrichment {
    let mut e = Enrichment::default();
    let bin = if util::is_available("pip3") { "pip3" } else { "pip" };
    let Some(out) = util::run_with(bin, &["show", name], Duration::from_secs(8)).await else {
        return e;
    };
    for line in out.lines() {
        if let Some(v) = line.strip_prefix("Summary:") {
            let s = v.trim();
            if !s.is_empty() && s != "UNKNOWN" {
                e.description = Some(s.to_string());
            }
        } else if let Some(v) = line.strip_prefix("Home-page:") {
            let s = v.trim();
            if !s.is_empty() && s != "UNKNOWN" {
                e.homepage = Some(s.to_string());
            }
        } else if let Some(v) = line.strip_prefix("Version:") {
            e.installed_version = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Location:") {
            // Derive install date from the package's dist-info dir mtime.
            let loc = v.trim();
            e.installed_at = pip_install_date(loc, name);
        }
    }
    e
}

fn pip_install_date(location: &str, name: &str) -> Option<String> {
    let dir = Path::new(location);
    let norm = name.replace('-', "_").to_lowercase();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_lowercase();
            if fname.ends_with(".dist-info") && fname.starts_with(&norm) {
                return mtime_date(&entry.path());
            }
        }
    }
    None
}

async fn npm(name: &str) -> Enrichment {
    let mut e = Enrichment::default();
    let Some(out) = util::run_with(
        "npm",
        &["view", name, "description", "homepage", "version", "--json"],
        Duration::from_secs(10),
    )
    .await
    else {
        return e;
    };
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) {
        e.description = v.get("description").and_then(|d| d.as_str()).map(str::to_string);
        e.homepage = v.get("homepage").and_then(|h| h.as_str()).map(str::to_string);
        e.latest_version = v.get("version").and_then(|s| s.as_str()).map(str::to_string);
    }
    e
}

async fn ollama(name: &str) -> Enrichment {
    let mut e = Enrichment::default();
    if let Some(out) = util::run_with("ollama", &["show", name], Duration::from_secs(8)).await {
        // Use the compact "Model"/parameters section as a description.
        let summary: Vec<&str> = out
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(8)
            .collect();
        if !summary.is_empty() {
            e.description = Some(summary.join(" · "));
        }
    }
    e
}

/// Read a Claude skill/command description from its SKILL.md frontmatter.
async fn skill(source_path: &Option<String>) -> Enrichment {
    let mut e = Enrichment::default();
    let Some(base) = source_path else {
        return e;
    };
    let base = Path::new(base);
    let candidates = [
        base.join("SKILL.md"),
        base.join("skill.md"),
        base.to_path_buf(),
    ];
    for c in candidates {
        if c.is_file() {
            if let Ok(text) = std::fs::read_to_string(&c) {
                if let Some(desc) = frontmatter_description(&text) {
                    e.description = Some(desc);
                }
                e.installed_at = mtime_date(&c);
                break;
            }
        }
    }
    if e.installed_at.is_none() {
        e.installed_at = mtime_date(base);
    }
    e
}

/// Pull `description:` out of a YAML frontmatter block.
fn frontmatter_description(text: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(v) = line.strip_prefix("description:") {
            let s = v.trim().trim_matches('"').trim_matches('\'').trim();
            if !s.is_empty() {
                let truncated: String = s.chars().take(400).collect();
                return Some(truncated);
            }
        }
    }
    None
}

fn mtime_date(path: &Path) -> Option<String> {
    let md = std::fs::metadata(path).ok()?;
    let modified = md.modified().ok()?;
    let dt: chrono::DateTime<chrono::Local> = modified.into();
    Some(dt.format("%Y-%m-%d").to_string())
}
