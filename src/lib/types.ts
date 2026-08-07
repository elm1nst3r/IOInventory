export type Domain =
  | "package_manager"
  | "runtime"
  | "project"
  | "ai_agent"
  | "container";

export interface Item {
  item_key: string;
  domain: Domain;
  collector: string;
  name: string;
  version?: string | null;
  source_path?: string | null;
  size_bytes?: number | null;
  metadata: any;
  note?: string | null;
  why?: string | null;
  tags?: string[];
}

export interface ScanInfo {
  id: number;
  started_at: string;
  finished_at: string;
  host: string;
  os: string;
  duration_ms: number;
  item_count: number;
}

export interface Inventory {
  scan: ScanInfo;
  items: Item[];
}

export interface GraphNode {
  id: string;
  kind: "root" | "domain" | "collector" | "item";
  label: string;
  parent?: string | null;
  count: number;
  size_bytes?: number | null;
  meta: any;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface CleanupAction {
  id: string;
  title: string;
  description: string;
  category: string;
  command: string;
  available: boolean;
}

export interface CleanupPreview {
  id: string;
  command: string;
  output: string;
}

export interface CleanupResult {
  id: string;
  command: string;
  output: string;
  success: boolean;
}

export interface ActionInfo {
  update?: string | null;
  delete?: string | null;
  available: boolean;
}

export interface ActionResult {
  command: string;
  output: string;
  success: boolean;
}

export interface SnapshotMeta {
  id: number;
  name: string;
  created_at: string;
  host: string;
  os: string;
  item_count: number;
  source: string; // "scan" | "import"
}

export interface DiffItem {
  name: string;
  collector: string;
  domain: Domain;
  version?: string | null;
}

export interface DiffChange {
  name: string;
  collector: string;
  domain: Domain;
  old_version?: string | null;
  new_version?: string | null;
}

export interface Diff {
  base_label: string;
  target_label: string;
  added: DiffItem[];
  removed: DiffItem[];
  changed: DiffChange[];
  unchanged: number;
}

export interface Enrichment {
  description?: string | null;
  homepage?: string | null;
  latest_version?: string | null;
  installed_version?: string | null;
  outdated?: boolean | null;
  installed_at?: string | null;
}

export const DOMAIN_LABELS: Record<Domain, string> = {
  package_manager: "Package Managers",
  runtime: "Runtimes",
  project: "Projects",
  ai_agent: "AI & Agents",
  container: "Containers",
};
