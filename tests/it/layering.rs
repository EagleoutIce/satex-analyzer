//! Layering: a module may only use modules of its own layer or below. The
//! violations that exist today are listed; a new one fails, a fixed one must
//! leave the list (so it only shrinks).

use std::collections::BTreeSet;
use std::path::Path;

/// Layer of a top-level module, lowest first. `lsp/plugins` counts as `lsp`.
const LAYERS: &[&[&str]] = &[
    &["fixed", "text", "util"],
    &["engine"],
    &["analysis"],
    &["caches", "project", "plugins"],
    &["knowledge"],
    &["linter", "queries"],
    &["lsp"],
    &["cli", "oracle", "bin", "main", "lib"],
];

/// (file, module it uses upward), as of the introduction of this test.
const KNOWN: &str = include_str!("layering.txt");

fn layer(module: &str) -> Option<usize> {
    LAYERS.iter().position(|names| names.contains(&module))
}

/// The modules named by `crate::` on a line: `crate::m::x` and `crate::{m, n::x}`.
fn used(line: &str) -> Vec<&str> {
    let ident = |s: &str| s.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(s.len());
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("crate::") {
        rest = &rest[at + 7..];
        if let Some(group) = rest.strip_prefix('{') {
            let end = group.find('}').unwrap_or(group.len());
            let mut depth = 0;
            let mut start = 0;
            for (i, c) in group[..end].char_indices().chain([(end, ',')]) {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    ',' if depth == 0 => {
                        let item = group[start..i].trim();
                        out.push(&item[..ident(item)]);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
        } else {
            out.push(&rest[..ident(rest)]);
        }
    }
    out
}

fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn modules_use_only_their_own_layer_or_below() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut paths = Vec::new();
    files(&src, &mut paths);
    let mut found = BTreeSet::new();
    for path in paths {
        let rel = path.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        let top = rel.split('/').next().unwrap().trim_end_matches(".rs");
        let Some(own) = layer(top) else { panic!("src/{rel}: top-level module `{top}` has no layer in LAYERS") };
        let text = std::fs::read_to_string(&path).unwrap();
        for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
            for module in used(line) {
                if layer(module).is_some_and(|l| l > own) {
                    found.insert(format!("{rel} {module}"));
                }
            }
        }
    }
    let known: BTreeSet<String> = KNOWN.lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
    let new: Vec<_> = found.difference(&known).collect();
    let fixed: Vec<_> = known.difference(&found).collect();
    assert!(
        new.is_empty(),
        "new upward uses (file, module):\n{}",
        new.iter().map(|s| s.to_string()).collect::<Vec<_>>().join("\n")
    );
    assert!(
        fixed.is_empty(),
        "fixed, remove from tests/it/layering.txt:\n{}",
        fixed.iter().map(|s| s.to_string()).collect::<Vec<_>>().join("\n")
    );
}
