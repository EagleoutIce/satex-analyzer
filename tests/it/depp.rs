//! depp: the TeX Live packages a run reads from, named by depp's rule, held
//! against the project's dependency file.

use std::path::PathBuf;

use crate::support::installed;
use satex::machine::{Analysis, Machine};

fn project(name: &str, depends: &str) -> (PathBuf, Analysis) {
    let dir = crate::support::tmp_root().join(format!("satex-depp-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("DEPENDS.txt"), depends).unwrap();
    let path = dir.join("doc.tex");
    let source =
        "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\includegraphics{x}\n\\end{document}\n";
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    (dir, analysis)
}

fn codes(analysis: &Analysis, code: &str) -> Vec<String> {
    satex::lint::lint(analysis)
        .into_iter()
        .filter(|record| record["code"] == code)
        .map(|record| record["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn missing_and_unused_dependencies_are_reported() {
    if !installed() {
        return;
    }
    let (dir, analysis) = project("lint", "# depp\nhard latex\nhard xcolor\n");
    let depp = analysis.plugins.depp.as_ref().expect("the dependency file is read");
    assert!(depp.needed.iter().any(|n| n.package == "graphics"), "graphicx lives in tex/latex/graphics");
    // graphicx pulls in graphics-cfg, kvoptions and the rest, all missing.
    let missing = codes(&analysis, "missing-dependency");
    assert!(missing.contains(&"graphics".to_string()), "{missing:?}");
    assert!(!missing.contains(&"latex".to_string()), "{missing:?}");
    assert_eq!(codes(&analysis, "unused-dependency"), vec!["xcolor".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_dependencies_query_names_the_texlive_package() {
    if !installed() {
        return;
    }
    let (dir, analysis) = project("query", "latex graphics\n");
    let records = satex::query::run(&analysis, satex::query::Query::Dependencies, &satex::query::Filter::Always);
    let graphicx = records.iter().find(|r| r["name"] == "graphicx").expect("graphicx is a dependency");
    assert_eq!(graphicx["texlive"], "graphics");
    assert!(!codes(&analysis, "missing-dependency").contains(&"graphics".to_string()));
    let row = analysis.plugins.rows().into_iter().find(|(kind, _, _)| *kind == satex::plugin::Kind::Depp);
    assert!(row.is_some_and(|(_, text, _)| text.contains("DEPENDS.txt lists 2")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_load_under_an_undecided_conditional_is_not_certain() {
    let dir = crate::support::tmp_root().join(format!("satex-depp-certain-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.tex"), "\\def\\a{}\n").unwrap();
    std::fs::write(dir.join("b.tex"), "\\def\\b{}\n").unwrap();
    let path = dir.join("doc.tex");
    let source = "\\input a\n\\ifnum\\pdfuniformdeviate2=0 \\input b \\fi\n\\bye\n";
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    let records = satex::query::run(&analysis, satex::query::Query::Dependencies, &satex::query::Filter::Always);
    let certain = |name: &str| records.iter().find(|r| r["name"] == name).map(|r| r["certain"].clone());
    assert_eq!(certain("a"), Some(true.into()), "{records:?}");
    assert_eq!(certain("b"), Some(false.into()), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_distribution_query_lists_the_indexed_directories() {
    if !installed() {
        return;
    }
    let (dir, analysis) = project("dirs", "");
    let records = satex::query::run(&analysis, satex::query::Query::Distribution, &satex::query::Filter::Always);
    let graphics =
        records.iter().find(|r| r.get("kind").is_some_and(|k| k == "directory") && r["name"] == "tex/latex/graphics");
    // graphics.sty and graphicx.sty at least
    assert!(graphics.is_some_and(|r| r["count"].as_u64() >= Some(2)), "{graphics:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
