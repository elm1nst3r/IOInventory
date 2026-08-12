// Matching and ranking for the inventory search.
//
// One place so the list, the graph and the autocomplete can never disagree
// about what "matches". A query matches a lot of fields — name, path, tags,
// notes, detected stacks — because you rarely remember which one holds the
// word you're thinking of; ranking is what keeps that from being noise.

import type { Item } from "./types";

/** Where a query hit, worst to best. Only the strongest hit counts. */
const enum Score {
  None = 0,
  Note = 20,
  Path = 25,
  Version = 35,
  Stack = 45,
  Collector = 55,
  Tag = 65,
  NameContains = 80,
  NamePrefix = 100,
  NameExact = 120,
}

function has(haystack: string | null | undefined, q: string): boolean {
  return !!haystack && haystack.toLowerCase().includes(q);
}

/**
 * How well an item matches an already-lowercased query. 0 means no match.
 *
 * Name hits beat metadata hits so that typing "node" surfaces the Node.js
 * runtime before the dozen packages that merely mention it in a path.
 */
export function matchScore(item: Item, q: string): number {
  if (!q) return Score.None;
  const name = item.name.toLowerCase();
  if (name === q) return Score.NameExact;
  if (name.startsWith(q)) return Score.NamePrefix;
  if (name.includes(q)) return Score.NameContains;

  if ((item.tags ?? []).some((t) => t.toLowerCase().includes(q))) return Score.Tag;
  if (item.collector.toLowerCase().includes(q)) return Score.Collector;

  const stacks: string[] = item.metadata?.stacks ?? [];
  if (stacks.some((s) => typeof s === "string" && s.toLowerCase().includes(q))) return Score.Stack;
  if (has(item.metadata?.description, q)) return Score.Stack;

  if (has(item.version, q)) return Score.Version;
  if (has(item.source_path, q)) return Score.Path;
  if (has(item.why, q) || has(item.note, q)) return Score.Note;
  return Score.None;
}

export function matchesQuery(item: Item, q: string): boolean {
  return matchScore(item, q) > Score.None;
}

/** Best matches first; ties go to the shorter name, then alphabetically. */
export function rankMatches(items: Item[], q: string): Item[] {
  return items
    .map((item) => ({ item, score: matchScore(item, q) }))
    .filter((m) => m.score > Score.None)
    .sort(
      (a, b) =>
        b.score - a.score ||
        a.item.name.length - b.item.name.length ||
        a.item.name.localeCompare(b.item.name),
    )
    .map((m) => m.item);
}

// ------------------------------------------------------------- autocomplete

export type Suggestion =
  | { kind: "item"; key: string; item: Item }
  /** Switch to a saved tag view. */
  | { kind: "tag"; key: string; tag: string; count: number }
  /** Select a collector's aggregate node, which lists everything under it. */
  | { kind: "collector"; key: string; collector: string; nodeId: string; count: number };

const MAX_ITEMS = 7;
const MAX_GROUPS = 3;

/**
 * Ranked items, plus the tags and collectors the query names. Those two are
 * navigation rather than results — "#favorite" and "homebrew" are things you
 * want to *go to*, not 40 rows you then have to read.
 *
 * `pool` is the already-scoped item set, so suggestions never offer something
 * the active view or filters would hide.
 */
export function buildSuggestions(pool: Item[], q: string): Suggestion[] {
  if (!q) return [];
  const out: Suggestion[] = rankMatches(pool, q)
    .slice(0, MAX_ITEMS)
    .map((item) => ({ kind: "item", key: `i:${item.item_key}`, item }));

  const tags = new Map<string, number>();
  const collectors = new Map<string, { id: string; n: number }>();
  for (const i of pool) {
    for (const t of i.tags ?? []) {
      if (t.toLowerCase().includes(q)) tags.set(t, (tags.get(t) ?? 0) + 1);
    }
    if (i.collector.toLowerCase().includes(q)) {
      const e = collectors.get(i.collector) ?? { id: `c:${i.domain}:${i.collector}`, n: 0 };
      e.n += 1;
      collectors.set(i.collector, e);
    }
  }

  for (const [tag, count] of [...tags].sort((a, b) => b[1] - a[1]).slice(0, MAX_GROUPS)) {
    out.push({ kind: "tag", key: `t:${tag}`, tag, count });
  }
  for (const [collector, { id, n }] of [...collectors]
    .sort((a, b) => b[1].n - a[1].n)
    .slice(0, MAX_GROUPS)) {
    out.push({ kind: "collector", key: `c:${collector}`, collector, nodeId: id, count: n });
  }
  return out;
}
