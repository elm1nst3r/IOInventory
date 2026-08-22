use super::util;
use crate::model::{Domain, Item};

/// Hugging Face model cache (~/.cache/huggingface/hub). Each cached repo lives
/// in a `models--org--name` directory.
pub async fn collect() -> Vec<Item> {
    let hub = util::home().join(".cache/huggingface/hub");
    if !hub.exists() {
        return Vec::new();
    }
    let mut items = Vec::new();
    let Ok(entries) = std::fs::read_dir(&hub) else {
        return items;
    };
    for entry in entries.flatten() {
        let fname = entry.file_name().to_string_lossy().into_owned();
        if !fname.starts_with("models--") && !fname.starts_with("datasets--") {
            continue;
        }
        let kind = if fname.starts_with("datasets--") { "dataset" } else { "model" };
        let pretty = fname
            .trim_start_matches("models--")
            .trim_start_matches("datasets--")
            .replace("--", "/");
        let size = util::dir_size(&entry.path());
        let mut item = Item::new(Domain::AiAgent, "huggingface", pretty)
            .path(entry.path().to_string_lossy().into_owned())
            .meta(serde_json::json!({ "kind": kind }));
        if let Some(b) = size {
            item = item.size(b);
        }
        items.push(item);
    }
    items
}
