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

/// [`differs`] for two runs that were not served from one another: the maps
/// of facts filled per site hold the same entries, in the order each run met
/// them (a run that resumes at a checkpoint meets them in another order).
fn differs_unordered(a: &Analysis, b: &Analysis) -> Option<&'static str> {
    fn sorted<K: std::fmt::Debug, V: std::fmt::Debug>(map: impl IntoIterator<Item = (K, V)>) -> Vec<String> {
        let mut entries: Vec<String> = map.into_iter().map(|e| format!("{e:?}")).collect();
        entries.sort();
        entries
    }
    let split = |x: &Analysis| {
        let mut x = x.clone();
        let facts = &mut x.facts;
        let maps = (
            sorted(std::mem::take(&mut facts.shapes)),
            sorted(std::mem::take(&mut facts.csnames)),
            sorted(std::mem::take(&mut facts.spaces)),
        );
        (maps, x)
    };
    let ((maps_a, a), (maps_b, b)) = (split(a), split(b));
    if maps_a != maps_b {
        return Some("the maps of facts");
    }
    // The bindings by value: a hash map serializes in the order its entries came in.
    let env = |x: &Analysis| serde_json::to_value((&x.env, &x.catcodes, &x.preamble)).expect("encodable");
    if env(&a) != env(&b) {
        return Some("env");
    }
    // The graph's and the calls' tables too: the same bytes, in the order each run filled them.
    let multiset = |(name, mut bytes): (&'static str, Vec<u8>)| {
        if matches!(name, "graph" | "calls") {
            bytes.sort_unstable();
        }
        (name, bytes)
    };
    let ordered =
        |x: &Analysis| facts(x).into_iter().filter(|(name, _)| *name != "env").map(multiset).collect::<Vec<_>>();
    ordered(&a).into_iter().zip(ordered(&b)).find(|(x, y)| x != y).map(|(x, _)| x.0)
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

/// A build that `limits.seconds` cut between two runs is kept with the files
/// its next run reads. The next process goes on with that run instead of run
/// 1, ends with the facts of a build nothing cut, and the one after is served.
/// Before, such a build was never stored: every process began again.
#[test]
fn a_build_cut_between_runs_goes_on_in_the_next_process() {
    if !installed() {
        return;
    }
    let dir = project("pending");
    let mut cfg = config(&dir);
    // The cut needs a limit to report. An hour never ends a test's run.
    cfg.limits.seconds = 3600;
    let doc = dir.join("doc.tex");
    let source = "\\documentclass{article}\\begin{document}\\tableofcontents\\section{A}\\label{a}\
        See \\ref{a} and \\pageref{z}.\\newpage\\section{B}\\label{z}\\end{document}\n";
    // The reference: the whole build in one process, on warm package caches.
    let whole = Config { cache: Cache { document: false, ..cfg.cache.clone() }, ..cfg.clone() };
    analyze(&doc, source, &whole);
    let whole = analyze(&doc, source, &whole);
    assert!(whole.build_runs > 1, "the build needs a second run: {}", whole.build_runs);
    satex::analysis::machine::BUILD_CUT.set(1);
    let cut = analyze(&doc, source, &cfg);
    satex::analysis::machine::BUILD_CUT.set(0);
    assert!(cut.stopped() && cut.build_pending.is_some() && cut.build_runs == 1, "{:?}", cut.limits);
    assert!(cut.limit_notes().iter().any(|n| n.contains("goes on from here")), "{:?}", cut.limit_notes());
    let next = analyze(&doc, source, &cfg);
    assert!(!served(&next) && !next.stopped(), "{:?} {:?}", next.package_caches, next.limits);
    assert_eq!(next.build_runs, whole.build_runs);
    assert_eq!(differs_unordered(&whole, &next), None, "the build in two processes differs from the build in one");
    let last = analyze(&doc, source, &cfg);
    assert!(served(&last), "{:?}", last.package_caches);
    assert_eq!(differs(&next, &last), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A finished build is kept with the files it left. The next process builds an edited text
/// from them: an edit that changes no build file settles in one run, one that adds a label
/// takes the runs that change needs, and both end with the facts of a build from nothing.
/// Before, every process built from nothing: three runs of the whole body for a typed word.
#[test]
fn an_edited_document_builds_from_the_last_builds_files() {
    if !installed() {
        return;
    }
    let dir = project("seed");
    let cfg = config(&dir);
    let doc = dir.join("doc.tex");
    let text = |body: &str| {
        format!(
            "\\documentclass{{article}}\\begin{{document}}\\tableofcontents\\section{{A}}\\label{{a}}\
            See \\ref{{a}} and \\pageref{{z}}. {body}\\newpage\\section{{B}}\\label{{z}}\\end{{document}}\n"
        )
    };
    let fresh = Config { cache: Cache { document: false, ..cfg.cache.clone() }, ..cfg.clone() };
    let first = analyze(&doc, &text(""), &cfg);
    assert!(first.build_runs > 1, "the build needs a second run: {}", first.build_runs);
    // A typed word: the build files stay what they were.
    let word = analyze(&doc, &text("More."), &cfg);
    assert!(!served(&word) && !word.stopped(), "{:?} {:?}", word.package_caches, word.limits);
    assert_eq!(word.build_runs, 1, "a run that writes what it read ends the build");
    assert_eq!(
        differs_unordered(&analyze(&doc, &text("More."), &fresh), &word),
        None,
        "differs from a build from nothing"
    );
    // A new label that is referenced: the run that first writes it is not the last.
    let label = text("\\section{C}\\label{c} See \\ref{c}.");
    let more = analyze(&doc, &label, &cfg);
    assert!(more.build_runs > 1 && !served(&more), "{} runs", more.build_runs);
    assert_eq!(differs_unordered(&analyze(&doc, &label, &fresh), &more), None, "differs from a build from nothing");
    assert!(served(&analyze(&doc, &label, &cfg)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A run a budget stopped is stored once it built no cache: the next run
/// would stop at the same place, so it is served, and the limit's note says
/// so. The run that built the class's cache is not stored, since the next
/// one gets further in the same time. Before, a stopped run ran again each time.
#[test]
fn a_stopped_run_is_served_once_no_cache_is_left_to_build() {
    if !installed() {
        return;
    }
    let dir = project("stopped");
    let mut cfg = config(&dir);
    let doc = dir.join("doc.tex");
    let source = "\\documentclass{article}\\begin{document}\\section{A}one two three\\end{document}\n";
    // The budget ends the run just before the document does.
    let whole = Config { cache: Cache { dir: Some(dir.join("other")), ..cfg.cache.clone() }, ..cfg.clone() };
    crate::support::seed_kernel(&whole);
    cfg.limits.steps = analyze(&doc, source, &whole).steps - 1;
    let cold = analyze(&doc, source, &cfg);
    assert!(cold.stopped() && cold.caches_built() > 0, "{:?} {:?}", cold.limits, cold.package_caches);
    let warm = analyze(&doc, source, &cfg);
    assert!(warm.stopped() && !served(&warm) && warm.caches_built() == 0, "{:?}", warm.package_caches);
    let again = analyze(&doc, source, &cfg);
    assert!(again.stopped() && served(&again), "{:?}", again.package_caches);
    assert_eq!(differs(&warm, &again), None, "served facts differ");
    assert!(again.limit_notes().iter().any(|n| n.contains("stored result")), "{:?}", again.limit_notes());
    let _ = std::fs::remove_dir_all(&dir);
}

/// One cache file per document: runs of its edited text, and a run under
/// another setting, replace the entry, and the listing names the document.
/// Before, each text and setting left its own file behind for good.
#[test]
fn an_edited_document_keeps_one_entry() {
    if !installed() {
        return;
    }
    let dir = project("one-entry");
    let cfg = config(&dir);
    let doc = dir.join("doc.tex");
    let text = |word: &str| format!("\\documentclass{{article}}\\begin{{document}}{word}\\end{{document}}\n");
    for word in ["one", "two", "three"] {
        assert!(!served(&analyze(&doc, &text(word), &cfg)));
    }
    let other = Config { record_arguments: !cfg.record_arguments, ..cfg.clone() };
    assert!(!served(&analyze(&doc, &text("three"), &other)));
    assert!(served(&analyze(&doc, &text("three"), &other)));
    let stored = satex::caches::document::stored(&dir.join("cache"));
    let documents: Vec<_> = stored.iter().map(|(document, ..)| document.clone()).collect();
    assert_eq!(documents, [Some(std::path::absolute(&doc).expect("a path"))]);
    let name = stored[0].1.file_name().expect("a file").to_string_lossy().into_owned();
    assert!(name.starts_with("document-doc.tex-"), "{name}");
    // A second document gets its own.
    analyze(&dir.join("more.tex"), &text("four"), &cfg);
    assert_eq!(satex::caches::document::stored(&dir.join("cache")).len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--stats` measures a run, so it is never served. Intuition: it still starts from the files
/// the last build left, as an edit does, so an unchanged document takes one run whose files are
/// what it read, with the findings of a build from nothing. The bug it pins: `--stats` turned the
/// document cache off, so every call built from an empty `.aux` and took two runs (4.9 s for a
/// tagged document where one run takes 3 s).
#[test]
fn stats_builds_from_the_last_builds_files() {
    if !installed() {
        return;
    }
    let dir = project("stats");
    let doc = dir.join("doc.tex");
    std::fs::write(
        &doc,
        "\\documentclass{article}\\begin{document}\\section{A}\\label{a}See \\ref{a}.\\end{document}\n",
    )
    .expect("the document");
    let lint = || {
        let out = crate::support::satex()
            .args(["-v", "--stats", "-f", "doc.tex", "lint", "--format", "json"])
            .current_dir(&dir)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("satex runs");
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        (out.stdout, err.lines().filter(|l| l.contains("build run")).count())
    };
    let (cold, cold_runs) = lint();
    assert!(cold_runs > 1, "the label needs a second run from nothing: {cold_runs}");
    let (warm, warm_runs) = lint();
    assert_eq!(warm_runs, 1, "a run that writes what it read ends the build");
    assert_eq!(String::from_utf8_lossy(&warm), String::from_utf8_lossy(&cold), "differs from a build from nothing");
    let _ = std::fs::remove_dir_all(&dir);
}
