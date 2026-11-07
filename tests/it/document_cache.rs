//! A document's whole analysis is kept between runs: a rerun of an
//! unchanged document is served, with facts byte for byte those of the run
//! that stored them, and any change to what the run read reads again.

use std::path::{Path, PathBuf};

use crate::support::installed;
use satex::analysis::machine::{Analysis, Machine};
use satex::project::config::{Cache, Config};

fn project(name: &str) -> PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-document-cache-{name}-{}", std::process::id()));
    crate::support::scratch_dir(dir)
}

fn config(dir: &Path) -> Config {
    let cfg =
        Config { cache: Cache { dir: Some(dir.join("cache")), ..Default::default() }, ..crate::support::config() };
    crate::support::seed_kernel(&cfg);
    cfg
}

fn analyze(path: &Path, source: &str, cfg: &Config) -> Analysis {
    std::fs::write(path, source).expect("the document");
    Machine::analyze(source, Some(path), cfg)
}

fn served(analysis: &Analysis) -> bool {
    analysis.package_caches.iter().any(|(name, _, state)| name == "document" && state == "cached")
}

/// Everything a query or lint reads of a run, as bytes, part by part.
fn facts(a: &Analysis) -> Vec<(&'static str, Vec<u8>)> {
    fn bytes(t: &impl serde::Serialize) -> Vec<u8> {
        postcard::to_allocvec(t).expect("encodable")
    }
    vec![
        ("facts", bytes(&a.facts)),
        ("graph", bytes(&a.graph)),
        ("calls", bytes(&(&a.calls, &a.observed))),
        ("names", bytes(&a.interner)),
        ("files", bytes(&a.files)),
        ("env", bytes(&(&a.env, &a.catcodes, &a.preamble))),
        ("rest", bytes(&(&a.fonts, &a.font_states, &a.metadata, &a.written, a.steps))),
    ]
}

/// The first part two runs disagree on.
fn differs(a: &Analysis, b: &Analysis) -> Option<&'static str> {
    facts(a).into_iter().zip(facts(b)).find(|(x, y)| x != y).map(|(x, _)| x.0)
}

/// Every site example, run twice: the second is served and agrees byte for byte.
#[test]
fn every_example_is_served_unchanged() {
    if !installed() {
        return;
    }
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("doc/site/examples");
    let dir = project("examples");
    let mut cfg = config(&dir);
    cfg.load.packages = false;
    for entry in std::fs::read_dir(&examples).expect("the examples").flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e != "tex") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("an example");
        let cold = Machine::analyze(&source, Some(&path), &cfg);
        let warm = Machine::analyze(&source, Some(&path), &cfg);
        assert!(served(&warm), "{}: not served: {:?}", path.display(), warm.package_caches);
        assert_eq!(differs(&cold, &warm), None, "{}: served facts differ", path.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Runs 2 and 3 rebuild nothing and run nothing.
#[test]
fn a_rerun_rebuilds_and_runs_nothing() {
    if !installed() {
        return;
    }
    let dir = project("guard");
    let cfg = config(&dir);
    let source = "\\documentclass{article}\\usepackage{hyperref}\\begin{document}\\section{A}\\label{a}\\ref{a}\\end{document}\n";
    let cold = analyze(&dir.join("doc.tex"), source, &cfg);
    for run in [2, 3] {
        let warm = analyze(&dir.join("doc.tex"), source, &cfg);
        let rebuilt: Vec<_> = warm.cache_report().into_iter().filter(|l| l.contains("rebuilt")).collect();
        assert!(rebuilt.is_empty(), "run {run}: {rebuilt:?}");
        assert_eq!(warm.steps, warm.served_steps, "run {run} ran steps");
        assert_eq!(differs(&cold, &warm), None, "run {run}: served facts differ");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// An edit to a file the document reads, a file it looked for and did not
/// find appearing, and an edit to the document itself each read again.
#[test]
fn a_changed_input_is_read_again() {
    if !installed() {
        return;
    }
    let dir = project("inputs");
    let cfg = config(&dir);
    let doc = dir.join("doc.tex");
    let source = "\\documentclass{article}\\begin{document}\\input{part}\\IfFileExists{late.tex}{\\input{late}}{}\\end{document}\n";
    let defined = |a: &Analysis, name: &str| a.facts.defs.iter().any(|d| a.interner.name(d.name) == name);
    std::fs::write(dir.join("part.tex"), "\\def\\one{}").expect("part.tex");
    analyze(&doc, source, &cfg);
    let again = analyze(&doc, source, &cfg);
    assert!(served(&again) && defined(&again, "one"));
    std::fs::write(dir.join("part.tex"), "\\def\\two{}").expect("part.tex");
    let edited = analyze(&doc, source, &cfg);
    assert!(!served(&edited) && defined(&edited, "two") && !defined(&edited, "one"));
    std::fs::write(dir.join("late.tex"), "\\def\\three{}").expect("late.tex");
    let appeared = analyze(&doc, source, &cfg);
    assert!(!served(&appeared) && defined(&appeared, "three"));
    let changed = analyze(&doc, &source.replace("\\end{document}", "\\def\\four{}\\end{document}"), &cfg);
    assert!(!served(&changed) && defined(&changed, "four"));
    let off = Config { cache: Cache { document: false, ..cfg.cache.clone() }, ..cfg.clone() };
    assert!(!served(&analyze(&doc, source, &off)));
    let _ = std::fs::remove_dir_all(&dir);
}
