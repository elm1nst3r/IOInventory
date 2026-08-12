import { useEffect, useMemo, useRef, useState } from "react";
import { SlidersHorizontal, X, Bookmark, ChevronDown } from "lucide-react";
import { useStore } from "../store";
import { FILTERS, countMatching, allTags, countDependencies } from "../lib/filters";

export default function FilterBar() {
  const inventory = useStore((s) => s.inventory);
  const filters = useStore((s) => s.filters);
  const toggleFilter = useStore((s) => s.toggleFilter);
  const clearFilters = useStore((s) => s.clearFilters);
  const activeView = useStore((s) => s.activeView);
  const setView = useStore((s) => s.setView);
  const showDependencies = useStore((s) => s.showDependencies);
  const toggleDependencies = useStore((s) => s.toggleDependencies);

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

  const tags = useMemo(() => (inventory ? allTags(inventory.items) : []), [inventory]);
  const depCount = useMemo(
    () => (inventory ? countDependencies(inventory.items) : 0),
    [inventory],
  );

  if (!inventory) return null;

  const chips = FILTERS.map((f) => ({
    ...f,
    count: countMatching(inventory.items, f.key),
  })).filter((f) => f.count > 0 || filters.has(f.key));

  // Nothing to show at all (no tags, no matchable quick filters, no deps).
  if (chips.length === 0 && tags.length === 0 && depCount === 0) return null;

  return (
    <div className="filterbar">
      {(tags.length > 0 || activeView) && (
        <div className="ms" ref={ddRef}>
          <button
            className={`views-btn ${activeView ? "active" : ""}`}
            onClick={() => setOpen((o) => !o)}
            aria-expanded={open}
            aria-haspopup="true"
          >
            <Bookmark size={13} />
            {activeView ? `#${activeView}` : "All items"}
            <ChevronDown size={13} />
          </button>
          {open && (
            <div className="ms-panel" role="radiogroup" aria-label="Tagged views">
              <label className="ms-row ms-all">
                <input
                  type="radio"
                  checked={!activeView}
                  onChange={() => {
                    setView(null);
                    setOpen(false);
                  }}
                />
                All items
              </label>
              <div className="ms-group-label">Views (tags)</div>
              {tags.map(({ tag, count }) => (
                <label key={tag} className="ms-row">
                  <input
                    type="radio"
                    checked={activeView === tag}
                    onChange={() => {
                      setView(tag);
                      setOpen(false);
                    }}
                  />
                  <span className="ms-name">#{tag}</span>
                  <span className="ms-count">{count}</span>
                </label>
              ))}
            </div>
          )}
        </div>
      )}

      {chips.length > 0 && (
        <>
          <SlidersHorizontal size={14} className="fb-icon" />
          <span className="fb-label">Filters</span>
          {chips.map((f) => (
            <button
              key={f.key}
              className={`fb-chip ${filters.has(f.key) ? "active" : ""}`}
              onClick={() => toggleFilter(f.key)}
            >
              {f.label}
              <span className="fb-count">{f.count}</span>
            </button>
          ))}
        </>
      )}

      {depCount > 0 && (
        <button
          className={`fb-chip fb-deps ${showDependencies ? "active" : ""}`}
          onClick={toggleDependencies}
          aria-pressed={showDependencies}
          title={
            showDependencies
              ? "Hide packages that were pulled in by something else"
              : "Show packages that were pulled in by something else"
          }
        >
          {showDependencies ? "Hide dependencies" : "Dependencies"}
          <span className="fb-count">{depCount}</span>
        </button>
      )}

      {(filters.size > 0 || activeView) && (
        <button
          className="fb-clear"
          onClick={() => {
            clearFilters();
            setView(null);
          }}
        >
          <X size={12} /> Clear
        </button>
      )}
    </div>
  );
}
