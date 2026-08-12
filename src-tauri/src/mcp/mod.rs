//! MCP (Model Context Protocol) server exposing the inventory to AI agents.
//!
//! Runs as its own process (`ioinv-mcp`), speaking JSON-RPC 2.0 over stdio:
//! one JSON message per line on stdin, one per line on stdout. **stdout is the
//! protocol channel — nothing may be printed there except responses.** All
//! diagnostics go to stderr via [`log`].
//!
//! It reuses the same scan engine and the same `ledger.sqlite` as the desktop
//! app (see [`crate::db::default_path`]), so an agent sees exactly what the UI
//! sees and doesn't need the app to be running. The database is opened in WAL
//! mode so both processes can be live at once.
//!
//! Requests are handled one at a time. That's deliberate: a scan is ~2s and a
//! cleanup can be minutes, and serialising keeps the single `Db` handle
//! straightforward. MCP clients wait for a response before sending the next
//! request, so this costs nothing in practice.

mod tools;

use crate::db::Db;
use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub const SERVER_NAME: &str = "io-inventory";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Protocol revisions we know how to speak, newest first. We echo back the
/// client's version when we recognise it, otherwise we answer with our latest.
const SUPPORTED_PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const DEFAULT_PROTOCOL: &str = "2025-06-18";

/// Sent to the client on `initialize`; it's the model's orientation to what
/// this server is for.
const INSTRUCTIONS: &str = "\
IO Inventory indexes everything developer- and AI-related installed on this machine: \
Homebrew/npm/pip/cargo/gem packages, language runtimes, git repositories, Docker images \
and containers, Ollama models, Hugging Face caches, and AI agent config (Claude skills, \
commands, agents, MCP servers).

Start with `inventory_summary` to see what's here and how fresh the data is, then narrow \
with `search_items` (filter by domain, collector, tag, or outdated) and `get_item` for \
detail on one thing. `export_agent_map` renders the whole environment as Markdown, which \
is the best single artifact to read when onboarding to this machine.

Results come from the last saved scan. Run `scan` first if the summary says the data is \
stale, or after installing something. `set_note` and `set_tags` let you record why a tool \
is here; those annotations survive re-scans and show up in the desktop app.";

/// What the desktop app needs to show on the MCP settings page.
#[derive(serde::Serialize)]
pub struct McpInfo {
    /// Absolute path to the `ioinv-mcp` binary, if it's present.
    pub binary_path: Option<String>,
    /// False in a dev run, where the sidecar isn't staged next to the app.
    pub available: bool,
    /// Ledger the server will read — the same one the app uses.
    pub db_path: String,
    /// Ready-to-paste `mcpServers` config for Claude Desktop and friends.
    pub config_json: String,
    /// One-liner for `claude mcp add`.
    pub cli_command: String,
    pub server_name: &'static str,
    pub version: &'static str,
}

/// Locate the bundled MCP server. Tauri drops external binaries next to the
/// main executable (`IO Inventory.app/Contents/MacOS/ioinv-mcp`), so that's
/// where we look. In `tauri dev` there's no sidecar and this reports
/// unavailable rather than guessing at a target/ path.
pub fn info() -> McpInfo {
    let exe_name = if cfg!(windows) { "ioinv-mcp.exe" } else { "ioinv-mcp" };
    let found = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe_name)))
        .filter(|p| p.is_file());

    let path_str = found
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| exe_name.to_string());

    let config = serde_json::json!({
        "mcpServers": {
            SERVER_NAME: { "command": path_str, "args": [] }
        }
    });

    McpInfo {
        available: found.is_some(),
        binary_path: found.map(|p| p.to_string_lossy().into_owned()),
        db_path: crate::db::default_path().to_string_lossy().into_owned(),
        config_json: serde_json::to_string_pretty(&config).unwrap_or_default(),
        cli_command: format!("claude mcp add {SERVER_NAME} -- \"{path_str}\""),
        server_name: SERVER_NAME,
        version: SERVER_VERSION,
    }
}

pub struct Options {
    pub db_path: PathBuf,
    /// `--roots` on the command line. `None` means "use whatever the user
    /// configured in the app's settings", which is the normal case.
    pub roots: Option<Vec<PathBuf>>,
    /// Force the machine-changing tools on regardless of the app's toggle
    /// (the `--allow-write` flag).
    pub forced_write: bool,
}

pub struct Server {
    db: Db,
    roots: Option<Vec<PathBuf>>,
    /// `--allow-write` was passed on the command line. This forces write access
    /// on regardless of the app setting; without it the app's toggle decides.
    forced_write: bool,
}

impl Server {
    /// Directories to search for git repos: an explicit `--roots` wins,
    /// otherwise the roots the user set in the desktop app.
    fn effective_roots(&self) -> Vec<PathBuf> {
        match &self.roots {
            Some(r) => r.clone(),
            None => self.db.settings().roots(),
        }
    }

    /// Whether the machine-changing tools are available right now.
    ///
    /// Read fresh from the ledger on every call rather than cached at startup,
    /// so flipping the toggle off in the app takes effect immediately — the
    /// safety-critical direction shouldn't wait for the agent to reconnect.
    fn allow_write(&self) -> bool {
        self.forced_write || self.db.settings().mcp_allow_write
    }
}

/// Write a diagnostic line to stderr. Never use stdout — it carries the protocol.
pub fn log(msg: &str) {
    eprintln!("[{SERVER_NAME}] {msg}");
}

/// Run the server until stdin closes.
pub async fn serve(opts: Options) -> Result<()> {
    let db = Db::open(&opts.db_path)?;
    log(&format!(
        "v{SERVER_VERSION} ready — ledger: {} — {}",
        opts.db_path.display(),
        if opts.forced_write {
            "write actions FORCED ON by --allow-write"
        } else {
            "write actions follow the app's Settings toggle (read-only unless enabled there)"
        }
    ));

    let mut server = Server {
        db,
        roots: opts.roots,
        forced_write: opts.forced_write,
    };

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let resp = error_response(Value::Null, -32700, &format!("parse error: {e}"));
                send(&mut out, &resp).await?;
                continue;
            }
        };

        // JSON-RPC batches were dropped in MCP 2025-06-18 but older clients may
        // still send them; handling arrays costs nothing.
        match msg {
            Value::Array(batch) => {
                for m in batch {
                    if let Some(resp) = server.handle(m).await {
                        send(&mut out, &resp).await?;
                    }
                }
            }
            m => {
                if let Some(resp) = server.handle(m).await {
                    send(&mut out, &resp).await?;
                }
            }
        }
    }

    log("stdin closed — shutting down");
    Ok(())
}

async fn send<W: AsyncWriteExt + Unpin>(out: &mut W, msg: &Value) -> Result<()> {
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    out.write_all(line.as_bytes()).await?;
    out.flush().await?;
    Ok(())
}

/// A JSON-RPC level failure (bad method, malformed params). Failures *inside* a
/// tool are reported as `isError` results instead, so the model can read and
/// recover from them.
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn method_not_found(m: &str) -> RpcError {
        RpcError { code: -32601, message: format!("method not found: {m}") }
    }
    fn invalid_params(m: &str) -> RpcError {
        RpcError { code: -32602, message: m.into() }
    }
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// A tool result the model should treat as a failure but can act on.
fn tool_error(message: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": message.into() }], "isError": true })
}

fn tool_ok(text: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": false })
}

impl Server {
    /// Handle one message. Returns `None` for notifications (no `id`), which
    /// must not be answered.
    async fn handle(&mut self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let Some(method) = msg.get("method").and_then(|m| m.as_str()).map(str::to_string) else {
            // A response to something we never sent; nothing to do.
            return None;
        };
        let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

        let result = match method.as_str() {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::list(self.allow_write()) })),
            "tools/call" => self.call_tool(&params).await,
            "resources/list" => Ok(json!({ "resources": tools::resources() })),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
            "resources/read" => self.read_resource(&params).await,
            "prompts/list" => Ok(json!({ "prompts": [] })),
            "logging/setLevel" => Ok(json!({})),
            m if m.starts_with("notifications/") => Ok(json!({})),
            other => Err(RpcError::method_not_found(other)),
        };

        // Notifications get no reply, even on error.
        let id = id?;
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => error_response(id, e.code, &e.message),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_PROTOCOL);
        let protocol = if SUPPORTED_PROTOCOLS.contains(&requested) {
            requested
        } else {
            DEFAULT_PROTOCOL
        };
        json!({
            "protocolVersion": protocol,
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false },
            },
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
            "instructions": INSTRUCTIONS,
        })
    }

    async fn call_tool(&mut self, params: &Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("tools/call requires a `name`"))?
            .to_string();
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        if tools::is_write_tool(&name) && !self.allow_write() {
            return Ok(tool_error(format!(
                "`{name}` changes what is installed on this machine and is currently disabled. \
                 The user can enable it in the IO Inventory app under Settings → MCP server → \
                 \"Allow write actions\" (it takes effect immediately; you may need to re-list \
                 tools to see it). Read-only tools still work — `item_actions` and \
                 `preview_cleanup` show exactly what would run, so report the command instead."
            )));
        }

        Ok(match tools::dispatch(self, &name, &args).await {
            Ok(text) => tool_ok(text),
            Err(msg) => tool_error(msg),
        })
    }

    async fn read_resource(&mut self, params: &Value) -> Result<Value, RpcError> {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("resources/read requires a `uri`"))?;
        let (mime, text) = tools::read_resource(self, uri)
            .map_err(|e| RpcError::invalid_params(&e))?;
        Ok(json!({
            "contents": [{ "uri": uri, "mimeType": mime, "text": text }]
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server backed by a throwaway ledger. Tests run in parallel in one
    /// process, so the filename has to be unique per call, not per process.
    ///
    /// A wall-clock stamp isn't enough: `SystemTime` is only microsecond-
    /// resolution on macOS, two tests starting in the same tick collided on one
    /// path, and the loser failed with "database is locked". A counter can't
    /// collide.
    fn test_server() -> Server {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("io-inv-mcp-{}-{seq}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut db = Db::open(&path).unwrap();
        let items = vec![
            crate::model::Item::new(crate::model::Domain::PackageManager, "homebrew", "ripgrep")
                .version("14.1.0")
                .size(12_400_000),
            crate::model::Item::new(crate::model::Domain::AiAgent, "ollama", "llama3")
                .version("8b"),
        ];
        db.save_scan("testhost", "macOS", "t0", "t1", 1234, &items, &[]).unwrap();
        Server { db, roots: None, forced_write: false }
    }

    fn req(id: i64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    /// Drives a realistic client handshake through the dispatcher: initialize,
    /// list tools, call a read tool, and confirm write tools stay hidden and
    /// refuse to run while --allow-write is off.
    #[tokio::test]
    async fn mcp_handshake_and_tools() {
        let mut s = test_server();

        let init = s.handle(req(1, "initialize", json!({ "protocolVersion": "2025-06-18" })))
            .await
            .expect("initialize must be answered");
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], SERVER_NAME);

        // Notifications are never answered.
        assert!(s.handle(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await.is_none());

        let listed = s.handle(req(2, "tools/list", json!({}))).await.unwrap();
        let names: Vec<String> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"search_items".to_string()));
        assert!(names.contains(&"inventory_summary".to_string()));
        assert!(
            !names.contains(&"run_cleanup".to_string()),
            "write tools must not be advertised in read-only mode: {names:?}"
        );

        let summary = s.handle(req(3, "tools/call", json!({ "name": "inventory_summary" })))
            .await
            .unwrap();
        let text = summary["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("testhost"), "summary missing host:\n{text}");
        assert!(text.contains("ripgrep") || text.contains("homebrew"), "summary missing data:\n{text}");

        let search = s.handle(req(4, "tools/call", json!({
            "name": "search_items",
            "arguments": { "query": "ripgrep" }
        }))).await.unwrap();
        let text = search["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("package_manager:homebrew:ripgrep"), "search missing item_key:\n{text}");

        // Even if a client calls a hidden write tool by name, it's refused.
        let blocked = s.handle(req(5, "tools/call", json!({
            "name": "run_cleanup",
            "arguments": { "id": "brew-cleanup" }
        }))).await.unwrap();
        assert_eq!(blocked["result"]["isError"], true);
        assert!(blocked["result"]["content"][0]["text"].as_str().unwrap().contains("Allow write actions"));

        // Unknown methods are a protocol-level error.
        let bad = s.handle(req(6, "does/not/exist", json!({}))).await.unwrap();
        assert_eq!(bad["error"]["code"], -32601);

        println!("mcp_handshake_and_tools OK — {} tools exposed read-only", names.len());
    }

    /// The app's "Allow write actions" toggle must gate the mutating tools
    /// live — turning it on exposes them without restarting the server, and
    /// turning it back off hides and refuses them again immediately.
    #[tokio::test]
    async fn mcp_write_toggle_gates_tools_live() {
        let mut s = test_server();

        let write_tools_listed = |resp: &Value| -> bool {
            resp["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "run_cleanup")
        };

        // Off by default.
        assert!(!s.allow_write());
        let listed = s.handle(req(1, "tools/list", json!({}))).await.unwrap();
        assert!(!write_tools_listed(&listed), "write tools exposed while the toggle is off");

        // Flip it on in the ledger, exactly as the app's settings page does.
        let on = crate::settings::Settings { mcp_allow_write: true, ..Default::default() };
        s.db.save_settings(&on).unwrap();

        assert!(s.allow_write(), "toggle did not take effect without a restart");
        let listed = s.handle(req(2, "tools/list", json!({}))).await.unwrap();
        assert!(write_tools_listed(&listed), "write tools still hidden after enabling");

        // And they're no longer refused outright (this one fails on a bogus id,
        // which is the tool running, not the gate rejecting it).
        let called = s.handle(req(3, "tools/call", json!({
            "name": "run_cleanup",
            "arguments": { "id": "definitely-not-an-action" }
        }))).await.unwrap();
        let text = called["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            !text.contains("Allow write actions"),
            "still blocked by the gate after enabling:\n{text}"
        );

        // Turning it off again takes effect immediately — the direction that matters.
        let off = crate::settings::Settings { mcp_allow_write: false, ..Default::default() };
        s.db.save_settings(&off).unwrap();
        assert!(!s.allow_write());
        let blocked = s.handle(req(4, "tools/call", json!({
            "name": "run_cleanup",
            "arguments": { "id": "brew-cleanup" }
        }))).await.unwrap();
        assert_eq!(blocked["result"]["isError"], true);
        assert!(blocked["result"]["content"][0]["text"].as_str().unwrap().contains("Allow write actions"));

        // The --allow-write flag still forces it on regardless of the setting.
        s.forced_write = true;
        assert!(s.allow_write(), "--allow-write must override the app toggle");

        println!("mcp_write_toggle_gates_tools_live OK");
    }

    /// A snapshot can be diffed against the live inventory (no `target_id`) or
    /// against a second snapshot, which must not touch the current state.
    #[tokio::test]
    async fn mcp_diffs_snapshot_against_snapshot() {
        let mut s = test_server();

        // Snapshot A: the machine as the fixture left it.
        let a = s.handle(req(1, "tools/call", json!({
            "name": "save_snapshot", "arguments": { "name": "A" }
        }))).await.unwrap();
        assert_eq!(a["result"]["isError"], false, "{a}");

        // Move the machine on, then take snapshot B from the new state.
        let moved = vec![
            crate::model::Item::new(crate::model::Domain::PackageManager, "homebrew", "ripgrep")
                .version("15.0.0"),
            crate::model::Item::new(crate::model::Domain::PackageManager, "homebrew", "fd")
                .version("10.2.0"),
        ];
        s.db.save_scan("testhost", "macOS", "t2", "t3", 900, &moved, &[]).unwrap();
        let b = s.handle(req(2, "tools/call", json!({
            "name": "save_snapshot", "arguments": { "name": "B" }
        }))).await.unwrap();
        assert_eq!(b["result"]["isError"], false, "{b}");

        let ids: Vec<i64> = s.db.list_snapshots().unwrap().iter().map(|m| m.id).collect();
        let (b_id, a_id) = (ids[0], ids[1]); // newest first

        let diffed = s.handle(req(3, "tools/call", json!({
            "name": "diff_snapshot", "arguments": { "id": a_id, "target_id": b_id }
        }))).await.unwrap();
        let text = diffed["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("A ·") && text.contains("B ·"), "both labels expected:\n{text}");
        assert!(
            !text.contains("Current scan"),
            "a snapshot-to-snapshot diff must not involve the live inventory:\n{text}"
        );
        // llama3 was dropped, fd gained, ripgrep bumped.
        assert!(text.contains("fd"), "added item missing:\n{text}");
        assert!(text.contains("llama3"), "removed item missing:\n{text}");
        assert!(text.contains("14.1.0 → 15.0.0"), "version change missing:\n{text}");

        // Omitting target_id still compares against the current inventory.
        let vs_current = s.handle(req(4, "tools/call", json!({
            "name": "diff_snapshot", "arguments": { "id": a_id }
        }))).await.unwrap();
        assert!(vs_current["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Current scan"));

        // Comparing a snapshot with itself is a user error, not an empty diff.
        let same = s.handle(req(5, "tools/call", json!({
            "name": "diff_snapshot", "arguments": { "id": a_id, "target_id": a_id }
        }))).await.unwrap();
        assert_eq!(same["result"]["isError"], true, "{same}");

        println!("mcp_diffs_snapshot_against_snapshot OK");
    }

    #[tokio::test]
    async fn mcp_notes_tags_and_resources() {
        let mut s = test_server();
        let key = "package_manager:homebrew:ripgrep";

        let r = s.handle(req(1, "tools/call", json!({
            "name": "set_tags",
            "arguments": { "item_key": key, "tags": ["cli", "favorite"] }
        }))).await.unwrap();
        assert_eq!(r["result"]["isError"], false, "{r}");

        let r = s.handle(req(2, "tools/call", json!({
            "name": "set_note",
            "arguments": { "item_key": key, "why": "fast grep" }
        }))).await.unwrap();
        assert_eq!(r["result"]["isError"], false, "{r}");

        // Annotations come back on the item and survive into the export.
        let item = s.handle(req(3, "tools/call", json!({
            "name": "get_item",
            "arguments": { "item_key": key, "enrich": false }
        }))).await.unwrap();
        let text = item["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("fast grep"), "note missing:\n{text}");
        assert!(text.contains("favorite"), "tags missing:\n{text}");

        // A tag filter finds it without a re-scan.
        let tagged = s.handle(req(4, "tools/call", json!({
            "name": "search_items",
            "arguments": { "tag": "favorite" }
        }))).await.unwrap();
        assert!(tagged["result"]["content"][0]["text"].as_str().unwrap().contains("ripgrep"));

        let res = s.handle(req(5, "resources/read", json!({ "uri": "ioinv://agent-map.md" })))
            .await
            .unwrap();
        let md = res["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(md.contains("# AGENT_MAP.md"), "resource is not the agent map:\n{md}");

        let missing = s.handle(req(6, "resources/read", json!({ "uri": "ioinv://nope" }))).await.unwrap();
        assert_eq!(missing["error"]["code"], -32602);

        println!("mcp_notes_tags_and_resources OK");
    }
}
