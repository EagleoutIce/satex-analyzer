//! What a loop of unknown bound may affect, per TeX state class: the
//! written value covers etex's, everything the loop never touches stays
//! exact, and the loop ends by a fixpoint, not a limit. Then type
//! refinement from use, with `analysis.assume_compiles` on and off.
//!
//! Every probe runs `\lp` with `\n=\pdfelapsedtime` (unknown); etex runs
//! the same source with `\n=3` (and `\n=2` where noted). A probe marks
//! what holds with `\def\okX{}` and what must not hold with `\def\badX{}`:
//! etex defines every `ok` and no `bad` one. satex must define every `ok`
//! on some path (cover) and no `bad` on any (untouched stays exact).
//! Failing rows are the to-do list: never ignore them.

use crate::support::PRELUDE;
use satex::config::Config;
use satex::machine::{Analysis, Machine};

/// `\i` counts passes, `\n` is the unknown bound.
const HEAD: &str = r"\countdef\i=200 \countdef\n=201 \n=\N \i=0 ";

fn cfg(assume_compiles: bool) -> Config {
    let mut cfg = Config {
        engine: Some(satex::config::Engine::PdfTeX),
        kernel: Some(satex::plugin::Kernel::None),
        ..crate::support::config()
    };
    cfg.limits.steps = 5_000_000;
    cfg.analysis.assume_compiles = assume_compiles;
    cfg
}

fn source(setup: &str, body: &str, check: &str) -> String {
    let head = HEAD.replace(r"\N", r"\pdfelapsedtime");
    format!(
        "{PRELUDE}{head}{setup}\\def\\lp{{\\ifnum\\i<\\n \\advance\\i1 {body}\\relax\\expandafter\\lp\\fi}}\\lp {check}"
    )
}

/// Every name defined on any path.
fn defined(analysis: &Analysis) -> Vec<String> {
    analysis.facts.defs.iter().map(|d| analysis.interner.name(d.name).to_string()).collect()
}

/// The `\def\okX`/`\def\badX` markers a probe text names.
fn markers(text: &str, kind: &str) -> Vec<String> {
    let pat = format!(r"\def\{kind}");
    let mut out: Vec<String> = text
        .match_indices(&pat)
        .map(|(i, _)| {
            let rest = &text[i + pat.len()..];
            let end = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
            format!("{kind}{}", &rest[..end])
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Diagnostics that say a limit, not a fixpoint, ended something.
const CUT: &[&str] = &["limit-reached", "line-budget-cut", "nesting-widened", "recursion-widened", "budget-exhausted"];

fn problems(analysis: &Analysis, text: &str) -> Option<String> {
    let defs = defined(analysis);
    let mut why = Vec::new();
    let missing: Vec<String> = markers(text, "ok").into_iter().filter(|m| !defs.contains(m)).collect();
    if !missing.is_empty() {
        why.push(format!("does not cover etex: {missing:?} never defined"));
    }
    let wrong: Vec<String> = markers(text, "bad").into_iter().filter(|m| defs.contains(m)).collect();
    if !wrong.is_empty() {
        why.push(format!("not exact: {wrong:?} defined"));
    }
    let cut: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).filter(|c| CUT.contains(c)).collect();
    if !cut.is_empty() || !analysis.limits.is_empty() {
        let keys: Vec<&str> = analysis.limits.iter().map(|h| h.key).collect();
        why.push(format!("ended by a limit: {cut:?} {keys:?}"));
    }
    (!why.is_empty()).then(|| why.join("; "))
}

/// One state class: (name, setup, loop body, check after the loop).
type Row = (&'static str, &'static str, &'static str, &'static str);

fn run_rows(rows: &[Row]) {
    crate::support::table(
        rows,
        |r| r.0,
        |&(_, setup, body, check)| {
            let analysis = Machine::analyze(&source(setup, body, check), None, &cfg(true));
            problems(&analysis, check)
        },
    );
}

/// Registers, by number and by computed number.
const REGISTERS: &[Row] = &[
    // etex: \count1=6, \count2=5
    (
        "count",
        r"\count2=5 \count1=0 ",
        r"\advance\count1 2 ",
        r"\ifnum\count1=6 \def\okA{}\fi \ifnum\count2=5 \else\def\badA{}\fi \ifnum\count1<0 \def\badB{}\fi ",
    ),
    // etex: \count11..13=1..3, \count5=7
    (
        "count_computed",
        r"\count5=7 ",
        r"\count\numexpr10+\i\relax=\i ",
        r"\ifnum\count13=3 \def\okA{}\fi \ifnum\count5=7 \else\def\badA{}\fi ",
    ),
    // etex: \dimen1=3pt, \dimen2=3pt
    (
        "dimen",
        r"\dimen2=3pt \dimen1=0pt ",
        r"\advance\dimen1 1pt ",
        r"\ifdim\dimen1=3pt \def\okA{}\fi \ifdim\dimen2=3pt \else\def\badA{}\fi \ifdim\dimen1<0pt \def\badB{}\fi ",
    ),
    // etex: \dimen13=3pt, \dimen5=2pt
    (
        "dimen_computed",
        r"\dimen5=2pt ",
        r"\dimen\numexpr10+\i\relax=\i pt ",
        r"\ifdim\dimen13=3pt \def\okA{}\fi \ifdim\dimen5=2pt \else\def\badA{}\fi ",
    ),
    // etex: \skip1=3pt plus 3fil, \skip2=1pt plus 2pt
    (
        "skip",
        r"\skip2=1pt plus 2pt \skip1=0pt ",
        r"\advance\skip1 1pt plus 1fil ",
        r"\ifdim\skip1=3pt \def\okA{}\fi \ifdim\skip2=1pt \else\def\badA{}\fi \ifdim\gluestretch\skip2=2pt \else\def\badB{}\fi ",
    ),
    // etex: \toks1={axxx}, \toks2={keep}
    (
        "toks_append",
        r"\toks1={a}\toks2={keep}\def\keep{keep}\def\first#1#2\end{\if#1a\else\def\badB{}\fi}",
        r"\toks1=\expandafter{\the\toks1 x}",
        r"\edef\v{\the\toks1}\def\w{axxx}\ifx\v\w\def\okA{}\fi \edef\v{\the\toks2}\ifx\v\keep\else\def\badA{}\fi \edef\v{\the\toks1}\expandafter\first\v\end ",
    ),
    // etex: \box1 an hbox, \box2 an hbox, \box3 void
    (
        "box",
        r"\setbox2=\hbox{}",
        r"\setbox1=\hbox{\unhcopy1 x}",
        r"\ifhbox1 \def\okA{}\fi \ifhbox2 \else\def\badA{}\fi \ifvoid3 \else\def\badB{}\fi ",
    ),
    // etex: \box13 an hbox, \box5 void
    (
        "box_computed",
        "",
        r"\setbox\numexpr10+\i\relax=\hbox{}",
        r"\ifhbox13 \def\okA{}\fi \ifvoid5 \else\def\badA{}\fi ",
    ),
];

/// Meanings: token lists that grow, names built from numbers, callees
/// chosen by number.
const MEANINGS: &[Row] = &[
    // etex: \t=abxxx, \u=cd
    (
        "macro_append",
        r"\def\t{ab}\def\u{cd}\def\first#1#2\end{\if#1a\else\def\badB{}\fi}",
        r"\edef\t{\t x}",
        r"\def\w{abxxx}\ifx\t\w\def\okA{}\fi \def\w{cd}\ifx\u\w\else\def\badA{}\fi \expandafter\first\t\end ",
    ),
    // etex: \d=7111: digits stay digits
    ("macro_digits", r"\def\d{7}", r"\edef\d{\d1}", r"\ifnum\d=7111 \def\okA{}\fi \ifnum\d<0 \def\badB{}\fi "),
    // etex: \f1..\f3 defined to their digit, \g3 not
    (
        "csname_family",
        "",
        r"\expandafter\edef\csname f\the\i\endcsname{\the\i}",
        r"\expandafter\ifx\csname f3\endcsname\relax\else\def\okA{}\fi \expandafter\ifx\csname g3\endcsname\relax\else\def\badA{}\fi \ifnum\csname f2\endcsname=2 \def\okB{}\fi \ifnum\csname f2\endcsname<0 \def\badB{}\fi ",
    ),
    // etex: \h1..\h3 run, \count21..23=1, \count29=4
    (
        "callee_family",
        r"\count29=4 \def\mk#1{\expandafter\def\csname h#1\endcsname{\advance\count2#1 1 }}\mk1\mk2\mk3\mk4\mk5",
        r"\csname h\the\i\endcsname",
        r"\ifnum\count23=1 \def\okA{}\fi \ifnum\count21=1 \def\okB{}\fi \ifnum\count29=4 \else\def\badA{}\fi ",
    ),
    // etex: as callee_family, the name passed as an argument (\@nameuse)
    (
        "callee_nameuse",
        r"\count29=4 \def\use#1{\csname#1\endcsname}\def\mk#1{\expandafter\def\csname h#1\endcsname{\advance\count2#1 1 }}\mk1\mk2\mk3\mk4\mk5",
        r"\use{h\the\i}",
        r"\ifnum\count23=1 \def\okA{}\fi \ifnum\count21=1 \def\okB{}\fi \ifnum\count29=4 \else\def\badA{}\fi ",
    ),
    // etex: the callee defines a new name per pass; \k3 defined, \kk not
    (
        "callee_defines",
        r"\def\mk#1{\expandafter\def\csname k#1\endcsname{}}",
        r"\expandafter\mk\expandafter{\the\i}",
        r"\expandafter\ifx\csname k3\endcsname\relax\else\def\okA{}\fi \ifdefined\kk\def\badA{}\fi ",
    ),
];

/// Character code tables (tex.web §§ 230-236).
const CODES: &[Row] = &[
    // etex: \catcode131=11, ! stays 12, a stays 11
    (
        "catcode_computed",
        "",
        r"\catcode\numexpr128+\i\relax=11 ",
        r"\ifnum\catcode131=11 \def\okA{}\fi \ifnum\catcode`\!=12 \else\def\badA{}\fi \ifnum\catcode`\a=11 \else\def\badB{}\fi ",
    ),
    // etex: \catcode`\~=12, ! stays 12
    (
        "catcode_char",
        "",
        r"\catcode`\~=\ifodd\i 12\else 13\fi ",
        r"\ifnum\catcode`\~=12 \def\okA{}\fi \ifnum\catcode`\!=12 \else\def\badA{}\fi ",
    ),
    // etex: \lccode131=3, \lccode`\A=97
    (
        "lccode",
        "",
        r"\lccode\numexpr128+\i\relax=\i ",
        r"\ifnum\lccode131=3 \def\okA{}\fi \ifnum\lccode`\A=97 \else\def\badA{}\fi ",
    ),
    // etex: \uccode131=3, \uccode`\a=65
    (
        "uccode",
        "",
        r"\uccode\numexpr128+\i\relax=\i ",
        r"\ifnum\uccode131=3 \def\okA{}\fi \ifnum\uccode`\a=65 \else\def\badA{}\fi ",
    ),
    // etex: \sfcode131=3, \sfcode`\A=999
    (
        "sfcode",
        "",
        r"\sfcode\numexpr128+\i\relax=\i ",
        r"\ifnum\sfcode131=3 \def\okA{}\fi \ifnum\sfcode`\A=999 \else\def\badA{}\fi ",
    ),
    // etex: \mathcode131=3, \mathcode`\a=29025 ("7161)
    (
        "mathcode",
        "",
        r"\mathcode\numexpr128+\i\relax=\i ",
        r"\ifnum\mathcode131=3 \def\okA{}\fi \ifnum\mathcode`\a=29025 \else\def\badA{}\fi ",
    ),
    // etex: \delcode131=3, \delcode`\a=-1
    (
        "delcode",
        "",
        r"\delcode\numexpr128+\i\relax=\i ",
        r"\ifnum\delcode131=3 \def\okA{}\fi \ifnum\delcode`\a=-1 \else\def\badA{}\fi ",
    ),
];

/// Parameters, fonts, interaction, waiting tokens, streams.
const PARAMETERS: &[Row] = &[
    // etex: \everypar={axxx}, \everyhbox={keep}
    (
        "every_append",
        r"\everypar={a}\everyhbox={keep}\def\keep{keep}",
        r"\everypar=\expandafter{\the\everypar x}",
        r"\edef\v{\the\everypar}\def\w{axxx}\ifx\v\w\def\okA{}\fi \edef\v{\the\everyhbox}\ifx\v\keep\else\def\badA{}\fi ",
    ),
    // etex: \tolerance=6, \pretolerance=7
    (
        "integer",
        r"\pretolerance=7 \tolerance=0 ",
        r"\advance\tolerance 2 ",
        r"\ifnum\tolerance=6 \def\okA{}\fi \ifnum\pretolerance=7 \else\def\badA{}\fi ",
    ),
    // etex: \endlinechar=-1, \escapechar=92
    (
        "endlinechar",
        "",
        r"\endlinechar=\ifodd\i -1\else 13\fi ",
        r"\ifnum\endlinechar=-1 \def\okA{}\fi \ifnum\escapechar=92 \else\def\badA{}\fi ",
    ),
    // etex: \escapechar=47, \newlinechar=0
    (
        "escapechar",
        "",
        r"\escapechar=\ifodd\i 47\else 92\fi ",
        r"\ifnum\escapechar=47 \def\okA{}\fi \ifnum\newlinechar=0 \else\def\badA{}\fi ",
    ),
    // etex: \globaldefs=1, \tolerance=10000
    (
        "globaldefs",
        "",
        r"\globaldefs=\ifodd\i 1\else 0\fi ",
        r"\ifnum\globaldefs=1 \def\okA{}\fi \ifnum\tolerance=10000 \else\def\badA{}\fi ",
    ),
    // etex: \fontdimen2\f=3pt, \fontdimen6\f=10.00002pt
    (
        "fontdimen",
        r"\font\f=cmr10 ",
        r"\fontdimen2\f=\i pt ",
        r"\ifdim\fontdimen2\f=3pt \def\okA{}\fi \ifdim\fontdimen6\f=10.00002pt \else\def\badA{}\fi ",
    ),
    // etex: \hyphenchar\f=3, \skewchar\f=0 (\defaultskewchar)
    (
        "hyphenchar",
        r"\font\f=cmr10 ",
        r"\hyphenchar\f=\i ",
        r"\ifnum\hyphenchar\f=3 \def\okA{}\fi \ifnum\skewchar\f=0 \else\def\badA{}\fi ",
    ),
    // etex: the current font is \f, \textfont0 stays \nullfont
    (
        "current_font",
        r"\font\f=cmr10 ",
        r"\ifodd\i\f\else\nullfont\fi ",
        r"\expandafter\ifx\the\font\f\def\okA{}\fi \expandafter\ifx\the\textfont0\nullfont\else\def\badA{}\fi ",
    ),
    // etex: \interactionmode=0, \tolerance=10000
    (
        "interaction",
        "",
        r"\ifodd\i\batchmode\else\nonstopmode\fi ",
        r"\ifnum\interactionmode=0 \def\okA{}\fi \ifnum\tolerance=10000 \else\def\badA{}\fi ",
    ),
    // etex: \count7=3 after the group, \count8=4
    (
        "aftergroup",
        r"\count8=4 \def\y{\advance\count7 1 }\begingroup ",
        r"\aftergroup\y ",
        r"\endgroup\ifnum\count7=3 \def\okA{}\fi \ifnum\count8=4 \else\def\badA{}\fi ",
    ),
    // etex: \count7=3 once the waiting \y runs, \count8=4
    (
        "afterassignment",
        r"\count8=4 \def\y{\global\advance\count7 1 }",
        r"\afterassignment\y ",
        r"\count9=0 \ifnum\count7=3 \def\okA{}\fi \ifnum\count8=4 \else\def\badA{}\fi ",
    ),
    // etex: write streams opened and closed per pass; \read stream 9 untouched
    (
        "streams",
        "",
        r"\immediate\openout\i=wsout\the\i.tmp \immediate\write\i{x}\immediate\closeout\i \openin\i=wsnone\the\i \closein\i ",
        r"\def\okA{}\ifeof9 \else\def\badA{}\fi ",
    ),
];

/// Typeset output, last items, nesting.
const NESTING: &[Row] = &[
    // etex: horizontal mode, not inner
    ("mode", "", r"\ifodd\i\hbox{}\else\indent\fi ", r"\ifhmode\def\okA{}\fi \ifinner\def\badA{}\fi "),
    // etex: \lastskip=3pt
    (
        "lastskip",
        r"\indent ",
        r"\hskip\i pt ",
        r"\ifdim\lastskip=3pt \def\okA{}\fi \ifdim\lastskip<0pt \def\badA{}\fi ",
    ),
    // etex: \currentgrouplevel=3
    (
        "group_depth",
        "",
        r"\begingroup ",
        r"\ifnum\currentgrouplevel=3 \def\okA{}\fi \ifnum\currentgrouplevel<0 \def\badA{}\fi ",
    ),
    // etex: \currentiflevel=4 inside the test (each pass leaves its \ifnum open)
    (
        "conditional_depth",
        r"\def\lpb{\iftrue}",
        r"\lpb ",
        r"\ifnum\currentiflevel=4 \def\okA{}\fi \ifnum\currentiflevel<0 \def\badA{}\fi ",
    ),
];

#[test]
fn registers() {
    run_rows(REGISTERS);
}

#[test]
fn meanings_and_callees() {
    run_rows(MEANINGS);
}

#[test]
fn code_tables() {
    run_rows(CODES);
}

#[test]
fn parameters_fonts_streams() {
    run_rows(PARAMETERS);
}

#[test]
fn modes_and_nesting() {
    run_rows(NESTING);
}

/// expl3 variables under the real kernel.
#[test]
fn expl3_variables() {
    // etex (latex format, \N=3): \l_tmpa_tl=abxxx, \g_tmpa_int=6, the others untouched
    let probe = r"\ExplSyntaxOn
\tl_set:Nn \l_tmpa_tl {ab} \tl_set:Nn \l_tmpb_tl {keep} \int_gset:Nn \g_tmpb_int {5}
\int_step_inline:nn {\N} { \tl_put_right:Nn \l_tmpa_tl {x} \int_gadd:Nn \g_tmpa_int {2} }
\tl_if_eq:NnT \l_tmpa_tl {abxxx} {\def\okA{}} \tl_if_eq:NnF \l_tmpb_tl {keep} {\def\badA{}}
\int_compare:nNnT \g_tmpa_int = {6} {\def\okB{}} \int_compare:nNnF \g_tmpb_int = {5} {\def\badB{}}
\int_compare:nNnT \g_tmpa_int < {0} {\def\badC{}}
\ExplSyntaxOff";
    let mut cfg = crate::support::config();
    cfg.load = satex::config::Load { packages: false, classes: false, ..Default::default() };
    let analysis = Machine::analyze(&probe.replace(r"\N", r"\pdfelapsedtime"), None, &cfg);
    if let Some(why) = problems(&analysis, probe) {
        panic!("{why}");
    }
}

/// Type refinement from use. Each probe makes `\x` one of several texts
/// (a join, or a widened loop) and reads it where TeX requires a type.
/// `\isdig` marks the path where `\x` begins with a digit, `\islet` where it
/// begins with a letter; etex with `\n=3` takes the first (it compiles),
/// with `\n=2` the second (it raises "Missing number" and goes on).
/// (name, setup, loop body, the typed use, whether `\n=2` errs in etex)
type Refine = (&'static str, &'static str, &'static str, &'static str, bool);

const REFINE: &[Refine] = &[
    // etex \n=3: \x=1, compiles; \n=2: \x=a, Missing number, \count1=0
    ("number_join", r"\def\x{}", r"\edef\x{\ifodd\i 1\else a\fi}", r"\count1=\x\relax ", true),
    // etex \n=3: \x=1, compiles; \n=2: \x=a, Missing number, \ifnum 0>0
    ("ifnum_join", r"\def\x{}", r"\edef\x{\ifodd\i 1\else a\fi}", r"\ifnum\x>0 \fi ", true),
    // etex \n=3: \x=1a1, \count1=1; \n=2: \x=1a, \count1=1: never an error
    ("number_widened", r"\def\x{}", r"\edef\x{\x\ifodd\i 1\else a\fi}", r"\count1=\x\relax ", false),
    // etex \n=3: \x=1, \dimen1=1pt; \n=2: \x=a, Missing number, Illegal unit
    ("dimen_join", r"\def\x{}", r"\edef\x{\ifodd\i 1\else a\fi}", r"\dimen1=\x pt\relax ", true),
];

const CLASSIFY: &str = r"\def\first#1#2\end{\ifcat#1a\def\islet{}\fi\ifcat#11\def\isdig{}\fi}";

fn refine_source(setup: &str, body: &str, use_: &str) -> String {
    // The use, then the binding read again: a refinement must stick to it.
    source(&format!("{CLASSIFY}{setup}"), body, &format!(r"{use_}\expandafter\first\x\end "))
}

/// Assumption on: the inputs that compile are exactly etex's (`\isdig` only),
/// the error path is dropped with its facts, and the drop is reported.
#[test]
fn refinement_with_assume_compiles() {
    crate::support::table(
        REFINE,
        |r| r.0,
        |&(_, setup, body, use_, errs)| {
            let analysis = Machine::analyze(&refine_source(setup, body, use_), None, &cfg(true));
            let defs = defined(&analysis);
            let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
            let mut why = Vec::new();
            if !defs.iter().any(|d| d == "isdig") {
                why.push("\\isdig never defined".to_string());
            }
            if defs.iter().any(|d| d == "islet") {
                why.push("\\islet defined: the path that cannot compile (or its facts) survived".to_string());
            }
            if errs && !codes.contains(&"assumed-compiles") {
                why.push(format!("no assumed-compiles reported: {codes:?}"));
            }
            (!why.is_empty()).then(|| why.join("; "))
        },
    );
}

/// Assumption off: every etex run is covered, the erring one included,
/// and the possible error is reported, never assumed away.
#[test]
fn refinement_without_assume_compiles() {
    crate::support::table(
        REFINE,
        |r| r.0,
        |&(_, setup, body, use_, errs)| {
            let analysis = Machine::analyze(&refine_source(setup, body, use_), None, &cfg(false));
            let defs = defined(&analysis);
            let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
            let mut why = Vec::new();
            if !defs.iter().any(|d| d == "isdig") {
                why.push("\\isdig never defined".to_string());
            }
            if errs && !defs.iter().any(|d| d == "islet") {
                why.push("\\islet never defined: etex \\n=2 defines it".to_string());
            }
            if errs && !codes.contains(&"missing-number") {
                why.push(format!("no missing-number reported: {codes:?}"));
            }
            if codes.contains(&"assumed-compiles") {
                why.push("assumed-compiles with the assumption off".to_string());
            }
            (!why.is_empty()).then(|| why.join("; "))
        },
    );
}

/// An error certain on every concrete run is real: reported and its path
/// kept, with or without the assumption.
#[test]
fn certain_error_is_kept() {
    // etex: Missing number, \count1=0, then \okE is defined
    let src = format!(
        "{PRELUDE}{}\\def\\z{{a}}\\ifnum\\n>5 \\count1=\\z\\relax\\def\\okE{{}}\\fi ",
        HEAD.replace(r"\N", r"\pdfelapsedtime")
    );
    for on in [true, false] {
        let analysis = Machine::analyze(&src, None, &cfg(on));
        let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
        assert!(codes.contains(&"missing-number"), "assume_compiles={on}: not reported: {codes:?}");
        assert!(defined(&analysis).iter().any(|d| d == "okE"), "assume_compiles={on}: the path was dropped");
    }
}
