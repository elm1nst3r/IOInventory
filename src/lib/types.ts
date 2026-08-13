export type Domain =
  | "package_manager"
  | "runtime"
  | "project"
  | "ai_agent"
  | "container"
  | "application";

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
  warnings: { source: string; message: string }[];
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
  install?: string | null;
  available: boolean;
  /** Why nothing is offered, or a caveat about what will happen. */
  note?: string | null;
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

/** One toggleable part of the machine to scan. */
export interface ScanSource {
  id: string;
  label: string;
  description: string;
  domain: Domain;
}

export interface Settings {
  /** Source ids switched off. Empty = scan everything. */
  disabled_sources: string[];
  /** Empty = use the auto-detected workspace roots. */
  roots: string[];
  /** Whether the MCP server exposes install/update/uninstall and cleanups. */
  mcp_allow_write: boolean;
  /** Whether the app looks for a new release on launch. On by default. */
  auto_update_check: boolean;
}

export interface McpInfo {
  binary_path?: string | null;
  available: boolean;
  db_path: string;
  config_json: string;
  cli_command: string;
  server_name: string;
  version: string;
}

export const DOMAIN_LABELS: Record<Domain, string> = {
  package_manager: "Package Managers",
  runtime: "Runtimes",
  project: "Projects",
  ai_agent: "AI & Agents",
  container: "Containers",
  application: "Applications",
};
