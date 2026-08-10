import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Handle,
  Position,
  type Node,
  type Edge,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  Boxes,
  Cpu,
  FolderGit2,
  Bot,
  Container,
  Monitor,
  Package,
} from "lucide-react";
import { useStore } from "../store";
import { accentById } from "../lib/appearance";
import { formatBytes } from "../lib/api";
import { passesFilters, itemInView } from "../lib/filters";
import type { Graph as GraphData, GraphNode } from "../lib/types";

const SIZES: Record<string, { w: number; h: number }> = {
  root: { w: 180, h: 62 },
  domain: { w: 190, h: 54 },
  collector: { w: 200, h: 50 },
  item: { w: 170, h: 42 },
};

const DOMAIN_ICON: Record<string, any> = {
  "d:package_manager": Boxes,
  "d:runtime": Cpu,
  "d:project": FolderGit2,
  "d:ai_agent": Bot,
  "d:container": Container,
};

function iconFor(n: GraphNode) {
  if (n.kind === "root") return Monitor;
  if (n.kind === "domain") return DOMAIN_ICON[n.id] ?? Package;
  if (n.kind === "collector") return Package;
  return null;
}

/** Single node renderer; visual style keys off data.kind. */
function Cell({ data }: NodeProps) {
  const n = data.node as GraphNode;
  const selected = data.selected as boolean;
  const expandable = data.expandable as boolean;
  const expanded = data.expanded as boolean;
  const radial = data.radial as boolean;
  const outdated = Boolean((n.meta as any)?.metadata?.outdated);
  const Icon = iconFor(n);

  const handleProps = radial
    ? { position: Position.Top, className: "ghandle ghandle-center" }
    : { position: Position.Left, className: "ghandle" };
  const sourceProps = radial
    ? { position: Position.Top, className: "ghandle ghandle-center" }
    : { position: Position.Right, className: "ghandle" };

  return (
    <div className={`gnode gnode-${n.kind} ${selected ? "gnode-sel" : ""}`}>
      <Handle type="target" {...handleProps} />
      {outdated && <span className="gnode-badge" title="Update available" />}
      <div className="gnode-body">
        {Icon && <Icon size={15} className="gnode-icon" />}
        <div className="gnode-text">
          <div className="gnode-label">{n.label}</div>
          {n.kind !== "item" && n.count > 0 && (
            <div className="gnode-sub">
              {n.count} {n.count === 1 ? "item" : "items"}
              {n.size_bytes ? ` · ${formatBytes(n.size_bytes)}` : ""}
            </div>
          )}
          {n.kind === "item" && (n.meta as any)?.version && (
            <div className="gnode-sub">{(n.meta as any).version}</div>
          )}
        </div>
        {expandable && <div className="gnode-chevron">{expanded ? "−" : "+"}</div>}
      </div>
      <Handle type="source" {...sourceProps} />
    </div>
  );
}

const nodeTypes = { cell: Cell };

/** Radial tree layout: allocate angular sectors weighted by leaf count. */
function radialLayout(
  visible: GraphNode[],
  edges: { source: string; target: string }[],
  rootId: string,
): Map<string, { x: number; y: number }> {
  const children = new Map<string, string[]>();
  for (const e of edges) {
    if (!children.has(e.source)) children.set(e.source, []);
    children.get(e.source)!.push(e.target);
  }
  const depthOf: Record<string, number> = { root: 0, domain: 1, collector: 2, item: 3 };
  const kindById = new Map(visible.map((n) => [n.id, n.kind]));

  const leaves = new Map<string, number>();
  const countLeaves = (id: string): number => {
    const ch = children.get(id) ?? [];
    if (ch.length === 0) {
      leaves.set(id, 1);
      return 1;
    }
    let sum = 0;
    for (const c of ch) sum += countLeaves(c);
    leaves.set(id, sum);
    return sum;
  };
  countLeaves(rootId);

  // Cumulative radius per depth level.
  const RADII = [0, 250, 470, 660];
  const pos = new Map<string, { x: number; y: number }>();

  const place = (id: string, a0: number, a1: number) => {
    const angle = (a0 + a1) / 2;
    const depth = depthOf[kindById.get(id) ?? "item"] ?? 3;
    const r = RADII[Math.min(depth, RADII.length - 1)];
    pos.set(id, { x: r * Math.cos(angle), y: r * Math.sin(angle) });

    const ch = children.get(id) ?? [];
    const total = leaves.get(id) ?? 1;
    // Small angular padding between siblings so nodes breathe.
    const pad = ch.length > 1 ? (a1 - a0) * 0.04 : 0;
    let a = a0 + pad / 2;
    const usable = a1 - a0 - pad;
    for (const c of ch) {
      const frac = (leaves.get(c) ?? 1) / total;
      const span = usable * frac;
      place(c, a, a + span);
      a += span;
    }
  };
  // Start at the top and go clockwise.
  place(rootId, -Math.PI / 2, (3 * Math.PI) / 2);
  return pos;
}

export default function GraphView() {
  const graph = useStore((s) => s.graph) as GraphData | null;
  const select = useStore((s) => s.select);
  const selectedKey = useStore((s) => s.selectedKey);
  const theme = useStore((s) => s.theme);
  const accentId = useStore((s) => s.accentId);
  // React Flow paints the minimap and dot grid to a canvas, so these can't be
  // themed from CSS — they have to be read from the accent directly.
  const accent = accentById(accentId);
  const matrix = accentId === "matrix" && theme === "dark";
  const layout = useStore((s) => s.layout);
  const setLayout = useStore((s) => s.setLayout);
  const filters = useStore((s) => s.filters);
  const activeView = useStore((s) => s.activeView);
  const inventory = useStore((s) => s.inventory);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [nodes, setNodes] = useState<Node[]>([]);
  const [edges, setEdges] = useState<Edge[]>([]);

  const collectorsWithItems = useMemo(() => {
    const s = new Set<string>();
    graph?.nodes.forEach((n) => {
      if (n.kind === "item" && n.parent) s.add(n.parent);
    });
    return s;
  }, [graph]);

  const toggle = useCallback((id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  }, []);

  useEffect(() => {
    if (!graph) {
      setNodes([]);
      setEdges([]);
      return;
    }
    // "Restricted" means either quick-filters or a saved view is active; in
    // that mode we show every matching item and prune empty collectors/domains.
    const filtersActive = filters.size > 0 || activeView !== null;
    const matchItems = new Set<string>();
    const matchCollectors = new Set<string>();
    const matchDomains = new Set<string>();
    if (filtersActive) {
      // Match against the live inventory (always current, incl. just-added tags)
      // rather than graph-node metadata baked in at scan time.
      const matchKeys = new Set(
        (inventory?.items ?? [])
          .filter((i) => passesFilters(i, filters) && itemInView(i, activeView))
          .map((i) => i.item_key),
      );
      for (const n of graph.nodes) {
        if (n.kind === "item" && matchKeys.has(n.id)) {
          matchItems.add(n.id);
          if (n.parent) matchCollectors.add(n.parent);
        }
      }
      for (const n of graph.nodes) {
        if (n.kind === "collector" && matchCollectors.has(n.id) && n.parent) {
          matchDomains.add(n.parent);
        }
      }
    }

    const visible = graph.nodes.filter((n) => {
      if (filtersActive) {
        if (n.kind === "item") return matchItems.has(n.id);
        if (n.kind === "collector") return matchCollectors.has(n.id);
        if (n.kind === "domain") return matchDomains.has(n.id);
        return true; // root
      }
      if (n.kind === "item") return n.parent && expanded.has(n.parent);
      return true;
    });
    const visibleIds = new Set(visible.map((n) => n.id));
    const vEdges = graph.edges.filter(
      (e) => visibleIds.has(e.source) && visibleIds.has(e.target),
    );

    const buildNodes = (pos: Map<string, { x: number; y: number }>) => {
      const radial = layout === "radial";
      const rfNodes: Node[] = visible.map((n) => {
        const size = SIZES[n.kind] ?? SIZES.item;
        const p = pos.get(n.id) ?? { x: 0, y: 0 };
        return {
          id: n.id,
          type: "cell",
          // Radial positions are node centers; convert to top-left.
          position: radial ? { x: p.x - size.w / 2, y: p.y - size.h / 2 } : p,
          data: {
            node: n,
            selected: selectedKey === n.id,
            expandable: n.kind === "collector" && collectorsWithItems.has(n.id),
            expanded: expanded.has(n.id),
            radial,
          },
          draggable: false,
        };
      });
      const rfEdges: Edge[] = vEdges.map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        type: radial ? "straight" : "smoothstep",
      }));
      setNodes(rfNodes);
      setEdges(rfEdges);
    };

    if (layout === "radial") {
      buildNodes(radialLayout(visible, vEdges, "root"));
      return;
    }

    // Tree layout via elk (left-to-right layered).
    const layoutGraph = {
      id: "root",
      layoutOptions: {
        "elk.algorithm": "layered",
        "elk.direction": "RIGHT",
        "elk.layered.spacing.nodeNodeBetweenLayers": "90",
        "elk.spacing.nodeNode": "16",
      },
      children: visible.map((n) => ({
        id: n.id,
        width: (SIZES[n.kind] ?? SIZES.item).w,
        height: (SIZES[n.kind] ?? SIZES.item).h,
      })),
      edges: vEdges.map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] })),
    };
    let cancelled = false;
    import("elkjs/lib/elk.bundled.js").then(({ default: ELK }) => {
      const elk = new ELK();
      return elk.layout(layoutGraph as any);
    }).then((res) => {
        if (cancelled) return;
        const pos = new Map<string, { x: number; y: number }>();
        (res.children ?? []).forEach((c: any) => pos.set(c.id, { x: c.x, y: c.y }));
        buildNodes(pos);
      });
    return () => {
      cancelled = true;
    };
  }, [graph, expanded, selectedKey, collectorsWithItems, layout, filters, activeView, inventory]);

  const onNodeClick = useCallback(
    (_: any, node: Node) => {
      const n = node.data.node as GraphNode;
      select(n.id);
      if (n.kind === "collector" && collectorsWithItems.has(n.id)) {
        toggle(n.id);
      }
    },
    [select, toggle, collectorsWithItems],
  );

  if (!graph) {
    return <div className="empty-hint">No scan yet — hit “Scan” to map your machine.</div>;
  }

  return (
    <div className="graph-wrap">
      <div className="graph-toolbar">
        <button
          className={layout === "radial" ? "active" : ""}
          onClick={() => setLayout("radial")}
        >
          Radial
        </button>
        <button
          className={layout === "tree" ? "active" : ""}
          onClick={() => setLayout("tree")}
        >
          Tree
        </button>
      </div>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodeClick={onNodeClick}
        fitView
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        nodesConnectable={false}
        nodesDraggable={false}
      >
        <Background
          color={matrix ? "#12401a" : theme === "light" ? "#d8dce4" : "#2a2f3a"}
          gap={22}
        />
        <Controls showInteractive={false} />
        <MiniMap
          pannable
          zoomable
          bgColor={matrix ? "#040a04" : theme === "light" ? "#eef1f6" : "#12151d"}
          nodeColor={(n) => {
            const k = (n.data?.node as GraphNode)?.kind;
            return k === "domain" ? accent.color : k === "collector" ? accent.light : accent.edge;
          }}
          maskColor={
            matrix
              ? "rgba(0,0,0,0.65)"
              : theme === "light"
                ? "rgba(230,233,240,0.6)"
                : "rgba(6,8,12,0.6)"
          }
        />
      </ReactFlow>
    </div>
  );
}
