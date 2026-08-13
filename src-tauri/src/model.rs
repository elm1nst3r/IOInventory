use serde::{Deserialize, Serialize};

/// Top-level domains an item can belong to. These become the first ring of
/// nodes in the architecture graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Domain {
    #[serde(rename = "package_manager")]
    PackageManager,
    #[serde(rename = "runtime")]
    Runtime,
    #[serde(rename = "project")]
    Project,
    #[serde(rename = "ai_agent")]
    AiAgent,
    #[serde(rename = "container")]
    Container,
    #[serde(rename = "application")]
    Application,
}

impl Domain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Domain::PackageManager => "package_manager",
            Domain::Runtime => "runtime",
            Domain::Project => "project",
            Domain::AiAgent => "ai_agent",
            Domain::Container => "container",
            Domain::Application => "application",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Domain::PackageManager => "Package Managers",
            Domain::Runtime => "Runtimes",
            Domain::Project => "Projects",
            Domain::AiAgent => "AI & Agents",
            Domain::Container => "Containers",
            Domain::Application => "Applications",
        }
    }
}

/// A single inventoried thing: a package, a repo, a skill, a model, an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Stable fingerprint used to attach notes across re-scans.
    pub item_key: String,
    pub domain: Domain,
    /// Collector that produced this (e.g. "homebrew", "npm", "claude").
    pub collector: String,
    pub name: String,
    pub version: Option<String>,
    pub source_path: Option<String>,
    pub size_bytes: Option<i64>,
    /// Free-form structured extras (kind, stacks, launch_cmd, remote, ...).
    pub metadata: serde_json::Value,
    /// User note ("why used"), merged in from the notes table on read.
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
    /// User-assigned tags, merged in from the tags table on read.
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Item {
    pub fn new(domain: Domain, collector: &str, name: impl Into<String>) -> Item {
        let name = name.into();
        let item_key = format!("{}:{}:{}", domain.as_str(), collector, name);
        Item {
            item_key,
            domain,
            collector: collector.to_string(),
            name,
            version: None,
            source_path: None,
            size_bytes: None,
            metadata: serde_json::json!({}),
            note: None,
            why: None,
            tags: Vec::new(),
        }
    }

    /// Disambiguate the fingerprint with a stable extra part (e.g. a config
    /// scope or filesystem path) so two items that share a name but are
    /// genuinely distinct don't collide on `item_key`.
    pub fn keyed(mut self, extra: &str) -> Self {
        self.item_key = format!("{}#{}", self.item_key, extra);
        self
    }

    pub fn version(mut self, v: impl Into<String>) -> Self {
        self.version = Some(v.into());
        self
    }
    pub fn path(mut self, p: impl Into<String>) -> Self {
        self.source_path = Some(p.into());
        self
    }
    pub fn size(mut self, b: i64) -> Self {
        self.size_bytes = Some(b);
        self
    }
    pub fn meta(mut self, v: serde_json::Value) -> Self {
        self.metadata = v;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanWarning {
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanInfo {
    pub id: i64,
    pub started_at: String,
    pub finished_at: String,
    pub host: String,
    pub os: String,
    pub duration_ms: i64,
    pub item_count: i64,
    #[serde(default)]
    pub warnings: Vec<ScanWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub scan: ScanInfo,
    pub items: Vec<Item>,
}

// ---- Graph model consumed by the React Flow frontend ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    /// "root" | "domain" | "collector" | "item"
    pub kind: String,
    pub label: String,
    pub parent: Option<String>,
    pub count: i64,
    pub size_bytes: Option<i64>,
    /// Extra info surfaced on hover / in the detail panel.
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

// ---- Cleanup actions ----

// ---- Snapshots & diff ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub host: String,
    pub os: String,
    pub item_count: i64,
    /// "scan" (saved from a live scan) or "import" (loaded from a file).
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffItem {
    pub name: String,
    pub collector: String,
    pub domain: Domain,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffChange {
    pub name: String,
    pub collector: String,
    pub domain: Domain,
    pub old_version: Option<String>,
    pub new_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff {
    pub base_label: String,
    pub target_label: String,
    /// In target but not base (installed since the snapshot).
    pub added: Vec<DiffItem>,
    /// In base but not target (removed since the snapshot).
    pub removed: Vec<DiffItem>,
    /// Same item, different version.
    pub changed: Vec<DiffChange>,
    pub unchanged: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupAction {
    pub id: String,
    pub title: String,
    pub description: String,
    /// "update" or "cleanup" — groups the action in the Utilities view.
    pub category: String,
    /// Human-readable command that will run.
    pub command: String,
    /// Whether this collector/tool is available on this machine.
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPreview {
    pub id: String,
    pub command: String,
    /// Output of the dry-run / size probe.
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    pub id: String,
    pub command: String,
    pub output: String,
    pub success: bool,
}
