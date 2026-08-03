use crate::model::{Domain, Graph, GraphEdge, GraphNode, Inventory};
use std::collections::BTreeMap;

/// Build the architecture graph: This Mac → domains → collectors → items.
pub fn build(inv: &Inventory) -> Graph {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    let root_id = "root".to_string();
    nodes.push(GraphNode {
        id: root_id.clone(),
        kind: "root".into(),
        label: inv.scan.host.clone(),
        parent: None,
        count: inv.items.len() as i64,
        size_bytes: None,
        meta: serde_json::json!({ "os": inv.scan.os }),
    });

    // Group items: domain -> collector -> item indices.
    let mut by_domain: BTreeMap<String, BTreeMap<String, Vec<usize>>> = BTreeMap::new();
    for (idx, item) in inv.items.iter().enumerate() {
        by_domain
            .entry(item.domain.as_str().to_string())
            .or_default()
            .entry(item.collector.clone())
            .or_default()
            .push(idx);
    }

    // Keep domains in a friendly, stable order.
    let order = [
        Domain::PackageManager,
        Domain::Runtime,
        Domain::Project,
        Domain::AiAgent,
        Domain::Container,
    ];

    for domain in order {
        let dkey = domain.as_str();
        let Some(collectors) = by_domain.get(dkey) else {
            continue;
        };
        let domain_id = format!("d:{dkey}");
        let domain_count: i64 = collectors.values().map(|v| v.len() as i64).sum();
        let domain_size = sum_size(inv, collectors.values().flatten().copied());
        nodes.push(GraphNode {
            id: domain_id.clone(),
            kind: "domain".into(),
            label: domain.label().into(),
            parent: Some(root_id.clone()),
            count: domain_count,
            size_bytes: domain_size,
            meta: serde_json::json!({}),
        });
        edges.push(edge(&root_id, &domain_id));

        for (collector, idxs) in collectors {
            let collector_id = format!("c:{dkey}:{collector}");
            let csize = sum_size(inv, idxs.iter().copied());
            nodes.push(GraphNode {
                id: collector_id.clone(),
                kind: "collector".into(),
                label: pretty_collector(collector),
                parent: Some(domain_id.clone()),
                count: idxs.len() as i64,
                size_bytes: csize,
                meta: serde_json::json!({ "collector": collector }),
            });
            edges.push(edge(&domain_id, &collector_id));

            for &idx in idxs {
                let item = &inv.items[idx];
                nodes.push(GraphNode {
                    id: item.item_key.clone(),
                    kind: "item".into(),
                    label: item.name.clone(),
                    parent: Some(collector_id.clone()),
                    count: 0,
                    size_bytes: item.size_bytes,
                    meta: serde_json::json!({
                        "version": item.version,
                        "path": item.source_path,
                        "metadata": item.metadata,
                        "note": item.note,
                        "why": item.why,
                        "tags": item.tags,
                        "collector": item.collector,
                    }),
                });
                edges.push(edge(&collector_id, &item.item_key));
            }
        }
    }

    Graph { nodes, edges }
}

fn sum_size(inv: &Inventory, idxs: impl Iterator<Item = usize>) -> Option<i64> {
    let mut total = 0i64;
    let mut any = false;
    for i in idxs {
        if let Some(b) = inv.items[i].size_bytes {
            total += b;
            any = true;
        }
    }
    if any {
        Some(total)
    } else {
        None
    }
}

fn edge(source: &str, target: &str) -> GraphEdge {
    GraphEdge {
        id: format!("{source}->{target}"),
        source: source.to_string(),
        target: target.to_string(),
    }
}

/// Human-friendly labels for collector node headers.
fn pretty_collector(c: &str) -> String {
    match c {
        "homebrew" => "Homebrew",
        "homebrew-cask" => "Homebrew Casks",
        "npm" => "npm (global)",
        "pnpm" => "pnpm (global)",
        "pip" => "pip",
        "pipx" => "pipx",
        "cargo" => "Cargo",
        "gem" => "RubyGems",
        "runtime" => "Language Runtimes",
        "rustup-toolchain" => "Rust Toolchains",
        "version-manager" => "Version Managers",
        "git" => "Git Repositories",
        "docker-image" => "Docker Images",
        "docker-container" => "Docker Containers",
        "claude-skill" => "Claude Skills",
        "claude-plugin" => "Claude Plugins",
        "claude-command" => "Claude Commands",
        "claude-agent" => "Claude Agents",
        "mcp-server" => "MCP Servers",
        "ai-app" => "AI Apps & IDEs",
        "ai-cli" => "AI CLIs & Agents",
        "ollama" => "Ollama Models",
        "huggingface" => "Hugging Face Cache",
        "python-ai-lib" => "Python AI Libraries",
        other => other,
    }
    .to_string()
}
