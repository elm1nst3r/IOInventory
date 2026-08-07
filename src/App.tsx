import { useEffect, useRef, useState } from "react";
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
  Settings as SettingsIcon,
  DownloadCloud,
} from "lucide-react";
import { useStore } from "./store";
import { api } from "./lib/api";
import GraphView from "./graph/GraphView";
import ListView from "./views/ListView";
import DetailPanel from "./panels/DetailPanel";
import CleanupPanel from "./panels/CleanupPanel";
import FilterBar from "./panels/FilterBar";
import SnapshotsPanel from "./panels/SnapshotsPanel";
import SettingsPanel from "./panels/SettingsPanel";
import "./App.css";

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
  } = useStore();
  const [msg, setMsg] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const exportRef = useRef<HTMLDivElement>(null);

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

  function flash(text: string) {
    setMsg(text);
    setTimeout(() => setMsg(null), 4500);
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

        <nav className="tabs">
          <button className={tab === "graph" ? "active" : ""} onClick={() => setTab("graph")}>
            <Network size={15} /> Architecture
          </button>
          <button className={tab === "list" ? "active" : ""} onClick={() => setTab("list")}>
            <List size={15} /> List
          </button>
          <button className={tab === "cleanup" ? "active" : ""} onClick={() => setTab("cleanup")}>
            <Wrench size={15} /> Utilities
          </button>
          <button className={tab === "history" ? "active" : ""} onClick={() => setTab("history")}>
            <History size={15} /> History
          </button>
        </nav>

        {tab === "list" && (
          <div className="searchbox">
            <Search size={14} />
            <input
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
          >
            {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
          </button>

          <div className="ms" ref={exportRef}>
            <button
              className="btn btn-ghost"
              onClick={() => setExportOpen((o) => !o)}
              disabled={!inventory}
            >
              <FileDown size={15} /> Export <ChevronDown size={13} />
            </button>
            {exportOpen && (
              <div className="ms-panel export-menu">
                <button className="ms-row" onClick={exportLedger}>
                  Ledger (AGENT_MAP.md)
                </button>
                <button className="ms-row" onClick={exportSnapshotFile}>
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
      {error && <div className="banner error">{error}</div>}
      {msg && <div className="banner info">{msg}</div>}
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
      </main>
    </div>
  );
}
