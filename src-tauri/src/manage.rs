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

pub fn info(collector: &str, name: &str) -> ActionInfo {
    let Some(spec) = find(collector) else {
        return ActionInfo::default();
    };
    let tool = effective_tool(spec);
    ActionInfo {
        update: spec.update.map(|a| render(tool, a, name)),
        delete: spec.delete.map(|a| render(tool, a, name)),
        install: spec.install.map(|a| render(tool, a, name)),
        available: util::is_available(tool),
    }
}

pub async fn run(collector: &str, name: &str, action: &str) -> ActionResult {
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
