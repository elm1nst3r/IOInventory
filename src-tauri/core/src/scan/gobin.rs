//! Binaries installed with `go install`.
//!
//! They land in `$GOBIN`, else `$GOPATH/bin`, else `~/go/bin`. One
//! `go version -m <dir>` call reports the module and version of every binary
//! in the directory at once, so this stays a single subprocess however many
//! tools are installed.

use super::util;
use crate::model::{Domain, Item};
use std::path::PathBuf;

fn bin_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("GOBIN").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    if let Some(p) = std::env::var_os("GOPATH").filter(|p| !p.is_empty()) {
        if let Some(first) = std::env::split_paths(&p).next() {
            return first.join("bin");
        }
    }
    util::home().join("go").join("bin")
}

pub async fn collect() -> Vec<Item> {
    let dir = bin_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    if names.is_empty() {
        return Vec::new();
    }

    let dir_s = dir.to_string_lossy().into_owned();
    let info = if util::is_available("go") {
        util::run("go", &["version", "-m", &dir_s]).await.map(|o| parse(&o)).unwrap_or_default()
    } else {
        Default::default()
    };

    names
        .into_iter()
        .map(|name| {
            let path = dir.join(&name).to_string_lossy().into_owned();
            let mut item = Item::new(Domain::PackageManager, "go-bin", &name).path(path.clone());
            if let Some(m) = info.get(&path) {
                if let Some(v) = &m.version {
                    item = item.version(v.clone());
                }
                item = item.meta(serde_json::json!({ "module": m.module, "go": m.go }));
            }
            item
        })
        .collect()
}

#[derive(Default, Clone)]
struct ModInfo {
    module: Option<String>,
    version: Option<String>,
    go: Option<String>,
}

/// `go version -m` output: a `<path>: go1.x` header per binary, then indented
/// `path`/`mod`/`dep` lines. The `mod` line carries the module and version.
fn parse(out: &str) -> std::collections::HashMap<String, ModInfo> {
    let mut map = std::collections::HashMap::new();
    let mut current: Option<String> = None;
    for line in out.lines() {
        if !line.starts_with(char::is_whitespace) {
            if let Some((path, go)) = line.rsplit_once(": ") {
                current = Some(path.to_string());
                map.insert(
                    path.to_string(),
                    ModInfo { go: Some(go.trim().trim_start_matches("go").to_string()), ..Default::default() },
                );
            }
            continue;
        }
        let mut parts = line.split_whitespace();
        if parts.next() == Some("mod") {
            if let Some(info) = current.as_ref().and_then(|c| map.get_mut(c)) {
                info.module = parts.next().map(str::to_string);
                info.version = parts.next().filter(|v| *v != "(devel)").map(str::to_string);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_m_output_parses() {
        let m = parse(
            "/Users/x/go/bin/gopls: go1.23.4\n\tpath\tgolang.org/x/tools/gopls\n\
             \tmod\tgolang.org/x/tools/gopls\tv0.16.2\th1:abc=\n\tdep\tgithub.com/x\tv1\n\
             /Users/x/go/bin/local: go1.22.0\n\tpath\texample.com/local\n\tmod\texample.com/local\t(devel)\t\n",
        );
        let g = &m["/Users/x/go/bin/gopls"];
        assert_eq!(g.module.as_deref(), Some("golang.org/x/tools/gopls"));
        assert_eq!(g.version.as_deref(), Some("v0.16.2"));
        assert_eq!(g.go.as_deref(), Some("1.23.4"));
        assert_eq!(m["/Users/x/go/bin/local"].version, None);
        println!("version_m_output_parses OK");
    }
}
