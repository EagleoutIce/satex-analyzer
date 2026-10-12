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
    cfg.limits.line_steps = steps / 4;
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

    /// A `\parshape` whose line count satex cannot pin down (the engine's
    /// clock gives it), always the one in `\x`'s body, so every occurrence
    /// carries the same code and the same span (tex.web § 1248: the count
    /// says how much follows, so what is read after it is unknown). The
    /// engine's own errors (`extra-fi`) are recovered as TeX recovers them
    /// and never cut.
    fn same_gap_n_times() -> String {
        format!("\\catcode`\\{{=1 \\catcode`\\}}=2 \\def\\x{{\\parshape\\pdfelapsedtime }}{}", "\\x".repeat(USES))
    }

    fn cfg(bound: u32) -> Config {
        let mut cfg = Config { engine: Some(Engine::PdfTeX), kernel: Some(Kernel::None), ..crate::support::config() };
        cfg.limits.steps = 200_000;
        cfg.limits.line_steps = 50_000;
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
        assert_eq!(rows(&analysis, "unknown-parshape"), vec![8]);
        assert_eq!(rows(&analysis, "repeated-failure"), vec![1]);
    }

    /// The bound is the limit's, not a constant: three means three.
    #[test]
    fn the_bound_follows_the_limit() {
        let analysis = Machine::analyze(&same_gap_n_times(), None, &cfg(3));
        assert_eq!(rows(&analysis, "unknown-parshape"), vec![3]);
        assert_eq!(rows(&analysis, "repeated-failure"), vec![1]);
    }

    /// Zero lifts the bound, which is what the run did before: every
    /// occurrence is counted and nothing cuts the path.
    #[test]
    fn zero_lifts_the_bound() {
        let analysis = Machine::analyze(&same_gap_n_times(), None, &cfg(0));
        assert_eq!(rows(&analysis, "unknown-parshape"), vec![USES as u32]);
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
        cfg.limits.line_steps = 50_000;
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
    cfg.limits.line_steps = 50_000;
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
/// groups at `limits.line_steps`, and an argument that grows every pass
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

/// A recursion that consumes digits of unknown count one by one, with an
/// accumulator that grows each pass, reaches its fixpoint well inside a
/// small step budget, with no limit reached.
#[test]
fn digit_consuming_recursion_with_an_accumulator_converges() {
    let source = r"\catcode`\{=1 \catcode`\}=2 \catcode`\#=6 \count1=\pdfelapsedtime \edef\n{\the\count1}
        \def\stop{}\def\r{}\def\a#1{\ifx\stop#1\else\advance\count2 1 \edef\r{\r#1}\expandafter\a\fi}
        \expandafter\a\n\stop\def\after{}";
    let analysis = Machine::analyze(source, None, &cfg(20_000));
    let limits: Vec<&str> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| ["budget-exhausted", "limit-reached", "repeated-failure"].contains(&d.code))
        .map(|d| d.message.as_str())
        .collect();
    assert!(limits.is_empty(), "{limits:?}");
    assert!(analysis.steps < 10_000, "{} steps", analysis.steps);
}

/// A call re-entered with the same arguments and input, one conditional
/// deeper each time (its `\fi` taken as its argument), never ends: it is
/// cut after a few rounds, not at `limits.site_expansions`.
#[test]
fn reentry_nested_deeper_each_round_is_cut_early() {
    let source = r"\catcode`\{=1 \catcode`\}=2 \catcode`\#=6 \def\x{0}\let\expandafter\x
        \def\a#1{\ifx\stop#1\relax\else\expandafter\a\fi}\a\fi\stop";
    let analysis = Machine::analyze(source, None, &cfg(200_000));
    assert!(analysis.steps < 2_000, "{} steps", analysis.steps);
    assert!(analysis.facts.diagnostics.iter().any(|d| d.message.contains("nested deeper each time")));
}

/// Paths joined at unequal input depth: code the deeper one still has to
/// run is not one data token, which an x-expansion would store unexpanded
/// and which later breaks the braces ("Argument has an extra }").
#[test]
fn a_join_never_folds_code_into_data() {
    if !crate::support::installed() {
        return;
    }
    let source = r"\documentclass{article}
\ExplSyntaxOn
\str_set:Ne \l_tmpb_str { a \int_to_Hex:n{\int_rand:n{255}} b }
\exp_args:NNo \str_set_convert:Nnnn \l_tmpa_str \l_tmpb_str {utf8}{}
\ExplSyntaxOff
\begin{document}x\end{document}";
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/termination/join-code");
    let cfg = Config {
        cache: satex::config::Cache { dir: Some(crate::support::scratch_dir(dir)), ..Default::default() },
        load: satex::config::Load { packages: true, classes: true, ..Default::default() },
        ..crate::support::config()
    };
    crate::support::seed_kernel(&cfg);
    let analysis = Machine::analyze(source, None, &cfg);
    let bad: Vec<String> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| {
            d.message.contains("extra }")
                || d.code == "budget-exhausted"
                || (d.code == "limit-reached"
                    && ["limits.steps", "limits.seconds"].iter().any(|l| d.message.contains(l)))
        })
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
}

/// A summarized loop never ends the analysis: the input after it, here
/// a second file, is still read, and nothing ends early.
#[test]
fn a_summarized_loop_leaves_the_rest_of_the_document_read() {
    if !crate::support::installed() {
        return;
    }
    let dir = crate::support::scratch_dir(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/termination/rest-read"),
    );
    std::fs::write(dir.join("rest.tex"), r"\def\reached{yes}").unwrap();
    let source = format!(
        r"\documentclass{{article}}
\countdef\n=9 \n=\pdfelapsedtime
\def\a{{\ifnum\count1<\n \advance\count1 1 \begingroup\expandafter\a\fi}}
\begin{{document}}
\a{{}}
\input{{{}/rest.tex}}
\ifdefined\reached \else\errmessage{{not reached}}\fi
\end{{document}}",
        dir.display()
    );
    let cfg = Config {
        cache: satex::config::Cache { dir: Some(dir.join("cache")), ..Default::default() },
        load: satex::config::Load { packages: true, classes: true, ..Default::default() },
        ..crate::support::config()
    };
    crate::support::seed_kernel(&cfg);
    let analysis = Machine::analyze(&source, None, &cfg);
    let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(!codes.contains(&"analysis-ended-early"), "{codes:?}");
    assert!(codes.contains(&"recursion-widened"), "the loop is summarized: {codes:?}");
    let reached = analysis.facts.diagnostics.iter().any(|d| d.message.contains("not reached"));
    assert!(!reached, "the second file was not read");
    assert!(analysis.files.iter().any(|f| f.path.ends_with("rest.tex")), "rest.tex is read");
}

/// A summarized loop in the body consumes no source: the text and the
/// `\section` after it are still read.
#[test]
fn a_summarized_loop_consumes_no_source_text() {
    if !crate::support::installed() {
        return;
    }
    let source = r"\documentclass{article}
\countdef\n=9 \n=\pdfelapsedtime
\def\a{\ifnum\count1<\n \advance\count1 1 \begingroup\expandafter\a\fi}
\begin{document}
\a{}
Plain text after the loop.
\section{After}\label{sec:after}
\end{document}";
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/termination/no-gobble");
    let cfg = Config {
        cache: satex::config::Cache { dir: Some(crate::support::scratch_dir(dir)), ..Default::default() },
        load: satex::config::Load { packages: true, classes: true, ..Default::default() },
        ..crate::support::config()
    };
    crate::support::seed_kernel(&cfg);
    let analysis = Machine::analyze(source, None, &cfg);
    let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&"recursion-widened"), "the loop is summarized: {codes:?}");
    assert!(!codes.contains(&"analysis-ended-early"), "{codes:?}");
    let label = analysis.interner.lookup("r@sec:after").or_else(|| analysis.interner.lookup("sec:after"));
    let seen = format!("{:?}", analysis.facts.diagnostics).contains("sec:after")
        || label.is_some()
        || analysis.document_depth.is_some();
    assert!(seen, "the \\section after the loop is read: {codes:?}");
}

/// The termination guard (`limits.line_steps`). Each loop ends within the
/// bound with the next line read, once as the precision cuts leave it and
/// once with them all off: then the guard alone cuts it, a second loop on
/// the line skips its rest, and the next line is read. The run takes at
/// most 3 times the budget per line read.
mod line_guard {
    use satex::machine::{Analysis, Machine};

    const LINE: u64 = 100_000;
    const HEAD: &str =
        "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\count1=\\pdfelapsedtime \\edef\\u{\\the\\count1}";

    fn run(defs: &str, call: &str, alone: bool) -> Analysis {
        let mut cfg = super::cfg(1_000_000_000);
        cfg.limits.line_steps = LINE;
        cfg.limits.seconds = 60;
        if alone {
            let l = &mut cfg.limits;
            (l.widen_after, l.cycle_after, l.recursion_depth, l.expansion_depth) =
                (u16::MAX, u16::MAX, u16::MAX, u16::MAX);
            (l.call_summary_after, l.site_expansions, l.repeat_failures) = (0, u32::MAX, u32::MAX);
            (l.expansion_stack, l.held_tokens) = (usize::MAX, usize::MAX);
        }
        let source = format!("{HEAD}\n{defs}\n{call}{call}\\def\\Lost{{}}\n\\def\\Next{{}}\n");
        let lines = source.lines().count() as u64;
        let a = Machine::analyze(&source, None, &cfg);
        assert!(a.steps <= lines * 3 * LINE + 10_000, "{} steps over the bound", a.steps);
        a
    }

    fn check(defs: &str, call: &str) {
        for alone in [false, true] {
            let a = run(defs, call, alone);
            let codes: Vec<&str> = a.facts.diagnostics.iter().map(|d| d.code).collect();
            let defined = |name: &str| a.facts.defs.iter().any(|d| a.interner.name(d.name) == name);
            assert!(defined("Next"), "the next line is read ({alone}): {codes:?}");
            assert!(!a.exhausted, "{codes:?}");
            if alone {
                assert!(codes.contains(&"line-budget-cut") && codes.contains(&"stalled-line-skipped"), "{codes:?}");
                assert!(!defined("Lost"), "{codes:?}");
            }
        }
    }

    #[test]
    fn endless_recursion_on_an_unknown_test() {
        check("\\def\\x{\\advance\\count2 1 \\ifx\\u\\relax\\expandafter\\x\\else\\expandafter\\x\\fi}", "\\x");
    }

    #[test]
    fn char_loop_over_unknown_text_of_unknown_length() {
        check("\\def\\c#1{\\expandafter\\c\\u#1}", "\\expandafter\\c\\u");
    }

    #[test]
    fn recursion_inside_setbox() {
        check("\\def\\r{\\advance\\count2 1 \\r}", "\\setbox0\\hbox{\\r}");
    }

    #[test]
    fn endless_loop_in_an_edef() {
        check("\\def\\l{\\ifnum\\count1>0 x\\expandafter\\l\\else\\expandafter\\l\\fi}", "\\edef\\e{\\l}");
    }
}
