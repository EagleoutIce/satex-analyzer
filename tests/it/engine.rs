//! Small, single-rule tests for the TeX and e-TeX semantics the interpreter
//! (`src/exec.rs`, `src/machine.rs`) models. Each test pins one rule by
//! defining a macro on one branch and a different one on the other, then
//! reading the body back.

use crate::support::{PRELUDE, analyze_plain as analyze, depth_of};
use crate::support::{body, definition};
use satex::config::{Config, Engine, Load, Texmf};
use satex::machine::{Analysis, Machine};

fn defined(analysis: &Analysis, name: &str) -> bool {
    definition(analysis, name).is_some()
}

// Conditionals

/// Probes that each define macros, with the body TeX gives each one.
const BODIES: &[crate::support::BodyRow] = &[
    ("iftrue_takes_the_then_branch", r"\iftrue\def\seen{yes}\else\def\seen{no}\fi", &[("seen", "yes")]),
    ("iffalse_takes_the_else_branch", r"\iffalse\def\seen{yes}\else\def\seen{no}\fi", &[("seen", "no")]),
    ("ifnum_less_than", r"\ifnum 3<5 \def\seen{yes}\else\def\seen{no}\fi", &[("seen", "yes")]),
    ("ifnum_greater_than", r"\ifnum 3>5 \def\seen{yes}\else\def\seen{no}\fi", &[("seen", "no")]),
    ("ifnum_equals", r"\ifnum 5=5 \def\seen{yes}\else\def\seen{no}\fi", &[("seen", "yes")]),
    // The TeXbook, ch. 20: \ifdim compares two dimensions.
    ("ifdim_compares_scaled_points", r"\ifdim 1in>72pt \def\seen{yes}\else\def\seen{no}\fi", &[("seen", "yes")]),
    // tex.web § 509: \ifcase counts \or's until it reaches the selector.
    (
        "ifcase_selects_the_numbered_arm",
        r"\ifcase 1 \def\seen{zero}\or\def\seen{one}\or\def\seen{two}\fi",
        &[("seen", "one")],
    ),
    (
        "ifcase_falls_to_else_past_the_last_or",
        r"\ifcase 9 \def\seen{zero}\or\def\seen{one}\else\def\seen{other}\fi",
        &[("seen", "other")],
    ),
    (
        "ifx_true_for_identically_defined_macros",
        r"\def\a{x}\def\b{x}\ifx\a\b\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    (
        "ifx_false_for_macros_with_different_bodies",
        r"\def\a{x}\def\b{y}\ifx\a\b\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "no")],
    ),
    (
        "ifx_distinguishes_a_primitive_from_a_macro",
        r"\def\a{x}\ifx\a\relax\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "no")],
    ),
    // tex.web § 507: \ifx compares eq_type and equiv; two undefined control
    // sequences have the same (undefined) meaning.
    (
        "ifx_is_true_for_two_never_defined_control_sequences",
        r"\ifx\neverone\nevertwo\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    (
        "ifdefined_is_true_for_a_known_control_sequence",
        r"\def\a{x}\ifdefined\a\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    // eTeX manual § 3.2.
    (
        "ifdefined_is_false_for_an_unknown_control_sequence",
        r"\ifdefined\neverheard\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "no")],
    ),
    (
        "ifcsname_is_true_for_a_defined_name",
        r"\def\marker{x}\ifcsname marker\endcsname\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    // eTeX manual § 3.1.
    ("unless_ifnum_negates_the_test", r"\unless\ifnum 1=1 \def\seen{yes}\else\def\seen{no}\fi", &[("seen", "no")]),
    ("a_conditional_inside_edef_is_resolved_before_the_body_is_fixed", r"\edef\a{\iftrue X\else Y\fi}", &[("a", "X")]),
    // tex.web § 367: expand the second token once before reinserting the
    // held first token.
    ("expandafter_skips_exactly_one_token", r"\def\a{X}\expandafter\def\csname b\endcsname{\a}", &[("b", "\\a ")]),
    // tex.web § 367: \expandafter expands its second token exactly once, so
    // \noexpand shields whatever that single step produces.
    (
        "a_single_expandafter_shields_only_the_first_expansion",
        r"\def\a{\b}\def\b{final}\edef\out{\expandafter\noexpand\a}",
        &[("out", "\\b ")],
    ),
    // Each extra \expandafter defers one more already-read token, moving the
    // one-step expansion of \a's own body (\b) past \noexpand instead of
    // being shielded by it, so \b is free to expand too.
    (
        "chained_expandafter_reaches_further_into_the_expansion",
        r"\def\a{\b}\def\b{final}\edef\out{\expandafter\expandafter\expandafter\noexpand\a}",
        &[("out", "final")],
    ),
    (
        "edef_expands_the_body_def_does_not",
        r"\def\inner{X}\def\a{\inner}\edef\b{\inner}",
        &[("a", r"\inner "), ("b", "X")],
    ),
    (
        "the_of_a_count_register_reads_its_value",
        r"\countdef\mycount=3 \mycount=42 \edef\a{\the\mycount}",
        &[("a", "42")],
    ),
    // tex.web § 465: \the of a token list gives back the tokens themselves,
    // not a textual rendering, so a \def stored in one still executes.
    (
        "the_of_a_toks_register_reinserts_live_tokens",
        r"\toksdef\mytoks=5 \mytoks={\def\inside{x}}\the\mytoks",
        &[("inside", "x")],
    ),
    (
        "number_reads_a_count_registers_decimal_value",
        r"\countdef\mycount=3 \mycount=-7 \edef\a{\number\mycount}",
        &[("a", "-7")],
    ),
    ("romannumeral_of_a_positive_integer", r"\edef\a{\romannumeral 1984}", &[("a", "mcmlxxxiv")]),
    // The TeXbook, ch. 20: no glyph exists for zero.
    ("romannumeral_of_zero_is_empty", r"\edef\a{[\romannumeral 0]}", &[("a", "[]")]),
    ("romannumeral_of_a_negative_integer_is_empty", r"\edef\a{[\romannumeral -5]}", &[("a", "[]")]),
    // tex.web § 69: the escape character printed is \escapechar.
    ("string_escapes_a_control_sequence_with_escapechar", r"\edef\a{\string\foo}", &[("a", r"\foo")]),
    ("string_of_a_control_sequence_with_no_escapechar", r"\escapechar=-1 \edef\a{\string\foo}", &[("a", "foo")]),
    ("meaning_of_an_undefined_control_sequence", r"\edef\a{\meaning\nosuchthing}", &[("a", "undefined")]),
    ("meaning_of_a_macro_shows_its_replacement_text", r"\def\greet{hi}\edef\a{\meaning\greet}", &[("a", "macro:->hi")]),
    // tex.web § 373: an undefined \csname is entered as \relax. \meaning
    // itself reads its argument unexpanded, so \expandafter is needed to
    // let \csname…\endcsname build the control sequence first.
    (
        "csname_of_an_undefined_name_becomes_relax",
        r"\edef\a{\expandafter\meaning\csname brandnew\endcsname}",
        &[("a", r"\relax")],
    ),
    // eTeX manual § 3.4: catcode-12/10 text, unaffected by \escapechar.
    ("detokenize_turns_a_token_list_into_characters", r"\edef\a{\detokenize{x#1}}", &[("a", "x##1")]),
    (
        "a_delimited_parameter_reads_up_to_its_delimiter",
        r"\def\upto#1STOP{\def\seen{#1}}\upto abcSTOP",
        &[("seen", "abc")],
    ),
    // tex.web § 476: `#{` ends the parameter text and the brace is put back.
    (
        "a_brace_delimited_final_parameter_stops_before_the_group",
        r"\def\take#1#{\def\seen{#1}}\take xy{z}",
        &[("seen", "xy")],
    ),
    // tex.web § 475: `##` in a parameter text or replacement text stands
    // for one literal `#`.
    (
        "doubled_hash_produces_one_hash_in_the_inner_macro",
        r"\def\wrap{\def\inner##1{[##1]}}\wrap",
        &[("inner", "[#1]")],
    ),
    (
        "nine_parameters_are_all_reachable",
        r"\def\nine#1#2#3#4#5#6#7#8#9{#9#8#7#6#5#4#3#2#1}\edef\a{\nine abcdefghi}",
        &[("a", "ihgfedcba")],
    ),
    // eTeX manual § 3.5: \protected only shields a macro from expansion
    // inside \edef/\xdef, not from being called normally.
    ("protected_macros_still_expand_under_an_ordinary_call", r"\protected\def\p{\def\seen{ran}}\p", &[("seen", "ran")]),
    (
        "let_assigns_a_macros_meaning",
        r"\def\a{x}\let\b\a\ifx\a\b\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    (
        "let_assigns_a_primitives_meaning",
        r"\let\myrelax\relax\ifx\myrelax\relax\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    // The TeXbook, ch. 20: \futurelet\x\t\u sets \x to the meaning of \u.
    (
        "futurelet_gives_a_name_the_second_upcoming_tokens_meaning",
        r"\def\a{X}\def\b{Y}\futurelet\next\a\b\ifx\next\b\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    ("a_count_register_is_addressed_by_number", r"\count7=99 \edef\a{\the\count7}", &[("a", "99")]),
    (
        "a_dimen_register_accepts_a_unit",
        r"\dimen0=1in \ifdim\dimen0=72.26999pt\def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    // tex.web § 103 `print_scaled`: as few decimal digits as will still read
    // back to the same scaled-point value; 1in famously prints as
    // "72.26999pt" rather than the exact "72.26999...pt" it stands for.
    (
        "the_of_a_dimen_prints_the_shortest_round_tripping_decimal",
        r"\dimen0=1pt \edef\a{\the\dimen0}\dimen1=1in \edef\b{\the\dimen1}",
        &[("a", "1.0pt"), ("b", "72.26999pt")],
    ),
    ("a_toks_register_holds_a_token_list_by_number", r"\toks9={hello}\edef\a{\the\toks9}", &[("a", "hello")]),
    ("countdef_names_a_count_register", r"\countdef\mycount=8 \mycount=5 \edef\a{\the\mycount}", &[("a", "5")]),
    ("toksdef_names_a_toks_register", r"\toksdef\mytoks=6 \mytoks={z}\edef\a{\the\mytoks}", &[("a", "z")]),
    (
        "advance_adds_to_a_named_count_register",
        r"\countdef\mycount=1 \mycount=10 \advance\mycount by 5 \edef\a{\the\mycount}",
        &[("a", "15")],
    ),
    (
        "multiply_scales_a_named_count_register",
        r"\countdef\mycount=1 \mycount=6 \multiply\mycount by 7 \edef\a{\the\mycount}",
        &[("a", "42")],
    ),
    // tex.web § 1239: integer division truncates.
    (
        "divide_truncates_toward_zero",
        r"\countdef\mycount=1 \mycount=-7 \divide\mycount by 2 \edef\a{\the\mycount}",
        &[("a", "-3")],
    ),
    // eTeX manual § 3.5.
    ("numexpr_follows_ordinary_precedence", r"\edef\a{\the\numexpr 2+3*4\relax}", &[("a", "14")]),
    (
        "numexpr_division_rounds_to_the_nearest_tying_away_from_zero",
        r"\edef\a{\the\numexpr 7/2\relax}\edef\b{\the\numexpr -7/2\relax}",
        &[("a", "4"), ("b", "-4")],
    ),
    (
        "dimexpr_computes_in_scaled_points",
        r"\ifdim\dimexpr 3pt*2\relax=6pt \def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    (
        "glueexpr_computes_like_dimexpr_on_its_natural_width",
        r"\ifdim\glueexpr 2pt+3pt\relax=5pt \def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    (
        "a_local_assignment_is_rolled_back_when_its_group_ends",
        r"\countdef\mycount=1 \mycount=1 {\mycount=2 }\edef\a{\the\mycount}",
        &[("a", "1")],
    ),
    (
        "a_global_assignment_survives_its_group",
        r"\countdef\mycount=1 \mycount=1 {\global\mycount=2 }\edef\a{\the\mycount}",
        &[("a", "2")],
    ),
    ("catcode_assignment_is_readable_with_the", r"\catcode`\~=13 \edef\a{\the\catcode`\~}", &[("a", "13")]),
    // eTeX manual § 3.4.
    ("unexpanded_shields_its_argument_inside_edef", r"\def\foo{X}\edef\a{\unexpanded{\foo}}", &[("a", r"\foo ")]),
    (
        "advance_updates_a_register_addressed_by_number",
        r"\count5=10 \advance\count5 by 1 \edef\a{\the\count5}",
        &[("a", "11")],
    ),
    (
        "a_local_assignment_to_a_numbered_register_is_rolled_back",
        r"\count5=1 {\count5=2 }\edef\a{\the\count5}",
        &[("a", "1")],
    ),
    // eTeX manual § 3.9: the text is read as a file, so without
    // `\endlinechar=-1` its line would end in a space, and without the
    // `\everyeof{\noexpand}` idiom the `\edef` would meet the file's end.
    // `\def~{active}` must itself run through `\scantokens`: written plainly
    // in `\test`'s body it would tokenize `~` at `\def\test` time, while it
    // is still catcode-other (pdftex -ini -etex confirms).
    (
        "scantokens_retokenizes_under_the_current_catcodes",
        r"\catcode`\~=12 \def\test#1#2{\catcode`\~=13 \endlinechar=-1 \everyeof{\noexpand}\scantokens{#2}\edef\a{\scantokens{#1}}}\test{x~y}{\def~{active}}",
        &[("a", "xactivey")],
    ),
    // `\bgroup` and `\egroup` are `\let` to the braces (The TeXbook, ch. 24),
    // so a group opened through a macro still ends where `\egroup` does and
    // still releases what `\aftergroup` saved (tex.web § 326).
    (
        "an_implicit_group_runs_its_aftergroup_tokens",
        r"\def\bg{\bgroup}\def\afterA{done}\bg\def\inner{1}\aftergroup\afterA\egroup",
        &[("inner", "1"), ("afterA", "done")],
    ),
    // The classic trick: each token is saved on its own and they arrive in
    // order once the group closes, so the definition is made outside it.
    (
        "aftergroup_can_assemble_a_definition_token_by_token",
        r"{\aftergroup\def\aftergroup\later\aftergroup{\aftergroup L\aftergroup}}",
        &[("later", "L")],
    ),
    // `\expandafter\expandafter\expandafter` expands the token two places
    // ahead twice: `\a` becomes `\b`, then `B`, before `\y` reads it
    // (pdfTeX: `\x=macro:->B`).
    (
        "a_tripled_expandafter_reaches_the_third_token",
        r"\def\a{\b}\def\b{B}\def\y#1{\def\x{#1}}\expandafter\expandafter\expandafter\y\a",
        &[("x", "B")],
    ),
    // `\expandafter\noexpand\csname a\endcsname` stores the control sequence
    // itself, not what it means (The TeXbook, ch. 20).
    (
        "noexpand_shields_a_name_built_by_csname",
        r"\def\a{A}\edef\z{\expandafter\noexpand\csname a\endcsname}",
        &[("z", r"\a ")],
    ),
];

#[test]
fn macro_bodies_agree_with_tex() {
    crate::support::body_rows(BODIES, analyze);
}

#[test]
fn ifodd_tests_the_parity() {
    let odd = analyze(r"\ifodd 3 \def\seen{odd}\else\def\seen{even}\fi");
    assert_eq!(body(&odd, "seen"), "odd");
    let even = analyze(r"\ifodd 4 \def\seen{odd}\else\def\seen{even}\fi");
    assert_eq!(body(&even, "seen"), "even");
}

#[test]
fn ifcase_with_no_matching_arm_and_no_else_runs_nothing() {
    let analysis = analyze(r"\ifcase 9 \def\a{0}\or\def\b{1}\fi\def\after{reached}");
    assert!(!defined(&analysis, "a"));
    assert!(!defined(&analysis, "b"));
    assert!(defined(&analysis, "after"));
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
fn ifcsname_tests_a_built_name_without_defining_it() {
    // eTeX manual § 3.2: unlike \csname, \ifcsname does not create the
    // control sequence when it was not already defined.
    let analysis = analyze(r"\ifcsname nosuchname\endcsname\def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "no");
    assert!(!defined(&analysis, "nosuchname"));
}

#[test]
fn nested_conditionals_select_independently() {
    let analysis =
        analyze(r"\iftrue\ifnum 2>1 \def\seen{inner-yes}\else\def\seen{inner-no}\fi\else\def\seen{outer-no}\fi");
    assert_eq!(body(&analysis, "seen"), "inner-yes");
}

// Expansion

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
fn assigning_to_a_toks_register_does_not_expand_its_content() {
    // tex.web § 477: a token register's ⟨general text⟩ is read the same way
    // a macro body is, with nothing expanded.
    let analysis = analyze(r"\def\hidden{\def\seen{yes}}\toksdef\mytoks=5 \mytoks={\hidden}");
    assert!(!defined(&analysis, "seen"));
}

// Macros

#[test]
fn global_def_escapes_every_enclosing_group() {
    let analysis = analyze(r"{{{\global\def\a{deep}}}}");
    let definition = definition(&analysis, "a").expect("a is defined");
    assert!(definition.global);
}

#[test]
fn let_assigns_a_characters_meaning() {
    // plain.tex: \let\bgroup={  makes \bgroup behave exactly like `{`.
    let analysis = analyze(r"\let\open={ \open\def\inside{x}}");
    assert_eq!(depth_of(&analysis, "inside"), Some(1));
}

// Registers and arithmetic

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
fn a_plain_macros_argument_may_not_contain_par() {
    // The TeXbook, ch. 20: without \long, \par may not appear in an
    // argument; tex.web § 1035 signals a runaway argument.
    let analysis = analyze(r"\def\x#1{[#1]}\x{a\par b}");
    assert!(
        analysis.facts.diagnostics.iter().any(|d| d.code == "runaway-argument"),
        "no diagnostic was raised for \\par inside a non-\\long argument"
    );
}

// Engines beyond pdfTeX: LuaTeX, XeTeX and ConTeXt (all experimental)

/// A run configured for one engine, with the kernel left out so the test
/// sees only what the engine itself contributes.
fn analyze_with(engine: Engine, source: &str) -> Analysis {
    let cfg = Config {
        engine: Some(engine),
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
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
    let dir = crate::support::tmp_root().join("satex-engine-probe").join(engine);
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
            // A primitive prints its own name, or the name it is an alias
            // of: `\meaning\XeTeXmathcode` is `\Umathcode`.
            .filter(|(_, meaning)| {
                meaning
                    .trim()
                    .strip_prefix('\\')
                    .is_some_and(|m| !m.is_empty() && m.chars().all(|c| c.is_ascii_alphabetic()))
            })
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

/// The LuaLaTeX kernel builds to the end: `\newinsert` past e-TeX's
/// register range must not weaken every register, which once sent the
/// kernel into the missing-file loop. Skipped without `lualatex`.
#[test]
fn the_lualatex_kernel_builds_without_broken_gaps() {
    use satex::facts::Severity;
    if !crate::support::installed() || satex::paths::which("lualatex").is_err() {
        eprintln!("no lualatex found; skipping the LuaLaTeX kernel build");
        return;
    }
    let dir = crate::support::scratch_dir(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/it-lualatex-kernel"),
    );
    let source = "% !TeX program = lualatex\n\\documentclass{article}\n\\begin{document}\nText.\n\\end{document}\n";
    let path = dir.join("main.tex");
    std::fs::write(&path, source).unwrap();
    let cache = satex::config::Cache { dir: Some(dir.join("cache")), ..Default::default() };
    let cfg = Config { cache, ..crate::support::config() };
    let analysis = Machine::analyze(source, Some(&path), &cfg);
    let broken: Vec<String> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Unsupported)
        .map(|d| format!("{} {}:{} {}", d.code, analysis.file_name(d.span.file), d.span.line, d.message))
        .collect();
    assert!(broken.is_empty(), "{broken:#?}");
}

/// LuaTeX dumps no Lua state, so the LuaLaTeX kernel is cached like any
/// other: the next run installs it, `\everyjob` runs the kernel's Lua
/// again, and expl3's Lua-defined `\tex_strcmp:D` decides
/// `\str_if_eq:nnTF`. Skipped without `lualatex`.
#[test]
fn the_lualatex_kernel_is_cached_and_expl3_works() {
    if !crate::support::installed() || satex::paths::which("lualatex").is_err() {
        eprintln!("no lualatex found; skipping the LuaLaTeX kernel cache");
        return;
    }
    let dir = crate::support::scratch_dir(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/it-lualatex-cache"),
    );
    let source = "% !TeX program = lualatex\n\\documentclass{article}\n\\ExplSyntaxOn\n\
\\str_if_eq:nnTF {a} {a} { \\def\\same{} } { \\def\\different{} }\n\\ExplSyntaxOff\n\
\\begin{document}\nText.\n\\end{document}\n";
    let path = dir.join("main.tex");
    std::fs::write(&path, source).unwrap();
    let cache = satex::config::Cache { dir: Some(dir.join("cache")), ..Default::default() };
    let cfg = Config { cache, ..crate::support::config() };
    let cold = Machine::analyze(source, Some(&path), &cfg);
    let warm = Machine::analyze(source, Some(&path), &cfg);
    assert!(cold.format.as_ref().is_some_and(|f| !f.cached), "the first run interpreted the kernel");
    assert!(warm.format.as_ref().is_some_and(|f| f.cached), "the second run installed the cached kernel");
    for a in [&cold, &warm] {
        assert!(crate::support::defined(a, "same") && !crate::support::defined(a, "different"));
        let missing: Vec<String> = a
            .facts
            .diagnostics
            .iter()
            .filter(|d| d.code == "missing-number" && a.file_name(d.span.file).ends_with("expl3-code.tex"))
            .map(|d| format!("{}:{} {}", a.file_name(d.span.file), d.span.line, d.message))
            .collect();
        assert!(missing.is_empty(), "{missing:#?}");
    }
}

/// XeTeX's case codes reach the whole of Unicode too (XeTeX reference
/// guide, "Character codes"), while pdfTeX stops at 255 (tex.web § 1232).
#[test]
fn case_codes_past_255_are_valid_only_under_unicode_engines() {
    let run = |engine: Engine| {
        let cfg = Config { engine: Some(engine), kernel: Some(satex::plugin::Kernel::None), ..crate::support::bare() };
        let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\lccode65=1114111 \\ifnum\\lccode65=1114111 \\def\\ok{}\\fi";
        Machine::analyze(source, None, &cfg)
    };
    let xetex = run(Engine::XeTeX);
    assert!(definition(&xetex, "ok").is_some());
    assert!(!xetex.facts.diagnostics.iter().any(|d| d.code == "invalid-code"));
    let pdftex = run(Engine::PdfTeX);
    assert!(pdftex.facts.diagnostics.iter().any(|d| d.code == "invalid-code"));
}
