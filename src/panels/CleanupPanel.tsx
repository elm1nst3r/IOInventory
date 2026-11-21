import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../store";
import RunningLabel from "./RunningLabel";
import UpdateAllCard from "./UpdateAllCard";
import { AlertTriangle } from "lucide-react";
import type { CleanupAction, CleanupPreview, CleanupResult } from "../lib/types";

export default function CleanupPanel() {
  const [actions, setActions] = useState<CleanupAction[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [busySince, setBusySince] = useState<number | null>(null);
  const [preview, setPreview] = useState<CleanupPreview | null>(null);
  const [result, setResult] = useState<CleanupResult | null>(null);
  const scan = useStore((state) => state.scan);
  const inventory = useStore((state) => state.inventory);
  const select = useStore((state) => state.select);
  const setTab = useStore((state) => state.setTab);
  // Tools installed more than once, where only the first on PATH runs — the
  // updates above can't fix those, so they're called out here.
  const conflicts = useMemo(
    () => (inventory?.items ?? []).filter((i) => i.collector === "path-conflict"),
    [inventory],
  );
  // The update-all run holds the backend's operation lock between steps;
  // starting a single utility meanwhile would just fail with "already running".
  const updateRunning = useStore((state) => state.updateRun !== null && !state.updateRun.finishedAt);
  const locked = busy !== null || updateRunning;

  useEffect(() => {
    api.listCleanups().then(setActions).catch((error) => {
      setResult({ id: "load", command: "", output: String(error), success: false });
    });
  }, []);

  // Every installed updater, in the order "Update everything" runs them.
  const updaters = useMemo(
    () => actions.filter((a) => a.category === "update" && a.id !== "update-all" && a.available),
    [actions],
  );

  const groups = useMemo(() => {
    // "Update everything" has its own card at the top.
    const updates = actions.filter((a) => a.category === "update" && a.id !== "update-all");
    const cleanup = actions.filter((a) => a.category === "cleanup");
    const system = actions.filter((a) => a.category === "system");
    // Installed updaters first; the ones that can't run here sink to the end.
    updates.sort((a, b) => Number(b.available) - Number(a.available));
    return [
      { key: "update", title: "Updates", subtitle: "Bring tools and packages up to date.", items: updates },
      { key: "cleanup", title: "Cleanup", subtitle: "Reclaim disk space. Non-destructive — no packages removed.", items: cleanup },
      { key: "system", title: "System", subtitle: "Checked on demand. Installing needs an administrator, so it happens outside this app.", items: system },
    ].filter((g) => g.items.length > 0);
  }, [actions]);

  async function doPreview(id: string) {
    setResult(null);
    setBusy(id);
    setBusySince(Date.now());
    try {
      setPreview(await api.previewCleanup(id));
    } catch (error) {
      setResult({ id, command: "", output: String(error), success: false });
    } finally {
      setBusy(null);
      setBusySince(null);
    }
  }

  async function doRun(id: string) {
    setBusy(id);
    setBusySince(Date.now());
    try {
      const r = await api.runCleanup(id);
      setResult(r);
      setPreview(null);
      if (r.success) await scan();
    } catch (error) {
      setResult({ id, command: "", output: String(error), success: false });
    } finally {
      setBusy(null);
      setBusySince(null);
    }
  }

  return (
    <div className="cleanup">
      <div className="cleanup-intro">
        <h2>Utilities</h2>
        <p>
          Maintenance tasks for your dev environment. Each action shows a{" "}
          <strong>preview first</strong> and only runs when you confirm.
        </p>
      </div>

      {result?.id === "load" && (
        <div className="cc-result fail" role="alert">
          <div className="cc-preview-head">Could not load utilities</div>
          <pre>{result.output}</pre>
        </div>
      )}

      {actions.length > 0 && <UpdateAllCard updaters={updaters} otherBusy={busy !== null} />}

      {conflicts.length > 0 && (
        <section className="util-group">
          <div className="util-group-head">
            <h3>
              <AlertTriangle size={15} className="pc-ico" /> Installed more than once
            </h3>
            <span>Only the first copy on your PATH runs. Updating another copy changes nothing you use.</span>
          </div>
          <div className="pc-list">
            {conflicts.map((c) => (
              <button
                key={c.item_key}
                className="pc-row"
                onClick={() => {
                  select(c.item_key);
                  setTab("list");
                }}
              >
                <code className="pc-name">{c.name}</code>
                <span className="pc-desc">{c.metadata?.description}</span>
              </button>
            ))}
          </div>
        </section>
      )}

      {groups.map((g) => (
        <section key={g.key} className="util-group">
          <div className="util-group-head">
            <h3>{g.title}</h3>
            <span>{g.subtitle}</span>
          </div>
          <div className="cleanup-cards">
            {g.items.map((a) => (
              <div key={a.id} className={`cleanup-card ${a.available ? "" : "disabled"}`}>
                <div className="cc-head">
                  <h3>{a.title}</h3>
                  {!a.available && <span className="cc-na">{a.note ? "unavailable" : "not installed"}</span>}
                </div>
                <p>{a.description}</p>
                {a.note && <p className="cc-note">{a.note}</p>}
                <code className="cc-cmd">{a.command}</code>
                <div className="cc-actions">
                  <button
                    className="btn"
                    disabled={!a.available || locked}
                    onClick={() => doPreview(a.id)}
                  >
                    {busy === a.id && preview?.id !== a.id ? <RunningLabel label="Checking" since={busySince ?? undefined} /> : a.category === "system" ? "Check now" : "Preview"}
                  </button>
                </div>

                {preview?.id === a.id && (
                  <div className="cc-preview">
                    <div className="cc-preview-head">
                      {a.category === "update"
                        ? "What will change"
                        : a.category === "system"
                          ? "Pending software updates"
                          : "Dry-run preview"}
                    </div>
                    <pre>{preview.output}</pre>
                    {!a.runnable ? (
                      <div className="cc-confirm">
                        <button className="btn btn-ghost" onClick={() => setPreview(null)}>
                          Close
                        </button>
                      </div>
                    ) : (
                    <div className="cc-confirm">
                      <span>Run {a.category === "update" ? "update" : "this"}?</span>
                      <button
                        className="btn btn-danger"
                        disabled={locked}
                        onClick={() => doRun(a.id)}
                      >
                        {busy === a.id ? <RunningLabel since={busySince ?? undefined} /> : "Confirm & run"}
                      </button>
                      <button className="btn btn-ghost" disabled={busy !== null} onClick={() => setPreview(null)}>
                        Cancel
                      </button>
                    </div>
                    )}
                  </div>
                )}

                {result?.id === a.id && (
                  <div className={`cc-result ${result.success ? "ok" : "fail"}`}>
                    <div className="cc-preview-head">{result.success ? "Done ✓" : "Failed"}</div>
                    <pre>{result.output}</pre>
                  </div>
                )}
              </div>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
