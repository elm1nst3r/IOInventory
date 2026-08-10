import { create } from "zustand";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "./lib/api";
import {
  accentById,
  applyAppearance,
  initialAccentId,
  initialReduceMotion,
  initialThemeMode,
  resolveTheme,
  saveAccentId,
  saveReduceMotion,
  saveThemeMode,
  watchSystemTheme,
  type Theme,
  type ThemeMode,
} from "./lib/appearance";
import type {
  Enrichment,
  Graph,
  Inventory,
  Item,
  ScanSource,
  Settings,
  SnapshotMeta,
} from "./lib/types";

// The Update object carries methods and isn't serializable — keep it here,
// out of the reactive store, and expose only plain metadata to the UI.
let pendingUpdate: Update | null = null;

// Read once at module load so the very first render already has the right
// theme; `init` then applies it to the document.
const initialMode = initialThemeMode();
const initialAccent = initialAccentId();
const initialMotion = initialReduceMotion();
let systemThemeWatched = false;

type Tab = "graph" | "list" | "cleanup" | "history" | "settings";
type Layout = "radial" | "tree";

interface State {
  inventory: Inventory | null;
  graph: Graph | null;
  scanning: boolean;
  loading: boolean;
  error: string | null;
  tab: Tab;
  search: string;
  selectedKey: string | null;
  // Appearance
  themeMode: ThemeMode;
  /** The theme actually in effect — `themeMode` resolved against the OS. */
  theme: Theme;
  accentId: string;
  reduceMotion: boolean;
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
  // Settings
  settings: Settings;
  scanSources: ScanSource[];
  settingsSaving: boolean;
  appVersion: string;

  init: () => Promise<void>;
  scan: () => Promise<void>;
  setTab: (t: Tab) => void;
  setSearch: (s: string) => void;
  select: (key: string | null) => void;
  saveNote: (key: string, note: string, why: string) => Promise<void>;
  toggleTheme: () => void;
  setThemeMode: (m: ThemeMode) => void;
  setAccent: (id: string) => void;
  setReduceMotion: (on: boolean) => void;
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
  toggleSource: (id: string) => Promise<void>;
  setAllSources: (enabled: boolean) => Promise<void>;
  setRoots: (roots: string[]) => Promise<void>;
  setMcpAllowWrite: (allow: boolean) => Promise<void>;
  /** Shared write-through used by the three setters above. */
  persistSettings: (next: Settings) => Promise<void>;
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
  themeMode: initialMode,
  theme: resolveTheme(initialMode),
  accentId: initialAccent,
  reduceMotion: initialMotion,
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
  settings: { disabled_sources: [], roots: [], mcp_allow_write: false },
  scanSources: [],
  settingsSaving: false,
  appVersion: "",

  init: async () => {
    const { themeMode, accentId, reduceMotion } = get();
    applyAppearance(resolveTheme(themeMode), accentById(accentId), reduceMotion);
    // Follow the OS while in "system" mode, including live changes. Registered
    // once for the app's lifetime — StrictMode runs this effect twice in dev,
    // and the listener is never torn down.
    if (!systemThemeWatched) {
      systemThemeWatched = true;
      watchSystemTheme((sysTheme) => {
        if (get().themeMode !== "system") return;
        set({ theme: sysTheme });
        applyAppearance(sysTheme, accentById(get().accentId), get().reduceMotion);
      });
    }
    set({ loading: true });
    getVersion()
      .then((appVersion) => set({ appVersion }))
      .catch(() => {});
    try {
      const [inventory, graph, snapshots, settings, scanSources] = await Promise.all([
        api.getInventory(),
        api.getGraph(),
        api.listSnapshots(),
        api.getSettings(),
        api.listScanSources(),
      ]);
      set({
        inventory,
        graph,
        liveInventory: inventory,
        liveGraph: graph,
        snapshots,
        settings,
        scanSources,
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

  // The top-bar button flips to the opposite of what's showing, which also
  // means leaving "system" mode — an explicit click is an explicit choice.
  toggleTheme: () => {
    get().setThemeMode(get().theme === "dark" ? "light" : "dark");
  },

  setThemeMode: (themeMode) => {
    const theme = resolveTheme(themeMode);
    saveThemeMode(themeMode);
    applyAppearance(theme, accentById(get().accentId), get().reduceMotion);
    set({ themeMode, theme });
  },

  setAccent: (accentId) => {
    const accent = accentById(accentId);
    saveAccentId(accentId);
    // A preset that repaints every surface only works on its intended ground,
    // so switch the theme with it rather than silently ignoring the mode.
    if (accent.forcesDark && get().theme !== "dark") {
      saveThemeMode("dark");
      set({ themeMode: "dark", theme: "dark" });
    }
    applyAppearance(get().theme, accent, get().reduceMotion);
    set({ accentId });
  },

  setReduceMotion: (reduceMotion) => {
    saveReduceMotion(reduceMotion);
    applyAppearance(get().theme, accentById(get().accentId), reduceMotion);
    set({ reduceMotion });
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
      const inventory = { ...inv, items };
      set({ inventory, liveInventory: inventory });
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

  // Settings are written through to SQLite immediately; the backend returns the
  // stored value (unknown source ids dropped) and that's what we keep.
  toggleSource: async (id) => {
    const { settings } = get();
    const disabled = settings.disabled_sources.includes(id)
      ? settings.disabled_sources.filter((d) => d !== id)
      : [...settings.disabled_sources, id];
    await get().persistSettings({ ...settings, disabled_sources: disabled });
  },

  setAllSources: async (enabled) => {
    const { settings, scanSources } = get();
    await get().persistSettings({
      ...settings,
      disabled_sources: enabled ? [] : scanSources.map((s) => s.id),
    });
  },

  setRoots: async (roots) => {
    await get().persistSettings({ ...get().settings, roots });
  },

  setMcpAllowWrite: async (mcp_allow_write) => {
    await get().persistSettings({ ...get().settings, mcp_allow_write });
  },

  persistSettings: async (next: Settings) => {
    set({ settingsSaving: true, error: null });
    try {
      set({ settings: await api.setSettings(next), settingsSaving: false });
    } catch (e) {
      set({ error: String(e), settingsSaving: false });
    }
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
      const inventory = { ...inv, items };
      set({ inventory, liveInventory: inventory });
    }
  },
}));
