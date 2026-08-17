import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Camera,
  Upload,
  Eye,
  GitCompare,
  Download,
  Trash2,
  PackagePlus,
  ArrowRight,
  Pencil,
  Check,
  X,
} from "lucide-react";
import { useStore } from "../store";
import RunningLabel from "./RunningLabel";
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
  const liveInventory = useStore((s) => s.liveInventory);

  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const [diff, setDiff] = useState<Diff | null>(null);
  const [diffFor, setDiffFor] = useState<SnapshotMeta | null>(null);
  /** The "after" side of the comparison. null = the current scan. */
  const [targetId, setTargetId] = useState<number | null>(null);
  const [reloadToken, setReloadToken] = useState(0);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [renamingId, setRenamingId] = useState<number | null>(null);
  const [renameValue, setRenameValue] = useState("");
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

  // Load whenever the pair being compared changes, or after an install run
  // bumps the token. Keyed on ids so re-fetching the snapshot list (which
  // returns fresh objects) doesn't retrigger it.
  useEffect(() => {
    if (!diffFor) {
      setDiff(null);
      return;
    }
    let cancelled = false;
    setDiff(null);
    setSelected(new Set());
    api
      .diffSnapshot(diffFor.id, targetId)
      .then((d) => {
        if (!cancelled) setDiff(d);
      })
      .catch((e) => {
        if (!cancelled) {
          flash(String(e));
          setDiffFor(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [diffFor?.id, targetId, reloadToken]);

  function compare(meta: SnapshotMeta) {
    setResults([]);
    setTargetId(null);
    setDiffFor(meta);
  }

  async function exportSnap(meta: SnapshotMeta) {
    try {
      const { path } = await api.exportSnapshot(meta.id);
      flash(`Exported to ${path}`);
    } catch (e) {
      flash(String(e));
    }
  }

  function startRename(meta: SnapshotMeta) {
    setRenamingId(meta.id);
    setRenameValue(meta.name);
  }

  function cancelRename() {
    setRenamingId(null);
    setRenameValue("");
  }

  async function commitRename() {
    const id = renamingId;
    if (id === null) return;
    const trimmed = renameValue.trim();
    const current = snapshots.find((s) => s.id === id);
    if (!trimmed || trimmed === current?.name) {
      cancelRename();
      return;
    }
    try {
      await api.renameSnapshot(id, trimmed);
      cancelRename();
      await refreshSnapshots();
    } catch (e) {
      flash(String(e));
    }
  }

  async function del(meta: SnapshotMeta) {
    if (!confirm(`Delete snapshot “${meta.name}”?`)) return;
    try {
      await api.deleteSnapshot(meta.id);
      if (diffFor?.id === meta.id) setDiffFor(null);
      // Deleting the snapshot being compared against falls back to the current scan.
      if (targetId === meta.id) setTargetId(null);
      await refreshSnapshots();
    } catch (e) {
      flash(String(e));
    }
  }

  // What's worth offering to install is decided by this machine, not by which
  // two things are being compared: installing always means installing *here*,
  // and the diff is only a source of candidates. So the test is "installable,
  // and not already present locally" — which works the same whether the other
  // side is the current scan or a second snapshot, and stays right if a diff
  // goes stale after something is installed.
  const presentHere = useMemo(() => {
    const live = liveInventory ?? inventory;
    return new Set((live?.items ?? []).map((i) => `${i.collector}:${i.name.toLowerCase()}`));
  }, [liveInventory, inventory]);

  const canInstall = useCallback(
    (i: DiffItem) =>
      INSTALLABLE.has(i.collector) && !presentHere.has(`${i.collector}:${i.name.toLowerCase()}`),
    [presentHere],
  );

  // Both columns can hold things this machine lacks once neither side is the
  // current scan, so both get checkboxes.
  const installable = useMemo(
    () => (diff ? [...diff.removed, ...diff.added].filter(canInstall) : []),
    [diff, canInstall],
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
      prev.size === installable.length
        ? new Set()
        : new Set(installable.map(diKey)),
    );
  }

  async function installSelected() {
    const chosen = installable.filter((i) => selected.has(diKey(i)));
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
        const r = await api.runItemAction({ collector: it.collector, name: it.name }, "install");
        done.push({ name: it.name, ok: r.success });
      } catch {
        done.push({ name: it.name, ok: false });
      }
      setResults([...done]);
    }
    // Refresh live inventory, then the diff so installed items drop off the
    // list. Stay "installing" until that settles — re-enabling the controls
    // over a diff that's about to be replaced just invites a double run.
    await scan();
    setReloadToken((t) => t + 1);
    setInstalling(false);
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
                {renamingId === s.id ? (
                  <div className="snap-card-title snap-rename">
                    <input
                      autoFocus
                      className="snap-rename-input"
                      value={renameValue}
                      onChange={(e) => setRenameValue(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") commitRename();
                        if (e.key === "Escape") cancelRename();
                      }}
                      onBlur={commitRename}
                    />
                    <button title="Save" onMouseDown={(e) => e.preventDefault()} onClick={commitRename}>
                      <Check size={14} />
                    </button>
                    <button title="Cancel" onMouseDown={(e) => e.preventDefault()} onClick={cancelRename}>
                      <X size={14} />
                    </button>
                  </div>
                ) : (
                  <div className="snap-card-title">
                    {s.name}
                    <button title="Rename" className="snap-rename-btn" onClick={() => startRename(s)}>
                      <Pencil size={13} />
                    </button>
                    <span className={`snap-src ${s.source}`}>{s.source}</span>
                  </div>
                )}
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
          {!diffFor ? (
            <div className="empty-hint" style={{ height: "100%" }}>
              Pick <GitCompare size={14} style={{ margin: "0 4px", verticalAlign: "-2px" }} /> on a
              snapshot to compare it with your current scan — or with another snapshot.
            </div>
          ) : !diff ? (
            <div className="empty-hint" style={{ height: "100%" }}>
              Comparing…
            </div>
          ) : (
            <DiffView
              diff={diff}
              base={diffFor}
              snapshots={snapshots}
              targetId={targetId}
              onTargetChange={(id) => {
                setResults([]);
                setTargetId(id);
              }}
              installable={installable}
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
  base,
  snapshots,
  targetId,
  onTargetChange,
  installable,
  selected,
  onToggle,
  onToggleAll,
  onInstall,
  installing,
  progress,
  results,
}: {
  diff: Diff;
  base: SnapshotMeta;
  snapshots: SnapshotMeta[];
  targetId: number | null;
  onTargetChange: (id: number | null) => void;
  installable: DiffItem[];
  selected: Set<string>;
  onToggle: (k: string) => void;
  onToggleAll: () => void;
  onInstall: () => void;
  installing: boolean;
  progress: { done: number; total: number; current: string };
  results: { name: string; ok: boolean }[];
}) {
  const installableKeys = new Set(installable.map(diKey));
  const okCount = results.filter((r) => r.ok).length;
  const vsCurrent = targetId === null;

  return (
    <div className="diff">
      <div className="diff-head">
        <span className="diff-base">{diff.base_label}</span>
        <ArrowRight size={14} />
        <label className="diff-target-pick">
          <span className="sr-only">Compare against</span>
          <select
            value={targetId ?? ""}
            disabled={installing}
            onChange={(e) => onTargetChange(e.target.value === "" ? null : Number(e.target.value))}
          >
            <option value="">Current scan</option>
            {snapshots
              .filter((s) => s.id !== base.id)
              .map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name} · {s.created_at.slice(0, 10)}
                </option>
              ))}
          </select>
        </label>
      </div>
      <div className="diff-summary">
        <span className="d-add">+{diff.added.length} added</span>
        <span className="d-rem">
          −{diff.removed.length} {vsCurrent ? "missing here" : "only in base"}
        </span>
        <span className="d-chg">~{diff.changed.length} changed</span>
        <span className="d-same">{diff.unchanged} unchanged</span>
      </div>

      {installable.length > 0 && (
        <div className="install-bar">
          <label className="sel-all">
            <input
              type="checkbox"
              checked={selected.size === installable.length && selected.size > 0}
              onChange={onToggleAll}
            />
            select all {installable.length} you don't have
          </label>
          <button
            className="btn btn-primary"
            disabled={selected.size === 0 || installing}
            onClick={onInstall}
          >
            <PackagePlus size={14} />
            {installing ? (
              <RunningLabel label={`Installing ${progress.done}/${progress.total}`} />
            ) : (
              `Install ${selected.size} selected`
            )}
          </button>
          <span className="install-note">Installs the current version, not the one recorded.</span>
        </div>
      )}
      {installing && progress.current && (
        <div className="install-progress">Installing <strong>{progress.current}</strong>…</div>
      )}
      {results.length > 0 && (
        <div className="install-results">
          Done: {okCount}/{results.length} succeeded
          {results.some((r) => !r.ok) && " — some failed (check the tool is installed)"}
        </div>
      )}

      <section className="diff-sec">
        <div className="diff-sec-head">
          <h4>
            {vsCurrent
              ? `Missing here — only in the snapshot (${diff.removed.length})`
              : `Only in ${diff.base_label} (${diff.removed.length})`}
          </h4>
        </div>
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
          {diff.removed.length === 0 && (
            <div className="diff-empty">
              {vsCurrent
                ? "Nothing — you have everything in the snapshot."
                : "Nothing — everything in the base is also in the comparison."}
            </div>
          )}
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
          <div className="diff-sec-head">
            <h4>
              {vsCurrent
                ? `Only in current — not in the snapshot (${diff.added.length})`
                : `Only in ${diff.target_label} (${diff.added.length})`}
            </h4>
          </div>
          <div className="diff-rows">
            {diff.added.map((i) => {
              const k = diKey(i);
              const offerInstall = installableKeys.has(k);
              const res = results.find((r) => r.name === i.name);
              return (
                <label key={k} className={`diff-row ${offerInstall ? "installable" : ""}`}>
                  {offerInstall ? (
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
                  {res && (
                    <span className={`diff-res ${res.ok ? "ok" : "fail"}`}>
                      {res.ok ? "installed ✓" : "failed"}
                    </span>
                  )}
                </label>
              );
            })}
          </div>
        </section>
      )}
    </div>
  );
}
