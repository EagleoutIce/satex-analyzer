
use satex::builtins::OccKind;
use satex::config::Config;
use satex::facts::MeaningKind;
use satex::machine::{Analysis, Machine};
use satex::query::{self, Filter, Query};

/// The LaTeX kernel read for real (and cached): expl3 is `expl3-code.tex`
/// itself, which only a format run has.
fn analyze_kernel(source: &str) -> Analysis {
    let cfg = Config { load_packages: false, load_classes: false, ..Config::default() };
    Machine::analyze(source, None, &cfg)
}

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

fn names(analysis: &Analysis) -> Vec<String> {
    analysis.facts.defs.iter().map(|d| analysis.interner.name(d.name).to_string()).collect()
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

#[test]
fn def_records_parameter_text_and_body() {
    let analysis = analyze(r"\def\pair#1,#2.{(#1|#2)}");
    let definition = definition(&analysis, "pair").expect("\\pair is defined");
    let mac = definition.mac.as_ref().expect("a macro");
    assert_eq!(mac.parameter_text.arity, 2);
    assert!(!mac.parameter_text.is_simple(), "the parameter text is delimited");
    assert_eq!(body(&analysis, "pair"), "(#1|#2)");
}

#[test]
fn delimited_arguments_are_matched() {
    let analysis = analyze(r"\def\pair#1,#2.{\def\seen{#1-#2}}\pair a,b.");
    assert_eq!(body(&analysis, "seen"), "a-b");
}

#[test]
fn undelimited_argument_loses_one_brace_level() {
    let analysis = analyze(r"\def\id#1{\def\seen{#1}}\id{{x}}");
    assert_eq!(body(&analysis, "seen"), "{x}");
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

#[test]
fn an_undecidable_conditional_analyzes_both_arms() {
    let analysis = analyze(r"\ifnum\pdfuniformdeviate2=0 \def\one{a}\else\def\two{b}\fi");
    let defined = names(&analysis);
    assert!(defined.contains(&"one".to_string()) && defined.contains(&"two".to_string()));
    assert!(!definition(&analysis, "one").expect("one").certain);
}

#[test]
fn ifnum_is_decided_from_counter_values() {
    let analysis = analyze(r"\newcounter{n}\setcounter{n}{7}\ifnum\value{n}>3\def\big{yes}\else\def\big{no}\fi");
    assert_eq!(body(&analysis, "big"), "yes");
}

#[test]
fn ifx_compares_meanings() {
    let analysis = analyze(r"\def\a{x}\def\b{x}\ifx\a\b\def\same{yes}\else\def\same{no}\fi");
    assert_eq!(body(&analysis, "same"), "yes");
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
    let labels: Vec<&str> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Label)
        .map(|o| o.key.as_str())
        .collect();
    assert_eq!(labels, ["sec:one"]);
    let lints = satex::lint::lint(&analysis);
    assert!(lints.iter().any(|l| l["code"] == "undefined-reference"));
}

#[test]
fn recursion_is_found() {
    let analysis = analyze_kernel(r"\newcommand{\qloop}[1]{#1\qloop{#1}}\newcommand{\ping}{\pong}\newcommand{\pong}{\ping}");
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
    assert!(analysis
        .facts
        .expansions
        .iter()
        .any(|e| e.meaning == MeaningKind::Undefined
            && analysis.interner.name(e.name) == "thisdoesnotexist"));
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
    let directory = std::env::temp_dir().join(format!("satex-options-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("qopts.sty"),
        "\\DeclareOption{draft}{\\def\\qmode{draft}}\\DeclareOption{final}{\\def\\qfinal{final}}\\ProcessOptions\\relax\n",
    )
    .unwrap();
    let source = "\\documentclass{article}\n\\usepackage[draft]{qopts}\n\\begin{document}\n\\end{document}\n";
    let document = directory.join("doc.tex");
    std::fs::write(&document, source).unwrap();
    let cfg = Config { load_packages: true, ..Config::default() };
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
            analysis
                .graph
                .outgoing(*id as u32)
                .iter()
                .any(|(_, kind)| kind.intersects(EdgeKind::SIDE_EFFECT_ON_CALL))
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
        assert!(
            name.chars().all(|c| !c.is_control()),
            "unprintable name: {name:?}"
        );
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
    let analysis = analyze_kernel(
        "\\makeatletter\\newcommand{\\my@internal}{1}\\makeatother\\newcommand{\\myvisible}{2}\n",
    );
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
    let analysis = analyze(
        r"\ExplSyntaxOn \seq_map_inline:Nn \l_tmpa_seq { #1 } \def\after{here} \ExplSyntaxOff",
    );
    assert_eq!(body(&analysis, "after"), "here", "the call did not swallow what follows it");
    assert!(
        !analysis.facts.expansions.iter().any(|e| e.meaning == MeaningKind::Undefined
            && analysis.interner.name(e.name) == "seq_map_inline:Nn"),
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
fn etex_expressions_are_evaluated() {
    let analysis = analyze(r"\ifnum\numexpr 2*3+1\relax>6 \def\big{yes}\else\def\big{no}\fi");
    assert_eq!(body(&analysis, "big"), "yes");
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
    use satex::value::{parse_dimen, UNIT};
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
    let direction =
        if forward { satex::query::Direction::Forward } else { satex::query::Direction::Backward };
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
    let definitions: Vec<u32> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| analysis.interner.name(d.name) == "a")
        .map(|d| d.span.line)
        .collect();
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
    let dir = std::env::temp_dir().join("satex-load-tree-test");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("part.tex"), "\\def\\b{2}\n").unwrap();
    let main = dir.join("main.tex");
    std::fs::write(&main, format!("{PRELUDE}\\input{{part}}\n")).unwrap();
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
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
fn a_skip_begun_in_another_file_ends_where_its_text_ends() {
    // TeX skips `\opener`'s `\iffalse` on through main.tex to its `\else`;
    // satex also follows the path where the skip ended at main.tex's text,
    // joins the two and records the gap (SEMANTICS.md § 14).
    let dir = std::env::temp_dir().join(format!("satex-skip-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("opener.tex"), "\\def\\opener{\\iffalse}\n").unwrap();
    let main = dir.join("main.tex");
    std::fs::write(
        &main,
        format!("{PRELUDE}\\input opener \\opener\\def\\a{{}}\\else\\def\\b{{}}\\fi\\def\\after{{}}\n"),
    )
    .unwrap();
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    };
    let source = std::fs::read_to_string(&main).unwrap();
    let analysis = Machine::analyze(&source, Some(&main), &cfg);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!definition(&analysis, "a").expect("the path that ended the skip").certain);
    assert!(definition(&analysis, "b").is_some(), "the path that skipped on");
    assert!(definition(&analysis, "after").expect("after the join").certain);
    assert_eq!(diagnosed(&analysis, "conditional-crosses-file").len(), 1);
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
fn a_nested_definition_substitutes_one_level_of_parameters() {
    let analysis = analyze("\\def\\nest#1{\\def\\mid##1{\\def\\deep####1{(#1,##1,####1)}}}\n\\nest{A}\n\\mid{B}\n");
    assert_eq!(body(&analysis, "deep"), "(A,B,#1)");
}

#[test]
fn edef_expands_a_macro_that_takes_arguments() {
    let analysis = analyze(r"\def\id#1{[#1]}\edef\a{\id{x}}");
    assert_eq!(body(&analysis, "a"), "[x]");
}

#[test]
fn edef_expands_undelimited_and_delimited_calls() {
    let analysis = analyze(r"\def\two#1#2{(#1,#2)}\def\del#1,#2.{<#1|#2>}\edef\a{\two ab}\edef\b{\del u,v.}");
    assert_eq!(body(&analysis, "a"), "(a,b)");
    assert_eq!(body(&analysis, "b"), "<u|v>");
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
fn edef_keeps_what_noexpand_and_protected_shield() {
    let analysis = analyze(
        r"\def\id#1{[#1]}\protected\def\p{q}\edef\a{\noexpand\id{x}}\edef\b{\p}",
    );
    assert_eq!(body(&analysis, "a"), r"\id {x}");
    assert_eq!(body(&analysis, "b"), "\\p ");
}

#[test]
fn expandafter_leaves_an_unexpandable_primitive_alone() {
    let analysis = analyze(r"\expandafter\relax\def\a{x}");
    assert_eq!(body(&analysis, "a"), "x");
}

#[test]
fn csname_expands_while_it_reads_the_name() {
    let analysis = analyze(r"\def\stop{\endcsname}\expandafter\def\csname built\stop{body}");
    assert_eq!(body(&analysis, "built"), "body");
}

#[test]
fn let_takes_the_space_after_one_optional_space() {
    let analysis = analyze("\\def\\:{\\let\\spce= } \\: \\ifx\\spce\\relax\\def\\seen{relax}\\else\\def\\seen{space}\\fi");
    assert_eq!(body(&analysis, "seen"), "space");
}

#[test]
fn ifcat_sees_through_noexpand() {
    let analysis = analyze(
        r"\def\test#1{\ifcat\noexpand~\noexpand#1\def\seen{active}\else\def\seen{control}\fi}\test\relax",
    );
    assert_eq!(body(&analysis, "seen"), "control");
}

#[test]
fn a_number_may_come_out_of_an_expansion() {
    let analysis = analyze(r"\def\one{1}\ifnum0\one=1 \def\seen{yes}\else\def\seen{no}\fi");
    assert_eq!(body(&analysis, "seen"), "yes");
}

fn depth_of(analysis: &Analysis, name: &str) -> Option<u16> {
    definition(analysis, name).map(|d| d.depth)
}

fn diagnosed(analysis: &Analysis, code: &str) -> Vec<String> {
    analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.message.clone())
        .collect()
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
    let analysis = analyze(
        r"\begingroup\aftergroup\gdef\aftergroup\tt\aftergroup{\aftergroup2\aftergroup}\endgroup",
    );
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
fn a_brace_delimited_parameter_stops_at_the_group() {
    // tex.web § 476: `#{` ends the parameter text and the `{` is put back.
    let analysis = analyze(r"\def\grab#1#{\def\got{#1}}\grab abc{x}");
    assert_eq!(body(&analysis, "got"), "abc");
}

#[test]
fn a_parameter_may_be_delimited_by_fi() {
    // The arm of a conditional is read token by token, so `\fi` is still a
    // token the macro can match on.
    let analysis = analyze(r"\def\grab#1\fi{\def\got{#1}}\iftrue \grab AB\fi");
    assert_eq!(body(&analysis, "got"), "AB");
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
fn a_switch_asked_about_governs_both_arms() {
    // `satex controls \ifdraft` makes that one switch undecidable, which is
    // what lets the reached-through-\csname arm be recorded as well.
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        opaque_conditionals: vec!["ifdraft".into()],
        ..Config::default()
    };
    let analysis = Machine::analyze(
        r"\newif\ifdraft\csname ifdraft\endcsname\def\a{}\else\def\b{}\fi",
        None,
        &cfg,
    );
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
    let cfg = Config { load_classes: true, opaque_conditionals: vec![satex::config::EVERY_SWITCH.into()], ..Config::default() };
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

#[test]
fn csname_keeps_spaces_and_braces_in_the_name() {
    // etex: `\ a` and `\a{b}`
    assert_eq!(out(r"\def\sp{ }\edef\out{\expandafter\string\csname\sp a\endcsname}"), r"\ a");
    assert_eq!(out(r"\edef\out{\expandafter\string\csname a{b}\endcsname}"), r"\a{b}");
}

#[test]
fn ifcsname_expands_the_name_it_tests() {
    // etex: `Y`, then `N` for a name cut short by an unexpandable token
    assert_eq!(out(r"\def\a{xx}\def\xx{}\edef\out{\ifcsname\a\endcsname Y\else N\fi}"), "Y");
    assert_eq!(out(r"\edef\out{\ifcsname\relax\endcsname Y\else N\fi}"), "N");
}

#[test]
fn box_tests_read_their_register_number() {
    // etex: `x`; the `0` belongs to `\ifvoid`
    assert_eq!(out(r"\edef\out{\ifvoid0 \fi x}"), "x");
}

#[test]
fn the_token_register_is_not_expanded_again_in_edef() {
    // etex: `\a `
    assert_eq!(out(r"\def\a{xx}\toks0={\a}\edef\out{\the\toks0}"), r"\a ");
}

#[test]
fn unexpanded_expands_to_find_its_brace() {
    // etex: `\a `
    assert_eq!(out(r"\def\a{xx}\def\b{\a}\edef\out{\unexpanded\expandafter{\b}}"), r"\a ");
}

#[test]
fn the_expands_its_operand() {
    // etex: `7`
    assert_eq!(out(r"\countdef\c=5 \c=7 \edef\out{\the\csname c\endcsname}"), "7");
}

#[test]
fn escapechar_governs_string_meaning_and_detokenize() {
    // etex: `relax`, `relax`, `/long macro:->/b `, `a `
    assert_eq!(out(r"\escapechar=-1 \edef\out{\string\relax}"), "relax");
    assert_eq!(out(r"\escapechar=300 \edef\out{\string\relax}"), "relax");
    assert_eq!(out(r"\long\def\a{\b}\escapechar=`\/ \edef\out{\meaning\a}"), "/long macro:->/b ");
    assert_eq!(out(r"\escapechar=-1 \edef\out{\detokenize{\a}}"), "a ");
}

#[test]
fn meaning_of_a_brace_delimited_macro_shows_the_brace_once() {
    // etex: `macro:#1{->x{`
    assert_eq!(out(r"\def\d#1#{x}\edef\out{\meaning\d}"), "macro:#1{->x{");
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

#[test]
fn the_empty_control_sequence_prints_as_csname_endcsname() {
    // etex: `\csname\endcsname`
    assert_eq!(out(r"\edef\out{\expandafter\string\csname\endcsname}"), r"\csname\endcsname");
}

#[test]
fn endlinechar_is_fixed_when_the_line_is_read() {
    // etex: `a b` — the first line already ended in `^^M` when
    // `\endlinechar=-1` was executed.
    assert_eq!(out("\\endlinechar=-1 \\edef\\out{a\nb}"), "a b");
}

#[test]
fn every_line_starts_in_state_n() {
    // etex: `x\^^My` — the escape at the end of the line names `^^M`, and the
    // next line's leading spaces are skipped.
    assert_eq!(out("\\def\\out{x\\\n   y}"), "x\\\ry");
}

/// Each arm of an undecided conditional runs on its own copy of the state:
/// the `\else` arm does not see what the true arm defined.
#[test]
fn undecided_arms_do_not_see_each_other() {
    let analysis = analyze(r"\ifnum\pdfuniformdeviate2=0 \def\x{a}\else\ifx\x\undefined\def\clean{yes}\fi\fi");
    assert_eq!(body(&analysis, "clean"), "yes");
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
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
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
fn signs_apply_to_internal_quantities() {
    // etex: `-5`, `-2.0pt`, `-2.0pt`
    assert_eq!(out(r"\count1=5 \edef\out{\number-\count1}"), "-5");
    assert_eq!(out(r"\dimen1=2pt \edef\out{\the\dimexpr-\dimen1\relax}"), "-2.0pt");
    assert_eq!(out(r"\dimen1=2pt \dimen2=-\dimen1 \edef\out{\the\dimen2}"), "-2.0pt");
}

#[test]
fn glue_is_scanned_with_its_keywords_and_orders() {
    // etex: `1.0pt plus 1.0fill minus 2.0fill`, then negated, then coerced
    let set = r"\skip3=1pt plus 1fil l minus 2 fill ";
    assert_eq!(out(&format!(r"{set}\edef\out{{\the\skip3}}")), "1.0pt plus 1.0fill minus 2.0fill");
    assert_eq!(
        out(&format!(r"{set}\skip4=-\skip3 \edef\out{{\the\skip4}}")),
        "-1.0pt plus -1.0fill minus -2.0fill"
    );
    assert_eq!(out(&format!(r"{set}\dimen0=\skip3 \edef\out{{\the\dimen0}}")), "1.0pt");
    assert_eq!(out(r"\count1=5 \skip7=\count1 sp \edef\out{\the\skip7}"), "0.00008pt");
}

#[test]
fn dimensions_follow_the_engine_units_and_factors() {
    // etex: `72.26999pt`; an integer register as unit counts in sp and the
    // unit after it is left over: `0.0001ptpt\relax `
    assert_eq!(out(r"\edef\out{\the\dimexpr 1truein\relax}"), "72.26999pt");
    assert_eq!(out(r"\count1=5 \edef\out{\the\dimexpr 1.5\count1 pt\relax}"), r"0.0001ptpt\relax ");
    assert_eq!(out(r"\count1=5 \edef\out{\number\count1\count1}"), r"5\count 1");
}

#[test]
fn glue_and_token_parameters_hold_their_kind() {
    // etex: `2.0pt plus 1.0pt`, `3.0mu plus 1.0fill`, `abc,abcx`
    assert_eq!(out(r"\parskip=2pt plus 1pt \edef\out{\the\parskip}"), "2.0pt plus 1.0pt");
    assert_eq!(out(r"\thinmuskip=3mu plus 1fill \edef\out{\the\thinmuskip}"), "3.0mu plus 1.0fill");
    assert_eq!(
        out(r"\everypar{abc}\toks3=\expandafter{\the\everypar x}\edef\out{\the\everypar,\the\toks3}"),
        "abc,abcx"
    );
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
fn pdftex_absolute_conditionals_are_conditionals() {
    // etex: `YN`; and `N`, because the skipped `\ifpdfabsnum` needs its own `\fi`
    assert_eq!(out(r"\edef\out{\ifpdfabsnum -5>3 Y\else N\fi\ifpdfabsdim -2pt<1pt Y\else N\fi}"), "YN");
    assert_eq!(out(r"\edef\out{\iffalse\ifpdfabsnum 1=1 \fi Y\else N\fi}"), "N");
}

#[test]
fn engine_versions_and_levels_are_known() {
    // etex: `2.6,140`, `0`, `1`, `Y`, `1`
    assert_eq!(out(r"\edef\out{\the\eTeXversion\eTeXrevision,\the\pdftexversion}"), "2.6,140");
    assert_eq!(out(r"\edef\out{\the\currentgrouplevel}"), "0");
    assert_eq!(out(r"{\xdef\out{\the\currentgrouplevel}}"), "1");
    assert_eq!(out(r"\edef\out{\ifnum\currentiflevel=1 Y\else N\fi}"), "Y");
    assert_eq!(out(r"\edef\out{\the\inputlineno}"), "1");
}

#[test]
fn a_font_table_is_extended_only_past_its_end() {
    // etex: `0.00018pt,0.0pt,42` — cmr10 has 7 parameters; the font loaded
    // last grows to 9, the new ones starting at zero.
    assert_eq!(
        out(r"\font\w=cmr10 at 7sp \fontdimen9\w=12sp \hyphenchar\w=42 \edef\out{\the\fontdimen9\w,\the\fontdimen8\w,\the\hyphenchar\w}"),
        "0.00018pt,0.0pt,42"
    );
}

#[test]
fn pdftex_string_utilities_compute_their_result() {
    // pdftex: `-101,415A,AJ` and the MD5 of `abc`
    assert_eq!(
        out(r"\edef\out{\pdfstrcmp{abc}{abd}\pdfstrcmp{\relax}{\relax}\pdfstrcmp{b}{a},\pdfescapehex{AZ},\pdfunescapehex{414a}}"),
        "-101,415A,AJ"
    );
    assert_eq!(out(r"\edef\out{\pdfmdfivesum{abc}}"), "900150983CD24FB0D6963F7D28E17F72");
}

#[test]
fn parshape_and_penalty_arrays_are_read_back() {
    // etex: `2,2.0pt,3.0pt,3.0pt,3.0pt,4.0pt` and `3,102,103`
    assert_eq!(
        out(r"\parshape 2 1pt 2pt 3pt 4pt \edef\out{\the\parshape,\the\parshapelength 1,\the\parshapeindent 2,\the\parshapedimen 3,\the\parshapedimen 7,\the\parshapelength 5}"),
        "2,2.0pt,3.0pt,3.0pt,3.0pt,4.0pt"
    );
    assert_eq!(
        out(r"\interlinepenalties 3 101 102 103 \edef\out{\the\interlinepenalties0,\the\interlinepenalties2,\the\interlinepenalties9}"),
        "3,102,103"
    );
}

#[test]
fn mathchardef_is_a_meaning_of_its_own() {
    // etex: `\mathchar"7123` and `N`
    assert_eq!(out(r#"\mathchardef\mc="7123 \edef\out{\meaning\mc}"#), r#"\mathchar"7123"#);
    assert_eq!(out(r"\chardef\a=1 \mathchardef\b=1 \edef\out{\ifx\a\b Y\else N\fi}"), "N");
}

#[test]
fn stomach_primitives_stay_unexpanded_in_edef() {
    // etex: `\font \x \openin 1\message {a}`
    assert_eq!(out(r"\let\x\relax \edef\out{\font\x\openin1\message{a}}"), r"\font \x \openin 1\message {a}");
}

#[test]
fn endlinechar_is_restored_by_the_end_of_a_group() {
    // etex: `a b`
    assert_eq!(out("{\\endlinechar=-1 }\n\\edef\\out{a\nb}"), "a b");
}

#[test]
fn globaldefs_overrides_the_prefix() {
    // etex: `G`, `N`, `4`
    assert_eq!(out(r"{\globaldefs=1 \def\gd{G}}\edef\out{\gd}"), "G");
    assert_eq!(out(r"\globaldefs=-1 {\global\def\ge{E}}\globaldefs=0 \edef\out{\ifdefined\ge Y\else N\fi}"), "N");
    assert_eq!(out(r"\count1=1 {\globaldefs=1 \count1=4 }\edef\out{\the\count1}"), "4");
}

#[test]
fn active_characters_and_control_symbols_are_distinct() {
    // etex: `AB`, and a `~` of category 12 in `\csname` names the control symbol `\~`
    assert_eq!(out(r"\catcode`\~=13 \def~{A}\def\~{B}\edef\out{~\~}"), "AB");
    assert_eq!(out(r"\catcode`\~=13 \def~{A}\def\~{B}\edef\out{\csname\string~\endcsname}"), "B");
}

#[test]
fn afterassignment_after_setbox_goes_inside_the_box() {
    // etex: `inbox:b` — the token runs at the start of the box's contents
    assert_eq!(
        out(r"\def\out{}\def\b{\edef\out{\out b}}\afterassignment\b\setbox0\hbox{\edef\out{inbox:\out}}"),
        "inbox:b"
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
fn edef_stores_what_the_and_unexpanded_give_without_parameters() {
    // etex: `macro:#1->##1a#1`: tex.web § 478 appends those tokens past the
    // parameter scan, so their `#` stays a character (expl3's
    // `\tl_put_right:Nn` builds token lists of code that way).
    assert_eq!(
        out(r"\toks0{#1}\edef\y#1{\the\toks0\unexpanded{a}#1}\edef\out{\meaning\y}"),
        "macro:#1->##1a#1"
    );
}

#[test]
fn a_copy_of_a_primitive_is_that_primitive() {
    // etex: `aftergroup TF-1` (no escape character once it is -1): `\let` keeps which primitive it copied, so
    // expl3's `\tex_escapechar:D` sets `\escapechar` itself.
    assert_eq!(
        out(r"\def\space{ }\let\a\aftergroup \let\e\escapechar \e=-1
            \edef\out{\meaning\a\space\ifx\a\aftergroup T\else F\fi
              \ifx\a\afterassignment T\else F\fi
              \expandafter\ifx\csname new\endcsname\relax T\else F\fi\the\escapechar}"),
        "aftergroup TFT-1"
    );
}

#[test]
fn an_expression_expands_to_find_its_operators() {
    // etex: `3|21`: e-TeX reads the next non-blank non-call token.
    assert_eq!(
        out(r"\def\a{(4-1)}\edef\out{\number\numexpr\a\relax|\number\numexpr 2*\a*\a+\a\relax}"),
        "3|21"
    );
}

#[test]
fn a_register_starts_at_zero_for_advance() {
    // etex: `1`: expl3's `\int_gincr:N` on a fresh `\newcount` register.
    assert_eq!(out(r"\countdef\x=30 \global\advance\x by 1 \edef\out{\the\x}"), "1");
}

#[test]
fn edef_body_ends_at_the_brace_expansion_balances() {
    // etex: `ab`: tex.web § 477 scans the body while expanding it, so the
    // `}` skipped by `\iffalse` does not end it (expl3's `\__iow_wrap_line:nw`).
    assert_eq!(out(r"\edef\out{\iffalse}\fi a\iffalse{\fi b}"), "ab");
}

#[test]
fn a_conditional_its_test_opened_ends_first() {
    // etex: `F|T|F`: the inner `\ifx` is still open when `\if` decides, so
    // the first `\fi` skipped is its own (tex.web § 500); expl3's
    // `\__fp_parse_exponent:N` tests `\if:w e \if:w E …\fi:`.
    assert_eq!(
        out(r"\edef\out{\if e\ifx AB x\else y\fi T\else F\fi|\if y\ifx AB x\else y\fi T\else F\fi|\if x\ifx AA x\else y\fi T\else F\fi}"),
        "F|T|F"
    );
}

#[test]
fn expanded_scans_its_text_while_expanding() {
    // etex: `{ab}`: pdfTeX's `\expanded` reads like an `\edef` body, so the
    // `}}` its `\iffalse` skips do not end it (expl3's `\__keys_property_find_auxii:w`).
    assert_eq!(out(r"\edef\out{\expanded{{a\iffalse}}\fi b}}}"), "{ab}");
}

#[test]
fn read_appends_the_end_of_line_character_and_balances_braces() {
    // etex: `a;b |{c d}e |\par `: tex.web § 483 appends `\endlinechar` to
    // every line and reads on until the braces balance; past the end the
    // line is empty (expl3 reads UnicodeData.txt with `\ior_map_variable`).
    let dir = std::env::temp_dir().join(format!("satex-read-{}", std::process::id()));
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
fn a_font_identifier_means_the_font_it_selects() {
    // etex: `select font cmr10 at 0.00003pt|select font nullfont`
    // (tex.web § 1261); expl3's cctabs and intarrays test for it.
    assert_eq!(
        out(r"\font\a=cmr10 at 2sp \edef\out{\meaning\a|\meaning\nullfont}"),
        "select font cmr10 at 0.00003pt|select font nullfont"
    );
}

#[test]
fn an_edef_copies_the_expl3_definition_functions() {
    // etex with expl3: `\cs_set_eq:NN \a \relax x|undefined`: `\cs_set_eq:NN`
    // is `\protected` (l3basics), so the `\edef` stores it and defines
    // nothing.
    assert_eq!(
        out(r"\catcode`\_=11 \catcode`\:=11
            \edef\t{\cs_set_eq:NN \noexpand\a \relax x}
            \edef\out{\meaning\t|\meaning\a}"),
        r"macro:->\cs_set_eq:NN \a \relax x|undefined"
    );
}

#[test]
fn a_group_saves_a_name_once_and_keeps_a_global_one() {
    // tex: `[4][7][7]`: a local assignment saves the old meaning once per
    // level, and one made global since is retained when the group ends
    // (tex.web §§ 277, 283).
    assert_eq!(
        out(r"\def\a{1}{\def\a{2}\def\a{3}\gdef\a{4}\def\a{5}}\edef\x{[\a]}
            {\def\a{6}{\gdef\a{7}}\xdef\y{[\a]}}\edef\out{\x\y[\a]}"),
        "[4][7][7]"
    );
}

#[test]
fn errmessage_takes_only_its_message() {
    // etex: `a`: latex.ltx opens with `\ifnum…\errmessage{…}\fi`; the
    // engine primitive has no help text to read, so the `\else` stays.
    assert_eq!(out(r"\def\out{a}\iftrue\errmessage{oops}\else\def\out{b}\fi"), "a");
}

#[test]
fn the_format_is_read_in_vertical_mode() {
    // pdflatex: latex.ltx typesets nothing, so its `\par` finds vertical
    // mode (tex.web § 211); `\ifhmode` in the preamble is false too.
    let analysis = analyze_kernel(r"\ifnum\pdfuniformdeviate2=0 \def\a{v}\else\def\a{x}\fi\ifhmode\def\b{h}\else\def\b{v}\fi");
    assert_eq!(body(&analysis, "a"), "v");
    assert_eq!(body(&analysis, "b"), "v");
}

#[test]
fn an_undefined_name_inside_csname_is_dropped() {
    // etex: `\ab`: expanding an undefined control sequence reports it and
    // removes it (tex.web § 370), so the name goes on to `\endcsname`.
    assert_eq!(out(r"\edef\out{\expandafter\string\csname a\undefinedthing b\endcsname}"), r"\ab");
}

#[test]
fn a_line_end_made_a_space_is_skipped_by_futurelet_and_a_space_delimited_macro() {
    // etex: `Y`: latex.ltx's `\@ifnextchar` skips a space by `\@xifnch`,
    // whose parameter text is one space; the line end is tokenized with the
    // catcode in force when the reader reaches it (tex.web § 343), here 10.
    assert_eq!(
        out("\\catcode`\\@=11 \\catcode`\\[=12
\\def\\:{\\let\\@sptoken= } \\:
\\def\\:{\\@xifnch} \\expandafter\\def\\: {\\futurelet\\@let@token\\@ifnch}
\\def\\@ifnch{\\ifx\\@let@token\\@sptoken \\let\\reserved@c\\@xifnch \\else \\ifx\\@let@token[\\def\\r{Y}\\else\\def\\r{N}\\fi \\let\\reserved@c\\relax\\fi \\reserved@c}
\\def\\t{\\futurelet\\@let@token\\@ifnch}
\\begingroup\\catcode13=10 \\t
 [x]\\global\\let\\r\\r\\endgroup
\\edef\\out{\\r}"),
        "Y"
    );
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
    assert!(diagnosed(&analysis, "recursion-widened").is_empty());
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
/// when all ten are defined (pgfmath's number check).
#[test]
fn one_unknown_digit_names_one_of_ten_names() {
    let analysis = analyze(
        r"\setbox0\hbox{x}\def\d#1{\expandafter\def\csname d@#1\endcsname{}}
          \d0\d1\d2\d3\d4\d5\d6\d7\d8\d9
          \def\a#1{\expandafter\ifx\csname d@\string#1\endcsname\relax\def\no{}\else\def\yes{}\fi}
          \edef\t{\the\wd0}\expandafter\a\t",
    );
    let defined = names(&analysis);
    assert!(defined.contains(&"yes".to_string()) && !defined.contains(&"no".to_string()), "{defined:?}");
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
