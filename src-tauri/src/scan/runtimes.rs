use super::util;
use crate::model::{Domain, Item};

/// Language runtimes and version managers present on the machine.
pub async fn collect() -> Vec<Item> {
    let mut items = Vec::new();

    // Language runtimes: (binary, display name, version args)
    let runtimes: &[(&str, &str, &[&str])] = &[
        ("node", "Node.js", &["--version"]),
        ("python3", "Python", &["--version"]),
        ("ruby", "Ruby", &["--version"]),
        ("go", "Go", &["version"]),
        ("rustc", "Rust", &["--version"]),
        ("java", "Java", &["-version"]),
        ("php", "PHP", &["--version"]),
        ("deno", "Deno", &["--version"]),
        ("bun", "Bun", &["--version"]),
    ];
    for (bin, label, args) in runtimes {
        if let Some(path) = util::which(bin) {
            let version = util::run(bin, args)
                .await
                .map(|s| first_version(&s))
                .unwrap_or_default();
            let mut item = Item::new(Domain::Runtime, "runtime", *label)
                .path(path.to_string_lossy().into_owned());
            if !version.is_empty() {
                item = item.version(version);
            }
            items.push(item.meta(serde_json::json!({ "binary": bin })));
        }
    }

    // rustup toolchains
    if util::is_available("rustup") {
        if let Some(out) = util::run("rustup", &["toolchain", "list"]).await {
            for line in out.lines() {
                let name = line.trim().trim_end_matches(" (default)").trim_end_matches(" (override)");
                if name.is_empty() {
                    continue;
                }
                items.push(
                    Item::new(Domain::Runtime, "rustup-toolchain", name)
                        .meta(serde_json::json!({ "default": line.contains("(default)") })),
                );
            }
        }
    }

    // Version managers (presence detection)
    let managers: &[(&str, &str)] = &[
        ("pyenv", "pyenv"),
        ("nvm", "nvm"),
        ("fnm", "fnm"),
        ("volta", "Volta"),
        ("asdf", "asdf"),
        ("mise", "mise"),
        ("rbenv", "rbenv"),
        ("conda", "conda"),
    ];
    for (bin, label) in managers {
        let present = util::which(bin).is_some()
            || util::home().join(format!(".{bin}")).exists();
        if present {
            items.push(
                Item::new(Domain::Runtime, "version-manager", *label)
                    .meta(serde_json::json!({ "binary": bin })),
            );
        }
    }

    items
}

/// Extract the first version-looking token from a `--version` line.
fn first_version(s: &str) -> String {
    let first = s.lines().next().unwrap_or("");
    for tok in first.split([' ', '"', ',']) {
        let t = tok.trim().trim_start_matches('v');
        if !t.is_empty() && t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return t.to_string();
        }
    }
    String::new()
}
