<div align="center">

<img src="public/logo.png" width="128" alt="IO Inventory logo" />

# IO Inventory

**See everything Developer- and AI-related on your machine — in one scan.**

A native desktop app that maps your local dev + AI environment into a clear, interactive
architecture graph. Think *Mole / CleanMyMac*, but for the black box of tools, packages,
repos, and AI agents that quietly pile up on a developer's machine.

<br/>

[![Stars](https://img.shields.io/github/stars/elm1nst3r/IOInventory?style=flat-square&logo=github&color=2f9bf5)](https://github.com/elm1nst3r/IOInventory/stargazers)
[![Forks](https://img.shields.io/github/forks/elm1nst3r/IOInventory?style=flat-square&logo=github&color=2f9bf5)](https://github.com/elm1nst3r/IOInventory/network/members)
[![Issues](https://img.shields.io/github/issues/elm1nst3r/IOInventory?style=flat-square&logo=github)](https://github.com/elm1nst3r/IOInventory/issues)
[![Last commit](https://img.shields.io/github/last-commit/elm1nst3r/IOInventory?style=flat-square)](https://github.com/elm1nst3r/IOInventory/commits)
[![Repo size](https://img.shields.io/github/repo-size/elm1nst3r/IOInventory?style=flat-square)](https://github.com/elm1nst3r/IOInventory)
[![Top language](https://img.shields.io/github/languages/top/elm1nst3r/IOInventory?style=flat-square&color=dea584)](https://github.com/elm1nst3r/IOInventory)
[![License: MIT](https://img.shields.io/github/license/elm1nst3r/IOInventory?style=flat-square&color=green)](LICENSE)

[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![React](https://img.shields.io/badge/React-19-61DAFB?style=for-the-badge&logo=react&logoColor=black)](https://react.dev)
[![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?style=for-the-badge&logo=typescript&logoColor=white)](https://www.typescriptlang.org)
[![SQLite](https://img.shields.io/badge/SQLite-003B57?style=for-the-badge&logo=sqlite&logoColor=white)](https://www.sqlite.org)

![macOS](https://img.shields.io/badge/macOS-000000?style=flat-square&logo=apple&logoColor=white)
![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat-square&logo=windows&logoColor=white)

</div>

---

## Why?

As development shifts toward AI-agent workflows, your machine quietly fills up with **skills,
toolchains, Homebrew packages, global npm/pip installs, half-forgotten repos, models, and a
growing zoo of AI agents**. There's no single place to see it all — an oversight deficit, a
literal black box.

**IO Inventory scans your system in ~2 seconds and lays it all out**, so you always know what's
installed, why it's there, whether it's up to date, and what's safe to clean up.

## ✨ Features

- 🗺️ **Interactive architecture graph** — a radial map of `This Mac → domains → tools → items`,
  pan/zoom, expand-on-click, with a Tree layout toggle.
- 📋 **List view + search** over the same data, grouped and scannable.
- 🤖 **AI-agent aware** — inventories Claude Code (skills, plugins, commands, MCP servers),
  plus Cursor, Windsurf, OpenAI Codex, Gemini CLI, Antigravity, Continue, Copilot, aider,
  and more — with cross-agent MCP servers from Claude, Codex, and Gemini.
- 🔎 **Rich per-item context** — description, homepage, install date, on-disk size (incl.
  per-formula Homebrew sizes), and an **"up to date / update available"** check, fetched on demand.
- 🔧 **Manage in place** — per-item **Update** and **Uninstall** buttons for Homebrew, npm, pip,
  pipx, gem, cargo, Ollama, and Docker — each with an explicit confirm.
- 🏷️ **Tags & saved views** — tag your favorite tools/skills, then flip a **view** to see only
  those items in both the graph and the list.
- 🔬 **Filter & sort** — quick filters (**update available / deprecated / noted**), a category
  multi-select, and sort by name or size — all composable with search.
- 📝 **Notes** — jot *why* something is installed; notes and tags persist across re-scans.
- 🧰 **Utilities** — one-click, allowlisted **updates** (`brew`, `npm`, `rustup`, `pipx`,
  `cargo`, `gh`, or "update everything") and **cleanups** (brew/docker/cache), each with a
  **dry-run preview and confirm** — nothing destructive runs blind.
- 📄 **Export `AGENT_MAP.md`** — a human-readable ledger of your whole environment, including a
  **Tagged Views** section.
- 🌗 **Light & dark** blueprint-blue theme.
- 🔒 **100% local** — no network calls, no telemetry. Config scans detect the *presence* of API
  keys, never their values.

## 🔍 What it scans

| Category | Detected |
|---|---|
| **Package managers** | Homebrew formulae/casks, npm/pnpm global, pip/pipx, cargo, gem |
| **Runtimes** | Node, Python, Ruby, Go, Rust, Java, Deno, Bun + version managers (pyenv, nvm, asdf, mise…) |
| **Projects** | Git repos in your workspaces, with tech-stack + launch-command detection, remote & last-commit |
| **AI & Agents** | Claude Code skills/plugins/commands/MCP, other AI IDEs & CLIs, Ollama models, Hugging Face cache, Python AI libs |
| **Containers** | Docker images & containers |

## 🚀 Getting started

**Prerequisites:** [Rust](https://www.rust-lang.org/tools/install), [Node.js](https://nodejs.org),
and (macOS) Xcode Command Line Tools.

```bash
git clone https://github.com/elm1nst3r/IOInventory.git
cd IOInventory
npm install
npm run tauri dev      # launch the app
```

Build a distributable:

```bash
npm run tauri build    # → .app / .dmg (macOS) or .msi / .exe (Windows)
```

## 🏗️ How it works

A **Rust core** runs every collector concurrently with per-command timeouts (so a hung tool
can't stall a scan) and PATH-augmentation (so a bundled app still finds `brew`, `cargo`, etc.).
Results land in **SQLite**; a **React + React Flow** UI renders the graph. A full scan of a
typical machine completes in **~2 seconds**.

```
src-tauri/src/scan/   → one module per collector + concurrent orchestrator
src-tauri/src/db.rs   → SQLite schema, migrations, note persistence
src-tauri/src/graph.rs→ builds the node/edge graph from an inventory
src-tauri/src/*.rs    → cleanup (allowlisted), export, Tauri commands
src/                  → React UI: GraphView, ListView, DetailPanel, CleanupPanel
```

Verify the scan engine against your own machine:

```bash
cd src-tauri && cargo test scan_smoke -- --nocapture
```

## 🗺️ Roadmap

- [ ] **Import** — load a previously exported inventory/ledger, and save/restore **static views**
      (named, frozen versions of the lists) so you can snapshot and diff your environment over time
- [ ] **Fix the logo & app icon** — polish the blueprint icon and its packaging across sizes/platforms
- [ ] Install new packages (not just update/uninstall) from the app
- [ ] Agent-driven self-updates — a CLI + Claude Code hook that appends to the ledger automatically
- [ ] Background / scheduled scans
- [ ] Windows package managers (winget / scoop / choco) + code signing

## 🔐 Privacy

Everything runs locally. No data leaves your machine, there is no telemetry, and secret values
(API keys, tokens) are never read or stored — only the *presence* of a configured provider is noted.

## 📄 License

[MIT](LICENSE) © [@elm1nst3r](https://github.com/elm1nst3r)

---

<div align="center">
<sub>Built with 🦀 Rust + ⚡ Tauri. If IO Inventory helps you tame your machine, consider leaving a ⭐.</sub>
</div>
