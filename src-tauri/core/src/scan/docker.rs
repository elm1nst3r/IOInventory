use super::util;
use crate::model::{Domain, Item};
use std::time::Duration;

/// Docker images and containers. If the daemon isn't running these commands
/// error out quickly (or hit the timeout) and we return whatever we got.
pub async fn collect() -> Vec<Item> {
    if !util::is_available("docker") {
        return Vec::new();
    }
    let mut items = Vec::new();
    let timeout = Duration::from_secs(3);

    // Images: one JSON object per line.
    let (ok, out) = util::run_capture(
        "docker",
        &["images", "--format", "{{json .}}"],
        timeout,
    )
    .await;
    if ok {
        for line in out.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let repo = v.get("Repository").and_then(|x| x.as_str()).unwrap_or("<none>");
            let tag = v.get("Tag").and_then(|x| x.as_str()).unwrap_or("");
            let name = if tag.is_empty() { repo.to_string() } else { format!("{repo}:{tag}") };
            let size = v.get("Size").and_then(|x| x.as_str()).unwrap_or("");
            items.push(
                Item::new(Domain::Container, "docker-image", name)
                    .meta(serde_json::json!({ "size_h": size, "id": v.get("ID") })),
            );
        }
    }

    // Containers (all states).
    let (ok, out) = util::run_capture(
        "docker",
        &["ps", "-a", "--format", "{{json .}}"],
        timeout,
    )
    .await;
    if ok {
        for line in out.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let name = v.get("Names").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let status = v.get("Status").and_then(|x| x.as_str()).unwrap_or("");
            let image = v.get("Image").and_then(|x| x.as_str()).unwrap_or("");
            let mut item = Item::new(Domain::Container, "docker-container", name)
                .meta(serde_json::json!({ "image": image }));
            // An empty status would render as a bare "v" in the UI.
            if !status.is_empty() {
                item = item.version(status);
            }
            items.push(item);
        }
    }

    items
}
