use crate::scan::util;
use serde::Serialize;
use std::time::Duration;

/// Per-collector update / uninstall command templates. `{name}` is replaced
/// with the item name at call time — always passed as a single process
/// argument (never through a shell), so there's no injection surface.
struct ManageSpec {
    collector: &'static str,
    tool: &'static str,
    update: Option<&'static [&'static str]>,
    delete: Option<&'static [&'static str]>,
    install: Option<&'static [&'static str]>,
}

const MANAGE: &[ManageSpec] = &[
    ManageSpec {
        collector: "homebrew",
        tool: "brew",
        update: Some(&["upgrade", "{name}"]),
        delete: Some(&["uninstall", "{name}"]),
        install: Some(&["install", "{name}"]),
    },
    ManageSpec {
        collector: "homebrew-cask",
        tool: "brew",
        update: Some(&["upgrade", "--cask", "{name}"]),
        delete: Some(&["uninstall", "--cask", "{name}"]),
        install: Some(&["install", "--cask", "{name}"]),
    },
    ManageSpec {
        collector: "npm",
        tool: "npm",
        update: Some(&["update", "-g", "{name}"]),
        delete: Some(&["uninstall", "-g", "{name}"]),
        install: Some(&["install", "-g", "{name}"]),
    },
    ManageSpec {
        collector: "pnpm",
        tool: "pnpm",
        update: Some(&["update", "-g", "{name}"]),
        delete: Some(&["remove", "-g", "{name}"]),
        install: Some(&["add", "-g", "{name}"]),
    },
    ManageSpec {
        collector: "pip",
        tool: "pip3",
        update: Some(&["install", "--upgrade", "{name}"]),
        delete: Some(&["uninstall", "-y", "{name}"]),
        install: Some(&["install", "{name}"]),
    },
    ManageSpec {
        collector: "python-ai-lib",
        tool: "pip3",
        update: Some(&["install", "--upgrade", "{name}"]),
        delete: Some(&["uninstall", "-y", "{name}"]),
        install: Some(&["install", "{name}"]),
    },
    ManageSpec {
        collector: "pipx",
        tool: "pipx",
        update: Some(&["upgrade", "{name}"]),
        delete: Some(&["uninstall", "{name}"]),
        install: Some(&["install", "{name}"]),
    },
    ManageSpec {
        collector: "gem",
        tool: "gem",
        update: Some(&["update", "{name}"]),
        delete: Some(&["uninstall", "{name}"]),
        install: Some(&["install", "{name}"]),
    },
    ManageSpec {
        collector: "cargo",
        tool: "cargo",
        update: None,
        delete: Some(&["uninstall", "{name}"]),
        install: Some(&["install", "{name}"]),
    },
    ManageSpec {
        collector: "ollama",
        tool: "ollama",
        update: Some(&["pull", "{name}"]),
        delete: Some(&["rm", "{name}"]),
        install: Some(&["pull", "{name}"]),
    },
    ManageSpec {
        collector: "docker-image",
        tool: "docker",
        update: None,
        delete: Some(&["rmi", "{name}"]),
        install: Some(&["pull", "{name}"]),
    },
    ManageSpec {
        collector: "docker-container",
        tool: "docker",
        update: None,
        delete: Some(&["rm", "{name}"]),
        install: None,
    },
];

/// What actions are available for an item, with the exact command that will run.
#[derive(Serialize, Default)]
pub struct ActionInfo {
    pub update: Option<String>,
    pub delete: Option<String>,
    pub install: Option<String>,
    /// Whether the underlying tool is installed.
    pub available: bool,
    /// Why nothing is on offer, when that's worth explaining rather than just
    /// showing no buttons (a macOS system app, say).
    pub note: Option<String>,
}

// ------------------------------------------------------------- applications
//
// Apps don't come from a package manager, so they don't fit the command
// template above. Two cases:
//
//   * Homebrew installed it  -> `brew uninstall --cask <token>`, because
//     dragging a cask's app to the Trash leaves brew believing it's installed.
//   * Anything else          -> move the bundle to the Trash, which is
//     reversible; a hard delete of something the user dragged in isn't.
//
// macOS protects its own apps with the SF_RESTRICTED flag, and no amount of
// permission gets around it, so those are refused with an explanation.

/// The `.app` bundle directories the applications collector scans. Removal is
/// confined to these: `run_item_action` is reachable over MCP, and a "trash
/// this path" action that accepted anything would be an unrestricted delete
/// primitive rather than an allowlisted one.
#[cfg(target_os = "macos")]
fn app_roots() -> Vec<std::path::PathBuf> {
    vec![
        std::path::PathBuf::from("/Applications"),
        util::home().join("Applications"),
    ]
}

/// macOS marks its own bundles `restricted`; SIP refuses to remove them.
#[cfg(target_os = "macos")]
fn is_sip_protected(path: &std::path::Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    const SF_RESTRICTED: u32 = 0x0008_0000;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return true; // unrepresentable path — treat as untouchable
    };
    // SAFETY: `stat` writes into a zeroed struct we own, and the path is a
    // valid NUL-terminated C string for the duration of the call.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        if libc::lstat(c_path.as_ptr(), &mut st) != 0 {
            return true; // can't tell — assume protected
        }
        st.st_flags & SF_RESTRICTED != 0
    }
}

/// Whether this bundle may be moved to the Trash, or why not.
#[cfg(target_os = "macos")]
fn trashable(path: &std::path::Path) -> Result<(), String> {
    if path.extension().and_then(|e| e.to_str()) != Some("app") {
        return Err("Only .app bundles can be moved to the Trash from here.".into());
    }
    if !path.exists() {
        return Err("This app is no longer at the recorded path — re-scan to refresh.".into());
    }
    // Immediate child of a scanned root: no traversal, no nested targets.
    let in_root = app_roots()
        .iter()
        .any(|root| path.parent() == Some(root.as_path()));
    if !in_root {
        return Err("Only apps in /Applications can be removed from here.".into());
    }
    if is_sip_protected(path) {
        return Err("This is a macOS system app. It's protected by the OS and can't be removed.".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn app_info(name: &str, source_path: Option<&str>, cask: Option<&str>) -> ActionInfo {
    if let Some(token) = cask {
        return ActionInfo {
            delete: Some(format!("brew uninstall --cask {token}")),
            available: util::is_available("brew"),
            note: Some("Homebrew installed this app, so it's removed through Homebrew.".into()),
            ..Default::default()
        };
    }
    let Some(path) = source_path else {
        return ActionInfo::default();
    };
    match trashable(std::path::Path::new(path)) {
        Ok(()) => ActionInfo {
            delete: Some(format!("Move “{name}” to the Trash")),
            available: true,
            note: Some("Moved to the Trash, so you can put it back.".into()),
            ..Default::default()
        },
        Err(why) => ActionInfo { note: Some(why), ..Default::default() },
    }
}

#[cfg(not(target_os = "macos"))]
fn app_info(_name: &str, _source_path: Option<&str>, _cask: Option<&str>) -> ActionInfo {
    ActionInfo::default()
}


#[derive(Serialize)]
pub struct ActionResult {
    pub command: String,
    pub output: String,
    pub success: bool,
}

fn find(collector: &str) -> Option<&'static ManageSpec> {
    MANAGE.iter().find(|s| s.collector == collector)
}

fn effective_tool(spec: &ManageSpec) -> &'static str {
    if spec.tool == "pip3" && !util::is_available("pip3") && util::is_available("pip") {
        "pip"
    } else {
        spec.tool
    }
}

fn render(tool: &str, args: &[&str], name: &str) -> String {
    let filled: Vec<String> = args
        .iter()
        .map(|a| a.replace("{name}", name))
        .collect();
    format!("{} {}", tool, filled.join(" "))
}

pub fn info(collector: &str, name: &str, source_path: Option<&str>, cask: Option<&str>) -> ActionInfo {
    if collector == "app" {
        return app_info(name, source_path, cask);
    }
    let Some(spec) = find(collector) else {
        return ActionInfo::default();
    };
    let tool = effective_tool(spec);
    ActionInfo {
        update: spec.update.map(|a| render(tool, a, name)),
        delete: spec.delete.map(|a| render(tool, a, name)),
        install: spec.install.map(|a| render(tool, a, name)),
        available: util::is_available(tool),
        note: None,
    }
}

pub async fn run(
    collector: &str,
    name: &str,
    action: &str,
    source_path: Option<&str>,
    cask: Option<&str>,
) -> ActionResult {
    if collector == "app" {
        return run_app(name, action, source_path, cask).await;
    }
    let Some(spec) = find(collector) else {
        return ActionResult {
            command: String::new(),
            output: format!("No actions available for {collector}"),
            success: false,
        };
    };
    let template = match action {
        "update" => spec.update,
        "delete" => spec.delete,
        "install" => spec.install,
        _ => None,
    };
    let Some(template) = template else {
        return ActionResult {
            command: String::new(),
            output: format!("Action '{action}' not supported for {collector}"),
            success: false,
        };
    };

    // Substitute the name as a discrete argument (no shell involved).
    let tool = effective_tool(spec);
    let args: Vec<String> = template.iter().map(|a| a.replace("{name}", name)).collect();
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let command = format!("{} {}", tool, args.join(" "));

    let Some(_operation_guard) = util::try_mutation_lock() else {
        return ActionResult {
            command,
            output: "Another install, update, uninstall, or cleanup is already running.".into(),
            success: false,
        };
    };

    let (success, output) =
        util::run_capture(tool, &arg_refs, Duration::from_secs(600)).await;

    ActionResult {
        command,
        output: if output.trim().is_empty() {
            "(done)".into()
        } else {
            output.trim().into()
        },
        success,
    }
}

/// Remove an application: through Homebrew when a cask owns it, otherwise to
/// the Trash.
///
/// Every check from `trashable` runs again here rather than trusting whatever
/// `info` reported. The two calls are separated by a user confirmation, the
/// arguments arrive from the frontend or an MCP client, and this is the code
/// that actually deletes — so it re-derives its own verdict.
#[cfg(target_os = "macos")]
async fn run_app(
    name: &str,
    action: &str,
    source_path: Option<&str>,
    cask: Option<&str>,
) -> ActionResult {
    if action != "delete" {
        return ActionResult {
            command: String::new(),
            output: format!("Applications support only 'delete' (got '{action}')."),
            success: false,
        };
    }

    // A cask's app belongs to Homebrew; removing it any other way leaves brew
    // convinced it's still installed.
    if let Some(token) = cask {
        let command = format!("brew uninstall --cask {token}");
        let Some(_operation_guard) = util::try_mutation_lock() else {
            return ActionResult {
                command,
                output: "Another install, update, uninstall, or cleanup is already running.".into(),
                success: false,
            };
        };
        let (success, output) =
            util::run_capture("brew", &["uninstall", "--cask", token], Duration::from_secs(600))
                .await;
        return ActionResult {
            command,
            output: if output.trim().is_empty() { "(done)".into() } else { output.trim().into() },
            success,
        };
    }

    let command = format!("Move “{name}” to the Trash");
    let Some(path) = source_path else {
        return ActionResult {
            command,
            output: "No path recorded for this app — re-scan and try again.".into(),
            success: false,
        };
    };
    let path = std::path::Path::new(path);
    if let Err(why) = trashable(path) {
        return ActionResult { command, output: why, success: false };
    }

    let Some(_operation_guard) = util::try_mutation_lock() else {
        return ActionResult {
            command,
            output: "Another install, update, uninstall, or cleanup is already running.".into(),
            success: false,
        };
    };

    // `trash` goes through the platform API, so the item lands in the Trash
    // with its "Put Back" record intact rather than being moved by hand.
    match trash::delete(path) {
        Ok(()) => ActionResult {
            command,
            output: format!("Moved to the Trash. Recover it from there if you didn't mean to.\n\n{}", path.display()),
            success: true,
        },
        Err(error) => ActionResult {
            command,
            output: format!(
                "Could not move it to the Trash: {error}\n\nApps that are running, or that need an \
                 administrator, have to be quit or removed in Finder."
            ),
            success: false,
        },
    }
}

#[cfg(not(target_os = "macos"))]
async fn run_app(
    _name: &str,
    _action: &str,
    _source_path: Option<&str>,
    _cask: Option<&str>,
) -> ActionResult {
    ActionResult {
        command: String::new(),
        output: "Removing applications is only supported on macOS.".into(),
        success: false,
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::path::Path;

    /// The trash path is reachable over MCP, so its guards are the security
    /// boundary — not the confirm dialog in the UI.
    #[test]
    fn trash_refuses_anything_but_a_plain_app_bundle() {
        // Not an .app.
        assert!(trashable(Path::new("/Applications/../etc/passwd")).is_err());
        assert!(trashable(Path::new("/etc/hosts")).is_err());
        // An .app, but nowhere we scan.
        assert!(trashable(Path::new("/tmp/Evil.app")).is_err());
        // Nested inside a real root rather than sitting directly in it.
        assert!(trashable(Path::new("/Applications/Xcode.app/Contents/Frameworks/Nested.app")).is_err());

        // macOS system apps carry SF_RESTRICTED and must be refused with a
        // reason, not silently.
        if Path::new("/Applications/Safari.app").exists() {
            let why = trashable(Path::new("/Applications/Safari.app")).unwrap_err();
            assert!(why.contains("system app"), "unexpected reason: {why}");
        }
        println!("trash_refuses_anything_but_a_plain_app_bundle OK");
    }

    /// A cask's app must route to Homebrew: trashing it leaves brew convinced
    /// it's still installed.
    #[test]
    fn cask_apps_route_to_homebrew() {
        let info = app_info("WezTerm", Some("/Applications/WezTerm.app"), Some("wezterm"));
        assert_eq!(info.delete.as_deref(), Some("brew uninstall --cask wezterm"));

        // Without a cask the same app is a Trash candidate, and the offer is
        // explicit that it's recoverable.
        if Path::new("/Applications").exists() {
            let plain = app_info("Nope", Some("/Applications/DefinitelyNotInstalled.app"), None);
            assert!(plain.delete.is_none(), "a missing app must not be offered");
            assert!(plain.note.is_some(), "and it must say why");
        }
        println!("cask_apps_route_to_homebrew OK");
    }

    /// `run` re-derives its own verdict; a caller can't talk it into deleting
    /// something `info` would have refused.
    #[tokio::test]
    async fn run_revalidates_the_path() {
        let r = run_app("passwd", "delete", Some("/etc/passwd"), None).await;
        assert!(!r.success);
        assert!(r.output.contains(".app"), "unexpected output: {}", r.output);

        // Only 'delete' means anything for an app.
        let r = run_app("Notion", "update", Some("/Applications/Notion.app"), None).await;
        assert!(!r.success);
        assert!(r.output.contains("only 'delete'"), "unexpected output: {}", r.output);
        println!("run_revalidates_the_path OK");
    }
}
