//! A package read once is replayed from its cache on the next run, and the
//! replay reports what reading it would have.

use std::path::{Path, PathBuf};

use crate::support::{defined, installed};
use satex::config::{Cache, Config};
use satex::facts::LoadStatus;
use satex::machine::{Analysis, Machine};

fn project(name: &str) -> PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-package-cache-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temporary directory");
    std::fs::write(dir.join("inner.sty"), "\\ProvidesPackage{inner}[2020/01/01 v1]\n\\def\\innermacro#1{[#1]}\n")
        .expect("inner.sty");
    std::fs::write(
        dir.join("outer.sty"),
        "\\ProvidesPackage{outer}[2020/01/01 v1]\n\\RequirePackage{inner}\n\
         \\def\\outermacro{\\innermacro{x}}\n\\newcount\\outercount \\outercount=7\n\
         \\AtBeginDocument{\\def\\fromhook{}}\n\\ifdefined\\userflag\\def\\flagged{}\\fi\n\\outerundefined\n",
    )
    .expect("outer.sty");
    std::fs::write(
        dir.join("withopts.sty"),
        "\\ProvidesPackage{withopts}\n\\def\\before{}\n\\DeclareOption{draft}{\\def\\isdraft{}}\n\\ProcessOptions\\relax\n\\def\\after{}\n",
    )
    .expect("withopts.sty");
    dir
}

fn run(dir: &Path, source: &str) -> Analysis {
    run_with(dir, source, true)
}

fn run_with(dir: &Path, source: &str, auto: bool) -> Analysis {
    let mut cfg = latex(dir);
    cfg.cache.index.auto = auto;
    analyze(dir, source, &cfg)
}

fn analyze(dir: &Path, source: &str, cfg: &Config) -> Analysis {
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).expect("doc.tex");
    Machine::analyze(source, Some(&path), cfg)
}

/// The installation's LaTeX, with the kernel already interpreted: every test
/// directory starts from one shared kernel cache, and keeps its own package
/// caches.
fn latex(dir: &Path) -> Config {
    static KERNEL: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let shared = KERNEL.get_or_init(|| {
        let shared = crate::support::tmp_root().join(format!("satex-package-cache-kernel-{}", std::process::id()));
        let cfg =
            Config { cache: Cache { dir: Some(shared.clone()), ..Default::default() }, ..crate::support::config() };
        Machine::analyze("\\relax\n", Some(&shared.join("kernel.tex")), &cfg);
        shared
    });
    let cache = dir.join("cache");
    std::fs::create_dir_all(&cache).expect("cache");
    for entry in std::fs::read_dir(shared).expect("kernel cache").flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".postcard") && !cache.join(&name).exists() {
            std::fs::copy(entry.path(), cache.join(&name)).expect("kernel cache copied");
        }
    }
    Config { cache: Cache { dir: Some(cache), ..Default::default() }, ..crate::support::config() }
}

const DOCUMENT: &str = "\\documentclass{article}\n\\usepackage{outer}\n\\begin{document}\\outermacro\\end{document}\n";

/// `DOCUMENT` with `lines` before `\usepackage{outer}`.
fn preamble(lines: &str) -> String {
    DOCUMENT.replacen("\\usepackage", &format!("{lines}\n\\usepackage"), 1)
}

/// Whether the package came from its cache (`Some(true)`) or was read and
/// stored (`Some(false)`).
fn served(analysis: &Analysis, package: &str) -> Option<bool> {
    analysis.package_caches.iter().rev().find(|(name, ..)| name == package).and_then(|(.., state)| {
        match state.as_str() {
            state if satex::machine::cache_served(state) => Some(true),
            state if state.ends_with("stored") => Some(false),
            _ => None,
        }
    })
}

/// Everything a query or lint reads, rendered so two runs compare.
fn facts(analysis: &Analysis) -> Vec<String> {
    let mut out = vec![format!("{:?}", definitions(analysis)), format!("{:?}", diagnostics(analysis))];
    out.push(format!("{:?}", loads(analysis)));
    out.extend(analysis.facts.occurrences.iter().map(|o| format!("{:?} {} {:?}", o.kind, o.key, o.span)));
    out.extend(
        analysis
            .facts
            .expansions
            .iter()
            .map(|e| format!("{} {:?} {} {}", analysis.interner.name(e.name), e.span, e.meaning.as_str(), e.count)),
    );
    out.push(format!("{} vertices, {} edges", analysis.graph.len(), analysis.graph.edge_count()));
    out.push(serde_json::to_string(&satex::lint::lint(analysis)).unwrap_or_default());
    out
}

fn definitions(analysis: &Analysis) -> Vec<(String, u32)> {
    analysis.facts.defs.iter().map(|d| (analysis.interner.name(d.name).to_string(), d.span.line)).collect()
}

fn diagnostics(analysis: &Analysis) -> Vec<(&'static str, String)> {
    analysis
        .facts
        .diagnostics
        .iter()
        .map(|d| (d.code, format!("{}:{}", analysis.file_name(d.span.file), d.span.line)))
        .collect()
}

fn loads(analysis: &Analysis) -> Vec<(String, &'static str, Option<String>)> {
    analysis.facts.loads.iter().map(|l| (l.name.clone(), l.status.as_str(), l.path.clone())).collect()
}

/// Every dependency-graph edge, by the sites of its ends.
fn edges(a: &Analysis) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for (i, v) in a.graph.vertices.iter().enumerate() {
        for (to, kind) in a.graph.outgoing(i as u32) {
            let t = &a.graph.vertices[*to as usize];
            out.insert(format!(
                "{} {}:{}:{} {:?} -> {} {}:{}:{} {:?} {:?}",
                a.interner.name(v.name),
                a.file_name(v.span.file),
                v.span.line,
                v.span.col,
                v.tag,
                a.interner.name(t.name),
                a.file_name(t.span.file),
                t.span.line,
                t.span.col,
                t.tag,
                kind.names()
            ));
        }
    }
    out
}

/// The first fact two runs disagree on, for a readable failure.
fn same_facts(a: &Analysis, b: &Analysis) {
    if std::env::var_os("SATEX_FACTS_CONTEXT").is_some() {
        let (x, y) = (edges(a), edges(b));
        for e in x.difference(&y).take(10) {
            eprintln!("cold only {e}");
        }
        for e in y.difference(&x).take(10) {
            eprintln!("warm only {e}");
        }
        let lints = |a: &Analysis| -> std::collections::BTreeSet<String> {
            satex::lint::lint(a)
                .iter()
                .map(|r| format!("{} {} {} {}", r["code"], r["file"], r["line"], r["message"]))
                .collect()
        };
        let (x, y) = (lints(a), lints(b));
        for e in x.difference(&y).take(10) {
            eprintln!("cold lint only {e}");
        }
        for e in y.difference(&x).take(10) {
            eprintln!("warm lint only {e}");
        }
    }
    let (a, b) = (facts(a), facts(b));
    if a.len() != b.len() {
        let at = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        panic!("facts differ from {at}: {:?} / {:?}", a.get(at), b.get(at));
    }
    for (k, (x, y)) in a.iter().zip(&b).enumerate() {
        if x != y {
            if std::env::var_os("SATEX_FACTS_CONTEXT").is_some() {
                for j in k.saturating_sub(4)..(k + 3).min(a.len()) {
                    eprintln!(
                        "{j} cold {} | warm {}",
                        a[j].chars().take(160).collect::<String>(),
                        b[j].chars().take(160).collect::<String>()
                    );
                }
            }
            let at = x.chars().zip(y.chars()).take_while(|(p, q)| p == q).count();
            let from = at.saturating_sub(200);
            panic!(
                "facts differ at {at}:\n  {}\n  {}",
                x.chars().skip(from).take(400).collect::<String>(),
                y.chars().skip(from).take(400).collect::<String>()
            );
        }
    }
}

#[test]
fn a_second_run_replays_the_package_and_reports_the_same() {
    let dir = project("replay");
    let first = run(&dir, DOCUMENT);
    let second = run(&dir, DOCUMENT);
    assert_eq!(
        served(&first, "outer"),
        Some(false),
        "the first run stores the cache: {:?} {:?}",
        loads(&first),
        diagnostics(&first)
    );
    assert_eq!(served(&second, "outer"), Some(true), "the second run reads it: {:?}", second.package_caches);
    assert_eq!(definitions(&first), definitions(&second));
    assert_eq!(diagnostics(&first), diagnostics(&second));
    // TeX's "Undefined control sequence" in the package is its expansion.
    let undefined = |a: &Analysis| {
        a.facts.expansions.iter().any(|e| {
            e.meaning == satex::facts::MeaningKind::Undefined
                && a.file_name(e.span.file).ends_with("outer.sty")
                && e.span.line == 7
        })
    };
    assert!(undefined(&second), "the package's own error is kept");
    assert_eq!(loads(&first), loads(&second));
    assert!(loads(&second).iter().any(|(name, ..)| name == "inner"), "a nested load is still reported");
    for name in ["outermacro", "innermacro"] {
        assert_eq!(defined(&first, name), defined(&second, name), "\\{name} after a replay");
        assert!(defined(&first, name), "\\{name} is defined");
    }
    assert_eq!(first.steps, second.steps);
    same_facts(&first, &second);
    let uncached = run_cold(&dir, DOCUMENT);
    assert_eq!(served(&uncached, "outer"), None, "a cold run neither reads nor stores it");
    same_facts(&uncached, &second);
}

#[test]
fn a_replayed_package_reports_the_tokens_and_time_it_stands_for() {
    let dir = project("timed");
    let timed = |dir: &Path| {
        let mut cfg = latex(dir);
        cfg.timings = true;
        cfg
    };
    let _first = analyze(&dir, DOCUMENT, &timed(&dir));
    let second = analyze(&dir, DOCUMENT, &timed(&dir));
    assert_eq!(served(&second, "outer"), Some(true), "the second run replays outer.sty from its cache");
    let root = satex::query::summary(&second);
    let outer = root.children.iter().find(|n| n.name == "outer.sty").expect("outer.sty in the tree");
    assert!(outer.cached, "the replayed file is marked cached");
    assert!(outer.tokens > 0, "outer.sty reports the tokens its cache stands for, got {}", outer.tokens);
    // `inner.sty` is only reached inside outer's cached segment: this run
    // opened no frame of its own for it, so it is folded into one line
    // instead of shown as its own zero-cost row.
    let text = satex::render::timings(&second, satex::render::Links(false));
    assert!(
        !text.lines().any(|line| line.contains("inner.sty") && line.contains("tokens")),
        "inner.sty should not get its own row:\n{text}"
    );
    assert!(text.contains("inner"), "inner.sty is named in the folded line:\n{text}");
}

#[test]
fn a_changed_package_file_is_read_again() {
    let dir = project("stamp");
    run(&dir, DOCUMENT);
    std::fs::write(dir.join("inner.sty"), "\\ProvidesPackage{inner}\n\\def\\innermacro#1{}\\def\\added{}\n")
        .expect("inner.sty");
    let again = run(&dir, DOCUMENT);
    assert_eq!(served(&again, "outer"), Some(false));
    assert!(
        again.package_caches.iter().any(|(name, _, state)| name == "outer"
            && state.contains("content changed")
            && state.contains("inner.sty")),
        "the stale cache says why: {:?}",
        again.package_caches
    );
    assert!(defined(&again, "added"));
}

/// An edit that keeps the size and the modification time (a save within
/// the same second, `cp -p`) still makes the cache stale.
#[test]
fn a_same_size_edit_in_the_same_second_is_read_again() {
    let dir = project("same-size");
    let path = dir.join("inner.sty");
    let text = std::fs::read_to_string(&path).expect("inner.sty");
    let edited = text.replacen("innermacro", "innermacrx", 1);
    assert_eq!(edited.len(), text.len());
    run(&dir, DOCUMENT);
    // The old modification time put back, as a save within the same second
    // or `cp -p` leaves it.
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).expect("a modification time");
    std::fs::write(&path, &edited).expect("inner.sty");
    std::fs::File::options().write(true).open(&path).and_then(|f| f.set_modified(modified)).expect("set mtime");
    let again = run(&dir, DOCUMENT);
    assert_eq!(served(&again, "outer"), Some(false), "{:?}", again.package_caches);
    assert!(defined(&again, "innermacrx"));
}

/// A name the cached package defined, gone from the edited file, is
/// undefined where the document uses it: the replay carries no stale
/// definition.
#[test]
fn a_name_the_package_no_longer_defines_is_undefined() {
    let dir = project("gone");
    let path = dir.join("inner.sty");
    let text = std::fs::read_to_string(&path).expect("inner.sty");
    let source = "\\documentclass{article}\n\\usepackage{outer}\n\\begin{document}\\innermacro{x}\\end{document}\n";
    let undefined = |a: &Analysis| {
        let named = |r: &serde_json::Map<String, serde_json::Value>| {
            r["message"].as_str().is_some_and(|m| m.contains("`\\innermacro`"))
        };
        satex::lint::lint(a).iter().any(|r| r["code"] == "undefined-control-sequence" && named(r))
    };
    let warm = run(&dir, source);
    assert!(!undefined(&warm));
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).expect("a modification time");
    std::fs::write(&path, text.replacen("innermacro", "innermacrx", 1)).expect("inner.sty");
    std::fs::File::options().write(true).open(&path).and_then(|f| f.set_modified(modified)).expect("set mtime");
    let again = run(&dir, source);
    assert!(!defined(&again, "innermacro"));
    assert!(undefined(&again), "{:?}", again.package_caches);
}

#[test]
fn the_same_package_after_a_different_preamble_is_served_and_reports_what_reading_it_does() {
    let dir = project("state");
    run(&dir, DOCUMENT);
    let other = preamble("\\def\\before{}\\def\\another{x}");
    let warm = run(&dir, &other);
    assert_eq!(served(&warm, "outer"), Some(true), "{:?}", warm.package_caches);
    let cold = run_cold(&dir, &other);
    assert_eq!(facts(&cold).len(), facts(&warm).len());
    same_facts(&cold, &warm);
}

#[test]
fn a_prior_definition_the_package_tests_makes_it_miss() {
    let dir = project("tested");
    run(&dir, DOCUMENT);
    let warm = run(&dir, &preamble("\\def\\userflag{}"));
    assert_ne!(served(&warm, "outer"), Some(true), "{:?}", warm.package_caches);
    assert!(defined(&warm, "flagged"));
}

#[test]
fn other_options_make_it_miss() {
    let dir = project("options");
    run(&dir, "\\documentclass{article}\n\\usepackage{withopts}\n\\begin{document}\\end{document}\n");
    let other = "\\documentclass{article}\n\\usepackage[draft]{withopts}\n\\begin{document}\\end{document}\n";
    let warm = run(&dir, other);
    assert_ne!(served(&warm, "withopts"), Some(true), "{:?}", warm.package_caches);
    assert!(defined(&warm, "isdraft"));
    let cold = run_cold(&dir, other);
    same_facts(&cold, &warm);
}

#[test]
fn a_file_that_now_shadows_a_lookup_makes_the_cache_stale() {
    let dir = project("shadow");
    let other = dir.join("other");
    std::fs::create_dir_all(&other).expect("other");
    std::fs::rename(dir.join("inner.sty"), other.join("inner.sty")).expect("move");
    let mut cfg = latex(&dir);
    cfg.search_paths = vec![other.clone()];
    let first = analyze(&dir, DOCUMENT, &cfg);
    assert_eq!(served(&first, "outer"), Some(false));
    std::fs::write(dir.join("inner.sty"), "\\def\\innermacro#1{}\\def\\shadowed{}\n").expect("inner.sty");
    let second = analyze(&dir, DOCUMENT, &cfg);
    assert_eq!(served(&second, "outer"), Some(false), "{:?}", second.package_caches);
    assert!(defined(&second, "shadowed"));
}

/// Two different repositories can each ship their own `outer.sty` with the
/// same package name. A cache shared between them (the ordinary case: the
/// platform cache directory is one per machine, not one per project) must
/// not let one project's analysis stand for the other's: the key has to
/// include the resolved path, not just the name.
#[test]
fn two_projects_sharing_a_cache_do_not_shadow_each_others_package() {
    let shared_cache = crate::support::tmp_root().join(format!("satex-package-cache-shared-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&shared_cache);
    std::fs::create_dir_all(&shared_cache).expect("shared cache dir");

    let a = project("shadow-a");
    let b = project("shadow-b");
    // Same package name, different content, in two unrelated directories.
    std::fs::write(
        a.join("outer.sty"),
        "\\ProvidesPackage{outer}[2020/01/01 v1]\n\\def\\outermacro{fromA}\n\\def\\onlyinA{}\n",
    )
    .expect("outer.sty in a");
    std::fs::write(
        b.join("outer.sty"),
        "\\ProvidesPackage{outer}[2020/01/01 v1]\n\\def\\outermacro{fromB}\n\\def\\onlyinB{}\n",
    )
    .expect("outer.sty in b");

    let cfg_for = |dir: &Path| -> Config {
        let mut cfg = latex(dir);
        cfg.cache.dir = Some(shared_cache.clone());
        cfg
    };

    let from_a = analyze(&a, DOCUMENT, &cfg_for(&a));
    assert!(defined(&from_a, "onlyinA"), "a's own package was read: {:?}", from_a.package_caches);
    assert!(!defined(&from_a, "onlyinB"), "a must not see b's package");

    let from_b = analyze(&b, DOCUMENT, &cfg_for(&b));
    assert!(defined(&from_b, "onlyinB"), "b must read its own outer.sty, not a's cache: {:?}", from_b.package_caches);
    assert!(!defined(&from_b, "onlyinA"), "b must not see a's package");
    assert_ne!(
        served(&from_b, "outer"),
        Some(true),
        "a's cache for the same name must not be served to b: {:?}",
        from_b.package_caches
    );

    // Running each project a second time replays its own cache, still
    // without crossing over.
    let a_again = analyze(&a, DOCUMENT, &cfg_for(&a));
    assert_eq!(served(&a_again, "outer"), Some(true), "{:?}", a_again.package_caches);
    assert!(defined(&a_again, "onlyinA"));
    assert!(!defined(&a_again, "onlyinB"));

    let b_again = analyze(&b, DOCUMENT, &cfg_for(&b));
    assert_eq!(served(&b_again, "outer"), Some(true), "{:?}", b_again.package_caches);
    assert!(defined(&b_again, "onlyinB"));
    assert!(!defined(&b_again, "onlyinA"));
}

#[test]
fn cache_build_builds_what_is_missing_and_refresh_rebuilds() {
    let dir = project("index");
    let mut cfg = latex(&dir);
    cfg.search_paths = vec![dir.clone()];
    cfg.cache.index.packages = vec!["outer".into()];
    cfg.cache.index.auto = false;
    let states = |refresh| -> Vec<String> {
        let mut lines = Vec::new();
        let prepared =
            satex::cmd::cache::build::prepare(&cfg, refresh, &[], &[], None, &mut |line| lines.push(line.to_string()))
                .expect("prepared");
        assert!(lines[0].starts_with("plan:"), "the plan comes first: {lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("[2/2] outer")), "then each item: {lines:?}");
        prepared
            .into_iter()
            .filter(|p| p.package != "kernel")
            .map(|p| {
                if p.outcome == "failed" {
                    format!("{} failed: {:?} {}", p.package, p.reason, p.state)
                } else {
                    format!("{} {}", p.package, p.outcome)
                }
            })
            .collect()
    };
    assert_eq!(states(false), ["outer built"]);
    assert_eq!(states(false), ["outer fresh"]);
    assert_eq!(states(true), ["outer built"]);
    std::fs::write(dir.join("inner.sty"), "\\ProvidesPackage{inner}\n\\def\\innermacro#1{}\n").expect("inner.sty");
    assert_eq!(states(false), ["outer rebuilt"], "a changed file is rejected and rebuilt");
}

fn run_latex(dir: &Path, source: &str, auto: bool) -> Analysis {
    let mut cfg = latex(dir);
    cfg.cache.index.auto = auto;
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).expect("doc.tex");
    Machine::analyze(source, Some(&path), &cfg)
}

fn register_of(analysis: &Analysis, name: &str) -> Option<u16> {
    let sym = analysis.interner.lookup(name)?;
    match analysis.env.meaning(sym) {
        satex::tex::Meaning::Register(_, index) => Some(index),
        _ => None,
    }
}

#[test]
fn a_register_the_package_allocates_is_allocated_again_after_other_allocations() {
    if !installed() {
        return;
    }
    let dir = project("registers");
    // The kernel is cached first, as `satex cache build` does: a run that
    // interprets latex.ltx itself leaves a state no later run starts from.
    run_latex(&dir, "\\relax\n", true);
    let first =
        run_latex(&dir, "\\documentclass{article}\n\\usepackage{outer}\n\\begin{document}\\end{document}\n", true);
    let other = "\\documentclass{article}\n\\newcount\\mine\\newcount\\yours\n\\usepackage{outer}\n\\begin{document}\\end{document}\n";
    let warm = run_latex(&dir, other, true);
    assert_eq!(served(&warm, "outer"), Some(true), "{:?}", warm.package_caches);
    let (before, after) = (register_of(&first, "outercount"), register_of(&warm, "outercount"));
    assert_eq!(before.map(|n| n + 2), after, "the register is renumbered after the two the preamble took");
    // The kernel cache stays: a run that interprets latex.ltx reports the
    // kernel's own diagnostics too, which says nothing about packages.
    for entry in std::fs::read_dir(dir.join("cache")).expect("cache").flatten() {
        if entry.file_name().to_string_lossy().starts_with("package-") {
            std::fs::remove_file(entry.path()).expect("removed");
        }
    }
    let cold = run_latex(&dir, other, false);
    assert_eq!(register_of(&cold, "outercount"), after);
    same_facts(&cold, &warm);
}

/// A run that reads every package, from the kernel cache only.
fn run_cold(dir: &Path, source: &str) -> Analysis {
    let mut cfg = latex(dir);
    cfg.cache.index.auto = false;
    cfg.cache.index.refresh = true;
    analyze(dir, source, &cfg)
}

const BASE: &[&str] = &[
    "\\documentclass[11pt]{article}",
    "\\usepackage{amsmath}",
    "\\usepackage{graphicx}",
    "\\usepackage{xcolor}",
    "\\begin{document}x\\end{document}",
];

/// Warm the caches with `base`, then run `edited` and check which packages
/// are served whole from their cache, and that the run reports exactly what
/// a run that reads every package does.
fn after_edit(name: &str, base: &[String], edited: &[String], hit: &[&str], miss: &[&str]) {
    if !installed() {
        return;
    }
    let dir = project(name);
    run_latex(&dir, "\\relax\n", true);
    run_latex(&dir, &base.join("\n"), true);
    let source = edited.join("\n");
    let warm = run_latex(&dir, &source, true);
    for package in hit {
        assert_eq!(served(&warm, package), Some(true), "{package} is served after {name}: {:?}", warm.package_caches);
    }
    for package in miss {
        assert_ne!(
            served(&warm, package),
            Some(true),
            "{package} is read again after {name}: {:?}",
            warm.package_caches
        );
    }
    let cold = run_cold(&dir, &source);
    same_facts(&cold, &warm);
}

fn lines(from: &[&str]) -> Vec<String> {
    from.iter().map(|l| l.to_string()).collect()
}

fn edit(from: &[&str], at: usize, remove: usize, insert: &[&str]) -> Vec<String> {
    let mut lines = lines(from);
    lines.splice(at..at + remove, insert.iter().map(|l| l.to_string()));
    lines
}

#[test]
fn an_unrelated_package_added_before_or_after_keeps_the_others_served() {
    let before = edit(BASE, 1, 0, &["\\usepackage{xspace}"]);
    after_edit("added-before", &lines(BASE), &before, &["article", "amsmath", "graphicx", "xcolor"], &[]);
    let after = edit(BASE, 4, 0, &["\\usepackage{xspace}"]);
    after_edit("added-after", &lines(BASE), &after, &["article", "amsmath", "graphicx", "xcolor"], &[]);
}

#[test]
fn an_unrelated_package_removed_or_moved_keeps_the_others_served() {
    let with = edit(BASE, 2, 0, &["\\usepackage{xspace}"]);
    after_edit("removed", &with, &lines(BASE), &["article", "amsmath", "graphicx", "xcolor"], &[]);
    let moved = edit(BASE, 1, 0, &["\\usepackage{xspace}"]);
    after_edit("moved", &with, &moved, &["article", "amsmath", "graphicx", "xcolor"], &[]);
}

/// The kernel's `\ProcessOptions` looks for each declared option in the
/// class options (`\in@` over `\@classoptionslist`), so a package that
/// declares options reads the list and is read again when it changes; one
/// that declares none never looks at it and stays served.
#[test]
fn class_options_are_read_by_the_packages_that_declare_options() {
    let base = edit(BASE, 1, 0, &["\\usepackage{etoolbox}"]);
    let larger = edit(&base.iter().map(String::as_str).collect::<Vec<_>>(), 0, 1, &["\\documentclass[12pt]{article}"]);
    after_edit("class-options", &base, &larger, &["etoolbox"], &["article", "amsmath", "graphicx", "xcolor"]);
}

#[test]
fn preamble_definitions_and_the_body_a_package_does_not_read_keep_it_served() {
    let all = ["article", "amsmath", "graphicx", "xcolor"];
    let defined = edit(BASE, 1, 0, &["\\newcommand\\foo{x}\\setlength\\parskip{1pt}\\def\\definecolor{}"]);
    after_edit("preamble", &lines(BASE), &defined, &all, &[]);
    let body = edit(BASE, 4, 1, &["\\begin{document}\\textcolor{red}{y}\\end{document}"]);
    after_edit("body", &lines(BASE), &body, &all, &[]);
}

#[test]
fn an_option_of_the_package_itself_reads_it_again() {
    let table = edit(BASE, 3, 1, &["\\usepackage[table]{xcolor}"]);
    after_edit("own-option", &lines(BASE), &table, &["article", "amsmath", "graphicx"], &["xcolor"]);
    let passed = edit(BASE, 3, 0, &["\\PassOptionsToPackage{table}{xcolor}"]);
    after_edit("passed-option", &lines(BASE), &passed, &["article", "amsmath", "graphicx"], &["xcolor"]);
}

#[test]
fn a_preamble_definition_of_a_name_the_package_reads_reads_it_again() {
    let defined = edit(BASE, 1, 0, &["\\makeatletter\\def\\Gin@driver{dvips.def}\\makeatother"]);
    after_edit("read-name", &lines(BASE), &defined, &["article", "amsmath", "xcolor"], &["graphicx"]);
}

/// The characters of `name`'s replacement text.
fn text_of(analysis: &Analysis, name: &str) -> Option<String> {
    let sym = analysis.interner.lookup(name)?;
    let meaning = analysis.env.meaning(sym);
    let text = &meaning.as_macro()?.replacement_text;
    Some(
        text.iter()
            .filter_map(|t| match t.tok {
                satex::tex::Tok::Chr(c, _) => Some(c),
                _ => None,
            })
            .collect(),
    )
}

/// A package that appends to a list with `\xdef\list{\list,…}` copies the
/// list's old text through without looking at it: after a different list,
/// it is served, and the list is what reading the package makes of it.
#[test]
fn a_list_the_package_appends_to_unread_keeps_it_served() {
    if !installed() {
        return;
    }
    let dir = project("copy-through");
    std::fs::write(
        dir.join("appender.sty"),
        "\\ProvidesPackage{appender}\n\\xdef\\biglist{\\biglist,appender}\n\\def\\appended{}\n",
    )
    .expect("appender.sty");
    let doc = |list: &str| {
        format!(
            "\\documentclass{{article}}\n\\gdef\\biglist{{{list}}}\n\\usepackage{{appender}}\n\\begin{{document}}\\end{{document}}\n"
        )
    };
    run_latex(&dir, "\\relax\n", true);
    run_latex(&dir, &doc("a"), true);
    let source = doc("b,c");
    let warm = run_latex(&dir, &source, true);
    assert_eq!(served(&warm, "appender"), Some(true), "{:?}", warm.package_caches);
    assert_eq!(text_of(&warm, "biglist").as_deref(), Some("b,c,appender"));
    let cold = run_cold(&dir, &source);
    assert_eq!(text_of(&cold, "biglist"), text_of(&warm, "biglist"));
    same_facts(&cold, &warm);
}

/// A package that looks at the list's old text before appending to it reads
/// it: after a different list, it is read again.
#[test]
fn a_list_the_package_inspects_before_appending_makes_it_miss() {
    if !installed() {
        return;
    }
    let dir = project("copy-inspected");
    std::fs::write(
        dir.join("inspector.sty"),
        "\\ProvidesPackage{inspector}\n\\def\\reference{a}\n\\ifx\\biglist\\reference\\def\\wasa{}\\fi\n\
         \\xdef\\biglist{\\biglist,inspector}\n",
    )
    .expect("inspector.sty");
    let doc = |list: &str| {
        format!(
            "\\documentclass{{article}}\n\\gdef\\biglist{{{list}}}\n\\usepackage{{inspector}}\n\\begin{{document}}\\end{{document}}\n"
        )
    };
    run_latex(&dir, "\\relax\n", true);
    run_latex(&dir, &doc("a"), true);
    let source = doc("b");
    let warm = run_latex(&dir, &source, true);
    assert_ne!(served(&warm, "inspector"), Some(true), "{:?}", warm.package_caches);
    assert!(!defined(&warm, "wasa"));
    assert_eq!(text_of(&warm, "biglist").as_deref(), Some("b,inspector"));
    same_facts(&run_cold(&dir, &source), &warm);
}

/// A `\input` name that only exists on disk under different case is a
/// `LoadStatus::CaseMismatch { on_disk }`, and `unresolved-file` reports it
/// with the exact name found on disk, not just "not found". (`\usepackage`
/// checks existence with `\IfFileExists`/`\openin` first, exactly like a
/// real case-sensitive engine would, so it never reaches `\input` for a
/// case-mismatched name at all; a bare `\input` has no such gate.)
#[test]
fn a_case_mismatched_load_reports_the_disk_name_in_the_lint() {
    if !installed() {
        return;
    }
    let dir = project("case-mismatch-lint");
    std::fs::write(dir.join("Mismatch.sty"), "\\ProvidesPackage{Mismatch}[2020/01/01 v1]\n\\def\\mismatchmacro{}\n")
        .expect("Mismatch.sty");
    // `\input{mismatch}` (braced) goes through the kernel's `\@iinput`,
    // which checks existence with `\IfFileExists`/`\openin` first, exactly
    // like `\usepackage` does; on this host that check is honestly
    // case-sensitive and so genuinely fails, matching a real engine, and
    // `\input` is never reached. `\@@input` is the raw primitive latex.ltx
    // keeps aside (`\let\@@input\input`) precisely for this unconditional
    // form, so it is what exercises the loader's own case-fold detection.
    let analysis =
        run(&dir, "\\documentclass{article}\n\\makeatletter\n\\begin{document}\\@@input mismatch \\end{document}\n");

    let load = analysis.facts.loads.iter().find(|l| l.name == "mismatch").expect("a load for `mismatch`");
    let LoadStatus::CaseMismatch { on_disk } = load.status else {
        panic!("expected a case-mismatch status, got {:?}", load.status);
    };
    assert_eq!(analysis.interner.name(on_disk), "Mismatch.sty");

    let findings = satex::lint::lint(&analysis);
    let finding = findings
        .iter()
        .find(|r| r.get("code").and_then(|c| c.as_str()) == Some("unresolved-file"))
        .expect("an unresolved-file finding");
    assert_eq!(
        finding.get("message").and_then(|m| m.as_str()),
        Some("exists as `Mismatch.sty` but was asked for as `mismatch`: works here, breaks on case-sensitive systems")
    );
    assert_eq!(
        finding.get("fix").and_then(|m| m.as_str()),
        Some("rename `Mismatch.sty` to match the requested name `mismatch`")
    );
}

/// The case-mismatch payload is carried on the side in `CachedLoad`
/// (`case_mismatch_on_disk`), not inside the index-based `STATUSES` array,
/// so it must survive a package-cache round trip: cache a package whose
/// body case-mismatches a load, replay it, and check the disk name the
/// replay reports is still exactly right.
#[test]
fn a_case_mismatch_survives_a_package_cache_round_trip() {
    if !installed() {
        return;
    }
    let dir = project("case-mismatch-cache");
    std::fs::write(dir.join("Mismatch.sty"), "\\ProvidesPackage{Mismatch}[2020/01/01 v1]\n\\def\\mismatchmacro{}\n")
        .expect("Mismatch.sty");
    std::fs::write(
        dir.join("container.sty"),
        "\\ProvidesPackage{container}[2020/01/01 v1]\n\\@@input mismatch \n\\def\\containermacro{}\n",
    )
    .expect("container.sty");
    let source = "\\documentclass{article}\n\\usepackage{container}\n\\begin{document}\\end{document}\n";

    let cold = run(&dir, source);
    assert_eq!(served(&cold, "container"), Some(false), "{:?}", cold.package_caches);
    let warm = run(&dir, source);
    assert_eq!(served(&warm, "container"), Some(true), "{:?}", warm.package_caches);

    for (analysis, label) in [(&cold, "cold"), (&warm, "warm")] {
        let load = analysis
            .facts
            .loads
            .iter()
            .find(|l| l.name == "mismatch")
            .unwrap_or_else(|| panic!("{label}: no load for `mismatch`"));
        let LoadStatus::CaseMismatch { on_disk } = load.status else {
            panic!("{label}: expected a case-mismatch status, got {:?}", load.status);
        };
        assert_eq!(analysis.interner.name(on_disk), "Mismatch.sty", "{label} run");
    }
}

/// A cache is keyed by what changes the interpretation only: a run that
/// reports, lints or traces differently, has other resource limits, or
/// names the engine it would pick anyway reads the same cache.
#[test]
fn settings_that_do_not_change_the_interpretation_share_a_cache() {
    let dir = project("key");
    let first = run(&dir, DOCUMENT);
    assert_eq!(served(&first, "outer"), Some(false), "{:?}", first.package_caches);
    let mut other = latex(&dir);
    other.verbose = 2;
    other.lint_off = vec!["unused-label".into()];
    other.profile = Some(satex::config::Profile::Document);
    other.engine = Some(first.plugins.engine);
    other.limits.memory = bytesize::ByteSize::gib(3);
    other.limits.degrade_at_percent = 90;
    let second = analyze(&dir, DOCUMENT, &other);
    assert_eq!(served(&second, "outer"), Some(true), "{:?}", second.package_caches);
}

#[test]
fn a_local_class_is_reused_by_a_second_run_and_the_report_says_so() {
    if !installed() {
        return;
    }
    let dir = project("class-reuse");
    std::fs::write(dir.join("part.tex"), "\\def\\frompart{x}\n").expect("part.tex");
    std::fs::write(
        dir.join("mine.cls"),
        "\\NeedsTeXFormat{LaTeX2e}\\ProvidesClass{mine}[2024/01/01]\n\\DeclareOption*{\\PassOptionsToClass{\\CurrentOption}{article}}\n\
         \\ProcessOptions\\relax\n\\LoadClass{article}\n\\RequirePackage{inner}\n\
         \\IfFileExists{missing.sty}{\\def\\there{}}{\\def\\gone{}}\n\\InputIfFileExists{part.tex}{}{}\n",
    )
    .expect("mine.cls");
    let source = "\\documentclass{mine}\n\\begin{document}\\frompart\\end{document}\n";
    let first = run(&dir, source);
    assert!(first.cache_report().iter().any(|l| l.contains("class mine: rebuilt")), "{:?}", first.cache_report());
    let second = run(&dir, source);
    let report = second.cache_report();
    assert!(report.iter().any(|l| l == "cache class mine: reused"), "{report:?}");
    assert_eq!(served(&second, "mine"), Some(true), "{report:?}");
    std::fs::write(dir.join("part.tex"), "\\def\\frompart{yy}\n").expect("part.tex");
    let third = run(&dir, source);
    assert!(
        third.cache_report().iter().any(|l| l.contains("class mine: rebuilt") && l.contains("content changed")),
        "{:?}",
        third.cache_report()
    );
}

/// A package that reads the clock is replayed from its cache with the
/// cached date. The replay says so in the one clock assumption, not in a
/// diagnostic of its own, so the cached run diagnoses as the cold one.
#[test]
fn a_cached_package_that_read_the_clock_diagnoses_as_the_cold_run() {
    if !installed() {
        return;
    }
    let dir = project("clock");
    std::fs::write(dir.join("dated.sty"), "\\ProvidesPackage{dated}\n\\edef\\stamp{\\the\\year}\n").expect("dated.sty");
    let source = "\\documentclass{article}\\usepackage{dated}\\begin{document}\\stamp\\end{document}\n";
    let codes = |a: &Analysis| {
        a.facts.diagnostics.iter().map(|d| format!("{}|{}|{}", d.code, d.span.line, d.message)).collect::<Vec<_>>()
    };
    let built = run(&dir, source);
    let read = run(&dir, source);
    assert!(read.cache_report().iter().any(|l| l.contains("segments: 1 reused")), "{:?}", read.cache_report());
    assert_eq!(codes(&built), codes(&read));
}

fn files_of(cache: &Path) -> Vec<String> {
    let mut names: Vec<String> =
        std::fs::read_dir(cache).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

fn aged(path: &Path, bytes: u64) {
    let file = std::fs::File::options().create(true).append(true).open(path).unwrap();
    file.set_len(bytes).unwrap();
    file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600)).unwrap();
}

/// A cache file of this build, as a template for stale neighbours.
fn own_package_cache(cache: &Path) -> String {
    files_of(cache)
        .into_iter()
        .find(|n| n.starts_with("package-outer-v") && n.ends_with(".postcard"))
        .expect("outer cached")
}

#[test]
fn a_write_removes_what_another_build_or_encoding_left() {
    if !installed() {
        return;
    }
    let dir = project("evict-stale");
    run_latex(&dir, DOCUMENT, true);
    let cache = dir.join("cache");
    let own = own_package_cache(&cache);
    let (family, rest) = own.split_once("-v").unwrap();
    let (version, tail) = rest.split_once('-').unwrap();
    let (_digest, key) = tail.split_once('-').unwrap();
    let foreign = format!("{family}-v{version}-deadbeefdeadbeef-{key}");
    let old_encoding = format!("{family}-v1-{}", tail);
    for name in [&foreign, &old_encoding] {
        aged(&cache.join(name), 10);
        aged(&cache.join(name.replace(".postcard", ".lock")), 0);
    }
    let orphan = "package-gone-v1-0123456789abcdef-0123456789abcdef.lock";
    aged(&cache.join(orphan), 0);
    // A change to a package makes the next run write a cache.
    std::fs::write(dir.join("inner.sty"), "\\ProvidesPackage{inner}\n\\def\\innermacro#1{}\n").unwrap();
    run_latex(&dir, DOCUMENT, true);
    let left = files_of(&cache);
    for gone in
        [&foreign, &old_encoding, &foreign.replace(".postcard", ".lock"), &old_encoding.replace(".postcard", ".lock")]
    {
        assert!(!left.contains(gone), "{gone} survived a write: {left:?}");
    }
    assert!(!left.iter().any(|n| n == orphan), "a lock nothing holds survived: {left:?}");
    assert!(left.contains(&own) || left.iter().any(|n| n.starts_with("package-outer-v")), "{left:?}");
}

#[test]
fn the_cache_directory_stays_within_its_size_bound() {
    if !installed() {
        return;
    }
    let dir = project("evict-bound");
    let mut cfg = latex(&dir);
    run_latex(&dir, DOCUMENT, true);
    let cache = dir.join("cache");
    let own = own_package_cache(&cache);
    let (family, tail) = own.split_once("-v").unwrap();
    let (version, rest) = tail.split_once('-').unwrap();
    let (digest, _) = rest.split_once('-').unwrap();
    // Older neighbours of this build, big enough to burst the bound.
    for (i, age) in [3000u64, 2000, 1000].iter().enumerate() {
        let name = format!("{family}-v{version}-{digest}-{i:016x}.postcard");
        let path = cache.join(&name);
        aged(&path, 1_000_000);
        let file = std::fs::File::options().append(true).open(&path).unwrap();
        file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(*age)).unwrap();
    }
    let size = |dir: &Path| -> u64 {
        std::fs::read_dir(dir).unwrap().flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()
    };
    let kernel: u64 = files_of(&cache)
        .iter()
        .filter(|n| n.starts_with("format-"))
        .map(|n| std::fs::metadata(cache.join(n)).unwrap().len())
        .sum();
    let bound = kernel + 1_700_000;
    cfg.limits.cache_size = bytesize::ByteSize::b(bound);
    std::fs::write(dir.join("inner.sty"), "\\ProvidesPackage{inner}\n\\def\\innermacro#1{}\n").unwrap();
    let path = dir.join("doc.tex");
    std::fs::write(&path, DOCUMENT).unwrap();
    Machine::analyze(DOCUMENT, Some(&path), &cfg);
    let left = files_of(&cache);
    let sizes: Vec<(String, u64)> =
        left.iter().map(|n| (n.clone(), std::fs::metadata(cache.join(n)).map_or(0, |m| m.len()))).collect();
    assert!(size(&cache) <= bound, "{} > {bound}: {sizes:?}", size(&cache));
    assert!(left.iter().any(|n| n.starts_with("format-")), "the kernel cache is never evicted: {left:?}");
    assert!(
        !left.iter().any(|n| n.contains("0000000000000000.postcard")),
        "the least recently used goes first: {left:?}"
    );
}

/// A class that assigns only the value of a register (a box, an uccode) is
/// read again by the next segment from the state replayed so far: the
/// replay agrees with the live run, so runs after the first reuse the cache
/// whole and the cache file stops growing.
#[test]
fn a_class_assigning_register_values_is_reused_whole() {
    let dir = project("register-values");
    let cfg = latex(&dir);
    let source = "\\documentclass{scrbook}\n\\begin{document}x\\end{document}\n";
    let class_file = |dir: &Path| -> (PathBuf, u64) {
        std::fs::read_dir(dir.join("cache"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("package-scrbook-"))
            .filter(|e| e.file_name().to_string_lossy().ends_with(".postcard"))
            .map(|e| (e.path(), e.metadata().unwrap().len()))
            .next()
            .expect("the class cache file")
    };
    let first = analyze(&dir, source, &cfg);
    let (_, stored) = class_file(&dir);
    assert_eq!(served(&first, "scrbook"), Some(false), "{:?}", first.package_caches);
    for run in 2..=3 {
        let again = analyze(&dir, source, &cfg);
        let state = again.package_caches.iter().rev().find(|(n, ..)| n == "scrbook").map(|(.., s)| s.clone());
        assert_eq!(state.as_deref(), Some("cached"), "run {run}: {:?}", again.package_caches);
        let (_, size) = class_file(&dir);
        assert_eq!(size, stored, "run {run}: the cache file grew");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Caches carry paths relative to their tree and check file contents, not
/// times: a tree copied elsewhere, with new modification times, reuses them,
/// and an edit in the copy is still seen.
#[test]
fn caches_built_against_one_tree_are_reused_from_a_copy_and_an_edit_rebuilds_them() {
    let dir = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-package-cache-moved-{}", std::process::id())),
    );
    let (first, cache, doc) = (dir.join("a"), dir.join("cache"), dir.join("doc"));
    let tree_a = crate::support::mini_texmf(&first);
    std::fs::create_dir_all(&doc).expect("doc");
    let source = "\\usepackage{demo}\n\\demomacro\n";
    let run = |tree: &Path| {
        let path = doc.join("doc.tex");
        std::fs::write(&path, source).expect("doc.tex");
        Machine::analyze(source, Some(&path), &crate::support::mini_config(tree, &cache))
    };
    let built = run(&tree_a);
    assert_eq!(served(&built, "demo"), Some(false), "{:?}", built.package_caches);
    // Copied without `-p`, and the original gone: nothing can be read from it.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let second = dir.join("b");
    let copied = std::process::Command::new("cp").arg("-r").arg(&first).arg(&second).status().expect("cp");
    assert!(copied.success());
    std::fs::remove_dir_all(&first).expect("the first tree removed");
    let tree_b = second.join("texmf-dist");
    let moved = run(&tree_b);
    let report = moved.cache_report();
    assert!(report.iter().any(|l| l.ends_with("kernel pdftex: reused")), "{report:?}");
    assert_eq!(served(&moved, "demo"), Some(true), "{:?}", moved.package_caches);
    assert!(defined(&moved, "demomacro") && defined(&moved, "kernelend"));
    // Provenance resolves to the copy.
    let site = crate::support::definition(&moved, "demomacro").expect("\\demomacro defined");
    assert!(
        moved.file_name(site.span.file).starts_with(&*tree_b.to_string_lossy()),
        "{}",
        moved.file_name(site.span.file)
    );
    let kernel = crate::support::definition(&moved, "kernelmacro").map(|d| moved.file_name(d.span.file).to_string());
    assert!(kernel.as_deref().is_none_or(|k| k.starts_with(&*tree_b.to_string_lossy())), "{kernel:?}");
    // An edit in the copy.
    let package = tree_b.join("tex/latex/demo/demo.sty");
    let text = std::fs::read_to_string(&package).expect("demo.sty");
    std::fs::write(&package, text.replace("\\def\\demoend{}", "\\def\\demoended{}")).expect("demo.sty");
    let edited = run(&tree_b);
    let report = edited.cache_report();
    assert!(
        report.iter().any(|l| l.contains("package demo: rebuilt (content changed: ") && l.contains("demo.sty")),
        "{report:?}"
    );
    assert!(defined(&edited, "demoended"));
    let kernel = tree_b.join("tex/latex/base/minipart.ltx");
    std::fs::write(&kernel, "\\def\\fromkernelpart#1{\\kernelmacro{#1#1}}\n").expect("minipart.ltx");
    let report = run(&tree_b).cache_report();
    assert!(
        report.iter().any(|l| l.contains("kernel pdftex: rebuilt (content changed: ") && l.contains("minipart.ltx")),
        "{report:?}"
    );
}

/// Trees A, a cache dir built against it, and a document, for the archive tests.
struct Portable {
    dir: PathBuf,
    tree: PathBuf,
    cache: PathBuf,
    archive: PathBuf,
}

impl Portable {
    fn new(name: &str) -> Portable {
        let dir = crate::support::scratch_dir(
            crate::support::tmp_root().join(format!("satex-package-cache-{name}-{}", std::process::id())),
        );
        let tree = crate::support::mini_texmf(&dir.join("a"));
        let cache = dir.join("cache");
        let archive = dir.join("caches.bin");
        std::fs::create_dir_all(dir.join("doc")).expect("doc");
        Portable { dir, tree, cache, archive }
    }

    fn run(&self, cfg: &Config) -> Analysis {
        let source = "\\usepackage{demo}\n\\demomacro\n";
        let path = self.dir.join("doc/doc.tex");
        std::fs::write(&path, source).expect("doc.tex");
        Machine::analyze(source, Some(&path), cfg)
    }

    fn built(&self) -> Config {
        let cfg = crate::support::mini_config(&self.tree, &self.cache);
        let first = self.run(&cfg);
        assert_eq!(served(&first, "demo"), Some(false), "{:?}", first.package_caches);
        cfg
    }

    fn export(&self, cfg: &Config) {
        let mut out = Vec::new();
        satex::cmd::cache::export(cfg, &self.archive, (&[], &[]), false, &mut out).expect("export");
    }

    fn import(&self, cfg: &Config) -> String {
        let mut out = Vec::new();
        satex::cmd::cache::import(cfg, &self.archive, false, &mut out).expect("import");
        String::from_utf8_lossy(&out).into_owned()
    }
}

fn all_reused(analysis: &Analysis) {
    let report = analysis.cache_report();
    assert!(report.iter().any(|l| l.ends_with("kernel pdftex: reused")), "{report:?}");
    assert_eq!(served(analysis, "demo"), Some(true), "{:?}", analysis.package_caches);
    assert!(report.iter().all(|l| !l.contains("rebuilt")), "{report:?}");
}

/// An exported archive, imported after the cache directory is gone, serves every cache.
#[test]
fn an_exported_cache_archive_is_reused_after_import() {
    let p = Portable::new("archive");
    let cfg = p.built();
    p.export(&cfg);
    std::fs::remove_dir_all(&p.cache).expect("cache wiped");
    let said = p.import(&cfg);
    assert!(said.contains("imported ") && !said.contains("skipped"), "{said}");
    all_reused(&p.run(&cfg));
}

/// A tree with one file changed rebuilds that cache after import.
#[test]
fn an_imported_cache_is_rebuilt_when_the_tree_differs() {
    let p = Portable::new("archive-edit");
    let cfg = p.built();
    p.export(&cfg);
    std::fs::remove_dir_all(&p.cache).expect("cache wiped");
    p.import(&cfg);
    let package = p.tree.join("tex/latex/demo/demo.sty");
    let text = std::fs::read_to_string(&package).expect("demo.sty");
    std::fs::write(&package, text.replace("\\def\\demoend{}", "\\def\\demoended{}")).expect("demo.sty");
    let report = p.run(&cfg).cache_report();
    assert!(report.iter().any(|l| l.contains("package demo: rebuilt (content changed: ")), "{report:?}");
}

/// An archive of another semantic version is skipped, with a line saying so.
#[test]
fn an_archive_of_another_semantic_version_is_skipped() {
    let p = Portable::new("archive-version");
    let cfg = p.built();
    p.export(&cfg);
    // The version is the first field after the magic string.
    let mut bytes = std::fs::read(&p.archive).expect("archive");
    let at = bytes.windows(19).position(|w| w == b"satex-cache-archive").expect("magic") + 19;
    bytes[at] = bytes[at].wrapping_add(1);
    std::fs::write(&p.archive, bytes).expect("archive");
    std::fs::remove_dir_all(&p.cache).expect("cache wiped");
    let said = p.import(&cfg);
    assert!(said.contains("skipped ") && said.contains("built for semantic version"), "{said}");
    assert!(said.contains("imported 0 cache file(s)"), "{said}");
    let report = p.run(&cfg).cache_report();
    assert!(report.iter().any(|l| l.contains("kernel pdftex: rebuilt")), "{report:?}");
}

/// A bundle as the only root, with the imported caches: no installation needed.
#[test]
fn a_bundle_is_the_only_root_and_reuses_the_imported_caches() {
    let p = Portable::new("archive-bundle");
    let cfg = p.built();
    p.export(&cfg);
    let bundle = p.dir.join("bundle");
    let copied = std::process::Command::new("cp").arg("-r").arg(&p.tree).arg(&bundle).status().expect("cp");
    assert!(copied.success());
    std::fs::remove_dir_all(p.dir.join("a")).expect("the first tree removed");
    std::fs::remove_dir_all(&p.cache).expect("cache wiped");
    let mut cfg = crate::support::mini_config(&bundle, &p.cache);
    cfg.texmf.roots.clear();
    cfg.texmf.bundle = Some(bundle.clone());
    p.import(&cfg);
    let analysis = p.run(&cfg);
    all_reused(&analysis);
    assert_eq!(analysis.distribution.roots, vec![bundle]);
    assert!(defined(&analysis, "demomacro") && defined(&analysis, "kernelend"));
}

/// Caches built against an installation of several trees serve a bundle that
/// holds the same files: roots and their tags are no part of the input.
#[test]
fn caches_built_against_several_trees_serve_a_bundle_alone() {
    let p = Portable::new("archive-trees");
    let local = p.dir.join("texmf-local");
    std::fs::create_dir_all(&local).expect("an empty local tree");
    let mut cfg = crate::support::mini_config(&p.tree, &p.cache);
    cfg.texmf.roots = vec![local, p.tree.clone()];
    let first = p.run(&cfg);
    assert_eq!(served(&first, "demo"), Some(false), "{:?}", first.package_caches);
    p.export(&cfg);
    let bundle = p.dir.join("bundle");
    let copied = std::process::Command::new("cp").arg("-r").arg(&p.tree).arg(&bundle).status().expect("cp");
    assert!(copied.success());
    std::fs::remove_dir_all(&p.cache).expect("cache wiped");
    let mut cfg = crate::support::mini_config(&bundle, &p.cache);
    cfg.texmf.roots.clear();
    cfg.texmf.bundle = Some(bundle);
    p.import(&cfg);
    let analysis = p.run(&cfg);
    assert_eq!(served(&analysis, "demo"), Some(true), "{:?}", analysis.package_caches);
}

/// A kernel's cache family comes from its own release, not from its path.
#[test]
fn a_kernel_is_named_by_its_release_not_its_path() {
    let dir = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-release-name-{}", std::process::id())),
    );
    let kernel = dir.join("latex.ltx");
    std::fs::write(&kernel, "\\edef\\fmtversion{2031-06-01}\n").expect("kernel");
    assert_eq!(satex::format::release_name(&kernel), "2031");
    let bare = dir.join("other.ltx");
    std::fs::write(&bare, "\\relax\n").expect("kernel");
    assert_eq!(satex::format::release_name(&bare), satex::format::tree_name(&bare));
}

/// No state from an interrupted run: every package cache entry of `a`.
fn no_leftover_misses(a: &Analysis) {
    for (name, _, state) in &a.package_caches {
        assert!(!state.contains("root differs"), "{name}: {state}");
    }
}

#[test]
fn a_run_that_lowered_its_precision_stores_nothing_the_next_run_replays() {
    let dir = project("degraded");
    let mut cfg = latex(&dir);
    cfg.limits.degrade_at_percent = 0;
    let degraded = analyze(&dir, DOCUMENT, &cfg);
    assert_eq!(served(&degraded, "outer"), None, "{:?}", degraded.package_caches);
    let next = run(&dir, DOCUMENT);
    no_leftover_misses(&next);
    assert_eq!(served(&next, "outer"), Some(false), "{:?}", next.package_caches);
    assert_eq!(served(&run(&dir, DOCUMENT), "outer"), Some(true));
}

#[test]
fn a_cache_file_cut_short_by_a_killed_run_is_replaced() {
    let dir = project("killed");
    let first = run(&dir, DOCUMENT);
    let (_, cache, _) = first.package_caches.iter().find(|(name, ..)| name == "outer").expect("outer cached");
    let cache = PathBuf::from(cache);
    let bytes = std::fs::read(&cache).expect("cache file");
    std::fs::write(&cache, &bytes[..bytes.len() / 2]).expect("truncated");
    std::fs::write(cache.with_extension("1-0.tmp"), &bytes[..10]).expect("temporary");
    let next = run(&dir, DOCUMENT);
    no_leftover_misses(&next);
    assert_eq!(served(&next, "outer"), Some(false), "{:?}", next.package_caches);
    let third = run(&dir, DOCUMENT);
    assert_eq!(served(&third, "outer"), Some(true), "{:?}", third.package_caches);
    same_facts(&first, &third);
}

/// Paths, joins and loop heads per line of the document itself.
fn path_stats(analysis: &Analysis) -> Vec<(&'static str, u32, u64)> {
    let Some(hot) = &analysis.hot else { return Vec::new() };
    let doc = |file| analysis.file_name(file).ends_with("doc.tex");
    let mut out: Vec<_> = [("fork", &hot.forks), ("join", &hot.joins), ("head", &hot.heads)]
        .into_iter()
        .flat_map(|(kind, table)| {
            table.iter().filter(|((file, _), _)| doc(*file)).map(move |((_, line), n)| (kind, *line, *n))
        })
        .collect();
    out.sort();
    out
}

/// A package read, stored, or replayed from its cache leaves the same state:
/// the document after three packages reports the same findings and takes
/// the same paths in each case.
#[test]
fn three_packages_read_or_replayed_give_the_same_findings_and_paths() {
    if !installed() {
        return;
    }
    satex::machine::HOT.store(true, std::sync::atomic::Ordering::Relaxed);
    let dir = project("three");
    let source = "\\documentclass{article}\n\\usepackage{tikz}\n\\usepackage{booktabs}\n\\usepackage{soul}\n\
                  \\begin{document}\nSome text -- with ``quotes''.\n\
                  \\begin{tikzpicture}\n\\node[draw, rounded corners] at (0,1) {\\rule{1pt}{\\pdfrandomseed sp}A};\n\
                  \\node[draw] at (0,2) {\\rule{1pt}{\\pdfrandomseed sp}B};\n\\end{tikzpicture}\n\
                  \\pgfmathparse{\\pdfuniformdeviate 100}\\pgfmathparse{\\pdfuniformdeviate 100}\\pgfmathparse{\\pdfuniformdeviate 100}\n\
                  \\begin{tabular}{ll}\\toprule a & b\\\\\\bottomrule\\end{tabular}\n\\hl{marked} text.\n\\end{document}\n";
    let with = |auto: bool, refresh: bool| {
        let mut cfg = latex(&dir);
        cfg.set("clock", "2026-01-01T12:00:00Z").expect("clock");
        cfg.cache.index.auto = auto;
        cfg.cache.index.refresh = refresh;
        analyze(&dir, source, &cfg)
    };
    let read = with(false, true);
    let stored = with(true, false);
    let replayed = with(true, false);
    for package in ["tikz", "booktabs", "soul"] {
        assert_eq!(served(&stored, package), Some(false), "{package} stored: {:?}", stored.package_caches);
        assert_eq!(served(&replayed, package), Some(true), "{package} replayed: {:?}", replayed.package_caches);
    }
    let findings = |a: &Analysis| serde_json::to_string(&satex::lint::lint(a)).unwrap_or_default();
    assert_eq!(findings(&read), findings(&stored));
    assert_eq!(findings(&stored), findings(&replayed));
    assert_eq!(path_stats(&read), path_stats(&stored));
    assert_eq!(path_stats(&stored), path_stats(&replayed));
}

/// A replayed package is the work of its own lines, not of the line that
/// loads it: the termination guard (`limits.line_steps`) charges the
/// replay to no line, so a package whose every line is cheap never cuts
/// the `\usepackage` line that replays it.
#[test]
fn a_replay_is_not_charged_to_the_line_that_loads_it() {
    let dir = project("guard");
    let line = "\\count@=0 \\loop\\advance\\count@ 1 \\ifnum\\count@<3000 \\repeat\n";
    std::fs::write(
        dir.join("busy.sty"),
        format!("\\ProvidesPackage{{busy}}\n{}\\def\\busydone{{}}\n", line.repeat(80)),
    )
    .expect("busy.sty");
    let source = "\\documentclass{article}\n\\usepackage{busy}\n\\begin{document}\\end{document}\n";
    let mut cfg = latex(&dir);
    cfg.cache.index.auto = true;
    cfg.limits.line_steps = 2_000_000;
    for (run, cached) in [("first", false), ("second", true)] {
        let a = analyze(&dir, source, &cfg);
        assert_eq!(served(&a, "busy"), Some(cached), "{run} run: {:?} {:?}", a.package_caches, diagnostics(&a));
        let cut: Vec<_> =
            diagnostics(&a).into_iter().filter(|(code, _)| code.contains("budget") || code.contains("limit")).collect();
        assert!(cut.is_empty(), "{run} run: {cut:?}");
        assert!(definitions(&a).iter().any(|(name, _)| name == "busydone"), "{run} run");
    }
}

/// A class that loads two packages, and a package of the same name loaded
/// by a document.
fn class_project(name: &str) -> PathBuf {
    let dir = project(name);
    std::fs::write(
        dir.join("mycls.cls"),
        "\\ProvidesClass{mycls}\n\\LoadClass{article}\n\\RequirePackage{pa}\n\\RequirePackage{pb}\n",
    )
    .expect("mycls.cls");
    std::fs::write(dir.join("pa.sty"), "\\ProvidesPackage{pa}\n\\def\\pamacro{a}\n").expect("pa.sty");
    std::fs::write(dir.join("pb.sty"), "\\ProvidesPackage{pb}\n\\def\\pbmacro{b}\n").expect("pb.sty");
    dir
}

const WITH_CLASS: &str = "\\documentclass{mycls}\n\\begin{document}\\pamacro\\pbmacro\\end{document}\n";

#[test]
fn each_package_a_class_loads_has_a_cache_entry_of_its_own() {
    if !installed() {
        return;
    }
    let dir = class_project("class-entries");
    run_latex(&dir, "\\relax\n", true);
    let first = run_latex(&dir, WITH_CLASS, true);
    for package in ["pa", "pb"] {
        assert_eq!(served(&first, package), Some(false), "{package}: {:?}", first.package_caches);
    }
    // Another class that loads one of them finds the entry the first class made.
    std::fs::write(dir.join("other.cls"), "\\ProvidesClass{other}\n\\LoadClass{article}\n\\RequirePackage{pb}\n")
        .unwrap();
    let source = "\\documentclass{other}\n\\begin{document}\\pbmacro\\end{document}\n";
    let other = run_latex(&dir, source, true);
    assert_eq!(served(&other, "pb"), Some(true), "{:?}", other.package_caches);
}

#[test]
fn a_changed_package_of_a_class_rebuilds_only_that_package() {
    if !installed() {
        return;
    }
    let dir = class_project("class-edit");
    run_latex(&dir, "\\relax\n", true);
    run_latex(&dir, WITH_CLASS, true);
    std::fs::write(dir.join("pa.sty"), "\\ProvidesPackage{pa}\n\\def\\pamacro{changed}\n").unwrap();
    let again = run_latex(&dir, WITH_CLASS, true);
    assert_eq!(served(&again, "pb"), Some(true), "{:?}", again.package_caches);
    assert_ne!(served(&again, "pa"), Some(true), "{:?}", again.package_caches);
    same_facts(&run_cold(&dir, WITH_CLASS), &again);
}

/// `--only` and `--without` keep the caches that hold, or do not hold, the named package.
#[test]
fn an_export_can_be_limited_to_a_package() {
    let p = Portable::new("archive-filter");
    let cfg = p.built();
    let count = |only: &[&str], without: &[&str]| {
        let (only, without): (Vec<String>, Vec<String>) =
            (only.iter().map(|s| s.to_string()).collect(), without.iter().map(|s| s.to_string()).collect());
        let mut out = Vec::new();
        satex::cmd::cache::export(&cfg, &p.archive, (&only, &without), true, &mut out).expect("export");
        let said: serde_json::Value = serde_json::from_slice(&out).expect("json");
        (said["entries"].as_u64().unwrap(), said["holds"].as_object().unwrap().keys().cloned().collect::<Vec<_>>())
    };
    let (all, held) = count(&[], &[]);
    assert!(held.iter().any(|n| n.starts_with("package-demo-")), "{held:?}");
    let (only, held) = count(&["demo"], &[]);
    assert_eq!((only, held.len() as u64), (1, 1), "{held:?}");
    let (without, held) = count(&[], &["demo"]);
    assert_eq!(without + only, all);
    assert!(held.iter().all(|n| !n.starts_with("package-demo-")), "{held:?}");
}

/// `\RequirePackage` on a package's last line looks past its end for `[`
/// while the load is still running: the package it names is read, on a
/// run that stores the cache and on one that replays it.
#[test]
fn a_package_loaded_on_the_last_line_is_read_cold_and_warm() {
    if !installed() {
        return;
    }
    let dir = class_project("last-line");
    run_latex(&dir, "\\relax\n", true);
    for run in ["cold", "warm"] {
        let analysis = run_latex(&dir, WITH_CLASS, true);
        let undefined: Vec<_> = satex::lint::lint(&analysis)
            .into_iter()
            .filter(|r| r["code"] == "undefined-control-sequence")
            .map(|r| r["message"].to_string())
            .collect();
        assert!(undefined.is_empty(), "{run}: {undefined:?}");
    }
}
