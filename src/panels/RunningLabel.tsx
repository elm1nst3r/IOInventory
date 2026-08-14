import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";

/**
 * "Running ⋯ 0:12" while a package-manager command is in flight.
 *
 * These commands run to completion in the backend and only report back at the
 * end, so there's no real progress to show — a `brew upgrade` can be silent for
 * minutes. What the UI can honestly say is that it's still alive and how long
 * it's been, which is what a stalled-looking button fails to convey.
 *
 * The elapsed clock does the work; the spinner and moving dots exist so the
 * indicator still reads as active in the first second, before the clock ticks.
 */
export default function RunningLabel({
  label = "Running",
  since,
}: {
  label?: string;
  /** `Date.now()` when the command started. Omit to show no clock. */
  since?: number;
}) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (since === undefined) return;
    setElapsed(Math.floor((Date.now() - since) / 1000));
    const id = setInterval(() => setElapsed(Math.floor((Date.now() - since) / 1000)), 1000);
    return () => clearInterval(id);
  }, [since]);

  return (
    <span className="running-label">
      <Loader2 size={13} className="spin" />
      {label}
      <span className="running-dots" aria-hidden="true">
        <i />
        <i />
        <i />
      </span>
      {since !== undefined && <span className="running-clock">{formatElapsed(elapsed)}</span>}
    </span>
  );
}

function formatElapsed(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
