//! A package read once is replayed from its cache on the next run, and the
//! replay reports what reading it would have.

use std::path::{Path, PathBuf};

use satex::config::Config;
use satex::machine::{Analysis, Machine};

fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-package-cache-{name}-{}", std::process::id()));
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
    cfg.cache_index.auto = auto;
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
        let shared = std::env::temp_dir().join(format!("satex-package-cache-kernel-{}", std::process::id()));
        let cfg = Config { cache_dir: Some(shared.clone()), ..Config::default() };
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
    Config { cache_dir: Some(cache), ..Config::default() }
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
            "cached" => Some(true),
            "stored" => Some(false),
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

fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
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
        again.package_caches.iter().any(|(name, _, state)| name == "outer" && state.contains("inner.sty changed")),
        "the stale cache says why: {:?}",
        again.package_caches
    );
    assert!(defined(&again, "added"));
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

#[test]
fn cache_build_builds_what_is_missing_and_refresh_rebuilds() {
    let dir = project("index");
    let mut cfg = latex(&dir);
    cfg.search_paths = vec![dir.clone()];
    cfg.cache_index.packages = vec!["outer".into()];
    cfg.cache_index.auto = false;
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

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

fn run_latex(dir: &Path, source: &str, auto: bool) -> Analysis {
    let mut cfg = latex(dir);
    cfg.cache_index.auto = auto;
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
    cfg.cache_index.auto = false;
    cfg.cache_index.refresh = true;
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
