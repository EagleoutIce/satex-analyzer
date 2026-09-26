//! Minimal reproducers for what the user's own libraries (tikzpingus, TeXCHR,
//! xlistings, fancyqr, the lambda-calculus visualizer) exposed.  Each test
//! stands alone; the `#[ignore]`d one at the end runs the libraries
//! themselves when they are checked out.

use satex::config::Config;
use satex::facts::Severity;
use satex::machine::{Analysis, Machine};
use satex::tex::Meaning;

fn bare() -> Config {
    Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    }
}

// Without a format the catcodes are INITEX's (tex.web § 232).
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

fn analyze(source: &str) -> Analysis {
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &bare())
}

/// With the LaTeX kernel, whose `\newcount` and relatives are its own code.
fn analyze_kernel(source: &str) -> Analysis {
    Machine::analyze(source, None, &Config { load_format: true, use_kpsewhich: true, ..bare() })
}

fn gaps(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Unsupported)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn body(analysis: &Analysis, name: &str) -> String {
    match analysis.interner.lookup(name).map(|sym| analysis.env.meaning(sym)) {
        Some(Meaning::Macro(m)) => satex::tex::detokenize(&m.replacement_text, &analysis.interner),
        other => format!("{other:?}"),
    }
}

#[test]
fn a_register_is_assigned_from_a_numbered_register() {
    // plain.tex's `\alloc@`: `\allocationnumber=\count11`.
    let analysis = analyze(
        r"\count11=10 \countdef\an=21 \an=\count11
\ifnum\count21=10 \def\ok{yes}\else\def\ok{no}\fi",
    );
    assert!(gaps(&analysis).is_empty(), "{:?}", gaps(&analysis));
    assert_eq!(body(&analysis, "ok"), "yes");
}

#[test]
fn a_countdef_name_and_its_register_are_one_register() {
    // plain.tex's `\countdef\insc@unt=20` after `\count20=255`.
    let analysis = analyze(
        r"\count20=255 \countdef\ins=20 \advance\ins by -1
\ifnum\count20=254 \def\ok{yes}\else\def\ok{no}\fi
\chardef\c=7 \ifnum\c=7 \def\ch{yes}\else\def\ch{no}\fi",
    );
    assert_eq!(body(&analysis, "ok"), "yes");
    assert_eq!(body(&analysis, "ch"), "yes");
}

#[test]
fn newcount_hands_out_the_next_register_number() {
    // supp-pdf.mkii allocates until `\meaning` shows a number above 20.
    let analysis = analyze_kernel(
        r"\newcount\qa \newcount\qb \qa=5
\edef\shown{\meaning\qa|\meaning\qb}
\ifnum\qa=5 \def\ok{yes}\else\def\ok{no}\fi",
    );
    let shown = body(&analysis, "shown");
    let numbers: Vec<u32> = shown.split('|').filter_map(|m| m.strip_prefix(r"\count")?.parse().ok()).collect();
    assert!(numbers.len() == 2 && numbers[1] == numbers[0] + 1 && numbers[0] > 22, "{shown}");
    assert_eq!(body(&analysis, "ok"), "yes");
}

#[test]
fn an_else_that_ends_the_test_of_its_own_conditional_waits_for_it() {
    // supp-pdf.mkii: `\ifnum\expandafter\strip\meaning#2>20\else…\fi`, where
    // the `\else` ends the number (tex.web § 510).
    let analysis = analyze_kernel(
        r"\def\strip#1{\ifnum10<9#1 #1\else\expandafter\strip\fi}
\newcount\qa
\ifnum\expandafter\strip\meaning\qa>20\else\def\bad{}\fi
\ifnum1=1\else\def\bad{}\fi
\ifnum1=2\fi",
    );
    assert!(gaps(&analysis).is_empty(), "{:?}", gaps(&analysis));
    assert!(matches!(
        analysis.interner.lookup("bad").map(|s| analysis.env.meaning(s)),
        None | Some(Meaning::Undefined)
    ));
}

#[test]
fn the_relation_of_ifnum_is_read_with_expansion() {
    // xcolor.sty's `\XC@getmodclr`: the `\fi` of an `\ifcase` stands between
    // the number and the `>` (tex.web § 503).
    let analysis = analyze(r"\def\t#1{\ifnum\ifcase#1 0\or1 \fi>0 \def\r{yes}\else\def\r{no}\fi}\t1");
    assert_eq!(body(&analysis, "r"), "yes");
}

#[test]
fn afterassignment_fires_after_a_named_register_assignment() {
    // `\@defaultunits\@tempdimb=#2pt\relax\@nnil` in xcolor's `\rdivide`.
    let analysis = analyze_kernel(
        r"\def\strip#1\stop{}\newdimen\d
\afterassignment\strip\d=255pt\relax\stop \def\after{yes}",
    );
    assert!(gaps(&analysis).is_empty(), "{:?}", gaps(&analysis));
    assert_eq!(body(&analysis, "after"), "yes");
    assert!(analysis.facts.diagnostics.iter().all(|d| d.code != "runaway-argument"));
}

#[test]
fn the_meaning_of_a_let_copy_of_a_primitive_names_the_primitive() {
    // pgfkeys.code.tex compares `\string\expanded` with `\meaning` of its
    // `\let` copy, and stops loading when they differ.
    let analysis = analyze(r"\let\e\expanded \edef\a{\string\expanded}\edef\b{\meaning\e}");
    assert_eq!(body(&analysis, "a"), body(&analysis, "b"));
}

#[test]
fn a_package_sees_its_own_name_and_options() {
    // xkeyval's `\ProcessOptionsX` (FiraSans.sty) reads `\@currname`,
    // `\@currext` and `\opt@⟨name⟩.⟨ext⟩`.
    let directory = std::env::temp_dir().join(format!("satex-currname-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("cn.sty"),
        "\\edef\\cnname{\\@currname.\\@currext}\\edef\\cnopt{\\csname opt@\\@currname.\\@currext\\endcsname}\n",
    )
    .unwrap();
    let document = directory.join("doc.tex");
    let source = "\\documentclass{article}\\usepackage[a,b]{cn}\\catcode`\\@=11 \\edef\\after{\\@currname}\n";
    std::fs::write(&document, source).unwrap();
    let cfg = Config { load_packages: true, load_format: true, use_kpsewhich: true, ..bare() };
    let analysis = Machine::analyze(source, Some(&document), &cfg);
    assert_eq!(body(&analysis, "cnname"), "cn.sty");
    assert_eq!(body(&analysis, "cnopt"), "a,b");
    assert_eq!(body(&analysis, "after"), "");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_macro_whose_expansion_is_over_is_not_re_entered() {
    // pgfmath's `\expandafter\def\expandafter\pgfmathresult\expandafter{…}`
    // run many times in a row: each expansion ends before the next begins.
    let mut source = String::from(r"\def\r{1}\def\step{\expandafter\def\expandafter\r\expandafter{\r}}");
    for _ in 0..100 {
        source.push_str(r"\step");
    }
    let analysis = analyze(&source);
    assert!(
        analysis.facts.diagnostics.iter().all(|d| d.code != "recursion-widened"),
        "{:?}",
        analysis.facts.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert_eq!(body(&analysis, "r"), "1");
}

#[test]
fn an_unknown_dimension_still_prints_its_unit() {
    // pgfsys.code.tex: `\expandafter\Pgf@geT\the\pgf@x` with `\Pgf@geT#1pt`.
    let analysis = analyze_kernel(
        r"{\catcode`\p=12 \catcode`\t=12 \gdef\geT#1pt{#1}}\newdimen\d
\ifnum\pdfuniformdeviate2=0 \d=1pt\else\d=2pt\fi \edef\v{\expandafter\geT\the\d}",
    );
    assert!(analysis.facts.diagnostics.iter().all(|d| d.code != "runaway-argument"));
    assert!(gaps(&analysis).is_empty(), "{:?}", gaps(&analysis));
}

#[test]
fn the_pgfkeys_query_lists_every_key_with_its_handler() {
    if which::which("kpsewhich").is_err() {
        return;
    }
    // pgfkeys.code.tex needs `@` to be a letter (its own header says so);
    // the kernel leaves it `other`, as pdflatex does.
    let source = r"\makeatletter\input pgfkeys.code.tex
\newif\ifdraft
\pgfkeys{/demo/.is family, /demo,
  width/.initial=2cm, color/.style={fill=#1}, color/.default=red,
  draft/.is if=draft, mode/.is choice, mode/fast/.code={}, label/.code 2 args={}}
";
    let cfg = Config { load_inputs: true, ..Config::default() };
    let analysis = Machine::analyze(source, None, &cfg);
    let keys = satex::query::pgfkeys(&analysis, Some("/demo"));
    let kind = |key: &str| {
        keys.iter()
            .find(|r| r["key"] == key)
            .map(|r| r["kind"].as_str().unwrap_or_default().to_string())
            .unwrap_or_else(|| panic!("{key} missing from {keys:?}"))
    };
    assert_eq!(kind("/demo"), "is family");
    assert_eq!(kind("/demo/width"), "initial");
    assert_eq!(kind("/demo/color"), "style");
    assert_eq!(kind("/demo/draft"), "is if");
    assert_eq!(kind("/demo/mode"), "is choice");
    assert_eq!(kind("/demo/mode/fast"), "code");
    assert_eq!(kind("/demo/label"), "code args");
    let color = keys.iter().find(|r| r["key"] == "/demo/color").unwrap();
    assert_eq!(color["default"], "red");
    assert_eq!(color["line"], 3);
    assert!(keys.iter().all(|r| !r["key"].as_str().unwrap().ends_with("activetrue")));
}
