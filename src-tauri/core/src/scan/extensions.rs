//! Editor extensions for VS Code and its forks.
//!
//! Each editor keeps a manifest of what's installed in
//! `~/.<editor>/extensions/extensions.json`, so this reads a file instead of
//! spawning the editor's CLI — fast, and it works when the CLI isn't on PATH.
//! The manifest lists only current versions; the stale version folders an
//! update leaves behind beside it are ignored.

use super::util;
use crate::model::{Domain, Item};

/// (collector, display name, folder under ~).
const EDITORS: &[(&str, &str, &str)] = &[
    ("vscode-extension", "VS Code", ".vscode"),
    ("cursor-extension", "Cursor", ".cursor"),
    ("windsurf-extension", "Windsurf", ".windsurf"),
];

pub async fn collect() -> Vec<Item> {
    let home = util::home();
    let mut items = Vec::new();
    for (collector, editor, dir) in EDITORS {
        let manifest = home.join(dir).join("extensions").join("extensions.json");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        items.extend(parse(&text, collector, editor));
    }
    items
}

fn parse(text: &str, collector: &str, editor: &str) -> Vec<Item> {
    let Ok(serde_json::Value::Array(entries)) = serde_json::from_str(text) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for e in entries {
        let Some(id) = e.pointer("/identifier/id").and_then(|v| v.as_str()) else {
            continue;
        };
        let mut item = Item::new(Domain::PackageManager, collector, id).meta(serde_json::json!({
            "editor": editor,
            "publisher": e.pointer("/metadata/publisherDisplayName"),
            "pinned": e.pointer("/metadata/pinned"),
        }));
        if let Some(v) = e.get("version").and_then(|v| v.as_str()) {
            item = item.version(v);
        }
        if let Some(p) = e.pointer("/location/path").and_then(|v| v.as_str()) {
            item = item.path(p);
        }
        items.push(item);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses() {
        let items = parse(
            r#"[{"identifier":{"id":"ms-azuretools.vscode-containers"},"version":"2.5.3",
                 "location":{"path":"/x/ms-azuretools.vscode-containers-2.5.3"},
                 "metadata":{"publisherDisplayName":"Microsoft"}},
                {"no_identifier":true}]"#,
            "vscode-extension",
            "VS Code",
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "ms-azuretools.vscode-containers");
        assert_eq!(items[0].version.as_deref(), Some("2.5.3"));
        assert_eq!(items[0].metadata["publisher"], "Microsoft");
        println!("manifest_parses OK");
    }
}
