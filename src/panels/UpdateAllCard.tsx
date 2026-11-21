import { useMemo, useState } from "react";
import { CheckCircle2, ChevronRight, Circle, Loader2, RefreshCw, XCircle } from "lucide-react";
import { useStore, type UpdateStep } from "../store";
import RunningLabel from "./RunningLabel";
import type { CleanupAction } from "../lib/types";

/**
 * Collectors whose outdated flags each updater clears. Only Homebrew and npm
 * are checked for staleness during a scan, so only those updaters get a count;
 * the rest are listed without one rather than with a misleading zero.
 */
const OUTDATED_FROM: Record<string, string[]> = {
  "brew-upgrade": ["homebrew", "homebrew-cask", "app"],
  "npm-upgrade": ["npm"],
};

/** Short names for the chips; the card titles are full sentences. */
const SHORT: Record<string, string> = {
  "brew-upgrade": "Homebrew",
  "npm-upgrade": "npm",
  "rustup-update": "Rust",
  "pipx-upgrade": "pipx",
  "cargo-update": "cargo",
  "gh-upgrade": "gh extensions",
};

function ago(at: number): string {
  const mins = Math.round((Date.now() - at) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

function duration(step: UpdateStep): string {
  if (!step.startedAt || !step.finishedAt) return "";
  const s = Math.max(0, Math.round((step.finishedAt - step.startedAt) / 1000));
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
}

const STATUS_ICON = {
  pending: <Circle size={15} className="ua-ico pending" />,
  running: <Loader2 size={15} className="ua-ico running spin" />,
  ok: <CheckCircle2 size={15} className="ua-ico ok" />,
  fail: <XCircle size={15} className="ua-ico fail" />,
};

export default function UpdateAllCard({
  updaters,
  otherBusy,
}: {
  /** Available update-category actions, in run order (no "update-all"). */
  updaters: CleanupAction[];
  /** A single utility is running in the panel; don't start alongside it. */
  otherBusy: boolean;
}) {
  const inventory = useStore((s) => s.inventory);
  const run = useStore((s) => s.updateRun);
  const last = useStore((s) => s.lastUpdateRun);
  const runUpdateAll = useStore((s) => s.runUpdateAll);
  const dismiss = useStore((s) => s.dismissUpdateRun);
  const [confirming, setConfirming] = useState(false);
  const [open, setOpen] = useState<Set<string>>(new Set());

  const outdated = useMemo(() => {
    const by: Record<string, number> = {};
    let total = 0;
    for (const item of inventory?.items ?? []) {
      if (!item.metadata?.outdated) continue;
      for (const [id, collectors] of Object.entries(OUTDATED_FROM)) {
        if (collectors.includes(item.collector)) {
          by[id] = (by[id] ?? 0) + 1;
          total++;
        }
      }
    }
    return { by, total };
  }, [inventory]);

  const running = run !== null && !run.finishedAt;
  const done = run?.steps.filter((s) => s.status === "ok" || s.status === "fail").length ?? 0;
  const failed = run?.steps.filter((s) => s.status === "fail") ?? [];

  const toggle = (id: string) =>
    setOpen((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });

  const start = () => {
    setConfirming(false);
    setOpen(new Set());
    runUpdateAll(updaters);
  };

  // Failed steps open by default once the run ends: that output is the reason
  // you're looking.
  const isOpen = (step: UpdateStep) =>
    open.has(step.id) !== (run?.finishedAt !== undefined && step.status === "fail");

  const lastLine = last
    ? `Last run ${ago(last.at)} · ${
        last.failed.length === 0 ? "all succeeded" : `${last.failed.length} failed`
      }`
    : "Never run from here";

  return (
    <section className={`ua-card ${running ? "is-running" : ""}`} aria-live="polite">
      <div className="ua-head">
        <div className="ua-badge">
          <RefreshCw size={20} className={running ? "spin-slow" : ""} />
        </div>
        <div className="ua-title">
          <h2>Update everything</h2>
          <p>
            {running ? (
              <>
                Updating {Math.min(done + 1, run!.steps.length)} of {run!.steps.length}…
              </>
            ) : run?.finishedAt ? (
              failed.length === 0 ? (
                <>All {run.steps.length} updaters succeeded.</>
              ) : (
                <>
                  {run.steps.length - failed.length} of {run.steps.length} succeeded ·{" "}
                  <span className="ua-fail-text">{failed.length} failed</span>
                </>
              )
            ) : (
              <>
                {outdated.total > 0 ? (
                  <strong>
                    {outdated.total} outdated package{outdated.total === 1 ? "" : "s"}
                  </strong>
                ) : (
                  "Nothing flagged outdated"
                )}{" "}
                in the last scan · {lastLine}
              </>
            )}
          </p>
        </div>
        <div className="ua-cta">
          {run?.finishedAt ? (
            <button className="btn" onClick={dismiss}>
              Done
            </button>
          ) : !running && !confirming ? (
            <button
              className="btn btn-primary ua-go"
              disabled={updaters.length === 0 || otherBusy}
              onClick={() => setConfirming(true)}
            >
              Update everything <ChevronRight size={15} />
            </button>
          ) : null}
        </div>
      </div>

      {running && (
        <div
          className="ua-progress"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={run!.steps.length}
          aria-valuenow={done}
        >
          <i style={{ width: `${(done / run!.steps.length) * 100}%` }} />
        </div>
      )}

      {!run && !confirming && (
        <div className="ua-chips">
          {updaters.length === 0 ? (
            <span className="ua-chip muted">No updaters installed</span>
          ) : (
            updaters.map((u) => (
              <span key={u.id} className="ua-chip">
                {SHORT[u.id] ?? u.title}
                {outdated.by[u.id] ? <b>{outdated.by[u.id]}</b> : null}
              </span>
            ))
          )}
        </div>
      )}

      {confirming && !run && (
        <div className="ua-confirm">
          <div className="cc-preview-head">These run in order, one after another</div>
          <ol>
            {updaters.map((u) => (
              <li key={u.id}>
                <span>{u.title}</span>
                <code>{u.command}</code>
              </li>
            ))}
          </ol>
          <p className="ua-note">
            If one fails, the rest still run. Large Homebrew upgrades can take several minutes.
          </p>
          <div className="cc-confirm">
            <button className="btn btn-primary" onClick={start}>
              Run {updaters.length} updater{updaters.length === 1 ? "" : "s"}
            </button>
            <button className="btn btn-ghost" onClick={() => setConfirming(false)}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {run && (
        <ol className="ua-steps">
          {run.steps.map((step) => {
            const finished = step.status === "ok" || step.status === "fail";
            return (
              <li key={step.id} className={`ua-step ${step.status}`}>
                <button
                  className="ua-step-row"
                  disabled={!finished}
                  aria-expanded={finished ? isOpen(step) : undefined}
                  onClick={() => toggle(step.id)}
                >
                  {STATUS_ICON[step.status]}
                  <span className="ua-step-title">{step.title}</span>
                  <code className="ua-step-cmd">{step.command}</code>
                  <span className="ua-step-time">
                    {step.status === "running" ? (
                      <RunningLabel label="" since={step.startedAt} />
                    ) : (
                      duration(step)
                    )}
                  </span>
                  {finished && (
                    <ChevronRight size={14} className={`ua-chev ${isOpen(step) ? "open" : ""}`} />
                  )}
                </button>
                {finished && isOpen(step) && <pre className="ua-out">{step.output || "(no output)"}</pre>}
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
