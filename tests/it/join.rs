//! Undecided choices inside one command: the arms part where the choice
//! is and join where they meet again, inside the command, instead of the
//! command running once per sequence of arms (SEMANTICS.md § 14).

use crate::support::{body, installed};
use satex::machine::{Analysis, Machine};

const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

fn analyze(source: &str) -> Analysis {
    let cfg = satex::config::Config { arm_joins: true, ..crate::support::bare() };
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

fn analyze_latex(source: &str) -> Analysis {
    let cfg = satex::config::Config { arm_joins: true, ..crate::support::config() };
    Machine::analyze(source, None, &cfg)
}

fn names(analysis: &Analysis) -> Vec<String> {
    analysis.facts.defs.iter().map(|d| analysis.interner.name(d.name).to_string()).collect()
}

/// Diagnostics with `code`, as `code: message`.
fn with_code(analysis: &Analysis, code: &str) -> Vec<String> {
    analysis.facts.diagnostics.iter().filter(|d| d.code == code).map(|d| format!("{}: {}", d.code, d.message)).collect()
}

/// Every broken gap of a run.
fn broken_gaps(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::facts::Severity::Unsupported)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

/// Twelve independent undecided tests in one `\edef` body: 2^12 sequences
/// of arms, which no replay budget covers. The arms of each test join
/// right after its `\fi`, so the body is twelve tokens each standing for
/// either arm, and no arm is skipped.
#[test]
fn edef_with_twelve_independent_undecided_tests_keeps_every_arm() {
    let test = r"\ifnum\pdfuniformdeviate2=0 a\else b\fi ";
    let analysis = analyze(&format!(r"\edef\t{{{}}}", test.repeat(12)));
    assert!(with_code(&analysis, "undecided-condition").is_empty(), "{:?}", broken_gaps(&analysis));
    let t = body(&analysis, "t");
    assert_eq!(t.matches("one of:").count(), 12, "{t}");
}

/// What the joined body stands for is every arm: run, it acts as either.
#[test]
fn joined_edef_body_acts_as_either_arm() {
    let analysis = analyze(
        r"\def\a{\def\gotA{}}\def\b{\def\gotB{}}
          \edef\t{x\ifnum\pdfuniformdeviate2=0 \noexpand\a\else\noexpand\b\fi y}
          \def\x{}\def\y{}\t",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"gotA".to_string()) && defined.contains(&"gotB".to_string()), "{defined:?}");
}

/// An undecided test whose arms make different text before and after a
/// common part: the join keeps the common text as it is.
#[test]
fn joined_edef_body_keeps_common_text() {
    let analysis = analyze(r"\edef\t{p\ifnum\pdfuniformdeviate2=0 a\else bb\fi q}");
    let t = body(&analysis, "t");
    assert!(t.starts_with('p') && t.ends_with('q') && t.contains("one of:"), "{t}");
}

/// An undecided test nested inside an undecided test: each arm joins at
/// its own `\fi`, the inner ones first.
#[test]
fn nested_undecided_tests_in_an_edef_join_inside_out() {
    let analysis =
        analyze(r"\edef\t{\ifnum\pdfuniformdeviate2=0 a\ifnum\pdfuniformdeviate2=0 b\else c\fi\else d\fi e}");
    assert!(with_code(&analysis, "undecided-condition").is_empty(), "{:?}", broken_gaps(&analysis));
    let t = body(&analysis, "t");
    assert!(t.ends_with('e') && t.contains("one of:"), "{t}");
}

/// datatool's `\DTLifinlist` in an `\edef`, its item unknown text: expl3's
/// `\tl_if_empty:n` and the list walk test the unknown text at every step.
/// Each test's arms join in the body, so no arm is skipped.
#[test]
fn datatool_list_test_in_an_edef_over_unknown_text_keeps_every_arm() {
    if !installed() {
        return;
    }
    let analysis = analyze_latex(
        r"\documentclass{article}
\usepackage{datatool}
\begin{document}
\edef\unk{\pdfuniformdeviate 100}
\edef\t{\DTLifinlist{\unk}{1,2,3}{yes}{no}}
\ExplSyntaxOn
\edef\u{\expandafter\tl_if_empty:nTF\expandafter{\unk}{e}{n}}
\ExplSyntaxOff
\end{document}",
    );
    assert!(with_code(&analysis, "undecided-condition").is_empty(), "{:?}", broken_gaps(&analysis));
    let (t, u) = (body(&analysis, "t"), body(&analysis, "u"));
    assert!(t.contains("one of:") || t.contains("unknown"), "{t}");
    assert!(u == "e" || u == "n" || u.contains("one of:") || u.contains("unknown"), "{u}");
}

/// An expandable recursion of undecided length inside an `\edef` (`\rec`
/// appends a token and may call itself again): every pass meets the same
/// test at the same place with a longer body. The body joins to a
/// fixpoint, widened to unknown text after `widen_after` passes.
#[test]
fn expandable_loop_of_undecided_length_in_an_edef_converges() {
    let analysis = analyze(
        r"\def\rec{x\ifnum\pdfuniformdeviate2=0 \expandafter\rec\fi}
          \edef\t{\rec}\def\after{}",
    );
    assert!(with_code(&analysis, "budget-exhausted").is_empty(), "{:?}", broken_gaps(&analysis));
    assert!(with_code(&analysis, "undecided-condition").is_empty(), "{:?}", broken_gaps(&analysis));
    let t = body(&analysis, "t");
    assert!(t.starts_with('x') && (t.contains("one of:") || t.contains("unknown")), "{t}");
    assert!(names(&analysis).contains(&"after".to_string()));
}

/// `\csname` over undecided tests: the name's arms join at `\endcsname`
/// as one of the names built, and the command goes on with each, so both
/// names are defined. Past `meaning_set` names it is one name with an
/// unknown part.
#[test]
fn csname_over_undecided_tests_defines_every_name() {
    let analysis = analyze(
        r"\expandafter\def\csname pre\ifnum\pdfuniformdeviate2=0 a\else b\fi\ifnum\pdfuniformdeviate2=0 c\else d\fi post\endcsname{}
          \def\after{}",
    );
    assert!(with_code(&analysis, "undecided-condition").is_empty(), "{:?}", broken_gaps(&analysis));
    let defined = names(&analysis);
    for name in ["preacpost", "preadpost", "prebcpost", "prebdpost", "after"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
}

/// `\pdfuniformdeviate n` is a number in [0, n): its digits are typed
/// with that interval, and the interval survives `\numexpr`, `\the`,
/// `\number` and a macro argument. A test the interval decides is decided.
#[test]
fn uniformdeviate_is_bounded_through_text() {
    let analysis = analyze(
        r"\edef\a{\pdfuniformdeviate 100}
          \def\dt#1#2\sep{(#1#2-8)/16}
          \count1=\numexpr\expandafter\dt\a\sep\relax
          \ifnum\count1>6 \def\big{}\fi
          \ifnum\count1<-1 \def\neg{}\fi
          \ifnum\count1<7 \def\small{}\fi
          \count2=\numexpr\the\numexpr\pdfuniformdeviate 10+1\relax*2\relax
          \ifnum\count2>20 \def\bigb{}\fi \ifnum\count2<2 \def\smallb{}\fi",
    );
    let a = body(&analysis, "a");
    assert!(a.contains("unknown digits:0..99"), "{a}");
    let defined = names(&analysis);
    for name in ["big", "neg", "bigb", "smallb"] {
        assert!(!defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
    assert!(defined.contains(&"small".to_string()), "{defined:?}");
}

/// A recursion that divides a bounded number until it is small
/// (`\int_to_Hex:n` over `\int_rand:n`) ends where the bound says: no
/// arm is skipped and no depth limit is reached.
#[test]
fn recursion_over_a_bounded_number_ends_by_its_bound() {
    let analysis = analyze(
        r"\def\rec#1{\ifnum#1<16 x\else\expandafter\rec\expandafter{\number\numexpr(#1-8)/16\relax}\fi}
          \edef\h{\rec{\pdfuniformdeviate 65536}}",
    );
    assert!(with_code(&analysis, "limit-reached").is_empty(), "{:?}", broken_gaps(&analysis));
    assert!(with_code(&analysis, "undecided-condition").is_empty(), "{:?}", broken_gaps(&analysis));
    let h = body(&analysis, "h");
    assert!(h.contains('x'), "{h}");
}

/// `\int_to_Hex:n` over a bounded number, its division taken apart digit
/// by digit (`\__int_div_truncate:NwNw`): the arms of each test inside a
/// `\number` scan join at its end, so the paths stay few and the quotient
/// keeps its bound. etex: `\h` is 1 to 4 hex digits, `\count2` in 0..4095.
#[test]
fn digit_tests_inside_a_number_join_at_its_end() {
    let analysis = analyze(
        r"\let\sep=;
          \def\divt#1#2\sep#3#4\sep{\ifx0#1 0\else(#1#2\ifx-#1+\else-\fi(\ifx-#3-\fi#3#4-1)/2)\fi/#3#4}
          \def\div#1#2{\number\numexpr\expandafter\divt\number\numexpr#1\expandafter\sep\number\numexpr#2\sep\relax}
          \def\letter#1{\ifcase\numexpr#1-10\relax A\or B\or C\or D\or E\or F\else\number#1\fi}
          \def\hex#1{\ifnum#1<16 \letter{#1}\else\expandafter\hex\expandafter{\number\div{#1}{16}}\letter{\number\numexpr#1-16*\div{#1}{16}\relax}\fi}
          \count1=\pdfuniformdeviate 65536
          \edef\h{\expandafter\hex\expandafter{\the\count1}}
          \count2=\div{\count1}{16}
          \ifnum\count2>4095 \def\toobig{}\fi \ifnum\count2<0 \def\negative{}\fi \ifnum\count2<4096 \def\fits{}\fi",
    );
    assert!(with_code(&analysis, "limit-reached").is_empty(), "{:?}", broken_gaps(&analysis));
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
    let defined = names(&analysis);
    // The `0` arm of `\ifx0#1` narrows the number to 0, the other to 1..
    for name in ["toobig", "negative"] {
        assert!(!defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
    assert!(defined.contains(&"fits".to_string()) && defined.contains(&"h".to_string()), "{defined:?}");
}

/// pgfmath's `rand` takes a 7-digit number apart place by place
/// (`\pgfmathrand@@#1#2#3#4\pgfmath@`). TeX reads one digit per
/// undelimited parameter; the places keep their tie, so all of them
/// together are the number again. etex: `#1` is `1`, `\count3` is
/// `\count1` in 1000000..1100000, `\count2` its last six places.
#[test]
fn a_bounded_number_taken_apart_digit_by_digit_keeps_its_places() {
    let analysis = analyze(
        r"\count1=\numexpr1000000+\pdfuniformdeviate 100001\relax
          \def\r#1#2#3#4\stop{\ifx1#1\def\one{}\else\def\notone{}\fi \count2=#2#3#4\relax \count3=#1#2#3#4\relax}
          \expandafter\r\the\count1 \stop
          \ifnum\count2>999999 \def\toobig{}\fi \ifnum\count3<1000000 \def\toosmall{}\fi
          \ifnum\count3>1100000 \def\toobigc{}\fi",
    );
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
    assert!(with_code(&analysis, "digits-read-as-one-token").is_empty());
    let defined = names(&analysis);
    for name in ["notone", "toobig", "toosmall", "toobigc"] {
        assert!(!defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
    assert!(defined.contains(&"one".to_string()), "{defined:?}");
}

/// Typed digits are other characters beginning with a digit: `\ifx`
/// against a letter is false (etex: `\bad` stays undefined), so expl3's
/// `\if_meaning:w A #9` loop ends (`\__kernel_str_to_other_fast:n`).
#[test]
fn typed_digits_never_equal_a_letter() {
    let analysis = analyze(r"\catcode`\A=11 \edef\a{\pdfuniformdeviate 100}\expandafter\ifx\a A\def\bad{}\fi");
    assert!(!names(&analysis).contains(&"bad".to_string()), "{:?}", names(&analysis));
}

/// `\ifx` of a token that stands for one of several texts compares the
/// first token of each text on a path of its own (tex.web § 507 compares
/// one token): decided on every path, never left undecided as a whole.
#[test]
fn ifx_of_a_joined_text_is_decided_per_member() {
    let analysis = analyze(
        r"\def\cls{cls}\def\sty{sty}
          \edef\ext{\ifnum\pdfuniformdeviate2=0 \noexpand\cls\else\noexpand\sty\fi}
          \def\test#1{\ifx#1\cls\def\isclass{}\else\def\ispackage{}\fi}
          \expandafter\test\ext",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"isclass".to_string()) && defined.contains(&"ispackage".to_string()), "{defined:?}");
    assert!(with_code(&analysis, "undecided-condition").is_empty(), "{:?}", broken_gaps(&analysis));
}

/// A loop over an unknown count is joined and widened after `widen_after`
/// passes, never cut: what follows the loop still runs.
#[test]
fn a_loop_past_its_passes_is_widened_not_cut() {
    let cfg = satex::config::Config { arm_joins: true, ..crate::support::bare() };
    let analysis = Machine::analyze(
        &format!(
            "{PRELUDE}\\def\\a{{\\advance\\count1 1 \\ifnum\\count1<\\count2 \\expandafter\\a\\fi}}\\count2=\\pdfuniformdeviate 100000 \\a\\def\\after{{}}"
        ),
        None,
        &cfg,
    );
    assert!(names(&analysis).contains(&"after".to_string()), "{:?}", names(&analysis));
}

/// `analysis.join_paths_at`: paths still apart at `\document` are joined
/// there, whatever keeps them apart, and the join names what differed.
#[test]
fn paths_join_at_document() {
    let analysis = analyze(
        r"\def\a{\def\m{a}\document}\def\b{\def\m{b}\document}\def\document{\relax}
          \def\pick{\ifnum\pdfuniformdeviate2=0 \expandafter\a\else\expandafter\b\fi}
          \pick \edef\z{\m}",
    );
    let joined = with_code(&analysis, "paths-joined-at");
    assert_eq!(joined.len(), 1, "{joined:?}");
    assert!(joined[0].contains("2 paths") && joined[0].contains(r"\m"), "{joined:?}");
    assert!(body(&analysis, "z").contains("one of:"), "{}", body(&analysis, "z"));
}

/// The same in LaTeX: a preamble that defines a macro per arm of an
/// undecided `\ifx` starts the body on one path.
#[test]
fn latex_body_starts_on_one_path() {
    if !installed() {
        return;
    }
    let analysis = analyze_latex(
        r"\documentclass{article}
          \ifnum\pdfuniformdeviate 2=0 \def\m{a}\else\def\m{b}\fi
          \begin{document}\edef\z{\m}\end{document}",
    );
    let joined = with_code(&analysis, "paths-joined-at");
    assert!(joined.len() <= 1, "{joined:?}");
    assert!(body(&analysis, "z").contains("one of:"), "{}", body(&analysis, "z"));
}

/// Paths that differ only in what a group end restores meet right after
/// it: a macro with an undecided test inside a group, called over and
/// over, keeps one path, and the steps grow linearly.
#[test]
fn paths_meet_after_group_end() {
    let run = |n: usize| {
        let analysis = analyze(&format!(
            r"\def\x{{\begingroup\ifnum\pdfuniformdeviate 2=0 \def\y{{a}}\else\def\y{{b}}\fi\endgroup}}{}",
            r"\x".repeat(n)
        ));
        assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
        assert!(with_code(&analysis, "paths-widened").is_empty());
        analysis.steps
    };
    let (three, twelve) = (run(3), run(12));
    assert!(twelve <= 5 * three, "{three} steps for 3 calls, {twelve} for 12");
}

/// The same with the group of a LaTeX command: `\textrm{…}` in a macro
/// keeps no path apart.
#[test]
fn text_command_group_keeps_no_path_apart() {
    if !installed() {
        return;
    }
    let doc = |n: usize| {
        format!(
            r"\documentclass{{article}}\def\c{{\textrm{{c\ifnum\pdfuniformdeviate 2=0 \def\y{{a}}\else\def\y{{b}}\fi}}}}
              \begin{{document}}{}\end{{document}}",
            r"\c ".repeat(n)
        )
    };
    let (three, twelve) = (analyze_latex(&doc(3)).steps, analyze_latex(&doc(12)).steps);
    let base = analyze_latex(&doc(0)).steps;
    assert!(twelve - base <= 5 * (three - base), "{base} base, {three} for 3 calls, {twelve} for 12");
}

/// `analysis.assume_compiles`: an arm that raises a certain error is no
/// run of a document that compiles, and is dropped, said once.
#[test]
fn certain_error_drops_its_path() {
    let source = r"\ifnum\pdfuniformdeviate2=0 \def\m{a}\undefinedcs\else\def\m{b}\fi \edef\z{\m}";
    let analysis = analyze(source);
    assert_eq!(body(&analysis, "z"), "b");
    let assumed = with_code(&analysis, "assumed-compiles");
    assert_eq!(assumed.len(), 1, "{assumed:?}");
    assert!(assumed[0].contains("Undefined control sequence"), "{assumed:?}");
    let mut cfg = satex::config::Config { arm_joins: true, ..crate::support::bare() };
    cfg.analysis.assume_compiles = false;
    let kept = Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg);
    assert!(body(&kept, "z").contains("one of:"), "{}", body(&kept, "z"));
    assert!(with_code(&kept, "assumed-compiles").is_empty());
}

/// Undecided loops run one after another from one macro's text, with no
/// source read in between (a hook's code), on paths a group keeps apart:
/// the heads of the loops already
/// left are retired, so the heads kept stay few however many loops run.
/// Before, every head was kept and a large picture took all of memory.
#[test]
fn loop_heads_left_behind_are_retired() {
    let cfg = satex::config::Config {
        engine: Some(satex::config::Engine::PdfTeX),
        kernel: Some(satex::plugin::Kernel::None),
        ..crate::support::config()
    };
    let calls = r"\l".repeat(200);
    let source = format!(
        r"\catcode`\{{=1 \catcode`\}}=2 \countdef\n=9 \n=\pdfelapsedtime
\def\i{{\ifnum\count1<\n \advance\count1 1 \expandafter\i\fi}}
\def\l{{\count1=0 \i\relax}}\def\hook{{\ifnum\n>9 \begingroup\fi{calls}}}\hook"
    );
    let analysis = Machine::analyze(&source, None, &cfg);
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "undecided-loop"), "no loop ran");
    assert!(analysis.loop_heads <= 8, "{} loop heads kept at once", analysis.loop_heads);
}

/// An undecided loop through two heads that alternate, each reached by a
/// tail call from a frame popped and pushed anew (biblatex's `\unspace`):
/// neither head is left behind while the loop runs, so both meet again
/// and the loop reaches a fixpoint. Before, each retired the other and the
/// loop unfolded until a limit.
#[test]
fn alternating_heads_of_a_tail_loop_meet_again() {
    let mut cfg = satex::config::Config {
        engine: Some(satex::config::Engine::PdfTeX),
        kernel: Some(satex::plugin::Kernel::None),
        ..crate::support::config()
    };
    cfg.limits.steps = 2_000_000;
    cfg.limits.seconds = 0;
    let source = r"\catcode`\{=1 \catcode`\}=2 \catcode`\#=6 \countdef\n=9 \n=\pdfelapsedtime
\def\first#1#2{#1}\def\second#1#2{#2}
\def\ifdimpos#1{\ifdim#1>0pt \expandafter\first\else\expandafter\second\fi}
\def\ifnumpos#1{\ifnum#1>0 \expandafter\first\else\expandafter\second\fi}
\def\u{\ifdimpos\lastskip{\unskip\u}{\ifnumpos\lastpenalty{\unpenalty\u}{}}}
\indent x\hskip\n sp\penalty\n\hskip\n sp\penalty\n\hskip\n sp\penalty\n\u.";
    let analysis = Machine::analyze(source, None, &cfg);
    let codes: Vec<_> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(!codes.contains(&"limit-reached") && !codes.contains(&"budget-exhausted"), "{codes:?}");
    assert!(analysis.steps < 100_000, "{} steps", analysis.steps);
}

/// Arms of a `\csname` that end its name at different places in the
/// input: one ends it in a macro body, the other after eating one more
/// token of the source. The join keeps what each has left to read, so
/// the tokens only one arm ate still run on the other.
#[test]
fn csname_arms_ending_apart_keep_their_input() {
    for arms in [r"\expandafter\p\else\expandafter\q", r"\expandafter\q\else\expandafter\p"] {
        let analysis = analyze(&format!(
            r"\def\p{{\endcsname}}\def\q#1{{\endcsname}}
              \def\x{{\ifnum\pdfuniformdeviate2=0 {arms}\fi}}
              \csname n\x\def\gotA{{}}\def\gotB{{}}"
        ));
        let defined = names(&analysis);
        for name in ["gotA", "gotB"] {
            assert!(defined.contains(&name.to_string()), "{arms}: {name}: {defined:?}");
        }
    }
}

/// A delimited argument read past a token for one of several texts, none
/// holding the delimiter (pgfmath's `\pgfmathdivide@advance@last@digit`
/// over a joined `\pgfmathresult`), into the `\fi` of a conditional open
/// around the call: the join's artifact. The argument ends in that text,
/// so the conditional and what follows it are read as the call's own.
#[test]
fn an_argument_over_joined_text_leaves_no_open_conditional() {
    let analysis = analyze(
        r"\edef\r{\ifnum\pdfuniformdeviate2=0 0\else 00\fi}
          \def\digit#1.#2\end{\def\got{#1}}
          \def\run{\ifnum1=1 \expandafter\digit\r C\end\def\inside{}\fi\def\after{}}
          \run",
    );
    let defined = names(&analysis);
    for name in ["got", "inside", "after"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
    assert!(!with_code(&analysis, "argument-ends-in-unknown").is_empty(), "{:?}", broken_gaps(&analysis));
}
