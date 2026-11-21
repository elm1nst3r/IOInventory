use crate::model::{CleanupAction, CleanupPreview, CleanupResult};
use crate::scan::util;
use std::time::Duration;

type Step = (&'static str, &'static [&'static str]);

/// How a spec finds the binary it runs, and whether it can run here at all.
#[derive(Clone, Copy, PartialEq)]
enum Resolve {
    /// The tool by name on the augmented PATH.
    Tool,
    /// A tool that updates itself (`claude update`, `uv self update`…). Run the
    /// copy *outside* Homebrew by absolute path: a Homebrew copy is updated by
    /// `brew upgrade`, and the app's PATH puts Homebrew first, so running the
    /// bare name could hit the wrong copy when both exist.
    SelfUpdate,
    /// RubyGems — but not macOS's built-in Ruby, where `gem update` needs sudo.
    UserRuby,
}

/// A safe, allowlisted utility action: a preview (dry-run / status probe) and
/// one or more run steps. Nothing outside this list can ever be executed.
struct Spec {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    /// "update", "cleanup", or "system" (check-only; never run from here).
    category: &'static str,
    /// The tool the steps run; also what availability is checked against.
    tool: &'static str,
    resolve: Resolve,
    /// Preview command (tool, args).
    preview: Step,
    /// Sequential run steps. Empty for check-only actions.
    steps: &'static [Step],
    /// Display string for the command(s) that will run.
    command: &'static str,
}

const SPECS: &[Spec] = &[
    // ---- Updates (also the order "Update everything" runs them in) ----
    Spec {
        id: "brew-upgrade",
        title: "Update Homebrew packages",
        description: "Refresh formula/cask definitions, then upgrade everything that's outdated — \
                      including apps that normally update themselves.",
        category: "update",
        tool: "brew",
        resolve: Resolve::Tool,
        preview: ("brew", &["outdated", "--verbose", "--greedy"]),
        // --greedy, to match the preview and the scan's outdated flags. Without
        // it, casks that self-update were listed as "will change" and then
        // silently skipped.
        steps: &[("brew", &["update"]), ("brew", &["upgrade", "--greedy"])],
        command: "brew update && brew upgrade --greedy",
    },
    Spec {
        id: "npm-upgrade",
        title: "Update global npm packages",
        description: "Update all globally-installed npm packages to their latest versions.",
        category: "update",
        tool: "npm",
        resolve: Resolve::Tool,
        preview: ("npm", &["outdated", "-g"]),
        steps: &[("npm", &["update", "-g"])],
        command: "npm update -g",
    },
    Spec {
        id: "pnpm-upgrade",
        title: "Update global pnpm packages",
        description: "Update all globally-installed pnpm packages.",
        category: "update",
        tool: "pnpm",
        resolve: Resolve::Tool,
        preview: ("pnpm", &["outdated", "-g"]),
        steps: &[("pnpm", &["update", "-g"])],
        command: "pnpm update -g",
    },
    Spec {
        id: "rustup-update",
        title: "Update Rust toolchains",
        description: "Update rustup and all installed Rust toolchains to the latest release.",
        category: "update",
        tool: "rustup",
        resolve: Resolve::Tool,
        preview: ("rustup", &["check"]),
        steps: &[("rustup", &["update"])],
        command: "rustup update",
    },
    Spec {
        id: "pipx-upgrade",
        title: "Update pipx applications",
        description: "Upgrade every application installed with pipx to its latest version.",
        category: "update",
        tool: "pipx",
        resolve: Resolve::Tool,
        preview: ("pipx", &["list", "--short"]),
        steps: &[("pipx", &["upgrade-all"])],
        command: "pipx upgrade-all",
    },
    Spec {
        id: "uv-self",
        title: "Update uv",
        description: "Update the standalone uv installer to its latest release.",
        category: "update",
        tool: "uv",
        resolve: Resolve::SelfUpdate,
        preview: ("uv", &["self", "update", "--dry-run"]),
        steps: &[("uv", &["self", "update"])],
        command: "uv self update",
    },
    Spec {
        id: "uv-tools",
        title: "Update uv tools",
        description: "Upgrade every tool installed with `uv tool install`.",
        category: "update",
        tool: "uv",
        resolve: Resolve::Tool,
        preview: ("uv", &["tool", "list"]),
        steps: &[("uv", &["tool", "upgrade", "--all"])],
        command: "uv tool upgrade --all",
    },
    Spec {
        id: "cargo-update",
        title: "Update cargo binaries",
        description: "Update crates installed via `cargo install` (needs the cargo-update crate).",
        category: "update",
        tool: "cargo-install-update",
        resolve: Resolve::Tool,
        preview: ("cargo", &["install-update", "-l"]),
        steps: &[("cargo", &["install-update", "-a"])],
        command: "cargo install-update -a",
    },
    Spec {
        id: "gem-update",
        title: "Update Ruby gems",
        description: "Update every installed gem to its latest version.",
        category: "update",
        tool: "gem",
        resolve: Resolve::UserRuby,
        preview: ("gem", &["outdated"]),
        steps: &[("gem", &["update"])],
        command: "gem update",
    },
    Spec {
        id: "gh-upgrade",
        title: "Update GitHub CLI extensions",
        description: "Upgrade all installed `gh` extensions to their latest versions.",
        category: "update",
        tool: "gh",
        resolve: Resolve::Tool,
        preview: ("gh", &["extension", "list"]),
        steps: &[("gh", &["extension", "upgrade", "--all"])],
        command: "gh extension upgrade --all",
    },
    Spec {
        id: "deno-upgrade",
        title: "Update Deno",
        description: "Upgrade a standalone Deno install to the latest release.",
        category: "update",
        tool: "deno",
        resolve: Resolve::SelfUpdate,
        preview: ("deno", &["upgrade", "--dry-run"]),
        steps: &[("deno", &["upgrade"])],
        command: "deno upgrade",
    },
    Spec {
        id: "bun-upgrade",
        title: "Update Bun",
        description: "Upgrade a standalone Bun install to the latest release.",
        category: "update",
        tool: "bun",
        resolve: Resolve::SelfUpdate,
        preview: ("bun", &["--version"]),
        steps: &[("bun", &["upgrade"])],
        command: "bun upgrade",
    },
    Spec {
        id: "claude-update",
        title: "Update Claude Code",
        description: "Update the native Claude Code install to its latest version.",
        category: "update",
        tool: "claude",
        resolve: Resolve::SelfUpdate,
        preview: ("claude", &["--version"]),
        steps: &[("claude", &["update"])],
        command: "claude update",
    },
    Spec {
        id: "codex-update",
        title: "Update Codex",
        description: "Update the standalone Codex CLI to its latest version.",
        category: "update",
        tool: "codex",
        resolve: Resolve::SelfUpdate,
        preview: ("codex", &["--version"]),
        steps: &[("codex", &["update"])],
        command: "codex update",
    },
    Spec {
        id: "vscode-extensions",
        title: "Update VS Code extensions",
        description: "Update every installed VS Code extension to its latest version.",
        category: "update",
        tool: "code",
        resolve: Resolve::Tool,
        preview: ("code", &["--list-extensions", "--show-versions"]),
        steps: &[("code", &["--update-extensions"])],
        command: "code --update-extensions",
    },
    Spec {
        id: "ollama-models",
        title: "Update Ollama models",
        description: "Pull the latest version of every local Ollama model.",
        category: "update",
        tool: "ollama",
        resolve: Resolve::Tool,
        preview: ("ollama", &["list"]),
        // Expanded at run time to one `ollama pull <model>` per model.
        steps: &[],
        command: "ollama pull <each model>",
    },
    // ---- Cleanup ----
    Spec {
        id: "brew-cleanup",
        title: "Homebrew cleanup",
        description: "Remove stale downloads and old versions of installed formulae/casks.",
        category: "cleanup",
        tool: "brew",
        resolve: Resolve::Tool,
        preview: ("brew", &["cleanup", "--dry-run"]),
        steps: &[("brew", &["cleanup"])],
        command: "brew cleanup",
    },
    Spec {
        id: "docker-prune",
        title: "Docker prune",
        description: "Remove dangling images, stopped containers, unused networks and build cache.",
        category: "cleanup",
        tool: "docker",
        resolve: Resolve::Tool,
        preview: ("docker", &["system", "df"]),
        steps: &[("docker", &["system", "prune", "-f"])],
        command: "docker system prune -f",
    },
    Spec {
        id: "npm-cache",
        title: "npm cache clean",
        description: "Clear the global npm download cache.",
        category: "cleanup",
        tool: "npm",
        resolve: Resolve::Tool,
        // Not `npm cache verify`: despite the name it garbage-collects the
        // cache, and a preview must change nothing. See `npm_cache_preview`.
        preview: ("npm", &["config", "get", "cache"]),
        steps: &[("npm", &["cache", "clean", "--force"])],
        command: "npm cache clean --force",
    },
    Spec {
        id: "pip-cache",
        title: "pip cache purge",
        description: "Remove all wheels and downloads from the pip cache.",
        category: "cleanup",
        tool: "pip3",
        resolve: Resolve::Tool,
        preview: ("pip3", &["cache", "info"]),
        steps: &[("pip3", &["cache", "purge"])],
        command: "pip3 cache purge",
    },
    // ---- System (check only) ----
    Spec {
        id: "system-updates",
        title: "macOS & Command Line Tools updates",
        description: "Check Apple's software updates, including the Xcode Command Line Tools \
                      compilers use. Installing needs an administrator, so it's done outside \
                      this app.",
        category: "system",
        tool: "softwareupdate",
        resolve: Resolve::Tool,
        preview: ("softwareupdate", &["--list"]),
        steps: &[],
        command: "softwareupdate --list",
    },
];

const UPDATE_ALL: &str = "update-all";

/// Homebrew/system Pythons ship `pip3`; some setups only have `pip`. Same
/// fallback `manage.rs` uses for its install/update/uninstall templates.
fn fallback_name(tool: &'static str) -> &'static str {
    if tool == "pip3" && !util::is_available("pip3") && util::is_available("pip") {
        "pip"
    } else {
        tool
    }
}

/// The binary to run for this spec, or why it can't run on this machine.
fn resolve(spec: &Spec) -> Result<String, String> {
    match spec.resolve {
        Resolve::Tool => {
            let tool = fallback_name(spec.tool);
            if util::is_available(tool) {
                Ok(tool.to_string())
            } else {
                Err("not installed".into())
            }
        }
        Resolve::UserRuby => match util::which(spec.tool) {
            None => Err("not installed".into()),
            Some(p) if p.starts_with("/usr/bin") || p.starts_with("/System") => Err(
                "Uses macOS's built-in Ruby, where updating gems needs sudo. Install Ruby with \
                 Homebrew (or a version manager) to update gems from here."
                    .into(),
            ),
            Some(_) => Ok(spec.tool.to_string()),
        },
        Resolve::SelfUpdate => {
            let copies = util::which_all(spec.tool);
            if copies.is_empty() {
                return Err("not installed".into());
            }
            copies
                .iter()
                .find(|p| !util::is_homebrew_owned(p))
                .map(|p| p.to_string_lossy().into_owned())
                .ok_or_else(|| {
                    "Installed with Homebrew, so “Update Homebrew packages” keeps it current.".into()
                })
        }
    }
}

/// Whether a spec ever runs anything from here (check-only ones don't).
fn runnable(spec: &Spec) -> bool {
    spec.category != "system"
}

/// Update-category specs that can run on this machine.
fn available_updates() -> Vec<&'static Spec> {
    SPECS
        .iter()
        .filter(|s| s.category == "update" && resolve(s).is_ok())
        .collect()
}

pub fn list() -> Vec<CleanupAction> {
    let mut actions: Vec<CleanupAction> = SPECS
        .iter()
        .map(|s| {
            let r = resolve(s);
            CleanupAction {
                id: s.id.into(),
                title: s.title.into(),
                description: s.description.into(),
                category: s.category.into(),
                command: s.command.into(),
                available: r.is_ok(),
                // "not installed" is what the badge already says.
                note: r.err().filter(|why| why != "not installed"),
                runnable: runnable(s),
            }
        })
        .collect();

    // Prepend a "run all available updaters" action to the Updates group.
    let updates = available_updates();
    actions.insert(
        0,
        CleanupAction {
            id: UPDATE_ALL.into(),
            title: "Update everything".into(),
            description: "Run every available updater below, one after another.".into(),
            category: "update".into(),
            command: "all available updaters".into(),
            available: !updates.is_empty(),
            note: None,
            runnable: true,
        },
    );
    actions
}

fn find(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id == id)
}

/// Substitute the resolved binary wherever a step names the spec's own tool.
fn bin_for<'a>(step_tool: &'static str, spec: &Spec, resolved: &'a str) -> std::borrow::Cow<'a, str> {
    if step_tool == spec.tool {
        std::borrow::Cow::Borrowed(resolved)
    } else {
        std::borrow::Cow::Owned(fallback_name(step_tool).to_string())
    }
}

pub async fn preview(id: &str) -> CleanupPreview {
    if id == UPDATE_ALL {
        let list: Vec<String> = available_updates()
            .iter()
            .map(|s| format!("  • {}  ({})", s.title, s.command))
            .collect();
        let output = if list.is_empty() {
            "No updaters are available on this machine.".into()
        } else {
            format!("This will run, in order:\n{}", list.join("\n"))
        };
        return CleanupPreview {
            id: UPDATE_ALL.into(),
            command: "all available updaters".into(),
            output,
        };
    }

    let Some(spec) = find(id) else {
        return CleanupPreview {
            id: id.into(),
            command: String::new(),
            output: format!("Unknown action: {id}"),
        };
    };
    let resolved = match resolve(spec) {
        Ok(bin) => bin,
        Err(why) => {
            return CleanupPreview { id: spec.id.into(), command: spec.command.into(), output: why }
        }
    };
    let (tool, args) = spec.preview;
    let out = match spec.id {
        "npm-cache" => npm_cache_preview().await,
        "system-updates" => system_updates_preview().await,
        _ => {
            util::run_capture(&bin_for(tool, spec, &resolved), args, Duration::from_secs(30))
                .await
                .1
        }
    };
    let out = out.trim();
    let output = if out.is_empty() {
        match spec.category {
            "update" => "Everything is already up to date.".into(),
            "system" => "No software updates available.".into(),
            _ => "Nothing to clean.".into(),
        }
    } else {
        out.to_string()
    };
    CleanupPreview {
        id: spec.id.into(),
        command: spec.command.into(),
        output,
    }
}

/// The concrete commands a spec runs: its static steps, or for Ollama one pull
/// per installed model (the list isn't known until run time).
async fn steps_for(spec: &Spec, resolved: &str) -> Vec<(String, Vec<String>)> {
    if spec.id == "ollama-models" {
        let Some(out) = util::run(resolved, &["list"]).await else {
            return Vec::new();
        };
        return ollama_models(&out)
            .into_iter()
            .map(|m| (resolved.to_string(), vec!["pull".to_string(), m]))
            .collect();
    }
    spec.steps
        .iter()
        .map(|(tool, args)| {
            (
                bin_for(tool, spec, resolved).into_owned(),
                args.iter().map(|a| a.to_string()).collect(),
            )
        })
        .collect()
}

/// Model names from `ollama list` (first column, header skipped).
fn ollama_models(out: &str) -> Vec<String> {
    out.lines()
        .skip(1)
        .filter_map(|l| l.split_whitespace().next())
        .filter(|m| !m.is_empty() && !m.starts_with('-'))
        .map(str::to_string)
        .collect()
}

/// Run a spec's steps, appending to `output`. Returns false on the first
/// failing step (later steps are skipped).
async fn run_spec_steps(spec: &Spec, timeout: Duration, output: &mut String) -> bool {
    let resolved = match resolve(spec) {
        Ok(bin) => bin,
        Err(why) => {
            output.push_str(&why);
            return false;
        }
    };
    let steps = steps_for(spec, &resolved).await;
    if steps.is_empty() {
        output.push_str("Nothing to do.\n");
        return true;
    }
    let home = util::home().to_string_lossy().into_owned();
    for (bin, args) in &steps {
        // Show the short name; the absolute path is an implementation detail
        // (and would print the user's home directory into every log).
        output.push_str(&format!("$ {} {}\n", bin.replacen(&home, "~", 1), args.join(" ")));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let (ok, out) = util::run_capture(bin, &refs, timeout).await;
        output.push_str(out.trim());
        output.push_str("\n\n");
        if !ok {
            return false;
        }
    }
    true
}

pub async fn run(id: &str) -> CleanupResult {
    if let Some(spec) = find(id).filter(|s| !runnable(s)) {
        return CleanupResult {
            id: id.into(),
            command: spec.command.into(),
            output: "This is a check only. Install updates from System Settings → General → \
                     Software Update, or with `sudo softwareupdate -i <label>` in Terminal."
                .into(),
            success: false,
        };
    }

    let Some(_operation_guard) = util::try_mutation_lock() else {
        return CleanupResult {
            id: id.into(),
            command: find(id).map(|spec| spec.command).unwrap_or("all available updaters").into(),
            output: "Another install, update, uninstall, or cleanup is already running.".into(),
            success: false,
        };
    };

    // "Update everything": run each available updater in sequence.
    if id == UPDATE_ALL {
        let timeout = Duration::from_secs(1800);
        let mut output = String::new();
        // Updaters are independent: a broken npm shouldn't cost you the
        // rustup and pipx updates queued behind it. Run them all, then say
        // which failed.
        let mut failed: Vec<&str> = Vec::new();
        for spec in available_updates() {
            output.push_str(&format!("=== {} ({}) ===\n", spec.title, spec.command));
            if !run_spec_steps(spec, timeout, &mut output).await {
                failed.push(spec.title);
                output.push_str("(failed — continuing with the next updater)\n\n");
            }
        }
        let success = failed.is_empty();
        if !success {
            output.push_str(&format!("Failed: {}\n", failed.join(", ")));
        }
        return CleanupResult {
            id: UPDATE_ALL.into(),
            command: "all available updaters".into(),
            output: if output.trim().is_empty() { "(nothing to do)".into() } else { output.trim().into() },
            success,
        };
    }

    let Some(spec) = find(id) else {
        return CleanupResult {
            id: id.into(),
            command: String::new(),
            output: format!("Unknown action: {id}"),
            success: false,
        };
    };
    // Updates can take a while (large upgrades); give them a generous budget.
    let timeout = if spec.category == "update" {
        Duration::from_secs(1800)
    } else {
        Duration::from_secs(300)
    };

    let mut output = String::new();
    let success = run_spec_steps(spec, timeout, &mut output).await;
    CleanupResult {
        id: spec.id.into(),
        command: spec.command.into(),
        output: if output.trim().is_empty() { "(done)".into() } else { output.trim().into() },
        success,
    }
}

/// Size and location of the npm cache, without touching it.
async fn npm_cache_preview() -> String {
    let Some(path) = util::run("npm", &["config", "get", "cache"]).await else {
        return "Couldn't locate the npm cache.".into();
    };
    let path = path.trim().to_string();
    // A big cache is a lot of files; walk it off the async runtime.
    let walk = path.clone();
    let size = tokio::task::spawn_blocking(move || util::dir_size(std::path::Path::new(&walk)))
        .await
        .ok()
        .flatten();
    match size {
        Some(0) | None => String::new(), // → "Nothing to clean."
        Some(bytes) => format!(
            "npm cache at {path}: {:.1} MB would be removed.",
            bytes as f64 / 1_048_576.0
        ),
    }
}

/// `softwareupdate --list`, reduced to what's pending. It contacts Apple, so
/// it can take tens of seconds — fine on demand, far too slow for a scan.
async fn system_updates_preview() -> String {
    let (_, out) =
        util::run_capture_untracked("softwareupdate", &["--list"], Duration::from_secs(120)).await;
    let updates = parse_software_updates(&out);
    if updates.is_empty() {
        return if out.contains("No new software available") {
            String::new()
        } else {
            out
        };
    }
    let mut s = String::from("Available:\n");
    for (label, title, restart) in &updates {
        s.push_str(&format!(
            "  • {title}{}\n    install: sudo softwareupdate -i \"{label}\"\n",
            if *restart { "  (needs a restart)" } else { "" }
        ));
    }
    s.push_str("\nOr install from System Settings → General → Software Update.");
    s
}

/// (label, title, needs restart) for each `* Label:` / `Title:` pair.
fn parse_software_updates(out: &str) -> Vec<(String, String, bool)> {
    let mut updates = Vec::new();
    let mut label: Option<String> = None;
    for line in out.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("* Label:") {
            label = Some(rest.trim().to_string());
        } else if let Some(rest) = l.strip_prefix("Title:") {
            let title = rest.split(',').next().unwrap_or("").trim().to_string();
            if let Some(lbl) = label.take() {
                updates.push((lbl, title, l.contains("Action: restart")));
            }
        }
    }
    updates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn software_update_list_parses() {
        let out = "Software Update Tool\n\nFinding available software\nSoftware Update found the following new or updated software:\n\
                   * Label: Command Line Tools for Xcode 27.2-27.2\n\
                   \tTitle: Command Line Tools for Xcode 27.2, Version: 27.2, Size: 521120KiB, Recommended: YES, \n\
                   * Label: macOS 27.2.1-26B131\n\
                   \tTitle: macOS 27.2.1, Version: 27.2.1, Size: 6069131KiB, Recommended: YES, Action: restart, \n";
        let u = parse_software_updates(out);
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].0, "Command Line Tools for Xcode 27.2-27.2");
        assert_eq!(u[0].1, "Command Line Tools for Xcode 27.2");
        assert!(!u[0].2 && u[1].2);
        println!("software_update_list_parses OK");
    }

    #[test]
    fn ollama_list_parses() {
        let m = ollama_models("NAME            ID      SIZE   MODIFIED\nllama3:8b  abc  4.7 GB  2 days ago\nqwen2.5-coder:7b def 4 GB 1 week ago\n");
        assert_eq!(m, ["llama3:8b", "qwen2.5-coder:7b"]);
        println!("ollama_list_parses OK");
    }

    /// Check-only actions refuse to run, including over MCP's run_cleanup.
    #[tokio::test]
    async fn system_check_is_not_runnable() {
        let r = run("system-updates").await;
        assert!(!r.success);
        assert!(r.output.contains("check only"));
        println!("system_check_is_not_runnable OK");
    }

    /// Every update spec but Ollama's has steps; check-only ones have none.
    #[test]
    fn specs_are_well_formed() {
        for s in SPECS {
            match s.category {
                "system" => assert!(s.steps.is_empty(), "{} must be check-only", s.id),
                _ if s.id == "ollama-models" => {}
                _ => assert!(!s.steps.is_empty(), "{} has no steps", s.id),
            }
        }
        println!("specs_are_well_formed OK");
    }
}
