//! Never hang: pathological input under small limits ends within a bound,
//! whatever loop it drives — expansion, `\edef`, `\csname`, `\expanded`,
//! `\message`, number and keyword scans, delimited arguments, skipped
//! conditionals, undecided loops, groups and `\scantokens`.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use satex::config::{Config, Engine};
use satex::machine::Machine;
use satex::plugin::Kernel;

/// Each input, with a budget small enough that a run reading tokens at any
/// sane rate reaches it in well under the bound.
const INPUTS: &[(&str, &str)] = &[
    ("expansion", "\\def\\a{\\a}\\a"),
    ("growing expansion", "\\def\\a{x\\a}\\a"),
    ("doubling", "\\def\\a{\\a\\a}\\a"),
    ("edef", "\\def\\a{x\\a}\\edef\\b{\\a}"),
    ("xdef of nothing", "\\def\\a{\\a}\\xdef\\b{\\a}"),
    ("csname", "\\def\\a{x\\a}\\csname\\a\\endcsname"),
    ("ifcsname", "\\def\\a{x\\a}\\ifcsname\\a\\endcsname\\fi"),
    ("expanded", "\\def\\a{x\\a}\\expanded{\\a}"),
    ("message", "\\def\\a{x\\a}\\message{\\a}"),
    ("write", "\\def\\a{x\\a}\\immediate\\write16{\\a}"),
    ("number", "\\def\\a{1\\a}\\count0=\\a"),
    ("dimension", "\\def\\a{1\\a}\\dimen0=\\a pt"),
    ("keyword fill", "\\def\\a{l\\a}\\hskip 1pt plus 1fil\\a"),
    ("delimited argument", "\\def\\a{x\\a}\\def\\b#1\\end{}\\expandafter\\b\\a"),
    ("undelimited argument", "\\def\\a#1{\\a{#1#1}}\\a x"),
    ("skipped conditional", "\\def\\a{\\iffalse\\a}\\a"),
    ("skipping forever", "\\def\\a{x\\a}\\iffalse\\a\\fi"),
    ("undecided loop", "\\def\\a{\\ifnum\\x>0 \\advance\\count1 1 \\expandafter\\a\\fi}\\a"),
    (
        "undecided loop with an unknown counter",
        "\\setbox0\\hbox{x}\\def\\a{\\advance\\dimen1 1pt \\ifdim\\dimen1<\\wd0 \\expandafter\\a\\fi}\\a",
    ),
    ("dispatch loop", "\\def\\a{\\ifnum\\x>0 \\let\\n\\a\\else\\let\\n\\relax\\fi\\edef\\c{\\c x}\\n}\\def\\c{}\\a"),
    ("undecided recursion", "\\def\\a{\\ifx\\x\\y\\a\\else\\a\\a\\fi}\\a"),
    ("groups", "\\def\\a{\\begingroup\\a}\\a"),
    ("braces", "\\def\\a{{\\a}\\a"),
    ("assignments", "\\def\\a{\\global\\advance\\count1 1 \\a}\\a"),
    ("scantokens", "\\def\\a{\\scantokens{\\a}}\\a"),
    ("everyeof", "\\everyeof{\\a}\\def\\a{\\scantokens{}}\\a"),
    ("uppercase", "\\def\\a{\\uppercase{\\a}}\\a"),
    ("afterassignment", "\\def\\a{\\afterassignment\\a\\count1=1 }\\a"),
    ("aftergroup", "\\def\\a{{\\aftergroup\\a}}\\a"),
    ("romannumeral", "\\def\\a{\\romannumeral 100000000 \\a}\\edef\\b{\\a}"),
    ("toks", "\\def\\a{\\toks0\\expandafter{\\the\\toks0 x}\\a}\\a"),
    ("string", "\\def\\a{\\edef\\b{\\detokenize\\expandafter{\\b\\b}}\\a}\\def\\b{x}\\a"),
    ("box", "\\def\\a{\\setbox0\\hbox{\\a}}\\a"),
    ("output", "\\output{\\a}\\def\\a{\\shipout\\hbox{}\\a}\\a"),
    // Each step touches everything built so far: bounded by the budget
    // only if such work counts.
    (
        "message of a register",
        "\\def\\b{xxxxxxxxxxxxxxxx}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b}\\toks0\\expandafter{\\b}\\def\\a{\\message{\\the\\toks0}\\a}\\a",
    ),
    (
        "edef of a register",
        "\\def\\b{xxxxxxxxxxxxxxxx}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b}\\toks0\\expandafter{\\b}\\def\\a{\\edef\\c{\\the\\toks0}\\a}\\a",
    ),
    (
        "comparing macros",
        "\\def\\b{xxxxxxxxxxxxxxxx}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b}\\toks0\\expandafter{\\b}\\let\\c\\b\\edef\\c{\\b}\\def\\a{\\ifx\\b\\c\\fi\\a}\\a",
    ),
    (
        "doubling a register",
        "\\toks0{x}\\def\\a{\\toks0\\expandafter{\\the\\expandafter\\toks\\expandafter0\\the\\toks0}\\a}\\a",
    ),
    (
        "showing a register",
        "\\def\\b{xxxxxxxxxxxxxxxx}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b}\\toks0\\expandafter{\\b}\\def\\a{\\showthe\\toks0 \\a}\\a",
    ),
    (
        "meaning in message",
        "\\def\\b{xxxxxxxxxxxxxxxx}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b}\\toks0\\expandafter{\\b}\\def\\a{\\message{\\meaning\\b}\\a}\\a",
    ),
    (
        "detokenize in write",
        "\\def\\b{xxxxxxxxxxxxxxxx}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b}\\toks0\\expandafter{\\b}\\def\\a{\\immediate\\write16{\\detokenize\\expandafter{\\the\\toks0}}\\a}\\a",
    ),
    (
        "scantokens of a register",
        "\\def\\b{xxxxxxxxxxxxxxxx}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b\\b\\b\\b\\b}\\edef\\b{\\b\\b\\b\\b}\\toks0\\expandafter{\\b}\\def\\a{\\edef\\c{\\scantokens\\expandafter{\\the\\toks0}}\\a}\\a",
    ),
];

fn cfg(steps: u64) -> Config {
    let mut cfg = Config { engine: Some(Engine::PdfTeX), kernel: Some(Kernel::None), ..crate::support::config() };
    cfg.limits.steps = steps;
    cfg.limits.stall_tokens = steps / 4;
    cfg.limits.seconds = 0;
    cfg
}

/// One gap at one place, however often the path reaches it, is counted to
/// `limits.repeat_failures` and no further: the expansion is then given up
/// and widened, because a path that fails at a site fails there again on
/// every round.
mod repeated_failures {
    use satex::config::{Config, Engine};
    use satex::facts::Severity;
    use satex::machine::{Analysis, Machine};
    use satex::plugin::Kernel;

    /// How often the input below reaches the one failing site.
    const USES: usize = 40;

    /// A code table entry satex cannot pin down (the engine's clock names
    /// the character), always the one in `\x`'s body, so every occurrence
    /// carries the same code and the same span. The engine's own errors
    /// (`extra-fi`) are recovered as TeX recovers them and never cut.
    fn same_gap_n_times() -> String {
        format!("\\catcode`\\{{=1 \\catcode`\\}}=2 \\def\\x{{\\lccode\\pdfelapsedtime=11 }}{}", "\\x".repeat(USES))
    }

    fn cfg(bound: u32) -> Config {
        let mut cfg = Config { engine: Some(Engine::PdfTeX), kernel: Some(Kernel::None), ..crate::support::config() };
        cfg.limits.steps = 200_000;
        cfg.limits.stall_tokens = 50_000;
        cfg.limits.seconds = 0;
        cfg.limits.repeat_failures = bound;
        cfg
    }

    /// The count of every unsupported row carrying `code`.
    fn rows(analysis: &Analysis, code: &str) -> Vec<u32> {
        analysis
            .facts
            .diagnostics
            .iter()
            .filter(|d| d.code == code && d.severity == Severity::Unsupported)
            .map(|d| d.count)
            .collect()
    }

    /// The default the rest of the tests assert against.
    #[test]
    fn the_default_bound_is_eight() {
        assert_eq!(Config::default().limits.repeat_failures, 8);
    }

    /// Exactly `repeat_failures` of the gap, then one `repeated-failure`
    /// saying the expansion was widened.
    #[test]
    fn one_site_reports_the_gap_exactly_the_bound_of_times() {
        let analysis = Machine::analyze(&same_gap_n_times(), None, &cfg(8));
        assert_eq!(rows(&analysis, "unknown-char-code"), vec![8]);
        assert_eq!(rows(&analysis, "repeated-failure"), vec![1]);
    }

    /// The bound is the limit's, not a constant: three means three.
    #[test]
    fn the_bound_follows_the_limit() {
        let analysis = Machine::analyze(&same_gap_n_times(), None, &cfg(3));
        assert_eq!(rows(&analysis, "unknown-char-code"), vec![3]);
        assert_eq!(rows(&analysis, "repeated-failure"), vec![1]);
    }

    /// Zero lifts the bound, which is what the run did before: every
    /// occurrence is counted and nothing cuts the path.
    #[test]
    fn zero_lifts_the_bound() {
        let analysis = Machine::analyze(&same_gap_n_times(), None, &cfg(0));
        assert_eq!(rows(&analysis, "unknown-char-code"), vec![USES as u32]);
        assert_eq!(rows(&analysis, "repeated-failure"), Vec::<u32>::new());
    }

    /// The cut spends the budget on the rest of the document instead of on
    /// one site: `budget-exhausted` is never reached.
    #[test]
    fn the_cut_keeps_the_run_inside_its_budget() {
        let analysis = Machine::analyze(&same_gap_n_times(), None, &cfg(8));
        assert_eq!(rows(&analysis, "budget-exhausted"), Vec::<u32>::new());
        assert!(!analysis.exhausted, "the run was exhausted");
    }
}

/// A macro whose entire body is itself (a "quark" in expl3, e.g. `\q_nil`)
/// is a fixed point by construction: it takes no argument, so nothing about
/// a re-entry at its call site can ever shrink, and expanding it can never
/// read on or assign. expl3 never expands one, only compares it, but a path
/// satex could not rule out may try to — the way `\__prop_if_flat:NTF`
/// reaches `\q_nil` through `\exp_after:wN` in expl3-code.tex. Before this
/// was recognized, such a call was only ever cut by the generic
/// `limits.site_expansions` bound (16384 rounds); now it is cut at
/// `limits.recursion_depth` rounds instead, the same cheap way a repeated gap
/// is (`repeated-failure`).
mod self_referential_macros {
    use satex::config::{Config, Engine};
    use satex::facts::Severity;
    use satex::machine::{Analysis, Machine};
    use satex::plugin::Kernel;

    fn cfg() -> Config {
        let mut cfg = Config { engine: Some(Engine::PdfTeX), kernel: Some(Kernel::None), ..crate::support::config() };
        cfg.limits.steps = 200_000;
        cfg.limits.stall_tokens = 50_000;
        cfg.limits.seconds = 0;
        cfg
    }

    fn messages(analysis: &Analysis, code: &str) -> Vec<String> {
        analysis
            .facts
            .diagnostics
            .iter()
            .filter(|d| d.code == code && d.severity == Severity::Imprecision)
            .map(|d| d.message.clone())
            .collect()
    }

    /// `\q`'s call inside `\edef` is reached by `\expandafter`, exactly as
    /// `\exp_after:wN` reaches a quark in the real bug: one
    /// `recursion-widened` gap, raised after `recursion_depth + 1` rounds,
    /// never after the full 16384.
    #[test]
    fn a_self_referential_macro_through_expandafter_widens_cheaply() {
        let analysis = Machine::analyze(
            "\\catcode`\\{=1 \\catcode`\\}=2 \\def\\q{\\q}\\edef\\b{\\expandafter\\relax\\q}",
            None,
            &cfg(),
        );
        let widened = messages(&analysis, "recursion-widened");
        assert_eq!(widened.len(), 1, "{widened:?}");
        let rounds = format!("\\q: expanded {} times", cfg().limits.recursion_depth + 1);
        assert!(widened[0].contains(&rounds), "{widened:?}");
        assert!(!widened[0].contains("16385"), "{widened:?}");
    }

    /// The bound follows `recursion_depth`, not a hardcoded constant.
    #[test]
    fn the_bound_follows_recursion_depth() {
        let mut c = cfg();
        c.limits.recursion_depth = 6;
        let analysis = Machine::analyze(
            "\\catcode`\\{=1 \\catcode`\\}=2 \\def\\q{\\q}\\edef\\b{\\expandafter\\relax\\q}",
            None,
            &c,
        );
        let widened = messages(&analysis, "recursion-widened");
        assert_eq!(widened.len(), 1, "{widened:?}");
        assert!(widened[0].contains("\\q: expanded 7 times"), "{widened:?}");
    }
}

/// satex's own marker for text it could not read is itself a fixed point:
/// `Meaning::Unknown` on that one reserved symbol expands back to the same
/// symbol (`Machine::unknown_token`). `\exp_after:wN`, expl3's
/// `\expandafter`, is exactly how the real bug (`\__prop_if_flat:NTF`
/// in expl3-code.tex) reaches it: the T-branch of `\prop_if_exist:NT` is
/// the bare token `\exp_after:wN`, which then expands the prop variable in
/// place. `\pdfcreationdate` is a real, side-effect-free source of the
/// marker (see `tests/it/counterexamples.rs`'s use of `\pdfuniformdeviate`
/// for the same kind of stand-in).
///
/// This terminates today, with no code change: `expand_once` (what
/// `\expandafter` drives) already refuses to expand a symbol
/// `is_unknown_marker` names, falling back to leaving it as it stands, so
/// the marker cannot loop through itself. What follows is the one gap the
/// run does raise once the marker reaches a number scan: not the marker's
/// own expansion, but the ordinary `\relax` `\expandafter` held back
/// breaking the scan before it can read on.
#[test]
fn unknown_marker_through_expandafter_terminates_with_one_gap() {
    let mut cfg = Config { engine: Some(Engine::PdfTeX), kernel: Some(Kernel::None), ..crate::support::config() };
    cfg.limits.steps = 200_000;
    cfg.limits.stall_tokens = 50_000;
    cfg.limits.seconds = 0;
    // The creation date stands for text satex cannot know only when the
    // clock is not assumed.
    cfg.clock = satex::config::Clock::Unknown;
    let analysis = Machine::analyze(
        "\\catcode`\\{=1 \\catcode`\\}=2 \
         \\edef\\b{\\expandafter\\relax\\pdfcreationdate}\\ifnum\\b=0 \\fi",
        None,
        &cfg,
    );
    let gaps: Vec<_> = analysis.facts.diagnostics.iter().map(|d| d.code).filter(|&c| c != "no-format").collect();
    assert_eq!(gaps, vec!["missing-number"], "{gaps:?}");
}

#[test]
fn pathological_input_terminates_within_its_budget() {
    // Generous for a debug build: a run is ~200k tokens.
    let bound = Duration::from_secs(60);
    let mut slow = Vec::new();
    for &(what, source) in INPUTS {
        let (done, ended) = mpsc::channel();
        let started = Instant::now();
        let source = source.to_string();
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                let analysis = Machine::analyze(&source, None, &cfg(200_000));
                let _ = done.send(analysis.steps);
            })
            .unwrap();
        match ended.recv_timeout(bound) {
            Ok(steps) => assert!(steps <= 200_000 + 1, "{what}: {steps} steps past the budget"),
            Err(_) => slow.push(what),
        }
        eprintln!("{what}: {:?}", started.elapsed());
    }
    assert!(slow.is_empty(), "no end within {bound:?}: {slow:?}");
}

/// Loops a real run ends, whose bound the analysis does not know: each
/// counts up to or grows by `\n`, a number read from `\pdfelapsedtime`.
/// Widening must reach a fixpoint long before any budget, so none of them
/// may end by a limit. Each defines `\a`, which the run then calls.
const CONVERGING: &[(&str, &str)] = &[
    ("counting", r"\def\a{\ifnum\count1<\n \advance\count1 1 \expandafter\a\fi}"),
    ("growing a macro", r"\def\a{\ifnum\count1<\n \advance\count1 1 \edef\c{\c x}\expandafter\a\fi}"),
    ("doubling a macro", r"\def\a{\ifnum\count1<\n \advance\count1 1 \edef\c{\c\c}\expandafter\a\fi}"),
    (
        "growing a register",
        r"\def\a{\ifnum\count1<\n \advance\count1 1 \toks0\expandafter{\the\toks0 x}\expandafter\a\fi}",
    ),
    ("recursing", r"\def\a{\ifnum\count1<\n \advance\count1 1 \a\fi}"),
    ("nesting groups", r"\def\a{\ifnum\count1<\n \advance\count1 1 \begingroup\expandafter\a\fi}"),
    (
        "changing meanings",
        r"\def\a{\ifnum\count1<\n \advance\count1 1 \ifodd\count1 \let\m\relax\else\def\m{y\m}\fi\expandafter\a\fi}",
    ),
    ("dimensions", r"\def\a{\ifdim\dimen1<\n pt \advance\dimen1 1pt \expandafter\a\fi}"),
    ("boxes", r"\def\a{\ifnum\count1<\n \advance\count1 1 \setbox0\hbox{\box0 x}\expandafter\a\fi}"),
    (
        "csnames",
        r"\def\a{\ifnum\count1<\n \advance\count1 1 \expandafter\def\csname c\the\count1\endcsname{}\expandafter\a\fi}",
    ),
    ("mutual recursion", r"\def\b{\a\relax}\def\a{\ifnum\count1<\n \advance\count1 1 \b\fi}"),
    (
        "growing argument",
        r"\def\g#1{}\def\a#1{\ifnum\count1<\n \advance\count1 1 \expandafter\a\else\expandafter\g\fi{#1x}}",
    ),
    ("afterassignment", r"\def\a{\ifnum\count1<\n \afterassignment\a\advance\count1 1 \fi}"),
    (
        "nested loops",
        r"\def\i{\ifnum\count2<\n \advance\count2 1 \expandafter\i\fi}\def\a{\ifnum\count1<\n \advance\count1 1 \count2=0 \i\expandafter\a\fi}",
    ),
];

/// `n` conditionals in a row on the unknown bound: a path per outcome
/// would be 2^n, so the paths must meet and join.
fn undecided_tests(n: usize) -> String {
    (0..n).map(|_| r"\ifnum\count1<\n \advance\count2 1 \fi\advance\count1 1 ").collect()
}

/// `n` tests that each give `\m` its own meaning or `\relax`, appended to
/// `\c` every time: the meaning and value sets must stay bounded.
fn diverging_meanings(n: usize) -> String {
    (0..n).map(|k| format!(r"\ifnum\n>{k} \def\m{{a{k}}}\else\let\m\relax\fi\edef\c{{\c\m}}")).collect()
}

/// Loops that still end by a limit, where they should widen: a recursion
/// that keeps a `\fi` per level is cut at `limits.recursion_depth`, nested
/// groups at `limits.stall_tokens`, and an argument that grows every pass
/// makes a new loop head each time.
const OPEN: &[&str] = &["recursing", "nesting groups", "mutual recursion", "afterassignment", "growing argument"];

#[test]
fn undecided_loops_reach_a_fixpoint_before_any_limit() {
    converge(|what| !OPEN.contains(&what));
}

#[test]
fn deep_undecided_recursion_reaches_a_fixpoint_before_any_limit() {
    converge(|what| OPEN.contains(&what));
}

/// Runs every input `pick` accepts: each must end by a fixpoint, under half
/// the step budget and with no limit reached, and a loop must have run.
fn converge(pick: impl Fn(&str) -> bool) {
    let bound = Duration::from_secs(60);
    let budget = 200_000;
    let prefix = r"\catcode`\{=1 \catcode`\}=2 \catcode`\#=6 \countdef\n=9 \n=\pdfelapsedtime \def\c{x}";
    let loops = CONVERGING.iter().map(|&(what, defs)| (what, format!(r"{prefix}{defs}\a{{}}"), true));
    let generated = [
        ("forty undecided tests", format!("{prefix}{}", undecided_tests(40)), false),
        ("thirty diverging meanings", format!("{prefix}{}", diverging_meanings(30)), false),
    ];
    let mut failed = Vec::new();
    for (what, source, is_loop) in loops.chain(generated).filter(|(what, ..)| pick(what)) {
        let (done, ended) = mpsc::channel();
        let started = Instant::now();
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                let analysis = Machine::analyze(&source, None, &cfg(budget));
                let limits: Vec<String> = analysis
                    .facts
                    .diagnostics
                    .iter()
                    .filter(|d| ["budget-exhausted", "limit-reached", "repeated-failure"].contains(&d.code))
                    .map(|d| format!("{} {}", d.code, d.message))
                    .collect();
                // A recursion widened at its depth ran as a loop too.
                let looped = analysis
                    .facts
                    .diagnostics
                    .iter()
                    .any(|d| ["undecided-loop", "recursion-widened"].contains(&d.code));
                let _ = done.send((analysis.steps, limits, looped));
            })
            .unwrap();
        match ended.recv_timeout(bound) {
            Ok((steps, limits, _)) if steps >= budget / 2 || !limits.is_empty() => {
                failed.push(format!("{what}: {steps} steps, {limits:?}"))
            }
            // A loop that never ran proves nothing.
            Ok((_, _, false)) if is_loop => failed.push(format!("{what}: no undecided loop, so the input is wrong")),
            Ok(_) => {}
            Err(_) => failed.push(format!("{what}: no end within {bound:?}")),
        }
        eprintln!("{what}: {:?}", started.elapsed());
    }
    assert!(failed.is_empty(), "ended by a limit, not a fixpoint:\n{}", failed.join("\n"));
}
