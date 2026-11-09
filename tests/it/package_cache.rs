//! A package read once is replayed from its cache on the next run, and the
//! replay reports what reading it would have.

use std::path::{Path, PathBuf};

use crate::support::{defined, installed, undefined};
use satex::analysis::facts::LoadStatus;
use satex::analysis::machine::{Analysis, Machine};
use satex::project::config::{Cache, Config};

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
    let cfg = Config {
        cache: Cache { dir: Some(dir.join("cache")), document: false, ..Default::default() },
        ..crate::support::config()
    };
    crate::support::seed_kernel(&cfg);
    cfg
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
            state if satex::analysis::machine::cache_served(state) => Some(true),
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
    let text = &analysis.facts.text;
    out.push(format!("typeset {:?} pars {:?} open {:?}", text.chars, text.pars, text.open));
    out.push(serde_json::to_string(&satex::linter::lint(analysis)).unwrap_or_default());
    out
}

/// Every [`satex::analysis::facts::Facts`] field, by name, rendered with names for symbols.
fn every_fact(a: &Analysis) -> Vec<(&'static str, Vec<String>)> {
    let f = &a.facts;
    let n = |s: satex::engine::tex::Sym| a.interner.name(s).to_string();
    let o = |s: Option<satex::engine::tex::Sym>| s.map(n);
    let sorted = |mut v: Vec<String>| {
        v.sort();
        v
    };
    let t = &f.text;
    vec![
        (
            "defs",
            f.defs
                .iter()
                .map(|d| {
                    format!("{} {} {} {:?} {} {} {}", n(d.name), n(d.by), d.tag, d.span, d.global, d.certain, d.depth)
                })
                .collect(),
        ),
        ("written_files", f.written_files.iter().cloned().collect()),
        ("absent", f.absent.iter().cloned().collect()),
        ("shapes", sorted(f.shapes.iter().map(|(k, v)| format!("{} {v}", n(*k))).collect())),
        ("types", sorted(f.types.iter().map(|(k, v)| format!("{k:?} {v:?}")).collect())),
        ("length_fonts", sorted(f.length_fonts.iter().map(|(k, v)| format!("{k:?} {v:?}")).collect())),
        ("vars", f.vars.iter().map(|v| format!("{} {:?} {} {:?}", n(v.name), v.kind, n(v.by), v.span)).collect()),
        ("letters", f.letters.iter().map(|l| format!("{l:?}")).collect()),
        (
            "expansions",
            f.expansions
                .iter()
                .map(|e| format!("{} {:?} {} {} {:?}", n(e.name), e.span, e.meaning.as_str(), e.count, o(e.within)))
                .collect(),
        ),
        (
            "loads",
            f.loads.iter().map(|l| format!("{} {} {:?} {:?}", l.name, l.status.as_str(), l.path, l.span)).collect(),
        ),
        (
            "occurrences",
            f.occurrences
                .iter()
                .map(|x| format!("{:?} {} {:?} {:?} {}", x.kind, x.key, x.detail, x.span, x.certain))
                .collect(),
        ),
        ("literals", f.literals.iter().map(|l| format!("{l:?}")).collect()),
        (
            "diagnostics",
            f.diagnostics.iter().map(|d| format!("{} {:?} {} {}", d.code, d.span, d.message, d.count)).collect(),
        ),
        ("csnames", sorted(f.csnames.iter().map(|(k, v)| format!("{k:?} {v:?}")).collect())),
        ("slots", sorted(f.slots.iter().map(|(k, v)| format!("{k:?} {v}")).collect())),
        ("groups", f.groups.iter().map(|g| format!("{g:?}")).collect()),
        ("raised", f.raised.iter().map(|r| format!("{} {} {:?} {}", n(r.name), r.text, r.span, r.certain)).collect()),
        ("spaces", sorted(f.spaces.iter().map(|(k, v)| format!("{k:?} {v:?}")).collect())),
        ("kept_spaces", sorted(f.kept_spaces.iter().map(|k| format!("{k:?}")).collect())),
        (
            "conditionals",
            f.conditionals
                .iter()
                .map(|c| format!("{} {:?} {:?} {:?} {:?} {}", n(c.name), c.at, c.arms, c.fi, c.taken, c.undecided))
                .collect(),
        ),
        (
            "quantities",
            f.quantities
                .iter()
                .map(|q| format!("{} {} {:?} {:?} {}", q.number, q.unit, q.span, o(q.via), q.certain))
                .collect(),
        ),
        (
            "colors",
            f.colors
                .iter()
                .map(|c| format!("{} {:?} {:?} {:?} {}", c.name, c.span, o(c.via), o(c.package), c.certain))
                .collect(),
        ),
        (
            "color_uses",
            f.color_uses
                .iter()
                .map(|c| format!("{} {:?} {} {:?} {}", c.name, c.role, c.defined, c.span, c.certain))
                .collect(),
        ),
        (
            "color_stack",
            f.color_stack.iter().map(|c| format!("{:?} {:?} {:?} {}", c.stack, c.op, c.span, c.certain)).collect(),
        ),
        ("text", vec![format!("{:?} {:?} {:?} {:?} {:?} {:?}", t.chars, t.at, t.pars, t.open, t.kept, t.closed)]),
        ("lines", sorted(f.lines.iter().map(|(k, v)| format!("{k:?} {v:?}")).collect())),
    ]
}

/// Each field a read and a replayed run disagree on.
fn differing_facts(read: &Analysis, replayed: &Analysis) -> Vec<String> {
    every_fact(read)
        .into_iter()
        .zip(every_fact(replayed))
        .filter(|((_, x), (_, y))| x != y)
        .map(|((name, x), (_, y))| {
            let at = x.iter().zip(&y).take_while(|(p, q)| p == q).count();
            format!("{name}: {} / {}: {:?} / {:?}", x.len(), y.len(), x.get(at), y.get(at))
        })
        .collect()
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
            satex::linter::lint(a)
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

/// A package of the user's own directory records what the user's files do
/// (expl3 variables, letters, argument types, groups): its replay reports
/// every such fact reading it did.
#[test]
fn an_own_package_replays_every_fact_reading_it_records() {
    if !installed() {
        return;
    }
    let dir = project("own-facts");
    std::fs::write(
        dir.join("facty.sty"),
        "\\ProvidesPackage{facty}[2020/01/01 v1]\n\\makeatletter\n\\newcount\\factycount \\factycount=3\n\
         \\newdimen\\factydim \\factydim=2em\n\\begingroup\n\\def\\factyx{}\n\\endgroup\n\
         \\expandafter\\def\\csname r@facty\\endcsname{}\n\\IfFileExists{facty-absent.tex}{}{}\n\
         \\ExplSyntaxOn\n\\tl_new:N \\g_facty_tl\n\\tl_gset:Nn \\g_facty_tl {a}\n\\ExplSyntaxOff\n\\makeatother\n",
    )
    .expect("facty.sty");
    let document = "\\documentclass{article}\n\\usepackage{facty}\n\\begin{document}x\\end{document}\n";
    let read = run(&dir, document);
    let replayed = run(&dir, document);
    assert_eq!(served(&read, "facty"), Some(false), "{:?}", read.package_caches);
    assert_eq!(served(&replayed, "facty"), Some(true), "{:?}", replayed.package_caches);
    assert!(read.facts.vars.iter().any(|v| read.interner.name(v.name).contains("g_facty_tl")));
    assert_eq!(differing_facts(&read, &replayed), Vec::<String>::new());
}

/// A replayed load that set nothing on the main vertical list keeps its tail: `\\lastkern`
/// after it reads the kern before it, served or read.
#[test]
fn a_replayed_load_that_appends_nothing_keeps_the_main_list_tail() {
    if !installed() {
        return;
    }
    let dir = project("main-tail");
    std::fs::write(dir.join("quiet.sty"), "\\ProvidesPackage{quiet}[2020/01/01 v1]\n\\def\\quietmacro{}\n")
        .expect("quiet.sty");
    let document = "\\documentclass{article}\n\\kern3pt\n\\usepackage{quiet}\n\\edef\\R{\\the\\lastkern}\n\\begin{document}x\\end{document}\n";
    let read = run(&dir, document);
    let replayed = run(&dir, document);
    assert_eq!(served(&replayed, "quiet"), Some(true), "{:?}", replayed.package_caches);
    for analysis in [&read, &replayed] {
        assert_eq!(crate::support::last_body(analysis, "R").replace(' ', ""), "3.0pt");
    }
}

/// A register the package allocates and assigns, then assigns again past a
/// line the preamble decides: a run that serves the front and records the
/// rest keys it as it stands, so the next run serves all of it and reports
/// what reading it does.
#[test]
fn a_register_assigned_in_the_served_front_keys_the_recorded_rest_as_it_stands() {
    if !installed() {
        return;
    }
    let dir = project("served-front");
    std::fs::write(
        dir.join("fronty.sty"),
        "\\ProvidesPackage{fronty}[2020/01/01 v1]\n\\newdimen\\frontydim\n\\frontydim=3pt\n\\relax\n\
         \\ifdefined\\userflag\\def\\frontyflagged{}\\fi\n\\frontydim=5pt\n\\relax\n",
    )
    .expect("fronty.sty");
    let plain = "\\documentclass{article}\n\\usepackage{fronty}\n\\begin{document}x\\end{document}\n";
    let flagged = plain.replacen("\\usepackage", "\\def\\userflag{}\n\\usepackage", 1);
    run(&dir, plain);
    let resumed = run(&dir, &flagged);
    assert_eq!(served(&resumed, "fronty"), Some(false), "{:?}", resumed.package_caches);
    let replayed = run(&dir, &flagged);
    let state = replayed.package_caches.iter().rfind(|(name, ..)| name == "fronty").map(|(.., s)| s.as_str());
    assert_eq!(state, Some("cached"), "{:?}", replayed.package_caches);
    assert_eq!(differing_facts(&run_cold(&dir, &flagged), &replayed), Vec::<String>::new());
}

/// tagpdf replays whole: the random ids (`\\int_rand:n`) its namespaces
/// get fork, and their paths leave fewer finished input levels below the
/// file than it opened on. The replay reports what reading it did.
#[test]
fn tagpdf_replays_whole_and_reports_the_same() {
    if !installed() {
        return;
    }
    let dir = project("tagpdf");
    let document = "\\documentclass{article}\n\\usepackage{tagpdf}\n\\begin{document}\nx\n\\end{document}\n";
    let first = run(&dir, document);
    let second = run(&dir, document);
    assert_eq!(served(&second, "tagpdf"), Some(true), "{:?}", second.package_caches);
    assert!(second.served_steps * 10 > second.load_steps * 9, "{} of {}", second.served_steps, second.load_steps);
    assert_eq!(first.steps, second.steps);
    same_facts(&first, &second);
}

/// Runs 2 and 3 of `document` on one cache rebuild nothing: every key a
/// cache entry has is what the next run sees again. The project and run 3.
fn rebuilds_nothing_warm(name: &str, document: &str) -> Option<(PathBuf, Analysis)> {
    if !installed() {
        return None;
    }
    let dir = project(name);
    run(&dir, document);
    let mut last = None;
    for warm in [2, 3] {
        let analysis = run(&dir, document);
        let report = analysis.cache_report();
        let rebuilt: Vec<_> = report.iter().filter(|l| l.contains("rebuilt")).collect();
        assert!(rebuilt.is_empty(), "run {warm}: {rebuilt:?}");
        last = Some(analysis);
    }
    Some((dir, last?))
}

#[test]
fn document_metadata_rebuilds_nothing_warm() {
    rebuilds_nothing_warm(
        "warm-metadata",
        "\\DocumentMetadata{pdfversion=2.0}\\documentclass{article}\\begin{document}x\\end{document}\n",
    );
}

/// beamer's `\mode<presentation>` reads its theme files ahead: they are
/// stored all the same.
#[test]
fn beamer_rebuilds_nothing_warm() {
    rebuilds_nothing_warm("warm-beamer", BEAMER);
}

const BEAMER: &str = "\\documentclass{beamer}\\begin{document}\\begin{frame}x\\end{frame}\\end{document}\n";

/// The replay decides hyperref's `\ifnum\lastnodetype=1` in
/// `\Hy@SaveLastskip` as reading it does.
#[test]
fn beamer_replays_what_reading_it_reports() {
    if let Some((dir, replayed)) = rebuilds_nothing_warm("replay-beamer", BEAMER) {
        assert_eq!(differing_facts(&run_cold(&dir, BEAMER), &replayed), Vec::<String>::new());
    }
}

/// Lines a lookahead reads ahead, as beamer's `\mode` does (the last one of
/// a package it loads too), start at rest once the tokens are given back:
/// the package is stored, and its replay reports what reading it did.
#[test]
fn a_package_read_ahead_by_a_lookahead_is_stored_and_replays_the_same() {
    if !installed() {
        return;
    }
    let dir = project("lookahead");
    let mode = "\\def\\peekmode{\\@ifstar\\relax\\peekat}\\def\\peekat<#1>{\\@ifnextchar\\bgroup\\@firstofone\\relax}";
    let peeky = format!(
        "\\ProvidesPackage{{peeky}}\n{mode}\n\\peekmode<a>\n\n\\def\\peekx{{x}}\n\\RequirePackage{{peekin}}\n\n\
         \\def\\peeky{{\\peekx\\peekz}}\n\\peekmode\n<b>\n"
    );
    std::fs::write(dir.join("peeky.sty"), peeky).expect("peeky.sty");
    std::fs::write(dir.join("peekin.sty"), "\\ProvidesPackage{peekin}\n\\def\\peekz{z}\n\\peekmode\n<c>\n")
        .expect("peekin.sty");
    let document = "\\documentclass{article}\n\\usepackage{peeky}\n\\begin{document}\\peeky\\end{document}\n";
    let read = run(&dir, document);
    let replayed = run(&dir, document);
    let report = replayed.cache_report();
    assert!(!report.iter().any(|l| l.contains("peek") && l.contains("rebuilt")), "{report:?}");
    assert_eq!(served(&replayed, "peeky"), Some(true), "{:?}", replayed.package_caches);
    assert_eq!(differing_facts(&read, &replayed), Vec::<String>::new());
    same_facts(&read, &replayed);
}

#[test]
fn tikz_rebuilds_nothing_warm() {
    rebuilds_nothing_warm(
        "warm-tikz",
        "\\documentclass{article}\\usepackage{tikz}\\begin{document}\\tikz\\draw(0,0)--(1,1);\\end{document}\n",
    );
}

#[test]
fn hyperref_rebuilds_nothing_warm() {
    rebuilds_nothing_warm(
        "warm-hyperref",
        "\\documentclass{article}\\usepackage{hyperref}\\begin{document}\\section{A}\\label{a}\\ref{a}\\end{document}\n",
    );
}

#[test]
fn biblatex_rebuilds_nothing_warm() {
    rebuilds_nothing_warm(
        "warm-biblatex",
        "\\documentclass{article}\\usepackage{biblatex}\\begin{document}\\cite{a}\\printbibliography\\end{document}\n",
    );
}

/// expl3's `\seq_map_inline:Nn` gsets `\__seq_map_<level>:w`, whose
/// `\csname` asks whether the name exists only to make it `\relax`. A package
/// recorded where earlier maps left those functions is served where none
/// did, and the replay reports what reading it does.
#[test]
fn a_map_function_some_earlier_package_left_does_not_key_the_cache() {
    if !installed() {
        return;
    }
    let nested = |name: &str| {
        format!(
            "\\ProvidesExplPackage{{{name}}}{{2020/01/01}}{{1}}{{t}}\n\\seq_new:N \\g_{name}_seq\n\
             \\seq_gput_right:Nn \\g_{name}_seq {{a}}\n\
             \\seq_map_inline:Nn \\g_{name}_seq {{ \\seq_map_inline:Nn \\g_{name}_seq {{\n\
             \\seq_map_inline:Nn \\g_{name}_seq {{ \\cs_gset:cpn {{{name}_##1####1########1}} {{}} }} }} }}\n\
             \\cs_new:Npn \\{name}done {{}}\n"
        )
    };
    let dir = project("mapped");
    std::fs::write(dir.join("mapper.sty"), nested("mapper")).expect("mapper.sty");
    // What an earlier map leaves: the map functions of its levels.
    let made: String =
        (1..=3).map(|level| format!("\\expandafter\\gdef\\csname __seq_map_{level}:w\\endcsname#1{{}}\n")).collect();
    let both =
        format!("\\documentclass{{article}}\n{made}\\usepackage{{mapper}}\n\\begin{{document}}x\\end{{document}}\n");
    let alone = "\\documentclass{article}\n\\usepackage{mapper}\n\\begin{document}x\\end{document}\n";
    run(&dir, &both);
    let replayed = run(&dir, alone);
    assert_eq!(served(&replayed, "mapper"), Some(true), "{:?}", replayed.package_caches);
    assert!(defined(&replayed, "mapperdone"));
    for entry in std::fs::read_dir(dir.join("cache")).expect("cache directory").flatten() {
        if entry.file_name().to_string_lossy().starts_with("package-mapper-") {
            std::fs::remove_file(entry.path()).expect("mapper's cache");
        }
    }
    let read = run(&dir, alone);
    assert_eq!(served(&read, "mapper"), Some(false), "{:?}", read.package_caches);
    assert_eq!(differing_facts(&read, &replayed), Vec::<String>::new());
    same_facts(&read, &replayed);
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
            e.meaning == satex::analysis::facts::MeaningKind::Undefined
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
    let root = satex::queries::query::summary(&second);
    let outer = root.children.iter().find(|n| n.name == "outer.sty").expect("outer.sty in the tree");
    assert!(outer.cached, "the replayed file is marked cached");
    assert!(outer.tokens > 0, "outer.sty reports the tokens its cache stands for, got {}", outer.tokens);
    // `inner.sty` has a cache entry of its own, replayed between outer's segments.
    assert_eq!(served(&second, "inner"), Some(true), "{:?}", second.package_caches);
    let text = satex::cli::render::timings(&second, satex::cli::render::Links(false));
    assert!(text.contains("inner"), "inner.sty is named:\n{text}");
}

#[test]
fn a_changed_package_file_is_read_again() {
    let dir = project("stamp");
    run(&dir, DOCUMENT);
    std::fs::write(dir.join("inner.sty"), "\\ProvidesPackage{inner}\n\\def\\innermacro#1{}\\def\\added{}\n")
        .expect("inner.sty");
    let again = run(&dir, DOCUMENT);
    // The nested load has its own entry: only it is read again.
    assert_eq!(served(&again, "inner"), Some(false));
    assert_eq!(served(&again, "outer"), Some(true), "{:?}", again.package_caches);
    assert!(
        again.package_caches.iter().any(|(name, _, state)| name == "inner"
            && state.contains("content changed")
            && state.contains("inner.sty")),
        "the stale cache says why: {:?}",
        again.package_caches
    );
    assert!(defined(&again, "added"));
}

/// A package loaded from a file `\input` by a class, and one from a file the
/// document `\input`s: each has its own entry, the class resumes after the
/// input, a second run serves all three and reports what reading does, and
/// an edit to the package reads only it again.
#[test]
fn a_package_loaded_from_an_input_file_is_cached() {
    if !installed() {
        return;
    }
    let dir = project("via-input");
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).expect(name);
    write(
        "viaclass.cls",
        "\\ProvidesClass{viaclass}\n\\LoadClass{article}\n\\input{viasetup.tex}\n\\def\\afterinput{}\n",
    );
    write("viasetup.tex", "\\RequirePackage{vialocal}\n\\def\\setupdone{}\n");
    write("vialocal.sty", "\\ProvidesPackage{vialocal}\n\\def\\localmacro{L}\n\\newcount\\localcount\n");
    write("viadoc.tex", "\\RequirePackage{viaother}\n");
    write("viaother.sty", "\\ProvidesPackage{viaother}\n\\def\\othermacro{O}\n");
    let document =
        "\\documentclass{viaclass}\n\\input{viadoc.tex}\n\\begin{document}\\localmacro\\othermacro\\end{document}\n";
    let read = run(&dir, document);
    let replayed = run(&dir, document);
    for name in ["vialocal", "viaother", "viaclass"] {
        assert_eq!(served(&read, name), Some(false), "{name}: {:?}", read.package_caches);
        assert_eq!(served(&replayed, name), Some(true), "{name}: {:?}", replayed.package_caches);
    }
    assert!(defined(&replayed, "afterinput") && defined(&replayed, "setupdone"));
    assert_eq!(differing_facts(&read, &replayed), Vec::<String>::new());
    same_facts(&read, &replayed);
    write("vialocal.sty", "\\ProvidesPackage{vialocal}\n\\def\\localmacro{M}\n\\def\\added{}\n");
    let edited = run(&dir, document);
    assert_eq!(served(&edited, "vialocal"), Some(false), "{:?}", edited.package_caches);
    assert_eq!(served(&edited, "viaother"), Some(true), "{:?}", edited.package_caches);
    assert!(defined(&edited, "added") && undefined(&edited, "localcount"));
    same_facts(&run_cold(&dir, document), &edited);
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
    assert_eq!(served(&again, "inner"), Some(false), "{:?}", again.package_caches);
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
        satex::linter::lint(a).iter().any(|r| r["code"] == "undefined-control-sequence" && named(r))
    };
    let warm = run(&dir, source);
    assert!(!undefined(&warm));
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).expect("a modification time");
    std::fs::write(&path, text.replacen("innermacro", "innermacrx", 1)).expect("inner.sty");
    std::fs::File::options().write(true).open(&path).and_then(|f| f.set_modified(modified)).expect("set mtime");
    let again = run(&dir, source);
    assert!(crate::support::undefined(&again, "innermacro"));
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
fn a_replay_keeps_the_primitive_copies_made_before_the_load() {
    // Recorded where `\mydraft` is no copy of `\pdfdraftmode`: replayed
    // where it is one, the copy stays and the test on it stays decided.
    let dir = project("aliases");
    std::fs::write(dir.join("few.sty"), "\\ProvidesPackage{few}\n\\def\\fewmacro{x}\n").expect("few.sty");
    let document = |preamble: &str, body: &str| {
        format!(
            "\\documentclass{{article}}\n{preamble}\\usepackage{{few}}\n\\begin{{document}}{body}\\end{{document}}\n"
        )
    };
    run(&dir, &document("", ""));
    let warm = run(&dir, &document("\\let\\mydraft\\pdfdraftmode\n", "\\ifnum\\mydraft=0 \\else\\notdefined\\fi"));
    assert_eq!(served(&warm, "few"), Some(true), "{:?}", warm.package_caches);
    // Lost, it forks: two paths join that "differed in \mydraft".
    let wrong: Vec<_> = warm.facts.diagnostics.iter().filter(|d| d.message.contains("draft")).collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[test]
fn a_prior_definition_the_package_tests_makes_it_miss() {
    let dir = project("tested");
    run(&dir, DOCUMENT);
    let warm = run(&dir, &preamble("\\def\\userflag{}"));
    assert_eq!(served(&warm, "outer"), Some(false), "{:?}", warm.package_caches);
    assert!(defined(&warm, "flagged"));
}

#[test]
fn other_options_make_it_miss() {
    let dir = project("options");
    run(&dir, "\\documentclass{article}\n\\usepackage{withopts}\n\\begin{document}\\end{document}\n");
    let other = "\\documentclass{article}\n\\usepackage[draft]{withopts}\n\\begin{document}\\end{document}\n";
    let warm = run(&dir, other);
    assert_eq!(served(&warm, "withopts"), Some(false), "{:?}", warm.package_caches);
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
    assert_eq!(served(&second, "inner"), Some(false), "{:?}", second.package_caches);
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
    assert!(undefined(&from_a, "onlyinB"), "a must not see b's package");

    let from_b = analyze(&b, DOCUMENT, &cfg_for(&b));
    assert!(defined(&from_b, "onlyinB"), "b must read its own outer.sty, not a's cache: {:?}", from_b.package_caches);
    assert!(undefined(&from_b, "onlyinA"), "b must not see a's package");
    assert_eq!(
        served(&from_b, "outer"),
        Some(false),
        "a's cache for the same name must not be served to b: {:?}",
        from_b.package_caches
    );

    // Running each project a second time replays its own cache, still
    // without crossing over.
    let a_again = analyze(&a, DOCUMENT, &cfg_for(&a));
    assert_eq!(served(&a_again, "outer"), Some(true), "{:?}", a_again.package_caches);
    assert!(defined(&a_again, "onlyinA"));
    assert!(undefined(&a_again, "onlyinB"));

    let b_again = analyze(&b, DOCUMENT, &cfg_for(&b));
    assert_eq!(served(&b_again, "outer"), Some(true), "{:?}", b_again.package_caches);
    assert!(defined(&b_again, "onlyinB"));
    assert!(undefined(&b_again, "onlyinA"));
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
            satex::cli::cache::build::prepare(&cfg, refresh, &[], &[], None, &mut |line| lines.push(line.to_string()))
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
    let outer = std::fs::read_to_string(dir.join("outer.sty")).expect("outer.sty");
    std::fs::write(dir.join("outer.sty"), outer.replacen("outermacro", "outermacrx", 1)).expect("outer.sty");
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
        satex::engine::tex::Meaning::Register(_, index) => Some(index),
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
/// are served whole from their cache and which are read and stored again (a miss
/// must show as stored: a package missing from the report would pass a weaker check), and that the run reports exactly what
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
        assert_eq!(
            served(&warm, package),
            Some(false),
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
    after_edit("class-options", &base, &larger, &["etoolbox"], &["article", "amsmath", "graphics", "xcolor"]);
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
    after_edit("read-name", &lines(BASE), &defined, &["article", "amsmath", "graphicx", "xcolor"], &["graphics"]);
}

/// The characters of `name`'s replacement text.
fn text_of(analysis: &Analysis, name: &str) -> Option<String> {
    let sym = analysis.interner.lookup(name)?;
    let meaning = analysis.env.meaning(sym);
    let text = &meaning.as_macro()?.replacement_text;
    Some(
        text.iter()
            .filter_map(|t| match t.tok {
                satex::engine::tex::Tok::Chr(c, _) => Some(c),
                _ => None,
            })
            .collect(),
    )
}

/// The site's examples report the same, their packages read or replayed.
#[test]
fn the_examples_report_the_same_cold_and_warm() {
    if !installed() {
        return;
    }
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/site/examples");
    for name in ["thesis", "spacing", "option-clash"] {
        let dir = project(&format!("example-{name}"));
        for entry in std::fs::read_dir(&examples).expect("examples").flatten() {
            std::fs::copy(entry.path(), dir.join(entry.file_name())).expect("example file");
        }
        let source = std::fs::read_to_string(dir.join(format!("{name}.tex"))).expect("example");
        let path = dir.join(format!("{name}.tex"));
        let cfg = latex(&dir);
        Machine::analyze(&source, Some(&path), &cfg);
        let warm = Machine::analyze(&source, Some(&path), &cfg);
        let mut read = cfg.clone();
        (read.cache.index.auto, read.cache.index.refresh) = (false, true);
        let cold = Machine::analyze(&source, Some(&path), &read);
        same_facts(&cold, &warm);
    }
}

/// A group that wrote a name locally, then globally, then locally again
/// leaves the global meaning (tex.web § 283): the replay restores it too.
#[test]
fn a_global_definition_between_local_ones_in_a_group_is_replayed() {
    if !installed() {
        return;
    }
    let dir = project("group-global");
    std::fs::write(
        dir.join("grouped.sty"),
        "\\ProvidesPackage{grouped}\n\\begingroup\n\\let\\groupedname\\relax\n\\gdef\\groupedname{G}\n\
         \\def\\groupedname{L}\n\\endgroup\n",
    )
    .expect("grouped.sty");
    let doc = "\\documentclass{article}\n\\usepackage{grouped}\n\\begin{document}\\groupedname\\end{document}\n";
    let cold = run(&dir, doc);
    let warm = run(&dir, doc);
    assert_eq!(served(&warm, "grouped"), Some(true), "{:?}", warm.package_caches);
    assert_eq!(text_of(&warm, "groupedname").as_deref(), Some("G"));
    same_facts(&cold, &warm);
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
    assert_eq!(served(&warm, "inspector"), Some(false), "{:?}", warm.package_caches);
    assert!(undefined(&warm, "wasa"));
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

    let findings = satex::linter::lint(&analysis);
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
/// reports, lints or traces differently, keeps call arguments (the
/// language server does), has other resource limits, or names the engine
/// it would pick anyway reads the same cache.
#[test]
fn settings_that_do_not_change_the_interpretation_share_a_cache() {
    let dir = project("key");
    let first = run(&dir, DOCUMENT);
    assert_eq!(served(&first, "outer"), Some(false), "{:?}", first.package_caches);
    let mut other = latex(&dir);
    other.verbose = 2;
    other.lint_off = vec!["unused-label".into()];
    other.profile = Some(satex::project::config::Profile::Document);
    other.engine = Some(first.plugins.engine);
    other.limits.memory = bytesize::ByteSize::gib(3);
    other.limits.degrade_at_percent = 90;
    other.record_arguments = true;
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
            .filter(|e| e.file_name().to_string_lossy().starts_with("package-scrbook.cls-"))
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
        crate::support::tmp_root().join(format!("satex-package-cache-tree-copy-{}", std::process::id())),
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
        satex::cli::cache::export(cfg, &self.archive, (&[], &[]), false, &mut out).expect("export");
    }

    fn import(&self, cfg: &Config) -> String {
        let mut out = Vec::new();
        satex::cli::cache::import(cfg, &self.archive, false, &mut out).expect("import");
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
    assert_eq!(satex::caches::format::release_name(&kernel), "2031");
    let bare = dir.join("other.ltx");
    std::fs::write(&bare, "\\relax\n").expect("kernel");
    assert_eq!(satex::caches::format::release_name(&bare), satex::caches::format::tree_name(&bare));
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
    assert_eq!(served(&again, "pa"), Some(false), "{:?}", again.package_caches);
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
        satex::cli::cache::export(&cfg, &p.archive, (&only, &without), true, &mut out).expect("export");
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
        let undefined: Vec<_> = satex::linter::lint(&analysis)
            .into_iter()
            .filter(|r| r["code"] == "undefined-control-sequence")
            .map(|r| r["message"].to_string())
            .collect();
        assert!(undefined.is_empty(), "{run}: {undefined:?}");
    }
}

/// A name partly built from unknown text may be any name of its shape, so a
/// package that asks whether such a name is defined must not be served the
/// answer a run without that name gave.
#[test]
fn a_package_read_after_a_name_of_unknown_spelling_is_not_served_the_plain_answer() {
    if !installed() {
        return;
    }
    let package = "\\ProvidesPackage{wp}\n\\ifdefined\\wpx\\def\\wpseen{}\\fi\n\\relax\n";
    let (dir, fresh) = (project("wild"), project("wild-fresh"));
    for d in [&dir, &fresh] {
        std::fs::write(d.join("wp.sty"), package).expect("wp.sty");
    }
    let plain = "\\documentclass{article}\\usepackage{wp}\\begin{document}x\\end{document}\n";
    let wild = "\\documentclass{article}\\expandafter\\def\\csname wp\\romannumeral\\pdfuniformdeviate 30\\endcsname{}\n\
                \\usepackage{wp}\\begin{document}x\\end{document}\n";
    let _ = run(&dir, plain);
    let warm = run(&dir, wild);
    let cold = run(&fresh, wild);
    assert!(!defined(&cold, "wpseen") && !undefined(&cold, "wpseen"), "a fresh read leaves it open");
    assert!(!defined(&warm, "wpseen") && !undefined(&warm, "wpseen"), "{:?}", warm.package_caches);
}

/// A package loaded once the aux file is open (graphics loads epstopdf-base
/// at `\begin{document}`) replays in a document of another name: the files
/// written so far count by their name past `\jobname`. Keyed by `a.aux`,
/// the playground rebuilt epstopdf-base for every example but the one the
/// bundle was built with.
#[test]
fn a_package_loaded_after_the_aux_opened_replays_under_another_job_name() {
    if !installed() {
        return;
    }
    let dir = project("job-name");
    std::fs::write(dir.join("late.sty"), "\\ProvidesPackage{late}[2020/01/01 v1]\n\\def\\latemacro{}\n")
        .expect("late.sty");
    let source =
        "\\documentclass{article}\n\\AtBeginDocument{\\RequirePackage{late}}\n\\begin{document}x\\end{document}\n";
    let cfg = latex(&dir);
    let named = |job: &str| {
        let path = dir.join(format!("{job}.tex"));
        std::fs::write(&path, source).expect("document");
        Machine::analyze(source, Some(&path), &cfg)
    };
    assert_eq!(served(&named("a"), "late"), Some(false));
    let other = named("b");
    assert_eq!(served(&other, "late"), Some(true), "{:?}", other.package_caches);
}

/// A load in a paragraph runs in that mode: what a load in vertical mode
/// stored does not stand for it, nor the other way round.
/// Real: a document with text before its `\usepackage` lost its pages to
/// an entry another document had stored.
#[test]
fn a_package_loaded_in_a_paragraph_stays_out_of_the_cache() {
    if !installed() {
        return;
    }
    let dir = project("in-paragraph");
    assert_eq!(served(&run(&dir, &preamble("")), "outer"), Some(false));
    assert_eq!(served(&run(&dir, &preamble("text")), "outer"), None);
    assert_eq!(served(&run(&dir, &preamble("")), "outer"), Some(true));
}
