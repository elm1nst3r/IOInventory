use super::util;
use crate::model::{Domain, Item};

/// Locally-pulled Ollama models.
pub async fn collect() -> Vec<Item> {
    if !util::is_available("ollama") {
        return Vec::new();
    }
    let Some(out) = util::run("ollama", &["list"]).await else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for (i, line) in out.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue; // header row
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.is_empty() {
            continue;
        }
        let name = cols[0].to_string();
        // Columns: NAME ID SIZE(2 tokens e.g. "4.7 GB") MODIFIED...
        let size_bytes = if cols.len() >= 4 {
            parse_human_size(cols[2], cols[3])
        } else {
            None
        };
        let mut item = Item::new(Domain::AiAgent, "ollama", name);
        if let Some(b) = size_bytes {
            item = item.size(b);
        }
        items.push(item);
    }
    items
}

/// Parse "4.7 GB" style sizes into bytes.
fn parse_human_size(num: &str, unit: &str) -> Option<i64> {
    let n: f64 = num.parse().ok()?;
    let mult = match unit.to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    Some((n * mult) as i64)
}
