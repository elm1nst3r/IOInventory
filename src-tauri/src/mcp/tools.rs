//! Tool definitions and dispatch for the MCP server.
//!
//! Every tool returns plain text shaped for a model to read: compact lines,
//! `item_key` included wherever a follow-up call would need it, and result
//! counts stated up front so the model knows when it's seeing a truncated view.
//!
//! Tools listed in [`WRITE_TOOLS`] change what is installed on the machine.
//! They're hidden from `tools/list` and refused by `tools/call` unless the user
//! enabled writes (Settings -> MCP server, or the `--allow-write` flag) —
//! mirroring the desktop app's rule that mutating actions are allowlisted and
//! explicitly confirmed.

use super::Server;
use crate::model::{Inventory, Item};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

/// Tools that mutate the machine. Gated behind the app's write toggle.
const WRITE_TOOLS: &[&str] = &["run_item_action", "run_cleanup"];

pub fn is_write_tool(name: &str) -> bool {
    WRITE_TOOLS.contains(&name)
}

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;
/// Cap on `export_agent_map` output so a large machine can't flood the context.
const MAX_MAP_CHARS: usize = 60_000;

// ---------------------------------------------------------------- definitions

fn tool(name: &str, description: &str, properties: Value, required: &[&str], ann: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
        },
        "annotations": ann,
    })
}

/// `readOnlyHint` = doesn't change anything. `destructiveHint` = can remove
/// things. `openWorldHint` = reaches the network.
fn ann(read_only: bool, destructive: bool, open_world: bool) -> Value {
    json!({
        "readOnlyHint": read_only,
        "destructiveHint": destructive,
        "openWorldHint": open_world,
    })
}

pub fn list(allow_write: bool) -> Vec<Value> {
    let mut tools = vec![
        tool(
            "inventory_summary",
            "Overview of everything installed on this machine: item counts per domain and \
             collector, disk usage, how many packages are outdated, and how old the scan is. \
             Start here — it tells you what exists before you go looking for specifics.",
            json!({}),
            &[],
            ann(true, false, false),
        ),
        tool(
            "search_items",
            "Search and filter the inventory. Every filter is optional and they combine \
             (AND). Returns one compact line per item, starting with its item_key — pass \
             that key to get_item, set_note, or set_tags.",
            json!({
                "query": { "type": "string", "description": "Case-insensitive substring matched against the item name and its path." },
                "domain": { "type": "string", "enum": ["package_manager", "runtime", "project", "ai_agent", "container"], "description": "Restrict to one domain." },
                "collector": { "type": "string", "description": "Restrict to one collector, e.g. homebrew, npm, pip, cargo, gem, ollama, docker-image, git-repo, claude-skill, mcp-server. Use list_collectors to see what this machine has." },
                "tag": { "type": "string", "description": "Only items carrying this user tag." },
                "outdated": { "type": "boolean", "description": "true = only packages with a newer version available." },
                "has_note": { "type": "boolean", "description": "true = only items the user has annotated." },
                "min_size_bytes": { "type": "integer", "description": "Only items at least this large on disk. Useful for finding what's worth removing." },
                "sort": { "type": "string", "enum": ["name", "size"], "description": "Default 'name'. 'size' sorts largest first." },
                "limit": { "type": "integer", "description": "Max results, default 50, cap 500." },
                "offset": { "type": "integer", "description": "Skip this many results, for paging through a large match set." }
            }),
            &[],
            ann(true, false, false),
        ),
        tool(
            "get_item",
            "Full detail for one item: version, path, size, metadata, user note and tags, \
             and the exact install/update/uninstall commands that apply to it. By default \
             also fetches live extras (description, homepage, latest version, install date), \
             which needs network access and takes a moment — pass enrich:false to skip.",
            json!({
                "item_key": { "type": "string", "description": "Preferred. The stable key from search_items." },
                "collector": { "type": "string", "description": "Alternative to item_key, combined with name." },
                "name": { "type": "string", "description": "Alternative to item_key, combined with collector." },
                "enrich": { "type": "boolean", "description": "Fetch live description/homepage/latest version. Default true." }
            }),
            &[],
            ann(true, false, true),
        ),
        tool(
            "list_collectors",
            "Which collectors found anything on this machine, with item counts and total \
             disk usage each. Use it to discover valid `collector` values for search_items.",
            json!({}),
            &[],
            ann(true, false, false),
        ),
        tool(
            "list_tags",
            "All user-assigned tags with how many items carry each one.",
            json!({}),
            &[],
            ann(true, false, false),
        ),
        tool(
            "export_agent_map",
            "Render the whole environment as an AGENT_MAP.md Markdown ledger — projects with \
             their stacks and launch commands, then packages grouped by collector, plus any \
             tagged views. This is the single best artifact to read when onboarding to this \
             machine or reproducing it elsewhere.",
            json!({
                "domain": { "type": "string", "enum": ["package_manager", "runtime", "project", "ai_agent", "container"], "description": "Limit the map to one domain to keep it short." },
                "write_file": { "type": "boolean", "description": "Also write AGENT_MAP.md to the user's Documents folder. Default false." }
            }),
            &[],
            ann(true, false, false),
        ),
        tool(
            "scan",
            "Re-scan the machine and save the result as the current inventory (~2s). Run this \
             when inventory_summary reports stale data, or after something has been installed \
             or removed. Returns the fresh summary.",
            json!({
                "roots": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Directories to search for git repositories. Defaults to the configured workspace roots — see get_roots."
                }
            }),
            &[],
            ann(false, false, true),
        ),
        tool(
            "get_roots",
            "The directories scanned for git repositories on this machine.",
            json!({}),
            &[],
            ann(true, false, false),
        ),
        tool(
            "item_actions",
            "Show the exact install/update/uninstall commands that apply to an item, and \
             whether the underlying tool is present — without running anything. Use this to \
             tell the user what would happen before proposing a change.",
            json!({
                "collector": { "type": "string", "description": "e.g. homebrew, npm, pip, cargo, ollama." },
                "name": { "type": "string", "description": "The item name." }
            }),
            &["collector", "name"],
            ann(true, false, false),
        ),
        tool(
            "set_note",
            "Record why an item is on this machine. Persists across re-scans and appears in \
             the desktop app and in export_agent_map. Omitted fields keep their current value.",
            json!({
                "item_key": { "type": "string", "description": "From search_items." },
                "why": { "type": "string", "description": "Short reason the item is here — this is what shows up in AGENT_MAP.md." },
                "note": { "type": "string", "description": "Longer free-form note." }
            }),
            &["item_key"],
            ann(false, false, false),
        ),
        tool(
            "set_tags",
            "Replace an item's tags (pass an empty array to clear them). Tags drive the saved \
             views and filters in the desktop app and persist across re-scans.",
            json!({
                "item_key": { "type": "string", "description": "From search_items." },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "The complete new tag set — this replaces existing tags rather than adding to them." }
            }),
            &["item_key", "tags"],
            ann(false, false, false),
        ),
        tool(
            "list_snapshots",
            "Saved snapshots of this environment, newest first, with their ids for diff_snapshot.",
            json!({}),
            &[],
            ann(true, false, false),
        ),
        tool(
            "save_snapshot",
            "Save the current inventory as a named snapshot so it can be diffed later.",
            json!({
                "name": { "type": "string", "description": "Defaults to a timestamped name." }
            }),
            &[],
            ann(false, false, false),
        ),
        tool(
            "diff_snapshot",
            "Compare a saved snapshot against the current inventory: what was added, removed, \
             or changed version since. Use it to answer 'what changed on this machine?'.",
            json!({
                "id": { "type": "integer", "description": "Snapshot id from list_snapshots." }
            }),
            &["id"],
            ann(true, false, false),
        ),
        tool(
            "list_cleanups",
            "Allowlisted maintenance actions (bulk updaters and cache/disk cleanups) and \
             whether each one's tool is available here.",
            json!({}),
            &[],
            ann(true, false, false),
        ),
        tool(
            "preview_cleanup",
            "Dry-run a maintenance action: shows what it would update or reclaim without \
             changing anything.",
            json!({
                "id": { "type": "string", "description": "Action id from list_cleanups." }
            }),
            &["id"],
            ann(true, false, true),
        ),
    ];

    if allow_write {
        tools.push(tool(
            "run_item_action",
            "Install, update, or uninstall a single item using its package manager. This \
             changes the machine. Show the user the exact command from item_actions and get \
             their agreement before calling this.",
            json!({
                "collector": { "type": "string", "description": "e.g. homebrew, npm, pip, cargo, ollama." },
                "name": { "type": "string", "description": "The item name." },
                "action": { "type": "string", "enum": ["install", "update", "delete"], "description": "'delete' uninstalls it." }
            }),
            &["collector", "name", "action"],
            ann(false, true, true),
        ));
        tools.push(tool(
            "run_cleanup",
            "Run a maintenance action for real. This changes the machine and can take a long \
             time for bulk updates. Run preview_cleanup first and confirm with the user.",
            json!({
                "id": { "type": "string", "description": "Action id from list_cleanups." }
            }),
            &["id"],
            ann(false, true, true),
        ));
    }

    tools
}

pub fn resources() -> Vec<Value> {
    vec![
        json!({
            "uri": "ioinv://summary",
            "name": "Environment summary",
            "description": "Counts, disk usage and freshness for the current inventory.",
            "mimeType": "text/markdown",
        }),
        json!({
            "uri": "ioinv://agent-map.md",
            "name": "AGENT_MAP.md",
            "description": "The full environment rendered as a Markdown ledger.",
            "mimeType": "text/markdown",
        }),
        json!({
            "uri": "ioinv://inventory.json",
            "name": "Inventory (JSON)",
            "description": "The complete current inventory as structured JSON. Large.",
            "mimeType": "application/json",
        }),
    ]
}

pub fn read_resource(s: &mut Server, uri: &str) -> Result<(String, String), String> {
    let inv = inventory(s)?;
    match uri {
        "ioinv://summary" => Ok(("text/markdown".into(), summary_text(&inv))),
        "ioinv://agent-map.md" => Ok(("text/markdown".into(), crate::export::to_agent_map(&inv))),
        "ioinv://inventory.json" => Ok((
            "application/json".into(),
            serde_json::to_string_pretty(&inv).map_err(|e| e.to_string())?,
        )),
        other => Err(format!("unknown resource: {other}")),
    }
}

// ------------------------------------------------------------------- dispatch

pub async fn dispatch(s: &mut Server, name: &str, a: &Value) -> Result<String, String> {
    match name {
        "inventory_summary" => Ok(summary_text(&inventory(s)?)),
        "search_items" => search_items(s, a),
        "get_item" => get_item(s, a).await,
        "list_collectors" => list_collectors(s),
        "list_tags" => list_tags(s),
        "export_agent_map" => export_agent_map(s, a),
        "scan" => scan(s, a).await,
        "get_roots" => get_roots(s),
        "item_actions" => item_actions(a),
        "set_note" => set_note(s, a),
        "set_tags" => set_tags(s, a),
        "list_snapshots" => list_snapshots(s),
        "save_snapshot" => save_snapshot(s, a),
        "diff_snapshot" => diff_snapshot(s, a),
        "list_cleanups" => Ok(list_cleanups()),
        "preview_cleanup" => preview_cleanup(a).await,
        "run_item_action" => run_item_action(a).await,
        "run_cleanup" => run_cleanup(a).await,
        other => Err(format!("unknown tool: {other}")),
    }
}

// ------------------------------------------------------------- argument helpers

fn str_arg(a: &Value, key: &str) -> Option<String> {
    a.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn req_str(a: &Value, key: &str) -> Result<String, String> {
    str_arg(a, key).ok_or_else(|| format!("missing required argument `{key}`"))
}

fn bool_arg(a: &Value, key: &str) -> Option<bool> {
    a.get(key).and_then(|v| v.as_bool())
}

fn i64_arg(a: &Value, key: &str) -> Option<i64> {
    a.get(key).and_then(|v| v.as_i64())
}

fn usize_arg(a: &Value, key: &str) -> Option<usize> {
    i64_arg(a, key).and_then(|n| usize::try_from(n).ok())
}

fn inventory(s: &Server) -> Result<Inventory, String> {
    s.db.latest_inventory()
        .map_err(|e| format!("could not read the ledger: {e}"))?
        .ok_or_else(|| "No inventory yet — call `scan` first to index this machine.".into())
}

// -------------------------------------------------------------- formatting

fn fmt_size(bytes: i64) -> String {
    const KB: f64 = 1000.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.1} GB", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn meta_bool(it: &Item, key: &str) -> bool {
    it.metadata.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn meta_str(it: &Item, key: &str) -> Option<String> {
    it.metadata
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// One item as a single line, `item_key` first so a follow-up call is obvious.
fn item_line(it: &Item) -> String {
    let mut parts = vec![it.name.clone()];
    if let Some(v) = &it.version {
        parts.push(format!("v{v}"));
    }
    if let Some(b) = it.size_bytes {
        parts.push(fmt_size(b));
    }
    // Repos carry no version, so lean on their metadata to make the line useful.
    if let Some(stacks) = it.metadata.get("stacks").and_then(|v| v.as_array()) {
        let list: Vec<&str> = stacks.iter().filter_map(|v| v.as_str()).collect();
        if !list.is_empty() {
            parts.push(list.join("/"));
        }
    }
    if let Some(c) = meta_str(it, "last_commit") {
        parts.push(format!("last commit {c}"));
    }
    if meta_bool(it, "outdated") {
        parts.push(match meta_str(it, "latest") {
            Some(l) => format!("outdated → {l}"),
            None => "outdated".into(),
        });
    }
    if meta_bool(it, "deprecated") {
        parts.push("DEPRECATED".into());
    }
    if !it.tags.is_empty() {
        parts.push(it.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" "));
    }
    if let Some(w) = it.why.as_deref().filter(|s| !s.is_empty()) {
        parts.push(format!("— {w}"));
    }
    format!("{}  |  {}", it.item_key, parts.join("  "))
}

/// How long ago the scan ran, in words, so the model can judge staleness.
fn scan_age(finished_at: &str) -> String {
    let Ok(then) = chrono::DateTime::parse_from_rfc3339(finished_at) else {
        return finished_at.to_string();
    };
    let mins = (chrono::Utc::now() - then.with_timezone(&chrono::Utc)).num_minutes();
    match mins {
        m if m < 0 => "just now".into(),
        m if m < 2 => "just now".into(),
        m if m < 60 => format!("{m} minutes ago"),
        m if m < 60 * 48 => format!("{} hours ago", m / 60),
        m => format!("{} days ago", m / (60 * 24)),
    }
}

fn summary_text(inv: &Inventory) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# {} ({})\n\n{} items · scanned {} ({} in {} ms)\n",
        inv.scan.host,
        inv.scan.os,
        inv.scan.item_count,
        scan_age(&inv.scan.finished_at),
        inv.scan.finished_at,
        inv.scan.duration_ms,
    ));
    if let Ok(then) = chrono::DateTime::parse_from_rfc3339(&inv.scan.finished_at) {
        let hours = (chrono::Utc::now() - then.with_timezone(&chrono::Utc)).num_hours();
        if hours >= 24 {
            out.push_str("\n> This data is over a day old. Call `scan` to refresh it.\n");
        }
    }

    // Domains, in the order the app presents them.
    let mut by_domain: BTreeMap<&str, i64> = BTreeMap::new();
    let mut by_collector: BTreeMap<String, (i64, i64)> = BTreeMap::new(); // count, bytes
    let mut total_bytes: i64 = 0;
    for it in &inv.items {
        *by_domain.entry(it.domain.as_str()).or_default() += 1;
        let e = by_collector.entry(it.collector.clone()).or_default();
        e.0 += 1;
        if let Some(b) = it.size_bytes {
            e.1 += b;
            total_bytes += b;
        }
    }

    out.push_str("\n## By domain\n");
    for (d, n) in &by_domain {
        out.push_str(&format!("- {d}: {n}\n"));
    }

    out.push_str("\n## By collector\n");
    let mut collectors: Vec<_> = by_collector.iter().collect();
    collectors.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    for (c, (n, bytes)) in collectors {
        let size = if *bytes > 0 { format!(" · {}", fmt_size(*bytes)) } else { String::new() };
        out.push_str(&format!("- {c}: {n}{size}\n"));
    }

    let outdated = inv.items.iter().filter(|i| meta_bool(i, "outdated")).count();
    let deprecated = inv.items.iter().filter(|i| meta_bool(i, "deprecated")).count();
    out.push_str(&format!(
        "\n## Attention\n- {outdated} outdated · {deprecated} deprecated\n- {} tracked on disk\n",
        fmt_size(total_bytes)
    ));

    let mut tags: BTreeMap<&str, usize> = BTreeMap::new();
    for it in &inv.items {
        for t in &it.tags {
            *tags.entry(t.as_str()).or_default() += 1;
        }
    }
    if !tags.is_empty() {
        let list: Vec<String> = tags.iter().map(|(t, n)| format!("#{t} ({n})")).collect();
        out.push_str(&format!("- tags: {}\n", list.join(", ")));
    }

    out.push_str("\nNarrow down with `search_items`, or read the whole map with `export_agent_map`.\n");
    out
}

// ------------------------------------------------------------------ read tools

fn search_items(s: &mut Server, a: &Value) -> Result<String, String> {
    let inv = inventory(s)?;

    let query = str_arg(a, "query").map(|q| q.to_lowercase());
    let domain = str_arg(a, "domain").map(|d| d.to_lowercase());
    let collector = str_arg(a, "collector").map(|c| c.to_lowercase());
    let tag = str_arg(a, "tag").map(|t| t.to_lowercase());
    let outdated = bool_arg(a, "outdated");
    let has_note = bool_arg(a, "has_note");
    let min_size = i64_arg(a, "min_size_bytes");

    let mut matched: Vec<&Item> = inv
        .items
        .iter()
        .filter(|it| {
            if let Some(q) = &query {
                let in_name = it.name.to_lowercase().contains(q);
                let in_path = it
                    .source_path
                    .as_deref()
                    .map(|p| p.to_lowercase().contains(q))
                    .unwrap_or(false);
                if !in_name && !in_path {
                    return false;
                }
            }
            if let Some(d) = &domain {
                if it.domain.as_str() != d {
                    return false;
                }
            }
            if let Some(c) = &collector {
                if it.collector.to_lowercase() != *c {
                    return false;
                }
            }
            if let Some(t) = &tag {
                if !it.tags.iter().any(|x| x.to_lowercase() == *t) {
                    return false;
                }
            }
            if let Some(o) = outdated {
                if meta_bool(it, "outdated") != o {
                    return false;
                }
            }
            if let Some(n) = has_note {
                let annotated = it.why.as_deref().is_some_and(|w| !w.is_empty())
                    || it.note.as_deref().is_some_and(|w| !w.is_empty());
                if annotated != n {
                    return false;
                }
            }
            if let Some(m) = min_size {
                if it.size_bytes.unwrap_or(0) < m {
                    return false;
                }
            }
            true
        })
        .collect();

    if str_arg(a, "sort").as_deref() == Some("size") {
        matched.sort_by(|x, y| y.size_bytes.unwrap_or(0).cmp(&x.size_bytes.unwrap_or(0)));
    } else {
        matched.sort_by(|x, y| {
            x.collector
                .cmp(&y.collector)
                .then(x.name.to_lowercase().cmp(&y.name.to_lowercase()))
        });
    }

    let total = matched.len();
    let offset = usize_arg(a, "offset").unwrap_or(0);
    let limit = usize_arg(a, "limit").unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let page: Vec<&Item> = matched.into_iter().skip(offset).take(limit).collect();

    if total == 0 {
        return Ok(format!(
            "No items matched. This machine has {} items in total — try `list_collectors` to \
             see what's available, or drop a filter.",
            inv.scan.item_count
        ));
    }

    let mut out = if page.len() < total {
        format!(
            "{total} items matched — showing {}–{} (use `offset` to page).\n\n",
            offset + 1,
            offset + page.len()
        )
    } else {
        format!("{total} items matched.\n\n")
    };
    for it in page {
        out.push_str(&item_line(it));
        out.push('\n');
    }
    Ok(out)
}

fn find_item<'a>(inv: &'a Inventory, a: &Value) -> Result<&'a Item, String> {
    if let Some(key) = str_arg(a, "item_key") {
        return inv
            .items
            .iter()
            .find(|i| i.item_key == key)
            .ok_or_else(|| format!("no item with item_key `{key}` in the current inventory"));
    }
    let (Some(c), Some(n)) = (str_arg(a, "collector"), str_arg(a, "name")) else {
        return Err("provide either `item_key`, or both `collector` and `name`".into());
    };
    inv.items
        .iter()
        .find(|i| i.collector.eq_ignore_ascii_case(&c) && i.name.eq_ignore_ascii_case(&n))
        .ok_or_else(|| format!("no item named `{n}` from collector `{c}`"))
}

async fn get_item(s: &mut Server, a: &Value) -> Result<String, String> {
    let inv = inventory(s)?;
    let it = find_item(&inv, a)?;

    let mut out = format!("# {}\n\n", it.name);
    out.push_str(&format!("- item_key: `{}`\n", it.item_key));
    out.push_str(&format!("- domain: {}\n- collector: {}\n", it.domain.as_str(), it.collector));
    if let Some(v) = &it.version {
        out.push_str(&format!("- version: {v}\n"));
    }
    if let Some(p) = &it.source_path {
        out.push_str(&format!("- path: {p}\n"));
    }
    if let Some(b) = it.size_bytes {
        out.push_str(&format!("- size: {}\n", fmt_size(b)));
    }
    if meta_bool(it, "outdated") {
        out.push_str(&format!(
            "- outdated: yes{}\n",
            meta_str(it, "latest").map(|l| format!(" (latest {l})")).unwrap_or_default()
        ));
    }
    if meta_bool(it, "deprecated") {
        out.push_str("- deprecated: yes\n");
    }
    if let Some(w) = it.why.as_deref().filter(|s| !s.is_empty()) {
        out.push_str(&format!("- why: {w}\n"));
    }
    if let Some(n) = it.note.as_deref().filter(|s| !s.is_empty()) {
        out.push_str(&format!("- note: {n}\n"));
    }
    if !it.tags.is_empty() {
        out.push_str(&format!("- tags: {}\n", it.tags.join(", ")));
    }

    if it.metadata.as_object().is_some_and(|m| !m.is_empty()) {
        out.push_str(&format!(
            "\n## Metadata\n```json\n{}\n```\n",
            serde_json::to_string_pretty(&it.metadata).unwrap_or_default()
        ));
    }

    if bool_arg(a, "enrich").unwrap_or(true) {
        let e = crate::scan::enrich::enrich(&it.collector, &it.name, it.source_path.clone()).await;
        let mut lines = Vec::new();
        if let Some(d) = &e.description {
            lines.push(format!("- description: {d}"));
        }
        if let Some(h) = &e.homepage {
            lines.push(format!("- homepage: {h}"));
        }
        if let Some(l) = &e.latest_version {
            lines.push(format!("- latest version: {l}"));
        }
        if let Some(i) = &e.installed_at {
            lines.push(format!("- installed/updated: {i}"));
        }
        if !lines.is_empty() {
            out.push_str(&format!("\n## Details\n{}\n", lines.join("\n")));
        }
    }

    let actions = crate::manage::info(&it.collector, &it.name);
    let mut lines = Vec::new();
    if let Some(c) = &actions.install {
        lines.push(format!("- install: `{c}`"));
    }
    if let Some(c) = &actions.update {
        lines.push(format!("- update: `{c}`"));
    }
    if let Some(c) = &actions.delete {
        lines.push(format!("- uninstall: `{c}`"));
    }
    if lines.is_empty() {
        out.push_str("\n## Actions\nNo managed install/update/uninstall for this collector.\n");
    } else {
        out.push_str(&format!(
            "\n## Actions\n{}\n- tool available here: {}\n",
            lines.join("\n"),
            if actions.available { "yes" } else { "no" }
        ));
        if !s.allow_write() {
            out.push_str(
                "\nThese are not runnable right now (writes are disabled in IO Inventory's \
                 settings). Report the command to the user so they can run it themselves.\n",
            );
        }
    }
    Ok(out)
}

fn list_collectors(s: &mut Server) -> Result<String, String> {
    let inv = inventory(s)?;
    let mut by: BTreeMap<(&str, &str), (i64, i64)> = BTreeMap::new();
    for it in &inv.items {
        let e = by.entry((it.domain.as_str(), it.collector.as_str())).or_default();
        e.0 += 1;
        e.1 += it.size_bytes.unwrap_or(0);
    }
    let mut out = format!("{} collectors found items on this machine.\n\n", by.len());
    out.push_str("domain / collector — count · size\n");
    for ((d, c), (n, bytes)) in by {
        let size = if bytes > 0 { format!(" · {}", fmt_size(bytes)) } else { String::new() };
        out.push_str(&format!("- {d} / {c} — {n}{size}\n"));
    }
    Ok(out)
}

fn list_tags(s: &mut Server) -> Result<String, String> {
    let inv = inventory(s)?;
    let mut tags: BTreeMap<&str, usize> = BTreeMap::new();
    for it in &inv.items {
        for t in &it.tags {
            *tags.entry(t.as_str()).or_default() += 1;
        }
    }
    if tags.is_empty() {
        return Ok("No tags assigned yet. Use `set_tags` to group items into views.".into());
    }
    let mut out = format!("{} tags in use.\n\n", tags.len());
    for (t, n) in tags {
        out.push_str(&format!("- #{t}: {n} items\n"));
    }
    Ok(out)
}

fn export_agent_map(s: &mut Server, a: &Value) -> Result<String, String> {
    let mut inv = inventory(s)?;

    if let Some(d) = str_arg(a, "domain").map(|d| d.to_lowercase()) {
        inv.items.retain(|i| i.domain.as_str() == d);
        inv.scan.item_count = inv.items.len() as i64;
        if inv.items.is_empty() {
            return Err(format!("no items in domain `{d}`"));
        }
    }

    let content = crate::export::to_agent_map(&inv);
    let mut out = String::new();

    if bool_arg(a, "write_file").unwrap_or(false) {
        let dir = dirs::document_dir().unwrap_or_else(crate::scan::util::home);
        let path = dir.join("AGENT_MAP.md");
        std::fs::write(&path, &content).map_err(|e| format!("could not write the file: {e}"))?;
        out.push_str(&format!("Wrote {}\n\n---\n\n", path.display()));
    }

    if content.chars().count() > MAX_MAP_CHARS {
        let truncated: String = content.chars().take(MAX_MAP_CHARS).collect();
        out.push_str(&truncated);
        out.push_str(&format!(
            "\n\n… truncated at {MAX_MAP_CHARS} of {} characters. Pass a `domain` to narrow the \
             map, or `write_file: true` and read the file directly.\n",
            content.chars().count()
        ));
    } else {
        out.push_str(&content);
    }
    Ok(out)
}

fn get_roots(s: &mut Server) -> Result<String, String> {
    let roots = s.effective_roots();
    if roots.is_empty() {
        return Ok("No workspace roots configured — git repositories won't be found. Set them \
                   in the app's Settings, pass `roots` to `scan`, or start ioinv-mcp with \
                   --roots."
            .into());
    }
    let list: Vec<String> = roots.iter().map(|p| format!("- {}", p.display())).collect();
    let mut out = format!("Scanned for git repositories:\n{}\n", list.join("\n"));

    let settings = s.db.settings();
    let off: Vec<&str> = crate::settings::SOURCES
        .iter()
        .filter(|src| !settings.is_enabled(src.id))
        .map(|src| src.label)
        .collect();
    if !off.is_empty() {
        out.push_str(&format!(
            "\nThese sources are switched off in the app's settings and won't appear in the \
             inventory: {}.\n",
            off.join(", ")
        ));
    }
    Ok(out)
}

fn item_actions(a: &Value) -> Result<String, String> {
    let collector = req_str(a, "collector")?;
    let name = req_str(a, "name")?;
    let info = crate::manage::info(&collector, &name);
    let mut lines = Vec::new();
    if let Some(c) = &info.install {
        lines.push(format!("- install: `{c}`"));
    }
    if let Some(c) = &info.update {
        lines.push(format!("- update: `{c}`"));
    }
    if let Some(c) = &info.delete {
        lines.push(format!("- uninstall: `{c}`"));
    }
    if lines.is_empty() {
        return Ok(format!("No managed actions for collector `{collector}`."));
    }
    Ok(format!(
        "Actions for {name} ({collector}):\n{}\n\nTool installed on this machine: {}\n",
        lines.join("\n"),
        if info.available { "yes" } else { "no" }
    ))
}

// ----------------------------------------------------------- annotation tools

fn set_note(s: &mut Server, a: &Value) -> Result<String, String> {
    let key = req_str(a, "item_key")?;
    let inv = inventory(s)?;
    let existing = inv.items.iter().find(|i| i.item_key == key);
    if existing.is_none() {
        return Err(format!(
            "no item with item_key `{key}` in the current inventory — check it with `search_items`"
        ));
    }
    // Omitted fields keep their current value rather than being cleared.
    let note = str_arg(a, "note")
        .or_else(|| existing.and_then(|i| i.note.clone()))
        .unwrap_or_default();
    let why = str_arg(a, "why")
        .or_else(|| existing.and_then(|i| i.why.clone()))
        .unwrap_or_default();
    s.db.set_note(&key, &note, &why).map_err(|e| e.to_string())?;
    Ok(format!("Saved. {key} — why: {}", if why.is_empty() { "(none)" } else { &why }))
}

fn set_tags(s: &mut Server, a: &Value) -> Result<String, String> {
    let key = req_str(a, "item_key")?;
    let tags: Vec<String> = a
        .get("tags")
        .and_then(|v| v.as_array())
        .ok_or("`tags` must be an array of strings")?
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let inv = inventory(s)?;
    if !inv.items.iter().any(|i| i.item_key == key) {
        return Err(format!(
            "no item with item_key `{key}` in the current inventory — check it with `search_items`"
        ));
    }
    s.db.set_item_tags(&key, &tags).map_err(|e| e.to_string())?;
    Ok(if tags.is_empty() {
        format!("Cleared all tags on {key}.")
    } else {
        format!("{key} is now tagged: {}", tags.join(", "))
    })
}

// -------------------------------------------------------------------- scanning

async fn scan(s: &mut Server, a: &Value) -> Result<String, String> {
    let roots: Vec<PathBuf> = match a.get("roots").and_then(|v| v.as_array()) {
        Some(arr) => arr.iter().filter_map(|v| v.as_str()).map(PathBuf::from).collect(),
        None => s.effective_roots(),
    };
    // Honour the sources the user switched off in the app's settings, so the
    // agent's view matches the UI's.
    let settings = s.db.settings();

    let started_at = chrono::Utc::now().to_rfc3339();
    let timer = Instant::now();
    let items = crate::scan::run_all(roots, &settings).await;
    let finished_at = chrono::Utc::now().to_rfc3339();
    let duration_ms = timer.elapsed().as_millis() as i64;

    s.db.save_scan(
        &crate::scan::util::host_name(),
        &crate::scan::util::os_name(),
        &started_at,
        &finished_at,
        duration_ms,
        &items,
    )
    .map_err(|e| format!("scan succeeded but could not be saved: {e}"))?;

    let inv = inventory(s)?;
    Ok(format!("Scan complete — {} items in {duration_ms} ms.\n\n{}", items.len(), summary_text(&inv)))
}

// ------------------------------------------------------------------- snapshots

fn list_snapshots(s: &mut Server) -> Result<String, String> {
    let snaps = s.db.list_snapshots().map_err(|e| e.to_string())?;
    if snaps.is_empty() {
        return Ok("No snapshots saved yet. Use `save_snapshot` to record the current state so \
                   it can be diffed later."
            .into());
    }
    let mut out = format!("{} snapshots, newest first.\n\n", snaps.len());
    for m in snaps {
        out.push_str(&format!(
            "- id {} · {} · {} items · {} · {} ({})\n",
            m.id, m.name, m.item_count, m.created_at, m.host, m.source
        ));
    }
    out.push_str("\nUse `diff_snapshot` with an id to see what changed since.\n");
    Ok(out)
}

fn save_snapshot(s: &mut Server, a: &Value) -> Result<String, String> {
    let inv = inventory(s)?;
    let name = str_arg(a, "name")
        .unwrap_or_else(|| format!("Snapshot {}", chrono::Local::now().format("%Y-%m-%d %H:%M")));
    let created_at = chrono::Utc::now().to_rfc3339();
    let meta = s
        .db
        .save_snapshot(&name, &created_at, &inv.scan.host, &inv.scan.os, "scan", &inv.items)
        .map_err(|e| e.to_string())?;
    Ok(format!("Saved snapshot id {} — \"{}\" with {} items.", meta.id, meta.name, meta.item_count))
}

fn diff_snapshot(s: &mut Server, a: &Value) -> Result<String, String> {
    let id = i64_arg(a, "id").ok_or("`id` must be a snapshot id (see list_snapshots)")?;
    let (meta, base_items) = s
        .db
        .get_snapshot(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no snapshot with id {id}"))?;
    let current = inventory(s)?;

    let label = format!("{} · {}", meta.name, meta.created_at.chars().take(10).collect::<String>());
    let d = crate::snapshot::diff(&base_items, &current.items, &label, "Current scan");

    let mut out = format!(
        "# {} → {}\n\n{} added · {} removed · {} changed · {} unchanged\n",
        d.base_label,
        d.target_label,
        d.added.len(),
        d.removed.len(),
        d.changed.len(),
        d.unchanged
    );
    if !d.added.is_empty() {
        out.push_str("\n## Added\n");
        for i in &d.added {
            out.push_str(&format!(
                "- {} ({}){}\n",
                i.name,
                i.collector,
                i.version.as_deref().map(|v| format!(" v{v}")).unwrap_or_default()
            ));
        }
    }
    if !d.removed.is_empty() {
        out.push_str("\n## Removed\n");
        for i in &d.removed {
            out.push_str(&format!(
                "- {} ({}){}\n",
                i.name,
                i.collector,
                i.version.as_deref().map(|v| format!(" v{v}")).unwrap_or_default()
            ));
        }
    }
    if !d.changed.is_empty() {
        out.push_str("\n## Version changed\n");
        for c in &d.changed {
            out.push_str(&format!(
                "- {} ({}): {} → {}\n",
                c.name,
                c.collector,
                c.old_version.as_deref().unwrap_or("?"),
                c.new_version.as_deref().unwrap_or("?")
            ));
        }
    }
    Ok(out)
}

// -------------------------------------------------------------------- cleanups

fn list_cleanups() -> String {
    let actions = crate::cleanup::list();
    let mut out = String::from("Maintenance actions. Run `preview_cleanup` first to see what \
                                each would do.\n\n");
    for c in actions {
        out.push_str(&format!(
            "- `{}` [{}] {} — `{}`{}\n  {}\n",
            c.id,
            c.category,
            c.title,
            c.command,
            if c.available { "" } else { "  (tool NOT installed here)" },
            c.description
        ));
    }
    out
}

async fn preview_cleanup(a: &Value) -> Result<String, String> {
    let id = req_str(a, "id")?;
    let p = crate::cleanup::preview(&id).await;
    Ok(format!("Dry run of `{}` — would run: {}\n\n{}", p.id, p.command, p.output))
}

// ---------------------------------------------------------------- write tools

async fn run_item_action(a: &Value) -> Result<String, String> {
    let collector = req_str(a, "collector")?;
    let name = req_str(a, "name")?;
    let action = req_str(a, "action")?;
    if !matches!(action.as_str(), "install" | "update" | "delete") {
        return Err(format!("`action` must be install, update, or delete (got `{action}`)"));
    }
    let r = crate::manage::run(&collector, &name, &action).await;
    let status = if r.success { "succeeded" } else { "FAILED" };
    if r.success {
        Ok(format!("`{}` {status}.\n\n{}\n\nRun `scan` to refresh the inventory.", r.command, r.output))
    } else {
        Err(format!("`{}` {status}.\n\n{}", r.command, r.output))
    }
}

async fn run_cleanup(a: &Value) -> Result<String, String> {
    let id = req_str(a, "id")?;
    let r = crate::cleanup::run(&id).await;
    if r.success {
        Ok(format!("`{}` completed.\n\n{}\n\nRun `scan` to refresh the inventory.", r.command, r.output))
    } else {
        Err(format!("`{}` failed.\n\n{}", r.command, r.output))
    }
}
