#!/usr/bin/env bash
# Cut a signed macOS (Apple Silicon) release with auto-updater artifacts.
#
# Usage:  scripts/release.sh <version> <notes-file>
#   e.g.  scripts/release.sh 0.9.2 notes.md
#
# Prereqs:
#   - .secrets/updater.key  (the updater private key; keep it safe, never commit)
#   - versions already bumped in package.json, src-tauri/Cargo.toml, tauri.conf.json
#   - gh authenticated; git tag will be created/pushed
#
# What it does:
#   1. Builds the app bundle (skips the DMG target — Tauri's dmg step hangs on
#      Finder AppleScript in a non-interactive shell) with signing env set, so
#      Tauri emits `<app>.app.tar.gz` + `.sig` (the updater artifacts).
#   2. Packages a plain DMG from the .app via hdiutil (for manual download).
#   3. Generates latest.json (the updater manifest).
#   4. Creates the GitHub release and uploads: DMG, .app.tar.gz, latest.json.
set -euo pipefail

VERSION="${1:?usage: release.sh <version> <notes-file>}"
NOTES="${2:?usage: release.sh <version> <notes-file>}"
REPO="elm1nst3r/IOInventory"
KEY=".secrets/updater.key"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

[ -f "$KEY" ] || { echo "missing $KEY"; exit 1; }
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY")"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

echo "==> Building signed app bundle v$VERSION"
# --config layers in the ioinv-mcp sidecar (staged by beforeBuildCommand) so the
# MCP server ships inside Contents/MacOS and reaches users via auto-update.
npm run tauri build -- --bundles app --config src-tauri/tauri.mcp.conf.json

BUNDLE="src-tauri/target/release/bundle/macos"
APP="$BUNDLE/IO Inventory.app"
TARGZ="$BUNDLE/IO Inventory.app.tar.gz"
SIG="$BUNDLE/IO Inventory.app.tar.gz.sig"
[ -d "$APP" ] && [ -f "$TARGZ" ] && [ -f "$SIG" ] || { echo "missing build artifacts"; exit 1; }
[ -x "$APP/Contents/MacOS/ioinv-mcp" ] || { echo "MCP sidecar missing from the bundle"; exit 1; }
# Guard against Tauri bundling the MCP server as the app (it does that if the
# server ever becomes a second [[bin]] of the app crate) — the build still
# "succeeds" and the resulting .app launches nothing. Matched on a string only
# the MCP entry point contains; don't exec the app binary here, it's a GUI and
# would hang the release.
[ -x "$APP/Contents/MacOS/agent-ledger" ] || { echo "main app binary missing from the bundle"; exit 1; }
grep -aq 'ioinv-mcp \[OPTIONS\]' "$APP/Contents/MacOS/agent-ledger" \
  && { echo "BUG: the bundled app binary is the MCP server, not the app"; exit 1; }
grep -aq 'ioinv-mcp \[OPTIONS\]' "$APP/Contents/MacOS/ioinv-mcp" \
  || { echo "BUG: the bundled sidecar is not the MCP server"; exit 1; }

OUT="dist-release"; mkdir -p "$OUT"
DMG="$OUT/IO_Inventory_${VERSION}_aarch64.dmg"
UPD="$OUT/IO_Inventory_${VERSION}_aarch64.app.tar.gz"   # no spaces (GitHub-safe URL)
cp "$TARGZ" "$UPD"

echo "==> Packaging DMG via hdiutil"
STAGE="$(mktemp -d)"; cp -R "$APP" "$STAGE/"; ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "IO Inventory" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"

echo "==> Generating latest.json"
LATEST="$OUT/latest.json"
URL="https://github.com/$REPO/releases/download/v${VERSION}/$(basename "$UPD")"
cat > "$LATEST" <<JSON
{
  "version": "$VERSION",
  "notes": "See the release notes on GitHub.",
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {
    "darwin-aarch64": {
      "signature": "$(cat "$SIG")",
      "url": "$URL"
    }
  }
}
JSON

echo "==> Creating GitHub release v$VERSION"
git tag -a "v$VERSION" -m "IO Inventory v$VERSION" 2>/dev/null || true
git push origin "v$VERSION" 2>/dev/null || true
gh release create "v$VERSION" \
  "$DMG#IO Inventory $VERSION — macOS (Apple Silicon) .dmg" \
  "$UPD" "$LATEST" \
  --repo "$REPO" --title "IO Inventory v$VERSION" --notes-file "$NOTES" --latest

echo "==> Done: https://github.com/$REPO/releases/tag/v$VERSION"
