import type { Item } from "./types";

export type FilterKey = "outdated" | "deprecated" | "noted";

export const FILTERS: { key: FilterKey; label: string }[] = [
  { key: "outdated", label: "Update available" },
  { key: "deprecated", label: "Deprecated" },
  { key: "noted", label: "Noted" },
];

export function itemHasFlag(item: Item, key: FilterKey): boolean {
  switch (key) {
    case "outdated":
      return !!item.metadata?.outdated;
    case "deprecated":
      return !!item.metadata?.deprecated;
    case "noted":
      return !!(item.why && item.why.length);
  }
}

/** OR semantics: with filters active, an item passes if it matches any of them. */
export function passesFilters(item: Item, active: Set<string>): boolean {
  if (active.size === 0) return true;
  for (const k of active) {
    if (itemHasFlag(item, k as FilterKey)) return true;
  }
  return false;
}

/** Same test, from a graph item node's `meta` payload. */
export function metaPasses(meta: any, active: Set<string>): boolean {
  if (active.size === 0) return true;
  const m = meta?.metadata ?? {};
  for (const k of active) {
    if (k === "outdated" && m.outdated) return true;
    if (k === "deprecated" && m.deprecated) return true;
    if (k === "noted" && meta?.why) return true;
  }
  return false;
}

export function countMatching(items: Item[], key: FilterKey): number {
  return items.reduce((n, i) => n + (itemHasFlag(i, key) ? 1 : 0), 0);
}

// ---- Tags / saved views ----

export function itemInView(item: Item, view: string | null): boolean {
  return !view || !!item.tags?.includes(view);
}

/** View test from a graph item node's `meta` payload. */
export function metaInView(meta: any, view: string | null): boolean {
  return !view || (Array.isArray(meta?.tags) && meta.tags.includes(view));
}

/** Distinct tags across the inventory with item counts, sorted by name. */
export function allTags(items: Item[]): { tag: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const i of items) {
    for (const t of i.tags ?? []) counts.set(t, (counts.get(t) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([tag, count]) => ({ tag, count }))
    .sort((a, b) => a.tag.localeCompare(b.tag));
}
