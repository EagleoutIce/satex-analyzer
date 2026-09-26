//! Small, single-rule tests for the TeX and e-TeX semantics the interpreter
//! (`src/exec.rs`, `src/machine.rs`) models. Each test pins one rule by
//! defining a macro on one branch and a different one on the other, then
//! reading the body back.

use satex::config::{Config, Engine};
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

fn body(analysis: &Analysis, name: &str) -> String {
    definition(analysis, name)
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

fn defined(analysis: &Analysis, name: &str) -> bool {
    definition(analysis, name).is_some()
}

fn depth_of(analysis: &Analysis, name: &str) -> Option<u16> {
    definition(analysis, name).map(|d| d.depth)
}

// Conditionals

#[test]
fn iftrue_takes_the_then_branch() {
    let analysis = analyze(r"\iftrue\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn iffalse_takes_the_else_branch() {
    let analysis = analyze(r"\iffalse\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
}

#[test]
fn ifnum_less_than() {
    let analysis = analyze(r"\ifnum 3<5 \def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn ifnum_greater_than() {
    let analysis = analyze(r"\ifnum 3>5 \def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
}

#[test]
fn ifnum_equals() {
    let analysis = analyze(r"\ifnum 5=5 \def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn ifdim_compares_scaled_points() {
    // The TeXbook, ch. 20: \ifdim compares two dimensions.
    let analysis = analyze(r"\ifdim 1in>72pt \def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn ifodd_tests_the_parity() {
    let odd = analyze(r"\ifodd 3 \def\seen{odd}\else\def\seen{even}\fi");
    assert_eq!(body(&odd, "seen"), "odd");
    let even = analyze(r"\ifodd 4 \def\seen{odd}\else\def\seen{even}\fi");
    assert_eq!(body(&even, "seen"), "even");
}

#[test]
fn ifcase_selects_the_numbered_arm() {
    // tex.web § 509: \ifcase counts \or's until it reaches the selector.
    let analysis = analyze(r"\ifcase 1 \def\seen{zero}\or\def\seen{one}\or\def\seen{two}\fi");
    assert_eq!(body(&analysis, "seen"), "one");
}

#[test]
fn ifcase_falls_to_else_past_the_last_or() {
    let analysis = analyze(r"\ifcase 9 \def\seen{zero}\or\def\seen{one}\else\def\seen{other}\fi");
    assert_eq!(body(&analysis, "seen"), "other");
}

#[test]
fn ifcase_with_no_matching_arm_and_no_else_runs_nothing() {
    let analysis = analyze(r"\ifcase 9 \def\a{0}\or\def\b{1}\fi\def\after{reached}");
    assert!(!defined(&analysis, "a"));
    assert!(!defined(&analysis, "b"));
    assert!(defined(&analysis, "after"));
}

#[test]
fn ifx_true_for_identically_defined_macros() {
    let analysis = analyze(r"\def\a{x}\def\b{x}\ifx\a\b\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn ifx_false_for_macros_with_different_bodies() {
    let analysis = analyze(r"\def\a{x}\def\b{y}\ifx\a\b\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
}

#[test]
fn ifx_distinguishes_a_primitive_from_a_macro() {
    let analysis = analyze(r"\def\a{x}\ifx\a\relax\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
}

#[test]
fn ifx_is_true_for_two_never_defined_control_sequences() {
    // tex.web § 507: \ifx compares eq_type and equiv; two undefined control
    // sequences have the same (undefined) meaning.
    let analysis = analyze(r"\ifx\neverone\nevertwo\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn if_compares_character_codes() {
    // The TeXbook, ch. 20: \if compares character codes after expansion.
    let analysis = analyze(r"\if ab\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
    let same = analyze(r"\if aa\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&same, "seen"), "yes");
}

#[test]
fn ifcat_compares_categories_not_characters() {
    let analysis = analyze(r"\ifcat ab\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes", "a and b are both letters");
    let differ = analyze(r"\ifcat a1\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&differ, "seen"), "no", "a letter and a digit differ in category");
}

#[test]
fn ifdefined_is_true_for_a_known_control_sequence() {
    let analysis = analyze(r"\def\a{x}\ifdefined\a\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn ifdefined_is_false_for_an_unknown_control_sequence() {
    // eTeX manual § 3.2.
    let analysis = analyze(r"\ifdefined\neverheard\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
}

#[test]
fn ifcsname_tests_a_built_name_without_defining_it() {
    // eTeX manual § 3.2: unlike \csname, \ifcsname does not create the
    // control sequence when it was not already defined.
    let analysis = analyze(r"\ifcsname nosuchname\endcsname\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
    assert!(!defined(&analysis, "nosuchname"));
}

#[test]
fn ifcsname_is_true_for_a_defined_name() {
    let analysis = analyze(r"\def\marker{x}\ifcsname marker\endcsname\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn unless_ifnum_negates_the_test() {
    // eTeX manual § 3.1.
    let analysis = analyze(r"\unless\ifnum 1=1 \def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
}

#[test]
fn nested_conditionals_select_independently() {
    let analysis =
        analyze(r"\iftrue\ifnum 2>1 \def\seen{inner-yes}\else\def\seen{inner-no}\fi\else\def\seen{outer-no}\fi");
    assert_eq!(body(&analysis, "seen"), "inner-yes");
}

#[test]
fn a_conditional_inside_edef_is_resolved_before_the_body_is_fixed() {
    let analysis = analyze(r"\edef\a{\iftrue X\else Y\fi}");
    assert_eq!(body(&analysis, "a"), "X");
}

// Expansion

#[test]
fn expandafter_skips_exactly_one_token() {
    // tex.web § 367: expand the second token once before reinserting the
    // held first token.
    let analysis = analyze(r"\def\a{X}\expandafter\def\csname b\endcsname{\a}");
    assert_eq!(body(&analysis, "b"), "\\a ");
}

#[test]
fn a_single_expandafter_shields_only_the_first_expansion() {
    // tex.web § 367: \expandafter expands its second token exactly once, so
    // \noexpand shields whatever that single step produces.
    let analysis = analyze(r"\def\a{\b}\def\b{final}\edef\out{\expandafter\noexpand\a}");
    assert_eq!(body(&analysis, "out"), "\\b ");
}

#[test]
fn chained_expandafter_reaches_further_into_the_expansion() {
    // Each extra \expandafter defers one more already-read token, moving the
    // one-step expansion of \a's own body (\b) past \noexpand instead of
    // being shielded by it, so \b is free to expand too.
    let analysis = analyze(r"\def\a{\b}\def\b{final}\edef\out{\expandafter\expandafter\expandafter\noexpand\a}");
    assert_eq!(body(&analysis, "out"), "final");
}

#[test]
fn noexpand_keeps_an_undefined_control_sequence_from_being_reported() {
    let analysis = analyze(r"\edef\a{\noexpand\nosuch}");
    assert_eq!(body(&analysis, "a"), r"\nosuch ");
    assert!(
        !analysis
            .facts
            .expansions
            .iter()
            .any(|e| e.meaning == satex::facts::MeaningKind::Undefined && analysis.interner.name(e.name) == "nosuch"),
        "\\noexpand keeps the token from being obeyed, so it is never looked up"
    );
}

#[test]
fn edef_expands_the_body_def_does_not() {
    let analysis = analyze(r"\def\inner{X}\def\a{\inner}\edef\b{\inner}");
    assert_eq!(body(&analysis, "a"), r"\inner ");
    assert_eq!(body(&analysis, "b"), "X");
}

#[test]
fn the_of_a_count_register_reads_its_value() {
    let analysis = analyze(r"\countdef\mycount=3 \mycount=42 \edef\a{\the\mycount}");
    assert_eq!(body(&analysis, "a"), "42");
}

#[test]
fn assigning_to_a_toks_register_does_not_expand_its_content() {
    // tex.web § 477: a token register's ⟨general text⟩ is read the same way
    // a macro body is, with nothing expanded.
    let analysis = analyze(r"\def\hidden{\def\seen{yes}}\toksdef\mytoks=5 \mytoks={\hidden}");
    assert!(!defined(&analysis, "seen"));
}

#[test]
fn the_of_a_toks_register_reinserts_live_tokens() {
    // tex.web § 465: \the of a token list gives back the tokens themselves,
    // not a textual rendering, so a \def stored in one still executes.
    let analysis = analyze(r"\toksdef\mytoks=5 \mytoks={\def\inside{x}}\the\mytoks");
    assert_eq!(body(&analysis, "inside"), "x");
}

#[test]
fn number_reads_a_count_registers_decimal_value() {
    let analysis = analyze(r"\countdef\mycount=3 \mycount=-7 \edef\a{\number\mycount}");
    assert_eq!(body(&analysis, "a"), "-7");
}

#[test]
fn romannumeral_of_a_positive_integer() {
    let analysis = analyze(r"\edef\a{\romannumeral 1984}");
    assert_eq!(body(&analysis, "a"), "mcmlxxxiv");
}

#[test]
fn romannumeral_of_zero_is_empty() {
    // The TeXbook, ch. 20: no glyph exists for zero.
    let analysis = analyze(r"\edef\a{[\romannumeral 0]}");
    assert_eq!(body(&analysis, "a"), "[]");
}

#[test]
fn romannumeral_of_a_negative_integer_is_empty() {
    let analysis = analyze(r"\edef\a{[\romannumeral -5]}");
    assert_eq!(body(&analysis, "a"), "[]");
}

#[test]
fn string_escapes_a_control_sequence_with_escapechar() {
    // tex.web § 69: the escape character printed is \escapechar.
    let analysis = analyze(r"\edef\a{\string\foo}");
    assert_eq!(body(&analysis, "a"), r"\foo");
}

#[test]
fn string_of_a_control_sequence_with_no_escapechar() {
    let analysis = analyze(r"\escapechar=-1 \edef\a{\string\foo}");
    assert_eq!(body(&analysis, "a"), "foo");
}

#[test]
fn meaning_of_an_undefined_control_sequence() {
    let analysis = analyze(r"\edef\a{\meaning\nosuchthing}");
    assert_eq!(body(&analysis, "a"), "undefined");
}

#[test]
fn meaning_of_a_macro_shows_its_replacement_text() {
    let analysis = analyze(r"\def\greet{hi}\edef\a{\meaning\greet}");
    assert_eq!(body(&analysis, "a"), "macro:->hi");
}

#[test]
fn csname_of_an_undefined_name_becomes_relax() {
    // tex.web § 373: an undefined \csname is entered as \relax. \meaning
    // itself reads its argument unexpanded, so \expandafter is needed to
    // let \csname…\endcsname build the control sequence first.
    let analysis = analyze(r"\edef\a{\expandafter\meaning\csname brandnew\endcsname}");
    assert_eq!(body(&analysis, "a"), r"\relax");
}

#[test]
fn detokenize_turns_a_token_list_into_characters() {
    // eTeX manual § 3.4: catcode-12/10 text, unaffected by \escapechar.
    let analysis = analyze(r"\edef\a{\detokenize{x#1}}");
    assert_eq!(body(&analysis, "a"), "x##1");
}

// Macros

#[test]
fn a_delimited_parameter_reads_up_to_its_delimiter() {
    let analysis = analyze(r"\def\upto#1STOP{\def\seen{#1}}\upto abcSTOP");
    assert_eq!(body(&analysis, "seen"), "abc");
}

#[test]
fn a_brace_delimited_final_parameter_stops_before_the_group() {
    // tex.web § 476: `#{` ends the parameter text and the brace is put back.
    let analysis = analyze(r"\def\take#1#{\def\seen{#1}}\take xy{z}");
    assert_eq!(body(&analysis, "seen"), "xy");
}

#[test]
fn doubled_hash_produces_one_hash_in_the_inner_macro() {
    // tex.web § 475: `##` in a parameter text or replacement text stands
    // for one literal `#`.
    let analysis = analyze(r"\def\wrap{\def\inner##1{[##1]}}\wrap");
    assert_eq!(body(&analysis, "inner"), "[#1]");
}

#[test]
fn nine_parameters_are_all_reachable() {
    let analysis = analyze(r"\def\nine#1#2#3#4#5#6#7#8#9{#9#8#7#6#5#4#3#2#1}\edef\a{\nine abcdefghi}");
    assert_eq!(body(&analysis, "a"), "ihgfedcba");
}

#[test]
fn global_def_escapes_every_enclosing_group() {
    let analysis = analyze(r"{{{\global\def\a{deep}}}}");
    let definition = definition(&analysis, "a").expect("a is defined");
    assert!(definition.global);
}

#[test]
fn protected_macros_still_expand_under_an_ordinary_call() {
    // eTeX manual § 3.5: \protected only shields a macro from expansion
    // inside \edef/\xdef, not from being called normally.
    let analysis = analyze(r"\protected\def\p{\def\seen{ran}}\p");
    assert_eq!(body(&analysis, "seen"), "ran");
}

#[test]
fn let_assigns_a_macros_meaning() {
    let analysis = analyze(r"\def\a{x}\let\b\a\ifx\a\b\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn let_assigns_a_characters_meaning() {
    // plain.tex: \let\bgroup={  makes \bgroup behave exactly like `{`.
    let analysis = analyze(r"\let\open={ \open\def\inside{x}}");
    assert_eq!(depth_of(&analysis, "inside"), Some(1));
}

#[test]
fn let_assigns_a_primitives_meaning() {
    let analysis = analyze(r"\let\myrelax\relax\ifx\myrelax\relax\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn futurelet_gives_a_name_the_second_upcoming_tokens_meaning() {
    // The TeXbook, ch. 20: \futurelet\x\t\u sets \x to the meaning of \u.
    let analysis = analyze(r"\def\a{X}\def\b{Y}\futurelet\next\a\b\ifx\next\b\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

// Registers and arithmetic

#[test]
fn a_count_register_is_addressed_by_number() {
    let analysis = analyze(r"\count7=99 \edef\a{\the\count7}");
    assert_eq!(body(&analysis, "a"), "99");
}

#[test]
fn a_dimen_register_accepts_a_unit() {
    let analysis = analyze(r"\dimen0=1in \ifdim\dimen0=72.26999pt\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn the_of_a_dimen_prints_the_shortest_round_tripping_decimal() {
    // tex.web § 103 `print_scaled`: as few decimal digits as will still read
    // back to the same scaled-point value; 1in famously prints as
    // "72.26999pt" rather than the exact "72.26999...pt" it stands for.
    let analysis = analyze(r"\dimen0=1pt \edef\a{\the\dimen0}\dimen1=1in \edef\b{\the\dimen1}");
    assert_eq!(body(&analysis, "a"), "1.0pt");
    assert_eq!(body(&analysis, "b"), "72.26999pt");
}

#[test]
fn a_toks_register_holds_a_token_list_by_number() {
    let analysis = analyze(r"\toks9={hello}\edef\a{\the\toks9}");
    assert_eq!(body(&analysis, "a"), "hello");
}

#[test]
fn countdef_names_a_count_register() {
    let analysis = analyze(r"\countdef\mycount=8 \mycount=5 \edef\a{\the\mycount}");
    assert_eq!(body(&analysis, "a"), "5");
}

#[test]
fn toksdef_names_a_toks_register() {
    let analysis = analyze(r"\toksdef\mytoks=6 \mytoks={z}\edef\a{\the\mytoks}");
    assert_eq!(body(&analysis, "a"), "z");
}

#[test]
fn advance_adds_to_a_named_count_register() {
    let analysis = analyze(r"\countdef\mycount=1 \mycount=10 \advance\mycount by 5 \edef\a{\the\mycount}");
    assert_eq!(body(&analysis, "a"), "15");
}

#[test]
fn multiply_scales_a_named_count_register() {
    let analysis = analyze(r"\countdef\mycount=1 \mycount=6 \multiply\mycount by 7 \edef\a{\the\mycount}");
    assert_eq!(body(&analysis, "a"), "42");
}

#[test]
fn divide_truncates_toward_zero() {
    // tex.web § 1239: integer division truncates.
    let analysis = analyze(r"\countdef\mycount=1 \mycount=-7 \divide\mycount by 2 \edef\a{\the\mycount}");
    assert_eq!(body(&analysis, "a"), "-3");
}

#[test]
fn numexpr_follows_ordinary_precedence() {
    // eTeX manual § 3.5.
    let analysis = analyze(r"\edef\a{\the\numexpr 2+3*4\relax}");
    assert_eq!(body(&analysis, "a"), "14");
}

#[test]
fn numexpr_division_rounds_to_the_nearest_tying_away_from_zero() {
    let analysis = analyze(r"\edef\a{\the\numexpr 7/2\relax}\edef\b{\the\numexpr -7/2\relax}");
    assert_eq!(body(&analysis, "a"), "4");
    assert_eq!(body(&analysis, "b"), "-4");
}

#[test]
fn dimexpr_computes_in_scaled_points() {
    let analysis = analyze(r"\ifdim\dimexpr 3pt*2\relax=6pt \def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

#[test]
fn glueexpr_computes_like_dimexpr_on_its_natural_width() {
    let analysis = analyze(r"\ifdim\glueexpr 2pt+3pt\relax=5pt \def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

// Groups

#[test]
fn a_brace_group_is_local() {
    let analysis = analyze(r"\def\a{outer}{\def\a{inner}\def\seen{\a}}");
    assert_eq!(body(&analysis, "seen"), "\\a ");
    assert_eq!(depth_of(&analysis, "seen"), Some(1));
    let outer = definition(&analysis, "a").expect("the outer \\a survives");
    assert_eq!(outer.depth, 0);
}

#[test]
fn begingroup_endgroup_form_a_group() {
    // tex.web § 1063: \begingroup/\endgroup make a "semi simple group".
    let analysis = analyze(r"\begingroup\def\a{1}\endgroup\def\after{2}");
    assert_eq!(depth_of(&analysis, "a"), Some(1));
    assert_eq!(depth_of(&analysis, "after"), Some(0));
}

#[test]
fn bgroup_egroup_form_a_group() {
    // plain.tex: \let\bgroup={ \let\egroup=}
    let analysis = analyze(r"\let\bgroup={ \let\egroup=} \bgroup\def\a{1}\egroup\def\after{2}");
    assert_eq!(depth_of(&analysis, "a"), Some(1));
    assert_eq!(depth_of(&analysis, "after"), Some(0));
}

#[test]
fn a_local_assignment_is_rolled_back_when_its_group_ends() {
    let analysis = analyze(r"\countdef\mycount=1 \mycount=1 {\mycount=2 }\edef\a{\the\mycount}");
    assert_eq!(body(&analysis, "a"), "1");
}

#[test]
fn a_global_assignment_survives_its_group() {
    let analysis = analyze(r"\countdef\mycount=1 \mycount=1 {\global\mycount=2 }\edef\a{\the\mycount}");
    assert_eq!(body(&analysis, "a"), "2");
}

#[test]
fn aftergroup_reinserts_a_token_when_the_group_ends() {
    // tex.web § 326: the token is read back once the group has already
    // closed, so it runs at the enclosing depth.
    // The token is saved, not its meaning: a definition made inside the group
    // is gone by the time it runs.
    let analysis = analyze(r"\def\a{\def\seen{yes}}\begingroup\aftergroup\a\endgroup");
    assert_eq!(body(&analysis, "seen"), "yes");
    assert_eq!(depth_of(&analysis, "seen"), Some(0));
}

// Catcodes

#[test]
fn catcode_assignment_is_readable_with_the() {
    let analysis = analyze(r"\catcode`\~=13 \edef\a{\the\catcode`\~}");
    assert_eq!(body(&analysis, "a"), "13");
}

#[test]
fn an_active_character_can_be_defined_like_a_macro() {
    let analysis = analyze(r"\catcode`\~=13 \def~{tilde}");
    assert_eq!(body(&analysis, &format!("{}~", satex::tex::ACTIVE)), "tilde");
}

#[test]
fn a_catcode_change_inside_a_group_is_undone() {
    let before = analyze(r"\edef\a{\the\catcode`\@}");
    assert_eq!(body(&before, "a"), "12", "@ is an \"other\" character by default");
    let analysis = analyze(r"{\catcode`\@=11 }\edef\a{\the\catcode`\@}");
    assert_eq!(body(&analysis, "a"), "12");
}

#[test]
fn unexpanded_shields_its_argument_inside_edef() {
    // eTeX manual § 3.4.
    let analysis = analyze(r"\def\foo{X}\edef\a{\unexpanded{\foo}}");
    assert_eq!(body(&analysis, "a"), r"\foo ");
}

#[test]
fn a_plain_macros_argument_may_not_contain_par() {
    // The TeXbook, ch. 20: without \long, \par may not appear in an
    // argument; tex.web § 1035 signals a runaway argument.
    let analysis = analyze(r"\def\x#1{[#1]}\x{a\par b}");
    assert!(
        analysis.facts.diagnostics.iter().any(|d| d.code == "runaway-argument"),
        "no diagnostic was raised for \\par inside a non-\\long argument"
    );
}

#[test]
fn advance_updates_a_register_addressed_by_number() {
    let analysis = analyze(r"\count5=10 \advance\count5 by 1 \edef\a{\the\count5}");
    assert_eq!(body(&analysis, "a"), "11");
}

#[test]
fn a_local_assignment_to_a_numbered_register_is_rolled_back() {
    let analysis = analyze(r"\count5=1 {\count5=2 }\edef\a{\the\count5}");
    assert_eq!(body(&analysis, "a"), "1");
}

#[test]
fn scantokens_retokenizes_under_the_current_catcodes() {
    // eTeX manual § 3.9: the text is read as a file, so without
    // `\endlinechar=-1` its line would end in a space, and without the
    // `\everyeof{\noexpand}` idiom the `\edef` would meet the file's end.
    // `\def~{active}` must itself run through `\scantokens`: written plainly
    // in `\test`'s body it would tokenize `~` at `\def\test` time, while it
    // is still catcode-other (pdftex -ini -etex confirms).
    let analysis = analyze(
        r"\catcode`\~=12 \def\test#1#2{\catcode`\~=13 \endlinechar=-1 \everyeof{\noexpand}\scantokens{#2}\edef\a{\scantokens{#1}}}\test{x~y}{\def~{active}}",
    );
    assert_eq!(body(&analysis, "a"), "xactivey");
}

// Engines beyond pdfTeX: LuaTeX, XeTeX and ConTeXt (all experimental)

/// A run configured for one engine, with the kernel left out so the test
/// sees only what the engine itself contributes.
fn analyze_with(engine: Engine, source: &str) -> Analysis {
    let cfg = Config {
        engine: Some(engine),
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    };
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

#[test]
fn directlua_consumes_its_body_instead_of_reading_it_as_tex() {
    // The braces around Lua are the primitive's argument, not a TeX group
    // (LuaTeX manual, "Lua related primitives").
    let analysis = analyze_with(Engine::LuaTeX, r"\directlua{tex.print('x') \relax}\def\after{reached}");
    assert!(defined(&analysis, "after"));
    let lua: Vec<&str> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == satex::builtins::OccKind::Lua)
        .map(|o| o.key.as_str())
        .collect();
    assert_eq!(lua.len(), 1, "the Lua body was not recorded: {lua:?}");
    assert!(lua[0].contains("tex.print"), "the Lua body was not kept: {lua:?}");
}

#[test]
fn a_lua_body_that_opens_a_group_does_not_leave_one_open() {
    let analysis = analyze_with(Engine::LuaTeX, r"\directlua{if x then y end}\def\after{reached}");
    assert_eq!(depth_of(&analysis, "after"), Some(0));
}

#[test]
fn catcodetable_consumes_its_number() {
    // `\catcodetable⟨number⟩` (LuaTeX manual, "Catcode tables"): without the
    // number being scanned, the digits would be typeset instead.
    let analysis = analyze_with(Engine::LuaTeX, r"\catcodetable 3 \def\after{reached}");
    assert!(defined(&analysis, "after"));
}

#[test]
fn a_context_document_is_not_reported_as_latex() {
    let analysis = analyze(CONTEXT_SAMPLE);
    let identity = satex::query::identity(&analysis);
    assert_eq!(identity.kind, "ConTeXt document (experimental)");
    assert_eq!(analysis.plugins.kernel, satex::plugin::Kernel::Context);
}

#[test]
fn a_context_document_does_not_report_its_own_commands_as_undefined() {
    // satex knows the engine primitives and nothing ConTeXt defines, so it
    // must not claim every ConTeXt command is missing.
    let analysis = analyze(CONTEXT_SAMPLE);
    let undefined: Vec<String> = satex::lint::lint(&analysis)
        .into_iter()
        .filter(|record| record["code"] == "undefined-control-sequence")
        .map(|record| record["name"].to_string())
        .collect();
    assert!(undefined.is_empty(), "ConTeXt commands reported as undefined: {undefined:?}");
}

const CONTEXT_SAMPLE: &str = r"% !TeX program = context
\usemodule[letter]
\setupbodyfont[10pt]
\starttext
\startsection[title={Hello}]
  Some text.
\stopsection
\stoptext
";

/// Where the installed engines live, if there are any.
fn engine_binary(name: &str) -> Option<std::path::PathBuf> {
    let which = std::process::Command::new("kpsewhich").arg("--var-value=SELFAUTOLOC").output().ok()?;
    let directory = std::path::PathBuf::from(String::from_utf8(which.stdout).ok()?.trim());
    let binary = directory.join(name);
    binary.is_file().then_some(binary)
}

/// The names satex registers for `engine` and for no plain-TeX run: its
/// engine-specific catalog.
fn engine_only_names(engine: Engine) -> Vec<String> {
    let mut it = satex::tex::Interner::default();
    let base = satex::builtins::initial_meanings(&mut it, Engine::Tex);
    let table = satex::builtins::initial_meanings(&mut it, engine);
    let mut names: Vec<String> = table
        .keys()
        .filter(|sym| !base.contains_key(*sym))
        .map(|sym| it.name(*sym).to_string())
        .filter(|name| name.chars().all(|c| c.is_ascii_alphabetic()))
        .collect();
    names.sort();
    names
}

/// Ask the engine itself which of `names` it defines as a primitive:
/// `\meaning` of a primitive is the primitive's own name (The TeXbook,
/// ch. 20).  Returns `None` when the engine is not installed.
fn engine_primitives(engine: &str, names: &[String]) -> Option<std::collections::BTreeSet<String>> {
    let binary = engine_binary(engine)?;
    let dir = std::env::temp_dir().join("satex-engine-probe").join(engine);
    std::fs::create_dir_all(&dir).ok()?;
    let mut probe = String::from("\\catcode`\\{=1 \\catcode`\\}=2 \\immediate\\openout0=found.txt\n");
    for name in names {
        probe.push_str(&format!(
            "\\expandafter\\ifx\\csname {name}\\endcsname\\relax\\else\\immediate\\write0{{{name} \
             \\expandafter\\meaning\\csname {name}\\endcsname}}\\fi\n"
        ));
    }
    probe.push_str("\\immediate\\closeout0\\end\n");
    std::fs::write(dir.join("probe.tex"), probe).ok()?;
    let _ = std::process::Command::new(&binary)
        .arg("--interaction=batchmode")
        .arg("probe.tex")
        .current_dir(&dir)
        .output()
        .ok()?;
    let found = std::fs::read_to_string(dir.join("found.txt")).ok()?;
    Some(
        found
            .lines()
            .filter_map(|line| line.split_once(' '))
            .filter(|(name, meaning)| meaning.trim() == format!("\\{name}"))
            .map(|(name, _)| name.to_string())
            .collect(),
    )
}

#[test]
fn the_luatex_catalog_claims_nothing_luatex_does_not_have() {
    let names = engine_only_names(Engine::LuaTeX);
    let Some(real) = engine_primitives("luatex", &names) else {
        eprintln!("no luatex found; skipping the LuaTeX catalog probe");
        return;
    };
    let extra: Vec<&String> = names.iter().filter(|name| !real.contains(*name)).collect();
    assert!(extra.is_empty(), "satex claims these for LuaTeX, the engine has none of them: {extra:?}");
}

#[test]
fn the_xetex_catalog_claims_nothing_xetex_does_not_have() {
    let names = engine_only_names(Engine::XeTeX);
    let Some(real) = engine_primitives("xetex", &names) else {
        eprintln!("no xetex found; skipping the XeTeX catalog probe");
        return;
    };
    let extra: Vec<&String> = names.iter().filter(|name| !real.contains(*name)).collect();
    assert!(extra.is_empty(), "satex claims these for XeTeX, the engine has none of them: {extra:?}");
}

#[test]
fn an_implicit_group_runs_its_aftergroup_tokens() {
    // `\bgroup` and `\egroup` are `\let` to the braces (The TeXbook, ch. 24),
    // so a group opened through a macro still ends where `\egroup` does and
    // still releases what `\aftergroup` saved (tex.web § 326).
    let analysis = analyze(r"\def\bg{\bgroup}\def\afterA{done}\bg\def\inner{1}\aftergroup\afterA\egroup");
    assert_eq!(body(&analysis, "inner"), "1");
    assert_eq!(body(&analysis, "afterA"), "done");
}

#[test]
fn aftergroup_can_assemble_a_definition_token_by_token() {
    // The classic trick: each token is saved on its own and they arrive in
    // order once the group closes, so the definition is made outside it.
    let analysis = analyze(r"{\aftergroup\def\aftergroup\later\aftergroup{\aftergroup L\aftergroup}}");
    assert_eq!(body(&analysis, "later"), "L");
}

#[test]
fn a_tripled_expandafter_reaches_the_third_token() {
    // `\expandafter\expandafter\expandafter` expands the token two places
    // ahead, which is how a macro's meaning reaches a `\def` body.
    let analysis = analyze(r"\def\a{A}\expandafter\expandafter\expandafter\def\expandafter\x\expandafter{\a}");
    assert_eq!(body(&analysis, "x"), "A");
}

#[test]
fn noexpand_shields_a_name_built_by_csname() {
    // `\expandafter\noexpand\csname a\endcsname` stores the control sequence
    // itself, not what it means (The TeXbook, ch. 20).
    let analysis = analyze(r"\def\a{A}\edef\z{\expandafter\noexpand\csname a\endcsname}");
    assert_eq!(body(&analysis, "z"), r"\a ");
}

#[test]
fn every_evaluated_conditional_is_a_fact() {
    let a = analyze(
        "\\count1=1\n\\ifnum\\count1=1\n\\def\\a{}\n\\else\n\\def\\b{}\n\\fi\n\\ifcase\\count1 x\\or y\\or z\\fi\n\\ifx\\undefinedthing\\relax\\fi\n\\ifnum\\count1=2 \\else\\fi\n",
    );
    let at = |line: u32| {
        a.facts
            .conditionals
            .iter()
            .find(|c| c.at.line == line)
            .unwrap_or_else(|| panic!("no conditional on line {line}"))
    };
    let first = at(2);
    assert_eq!((first.taken.clone(), first.undecided), (vec![Some(0)], false));
    assert_eq!(first.arms.iter().map(|s| s.line).collect::<Vec<_>>(), [4]);
    assert_eq!(first.fi.map(|s| s.line), Some(6));
    let case = at(7);
    assert_eq!(case.taken, [Some(1)]);
    assert_eq!(case.arms.len(), 2);
    assert_eq!(at(9).taken, [Some(1)]);
    assert_eq!(at(9).fi.map(|s| s.line), Some(9));
}
