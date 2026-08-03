import { useMemo } from "react";
import { useStore } from "../store";
import { formatBytes } from "../lib/api";
import { DOMAIN_LABELS, type Domain, type Item } from "../lib/types";

const DOMAIN_ORDER: Domain[] = [
  "package_manager",
  "runtime",
  "project",
  "ai_agent",
  "container",
];

export default function ListView() {
  const inventory = useStore((s) => s.inventory);
  const search = useStore((s) => s.search);
  const select = useStore((s) => s.select);
  const selectedKey = useStore((s) => s.selectedKey);

  const grouped = useMemo(() => {
    if (!inventory) return [];
    const q = search.trim().toLowerCase();
    const items = q
      ? inventory.items.filter(
          (i) =>
            i.name.toLowerCase().includes(q) ||
            i.collector.toLowerCase().includes(q) ||
            (i.version ?? "").toLowerCase().includes(q),
        )
      : inventory.items;

    return DOMAIN_ORDER.map((domain) => {
      const domItems = items.filter((i) => i.domain === domain);
      const byCollector = new Map<string, Item[]>();
      for (const i of domItems) {
        if (!byCollector.has(i.collector)) byCollector.set(i.collector, []);
        byCollector.get(i.collector)!.push(i);
      }
      return { domain, byCollector, count: domItems.length };
    }).filter((g) => g.count > 0);
  }, [inventory, search]);

  if (!inventory) {
    return <div className="empty-hint">No scan yet — hit “Scan” to map your machine.</div>;
  }

  return (
    <div className="list-view">
      {grouped.map((g) => (
        <section key={g.domain} className="list-domain">
          <h3>
            {DOMAIN_LABELS[g.domain]} <span className="count-pill">{g.count}</span>
          </h3>
          {[...g.byCollector.entries()].map(([collector, items]) => (
            <div key={collector} className="list-collector">
              <div className="list-collector-head">{collector} · {items.length}</div>
              <div className="list-grid">
                {items.map((i) => (
                  <button
                    key={i.item_key}
                    className={`chip ${selectedKey === i.item_key ? "chip-sel" : ""} ${
                      i.why ? "chip-noted" : ""
                    }`}
                    title={i.why ?? ""}
                    onClick={() => select(i.item_key)}
                  >
                    {i.metadata?.outdated && (
                      <span className="chip-dot" title="Update available" />
                    )}
                    <span className="chip-name">{i.name}</span>
                    {i.version && <span className="chip-ver">{i.version}</span>}
                    {i.size_bytes ? (
                      <span className="chip-size">{formatBytes(i.size_bytes)}</span>
                    ) : null}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}
