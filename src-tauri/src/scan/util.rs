use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;

/// Default per-command timeout so a hung tool can never block a scan.
pub const CMD_TIMEOUT: Duration = Duration::from_secs(4);

/// A GUI app launched from Finder inherits a minimal PATH that omits
/// Homebrew, cargo, etc. Build an augmented PATH covering the usual install
/// locations so `which` and command execution find the real binaries.
fn augmented_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let home = dirs::home_dir();
        let candidates = [
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ];
        for c in candidates {
            dirs.push(PathBuf::from(c));
        }
        if let Some(h) = &home {
            for sub in [".cargo/bin", ".local/bin", "go/bin", ".volta/bin", ".asdf/shims"] {
                dirs.push(h.join(sub));
            }
        }
        // Preserve anything already on PATH (dev runs inherit a rich PATH).
        if let Ok(existing) = std::env::var("PATH") {
            for p in existing.split(':') {
                dirs.push(PathBuf::from(p));
            }
        }
        // Dedup while preserving order.
        let mut seen = std::collections::HashSet::new();
        let joined: Vec<String> = dirs
            .into_iter()
            .filter_map(|d| {
                let s = d.to_string_lossy().into_owned();
                if s.is_empty() || !seen.insert(s.clone()) {
                    None
                } else {
                    Some(s)
                }
            })
            .collect();
        joined.join(":")
    })
}

/// Locate an executable on the augmented PATH. Returns the absolute path.
pub fn which(cmd: &str) -> Option<PathBuf> {
    for dir in augmented_path().split(':') {
        let p = Path::new(dir).join(cmd);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn is_available(cmd: &str) -> bool {
    which(cmd).is_some()
}

/// Run a command with the augmented PATH and a timeout. Returns stdout on a
/// clean exit; None on timeout, spawn failure, or non-zero exit.
pub async fn run(cmd: &str, args: &[&str]) -> Option<String> {
    run_with(cmd, args, CMD_TIMEOUT).await
}

pub async fn run_with(cmd: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let bin = which(cmd)?;
    let mut c = Command::new(bin);
    c.args(args);
    c.env("PATH", augmented_path());
    c.kill_on_drop(true);
    match tokio::time::timeout(timeout, c.output()).await {
        Ok(Ok(out)) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        _ => None,
    }
}

/// Like `run`, but returns combined stdout+stderr regardless of exit status.
/// Used by cleanup previews where non-zero exits still carry useful output.
pub async fn run_capture(cmd: &str, args: &[&str], timeout: Duration) -> (bool, String) {
    let bin = match which(cmd) {
        Some(b) => b,
        None => return (false, format!("{cmd} not found")),
    };
    let mut c = Command::new(bin);
    c.args(args);
    c.env("PATH", augmented_path());
    c.kill_on_drop(true);
    match tokio::time::timeout(timeout, c.output()).await {
        Ok(Ok(out)) => {
            let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.trim().is_empty() {
                s.push_str(&err);
            }
            (out.status.success(), s)
        }
        Ok(Err(e)) => (false, format!("failed to run: {e}")),
        Err(_) => (false, "timed out".into()),
    }
}

/// Best-effort recursive size of a directory in bytes, bounded so it can't
/// dominate a scan. Skips symlinks.
pub fn dir_size(path: &Path) -> Option<i64> {
    if !path.exists() {
        return None;
    }
    let mut total: i64 = 0;
    let walker = walkdir::WalkDir::new(path)
        .max_depth(8)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in walker {
        if let Ok(md) = entry.metadata() {
            if md.is_file() {
                total += md.len() as i64;
            }
        }
    }
    Some(total)
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}
