import { useEffect, useMemo, useRef, useState } from "react";
import {
  Camera,
  Upload,
  Eye,
  GitCompare,
  Download,
  Trash2,
  PackagePlus,
  ArrowRight,
} from "lucide-react";
import { useStore } from "../store";
import { api } from "../lib/api";
import { collectorLabel } from "../lib/labels";
import type { Diff, DiffItem, SnapshotMeta } from "../lib/types";

// Collectors whose items can be installed via a package manager.
const INSTALLABLE = new Set([
  "homebrew",
  "homebrew-cask",
  "npm",
  "pnpm",
  "pip",
  "python-ai-lib",
  "pipx",
  "gem",
  "cargo",
  "ollama",
  "docker-image",
]);

const diKey = (i: DiffItem) => `${i.collector}:${i.name}`;

export default function SnapshotsPanel() {
  const snapshots = useStore((s) => s.snapshots);
  const refreshSnapshots = useStore((s) => s.refreshSnapshots);
  const viewSnapshot = useStore((s) => s.viewSnapshot);
  const scan = useStore((s) => s.scan);
  const inventory = useStore((s) => s.inventory);

  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const [diff, setDiff] = useState<Diff | null>(null);
  const [diffFor, setDiffFor] = useState<SnapshotMeta | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number; current: string }>({
    done: 0,
    total: 0,
    current: "",
  });
  const [results, setResults] = useState<{ name: string; ok: boolean }[]>([]);

  useEffect(() => {
    refreshSnapshots();
  }, [refreshSnapshots]);

  function flash(t: string) {
    setMsg(t);
    setTimeout(() => setMsg(null), 4000);
  }

  async function saveNow() {
    setBusy(true);
    try {
      await api.saveSnapshot(name.trim());
      setName("");
      await refreshSnapshots();
      flash("Snapshot saved.");
    } catch (e) {
      flash(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    setBusy(true);
    try {
      const text = await file.text();
      const nm = file.name.replace(/\.(ioinv\.)?json$/i, "");
      await api.importSnapshot(text, nm);
      await refreshSnapshots();
      flash(`Imported “${nm}”.`);
    } catch (e) {
      flash(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function compare(meta: SnapshotMeta) {
    setDiff(null);
    setResults([]);
    setSelected(new Set());
    setDiffFor(meta);
    try {
      setDiff(await api.diffSnapshot(meta.id));
    } catch (e) {
      flash(String(e));
      setDiffFor(null);
    }
  }

  async function exportSnap(meta: SnapshotMeta) {
    try {
      const { path } = await api.exportSnapshot(meta.id);
      flash(`Exported to ${path}`);
    } catch (e) {
      flash(String(e));
    }
  }

  async function del(meta: SnapshotMeta) {
    if (!confirm(`Delete snapshot “${meta.name}”?`)) return;
    try {
      await api.deleteSnapshot(meta.id);
      if (diffFor?.id === meta.id) {
        setDiff(null);
        setDiffFor(null);
      }
      await refreshSnapshots();
    } catch (e) {
      flash(String(e));
    }
  }

  // The installable subset of "missing here" (present in snapshot, not current).
  const installableMissing = useMemo(
    () => (diff ? diff.removed.filter((i) => INSTALLABLE.has(i.collector)) : []),
    [diff],
  );

  function toggle(key: string) {
    setSelected((prev) => {
      const n = new Set(prev);
      n.has(key) ? n.delete(key) : n.add(key);
      return n;
    });
  }
  function toggleAll() {
    setSelected((prev) =>
      prev.size === installableMissing.length
        ? new Set()
        : new Set(installableMissing.map(diKey)),
    );
  }

  async function installSelected() {
    const chosen = installableMissing.filter((i) => selected.has(diKey(i)));
    if (chosen.length === 0) return;
    if (!confirm(`Install ${chosen.length} item(s)? This runs each package manager's install command.`))
      return;
    setInstalling(true);
    setResults([]);
    setProgress({ done: 0, total: chosen.length, current: "" });
    const done: { name: string; ok: boolean }[] = [];
    for (const it of chosen) {
      setProgress({ done: done.length, total: chosen.length, current: it.name });
      try {
        const r = await api.runItemAction(it.collector, it.name, "install");
        done.push({ name: it.name, ok: r.success });
      } catch {
        done.push({ name: it.name, ok: false });
      }
      setResults([...done]);
    }
    setInstalling(false);
    // Refresh live inventory and the diff so installed items drop off the list.
    await scan();
    if (diffFor) {
      try {
        setDiff(await api.diffSnapshot(diffFor.id));
      } catch {
        /* ignore */
      }
    }
    setSelected(new Set());
  }

  return (
    <div className="snapshots">
      <div className="snap-head">
        <div>
          <h2>History &amp; snapshots</h2>
          <p>
            Save the current environment as a snapshot, import one from a file, then view it or
            compare it to your current scan — and bulk-install what's missing.
          </p>
        </div>
      </div>

      <div className="snap-actions">
        <input
          className="snap-name"
          placeholder="Snapshot name (optional)"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button className="btn btn-primary" onClick={saveNow} disabled={busy || !inventory}>
          <Camera size={15} /> Save current
        </button>
        <button className="btn" onClick={() => fileRef.current?.click()} disabled={busy}>
          <Upload size={15} /> Import…
        </button>
        <input
          ref={fileRef}
          type="file"
          accept=".json,.ioinv.json,application/json"
          style={{ display: "none" }}
          onChange={onFile}
        />
        {msg && <span className="snap-msg">{msg}</span>}
      </div>

      <div className="snap-grid">
        <div className="snap-list">
          {snapshots.length === 0 && (
            <div className="empty-hint" style={{ height: "auto", padding: "30px 0" }}>
              No snapshots yet — save your current setup or import a file.
            </div>
          )}
          {snapshots.map((s) => (
            <div key={s.id} className={`snap-card ${diffFor?.id === s.id ? "active" : ""}`}>
              <div className="snap-card-main">
                <div className="snap-card-title">
                  {s.name}
                  <span className={`snap-src ${s.source}`}>{s.source}</span>
                </div>
                <div className="snap-card-meta">
                  {s.created_at.slice(0, 10)} · {s.item_count} items · {s.host}
                </div>
              </div>
              <div className="snap-card-actions">
                <button title="View (read-only)" onClick={() => viewSnapshot(s)}>
                  <Eye size={15} />
                </button>
                <button title="Compare to current" onClick={() => compare(s)}>
                  <GitCompare size={15} />
                </button>
                <button title="Export to file" onClick={() => exportSnap(s)}>
                  <Download size={15} />
                </button>
                <button title="Delete" className="danger" onClick={() => del(s)}>
                  <Trash2 size={15} />
                </button>
              </div>
            </div>
          ))}
        </div>

        <div className="snap-diff">
          {!diff ? (
            <div className="empty-hint" style={{ height: "100%" }}>
              Pick <GitCompare size={14} style={{ margin: "0 4px", verticalAlign: "-2px" }} /> on a
              snapshot to compare it with your current scan.
            </div>
          ) : (
            <DiffView
              diff={diff}
              installableMissing={installableMissing}
              selected={selected}
              onToggle={toggle}
              onToggleAll={toggleAll}
              onInstall={installSelected}
              installing={installing}
              progress={progress}
              results={results}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function DiffView({
  diff,
  installableMissing,
  selected,
  onToggle,
  onToggleAll,
  onInstall,
  installing,
  progress,
  results,
}: {
  diff: Diff;
  installableMissing: DiffItem[];
  selected: Set<string>;
  onToggle: (k: string) => void;
  onToggleAll: () => void;
  onInstall: () => void;
  installing: boolean;
  progress: { done: number; total: number; current: string };
  results: { name: string; ok: boolean }[];
}) {
  const installableKeys = new Set(installableMissing.map(diKey));
  const okCount = results.filter((r) => r.ok).length;

  return (
    <div className="diff">
      <div className="diff-head">
        <span className="diff-base">{diff.base_label}</span>
        <ArrowRight size={14} />
        <span className="diff-target">{diff.target_label}</span>
      </div>
      <div className="diff-summary">
        <span className="d-add">+{diff.added.length} added</span>
        <span className="d-rem">−{diff.removed.length} missing here</span>
        <span className="d-chg">~{diff.changed.length} changed</span>
        <span className="d-same">{diff.unchanged} unchanged</span>
      </div>

      {/* Missing here → installable */}
      <section className="diff-sec">
        <div className="diff-sec-head">
          <h4>Missing here — only in the snapshot ({diff.removed.length})</h4>
          {installableMissing.length > 0 && (
            <div className="install-bar">
              <label className="sel-all">
                <input
                  type="checkbox"
                  checked={selected.size === installableMissing.length && selected.size > 0}
                  onChange={onToggleAll}
                />
                select all installable
              </label>
              <button
                className="btn btn-primary"
                disabled={selected.size === 0 || installing}
                onClick={onInstall}
              >
                <PackagePlus size={14} />
                {installing
                  ? `Installing ${progress.done}/${progress.total}…`
                  : `Install ${selected.size} selected`}
              </button>
            </div>
          )}
        </div>
        {installing && progress.current && (
          <div className="install-progress">Installing <strong>{progress.current}</strong>…</div>
        )}
        {results.length > 0 && (
          <div className="install-results">
            Done: {okCount}/{results.length} succeeded
            {results.some((r) => !r.ok) && " — some failed (check the tool is installed)"}
          </div>
        )}
        <div className="diff-rows">
          {diff.removed.map((i) => {
            const k = diKey(i);
            const canInstall = installableKeys.has(k);
            const res = results.find((r) => r.name === i.name);
            return (
              <label key={k} className={`diff-row ${canInstall ? "installable" : ""}`}>
                {canInstall ? (
                  <input
                    type="checkbox"
                    checked={selected.has(k)}
                    disabled={installing}
                    onChange={() => onToggle(k)}
                  />
                ) : (
                  <span className="diff-nobox" />
                )}
                <span className="diff-name">{i.name}</span>
                {i.version && <span className="diff-ver">{i.version}</span>}
                <span className="diff-col">{collectorLabel(i.collector)}</span>
                {res && <span className={`diff-res ${res.ok ? "ok" : "fail"}`}>{res.ok ? "installed ✓" : "failed"}</span>}
              </label>
            );
          })}
          {diff.removed.length === 0 && <div className="diff-empty">Nothing — you have everything in the snapshot.</div>}
        </div>
      </section>

      {/* Version changed */}
      {diff.changed.length > 0 && (
        <section className="diff-sec">
          <div className="diff-sec-head"><h4>Version changed ({diff.changed.length})</h4></div>
          <div className="diff-rows">
            {diff.changed.map((c) => (
              <div key={`${c.collector}:${c.name}`} className="diff-row">
                <span className="diff-nobox" />
                <span className="diff-name">{c.name}</span>
                <span className="diff-ver">{c.old_version ?? "—"}</span>
                <ArrowRight size={12} />
                <span className="diff-ver new">{c.new_version ?? "—"}</span>
                <span className="diff-col">{collectorLabel(c.collector)}</span>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Added (only in current) */}
      {diff.added.length > 0 && (
        <section className="diff-sec">
          <div className="diff-sec-head"><h4>Only in current — not in the snapshot ({diff.added.length})</h4></div>
          <div className="diff-rows">
            {diff.added.map((i) => (
              <div key={diKey(i)} className="diff-row">
                <span className="diff-nobox" />
                <span className="diff-name">{i.name}</span>
                {i.version && <span className="diff-ver">{i.version}</span>}
                <span className="diff-col">{collectorLabel(i.collector)}</span>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
