use super::util;
use crate::model::{Domain, Item};
use std::path::PathBuf;

/// A known AI agent / coding tool and how to detect it.
struct Tool {
    name: &'static str,
    collector: &'static str, // "ai-app" | "ai-cli"
    bins: &'static [&'static str],
    apps: &'static [&'static str], // .app bundle names (without extension)
    config: &'static [&'static str], // home-relative config paths
    homepage: &'static str,
    description: &'static str,
}

const TOOLS: &[Tool] = &[
    Tool {
        name: "Claude Code",
        collector: "ai-cli",
        bins: &["claude"],
        apps: &[],
        config: &[".claude"],
        homepage: "https://claude.com/claude-code",
        description: "Anthropic's agentic coding CLI (this app scans its skills/plugins/MCP).",
    },
    Tool {
        name: "Claude",
        collector: "ai-app",
        bins: &[],
        apps: &["Claude"],
        config: &[],
        homepage: "https://claude.ai/download",
        description: "Anthropic's Claude desktop app.",
    },
    Tool {
        name: "OpenAI Codex",
        collector: "ai-cli",
        bins: &["codex"],
        apps: &["Codex"],
        config: &[".codex"],
        homepage: "https://github.com/openai/codex",
        description: "OpenAI's agentic coding CLI.",
    },
    Tool {
        name: "Gemini CLI",
        collector: "ai-cli",
        bins: &["gemini"],
        apps: &["Gemini"],
        config: &[".gemini"],
        homepage: "https://github.com/google-gemini/gemini-cli",
        description: "Google's Gemini agentic command-line tool.",
    },
    Tool {
        name: "Antigravity",
        collector: "ai-app",
        bins: &[],
        apps: &["Antigravity"],
        config: &[".gemini/antigravity-cli", ".antigravity"],
        homepage: "https://antigravity.google",
        description: "Google's agentic IDE, powered by Gemini.",
    },
    Tool {
        name: "Cursor",
        collector: "ai-app",
        bins: &["cursor"],
        apps: &["Cursor"],
        config: &[".cursor"],
        homepage: "https://cursor.com",
        description: "AI-first code editor.",
    },
    Tool {
        name: "Windsurf",
        collector: "ai-app",
        bins: &["windsurf"],
        apps: &["Windsurf"],
        config: &[".codeium/windsurf", ".codeium"],
        homepage: "https://windsurf.com",
        description: "Codeium's agentic IDE.",
    },
    Tool {
        name: "GitHub Copilot CLI",
        collector: "ai-cli",
        bins: &["copilot"],
        apps: &[],
        config: &[".config/github-copilot", ".copilot"],
        homepage: "https://github.com/github/copilot-cli",
        description: "GitHub Copilot in the terminal.",
    },
    Tool {
        name: "Continue",
        collector: "ai-cli",
        bins: &["continue", "cn"],
        apps: &[],
        config: &[".continue"],
        homepage: "https://continue.dev",
        description: "Open-source AI code assistant.",
    },
    Tool {
        name: "aider",
        collector: "ai-cli",
        bins: &["aider"],
        apps: &[],
        config: &[".aider.conf.yml", ".aider"],
        homepage: "https://aider.chat",
        description: "AI pair programming in the terminal.",
    },
    Tool {
        name: "Zed",
        collector: "ai-app",
        bins: &["zed"],
        apps: &["Zed"],
        config: &[".config/zed"],
        homepage: "https://zed.dev",
        description: "High-performance editor with a built-in AI agent.",
    },
    Tool {
        name: "LM Studio",
        collector: "ai-app",
        bins: &["lms"],
        apps: &["LM Studio"],
        config: &[".lmstudio", ".cache/lm-studio"],
        homepage: "https://lmstudio.ai",
        description: "Desktop app for running local LLMs.",
    },
    Tool {
        name: "Jan",
        collector: "ai-app",
        bins: &[],
        apps: &["Jan"],
        config: &[".jan"],
        homepage: "https://jan.ai",
        description: "Open-source local AI assistant.",
    },
    Tool {
        name: "ChatGPT",
        collector: "ai-app",
        bins: &[],
        apps: &["ChatGPT"],
        config: &[],
        homepage: "https://openai.com/chatgpt/desktop",
        description: "OpenAI's ChatGPT desktop app.",
    },
    Tool {
        name: "Perplexity",
        collector: "ai-app",
        bins: &[],
        apps: &["Perplexity"],
        config: &[],
        homepage: "https://perplexity.ai",
        description: "AI answer-engine desktop app.",
    },
    Tool {
        name: "Warp",
        collector: "ai-app",
        bins: &[],
        apps: &["Warp"],
        config: &[],
        homepage: "https://warp.dev",
        description: "AI-powered terminal.",
    },
];

pub async fn collect() -> Vec<Item> {
    let home = util::home();

    // Pass 1 (fast, filesystem only): detect presence and build items without
    // versions. Note which tools need a (slow) `--version` subprocess.
    let mut items: Vec<Item> = Vec::new();
    let mut version_jobs: tokio::task::JoinSet<(usize, String)> = tokio::task::JoinSet::new();

    for tool in TOOLS {
        let bin = tool.bins.iter().find(|b| util::which(b).is_some()).copied();
        let app = tool.apps.iter().find_map(|a| app_path(a));
        let config = tool.config.iter().map(|c| home.join(c)).find(|p| p.exists());

        if bin.is_none() && app.is_none() && config.is_none() {
            continue;
        }

        let source_path = config
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .or_else(|| app.as_ref().map(|p| p.to_string_lossy().into_owned()));

        let mut item = Item::new(Domain::AiAgent, tool.collector, tool.name);
        if let Some(p) = &source_path {
            item = item.path(p.clone());
        }
        item = item.meta(serde_json::json!({
            "homepage": tool.homepage,
            "description": tool.description,
            "installed": {
                "cli": bin.is_some(),
                "app": app.is_some(),
                "config": config.is_some(),
            },
        }));

        let idx = items.len();
        items.push(item);

        // Pass 2 is these version probes, run concurrently.
        if let Some(b) = bin {
            version_jobs.spawn(async move {
                let v = util::run(b, &["--version"]).await.unwrap_or_default();
                (idx, first_version(&v))
            });
        }
    }

    // Drain every probe: a `while let Some(Ok(..))` would stop at the first
    // join error and silently drop the versions still in flight behind it.
    while let Some(joined) = version_jobs.join_next().await {
        if let Ok((idx, version)) = joined {
            if !version.is_empty() {
                items[idx].version = Some(version);
            }
        }
    }

    // MCP servers configured for other agents (Claude's are handled separately).
    collect_codex_mcp(&mut items);
    collect_gemini_mcp(&mut items);

    items
}

fn app_path(name: &str) -> Option<PathBuf> {
    for base in ["/Applications", "/System/Applications"] {
        let p = PathBuf::from(base).join(format!("{name}.app"));
        if p.exists() {
            return Some(p);
        }
    }
    let user = util::home().join("Applications").join(format!("{name}.app"));
    if user.exists() {
        return Some(user);
    }
    None
}

/// Codex MCP servers live under `[mcp_servers.<name>]` tables in config.toml.
fn collect_codex_mcp(items: &mut Vec<Item>) {
    let path = util::home().join(".codex/config.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    for line in text.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("[mcp_servers.") {
            let name = rest.trim_end_matches(']').trim_matches('"');
            if !name.is_empty() {
                items.push(
                    Item::new(Domain::AiAgent, "mcp-server", format!("{name} · codex"))
                        .keyed("codex")
                        .meta(serde_json::json!({ "scope": "codex" })),
                );
            }
        }
    }
}

/// Gemini MCP servers are declared as JSON under `mcpServers` in settings.json.
fn collect_gemini_mcp(items: &mut Vec<Item>) {
    let path = util::home().join(".gemini/settings.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    if let Some(obj) = v.get("mcpServers").and_then(|m| m.as_object()) {
        for name in obj.keys() {
            items.push(
                Item::new(Domain::AiAgent, "mcp-server", format!("{name} · gemini"))
                    .keyed("gemini")
                    .meta(serde_json::json!({ "scope": "gemini" })),
            );
        }
    }
}

fn first_version(s: &str) -> String {
    let first = s.lines().next().unwrap_or("");
    for tok in first.split([' ', '"', ',', '(']) {
        let t = tok.trim().trim_start_matches('v');
        if !t.is_empty() && t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return t.to_string();
        }
    }
    String::new()
}
