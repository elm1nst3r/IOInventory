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
        if let Some(item) = parse_line(line) {
            items.push(item);
        }
    }
    items
}

/// One `gem list` row. Two shapes matter:
///   `rake (13.2.1, 13.0.6)`     — a user-installed gem
///   `bigdecimal (default: 1.4.1)` — a gem bundled with the Ruby itself
/// Some RubyGems versions also print a `*** LOCAL GEMS ***` banner, which must
/// not become an item.
fn parse_line(line: &str) -> Option<Item> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('*') {
        return None;
    }
    let (name, rest) = line.split_once(" (")?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let versions = rest.trim_end_matches(')').trim();
    // `default:` marks a gem that ships with Ruby; keep it out of the version
    // string and record it as metadata instead.
    let (versions, is_default) = match versions.strip_prefix("default:") {
        Some(v) => (v.trim(), true),
        None => (versions, false),
    };

    let mut item = Item::new(Domain::PackageManager, "gem", name)
        .meta(serde_json::json!({ "default": is_default }));
    if !versions.is_empty() {
        item = item.version(versions);
    }
    Some(item)
}

#[cfg(test)]
mod tests {
    use super::parse_line;

    #[test]
    fn gem_rows_parse() {
        let rake = parse_line("rake (13.2.1, 13.0.6)").unwrap();
        assert_eq!(rake.name, "rake");
        assert_eq!(rake.version.as_deref(), Some("13.2.1, 13.0.6"));
        assert_eq!(rake.metadata["default"], false);

        // The `default:` marker belongs in metadata, not in the version string.
        let bd = parse_line("bigdecimal (default: 1.4.1)").unwrap();
        assert_eq!(bd.name, "bigdecimal");
        assert_eq!(bd.version.as_deref(), Some("1.4.1"));
        assert_eq!(bd.metadata["default"], true);

        // Banner lines and blanks are not gems.
        assert!(parse_line("*** LOCAL GEMS ***").is_none());
        assert!(parse_line("").is_none());
        println!("gem_rows_parse OK");
    }
}
