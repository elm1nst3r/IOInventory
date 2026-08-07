import { create } from "zustand";
import { api } from "./lib/api";
import type { Enrichment, Graph, Inventory, Item, SnapshotMeta } from "./lib/types";

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
