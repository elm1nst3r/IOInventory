import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Search, X, Hash, Package } from "lucide-react";
import { useStore } from "../store";
import { formatBytes } from "../lib/api";
import { isDependency, itemInView, passesFilters } from "../lib/filters";
import { buildSuggestions, type Suggestion } from "../lib/search";
import { collectorLabel } from "../lib/labels";

/**
 * Inventory search with autocomplete, shared by the graph and list tabs.
 *
 * The query is a *scope*, not a jump: typing filters both views live, and the
 * dropdown is a shortcut to one result rather than the only way to see them.
 * That's why picking an item keeps the text — clearing it would collapse the
 * graph you just narrowed.
 */
export default function SearchBox() {
  const search = useStore((s) => s.search);
  const setSearch = useStore((s) => s.setSearch);
  const inventory = useStore((s) => s.inventory);
  const filters = useStore((s) => s.filters);
  const activeView = useStore((s) => s.activeView);
  const showDependencies = useStore((s) => s.showDependencies);
  const select = useStore((s) => s.select);
  const setView = useStore((s) => s.setView);

  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const boxRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const q = search.trim().toLowerCase();

  // Search honours the active view and quick-filters — "across whatever is
  // filtered". Dependencies are the one exception: they're hidden to cut noise
  // while browsing, but you shouldn't have to know a package is transitive to
  // be able to find it by name.
  const pool = useMemo(() => {
    const items = inventory?.items ?? [];
    return items.filter(
      (i) =>
        itemInView(i, activeView) &&
        passesFilters(i, filters) &&
        (showDependencies || q.length > 0 || !isDependency(i)),
    );
  }, [inventory, activeView, filters, showDependencies, q]);

  const suggestions = useMemo(() => buildSuggestions(pool, q), [pool, q]);

  useEffect(() => setActive(0), [q]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // ⌘K / ⌘F focus the box from anywhere.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "k" || e.key === "f")) {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const apply = useCallback(
    (s: Suggestion) => {
      setOpen(false);
      if (s.kind === "item") {
        select(s.item.item_key);
      } else if (s.kind === "collector") {
        select(s.nodeId);
      } else {
        // A view replaces the query as the scope, so the text goes.
        setView(s.tag);
        setSearch("");
      }
    },
    [select, setView, setSearch],
  );

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Escape") {
      open ? setOpen(false) : setSearch("");
      return;
    }
    if (!open || suggestions.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((i) => (i + 1) % suggestions.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((i) => (i - 1 + suggestions.length) % suggestions.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      apply(suggestions[Math.min(active, suggestions.length - 1)]);
    }
  }

  const scope = activeView
    ? `#${activeView}`
    : filters.size > 0
      ? "the active filters"
      : null;

  return (
    <div className="searchbox-wrap" ref={boxRef}>
      <div className="searchbox">
        <Search size={14} />
        <input
          ref={inputRef}
          aria-label="Search inventory"
          placeholder="Search packages, repos, skills…  ⌘K"
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={onKeyDown}
          role="combobox"
          aria-expanded={open && suggestions.length > 0}
          aria-controls="search-suggestions"
          aria-autocomplete="list"
        />
        {search && (
          <button className="sb-clear" aria-label="Clear search" onClick={() => setSearch("")}>
            <X size={13} />
          </button>
        )}
      </div>

      {open && q.length > 0 && (
        <div className="sb-panel" id="search-suggestions" role="listbox">
          {suggestions.length === 0 ? (
            <div className="sb-empty">
              Nothing matches “{search.trim()}”
              {scope ? <> within {scope}</> : null}.
            </div>
          ) : (
            suggestions.map((s, i) => (
              <button
                key={s.key}
                role="option"
                aria-selected={i === active}
                className={`sb-row ${i === active ? "active" : ""}`}
                onMouseEnter={() => setActive(i)}
                onClick={() => apply(s)}
              >
                {s.kind === "item" ? (
                  <>
                    <span className="sb-name">{s.item.name}</span>
                    {s.item.version && <span className="sb-ver">{s.item.version}</span>}
                    <span className="sb-meta">{collectorLabel(s.item.collector)}</span>
                    {s.item.size_bytes ? (
                      <span className="sb-size">{formatBytes(s.item.size_bytes)}</span>
                    ) : null}
                  </>
                ) : s.kind === "tag" ? (
                  <>
                    <Hash size={12} className="sb-icon" />
                    <span className="sb-name">{s.tag}</span>
                    <span className="sb-meta">switch to this view · {s.count}</span>
                  </>
                ) : (
                  <>
                    <Package size={12} className="sb-icon" />
                    <span className="sb-name">{collectorLabel(s.collector)}</span>
                    <span className="sb-meta">show this group · {s.count}</span>
                  </>
                )}
              </button>
            ))
          )}
          {scope && suggestions.length > 0 && (
            <div className="sb-scope">Searching within {scope}</div>
          )}
        </div>
      )}
    </div>
  );
}
