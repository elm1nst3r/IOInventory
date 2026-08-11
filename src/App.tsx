import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import {
  RefreshCw,
  Network,
  List,
  Wrench,
  History,
  FileDown,
  Search,
  Sun,
  Moon,
  ChevronDown,
  Eye,
  X,
  AlertTriangle,
  Settings as SettingsIcon,
  DownloadCloud,
} from "lucide-react";
import { useStore } from "./store";
import { api } from "./lib/api";
import GraphView from "./graph/GraphView";
import ListView from "./views/ListView";
import DetailPanel from "./panels/DetailPanel";
import FilterBar from "./panels/FilterBar";
import "./App.css";

const CleanupPanel = lazy(() => import("./panels/CleanupPanel"));
const SnapshotsPanel = lazy(() => import("./panels/SnapshotsPanel"));
const SettingsPanel = lazy(() => import("./panels/SettingsPanel"));

export default function App() {
  const {
    accentId,
    inventory,
    scanning,
    loading,
    error,
    tab,
    search,
    theme,
    viewingSnapshot,
    init,
    scan,
    setTab,
    setSearch,
    toggleTheme,
    exitSnapshot,
    updateAvailable,
    updateStatus,
    updateProgress,
    installUpdate,
    dismissUpdate,
    scanSources,
    toggleSource,
    settingsSaving,
  } = useStore();
  const [msg, setMsg] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const exportRef = useRef<HTMLDivElement>(null);
  const msgTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [warningsOpen, setWarningsOpen] = useState(false);

  useEffect(() => {
    init();
  }, [init]);

  useEffect(() => {
    if (!exportOpen) return;
    const onDown = (e: MouseEvent) => {
      if (exportRef.current && !exportRef.current.contains(e.target as Node)) setExportOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [exportOpen]);

  // Clear any in-flight timer first: without this the previous message's
  // timeout fires against the new one and hides it early.
  useEffect(() => () => {
    if (msgTimer.current) clearTimeout(msgTimer.current);
  }, []);

  function flash(text: string) {
    if (msgTimer.current) clearTimeout(msgTimer.current);
    setMsg(text);
    msgTimer.current = setTimeout(() => setMsg(null), 4500);
  }

  async function exportLedger() {
    setExportOpen(false);
    try {
      const { path } = await api.exportAgentMap();
      flash(`Ledger saved to ${path}`);
    } catch (e) {
      flash(String(e));
    }
  }
  async function exportSnapshotFile() {
    setExportOpen(false);
    try {
      const { path } = await api.exportSnapshot(viewingSnapshot?.id ?? null);
      flash(`Snapshot exported to ${path}`);
    } catch (e) {
      flash(String(e));
    }
  }

  const scan_ = inventory?.scan;
  const warnings = scan_?.warnings;

  // One entry per source that reported something, so five brew failures read as
  // "Homebrew (5)" rather than five loose lines. A warning source is a
  // collector id, which is also a ScanSource id — except `version_checks`,
  // which is the outdated/deprecated pass and isn't separately toggleable.
  const warningGroups = useMemo(() => {
    const by = new Map<string, string[]>();
    for (const w of warnings ?? []) {
      if (!by.has(w.source)) by.set(w.source, []);
      by.get(w.source)!.push(w.message);
    }
    return [...by.entries()].map(([source, messages]) => {
      const known = scanSources.find((s) => s.id === source);
      return { source, messages, label: known?.label ?? source, canDisable: !!known };
    });
  }, [warnings, scanSources]);

  // Collapse again when a new scan arrives, so the panel isn't left hanging
  // open over a different set of warnings.
  useEffect(() => setWarningsOpen(false), [scan_?.id]);

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <img
            src={accentId === "matrix" && theme === "dark" ? "/logo-matrix.png" : "/logo.png"}
            className="brand-logo"
            alt="IO Inventory"
          />
          <span>IO Inventory</span>
        </div>

        <nav className="tabs" aria-label="Primary navigation">
          <button aria-current={tab === "graph" ? "page" : undefined} className={tab === "graph" ? "active" : ""} onClick={() => setTab("graph")}>
            <Network size={15} /> Architecture
          </button>
          <button aria-current={tab === "list" ? "page" : undefined} className={tab === "list" ? "active" : ""} onClick={() => setTab("list")}>
            <List size={15} /> List
          </button>
          <button aria-current={tab === "cleanup" ? "page" : undefined} className={tab === "cleanup" ? "active" : ""} onClick={() => setTab("cleanup")}>
            <Wrench size={15} /> Utilities
          </button>
          <button aria-current={tab === "history" ? "page" : undefined} className={tab === "history" ? "active" : ""} onClick={() => setTab("history")}>
            <History size={15} /> History
          </button>
        </nav>

        {tab === "list" && (
          <div className="searchbox">
            <Search size={14} />
            <input
              aria-label="Search inventory"
              placeholder="Search packages, repos, skills…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
        )}

        <div className="topbar-right">
          {scan_ && !viewingSnapshot && (
            <span className="scan-info">
              {scan_.item_count} items · {(scan_.duration_ms / 1000).toFixed(1)}s
            </span>
          )}
          <button
            className="btn btn-ghost icon-btn"
            onClick={toggleTheme}
            title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
            aria-label={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
          >
            {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
          </button>

          <div className="ms" ref={exportRef}>
            <button
              className="btn btn-ghost"
              onClick={() => setExportOpen((o) => !o)}
              disabled={!inventory}
              aria-expanded={exportOpen}
              aria-haspopup="menu"
            >
              <FileDown size={15} /> Export <ChevronDown size={13} />
            </button>
            {exportOpen && (
              <div className="ms-panel export-menu" role="menu">
                <button className="ms-row" role="menuitem" onClick={exportLedger}>
                  Ledger (AGENT_MAP.md)
                </button>
                <button className="ms-row" role="menuitem" onClick={exportSnapshotFile}>
                  {viewingSnapshot ? "This snapshot" : "Snapshot"} (.ioinv.json)
                </button>
              </div>
            )}
          </div>

          <button
            className="btn btn-primary"
            onClick={scan}
            disabled={scanning || !!viewingSnapshot}
            title={viewingSnapshot ? "Exit the snapshot to scan" : "Scan your machine"}
          >
            <RefreshCw size={15} className={scanning ? "spin" : ""} />
            {scanning ? "Scanning…" : "Scan"}
          </button>

          <button
            className={`btn btn-ghost icon-btn ${tab === "settings" ? "active" : ""}`}
            onClick={() => setTab("settings")}
            title="Settings"
            aria-label="Settings"
          >
            <SettingsIcon size={16} />
          </button>
        </div>
      </header>

      {updateAvailable && (
        <div className="banner update">
          <DownloadCloud size={15} />
          <span>
            Version <strong>{updateAvailable.version}</strong> is available.
          </span>
          {updateStatus === "downloading" ? (
            <span className="update-progress">
              Downloading… {Math.round(updateProgress * 100)}%
            </span>
          ) : (
            <>
              <button className="banner-cta" onClick={installUpdate}>
                Download &amp; install
              </button>
              <button className="banner-exit" onClick={dismissUpdate}>
                Later
              </button>
            </>
          )}
        </div>
      )}
      {error && <div className="banner error" role="alert">{error}</div>}
      {msg && <div className="banner info" role="status">{msg}</div>}
      {!viewingSnapshot && warnings && warnings.length > 0 && (
        <div className="banner warning warning-block" role="status">
          <button
            className="warning-summary"
            onClick={() => setWarningsOpen((o) => !o)}
            aria-expanded={warningsOpen}
          >
            <AlertTriangle size={15} />
            <span>
              Scan completed with {warnings.length} warning{warnings.length === 1 ? "" : "s"} from{" "}
              {warningGroups.length} source{warningGroups.length === 1 ? "" : "s"}. Some sources may
              be incomplete.
            </span>
            <ChevronDown size={14} className={`warning-caret ${warningsOpen ? "open" : ""}`} />
          </button>

          {warningsOpen && (
            <div className="warning-detail">
              {warningGroups.map((g) => (
                <div key={g.source} className="warning-group">
                  <div className="warning-group-head">
                    <strong>{g.label}</strong>
                    <span className="warning-count">{g.messages.length}</span>
                    {g.canDisable && (
                      <button
                        className="link-btn"
                        disabled={settingsSaving}
                        title={`Stop scanning ${g.label}`}
                        onClick={() => toggleSource(g.source)}
                      >
                        Turn off this source
                      </button>
                    )}
                  </div>
                  {g.messages.map((m, i) => (
                    <pre key={i} className="warning-msg">
                      {m}
                    </pre>
                  ))}
                </div>
              ))}
              <p className="warning-foot">
                Warnings usually mean a tool wasn't found, timed out, or exited with an error —
                that source's items may be missing. Turning one off takes effect on your next scan.
              </p>
            </div>
          )}
        </div>
      )}
      {viewingSnapshot && (
        <div className="banner snapshot">
          <Eye size={14} /> Viewing snapshot <strong>{viewingSnapshot.name}</strong> ·{" "}
          {viewingSnapshot.created_at.slice(0, 10)} · read-only
          <button className="banner-exit" onClick={exitSnapshot}>
            <X size={13} /> Exit to live
          </button>
        </div>
      )}

      <main className="main">
        <Suspense fallback={<div className="empty-hint">Loading view…</div>}>
        {loading ? (
          <div className="empty-hint">Loading…</div>
        ) : tab === "settings" ? (
          <SettingsPanel />
        ) : tab === "history" ? (
          <SnapshotsPanel />
        ) : tab === "cleanup" ? (
          <CleanupPanel />
        ) : (
          <div className="workspace">
            <FilterBar />
            <div className="split">
              <div className="split-main">
                {tab === "graph" ? <GraphView /> : <ListView />}
              </div>
              <DetailPanel />
            </div>
          </div>
        )}
        </Suspense>
      </main>
    </div>
  );
}
