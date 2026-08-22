//! Tauri-free core: scan engine, item model, ledger persistence, and the MCP
//! server. Depended on by the desktop app (`agent-ledger`), the `ioinv-mcp`
//! binary, and (via a git dependency) the standalone TUI.

pub mod cleanup;
pub mod db;
pub mod export;
pub mod graph;
pub mod manage;
pub mod mcp;
pub mod model;
pub mod scan;
pub mod settings;
pub mod snapshot;
pub mod state;
