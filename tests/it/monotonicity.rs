//! Property: raising a precision limit never makes the analysis worse.
//!
//! For each MWE and each of the seven precision limits, a run at a lower
//! value is compared against a run at a higher value (never against the
//! default directly — the default only sets the magnitude of "higher" and
//! "lower"). The higher-limit run must not: drop a broken gap the lower run
//! didn't have (a *new* one is fine to shrink, never to grow), lose a fact
//! the lower run recorded, or leave more conditionals undecided.

use crate::support::PRELUDE;
use std::collections::BTreeSet;

use crate::support::installed;
use satex::config::{Config, Limits, Load, Texmf};
use satex::facts::Severity;
use satex::machine::{Analysis, Machine};

// INITEX catcodes, as tests/it/semantics.rs's `analyze` uses them: no
// format, no packages, so these MWEs run in milliseconds.

fn base_initex() -> Config {
    let mut cfg = Config {
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    cfg.limits.seconds = 0;
    cfg
}

fn base_latex() -> Config {
    crate::support::config()
}

fn analyze_initex(source: &str, limits: &Limits) -> Analysis {
    let mut cfg = base_initex();
    cfg.limits = limits.clone();
    cfg.limits.seconds = 0;
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

fn analyze_latex(source: &str, limits: &Limits) -> Analysis {
    let mut cfg = base_latex();
    cfg.limits = limits.clone();
    cfg.limits.seconds = 0;
    Machine::analyze(source, None, &cfg)
}

/// Codes for *resource* limits (steps/memory/time), not under test here:
/// a run cut off by them is not a precision regression.
const RESOURCE_EXEMPT: &[&str] = &["limit-reached", "budget-exhausted", "repeated-failure"];

fn broken_gaps(analysis: &Analysis) -> BTreeSet<(&'static str, u16, u32)> {
    analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Unsupported && !RESOURCE_EXEMPT.contains(&d.code))
        .map(|d| (d.code, d.span.file, d.span.line))
        .collect()
}

/// Every label/ref/cite/... occurrence and every definition, keyed so a
/// run's set can be compared against another's regardless of the source
/// text's own ordering.
fn fact_keys(analysis: &Analysis) -> BTreeSet<(String, String, u16, u32)> {
    let mut set = BTreeSet::new();
    // A write is a fact by its place: what it writes is a value, which a
    // higher limit may make more precise (a page count of 1, not 1 or 2).
    for o in &analysis.facts.occurrences {
        let key = if o.kind == satex::builtins::OccKind::Write { String::new() } else { o.key.clone() };
        set.insert((format!("{:?}", o.kind), key, o.span.file, o.span.line));
    }
    for d in &analysis.facts.defs {
        set.insert(("def".to_string(), analysis.interner.name(d.name).to_string(), d.span.file, d.span.line));
    }
    set
}

fn undecided_count(analysis: &Analysis) -> u32 {
    analysis.facts.diagnostics.iter().filter(|d| d.code == "undecided-condition").map(|d| d.count).sum()
}

const LIMIT_NAMES: &[&str] = &[
    "choices",
    "paths",
    "meaning_splits",
    "widen_after",
    "recursion_depth",
    "meaning_set",
    "value_set",
    "interval_set",
];

fn get(limits: &Limits, name: &str) -> u16 {
    match name {
        "choices" => limits.choices,
        "paths" => limits.paths,
        "meaning_splits" => limits.meaning_splits,
        "widen_after" => limits.widen_after,
        "recursion_depth" => limits.recursion_depth,
        "meaning_set" => limits.meaning_set,
        "value_set" => limits.value_set,
        "interval_set" => limits.interval_set,
        _ => unreachable!("{name}"),
    }
}

fn set(limits: &mut Limits, name: &str, value: u16) {
    match name {
        "choices" => limits.choices = value,
        "paths" => limits.paths = value,
        "meaning_splits" => limits.meaning_splits = value,
        "widen_after" => limits.widen_after = value,
        "recursion_depth" => limits.recursion_depth = value,
        "meaning_set" => limits.meaning_set = value,
        "value_set" => limits.value_set = value,
        "interval_set" => limits.interval_set = value,
        _ => unreachable!("{name}"),
    }
}

/// Default x2 or +4, whichever is larger (a zero default goes to 4); the
/// lower half, at least 1.
fn higher_lower(default: u16) -> (u16, u16) {
    let higher = if default == 0 { 4 } else { default.saturating_mul(2).max(default.saturating_add(4)) };
    let lower = (default / 2).max(1);
    (higher, lower)
}

fn violations(limit: &str, mwe: &str, lo: &Analysis, hi: &Analysis) -> Vec<String> {
    let mut v = Vec::new();

    let lo_gaps = broken_gaps(lo);
    let hi_gaps = broken_gaps(hi);
    for g in hi_gaps.difference(&lo_gaps) {
        v.push(format!("{mwe}/{limit}: new broken gap at the higher limit not seen at the lower one: {g:?}"));
    }

    let lo_facts = fact_keys(lo);
    let hi_facts = fact_keys(hi);
    for f in lo_facts.difference(&hi_facts) {
        v.push(format!("{mwe}/{limit}: fact present at the lower limit lost at the higher one: {f:?}"));
    }

    let lo_undecided = undecided_count(lo);
    let hi_undecided = undecided_count(hi);
    if hi_undecided > lo_undecided {
        v.push(format!(
            "{mwe}/{limit}: undecided-condition count grew going from the lower to the higher limit: {lo_undecided} -> {hi_undecided}"
        ));
    }

    v
}

fn check_mwe(mwe: &str, source: &str, initex: bool) -> Vec<String> {
    let mut out = Vec::new();
    let defaults = Limits::default();
    for &name in LIMIT_NAMES {
        let default = get(&defaults, name);
        let (higher, lower) = higher_lower(default);
        let mut lo_limits = defaults.clone();
        set(&mut lo_limits, name, lower);
        let mut hi_limits = defaults.clone();
        set(&mut hi_limits, name, higher);
        let (lo, hi) = if initex {
            (analyze_initex(source, &lo_limits), analyze_initex(source, &hi_limits))
        } else {
            (analyze_latex(source, &lo_limits), analyze_latex(source, &hi_limits))
        };
        out.extend(violations(name, mwe, &lo, &hi));
    }
    out
}

// --- INITEX MWEs: no installation needed, each runs in milliseconds. ---

/// Minimal examples, each with a tag, its source, and whether it runs under
/// INITEX (no installation needed); the others are skipped without one.
const MWES: &[(&str, &str, &str, bool)] = &[
    // `\ifx` of a name a join left with several meanings, decided per meaning
    // (tests/it/semantics.rs `ifx_on_a_joined_meaning_is_decided_per_meaning`).
    (
        "ifx_on_a_joined_meaning_is_decided_per_meaning",
        "ifx_joined_meaning",
        r"\def\a{A}\def\b{B}\ifnum\pdfuniformdeviate2=0 \let\next\a\else\let\next\b\fi
          \ifx\next\a\def\wasA{1}\else\def\wasB{1}\fi
          \ifx\next\relax\def\wasRelax{1}\fi",
        true,
    ),
    // A dispatch loop whose exit is undecided ends
    // (tests/it/semantics.rs `a_dispatch_loop_on_a_joined_meaning_ends`).
    (
        "dispatch_loop_on_a_joined_meaning_ends",
        "dispatch_loop",
        r"\def\loop{\ifnum\pdfuniformdeviate2=0 \let\next\loop\else\let\next\relax\fi\next}\loop\def\after{done}",
        true,
    ),
    // A loop that takes unknown text apart at its spaces ends
    // (tests/it/semantics.rs `a_loop_over_unknown_text_ends`).
    (
        "loop_over_unknown_text_ends",
        "loop_over_unknown_text",
        r"\ifnum\pdfuniformdeviate2=0 \toks0{a b}\else\toks0{c d e}\fi
\edef\u{\the\toks0}
\def\strip#1>{}
\def\rem#1 #2{#1\ifx\stop#2\else\space\expandafter\rem\expandafter#2\fi}
\edef\R{\expandafter\expandafter\expandafter\rem\expandafter\strip\meaning\u \stop}",
        true,
    ),
    // A `\global` assignment on one path is undone before the next path runs;
    // the join afterwards leaves `\ifx\g\relax` undecided
    // (tests/it/semantics.rs `a_global_assignment_stays_on_its_path`).
    (
        "global_assignment_join_stays_undecided_correctly",
        "global_assignment_join",
        r"\ifnum\pdfuniformdeviate2=0 \global\let\g\relax\else\ifx\g\relax\def\leak{yes}\fi\fi\def\after{done}",
        true,
    ),
    (
        "tikz_rounded_node_of_unknown_height_stays_balanced",
        "tikz_rounded_node",
        r"\documentclass{article}
\usepackage{tikz}
\begin{document}
\begin{tikzpicture}
\node (0) {N};
\node[text width=4cm, rounded corners, draw] (1) {p};
\end{tikzpicture}
\label{after}
\end{document}",
        false,
    ),
    (
        "tikz_positioned_node_of_unknown_height_is_clean",
        "tikz_positioned_node",
        r"\documentclass{article}
\usepackage{tikz}
\usetikzlibrary{positioning}
\begin{document}
\begin{tikzpicture}
\node (0) {N};
\node[below=1mm of 0, text width=4cm, draw] (1) {p};
\end{tikzpicture}
\label{after}
\end{document}",
        false,
    ),
    (
        "esphack_after_an_unknown_skip_loses_nothing",
        "esphack_unknown_skip",
        r"\documentclass{article}
\begin{document}
Text \label{a} more\index{x} and \label{b} then
\hspace{1em}\label{c}. \par\vskip\parskip \label{d}
\end{document}",
        false,
    ),
    (
        "microtype_protrusion_reads_the_whole_document",
        "microtype_protrusion",
        r"\documentclass{article}
\usepackage{microtype}
\begin{document}
\section{One}\label{one}
``Quoted'' text, \textit{italic} and more.\label{two}
\begin{itemize}\item An item.\label{three}\end{itemize}
\end{document}",
        false,
    ),
    (
        "different_aftergroup_tokens_join_into_one_of_them",
        "aftergroup_join",
        r"\documentclass{article}
\def\one{\gdef\seen{1}}\def\two{\gdef\seen{2}}\def\three{\gdef\seen{3}}
\begin{document}
{\ifcase\numexpr\lastskip/65536\relax \aftergroup\one\or\aftergroup\two\else\aftergroup\three\fi}
\label{after}
\end{document}",
        false,
    ),
];

#[test]
fn mwes_have_no_violations() {
    let installed = installed();
    crate::support::table(
        MWES,
        |r| r.0,
        |&(_, tag, source, initex)| {
            if !initex && !installed {
                return None;
            }
            let violations = check_mwe(tag, source, initex);
            (!violations.is_empty()).then(|| format!("{violations:#?}"))
        },
    );
}

/// Every arm of an undecided `\ifcase` runs as its own path
/// (tests/it/semantics.rs `an_undecided_ifcase_runs_every_arm`).
#[test]
fn undecided_ifcase_runs_every_arm() {
    let violations =
        check_mwe("undecided_ifcase", r"\ifcase\lastpenalty\def\a{0}\or\def\b{1}\else\def\c{2}\fi\def\d{3}", true);
    assert!(violations.is_empty(), "{violations:#?}");
}

// --- LaTeX MWEs: gated on a real installation, anonymized from
// tests/it/latex.rs's fixtures (generic article class, generic text). ---
