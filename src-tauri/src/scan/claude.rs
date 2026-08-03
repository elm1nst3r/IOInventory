use super::util;
use crate::model::{Domain, Item};
use std::path::Path;

/// Inventory the Claude Code footprint under ~/.claude plus MCP servers
/// declared in ~/.claude.json: skills, plugins, commands, agents, MCP servers.
pub async fn collect() -> Vec<Item> {
    let mut items = Vec::new();
    let claude = util::home().join(".claude");

    // Skills: one directory per skill (record its SKILL.md for quick opening).
    collect_skills(&claude.join("skills"), &mut items);
    // Slash commands: one file (or dir) per command.
    collect_entries(&claude.join("commands"), "claude-command", &mut items);
    // Sub-agents.
    collect_entries(&claude.join("agents"), "claude-agent", &mut items);

    // Installed plugins from the plugin manifest.
    let plugins_json = claude.join("plugins/installed_plugins.json");
    if let Ok(text) = std::fs::read_to_string(&plugins_json) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            collect_plugins(&v, &mut items);
        }
    }

    // MCP servers configured in ~/.claude.json (global + per-project).
    let claude_json = util::home().join(".claude.json");
    if let Ok(text) = std::fs::read_to_string(&claude_json) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            collect_mcp(&v, &mut items);
        }
    }

    items
}

fn collect_skills(dir: &Path, items: &mut Vec<Item>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // Locate the skill definition file so the UI can open it directly.
        let file = ["SKILL.md", "skill.md"]
            .iter()
            .map(|f| e.path().join(f))
            .find(|p| p.is_file());
        let mut meta = serde_json::json!({});
        if let Some(f) = &file {
            meta = serde_json::json!({ "file": f.to_string_lossy() });
        }
        items.push(
            Item::new(Domain::AiAgent, "claude-skill", name)
                .path(e.path().to_string_lossy().into_owned())
                .meta(meta),
        );
    }
}

fn collect_entries(dir: &Path, collector: &str, items: &mut Vec<Item>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let clean = name
                .trim_end_matches(".md")
                .trim_end_matches(".json")
                .to_string();
            items.push(
                Item::new(Domain::AiAgent, collector, clean)
                    .path(e.path().to_string_lossy().into_owned()),
            );
        }
    }
}

fn collect_plugins(v: &serde_json::Value, items: &mut Vec<Item>) {
    // The manifest shape has varied over versions; handle object-of-objects
    // and arrays defensively.
    match v {
        serde_json::Value::Object(map) => {
            // Could be { "plugins": {...} } or { name: {...} }.
            if let Some(inner) = map.get("plugins") {
                collect_plugins(inner, items);
                return;
            }
            for (name, info) in map {
                let version = info.get("version").and_then(|x| x.as_str());
                let mut item = Item::new(Domain::AiAgent, "claude-plugin", name.clone());
                if let Some(ver) = version {
                    item = item.version(ver);
                }
                items.push(item);
            }
        }
        serde_json::Value::Array(arr) => {
            for info in arr {
                if let Some(name) = info.get("name").and_then(|x| x.as_str()) {
                    items.push(Item::new(Domain::AiAgent, "claude-plugin", name));
                }
            }
        }
        _ => {}
    }
}

fn collect_mcp(v: &serde_json::Value, items: &mut Vec<Item>) {
    // The same server name can be configured in multiple scopes/projects, so
    // each entry gets a scope-suffixed label and a path-keyed fingerprint.
    fn push_servers(
        servers: &serde_json::Value,
        scope: &str,
        scope_label: Option<&str>,
        key_extra: &str,
        items: &mut Vec<Item>,
    ) {
        let Some(obj) = servers.as_object() else {
            return;
        };
        for (name, cfg) in obj {
            let transport = cfg
                .get("command")
                .and_then(|c| c.as_str())
                .map(|_| "stdio")
                .or_else(|| cfg.get("url").and_then(|u| u.as_str()).map(|_| "http"))
                .unwrap_or("unknown");
            // Show which project a duplicate belongs to right in the label.
            let label = match scope_label {
                Some(s) => format!("{name} · {s}"),
                None => name.clone(),
            };
            items.push(
                Item::new(Domain::AiAgent, "mcp-server", label)
                    .keyed(key_extra)
                    .meta(serde_json::json!({ "scope": scope, "transport": transport })),
            );
        }
    }

    if let Some(global) = v.get("mcpServers") {
        push_servers(global, "global", None, "global", items);
    }
    if let Some(projects) = v.get("projects").and_then(|p| p.as_object()) {
        for (path, pcfg) in projects {
            if let Some(servers) = pcfg.get("mcpServers") {
                if servers.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
                    let short = Path::new(path)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone());
                    push_servers(
                        servers,
                        &format!("project:{short}"),
                        Some(&short),
                        path, // full path → guaranteed-unique key
                        items,
                    );
                }
            }
        }
    }
}
