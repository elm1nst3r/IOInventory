#!/usr/bin/env node
// Build the `ioinv-mcp` MCP server and stage it where Tauri expects a sidecar,
// so it ends up inside IO Inventory.app/Contents/MacOS/ and reaches users
// through the auto-updater like any other part of the app.
//
// Tauri's externalBin convention wants the target triple in the filename
// (ioinv-mcp-aarch64-apple-darwin); the bundler strips it again on the way in.
//
// Usage: node scripts/build-mcp.mjs [--release]
//
// Runs from beforeBuildCommand, i.e. before Tauri's own cargo build, so no
// cargo invocation is nested inside another and both share the same target dir.
//
// Note that `externalBin` lives in tauri.mcp.conf.json, not the base config:
// tauri-build validates the sidecar's existence on *every* cargo invocation, so
// declaring it in the base config would break `cargo check`/`cargo test` and
// make this very binary impossible to build. Bundling applies the overlay with
// `tauri build --config src-tauri/tauri.mcp.conf.json` (see scripts/release.sh).

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const release = process.argv.includes("--release");
const profile = release ? "release" : "debug";
const exe = process.platform === "win32" ? ".exe" : "";

const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((l) => l.startsWith("host:"))
  ?.slice("host:".length)
  .trim();

if (!triple) {
  console.error("build-mcp: could not determine the host target triple from `rustc -vV`");
  process.exit(1);
}

// -p selects the workspace member; it shares src-tauri/target with the app
// build, so the common dependencies are compiled once.
const args = ["build", "--manifest-path", join(root, "src-tauri", "Cargo.toml"), "-p", "ioinv-mcp"];
if (release) args.push("--release");

console.log(`build-mcp: cargo ${args.join(" ")}`);
// `tauri build --config …` exports the merged config as TAURI_CONFIG, which this
// nested cargo would inherit — and then tauri-build would demand the very
// sidecar we're about to produce. Drop it so this build sees the base config.
const env = { ...process.env };
delete env.TAURI_CONFIG;
execFileSync("cargo", args, { stdio: "inherit", env });

const built = join(root, "src-tauri", "target", profile, `ioinv-mcp${exe}`);
const outDir = join(root, "src-tauri", "binaries");
const staged = join(outDir, `ioinv-mcp-${triple}${exe}`);

mkdirSync(outDir, { recursive: true });
copyFileSync(built, staged);
console.log(`build-mcp: staged ${staged}`);
