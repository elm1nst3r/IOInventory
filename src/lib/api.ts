import { invoke } from "@tauri-apps/api/core";
import type {
  CleanupAction,
  CleanupPreview,
  CleanupResult,
  Enrichment,
  Graph,
  Inventory,
  Item,
} from "./types";

export const api = {
  scan: () => invoke<Inventory>("scan"),
  getInventory: () => invoke<Inventory | null>("get_inventory"),
  getGraph: () => invoke<Graph | null>("get_graph"),
  setNote: (item_key: string, note: string, why: string) =>
    invoke<void>("set_note", { itemKey: item_key, note, why }),
  enrichItem: (item: Item) =>
    invoke<Enrichment>("enrich_item", {
      collector: item.collector,
      name: item.name,
      sourcePath: item.source_path ?? null,
    }),
  listCleanups: () => invoke<CleanupAction[]>("list_cleanups"),
  previewCleanup: (id: string) => invoke<CleanupPreview>("preview_cleanup", { id }),
  runCleanup: (id: string) => invoke<CleanupResult>("run_cleanup", { id }),
  getRoots: () => invoke<string[]>("get_roots"),
  setRoots: (roots: string[]) => invoke<void>("set_roots", { roots }),
  exportAgentMap: () => invoke<{ path: string; content: string }>("export_agent_map"),
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
