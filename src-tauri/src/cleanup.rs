use crate::model::{CleanupAction, CleanupPreview, CleanupResult};
use crate::scan::util;
use std::time::Duration;

type Step = (&'static str, &'static [&'static str]);

/// A safe, allowlisted utility action: a preview (dry-run / status probe) and
/// one or more run steps. Nothing outside this list can ever be executed.
struct Spec {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    /// "update" or "cleanup".
    category: &'static str,
    /// Tool used to check availability.
    tool: &'static str,
    /// Preview command (tool, args).
    preview: Step,
    /// Sequential run steps.
    steps: &'static [Step],
    /// Display string for the command(s) that will run.
    command: &'static str,
}

const SPECS: &[Spec] = &[
    // ---- Updates ----
    Spec {
        id: "brew-upgrade",
        title: "Update Homebrew packages",
        description: "Refresh formula/cask definitions, then upgrade everything that's outdated.",
        category: "update",
        tool: "brew",
        preview: ("brew", &["outdated", "--verbose", "--greedy"]),
        steps: &[("brew", &["update"]), ("brew", &["upgrade"])],
        command: "brew update && brew upgrade",
    },
    Spec {
        id: "npm-upgrade",
        title: "Update global npm packages",
        description: "Update all globally-installed npm packages to their latest versions.",
        category: "update",
        tool: "npm",
        preview: ("npm", &["outdated", "-g"]),
        steps: &[("npm", &["update", "-g"])],
        command: "npm update -g",
    },
    Spec {
        id: "rustup-update",
        title: "Update Rust toolchains",
        description: "Update rustup and all installed Rust toolchains to the latest release.",
        category: "update",
        tool: "rustup",
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
        preview: ("pipx", &["list", "--short"]),
        steps: &[("pipx", &["upgrade-all"])],
        command: "pipx upgrade-all",
    },
    Spec {
        id: "cargo-update",
        title: "Update cargo binaries",
        description: "Update crates installed via `cargo install` (needs the cargo-update crate).",
        category: "update",
        tool: "cargo-install-update",
        preview: ("cargo", &["install-update", "-l"]),
        steps: &[("cargo", &["install-update", "-a"])],
        command: "cargo install-update -a",
    },
    Spec {
        id: "gh-upgrade",
        title: "Update GitHub CLI extensions",
        description: "Upgrade all installed `gh` extensions to their latest versions.",
        category: "update",
        tool: "gh",
        preview: ("gh", &["extension", "list"]),
        steps: &[("gh", &["extension", "upgrade", "--all"])],
        command: "gh extension upgrade --all",
    },
    // ---- Cleanup ----
    Spec {
        id: "brew-cleanup",
        title: "Homebrew cleanup",
        description: "Remove stale downloads and old versions of installed formulae/casks.",
        category: "cleanup",
        tool: "brew",
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
        preview: ("npm", &["cache", "verify"]),
        steps: &[("npm", &["cache", "clean", "--force"])],
        command: "npm cache clean --force",
    },
    Spec {
        id: "pip-cache",
        title: "pip cache purge",
        description: "Remove all wheels and downloads from the pip cache.",
        category: "cleanup",
        tool: "pip3",
        preview: ("pip3", &["cache", "info"]),
        steps: &[("pip3", &["cache", "purge"])],
        command: "pip3 cache purge",
    },
];

const UPDATE_ALL: &str = "update-all";

/// Update-category specs available on this machine.
fn available_updates() -> Vec<&'static Spec> {
    SPECS
        .iter()
        .filter(|s| s.category == "update" && util::is_available(s.tool))
        .collect()
}

pub fn list() -> Vec<CleanupAction> {
    let mut actions: Vec<CleanupAction> = SPECS
        .iter()
        .map(|s| CleanupAction {
            id: s.id.into(),
            title: s.title.into(),
            description: s.description.into(),
            category: s.category.into(),
            command: s.command.into(),
            available: util::is_available(s.tool),
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
        },
    );
    actions
}

fn find(id: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.id == id)
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
    let (tool, args) = spec.preview;
    let (_, out) = util::run_capture(tool, args, Duration::from_secs(30)).await;
    let out = out.trim();
    let output = if out.is_empty() {
        if spec.category == "update" {
            "Everything is already up to date.".into()
        } else {
            "Nothing to clean.".into()
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

/// Run a spec's steps, appending to `output`. Returns false on the first
/// failing step (later steps are skipped).
async fn run_spec_steps(spec: &Spec, timeout: Duration, output: &mut String) -> bool {
    for (tool, args) in spec.steps {
        output.push_str(&format!("$ {tool} {}\n", args.join(" ")));
        let (ok, out) = util::run_capture(tool, args, timeout).await;
        output.push_str(out.trim());
        output.push_str("\n\n");
        if !ok {
            return false;
        }
    }
    true
}

pub async fn run(id: &str) -> CleanupResult {
    // "Update everything": run each available updater in sequence.
    if id == UPDATE_ALL {
        let timeout = Duration::from_secs(1800);
        let mut output = String::new();
        let mut success = true;
        for spec in available_updates() {
            output.push_str(&format!("=== {} ({}) ===\n", spec.title, spec.command));
            if !run_spec_steps(spec, timeout, &mut output).await {
                success = false;
                output.push_str("(stopped — step failed)\n\n");
                break;
            }
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
