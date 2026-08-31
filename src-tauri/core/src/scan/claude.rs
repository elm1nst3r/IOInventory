use super::util;
use crate::model::{Domain, Item};
use std::path::Path;

/// Which agent everything in this module belongs to. Recorded on every item so
/// the AI view can attribute a capability to the agents that use it — and so
/// `agents::link` can spot the ones several agents share.
const AGENT: &str = "claude";

/// Inventory the Claude Code footprint under ~/.claude plus MCP servers
/// declared in ~/.claude.json: skills, plugins, marketplaces, commands,
/// agents, MCP servers.
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
    let plugins_dir = claude.join("plugins");
    let plugins_json = plugins_dir.join("installed_plugins.json");
    if let Ok(text) = std::fs::read_to_string(&plugins_json) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            collect_plugins(&v, &mut items);
        }
    }

    // The marketplaces those plugins are installed from.
    collect_marketplaces(&plugins_dir, &mut items);

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
        let mut meta = serde_json::json!({ "agent": AGENT });
        if let Some(f) = &file {
            meta = serde_json::json!({ "agent": AGENT, "file": f.to_string_lossy() });
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
            // Strip one definition extension, not every repetition of it —
            // `trim_end_matches` would turn "notes.md.md" into "notes".
            let clean = ["md", "json", "markdown"]
                .iter()
                .find_map(|ext| name.strip_suffix(&format!(".{ext}")))
                .unwrap_or(&name);
            let path = e.path().to_string_lossy().into_owned();
            items.push(
                // `foo.md` and `foo.json` share a stem; key by path so they
                // don't collide on item_key and get merged into one node.
                Item::new(Domain::AiAgent, collector, clean)
                    .keyed(&path)
                    .path(path)
                    .meta(serde_json::json!({ "agent": AGENT })),
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
                // Skip manifest scalars like `"version": 2` sitting alongside
                // the plugin entries — only objects/arrays describe a plugin.
                if !info.is_object() && !info.is_array() {
                    continue;
                }
                let mut item = Item::new(Domain::AiAgent, "claude-plugin", name.clone());
                if let Some(ver) = plugin_version(info) {
                    item = item.version(ver);
                }
                items.push(item.meta(plugin_meta(name)));
            }
        }
        serde_json::Value::Array(arr) => {
            for info in arr {
                if let Some(name) = info.get("name").and_then(|x| x.as_str()) {
                    let mut item = Item::new(Domain::AiAgent, "claude-plugin", name);
                    if let Some(ver) = plugin_version(info) {
                        item = item.version(ver);
                    }
                    items.push(item.meta(plugin_meta(name)));
                }
            }
        }
        _ => {}
    }
}

/// Manifest keys carry the marketplace a plugin came from as `name@marketplace`
/// (v2). The item keeps the whole key as its name — that's its identity, and
/// notes hang off it — while the split-out parts ride along in metadata so the
/// AI view can show a short name and group plugins under their marketplace.
fn plugin_meta(key: &str) -> serde_json::Value {
    match key.split_once('@') {
        Some((plugin, marketplace)) if !plugin.is_empty() && !marketplace.is_empty() => {
            serde_json::json!({ "agent": AGENT, "plugin": plugin, "marketplace": marketplace })
        }
        _ => serde_json::json!({ "agent": AGENT, "plugin": key }),
    }
}

/// Where a marketplace lives, gathered from whichever source named it.
#[derive(Default)]
struct Marketplace {
    source: Option<String>,
    path: Option<String>,
}

/// Plugin/skill marketplaces Claude Code knows about. The layout has moved
/// between versions, so read every shape we've seen instead of betting on one:
/// a JSON manifest of known marketplaces, plus the directories cloned under
/// `marketplaces/` or `repos/`. The directory sweep is the shape-independent
/// half and carries the scan when a manifest is absent or renamed again.
fn collect_marketplaces(plugins_dir: &Path, items: &mut Vec<Item>) {
    let mut found: std::collections::BTreeMap<String, Marketplace> = Default::default();

    // Files whose name declares their contents can be read whole. `config.json`
    // holds unrelated settings too, so only its marketplace-named sections are.
    for (file, whole_file) in [
        ("known_marketplaces.json", true),
        ("marketplaces.json", true),
        ("config.json", false),
    ] {
        let Ok(text) = std::fs::read_to_string(plugins_dir.join(file)) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if whole_file {
            harvest_marketplaces(&v, &mut found);
        }
        if let Some(map) = v.as_object() {
            for key in ["marketplaces", "knownMarketplaces", "extraKnownMarketplaces"] {
                if let Some(inner) = map.get(key) {
                    harvest_marketplaces(inner, &mut found);
                }
            }
        }
    }

    for dir in ["marketplaces", "repos"] {
        let Ok(entries) = std::fs::read_dir(plugins_dir.join(dir)) else {
            continue;
        };
        for e in entries.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let entry = found.entry(name).or_default();
            if entry.path.is_none() {
                entry.path = Some(e.path().to_string_lossy().into_owned());
            }
        }
    }

    for (name, info) in found {
        let mut item = Item::new(Domain::AiAgent, "claude-marketplace", name)
            .meta(serde_json::json!({ "agent": AGENT, "source": info.source }));
        if let Some(p) = info.path {
            item = item.path(p);
        }
        items.push(item);
    }
}

/// Marketplace entries, as either `{ name: {...} }` or `[ { "name": … } ]`.
fn harvest_marketplaces(
    v: &serde_json::Value,
    out: &mut std::collections::BTreeMap<String, Marketplace>,
) {
    let mut record = |name: &str, info: &serde_json::Value| {
        if name.is_empty() {
            return;
        }
        let entry = out.entry(name.to_string()).or_default();
        // First source found wins: a later file listing the same marketplace
        // without one must not blank out the URL an earlier one gave us.
        if entry.source.is_none() {
            entry.source = marketplace_source(info);
        }
    };
    match v {
        serde_json::Value::Object(map) => {
            for (name, info) in map {
                if info.is_object() {
                    record(name, info);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for info in arr {
                if let Some(name) = info.get("name").and_then(|x| x.as_str()) {
                    record(name, info);
                }
            }
        }
        _ => {}
    }
}

/// A marketplace's origin — a URL or `owner/repo`, stored flat or nested under
/// a `source` object depending on the version that wrote it.
fn marketplace_source(info: &serde_json::Value) -> Option<String> {
    for key in ["source", "url", "repo", "repository", "path"] {
        let Some(v) = info.get(key) else {
            continue;
        };
        if let Some(s) = v.as_str().filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
        for nested in ["repo", "url", "path"] {
            if let Some(s) = v.get(nested).and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// A plugin's version, whether the manifest stores the install record directly
/// (`{ "version": "1.0.0" }`) or as a list of them, one per scope (v2's
/// `{ "name@repo": [ { "scope": "user", "version": "1.0.0" } ] }`).
fn plugin_version(info: &serde_json::Value) -> Option<String> {
    let record = match info {
        serde_json::Value::Array(installs) => installs.first()?,
        other => other,
    };
    record
        .get("version")
        .and_then(|x| x.as_str())
        .map(str::to_string)
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
            let command = cfg
                .get("command")
                .and_then(|c| c.as_str())
                .or_else(|| cfg.get("url").and_then(|u| u.as_str()));
            items.push(
                Item::new(Domain::AiAgent, "mcp-server", label)
                    .keyed(key_extra)
                    .meta(serde_json::json!({
                        "agent": AGENT,
                        "server": name,
                        "scope": scope,
                        "transport": transport,
                        "command": command,
                    })),
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
