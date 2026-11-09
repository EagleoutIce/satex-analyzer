//! Undecided choices inside one command: the arms part where the choice
//! is and join where they meet again, inside the command, instead of the
//! command running once per sequence of arms (SEMANTICS.md § 14).

use crate::support::{PRELUDE, broken_gaps, names};
use crate::support::{body, installed};
use satex::analysis::machine::{Analysis, Machine};

fn analyze(source: &str) -> Analysis {
    let cfg = satex::project::config::Config { arm_joins: true, ..crate::support::bare() };
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

fn analyze_latex(source: &str) -> Analysis {
    let cfg = satex::project::config::Config { arm_joins: true, ..crate::support::config() };
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
    let cfg = satex::project::config::Config { arm_joins: true, ..crate::support::bare() };
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

/// A path at a join point waits for the others only while one may still
/// reach it. One arm reaches `\document`, the other passes by: the waiting
/// path runs on behind the other and meets it, so the text after is read
/// about once, not once per path. It waited to the end before, and what it
/// did after the join point was lost (`\z` held `b` only).
#[test]
fn a_join_point_another_path_passed_waits_no_longer() {
    let run = |join: &str| {
        let analysis = analyze(&format!(
            r"\def\document{{\relax}}\def\a{{\def\m{{a}}{join}}}\def\b{{\def\m{{b}}}}
              \ifnum\pdfuniformdeviate2=0 \expandafter\a\else\expandafter\b\fi{}\edef\z{{\m}}",
            r"\def\t{x}\edef\u{\t\t}".repeat(40)
        ));
        assert!(body(&analysis, "z").contains("one of:"), "{}", body(&analysis, "z"));
        analysis.steps
    };
    let (waits, plain) = (run(r"\document"), run(""));
    assert!(waits <= plain + plain / 4, "{waits} steps with a join point, {plain} without");
}

/// A path waits at a join point to the end when the running path never
/// stands in its mode past it (here in a paragraph): it still runs on
/// from there once the others ended, so what it does after is kept.
#[test]
fn a_path_waiting_to_the_end_still_runs_on() {
    let analysis = analyze(
        r"\def\document{\relax}\def\a{\def\m{a}\document x}\def\b{\def\m{b}x}
          \ifnum\pdfuniformdeviate2=0 \expandafter\a\else\expandafter\b\fi\edef\z{\m}",
    );
    assert!(body(&analysis, "z").contains("one of:"), "{}", body(&analysis, "z"));
    // `explain` (and so hover) lists each path's text.
    let (records, _) = satex::queries::query::explain(&analysis, &["z".into()], false).unwrap();
    let one_of = records.iter().find_map(|r| r.get("one_of")?.as_array().cloned()).unwrap_or_default();
    let bodies: Vec<&str> = one_of.iter().filter_map(|o| o.get("body")?.as_str()).collect();
    assert!(bodies.contains(&"a") && bodies.contains(&"b"), "{records:?}");
    // One path running a site again: its last text.
    let one = analyze(r"\def\set#1{\edef\z{#1}}\set a\set b");
    assert_eq!(body(&one, "z"), "b");
}

/// Paths that end apart (pending code that differs) still end in one
/// join: what every one of them did is kept, never only the last one's.
#[test]
fn paths_ending_apart_keep_every_effect() {
    for pending in [r"\afterassignment\relax", r"\begingroup\aftergroup\relax", "{"] {
        let analysis = analyze(&format!(r"\ifnum\pdfuniformdeviate2=0 \def\m{{a}}{pending}\else\def\m{{b}}\fi"));
        let may = analysis.interner.lookup("m").and_then(|m| analysis.env.slot(m)?.may.clone());
        assert_eq!(may.map_or(0, |may| may.len()), 2, "{pending}");
    }
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
    let mut cfg = satex::project::config::Config { arm_joins: true, ..crate::support::bare() };
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

/// `analysis.assume_compiles` refutes nothing once nothing compiles
/// (errors kept): a library conditional's `\else` skip crossing into the
/// document past an approximation is then followed both ways, never
/// ended as refuted.
#[test]
fn a_crossing_skip_is_refuted_only_while_the_document_may_compile() {
    let lib = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-join-cross-{}", std::process::id())),
    );
    std::fs::write(lib.join("opener.tex"), r"\def\opener{\iftrue}").unwrap();
    // Every arm raises an error: nothing compiles, so errors are kept.
    let bad = r"\ifnum\pdfuniformdeviate2=0 \undefinedcs\else\undefinedtoo\fi ";
    std::fs::write(lib.join("bad.tex"), bad).unwrap();
    let run = |error: &str| {
        let source = format!(
            r"\input {}/opener.tex {error}\opener\lccode\pdfuniformdeviate 200=65 \else\def\a{{}}\fi\def\after{{}}",
            crate::support::tex_path(&lib)
        );
        let mut cfg = satex::project::config::Config { arm_joins: true, ..crate::support::bare() };
        cfg.load.inputs = true;
        let analysis = Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg);
        with_code(&analysis, "conditional-crosses-file")
    };
    let refuted = run("");
    assert!(refuted.iter().any(|d| d.ends_with("it ends there")), "{refuted:?}");
    let kept = run(&format!(r"\input {}/bad.tex ", crate::support::tex_path(&lib)));
    assert!(kept.iter().any(|d| d.contains("both its end there and the skip on")), "{kept:?}");
}

/// Rows about `\argcs` other than the drop's own note.
fn raised_on_arm(analysis: &Analysis) -> Vec<String> {
    let rows = analysis.facts.diagnostics.iter().filter(|d| d.code != "assumed-compiles");
    rows.filter(|d| d.message.contains("argcs")).map(|d| format!("{}: {}", d.code, d.message)).collect()
}

/// What a library arm refuted under `analysis.assume_compiles` raised is
/// retracted with it: only the drop is said. Where every arm raises it,
/// no path is refuted and the rows stay; so they do without the
/// assumption.
#[test]
fn a_refuted_path_retracts_what_it_raised() {
    let lib = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-join-retract-{}", std::process::id())),
    );
    let run = |arm: &str, compiles: bool| {
        std::fs::write(lib.join("arm.tex"), arm).unwrap();
        let source = format!(r"\input {}/arm.tex \edef\z{{\m}}", crate::support::tex_path(&lib));
        let mut cfg = satex::project::config::Config { arm_joins: true, ..crate::support::bare() };
        cfg.load.inputs = true;
        cfg.analysis.assume_compiles = compiles;
        Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
    };
    let one = r"\def\argcs#1{}\ifnum\pdfuniformdeviate2=0 \def\m{a}\argcs\par\else\def\m{b}\fi ";
    let dropped = run(one, true);
    assert_eq!(with_code(&dropped, "assumed-compiles").len(), 1);
    assert_eq!(raised_on_arm(&dropped), Vec::<String>::new());
    assert!(!raised_on_arm(&run(one, false)).is_empty(), "kept without the assumption");
    let both = r"\def\argcs#1{}\ifnum\pdfuniformdeviate2=0 \def\m{a}\argcs\par\else\def\m{b}\argcs\par\fi ";
    let kept = run(both, true);
    assert!(!raised_on_arm(&kept).is_empty(), "no arm compiles: {:?}", with_code(&kept, "assumed-compiles"));
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
    let cfg = satex::project::config::Config {
        engine: Some(satex::project::config::Engine::PdfTeX),
        kernel: Some(satex::plugins::Kernel::None),
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
    let mut cfg = satex::project::config::Config {
        engine: Some(satex::project::config::Engine::PdfTeX),
        kernel: Some(satex::plugins::Kernel::None),
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

/// A name joined to a character and a primitive is that character behind
/// `\noexpand` where it means it (tex.web § 506): `\if` and `\ifcat` test
/// each meaning, never a control sequence for all.
#[test]
fn noexpand_of_a_name_joined_to_a_character_tests_each_meaning() {
    let analysis = analyze(&format!(
        r"{PICK2}\let\x=a\else\let\x=\relax\fi
          \ifcat\noexpand\x a\def\cat{{}}\else\def\nocat{{}}\fi
          \if\noexpand\x a\def\same{{}}\else\def\notsame{{}}\fi"
    ));
    let defined = names(&analysis);
    for name in ["cat", "nocat", "same", "notsame"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
}

/// A name joined to a glue parameter and to a macro of dimension text,
/// first in a dimension scan: each path reads its own member, the
/// parameter whole (tex.web § 449), never as an integer wanting a unit.
#[test]
fn a_name_joined_to_a_register_and_a_macro_scans_as_each_dimension() {
    let analysis = analyze(&format!(
        r"\baselineskip=12pt {PICK2}\let\h=\baselineskip\else\def\h{{2pt-1sp}}\fi
          \dimen5=\dimexpr 1pt+\h\relax
          \ifdim\dimen5>5pt \def\tall{{}}\else\def\short{{}}\fi"
    ));
    for code in ["illegal-unit", "missing-number"] {
        assert!(with_code(&analysis, code).is_empty(), "{code}: {:?}", with_code(&analysis, code));
    }
    let defined = names(&analysis);
    for name in ["tall", "short"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
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
    let mut cfg = satex::project::config::Config { arm_joins: true, ..crate::support::bare() };
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

/// A widened conditional whose arms read past their `\fi`
/// (`\expandafter\firstoftwo\else\expandafter\secondoftwo\fi{A}{B}`): `{A}`
/// and `{B}` are what the arms read, never code run after the `\fi`, so
/// neither sets `\a` for sure.
#[test]
fn a_widened_arm_reads_its_arguments_past_the_fi() {
    let mut cfg = crate::support::bare();
    cfg.limits.choices = 0;
    let source = r"\long\def\firstoftwo#1#2{#1}\long\def\secondoftwo#1#2{#2}
\def\test{\ifodd\pdfuniformdeviate2 \relax\fi\ifodd\pdfuniformdeviate2 \expandafter\firstoftwo\else\expandafter\secondoftwo\fi{\gdef\a{1}}{\gdef\a{2}}}
\iftrue\test\fi
\gdef\after{}";
    let analysis = Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg);
    assert!(!with_code(&analysis, "undecided-condition").is_empty(), "the conditional was not widened");
    let a = body(&analysis, "a");
    assert!(!["1", "2"].contains(&a.as_str()), "an argument ran as code after the \\fi: \\a is {a}");
    assert!(names(&analysis).contains(&"after".to_string()));
    assert!(with_code(&analysis, "extra-fi").is_empty(), "{:?}", with_code(&analysis, "extra-fi"));
}

/// `\vskip` where the mode is horizontal or internal vertical (an undecided
/// `\leavevmode` joined at its `\fi`) and LaTeX's `\par` is a macro: the
/// horizontal path reads `\par` first, the paths differ and each reads the
/// glue, so `\z@` is never left over as an assignment.
#[test]
fn vertical_glue_in_a_joined_mode_reads_its_glue() {
    if !installed() {
        return;
    }
    let analysis = analyze_latex(
        r"\documentclass{article}
\makeatletter
\def\maybe{\ifodd\pdfuniformdeviate 2 \leavevmode\fi}
\begin{document}
\vbox{\maybe\vskip\z@}
\end{document}",
    );
    assert!(with_code(&analysis, "missing-number").is_empty(), "{:?}", with_code(&analysis, "missing-number"));
}

/// `\vskip\the\skip@` with an unknown glue: the tail `\the` prints is one of
/// the texts TeX may print, which the glue scan reads whole, so vertical
/// mode stays vertical (hyperref's `\Hy@RestoreLastskip`).
#[test]
fn a_printed_glue_is_read_back_whole() {
    if !installed() {
        return;
    }
    let analysis = analyze_latex(
        r"\documentclass{article}
\makeatletter
\begin{document}
\par\skip@=-\lastskip
\edef\x{\vskip\the\skip@\relax}\x
\ifhmode\gdef\Horizontal{}\fi
\end{document}",
    );
    assert!(!names(&analysis).contains(&"Horizontal".to_string()), "part of the glue was typeset");
}

// Files written apart: the texts a file may hold are read one per path,
// and the paths join once each read its text.

/// [`analyze`], reading the files `\input` names.
fn analyze_files(source: &str) -> Analysis {
    let mut cfg = satex::project::config::Config { arm_joins: true, ..crate::support::bare() };
    cfg.load.inputs = true;
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

/// Writes `\a` on one path and `\b` and `\c` on the other to `name`,
/// so the file holds one of two texts of different lengths.
fn two_texts(name: &str, short_first: bool) -> String {
    let (short, long) =
        (r"\immediate\write1{\gdef\noexpand\a{}}", r"\immediate\write1{\gdef\noexpand\b{}\gdef\noexpand\c{}}");
    let (x, y) = if short_first { (short, long) } else { (long, short) };
    format!(
        r"\def\run{{\immediate\openout1={name} \ifnum\pdfuniformdeviate2=0 {x}\else {y}\fi \immediate\closeout1 }}\run "
    )
}

/// Two undecided tests after `text`: one path parts in two at a time.
const TWO_TESTS: &str = r"\def\run{\ifnum\pdfuniformdeviate2=0 \count1=1 \else \count1=2 \fi
  \ifnum\pdfuniformdeviate2=0 \count2=1 \else \count2=2 \fi}\run \gdef\reached{}";

/// A file of two texts of different lengths, `\input`: each path reads its
/// text, and past the file they stand at the same place with the same
/// progress, so they join and the tests after it part one path.
/// Variants: the longer text first.
#[test]
fn paths_that_read_different_texts_of_a_file_join_after_it() {
    for short_first in [true, false] {
        let analysis =
            analyze_files(&format!(r"{}\input twotexts.tex {TWO_TESTS}", two_texts("twotexts.tex", short_first)));
        let defined = names(&analysis);
        let all = ["a", "b", "c", "reached"].iter().all(|n| defined.contains(&n.to_string()));
        assert!(all, "short first {short_first}: {defined:?}");
        assert!(analysis.peak_paths <= 2, "short first {short_first}: {} paths", analysis.peak_paths);
    }
}

/// Writes `linea` on one path and `long` names on the other to `lines.tex`,
/// then reads it in a loop with `reader` (`\read` or `\readline`).
fn read_loop(long: usize, long_first: bool, reader: &str) -> (String, Vec<String>) {
    let name = |n: usize| format!("line{}", char::from(b'a' + n as u8));
    let line = |n: usize| format!(r"\immediate\write1{{{}}}", name(n));
    let (short, long_text) = (line(0), (1..=long).map(line).collect::<String>());
    let (x, y) = if long_first { (&long_text, &short) } else { (&short, &long_text) };
    let write = format!(
        r"\def\run{{\immediate\openout1=lines.tex \ifnum\pdfuniformdeviate2=0 {x}\else {y}\fi \immediate\closeout1 }}\run "
    );
    // Each line names a macro to define, read without a line end.
    let read = format!(
        r"\openin1=lines.tex \endlinechar=-1 \def\next{{\ifeof1 \else \{reader}1 to\l
          \expandafter\gdef\csname\l\endcsname{{}}\expandafter\next\fi}}\next \closein1 \endlinechar=13 "
    );
    (format!("{write}{read}{TWO_TESTS}"), (0..=long).map(name).collect())
}

/// A `\read` loop over a file of one line on one path and three on the
/// other ends on both, every line read, and the paths join after it.
/// Variants: the longer text first, `\readline`, a text longer than the
/// passes a loop runs before it widens.
#[test]
fn a_read_loop_over_a_file_of_two_lengths_ends_on_both_paths() {
    for (long, long_first, reader) in
        [(3, false, "read"), (3, true, "read"), (3, false, "readline"), (6, false, "read")]
    {
        let (source, lines) = read_loop(long, long_first, reader);
        let analysis = analyze_files(&source);
        let defined = names(&analysis);
        let case = format!("{long} lines, long first {long_first}, {reader}");
        assert!(
            lines.iter().chain([&"reached".to_string()]).all(|n| defined.contains(n)),
            "{case}: {defined:?} {:?}",
            analysis.facts.diagnostics.iter().map(|d| format!("{} {}", d.code, d.message)).collect::<Vec<_>>()
        );
        assert!(
            with_code(&analysis, "terminal-read").is_empty(),
            "{case}: {:?}",
            with_code(&analysis, "terminal-read")
        );
        assert!(analysis.peak_paths <= 2, "{case}: {} paths", analysis.peak_paths);
    }
}

/// A stream the paths stand at different places of: read once more than
/// the shorter text holds, outside a loop, the shorter path reads the empty
/// line past its end and `\ifeof` is true there only.
#[test]
fn a_stream_read_past_its_end_on_one_path_is_at_end_there_only() {
    let (source, _) = read_loop(2, false, "read");
    let write = &source[..source.find(r"\openin").expect("the read")];
    let read = r"\openin1=lines.tex \read1 to\l \read1 to\l \ifeof1 \gdef\ended{} \else \gdef\open{} \fi";
    let analysis = analyze_files(&format!("{write}{read}"));
    let defined = names(&analysis);
    assert!(["ended", "open"].iter().all(|n| defined.contains(&n.to_string())), "{defined:?}");
    assert!(with_code(&analysis, "terminal-read").is_empty(), "{:?}", with_code(&analysis, "terminal-read"));
}

/// scrlayer-notecolumn's `.slnc` loop, called twice: each pass reads a
/// line, may stop, and stops at `\ifeof`; a call starts only before the
/// end. TeX never reads the terminal. The flag and the stream's place
/// must not be joined apart into a path that reads a closed stream as a
/// certain fatal error. Variants: two lines, and six (the paths crowd).
#[test]
fn a_read_loop_flag_stays_with_its_stream_place() {
    for lines in [2, 6] {
        let write = (0..lines).map(|n| format!(r"\immediate\write1{{l{n}}}")).collect::<String>();
        let source = format!(
            r"\immediate\openout1=flag.tex {write}\immediate\closeout1 \nonstopmode \openin1=flag.tex
              \def\body{{\read1 to\l \let\go\body \ifnum\pdfuniformdeviate2=0 \let\go\relax\fi
                \ifeof1 \let\go\relax\fi \go}}
              \def\readto{{\ifeof1 \else \expandafter\body\fi}}\readto \readto \gdef\reached{{}}"
        );
        let analysis = analyze_files(&source);
        let certain: Vec<_> = analysis
            .facts
            .diagnostics
            .iter()
            .filter(|d| d.code == "terminal-read-fatal" && d.severity == satex::analysis::facts::Severity::Warning)
            .collect();
        assert!(certain.is_empty(), "{lines} lines: {:?}", with_code(&analysis, "terminal-read-fatal"));
        assert!(names(&analysis).contains(&"reached".to_string()), "{lines} lines: {:?}", names(&analysis));
    }
}

/// A written line of unknown digits reads back as typed digits, by `\read`
/// in a loop and by `\input`: the number stays a number.
#[test]
fn a_written_unknown_line_reads_back_typed() {
    let write = r"\count3=\pdfuniformdeviate 100 \immediate\openout1=unknown.tex
      \immediate\write1{\noexpand\count4=\the\count3 \relax}\immediate\closeout1 ";
    let read = r"\openin1=unknown.tex \def\next{\ifeof1 \else \read1 to\l \l \expandafter\next\fi}\next \closein1
      \ifnum\count4<100 \gdef\fits{}\fi \count4=-1 \input unknown.tex \ifnum\count4>-1 \gdef\again{}\fi";
    let analysis = analyze_files(&format!("{write}{read}"));
    let defined = names(&analysis);
    assert!(defined.contains(&"fits".to_string()) && defined.contains(&"again".to_string()), "{defined:?}");
    assert!(with_code(&analysis, "missing-number").is_empty(), "{:?}", with_code(&analysis, "missing-number"));
}

/// A word still set on one path (`2`, which may kern with what follows)
/// and ended on the other (`3\relax`) joins with each pending character
/// settled against any next one: cmr10's digits kern with nothing, so
/// the box keeps its width and the test after it is decided.
#[test]
fn a_word_set_on_one_path_and_ended_on_the_other_keeps_its_width() {
    if !installed() {
        return;
    }
    let analysis = analyze_latex(
        r"\documentclass{article}
\begin{document}
\setbox2\hbox{2.}
\setbox0\hbox{\ifodd\pdfrandomseed 2\else 3\relax\fi.}
\ifdim\wd0=\wd2 \gdef\Same{}\else\gdef\Apart{}\fi
\end{document}",
    );
    let names = names(&analysis);
    assert!(names.contains(&"Same".to_string()) && !names.contains(&"Apart".to_string()), "{names:?}");
}

/// Settling keeps every kern the pending character may get: `AV` kerns
/// (14.17pt) where `B\relax V` does not (14.58pt), so a box under 14.2pt
/// stays possible.
#[test]
fn a_settled_word_keeps_the_kern_it_may_get() {
    if !installed() {
        return;
    }
    let analysis = analyze_latex(
        r"\documentclass{article}
\begin{document}
\setbox0\hbox{\ifodd\pdfrandomseed A\else B\relax\fi V}
\ifdim\wd0<14.2pt \gdef\Kerned{}\fi
\ifdim\wd0>14.5pt \gdef\Plain{}\fi
\end{document}",
    );
    let names = names(&analysis);
    assert!(names.contains(&"Kerned".to_string()) && names.contains(&"Plain".to_string()), "{names:?}");
}

/// A pending character that may form a ligature is not settled: `fi` is
/// one 5.56pt ligature, narrower than `f` and `i` side by side, so a box
/// under 5.6pt stays possible.
#[test]
fn a_word_that_may_ligature_is_not_settled() {
    if !installed() {
        return;
    }
    let analysis = analyze_latex(
        r"\documentclass{article}
\begin{document}
\setbox0\hbox{\ifodd\pdfrandomseed f\else x\relax\fi i}
\ifdim\wd0<5.6pt \gdef\Ligature{}\fi
\ifdim\wd0>8pt \gdef\Wide{}\fi
\end{document}",
    );
    let names = names(&analysis);
    assert!(names.contains(&"Ligature".to_string()) && names.contains(&"Wide".to_string()), "{names:?}");
}

/// A name read back from a family a name of unknown text was given a
/// conditional (xkeyval's state saved under `\romannumeral` of its depth):
/// one of the family's meanings or `\relax`, so it may open the conditional
/// its `\fi` closes, never runs as unknown code.
#[test]
fn a_name_read_from_a_family_keeps_the_meanings_it_was_given() {
    let analysis = analyze(
        r"\count255=\pdfuniformdeviate 100000 \relax
          \expandafter\let\csname probe@\romannumeral\count255\endcsname\iftrue
          \expandafter\let\expandafter\probeif\csname probe@\romannumeral\count255\endcsname
          \probeif \def\inarm{}\fi",
    );
    assert!(with_code(&analysis, "unknown-text-run").is_empty(), "{:?}", with_code(&analysis, "unknown-text-run"));
    assert!(names(&analysis).contains(&"inarm".to_string()));
}

/// A call by a name of unknown spelling may reach a macro of more parameters
/// than the call was written for (`\pgf@colmarshal` holds a color command with
/// its values). Taking the code behind the list as arguments would drop that
/// code and run it as data: it could redefine `\let`, and did.
#[test]
fn a_call_by_an_unknown_name_ends_with_the_list_it_stands_in() {
    let analysis = analyze(
        r"\expandafter\def\csname fami\endcsname#1{}
          \expandafter\def\csname famii\endcsname#1#2#3{}
          \count255=\pdfuniformdeviate 100000 \relax
          \edef\call{\expandafter\noexpand\csname fam\romannumeral\count255\endcsname{x}}
          \call\def\after{}\relax",
    );
    assert_eq!(crate::support::presence(&analysis, "after"), "1");
}

/// A list of names an undecided loop appends to (a float queue), past
/// the exact set a join keeps: joined, it still holds only the names it
/// may, so running it with `\elt` runs `\elt` on them, never unknown code.
#[test]
fn lists_of_names_through_an_undecided_loop_keep_their_names() {
    let analysis = analyze(
        r"\chardef\bA=1 \chardef\bB=2 \def\F{}\let\elt\relax
          \def\loop{\ifnum\pdfuniformdeviate2=0
            \ifnum\pdfuniformdeviate2=0 \xdef\F{\F\elt\bA}\else\xdef\F{\F\elt\bB}\fi
            \expandafter\loop\fi}\loop
          \def\elt#1{\global\let\got#1}\F",
    );
    assert!(with_code(&analysis, "unknown-text-run").is_empty(), "{:?}", with_code(&analysis, "unknown-text-run"));
    assert!(crate::support::maybe_defined(&analysis, "got"));
}

/// Two lists a join left paired, as a float on the top list or deferred
/// but never both: one `\xdef` of both holds the float once on every arm.
#[test]
fn paired_lists_copied_together_keep_their_pairing() {
    let analysis = analyze(
        r"\chardef\bA=1 \def\T{}\def\D{\elt\bA}
          \ifnum\pdfuniformdeviate2=0 \def\T{\elt\bA}\def\D{}\fi
          \let\elt\relax \xdef\F{\T\D}",
    );
    assert_eq!(crate::support::last_body(&analysis, "F"), r"\elt \bA ");
}

/// A name one arm defines is only maybe defined after the join, so a check
/// that a run reached a marker fails when only some paths did.
#[test]
fn a_marker_one_arm_defines_is_not_surely_defined() {
    let analysis = analyze(r"\ifnum\pdfuniformdeviate2=0 \gdef\reached{}\fi");
    assert!(!crate::support::defined(&analysis, "reached") && !crate::support::undefined(&analysis, "reached"));
}

/// Library code that reads two lists a join decorrelated may meet a state
/// no run has: an error raised there is possible, never certain.
#[test]
fn an_error_after_a_split_of_joined_lists_is_not_certain() {
    let dir = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-joined-error-{}", std::process::id())),
    );
    let tree = crate::support::mini_texmf(&dir);
    std::fs::write(
        tree.join("tex/latex/demo/lists.sty"),
        "\\chardef\\bA=1 \\def\\T{}\\def\\D{\\elt\\bA}\n\
         \\ifnum\\pdfuniformdeviate2=0 \\def\\T{\\elt\\bA}\\def\\D{}\\fi\n\
         \\let\\elt\\relax \\xdef\\F{\\T\\D}\\def\\one{\\elt\\bA}\n\
         \\ifx\\F\\one\\else\\errmessage{lost}\\fi\n",
    )
    .expect("lists.sty");
    let source = "\\usepackage{lists}\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).expect("doc.tex");
    let analysis = Machine::analyze(source, Some(&path), &crate::support::mini_config(&tree, &dir.join("cache")));
    let certain: Vec<_> = analysis.facts.raised.iter().filter(|r| r.certain).map(|r| r.text.clone()).collect();
    assert!(certain.is_empty(), "{certain:?}");
}

/// A loop whose first passes leave a name alone: the passes not run may
/// still define it, so after the loop it is maybe defined, never undefined.
#[test]
fn a_loop_defines_late_what_its_first_passes_do_not() {
    let a = analyze(
        "\\def\\loop#1\\repeat{\\def\\body{#1}\\iterate}\
         \\def\\iterate{\\body\\let\\next\\iterate\\else\\let\\next\\relax\\fi\\next}\
         \\count1=0 \\loop\\advance\\count1 1 \\ifnum\\count1>3 \\gdef\\late{}\\fi\
         \\ifnum\\count1<\\pdfuniformdeviate100 \\repeat",
    );
    assert!(crate::support::maybe_defined(&a, "late"), "\\late is never defined after the loop");
}

/// Names of unknown text given a text per record (a note per line read):
/// the family's texts widen to their hull, so the paths a read of the
/// family parts into do not grow with the number of records.
#[test]
fn a_family_given_a_text_per_record_keeps_the_paths_few() {
    let peak = |records: usize| {
        let name = r"\csname rec@\romannumeral\count255\endcsname";
        let mut source = String::from(r"\long\def\addto#1#2{\toks0\expandafter{#1#2}\xdef#1{\the\toks0}}");
        for r in 0..records {
            source += &format!("\\count255=\\pdfuniformdeviate 100000 \\expandafter\\addto{name}{{\\def\\r{{{r}}}}}\n");
        }
        source += &format!("{name}{name}");
        analyze(&source).peak_paths
    };
    let (few, many) = (peak(2), peak(6));
    assert!(many <= few, "{many} paths with 6 records, {few} with 2");
}

/// A command that starts a paragraph where the mode is joined (internal
/// vertical or horizontal) parts by mode. Its arms part after the last
/// `\fi`, so they never meet at it before reading the command again: one
/// `\strut` per line is no loop, and nothing is summarized.
#[test]
fn a_mode_split_after_a_joined_fi_is_no_loop() {
    let a = analyze(
        r"\setbox1\hbox{\vrule height 8pt depth 3pt width 0pt}\def\strut{\relax\ifmmode\copy1\else\unhcopy1\fi}
          \everypar{\relax}
          \setbox0\vbox{\ifnum\pdfuniformdeviate2=0 x\fi
            \strut\ignorespaces a\par\strut\ignorespaces b\par\strut\ignorespaces c\par\gdef\inbox{}}\gdef\after{}",
    );
    assert!(with_code(&a, "recursion-widened").is_empty(), "{:?}", with_code(&a, "recursion-widened"));
    assert!(crate::support::defined(&a, "inbox") && crate::support::defined(&a, "after"));
}

/// Intuition: `\the` of a register gives one token of typed digits that stands for one number,
/// so two `\edef`s from that token hold equal texts on every path.
/// Bug: `\ifx` called them undecided, as any two texts with unknown parts. hyperref's
/// `\HyPsd@RemoveBraces` compares such copies per character and then ran its brace arm on a page
/// number, in a loop that a cut ended, with false runaway arguments behind it.
/// A changed register prints another number: that test must stay open.
#[test]
fn copies_of_a_typed_number_are_one_text() {
    let analysis = analyze(
        r"\count1=\pdflastypos \ifnum\count1<4 \count1=4 \fi
          \edef\x{\the\count1}\edef\a{\x x}\edef\b{\x x}
          \ifx\a\b \gdef\same{}\else \gdef\differ{}\fi
          \edef\c{\the\count1}\count1=\pdflastxpos \ifnum\count1<4 \count1=4 \fi \edef\d{\the\count1}
          \ifx\c\d \gdef\mayequal{}\else \gdef\maydiffer{}\fi",
    );
    let names = names(&analysis);
    assert!(names.contains(&"same".to_string()), "{names:?}");
    assert!(!names.contains(&"differ".to_string()), "{names:?}");
    assert!(names.contains(&"mayequal".to_string()) && names.contains(&"maydiffer".to_string()), "{names:?}");
}

/// Intuition: a number with a lower bound under ten and no upper bound can have any spelling
/// length, so a split by length keeps nothing of its bound, whatever that lower bound is.
/// Bug: only a lower bound of at most 1 counted as any length: a page number known to be at
/// least 4 split into ten paths per read, and the joined arms lost its digits.
#[test]
fn digits_of_any_length_stay_one_argument_above_any_small_bound() {
    let analysis = analyze(
        r"\count1=\pdflastypos \ifnum\count1<4 \count1=4 \fi
          \edef\x{\the\count1}\def\f#1#2\end{\gdef\got{#1}\gdef\rest{#2}}\expandafter\f\x\end",
    );
    assert!(!with_code(&analysis, "digits-read-as-one-token").is_empty());
    // A number with only a negative side has any length too.
    let negative = analyze(
        r"\count1=\pdflastypos \ifnum\count1>0 \count1=0 \fi
          \edef\x{\the\count1}\def\f#1#2\end{\gdef\got{#1}\gdef\rest{#2}}\expandafter\f\x\end",
    );
    assert!(!with_code(&negative, "digits-read-as-one-token").is_empty());
}

/// Intuition: a printed number is one `-` at most, first, then digits. No `--` can start in it, so
/// a scan for `--` over it finds only the delimiter after it.
/// Bug: the scan asked whether the number may hold a `-` and cut the argument inside it. hyperref
/// substitutes `---` and `--` in every PDF string, so a negative page number lost its class there:
/// any text that may hold `\`, and `\HyPsd@DoConvert` then ran into its own end.
/// `-` alone stays possible.
#[test]
fn no_double_dash_starts_in_a_printed_number() {
    let args = |delimiter: &str| {
        analyze(&format!(
            r"\count1=\pdflastypos \ifnum\count1>0 \count1=0 \fi \edef\x{{\the\count1}}
              \def\f#1{delimiter}#2\end{{\gdef\one{{#1}}\gdef\two{{#2}}}}\expandafter\f\x{delimiter}\end"
        ))
    };
    assert_eq!(body(&args("--"), "two"), "");
    assert_ne!(body(&args("-"), "two"), "");
}
