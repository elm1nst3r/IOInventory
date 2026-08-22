use super::util;
use crate::model::{Domain, Item};

/// One language runtime and how to ask it for its version.
struct Runtime {
    bin: &'static str,
    label: &'static str,
    args: &'static [&'static str],
    /// `java -version` writes to stderr, not stdout. Reading stdout only would
    /// always yield an empty version, so those probes read both streams.
    version_on_stderr: bool,
}

const RUNTIMES: &[Runtime] = &[
    Runtime { bin: "node", label: "Node.js", args: &["--version"], version_on_stderr: false },
    Runtime { bin: "python3", label: "Python", args: &["--version"], version_on_stderr: false },
    Runtime { bin: "ruby", label: "Ruby", args: &["--version"], version_on_stderr: false },
    Runtime { bin: "go", label: "Go", args: &["version"], version_on_stderr: false },
    Runtime { bin: "rustc", label: "Rust", args: &["--version"], version_on_stderr: false },
    Runtime { bin: "java", label: "Java", args: &["-version"], version_on_stderr: true },
    Runtime { bin: "php", label: "PHP", args: &["--version"], version_on_stderr: false },
    Runtime { bin: "deno", label: "Deno", args: &["--version"], version_on_stderr: false },
    Runtime { bin: "bun", label: "Bun", args: &["--version"], version_on_stderr: false },
];

/// Language runtimes and version managers present on the machine.
pub async fn collect() -> Vec<Item> {
    let mut items = Vec::new();

    for rt in RUNTIMES {
        let Some(path) = util::which(rt.bin) else {
            continue;
        };
        // macOS ships a `/usr/bin/java` shim on machines with no JDK at all: it
        // exists, but running it fails. Treat a failed probe as "not really
        // installed" so the inventory doesn't invent a runtime (and so the
        // failure doesn't surface as a scan warning on a healthy machine).
        let (ok, out) = if rt.version_on_stderr {
            util::run_capture_untracked(rt.bin, rt.args, util::CMD_TIMEOUT).await
        } else {
            match util::run(rt.bin, rt.args).await {
                Some(out) => (true, out),
                None => (false, String::new()),
            }
        };
        if !ok && rt.version_on_stderr {
            continue;
        }
        let version = first_version(&out);
        let mut item = Item::new(Domain::Runtime, "runtime", rt.label)
            .path(path.to_string_lossy().into_owned());
        if !version.is_empty() {
            item = item.version(version);
        }
        items.push(item.meta(serde_json::json!({ "binary": rt.bin })));
    }

    // rustup toolchains
    if util::is_available("rustup") {
        if let Some(out) = util::run("rustup", &["toolchain", "list"]).await {
            for line in out.lines() {
                let line = line.trim();
                // Newer rustup annotates the list: "stable-… (active, default)".
                let (name, flags) = match line.split_once(" (") {
                    Some((name, rest)) => (name.trim(), rest.trim_end_matches(')')),
                    None => (line, ""),
                };
                if name.is_empty() || name.starts_with("no installed toolchains") {
                    continue;
                }
                items.push(
                    Item::new(Domain::Runtime, "rustup-toolchain", name).meta(serde_json::json!({
                        "default": flags.split(',').any(|f| f.trim() == "default"),
                        "active": flags.split(',').any(|f| f.trim() == "active"),
                    })),
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
///
/// Handles the plain `1.2.3` / `v1.2.3` forms plus the prefixed ones some tools
/// print (`go version go1.21.0 …`), while ignoring pure words like `openjdk`.
fn first_version(s: &str) -> String {
    let first = s.lines().next().unwrap_or("");
    for tok in first.split([' ', '"', ',']) {
        // Drop any leading alphabetic prefix ("v", "go", "ruby"); what's left
        // only counts if it actually starts a number.
        let t = tok.trim().trim_start_matches(|c: char| c.is_ascii_alphabetic());
        if t.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return t.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::first_version;

    #[test]
    fn version_lines_parse() {
        assert_eq!(first_version("v22.11.0"), "22.11.0");
        assert_eq!(first_version("Python 3.13.1"), "3.13.1");
        assert_eq!(first_version("ruby 3.4.1 (2024-12-25 revision abc)"), "3.4.1");
        assert_eq!(first_version("rustc 1.83.0 (90b35a623 2024-11-26)"), "1.83.0");
        // `go version` embeds the number in a prefixed token.
        assert_eq!(first_version("go version go1.23.4 darwin/arm64"), "1.23.4");
        // `java -version` writes this to stderr.
        assert_eq!(first_version("openjdk version \"21.0.5\" 2024-10-15"), "21.0.5");
        assert_eq!(first_version("no version here"), "");
        println!("version_lines_parse OK");
    }
}
