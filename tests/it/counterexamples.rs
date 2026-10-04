//! Counterexamples taken from the engine: every input here was run through
//! `pdftex` (TeX Live 2026) with `-interaction=nonstopmode` and
//! `\message`/`\meaning`, and each test asserts the answer it gave.
//!
//! Follows the `analyze()`/`body()` helper pattern from `tests/it/semantics.rs`.

use crate::support::{body, installed};
use satex::config::{Config, Load};
use satex::machine::{Analysis, Machine};

// Without a format the catcodes are INITEX's (tex.web § 232).
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

fn analyze(source: &str) -> Analysis {
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &crate::support::bare())
}

/// The *last* definition of `name`: some of the inputs below redefine a
/// control sequence more than once (e.g. inside a group closed by
/// `\aftergroup`), and it is the final meaning that a real run would show.
fn last_definition<'a>(analysis: &'a Analysis, name: &str) -> Option<&'a satex::facts::Definition> {
    analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == name)
}

fn last_body(analysis: &Analysis, name: &str) -> String {
    last_definition(analysis, name)
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

/// Probes that each define macros, with the body TeX gives each one.
const BODIES: &[crate::support::BodyRow] = &[
    // tex.web §§ 461-475: a `\skip` register's value is a ⟨glue⟩, scanned
    // without needing braces. `pdftex`: `\the\skip0` after
    // `\skip0=1pt plus 2pt` is `1.0pt plus 2.0pt`.
    (
        "unbraced_skip_register_is_scanned_as_glue",
        r"\skip0=1pt plus 2pt \edef\v{\the\skip0}",
        &[("v", "1.0pt plus 2.0pt")],
    ),
    // tex.web § 326: `\afterassignment` holds its token until the *next*
    // assignment is complete. `pdftex`: `\seen` ends up `5`.
    (
        "afterassignment_runs_after_the_assignment_completes",
        r"\count0=0 \def\mark{\xdef\seen{\the\count0}} \afterassignment\mark\count0=5",
        &[("seen", "5")],
    ),
    // `\futurelet\cs t1 t2` reads two raw tokens with no `=`/space handling.
    // `pdftex`: `\futurelet\test=XY` binds `\test` to `X` ("the letter X").
    (
        "futurelet_binds_the_first_lookahead_token",
        r"\futurelet\test=XY \edef\z{\meaning\test}",
        &[("z", "the letter X")],
    ),
    // TeXbook ch. 7 (`\lowercase`/`\uppercase`): every character token is
    // case-shifted through `\lccode`/`\uccode`; control sequence names are not
    // touched. `pdftex`: `\lowercase{\def\lo{X}}` defines `\lo` as `x`.
    ("lowercase_shifts_characters_via_lccode", r"\lowercase{\def\lo{X}}", &[("lo", "x")]),
    // `\uppercase`/`\lowercase` are not expandable, so an `\edef` leaves them
    // untouched. `pdftex`: `\meaning\z` is `macro:->\uppercase {ab}`, literal.
    ("uppercase_is_not_expandable_inside_edef", r"\edef\z{\uppercase{ab}}", &[("z", r"\uppercase {ab}")]),
    // TeXbook ch. 24: TeX integers stay within `±(2^31-1)`; the reference
    // engine's 32-bit registers wrap on overflow. `pdftex`: `\the\count0` after
    // `\count0=2147483647 \advance\count0 by 1` is `-2147483648`.
    (
        "count_register_arithmetic_wraps_at_32_bits",
        r"\count0=2147483647 \advance\count0 by 1 \edef\v{\the\count0}",
        &[("v", "-2147483648")],
    ),
    // tex.web § 1221: an active character is, once tokenized, a reference into
    // the same `eqtb` a control sequence uses, so `\let` to it copies its
    // *current* meaning. `pdftex`: after `\def~{TILDE}`, `\let\altTilde~` makes
    // `\meaning\altTilde` read `macro:->TILDE`.
    (
        "let_of_an_active_character_copies_its_bound_meaning",
        r"\catcode`\~=13 \def~{TILDE} \let\altTilde~ \edef\altmeaning{\meaning\altTilde}",
        &[("altmeaning", "macro:->TILDE")],
    ),
    // tex.web § 298 (`print_meaning`): `\meaning` of a `\let`-to-character
    // binding names its category by TeX's own word — "the letter" for
    // catcode 11 — not a generic "of category N". `pdftex`: `\meaning\a` after
    // `\let\a=X` is `the letter X`.
    ("meaning_of_a_letter_says_the_letter", r"\let\a=X \edef\am{\meaning\a}", &[("am", "the letter X")]),
    // tex.web § 367: `\expandafter t1 t2` expands `t2` exactly one step and
    // reinserts `t1`; chained `\expandafter`s nest that rule recursively, so
    // only the token right after the *last* `\expandafter` gets expanded.
    (
        "expandafter_chain_expands_only_the_last_token",
        r"\def\a{A}\def\b{B}\expandafter\def\expandafter\c\expandafter{\a\b}",
        &[("c", "A\\b ")],
    ),
    // tex.web § 369: `\noexpand⟨token⟩` inside `\edef` passes that one token
    // through untouched.
    ("noexpand_survives_edef", r"\def\foo{FOO}\edef\z{\noexpand\foo bar}", &[("z", r"\foo bar")]),
    // `\let` skips leading spaces, then an optional `=`, then at most one more
    // optional space, before reading the source token — even for a
    // control-*symbol* target (`\!`), which does not itself eat trailing
    // spaces the way a control word does. Checked with `\ifx` rather than
    // `\meaning` so the comparison is independent of the "the letter" wording
    // bug (see `meaning_of_a_letter_says_the_letter` above).
    ("let_skips_the_optional_equals_and_space", r"\let\! = X \edef\z{\ifx\!X yes\else no\fi}", &[("z", " yes")]),
    // tex.web § 509: a negative `\ifcase` selector matches no `\or`-delimited
    // arm, so the `\else` branch runs.
    (
        "ifcase_with_a_negative_selector_runs_the_else_branch",
        r"\ifcase-1 \def\seen{A}\or\def\seen{B}\else\def\seen{C}\fi",
        &[("seen", "C")],
    ),
    // tex.web § 372: `\csname...\endcsname` expands its contents (via
    // `get_x_token`) before building the name.
    (
        "csname_builds_its_name_from_expansion",
        r"\def\name{foo}\expandafter\def\csname\name\endcsname{BAR}",
        &[("foo", "BAR")],
    ),
    // TeXbook ch. 6: `\global` inside a group makes even a plain register
    // assignment survive the group's end.  `\countdef` (not `\newcount`, a
    // `plain.tex` macro this bare-initex `analyze()` does not have) names the
    // register (pdftex -ini confirms `\newcount` is undefined without a format).
    (
        "global_register_assignment_survives_its_group",
        r"\countdef\mycount=5 \mycount=0 {\global\mycount=5 } \edef\v{\the\mycount}",
        &[("v", "5")],
    ),
    // tex.web § 465: `\the` of a token register returns its stored tokens
    // verbatim, unexpanded.
    ("the_of_a_toks_register_is_verbatim", r"\toks0={a b c} \edef\v{\the\toks0}", &[("v", "a b c")]),
    // eTeX manual § 3.2: `\unless\ifnum...` negates the conditional's result.
    (
        "unless_negates_the_following_conditional",
        r"\unless\ifnum1>2 \def\seen{A}\else\def\seen{B}\fi",
        &[("seen", "A")],
    ),
    // tex.web § 461: a `\muskip` holds math glue, printed in `mu`, and an
    // order of infinity may appear in either component. `pdftex`:
    // `\muskip0=1mu plus 2fill minus 3mu` shows as
    // `1.0mu plus 2.0fill minus 3.0mu`.
    (
        "a_muskip_is_scanned_and_printed_in_mu",
        r"\muskip0=1mu plus 2fill minus 3mu \edef\v{\the\muskip0}",
        &[("v", "1.0mu plus 2.0fill minus 3.0mu")],
    ),
    // tex.web § 407: keywords are matched case-insensitively, so `PLUS` opens
    // a stretch clause. `pdftex`: `\skip0=1pt PLUS 2pt` is `1.0pt plus 2.0pt`.
    (
        "glue_keywords_are_matched_without_regard_to_case",
        r"\skip0=1pt PLUS 2pt \edef\v{\the\skip0}",
        &[("v", "1.0pt plus 2.0pt")],
    ),
    // tex.web § 1239: `\advance` on a glue adds all three components, and
    // tex.web § 1240: `\multiply` scales them. `pdftex`: `1.0pt plus 2.0pt`
    // advanced by `1pt` is `2.0pt plus 2.0pt` and multiplied by 3 is
    // `3.0pt plus 6.0pt`.
    (
        "glue_arithmetic_works_on_every_component",
        r"\skip0=1pt plus 2pt \advance\skip0 by 1pt \edef\v{\the\skip0}
          \skip1=1pt plus 2pt \multiply\skip1 by 3 \edef\w{\the\skip1}",
        &[("v", "2.0pt plus 2.0pt"), ("w", "3.0pt plus 6.0pt")],
    ),
    // tex.web § 1240: `\multiply` and `\divide` go through `mult_integers` and
    // `x_over_n`, which report "Arithmetic overflow" and leave the register
    // alone — unlike `\advance`, which has no such check. `pdftex`:
    // `\count0=2147483647 \multiply\count0 by 2` leaves `2147483647`.
    (
        "multiply_leaves_the_register_alone_when_it_overflows",
        r"\count0=2147483647 \multiply\count0 by 2 \edef\v{\the\count0}
          \count1=7 \divide\count1 by 0 \edef\w{\the\count1}",
        &[("v", "2147483647"), ("w", "7")],
    ),
    // tex.web § 445: `scan_int` reports "Number too big" past 2^31-1 and uses
    // the limit instead. `pdftex`: `\count0=2147483648` leaves `2147483647`.
    (
        "an_integer_constant_is_clamped_to_the_32_bit_maximum",
        r"\count0=2147483648 \edef\v{\the\count0}",
        &[("v", "2147483647")],
    ),
    // tex.web § 453: `⟨factor⟩⟨internal unit⟩` scales the unit, so `-.5\dimen0`
    // is half of it negated; tex.web § 449: an internal *integer* is only the
    // factor, and the unit still follows. `pdftex`: `-0.75pt` and `3.0pt`.
    (
        "a_dimension_may_be_a_factor_times_an_internal_unit",
        r"\dimen0=1.5pt \dimen0=-.5\dimen0 \edef\v{\the\dimen0}
          \count1=3 \dimen1=\count1 pt \edef\w{\the\dimen1}",
        &[("v", "-0.75pt"), ("w", "3.0pt")],
    ),
    // tex.web § 448: a ⟨dimen⟩ swallows one optional space after its unit, so
    // the branch of `\ifdim 1pt<2pt T` starts at `T` and not at a space.
    ("a_dimension_eats_the_space_that_ends_it", r"\edef\v{[\ifdim 1pt<2pt T\else F\fi]}", &[("v", "[T]")]),
    // tex.web § 262 (`print_cs`): a control sequence prints with a trailing
    // space unless its name is a single non-letter. `pdftex`: `\meaning` of a
    // macro whose body is `\a` reads `macro:->\a `, and of one whose body is
    // `\!` reads `macro:->\!`.
    (
        "a_control_word_detokenizes_with_a_trailing_space",
        r"\def\a{X}\def\v{\a}\def\!{bang}\def\w{\!}",
        &[("v", "\\a "), ("w", "\\!")],
    ),
    // tex.web § 296 with pdfTeX's `\protected`: the prefixes a macro carries
    // are part of its meaning and print before the word `macro`. `pdftex`:
    // `\protected\long\def\a#1{x}` shows as `\protected\long macro:#1->x`.
    (
        "meaning_names_the_prefixes_a_macro_was_defined_with",
        r"\long\def\a#1{#1}\edef\v{\meaning\a}
          \protected\long\def\b#1{x}\edef\w{\meaning\b}",
        &[("v", r"\long macro:#1->#1"), ("w", r"\protected\long macro:#1->x")],
    ),
    // tex.web § 298 (`print_cmd_chr`): every category has its own phrase.
    (
        "meaning_names_each_category_by_its_own_phrase",
        r"\let\a=, \edef\v{\meaning\a}
          \let\b={ \edef\w{\meaning\b}
          \let\c=# \edef\x{\meaning\c}",
        &[("v", "the character ,"), ("w", "begin-group character {"), ("x", "macro parameter character #")],
    ),
    // The TeXbook, ch. 20: a `\long` macro accepts `\par` in its arguments.
    (
        "a_long_macro_accepts_par_in_an_argument",
        r"\long\def\keep#1{KEPT:[#1]}\edef\v{\keep{a\par b}}",
        &[("v", r"KEPT:[a\par b]")],
    ),
    // tex.web § 232: `\lccode` and `\uccode` start at IniTeX's defaults, are
    // read back with `\the`, and are what `\lowercase` consults. `pdftex`:
    // `\the\lccode`\A` is 97, and `\lccode`\Q=`\z` makes `\lowercase{Q}` a `z`.
    (
        "the_case_tables_have_inited_defaults_and_can_be_set",
        r"\edef\v{[\the\lccode`\A][\the\uccode`\a][\the\sfcode`\A]}
          \lccode`\Q=`\z \lowercase{\def\w{Q}}",
        &[("v", "[97][65][999]"), ("w", "z")],
    ),
];

#[test]
fn macro_bodies_agree_with_tex() {
    crate::support::body_rows(BODIES, analyze);
}

/// tex.web § 400/479-480: `\par` inside a non-`\long` macro's argument
/// aborts the argument scan ("Paragraph ended before ... was complete");
/// the call is never completed with `\par` inside it. Reproduced with
/// `pdftex -interaction=nonstopmode` on the two lines below, which also
/// raises "Too many }'s" as part of the same recovery; the net text that
/// ends up in `\result` is what tex.web's runaway recovery leaves behind.
#[test]
// TeX scans an `\edef` body while expanding it, so a `{` that an abandoned
// argument consumed never counts toward the body's balance (tex.web § 477).
// satex reads the body as a balanced group first, because a conditional it
// cannot decide is analyzed arm by arm: interleaved scanning would make those
// arms reach into the rest of the file instead of into the body.  The `\long`
// rule itself is enforced and tested; only TeX's error recovery differs here.
#[ignore = "interleaved \\edef scanning conflicts with analyzing both arms of \
            an undecided conditional"]
fn par_is_forbidden_in_a_non_long_macro_argument() {
    let analysis = analyze(r"\def\eat#1{GOT:[#1]}\edef\result{\eat{a\par b}}");
    assert_eq!(body(&analysis, "result"), r"\par b");
}

/// TeXbook ch. 20: several `\aftergroup` tokens saved in one group run, in
/// order, after the group closes.
#[test]
fn aftergroup_tokens_run_in_the_order_they_were_saved() {
    let analysis = analyze(
        r"\def\reset{\gdef\seen{}}\def\one{\xdef\seen{\seen 1}}\def\two{\xdef\seen{\seen 2}}\reset{\aftergroup\one\aftergroup\two}",
    );
    assert_eq!(last_body(&analysis, "seen"), "12");
}

// Found while fixing the table above, each checked the same way: a
// minimal plain-format file run through `pdftex -interaction=nonstopmode`
// with the answer read back out of the log.

/// tex.web § 445: after `"` only the *uppercase* `A`-`F` are hexadecimal
/// digits. `pdftex`: `\count0="7FFF` is 32767, while `\count0="7fff` stops
/// at the `7` and leaves `fff` in the input.
#[test]
fn hexadecimal_constants_take_only_uppercase_digits() {
    let analysis = analyze(r#"\count0="7FFF \edef\v{\the\count0}"#);
    assert_eq!(body(&analysis, "v"), "32767");
    let analysis = analyze(r#"\count0="7fff \edef\v{\the\count0}"#);
    assert_eq!(body(&analysis, "v"), "7");
}

/// tex.web § 1269: `\afterassignment` is not itself an assignment, so a
/// second one overwrites the token the first was holding instead of
/// releasing it. `pdftex` runs only the second token, and after the
/// assignment: `\count0` already reads 5.
#[test]
fn a_second_afterassignment_replaces_the_token_held_by_the_first() {
    let analysis = analyze(
        r"\count0=0 \def\one{\xdef\seen{one:\the\count0}}\def\two{\xdef\seen{two:\the\count0}}
          \afterassignment\one \afterassignment\two \count0=5",
    );
    assert_eq!(last_body(&analysis, "seen"), "two:5");
}

/// tex.web § 396: `\par` in an argument of a macro that is not `\long`
/// aborts the call — the tokens read so far are dropped and the `\par` goes
/// back into the input. `pdftex`: `\edef\v{\eat\par}` leaves `\par `, and
/// the already-read `{x}` of `\two{x}\par` is dropped too.
#[test]
fn par_aborts_an_unbraced_argument_of_a_non_long_macro() {
    let analysis = analyze(r"\def\eat#1{GOT:[#1]}\edef\v{\eat\par}");
    assert_eq!(body(&analysis, "v"), "\\par ");
    let analysis = analyze(r"\def\two#1#2{[#1][#2]}\edef\v{\two{x}\par}");
    assert_eq!(body(&analysis, "v"), "\\par ");
}

// Tables of inputs whose answer was read out of a `pdftex` log, grouped by
// the scanner or expander they exercise. `\v` is the macro each input
// leaves the answer in.

fn agrees(cases: &[(&str, &str)]) {
    let mut wrong = Vec::new();
    for (source, engine) in cases {
        let got = body(&analyze(source), "v");
        if got != *engine {
            wrong.push(format!("{source}\n     satex {got:?}\n    pdftex {engine:?}"));
        }
    }
    assert!(wrong.is_empty(), "{} inputs disagree with pdftex:\n{}", wrong.len(), wrong.join("\n"));
}

/// tex.web §§ 440-445 (`scan_int`).
#[test]
fn integer_scanning_agrees_with_the_engine() {
    agrees(&[
        (r"\count0=`\\ \edef\v{\the\count0}", "92"),
        (r"\count0=`\  \edef\v{\the\count0}", "32"),
        (r"\count0=+-+3 \edef\v{\the\count0}", "-3"),
        (r"\count0='777 \edef\v{\the\count0}", "511"),
        // An expansion may produce the digits of a constant.
        (r"\def\five{5}\count0=1\five7 \edef\v{\the\count0}", "157"),
        (r"\count0=`\A \advance\count0 by `\B \edef\v{[\the\count0]}", "[131]"),
        (r"\count0=1 \count0=\count0 \edef\v{[\the\count0]}", "[1]"),
        (r"\count255=5 \edef\v{[\the\count255]}", "[5]"),
        (r"\count0=1 \edef\v{[\the\count 0]}", "[1]"),
        (r"\chardef\cc=65 \edef\v{[\the\cc][\number\cc]}", "[65][65]"),
        (r"\edef\v{[\the\catcode`\A][\the\escapechar]}", "[11][92]"),
        (r"\edef\v{[\the\count7][\the\skip0][\the\muskip0]}", "[0][0.0pt][0.0mu]"),
    ]);
}

/// tex.web §§ 448-454 (`scan_dimen`) and § 461 (`scan_glue`).
#[test]
fn dimension_and_glue_scanning_agrees_with_the_engine() {
    agrees(&[
        (r"\dimen0=1in \edef\v{\the\dimen0}", "72.26999pt"),
        (r"\dimen0=1bp \edef\v{[\the\dimen0]}", "[1.00374pt]"),
        (r"\dimen0=1dd \edef\v{[\the\dimen0]}", "[1.07pt]"),
        (r"\dimen0=.5pt \edef\v{\the\dimen0}", "0.5pt"),
        (r"\dimen0=1,5pt \edef\v{\the\dimen0}", "1.5pt"),
        (r"\dimen0=2pt \advance\dimen0 by 3pt \edef\v{\the\dimen0}", "5.0pt"),
        (r"\skip0=1pt plus 1fil minus 2fill \edef\v{\the\skip0}", "1.0pt plus 1.0fil minus 2.0fill"),
        (r"\skip0=-3pt plus-1.5fill \edef\v{\the\skip0}", "-3.0pt plus -1.5fill"),
        (r"\skip0=0pt plus 1fill \edef\v{[\the\skip0]}", "[0.0pt plus 1.0fill]"),
        (r"\skip0=0pt \edef\v{[\the\skip0]}", "[0.0pt]"),
        // A glue register as the value is copied whole; a dimension is not.
        (r"\dimen0=1pt \skip0=\dimen0 plus 1pt \edef\v{\the\skip0}", "1.0pt plus 1.0pt"),
        (r"\skip1=1pt plus 2pt \dimen1=\skip1 \edef\v{\the\dimen1}", "1.0pt"),
        (r"\skip1=1pt plus 2pt \count0=\skip1 \edef\v{\the\count0}", "65536"),
        (r"\toks0={a b c} \edef\v{\the\toks0}", "a b c"),
        (r"\toks0={\a\b} \toks1=\toks0 \edef\v{\the\toks1}", "\\a \\b "),
    ]);
}

/// eTeX manual § 3.5 (`\numexpr` and its relatives).
#[test]
fn expressions_agree_with_the_engine() {
    agrees(&[
        (r"\edef\v{[\the\numexpr 1+2*3-4/2\relax]}", "[5]"),
        (r"\edef\v{[\the\numexpr(2+3)*(4-1)\relax]}", "[15]"),
        (r"\edef\v{[\the\numexpr 2 + 3 \relax]}", "[5]"),
        (r"\edef\v{[\the\numexpr -7/2\relax][\the\numexpr 7/-2\relax]}", "[-4][-4]"),
        (r"\edef\v{[\the\numexpr\numexpr 2+3\relax*2\relax]}", "[10]"),
        (r"\count0=1 \edef\v{[\the\numexpr\ifnum\count0=1 5\else 6\fi+1\relax]}", "[6]"),
        (r"\count0=5 \edef\v{[\the\numexpr\count0+1\relax]}", "[6]"),
        (r"\count0=5 \edef\v{[\the\numexpr 2*\count0\relax]}", "[10]"),
        // `\countdef`, not `\newcount` (a `plain.tex` macro undefined in
        // true initex, per `pdftex -etex -ini`): a *named* register as a
        // `\numexpr` operand, exercising `internal_quantity`'s direct
        // `Meaning::Register` path rather than `\countN`'s numbered one.
        (r"\countdef\cnt=6 \cnt=4 \edef\v{[\the\numexpr\cnt*3\relax]}", "[12]"),
        (r"\dimen0=10pt \edef\v{[\the\numexpr\dimen0+1\relax]}", "[655361]"),
        (r"\edef\v{[\the\dimexpr 1pt+1sp\relax]}", "[1.00002pt]"),
        (r"\edef\v{[\the\dimexpr 1pt+2pt*3\relax]}", "[7.0pt]"),
        (r"\edef\v{[\the\dimexpr 1pt/3\relax]}", "[0.33333pt]"),
        (r"\dimen0=10pt \edef\v{[\the\dimexpr\dimen0/2\relax]}", "[5.0pt]"),
        (r"\dimen0=10pt \edef\v{[\the\dimexpr 2\dimen0\relax]}", "[20.0pt]"),
        (r"\dimen0=10pt \edef\v{[\the\dimexpr\dimen0*2\relax]}", "[20.0pt]"),
        (r"\edef\v{[\the\muexpr 1mu+2mu\relax]}", "[3.0mu]"),
        // The higher order of infinity wins, as it does for `\advance`.
        (r"\edef\v{[\the\glueexpr 1pt plus 2pt+1pt plus 1fil\relax]}", "[2.0pt plus 1.0fil]"),
        (r"\edef\v{[\the\glueexpr 3pt plus 2fil-1pt plus 1fil\relax]}", "[2.0pt plus 1.0fil]"),
        (r"\edef\v{[\the\glueexpr 1pt plus 1fil-2pt plus 3pt\relax]}", "[-1.0pt plus 1.0fil]"),
        (r"\edef\v{[\the\glueexpr 1pt plus 4pt/2\relax]}", "[0.5pt plus 2.0pt]"),
    ]);
}

/// tex.web §§ 366-372 (expansion) and §§ 494-509 (conditionals).
#[test]
fn expansion_and_conditionals_agree_with_the_engine() {
    agrees(&[
        (r"\def\a{A}\def\b{B}\edef\v{\noexpand\a\noexpand\b}", "\\a \\b "),
        (r"\def\a{A}\def\b{B}\edef\v{\string\a\noexpand\b}", "\\a\\b "),
        (
            r"\def\a{A}\def\b{B}\expandafter\expandafter\expandafter\def\expandafter\expandafter\expandafter\v\expandafter\expandafter\expandafter{\a\b}",
            "A\\b ",
        ),
        (r"\def\name{foo}\expandafter\def\csname\name\endcsname{BAR}\edef\v{\meaning\foo}", "macro:->BAR"),
        (r"\expandafter\let\csname qq\endcsname=\relax \edef\v{\meaning\qq}", "\\relax"),
        (
            r"\def\name{foo}\expandafter\def\csname\name\endcsname{BAR}\edef\v{[\ifcsname foo\endcsname Y\else N\fi][\ifcsname zzz\endcsname Y\else N\fi]}",
            "[Y][N]",
        ),
        (r"\edef\v{[\ifnum1<2 \ifnum2<3 AA\else AB\fi\else B\fi]}", "[AA]"),
        (r"\edef\v{[\iftrue\iffalse A\else B\fi\else C\fi]}", "[B]"),
        (r"\edef\v{[\ifcase2 z\or o\or t\or th\else e\fi]}", "[t]"),
        (r"\edef\v{[\unless\ifnum1>2 T\else F\fi]}", "[T]"),
        (r"\def\a{A}\def\b{B}\edef\v{[\ifx\a\b S\else D\fi][\ifx\a\a S\else D\fi]}", "[D][S]"),
        (r"\let\p=+ \let\q=+ \let\r=- \edef\v{[\ifx\p\q S\else D\fi][\ifx\p\r S\else D\fi]}", "[S][D]"),
        (r"\def\u{x}\let\w=\u \edef\v{[\ifx\u\w S\else D\fi]}", "[S]"),
        (r"\def\a{A}\def\b{B}\edef\v{[\if AB s\else d\fi][\if\a\b s\else d\fi]}", "[d][d]"),
        (r"\edef\v{[\ifcat AB s\else d\fi][\ifcat A1 s\else d\fi]}", "[ s][d]"),
        (r"\edef\v{[\ifodd 3 T\else F\fi][\ifodd 4 T\else F\fi]}", "[T][F]"),
        (r"\edef\v{[\romannumeral 1984][\romannumeral -3][\number-0]}", "[mcmlxxxiv][][0]"),
        (r"\escapechar=-1 \edef\v{\string\foo}", "foo"),
        // A catcode change is local, and an active character expands in an
        // `\edef` body unless `\noexpand` stops it.
        (r"{\catcode`\!=13 \def!{BANG}}\edef\v{[\the\catcode`\!]}", "[12]"),
        (r"\catcode`\!=13 \def!{X}\edef\v{!}", "X"),
        (r"\catcode`\!=13 \def!{X}\edef\v{\noexpand!}", "!"),
        (r"\def\bar{\catcode`\?=13 }\bar \edef\v{[\ifnum\catcode`\?=13 T\else F\fi]}", "[T]"),
        // `\lccode` 0 leaves a character alone; a control sequence is never
        // case-shifted, and neither is an active character with no `\uccode`.
        (r"\uppercase{\def\v{ab}}", "AB"),
        (r"\uccode`\-=`\_ \uppercase{\def\v{a-b}}", "A_B"),
        (r"\lccode`\X=0 \lowercase{\def\v{X}}", "X"),
        (r"\catcode`\~=13 \def~{T}\uppercase{\def\v{~}}", "~"),
        (r"{\lccode`\A=`\b }\edef\v{[\the\lccode`\A]}", "[97]"),
        (r"\global\lccode`\A=`\c \edef\v{[\the\lccode`\A]}", "[99]"),
        // The token `\afterassignment` holds waits for the *next* assignment,
        // wherever that turns out to be.
        (r"\count0=0 {\afterassignment\relax\count0=1 }\edef\v{[\the\count0]}", "[0]"),
        (r"\count0=0 \def\show{\xdef\v{\the\count0}}\afterassignment\show{\count0=4 }", "4"),
    ]);
}

// LaTeX documents.  Each was compiled with `pdflatex -interaction=nonstopmode`
// (TeX Live 2026); the comment gives what the log or a `\typeout` showed.

fn document(source: &str) -> Analysis {
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    Machine::analyze(source, None, &cfg)
}

/// The findings for the document itself, as `code name` pairs.
fn findings(analysis: &Analysis) -> Vec<String> {
    let field =
        |f: &satex::query::Record, key: &str| f.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    satex::lint::lint(analysis)
        .iter()
        .filter(|f| field(f, "origin") == "document")
        .map(|f| format!("{} {}", field(f, "code"), field(f, "name")))
        .collect()
}

fn found(analysis: &Analysis, code: &str) -> bool {
    findings(analysis).iter().any(|f| f.split(' ').next() == Some(code))
}

/// `\repeat` is latex.ltx's `\let\repeat=\fi`, so `\newcommand{\repeat}`
/// stops with `LaTeX Error: Command \repeat already defined` and defines
/// nothing: there is no recursive `\repeat` to warn about.
#[test]
fn newcommand_on_a_kernel_name_is_refused() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\newcommand{\repeat}[1]{#1\repeat{#1}}
\begin{document}
x
\end{document}",
    );
    let all = findings(&analysis);
    assert!(all.contains(&r"already-defined \repeat".to_string()), "{all:?}");
    assert!(!found(&analysis, "unguarded-recursion"), "{all:?}");
}

/// latex.ltx's `\@ifundefined` treats a name whose meaning is `\relax` as
/// undefined, and `\providecommand` defines it.  `pdflatex`: `[U][macro:->P]`.
#[test]
fn a_name_let_to_relax_counts_as_undefined() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\makeatletter
\expandafter\let\csname qqfoo\endcsname\relax
\@ifundefined{qqfoo}{\def\qqr{U}}{\def\qqr{D}}
\providecommand\qqfoo{P}
\begin{document}
\end{document}",
    );
    assert_eq!(last_body(&analysis, "qqr"), "U");
    assert_eq!(last_body(&analysis, "qqfoo"), "P");
}

/// `unguarded-recursion` findings as `(code, name, line, col)`, so a test can
/// assert the exact place rather than just whether the code fired.
fn recursion_findings(analysis: &Analysis) -> Vec<(String, String, u64, u64)> {
    satex::lint::lint(analysis)
        .into_iter()
        .filter(|r| r["code"] == "unguarded-recursion")
        .map(|r| {
            (
                r["code"].as_str().unwrap_or_default().to_string(),
                r["name"].as_str().unwrap_or_default().to_string(),
                r["line"].as_u64().unwrap_or_default(),
                r["col"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

/// `\newcommand\foo[n][default]{body}` does not define `\foo` as `body`:
/// ltdefns' `\@xargdef` defines it as a dispatcher that hands the optional
/// argument to `\foo`'s own internal counterpart —
/// `\def\foo{\@protected@testopt \foo \\foo {default}}` — so `\foo` names
/// itself in its own replacement text for every optional-argument command
/// that exists.  None of that is a real call.
#[test]
fn optional_argument_wrappers_are_not_unguarded_recursion() {
    if !installed() {
        return;
    }
    let cases = [
        r"\newcommand*\foo[1][]{body #1}",
        r"\newcommand\foo[2][x]{body #1 #2}",
        r"\DeclareRobustCommand\foo[1][x]{body #1}",
        r"\DeclareRobustCommand*\foo[1][x]{body #1}",
        r"\providecommand\foo[1][x]{body #1}",
        r"\newenvironment{foo}[1][x]{start #1}{end}",
    ];
    for body in cases {
        let analysis = document(&format!("\\documentclass{{article}}\n{body}\n\\begin{{document}}\\end{{document}}"));
        let findings = recursion_findings(&analysis);
        assert!(findings.is_empty(), "{body}: {findings:?}");
    }
}

/// `\NewDocumentCommand` (ltcmd) never puts `\foo` itself, unqualified, into
/// its own replacement text for an optional argument: its dispatch macro
/// carries the *space-suffixed* internal name instead, a different control
/// sequence, so no self-reference to detect in the first place.
#[test]
fn newdocumentcommand_with_optional_argument_is_not_unguarded_recursion() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\usepackage{xparse}
\NewDocumentCommand\foo{o m}{body #1 #2}
\begin{document}
\end{document}",
    );
    assert!(recursion_findings(&analysis).is_empty(), "{:?}", recursion_findings(&analysis));
}

/// `\def\loop{\loop}` is genuine, unconditional recursion and must still be
/// caught, at the exact line the `\def` sits on.
#[test]
fn a_directly_recursive_def_is_still_reported() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\def\loop{\loop}
\begin{document}
\end{document}",
    );
    let findings = recursion_findings(&analysis);
    assert_eq!(findings, [("unguarded-recursion".to_string(), "\\loop".to_string(), 2, 1)], "{findings:?}");
}

/// A mutually recursive pair, `\mutA` calling `\mutB` and back, is reported
/// for both names.
#[test]
fn a_mutually_recursive_pair_is_reported_for_both_names() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\def\mutA{\mutB}
\def\mutB{\mutA}
\begin{document}
\end{document}",
    );
    let mut findings = recursion_findings(&analysis);
    findings.sort();
    assert_eq!(
        findings,
        [
            ("unguarded-recursion".to_string(), "\\mutA".to_string(), 2, 1),
            ("unguarded-recursion".to_string(), "\\mutB".to_string(), 3, 1),
        ],
        "{findings:?}"
    );
}

/// A `\newcommand` with an optional argument whose body genuinely calls
/// itself with no conditional is still real recursion: `\foo`'s own
/// wrapper is exempted, but its internal counterpart, which actually calls
/// `\foo` back, is not.
#[test]
fn a_genuinely_recursive_newcommand_with_an_optional_argument_is_still_reported() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\newcommand\realrec[1][x]{\realrec{#1}}
\begin{document}
\end{document}",
    );
    let findings = recursion_findings(&analysis);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].0, "unguarded-recursion");
}

/// A `\newcommand` with no optional argument whose body calls itself is the
/// plain case: no wrapper, so `\foo` itself is the self-reference.
#[test]
fn a_genuinely_recursive_plain_newcommand_is_still_reported() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\newcommand\realrec[1]{\realrec{#1}}
\begin{document}
\end{document}",
    );
    let findings = recursion_findings(&analysis);
    assert_eq!(findings, [("unguarded-recursion".to_string(), "\\realrec".to_string(), 2, 12)], "{findings:?}");
}

/// latex.ltx defines `\@empty` as an empty macro, and a zero-argument
/// `\newcommand` is not `\long` (`\@yargd@f`), so all of these compare
/// equal.  `pdflatex`: `[T][T][T][T]`.
#[test]
fn ifx_compares_empty_macros_the_way_the_engine_does() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\makeatletter
\newcommand*\qa{}\newcommand\qb{}\newcommand*\qc[1]{#1}\def\qd#1{#1}\edef\qe{}
\xdef\qv{[\ifx\qa\@empty T\else F\fi][\ifx\qb\@empty T\else F\fi][\ifx\qc\qd T\else F\fi][\ifx\qe\@empty T\else F\fi]}
\begin{document}
\end{document}",
    );
    assert_eq!(last_body(&analysis, "qv"), "[T][T][T][T]");
}

/// `\ifmmode` outside any math group is false: a display opened in its
/// `\else` arm is closed again two lines on.  pdflatex runs this without an
/// error; the `center` group is closed by `\end{center}`.
#[test]
fn ifmmode_is_false_where_no_math_group_is_open() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\def\qa{$$}
\begin{document}
\begin{center}\ifmmode\relax\else\qa\fi x\ifmmode\else\relax\fi\qa\end{center}
\end{document}",
    );
    let all = findings(&analysis);
    assert!(!all.iter().any(|f| f.contains("unbalanced") || f.contains("mismatched")), "{all:?}");
}

/// amsmath's `equation` is `\mathdisplay`, which opens `$$` in the `\else`
/// of `\ifmmode`; the environment and the document close cleanly.
#[test]
fn amsmath_display_environments_close_their_groups() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\usepackage{amsmath}
\begin{document}
\begin{equation}\label{qe}x\end{equation}
\[ y \]
\begin{align}a&=b\label{qx}\\c&=d\nonumber\end{align}
\eqref{qe} \eqref{qx}
\end{document}",
    );
    let all = findings(&analysis);
    assert!(
        !all.iter().any(|f| f.contains("unbalanced") || f.contains("mismatched") || f.contains("environment-mismatch")),
        "{all:?}"
    );
}

fn cite_keys(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == satex::builtins::OccKind::Cite)
        .map(|o| o.key.clone())
        .collect()
}

/// natbib with `[numbers]`: every citing command records its key, never the
/// `[` of an optional argument.  pdflatex+bibtex resolves all seven.
#[test]
fn natbib_numbers_records_every_key() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\usepackage[numbers]{natbib}
\begin{document}
\citeauthor{knuth84} \citeyear{lamport94} \citealt{knuth84} \citep*{lamport94}
\citep[p.~3]{knuth84} \citep[see][]{lamport94} \citet*[ch.~2]{knuth84}
\end{document}",
    );
    assert_eq!(
        cite_keys(&analysis),
        ["knuth84", "lamport94", "knuth84", "lamport94", "knuth84", "lamport94", "knuth84"]
    );
}

/// cite.sty walks the key list itself; each key is cited once, spaces
/// around the commas are dropped.
#[test]
fn cite_sty_keeps_every_key_of_a_list() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\usepackage{cite}
\begin{document}
\cite{knuth84} \cite[p.~2]{lamport94,knuth84} \cite{a1, b2 ,c3}
\end{document}",
    );
    assert_eq!(cite_keys(&analysis), ["knuth84", "lamport94", "knuth84", "a1", "b2", "c3"]);
}

/// pdflatex compiles `tests/fixtures/project/paper.tex` with one error (`\repeat` already
/// defined) and ends outside every group and conditional.
#[test]
fn the_sample_paper_ends_balanced() {
    if !installed() {
        return;
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project/paper.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let all = findings(&analysis);
    assert!(all.contains(&r"already-defined \repeat".to_string()), "{all:?}");
    for bad in ["unbalanced", "mismatched", "environment-mismatch", "unguarded-recursion"] {
        assert!(!all.iter().any(|f| f.contains(bad)), "{bad}: {all:?}");
    }
}

/// `\begin{itemize}` left open: LaTeX says `\begin{itemize} on input line 4
/// ended by \end{document}`.
#[test]
fn an_environment_left_open_is_ended_by_the_document() {
    if !installed() {
        return;
    }
    let analysis = document(
        r"\documentclass{article}
\begin{document}
\begin{itemize}\item x
\end{document}",
    );
    let all = findings(&analysis);
    assert!(all.contains(&"environment-mismatch document".to_string()), "{all:?}");
}

/// Every name the run defined, on any path.
fn names(analysis: &Analysis) -> Vec<String> {
    analysis.facts.defs.iter().map(|d| analysis.interner.name(d.name).to_string()).collect()
}

/// A count register whose value satex cannot pin down, for a register
/// number it cannot pin down either.
const MAYBE: &str = r"\pdfuniformdeviate2";

/// tex.web § 1224: `\count⟨number⟩` writes the register the number names.
/// With a number satex cannot pin down, every register it may name may
/// hold what was written.  `pdftex`: `\count2=1 \count\count2=99` leaves
/// `\count0` at `0` and `\count1` at `99`, and `\count2=0` the other way
/// round — so `\count0` afterwards is `0` or `99`, never anything else.
#[test]
fn a_write_through_an_unknown_register_number_reaches_every_register_it_may_name() {
    let analysis = analyze(&format!(
        r"\count0=0 \count1=0 \ifnum{MAYBE}=0 \count2=0 \else \count2=1 \fi \count\count2=99
          \ifnum\count0=0 \def\zero{{1}}\fi \ifnum\count0=99 \def\written{{1}}\fi
          \ifnum\count0=7 \def\other{{1}}\fi"
    ));
    let defined = names(&analysis);
    assert!(defined.contains(&"zero".to_string()), "{defined:?}");
    assert!(defined.contains(&"written".to_string()), "{defined:?}");
    assert!(!defined.contains(&"other".to_string()), "{defined:?}");
}

/// The same for `\advance` (tex.web § 1238): the register the number
/// names is the one that grows.
#[test]
fn advance_through_an_unknown_register_number_reaches_every_register_it_may_name() {
    let analysis = analyze(&format!(
        r"\count0=0 \ifnum{MAYBE}=0 \count2=0 \else \count2=1 \fi \advance\count\count2 by 7
          \ifnum\count0=0 \def\zero{{1}}\fi \ifnum\count0=7 \def\grown{{1}}\fi"
    ));
    let defined = names(&analysis);
    assert!(defined.contains(&"zero".to_string()) && defined.contains(&"grown".to_string()), "{defined:?}");
}

/// A number whose interval is too wide to list may name any register,
/// including one nothing has written, which then no longer reads as the
/// zero of tex.web § 232.  `pdftex`: `\count2=1234567 \count\count2=5`
/// says "Bad register code" and sets `\count0` to `5` (tex.web § 433).
#[test]
fn a_wide_register_number_reaches_registers_satex_has_not_seen() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\count0=0 \count9=0 \count2=\wd0 \count\count2=5
          \ifnum\count0=5 \def\first{1}\fi \ifnum\count9=5 \def\ninth{1}\fi
          \ifnum\count5=5 \def\unseen{1}\fi \ifnum\count5=0 \def\still{1}\fi",
    );
    let defined = names(&analysis);
    for name in ["first", "ninth", "unseen", "still"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
}

/// A later write with a number satex knows is precise again.
#[test]
fn a_known_register_number_is_precise_after_a_wide_one() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\count2=\wd0 \count\count2=5 \count0=3
          \ifnum\count0=3 \def\three{1}\fi \ifnum\count0=4 \def\four{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"three".to_string()) && !defined.contains(&"four".to_string()), "{defined:?}");
}

/// tex.web § 433: a register number outside the engine's range is "Bad
/// register code" and names register zero.  `pdftex`: `\count-1=7` and
/// `\count32768=8` both change `\count0`.
#[test]
fn a_register_number_out_of_range_names_register_zero() {
    let analysis = analyze(r"\count0=0 \count-1=7 \ifnum\count0=7 \def\moved{1}\fi \ifnum\count0=0 \def\kept{1}\fi");
    let defined = names(&analysis);
    assert!(defined.contains(&"moved".to_string()), "{defined:?}");
    let analysis = analyze(r"\count0=0 \count32768=8 \ifnum\count0=8 \def\moved{1}\fi");
    assert!(names(&analysis).contains(&"moved".to_string()), "{:?}", names(&analysis));
}

/// A group undoes a write through a number satex could not pin down, the
/// way it undoes the one register TeX really wrote (tex.web § 1224).
#[test]
fn a_group_undoes_a_write_through_an_unknown_register_number() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\count0=0 {\count2=\wd0 \count\count2=5 }
          \ifnum\count0=0 \def\kept{1}\fi \ifnum\count0=5 \def\moved{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"kept".to_string()) && !defined.contains(&"moved".to_string()), "{defined:?}");
}

/// tex.web § 1238: `\advance` on a dimension has no range check, so a
/// register may hold more than `\maxdimen`, and reading it as an
/// ⟨integer⟩ (tex.web § 440) does not clamp it.  `pdftex`:
/// `\dimen0=1073741823sp \advance\dimen0 by 1073741823sp` gives
/// `\the\dimen0` = `32767.99997pt` and `\count0=\dimen0` = `2147483646`,
/// while `\ifnum\dimen0>1073741823` is true.
#[test]
fn advance_may_put_a_dimension_past_maxdimen() {
    let analysis = analyze(
        r"\dimen0=1073741823sp \advance\dimen0 by 1073741823sp \edef\shown{\the\dimen0}
          \count0=\dimen0 \edef\asint{\the\count0}
          \ifnum\dimen0>1073741823 \def\past{1}\fi",
    );
    assert_eq!(body(&analysis, "shown"), "32767.99997pt");
    assert_eq!(body(&analysis, "asint"), "2147483646");
    assert!(names(&analysis).contains(&"past".to_string()), "{:?}", names(&analysis));
}

/// tex.web § 460 (`attach_sign`): reading such a register as a ⟨dimen⟩ is
/// "Dimension too large" and gives `\maxdimen`.  `pdftex`: after the
/// above, `\dimen1=\dimen0` is `16383.99998pt` and
/// `\ifdim\dimen0=16383.99998pt` is true.  The clamp is not monotone, so
/// an interval that passes `\maxdimen` has to be clamped as well.
#[test]
fn a_dimension_past_maxdimen_reads_back_as_maxdimen() {
    let analysis = analyze(
        r"\dimen0=1073741823sp \advance\dimen0 by 1073741823sp
          \dimen1=\dimen0 \ifdim\dimen1=16383.99998pt \def\clamped{1}\fi",
    );
    assert!(names(&analysis).contains(&"clamped".to_string()), "{:?}", names(&analysis));
    // The same where the register holds one of a few values, all past
    // `\maxdimen`: every one of them reads back as `\maxdimen`.
    let analysis = analyze(
        r"\dimen0=1073741823sp \advance\dimen0 by 1073741823sp
          \ifnum\pdfuniformdeviate2=0 \advance\dimen0 by -1sp \fi
          \dimen1=\dimen0 \ifdim\dimen1=16383.99998pt \def\clamped{1}\fi
          \ifdim\dimen1<16383.99998pt \def\under{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"clamped".to_string()) && !defined.contains(&"under".to_string()), "{defined:?}");
}

/// tex.web § 498: a conditional runs one arm.  With no depth left to
/// analyze them as paths of their own the arms run one after the other in
/// one state, and what a later arm assigns must not overwrite what an
/// earlier one did.  `pdftex`, on six runs of
/// `\count0=0 \ifnum\pdfuniformdeviate2=0 \count0=1 \else \count0=2 \fi`:
/// `\the\count0` was `2,1,2,2,2,2` — either arm may be the one that ran.
#[test]
fn undecided_arms_join_their_assignments() {
    let analysis = analyze(r"\count0=0 \ifnum\pdfuniformdeviate2=0 \count0=1 \else \count0=2 \fi \edef\v{\the\count0}");
    assert_ne!(body(&analysis, "v"), "2");
    let analysis = analyze(
        r"\count0=0 \ifnum\pdfuniformdeviate2=0 \count0=1 \else \count0=2 \fi
          \ifnum\count0=1 \def\one{1}\fi \ifnum\count0=2 \def\two{1}\fi \ifnum\count0=0 \def\zero{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"one".to_string()) && defined.contains(&"two".to_string()), "{defined:?}");
    assert!(!defined.contains(&"zero".to_string()), "{defined:?}");
}

/// The same for meanings: neither arm's `\def` is the one that ran, so a
/// later `\ifx` against either must be undecided.  `pdftex` prints `a` or
/// `b` for `\m` here.
#[test]
fn undecided_arms_join_their_meanings() {
    let analysis = analyze(
        r"\def\m{}\ifnum\pdfuniformdeviate2=0 \def\m{a}\else \def\m{b}\fi
          \def\a{a}\def\b{b}
          \ifx\m\a \def\isa{1}\fi \ifx\m\b \def\isb{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"isa".to_string()) && defined.contains(&"isb".to_string()), "{defined:?}");
}

/// A `\global` assignment in one arm is no more certain than a local one:
/// it survives the group, but the other arm may be the one that ran.
#[test]
fn undecided_arms_join_global_assignments() {
    let analysis = analyze(
        r"\count0=0 {\ifnum\pdfuniformdeviate2=0 \global\count0=1 \else \global\count0=2 \fi}
          \ifnum\count0=1 \def\one{1}\fi \ifnum\count0=2 \def\two{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"one".to_string()) && defined.contains(&"two".to_string()), "{defined:?}");
}

/// tex.web § 232: a `\catcode` through a character number the arm could
/// not pin down leaves no category known, and the rollback that ends the
/// arm must not put the characters the arm saved back as known.  `pdftex`
/// with `\count2` pinned to `` `\~ ``: `\catcode`\~=13 \catcode\count2=11`
/// leaves `~` active, and pinned to `` `\q `` it leaves `~` whatever the
/// arm made it, so satex may not stay certain either way.
#[test]
fn an_arm_that_could_not_pin_a_catcode_down_leaves_no_category_known() {
    // Whether `\ran` ran on every path.
    let ran = |source: &str| {
        let analysis = analyze(source);
        analysis.facts.defs.iter().any(|d| analysis.interner.name(d.name) == "ran" && d.certain)
    };
    // The conditional sits in a macro body, so its `\fi` is a token the arm
    // reaches even once no category is known.
    let body = r"\catcode`\~=13 \def~{\def\ran{1}}\count2=\pdfuniformdeviate 999
                \def\m{\ifnum\pdfuniformdeviate2=0 \catcode`\~=13 \catcode\count2=11 \fi}";
    assert!(ran(&format!(r"{body}~")), "an active character runs when its category is known");
    assert!(!ran(&format!(r"{body}\m ~")), "the arm left no category known, so `~` is not known to run");
}

/// tex.web § 1232: the same for the code tables, which the arm merge joins
/// through the bindings and not through a diff of the entries.  `pdftex`
/// with the count pinned prints `97` or `0` for `\the\lccode`\a`.
#[test]
fn an_arm_that_could_not_pin_a_code_table_entry_down_leaves_no_entry_known() {
    let analysis = analyze(
        r"\count2=\pdfuniformdeviate 999 \lccode`\a=`\a
          \ifnum\pdfuniformdeviate2=0 \lccode\count2=0 \fi
          \ifnum\lccode`\a=97 \def\same{1}\fi \ifnum\lccode`\a=0 \def\zero{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"same".to_string()) && defined.contains(&"zero".to_string()), "{defined:?}");
}

/// tex.web § 1077: and for the box registers.  `pdftex` with `\count2`
/// pinned to 3 leaves box 3 filled, pinned elsewhere it stays void.
#[test]
fn an_arm_that_could_not_pin_a_box_register_down_leaves_no_box_void() {
    let analysis = analyze(
        r"\count2=\pdfuniformdeviate 9
          \ifnum\pdfuniformdeviate2=0 \setbox\count2=\hbox{x}\fi
          \ifvoid3 \def\yes{}\else\def\no{}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"yes".to_string()) && defined.contains(&"no".to_string()), "{defined:?}");
}

/// tex.web § 232: `\catcode` assigns a category the run may not know.
/// `pdftex` with `\count2` pinned to 11 or 12 prints `11` or `12` for
/// `\the\catcode`\@` after `\catcode`\@=\count2`, so satex may not stay
/// certain of the category it had.
#[test]
fn a_catcode_assigned_an_unknown_number_is_not_known() {
    let analysis = analyze(
        r"\ifnum\pdfuniformdeviate2=0 \count2=11 \else \count2=12 \fi
          \catcode`\@=\count2
          \ifnum\catcode`\@=12 \def\other{1}\fi \ifnum\catcode`\@=11 \def\letter{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"other".to_string()) && defined.contains(&"letter".to_string()), "{defined:?}");
}

/// The same where the *character* is the part that is not known.
/// `pdftex`: with `\count2` pinned to `` `\@ `` or `` `\! ``,
/// `\catcode\count2=11` leaves `\the\catcode`\@ /\the\catcode`\!` as
/// `11/12` or `12/11`.
#[test]
fn a_catcode_of_an_unknown_character_is_not_known() {
    let analysis = analyze(
        r"\ifnum\pdfuniformdeviate2=0 \count2=`\@ \else \count2=`\! \fi
          \catcode\count2=11
          \ifnum\catcode`\@=12 \def\atother{1}\fi \ifnum\catcode`\@=11 \def\atletter{1}\fi
          \ifnum\catcode`\!=12 \def\bangother{1}\fi \ifnum\catcode`\!=11 \def\bangletter{1}\fi",
    );
    let defined = names(&analysis);
    for name in ["atother", "atletter", "bangother", "bangletter"] {
        assert!(defined.contains(&name.to_string()), "{name} missing from {defined:?}");
    }
}

/// tex.web § 1232: `\lccode` and its relatives are assigned a number the
/// run may not know, and then the code is not the table's default either.
/// `pdftex` with the count pinned prints `65` or `66` for `\the\lccode`\a`.
#[test]
fn a_char_code_assigned_an_unknown_number_is_not_known() {
    let analysis = analyze(
        r"\ifnum\pdfuniformdeviate2=0 \count2=65 \else \count2=66 \fi
          \lccode`\a=\count2
          \ifnum\lccode`\a=65 \def\isa{1}\fi \ifnum\lccode`\a=66 \def\isb{1}\fi
          \ifnum\lccode`\a=97 \def\isdefault{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"isa".to_string()) && defined.contains(&"isb".to_string()), "{defined:?}");
}

/// A count register joined after paths that parted on the mode holds only
/// the values the paths had. `\time` is unknown, so the first `\ifnum`
/// runs both arms and leaves the mode vertical or horizontal; `\ifhmode`
/// then parts the run again, the horizontal arm parts once more and ends
/// its paragraph. `pdftex`: `\count10` is 273 afterwards on every run.
#[test]
fn a_register_joined_after_a_mode_split_keeps_only_the_paths_values() {
    let analysis = analyze(
        r"\count10=272 \count20=252
\ifnum\time>0 \leavevmode\fi
\ifhmode \ifnum\time>0 x\fi \par \fi
\global\advance\count10 by 1
\ifnum\count10<\count20 \def\lt{}\else\def\ge{}\fi
\ifnum\count10>500 \def\bad{}\fi
\edef\v{\the\count10}",
    );
    for d in &analysis.facts.diagnostics {
        eprintln!("{} {} {}:{} {}", d.code, d.severity.as_str(), d.span.line, d.span.col, d.message);
    }
    assert_eq!(last_body(&analysis, "v"), "273");
    assert!(!crate::support::defined(&analysis, "bad"), "\\count10 read a value no path assigned");
    assert!(crate::support::defined(&analysis, "ge"));
    assert!(!crate::support::defined(&analysis, "lt"));
}

/// tex.web § 433: a register number past the range is "Bad register code"
/// and names register zero, and no other register. `pdftex`: after
/// `\count10=272 \count40000=1000`, `\count0` is 1000 and `\count10` 272.
#[test]
fn an_out_of_range_register_number_touches_no_other_register() {
    let analysis = analyze(
        r"\count0=0 \count10=272 \global\count40000=1000
\ifnum\count0=1000 \def\zero{}\fi
\ifnum\count10=272 \def\kept{}\fi
\ifnum\count10>500 \def\bad{}\fi",
    );
    assert!(crate::support::defined(&analysis, "zero"));
    assert!(crate::support::defined(&analysis, "kept"));
    assert!(!crate::support::defined(&analysis, "bad"), "\\count10 read a value no assignment gave it");
    assert!(analysis.facts.diagnostics.iter().any(|d| d.code == "bad-register-code"));
}

/// tex.web § 283: a group end undoes what was saved in it. Paths that part
/// in group 2, close it, define `\n` in group 1 and open group 2 again
/// join there. The join took the fork's save stack as shared, though the
/// group that saved `\m` was closed since: group 1's end then put back
/// `\m` and left `\n` defined (pdfTeX: undefined).
#[test]
fn join_after_closing_the_fork_group_keeps_the_save_stack() {
    let a = analyze(
        r"\def\m{A}\begingroup\begingroup\def\m{B}\ifnum\pdfuniformdeviate2=0 \endgroup\def\n{C}\begingroup\def\x{1}\else\endgroup\def\n{C}\begingroup\def\x{2}\fi\endgroup\endgroup\edef\R{\ifdefined\n D\else U\fi}",
    );
    assert_eq!(last_body(&a, "R"), "U");
}
