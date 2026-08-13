import { invoke } from "@tauri-apps/api/core";
import type {
  ActionInfo,
  ActionResult,
  CleanupAction,
  CleanupPreview,
  CleanupResult,
  Diff,
  Enrichment,
  Graph,
  Inventory,
  Item,
  McpInfo,
  ScanSource,
  Settings,
  SnapshotMeta,
} from "./types";

/** The least an item must carry for the backend to work out its actions. */
export type ActionTarget = Pick<Item, "collector" | "name"> &
  Partial<Pick<Item, "source_path" | "metadata">>;

export const api = {
  scan: () => invoke<Inventory>("scan"),
  getInventory: () => invoke<Inventory | null>("get_inventory"),
  getGraph: () => invoke<Graph | null>("get_graph"),
  setNote: (item_key: string, note: string, why: string) =>
    invoke<void>("set_note", { itemKey: item_key, note, why }),
  setItemTags: (item_key: string, tags: string[]) =>
    invoke<void>("set_item_tags", { itemKey: item_key, tags }),
  enrichItem: (item: Item) =>
    invoke<Enrichment>("enrich_item", {
      collector: item.collector,
      name: item.name,
      sourcePath: item.source_path ?? null,
    }),
  // Applications carry their own context: what can be done to one depends on
  // where the bundle lives and whether a Homebrew cask owns it. Typed as the
  // subset both a full Item and a diff row satisfy, since the snapshot
  // installer only ever has the latter.
  itemActions: (item: ActionTarget) =>
    invoke<ActionInfo>("item_actions", {
      collector: item.collector,
      name: item.name,
      sourcePath: item.source_path ?? null,
      cask: (item.metadata?.cask as string | undefined) ?? null,
    }),
  runItemAction: (item: ActionTarget, action: "update" | "delete" | "install") =>
    invoke<ActionResult>("run_item_action", {
      collector: item.collector,
      name: item.name,
      action,
      sourcePath: item.source_path ?? null,
      cask: (item.metadata?.cask as string | undefined) ?? null,
    }),
  listCleanups: () => invoke<CleanupAction[]>("list_cleanups"),
  previewCleanup: (id: string) => invoke<CleanupPreview>("preview_cleanup", { id }),
  runCleanup: (id: string) => invoke<CleanupResult>("run_cleanup", { id }),
  getRoots: () => invoke<string[]>("get_roots"),
  setRoots: (roots: string[]) => invoke<void>("set_roots", { roots }),
  exportAgentMap: () => invoke<{ path: string; content: string }>("export_agent_map"),

  // Settings
  listScanSources: () => invoke<ScanSource[]>("list_scan_sources"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  mcpInfo: () => invoke<McpInfo>("mcp_info"),

  // Snapshots
  saveSnapshot: (name: string) => invoke<SnapshotMeta>("save_snapshot", { name }),
  listSnapshots: () => invoke<SnapshotMeta[]>("list_snapshots"),
  getSnapshotInventory: (id: number) => invoke<Inventory>("get_snapshot_inventory", { id }),
  getSnapshotGraph: (id: number) => invoke<Graph>("get_snapshot_graph", { id }),
  deleteSnapshot: (id: number) => invoke<void>("delete_snapshot", { id }),
  /** `targetId` null compares the snapshot against the current scan. */
  diffSnapshot: (id: number, targetId: number | null = null) =>
    invoke<Diff>("diff_snapshot", { id, targetId }),
  exportSnapshot: (id: number | null) =>
    invoke<{ path: string }>("export_snapshot", { id }),
  importSnapshot: (content: string, name: string | null) =>
    invoke<SnapshotMeta>("import_snapshot", { content, name }),
};

export function formatBytes(bytes?: number | null): string {
  if (bytes == null) return "";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}
