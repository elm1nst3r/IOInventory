import { useEffect, useState } from "react";
import { RefreshCw, Network, List, Wrench, FileDown, Search, Sun, Moon } from "lucide-react";
import { useStore } from "./store";
import { api } from "./lib/api";
import GraphView from "./graph/GraphView";
import ListView from "./views/ListView";
import DetailPanel from "./panels/DetailPanel";
import CleanupPanel from "./panels/CleanupPanel";
import FilterBar from "./panels/FilterBar";
import "./App.css";

export default function App() {
  const {
    inventory,
    scanning,
    loading,
    error,
    tab,
    search,
    theme,
    init,
    scan,
    setTab,
    setSearch,
    toggleTheme,
  } = useStore();
  const [exportMsg, setExportMsg] = useState<string | null>(null);

  useEffect(() => {
    init();
  }, [init]);

  async function doExport() {
    try {
      const { path } = await api.exportAgentMap();
      setExportMsg(`Saved to ${path}`);
      setTimeout(() => setExportMsg(null), 4000);
    } catch (e) {
      setExportMsg(String(e));
    }
  }

  const scan_ = inventory?.scan;

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <img src="/logo.png" className="brand-logo" alt="IO Inventory" />
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
          {scan_ && (
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
          <button
            className="btn btn-ghost"
            onClick={doExport}
            disabled={!inventory}
            title="Export AGENT_MAP.md"
          >
            <FileDown size={15} /> Export
          </button>
          <button className="btn btn-primary" onClick={scan} disabled={scanning}>
            <RefreshCw size={15} className={scanning ? "spin" : ""} />
            {scanning ? "Scanning…" : "Scan"}
          </button>
        </div>
      </header>

      {error && <div className="banner error">{error}</div>}
      {exportMsg && <div className="banner info">{exportMsg}</div>}

      <main className="main">
        {loading ? (
          <div className="empty-hint">Loading…</div>
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
