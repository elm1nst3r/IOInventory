//! Cross-agent linking. Collectors record which agent a capability belongs to
//! in its `agent` metadata; this pass turns that into an `agents` list.
//!
//! The interesting case is MCP servers. The same server is routinely configured
//! for several agents at once, and read naively that looks like three unrelated
//! rows called `github`. Here they become one row that knows it is shared.

use crate::model::Item;
use std::collections::BTreeMap;

/// Agents in the order a merged row prefers to keep its identity. The survivor
/// keeps its `item_key`, and notes are attached to item keys — so preferring
/// Claude keeps notes on the row a user of *this* app most likely wrote them
/// against. Notes on the rows folded into it do not survive the merge.
const PRIORITY: &[&str] = &["claude", "codex", "gemini"];

pub fn link(items: &mut Vec<Item>) {
    merge_shared_mcp(items);

    // Everything else that names an agent gets a one-element list, so the UI
    // can read `agents` uniformly instead of special-casing the unshared case.
    for item in items.iter_mut() {
        if item.metadata.get("agents").is_some() {
            continue;
        }
        if let Some(agent) = agent_of(item) {
            set_meta(item, "agents", serde_json::json!([agent]));
        }
    }
}

/// Fold user-scope MCP servers that several agents configure into one row.
fn merge_shared_mcp(items: &mut Vec<Item>) {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, item) in items.iter().enumerate() {
        // A project-scoped server is scoped to that project, not shared across
        // agents, so it stays its own row.
        if item.collector != "mcp-server" || is_project_scoped(item) {
            continue;
        }
        groups
            .entry(server_name(item).to_lowercase())
            .or_default()
            .push(idx);
    }

    let mut discard: Vec<usize> = Vec::new();
    for idxs in groups.into_values() {
        if idxs.len() < 2 {
            continue;
        }

        // Read the whole group up front: the survivor can't be borrowed
        // mutably while the rows folding into it are still being read.
        let mut agents: Vec<String> = Vec::new();
        let mut sources: Vec<serde_json::Value> = Vec::new();
        for &i in &idxs {
            let Some(agent) = agent_of(&items[i]) else {
                continue;
            };
            if !agents.contains(&agent) {
                agents.push(agent.clone());
            }
            sources.push(serde_json::json!({
                "agent": agent,
                "transport": items[i].metadata.get("transport"),
                "command": items[i].metadata.get("command"),
            }));
        }
        // Two rows from one agent are duplicate config, not sharing.
        if agents.len() < 2 {
            continue;
        }

        let primary = *idxs
            .iter()
            .min_by_key(|&&i| rank(&items[i]))
            .expect("group is non-empty");
        agents.sort();
        set_meta(&mut items[primary], "agents", serde_json::json!(agents));
        set_meta(&mut items[primary], "sources", serde_json::Value::Array(sources));
        set_meta(&mut items[primary], "shared", serde_json::json!(true));
        discard.extend(idxs.into_iter().filter(|&i| i != primary));
    }

    // Highest index first, so each removal leaves the rest of the indices valid.
    discard.sort_unstable();
    discard.dedup();
    for i in discard.into_iter().rev() {
        items.remove(i);
    }
}

fn agent_of(item: &Item) -> Option<String> {
    item.metadata
        .get("agent")
        .and_then(|a| a.as_str())
        .filter(|a| !a.is_empty())
        .map(str::to_string)
}

/// The server's own name, which is what identifies it across agents — the item
/// name can carry a scope suffix for display.
fn server_name(item: &Item) -> String {
    item.metadata
        .get("server")
        .and_then(|s| s.as_str())
        .unwrap_or(&item.name)
        .to_string()
}

fn is_project_scoped(item: &Item) -> bool {
    item.metadata
        .get("scope")
        .and_then(|s| s.as_str())
        .is_some_and(|s| s.starts_with("project:"))
}

fn rank(item: &Item) -> usize {
    let agent = agent_of(item).unwrap_or_default();
    PRIORITY
        .iter()
        .position(|p| *p == agent)
        .unwrap_or(PRIORITY.len())
}

/// Insert into an item's metadata, replacing a non-object blob rather than
/// panicking the way `metadata[key] = …` would.
fn set_meta(item: &mut Item, key: &str, value: serde_json::Value) {
    if !item.metadata.is_object() {
        item.metadata = serde_json::json!({});
    }
    if let Some(obj) = item.metadata.as_object_mut() {
        obj.insert(key.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Domain;

    fn mcp(name: &str, agent: &str, scope: &str) -> Item {
        Item::new(Domain::AiAgent, "mcp-server", name)
            .keyed(agent)
            .meta(serde_json::json!({
                "agent": agent,
                "server": name,
                "scope": scope,
                "command": "npx",
            }))
    }

    fn agents_of(item: &Item) -> Vec<String> {
        item.metadata
            .get("agents")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    }

    /// The same server across agents becomes one row listing all of them, and
    /// keeps the highest-priority agent's key so its notes survive.
    #[test]
    fn shared_servers_merge_into_one_row() {
        let claude_key = mcp("github", "claude", "global").item_key.clone();
        let mut items = vec![
            mcp("github", "codex", "user"),
            mcp("github", "claude", "global"),
            mcp("github", "gemini", "user"),
            mcp("linear", "codex", "user"),
        ];
        link(&mut items);

        let github: Vec<&Item> = items.iter().filter(|i| i.name == "github").collect();
        assert_eq!(github.len(), 1, "shared server should collapse to one row");
        assert_eq!(agents_of(github[0]), ["claude", "codex", "gemini"].map(String::from));
        assert_eq!(
            github[0].item_key, claude_key,
            "merged row must keep the Claude key so notes stay attached"
        );
        assert_eq!(
            github[0].metadata["sources"].as_array().map(|s| s.len()),
            Some(3),
            "each agent's own config should survive on the merged row"
        );

        // An unshared server keeps its own row, and still reports its agent.
        let linear: Vec<&Item> = items.iter().filter(|i| i.name == "linear").collect();
        assert_eq!(linear.len(), 1);
        assert_eq!(agents_of(linear[0]), ["codex".to_string()]);
        assert!(linear[0].metadata.get("shared").is_none());
        println!("shared_servers_merge_into_one_row OK — {} rows left", items.len());
    }

    /// A project-scoped server is scoped to that project, not shared across
    /// agents — merging it into the user-scope row would erase that.
    #[test]
    fn project_scope_is_never_merged() {
        let mut items = vec![
            mcp("github", "claude", "global"),
            mcp("github", "claude", "project:api"),
        ];
        link(&mut items);
        assert_eq!(items.len(), 2, "project-scoped server must stay separate");
        assert!(items.iter().all(|i| i.metadata.get("shared").is_none()));
        println!("project_scope_is_never_merged OK");
    }

    /// Two rows for one agent are duplicate config, not a sharing relationship.
    #[test]
    fn one_agent_twice_is_not_shared() {
        let mut items = vec![mcp("github", "claude", "global"), mcp("github", "claude", "user")];
        link(&mut items);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.metadata.get("shared").is_none()));
        println!("one_agent_twice_is_not_shared OK");
    }
}
