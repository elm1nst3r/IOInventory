use super::util;
use crate::model::{Domain, Item};

/// User-installed Ruby gems. Uses `gem list --local`; default/system gems are
/// included by Ruby, so this can be a longer list — the scan timeout guards it.
pub async fn collect() -> Vec<Item> {
    if !util::is_available("gem") {
        return Vec::new();
    }
    let Some(out) = util::run("gem", &["list", "--local"]).await else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Format: "name (1.2.3, 1.0.0)"
        let Some((name, rest)) = line.split_once(' ') else {
            continue;
        };
        let version = rest.trim().trim_start_matches('(').trim_end_matches(')').to_string();
        let mut item = Item::new(Domain::PackageManager, "gem", name.trim());
        if !version.is_empty() {
            item = item.version(version);
        }
        items.push(item);
    }
    items
}
