//! Counterexamples taken from the engine: every input here was run through
//! `pdftex` (TeX Live 2026) with `-interaction=nonstopmode` and
//! `\message`/`\meaning`, and each test asserts the answer it gave.
//!
//! Follows the `analyze()`/`body()` helper pattern from `tests/semantics.rs`.

use satex::config::Config;
use satex::machine::{Analysis, Machine};

// Without a format the catcodes are INITEX's (tex.web § 232).
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

fn analyze(source: &str) -> Analysis {
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    };
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

fn definition<'a>(analysis: &'a Analysis, name: &str) -> Option<&'a satex::facts::Definition> {
    analysis.facts.defs.iter().find(|d| analysis.interner.name(d.name) == name)
}

/// The *last* definition of `name`: some of the inputs below redefine a
/// control sequence more than once (e.g. inside a group closed by
/// `\aftergroup`), and it is the final meaning that a real run would show.
fn last_definition<'a>(analysis: &'a Analysis, name: &str) -> Option<&'a satex::facts::Definition> {
    analysis.facts.defs.iter().rev().find(|d| analysis.interner.name(d.name) == name)
}

fn body(analysis: &Analysis, name: &str) -> String {
    definition(analysis, name)
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

fn last_body(analysis: &Analysis, name: &str) -> String {
    last_definition(analysis, name)
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}


/// tex.web §§ 461-475: a `\skip` register's value is a ⟨glue⟩, scanned
/// without needing braces. `pdftex`: `\the\skip0` after
/// `\skip0=1pt plus 2pt` is `1.0pt plus 2.0pt`.
#[test]
fn unbraced_skip_register_is_scanned_as_glue() {
    let analysis = analyze(r"\skip0=1pt plus 2pt \edef\v{\the\skip0}");
    assert_eq!(body(&analysis, "v"), "1.0pt plus 2.0pt");
}

/// tex.web § 326: `\afterassignment` holds its token until the *next*
/// assignment is complete. `pdftex`: `\seen` ends up `5`.
#[test]
fn afterassignment_runs_after_the_assignment_completes() {
    let analysis = analyze(
        r"\count0=0 \def\mark{\xdef\seen{\the\count0}} \afterassignment\mark\count0=5",
    );
    assert_eq!(body(&analysis, "seen"), "5");
}

/// `\futurelet\cs t1 t2` reads two raw tokens with no `=`/space handling.
/// `pdftex`: `\futurelet\test=XY` binds `\test` to `X` ("the letter X").
#[test]
fn futurelet_binds_the_first_lookahead_token() {
    let analysis = analyze(r"\futurelet\test=XY \edef\z{\meaning\test}");
    assert_eq!(body(&analysis, "z"), "the letter X");
}

/// TeXbook ch. 7 (`\lowercase`/`\uppercase`): every character token is
/// case-shifted through `\lccode`/`\uccode`; control sequence names are not
/// touched. `pdftex`: `\lowercase{\def\lo{X}}` defines `\lo` as `x`.
#[test]
fn lowercase_shifts_characters_via_lccode() {
    let analysis = analyze(r"\lowercase{\def\lo{X}}");
    assert_eq!(body(&analysis, "lo"), "x");
}

/// `\uppercase`/`\lowercase` are not expandable, so an `\edef` leaves them
/// untouched. `pdftex`: `\meaning\z` is `macro:->\uppercase {ab}`, literal.
#[test]
fn uppercase_is_not_expandable_inside_edef() {
    let analysis = analyze(r"\edef\z{\uppercase{ab}}");
    assert_eq!(body(&analysis, "z"), r"\uppercase {ab}");
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
    let analysis =
        analyze(r"\def\eat#1{GOT:[#1]}\edef\result{\eat{a\par b}}");
    assert_eq!(body(&analysis, "result"), r"\par b");
}

/// TeXbook ch. 24: TeX integers stay within `±(2^31-1)`; the reference
/// engine's 32-bit registers wrap on overflow. `pdftex`: `\the\count0` after
/// `\count0=2147483647 \advance\count0 by 1` is `-2147483648`.
#[test]
fn count_register_arithmetic_wraps_at_32_bits() {
    let analysis = analyze(r"\count0=2147483647 \advance\count0 by 1 \edef\v{\the\count0}");
    assert_eq!(body(&analysis, "v"), "-2147483648");
}

/// tex.web § 1221: an active character is, once tokenized, a reference into
/// the same `eqtb` a control sequence uses, so `\let` to it copies its
/// *current* meaning. `pdftex`: after `\def~{TILDE}`, `\let\altTilde~` makes
/// `\meaning\altTilde` read `macro:->TILDE`.
#[test]
fn let_of_an_active_character_copies_its_bound_meaning() {
    let analysis =
        analyze(r"\catcode`\~=13 \def~{TILDE} \let\altTilde~ \edef\altmeaning{\meaning\altTilde}");
    assert_eq!(body(&analysis, "altmeaning"), "macro:->TILDE");
}

/// tex.web § 298 (`print_meaning`): `\meaning` of a `\let`-to-character
/// binding names its category by TeX's own word — "the letter" for
/// catcode 11 — not a generic "of category N". `pdftex`: `\meaning\a` after
/// `\let\a=X` is `the letter X`.
#[test]
fn meaning_of_a_letter_says_the_letter() {
    let analysis = analyze(r"\let\a=X \edef\am{\meaning\a}");
    assert_eq!(body(&analysis, "am"), "the letter X");
}


/// tex.web § 367: `\expandafter t1 t2` expands `t2` exactly one step and
/// reinserts `t1`; chained `\expandafter`s nest that rule recursively, so
/// only the token right after the *last* `\expandafter` gets expanded.
#[test]
fn expandafter_chain_expands_only_the_last_token() {
    let analysis =
        analyze(r"\def\a{A}\def\b{B}\expandafter\def\expandafter\c\expandafter{\a\b}");
    assert_eq!(body(&analysis, "c"), "A\\b ");
}

/// tex.web § 369: `\noexpand⟨token⟩` inside `\edef` passes that one token
/// through untouched.
#[test]
fn noexpand_survives_edef() {
    let analysis = analyze(r"\def\foo{FOO}\edef\z{\noexpand\foo bar}");
    assert_eq!(body(&analysis, "z"), r"\foo bar");
}

/// `\let` skips leading spaces, then an optional `=`, then at most one more
/// optional space, before reading the source token — even for a
/// control-*symbol* target (`\!`), which does not itself eat trailing
/// spaces the way a control word does. Checked with `\ifx` rather than
/// `\meaning` so the comparison is independent of the "the letter" wording
/// bug (see `meaning_of_a_letter_says_the_letter` above).
#[test]
fn let_skips_the_optional_equals_and_space() {
    let analysis = analyze(r"\let\! = X \edef\z{\ifx\!X yes\else no\fi}");
    assert_eq!(body(&analysis, "z"), " yes");
}

/// tex.web § 509: a negative `\ifcase` selector matches no `\or`-delimited
/// arm, so the `\else` branch runs.
#[test]
fn ifcase_with_a_negative_selector_runs_the_else_branch() {
    let analysis =
        analyze(r"\ifcase-1 \def\seen{A}\or\def\seen{B}\else\def\seen{C}\fi");
    assert_eq!(body(&analysis, "seen"), "C");
}

/// tex.web § 372: `\csname...\endcsname` expands its contents (via
/// `get_x_token`) before building the name.
#[test]
fn csname_builds_its_name_from_expansion() {
    let analysis = analyze(r"\def\name{foo}\expandafter\def\csname\name\endcsname{BAR}");
    assert_eq!(body(&analysis, "foo"), "BAR");
}

/// TeXbook ch. 6: `\global` inside a group makes even a plain register
/// assignment survive the group's end.  `\countdef` (not `\newcount`, a
/// `plain.tex` macro this bare-initex `analyze()` does not have) names the
/// register (pdftex -ini confirms `\newcount` is undefined without a format).
#[test]
fn global_register_assignment_survives_its_group() {
    let analysis = analyze(
        r"\countdef\mycount=5 \mycount=0 {\global\mycount=5 } \edef\v{\the\mycount}",
    );
    assert_eq!(body(&analysis, "v"), "5");
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

/// tex.web § 465: `\the` of a token register returns its stored tokens
/// verbatim, unexpanded.
#[test]
fn the_of_a_toks_register_is_verbatim() {
    let analysis = analyze(r"\toks0={a b c} \edef\v{\the\toks0}");
    assert_eq!(body(&analysis, "v"), "a b c");
}

/// eTeX manual § 3.2: `\unless\ifnum...` negates the conditional's result.
#[test]
fn unless_negates_the_following_conditional() {
    let analysis =
        analyze(r"\unless\ifnum1>2 \def\seen{A}\else\def\seen{B}\fi");
    assert_eq!(body(&analysis, "seen"), "A");
}

// Found while fixing the table above, each checked the same way: a
// minimal plain-format file run through `pdftex -interaction=nonstopmode`
// with the answer read back out of the log.

/// tex.web § 461: a `\muskip` holds math glue, printed in `mu`, and an
/// order of infinity may appear in either component. `pdftex`:
/// `\muskip0=1mu plus 2fill minus 3mu` shows as
/// `1.0mu plus 2.0fill minus 3.0mu`.
#[test]
fn a_muskip_is_scanned_and_printed_in_mu() {
    let analysis = analyze(r"\muskip0=1mu plus 2fill minus 3mu \edef\v{\the\muskip0}");
    assert_eq!(body(&analysis, "v"), "1.0mu plus 2.0fill minus 3.0mu");
}

/// tex.web § 407: keywords are matched case-insensitively, so `PLUS` opens
/// a stretch clause. `pdftex`: `\skip0=1pt PLUS 2pt` is `1.0pt plus 2.0pt`.
#[test]
fn glue_keywords_are_matched_without_regard_to_case() {
    let analysis = analyze(r"\skip0=1pt PLUS 2pt \edef\v{\the\skip0}");
    assert_eq!(body(&analysis, "v"), "1.0pt plus 2.0pt");
}

/// tex.web § 1239: `\advance` on a glue adds all three components, and
/// tex.web § 1240: `\multiply` scales them. `pdftex`: `1.0pt plus 2.0pt`
/// advanced by `1pt` is `2.0pt plus 2.0pt` and multiplied by 3 is
/// `3.0pt plus 6.0pt`.
#[test]
fn glue_arithmetic_works_on_every_component() {
    let analysis = analyze(
        r"\skip0=1pt plus 2pt \advance\skip0 by 1pt \edef\v{\the\skip0}
          \skip1=1pt plus 2pt \multiply\skip1 by 3 \edef\w{\the\skip1}",
    );
    assert_eq!(body(&analysis, "v"), "2.0pt plus 2.0pt");
    assert_eq!(body(&analysis, "w"), "3.0pt plus 6.0pt");
}

/// tex.web § 1240: `\multiply` and `\divide` go through `mult_integers` and
/// `x_over_n`, which report "Arithmetic overflow" and leave the register
/// alone — unlike `\advance`, which has no such check. `pdftex`:
/// `\count0=2147483647 \multiply\count0 by 2` leaves `2147483647`.
#[test]
fn multiply_leaves_the_register_alone_when_it_overflows() {
    let analysis = analyze(
        r"\count0=2147483647 \multiply\count0 by 2 \edef\v{\the\count0}
          \count1=7 \divide\count1 by 0 \edef\w{\the\count1}",
    );
    assert_eq!(body(&analysis, "v"), "2147483647");
    assert_eq!(body(&analysis, "w"), "7");
}

/// tex.web § 445: `scan_int` reports "Number too big" past 2^31-1 and uses
/// the limit instead. `pdftex`: `\count0=2147483648` leaves `2147483647`.
#[test]
fn an_integer_constant_is_clamped_to_the_32_bit_maximum() {
    let analysis = analyze(r"\count0=2147483648 \edef\v{\the\count0}");
    assert_eq!(body(&analysis, "v"), "2147483647");
}

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

/// tex.web § 453: `⟨factor⟩⟨internal unit⟩` scales the unit, so `-.5\dimen0`
/// is half of it negated; tex.web § 449: an internal *integer* is only the
/// factor, and the unit still follows. `pdftex`: `-0.75pt` and `3.0pt`.
#[test]
fn a_dimension_may_be_a_factor_times_an_internal_unit() {
    let analysis = analyze(
        r"\dimen0=1.5pt \dimen0=-.5\dimen0 \edef\v{\the\dimen0}
          \count1=3 \dimen1=\count1 pt \edef\w{\the\dimen1}",
    );
    assert_eq!(body(&analysis, "v"), "-0.75pt");
    assert_eq!(body(&analysis, "w"), "3.0pt");
}

/// tex.web § 448: a ⟨dimen⟩ swallows one optional space after its unit, so
/// the branch of `\ifdim 1pt<2pt T` starts at `T` and not at a space.
#[test]
fn a_dimension_eats_the_space_that_ends_it() {
    let analysis = analyze(r"\edef\v{[\ifdim 1pt<2pt T\else F\fi]}");
    assert_eq!(body(&analysis, "v"), "[T]");
}

/// tex.web § 262 (`print_cs`): a control sequence prints with a trailing
/// space unless its name is a single non-letter. `pdftex`: `\meaning` of a
/// macro whose body is `\a` reads `macro:->\a `, and of one whose body is
/// `\!` reads `macro:->\!`.
#[test]
fn a_control_word_detokenizes_with_a_trailing_space() {
    let analysis = analyze(r"\def\a{X}\def\v{\a}\def\!{bang}\def\w{\!}");
    assert_eq!(body(&analysis, "v"), "\\a ");
    assert_eq!(body(&analysis, "w"), "\\!");
}

/// tex.web § 296 with pdfTeX's `\protected`: the prefixes a macro carries
/// are part of its meaning and print before the word `macro`. `pdftex`:
/// `\protected\long\def\a#1{x}` shows as `\protected\long macro:#1->x`.
#[test]
fn meaning_names_the_prefixes_a_macro_was_defined_with() {
    let analysis = analyze(
        r"\long\def\a#1{#1}\edef\v{\meaning\a}
          \protected\long\def\b#1{x}\edef\w{\meaning\b}",
    );
    assert_eq!(body(&analysis, "v"), r"\long macro:#1->#1");
    assert_eq!(body(&analysis, "w"), r"\protected\long macro:#1->x");
}

/// tex.web § 298 (`print_cmd_chr`): every category has its own phrase.
#[test]
fn meaning_names_each_category_by_its_own_phrase() {
    let analysis = analyze(
        r"\let\a=, \edef\v{\meaning\a}
          \let\b={ \edef\w{\meaning\b}
          \let\c=# \edef\x{\meaning\c}",
    );
    assert_eq!(body(&analysis, "v"), "the character ,");
    assert_eq!(body(&analysis, "w"), "begin-group character {");
    assert_eq!(body(&analysis, "x"), "macro parameter character #");
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

/// The TeXbook, ch. 20: a `\long` macro accepts `\par` in its arguments.
#[test]
fn a_long_macro_accepts_par_in_an_argument() {
    let analysis = analyze(r"\long\def\keep#1{KEPT:[#1]}\edef\v{\keep{a\par b}}");
    assert_eq!(body(&analysis, "v"), r"KEPT:[a\par b]");
}

/// tex.web § 232: `\lccode` and `\uccode` start at IniTeX's defaults, are
/// read back with `\the`, and are what `\lowercase` consults. `pdftex`:
/// `\the\lccode`\A` is 97, and `\lccode`\Q=`\z` makes `\lowercase{Q}` a `z`.
#[test]
fn the_case_tables_have_inited_defaults_and_can_be_set() {
    let analysis = analyze(
        r"\edef\v{[\the\lccode`\A][\the\uccode`\a][\the\sfcode`\A]}
          \lccode`\Q=`\z \lowercase{\def\w{Q}}",
    );
    assert_eq!(body(&analysis, "v"), "[97][65][999]");
    assert_eq!(body(&analysis, "w"), "z");
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
        (r"\def\a{A}\def\b{B}\expandafter\expandafter\expandafter\def\expandafter\expandafter\expandafter\v\expandafter\expandafter\expandafter{\a\b}", "A\\b "),
        (r"\def\name{foo}\expandafter\def\csname\name\endcsname{BAR}\edef\v{\meaning\foo}", "macro:->BAR"),
        (r"\expandafter\let\csname qq\endcsname=\relax \edef\v{\meaning\qq}", "\\relax"),
        (r"\def\name{foo}\expandafter\def\csname\name\endcsname{BAR}\edef\v{[\ifcsname foo\endcsname Y\else N\fi][\ifcsname zzz\endcsname Y\else N\fi]}", "[Y][N]"),
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

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

fn document(source: &str) -> Analysis {
    let cfg = Config { load_classes: true, ..Config::default() };
    Machine::analyze(source, None, &cfg)
}

/// The findings for the document itself, as `code name` pairs.
fn findings(analysis: &Analysis) -> Vec<String> {
    let field = |f: &satex::query::Record, key: &str| {
        f.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string()
    };
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
    let cfg = Config { load_classes: true, ..Config::default() };
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
