import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import type { CleanupAction, CleanupPreview, CleanupResult } from "../lib/types";

export default function CleanupPanel() {
  const [actions, setActions] = useState<CleanupAction[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [preview, setPreview] = useState<CleanupPreview | null>(null);
  const [result, setResult] = useState<CleanupResult | null>(null);

  useEffect(() => {
    api.listCleanups().then(setActions);
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
    try {
      setPreview(await api.previewCleanup(id));
    } finally {
      setBusy(null);
    }
  }

  async function doRun(id: string) {
    setBusy(id);
    try {
      const r = await api.runCleanup(id);
      setResult(r);
      setPreview(null);
    } finally {
      setBusy(null);
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
                    disabled={!a.available || busy === a.id}
                    onClick={() => doPreview(a.id)}
                  >
                    {busy === a.id && preview?.id !== a.id ? "…" : "Preview"}
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
                        disabled={busy === a.id}
                        onClick={() => doRun(a.id)}
                      >
                        {busy === a.id ? "Running…" : "Confirm & run"}
                      </button>
                      <button className="btn btn-ghost" onClick={() => setPreview(null)}>
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
