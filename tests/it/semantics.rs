use crate::support::{PRELUDE, analyze_kernel, analyze_plain as analyze, depth_of, diagnosed, last_body, names};
use crate::support::{body, definition};
use satex::builtins::OccKind;
use satex::config::{Config, Load, Texmf};
use satex::facts::MeaningKind;
use satex::machine::{Analysis, Machine};
use satex::query::{self, Filter, Query};

#[test]
fn def_records_parameter_text_and_body() {
    let analysis = analyze(r"\def\pair#1,#2.{(#1|#2)}");
    let definition = definition(&analysis, "pair").expect("\\pair is defined");
    let mac = definition.mac.as_ref().expect("a macro");
    assert_eq!(mac.parameter_text.arity, 2);
    assert!(!mac.parameter_text.is_simple(), "the parameter text is delimited");
    assert_eq!(body(&analysis, "pair"), "(#1|#2)");
}

/// Probes that each define macros, with the body TeX gives each one.
const BODIES: &[crate::support::BodyRow] = &[
    ("delimited_arguments_are_matched", r"\def\pair#1,#2.{\def\seen{#1-#2}}\pair a,b.", &[("seen", "a-b")]),
    ("undelimited_argument_loses_one_brace_level", r"\def\id#1{\def\seen{#1}}\id{{x}}", &[("seen", "{x}")]),
    (
        "ifnum_is_decided_from_counter_values",
        r"\newcounter{n}\setcounter{n}{7}\ifnum\value{n}>3\def\big{yes}\else\def\big{no}\fi",
        &[("big", "yes")],
    ),
    ("ifx_compares_meanings", r"\def\a{x}\def\b{x}\ifx\a\b\def\same{yes}\else\def\same{no}\fi", &[("same", "yes")]),
    (
        "ifx_of_a_name_with_itself_is_true_whatever_it_means",
        r"\expandafter\let\expandafter\s\csname x\the\pdfelapsedtime\endcsname\def\w#1{\ifx#1\s\def\r{yes}\else\def\r{no}\fi}\w\s",
        &[("r", "yes")],
    ),
    (
        "etex_expressions_are_evaluated",
        r"\ifnum\numexpr 2*3+1\relax>6 \def\big{yes}\else\def\big{no}\fi",
        &[("big", "yes")],
    ),
    (
        "a_nested_definition_substitutes_one_level_of_parameters",
        "\\def\\nest#1{\\def\\mid##1{\\def\\deep####1{(#1,##1,####1)}}}\n\\nest{A}\n\\mid{B}\n",
        &[("deep", "(A,B,#1)")],
    ),
    ("edef_expands_a_macro_that_takes_arguments", r"\def\id#1{[#1]}\edef\a{\id{x}}", &[("a", "[x]")]),
    (
        "edef_expands_undelimited_and_delimited_calls",
        r"\def\two#1#2{(#1,#2)}\def\del#1,#2.{<#1|#2>}\edef\a{\two ab}\edef\b{\del u,v.}",
        &[("a", "(a,b)"), ("b", "<u|v>")],
    ),
    (
        "edef_keeps_what_noexpand_and_protected_shield",
        r"\def\id#1{[#1]}\protected\def\p{q}\edef\a{\noexpand\id{x}}\edef\b{\p}",
        &[("a", r"\id {x}"), ("b", "\\p ")],
    ),
    ("expandafter_leaves_an_unexpandable_primitive_alone", r"\expandafter\relax\def\a{x}", &[("a", "x")]),
    (
        "csname_expands_while_it_reads_the_name",
        r"\def\stop{\endcsname}\expandafter\def\csname built\stop{body}",
        &[("built", "body")],
    ),
    (
        "a_number_may_come_out_of_an_expansion",
        r"\def\one{1}\ifnum0\one=1 \def\seen{yes}\else\def\seen{no}\fi",
        &[("seen", "yes")],
    ),
    // tex.web § 476: `#{` ends the parameter text and the `{` is put back.
    ("a_brace_delimited_parameter_stops_at_the_group", r"\def\grab#1#{\def\got{#1}}\grab abc{x}", &[("got", "abc")]),
    // The arm of a conditional is read token by token, so `\fi` is still a
    // token the macro can match on.
    ("a_parameter_may_be_delimited_by_fi", r"\def\grab#1\fi{\def\got{#1}}\iftrue \grab AB\fi", &[("got", "AB")]),
    // Each arm of an undecided conditional runs on its own copy of the state:
    // the `\else` arm does not see what the true arm defined.
    (
        "undecided_arms_do_not_see_each_other",
        r"\ifnum\pdfuniformdeviate2=0 \def\x{a}\else\ifx\x\undefined\def\clean{yes}\fi\fi",
        &[("clean", "yes")],
    ),
    // tex.web § 1224: `\Z` is `\relax` while its number is scanned, so it
    // does not expand to `3` and the register is `\count1`.
    (
        "shorthand_def_name_is_relax_while_its_number_is_scanned",
        r"\def\Z{3}\countdef\Z=1\Z=5 \edef\seen{\the\count1}",
        &[("seen", "5")],
    ),
];

#[test]
fn macro_bodies_agree_with_tex() {
    crate::support::body_rows(BODIES, analyze);
}

#[test]
fn let_copies_a_meaning() {
    let analysis = analyze(r"\let\newcommand\relax \newcommand{\gone}[1]{#1}");
    assert!(!names(&analysis).contains(&"gone".to_string()), "\\newcommand was disabled by \\let");
}

#[test]
fn groups_restore_definitions() {
    let analysis = analyze(r"\def\a{1}{\def\a{2}\def\inner{\a}}\def\after{\a}");
    let inner = definition(&analysis, "inner").expect("inner");
    assert_eq!(inner.depth, 1, "defined one group deep");
    let outer = definition(&analysis, "a").expect("a");
    assert_eq!(outer.depth, 0);
}

#[test]
fn global_definitions_escape_groups() {
    let analysis = analyze(r"{\gdef\survivor{x}}");
    let definition = definition(&analysis, "survivor").expect("survivor");
    assert!(definition.global);
}

#[test]
fn newif_builds_the_three_control_sequences() {
    let analysis = analyze_kernel(r"\newif\ifdraft");
    let defined = names(&analysis);
    for expected in ["drafttrue", "draftfalse", "ifdraft"] {
        assert!(defined.contains(&expected.to_string()), "{expected} is missing");
    }
    assert_eq!(definition(&analysis, "ifdraft").map(|d| d.tag), Some("switch"));
}

#[test]
fn makeatletter_makes_at_a_letter() {
    let analysis = analyze_kernel(r"\makeatletter\def\a@b{x}\makeatother\def\c@d{y}");
    assert!(definition(&analysis, "a@b").is_some());
    assert!(definition(&analysis, "c@d").is_none(), "\\makeatother gives @ back");
}

#[test]
fn a_switch_decides_its_conditional() {
    let analysis = analyze_kernel(r"\newif\ifdraft \drafttrue \ifdraft\def\mode{draft}\else\def\mode{final}\fi");
    assert_eq!(body(&analysis, "mode"), "draft");
    let analysis = analyze_kernel(r"\newif\ifdraft \ifdraft\def\mode{draft}\else\def\mode{final}\fi");
    assert_eq!(body(&analysis, "mode"), "final", "\\newif starts false");
}

/// Capabilities: termination, unknown inputs, progressive recursion. The
/// empty arm expands the undefined `\stop`, an error: under assume_compiles
/// that path is dropped, so the walk never reads on into the caller's code.
#[test]
fn a_walk_never_reads_past_an_undefined_stop_on_an_erroring_path() {
    let analysis = analyze(
        r"\def\walk#1{\ifx#1\stop\else\edef\rev{\rev#1}\expandafter\walk\fi}\def\conv#1{\def\rev{}\expandafter\walk#1\stop}\ifnum\pdfuniformdeviate2=0 \conv{12}\else\conv{}\fi\def\after{x}",
    );
    let bodies: Vec<String> = (analysis.facts.defs.iter())
        .filter(|d| analysis.interner.name(d.name) == "rev")
        .filter_map(|d| d.mac.as_ref().map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner)))
        .collect();
    assert!(bodies.iter().all(|b| ["", "1", "12"].contains(&b.as_str())), "{bodies:?}");
}

#[test]
fn an_undecidable_conditional_analyzes_both_arms() {
    let analysis = analyze(r"\ifnum\pdfuniformdeviate2=0 \def\one{a}\else\def\two{b}\fi");
    let defined = names(&analysis);
    assert!(defined.contains(&"one".to_string()) && defined.contains(&"two".to_string()));
    assert!(!definition(&analysis, "one").expect("one").certain);
}

#[test]
fn csname_builds_a_control_sequence() {
    let analysis = analyze(r"\expandafter\def\csname myname\endcsname{body}");
    assert!(names(&analysis).iter().any(|n| n == "myname"));
}

#[test]
fn newcommand_signature_survives() {
    let analysis = analyze_kernel(r"\newcommand{\qcite}[2][see]{#1:#2}");
    let definition = definition(&analysis, "qcite").expect("qcite");
    let spec = query::signature(&analysis, definition.name, definition.mac.as_ref().unwrap()).expect("signature");
    assert_eq!(spec.raw, "O{see}m");
}

#[test]
fn document_command_signature_is_parsed() {
    let analysis = analyze_kernel(r"\NewDocumentCommand\qx{s O{d} m}{#3}");
    let definition = definition(&analysis, "qx").expect("qx");
    let spec = query::signature(&analysis, definition.name, definition.mac.as_ref().unwrap()).expect("signature");
    assert_eq!(spec.items.len(), 3);
}

#[test]
fn environments_define_both_halves() {
    let analysis = analyze_kernel(r"\newenvironment{qbox}[1][plain]{begin #1}{end}");
    let defined = names(&analysis);
    assert!(defined.contains(&"qbox".to_string()) && defined.contains(&"endqbox".to_string()));
    assert_eq!(definition(&analysis, "qbox").map(|d| d.tag), Some("environment"));
}

#[test]
fn counters_and_lengths_hold_values() {
    let analysis = analyze_kernel(r"\newcounter{n}\addtocounter{n}{4}\newlength{\w}\setlength{\w}{1in}");
    assert_eq!(definition(&analysis, "c@n").map(|d| d.tag), Some("counter"));
    assert_eq!(definition(&analysis, "w").map(|d| d.tag), Some("length"));
}

#[test]
fn labels_and_references_are_recorded() {
    let analysis = analyze_kernel(r"\section{One}\label{sec:one}See \ref{sec:one} and \ref{missing}.");
    let labels: Vec<&str> =
        analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Label).map(|o| o.key.as_str()).collect();
    assert_eq!(labels, ["sec:one"]);
    let lints = satex::lint::lint(&analysis);
    assert!(lints.iter().any(|l| l["code"] == "undefined-reference"));
}

#[test]
fn recursion_is_found() {
    let analysis =
        analyze_kernel(r"\newcommand{\qloop}[1]{#1\qloop{#1}}\newcommand{\ping}{\pong}\newcommand{\pong}{\ping}");
    let found = query::run(&analysis, Query::Recursion, &Filter::Always);
    let names: Vec<&str> = found.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"\\qloop"));
    assert!(names.contains(&"\\ping") && names.contains(&"\\pong"));
}

#[test]
fn recursion_terminates_without_exhausting_the_budget() {
    let analysis = analyze(r"\def\spin{\spin}\spin");
    assert!(!analysis.exhausted, "the recursion guard fires before the token budget");
}

#[test]
fn undefined_control_sequences_are_reported() {
    let analysis = analyze(r"\thisdoesnotexist");
    assert!(
        analysis
            .facts
            .expansions
            .iter()
            .any(|e| e.meaning == MeaningKind::Undefined && analysis.interner.name(e.name) == "thisdoesnotexist")
    );
}

#[test]
fn catcode_changes_take_effect_immediately() {
    let analysis = analyze("\\catcode`\\@=11 \\def\\a@b{x}");
    assert!(names(&analysis).contains(&"a@b".to_string()));
}

#[test]
fn expl_syntax_makes_colon_and_underscore_letters() {
    let analysis = analyze_kernel(r"\ExplSyntaxOn \cs_new:Npn \my_fn:n #1 { #1 } \ExplSyntaxOff");
    assert!(names(&analysis).contains(&"my_fn:n".to_string()));
}

#[test]
fn package_options_run_their_code() {
    // The kernel's own `\DeclareOption` and `\ProcessOptions` (clsguide
    // § 4.5): only the option the document passed runs.
    let directory = crate::support::tmp_root().join(format!("satex-options-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("qopts.sty"),
        "\\DeclareOption{draft}{\\def\\qmode{draft}}\\DeclareOption{final}{\\def\\qfinal{final}}\\ProcessOptions\\relax\n",
    )
    .unwrap();
    let source = "\\documentclass{article}\n\\usepackage[draft]{qopts}\n\\begin{document}\n\\end{document}\n";
    let document = directory.join("doc.tex");
    std::fs::write(&document, source).unwrap();
    let cfg = Config { load: Load { packages: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(source, Some(&document), &cfg);
    assert_eq!(body(&analysis, "qmode"), "draft");
    assert!(definition(&analysis, "qfinal").is_none(), "final was not passed");
    assert_eq!(definition(&analysis, "ds@draft").map(|d| d.tag), Some("option"));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn filters_select_records() {
    let analysis = analyze_kernel(r"\newif\ifa \newcommand{\qq}{x}");
    let switches = query::run(&analysis, Query::Definitions, &Filter::parse("tag=switch").unwrap());
    assert!(!switches.is_empty());
    assert!(switches.iter().all(|r| r["tag"] == "switch"));

    let combined = Filter::parse(r"tag=macro and name~^\\qq$").unwrap();
    let macros = query::run(&analysis, Query::Definitions, &combined);
    assert_eq!(macros.len(), 1);

    let negated = Filter::parse("not tag=switch").unwrap();
    assert!(query::run(&analysis, Query::Definitions, &negated).iter().all(|r| r["tag"] != "switch"));
}

#[test]
fn filter_errors_are_reported() {
    assert!(Filter::parse("tag").is_err());
    assert!(Filter::parse("tag=(").is_err());
}

#[test]
fn a_global_definition_is_a_side_effect_of_the_expansion() {
    use satex::graph::EdgeKind;
    let analysis = analyze(r"\def\setup{\gdef\later{x}}\setup");
    let effects = analysis
        .graph
        .vertices
        .iter()
        .enumerate()
        .filter(|(id, _)| {
            analysis.graph.outgoing(*id as u32).iter().any(|(_, kind)| kind.intersects(EdgeKind::SIDE_EFFECT_ON_CALL))
        })
        .map(|(_, vertex)| analysis.interner.name(vertex.name).to_string())
        .collect::<Vec<_>>();
    assert!(effects.contains(&"later".to_string()), "got {effects:?}");
}

#[test]
fn dependency_graph_links_a_call_to_its_definition() {
    use satex::graph::{EdgeKind, VertexTag};
    let analysis = analyze_kernel(r"\newcommand{\greet}{hi}\greet");
    let call = analysis
        .graph
        .vertices
        .iter()
        .position(|v| v.tag == VertexTag::MacroCall && analysis.interner.name(v.name) == "greet")
        .expect("a call vertex");
    let edges = analysis.graph.outgoing(call as u32);
    assert!(edges.iter().any(|(_, kind)| kind.intersects(EdgeKind::READS)));
}

#[test]
fn scope_respects_position() {
    let analysis = analyze_kernel("\\newcommand{\\early}{1}\n\\newcommand{\\late}{2}\n");
    let visible = query::scope(&analysis, Some((analysis.main_file, 2, 1)), false);
    let names: Vec<&str> = visible.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"\\early"));
    assert!(!names.contains(&"\\late"));
}

#[test]
fn scope_has_one_row_per_name_the_meaning_in_force() {
    // Redefined once: the meaning at the end of the run is in force, and it
    // is the only row `\foo` gets, not one per definition.
    let analysis = analyze_kernel("\\newcommand{\\foo}{1}\\renewcommand{\\foo}{2}\n");
    let visible = query::scope(&analysis, None, false);
    let matches: Vec<_> = visible.iter().filter(|r| r["name"] == "\\foo").collect();
    assert_eq!(matches.len(), 1, "rows for \\foo: {matches:?}");
    assert_eq!(matches[0]["effective"], "\\foo");
}

#[test]
fn scope_never_shows_an_empty_or_unprintable_name() {
    let analysis = analyze_kernel("\\relax\n");
    let visible = query::scope(&analysis, None, false);
    assert!(!visible.is_empty());
    for record in &visible {
        let name = record["name"].as_str().expect("name is a string");
        assert!(!name.trim_start_matches('\\').is_empty(), "empty name: {record:?}");
        assert!(name.chars().all(|c| !c.is_control()), "unprintable name: {name:?}");
    }
}

#[test]
fn scope_puts_the_document_before_the_kernel() {
    let analysis = analyze_kernel("\\newcommand{\\myown}{1}\n");
    let visible = query::scope(&analysis, None, false);
    let document = visible.iter().position(|r| r["name"] == "\\myown").expect("\\myown in scope");
    let kernel = visible.iter().position(|r| r["package"] == "kernel").expect("a kernel name in scope");
    assert!(document < kernel, "document definitions should sort before the kernel");
    assert_eq!(visible[document]["package"], "document");
}

#[test]
fn scope_hides_internal_names_unless_all() {
    let analysis =
        analyze_kernel("\\makeatletter\\newcommand{\\my@internal}{1}\\makeatother\\newcommand{\\myvisible}{2}\n");
    let hidden = query::scope(&analysis, None, false);
    assert!(!hidden.iter().any(|r| r["name"] == "\\my@internal"));
    assert!(hidden.iter().any(|r| r["name"] == "\\myvisible"));

    let shown = query::scope(&analysis, None, true);
    assert!(shown.iter().any(|r| r["name"] == "\\my@internal"));
}

#[test]
fn expl3_functions_carry_their_arity_in_the_signature() {
    use satex::tex::expl3_arity;
    assert_eq!(expl3_arity("tl_set:Nn"), Some(2));
    assert_eq!(expl3_arity("seq_map_inline:Nn"), Some(2));
    assert_eq!(expl3_arity("cs_new:Npn"), None, "a parameter text has no fixed shape");
    assert_eq!(expl3_arity("group_begin:"), Some(0));
    assert_eq!(expl3_arity("newcommand"), None);
    assert_eq!(expl3_arity("foo:bar"), None);
}

#[test]
fn an_unmodeled_expl3_function_consumes_its_arguments() {
    let analysis = analyze(r"\ExplSyntaxOn \seq_map_inline:Nn \l_tmpa_seq { #1 } \def\after{here} \ExplSyntaxOff");
    assert_eq!(body(&analysis, "after"), "here", "the call did not swallow what follows it");
    assert!(
        !analysis
            .facts
            .expansions
            .iter()
            .any(|e| e.meaning == MeaningKind::Undefined && analysis.interner.name(e.name) == "seq_map_inline:Nn"),
        "an expl3 kernel function is not an undefined control sequence"
    );
}

#[test]
fn expl3_variants_are_generated() {
    let analysis = analyze_kernel(
        r"\ExplSyntaxOn \cs_new:Npn \my_do:nn #1#2 { #1#2 } \cs_generate_variant:Nn \my_do:nn { Vn , cn } \ExplSyntaxOff",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"my_do:Vn".to_string()));
    assert!(defined.contains(&"my_do:cn".to_string()));
}

#[test]
fn plain_tex_primitives_are_known() {
    let analysis = analyze(r"\hbox{x}\penalty100\relax");
    assert!(
        !analysis.facts.expansions.iter().any(|e| e.meaning == MeaningKind::Undefined),
        "plain TeX primitives are not undefined"
    );
}

#[test]
fn dimensions_use_scaled_points() {
    use satex::value::{UNIT, parse_dimen};
    assert_eq!(parse_dimen("1pt"), Some(UNIT));
    assert_eq!(parse_dimen("1in"), Some(4_736_286));
    assert_eq!(parse_dimen("1bp"), Some(65_781));
}

#[test]
fn a_file_that_ends_mid_construct_does_not_eat_its_parent() {
    // `\def` with no body: the reader must stop at the end of the token list,
    // not run on into whatever follows.
    let analysis = analyze(r"\def\broken");
    assert!(!analysis.exhausted);
}

#[test]
fn a_decimal_dimension_is_rounded_before_the_unit_is_applied() {
    use satex::value::parse_dimen;
    assert_eq!(parse_dimen("0.1pt"), Some(6554), "6553.6sp rounds away from zero");
    assert_eq!(parse_dimen("1in"), Some(4_736_286), "4736286.72sp truncates");
}

#[test]
fn expl_syntax_suppresses_the_end_of_line() {
    let analysis = analyze_kernel("\\ExplSyntaxOn\n\n\\cs_new:Npn \\a: { x }\n\n\\ExplSyntaxOff");
    assert!(
        !analysis.facts.expansions.iter().any(|e| analysis.interner.name(e.name) == "par"),
        "a blank line inside expl3 syntax makes no \\par"
    );
}

#[test]
fn etex_expressions_respect_precedence() {
    use satex::value::eval_expr;
    assert_eq!(eval_expr("1+2*3", false), Some(7));
    assert_eq!(eval_expr("(1+2)*3", false), Some(9));
    assert_eq!(eval_expr("-5", false), Some(-5));
    assert_eq!(eval_expr("7/2", false), Some(4), "ties away from zero");
}

#[test]
fn document_commands_take_the_full_argument_specification() {
    // pdflatex refuses `!` before a leading optional argument.
    let analysis = analyze_kernel(r"\NewDocumentCommand\qx{s o >{\TrimSpaces}m +m}{#4}");
    let definition = definition(&analysis, "qx").expect("qx");
    let spec = query::signature(&analysis, definition.name, definition.mac.as_ref().unwrap()).expect("signature");
    assert_eq!(spec.items.len(), 4);
}

fn sliced(analysis: &Analysis, names: &[&str], forward: bool) -> Vec<String> {
    let criteria: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let direction = if forward { satex::query::Direction::Forward } else { satex::query::Direction::Backward };
    query::slice(analysis, &criteria, None, direction)
        .into_iter()
        .map(|record| record["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn a_backward_slice_reaches_what_the_body_uses() {
    let analysis = analyze(r"\def\helper{H}\def\unused{U}\def\foo{\helper}\foo\unused");
    let names = sliced(&analysis, &["\\foo"], false);
    assert!(names.contains(&"\\helper".to_string()), "got {names:?}");
    assert!(!names.contains(&"\\unused".to_string()), "got {names:?}");
}

#[test]
fn a_forward_slice_is_the_transpose_of_the_backward_one() {
    let analysis = analyze(r"\def\helper{H}\def\foo{\helper}\foo");
    let backward = sliced(&analysis, &["\\foo"], false);
    let forward = sliced(&analysis, &["\\helper"], true);
    assert!(forward.contains(&"\\foo".to_string()), "got {forward:?}");
    assert!(backward.contains(&"\\helper".to_string()), "got {backward:?}");
}

#[test]
fn a_slice_follows_control_dependencies() {
    // a random number cannot be decided, so both outcomes are
    // analyzed and the guard governs them.
    let analysis = analyze(r"\def\a{1} \ifnum\pdfuniformdeviate2=0 \def\a{2}\fi \a");
    let names = sliced(&analysis, &["\\a"], false);
    assert!(names.contains(&"\\ifnum".to_string()), "the guard is part of the slice: {names:?}");
}

#[test]
fn a_conditional_without_an_else_keeps_the_earlier_definition() {
    let analysis = analyze("\\def\\a{1}\n\\ifnum\\pdfuniformdeviate2=0 \\def\\a{2}\\fi\n\\a\n");
    let definitions: Vec<u32> =
        analysis.facts.defs.iter().filter(|d| analysis.interner.name(d.name) == "a").map(|d| d.span.line).collect();
    assert_eq!(definitions, vec![1, 2], "the conditional definition does not replace the first");
    let names = sliced(&analysis, &["\\a"], false);
    assert!(names.contains(&"\\ifnum".to_string()), "got {names:?}");
}

#[test]
fn a_label_key_can_be_a_slicing_criterion() {
    let analysis = analyze_kernel("\\section{Intro}\n\\label{fig:one}\nSee \\ref{fig:one}.\n");
    let names = sliced(&analysis, &["fig:one"], false);
    assert_eq!(names.iter().filter(|n| *n == "fig:one").count(), 2, "label and reference: {names:?}");
}

#[test]
fn the_summary_states_what_the_input_is() {
    let analysis = analyze_kernel("\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n");
    let identity = query::identity(&analysis);
    assert_eq!(identity.kind, "LaTeX2e document");
    assert_eq!(identity.engine, "pdflatex");
    let fragment = analyze("\\def\\a{1}\\a\n");
    assert_eq!(query::identity(&fragment).kind, "TeX input");
}

#[test]
fn the_load_tree_carries_the_path_of_every_file() {
    let dir = crate::support::tmp_root().join("satex-load-tree-test");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("part.tex"), "\\def\\b{2}\n").unwrap();
    let main = dir.join("main.tex");
    std::fs::write(&main, format!("{PRELUDE}\\input{{part}}\n")).unwrap();
    let cfg = Config {
        load: Load { packages: false, classes: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    let source = std::fs::read_to_string(&main).unwrap();
    let analysis = Machine::analyze(&source, Some(&main), &cfg);
    let root = query::summary(&analysis);
    assert!(root.path.ends_with("main.tex"), "got {:?}", root.path);
    let part = root.children.first().expect("the input is a child of the main file");
    assert_eq!(part.status, "read");
    assert!(part.path.ends_with("part.tex"), "got {:?}", part.path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_skip_begun_in_another_file_skips_on_as_tex_does() {
    // TeX skips `\opener`'s `\iffalse` on through main.tex to its `\else`.
    // With no gap before it satex knows the skip is TeX's own, the way
    // versions.sty and comment.sty skip the document's text on purpose, and
    // follows only that path (SEMANTICS.md § 14).
    let dir = crate::support::tmp_root().join(format!("satex-skip-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("opener.tex"), "\\def\\opener{\\iffalse}\n").unwrap();
    let main = dir.join("main.tex");
    std::fs::write(
        &main,
        format!("{PRELUDE}\\input opener \\opener\\def\\a{{}}\\else\\def\\b{{}}\\fi\\def\\after{{}}\n"),
    )
    .unwrap();
    let cfg = Config {
        load: Load { packages: false, classes: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    let source = std::fs::read_to_string(&main).unwrap();
    let analysis = Machine::analyze(&source, Some(&main), &cfg);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(definition(&analysis, "a").is_none(), "the skipped text defines nothing");
    assert!(definition(&analysis, "b").expect("the false text").certain);
    assert!(definition(&analysis, "after").expect("after the conditional").certain);
    assert!(diagnosed(&analysis, "conditional-crosses-file").is_empty());
}

#[test]
fn a_terminal_read_in_nonstop_mode_ends_the_job() {
    // tex.web § 484: `*** (cannot \read from terminal in nonstop modes)`.
    let analysis = analyze(r"\nonstopmode\read-1 to\x \def\after{}");
    assert_eq!(diagnosed(&analysis, "terminal-read-fatal").len(), 1);
    assert!(definition(&analysis, "after").is_none());
    let analysis = analyze(r"\errorstopmode\read-1 to\x \def\after{}");
    assert!(definition(&analysis, "after").is_some());
}

#[test]
fn a_literal_parameter_character_is_shown_doubled() {
    let analysis = analyze(r"\def\outer#1{\def\inner##1{[#1|##1]}}");
    assert_eq!(body(&analysis, "outer"), r"\def \inner ##1{[#1|##1]}");
    let nested = analyze(r"\def\nest#1{\def\mid##1{\def\deep####1{(#1,##1,####1)}}}");
    assert_eq!(body(&nested, "nest"), r"\def \mid ##1{\def \deep ####1{(#1,##1,####1)}}");
}

#[test]
fn edef_runs_the_gullet_primitives() {
    let analysis =
        analyze(r"\def\body{zz}\edef\a{\csname body\endcsname}\edef\b{\romannumeral 2024}\edef\c{\the\catcode`\~}");
    assert_eq!(body(&analysis, "a"), "zz");
    assert_eq!(body(&analysis, "b"), "mmxxiv");
    // IniTeX's `~` is other, not active (tex.web § 232; pdftex -ini -etex agrees).
    assert_eq!(body(&analysis, "c"), "12");
}

#[test]
fn let_takes_the_space_after_one_optional_space() {
    let analysis =
        analyze("\\def\\:{\\let\\spce= } \\: \\ifx\\spce\\relax\\def\\seen{relax}\\else\\def\\seen{space}\\fi");
    assert_eq!(body(&analysis, "seen"), "space");
}

#[test]
fn ifcat_sees_through_noexpand() {
    let analysis =
        analyze(r"\def\test#1{\ifcat\noexpand~\noexpand#1\def\seen{active}\else\def\seen{control}\fi}\test\relax");
    assert_eq!(body(&analysis, "seen"), "control");
}

#[test]
fn an_implicit_brace_opens_and_closes_a_group() {
    // tex.web § 1063: a control sequence whose meaning is a begin-group or
    // end-group character acts exactly like the character.
    let analysis = analyze(r"\let\ob={ \let\cb=} \ob\def\w{1}\cb\def\after{2}");
    assert_eq!(depth_of(&analysis, "w"), Some(1));
    assert_eq!(depth_of(&analysis, "after"), Some(0));
}

#[test]
fn bgroup_and_egroup_are_a_group() {
    // plain.tex: `\let\bgroup={ \let\egroup=}`.
    let analysis = analyze(r"\let\bgroup={ \let\egroup=} \bgroup\def\d{1}\egroup");
    assert_eq!(depth_of(&analysis, "d"), Some(1));
}

#[test]
fn math_shift_opens_a_group() {
    // tex.web § 1145: `init_math` pushes a `math_shift_group`.  IniTeX's `$`
    // is other (§ 232), so this test sets it to math shift itself.
    let analysis = analyze(r"\catcode`\$=3 $\def\e{1}$\def\after{2}");
    assert_eq!(depth_of(&analysis, "e"), Some(1));
    assert_eq!(depth_of(&analysis, "after"), Some(0));
}

#[test]
fn display_math_is_one_group() {
    // tex.web § 1145 reads the second `$` when display math starts and
    // § 1197 asks for it again at the end, so `$$…$$` is a single group.
    // IniTeX's `$` is other (§ 232), so this test sets it to math shift itself.
    let analysis = analyze(r"\catcode`\$=3 $$\def\dd{1}$$\def\after{2}");
    assert_eq!(depth_of(&analysis, "dd"), Some(1));
    assert_eq!(depth_of(&analysis, "after"), Some(0));
}

#[test]
fn aftergroup_reinserts_its_tokens_when_the_group_ends() {
    // tex.web § 326 saves the token; § 282 puts the saved tokens back in the
    // order they were given.
    let analysis = analyze(r"\begingroup\aftergroup\gdef\aftergroup\tt\aftergroup{\aftergroup2\aftergroup}\endgroup");
    assert_eq!(body(&analysis, "tt"), "2");
    assert!(definition(&analysis, "tt").is_some_and(|d| d.global));
}

#[test]
fn a_closer_of_the_wrong_kind_is_reported() {
    // tex.web § 269 keeps the group kind, § 1069 complains about a `}` that
    // meets a `\begingroup` group and § 1064 about the other way round.
    let simple = analyze(r"{\endgroup");
    assert_eq!(diagnosed(&simple, "mismatched-group").len(), 1, "\\endgroup cannot close `{{`");
    let semi = analyze(r"\begingroup}");
    assert_eq!(diagnosed(&semi, "mismatched-group").len(), 1, "`}}` cannot close \\begingroup");
    let matched = analyze(r"{\def\a{1}}\begingroup\def\b{2}\endgroup");
    assert!(diagnosed(&matched, "mismatched-group").is_empty(), "matched closers are quiet");
}

#[test]
fn a_file_that_ends_inside_a_group_is_reported() {
    // tex.web § 1335 reports the groups still open when the input ends.
    let analysis = analyze(r"{\def\z{1}");
    let reported = diagnosed(&analysis, "unbalanced-file");
    assert_eq!(reported.len(), 1);
    assert!(reported[0].contains("at level 1; opened by { at"), "{}", reported[0]);
    assert!(diagnosed(&analyze(r"{\def\z{1}}"), "unbalanced-file").is_empty());
}

#[test]
fn endinput_ends_the_file_and_not_the_expansion() {
    // tex.web § 362 sets `force_eof` for the file being read; the token list
    // that contained the `\endinput` runs to its end first (tex.web § 360).
    let analysis = analyze("\\def\\stop{\\endinput\\def\\reached{1}}\\stop\n\\def\\after{2}");
    assert_eq!(body(&analysis, "reached"), "1");
    assert!(definition(&analysis, "after").is_none(), "the file ended");
}

#[test]
fn csname_stops_at_a_token_it_cannot_expand() {
    // tex.web § 372: the scan ends at the first token expansion cannot
    // remove, and the missing `\endcsname` is inserted there.
    let analysis = analyze(r"\expandafter\def\csname name\relax{body}\def\after{2}");
    assert_eq!(body(&analysis, "after"), "2", "the scan did not eat the rest of the file");
}

#[test]
fn a_helper_used_again_further_down_is_not_a_loop() {
    // The per-site bound exists for loops, which repeat without reading
    // anything new; a helper called once per source line makes progress
    // every time and must keep expanding.
    let mut source = String::from(r"\def\helper#1{\expandafter\def\csname n#1\endcsname{#1}}");
    for i in 0..64 {
        source.push_str(&format!("\\helper{{{i}}}"));
    }
    let analysis = analyze(&source);
    assert!(definition(&analysis, "n63").is_some(), "the 64th call still expanded");
}

#[test]
fn a_false_arm_is_skipped_without_being_read() {
    let analysis = analyze(r"\iffalse\def\no{1}\else\def\yes{2}\fi");
    assert!(definition(&analysis, "yes").is_some());
    assert!(definition(&analysis, "no").is_none());
}

#[test]
fn ifcase_takes_the_numbered_arm() {
    let analysis = analyze(r"\ifcase 2 \def\a{}\or\def\b{}\or\def\c{}\else\def\d{}\fi");
    assert_eq!(names(&analysis), vec!["c".to_string()]);
}

#[test]
fn an_ifcase_skips_past_the_conditionals_its_selector_left_open() {
    // tex.web § 509: the number scan leaves three conditionals open, and
    // their `\fi` pops one each while the skip to the third `\or` goes on.
    // expl3's `\__fp_sanitize:Nw` is written exactly this way.
    let analysis = analyze(
        r"\ifcase\ifnum1>2 1\else\ifnum1>2 2\else\ifx AA3 \fi\fi\fi0 \def\zero{}\or\def\one{}\or\def\two{}\or\def\three{}\else\def\other{}\fi",
    );
    assert_eq!(names(&analysis), vec!["three".to_string()]);
}

#[test]
fn a_switch_asked_about_governs_both_arms() {
    // `satex controls \ifdraft` makes that one switch undecidable, which is
    // what lets the reached-through-\csname arm be recorded as well.
    let cfg = Config {
        load: Load { packages: false, classes: false, inputs: false, ..Default::default() },
        opaque_conditionals: vec!["ifdraft".into()],
        ..crate::support::config()
    };
    let analysis = Machine::analyze(r"\newif\ifdraft\csname ifdraft\endcsname\def\a{}\else\def\b{}\fi", None, &cfg);
    let defined = names(&analysis);
    assert!(defined.iter().any(|n| n == "a"), "then arm: {defined:?}");
    assert!(defined.iter().any(|n| n == "b"), "else arm: {defined:?}");
    let governed = query::controls(&analysis, "ifdraft");
    let macros: Vec<_> = governed.iter().filter(|r| r["tag"] == "macro").collect();
    assert_eq!(macros.len(), 2, "both arms are recorded: {governed:?}");
}

#[test]
fn every_switch_means_the_documents_own() {
    // `satex controls` without a name leaves the document's switches
    // undecided, and only those: the kernel's `\if@tempswa` and the like
    // keep deciding the code they drive.
    if which::which("kpsewhich").is_err() {
        return;
    }
    let cfg = Config {
        load: Load { classes: true, ..Default::default() },
        opaque_conditionals: vec![satex::config::EVERY_SWITCH.into()],
        ..crate::support::config()
    };
    let analysis = Machine::analyze(
        r"\documentclass{article}
\newif\ifdraft
\ifdraft\def\a{}\else\def\b{}\fi
\begin{document}\end{document}",
        None,
        &cfg,
    );
    let defined = names(&analysis);
    assert!(defined.iter().any(|n| n == "a") && defined.iter().any(|n| n == "b"), "{defined:?}");
    let switches = query::switches(&analysis, false);
    assert!(switches.iter().any(|r| r["name"] == "\\ifdraft"), "{switches:?}");
    let gaps = query::run(&analysis, Query::Gaps, &Filter::Always);
    assert!(gaps.is_empty(), "{gaps:?}");
}

/// The text the last `\edef\out` or `\def\out` of a run stored, for checking
/// satex against what `etex -ini` stores for the same source.
fn out(source: &str) -> String {
    let analysis = analyze(source);
    analysis
        .facts
        .defs
        .iter()
        .rev()
        .find(|d| analysis.interner.name(d.name) == "out")
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

/// Probes and the text each one leaves in `\out`.
const OUTS: &[crate::support::PairRow] = &[
    // etex: `\ a` and `\a{b}`
    ("csname_keeps_spaces_and_braces_in_the_name", &[
        (r"\def\sp{ }\edef\out{\expandafter\string\csname\sp a\endcsname}", r"\ a"),
        (r"\edef\out{\expandafter\string\csname a{b}\endcsname}", r"\a{b}"),
    ]),
    // etex: `Y`, then `N` for a name cut short by an unexpandable token
    ("ifcsname_expands_the_name_it_tests", &[
        (r"\def\a{xx}\def\xx{}\edef\out{\ifcsname\a\endcsname Y\else N\fi}", "Y"),
        (r"\edef\out{\ifcsname\relax\endcsname Y\else N\fi}", "N"),
    ]),
    // etex: `x`; the `0` belongs to `\ifvoid`
    ("box_tests_read_their_register_number", &[(r"\edef\out{\ifvoid0 \fi x}", "x")]),
    // etex: `\a `
    ("the_token_register_is_not_expanded_again_in_edef", &[(r"\def\a{xx}\toks0={\a}\edef\out{\the\toks0}", r"\a ")]),
    // etex: `\a `
    ("unexpanded_expands_to_find_its_brace", &[
        (r"\def\a{xx}\def\b{\a}\edef\out{\unexpanded\expandafter{\b}}", r"\a "),
    ]),
    // etex: `7`
    ("the_expands_its_operand", &[(r"\countdef\c=5 \c=7 \edef\out{\the\csname c\endcsname}", "7")]),
    // etex: `relax`, `relax`, `/long macro:->/b `, `a `
    ("escapechar_governs_string_meaning_and_detokenize", &[
        (r"\escapechar=-1 \edef\out{\string\relax}", "relax"),
        (r"\escapechar=300 \edef\out{\string\relax}", "relax"),
        (r"\long\def\a{\b}\escapechar=`\/ \edef\out{\meaning\a}", "/long macro:->/b "),
        (r"\escapechar=-1 \edef\out{\detokenize{\a}}", "a "),
    ]),
    // etex: `macro:#1{->x{`
    ("meaning_of_a_brace_delimited_macro_shows_the_brace_once", &[
        (r"\def\d#1#{x}\edef\out{\meaning\d}", "macro:#1{->x{"),
    ]),
    // etex: `\csname\endcsname`
    ("the_empty_control_sequence_prints_as_csname_endcsname", &[
        (r"\edef\out{\expandafter\string\csname\endcsname}", r"\csname\endcsname"),
    ]),
    // etex: `a b` — the first line already ended in `^^M` when
    // `\endlinechar=-1` was executed.
    ("endlinechar_is_fixed_when_the_line_is_read", &[("\\endlinechar=-1 \\edef\\out{a\nb}", "a b")]),
    // etex: `x\^^My` — the escape at the end of the line names `^^M`, and the
    // next line's leading spaces are skipped.
    ("every_line_starts_in_state_n", &[("\\def\\out{x\\\n   y}", "x\\^^My")]),
    // etex: `-5`, `-2.0pt`, `-2.0pt`
    ("signs_apply_to_internal_quantities", &[
        (r"\count1=5 \edef\out{\number-\count1}", "-5"),
        (r"\dimen1=2pt \edef\out{\the\dimexpr-\dimen1\relax}", "-2.0pt"),
        (r"\dimen1=2pt \dimen2=-\dimen1 \edef\out{\the\dimen2}", "-2.0pt"),
    ]),
    // etex: `72.26999pt`; an integer register as unit counts in sp and the
    // unit after it is left over: `0.0001ptpt\relax `
    ("dimensions_follow_the_engine_units_and_factors", &[
        (r"\edef\out{\the\dimexpr 1truein\relax}", "72.26999pt"),
        (r"\count1=5 \edef\out{\the\dimexpr 1.5\count1 pt\relax}", r"0.0001ptpt\relax "),
        (r"\count1=5 \edef\out{\number\count1\count1}", r"5\count 1"),
    ]),
    // etex: `2.0pt plus 1.0pt`, `3.0mu plus 1.0fill`, `abc,abcx`
    ("glue_and_token_parameters_hold_their_kind", &[
        (r"\parskip=2pt plus 1pt \edef\out{\the\parskip}", "2.0pt plus 1.0pt"),
        (r"\thinmuskip=3mu plus 1fill \edef\out{\the\thinmuskip}", "3.0mu plus 1.0fill"),
        (r"\everypar{abc}\toks3=\expandafter{\the\everypar x}\edef\out{\the\everypar,\the\toks3}", "abc,abcx"),
    ]),
    // etex: `YN`; and `N`, because the skipped `\ifpdfabsnum` needs its own `\fi`
    ("pdftex_absolute_conditionals_are_conditionals", &[
        (r"\edef\out{\ifpdfabsnum -5>3 Y\else N\fi\ifpdfabsdim -2pt<1pt Y\else N\fi}", "YN"),
        (r"\edef\out{\iffalse\ifpdfabsnum 1=1 \fi Y\else N\fi}", "N"),
    ]),
    // etex: `2.6,140`, `0`, `1`, `Y`, `1`
    ("engine_versions_and_levels_are_known", &[
        (r"\edef\out{\the\eTeXversion\eTeXrevision,\the\pdftexversion}", "2.6,140"),
        (r"\edef\out{\the\currentgrouplevel}", "0"),
        (r"{\xdef\out{\the\currentgrouplevel}}", "1"),
        (r"\edef\out{\ifnum\currentiflevel=1 Y\else N\fi}", "Y"),
        (r"\edef\out{\the\inputlineno}", "1"),
    ]),
    // etex: `\mathchar"7123` and `N`
    ("mathchardef_is_a_meaning_of_its_own", &[
        (r#"\mathchardef\mc="7123 \edef\out{\meaning\mc}"#, r#"\mathchar"7123"#),
        (r"\chardef\a=1 \mathchardef\b=1 \edef\out{\ifx\a\b Y\else N\fi}", "N"),
    ]),
    // etex: `\font \x \openin 1\message {a}`
    ("stomach_primitives_stay_unexpanded_in_edef", &[
        (r"\let\x\relax \edef\out{\font\x\openin1\message{a}}", r"\font \x \openin 1\message {a}"),
    ]),
    // etex: `a b`
    ("endlinechar_is_restored_by_the_end_of_a_group", &[("{\\endlinechar=-1 }\n\\edef\\out{a\nb}", "a b")]),
    // etex: `G`, `N`, `4`
    ("globaldefs_overrides_the_prefix", &[
        (r"{\globaldefs=1 \def\gd{G}}\edef\out{\gd}", "G"),
        (r"\globaldefs=-1 {\global\def\ge{E}}\globaldefs=0 \edef\out{\ifdefined\ge Y\else N\fi}", "N"),
        (r"\count1=1 {\globaldefs=1 \count1=4 }\edef\out{\the\count1}", "4"),
    ]),
    // etex: `AB`, and a `~` of category 12 in `\csname` names the control symbol `\~`
    ("active_characters_and_control_symbols_are_distinct", &[
        (r"\catcode`\~=13 \def~{A}\def\~{B}\edef\out{~\~}", "AB"),
        (r"\catcode`\~=13 \def~{A}\def\~{B}\edef\out{\csname\string~\endcsname}", "B"),
    ]),
    // etex: `inbox:b` — the token runs at the start of the box's contents
    ("afterassignment_after_setbox_goes_inside_the_box", &[
        (r"\def\out{}\def\b{\edef\out{\out b}}\afterassignment\b\setbox0\hbox{\edef\out{inbox:\out}}", "inbox:b"),
    ]),
    // etex: `macro:#1->##1a#1`: tex.web § 478 appends those tokens past the
    // parameter scan, so their `#` stays a character (expl3's
    // `\tl_put_right:Nn` builds token lists of code that way).
    ("edef_stores_what_the_and_unexpanded_give_without_parameters", &[
        (r"\toks0{#1}\edef\y#1{\the\toks0\unexpanded{a}#1}\edef\out{\meaning\y}", "macro:#1->##1a#1"),
    ]),
    // etex: `aftergroup TF-1` (no escape character once it is -1): `\let` keeps which primitive it copied, so
    // expl3's `\tex_escapechar:D` sets `\escapechar` itself.
    ("a_copy_of_a_primitive_is_that_primitive", &[(r"\def\space{ }\let\a\aftergroup \let\e\escapechar \e=-1
            \edef\out{\meaning\a\space\ifx\a\aftergroup T\else F\fi
              \ifx\a\afterassignment T\else F\fi
              \expandafter\ifx\csname new\endcsname\relax T\else F\fi\the\escapechar}", "aftergroup TFT-1")]),
    // etex: `-1`: the group's `\let` ends with the group, so `\e` is a copy
    // of `\escapechar` again (kvsetkeys' local `\let\ifincsname\iftrue`).
    ("a_copy_made_in_a_group_ends_with_it", &[(r"\let\e\escapechar \begingroup\let\e\endlinechar\endgroup \e=-1
            \edef\out{\the\escapechar}", "-1")]),
    // etex: `3|21`: e-TeX reads the next non-blank non-call token.
    ("an_expression_expands_to_find_its_operators", &[
        (r"\def\a{(4-1)}\edef\out{\number\numexpr\a\relax|\number\numexpr 2*\a*\a+\a\relax}", "3|21"),
    ]),
    // etex: `1`: expl3's `\int_gincr:N` on a fresh `\newcount` register.
    ("a_register_starts_at_zero_for_advance", &[(r"\countdef\x=30 \global\advance\x by 1 \edef\out{\the\x}", "1")]),
    // etex: `ab`: tex.web § 477 scans the body while expanding it, so the
    // `}` skipped by `\iffalse` does not end it (expl3's `\__iow_wrap_line:nw`).
    ("edef_body_ends_at_the_brace_expansion_balances", &[(r"\edef\out{\iffalse}\fi a\iffalse{\fi b}", "ab")]),
    // etex: `{ab}`: pdfTeX's `\expanded` reads like an `\edef` body, so the
    // `}}` its `\iffalse` skips do not end it (expl3's `\__keys_property_find_auxii:w`).
    ("expanded_scans_its_text_while_expanding", &[(r"\edef\out{\expanded{{a\iffalse}}\fi b}}}", "{ab}")]),
    // etex: `select font cmr10 at 0.00003pt|select font nullfont`
    // (tex.web § 1261); expl3's cctabs and intarrays test for it.
    ("a_font_identifier_means_the_font_it_selects", &[
        (r"\font\a=cmr10 at 2sp \edef\out{\meaning\a|\meaning\nullfont}", "select font cmr10 at 0.00003pt|select font nullfont"),
    ]),
    // etex: `\cs_set_eq:NN \a \relax x|undefined`: `\cs_set_eq:NN` is
    // `\protected` (as in l3basics), so the `\edef` stores it and defines
    // nothing. Undefined, it would expand to nothing (tex.web § 370).
    ("an_edef_copies_the_expl3_definition_functions", &[(r"\catcode`\_=11 \catcode`\:=11
            \protected\def\cs_set_eq:NN#1#2{\let#1=#2}
            \edef\t{\cs_set_eq:NN \noexpand\a \relax x}
            \edef\out{\meaning\t|\meaning\a}", r"macro:->\cs_set_eq:NN \a \relax x|undefined")]),
    // tex: `[4][7][7]`: a local assignment saves the old meaning once per
    // level, and one made global since is retained when the group ends
    // (tex.web §§ 277, 283).
    ("a_group_saves_a_name_once_and_keeps_a_global_one", &[(r"\def\a{1}{\def\a{2}\def\a{3}\gdef\a{4}\def\a{5}}\edef\x{[\a]}
            {\def\a{6}{\gdef\a{7}}\xdef\y{[\a]}}\edef\out{\x\y[\a]}", "[4][7][7]")]),
    // etex: `a`: latex.ltx opens with `\ifnum…\errmessage{…}\fi`; the
    // engine primitive has no help text to read, so the `\else` stays.
    ("errmessage_takes_only_its_message", &[(r"\def\out{a}\iftrue\errmessage{oops}\else\def\out{b}\fi", "a")]),
    // etex: `\ab`: expanding an undefined control sequence reports it and
    // removes it (tex.web § 370), so the name goes on to `\endcsname`.
    ("an_undefined_name_inside_csname_is_dropped", &[
        (r"\edef\out{\expandafter\string\csname a\undefinedthing b\endcsname}", r"\ab"),
    ]),
    // etex: `Y`: latex.ltx's `\@ifnextchar` skips a space by `\@xifnch`,
    // whose parameter text is one space; the line end is tokenized with the
    // catcode in force when the reader reaches it (tex.web § 343), here 10.
    ("a_line_end_made_a_space_is_skipped_by_futurelet_and_a_space_delimited_macro", &[("\\catcode`\\@=11 \\catcode`\\[=12
\\def\\:{\\let\\@sptoken= } \\:
\\def\\:{\\@xifnch} \\expandafter\\def\\: {\\futurelet\\@let@token\\@ifnch}
\\def\\@ifnch{\\ifx\\@let@token\\@sptoken \\let\\reserved@c\\@xifnch \\else \\ifx\\@let@token[\\def\\r{Y}\\else\\def\\r{N}\\fi \\let\\reserved@c\\relax\\fi \\reserved@c}
\\def\\t{\\futurelet\\@let@token\\@ifnch}
\\begingroup\\catcode13=10 \\t
 [x]\\global\\let\\r\\r\\endgroup
\\edef\\out{\\r}", "Y")]),
];

#[test]
fn out_agrees_with_tex() {
    crate::support::pair_rows(OUTS, |source, expected| {
        let got = out(source);
        (got != expected).then(|| format!("\\out is {got:?}, expected {expected:?}"))
    });
}

/// Which gaps make an expansion give up on the text it assembled, and which
/// leave it standing.  The two sides are not the same kind of thing, and
/// `WIDEN_ON` in src/machine.rs is the list that tells them apart.
///
/// A gap satex invented: the scan for `\stop` meets a `}` after text it
/// could not follow, so where the argument ended is satex's guess and the
/// text is not what the engine makes.  What is not known stays unknown.
///
/// An error the engine makes too, whose recovery tex.web defines: `\csname`
/// that runs into an unexpandable token is "Missing \endcsname inserted"
/// (tex.web § 373).  The engine inserts the `\endcsname` and reads on, and
/// satex inserts the same one, so the text satex assembled *is* the text the
/// engine makes -- `etex -ini` stores `N` here.  An argument that runs into
/// the `}` of an `\edef` body is "Argument of \d has an extra }" (§ 395):
/// `\par` is inserted before the brace, the call ends in a runaway (§ 396)
/// and the `\par` is stored, so `\out` is `\par`.  Widening either would
/// throw away an answer satex had right.
#[test]
fn an_expansion_gives_up_only_over_a_gap_satex_invented() {
    let invented = r"\count0=\pdfuniformdeviate 9 \catcode`\Q=\count0 \def\d#1\stop{[#1]}\edef\out{\d zQ}";
    assert_eq!(diagnosed(&analyze(invented), "argument-ends-in-unknown").len(), 1);
    assert!(out(invented).contains("unknown"), "{:?}", out(invented));

    // etex: `N`.
    let engines = r"\edef\out{\ifcsname\relax\endcsname Y\else N\fi}";
    assert_eq!(diagnosed(&analyze(engines), "missing-endcsname").len(), 1);
    assert_eq!(out(engines), "N");

    // etex: `\par `.
    let brace = r"\def\d#1\stop{[#1]}\edef\out{\d z}";
    let analysis = analyze(brace);
    for code in ["extra-right-brace", "runaway-argument"] {
        assert_eq!(diagnosed(&analysis, code).len(), 1, "{code}");
    }
    assert_eq!(out(brace), "\\par ");
}

/// tex.web § 392: only the text before the first delimiter is free of it.
/// Unknown text split at `macro:` leaves a rest that may still hold `->`,
/// so expl3's `\__cs_prefix_arg_replacement:wN` may find a replacement
/// text in the `\meaning` of an unknown name.
#[test]
fn the_rest_of_split_unknown_text_may_hold_the_next_delimiter() {
    let source = r"\edef\next{\def\noexpand\parse##1\detokenize{macro:}##2\detokenize{->}##3\noexpand\stop
            {\edef\noexpand\out{\noexpand\if\relax\noexpand\detokenize{##3}\relax E\noexpand\else N\noexpand\fi}}}\next
        \expandafter\let\expandafter\x\csname u\number\pdfuniformdeviate 9\endcsname
        \expandafter\parse\meaning\x\stop";
    let analysis = analyze(source);
    let outs: Vec<String> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| analysis.interner.name(d.name) == "out")
        .filter_map(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .collect();
    // `E` alone (an empty replacement text for sure) or no `\out` at all
    // (a runaway) drops the paths the engine may take.
    assert!(outs.iter().any(|o| o != "E"), "{outs:?}");
}

/// tex.web § 392: the argument read so far ends in `\mark`, and unknown
/// digits may begin with the `1` that completes the delimiter `\mark1`.
/// A path ends the argument before `\mark`; none ends it inside the digits
/// with the `\mark` kept in the argument.
#[test]
fn a_delimiter_may_begin_before_unknown_text_and_end_in_it() {
    let source = r"\def\p#1\mark1#2\stop{\edef\out{[\detokenize{#1}]}}
        \edef\n{\number\pdfuniformdeviate 20}
        \expandafter\p\expandafter\mark\n\mark1\stop";
    let analysis = analyze(source);
    let outs: Vec<String> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| analysis.interner.name(d.name) == "out")
        .filter_map(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .collect();
    assert!(outs.iter().any(|o| o == "[]"), "{outs:?}");
}

#[test]
fn a_brace_after_no_parameter_is_a_delimiter_put_back() {
    // etex: `xy`, `wv`.  tex.web § 476: `\def\m#{…}` matches the `{` and
    // the replacement text restores it (pdftexcmds' `\pdf@filemdfivesum`).
    assert_eq!(out(r"\def\m#{\detokenize}\edef\out{\m{x}y}"), "xy");
    assert_eq!(out(r"\def\n z#{\detokenize}\edef\out{\n z{w}v}"), "wv");
    let analysis = analyze(r"\def\m#{\detokenize}\edef\foo{\m{a}}\def\after{}");
    assert!(definition(&analysis, "after").is_some());
}

/// `\expandafter\first\else\expandafter\second\fi{A}{B}`: each arm takes its
/// own argument, and the paths meet again after both arguments.
#[test]
fn undecided_arms_meet_after_the_arguments_they_take() {
    let analysis = analyze(
        r"\long\def\first#1#2{\def\r{#1}}\long\def\second#1#2{\def\r{#2}}
          \ifnum\pdfuniformdeviate2=0 \expandafter\first\else\expandafter\second\fi{A}{B}\def\after{done}",
    );
    let bodies: Vec<String> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| analysis.interner.name(d.name) == "r")
        .filter_map(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .collect();
    assert!(bodies.contains(&"A".to_string()) && bodies.contains(&"B".to_string()), "{bodies:?}");
    assert_eq!(analysis.facts.defs.iter().filter(|d| analysis.interner.name(d.name) == "after").count(), 1);
}

/// Macros whose texts begin with tokens of disjoint classes differ: unknown
/// digits are never `\egroup`, so `\ifx` decides false (pgfmathparser).
#[test]
fn ifx_on_digits_and_a_control_sequence_decides_false() {
    let analysis = analyze(
        r"\edef\a{\pdfuniformdeviate 100}\def\e{\egroup}\let\l\egroup
          \ifx\a\e\def\same{}\else\def\differ{}\fi
          \ifx\a\l\def\samel{}\else\def\differl{}\fi",
    );
    let defined = names(&analysis);
    for (yes, no) in [("differ", "same"), ("differl", "samel")] {
        assert!(defined.contains(&yes.to_string()) && !defined.contains(&no.to_string()), "{yes}: {defined:?}");
    }
}

/// A name the arms define differently has no known meaning after the join,
/// so a test on it is undecided in turn and both of its arms run.
#[test]
fn a_joined_meaning_leaves_later_tests_undecided() {
    let analysis = analyze(
        r"\def\a{1}\ifnum\pdfuniformdeviate2=0 \def\m{1}\else\def\m{2}\fi
          \ifx\m\a\def\same{yes}\else\def\other{yes}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"same".to_string()) && defined.contains(&"other".to_string()));
}

fn analyze_splitting(source: &str) -> Analysis {
    analyze_with_splits(source, 8)
}

fn analyze_with_splits(source: &str, splits: u16) -> Analysis {
    let mut cfg = Config {
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    cfg.limits.meaning_splits = splits;
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

const DISPATCH: &str = r"\def\a{\def\resA{yes}}\def\b{\def\resB{yes}}
    \def\dispatch{\ifnum\pdfuniformdeviate2=0 \let\next\a\else\let\next\b\fi\next}";

/// `\ifx … \let\next\a \else \let\next\b \fi \next`: after the join `\next`
/// is `\a` or `\b`, not unknown, and running it runs each on a path of its
/// own, so what either defines is there.
#[test]
fn a_let_dispatch_after_an_undecided_test_runs_every_meaning() {
    let analysis = analyze_splitting(&format!(r"{DISPATCH}\dispatch\def\after{{done}}"));
    let defined = names(&analysis);
    assert!(defined.contains(&"resA".to_string()) && defined.contains(&"resB".to_string()), "{defined:?}");
    assert_eq!(analysis.facts.defs.iter().filter(|d| analysis.interner.name(d.name) == "after").count(), 1);
}

/// Without splitting the joined `\next` has an unknown meaning, and
/// running it defines neither.
#[test]
fn a_let_dispatch_without_splitting_is_unknown() {
    let names = names(&analyze_with_splits(&format!(r"{DISPATCH}\dispatch"), 0));
    assert!(!names.contains(&"resA".to_string()) && !names.contains(&"resB".to_string()), "{names:?}");
}

/// `\ifx` of a name a join left with several meanings is decided for each
/// of them: `\next` is `\a` or `\b`, never `\relax`.
#[test]
fn ifx_on_a_joined_meaning_is_decided_per_meaning() {
    let analysis = analyze_splitting(
        r"\def\a{A}\def\b{B}\ifnum\pdfuniformdeviate2=0 \let\next\a\else\let\next\b\fi
          \ifx\next\a\def\wasA{1}\else\def\wasB{1}\fi
          \ifx\next\relax\def\wasRelax{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"wasA".to_string()) && defined.contains(&"wasB".to_string()), "{defined:?}");
    assert!(!defined.contains(&"wasRelax".to_string()), "{defined:?}");
}

/// A dispatch loop whose exit is undecided ends: the name is the loop or
/// its exit, and the loop is taken one more pass.
#[test]
fn a_dispatch_loop_on_a_joined_meaning_ends() {
    let analysis = analyze_splitting(
        r"\def\loop{\ifnum\pdfuniformdeviate2=0 \let\next\loop\else\let\next\relax\fi\next}\loop\def\after{done}",
    );
    assert!(names(&analysis).contains(&"after".to_string()));
}

/// A `\global` assignment on one path is undone before the next path runs.
#[test]
fn a_global_assignment_stays_on_its_path() {
    let analysis = analyze(r"\ifnum\pdfuniformdeviate2=0 \global\let\g\relax\else\ifx\g\relax\def\leak{yes}\fi\fi");
    assert!(!names(&analysis).contains(&"leak".to_string()));
}

/// The `\read` stream a path consumes is its own.
#[test]
fn a_path_does_not_consume_the_input_of_another() {
    let analysis = analyze(r"\ifnum\pdfuniformdeviate2=0 \def\x{1}\else\def\x{2}\fi\def\y{3}");
    assert_eq!(analysis.facts.defs.iter().filter(|d| analysis.interner.name(d.name) == "y").count(), 1);
}

/// Every arm of an undecided `\ifcase` runs as its own path.
#[test]
fn an_undecided_ifcase_runs_every_arm() {
    let analysis = analyze(r"\ifcase\lastpenalty\def\a{0}\or\def\b{1}\else\def\c{2}\fi\def\d{3}");
    let defined = names(&analysis);
    for name in ["a", "b", "c"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
    assert_eq!(defined.iter().filter(|n| *n == "d").count(), 1);
}

#[test]
fn glue_is_scanned_with_its_keywords_and_orders() {
    // etex: `1.0pt plus 1.0fill minus 2.0fill`, then negated, then coerced
    let set = r"\skip3=1pt plus 1fil l minus 2 fill ";
    assert_eq!(out(&format!(r"{set}\edef\out{{\the\skip3}}")), "1.0pt plus 1.0fill minus 2.0fill");
    assert_eq!(out(&format!(r"{set}\skip4=-\skip3 \edef\out{{\the\skip4}}")), "-1.0pt plus -1.0fill minus -2.0fill");
    assert_eq!(out(&format!(r"{set}\dimen0=\skip3 \edef\out{{\the\dimen0}}")), "1.0pt");
    assert_eq!(out(r"\count1=5 \skip7=\count1 sp \edef\out{\the\skip7}"), "0.00008pt");
}

#[test]
fn etex_glue_accessors_read_their_glue() {
    // etex: `2.0pt,1,2,3.0pt` and `1.0mu plus 2.0fil minus 3.0fill`
    let set = r"\skip1=1pt plus 2fil minus 3fill ";
    assert_eq!(
        out(&format!(
            r"{set}\edef\out{{\the\gluestretch\skip1,\the\gluestretchorder\skip1,\the\glueshrinkorder\skip1,\the\glueshrink\skip1}}"
        )),
        "2.0pt,1,2,3.0pt"
    );
    assert_eq!(out(&format!(r"{set}\edef\out{{\the\gluetomu\skip1}}")), "1.0mu plus 2.0fil minus 3.0fill");
}

#[test]
fn a_font_table_is_extended_only_past_its_end() {
    // etex: `0.00018pt,0.0pt,42` — cmr10 has 7 parameters; the font loaded
    // last grows to 9, the new ones starting at zero.
    assert_eq!(
        out(
            r"\font\w=cmr10 at 7sp \fontdimen9\w=12sp \hyphenchar\w=42 \edef\out{\the\fontdimen9\w,\the\fontdimen8\w,\the\hyphenchar\w}"
        ),
        "0.00018pt,0.0pt,42"
    );
}

#[test]
fn pdftex_string_utilities_compute_their_result() {
    // pdftex: `-101,415A,AJ` and the MD5 of `abc`
    assert_eq!(
        out(
            r"\edef\out{\pdfstrcmp{abc}{abd}\pdfstrcmp{\relax}{\relax}\pdfstrcmp{b}{a},\pdfescapehex{AZ},\pdfunescapehex{414a}}"
        ),
        "-101,415A,AJ"
    );
    assert_eq!(out(r"\edef\out{\pdfmdfivesum{abc}}"), "900150983CD24FB0D6963F7D28E17F72");
}

#[test]
fn parshape_and_penalty_arrays_are_read_back() {
    // etex: `2,2.0pt,3.0pt,3.0pt,3.0pt,4.0pt` and `3,102,103`
    assert_eq!(
        out(
            r"\parshape 2 1pt 2pt 3pt 4pt \edef\out{\the\parshape,\the\parshapelength 1,\the\parshapeindent 2,\the\parshapedimen 3,\the\parshapedimen 7,\the\parshapelength 5}"
        ),
        "2,2.0pt,3.0pt,3.0pt,3.0pt,4.0pt"
    );
    assert_eq!(
        out(
            r"\interlinepenalties 3 101 102 103 \edef\out{\the\interlinepenalties0,\the\interlinepenalties2,\the\interlinepenalties9}"
        ),
        "3,102,103"
    );
}

#[test]
fn immediate_write_and_message_expand_their_text() {
    let analysis = analyze(r"\def\a{xy}\immediate\write16{[\a]}\message{<\a>}");
    let keys: Vec<_> = analysis.facts.occurrences.iter().map(|o| o.key.clone()).collect();
    assert!(keys.iter().any(|k| k == "[xy]"), "{keys:?}");
    assert!(keys.iter().any(|k| k == "<xy>"), "{keys:?}");
}

/// Inside an `\edef` body an undecided conditional cannot run both arms as
/// commands: the text it yields is unknown, so the definition is not certain.
#[test]
fn an_undecided_conditional_in_an_edef_yields_one_arm() {
    let analysis = analyze(r"\edef\x{\ifnum\pdfuniformdeviate2=0 a\else b\fi}");
    assert!(!definition(&analysis, "x").expect("x").certain);
}

#[test]
fn noexpand_marks_the_token_for_ifx_and_the_stomach() {
    // etex: `FFFT`: behind the marker a macro is `relax` with
    // `no_expand_flag` (tex.web § 358), equal to neither itself nor `\relax`;
    // expl3's `\cs_generate_variant:Nn` tells protected functions apart so.
    let probe = r"\def\a#1{}\protected\def\b{}\def\c{\def\out{ran}}
        \edef\out{\expandafter\ifx\noexpand\a\a T\else F\fi
          \expandafter\ifx\noexpand\b\b T\else F\fi
          \expandafter\ifx\noexpand\a\relax T\else F\fi
          \expandafter\ifx\noexpand\relax\relax T\else F\fi}\noexpand\c";
    assert_eq!(out(probe), "FFFT");
}

#[test]
fn a_conditional_its_test_opened_ends_first() {
    // etex: `F|T|F`: the inner `\ifx` is still open when `\if` decides, so
    // the first `\fi` skipped is its own (tex.web § 500); expl3's
    // `\__fp_parse_exponent:N` tests `\if:w e \if:w E …\fi:`.
    assert_eq!(
        out(
            r"\edef\out{\if e\ifx AB x\else y\fi T\else F\fi|\if y\ifx AB x\else y\fi T\else F\fi|\if x\ifx AA x\else y\fi T\else F\fi}"
        ),
        "F|T|F"
    );
}

#[test]
fn read_appends_the_end_of_line_character_and_balances_braces() {
    // etex: `a;b |{c d}e |\par `: tex.web § 483 appends `\endlinechar` to
    // every line and reads on until the braces balance; past the end the
    // line is empty (expl3 reads UnicodeData.txt with `\ior_map_variable`).
    let dir = crate::support::tmp_root().join(format!("satex-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("rd.dat");
    std::fs::write(&file, "a;b\n{c\nd}e\n").unwrap();
    let source = format!(
        r"\openin3={} \read3 to\a \read3 to\b \edef\e{{\ifeof3 E\else N\fi}}\read3 to\c
          \edef\out{{\meaning\a|\meaning\b|\meaning\c|\e\ifeof3 E\else N\fi}}",
        file.display()
    );
    // `\ifeof` turns true only once a read has found no line left.
    assert_eq!(out(&source), r"macro:->a;b |macro:->{c d}e |macro:->\par |NE");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_format_is_read_in_vertical_mode() {
    // pdflatex: latex.ltx typesets nothing, so its `\par` finds vertical
    // mode (tex.web § 211); `\ifhmode` in the preamble is false too.
    let analysis =
        analyze_kernel(r"\ifnum\pdfuniformdeviate2=0 \def\a{v}\else\def\a{x}\fi\ifhmode\def\b{h}\else\def\b{v}\fi");
    assert_eq!(body(&analysis, "a"), "v");
    assert_eq!(body(&analysis, "b"), "v");
}

#[test]
fn summary_defines_line_counts_document_concepts_separately() {
    let analysis = analyze_kernel(
        "\\newcommand{\\foo}{1}\n\
\\newenvironment{myenv}{begin}{end}\n\
\\newif\\ifbar\n\
\\begin{myenv}\n\\end{myenv}\n\
\\begin{myenv}\n\\end{myenv}\n",
    );
    let root = query::summary(&analysis);
    let text = satex::render::summary(&analysis, &root, satex::render::Detail::brief(), satex::render::Links(false));
    assert!(text.contains("defines"), "{text}");
    assert!(text.contains("1 command"), "{text}");
    assert!(text.contains("1 environment"), "{text}");
    assert!(text.contains("1 switch"), "{text}");
    assert!(text.contains("environments used"), "{text}");
    assert!(text.contains("myenv (2×)"), "{text}");
}

/// A switch joined again with a switch stays a conditional: the join of an
/// undecided switch with `\iffalse` is undecided, not an unknown token whose
/// `\else` and `\fi` would be extra (LaTeX's `\if@newlist` through `\par`).
#[test]
fn a_join_of_joined_switches_is_a_conditional() {
    let analysis = analyze(
        r"\let\ifs\iftrue \def\sfalse{\let\ifs\iffalse}
\ifnum\pdfuniformdeviate2=0 \sfalse\fi
\ifnum\pdfuniformdeviate2=0 \sfalse\fi
\ifs \def\a{1}\else \def\a{2}\fi",
    );
    assert!(diagnosed(&analysis, "extra-else").is_empty());
    assert!(diagnosed(&analysis, "extra-fi").is_empty());
}

/// Identical diagnostics at one place are one row with a count, so a loop
/// that repeats a mistake does not grow the report.
#[test]
fn identical_diagnostics_are_counted_not_repeated() {
    let analysis = analyze(r"\def\a{\fi}\a\a\a\a");
    let rows: Vec<_> = analysis.facts.diagnostics.iter().filter(|d| d.code == "extra-fi").collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].count, 4);
}

/// Unknown text may hold a delimiter: a delimited argument ends in it and
/// the rest of it is read next, once, instead of running past the end of
/// the input (TeX reads `a b` or `c d` here and finds both delimiters).
#[test]
fn unknown_text_may_hold_a_delimiter() {
    let analysis = analyze(
        r"\ifnum\pdfuniformdeviate2=0 \toks0{a b}\else\toks0{xc d}\fi
\edef\u{\the\toks0}
\def\first#1 #2\stop{\def\one{#1}\def\two{#2}}
\expandafter\first\u\stop
\def\g x#1\stop{\def\three{#1}}
\expandafter\g\u\stop
\def\R{done}",
    );
    for code in ["unbalanced-argument", "extra-right-brace", "runaway-argument", "file-ended"] {
        assert!(diagnosed(&analysis, code).is_empty(), "{code}");
    }
    for name in ["one", "two", "three", "R"] {
        assert!(definition(&analysis, name).is_some(), "{name}");
    }
}

/// A loop that takes unknown text apart at its spaces ends: the rest of
/// the text is split once (microtype's `\MT@rem@last@space`).
#[test]
fn a_loop_over_unknown_text_ends() {
    let analysis = analyze(
        r"\ifnum\pdfuniformdeviate2=0 \toks0{a b}\else\toks0{c d e}\fi
\edef\u{\the\toks0}
\def\strip#1>{}
\def\rem#1 #2{#1\ifx\stop#2\else\space\expandafter\rem\expandafter#2\fi}
\edef\R{\expandafter\expandafter\expandafter\rem\expandafter\strip\meaning\u \stop}",
    );
    assert!(diagnosed(&analysis, "unbalanced-argument").is_empty());
    assert!(definition(&analysis, "R").is_some());
}

/// A loop whose exit is undecided and whose counter is unknown: the loop
/// head joins every pass's state and widens what still grows, so the run
/// ends and what the loop defines on any pass is there, and the counter
/// is not claimed to be any one number afterwards.
#[test]
fn a_loop_with_an_unknown_bound_reaches_a_widened_fixpoint() {
    let analysis = analyze_splitting(
        r"\setbox0\hbox{x}\count1=0 \def\body{\advance\count1 by 1 \ifnum\count1=3 \def\third{yes}\fi}
          \def\iterate{\body\ifdim\wd0>\count1 sp \expandafter\iterate\fi}\iterate
          \ifnum\count1=1 \def\one{yes}\else\def\more{yes}\fi \def\after{done}",
    );
    let defined = names(&analysis);
    for name in ["third", "one", "more", "after"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
}

/// Paths that leave a register one of a few values: `\ifnum` on it is
/// decided when every value decides it alike, and undecided otherwise.
#[test]
fn ifnum_on_joined_values_is_decided_by_every_member() {
    let analysis = analyze(
        r"\ifnum\pdfuniformdeviate2=0 \count1=4 \else\count1=6 \fi
          \ifnum\count1>3 \def\big{yes}\else\def\small{yes}\fi
          \ifodd\count1 \def\odd{yes}\else\def\even{yes}\fi
          \ifnum\count1=4 \def\four{yes}\else\def\six{yes}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"big".to_string()) && !defined.contains(&"small".to_string()), "{defined:?}");
    assert!(defined.contains(&"even".to_string()) && !defined.contains(&"odd".to_string()), "{defined:?}");
    assert!(defined.contains(&"four".to_string()) && defined.contains(&"six".to_string()), "{defined:?}");
}

/// An unknown dimension is some dimension: an undecided `\ifdim` teaches
/// each arm what it implies, arithmetic keeps the interval, and a later
/// test the interval decides is decided.
#[test]
fn intervals_narrow_on_each_arm_and_decide_later_tests() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\dimen1=\wd0
          \ifdim\dimen1>5pt \advance\dimen1 by 1pt
            \ifdim\dimen1>6pt \def\yes{1}\else\def\no{1}\fi
          \fi
          \ifdim\dimen1<0pt \def\neg{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"yes".to_string()) && !defined.contains(&"no".to_string()), "{defined:?}");
    assert!(defined.contains(&"neg".to_string()), "{defined:?}");
}

/// `\ifdefined` of a name a join left defined on one path and undefined on
/// the other is undecided, not true.
#[test]
fn ifdefined_on_a_joined_meaning_is_decided_per_meaning() {
    let analysis = analyze(
        r"\let\u\undefined \ifnum\pdfuniformdeviate2=0 \def\u{x}\fi
          \ifdefined\u \def\a{1}\else\def\b{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"a".to_string()) && defined.contains(&"b".to_string()), "{defined:?}");
}

/// `\numexpr`/`\dimexpr` of unknown value is still a number of its kind:
/// a register alone keeps its interval, so a test the interval decides is
/// decided.
#[test]
fn expressions_keep_the_interval_of_a_register() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\dimen1=\wd0
          \ifdim\dimen1>2pt \ifdim\dimexpr\dimen1\relax>1pt \def\yes{1}\else\def\no{1}\fi\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"yes".to_string()) && !defined.contains(&"no".to_string()), "{defined:?}");
}

/// Unknown digits taken as one token are one digit, 0 to 9: a name built
/// from it is one of ten names, and `\ifx` against `\relax` is decided
/// when all ten are defined (pgfmath's number check). The test leaves
/// the dimension not negative, so it has no sign.
#[test]
fn one_unknown_digit_names_one_of_ten_names() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\dimen1=\wd0 \def\d#1{\expandafter\def\csname d@#1\endcsname{}}
          \d0\d1\d2\d3\d4\d5\d6\d7\d8\d9
          \def\a#1{\expandafter\ifx\csname d@\string#1\endcsname\relax\def\no{}\else\def\yes{}\fi}
          \ifdim\dimen1<0pt \else\edef\t{\the\dimen1}\expandafter\a\t\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"yes".to_string()) && !defined.contains(&"no".to_string()), "{defined:?}");
}

/// `\the` never prints a leading zero (tex.web § 103): a dimension of at
/// least 1pt begins with a digit 1-9, so `\if` against `0` is false
/// (pgfmath's octal-prefix test).
#[test]
fn the_of_a_dimension_of_at_least_1pt_does_not_begin_with_0() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\dimen1=\wd0 \def\a#1#2\end{\if\noexpand#10\def\zero{}\else\def\other{}\fi}
          \ifdim\dimen1<1pt \else\ifdim\dimen1>100pt \else\edef\t{\the\dimen1}\expandafter\a\t\end\fi\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"other".to_string()) && !defined.contains(&"zero".to_string()), "{defined:?}");
}

/// `\the` prints integer digits without a leading zero (tex.web §§ 103,
/// 104): after a sign or alone, a `0` is the whole integer part, so a
/// dimension's `0` is followed by `.` and a count's `0` by nothing.
#[test]
fn the_of_an_unknown_number_has_nothing_after_a_leading_0() {
    leading_0_with(crate::support::bare());
}

/// The same with few paths before a join: what the joined code may define
/// still counts.
#[test]
fn the_of_an_unknown_number_has_nothing_after_a_leading_0_when_joined_early() {
    let mut cfg = crate::support::bare();
    cfg.limits.alike = 4;
    leading_0_with(cfg);
}

/// Joined past `limits.paths`, the code a path had left becomes unknown
/// text. What it may define is still defined: a name it, or a macro it
/// calls, may `\def` that had no meaning before.
#[test]
fn joined_code_counts_what_it_may_define() {
    let mut cfg = crate::support::bare();
    (cfg.limits.alike, cfg.limits.paths) = (1, 1);
    let analysis = Machine::analyze(
        &format!(
            "{PRELUDE}{}",
            r"\count1=\pdfuniformdeviate 1000 \advance\count1 by -500
          \def\d#1#2\end{\if\noexpand#10\if\relax\detokenize{#2}\relax\def\alone{}\else\def\follow{}\fi\fi}
          \def\c#1#2\end{\if\noexpand#1-\d#2\end\else\d#1#2\end\fi}
          \edef\u{\the\count1}\expandafter\c\u\end"
        ),
        None,
        &cfg,
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"alone".to_string()), "{defined:?}");
}

fn leading_0_with(cfg: Config) {
    let analysis = Machine::analyze(
        &format!(
            "{PRELUDE}{}",
            r"\dimen1=\pdfuniformdeviate 200000000sp \advance\dimen1 by -100000000sp
          \count1=\pdfuniformdeviate 1000 \advance\count1 by -500
          \def\b#1#2#3\end{\if\noexpand#10\if\noexpand#2.\def\dot{}\else\def\more{}\fi\else\def\nz{}\fi}
          \def\a#1#2\end{\if\noexpand#1-\def\neg{}\b#2\end\else\b#1#2\end\fi}
          \edef\t{\the\dimen1}\expandafter\a\t\end
          \def\d#1#2\end{\if\noexpand#10\if\relax\detokenize{#2}\relax\def\alone{}\else\def\follow{}\fi\fi}
          \def\c#1#2\end{\if\noexpand#1-\d#2\end\else\d#1#2\end\fi}
          \edef\u{\the\count1}\expandafter\c\u\end"
        ),
        None,
        &cfg,
    );
    let defined = names(&analysis);
    let has = |n: &str| defined.contains(&n.to_string());
    for n in ["dot", "nz", "neg", "alone"] {
        assert!(has(n), "{n}: {defined:?}");
    }
    assert!(!has("more") && !has("follow"), "{defined:?}");
}

/// TeX's arithmetic on joined values, member by member: `\advance` wraps,
/// `\multiply` and `\divide` keep the register on an overflow or a zero
/// divisor, `\numexpr` rounds (tex.web §§ 1238-1240, etex.ch `fract`), and
/// `\ifcase` runs only the arms the selector's values reach.
#[test]
fn arithmetic_on_joined_values_is_texs_member_by_member() {
    let analysis = analyze(
        r"\ifnum\pdfuniformdeviate2=0 \count1=4 \else\count1=6 \fi
          \count2=\count1 \divide\count2 by 2
          \ifcase\count2 \def\zero{}\or\def\one{}\or\def\two{}\or\def\three{}\else\def\other{}\fi
          \count3=-\count1
          \ifcase\count3 \def\nzero{}\else\def\negative{}\fi
          \count4=\numexpr\count1*3/4\relax
          \ifnum\count4=3 \def\three{}\fi \ifnum\count4=5 \def\five{}\fi \ifnum\count4=4 \def\four{}\fi
          \count5=\count1 \advance\count5 by 2147483643
          \ifnum\count5<0 \def\wrapped{}\else\def\unwrapped{}\fi
          \count6=\count1 \multiply\count6 by 400000000
          \ifnum\count6=4 \def\kept{}\fi \ifnum\count6=1600000000 \def\product{}\fi
          \count7=\count1 \advance\count7 -4 \count8=7 \divide\count8 by \count7
          \ifnum\count8=7 \def\divkept{}\fi \ifnum\count8=3 \def\quot{}\fi",
    );
    let defined = names(&analysis);
    let has = |n: &str| defined.contains(&n.to_string());
    for n in ["two", "three", "negative", "five", "wrapped", "unwrapped", "kept", "product", "divkept", "quot"] {
        assert!(has(n), "{n}: {defined:?}");
    }
    for n in ["zero", "one", "other", "nzero", "four"] {
        assert!(!has(n), "{n}: {defined:?}");
    }
}

/// An interval too wide to list: `\dimexpr` halves it with `quotient`'s
/// rounding, and a test outside the halves is decided.
#[test]
fn dimexpr_on_an_interval_bounds_its_result() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\dimen1=\wd0
          \dimen2=\dimexpr\dimen1/2\relax
          \ifdim\dimen2>8192pt \def\above{}\else\def\within{}\fi
          \ifdim\dimen2<-8192pt \def\below{}\else\def\within{}\fi
          \ifdim\dimen2>1pt \def\big{}\else\def\small{}\fi
          \ifnum\pdfuniformdeviate2=0 \dimen3=1pt \else\dimen3=2pt \fi \dimen4=-2\dimen3
          \ifdim\dimen4<-1pt \def\negative{}\else\def\positive{}\fi",
    );
    let defined = names(&analysis);
    let has = |n: &str| defined.contains(&n.to_string());
    assert!(has("within") && !has("above") && !has("below"), "{defined:?}");
    assert!(has("big") && has("small"), "{defined:?}");
    assert!(has("negative") && !has("positive"), "{defined:?}");
}

// ---- writes and reads through a number satex could not pin down ----------

// Each probe below makes a number satex cannot follow (`\pdfuniformdeviate`
// is the engine's random generator) and then uses it where the engine would
// name a stream, a register, a character or a font.  What satex cannot pin
// down has to widen: never a plausible concrete answer, never a dropped
// assignment.

/// tex.web § 510: `\ifeof` asks about one stream, and a number satex could
/// not pin down names none of them.
#[test]
fn an_ifeof_of_a_stream_satex_cannot_pin_down_is_undecided() {
    let analysis = analyze(r"\count0=\pdfuniformdeviate 9\relax \ifeof\count0 \def\yes{}\else\def\no{}\fi");
    let defined = names(&analysis);
    assert!(defined.contains(&"yes".to_string()) && defined.contains(&"no".to_string()), "{defined:?}");
}

/// tex.web § 1275: `\openin` through a number satex could not pin down may
/// have opened or closed any stream, so no `\ifeof` is decided after it.
#[test]
fn an_openin_through_an_unknown_stream_leaves_every_stream_undecided() {
    let analysis = analyze(
        r"\ifeof3 \def\closedbefore{}\fi
          \count0=\pdfuniformdeviate 9 \openin\count0=nosuchfile
          \ifeof3 \def\yes{}\else\def\no{}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"closedbefore".to_string()), "a stream nothing opened is at end");
    assert!(defined.contains(&"yes".to_string()) && defined.contains(&"no".to_string()), "{defined:?}");
}

/// tex.web § 484: a `\read` from the terminal ends the job in nonstop mode,
/// which a stream satex could not pin down may not be.
#[test]
fn a_read_from_a_stream_satex_cannot_pin_down_does_not_end_the_job() {
    let analysis = analyze(r"\count0=\pdfuniformdeviate 9 \read\count0 to\line \def\after{}");
    let defined = names(&analysis);
    assert!(defined.contains(&"after".to_string()), "the run went on: {defined:?}");
    assert!(diagnosed(&analysis, "terminal-read-fatal").is_empty());
}

/// `\toks0=\toks⟨n⟩` copies the other register (tex.web § 1227); a number
/// satex could not pin down names any of them, which is unknown text and
/// not the empty list.
#[test]
fn a_toks_register_satex_cannot_pin_down_reads_as_unknown_text() {
    let analysis = analyze(
        r"\count0=\pdfuniformdeviate 9 \toks1={a}\toks1=\toks\count0
          \edef\copied{\the\toks1}\def\marker{a}",
    );
    assert_ne!(body(&analysis, "copied"), "", "the copy is not the empty list");
    assert_ne!(body(&analysis, "copied"), "a", "and not what the register held before");
}

/// A token register never assigned holds the empty list (tex.web § 232),
/// also where `\toks0=\toks⟨n⟩` or a `\toksdef` name copies it
/// (amsart's `\nxandlist` does `\toks@\@emptytoks`).
#[test]
fn copying_an_unassigned_toks_register_copies_the_empty_list() {
    let analysis = analyze(
        r"\toksdef\et=7 \toks0={a}\toks0=\et \edef\named{\the\toks0}
          \toks0={a}\toks0=\toks8 \edef\numbered{\the\toks0}",
    );
    assert_eq!(body(&analysis, "named"), "");
    assert_eq!(body(&analysis, "numbered"), "");
}

/// `\string` of a name built from unknown text prints characters of
/// category 12 and spaces (tex.web § 464): the escape and what is known of
/// the name, and between them text that holds no control sequence. A scan
/// for a control sequence passes over it (expl3's `\__file_name_expand:n`).
#[test]
fn the_string_of_an_unknown_name_holds_no_control_sequence() {
    let analysis = analyze(
        r"\count0=\pdfuniformdeviate 9 \toks1=\toks\count0 \edef\u{\the\toks1}
          \edef\x{\expandafter\string\csname a\u b\endcsname}
          \def\g#1\stop{\def\got{#1}}\expandafter\g\x\stop\def\R{done}",
    );
    assert!(diagnosed(&analysis, "argument-ends-in-unknown").is_empty());
    assert!(body(&analysis, "got").starts_with("\\a"), "{}", body(&analysis, "got"));
    assert!(definition(&analysis, "R").is_some());
}

/// etex.ch: `\unless` before a name of unknown meaning may still open a
/// conditional, so its arms are analyzed rather than run one after the other
/// with the `\else` and `\fi` left over.
#[test]
fn an_unless_before_an_unknown_meaning_opens_a_conditional() {
    let analysis = analyze(
        r"\count0=\pdfuniformdeviate 9 \read\count0 to\maybeif
          \unless\maybeif\def\yes{}\else\def\no{}\fi \def\after{}",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"after".to_string()), "{defined:?}");
    for code in ["extra-else", "extra-fi"] {
        assert!(diagnosed(&analysis, code).is_empty(), "{code}");
    }
    for name in ["yes", "no"] {
        assert!(defined.contains(&name.to_string()), "{defined:?}");
    }
}

/// tex.web § 1232: `\catcode` of a character satex could not pin down may
/// have changed any category, so none of them is known afterwards -- not
/// even the braces, which is why nothing past it tokenizes.  Before, the
/// assignment was dropped and `\the\catcode`\a` read back 11.
#[test]
fn a_catcode_of_a_character_satex_cannot_pin_down_makes_every_category_unknown() {
    let analysis = analyze(
        r"\count0=\pdfuniformdeviate 999 \catcode\count0=11
          \edef\after{\the\catcode`\a}",
    );
    assert!(!diagnosed(&analysis, "unknown-catcode").is_empty());
    assert!(definition(&analysis, "after").is_none(), "no category is known, so nothing after it is read");
}

/// The same where the character's interval is narrow: only the categories
/// it may name go unknown, so the braces and the letters stay known and
/// the rest of the file is still read.
#[test]
fn a_catcode_of_a_character_in_a_narrow_interval_widens_only_that_interval() {
    let analysis = analyze(
        // `\relax` ends the number: as TeX reads it, the digits scan looks
        // past them and would expand the conditional before the assignment.
        r"\count0=\pdfuniformdeviate 999\relax
          \ifnum\count0>200 \ifnum\count0<300 \catcode\count0=11 \fi\fi
          \edef\letter{\the\catcode`\a}
          \ifnum\catcode250=12 \def\stillother{1}\else\def\changed{1}\fi",
    );
    let said = diagnosed(&analysis, "unknown-catcode");
    assert!(said.iter().any(|m| m.contains("each category it may name")), "{said:?}");
    assert!(!said.iter().any(|m| m.contains("no category is known")), "{said:?}");
    assert_eq!(body(&analysis, "letter"), "11", "a is still a letter");
    // The real state past the `\fi`, not just what the run said about it:
    // character 250 is one the interval may name, so its category is not
    // read back at the one it had.
    let defined = names(&analysis);
    assert!(defined.contains(&"stillother".to_string()) && defined.contains(&"changed".to_string()), "{defined:?}");
}

/// An assignment through an unpinned character number leaves each
/// character it may name at its old category or the new one: a character
/// already of the assigned category stays known, the others do not. This
/// holds for a narrow interval (each character) and a wide one (the reach).
#[test]
fn a_catcode_through_an_unpinned_number_keeps_categories_equal_to_the_new_one() {
    for most in ["999", "99999"] {
        let analysis = analyze(&format!(
            r"\catcode250=11 \catcode251=12 \count0=\pdfuniformdeviate {most}\relax
              \ifnum\count0>200 \catcode\count0=11 \fi
              \ifnum\catcode250=11 \def\same{{1}}\else\def\lost{{1}}\fi
              \ifnum\catcode251=12 \def\other{{1}}\else\def\letter{{1}}\fi"
        ));
        let defined = names(&analysis);
        assert!(defined.contains(&"same".to_string()) && !defined.contains(&"lost".to_string()), "{most}: {defined:?}");
        assert!(
            defined.contains(&"other".to_string()) && defined.contains(&"letter".to_string()),
            "{most}: {defined:?}"
        );
    }
}

/// tex.web §§ 232, 498: the arms of an undecided conditional may part
/// about a category, and what holds past the `\fi` is their join, not the
/// first arm's table.  `pdftex` with the count pinned to 11 or 12 prints
/// `11` or `12` for `\the\catcode`\@`.
#[test]
fn arms_that_part_about_a_category_leave_it_unknown_past_the_fi() {
    let analysis = analyze(
        r"\count2=\pdfuniformdeviate 9
          \ifnum\pdfuniformdeviate2=0 \relax \else \catcode`\@=\count2 \fi
          \ifnum\catcode`\@=12 \def\other{1}\fi \ifnum\catcode`\@=11 \def\letter{1}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"other".to_string()) && defined.contains(&"letter".to_string()), "{defined:?}");
}

/// The same for the code tables `\lccode` and its relatives: the entry is
/// not read back at the table's default.
#[test]
fn an_lccode_of_a_character_satex_cannot_pin_down_makes_the_table_unknown() {
    let analysis = analyze(
        r"\count0=\pdfuniformdeviate 999 \lccode`\a=`\a \lccode\count0=0
          \edef\after{\the\lccode`\a}",
    );
    assert!(!diagnosed(&analysis, "unknown-char-code").is_empty());
    assert!(definition(&analysis, "after").is_some(), "the run went on");
    assert_ne!(body(&analysis, "after"), "97", "the entry is no longer what it was set to");
}

/// tex.web § 1077: a `\setbox` through a register satex could not pin down
/// may have filled any box, so none of them is known to be void.
#[test]
fn a_setbox_through_a_register_satex_cannot_pin_down_leaves_every_box_unknown() {
    let analysis = analyze(
        r"\ifvoid3 \def\voidbefore{}\fi
          \count0=\pdfuniformdeviate 9 \setbox\count0=\hbox{x}
          \ifvoid3 \def\yes{}\else\def\no{}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"voidbefore".to_string()), "a box nothing filled is void");
    assert!(defined.contains(&"yes".to_string()) && defined.contains(&"no".to_string()), "{defined:?}");
}

/// tex.web § 580: a `\fontdimen` assigned a value satex could not follow is
/// not the font file's parameter any more.
#[test]
fn a_fontdimen_assigned_an_unknown_value_is_not_the_font_files() {
    let analysis = analyze(
        r"\font\f=cmr10 \dimen0=\fontdimen6\f \edef\design{\the\dimen0}
          \fontdimen6\f=\pdfuniformdeviate 999sp \dimen1=\fontdimen6\f
          \ifdim\dimen1=\design\def\same{}\else\def\other{}\fi",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"same".to_string()) && defined.contains(&"other".to_string()), "{defined:?}");
}

/// `\parshape` says how many ⟨dimen⟩ pairs follow (tex.web § 1248): a count
/// satex could not pin down leaves the shape unknown, not empty.
#[test]
fn a_parshape_count_satex_cannot_pin_down_leaves_the_shape_unknown() {
    let analysis = analyze(r"\parshape=\pdfuniformdeviate 9\relax \ifnum\parshape=0 \def\none{}\else\def\some{}\fi");
    let defined = names(&analysis);
    assert!(defined.contains(&"none".to_string()) && defined.contains(&"some".to_string()), "{defined:?}");
    assert!(!diagnosed(&analysis, "unknown-parshape").is_empty());
}

/// tex.web § 1370: `\write` to a stream satex could not pin down may reach
/// the shell, so the run says a shell escape may have happened.
#[test]
fn a_write_to_a_stream_satex_cannot_pin_down_may_be_a_shell_escape() {
    let analysis = analyze(r"\count0=\pdfuniformdeviate 9 \immediate\write\count0{ls}");
    let kinds: Vec<_> = analysis.facts.occurrences.iter().map(|o| o.kind).collect();
    assert!(kinds.contains(&OccKind::ShellEscape), "{kinds:?}");
    assert!(!diagnosed(&analysis, "unknown-write-stream").is_empty());
}

/// tex.web §§ 395-396: a delimited argument that runs into the `}` of the
/// group it was called in gets "Argument of \grab has an extra }", the
/// `\par` inserted before the brace ends the call as a runaway, and the
/// call is dropped: `\got` is never defined.  Dropping the argument alone
/// once said it was empty and ran the body.
#[test]
fn an_argument_scan_that_ran_into_a_brace_drops_the_call() {
    let analysis = analyze(r"\def\grab#1\stop{\def\got{#1}}{\grab abc}");
    for code in ["extra-right-brace", "runaway-argument"] {
        assert_eq!(diagnosed(&analysis, code).len(), 1, "{code}");
    }
    assert!(diagnosed(&analysis, "unbalanced-argument").is_empty());
    assert!(!analysis.facts.defs.iter().any(|d| analysis.interner.name(d.name) == "got"), "the call was dropped");
}

/// tex.web § 392: an argument is read by counting braces, and text satex
/// could not follow may hold a `{` or a `}` of its own.  A scan that met
/// such text inside a group does not know where the group ends, so the
/// delimiter it goes on to find is not known to be the one that ends the
/// argument.  Before, `\got` came out as `{xQy}z` with the braces taken on
/// trust.
#[test]
fn an_argument_whose_braces_hide_unknown_text_is_not_known() {
    let analysis = analyze(
        r"\count0=\pdfuniformdeviate 9 \catcode`\Q=\count0
          \def\grab#1\stop{\def\got{#1}}\grab {xQy}z\stop",
    );
    let got = body(&analysis, "got");
    assert!(!got.contains('}'), "the braces were not known to be the argument's: {got:?}");
    assert!(got.contains("unknown"), "what the argument holds is not known: {got:?}");
}

/// tex.web § 366: a macro call is replaced by its replacement text, and a
/// call satex stops expanding did not expand to nothing.  A scan that
/// cannot find the delimiter its argument ends at looks for it in the same
/// place on every round, so a loop around it is not converging and is
/// widened where a loop head widens, not run out to the per-site bound.
/// Before, one `\IfFileExists` on a name satex could not follow raised
/// `unbalanced-argument` 98297 times inside expl3's file-name scanners.
#[test]
fn a_scan_that_cannot_find_its_delimiter_does_not_run_out_the_site_budget() {
    let analysis = analyze_kernel(r"\count0=\pdfuniformdeviate 9 \catcode`\Q=\count0 \IfFileExists{x/Q}{}{}");
    let raised: u32 = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| matches!(d.code, "extra-right-brace" | "argument-ends-in-unknown"))
        .map(|d| d.count)
        .sum();
    assert!(raised > 0, "the scan does run into the end of its group");
    assert!(raised < 1000, "the loop around it is widened, not run out: {raised}");
}

/// tex.web § 366: expanding a macro puts its replacement text back for the
/// same reader, so a recursion that only assigns and re-enters has read
/// nothing.  The per-site bound counts an assignment as progress, to tell
/// apart the many calls one site sees between two tokens of a file, so a
/// recursion that assigns every round used to slip past it and run until
/// the stall limit.  Reading on is counted separately for that reason.
#[test]
fn a_recursion_that_only_assigns_is_bounded_before_the_stall() {
    let analysis = analyze(r"\def\spin{\count255=1 \spin}\spin");
    let codes: Vec<_> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&"recursion-widened"), "the loop is widened where it is found: {codes:?}");
    assert!(!codes.contains(&"line-budget-cut"), "and not left to the line guard: {codes:?}");
}

/// The same cut must not touch a loop that decides its test: `\@tfor` over
/// twenty real items assigns its variable every round, reads nothing from
/// the file and is cut by nothing, and every item it walks reaches the body.
#[test]
fn an_ordinary_loop_over_twenty_items_is_not_cut() {
    let analysis =
        analyze_kernel(r"\makeatletter\def\acc{}\@tfor\itm:=abcdefghijklmnopqrst\do{\edef\acc{\acc\itm}}\makeatother");
    assert_eq!(last_body(&analysis, "acc"), "abcdefghijklmnopqrst");
    assert_eq!(diagnosed(&analysis, "recursion-widened").len(), 0);
    assert_eq!(diagnosed(&analysis, "line-budget-cut").len(), 0);
}

/// tex.web §§ 232, 344: of the categories a character may have, only 13
/// runs a meaning of its own, and that meaning is the one bound for the
/// character.  So what a character satex could not pin down may do is
/// bounded by the active characters the document has, and executing it
/// must join in what those could assign.  Nothing else it could be
/// assigns, so a character nothing binds leaves the run as precise as it
/// was.
#[test]
fn a_character_whose_category_is_unknown_may_do_what_it_is_bound_to_do() {
    let unbound = names(&analyze(
        r"\count1=5 \count0=\pdfuniformdeviate 9 \catcode`\Q=\count0
          Q \ifnum\count1=5 \def\same{}\else\def\changed{}\fi",
    ));
    assert!(unbound.contains(&"same".to_string()), "{unbound:?}");
    assert!(!unbound.contains(&"changed".to_string()), "nothing binds Q, so no meaning of it assigns: {unbound:?}");
    let bound = names(&analyze(
        r"\catcode`\~=13 \def~{\count1=7 }\count1=5
          \count0=\pdfuniformdeviate 9 \catcode`\~=\count0
          ~ \ifnum\count1=5 \def\same{}\else\def\changed{}\fi",
    ));
    assert!(
        bound.contains(&"same".to_string()) && bound.contains(&"changed".to_string()),
        "the character may still be active, and that meaning assigns: {bound:?}"
    );
}

/// `\pdffilemoddate` of a file that exists has the form `D:` and fourteen
/// digits and a zone, even where the digits are not read: epstopdf takes
/// it apart with eight undelimited parameters inside a conditional, and
/// unknown text of any length let them run past the `\fi`.
#[test]
fn file_mod_date_keeps_its_form() {
    let analysis = analyze(concat!(
        r"\def\D#1\nil{\ifx\relax#1\relax\else\DD#1\nil\fi\def\after{ok}}",
        r"\def\DD#1:#2#3#4#5#6#7#8#9{\def\day{#8#9}\T}",
        r"\def\T#1#2#3#4#5#6#7\nil{\def\time{#1#2}}",
        r"\expandafter\expandafter\expandafter\D\pdffilemoddate{Cargo.toml}\nil",
    ));
    let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(!codes.contains(&"unbalanced-conditional"), "{codes:?}");
    assert_eq!(last_body(&analysis, "after"), "ok");
    assert!(names(&analysis).iter().any(|n| n == "time"), "the time part is parsed");
}

/// A macro that reverses unknown digits one at a time, its accumulator
/// growing each pass (graphics' `\GPT@Reverse`): every length is a path
/// that ends, the passes meet as one loop head, and the result is digits,
/// never a fixed count of them nor a runaway past the delimiter.
#[test]
fn digit_reversal_over_unknown_digits_converges() {
    let analysis = analyze(concat!(
        r"\def\empty{}\def\gobble#1{}\def\firstofone#1{#1}",
        r"\def\rev#1#2#3\nil{\ifx\empty#3\empty#2#1\expandafter\gobble\else\expandafter\firstofone\fi{\rev{#2#1}#3\nil}}",
        r"\def\zap#1{\ifx0#1\expandafter\zap\else#1\fi}",
        r"\edef\f{\the\pdflastxpos}",
        r"\edef\r{\expandafter\rev\expandafter{\expandafter}\f\nil}",
        r"\edef\z{\expandafter\zap\f\empty}",
        r"\def\after{ok}",
    ));
    let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
    for code in ["runaway-argument", "recursion-widened", "limit-reached", "extra-right-brace"] {
        assert!(!codes.contains(&code), "{code}: {codes:?}");
    }
    assert_eq!(last_body(&analysis, "after"), "ok");
    for name in ["r", "z"] {
        let body = last_body(&analysis, name);
        assert!(!body.contains("unknown:") && !body.contains("rev") && !body.contains("nil"), "\\{name}: {body}");
    }
    // Two digits exactly is what reading the rest as one digit gave.
    let r = last_body(&analysis, "r");
    assert!(r.contains("one of") || r.contains("unknown digits"), "{r}");
}

/// The errors, warnings and precision notes, as (code, severity, line,
/// column, message).
fn problems(analysis: &Analysis) -> Vec<(String, String, u64, u64, String)> {
    let s = |f: &satex::query::Record, k: &str| f.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let n = |f: &satex::query::Record, k: &str| f.get(k).and_then(|v| v.as_u64()).unwrap_or_default();
    satex::lint::lint(analysis)
        .iter()
        .filter(|f| s(f, "severity") != "info" || s(f, "code") == "analysis-imprecision")
        .map(|f| (s(f, "code"), s(f, "severity"), n(f, "line"), n(f, "col"), s(f, "message")))
        .collect()
}

/// tex.web §§ 338, 396: a file that ends inside an argument is one certain
/// error at the call. `\newcommand` makes `\long` macros, so it is not
/// "Paragraph ended", and the unclosed group is the same cause.
#[test]
fn an_argument_the_file_ends_in_is_one_error_at_the_call() {
    let analysis =
        analyze_kernel("\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\n\\greet{world\n\\end{document}\n");
    let found = problems(&analysis);
    assert_eq!(found.len(), 1, "{found:?}");
    let (code, severity, line, col, message) = &found[0];
    assert_eq!((code.as_str(), severity.as_str(), *line, *col), ("raised-error", "error", 3, 1), "{found:?}");
    assert!(message.starts_with("File ended while scanning use of \\greet"), "{message}");
}

/// tex.web § 396: `\par` in the argument of a macro not `\long` is one
/// error at the call.
#[test]
fn a_paragraph_end_in_a_short_argument_is_one_error() {
    let analysis = analyze_kernel("\\def\\g#1{}\n\\g{a\\par}\n");
    let found = problems(&analysis);
    assert_eq!(found.len(), 1, "{found:?}");
    let (code, severity, line, col, message) = &found[0];
    assert_eq!((code.as_str(), severity.as_str(), *line, *col), ("raised-error", "error", 2, 1), "{found:?}");
    assert!(message.starts_with("Paragraph ended before \\g was complete"), "{message}");
}

/// `\the` of an unknown dimension is a sign, digits, `.`, digits and `pt`
/// of category 12. A loop taking it apart a character at a time, and a
/// macro delimited by `pt`, leave the text after it intact.
#[test]
fn the_of_an_unknown_dimen_read_by_characters_keeps_what_follows() {
    let analysis = analyze(
        r"\dimen1=\pdfuniformdeviate 100000sp
          \def\eat#1{\ifx#1\stop \else \expandafter\eat\fi}
          \edef\x{\the\dimen1}
          \expandafter\eat\x\stop
          {\catcode`\P=12 \catcode`\T=12 \lccode`\P=`\p \lccode`\T=`\t
           \lowercase{\gdef\getpt#1PT}{\gdef\num{#1}}}
          \expandafter\getpt\the\dimen1
          \def\after{ok}",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"num".to_string()), "{defined:?}");
    assert_eq!(last_body(&analysis, "after"), "ok");
}

/// A box of text has the size its font's metrics give (`\showthe\wd0` in
/// pdflatex: 15.27782pt, 6.94444pt, 0.0pt), and an hbox's height and depth
/// are never negative (tex.web § 649).
#[test]
fn a_box_of_text_has_the_size_of_its_characters() {
    let analysis = analyze_kernel(
        r"\font\tenrm=cmr10 \tenrm \setbox0\hbox{abc}
          \ifdim\wd0>0pt \def\wide{}\else\def\boxnowidth{}\fi
          \ifdim\wd0>15.27pt \ifdim\wd0<15.28pt \def\exact{}\fi\fi
          \ifdim\ht0<0pt \def\boxneg{}\fi \ifdim\dp0<0pt \def\boxneg{}\fi
          \ifdim\ht0>6.94pt \def\high{}\fi",
    );
    let defined = names(&analysis);
    for name in ["wide", "exact", "high"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
    assert!(!defined.iter().any(|n| n == "boxnowidth" || n == "boxneg"), "{defined:?}");
}

/// Inline math of plain characters adds their boxes: `$N$` is 9.12497pt
/// wide (with its italic correction) and 6.83331pt high in pdflatex.
/// Math satex does not follow still leaves the height non-negative.
#[test]
fn inline_math_of_characters_has_a_bounded_size() {
    let analysis = analyze_kernel(
        r"\setbox0\hbox{$N$}
          \ifdim\wd0>8pt \ifdim\wd0<9.13pt \def\wide{}\fi\fi
          \ifdim\ht0>6.83pt \ifdim\ht0<6.84pt \def\high{}\fi\fi
          \setbox2\hbox{$\alpha_i^2$}
          \ifdim\ht2<0pt \def\boxneg{}\fi \ifdim\dp2<0pt \def\boxneg{}\fi",
    );
    let defined = names(&analysis);
    for name in ["wide", "high"] {
        assert!(defined.contains(&name.to_string()), "{name}: {defined:?}");
    }
    assert!(!defined.iter().any(|n| n == "boxneg"), "{defined:?}");
}

/// A loop counting the characters of `\the` of an unknown dimension
/// cannot know the count: it is a range or unknown, never one number.
#[test]
fn a_character_loop_over_an_unknown_dimen_leaves_one_path() {
    let analysis = analyze(
        r"\dimen1=\pdfuniformdeviate 100000sp \count1=0
          \def\eat#1{\ifx#1\stop \else \advance\count1 1 \expandafter\eat\fi}
          \edef\x{\the\dimen1}
          \expandafter\eat\x\stop
          \edef\n{\the\count1}",
    );
    let n = last_body(&analysis, "n");
    assert!(n.parse::<i64>().is_err(), "an exact count: {n}");
}

/// Two rests printed by one macro are two texts: a test on the first
/// teaches nothing about the second.
#[test]
fn a_test_on_one_rest_leaves_another_alone() {
    let analysis = analyze(
        r"\dimen1=-\pdfuniformdeviate 100000sp \advance\dimen1 50000sp
          \dimen2=-\pdfuniformdeviate 100000sp \advance\dimen2 50000sp
          \def\pr{\the\dimen1;\the\dimen2;}
          \edef\p{\pr}
          \def\a#1#2{\ifx#2.\expandafter\b\fi}
          \def\b#1;#2#3{\ifx#3.\else\gdef\mixed{}\fi}
          \expandafter\a\p",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"mixed".to_string()), "{defined:?}");
}

/// The box sizes an article document leaves in `\out`.
fn article_sizes(body: &str) -> String {
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let source = format!("\\documentclass{{article}}\\begin{{document}}{body}\\end{{document}}");
    last_body(&Machine::analyze(&source, None, &cfg), "out")
}

/// A paragraph in a `\vbox` is broken into lines (tex.web §§ 813-890) and
/// stacked with interline glue (§ 679). `\showthe` in pdflatex: 16.30554pt,
/// 0.0pt, 113.81102pt.
#[test]
fn a_paragraph_with_a_forced_break_has_its_lines_height() {
    let out = article_sizes(r"\setbox0\vbox{\hsize=4cm p\\r}\xdef\out{\the\ht0/\the\dp0/\the\wd0}");
    assert_eq!(out, "16.30554pt/0.0pt/113.81102pt");
}

/// LaTeX's `\par` after `\strut` asks `\lastnodetype` (a character: 0 or
/// 7, never 11) and may add `\hskip0pt`; § 816 makes a final glue
/// `\penalty10000`, no breakpoint. With finite stretch (TikZ's `align=center`)
/// it stays one line. pdflatex: 8.39996pt, 3.60004pt, 56.9055pt.
#[test]
fn a_final_glue_is_no_breakpoint() {
    let skips = r"\leftskip0pt plus2em\rightskip0pt plus2em\parfillskip=0pt ";
    let out = article_sizes(&format!(
        r"\setbox0\vbox{{\hsize=2cm{skips}B\strut\par}}\xdef\out{{\the\ht0/\the\dp0/\the\wd0}}"
    ));
    assert_eq!(out, "8.39996pt/3.60004pt/56.9055pt");
    let out = article_sizes(&format!(
        r"\setbox0\vbox{{\hsize=2cm{skips}B\hskip3pt\par}}\xdef\out{{\the\ht0/\the\dp0/\the\wd0}}"
    ));
    assert_eq!(out, "6.83331pt/0.0pt/56.9055pt");
    // 3pt short of the line with 2em of stretch: badness 0 (§ 108), one
    // line beats any break (§ 859). pdflatex: 6.94444pt, 0.0pt, 26.88893pt.
    let out = article_sizes(&format!(
        r"\setbox2\hbox{{ab cd}}\setbox0\vbox{{\hsize=\wd2 \advance\hsize 3pt{skips}\noindent ab cd\par}}\xdef\out{{\the\ht0/\the\dp0/\the\wd0}}"
    ));
    assert_eq!(out, "6.94444pt/0.0pt/26.88893pt");
}

/// pdfTeX's `\pdfendlink` and `\pdfstartlink` append a whatsit (pdfTeX
/// manual): `\unskip` after a link takes nothing. pdflatex: 9, 7.50002pt.
#[test]
fn a_link_ends_in_a_whatsit() {
    let out = article_sizes(
        r"\setbox0\hbox{\pdfstartlink user{/S}A\pdfendlink\xdef\t{\the\lastnodetype}\unskip}\xdef\out{\t/\the\wd0}",
    );
    assert_eq!(out, "9/7.50002pt");
}

/// Expanding `\ifdim\lastkern` inside a word does not end it: `A` and `V`
/// still kern (tex.web §§ 1034-1036), as csquotes relies on. A `\write`
/// is a whatsit, nothing after `\immediate` (§§ 1348, 1375); a font
/// setting is nothing. pdflatex: 13.8889pt twice, -1, 9, -1.
#[test]
fn expansion_inside_a_word_keeps_its_kerning() {
    let out = article_sizes(
        r"\setbox0\hbox{AV}\xdef\a{\the\wd0}\setbox0\hbox{A\ifdim\lastkern=0pt \fi V}\xdef\b{\the\wd0}
          \setbox0\hbox{\immediate\write-1{x}\xdef\c{\the\lastnodetype}}
          \setbox0\hbox{\write-1{x}\xdef\d{\the\lastnodetype}}
          \setbox0\hbox{\pdffontexpand\font 20 20 5 autoexpand\xdef\e{\the\lastnodetype}}\xdef\out{\a/\b/\c/\d/\e}",
    );
    assert_eq!(out, "13.8889pt/13.8889pt/-1/9/-1");
}

/// A centred minipage of two short lines is a `\vcenter` on the axis
/// (tex.web § 736): pdflatex gives 12.94444pt, 7.94444pt, 85.35826pt. Two
/// paragraphs stack the same way.
#[test]
fn a_minipage_of_two_lines_has_its_exact_size() {
    let out =
        article_sizes(r"\setbox0\hbox{\begin{minipage}{3cm}ab\\cg\end{minipage}}\xdef\out{\the\ht0/\the\dp0/\the\wd0}");
    assert_eq!(out, "12.94444pt/7.94444pt/85.35826pt");
    let out = article_sizes(r"\setbox0\vbox{\hsize=4cm p\par r}\xdef\out{\the\ht0/\the\dp0/\the\wd0}");
    assert_eq!(out, "16.30554pt/0.0pt/113.81102pt");
}

/// Where the line width is not known, the breaks are not either: the
/// height is a range from one line to one per breakpoint (four words, nine
/// breakpoints with hyphenation, ten lines of 12pt at most), never one
/// number and never negative.
#[test]
fn a_paragraph_of_uncertain_breaks_has_a_bounded_height() {
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(
        r"\documentclass{article}\begin{document}
          \setbox0\vbox{\hsize=\pdfuniformdeviate 300pt aa bb cc dd}
          \ifdim\ht0<0pt \gdef\negative{}\fi \ifdim\ht0>120pt \gdef\high{}\fi
          \xdef\out{\the\ht0}\end{document}",
        None,
        &cfg,
    );
    let out = last_body(&analysis, "out");
    assert!(out.contains("unknown"), "{out}");
    let defined = names(&analysis);
    assert!(!defined.iter().any(|n| n == "negative" || n == "high"), "{defined:?}");
}

/// Past `limits.choices` the reversal of unknown digits still gives an
/// over-approximation: never an empty or partial body without notice.
#[test]
fn digit_reversal_past_the_choices_limit_is_not_empty() {
    for choices in [4, 8] {
        let cfg = Config {
            limits: satex::config::Limits { choices, ..crate::support::bare().limits },
            ..crate::support::bare()
        };
        let analysis = Machine::analyze(
            &format!(
                "{PRELUDE}{}",
                concat!(
                    r"\def\empty{}\def\gobble#1{}\def\firstofone#1{#1}",
                    r"\def\rev#1#2#3\nil{\ifx\empty#3\empty#2#1\expandafter\gobble\else\expandafter\firstofone\fi{\rev{#2#1}#3\nil}}",
                    r"\count1=\pdfuniformdeviate 2000000000 \advance\count1 by -1000000000 ",
                    r"\edef\h{\the\count1}",
                    r"\edef\q{\expandafter\rev\expandafter{\expandafter}\h\nil}",
                )
            ),
            None,
            &cfg,
        );
        let q = last_body(&analysis, "q");
        let codes: Vec<&str> = analysis.facts.diagnostics.iter().map(|d| d.code).collect();
        assert!(!q.is_empty(), "choices {choices}: \\q is empty, {codes:?}");
        assert!(q.contains("one of") || q.contains("unknown"), "choices {choices}: \\q is {q}");
    }
}

/// tex.web deviations, each checked against pdfTeX (`target/scratch/tw/u*.tex`).
const TEXWEB: &[crate::support::BodyRow] = &[
    // § 1069: a `}` that meets `\begingroup` is deleted; the group stays open.
    (
        "brace_meeting_begingroup_is_deleted",
        r"\begingroup } \def\y{b} \endgroup \ifx\y\undefined\def\r{u}\else\def\r{d}\fi",
        &[("r", "u")],
    ),
    // § 1064: `\endgroup` meeting `{` inserts `}`, then closes its own group.
    (
        "endgroup_meeting_brace_inserts_one",
        r"\begingroup{\endgroup \def\y{b}}\ifx\y\undefined\def\r{u}\else\def\r{d}\fi",
        &[("r", "d")],
    ),
    // § 1193: a `$` meeting `{` in math inserts `}`, then ends the math.
    (
        "math_shift_meeting_brace_inserts_one",
        r"\catcode`\$=3 $a{\def\y{b}$\ifx\y\undefined\def\r{u}\else\def\r{d}\fi",
        &[("r", "u")],
    ),
    // § 59: `\meaning` and `\detokenize` print to a string, raw characters.
    (
        "meaning_and_detokenize_print_raw_characters",
        r"\def\c#1{\ifx#1\end\else\advance\count1 1 \expandafter\c\fi}{\catcode1=12 \gdef\x{^^A}}\edef\y{\meaning\x}\expandafter\c\y\end\edef\a{\the\count1}\count1=0 \edef\d{\detokenize{^^e9^^c3}}\expandafter\c\d\end\edef\b{\the\count1}",
        &[("a", "9"), ("b", "2")],
    ),
    // etex.ch `pseudo_start` splits the raw string at `\newlinechar`.
    (
        "scantokens_splits_at_an_unprintable_newlinechar",
        r"{\catcode1=12 \gdef\x{a^^Ab}}\newlinechar=1 \everyeof{\noexpand}\edef\r{\expandafter\scantokens\expandafter{\x}}",
        &[("r", "a b ")],
    ),
    // § 59: a file line breaks at `\newlinechar` before `^^` notation.
    (
        "write_splits_at_an_unprintable_newlinechar",
        r"\newlinechar=10 \immediate\openout3=twnl.tex \immediate\write3{a^^Jb}\immediate\closeout3 \openin1=twnl.tex \read1 to\x \read1 to\y",
        &[("x", "a "), ("y", "b ")],
    ),
    // §§ 63, 262: `\escapechar`, and a space after `\@` when `@` is a letter now.
    (
        "scantokens_prints_with_escapechar_and_current_letters",
        r"\def\t{\@x}\catcode`\@=11 \def\@{AT}\def\@x{WRONG}\everyeof{\noexpand}\edef\r{\expandafter\scantokens\expandafter{\t}}{\escapechar=-1 \xdef\s{\scantokens{\relax}}}",
        &[("r", "ATx "), ("s", "relax ")],
    ),
    // § 483: an unmatched `}` aborts the line `\read` reads.
    (
        "read_drops_the_line_after_an_unmatched_brace",
        r"\begingroup\lccode`\]=`\} \lowercase{\endgroup\def\rb{]}}\immediate\openout3=twrb.tex \immediate\write3{a\rb b}\immediate\closeout3 \openin1=twrb.tex \read1 to\x",
        &[("x", "a")],
    ),
    // § 1212: a prefix before a non-assignment is dropped.
    (
        "prefix_before_a_non_assignment_is_dropped",
        r"\begingroup\global\hskip1pt \def\x{a}\endgroup\ifx\x\undefined\def\r{u}\else\def\r{d}\fi",
        &[("r", "u")],
    ),
    // §§ 405, 443: a name or active character `\let` to a space is a spacer.
    (
        "implicit_spaces_are_spacers",
        r"\def\:{\let\sp= }\: \catcode`\~=13 \let~= \sp \count0~=7 \count1 \sp=6 \count2\sp=5 \edef\r{\the\count0,\the\count1,\the\count2}",
        &[("r", "7,6,5")],
    ),
    // etex.ch `scan_expr` swallows any `cur_cmd = relax`.
    ("numexpr_swallows_an_active_relax", r"\catcode`\~=13 \let~\relax \edef\x{\the\numexpr1~}", &[("x", "1")]),
    // § 288 `prepare_mag` resets an illegal `\mag` to 1000.
    ("true_resets_an_illegal_mag", r"\mag=0 \dimen0=1truept \edef\m{\the\mag}", &[("m", "1000")]),
];

#[test]
fn texweb_deviations_agree_with_tex() {
    crate::support::body_rows(TEXWEB, analyze);
}

#[test]
fn immediate_covers_only_the_next_command() {
    // tex.web § 1375: `\immediate\relax\write` is a deferred write.
    let analysis = analyze(
        r"\immediate\openout3=twim.tex \immediate\relax\write3{x}\immediate\closeout3 \openin1=twim.tex \read1 to\y",
    );
    assert_ne!(body(&analysis, "y"), "x ");
}

#[test]
fn if_tells_a_control_sequence_from_char_256() {
    // tex.web § 506; XeTeX's `too_big_usv` is past every character (probed with xetex).
    let cfg = Config { engine: Some(satex::config::Engine::XeTeX), ..crate::support::bare() };
    let analysis =
        Machine::analyze(&format!("{PRELUDE}\\if \u{100}\\relax\\def\\r{{T}}\\else\\def\\r{{F}}\\fi"), None, &cfg);
    assert_eq!(body(&analysis, "r"), "F");
}

/// An image is its PDF page's crop box, else its media box (the pdfTeX
/// manual, "Graphics"): pdflatex gives 10.0375pt square.
#[test]
fn a_pdf_image_has_its_page_size() {
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/documents/includegraphics/title-banner-image.pdf");
    let raw = article_sizes(&format!(
        r"\pdfximage{{{file}}}\setbox0\hbox{{\pdfrefximage\pdflastximage}}\xdef\out{{\the\wd0/\the\ht0/\the\dp0}}"
    ));
    assert_eq!(raw, "10.0375pt/10.0375pt/0.0pt");
}

/// An `\halign`'s rows are packed naturally (tex.web §§ 796, 799), so its
/// height and depth are never negative, and exact where its entries are
/// known. pdflatex: 18.30554pt/0.0pt/11.55557pt, 16.30554pt/0.0pt/50.0pt,
/// 6.94444pt/1.94444pt for two columns.
#[test]
fn an_halign_of_two_rows_has_its_exact_height() {
    let out = article_sizes(
        r"\setbox0\vbox{\tabskip=3pt\halign{\hfil#\hfil\cr g\cr\noalign{\kern2pt}b\cr}}\xdef\out{\the\ht0/\the\dp0/\the\wd0}",
    );
    assert_eq!(out, "18.30554pt/0.0pt/11.55557pt");
    let out = article_sizes(r"\setbox0\vbox{\halign to 50pt{#\cr g\cr b\cr}}\xdef\out{\the\ht0/\the\dp0/\the\wd0}");
    assert_eq!(out, "16.30554pt/0.0pt/50.0pt");
    let out = article_sizes(r"\setbox0\vbox{\halign{#&#\cr g&b\cr}}\xdef\out{\the\ht0/\the\dp0}");
    assert_eq!(out, "6.94444pt/1.94444pt");
}

/// tex.web § 372: `\meaning` of a name built from unknown digits is the
/// meaning of a name of its family it may spell, or `\relax`: a path each,
/// so the text is one of those two, never unknown text.
#[test]
fn meaning_of_a_built_name_is_a_family_member_or_relax() {
    let source = r"\def\zq1#1#2{}
        \edef\h{\number\pdfuniformdeviate 20}
        \edef\out{\expandafter\meaning\csname zq\h\endcsname}";
    let text = out(source);
    assert!(!text.contains("text of") && text.contains("one of"), "{text}");
}

/// tex.web § 294: `\meaning` prints unknown text in a body as characters
/// of its class, never as the name that stands for it, so a delimited
/// split of the meaning sees only the real delimiters (xcolor's sanitize).
#[test]
fn meaning_of_an_unknown_body_hides_no_delimiter() {
    let analysis = analyze(
        r"\edef\a{x\number\pdfuniformdeviate 9 y}\def\b#1:#2:#3\stop{\def\c{#3}}\expandafter\b\meaning\a:\stop",
    );
    assert_eq!(last_body(&analysis, "c"), "");
}

/// tex.web § 392: inside a group of a delimited argument no delimiter
/// matches, so unknown digits there (a `,` among their class) leave where
/// the argument ends known. The argument read stays typed, not untyped.
#[test]
fn unknown_digits_in_a_group_leave_a_delimited_argument_exact() {
    let analysis = analyze(concat!(
        r"\dimen0=\pdfuniformdeviate 100000sp \advance\dimen0 by -50000sp ",
        r"\def\grab#1,#2\stop{\gdef\first{#1}\gdef\second{#2}}",
        r"\expandafter\grab\expandafter{\the\dimen0}\relax\penalty\fi\else\ifx\iftrue\hbox\vbox\kern\hskip\vskip,tail\stop",
        r"\gdef\reached{}",
    ));
    assert_eq!(last_body(&analysis, "second"), "tail");
    assert!(names(&analysis).iter().any(|n| n == "reached"), "the end was reached");
    assert_eq!(analysis.untyped, 0, "every unknown is typed");
}
