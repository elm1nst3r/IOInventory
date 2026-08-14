import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { useStore } from "../store";
import RunningLabel from "./RunningLabel";
import type { CleanupAction, CleanupPreview, CleanupResult } from "../lib/types";

export default function CleanupPanel() {
  const [actions, setActions] = useState<CleanupAction[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [busySince, setBusySince] = useState<number | null>(null);
  const [preview, setPreview] = useState<CleanupPreview | null>(null);
  const [result, setResult] = useState<CleanupResult | null>(null);
  const scan = useStore((state) => state.scan);

  useEffect(() => {
    api.listCleanups().then(setActions).catch((error) => {
      setResult({ id: "load", command: "", output: String(error), success: false });
    });
  }, []);

  const groups = useMemo(() => {
    const updates = actions.filter((a) => a.category === "update");
    const cleanup = actions.filter((a) => a.category !== "update");
    return [
      { key: "update", title: "Updates", subtitle: "Bring tools and packages up to date.", items: updates },
      { key: "cleanup", title: "Cleanup", subtitle: "Reclaim disk space. Non-destructive — no packages removed.", items: cleanup },
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
                  {!a.available && <span className="cc-na">not installed</span>}
                </div>
                <p>{a.description}</p>
                <code className="cc-cmd">{a.command}</code>
                <div className="cc-actions">
                  <button
                    className="btn"
                    disabled={!a.available || busy !== null}
                    onClick={() => doPreview(a.id)}
                  >
                    {busy === a.id && preview?.id !== a.id ? <RunningLabel label="Checking" since={busySince ?? undefined} /> : "Preview"}
                  </button>
                </div>

                {preview?.id === a.id && (
                  <div className="cc-preview">
                    <div className="cc-preview-head">
                      {a.category === "update" ? "What will change" : "Dry-run preview"}
                    </div>
                    <pre>{preview.output}</pre>
                    <div className="cc-confirm">
                      <span>Run {a.category === "update" ? "update" : "this"}?</span>
                      <button
                        className="btn btn-danger"
                        disabled={busy !== null}
                        onClick={() => doRun(a.id)}
                      >
                        {busy === a.id ? <RunningLabel since={busySince ?? undefined} /> : "Confirm & run"}
                      </button>
                      <button className="btn btn-ghost" disabled={busy !== null} onClick={() => setPreview(null)}>
                        Cancel
                      </button>
                    </div>
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
