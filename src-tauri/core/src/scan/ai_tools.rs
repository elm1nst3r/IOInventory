use super::util;
use crate::model::{Domain, Item};
use std::path::PathBuf;

/// A known AI agent / coding tool and how to detect it.
struct Tool {
    /// Stable agent slug. Capabilities record the same slug in their `agent`
    /// metadata, which is what joins an MCP server or skill to the agent
    /// that uses it.
    id: &'static str,
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
        id: "claude",
        name: "Claude Code",
        collector: "ai-cli",
        bins: &["claude"],
        apps: &[],
        config: &[".claude"],
        homepage: "https://claude.com/claude-code",
        description: "Anthropic's agentic coding CLI (this app scans its skills/plugins/MCP).",
    },
    Tool {
        id: "claude-desktop",
        name: "Claude",
        collector: "ai-app",
        bins: &[],
        apps: &["Claude"],
        config: &[],
        homepage: "https://claude.ai/download",
        description: "Anthropic's Claude desktop app.",
    },
    Tool {
        id: "codex",
        name: "OpenAI Codex",
        collector: "ai-cli",
        bins: &["codex"],
        apps: &["Codex"],
        config: &[".codex"],
        homepage: "https://github.com/openai/codex",
        description: "OpenAI's agentic coding CLI.",
    },
    Tool {
        id: "gemini",
        name: "Gemini CLI",
        collector: "ai-cli",
        bins: &["gemini"],
        apps: &["Gemini"],
        config: &[".gemini"],
        homepage: "https://github.com/google-gemini/gemini-cli",
        description: "Google's Gemini agentic command-line tool.",
    },
    Tool {
        id: "antigravity",
        name: "Antigravity",
        collector: "ai-app",
        bins: &[],
        apps: &["Antigravity"],
        config: &[".gemini/antigravity-cli", ".antigravity"],
        homepage: "https://antigravity.google",
        description: "Google's agentic IDE, powered by Gemini.",
    },
    Tool {
        id: "cursor",
        name: "Cursor",
        collector: "ai-app",
        bins: &["cursor"],
        apps: &["Cursor"],
        config: &[".cursor"],
        homepage: "https://cursor.com",
        description: "AI-first code editor.",
    },
    Tool {
        id: "windsurf",
        name: "Windsurf",
        collector: "ai-app",
        bins: &["windsurf"],
        apps: &["Windsurf"],
        config: &[".codeium/windsurf", ".codeium"],
        homepage: "https://windsurf.com",
        description: "Codeium's agentic IDE.",
    },
    Tool {
        id: "copilot",
        name: "GitHub Copilot CLI",
        collector: "ai-cli",
        bins: &["copilot"],
        apps: &[],
        config: &[".config/github-copilot", ".copilot"],
        homepage: "https://github.com/github/copilot-cli",
        description: "GitHub Copilot in the terminal.",
    },
    Tool {
        id: "continue",
        name: "Continue",
        collector: "ai-cli",
        bins: &["continue", "cn"],
        apps: &[],
        config: &[".continue"],
        homepage: "https://continue.dev",
        description: "Open-source AI code assistant.",
    },
    Tool {
        id: "aider",
        name: "aider",
        collector: "ai-cli",
        bins: &["aider"],
        apps: &[],
        config: &[".aider.conf.yml", ".aider"],
        homepage: "https://aider.chat",
        description: "AI pair programming in the terminal.",
    },
    Tool {
        id: "zed",
        name: "Zed",
        collector: "ai-app",
        bins: &["zed"],
        apps: &["Zed"],
        config: &[".config/zed"],
        homepage: "https://zed.dev",
        description: "High-performance editor with a built-in AI agent.",
    },
    Tool {
        id: "lmstudio",
        name: "LM Studio",
        collector: "ai-app",
        bins: &["lms"],
        apps: &["LM Studio"],
        config: &[".lmstudio", ".cache/lm-studio"],
        homepage: "https://lmstudio.ai",
        description: "Desktop app for running local LLMs.",
    },
    Tool {
        id: "jan",
        name: "Jan",
        collector: "ai-app",
        bins: &[],
        apps: &["Jan"],
        config: &[".jan"],
        homepage: "https://jan.ai",
        description: "Open-source local AI assistant.",
    },
    Tool {
        id: "chatgpt",
        name: "ChatGPT",
        collector: "ai-app",
        bins: &[],
        apps: &["ChatGPT"],
        config: &[],
        homepage: "https://openai.com/chatgpt/desktop",
        description: "OpenAI's ChatGPT desktop app.",
    },
    Tool {
        id: "perplexity",
        name: "Perplexity",
        collector: "ai-app",
        bins: &[],
        apps: &["Perplexity"],
        config: &[],
        homepage: "https://perplexity.ai",
        description: "AI answer-engine desktop app.",
    },
    Tool {
        id: "warp",
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
            "agent": tool.id,
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

/// An MCP server another agent has configured. Named plainly, with the agent in
/// metadata rather than baked into the label: that's what lets `agents::link`
/// recognise the same server across agents and merge it into one shared row.
fn mcp_item(name: &str, agent: &str, command: Option<String>) -> Item {
    let transport = match command.as_deref() {
        Some(c) if c.starts_with("http") => "http",
        Some(_) => "stdio",
        None => "unknown",
    };
    Item::new(Domain::AiAgent, "mcp-server", name)
        .keyed(agent)
        .meta(serde_json::json!({
            "agent": agent,
            "server": name,
            "scope": "user",
            "transport": transport,
            "command": command,
        }))
}

/// Codex MCP servers live under `[mcp_servers.<name>]` tables in config.toml.
fn collect_codex_mcp(items: &mut Vec<Item>) {
    let path = util::home().join(".codex/config.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    items.extend(parse_codex_mcp(&text));
}

fn parse_codex_mcp(text: &str) -> Vec<Item> {
    let mut items = Vec::new();
    // Walked by hand rather than with a TOML parser (not a dependency here): a
    // `[mcp_servers.<name>]` header opens a block and any other `[section]`
    // closes it. The `command` inside is what tells us whether this is the same
    // server another agent has configured.
    let mut current: Option<String> = None;
    let mut command: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            // `[mcp_servers.<name>]` opens a block and any other `[section]`
            // closes it — but `[mcp_servers.<name>.env]` is still that same
            // block, and treating it as a new one would invent a server
            // called `<name>.env` and cut the real one short.
            let opened = l.strip_prefix("[mcp_servers.").map(|rest| {
                let table = rest.trim_end_matches(']');
                match table.strip_prefix('"') {
                    Some(quoted) => quoted.split('"').next().unwrap_or(quoted).to_string(),
                    None => table.split('.').next().unwrap_or(table).to_string(),
                }
            });
            if opened.as_deref() != current.as_deref() {
                if let Some(name) = current.take() {
                    items.push(mcp_item(&name, "codex", command.take()));
                }
                command = None;
                current = opened.filter(|n| !n.is_empty());
            }
            continue;
        }
        if current.is_some() && command.is_none() {
            if let Some(value) = l.strip_prefix("command").and_then(|r| r.trim().strip_prefix('=')) {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    command = Some(value.to_string());
                }
            }
        }
    }
    if let Some(name) = current {
        items.push(mcp_item(&name, "codex", command));
    }
    items
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
        for (name, cfg) in obj {
            let command = cfg
                .get("command")
                .and_then(|c| c.as_str())
                .or_else(|| cfg.get("url").and_then(|u| u.as_str()))
                .map(str::to_string);
            items.push(mcp_item(name, "gemini", command));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.name.as_str()).collect()
    }

    fn meta<'a>(items: &'a [Item], name: &str, key: &str) -> Option<&'a str> {
        items
            .iter()
            .find(|i| i.name == name)?
            .metadata
            .get(key)?
            .as_str()
    }

    /// The Codex config is walked without a TOML parser, so the block handling
    /// has to be exercised: sub-tables, quoted names, unrelated sections in
    /// between, and the last block in the file.
    #[test]
    fn codex_config_blocks_parse() {
        let items = parse_codex_mcp(
            r#"
model = "gpt-5"
command = "not-a-server"

[mcp_servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp_servers.github.env]
GITHUB_TOKEN = "x"

[mcp_servers."my.linear"]
command = "linear-mcp"

[tui]
command = "ignored"

[mcp_servers.no_command]
args = ["x"]
"#,
        );

        assert_eq!(names(&items), ["github", "my.linear", "no_command"]);
        // A sub-table must not cut the parent short or invent `github.env`.
        assert_eq!(meta(&items, "github", "command"), Some("npx"));
        assert_eq!(meta(&items, "my.linear", "command"), Some("linear-mcp"));
        // `command` outside any server block, or in an unrelated section, is
        // not a server's command.
        assert_eq!(meta(&items, "no_command", "command"), None);
        assert_eq!(meta(&items, "no_command", "transport"), Some("unknown"));
        assert_eq!(meta(&items, "github", "agent"), Some("codex"));
        assert_eq!(meta(&items, "github", "scope"), Some("user"));
        println!("codex_config_blocks_parse OK — {:?}", names(&items));
    }

    /// A key merely starting with "command" is a different key.
    #[test]
    fn codex_command_prefix_is_not_command() {
        let items = parse_codex_mcp("[mcp_servers.a]\ncommand_timeout = 30\ncommandline = \"x\"\n");
        assert_eq!(names(&items), ["a"]);
        assert_eq!(meta(&items, "a", "command"), None);
        println!("codex_command_prefix_is_not_command OK");
    }

    /// An http server is transport-tagged from its URL, not its command.
    #[test]
    fn url_servers_are_http() {
        let items = parse_codex_mcp("[mcp_servers.remote]\ncommand = \"https://example.com/mcp\"\n");
        assert_eq!(meta(&items, "remote", "transport"), Some("http"));
        println!("url_servers_are_http OK");
    }
}
