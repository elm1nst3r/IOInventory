use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;

#[derive(Clone)]
struct ScanDiagnostics {
    source: String,
    warnings: std::sync::Arc<std::sync::Mutex<Vec<crate::model::ScanWarning>>>,
}

tokio::task_local! {
    static SCAN_DIAGNOSTICS: ScanDiagnostics;
}

/// Run one collector with command failures attributed to that source.
pub async fn with_scan_diagnostics<F, T>(
    source: &str,
    warnings: std::sync::Arc<std::sync::Mutex<Vec<crate::model::ScanWarning>>>,
    future: F,
) -> T
where
    F: std::future::Future<Output = T>,
{
    SCAN_DIAGNOSTICS
        .scope(
            ScanDiagnostics {
                source: source.to_string(),
                warnings,
            },
            future,
        )
        .await
}

fn record_scan_warning(message: String) {
    let message = message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(500)
        .collect();
    let _ = SCAN_DIAGNOSTICS.try_with(|diagnostics| {
        if let Ok(mut warnings) = diagnostics.warnings.lock() {
            warnings.push(crate::model::ScanWarning {
                source: diagnostics.source.clone(),
                message,
            });
        }
    });
}

/// Default per-command timeout so a hung tool can never block a scan.
pub const CMD_TIMEOUT: Duration = Duration::from_secs(4);

/// A GUI app launched from Finder inherits a minimal PATH that omits
/// Homebrew, cargo, etc. Build an augmented PATH covering the usual install
/// locations so `which` and command execution find the real binaries.
fn augmented_path() -> &'static OsStr {
    static PATH: OnceLock<OsString> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let home_dir = dirs::home_dir();

        #[cfg(not(target_os = "windows"))]
        for candidate in [
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
            "/usr/local/sbin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ] {
            dirs.push(PathBuf::from(candidate));
        }
        if let Some(h) = &home_dir {
            for sub in [".cargo/bin", ".local/bin", "go/bin", ".volta/bin", ".asdf/shims"] {
                dirs.push(h.join(sub));
            }
            #[cfg(target_os = "windows")]
            for sub in ["AppData/Roaming/npm", "scoop/shims"] {
                dirs.push(h.join(sub));
            }
        }
        // Preserve anything already on PATH (dev runs inherit a rich PATH).
        if let Some(existing) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&existing));
        }
        // Dedup while preserving order.
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<PathBuf> = dirs
            .into_iter()
            .filter_map(|d| {
                let s = d.to_string_lossy().into_owned();
                if s.is_empty() || !seen.insert(s.clone()) {
                    None
                } else {
                    Some(d)
                }
            })
            .collect();
        std::env::join_paths(unique).unwrap_or_default()
    })
}

#[cfg(target_os = "windows")]
fn executable_names(cmd: &str) -> Vec<OsString> {
    if Path::new(cmd).extension().is_some() {
        return vec![OsString::from(cmd)];
    }
    let extensions = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let mut names = vec![OsString::from(cmd)];
    names.extend(
        extensions
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(|extension| OsString::from(format!("{cmd}{extension}"))),
    );
    names
}

#[cfg(not(target_os = "windows"))]
fn executable_names(cmd: &str) -> Vec<OsString> {
    vec![OsString::from(cmd)]
}

/// Locate an executable on the augmented PATH. Returns the absolute path.
pub fn which(cmd: &str) -> Option<PathBuf> {
    for dir in std::env::split_paths(augmented_path()) {
        for name in executable_names(cmd) {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
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
    let bin = match which(cmd) {
        Some(bin) => bin,
        None => {
            record_scan_warning(format!("{cmd} was not found"));
            return None;
        }
    };
    let mut c = Command::new(bin);
    c.args(args);
    c.env("PATH", augmented_path());
    c.kill_on_drop(true);
    match tokio::time::timeout(timeout, c.output()).await {
        Ok(Ok(out)) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(Ok(out)) => {
            record_scan_warning(format!(
                "{} exited with {}: {}",
                cmd,
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
            None
        }
        Ok(Err(error)) => {
            record_scan_warning(format!("failed to run {cmd}: {error}"));
            None
        }
        Err(_) => {
            record_scan_warning(format!("{cmd} timed out after {}s", timeout.as_secs()));
            None
        }
    }
}

/// Like `run`, but returns combined stdout+stderr regardless of exit status.
/// Used by cleanup previews where non-zero exits still carry useful output.
pub async fn run_capture(cmd: &str, args: &[&str], timeout: Duration) -> (bool, String) {
    run_capture_inner(cmd, args, timeout, true).await
}

/// Capture output without treating a non-zero status as a scan warning. Some
/// package managers use non-zero to mean "updates are available".
pub async fn run_capture_untracked(
    cmd: &str,
    args: &[&str],
    timeout: Duration,
) -> (bool, String) {
    run_capture_inner(cmd, args, timeout, false).await
}

async fn run_capture_inner(
    cmd: &str,
    args: &[&str],
    timeout: Duration,
    track_nonzero_status: bool,
) -> (bool, String) {
    let bin = match which(cmd) {
        Some(b) => b,
        None => {
            record_scan_warning(format!("{cmd} was not found"));
            return (false, format!("{cmd} not found"));
        }
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
            let success = out.status.success();
            if track_nonzero_status && !success {
                record_scan_warning(format!("{cmd} exited with {}: {}", out.status, s.trim()));
            }
            (success, s)
        }
        Ok(Err(e)) => {
            record_scan_warning(format!("failed to run {cmd}: {e}"));
            (false, format!("failed to run: {e}"))
        }
        Err(_) => {
            record_scan_warning(format!("{cmd} timed out after {}s", timeout.as_secs()));
            (false, "timed out".into())
        }
    }
}

static MUTATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub struct MutationGuard {
    _process_guard: tokio::sync::MutexGuard<'static, ()>,
    lock_path: PathBuf,
}

impl Drop for MutationGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[cfg(test)]
fn mutation_lock_path() -> PathBuf {
    std::env::temp_dir().join(format!("ioinv-operation-test-{}.lock", std::process::id()))
}

#[cfg(not(test))]
fn mutation_lock_path() -> PathBuf {
    crate::db::default_path().with_file_name("operation.lock")
}

fn claim_mutation_file(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let create = || {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        writeln!(file, "{}", std::process::id())?;
        Ok::<_, std::io::Error>(())
    };
    match create() {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let owner_running = mutation_owner_is_running(path);
            let stale = std::fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(24 * 60 * 60));
            if (!owner_running || stale) && std::fs::remove_file(path).is_ok() {
                create().is_ok()
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

#[cfg(unix)]
fn mutation_owner_is_running(path: &Path) -> bool {
    let Ok(pid) = std::fs::read_to_string(path).map(|value| value.trim().to_string()) else {
        return true;
    };
    if pid.parse::<u32>().is_err() {
        return true;
    }
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid])
        .status()
        .map(|status| status.success())
        .unwrap_or(true)
}

#[cfg(target_os = "windows")]
fn mutation_owner_is_running(path: &Path) -> bool {
    let Ok(pid) = std::fs::read_to_string(path).map(|value| value.trim().to_string()) else {
        return true;
    };
    if pid.parse::<u32>().is_err() {
        return true;
    }
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")))
        .unwrap_or(true)
}

#[cfg(not(any(unix, target_os = "windows")))]
fn mutation_owner_is_running(_path: &Path) -> bool {
    true
}

/// Destructive package-manager operations share one process-wide lock so two
/// updates, removals, installs, or cleanups cannot run at the same time.
pub fn try_mutation_lock() -> Option<MutationGuard> {
    let process_guard = MUTATION_LOCK.try_lock().ok()?;
    let lock_path = mutation_lock_path();
    if !claim_mutation_file(&lock_path) {
        return None;
    }
    Some(MutationGuard {
        _process_guard: process_guard,
        lock_path,
    })
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

/// Machine hostname, or a stable placeholder when it can't be read.
pub fn host_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "this-machine".into())
}

/// Human-facing OS name recorded on every scan.
pub fn os_name() -> String {
    #[cfg(target_os = "macos")]
    {
        "macOS".to_string()
    }
    #[cfg(target_os = "windows")]
    {
        "Windows".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::env::consts::OS.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_lock_rejects_overlapping_operations() {
        let first = try_mutation_lock().expect("first operation should acquire the lock");
        assert!(try_mutation_lock().is_none());
        drop(first);
        assert!(try_mutation_lock().is_some());
    }
}
