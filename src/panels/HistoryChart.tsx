import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useStore } from "../store";
import { api } from "../lib/api";
import { DOMAIN_LABELS, type Domain, type SnapshotStats } from "../lib/types";

/** Stack order, bottom to top. Each category keeps its colour when others are hidden. */
const DOMAINS: Domain[] = [
  "package_manager",
  "application",
  "ai_agent",
  "project",
  "container",
  "runtime",
];

const MAX_BARS = 60;
const HEIGHT = 250;
const M = { top: 12, right: 8, bottom: 28, left: 40 };

interface Point {
  key: string;
  name: string;
  at: Date | null;
  counts: Partial<Record<Domain, number>>;
  total: number;
  imported: boolean;
}

const countOf = (p: Point, focus: Domain | null) =>
  focus ? (p.counts[focus] ?? 0) : p.total;

const signed = (n: number) => (n > 0 ? `+${n}` : n < 0 ? `−${-n}` : "0");

function shortDate(d: Date | null, withYear: boolean) {
  if (!d) return "Now";
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    ...(withYear ? { year: "2-digit" } : {}),
  });
}

/** Round axis ticks from 0 up to at least `max`. */
function ticks(max: number): number[] {
  if (max <= 0) return [0, 1];
  const raw = max / 4;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = Math.max(1, ([1, 2, 2.5, 5, 10].find((m) => m * mag >= raw) ?? 10) * mag);
  const out: number[] = [];
  for (let v = 0; v < max + step; v += step) out.push(Math.round(v * 100) / 100);
  return out;
}

/** A bar segment; only the top of a stack gets rounded corners. */
function segment(x: number, y: number, w: number, h: number, rounded: boolean) {
  const r = rounded ? Math.min(4, w / 2, h) : 0;
  return `M${x},${y + h}V${y + r}Q${x},${y} ${x + r},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h}Z`;
}

export default function HistoryChart() {
  const snapshots = useStore((s) => s.snapshots);
  const liveInventory = useStore((s) => s.liveInventory);
  const inventory = useStore((s) => s.inventory);
  const viewing = useStore((s) => s.viewingSnapshot);
  const live = liveInventory ?? (viewing ? null : inventory);

  const [stats, setStats] = useState<SnapshotStats[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [focus, setFocus] = useState<Domain | null>(null);
  const [host, setHost] = useState<string | null>(null);
  const [asTable, setAsTable] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const [width, setWidth] = useState(0);
  const plotRef = useRef<HTMLDivElement>(null);

  // `snapshots` gets a new reference after every save, import, rename or delete.
  useEffect(() => {
    let cancelled = false;
    api
      .snapshotStats()
      .then((s) => {
        if (cancelled) return;
        setStats(s);
        setError(null);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [snapshots]);

  useLayoutEffect(() => {
    const el = plotRef.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [asTable, stats.length]);

  const liveHost = live?.scan.host ?? null;
  const hosts = useMemo(() => {
    const all = new Set(stats.map((s) => s.host));
    if (liveHost) all.add(liveHost);
    return [...all];
  }, [stats, liveHost]);
  // Imports can come from other machines; one trend line per machine.
  const activeHost = host && hosts.includes(host) ? host : (liveHost ?? hosts[0] ?? null);

  const { points, trimmed } = useMemo(() => {
    const all: Point[] = stats
      .filter((s) => s.host === activeHost)
      .map((s) => ({
        key: String(s.id),
        name: s.name,
        at: new Date(s.created_at),
        counts: s.domains,
        total: s.total,
        imported: s.source === "import",
      }));
    if (live && live.scan.host === activeHost) {
      const counts: Partial<Record<Domain, number>> = {};
      for (const item of live.items) counts[item.domain] = (counts[item.domain] ?? 0) + 1;
      all.push({
        key: "now",
        name: "Current scan",
        at: null,
        counts,
        total: live.items.length,
        imported: false,
      });
    }
    return { points: all.slice(-MAX_BARS), trimmed: Math.max(0, all.length - MAX_BARS) };
  }, [stats, live, activeHost]);

  if (error) return <section className="hist hist-empty">Couldn't load history: {error}</section>;
  if (stats.length === 0) return null;

  const first = points[0];
  const last = points[points.length - 1];
  const years = new Set(points.filter((p) => p.at).map((p) => p.at!.getFullYear()));
  const withYear = years.size > 1;
  const active = focus ? [focus] : DOMAINS;

  const yTicks = ticks(Math.max(...points.map((p) => countOf(p, focus)), 0));
  const yMax = yTicks[yTicks.length - 1];
  const plotH = HEIGHT - M.top - M.bottom;
  const plotW = Math.max(0, width - M.left - M.right);
  const slot = points.length ? plotW / points.length : 0;
  const barW = Math.max(3, Math.min(52, slot * 0.62));
  const y = (v: number) => M.top + plotH - (v / yMax) * plotH;
  const labelEvery = Math.max(1, Math.ceil(62 / Math.max(slot, 1)));

  const hovered = hover !== null ? points[hover] : null;
  const previous = hover !== null && hover > 0 ? points[hover - 1] : null;
  const headline = last ? countOf(last, focus) : 0;
  const change = first && last && first !== last ? headline - countOf(first, focus) : null;

  return (
    <section className="hist" aria-label="Inventory history">
      <div className="hist-head">
        <div>
          <div className="hist-eyebrow">{focus ? DOMAIN_LABELS[focus] : "Everything tracked"}</div>
          <div className="hist-hero">
            <strong>{headline.toLocaleString()}</strong>
            {change !== null && first && (
              <span>
                {signed(change)} since {shortDate(first.at, true)}
              </span>
            )}
          </div>
        </div>
        <div className="hist-controls">
          {hosts.length > 1 && (
            <label>
              <span className="sr-only">Machine</span>
              <select
                value={activeHost ?? ""}
                onChange={(e) => {
                  setHost(e.target.value);
                  setHover(null);
                }}
              >
                {hosts.map((h) => (
                  <option key={h} value={h}>
                    {h}
                  </option>
                ))}
              </select>
            </label>
          )}
          <div className="hist-toggle" role="group" aria-label="Display as">
            <button className={!asTable ? "on" : ""} aria-pressed={!asTable} onClick={() => setAsTable(false)}>
              Chart
            </button>
            <button className={asTable ? "on" : ""} aria-pressed={asTable} onClick={() => setAsTable(true)}>
              Table
            </button>
          </div>
        </div>
      </div>

      <div className="hist-legend">
        {DOMAINS.filter((d) => points.some((p) => p.counts[d])).map((d) => {
          const now = last?.counts[d] ?? 0;
          const delta = first && first !== last ? now - (first.counts[d] ?? 0) : null;
          return (
            <button
              key={d}
              className={focus === d ? "on" : focus ? "dim" : ""}
              aria-pressed={focus === d}
              title={focus === d ? "Show all categories" : `Show only ${DOMAIN_LABELS[d]}`}
              onClick={() => {
                setFocus(focus === d ? null : d);
                setHover(null);
              }}
            >
              <i style={{ background: `var(--hist-${d})` }} />
              <span>{DOMAIN_LABELS[d]}</span>
              <b>{now}</b>
              {delta !== null && <em>{signed(delta)}</em>}
            </button>
          );
        })}
      </div>

      {asTable ? (
        <div className="hist-table">
          <table>
            <thead>
              <tr>
                <th>Snapshot</th>
                {DOMAINS.map((d) => (
                  <th key={d}>{DOMAIN_LABELS[d]}</th>
                ))}
                <th>Total</th>
              </tr>
            </thead>
            <tbody>
              {[...points].reverse().map((p) => (
                <tr key={p.key}>
                  <td>
                    {p.name}
                    {p.at && <small>{shortDate(p.at, true)}</small>}
                  </td>
                  {DOMAINS.map((d) => (
                    <td key={d}>{p.counts[d] ?? 0}</td>
                  ))}
                  <td>
                    <b>{p.total}</b>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="hist-plot" ref={plotRef} onMouseLeave={() => setHover(null)}>
          {width > 0 && (
            <svg width={width} height={HEIGHT} role="img" aria-label="Item counts per snapshot, by category">
              {yTicks.map((t) => (
                <g key={t}>
                  <line className="hist-grid" x1={M.left} x2={width - M.right} y1={y(t)} y2={y(t)} />
                  <text className="hist-axis" x={M.left - 8} y={y(t) + 4} textAnchor="end">
                    {t}
                  </text>
                </g>
              ))}
              {points.map((p, i) => {
                const x = M.left + slot * i + (slot - barW) / 2;
                const shown = active.filter((d) => (p.counts[d] ?? 0) > 0);
                let base = 0;
                return (
                  <g key={p.key}>
                    {hover === i && (
                      <rect className="hist-hover" x={M.left + slot * i} y={M.top} width={slot} height={plotH} rx={5} />
                    )}
                    {shown.map((d, si) => {
                      const v = p.counts[d] ?? 0;
                      const top = y(base + v);
                      const full = y(base) - top;
                      base += v;
                      // A 2px gap between segments, without erasing tiny ones.
                      const h = si === shown.length - 1 ? full : Math.max(1.5, full - 2);
                      return (
                        <path
                          key={d}
                          d={segment(x, top, barW, h, si === shown.length - 1)}
                          style={{ fill: `var(--hist-${d})` }}
                        />
                      );
                    })}
                    {(i % labelEvery === 0 || i === points.length - 1) &&
                      (i === points.length - 1 || points.length - 1 - i >= labelEvery) && (
                        <text
                          className={`hist-axis ${p.at ? "" : "now"}`}
                          x={M.left + slot * i + slot / 2}
                          y={HEIGHT - 8}
                          textAnchor="middle"
                        >
                          {shortDate(p.at, withYear)}
                        </text>
                      )}
                    <rect
                      className="hist-hit"
                      x={M.left + slot * i}
                      y={M.top}
                      width={slot}
                      height={plotH + M.bottom}
                      tabIndex={0}
                      aria-label={`${p.name}: ${countOf(p, focus)} items`}
                      onMouseEnter={() => setHover(i)}
                      onFocus={() => setHover(i)}
                      onBlur={() => setHover(null)}
                    />
                  </g>
                );
              })}
            </svg>
          )}
          {hovered && hover !== null && (
            <div
              className="hist-tip"
              style={{
                left: Math.min(
                  Math.max(0, M.left + slot * hover + slot / 2 - 115),
                  Math.max(0, width - 230),
                ),
              }}
            >
              <div className="hist-tip-title">{hovered.name}</div>
              <div className="hist-tip-sub">
                {hovered.at ? hovered.at.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : "Latest scan"}
                {hovered.imported ? " · imported" : ""}
              </div>
              {[...active].reverse().filter((d) => hovered.counts[d]).map((d) => (
                <div className="hist-tip-row" key={d}>
                  <i style={{ background: `var(--hist-${d})` }} />
                  <span>{DOMAIN_LABELS[d]}</span>
                  <b>{hovered.counts[d]}</b>
                </div>
              ))}
              {!focus && (
                <div className="hist-tip-row total">
                  <span>Total</span>
                  <b>{hovered.total}</b>
                </div>
              )}
              {previous && (
                <div className="hist-tip-sub">
                  {signed(countOf(hovered, focus) - countOf(previous, focus))} since previous
                </div>
              )}
            </div>
          )}
        </div>
      )}
      <div className="hist-foot">
        {points.length < 2
          ? "Save another snapshot to start a trend."
          : "One bar per snapshot, evenly spaced regardless of the time between them."}
        {trimmed > 0 && ` Showing the latest ${MAX_BARS}.`}
      </div>
    </section>
  );
}
