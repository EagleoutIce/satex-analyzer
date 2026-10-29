//! Undecided choices inside one command: the arms part where the choice
//! is and join where they meet again, inside the command, instead of the
//! command running once per sequence of arms (SEMANTICS.md § 14).

use crate::support::{PRELUDE, broken_gaps, names};
use crate::support::{body, installed};
use satex::machine::{Analysis, Machine};

fn analyze(source: &str) -> Analysis {
    let cfg = satex::config::Config { arm_joins: true, ..crate::support::bare() };
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

fn analyze_latex(source: &str) -> Analysis {
    let cfg = satex::config::Config { arm_joins: true, ..crate::support::config() };
    Machine::analyze(source, None, &cfg)
}

/// Diagnostics with `code`, as `code: message`.
fn with_code(analysis: &Analysis, code: &str) -> Vec<String> {
    analysis.facts.diagnostics.iter().filter(|d| d.code == code).map(|d| format!("{}: {}", d.code, d.message)).collect()
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

/// A one-of token a read splits holds the text it took on that path,
/// wherever its copies went: a `\csname` of one copy decides a test of
/// another.
#[test]
fn split_one_of_narrows_its_copies_on_the_path() {
    let analysis = analyze(
        r"\edef\x{\ifnum\pdfuniformdeviate2=0 a\else b\fi}\edef\y{\x}\def\a{}
          \expandafter\ifx\csname\x\endcsname\a \expandafter\ifx\y a\def\same{}\else\def\apart{}\fi\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"same".to_string()) && !defined.contains(&"apart".to_string()), "{defined:?}");
}

/// Two choices of the same texts are independent, made in two places,
/// twice in one, or in passes a loop joins: a split of one narrows
/// neither the other nor its copies.
#[test]
fn split_one_of_leaves_other_choices_of_the_same_texts() {
    let analysis = analyze(
        r"\edef\x{\ifnum\pdfuniformdeviate2=0 a\else b\fi}\edef\y{\ifnum\pdfuniformdeviate2=0 a\else b\fi}
          \expandafter\ifx\x a\expandafter\ifx\y a\def\ya{}\else\def\yb{}\fi\fi
          \def\m#1{\expandafter\ifx\x#1\def\ms{}\else\def\md{}\fi}\count1=0
          \def\pass{\edef\x{\ifnum\pdfuniformdeviate2=0 a\else b\fi}\ifnum\count1=0 \let\z\x\fi
            \advance\count1 1 \ifnum\count1<2 \expandafter\pass\fi}
          \pass\expandafter\m\z
          \def\n#1{\expandafter\ifx\x#1\def\ns{}\else\def\nd{}\fi}\count1=0
          \def\more{\edef\x{\ifnum\pdfuniformdeviate2=0 a\else b\fi}\ifnum\count1=0 \let\w\x\fi
            \advance\count1 1 \ifnum\pdfuniformdeviate2=0 \expandafter\more\fi}
          \more\expandafter\n\w",
    );
    let defined = names(&analysis);
    for name in ["ya", "yb", "ms", "md", "ns", "nd"] {
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

/// `analysis.assume_compiles`: an arm of library code that raises a
/// certain error is no run of a document that compiles, and is dropped,
/// said once. The same arm in the document's own text is kept: its error
/// is the one to report and fix.
#[test]
fn certain_error_drops_its_path() {
    let arm = r"\ifnum\pdfuniformdeviate2=0 \def\m{a}\undefinedcs\else\def\m{b}\fi ";
    let lib =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-join-lib-{}", std::process::id())));
    std::fs::write(lib.join("arm.tex"), arm).unwrap();
    let source = format!(r"\input {}/arm.tex \edef\z{{\m}}", crate::support::tex_path(&lib));
    let mut cfg = satex::config::Config { arm_joins: true, ..crate::support::bare() };
    cfg.load.inputs = true;
    let analysis = Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg);
    assert_eq!(body(&analysis, "z"), "b");
    let assumed = with_code(&analysis, "assumed-compiles");
    assert_eq!(assumed.len(), 1, "{assumed:?}");
    assert!(assumed[0].contains("Undefined control sequence"), "{assumed:?}");
    let own = analyze(&format!(r"{arm}\edef\z{{\m}}"));
    assert!(body(&own, "z").contains("one of:"), "{}", body(&own, "z"));
    assert!(with_code(&own, "assumed-compiles").is_empty());
    cfg.analysis.assume_compiles = false;
    let kept = Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg);
    assert!(body(&kept, "z").contains("one of:"), "{}", body(&kept, "z"));
    assert!(with_code(&kept, "assumed-compiles").is_empty());
}

/// Once the running path raises a certain error too, nothing compiles: the
/// dropped path runs on at once and joins it, so the text after is read
/// once. Before, it ran on only after the end, and read the rest again.
#[test]
fn a_dropped_path_rejoins_once_nothing_compiles() {
    let source = r"\ifnum\pdfuniformdeviate2=0 \def\m{a}\undefinedcs\else\def\m{b}\fi
        \def\n{x}\undefinedtoo \def\w{\m}\edef\z{\m}";
    let analysis = analyze(source);
    let z = body(&analysis, "z");
    assert!(z.contains('a') && z.contains('b'), "{z}");
    let w = names(&analysis).iter().filter(|n| *n == "w").count();
    assert_eq!(w, 1, "the text after the errors was read {w} times");
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

/// How many times `name` is defined: once per path that reaches it.
fn defined_times(analysis: &Analysis, name: &str) -> usize {
    analysis.facts.defs.iter().filter(|d| analysis.interner.name(d.name) == name).count()
}

const PICK2: &str = r"\ifnum\pdfuniformdeviate2=0 ";

/// Typed digits are digits of category 12: `\ifcat\noexpand` against `0`
/// is true and `\if` against a letter or an unsigned `-` false, decided.
#[test]
fn if_and_ifcat_decide_typed_digits() {
    let analysis = analyze(
        r"\edef\a{\pdfuniformdeviate 100}
          \expandafter\ifcat\expandafter\noexpand\a 0\def\cat{}\else\def\nocat{}\fi
          \expandafter\if\a x\def\letter{}\else\def\noletter{}\fi
          \expandafter\if\a -\def\minus{}\else\def\nominus{}\fi",
    );
    let defined = names(&analysis);
    for (yes, no) in [("cat", "nocat"), ("noletter", "letter"), ("nominus", "minus")] {
        assert!(defined.contains(&yes.to_string()) && !defined.contains(&no.to_string()), "{yes}: {defined:?}");
    }
    assert!(with_code(&analysis, "undecided-condition").is_empty());
}

/// One-of texts whose first tokens all agree decide `\if`, `\ifcat`
/// (behind `\noexpand` too) and `\ifx` on one path.
#[test]
fn one_of_texts_that_agree_decide_the_test() {
    let analysis = analyze(&format!(
        r"\def\x{{}}\let\y=\x
          \edef\r{{{PICK2}a\else b\fi}}
          \expandafter\ifcat\expandafter\noexpand\r x\def\cat{{}}\else\def\nocat{{}}\fi
          \expandafter\if\r 0\def\zero{{}}\else\def\nozero{{}}\fi
          \edef\s{{{PICK2}\noexpand\x\else\noexpand\y\fi}}
          \expandafter\ifx\s\x\def\same{{}}\else\def\notsame{{}}\fi"
    ));
    let defined = names(&analysis);
    for (yes, no) in [("cat", "nocat"), ("nozero", "zero"), ("same", "notsame")] {
        assert!(defined.contains(&yes.to_string()) && !defined.contains(&no.to_string()), "{yes}: {defined:?}");
    }
    assert!(with_code(&analysis, "undecided-condition").is_empty());
}

/// One-of texts that disagree part into one path per answer, never one
/// per text: three texts, two answers, two paths.
#[test]
fn one_of_texts_that_disagree_split_per_answer() {
    let analysis = analyze(
        r"\edef\r{\ifnum\pdfuniformdeviate3=0 a\else\ifnum\pdfuniformdeviate2=0 b\else 1\fi\fi}
          \expandafter\ifcat\expandafter\noexpand\r x\def\yes{}\else\def\no{}\fi
          \expandafter\if\r b\def\isb{}\else\def\notb{}\fi",
    );
    for name in ["yes", "no", "isb", "notb"] {
        assert_eq!(defined_times(&analysis, name), 1, "{name}: {:?}", names(&analysis));
    }
    assert!(with_code(&analysis, "undecided-condition").is_empty());
}

/// A one-of text that may be empty leaves the operand unknown: undecided,
/// both arms run.
#[test]
fn one_of_text_that_may_be_empty_is_undecided() {
    let analysis = analyze(&format!(
        r"\edef\r{{{PICK2}a\fi}}\expandafter\ifcat\expandafter\noexpand\r x\def\yes{{}}\else\def\no{{}}\fi"
    ));
    let defined = names(&analysis);
    assert!(defined.contains(&"yes".to_string()) && defined.contains(&"no".to_string()), "{defined:?}");
}

/// A delimited scan over a one-of token where only some texts hold the
/// delimiter: the argument ends inside those texts, after the token for
/// the others.
#[test]
fn delimited_scan_over_one_of_where_some_texts_hold_the_delimiter() {
    let analysis = analyze(&format!(
        r"\edef\r{{{PICK2}a.b\else c\fi}}
          \def\get#1.#2\end{{\expandafter\def\csname got#1/#2\endcsname{{}}}}\expandafter\get\r.\end"
    ));
    let defined = names(&analysis);
    assert!(defined.contains(&"gota/b.".to_string()) && defined.contains(&"gotc/".to_string()), "{defined:?}");
}

// Joins at `\fi`: both arms of an undecided conditional that reach the same
// `\fi` go on as one path (SEMANTICS.md § 14).

const UNKNOWN_DIMS: &str = r"\setbox0\hbox{x}\dimen1=\wd0 \setbox2\hbox{y}\dimen3=\wd2 ";

/// Arms that only set data join at their `\fi`: the next undecided test
/// parts one path in two, not two in four, and the joined register is an
/// interval that decides what lies outside it.
#[test]
fn data_only_arms_join_at_fi_into_an_interval() {
    let analysis = analyze(&format!(
        r"{UNKNOWN_DIMS}\def\run{{\ifdim\dimen1>5pt \dimen2=1pt \else \dimen2=3pt \fi
          \ifdim\dimen3>5pt \count1=1 \else \count1=2 \fi
          \ifdim\dimen2<4pt \def\below{{}}\else\def\above{{}}\fi
          \ifdim\dimen2>2pt \def\big{{}}\fi \ifdim\dimen2<2pt \def\small{{}}\fi}}\run"
    ));
    let defined = names(&analysis);
    assert!(analysis.peak_paths <= 2, "{} paths", analysis.peak_paths);
    assert!(defined.contains(&"below".to_string()) && !defined.contains(&"above".to_string()), "{defined:?}");
    assert!(defined.contains(&"big".to_string()) && defined.contains(&"small".to_string()), "{defined:?}");
}

/// A macro defined differently in the arms is a meaning set of both
/// bodies after `\fi`: `\ifx` against either is undecided.
#[test]
fn arms_defining_a_macro_differently_join_into_a_meaning_set() {
    let analysis = analyze(&format!(
        r"{UNKNOWN_DIMS}\def\a{{a}}\def\b{{b}}
          \def\run{{\ifdim\dimen1>5pt \def\x{{a}}\else \def\x{{b}}\fi
          \ifdim\dimen3>5pt \count1=1 \else \count1=2 \fi
          \ifx\x\a \def\isa{{}}\fi \ifx\x\b \def\isb{{}}\fi}}\run"
    ));
    let defined = names(&analysis);
    assert!(analysis.peak_paths <= 2, "{} paths", analysis.peak_paths);
    assert!(defined.contains(&"isa".to_string()) && defined.contains(&"isb".to_string()), "{defined:?}");
}

/// A macro one arm defines is maybe defined after `\fi`: a later
/// `\ifdefined` takes both arms.
#[test]
fn a_macro_one_arm_defines_is_maybe_defined_after_fi() {
    let analysis = analyze(&format!(
        r"{UNKNOWN_DIMS}\def\run{{\ifdim\dimen1>5pt \def\y{{}}\fi
          \ifdim\dimen3>5pt \count1=1 \else \count1=2 \fi
          \ifdefined\y \def\has{{}}\else\def\hasnot{{}}\fi}}\run"
    ));
    let defined = names(&analysis);
    assert!(analysis.peak_paths <= 2, "{} paths", analysis.peak_paths);
    assert!(defined.contains(&"has".to_string()) && defined.contains(&"hasnot".to_string()), "{defined:?}");
}

/// A test whose interval straddles the constant splits it: each arm holds
/// its part, so a test inside an arm that the part decides is decided.
#[test]
fn a_straddling_test_splits_the_interval() {
    let analysis = analyze(&format!(
        r"{UNKNOWN_DIMS}\ifdim\dimen1<0pt \dimen1=0pt \fi \ifdim\dimen1>345pt \dimen1=345pt \fi
          \ifdim\dimen1>100pt \ifdim\dimen1>99pt \def\hi{{}}\else\def\nohi{{}}\fi
          \else \ifdim\dimen1>100pt \def\lo{{}}\else\def\nolo{{}}\fi\fi
          \ifdim\dimen1>345pt \def\over{{}}\fi"
    ));
    let defined = names(&analysis);
    assert!(defined.contains(&"hi".to_string()) && !defined.contains(&"nohi".to_string()), "{defined:?}");
    assert!(defined.contains(&"nolo".to_string()) && !defined.contains(&"lo".to_string()), "{defined:?}");
    assert!(!defined.contains(&"over".to_string()), "{defined:?}");
}

/// A recursion whose test is undecided on each level still ends: the arms
/// join at each level's `\fi`.
#[test]
fn a_recursive_undecided_conditional_terminates() {
    let analysis = analyze(&format!(
        r"{UNKNOWN_DIMS}\count1=0
          \def\r{{\advance\count1 1 \ifdim\dimen1>\count1 pt \ifnum\count1<30 \expandafter\expandafter\expandafter\r\fi\fi}}\r
          \def\done{{}}"
    ));
    assert!(names(&analysis).contains(&"done".to_string()), "{:?}", names(&analysis));
    assert!(!analysis.exhausted);
}

/// A pgf-like bounding box update: 50 undecided `\ifdim` in a row, each
/// arm a `\global` assignment that reads on to its `\fi`, stay at most two
/// paths.
#[test]
fn fifty_sequential_bbox_updates_stay_two_paths() {
    let mut body = String::new();
    for i in 0..50 {
        body.push_str(&format!(
            r"\dimen5={i}pt \advance\dimen5 by \dimen1 \ifdim\dimen5>\dimen7 \global\dimen7=\dimen5\fi "
        ));
    }
    let analysis = analyze(&format!(r"{UNKNOWN_DIMS}\dimen7=0pt \def\update{{{body}}}\update \def\done{{}}"));
    assert!(names(&analysis).contains(&"done".to_string()), "{:?}", names(&analysis));
    assert!(analysis.peak_paths <= 2, "{} paths", analysis.peak_paths);
}

/// Arms that leave different category codes stand apart by what a join
/// loses, yet both closed the same `\fi`: they join there, so twelve
/// such conditionals in a row stay two paths.
#[test]
fn arms_with_different_catcodes_join_at_their_fi() {
    let mut body = String::new();
    for i in 0..12 {
        body.push_str(&format!(r"\ifdim\dimen1>{i}pt \catcode`\~=13 \fi "));
    }
    let analysis = analyze(&format!(r"{UNKNOWN_DIMS}\def\update{{{body}}}\update \def\done{{}}"));
    assert!(names(&analysis).contains(&"done".to_string()), "{:?}", names(&analysis));
    assert!(analysis.peak_paths <= 2, "{} paths", analysis.peak_paths);
}

/// Arms that leave a number in disjoint intervals join to a set of
/// intervals, not their hull: a test splits it per member, so a value
/// between them never shows up (`limits.interval_set`).
#[test]
fn disjoint_intervals_join_to_an_interval_set() {
    let source = format!(
        r"{UNKNOWN_DIMS}\ifdim\dimen1<0pt \dimen1=0pt\fi \ifdim\dimen1>10pt \dimen1=10pt\fi
          \dimen2=\dimen1 \ifdim\dimen3>5pt \advance\dimen2 by 100pt \fi
          \ifdim\dimen2>50pt \def\big{{}}\else\def\small{{}}\fi
          \ifdim\dimen2>20pt \ifdim\dimen2<90pt \def\middle{{}}\fi\fi
          \ifdim\dimen2<120pt \def\below{{}}\else\def\above{{}}\fi"
    );
    let analysis = analyze(&source);
    let defined = names(&analysis);
    assert!(defined.contains(&"big".to_string()) && defined.contains(&"small".to_string()), "{defined:?}");
    assert!(!defined.contains(&"middle".to_string()), "{defined:?}");
    assert!(defined.contains(&"below".to_string()) && !defined.contains(&"above".to_string()), "{defined:?}");
    // One member keeps only the hull, which holds values between them.
    let mut cfg = satex::config::Config { arm_joins: true, ..crate::support::bare() };
    cfg.limits.interval_set = 1;
    let hull = Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg);
    assert!(names(&hull).contains(&"middle".to_string()), "{:?}", names(&hull));
}

/// Digits a loop prepends to a macro join into one-of texts; a walk over
/// them to a marker compares the marker with digits, never with a control
/// sequence, so no path leaves the marker unread.
#[test]
fn a_walk_over_joined_digits_meets_its_marker() {
    let analysis = crate::support::analyze_kernel(
        "\\newcount\\c \\newcount\\k \\k=\\pdfuniformdeviate 1000 \\let\\foo\\empty
\\loop\\ifnum\\k>0 \\edef\\foo{\\the\\k\\foo}\\divide\\k by 16 \\repeat
\\def\\walk#1{\\ifx\\stop#1\\expandafter\\relax\\else\\advance\\c-1 \\expandafter\\walk\\fi}
\\def\\stop{\\BADSTOP}
\\expandafter\\walk\\foo\\stop",
    );
    let bad = crate::support::diagnosed(&analysis, "undefined-control-sequence");
    assert!(!bad.iter().any(|m| m.contains("BADSTOP")), "{bad:?}");
}
