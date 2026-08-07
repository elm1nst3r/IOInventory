import { create } from "zustand";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "./lib/api";
import type { Enrichment, Graph, Inventory, Item, SnapshotMeta } from "./lib/types";

// The Update object carries methods and isn't serializable — keep it here,
// out of the reactive store, and expose only plain metadata to the UI.
let pendingUpdate: Update | null = null;

type Tab = "graph" | "list" | "cleanup" | "history";
type Theme = "dark" | "light";
type Layout = "radial" | "tree";

function initialTheme(): Theme {
  const saved = localStorage.getItem("al-theme");
  if (saved === "light" || saved === "dark") return saved;
  return "dark";
}
function applyTheme(t: Theme) {
  document.documentElement.dataset.theme = t;
}

interface State {
  inventory: Inventory | null;
  graph: Graph | null;
  scanning: boolean;
  loading: boolean;
  error: string | null;
  tab: Tab;
  search: string;
  selectedKey: string | null;
  theme: Theme;
  layout: Layout;
  filters: Set<string>;
  activeView: string | null;
  enrichCache: Record<string, Enrichment>;
  enriching: string | null;
  // Snapshots
  snapshots: SnapshotMeta[];
  viewingSnapshot: SnapshotMeta | null;
  liveInventory: Inventory | null;
  liveGraph: Graph | null;
  // Updater
  updateAvailable: { version: string; notes: string } | null;
  updateStatus: "idle" | "checking" | "downloading" | "error";
  updateProgress: number; // 0..1
  updateError: string | null;

  init: () => Promise<void>;
  scan: () => Promise<void>;
  setTab: (t: Tab) => void;
  setSearch: (s: string) => void;
  select: (key: string | null) => void;
  saveNote: (key: string, note: string, why: string) => Promise<void>;
  toggleTheme: () => void;
  setLayout: (l: Layout) => void;
  toggleFilter: (key: string) => void;
  clearFilters: () => void;
  setView: (tag: string | null) => void;
  setItemTags: (key: string, tags: string[]) => Promise<void>;
  enrich: (item: Item) => Promise<void>;
  refreshSnapshots: () => Promise<void>;
  viewSnapshot: (meta: SnapshotMeta) => Promise<void>;
  exitSnapshot: () => void;
  checkForUpdates: (silent: boolean) => Promise<void>;
  installUpdate: () => Promise<void>;
  dismissUpdate: () => void;
}

export const useStore = create<State>((set, get) => ({
  inventory: null,
  graph: null,
  scanning: false,
  loading: true,
  error: null,
  tab: "graph",
  search: "",
  selectedKey: null,
  theme: initialTheme(),
  layout: "radial",
  filters: new Set<string>(),
  activeView: null,
  enrichCache: {},
  enriching: null,
  snapshots: [],
  viewingSnapshot: null,
  liveInventory: null,
  liveGraph: null,
  updateAvailable: null,
  updateStatus: "idle",
  updateProgress: 0,
  updateError: null,

  init: async () => {
    applyTheme(get().theme);
    set({ loading: true });
    try {
      const [inventory, graph, snapshots] = await Promise.all([
        api.getInventory(),
        api.getGraph(),
        api.listSnapshots(),
      ]);
      set({
        inventory,
        graph,
        liveInventory: inventory,
        liveGraph: graph,
        snapshots,
        loading: false,
      });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
    // Silent update check on launch (shows a banner only if one is available).
    get().checkForUpdates(true);
  },

  scan: async () => {
    set({ scanning: true, error: null });
    try {
      const inventory = await api.scan();
      const graph = await api.getGraph();
      set({
        inventory,
        graph,
        liveInventory: inventory,
        liveGraph: graph,
        scanning: false,
        enrichCache: {},
        viewingSnapshot: null,
      });
    } catch (e) {
      set({ error: String(e), scanning: false });
    }
  },

  setTab: (tab) => set({ tab }),
  setSearch: (search) => set({ search }),
  select: (selectedKey) => set({ selectedKey }),

  toggleTheme: () => {
    const theme: Theme = get().theme === "dark" ? "light" : "dark";
    localStorage.setItem("al-theme", theme);
    applyTheme(theme);
    set({ theme });
  },

  setLayout: (layout) => set({ layout }),

  toggleFilter: (key) => {
    const next = new Set(get().filters);
    next.has(key) ? next.delete(key) : next.add(key);
    set({ filters: next });
  },
  clearFilters: () => set({ filters: new Set() }),

  setView: (activeView) => set({ activeView }),

  setItemTags: async (key, tags) => {
    await api.setItemTags(key, tags);
    const inv = get().inventory;
    if (inv) {
      const items = inv.items.map((it: Item) =>
        it.item_key === key ? { ...it, tags } : it,
      );
      set({ inventory: { ...inv, items } });
    }
    // If we removed the last item from the active view, drop the view.
    const view = get().activeView;
    if (view) {
      const stillHas = get().inventory?.items.some((i) => i.tags?.includes(view));
      if (!stillHas) set({ activeView: null });
    }
  },

  refreshSnapshots: async () => {
    try {
      set({ snapshots: await api.listSnapshots() });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  viewSnapshot: async (meta) => {
    set({ error: null });
    try {
      const [inventory, graph] = await Promise.all([
        api.getSnapshotInventory(meta.id),
        api.getSnapshotGraph(meta.id),
      ]);
      set({
        inventory,
        graph,
        viewingSnapshot: meta,
        tab: "graph",
        selectedKey: null,
        filters: new Set(),
        activeView: null,
        enrichCache: {},
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  exitSnapshot: () => {
    const { liveInventory, liveGraph } = get();
    set({
      inventory: liveInventory,
      graph: liveGraph,
      viewingSnapshot: null,
      selectedKey: null,
    });
  },

  checkForUpdates: async (silent) => {
    set({ updateStatus: "checking", updateError: null });
    try {
      const u = await check();
      if (u) {
        pendingUpdate = u;
        set({
          updateAvailable: { version: u.version, notes: u.body ?? "" },
          updateStatus: "idle",
        });
      } else {
        pendingUpdate = null;
        set({ updateAvailable: null, updateStatus: "idle" });
        if (!silent) set({ updateError: "You're on the latest version." });
      }
    } catch (e) {
      // Auto-checks fail quietly (e.g. no manifest yet / offline).
      set({ updateStatus: "idle" });
      if (!silent) set({ updateError: `Update check failed: ${e}` });
    }
  },

  installUpdate: async () => {
    if (!pendingUpdate) return;
    set({ updateStatus: "downloading", updateProgress: 0, updateError: null });
    try {
      let total = 0;
      let got = 0;
      await pendingUpdate.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          set({ updateProgress: total ? got / total : 0 });
        }
      });
      await relaunch();
    } catch (e) {
      set({ updateStatus: "error", updateError: `Update failed: ${e}` });
    }
  },

  dismissUpdate: () => set({ updateAvailable: null, updateError: null }),

  enrich: async (item) => {
    if (get().viewingSnapshot) return; // read-only while viewing a snapshot
    if (get().enrichCache[item.item_key]) return;
    set({ enriching: item.item_key });
    try {
      const e = await api.enrichItem(item);
      set((s) => ({
        enrichCache: { ...s.enrichCache, [item.item_key]: e },
        enriching: null,
      }));
    } catch {
      set({ enriching: null });
    }
  },

  saveNote: async (key, note, why) => {
    await api.setNote(key, note, why);
    const inv = get().inventory;
    if (inv) {
      const items = inv.items.map((it: Item) =>
        it.item_key === key ? { ...it, note, why } : it,
      );
      set({ inventory: { ...inv, items } });
    }
  },
}));
