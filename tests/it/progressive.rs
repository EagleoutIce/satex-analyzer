//! Progressive recursion: a loop that makes a new name each round
//! (`\csname a\the\n\endcsname`), defines it and goes on, possibly through
//! the name it just made. Every expected `\R` comes from `etex -ini -etex`.
//! With a known bound the run must be exact. With an unknown bound
//! (`\pdfelapsedtime`) `\R` must cover what etex printed, and the loop must
//! end by widening, not at a limit.

use satex::config::{Config, Engine};
use satex::facts::Severity;
use satex::machine::{Analysis, Machine};
use satex::plugin::Kernel;

pub(crate) const PRE: &str =
    "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\countdef\\n=10 \\n=0 \\countdef\\m=11\n";

fn cfg() -> Config {
    Config { engine: Some(Engine::PdfTeX), kernel: Some(Kernel::None), ..crate::support::bare() }
}

fn analyze_with(source: &str, cfg: &Config) -> Analysis {
    Machine::analyze(&format!("{PRE}{source}\n"), None, cfg)
}

fn analyze(source: &str) -> Analysis {
    analyze_with(source, &cfg())
}

/// Every recorded body of `\R`.
fn results(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .defs
        .iter()
        .filter(|d| analysis.interner.name(d.name) == "R")
        .filter_map(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .collect()
}

/// Whether `got`, `/`-separated, covers etex's `want`: each part equal, or
/// unknown digits whose interval holds it, or other unknown text.
fn covers(got: &str, want: &str) -> bool {
    let (g, w): (Vec<&str>, Vec<&str>) = (got.split('/').collect(), want.split('/').collect());
    g.len() == w.len()
        && g.iter().zip(&w).all(|(g, w)| {
            let g = g.trim();
            if g == w.trim() {
                return true;
            }
            // Unknown text detokenizes as `\` and a marker no name has.
            let Some(g) = g.strip_prefix("\\\u{2}") else { return false };
            if let Some(range) = g.strip_prefix("unknown digits:") {
                // The name goes on after the interval (`#id`, `@slice`).
                let range = range.split(['#', '@']).next().unwrap_or(range);
                let Some((lo, hi)) = range.split_once("..") else { return false };
                let (Ok(lo), Ok(hi), Ok(v)) = (lo.parse::<i64>(), hi.trim().parse::<i64>(), w.trim().parse::<i64>())
                else {
                    return false;
                };
                return lo <= v && v <= hi;
            }
            g.starts_with("unknown")
        })
}

/// Known bounds: exact.
pub(crate) const EXACT: &[(&str, &str, &str)] = &[
    // etex: 3/5/U
    (
        "tail_known_bound",
        r"\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<5 \expandafter\step\fi}\step
\edef\R{\csname a3\endcsname/\csname a5\endcsname/\expandafter\ifx\csname a6\endcsname\relax U\else D\fi}",
        "3/5/U",
    ),
    // etex: 3/5
    (
        "romannumeral_names",
        r"\def\step{\advance\n1 \expandafter\edef\csname a\romannumeral\n\endcsname{\the\n}\ifnum\n<5 \expandafter\step\fi}\step
\edef\R{\csname aiii\endcsname/\csname av\endcsname}",
        "3/5",
    ),
    // etex: 6/\a6 /U
    (
        "each_name_calls_the_generator",
        r"\def\gen{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\noexpand\ifnum\n<6 \noexpand\gen\noexpand\fi}\csname a\the\n\endcsname}\gen
\edef\R{\the\n/\expandafter\noexpand\csname a6\endcsname/\expandafter\ifx\csname a7\endcsname\relax U\else D\fi}",
        r"6/\a6 /U",
    ),
    // etex: x/x1555
    (
        "growing_argument",
        r"\def\g#1{\advance\n1 \expandafter\def\csname a\the\n\endcsname{#1}\ifnum\n<5 \expandafter\g\expandafter{\expandafter#1\the\n}\fi}\g{x}
\edef\R{\csname a1\endcsname/\csname a5\endcsname}",
        "x/x1555",
    ),
    // etex: 1234
    (
        "edef_accumulates_generated_names",
        r"\def\L{}\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\edef\L{\L\expandafter\noexpand\csname a\the\n\endcsname}\ifnum\n<4 \expandafter\step\fi}\step
\edef\R{\L}",
        "1234",
    ),
    // etex: PQPQP/U
    (
        "mutual_recursion_through_a_generated_name",
        r"\def\ping{\advance\n1 \expandafter\def\csname p\the\n\endcsname{P}\ifnum\n<5 \csname po\ifodd\n ng\else ng\fi\expandafter\endcsname\fi}\def\pong{\advance\n1 \expandafter\def\csname q\the\n\endcsname{Q}\ifnum\n<5 \expandafter\ping\fi}\ping
\edef\R{\csname p1\endcsname\csname q2\endcsname\csname p3\endcsname\csname q4\endcsname\csname p5\endcsname/\expandafter\ifx\csname q5\endcsname\relax U\else D\fi}",
        "PQPQP/U",
    ),
    // etex: 0/U
    (
        "local_names_end_with_the_group",
        r"\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<3 \expandafter\step\fi}{\step}
\edef\R{\the\n/\expandafter\ifx\csname a2\endcsname\relax U\else D\fi}",
        "0/U",
    ),
    // etex: 3/3
    (
        "global_names_survive_the_group",
        r"\def\step{\global\advance\n1 \expandafter\xdef\csname a\the\n\endcsname{\the\n}\ifnum\n<3 \expandafter\step\fi}{\step}
\edef\R{\the\n/\csname a3\endcsname}",
        "3/3",
    ),
    // etex: 4/4
    (
        "aftergroup_chain",
        r"\def\ag{\global\advance\n1 \expandafter\xdef\csname a\the\n\endcsname{\the\n}\ifnum\n<4 {\aftergroup\ag}\fi}\ag
\edef\R{\the\n/\csname a4\endcsname}",
        "4/4",
    ),
    // etex: 5/5
    (
        "non_tail_recursion",
        r"\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<5 \step\fi}\step
\edef\R{\the\n/\csname a5\endcsname}",
        "5/5",
    ),
    // etex: 300/300
    (
        "deep_non_tail_recursion",
        r"\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<300 \step\fi}\step
\edef\R{\the\n/\csname a300\endcsname}",
        "300/300",
    ),
    // etex: 3/3000/D
    (
        "long_tail_loop",
        r"\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<3000 \expandafter\step\fi}\step
\edef\R{\csname a3\endcsname/\csname a3000\endcsname/\expandafter\ifx\csname a5\endcsname\relax U\else D\fi}",
        "3/3000/D",
    ),
];

#[test]
fn known_bounds_are_exact() {
    crate::support::table(
        EXACT,
        |r| r.0,
        |&(_, source, want)| {
            let got = results(&analyze(source));
            (got != [want]).then(|| format!("\\R is {got:?}, etex {want:?}"))
        },
    );
}

#[test]
fn generated_names_are_recorded_one_by_one() {
    let analysis = analyze(EXACT[0].1);
    for n in 1..=5 {
        assert_eq!(crate::support::body(&analysis, &format!("a{n}")), n.to_string());
    }
    assert!(crate::support::definition(&analysis, "a6").is_none());
}

/// Unknown bounds: `\R` covers what etex printed. The clock makes `\m`
/// unknown to satex; etex's value is the one of that run.
pub(crate) const COVERED: &[(&str, &str, &str)] = &[
    // etex: 1/D/U
    (
        "tail_unknown_bound",
        r"\m=\pdfelapsedtime \divide\m by 1000000 \advance\m 4
\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<\m \expandafter\step\fi}\step
\edef\R{\csname a1\endcsname/\expandafter\ifx\csname a3\endcsname\relax U\else D\fi/\expandafter\ifx\csname a7\endcsname\relax U\else D\fi}",
        "1/D/U",
    ),
    // etex: 1/D/U or 1/D/D
    (
        "tail_unconstrained_bound",
        r"\m=\pdfuniformdeviate 12
\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<\m \expandafter\step\fi}\step
\edef\R{\csname a1\endcsname/\expandafter\ifx\csname a3\endcsname\relax U\else D\fi/\expandafter\ifx\csname a7\endcsname\relax U\else D\fi}",
        "1/D/U",
    ),
    // etex: 1/D/U or 1/D/D
    (
        "non_tail_unconstrained_bound",
        r"\m=\pdfuniformdeviate 12
\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<\m \step\fi}\step
\edef\R{\csname a1\endcsname/\expandafter\ifx\csname a3\endcsname\relax U\else D\fi/\expandafter\ifx\csname a7\endcsname\relax U\else D\fi}",
        "1/D/U",
    ),
    // etex: 112
    (
        "register_counter_unknown_bound",
        r"\m=\pdfelapsedtime \advance\m 4
\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<\m \expandafter\step\fi}\step
\edef\R{\the\n}",
        "112",
    ),
    // etex: 88
    (
        "register_counter_loop_without_names",
        r"\m=\pdfelapsedtime \advance\m 4
\def\step{\advance\n1 \ifnum\n<\m \expandafter\step\fi}\step
\edef\R{\the\n}",
        "88",
    ),
    // etex: 6/D/U
    (
        "each_name_calls_the_generator_unknown_bound",
        r"\m=\pdfelapsedtime \divide\m by 1000000 \advance\m 6
\def\gen{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\noexpand\ifnum\n<\m \noexpand\gen\noexpand\fi}\csname a\the\n\endcsname}\gen
\edef\R{\the\n/\expandafter\ifx\csname a2\endcsname\relax U\else D\fi/\expandafter\ifx\csname a7\endcsname\relax U\else D\fi}",
        "6/D/U",
    ),
    // etex: D/U
    (
        "aftergroup_chain_unknown_bound",
        r"\m=\pdfelapsedtime \divide\m by 1000000 \advance\m 4
\def\ag{\global\advance\n1 \expandafter\xdef\csname a\the\n\endcsname{\the\n}\ifnum\n<\m {\aftergroup\ag}\fi}\ag
\edef\R{\expandafter\ifx\csname a2\endcsname\relax U\else D\fi/\expandafter\ifx\csname a9\endcsname\relax U\else D\fi}",
        "D/U",
    ),
    // etex: 5/D/D
    (
        "unknown_start",
        r"\n=\pdfelapsedtime \divide\n by 1000000
\def\step{\advance\n1 \expandafter\edef\csname a\the\n\endcsname{\the\n}\ifnum\n<5 \expandafter\step\fi}\step
\edef\R{\the\n/\expandafter\ifx\csname a5\endcsname\relax U\else D\fi/\expandafter\ifx\csname a1\endcsname\relax U\else D\fi}",
        "5/D/D",
    ),
    // etex: 111
    (
        "non_tail_unknown_bound",
        r"\m=\pdfelapsedtime
\def\step{\advance\n1 \ifnum\n<\m \step\fi}\step
\edef\R{\the\n}",
        "111",
    ),
    // etex: D/D/Y
    (
        "name_from_unknown_number_read_back",
        r"\m=\pdfelapsedtime \divide\m by 1000000 \advance\m 5 \expandafter\def\csname a\the\m\endcsname{x}
\edef\R{\expandafter\ifx\csname a5\endcsname\relax U\else D\fi/\ifcsname a5\endcsname D\else U\fi/\expandafter\ifx\csname a5\endcsname\undefined N\else Y\fi}",
        "D/D/Y",
    ),
];

fn check_covered(rows: &[(&str, &str, &str)], cfg: &Config) {
    crate::support::table(
        rows,
        |r| r.0,
        |&(_, source, want)| {
            let got = results(&analyze_with(source, cfg));
            (!got.iter().any(|g| covers(g, want))).then(|| format!("\\R is {got:?}, which misses etex's {want:?}"))
        },
    );
}

#[test]
fn unknown_bounds_cover_etex() {
    check_covered(COVERED, &cfg());
}

/// Widening at the first pass must still cover every pass.
#[test]
fn unknown_bounds_cover_etex_when_widened_at_once() {
    let mut cfg = cfg();
    cfg.limits.widen_after = 0;
    check_covered(COVERED, &cfg);
}

/// An unknown bound ends by widening: no limit, no assumption.
#[test]
fn unknown_bounds_converge_without_a_limit() {
    crate::support::table(
        COVERED,
        |r| r.0,
        |&(_, source, _)| {
            let analysis = analyze(source);
            let bad: Vec<String> = analysis
                .facts
                .diagnostics
                .iter()
                .filter(|d| d.code == "limit-reached" || d.severity == Severity::Assumption)
                .map(|d| format!("{} {}", d.code, d.message))
                .collect();
            (!bad.is_empty()).then(|| bad.join("; "))
        },
    );
}
