//! Never hang: pathological input under small limits ends within a bound,
//! whatever loop it drives — expansion, `\edef`, `\csname`, `\expanded`,
//! `\message`, number and keyword scans, delimited arguments, skipped
//! conditionals, undecided loops, groups and `\scantokens`.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use satex::analysis::machine::Machine;
use satex::plugins::Kernel;
use satex::project::config::{Config, Engine};

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

/// A loop on known values runs as TeX runs it, however long one source line
/// holds it: a `pgfplotstable` row predicate re-walks its rows, quadratic in
/// the rows, and was cut as "spinning", which poisoned the rest of the run.
/// A loop that never ends still ends at the line budget.
#[test]
fn a_long_assigning_loop_on_known_values_is_not_cut() {
    let prelude = crate::support::PRELUDE;
    let mut config = cfg(50_000_000);
    config.limits.site_expansions = 64;
    let rounds = 32 * config.limits.site_expansions;
    let source = format!(
        r"{prelude}\def\walk{{\advance\count1 by 1 \ifnum\count1<{rounds} \expandafter\walk\fi}}\walk
          \ifnum\count1={rounds} \def\exact{{}}\else \def\cut{{}}\fi"
    );
    let analysis = Machine::analyze(&source, None, &config);
    assert!(crate::support::defined(&analysis, "exact"), "the loop was cut");
    assert!(!crate::support::maybe_defined(&analysis, "cut"), "the loop was cut");

    let endless =
        Machine::analyze(&format!(r"{prelude}\def\spin{{\advance\count1 by 1 \spin}}\spin"), None, &cfg(1_000_000));
    assert!(!endless.facts.diagnostics.is_empty(), "the endless loop was not reported");
}

/// One gap at one place, however often the path reaches it, is counted to
/// `limits.repeat_failures` and no further: the expansion is then given up
/// and widened, because a path that fails at a site fails there again on
/// every round.
mod repeated_failures {
    use satex::analysis::facts::Severity;
    use satex::analysis::machine::{Analysis, Machine};
    use satex::plugins::Kernel;
    use satex::project::config::{Config, Engine};

    /// How often the input below reaches the one failing site.
    const USES: usize = 40;

    /// A `\parshape` whose line count satex cannot pin down (the engine's
    /// clock gives it), always the one in `\x`'s body, so every occurrence
    /// carries the same code and the same span (tex.web § 1248: the count
    /// says how much follows, so what is read after it is unknown). The
    /// engine's own errors (`extra-fi`) are recovered as TeX recovers them
    /// and never cut. A `\relax` after each use: the next `\x`'s
    /// `\parshape` could be read as a line's dimension of the one before.
    fn same_gap_n_times() -> String {
        format!(
            "\\catcode`\\{{=1 \\catcode`\\}}=2 \\def\\x{{\\parshape\\pdfelapsedtime }}{}",
            "\\x\\relax ".repeat(USES)
        )
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
    use satex::analysis::facts::Severity;
    use satex::analysis::machine::{Analysis, Machine};
    use satex::plugins::Kernel;
    use satex::project::config::{Config, Engine};

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
    /// `\exp_after:wN` reaches a quark in the real bug. Each call meets the
    /// state the last one did (tex.web § 390 drops the used-up body before
    /// feeding the next, so TeX loops forever in constant space): the cycle
    /// cut ends it after `cycle_after` repeats, with one `endless-loop`
    /// warning and one `recursion-widened` gap, never after the full 16384.
    #[test]
    fn a_self_referential_macro_through_expandafter_widens_cheaply() {
        let analysis = Machine::analyze(
            "\\catcode`\\{=1 \\catcode`\\}=2 \\def\\q{\\q}\\edef\\b{\\expandafter\\relax\\q}",
            None,
            &cfg(),
        );
        let widened = messages(&analysis, "recursion-widened");
        assert_eq!(widened.len(), 1, "{widened:?}");
        assert!(widened[0].contains("\\q: it meets the state it was in before"), "{widened:?}");
        let endless = analysis.facts.diagnostics.iter().filter(|d| d.code == "endless-loop").count();
        assert_eq!(endless, 1);
    }

    /// A loop that assigns every round but leaves each name as it held it
    /// a round before is in the state it was in (tex.web § 366), as a
    /// quark is. pgfmath's `\pgfmath@ensurenumberlength`, handed its own
    /// sentinel on a path over unknown text, loops so. The cycle cut ends
    /// it after `cycle_after` repeats, not after the 16 × 16384 rounds a
    /// loop that writes is otherwise given.
    #[test]
    fn a_loop_writing_back_the_same_state_is_a_cycle() {
        let analysis = Machine::analyze(
            "\\catcode`\\{=1 \\catcode`\\}=2 \\def\\a{\\count1=3 \\advance\\count1 -1 \\def\\t{x}\\a}\\a",
            None,
            &cfg(),
        );
        let widened = messages(&analysis, "recursion-widened");
        assert!(widened.iter().any(|m| m.contains("it meets the state it was in before")), "{widened:?}");
        assert!(analysis.steps < 2_000, "{} steps", analysis.steps);
    }

    /// LaTeX's `\@whilesw` with a body that never clears its switch, as
    /// `\clearnotecolumns` runs inside an output routine. The body's `\b`
    /// meets its state of a round before. Cutting `\b` would leave the loop
    /// running, so the cut goes to `\iw`, the expansion that carries the
    /// loop, and the run reads on to `\done`. (A body that closes a group each
    /// round is not met as a repeat yet: see the plan.)
    #[test]
    fn a_loop_whose_body_repeats_is_cut_where_it_loops() {
        let analysis = Machine::analyze(
            "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\let\\ifs\\iftrue\n\
             \\long\\def\\w#1\\fi#2{#1#2\\iw{#1#2}\\fi\\fi}\n\
             \\long\\def\\iw#1\\fi{#1\\expandafter\\iw\\else\\g\\fi{#1}\\fi}\n\
             \\long\\def\\g#1#2{}\\def\\b{\\count1=1 }\n\
             \\w\\ifs\\fi{\\b}\\def\\done{}",
            None,
            &cfg(),
        );
        let widened = messages(&analysis, "recursion-widened");
        assert!(widened.iter().any(|m| m.contains("\\iw: it")), "{widened:?}");
        assert!(crate::support::defined(&analysis, "done"));
        assert!(analysis.steps < 5_000, "{} steps", analysis.steps);
    }

    /// With the cycle cut off (`cycle_after: 0`), the quark rule bounds the
    /// same fixed point, and it follows `recursion_depth`, not a hardcoded
    /// constant.
    #[test]
    fn the_bound_follows_recursion_depth() {
        let mut c = cfg();
        c.limits.cycle_after = 0;
        c.limits.recursion_depth = 6;
        let analysis = Machine::analyze(
            "\\catcode`\\{=1 \\catcode`\\}=2 \\def\\q{\\q}\\edef\\b{\\expandafter\\relax\\q}",
            None,
            &c,
        );
        let widened = messages(&analysis, "recursion-widened");
        assert_eq!(widened.len(), 1, "{widened:?}");
        assert!(widened[0].contains("\\q: expanded again and again into itself"), "{widened:?}");
    }
}

/// A loop that reads its text four tokens at a time, as pgf's
/// `\pgfprocesssplitsubpath` walks a soft path up to `\pgf@stop`, over text
/// a widened loop left unknown (`\x` is "some letters"). TeX's text is
/// finite, so the walk meets `\stop` after a few rounds. satex reads the
/// unknown text's first character and puts back a rest of the same kind,
/// so every round reads three copies of that rest and never the `\stop`
/// behind it, and no conditional is undecided to meet a loop head. The
/// walk ran until the line budget (500M steps by default): hang.tex's
/// 30-node `\foreach` spent 420M tokens there on an infeasible
/// `\ifpgf@worry` arm. A rest read from unbounded unknown text must make
/// progress, so the walk ends long before the line budget.
#[test]
fn a_walk_over_unknown_text_reaches_its_end_marker() {
    let analysis = Machine::analyze(
        "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \n\
         \\def\\x{}\\def\\l{\\edef\\x{\\x a}\\ifnum\\pdfuniformdeviate 2=0 \\expandafter\\l\\fi}\\l\n\
         \\def\\stop{\\stopx}\\def\\stopx{}\\def\\pre{}\n\
         \\def\\split#1#2#3#4{\\ifx#4\\stop\\let\\next\\relax\\else\\edef\\pre{\\pre#1}\\let\\next\\split\\fi\\next#4}\n\
         \\expandafter\\split\\x\\stop\\stop\\stop\\stop\\def\\done{}",
        None,
        &cfg(400_000),
    );
    let cut: Vec<_> = analysis.facts.diagnostics.iter().filter(|d| d.code == "line-budget-cut").collect();
    assert!(cut.is_empty(), "the walk ran into the line budget: {} steps", analysis.steps);
    assert!(crate::support::defined(&analysis, "done"));
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
    cfg.clock = satex::project::config::Clock::Unknown;
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

/// hyperref's `\HyPsd@CheckCatcodes` shape over one of two texts: a scan
/// that gives up types what it read, and typed text that may be empty
/// keeps the argument perhaps empty. Typed as non-empty, the end test
/// was decided "not yet" on every pass and the loop never ended.
#[test]
fn char_loop_over_text_that_may_be_empty_ends() {
    let source = r"\catcode`\{=1 \catcode`\}=2 \catcode`\#=6
        \count1=\pdfuniformdeviate 5 \count2=\pdfuniformdeviate 2 \edef\y{\the\count1}
        \ifnum\count2=0 \edef\x{\y|}\else\edef\x{\y\noexpand\/|}\fi
        \long\def\R#1\fi#2\E{\fi\fi#1}\def\S{}
        \def\C#1#2\E{\ifcat\relax\noexpand#1\relax\else\expandafter\def\expandafter\S\expandafter{\S#1}\fi
          \iftrue\ifx\scrollmode#2\scrollmode\else\R{\C#2\E}\fi\fi\E}
        \expandafter\C\x\E\def\after{}";
    let analysis = Machine::analyze(source, None, &cfg(400_000));
    let limits: Vec<&str> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| ["budget-exhausted", "limit-reached", "repeated-failure"].contains(&d.code))
        .map(|d| d.message.as_str())
        .collect();
    assert!(limits.is_empty(), "{limits:?}");
    assert!(analysis.steps < 100_000, "{} steps", analysis.steps);
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

/// A call re-entered with the same arguments, the input below it as it
/// was and nothing assigned, only with the rest of the last round's text
/// stacked on top, is the same nesting one level deeper: it is cut after a
/// few rounds, not at `limits.expansion_depth`. The second input is what
/// pgf's `\pgfmath@parse@numexpr` does in an error message once a join
/// made `\pgfmath@token` one of its own parser macros.
#[test]
fn reentry_deeper_on_the_input_stack_is_cut_early() {
    for source in [
        r"\catcode`\{=1 \catcode`\}=2 \def\a{\a x}\edef\b{\a}",
        r"\catcode`\{=1 \catcode`\}=2 \def\n{}\def\p{\expandafter\expandafter\expandafter\relax\expandafter\t\n x}
          \let\t\p\message{\p}",
    ] {
        let analysis = Machine::analyze(source, None, &cfg(200_000));
        let codes: Vec<_> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
        assert!(!codes.contains(&"limit-reached"), "{source}: {codes:?}");
        assert!(
            analysis.facts.diagnostics.iter().any(|d| d.message.contains("nested deeper each time")),
            "{source}: {codes:?}"
        );
        assert!(analysis.steps < 2_000, "{source}: {} steps", analysis.steps);
    }
}

/// A cut inside each round widens nothing new, so it changes no state:
/// the recursion around it is still the same one nested deeper and is cut
/// early too. This is LaTeX's `\par` death cycle in a list under an unknown
/// `\if@newlist` (`\par` → `\@noitemerr` → `\@latex@error`, each cut).
#[test]
fn a_cut_inside_each_round_hides_no_outer_nesting() {
    let source = r"\catcode`\{=1 \catcode`\}=2 \def\b{\b x}\def\a{\b\a y}\edef\c{\a}";
    let analysis = Machine::analyze(source, None, &cfg(200_000));
    let codes: Vec<_> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(!codes.contains(&"limit-reached"), "{codes:?}");
    let nested: Vec<_> = (analysis.facts.diagnostics.iter())
        .filter(|d| d.message.contains("nested deeper each time"))
        .map(|d| d.message.clone())
        .collect();
    assert!(nested.iter().any(|m| m.starts_with("\\a:")), "{nested:?}");
}

/// A recursion that nests deeper while assigning is getting somewhere:
/// it runs to its end, uncut.
#[test]
fn reentry_deeper_while_assigning_runs_to_its_end() {
    let source = r"\catcode`\{=1 \catcode`\}=2 \count1=0
        \def\a{\advance\count1 1 \ifnum\count1<40 \a x\fi}\setbox0\hbox{\a}";
    let analysis = Machine::analyze(source, None, &cfg(200_000));
    let codes: Vec<_> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(!codes.contains(&"recursion-widened"), "{codes:?}");
    assert!(!codes.contains(&"limit-reached"), "{codes:?}");
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
        cache: satex::project::config::Cache {
            dir: Some(crate::support::scratch_dir(dir)),
            document: false,
            ..Default::default()
        },
        load: satex::project::config::Load { packages: true, classes: true, ..Default::default() },
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

/// A cut ends a recursion that reads its terminator from the list below
/// it, here the `{\stop…}` group and the `\stoppoint` that `\stop` is
/// delimited by. What the cut left after its `\unknown` may have been
/// taken as arguments, so a `}` an argument scan meets there is not TeX's
/// "Argument has an extra }": the argument is unknown, the braces stay
/// balanced and the definition ends at its own `}`.
#[test]
fn text_a_cut_left_raises_no_extra_brace() {
    let prelude = r"\catcode`\{=1 \catcode`\}=2 \catcode`\#=6 \catcode`\ =10
        \def\empty{}\def\start#1{#1\start\empty}";
    // A delimited argument and an undelimited one.
    for tail in [
        r"\def\stop#1\stoppoint{}\edef\result{\start abcdef{\stop x}\stoppoint}",
        r"\def\stop#1{}\edef\result{\start abcdef{\stop}}",
    ] {
        let source = format!("{prelude}{tail}\\def\\after{{}}");
        let analysis = Machine::analyze(&source, None, &cfg(200_000));
        let ds = &analysis.facts.diagnostics;
        assert!(ds.iter().any(|d| d.code == "recursion-widened"), "no cut: {source}");
        assert!(!ds.iter().any(|d| d.message.contains("extra }") || d.code == "runaway-argument"), "{source}: {ds:?}");
        for name in ["result", "after"] {
            assert!(analysis.facts.defs.iter().any(|d| analysis.interner.name(d.name) == name), "{name}: {source}");
        }
    }
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
        crate::support::tex_path(&dir)
    );
    let cfg = Config {
        cache: satex::project::config::Cache { dir: Some(dir.join("cache")), document: false, ..Default::default() },
        load: satex::project::config::Load { packages: true, classes: true, ..Default::default() },
        ..crate::support::config()
    };
    crate::support::seed_kernel(&cfg);
    let analysis = Machine::analyze(&source, None, &cfg);
    let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(!codes.contains(&"analysis-ended-early"), "{codes:?}");
    // Its groups widen to any number: it may meet a fixpoint, not a summary.
    let ends = codes.iter().any(|c| matches!(*c, "recursion-widened" | "undecided-loop"));
    assert!(ends, "the loop is joined or summarized: {codes:?}");
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
        cache: satex::project::config::Cache {
            dir: Some(crate::support::scratch_dir(dir)),
            document: false,
            ..Default::default()
        },
        load: satex::project::config::Load { packages: true, classes: true, ..Default::default() },
        ..crate::support::config()
    };
    crate::support::seed_kernel(&cfg);
    let analysis = Machine::analyze(source, None, &cfg);
    let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    // Its groups widen to any number: it may meet a fixpoint, not a summary.
    let ends = codes.iter().any(|c| matches!(*c, "recursion-widened" | "undecided-loop"));
    assert!(ends, "the loop is joined or summarized: {codes:?}");
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
    use satex::analysis::machine::{Analysis, Machine};

    const LINE: u64 = 100_000;
    const HEAD: &str =
        "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\count1=\\pdfelapsedtime \\edef\\u{\\the\\count1}";

    fn run(defs: &str, call: &str, alone: bool) -> Analysis {
        let mut cfg = super::cfg(1_000_000_000);
        cfg.limits.line_steps = LINE;
        cfg.limits.seconds = 600; // a backstop: the step bound below is what the test checks
        if alone {
            let l = &mut cfg.limits;
            (l.widen_after, l.cycle_after, l.recursion_depth, l.expansion_depth) =
                (u16::MAX, u16::MAX, u16::MAX, u16::MAX);
            (l.call_summary_after, l.site_expansions, l.repeat_failures) = (0, u32::MAX, u32::MAX);
            (l.expansion_stack, l.held_tokens, l.site_forks) = (usize::MAX, usize::MAX, 0);
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

/// Documents whose values are unknown still converge: each reaches its end
/// with few paths, by the abstraction (exact `\relax` for a `\csname` that
/// can spell no defined name) or by the per-site fork budget, never by
/// running out of steps.
mod convergence {
    use satex::analysis::machine::{Analysis, Machine};
    use satex::project::config::Config;

    fn latex(name: &str, source: &str, tune: impl Fn(&mut Config)) -> Option<Analysis> {
        if !crate::support::installed() {
            return None;
        }
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/convergence").join(name);
        let dir = crate::support::scratch_dir(dir);
        let main = dir.join("main.tex");
        std::fs::write(&main, source).expect("the document is written");
        let mut cfg = crate::support::discover(&dir);
        cfg.cache.dir = Some(dir.join("cache"));
        tune(&mut cfg);
        crate::support::seed_kernel(&cfg);
        Some(Machine::analyze(source, Some(&main), &cfg))
    }

    /// The run read `\end{document}`, stopped nowhere by a budget, within
    /// `forks` undecided choices. Every path dropped at some point is no
    /// end: the run read on from there (`analysis-ended-early`).
    fn converged(a: &Analysis, forks: u64) {
        let budgets = ["limits.steps", "limits.seconds", "limits.line_steps", "limits.memory"];
        let cut: Vec<String> = a
            .facts
            .diagnostics
            .iter()
            .filter(|d| {
                d.code == "budget-exhausted"
                    || d.code == "line-budget-cut"
                    || (d.code == "limit-reached" && budgets.iter().any(|b| d.message.contains(b)))
                    || (d.code == "analysis-ended-early" && d.severity != satex::analysis::facts::Severity::Imprecision)
            })
            .map(|d| format!("{} {}", d.code, d.message))
            .collect();
        assert!(cut.is_empty(), "{cut:?}");
        let main: Vec<String> = a
            .facts
            .diagnostics
            .iter()
            .filter(|d| d.span.file == 0)
            .map(|d| format!("{}:{} {} {}", d.span.line, d.span.col, d.code, d.message))
            .collect();
        assert!(crate::support::defined(a, "SatexReachedEnd"), "the end was not reached: {main:#?}");
        assert!(a.forks <= forks, "{} forks, at most {forks} expected", a.forks);
    }

    fn document(preamble: &str, body: &str) -> String {
        format!(
            "\\documentclass{{article}}\n{preamble}\n\\AtEndDocument{{\\gdef\\SatexReachedEnd{{}}}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"
        )
    }

    /// (1) Three nested maps over constant lists whose body defines a name
    /// built with an unknown: `\cs_new:cpe` finds each name free, as no
    /// defined name has its known prefix and suffix, so it never takes the
    /// error path (which wraps its message over the unknown text).
    #[test]
    fn nested_expl3_maps_defining_names_with_an_unknown_part() {
        let preamble = r"\ExplSyntaxOn
\tl_new:N \l_my_unknown_tl
\tl_set:Nx \l_my_unknown_tl { \int_eval:n { \pdfuniformdeviate 100 } }
\tl_map_inline:nn { a b c d e f } {
  \tl_map_inline:nn { g h i j k l } {
    \tl_map_inline:nn { m n o p q r } {
      \cs_new:cpe { my_ #1 ##1 ####1 \l_my_unknown_tl :n } ########1 { #1 ##1 ####1 ########1 }
    }
  }
}
\ExplSyntaxOff";
        if !crate::support::engine_defines("cs_new:cpe") {
            return;
        }
        let Some(a) = latex("expl3-maps", &document(preamble, "Text."), |_| {}) else { return };
        converged(&a, 0);
        assert!(crate::support::diagnosed(&a, "site-forks-widened").is_empty());
    }

    /// (2) Thirty TikZ nodes whose text width depends on a random number,
    /// with pgfmath, `\colorlet` and positioning on them.
    #[test]
    fn tikz_nodes_of_unknown_width() {
        let preamble = "\\usepackage{tikz}\n\\usetikzlibrary{positioning}";
        let body = r"\begin{tikzpicture}
\foreach \i [count=\n from 0] in {1,...,30} {
  \pgfmathsetmacro{\w}{1+\pdfuniformdeviate 5}
  \pgfmathparse{\w*2+\i}
  \colorlet{c\i}{blue!\pgfmathresult!red}
  \ifnum\i=1
    \node[draw, text width=\w cm, fill=c\i] (n\i) {Node \i};
  \else
    \node[draw, text width=\w cm, fill=c\i, right=of n\n] (n\i) {Node \i};
  \fi
}
\end{tikzpicture}";
        let Some(a) = latex("tikz-nodes", &document(preamble, body), |_| {}) else { return };
        // A library scan that runs to the file's end gives the document's
        // text back instead of swallowing it, so the rest is read too.
        converged(&a, TIKZ_NODES_FORKS);
    }

    /// Forks the thirty nodes of [`tikz_nodes_of_unknown_width`] may take.
    const TIKZ_NODES_FORKS: u64 = 32_000;

    /// (2a) One node's color of (2): `\colorlet` over a pgfmath result of
    /// unknown value. xcolor reads the percentage digit by digit
    /// (xcolor.sty 407, 591), so its digit arms must rejoin within the
    /// line, on few paths and within one node's share of (2)'s forks. Bug
    /// (2026-11-17, owner p1): 1.3k forks and 8 paths to the line's end, as
    /// the digit-split wait in `partner` counts the parked arms but not the
    /// running one, so four arms at one place wait for a fifth. Since p1c
    /// (`\meaning` keeps typed digits): 700 forks, but 14 paths still
    /// meet only at the line's end, as the percentage is any dimension
    /// (pgfmath's result types as -16384..16383) and xcolor's clamps
    /// (xcolor.sty 479) part each component in three.
    #[test]
    fn colorlet_of_an_unknown_pgfmath_result() {
        const NODES: u64 = 30;
        const FEW_PATHS: usize = 3;
        let body = r"\pgfmathsetmacro{\w}{1+\pdfuniformdeviate 5}
\pgfmathparse{\w*2+3}
\colorlet{c}{blue!\pgfmathresult!red}";
        let Some(a) = latex("tikz-colorlet", &document("\\usepackage{tikz}", body), |_| {}) else { return };
        converged(&a, TIKZ_NODES_FORKS / NODES);
        assert!(a.peak_paths <= FEW_PATHS, "{} paths", a.peak_paths);
    }

    /// (2a') The number of (2a) keeps its bound through pgfmath: parse,
    /// operand stack and print of `1+\pdfuniformdeviate 5`, then `*2+3`,
    /// stay within their intervals, so no test past them parts on a wider
    /// range. Pins that (2a)'s paths come from xcolor's digit handling, not
    /// from a pgfmath result of any value (2026-11-17, p1d: it was thought
    /// to type as -16384..16383).
    #[test]
    fn pgfmath_keeps_the_bound_of_a_random_operand() {
        // `\w` is in 1..5 plus a fraction, `\pgfmathresult` in 5..13 plus one.
        let body = r"\pgfmathsetmacro{\w}{1+\pdfuniformdeviate 5}
\ifdim\w pt<1pt \gdef\Wide{}\fi \ifdim\w pt>6pt \gdef\Wide{}\fi
\pgfmathparse{\w*2+3}
\ifdim\pgfmathresult pt<5pt \gdef\Wide{}\fi \ifdim\pgfmathresult pt>14pt \gdef\Wide{}\fi";
        let Some(a) = latex("pgfmath-bound", &document("\\usepackage{tikz}", body), |_| {}) else { return };
        converged(&a, TIKZ_NODES_FORKS / 30);
        assert!(crate::support::undefined(&a, "Wide"), "a result left its interval");
    }

    /// (2b) A mix percentage of typed digits (5..9). xcolor sanitizes the
    /// spec with `\meaning` (`\XC@edef`), which must give the typed digits
    /// back as they are: they are category 12 (tex.web § 465). Bug
    /// (2026-11-17, p1c): `\meaning` printed them as typed text of no
    /// bound, the percentage became any dimension and every component's
    /// digits split by length (88 forks, 7 paths).
    #[test]
    fn mixing_a_color_by_typed_digits_keeps_their_bound() {
        const FORKS: u64 = 8;
        // `\strip@pt` forks once on whether the fraction is 0.
        const PATHS: usize = 2;
        let body = r"\makeatletter\edef\r{\the\numexpr\pdfuniformdeviate 5+5\relax}
\extractcolorspecs{blue!\r!red}\m\c";
        let Some(a) = latex("typed-mix", &document("\\usepackage{xcolor}", body), |_| {}) else { return };
        converged(&a, FORKS);
        assert!(a.peak_paths <= PATHS, "{} paths", a.peak_paths);
    }

    /// An environment whose name is one of two: `\end` looks its hooks up
    /// by a name holding the one-of token, many `\cs:w` at one site. Past
    /// the fresh entries of that point the token is an unknown part of the
    /// name: summarized instead, each lookup parted again (2k forks).
    #[test]
    fn environment_named_one_of_two_ends_in_few_forks() {
        let preamble = r"\newenvironment{va}{}{}\newenvironment{vb}{}{}
\edef\which{\ifodd\pdfuniformdeviate9 va\else vb\fi}";
        let body = r"\expandafter\begin\expandafter{\which}\expandafter\end\expandafter{\which}";
        let Some(a) = latex("one-of-environment", &document(preamble, body), |_| {}) else { return };
        converged(&a, 300);
    }

    /// hyperref's `\pdfstringdef` of a page number not known, plain and as
    /// its page anchors make it: its char loop ends on every path instead
    /// of running to `limits.line_steps`.
    #[test]
    fn pdf_string_of_an_unknown_page_number() {
        let body = r"\makeatletter\c@page=\pdfuniformdeviate 9 \relax
\pdfstringdef\x{\thepage}
{\let\@number\@firstofone\Hy@unicodefalse\pdfstringdef\x{\thepage}}";
        let Some(a) = latex("pdfstring-page", &document(r"\usepackage{hyperref}", body), |_| {}) else { return };
        // hyperref's Unicode conversion (`\HyPsd@DoConvert`, a test per
        // character) runs here since group ends restore exact meanings;
        // a leaked unknown meaning skipped it before (18k forks).
        converged(&a, 32_000);
    }

    /// (3) An expl3 loop whose bound is `\pdfelapsedtime`.
    #[test]
    fn expl3_loop_to_the_elapsed_time() {
        let preamble = r"\ExplSyntaxOn
\int_new:N \l_my_i_int
\int_new:N \l_my_bound_int
\int_set:Nn \l_my_bound_int { \pdfelapsedtime }
\int_do_while:nNnn { \l_my_i_int } < { \l_my_bound_int }
  {
    \int_incr:N \l_my_i_int
    \tl_set:Nx \l_tmpa_tl { \int_use:N \l_my_i_int }
  }
\int_step_inline:nn { \l_my_bound_int } { \tl_put_right:Nn \l_tmpb_tl { #1 } }
\ExplSyntaxOff";
        let Some(a) = latex("expl3-elapsed", &document(preamble, "Text."), |_| {}) else { return };
        converged(&a, 500);
    }

    /// (4) A beamer frame with microtype and a font size that is not known.
    #[test]
    fn beamer_frame_with_microtype_at_an_unknown_size() {
        let source = r"\documentclass{beamer}
\usepackage{microtype}
\newdimen\mysize
\mysize=\pdfuniformdeviate 20pt
\AtEndDocument{\gdef\SatexReachedEnd{}}
\begin{document}
\begin{frame}{A title}
\fontsize{\mysize}{1.2\mysize}\selectfont
Some text with microtype protrusion and expansion.
\begin{itemize}
\item One
\item Two
\end{itemize}
\end{frame}
\end{document}
";
        let Some(a) = latex("beamer-microtype", source, |_| {}) else { return };
        converged(&a, 3_000);
    }

    /// `\csname` of a name with an unknown part that can spell no defined
    /// name makes it `\relax`, as TeX makes every undefined name (tex.web
    /// § 372): a test of it is decided. Once a name of that shape may be
    /// defined, or one is, the test is undecided again.
    #[test]
    fn a_csname_that_can_spell_no_defined_name_is_relax() {
        const HEAD: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\count1=\\pdfelapsedtime \\edef\\u{\\the\\count1}";
        const TEST: &str = "\\expandafter\\ifx\\csname a\\u b\\endcsname\\relax\\def\\Free{}\\else\\def\\Taken{}\\fi";
        let run = |defs: &str| {
            let a = Machine::analyze(&format!("{HEAD}{defs}{TEST}"), None, &super::cfg(1_000_000));
            (crate::support::presence(&a, "Free"), crate::support::presence(&a, "Taken"))
        };
        assert_eq!(run(""), ("1", "0"));
        // Digits never spell letters: only a name with digits between.
        assert_eq!(run("\\def\\axyb{}"), ("1", "0"));
        assert_eq!(run("\\expandafter\\def\\csname a42b\\endcsname{}"), ("?", "?"));
        // The same digits spell the same name: the one just defined.
        assert_eq!(run("\\expandafter\\def\\csname a\\u b\\endcsname{}"), ("0", "1"));
        assert_eq!(run("\\def\\cxyd{}\\def\\xab{}"), ("1", "0"));
    }

    /// A number's digits in a name spell only its values as `\the` prints
    /// them: 10..89 never spells `0`, so pgfmath's `0x` base prefix stays
    /// undefined for it, also once the digits went through a macro.
    #[test]
    fn a_number_in_a_name_spells_only_its_values() {
        let source = "\\catcode`\\{=1 \\catcode`\\}=2
\\expandafter\\def\\csname base@0x\\endcsname{16}
\\edef\\r{\\the\\numexpr10+\\pdfuniformdeviate 80}
\\expandafter\\ifx\\csname base@\\r x\\endcsname\\relax\\def\\NotBase{}\\else\\def\\IsBase{}\\fi
\\def\\n{}\\expandafter\\def\\expandafter\\n\\expandafter{\\r}
\\expandafter\\ifx\\csname base@\\n x\\endcsname\\relax\\def\\NotBaseB{}\\else\\def\\IsBaseB{}\\fi
";
        let a = Machine::analyze(source, None, &super::cfg(1_000_000));
        for (name, defined) in [("NotBase", true), ("IsBase", false), ("NotBaseB", true), ("IsBaseB", false)] {
            assert_eq!(crate::support::defined(&a, name), defined, "{name}");
        }
    }

    /// One place that keeps parting the paths parts no more once it passed
    /// `limits.site_forks` while one line is read: its conditional is
    /// widened and the run reads on, instead of spending the budget.
    #[test]
    fn a_site_past_its_fork_budget_widens_its_conditional() {
        let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\count1=\\pdfelapsedtime \\count3=0
\\def\\x{\\advance\\count2 1 \\ifnum\\count1>\\count2 \\count3=1 \\expandafter\\x\\else\\expandafter\\x\\fi}\\x
\\ifnum\\count3=0 \\def\\Zero{}\\else\\def\\One{}\\fi
\\def\\Next{}
";
        let mut cfg = super::cfg(1_000_000_000);
        let l = &mut cfg.limits;
        (l.widen_after, l.cycle_after, l.recursion_depth) = (u16::MAX, u16::MAX, u16::MAX);
        (l.call_summary_after, l.site_expansions, l.site_forks) = (0, u32::MAX, 64);
        let a = Machine::analyze(source, None, &cfg);
        let codes: Vec<&str> = a.facts.diagnostics.iter().map(|d| d.code).collect();
        assert!(codes.contains(&"site-forks-widened"), "{codes:?}");
        assert!(!codes.contains(&"line-budget-cut"), "{codes:?}");
        assert!(crate::support::defined(&a, "Next"), "{codes:?}");
        assert!(crate::support::maybe_defined(&a, "Zero") && crate::support::maybe_defined(&a, "One"), "{codes:?}");
        assert!(a.forks <= 64 * 2, "{} forks", a.forks);
    }

    /// A widened conditional runs what its arms may do, but a separator
    /// that is `\let` (expl3's `\__int_sep:`) after `\expandafter` defines
    /// nothing: the name past it expands first. An alias of `\number` there
    /// keeps its meaning.
    #[test]
    fn a_widened_conditional_keeps_an_alias_after_an_expandafter_separator() {
        let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\count1=\\pdfelapsedtime
\\let\\sep\\let \\let\\value\\number
\\def\\x{\\advance\\count2 1 \\ifnum\\count1>\\count2 \\expandafter\\sep\\value 0 \\expandafter\\x\\else\\expandafter\\x\\fi}\\x
\\edef\\Check{\\value 12}
";
        let mut cfg = super::cfg(1_000_000_000);
        let l = &mut cfg.limits;
        (l.widen_after, l.cycle_after, l.recursion_depth) = (u16::MAX, u16::MAX, u16::MAX);
        (l.call_summary_after, l.site_expansions, l.site_forks) = (0, u32::MAX, 4);
        let a = Machine::analyze(source, None, &cfg);
        let codes: Vec<&str> = a.facts.diagnostics.iter().map(|d| d.code).collect();
        assert!(codes.contains(&"site-forks-widened"), "{codes:?}");
        assert_eq!(crate::support::body(&a, "Check"), "12", "{codes:?}");
    }

    /// A separator that is `\let`, read as a delimiter by a macro called
    /// before it (expl3's `\__int_mod:ww ... \__int_sep: \__int_eval_end:`),
    /// defines nothing: the alias after it keeps its meaning.
    #[test]
    fn a_widened_conditional_keeps_an_alias_after_a_delimiting_separator() {
        let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\count1=\\pdfelapsedtime
\\let\\sep\\let \\let\\stop\\relax \\def\\m#1\\sep#2\\sep{#1}
\\def\\x{\\advance\\count2 1 \\ifnum\\count1>\\count2 \\m 1\\sep 2\\sep\\stop \\expandafter\\x\\else\\expandafter\\x\\fi}\\x
\\edef\\Check{\\ifx\\stop\\relax y\\else n\\fi}
";
        let mut cfg = super::cfg(1_000_000_000);
        let l = &mut cfg.limits;
        (l.widen_after, l.cycle_after, l.recursion_depth) = (u16::MAX, u16::MAX, u16::MAX);
        (l.call_summary_after, l.site_expansions, l.site_forks) = (0, u32::MAX, 4);
        let a = Machine::analyze(source, None, &cfg);
        let codes: Vec<&str> = a.facts.diagnostics.iter().map(|d| d.code).collect();
        assert!(codes.contains(&"site-forks-widened"), "{codes:?}");
        assert_eq!(crate::support::body(&a, "Check"), "y", "{codes:?}");
    }

    /// A test of `\numexpr x-10` narrows `x` on each arm, `\ifcase` and
    /// `\ifnum` alike, so `\int_to_Hex:n`'s base recursion ends by intervals.
    #[test]
    fn a_test_of_a_register_plus_a_constant_narrows_the_register() {
        let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\count1=\\pdfuniformdeviate 16\\relax
\\ifcase\\numexpr\\count1-10\\relax \\def\\C{y}\\or\\def\\C{y}\\or\\def\\C{y}\\or\\def\\C{y}\\or\\def\\C{y}\\or\\def\\C{y}\\else\\edef\\C{\\ifnum\\count1<10 y\\else n\\fi}\\fi
\\ifnum\\numexpr\\count1-10\\relax<0 \\edef\\D{\\ifnum\\count1<10 y\\else n\\fi}\\else\\edef\\D{\\ifnum\\count1>9 y\\else n\\fi}\\fi
";
        let a = Machine::analyze(source, None, &super::cfg(1_000_000_000));
        assert_eq!(crate::support::body(&a, "D"), "y");
        assert_eq!(crate::support::body(&a, "C"), "y");
    }

    /// Steps a document whose message text is one of a few known texts may
    /// take: the bare article document takes about half of it.
    const WRAP_STEPS: u64 = 2_000_000;
    /// Undecided choices it may make: a few per text it may be.
    const WRAP_FORKS: u64 = 200;

    /// A document that sets `\l_my_tl` to `value` and then runs `call`.
    fn wrapped(name: &str, value: &str, call: &str) -> Option<Analysis> {
        let preamble = format!(
            r"\ExplSyntaxOn
\msg_new:nnn {{ my }} {{ x }} {{ The~value~'#1'~is~not~one~I~know~of,~so~I~wrap~it~over~lines~here~and~'#2'. }}
\tl_set:Ne \l_my_tl {{ {value} }}
{call}
\ExplSyntaxOff"
        );
        latex(name, &document(&preamble, "Text."), |_| {})
    }

    fn wrap_document(name: &str, value: &str, call: &str) {
        let Some(a) = wrapped(name, value, call) else { return };
        converged(&a, WRAP_FORKS);
        assert!(a.steps <= WRAP_STEPS, "{} steps, at most {WRAP_STEPS} expected", a.steps);
        let unsupported: Vec<&str> = a
            .facts
            .diagnostics
            .iter()
            .filter(|d| d.severity == satex::analysis::facts::Severity::Unsupported)
            .map(|d| d.code)
            .collect();
        assert_eq!(unsupported, Vec::<&str>::new());
    }

    /// expl3's `\iow_wrap` (every message) cuts its text into chunks of
    /// eight tokens. Over text that is one of several, its loop converges
    /// in a few paths per text, never a fork per chunk.
    #[test]
    fn a_message_wrapped_over_one_of_several_texts_converges() {
        wrap_document(
            "iow-wrap-one-of",
            r"\int_case:nnF { \pdfuniformdeviate 4 } { {0}{a~b} {1}{d} {2}{ccccccccccccccccccc} } { \c_space_tl }",
            r"\msg_warning:nnee { my } { x } { \l_my_tl } { \l_my_tl \l_my_tl }",
        );
    }

    /// A scan that gave up gives the document text it read back to the
    /// file: no path of the message reads the document ahead of the
    /// others, so none runs off its end or ends in the message's group.
    #[test]
    fn a_message_wrapped_over_one_of_several_texts_ends_on_its_line() {
        let Some(a) = wrapped(
            "iow-wrap-one-of-line",
            r"\int_case:nnF { \pdfuniformdeviate 4 } { {0}{a~b} {1}{d} {2}{ccccccccccccccccccc} } { \c_space_tl }",
            r"\msg_warning:nnee { my } { x } { \l_my_tl } { \l_my_tl \l_my_tl }",
        ) else {
            return;
        };
        let read_on = ["runaway-argument", "file-ended", "unbalanced-file", "unbalanced-conditional", "missing-end"];
        let found: Vec<&str> = a.facts.diagnostics.iter().map(|d| d.code).filter(|c| read_on.contains(c)).collect();
        assert_eq!(found, Vec::<&str>::new());
        assert!(crate::support::defined(&a, "SatexReachedEnd"));
    }

    /// The same over several unknown chunks, a word longer than a line and
    /// runs of spaces.
    #[test]
    fn a_message_wrapped_over_unknown_chunks_long_words_and_spaces_converges() {
        wrap_document(
            "iow-wrap-edge",
            r"\int_if_odd:nTF { \pdfuniformdeviate 4 } { a~b } { \int_if_odd:nTF { \pdfuniformdeviate 4 } { d~~~e } { ccccccccccccccccccc } }",
            r"\msg_warning:nnee { my } { x } { \l_my_tl ~ \int_eval:n { \pdfuniformdeviate 99 } ~ \l_my_tl }
  { averyveryverylongwordwithoutanyspaceinitthatrunsfarpastthelinewidthofseventynineletters ~~ \l_my_tl }",
        );
    }

    /// `analysis.assume_messages_output_only`: a wrap over unknown text is
    /// one step that leaves unknown string text, said as an assumption;
    /// the code the wrap runs before it (#3, the text's expansion) still
    /// runs, and a wrap over known text runs for real.
    fn wrap_call(name: &str, text: &str, assume: bool) -> Option<Analysis> {
        if !crate::support::engine_defines("cs_gset:Npe") {
            return None;
        }
        let preamble = format!(
            r"\ExplSyntaxOn
\int_new:N \g_my_int
\tl_set:Ne \l_my_tl {{ \int_eval:n {{ \pdfuniformdeviate 99 }} }}
\iow_wrap:nnnN {{ {text} }} {{ }} {{ \int_gset:Nn \g_my_int {{ 5 }} }} \iow_term:n
\cs_gset:Npe \After {{ \int_use:N \g_my_int }}
\ExplSyntaxOff"
        );
        latex(name, &document(&preamble, "Text."), |cfg| cfg.analysis.assume_messages_output_only = assume)
    }

    fn assumed(a: &Analysis) -> usize {
        a.facts.diagnostics.iter().filter(|d| d.code == "message-output-only").count()
    }

    #[test]
    fn a_wrap_over_unknown_text_is_assumed_output_only() {
        let Some(a) = wrap_call(
            "iow-assume-unknown",
            r"value~\l_my_tl~and~more~words~than~fit~on~one~line~of~the~terminal~here",
            true,
        ) else {
            return;
        };
        converged(&a, WRAP_FORKS);
        assert_eq!(assumed(&a), 1);
        assert_eq!(crate::support::body(&a, "After"), "5");
        assert!(crate::support::defined(&a, "SatexReachedEnd"));
    }

    #[test]
    fn a_wrap_over_known_text_runs_for_real() {
        let Some(a) =
            wrap_call("iow-assume-known", r"value~and~more~words~than~fit~on~one~line~of~the~terminal~here", true)
        else {
            return;
        };
        assert_eq!(assumed(&a), 0);
        assert_eq!(crate::support::body(&a, "After"), "5");
    }

    #[test]
    fn a_wrap_without_the_assumption_runs_for_real() {
        let Some(a) = wrap_call("iow-assume-off", r"value~\l_my_tl", false) else { return };
        assert_eq!(assumed(&a), 0);
        assert_eq!(crate::support::body(&a, "After"), "5");
        assert!(crate::support::defined(&a, "SatexReachedEnd"));
    }

    /// A loop whose flag a macro in a widened conditional clears
    /// (scrlayer-notecolumn's `\ifeof\slnc@file\@tempswafalse\fi` over a
    /// joined stream): the flag may be false after it, so the loop may end.
    #[test]
    fn a_widened_conditional_may_clear_a_loop_flag() {
        let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6
\\def\\stop{\\let\\ifgo\\iffalse}\\let\\ifgo\\iftrue
\\def\\x{\\ifgo\\ifnum\\pdfuniformdeviate2=0 \\stop\\fi\\expandafter\\x\\fi}\\x
\\def\\Next{}
";
        let mut cfg = super::cfg(20_000_000);
        cfg.limits.site_forks = 1;
        let a = Machine::analyze(source, None, &cfg);
        let codes: Vec<&str> = a.facts.diagnostics.iter().map(|d| d.code).collect();
        assert!(codes.contains(&"undecided-condition"), "{codes:?}");
        assert!(!codes.contains(&"line-budget-cut"), "{codes:?}");
        assert!(crate::support::defined(&a, "Next"), "{codes:?}");
    }
}

/// A line-budget cut that drops no list (the lists left called the
/// document's text: a `\@ifstar`/`\@ifnextchar` look ahead) widens only
/// what was written since that text, never every name: `\newcommand` stays
/// the kernel's, so `\foo` is defined after the cut.
#[test]
fn cut_after_a_look_ahead_keeps_earlier_definitions() {
    let mut cfg = Config {
        load: satex::project::config::Load { packages: false, classes: false, ..Default::default() },
        ..crate::support::config()
    };
    cfg.limits.line_steps = 300;
    let analysis = Machine::analyze(
        "\\makeatletter\\def\\h{\\@ifstar\\hh\\hh}\\def\\hh#1{\\@ifnextchar[\\relax\\relax}\\makeatother\n\
         \\h*{T}\\h{U}\\h{V} x \\newcommand\\foo{x}\\foo\n",
        None,
        &cfg,
    );
    let never: Vec<_> = analysis
        .facts
        .expansions
        .iter()
        .filter(|e| e.meaning == satex::analysis::facts::MeaningKind::Undefined)
        .map(|e| analysis.interner.name(e.name).to_string())
        .collect();
    assert!(never.is_empty(), "undefined on an exact path: {never:?}");
}

/// Lint probes share the run's clock: past `limits.seconds` they stop at
/// once instead of each starting afresh, and what they found is not kept
/// for a later hover.
#[test]
fn lint_probes_stop_at_the_run_deadline() {
    let mut cfg = crate::support::config();
    cfg.limits.seconds = 60;
    let a = satex::analysis::machine::Machine::analyze("\\def\\foo#1#2{#1}\n\\foo ab\n", None, &cfg);
    let foo = a.interner.lookup("foo").expect("defined");
    let past = Instant::now() - Duration::from_secs(cfg.limits.seconds + 1);
    let cut = satex::analysis::probe::within_run(Some(past), || {
        satex::analysis::probe::probe(&a, foo, satex::analysis::probe::State::End)
    });
    assert!(cut.as_ref().is_none_or(|p| !p.complete), "{cut:?}");
    assert!(satex::analysis::probe::probe(&a, foo, satex::analysis::probe::State::End).is_some_and(|p| p.complete));
}

/// A line cut on the look-ahead after a number leaves the `\fi` of a
/// conditional opened before the output routine next, in library code: the
/// cut refuses its read and the output routine may not close it. Reading
/// the pending `\fi`s must stop there, at every phase of the budget check.
#[test]
fn a_cut_before_a_pending_fi_in_the_output_routine_ends() {
    // `limits.line_steps`, and the steps between two budget checks: every
    // phase of the check against the look-ahead is tried.
    const LINE: u64 = 2_000;
    const PHASES: u64 = 512;
    let root = crate::support::tmp_root().join("satex-termination").join("pending-fi");
    let (doc, lib) = (root.join("doc"), root.join("lib"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&doc).unwrap();
    std::fs::create_dir_all(&lib).unwrap();
    let main = doc.join("main.tex");
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\count1=\\pdfelapsedtime \\input ../lib/p\n\
                  \\ifnum\\count1>0 \\def\\m{}\\else\\let\\m\\relax\\fi\n\
                  \\output={\\b\\setbox0\\box255 \\deadcycles0 }\n\
                  \\hsize100pt \\vsize100pt \\ifnum1=1 \\hbox{}\\penalty-10000\n\\def\\Next{}\n";
    std::fs::write(&main, source).unwrap();
    let mut cfg = Config {
        engine: Some(Engine::PdfTeX),
        kernel: Some(Kernel::None),
        load: satex::project::config::Load { inputs: true, ..Default::default() },
        ..crate::support::config()
    };
    let l = &mut cfg.limits;
    l.line_steps = LINE;
    (l.widen_after, l.cycle_after, l.recursion_depth, l.expansion_depth) = (u16::MAX, u16::MAX, u16::MAX, u16::MAX);
    (l.call_summary_after, l.site_expansions, l.repeat_failures, l.site_forks) = (0, u32::MAX, u32::MAX, 0);
    let bound = Duration::from_secs(30);
    for pad in LINE - PHASES..LINE {
        let body = "\\relax".repeat(pad as usize);
        std::fs::write(lib.join("p.tex"), format!("\\def\\b{{{body}\\count2=1\\m\\fi}}\n")).unwrap();
        let (done, ended) = mpsc::channel();
        let (cfg, main, source) = (cfg.clone(), main.clone(), source.to_string());
        std::thread::spawn(move || {
            let _ = done.send(Machine::analyze(&source, Some(&main), &cfg).steps);
        });
        assert!(ended.recv_timeout(bound).is_ok(), "{pad} steps of padding: no end within {bound:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// Runs `source` with kept call effects (`limits.call_memo_steps` 500) and
/// without, the plain catcodes set first.
fn memo_runs(source: &str) -> [satex::analysis::machine::Analysis; 2] {
    let source = format!("{}{source}", crate::support::PRELUDE);
    [500, 0].map(|memo| {
        let mut cfg = cfg(1_000_000_000);
        cfg.limits.call_memo_steps = memo;
        Machine::analyze(&source, None, &cfg)
    })
}

/// A long call met again on the same text, reading the same values, makes
/// the assignments its first run left instead of running. The kept writes
/// are every write the run left, also one that left the old value: the
/// call after `\gdef\got{0}` must leave 300 again, not 0.
#[test]
fn a_repeated_call_applies_its_kept_effect() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10
\\def\\upto#1{{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<#1 \\expandafter\\step\\fi}\\step\\xdef\\got{\\the\\n}}}
\\upto{300}\\upto{300}\\upto{300}\\upto{300}\\upto{300}
\\gdef\\got{0}\\upto{300}
\\def\\three{300}\\ifx\\got\\three\\def\\Kept{}\\else\\def\\Lost{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Kept") && !crate::support::maybe_defined(a, "Lost"));
    }
    // The first call is only noted, the second keeps, three replay. The
    // last one read `\\got` (an `\\xdef` looks at what it replaces), so it runs.
    assert_eq!(kept.call_memo[1], 3, "{:?} kept, replayed, saved", kept.call_memo);
    // Replays of the loop save three of the six runs, at least a third of all steps.
    assert!(kept.steps * 3 <= ran.steps * 2, "{} steps kept, {} run", kept.steps, ran.steps);
}

/// The kept effects are bounded by what they hold, `limits.call_memo_bytes`, not by their
/// number: 100k effects of any size held a quarter of a thesis run's memory. Past the bound
/// all go at once and the newest is kept: with room for one, `\a` and `\b` called in turn
/// push each other out and none replays; with the default room both replay.
#[test]
fn kept_effects_stay_within_their_bytes() {
    let source = format!(
        "{}\\countdef\\n=10
\\def\\upto#1{{{{\\n=0 \\def\\step{{\\advance\\n 1 \\ifnum\\n<#1 \\expandafter\\step\\fi}}\\step\\xdef\\got{{\\the\\n}}}}}}
{}",
        crate::support::PRELUDE,
        "\\upto{300}\\upto{301}".repeat(4)
    );
    let replays = |bytes: u64| {
        let mut cfg = cfg(1_000_000_000);
        cfg.limits.call_memo_steps = 500;
        cfg.limits.call_memo_bytes = bytesize::ByteSize::b(bytes);
        Machine::analyze(&source, None, &cfg).call_memo
    };
    let roomy = replays(cfg(1).limits.call_memo_bytes.as_u64());
    assert!(roomy[1] >= 4, "{roomy:?} kept, replayed, saved");
    let tight = replays(1);
    assert_eq!(tight[1], 0, "{tight:?} kept, replayed, saved");
}

/// Misses of a definition in one place say little of its calls elsewhere:
/// short `\upto{1}` runs (below `call_memo_steps`) used up its probes,
/// and the many long `\upto{300}` calls after them were never recorded
/// again. In a thesis, pgfmath's parser missed this way in the first calls
/// of the body, so every later tikz picture ran without kept effects (a
/// sankey diagram 120M -> 170M steps on its line). A probed-out definition
/// is probed again after geometrically more calls.
#[test]
fn early_misses_do_not_stop_recording_for_good() {
    let probes = usize::from(cfg(1).limits.call_memo_probes);
    // `probes` misses, then `probes` refused calls, then noted, kept, replayed.
    let long = "\\upto{300}".repeat(probes + 6);
    let [kept, ran] = memo_runs(&format!(
        "\\countdef\\n=10
\\def\\upto#1{{{{\\n=0 \\def\\step{{\\advance\\n 1 \\ifnum\\n<#1 \\expandafter\\step\\fi}}\\step\\xdef\\got{{\\the\\n}}}}}}
{}
{long}
\\def\\three{{300}}\\ifx\\got\\three\\def\\Kept{{}}\\else\\def\\Lost{{}}\\fi
",
        "\\upto{1}".repeat(probes + 1)
    ));
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Kept") && !crate::support::maybe_defined(a, "Lost"));
    }
    assert!(kept.call_memo[1] >= 3, "{:?} kept, replayed, saved", kept.call_memo);
}

/// A run is a function of what it read, also where a value it read is only known to
/// lie in an interval: it met no choice, so what it wrote holds for each value the
/// interval stands for. Such runs kept nothing ("vague read"), and in a thesis whose
/// box sizes, marks and page numbers are unknown that was most repeated calls (a third
/// of all steps). `\r` is 300 plus the unknown `\k`, replayed or run.
#[test]
fn a_call_that_reads_an_unknown_value_without_a_choice_keeps_its_effect() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10 \\countdef\\k=11 \\countdef\\r=12 \\k=\\pdfuniformdeviate 5
\\def\\upto{{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step\\global\\r=\\k \\global\\advance\\r\\n}}
\\upto\\upto\\upto\\upto\\upto
\\ifnum\\r>299 \\ifnum\\r<305 \\def\\Kept{}\\else\\def\\Lost{}\\fi\\else\\def\\Lost{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Kept") && !crate::support::maybe_defined(a, "Lost"));
    }
    assert_eq!(kept.call_memo[1], 3, "{:?} kept, replayed, saved", kept.call_memo);
}

/// The other side of the rule above: a run that gave an unknown value a name of its own
/// (the digits `\the` prints of an interval) wrote that one value. Replayed, a later
/// call's digits would carry the same name, and `\ifx` would take two numbers drawn
/// apart for equal. So such a run keeps nothing: `\a` and `\b` may differ.
#[test]
fn a_run_that_names_an_unknown_value_is_not_replayed_for_another() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10 \\countdef\\k=11
\\def\\upto{{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step\\xdef\\got{\\the\\k}}}
\\k=\\pdfuniformdeviate 5 \\upto\\upto\\upto \\let\\a\\got
\\k=\\pdfuniformdeviate 5 \\upto \\let\\b\\got
\\ifx\\a\\b \\def\\Same{}\\else\\def\\Differ{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::maybe_defined(a, "Differ"), "two draws were taken for one number");
    }
}

/// What a run can read of where it stands is the mode, the lists, the categories and the
/// pending prefixes, not how many conditionals or `\aftergroup` tokens are open around
/// it (`\currentiflevel` is a side effect that keeps nothing). A replay asked for the
/// same count of both, so a call kept at the top level ran again inside every `\if`.
#[test]
fn a_kept_effect_replays_inside_another_conditional_and_group() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10
\\def\\upto#1{{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<#1 \\expandafter\\step\\fi}\\step\\xdef\\got{\\the\\n}}}
\\upto{300}\\upto{300}
\\iftrue\\ifnum1=1 \\upto{300}\\fi\\fi
\\def\\three{300}\\ifx\\got\\three\\def\\Kept{}\\else\\def\\Lost{}\\fi
\\begingroup\\aftergroup\\relax\\aftergroup\\relax\\upto{300}\\endgroup
\\ifx\\got\\three\\def\\Grouped{}\\else\\def\\Lost{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Kept") && crate::support::defined(a, "Grouped"));
        assert!(!crate::support::maybe_defined(a, "Lost"));
    }
    assert_eq!(kept.call_memo[1], 2, "{:?} kept, replayed, saved", kept.call_memo);
}

/// A name written locally and then globally holds the global value at every level
/// (tex.web § 283), so one global assignment replays it: pgf's point macros work in a
/// group and hand the result out with `\global`, and none of them kept its effect
/// ("global and local"). A local write after the global one needs both values and is
/// still refused: `\m` must be 1 after the group, not 9 or 300.
#[test]
fn a_name_written_locally_then_globally_replays_as_the_global_write() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10 \\countdef\\m=11
\\def\\upto{\\begingroup\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step\\m=7 \\global\\m=\\n \\endgroup}
\\upto\\upto \\m=5 {\\upto}
\\ifnum\\m=300 \\def\\Kept{}\\else\\def\\Lost{}\\fi
\\def\\both{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step\\global\\m=1 \\m=\\n}
{\\both}{\\both}\\global\\m=9 {\\both}
\\ifnum\\m=1 \\def\\Outer{}\\else\\def\\Lost{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Kept") && crate::support::defined(a, "Outer"));
        assert!(!crate::support::maybe_defined(a, "Lost"));
    }
    assert!(kept.call_memo[1] >= 1, "{:?} kept, replayed, saved", kept.call_memo);
}

/// A `\chardef` is one assignment, also when satex writes its number into
/// the binding after defining it. That second write was taken for a change
/// no assignment replays, so no call that made a `\chardef` kept its effect
/// (a thesis's pgfmath calls never did). `\got` must still be 200 after
/// the replays, and a replay in a group stays local to it.
#[test]
fn a_chardef_in_a_call_keeps_its_effect() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10 \\chardef\\got=1
\\def\\upto{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<200 \\expandafter\\step\\fi}\\step\\chardef\\got=\\n}
\\upto\\upto\\upto\\upto\\upto \\ifnum\\got=200 \\def\\Kept{}\\else\\def\\Lost{}\\fi
\\chardef\\got=5 \\begingroup\\upto\\endgroup
\\ifnum\\got=5 \\def\\Local{}\\else\\def\\Leaked{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Kept") && !crate::support::maybe_defined(a, "Lost"));
        assert!(crate::support::defined(a, "Local") && !crate::support::maybe_defined(a, "Leaked"));
    }
    assert!(kept.call_memo[1] >= 3, "{:?} kept, replayed, saved", kept.call_memo);
}

/// A kept effect holds only while every name its run read holds what it
/// held: `\calc` reads `\base`, so after `\def\base{2}` it runs again and
/// leaves `2300`, not the `1300` its first runs kept.
#[test]
fn a_kept_effect_misses_when_a_name_it_read_changed() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10 \\def\\base{1}
\\def\\calc{{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step\\xdef\\got{\\base\\the\\n}}}
\\calc\\calc\\calc\\def\\base{2}\\calc
\\def\\want{2300}\\ifx\\got\\want\\def\\Fresh{}\\else\\def\\Stale{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Fresh") && !crate::support::maybe_defined(a, "Stale"));
    }
    assert_eq!(kept.call_memo[1], 1, "only the third call replays");
}

/// A name the run set in a group of its own and read after that group
/// ended holds what it held before the run: `\calc` reads `\x` so, and
/// after `\def\x{2}` it runs again and leaves `2300`.
#[test]
fn a_name_read_after_its_group_undid_the_write_is_read_from_before() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10 \\def\\x{1}
\\def\\calc{{\\begingroup\\def\\x{0}\\endgroup\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step\\xdef\\got{\\x\\the\\n}}}
\\calc\\calc\\calc\\def\\x{2}\\calc
\\def\\want{2300}\\ifx\\got\\want\\def\\Fresh{}\\else\\def\\Stale{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Fresh") && !crate::support::maybe_defined(a, "Stale"));
    }
    assert_eq!(kept.call_memo[1], 1, "only the third call replays");
}

/// pgfmath's pattern: `\outer` opens a group, `\inner` computes in it and
/// ends it with `\endgroup`, handing its result out. Such a run keeps its
/// effect up to that `\endgroup` and the text it had left: a replay makes
/// the global writes, closes the group (`\tmp` is gone again) and reads the
/// rest (`\def\res{300}`).
#[test]
fn a_call_that_ends_its_callers_group_replays_the_end() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10
\\def\\inner{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step\\gdef\\seen{y}\\expandafter\\endgroup\\expandafter\\def\\expandafter\\res\\expandafter{\\the\\n}}
\\def\\outer{\\begingroup\\def\\tmp{x}\\inner}
\\outer\\outer\\outer\\outer
\\def\\want{300}\\ifx\\res\\want\\def\\Right{}\\fi
\\ifx\\tmp\\undefined\\else\\def\\Leak{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Right") && crate::support::defined(a, "seen"));
        assert!(!crate::support::maybe_defined(a, "Leak"));
    }
    assert_eq!(kept.call_memo[1], 2, "{:?} kept, replayed, saved", kept.call_memo);
}

/// babel's `\csname extras<lang>\endcsname\relax` around cleveref's
/// `\cref@stack@add`: `\extras` ends in a call that opens a group, loops,
/// then `\expandafter\endgroup\ifnum…\fi`. The false `\ifnum` uses up
/// every list of the call, so the put-back `\endgroup` went into the empty
/// frame `\csname` had left below the call. The run seemed to end before
/// its `\endgroup` with a group open, so it never kept its effect.
#[test]
fn a_put_back_token_of_a_call_stays_its_own() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10
\\def\\add{\\begingroup\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step
  \\expandafter\\endgroup\\ifnum\\n<0 \\gdef\\Never{}\\fi}
\\def\\extras{\\gdef\\seen{y}\\add}
\\def\\switch#1{\\csname extras\\endcsname\\relax\\xdef\\last{#1}}
\\switch1\\switch2\\switch3\\switch4
\\ifnum\\currentgrouplevel=0 \\def\\Closed{}\\else\\def\\Open{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Closed") && crate::support::defined(a, "seen"));
        assert!(!crate::support::maybe_defined(a, "Open") && !crate::support::maybe_defined(a, "Never"));
    }
    assert!(kept.call_memo[1] >= 2, "{:?} kept, replayed, saved", kept.call_memo);
}

/// Only a call the main loop expands keeps or replays an effect. Under
/// `\expandafter\mark\bump` TeX expands `\bump` first but runs `\mark`
/// before `\bump`'s text, so `\mark` still sees `\done` as `\relax`. A
/// replay there would set `\done` before `\mark` runs.
#[test]
fn a_call_an_expandafter_expands_replays_no_effect() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10
\\def\\bump{{\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step}\\gdef\\done{y}}
\\def\\mark{\\xdef\\order{\\ifx\\done\\relax n\\else y\\fi}}
\\let\\done\\relax\\bump\\let\\done\\relax\\bump\\let\\done\\relax\\bump
\\let\\done\\relax\\expandafter\\mark\\bump
\\def\\nn{n}\\ifx\\order\\nn\\def\\Right{}\\else\\def\\Wrong{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Right") && !crate::support::maybe_defined(a, "Wrong"));
    }
    assert_eq!(kept.call_memo[1], 1, "only the third call replays");
}

/// A replayed local assignment stays local: `\bump` sets the count register
/// `\k` at its caller's level. Replayed inside a group, the group's end must
/// restore `\k` to 5. Registers start as global bindings, so taking the
/// binding's own `global` flag for the write's kind made it leak out.
#[test]
fn a_kept_local_assignment_stays_in_its_group() {
    let [kept, ran] = memo_runs(
        "\\countdef\\n=10 \\countdef\\k=11
\\def\\bump{\\begingroup\\n=0 \\def\\step{\\advance\\n 1 \\ifnum\\n<300 \\expandafter\\step\\fi}\\step
  \\expandafter\\endgroup\\expandafter\\k\\the\\n\\relax}
\\bump\\bump \\k=5 \\begingroup\\bump\\endgroup
\\ifnum\\k=5 \\def\\Local{}\\else\\def\\Leaked{}\\fi
",
    );
    for a in [&kept, &ran] {
        assert!(crate::support::defined(a, "Local") && !crate::support::maybe_defined(a, "Leaked"));
    }
    assert_eq!(kept.call_memo[1], 1, "the call in the group replays");
}

/// A macro redefined at one place before each call (`\use:x`'s scratch
/// macro) runs other code each time. Its call site has one place but many
/// bodies, so a summary of its first runs must not stand for a run of
/// another body: here the last call sets `\out` to 5, not to the unknown
/// number of the first two (a summary keyed by place alone lost a shipout's
/// group level this way, and with it the page's `.aux` line).
#[test]
fn a_summary_holds_only_for_the_body_it_ran() {
    let analysis = crate::support::analyze_plain(
        "\\count1=\\pdfuniformdeviate 9 \
         \\def\\set#1{\\edef\\tmp{\\def\\noexpand\\out{#1}\\advance\\count2\\count1 }\\tmp}\
         \\set{\\the\\count1}\\set{\\the\\count1}\\set{\\the\\count1}\\set{5}\\edef\\shown{\\out}",
    );
    let def = analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == "shown").expect("shown");
    let shown =
        satex::engine::tex::detokenize(&def.mac.as_ref().expect("a macro").replacement_text, &analysis.interner);
    assert_eq!(shown, "5");
}

/// Joining, comparing and resuming waiting paths reads no token. On a thesis
/// with 60 paths waiting at one line this work ran for minutes while the
/// step count stood still, so `limits.line_steps` never cut the line. The
/// map entries such work walks are charged to the line too: with the line's
/// budget at what all the run's steps cost, the line is cut once the walks
/// count (`limits.walk_per_step` 1) and not while they are free. Here two
/// paths stay apart in one group (one queued an `\aftergroup`) while both
/// define names, so each switch between them walks what they changed.
#[test]
fn work_on_waiting_paths_counts_against_the_line_budget() {
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\count1=\\pdfelapsedtime
\\def\\loop{\\ifnum\\count2<4000 \\advance\\count2 1 \\expandafter\\def\\csname n\\the\\count2\\endcsname{}\\expandafter\\loop\\fi}
\\begingroup\\ifnum\\count1>5 \\aftergroup\\relax\\fi\\loop\\endgroup
";
    let run = |walk_per_step: u64, line_steps: u64| {
        let mut cfg = cfg(1_000_000_000);
        (cfg.limits.walk_per_step, cfg.limits.line_steps) = (walk_per_step, line_steps);
        Machine::analyze(source, None, &cfg)
    };
    let cut = |a: &satex::analysis::machine::Analysis| a.facts.diagnostics.iter().any(|d| d.code == "line-budget-cut");
    let free = run(u64::MAX, 0);
    assert!(!cut(&run(u64::MAX, free.steps)), "steps alone stay within the budget");
    assert!(cut(&run(1, free.steps)), "the walks of the waiting paths are charged to the line");
}

/// A `#` with no parameter after it stays a `#` character (tex.web § 479):
/// `\def\b{#1}` in a macro of no parameters holds `#` and `1`, and the `#`
/// means "macro parameter character #". satex stored it as a reference to
/// parameter 1, which no call fills, so the reference itself leaked into the
/// input and `\meaning` printed nothing. KOMA's `\scr@ifiscount` recursion
/// (expand the first token while its meaning starts with `macro:`) took the
/// empty text for a macro and expanded the same token forever on one path,
/// 500M steps per line on a thesis whose toc entry ran into `\Ifstr{#1}`.
#[test]
fn a_hash_with_no_parameter_stays_a_character() {
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\def\\empty{}
\\def\\a{\\def\\b{#1}}\\a
\\def\\first#1#2\\end{\\edef\\m{\\meaning#1}}\\expandafter\\first\\b\\end
\\def\\loop#1{\\edef\\n{\\meaning#1}\\ifx\\n\\empty\\expandafter\\loop\\fi#1}\\expandafter\\loop\\b
";
    let analysis = Machine::analyze(source, None, &cfg(20_000_000));
    let cut: Vec<_> = analysis.facts.diagnostics.iter().map(|d| d.code).filter(|c| c.contains("budget")).collect();
    assert!(cut.is_empty(), "{cut:?}");
    let m = satex::analysis::machine::macro_text(&analysis, "m");
    assert_eq!(m.as_deref(), Some("macro parameter character #"));
}

/// The steps one source line may take: the goal for every line of a real
/// document, far above what the lines below need.
const LINE_GOAL: u64 = 10_000_000;

/// An expl3 preamble `body` in an article, run with packages and
/// `limits.line_steps` at [`LINE_GOAL`], caches under `scratch`.
fn expl3_run(body: &str, scratch: &str) -> satex::analysis::machine::Analysis {
    let source = format!(
        "\\documentclass{{article}}\n\\ExplSyntaxOn\n{body}\n\\ExplSyntaxOff\n\\begin{{document}}\nText.\n\\end{{document}}"
    );
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/termination").join(scratch);
    let mut cfg = Config {
        cache: satex::project::config::Cache {
            dir: Some(crate::support::scratch_dir(dir)),
            document: false,
            ..Default::default()
        },
        load: satex::project::config::Load { packages: true, classes: true, ..Default::default() },
        ..crate::support::config()
    };
    cfg.limits.line_steps = LINE_GOAL;
    crate::support::seed_kernel(&cfg);
    Machine::analyze(&source, None, &cfg)
}

/// A property list that is linked on one path and flat on the other.
const JOINED_PROP: &str =
    r"\int_compare:nNnTF {\pdfuniformdeviate 2} = 0 { \prop_new_linked:N \g_x_prop } { \prop_new:N \g_x_prop }";

/// A loop over unknown text that never forks: l3prop's `\__prop_clear_loop:Nw`
/// walks the items of a property list whose paths joined a flat and a linked
/// form, so its item names are unknown text. Each round splits a fresh rest
/// (a new serial) off that text, so no round met the state of the last one
/// exactly: no loop head, no cycle, no call summary (the round writes
/// nothing). `\prop_gclear:N` ran 100M+ steps on its line, and lthooks did
/// the same at `\begin{document}` after beamer's templates. Rounds equal but
/// for the rests they walk are a loop, cut after `limits.widen_after`.
#[test]
fn a_loop_walking_unknown_text_ends_within_the_line_goal() {
    if !crate::support::installed() {
        return;
    }
    let body = format!(
        "{JOINED_PROP}\n\\prop_gput:Nnn \\g_x_prop {{a}}{{1}}\n\\int_compare:nNnT {{\\pdfuniformdeviate 2}} = 0 {{ \\prop_gput:Nnn \\g_x_prop {{b}}{{2}} }}\n\\prop_gclear:N \\g_x_prop"
    );
    let analysis = expl3_run(&body, "unknown-walk");
    let limits: Vec<String> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| ["budget-exhausted", "limit-reached", "analysis-ended-early"].contains(&d.code))
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(limits.is_empty(), "{limits:?}");
}

/// A put into that list reads its form twice: `\__prop_split:NnTFn` tests
/// flat or linked, then `\__prop_put_linked:wnnN` takes the linked form
/// apart. Each read of the joined macro was one of its texts on its own, so
/// the linked arm met the flat text and raised "Paragraph ended before
/// \__prop_pop_linked_prev:w" and "Argument has an extra }", errors no
/// path of TeX raises. A path that took one text keeps the macro at it.
#[test]
fn a_put_into_a_joined_prop_raises_no_error() {
    if !crate::support::installed() {
        return;
    }
    let analysis = expl3_run(&format!("{JOINED_PROP}\n\\prop_gput:Nnn \\g_x_prop {{a}}{{1}}"), "joined-put");
    let errors: Vec<String> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| {
            ["runaway-argument", "extra-right-brace", "missing-number", "extra-fi", "extra-else"].contains(&d.code)
        })
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}
