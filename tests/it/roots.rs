//! A project with several roots, files no root reads, and files git ignores.

use std::path::{Path, PathBuf};

use satex::plugins::discovery;

/// Two roots sharing `shared.tex`; `orphan.tex` and `stray.bib` no root
/// reads; `ignored.tex` and `build/` are ignored.
fn project(tag: &str) -> PathBuf {
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join(format!("roots-{tag}")));
    let root = |more: &str| {
        format!("\\documentclass{{article}}\n\\begin{{document}}\n\\input{{shared}}\n{more}\\end{{document}}\n")
    };
    let files = [
        ("a.tex", root("\\bibliography{refs,more}\n")),
        ("b.tex", root("")),
        ("shared.tex", "Shared text.\n".into()),
        ("refs.bib", "".into()),
        ("more.bib", "".into()),
        ("orphan.tex", "Nobody reads me.\n".into()),
        ("stray.bib", "".into()),
        ("ignored.tex", root("")),
        ("build/out.tex", "Built.\n".into()),
        (".gitignore", "ignored.tex\nbuild/\n".into()),
    ];
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn names(paths: &[PathBuf], dir: &Path) -> Vec<String> {
    paths.iter().map(|p| p.strip_prefix(dir).unwrap_or(p).display().to_string()).collect()
}

#[test]
fn each_root_stands_alone_and_unread_files_apart() {
    let dir = project("discovery");
    let found = discovery::scan(&dir);
    assert_eq!(names(&found.roots(), &dir), ["a.tex", "b.tex"]);
    let mut shared = names(&found.candidate_roots(&dir.join("shared.tex")), &dir);
    shared.sort();
    assert_eq!(shared, ["a.tex", "b.tex"], "the shared file belongs to both roots");
    assert_eq!(names(&found.unreachable(), &dir), ["orphan.tex", "stray.bib"]);
    let rows = found.rows();
    let all: Vec<String> = rows.iter().flat_map(|(_, paths)| names(paths, &dir)).collect();
    assert!(!all.iter().any(|n| n.contains("ignored") || n.starts_with("build")), "{all:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn query_project_lists_the_unreachable_files() {
    let dir = project("query");
    let out = crate::support::satex()
        .args(["query", "project", "-f", "a.tex", "--format", "json", "--set", "limits.seconds=0"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let row =
        |kind: &str| rows.iter().find(|r| r["kind"] == kind).and_then(|r| r["detail"].as_str()).unwrap_or_default();
    assert!(row("unreachable").contains("orphan.tex") && row("unreachable").contains("stray.bib"), "{rows:?}");
    assert!(!row("unreachable").contains("shared") && !row("documents").contains("ignored"), "{rows:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
