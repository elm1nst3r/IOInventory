import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown } from "lucide-react";
import { useStore } from "../store";
import { formatBytes } from "../lib/api";
import { passesFilters, itemInView, isDependency } from "../lib/filters";
import { collectorLabel } from "../lib/labels";
import { DOMAIN_LABELS, type Domain, type Item } from "../lib/types";

const DOMAIN_ORDER: Domain[] = [
  "package_manager",
  "runtime",
  "project",
  "ai_agent",
  "container",
];

type Sort = "name-asc" | "name-desc" | "size-desc" | "size-asc";

const SORTS: { key: Sort; label: string }[] = [
  { key: "name-asc", label: "Name (A–Z)" },
  { key: "name-desc", label: "Name (Z–A)" },
  { key: "size-desc", label: "Size (largest first)" },
  { key: "size-asc", label: "Size (smallest first)" },
];

function sorter(sort: Sort) {
  return (a: Item, b: Item) => {
    const byName = (x: Item, y: Item) =>
      x.name.localeCompare(y.name, undefined, { sensitivity: "base" });
    switch (sort) {
      case "name-asc":
        return byName(a, b);
      case "name-desc":
        return byName(b, a);
      case "size-desc":
      case "size-asc": {
        const av = a.size_bytes ?? -1;
        const bv = b.size_bytes ?? -1;
        if (av < 0 && bv < 0) return byName(a, b);
        if (av < 0) return 1; // unknown sizes always last
        if (bv < 0) return -1;
        return sort === "size-desc" ? bv - av : av - bv;
      }
    }
  };
}

export default function ListView() {
  const inventory = useStore((s) => s.inventory);
  const search = useStore((s) => s.search);
  const filters = useStore((s) => s.filters);
  const activeView = useStore((s) => s.activeView);
  const select = useStore((s) => s.select);
  const selectedKey = useStore((s) => s.selectedKey);
  const showDependencies = useStore((s) => s.showDependencies);

  const [cats, setCats] = useState<Set<string>>(new Set()); // empty = all
  const [sort, setSort] = useState<Sort>("name-asc");
  const [open, setOpen] = useState(false);
  const ddRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ddRef.current && !ddRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // Items passing the shared quick-filters + search (before category filter).
  const base = useMemo(() => {
    if (!inventory) return [];
    const q = search.trim().toLowerCase();
    return inventory.items.filter((i) => {
      // Most of Homebrew and pip is transitive; hiding it by default is what
      // makes the list show what you actually installed. A search still
      // reaches dependencies — you shouldn't have to know the distinction to
      // find something by name.
      if (!showDependencies && !q && isDependency(i)) return false;
      if (!itemInView(i, activeView)) return false;
      if (!passesFilters(i, filters)) return false;
      if (!q) return true;
      return (
        i.name.toLowerCase().includes(q) ||
        i.collector.toLowerCase().includes(q) ||
        (i.version ?? "").toLowerCase().includes(q) ||
        (i.source_path ?? "").toLowerCase().includes(q) ||
        (i.why ?? "").toLowerCase().includes(q) ||
        (i.tags ?? []).some((tag) => tag.toLowerCase().includes(q)) ||
        String(i.metadata?.description ?? "").toLowerCase().includes(q) ||
        (i.metadata?.stacks ?? []).some((stack: string) => stack.toLowerCase().includes(q))
      );
    });
  }, [inventory, search, filters, activeView, showDependencies]);

  // Collector → count + domain, for the multi-select menu (grouped by domain).
  const menu = useMemo(() => {
    const counts = new Map<string, number>();
    const colDomain = new Map<string, Domain>();
    for (const i of base) {
      counts.set(i.collector, (counts.get(i.collector) ?? 0) + 1);
      colDomain.set(i.collector, i.domain);
    }
    return DOMAIN_ORDER.map((domain) => ({
      domain,
      collectors: [...counts.keys()]
        .filter((c) => colDomain.get(c) === domain)
        .sort((a, b) => collectorLabel(a).localeCompare(collectorLabel(b)))
        .map((c) => ({ collector: c, count: counts.get(c)! })),
    })).filter((g) => g.collectors.length > 0);
  }, [base]);

  const filtered = useMemo(
    () => base.filter((i) => cats.size === 0 || cats.has(i.collector)).sort(sorter(sort)),
    [base, cats, sort],
  );

  // When categories are selected, render one section per selected collector.
  const sections = useMemo(() => {
    if (cats.size === 0) return [];
    const byCol = new Map<string, Item[]>();
    const colDomain = new Map<string, Domain>();
    for (const i of filtered) {
      if (!byCol.has(i.collector)) byCol.set(i.collector, []);
      byCol.get(i.collector)!.push(i);
      colDomain.set(i.collector, i.domain);
    }
    return [...byCol.keys()]
      .sort((a, b) => {
        const da = DOMAIN_ORDER.indexOf(colDomain.get(a)!);
        const db = DOMAIN_ORDER.indexOf(colDomain.get(b)!);
        return da !== db ? da - db : collectorLabel(a).localeCompare(collectorLabel(b));
      })
      .map((c) => ({ collector: c, items: byCol.get(c)! }));
  }, [filtered, cats]);

  // For "all", keep the domain → collector grouping (items already sorted).
  const grouped = useMemo(() => {
    if (cats.size !== 0) return [];
    return DOMAIN_ORDER.map((domain) => {
      const domItems = filtered.filter((i) => i.domain === domain);
      const byCollector = new Map<string, Item[]>();
      for (const i of domItems) {
        if (!byCollector.has(i.collector)) byCollector.set(i.collector, []);
        byCollector.get(i.collector)!.push(i);
      }
      return { domain, byCollector, count: domItems.length };
    }).filter((g) => g.count > 0);
  }, [filtered, cats]);

  // Dependencies suppressed in the current view, per collector, so a header can
  // say "15 · 91 deps hidden" instead of silently under-reporting the total.
  const hiddenDeps = useMemo(() => {
    const m = new Map<string, number>();
    if (!inventory || showDependencies) return m;
    if (search.trim()) return m; // a search reaches deps, so none are hidden
    for (const i of inventory.items) {
      if (!isDependency(i)) continue;
      if (!itemInView(i, activeView) || !passesFilters(i, filters)) continue;
      m.set(i.collector, (m.get(i.collector) ?? 0) + 1);
    }
    return m;
  }, [inventory, showDependencies, search, activeView, filters]);

  if (!inventory) {
    return <div className="empty-hint">No scan yet — hit “Scan” to map your machine.</div>;
  }

  const toggleCat = (c: string) =>
    setCats((prev) => {
      const next = new Set(prev);
      next.has(c) ? next.delete(c) : next.add(c);
      return next;
    });

  const catLabel =
    cats.size === 0
      ? "All categories"
      : cats.size === 1
        ? collectorLabel([...cats][0])
        : `${cats.size} categories`;

  const chip = (i: Item) => (
    <button
      key={i.item_key}
      className={`chip ${selectedKey === i.item_key ? "chip-sel" : ""} ${i.why ? "chip-noted" : ""}`}
      title={i.why ?? ""}
      onClick={() => select(i.item_key)}
    >
      {i.metadata?.outdated && <span className="chip-dot" title="Update available" />}
      {i.metadata?.deprecated && <span className="chip-dot dep" title="Deprecated" />}
      <span className="chip-name">{i.name}</span>
      {i.version && <span className="chip-ver">{i.version}</span>}
      {i.size_bytes ? <span className="chip-size">{formatBytes(i.size_bytes)}</span> : null}
    </button>
  );

  return (
    <div className="list-view">
      <div className="list-toolbar">
        <label className="lt-field">
          Category
          <div className="ms" ref={ddRef}>
            <button
              className="ms-btn"
              onClick={() => setOpen((o) => !o)}
              aria-expanded={open}
              aria-haspopup="true"
            >
              {catLabel} <ChevronDown size={14} />
            </button>
            {open && (
              <div className="ms-panel" role="group" aria-label="Inventory categories">
                <label className="ms-row ms-all">
                  <input
                    type="checkbox"
                    checked={cats.size === 0}
                    onChange={() => setCats(new Set())}
                  />
                  All categories
                </label>
                {menu.map((g) => (
                  <div key={g.domain} className="ms-group">
                    <div className="ms-group-label">{DOMAIN_LABELS[g.domain]}</div>
                    {g.collectors.map(({ collector, count }) => (
                      <label key={collector} className="ms-row">
                        <input
                          type="checkbox"
                          checked={cats.has(collector)}
                          onChange={() => toggleCat(collector)}
                        />
                        <span className="ms-name">{collectorLabel(collector)}</span>
                        <span className="ms-count">{count}</span>
                      </label>
                    ))}
                  </div>
                ))}
              </div>
            )}
          </div>
        </label>

        <label className="lt-field">
          Sort by
          <select value={sort} onChange={(e) => setSort(e.target.value as Sort)}>
            {SORTS.map((s) => (
              <option key={s.key} value={s.key}>
                {s.label}
              </option>
            ))}
          </select>
        </label>

        {cats.size > 0 && (
          <button className="fb-clear" onClick={() => setCats(new Set())}>
            Reset
          </button>
        )}
        <span className="lt-count">{filtered.length} shown</span>
      </div>

      {filtered.length === 0 ? (
        <div className="empty-hint">Nothing matches the current filters.</div>
      ) : cats.size > 0 ? (
        sections.map((s) => (
          <section key={s.collector} className="list-domain">
            <h3>
              {collectorLabel(s.collector)} <span className="count-pill">{s.items.length}</span>
              {!showDependencies && hiddenDeps.get(s.collector) ? (
                <span className="lc-deps">{hiddenDeps.get(s.collector)} deps hidden</span>
              ) : null}
            </h3>
            <div className="list-grid">{s.items.map(chip)}</div>
          </section>
        ))
      ) : (
        grouped.map((g) => (
          <section key={g.domain} className="list-domain">
            <h3>
              {DOMAIN_LABELS[g.domain]} <span className="count-pill">{g.count}</span>
            </h3>
            {[...g.byCollector.entries()].map(([collector, items]) => (
              <div key={collector} className="list-collector">
                <div className="list-collector-head">
                  {collectorLabel(collector)} · {items.length}
                  {!showDependencies && hiddenDeps.get(collector) ? (
                    <span className="lc-deps"> · {hiddenDeps.get(collector)} deps hidden</span>
                  ) : null}
                </div>
                <div className="list-grid">{items.map(chip)}</div>
              </div>
            ))}
          </section>
        ))
      )}
    </div>
  );
}
