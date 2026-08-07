//! `ioinv-mcp` — the IO Inventory MCP server.
//!
//! Lets AI agents query and annotate this machine's developer/AI inventory over
//! the Model Context Protocol. Speaks JSON-RPC over stdio, so it's launched by
//! the agent client rather than run by hand:
//!
//! ```jsonc
//! { "mcpServers": { "io-inventory": { "command": "/path/to/ioinv-mcp" } } }
//! ```
//!
//! Run `ioinv-mcp --print-config` to get that snippet with the right path
//! filled in. It shares `ledger.sqlite` with the desktop app, so the app does
//! not need to be running.

use agent_ledger_lib::{db, mcp};
use std::path::PathBuf;

const HELP: &str = "\
ioinv-mcp — IO Inventory MCP server (stdio)

USAGE:
    ioinv-mcp [OPTIONS]

Normally launched by an MCP client, not run directly. With no options it serves
the protocol on stdin/stdout in read-only mode.

OPTIONS:
    --allow-write        Force the tools that change this machine (install /
                         update / uninstall / cleanups) on, ignoring the app's
                         toggle. Also settable via IOINV_MCP_ALLOW_WRITE=1.
                         Normally you don't need this: leave it off and use
                         Settings -> MCP server -> 'Allow write actions' in the
                         IO Inventory app, which this server re-reads on every
                         request.
    --db <PATH>          Ledger database to use. Defaults to the desktop app's,
                         so both see the same data. Also IOINV_DB.
    --roots <A,B,...>    Directories to search for git repositories. Overrides
                         the workspace roots configured in the app's settings.
    --print-config       Print an MCP client config snippet for this binary and
                         exit.
    -V, --version        Print the version and exit.
    -h, --help           Print this help and exit.
";

fn main() {
    let mut forced_write = std::env::var("IOINV_MCP_ALLOW_WRITE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let mut db_path: Option<PathBuf> = None;
    let mut roots: Option<Vec<PathBuf>> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return;
            }
            "-V" | "--version" => {
                println!("ioinv-mcp {}", mcp::SERVER_VERSION);
                return;
            }
            "--allow-write" => forced_write = true,
            "--print-config" => {
                print_config(forced_write);
                return;
            }
            "--db" => {
                i += 1;
                match args.get(i) {
                    Some(v) => db_path = Some(PathBuf::from(v)),
                    None => fail("--db needs a path"),
                }
            }
            "--roots" => {
                i += 1;
                match args.get(i) {
                    Some(v) => {
                        roots = Some(
                            v.split(',')
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(PathBuf::from)
                                .collect(),
                        )
                    }
                    None => fail("--roots needs a comma-separated list of directories"),
                }
            }
            other => fail(&format!("unknown option `{other}` — try --help")),
        }
        i += 1;
    }

    let opts = mcp::Options {
        db_path: db_path.unwrap_or_else(db::default_path),
        roots,
        forced_write,
    };

    // The scan runs collectors concurrently, so a multi-thread runtime it is.
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => fail(&format!("could not start the async runtime: {e}")),
    };
    if let Err(e) = rt.block_on(mcp::serve(opts)) {
        fail(&format!("{e}"));
    }
}

/// Print a ready-to-paste MCP client config. Written to stdout so it can be
/// piped straight into a config file.
fn print_config(forced_write: bool) {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "ioinv-mcp".into());
    let args = if forced_write {
        serde_json::json!(["--allow-write"])
    } else {
        serde_json::json!([])
    };
    let config = serde_json::json!({
        "mcpServers": {
            "io-inventory": { "command": exe, "args": args }
        }
    });
    println!("{}", serde_json::to_string_pretty(&config).unwrap_or_default());

    // Hints go to stderr so stdout stays valid JSON.
    eprintln!();
    eprintln!("Claude Code:    claude mcp add io-inventory -- \"{exe}\"{}",
        if forced_write { " --allow-write" } else { "" });
    eprintln!("Claude Desktop: merge the JSON above into claude_desktop_config.json");
    if !forced_write {
        eprintln!();
        eprintln!("Install/update/uninstall stay off until you enable them in the IO Inventory");
        eprintln!("app under Settings -> MCP server. No config change needed to flip it.");
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("ioinv-mcp: {msg}");
    std::process::exit(1)
}
