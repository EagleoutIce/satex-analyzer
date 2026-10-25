//! `satex explain`: what a control sequence means, and where it comes from.

use crate::support::{analyze_kernel, analyze_plain as analyze};
use serde_json::{Value as Json, json};

use crate::support::{analyze_latex, installed};
use satex::config::{Config, Load};
use satex::machine::{Analysis, Machine};
use satex::query::{self, Record};

fn explain_one(analysis: &Analysis, name: &str) -> Record {
    query::explain(analysis, &[name.to_string()], false)
        .unwrap_or_else(|e| panic!("\\{name} did not explain: {e}"))
        .0
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("\\{name} did not explain"))
}

fn str_field<'a>(record: &'a Record, field: &str) -> Option<&'a str> {
    record.get(field).and_then(Json::as_str)
}

#[test]
fn takes_is_a_min_max_object() {
    // `s O{d} m`: one mandatory argument, three items total (star included).
    let analysis = analyze_kernel(r"\NewDocumentCommand\x{s O{d} m}{#3}");
    let record = explain_one(&analysis, "x");
    assert_eq!(record.get("takes"), Some(&json!({ "min": 1, "max": 3 })));
}

#[test]
fn takes_is_unbounded_for_def() {
    let analysis = analyze(r"\relax");
    let record = explain_one(&analysis, "def");
    // `\def\name#params{body}`: two mandatory pieces, no upper bound.
    assert_eq!(record.get("takes"), Some(&json!({ "min": 2, "max": null })));
}

#[test]
fn effective_signature_for_a_plain_macro() {
    let analysis = analyze(r"\def\highlight#1{\textbf{#1}}");
    let record = explain_one(&analysis, "highlight");
    assert_eq!(str_field(&record, "effective"), Some(r"\highlight{1}"));
}

#[test]
fn effective_signature_with_an_optional_argument() {
    // `\cite` is already `latex.ltx`'s own (base.ltx:17754), so a fresh name
    // is used here instead of redefining a kernel command.
    let analysis = analyze_kernel(r"\newcommand{\quote}[2][see]{#1:#2}");
    let record = explain_one(&analysis, "quote");
    assert_eq!(str_field(&record, "effective"), Some(r"\quote[see]{1}"));
}

#[test]
fn effective_signature_for_usepackage() {
    // `\documentclass` makes `\usepackage` `\RequirePackage`; the probe
    // sees it take `[options]{names}[date]` in that order.
    let analysis = analyze_kernel(r"\documentclass{article}");
    let record = explain_one(&analysis, "usepackage");
    assert_eq!(str_field(&record, "effective"), Some(r"\usepackage[1]{2}[3]"));
    assert_eq!(str_field(&record, "signature"), Some("omo"));
}

/// A probe ends where the command prompts the terminal (a missing file's
/// `\typein`): what follows is the user's answer, not an argument.
#[test]
fn effective_signature_for_input_ends_at_the_prompt() {
    let analysis = analyze_kernel(r"\documentclass{article}");
    let record = explain_one(&analysis, "input");
    assert_eq!(str_field(&record, "effective"), Some(r"\input{1}"));
    assert_eq!(str_field(&record, "signature"), Some("m"));
}

#[test]
fn primitive_body_and_kind() {
    let analysis = analyze(r"\relax");
    let record = explain_one(&analysis, "relax");
    assert_eq!(str_field(&record, "body"), Some(r"\relax"));
    assert_eq!(str_field(&record, "kind"), Some("primitive"));
}

/// A `\newif` switch is `\let` to `\iffalse`: its body names that
/// primitive, and a conditional takes no arguments.
#[test]
fn a_switch_shows_the_conditional_it_is() {
    let analysis = analyze(r"\let\ifdraft=\iffalse");
    let record = explain_one(&analysis, "ifdraft");
    assert_eq!(str_field(&record, "body"), Some(r"\iffalse"));
    assert_eq!(str_field(&record, "effective"), Some(r"\ifdraft"));
}

#[test]
fn expands_lists_callees_in_order_deduplicated() {
    // `expands` is what a call actually ran, not what the body merely names
    // (`calls`, the static graph `satex query calls` still answers), so `\a`
    // has to run for there to be anything to list.
    let analysis = analyze(r"\def\b{B}\def\c{C}\def\a{\b\c\b}\a");
    let record = explain_one(&analysis, "a");
    let expands: Vec<&str> = record
        .get("expands")
        .and_then(Json::as_array)
        .expect("expands is an array")
        .iter()
        .map(|v| v.as_str().expect("expands entries are strings"))
        .collect();
    assert_eq!(expands, vec![r"\b", r"\c"]);
}

#[test]
fn origin_present_for_a_kernel_name() {
    if !installed() {
        return;
    }
    // With the real kernel loaded, a name with no definition site of its own
    // (a primitive) still carries the format's path, not a null one.
    let analysis = analyze_latex(r"\relax");
    let record = explain_one(&analysis, "relax");
    assert_eq!(str_field(&record, "package"), Some("kernel"));
    assert!(
        record.get("path").is_some_and(Json::is_string),
        "a kernel name's path should not be null once the format is known"
    );
}

#[test]
fn documentation_field_for_a_package_command() {
    if !installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(
        r"\documentclass{article}
\usepackage{enumitem}
\begin{document}\end{document}",
        None,
        &cfg,
    );
    let record = explain_one(&analysis, "setlist");
    assert_eq!(str_field(&record, "documentation"), Some("package enumitem"));
}

// --- Signatures come from running the command, not from the names of the
// kernel's dispatchers: `\@dblarg`, `\@ifstar` and copies of them under
// other names read the same.

#[test]
fn effective_signature_from_dblarg_dispatch_even_never_called() {
    // No call anywhere in this source: the probe runs `\mysec` itself.
    // `\@dblarg` reads an optional bracket and then a mandatory argument.
    let analysis = analyze_kernel(r"\makeatletter\def\myhelper[#1]#2{}\def\mysec{\@dblarg\myhelper}\makeatother");
    let record = explain_one(&analysis, "mysec");
    assert_eq!(str_field(&record, "effective"), Some(r"\mysec[1]{2}"));
    assert_eq!(str_field(&record, "signature"), Some("om"));
    assert_eq!(record.get("takes"), Some(&json!({ "min": 1, "max": 2 })));
}

#[test]
fn effective_signature_combines_star_and_dblarg_like_section() {
    let analysis = analyze_kernel(
        r"\makeatletter\def\starred#1{}\def\myhelper[#1]#2{}\def\mysec{\@ifstar\starred{\@dblarg\myhelper}}\makeatother",
    );
    let record = explain_one(&analysis, "mysec");
    assert_eq!(str_field(&record, "effective"), Some(r"\mysec*?[1]{2}"));
    assert_eq!(record.get("takes"), Some(&json!({ "min": 1, "max": 3 })));
}

#[test]
fn renamed_dispatchers_give_the_same_signature() {
    // The kernel's `\@ifstar`/`\kernel@ifnextchar`/`\@dblarg` logic under
    // names no table knows: the signature is still `s o m`.
    let analysis = analyze(
        r"\def\myifnext#1#2#3{\let\myd=#1\def\mya{#2}\def\myb{#3}\futurelet\mytok\myifnch}
\def\myifnch{\ifx\mytok\myd\let\myc\mya\else\let\myc\myb\fi\myc}
\def\myfirst#1#2{#1}
\def\myifstar#1{\myifnext*{\myfirst{#1}}}
\def\mydbl#1{\myifnext[{#1}{\myxdbl{#1}}}
\def\myxdbl#1#2{#1[{#2}]{#2}}
\def\starred#1{}
\def\unstarred[#1]#2{}
\def\mysec{\myifstar\starred{\mydbl\unstarred}}",
    );
    let record = explain_one(&analysis, "mysec");
    assert_eq!(str_field(&record, "signature"), Some("som"));
    assert_eq!(str_field(&record, "effective"), Some(r"\mysec*?[1]{2}"));
}

#[test]
fn a_default_shows_when_it_is_substituted() {
    // `\myopt` supplies `dflt` itself when the bracket is left out, the way
    // `\@testopt` does, under a name of its own.
    let analysis = analyze(
        r"\def\myopt{\futurelet\next\mychoose}
\def\mychoose{\ifx[\next\expandafter\myinner\else\expandafter\mydefault\fi}
\def\mydefault{\myinner[dflt]}
\def\myinner[#1]#2{#1#2}",
    );
    let record = explain_one(&analysis, "myopt");
    assert_eq!(str_field(&record, "signature"), Some("O{dflt}m"));
    assert_eq!(str_field(&record, "effective"), Some(r"\myopt[dflt]{1}"));
}

#[test]
fn section_is_star_optional_mandatory() {
    if !installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(r"\documentclass{article}\begin{document}\end{document}", None, &cfg);
    let record = explain_one(&analysis, "section");
    assert_eq!(str_field(&record, "signature"), Some("som"));
    assert_eq!(str_field(&record, "effective"), Some(r"\section*?[1]{2}"));
}

#[test]
fn document_command_signature_from_the_probe() {
    let analysis = analyze_kernel(r"\NewDocumentCommand\qx{s O{d} m}{#3}");
    let record = explain_one(&analysis, "qx");
    assert_eq!(str_field(&record, "effective"), Some(r"\qx*?[d]{1}"));
}

#[test]
fn never_called_newcommand_with_a_default() {
    let analysis = analyze_kernel(r"\newcommand{\qq}[3][see]{#1#2#3}");
    let record = explain_one(&analysis, "qq");
    assert_eq!(str_field(&record, "signature"), Some("O{see}mm"));
    assert_eq!(record.get("uses"), Some(&json!(0)));
}

#[test]
fn paper_usepackage_preamble_and_document() {
    if !installed() {
        return;
    }
    // A copy of samples/paper.tex and the package it loads from its folder.
    let directory = crate::support::tmp_root().join(format!("satex-explain-paper-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");
    for file in ["paper.tex", "mypackage.sty", "preamble.tex"] {
        std::fs::copy(samples.join(file), directory.join(file)).unwrap();
    }
    let path = directory.join("paper.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let analysis = Machine::analyze(&source, Some(&path), &crate::support::config());
    let (records, _) = query::explain(&analysis, &["usepackage".to_string()], false).unwrap();
    let _ = std::fs::remove_dir_all(&directory);
    assert_eq!(records.len(), 2, "records: {records:?}");
    let (preamble, document) = (&records[0], &records[1]);
    assert_eq!(str_field(preamble, "when"), Some("preamble"));
    // `\documentclass` makes `\usepackage` `\RequirePackage` (latex.ltx,
    // `\let\usepackage\RequirePackage`).
    assert_eq!(str_field(preamble, "by"), Some(r"\let"));
    assert_eq!(str_field(preamble, "signature"), Some("omo"));
    assert_eq!(str_field(preamble, "effective"), Some(r"\usepackage[1]{2}[3]"));
    assert_eq!(preamble.get("uses"), Some(&json!(4)));
    assert_eq!(str_field(document, "when"), Some("document"));
    assert_eq!(str_field(document, "effective"), Some(r"\usepackage"));
    assert_eq!(document.get("uses"), Some(&json!(0)));
    assert!(
        str_field(document, "error").is_some_and(|e| e.contains("only in preamble")),
        "the document meaning is the error: {document:?}"
    );
}

#[test]
fn kernel_call_graph_is_there_from_a_cached_format() {
    // Twice: the second run installs the format from its cache.
    for _ in 0..2 {
        let analysis = analyze_kernel(r"\relax");
        let from = analysis.interner.lookup("newcommand").unwrap();
        let callees: Vec<&str> = analysis.calls.get(from).map_or_else(Vec::new, |i| {
            analysis.calls.edges[i].iter().map(|t| analysis.interner.name(analysis.calls.nodes[*t])).collect()
        });
        assert!(callees.contains(&"new@command"), "\\newcommand calls: {callees:?}");
    }
}

#[test]
fn uses_counts_expansions_at_call_sites() {
    let analysis = analyze(r"\def\a{x}\a\a\a");
    let record = explain_one(&analysis, "a");
    assert_eq!(record.get("uses"), Some(&json!(3)));
}

#[test]
fn explain_has_no_calls_field_only_expands() {
    // `calls` (the static call graph) duplicated `expands` (what the run
    // actually expanded) for a macro with no conditionals; only `expands`
    // is in the record now, and the `calls` *query* is unaffected. `\a`
    // takes an argument so its expansion is tracked as `\b`'s caller (a
    // zero-argument macro's is not — a separate, existing thing).
    let analysis = analyze(r"\def\b{B}\def\a#1{\b#1}\a{x}");
    let record = explain_one(&analysis, "a");
    assert!(record.get("calls").is_none());
    assert!(record.get("expands").is_some());
}

#[test]
fn class_field_not_package_for_a_name_from_the_document_class() {
    if !installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(r"\documentclass{article}\begin{document}\end{document}", None, &cfg);
    let record = explain_one(&analysis, "section");
    assert_eq!(str_field(&record, "class"), Some("article"));
    assert!(record.get("package").is_none());
    assert_eq!(str_field(&record, "documentation"), Some("class article"));
}

#[test]
fn explain_shows_preamble_and_document_meanings_when_they_differ() {
    let analysis = analyze_kernel(
        r"\newcommand{\foo}{preamble}
\begin{document}
\renewcommand{\foo}{document}
\end{document}",
    );
    let (records, _) =
        query::explain(&analysis, &["foo".to_string()], false).unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert_eq!(records.len(), 2, "records: {records:?}");
    assert_eq!(str_field(&records[0], "when"), Some("preamble"));
    assert_eq!(str_field(&records[0], "body"), Some("preamble"));
    assert_eq!(str_field(&records[1], "when"), Some("document"));
    assert_eq!(str_field(&records[1], "body"), Some("document"));
}

#[test]
fn usepackage_differentiates_preamble_from_document_error() {
    // `\@preamblecmds` `\let`s every preamble-only command to `\@notprerr`
    // right after `\begin{document}` (ltclass.dtx); the default view shows
    // the meaning that answers `\usepackage{x}` and notes, separately, that
    // it turns into that error once the document has started.
    if !installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(r"\documentclass{article}\begin{document}\end{document}", None, &cfg);
    let (records, _) = query::explain(&analysis, &["usepackage".to_string()], false)
        .unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert_eq!(records.len(), 2, "records: {records:?}");
    assert_eq!(str_field(&records[0], "when"), Some("preamble"));
    assert!(
        !str_field(&records[0], "body").unwrap_or_default().contains("preamble"),
        "the preamble meaning should be the real \\usepackage, not the error stub: {:?}",
        records[0].get("body")
    );
    assert_eq!(str_field(&records[1], "when"), Some("document"));
    assert!(
        str_field(&records[1], "body").unwrap_or_default().contains("preamble"),
        "the document meaning should be the \\@notprerr error: {:?}",
        records[1].get("body")
    );
}

#[test]
fn explain_shows_one_record_when_preamble_and_document_agree() {
    let analysis = analyze_kernel(
        r"\newcommand{\foo}{same}
\begin{document}
\foo
\end{document}",
    );
    let (records, hints) =
        query::explain(&analysis, &["foo".to_string()], false).unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert_eq!(records.len(), 1, "records: {records:?}");
    assert!(records[0].get("when").is_none());
    assert!(hints.is_empty());
}

fn expl3_mismatches(source: &str) -> Vec<String> {
    let analysis = analyze_kernel(source);
    satex::lint::lint(&analysis)
        .into_iter()
        .filter(|r| r.get("code").and_then(Json::as_str) == Some("expl3-signature-mismatch"))
        .filter_map(|r| r.get("name").and_then(Json::as_str).map(str::to_string))
        .collect()
}

#[test]
fn expl3_signature_mismatch_reports_a_function_that_takes_fewer() {
    let found = expl3_mismatches(
        r"\ExplSyntaxOn
\cs_new:Npn \my_short:nn #1 { #1 }
\cs_new:Npn \my_right:nn #1#2 { #1#2 }
\ExplSyntaxOff",
    );
    assert_eq!(found, vec![r"\my_short:nn".to_string()]);
}

#[test]
fn a_delimited_parameter_text_is_its_own_signature() {
    // tex.web §§ 391-399: the text before `#1` must follow the name, and
    // each argument runs to the text after it.
    let analysis = analyze_kernel("\\def\\Weird key: #1, value: #2, magic: #3\\;{%\n  #1#2#3%\n}\n");
    let record = explain_one(&analysis, "Weird");
    assert_eq!(str_field(&record, "effective"), Some(r"\Weird key: {1}, value: {2}, magic: {3}\;"));
    assert_eq!(record.get("takes"), Some(&json!({ "min": 3, "max": 3 })));
}

#[test]
fn undelimited_parameters_are_mandatory_arguments() {
    let analysis = analyze_kernel(r"\def\pair#1#2{#1#2}");
    let record = explain_one(&analysis, "pair");
    assert_eq!(str_field(&record, "effective"), Some(r"\pair{1}{2}"));
    assert_eq!(record.get("takes"), Some(&json!({ "min": 2, "max": 2 })));
}

#[test]
fn a_trailing_delimiter_runs_to_its_text() {
    let analysis = analyze_kernel(r"\def\upto#1.{#1}");
    let record = explain_one(&analysis, "upto");
    assert_eq!(str_field(&record, "effective"), Some(r"\upto{1}."));
}

#[test]
fn a_tail_call_adds_what_it_reads_after_the_parameter_text() {
    let analysis = analyze_kernel(r"\newcommand\qtail[2][d]{#1#2}\def\qhead key: #1\;{#1\qtail}");
    let record = explain_one(&analysis, "qhead");
    assert_eq!(str_field(&record, "signature"), Some(r"key: u{\;}O{d}m"));
    assert_eq!(str_field(&record, "effective"), Some(r"\qhead key: {1}\;[d]{2}"));
}

/// `(name, effective, signature)` for each, from one analysis.
fn assert_calls(analysis: &Analysis, expected: &[(&str, &str, &str)]) {
    for (name, effective, signature) in expected {
        let record = explain_one(analysis, name);
        assert_eq!(str_field(&record, "effective"), Some(*effective), "\\{name}");
        assert_eq!(str_field(&record, "signature"), Some(*signature), "\\{name}");
    }
}

#[test]
fn parameter_texts_are_learned_from_what_fails_to_match() {
    // tex.web §§ 392, 398: leading, infix and trailing text; `\long`,
    // `\outer`, `\protected`; control words (§ 354: the space after one is
    // skipped) and control symbols (a space after one is kept).
    let analysis = analyze_kernel(
        r"\def\Weird key: #1, value: #2, magic: #3\;super #4\; {}
\def\lead x#1{}\def\infix#1-#2{}\def\trail#1.{}
\long\def\ld#1\par{}\outer\def\od#1;{}\protected\def\pd<#1>{}
\def\ctrlw\foo #1{}\def\ctrls\;#1{}",
    );
    assert_calls(
        &analysis,
        &[
            (
                "Weird",
                r"\Weird key: {1}, value: {2}, magic: {3}\;super {4}\; ",
                r"key: u{, value: }u{, magic: }u{\;super }u{\; }",
            ),
            ("lead", r"\lead x{1}", "xm"),
            ("infix", r"\infix{1}-{2}", "u{-}m"),
            ("trail", r"\trail{1}.", "u{.}"),
            ("ld", r"\ld{1}\par ", r"u{\par }"),
            ("od", r"\od{1};", "u{;}"),
            ("pd", r"\pd<{1}>", "<u{>}"),
            ("ctrlw", r"\ctrlw\foo {1}", r"\foo m"),
            ("ctrls", r"\ctrls\;{1}", r"\;m"),
        ],
    );
}

#[test]
fn delimited_macros_called_anywhere_are_followed() {
    // Tail and non-tail calls, into delimited and undelimited macros, and
    // through `\@ifnextchar` / `\futurelet` dispatch.
    let analysis = analyze_kernel(
        r"\makeatletter
\def\tailU{\tailB}\def\tailB#1#2{}
\def\midcall{\mid@a x\relax}\def\mid@a#1.{}
\def\nontail#1{\ntb\relax}\def\ntb#1;{}
\def\fut{\futurelet\next\futb}\def\futb{\futc}\def\futc#1:{}
\def\st{\@ifstar\sta\stb}\def\sta#1.{}\def\stb#1{}
\let\mystar\@ifstar\def\rs{\mystar\sta\stb}
\def\req[#1]#2{}
\makeatother",
    );
    assert_calls(
        &analysis,
        &[
            ("tailU", r"\tailU{1}{2}", "mm"),
            ("midcall", r"\midcall{1}.", "u{.}"),
            ("nontail", r"\nontail{1}{2};", "mu{;}"),
            ("fut", r"\fut{1}:", "u{:}"),
            ("st", r"\st*?{1}", "sm"),
            ("rs", r"\rs*?{1}", "sm"),
            // Required, not optional: the call without `[` does not match.
            ("req", r"\req[{1}]{2}", "[u{]}m"),
        ],
    );
}

#[test]
fn document_commands_of_every_argument_type() {
    let analysis = analyze_kernel(
        r"\NewDocumentCommand\na{s t+ o O{df} m}{}
\NewDocumentCommand\nb{r() R<>{rr} m}{}
\NewDocumentCommand\nc{d|| D(){dd} m}{}
\NewDocumentCommand\nd{m e{^_}}{}
\NewDocumentCommand\nv{v}{}
\NewDocumentCommand\nh{+m >{\TrimSpaces}m !o}{}
\NewDocumentEnvironment{menv}{o m}{}{}
\newenvironment{oenv}[2][q]{}{}
\DeclareRobustCommand\drc[2][x]{}",
    );
    assert_calls(
        &analysis,
        &[
            ("na", r"\na*?+?[1][df]{2}", "st+oO{df}m"),
            ("nb", r"\nb(1)<rr>{3}", "r()R<>{rr}m"),
            ("nc", r"\nc|1|?(dd)?{3}", "d||D(){dd}m"),
            ("nd", r"\nd{1}^{2}?_{3}?", "me{^}e{_}"),
            ("nv", r"\nv{1}", "m"),
            ("nh", r"\nh{1}{2}[3]", "mmo"),
            ("menv", r"\menv[1]{2}", "om"),
            ("oenv", r"\oenv[q]{1}", "O{q}m"),
            ("drc", r"\drc[x]{1}", "O{x}m"),
        ],
    );
}

#[test]
fn primitives_take_keywords_and_quantities() {
    // The TeXbook, chapter 24: `\hbox to ⟨dimen⟩`, rule specifications,
    // glue with `plus`, a register number.  IniTeX's `{` is other (§ 232).
    let analysis = analyze(r"\catcode`\{=1 \catcode`\}=2 ");
    assert_calls(
        &analysis,
        &[
            ("hbox", r"\hbox [to|spread ⟨dimen⟩]{1}", "[to|spread ⟨dimen⟩]m"),
            // tex.web § 405: the `=` is optional, the number is not.
            ("count", r"\count⟨number⟩ [=]⟨number⟩", "⟨number⟩[=]⟨number⟩"),
            // A keyword a scan gives back (§ 407) is not the command's:
            // no stray `{1}`; `minus` may follow `plus` (§ 461).
            ("hskip", r"\hskip⟨glue⟩ [plus ⟨dimen⟩] [minus ⟨dimen⟩]", "⟨glue⟩[plus ⟨dimen⟩][minus ⟨dimen⟩]"),
            (
                "vrule",
                r"\vrule [width ⟨dimen⟩] [height ⟨dimen⟩] [depth ⟨dimen⟩]",
                "[width ⟨dimen⟩][height ⟨dimen⟩][depth ⟨dimen⟩]",
            ),
            // § 1257: a control sequence, `=`, a name, then `at` or `scaled`.
            ("font", r"\font⟨cs⟩ [=]{1} [at ⟨dimen⟩|scaled ⟨number⟩]", "⟨cs⟩[=]m[at ⟨dimen⟩|scaled ⟨number⟩]"),
            ("read", r"\read⟨number⟩ [to]⟨cs⟩", "⟨number⟩[to]⟨cs⟩"),
        ],
    );
}

#[test]
fn forms_a_lookahead_leads_to() {
    // `#{` leaves the group to `\hbox`, which reads it; a star or a left-out
    // optional argument that leads to other arguments is a form of its own.
    let analysis = analyze_kernel(
        r"\def\brace#1#{\hbox#1}
\makeatletter
\def\nx{\@ifnextchar[\nxa\nxb}\def\nxa[#1]{}\def\nxb#1.{}
\def\st{\@ifstar\sta\stb}\def\sta#1#2{}\def\stb#1{}
\makeatother",
    );
    assert_calls(&analysis, &[("brace", r"\brace{1}{2}", "lm"), ("nx", r"\nx[1]", "o"), ("st", r"\st*?{1}", "sm")]);
    let forms = |name: &str| explain_one(&analysis, name).get("forms").cloned();
    assert_eq!(forms("nx"), Some(json!(["u{.}"])));
    assert_eq!(forms("st"), Some(json!(["smm"])));
}

#[test]
fn an_argument_up_to_a_group_is_followed_by_the_group_it_leaves() {
    // xparse `l` reads up to the `{`, which the `m` after it then reads
    // (pdflatex: `\foo ab{c}` is `[ab|c]`); a `#{` macro leaves it.
    if !installed() {
        return;
    }
    let cfg = crate::support::config();
    let analysis = Machine::analyze(
        "\\documentclass{article}\\usepackage{xparse}\\NewDocumentCommand\\foo{lm}{[#1|#2]}\\def\\a#1#{[#1]}\n\\begin{document}\\end{document}",
        None,
        &cfg,
    );
    assert_eq!(str_field(&explain_one(&analysis, "foo"), "signature"), Some("lm"));
    assert_eq!(str_field(&explain_one(&analysis, "a"), "signature"), Some("l"));
}

// --- A name can mean different things in different contexts of the run:
// the environment stack the kernel tracks when it defined (or called) each
// one, found by running the document, never a table of environment names.

#[test]
fn item_means_differently_outside_and_inside_a_list() {
    if !installed() {
        return;
    }
    let analysis = Machine::analyze(
        r"\documentclass{article}
\begin{document}
\item
\begin{enumerate}
\item hi
\end{enumerate}
\end{document}",
        None,
        &crate::support::config(),
    );
    let (records, _) =
        query::explain(&analysis, &["item".to_string()], false).unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert_eq!(records.len(), 2, "records: {records:?}");
    let outside = records
        .iter()
        .find(|r| str_field(r, "context") == Some("outside environments"))
        .unwrap_or_else(|| panic!("no outside-environments meaning: {records:?}"));
    assert!(
        str_field(outside, "error").is_some_and(|e| e.contains("item")),
        "\\item outside any list should carry the kernel's own error: {outside:?}"
    );
    let inside = records
        .iter()
        .find(|r| str_field(r, "context").is_some_and(|c| c.contains("enumerate")))
        .unwrap_or_else(|| panic!("no in-enumerate meaning: {records:?}"));
    assert!(str_field(inside, "error").is_none(), "\\item inside enumerate should not carry that error: {inside:?}");
}

#[test]
fn item_context_meanings_show_with_no_document_uses_at_all() {
    // The smallest document there is uses `\item` nowhere, so there is
    // nothing to read a real use's context off of; the distinct meanings
    // still show, found by really entering every environment worth trying
    // in a sandbox and calling `\item` there (src/probe.rs).
    if !installed() {
        return;
    }
    let analysis = Machine::analyze(
        "\\documentclass{article}\n\\begin{document}\n\\end{document}\n",
        None,
        &crate::support::config(),
    );
    let (records, _) =
        query::explain(&analysis, &["item".to_string()], false).unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert!(records.len() > 1, "should find more than one meaning by probing: {records:?}");
    // Merged with every other environment the probe found that raises the
    // same error `\item` does with nothing open at all — still starts with
    // "outside environments", since that is always among them.
    let outside = records
        .iter()
        .find(|r| str_field(r, "context").is_some_and(|c| c.starts_with("outside environments")))
        .unwrap_or_else(|| panic!("no outside-environments meaning: {records:?}"));
    assert!(
        str_field(outside, "error").is_some_and(|e| e.contains("item")),
        "\\item outside any list should carry the kernel's own error, probed: {outside:?}"
    );
    // `itemize`, `enumerate` and `description` are all built on `\trivlist`,
    // which sets up what `\item` needs before it is ever called there.
    for name in ["itemize", "enumerate", "description"] {
        let found = records.iter().find(|r| str_field(r, "context").is_some_and(|c| c.contains(name)));
        if let Some(record) = found {
            assert!(str_field(record, "error").is_none(), "\\item in {name} should not error: {record:?}");
        }
    }
}

/// The bare probe hands `.` to `\end`'s argument, so `\end{.}` mismatches
/// whatever is open: an error about the probe's argument, not a meaning.
#[test]
fn end_has_one_meaning_and_no_probed_error() {
    if !installed() {
        return;
    }
    let analysis = Machine::analyze(
        "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n",
        None,
        &crate::support::config(),
    );
    let (records, _) =
        query::explain(&analysis, &["end".to_string()], false).unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(str_field(&records[0], "error").is_none(), "{records:?}");
}

#[test]
fn macro_redefined_inside_a_custom_environment_is_a_distinct_meaning() {
    let analysis = analyze_kernel(
        r"\newcommand{\greet}{hello}
\newenvironment{loud}{\renewcommand{\greet}{HELLO}}{}
\begin{loud}\greet\end{loud}
\greet",
    );
    let (records, _) =
        query::explain(&analysis, &["greet".to_string()], false).unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert_eq!(records.len(), 2, "records: {records:?}");
    let outside = records
        .iter()
        .find(|r| str_field(r, "body") == Some("hello"))
        .unwrap_or_else(|| panic!("no outside meaning: {records:?}"));
    assert_eq!(str_field(outside, "context"), Some("outside environments"));
    let inside = records
        .iter()
        .find(|r| str_field(r, "body") == Some("HELLO"))
        .unwrap_or_else(|| panic!("no inside meaning: {records:?}"));
    assert!(str_field(inside, "context").is_some_and(|c| c.contains("loud")), "{inside:?}");
}

#[test]
fn kernel_local_definition_inside_an_environment_is_a_distinct_meaning() {
    // `\list` (ltlists.dtx) runs `\def\@itemlabel{#1}` itself, once per
    // `\begin{itemize}`/`\begin{enumerate}`/`\begin{description}`: kernel
    // code re-run at the same source site every time (`\@itemlabel` has no
    // meaning of its own outside a list-making environment), with a
    // different label each time it substitutes its own argument in — two
    // real uses here, so two distinct meanings, both reached through the
    // one mechanism (`\list`) that makes them.
    if !installed() {
        return;
    }
    let analysis = Machine::analyze(
        r"\documentclass{article}
\begin{document}
\begin{itemize}
\item x
\end{itemize}
\begin{enumerate}
\item y
\end{enumerate}
\end{document}",
        None,
        &crate::support::config(),
    );
    let (records, _) = query::explain(&analysis, &["@itemlabel".to_string()], false)
        .unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert_eq!(records.len(), 2, "records: {records:?}");
    let bodies: std::collections::HashSet<Option<&str>> = records.iter().map(|r| str_field(r, "body")).collect();
    assert_eq!(bodies.len(), 2, "itemize and enumerate should read \\@itemlabel differently: {records:?}");
    for record in &records {
        let context = str_field(record, "context").unwrap_or_default();
        assert!(
            context.contains("list") || context.contains("itemize") || context.contains("enumerate"),
            "should name the list-making mechanism or an environment that uses it: {record:?}"
        );
    }
}

#[test]
fn identical_redefinitions_in_different_environments_are_one_meaning() {
    let analysis = analyze_kernel(
        r"\newcommand{\shout}{HI}
\newenvironment{loud}{\renewcommand{\shout}{HI}}{}
\begin{loud}\shout\end{loud}",
    );
    let (records, _) =
        query::explain(&analysis, &["shout".to_string()], false).unwrap_or_else(|e| panic!("did not explain: {e}"));
    assert_eq!(records.len(), 1, "same meaning in both places should be one record: {records:?}");
    assert_eq!(str_field(&records[0], "context"), Some("outside environments, in loud"));
}

/// latex.ltx `\let`s `\documentclass` to `\@twoclasseserror` once a class
/// is loaded. The preamble view must show the meaning the call ran with:
/// an optional argument and a mandatory one, not the error's `#1#{`.
#[test]
fn documentclass_explains_the_meaning_it_was_called_with() {
    if !installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(r"\documentclass{article}\begin{document}\end{document}", None, &cfg);
    let record = explain_one(&analysis, "documentclass");
    let body = str_field(&record, "body").unwrap_or_default();
    assert!(!body.contains("Two"), "the call's meaning, not the error it became: {body}");
    assert_eq!(str_field(&record, "effective"), Some(r"\documentclass[1]{2}[3]"), "an optional argument");
}

/// A macro that redefines itself when it runs (`\documentclass`,
/// `\@onlypreamble` lists, one-shot hooks) is marked `once`, from what the
/// run saw its expansion do.
#[test]
fn a_self_redefining_macro_is_marked_once() {
    let analysis = analyze(r"\def\foo{\global\let\foo\relax x}\def\bar{x}\foo\bar");
    assert_eq!(explain_one(&analysis, "foo").get("once"), Some(&json!(true)));
    assert_eq!(explain_one(&analysis, "bar").get("once"), None);
}

#[test]
fn a_list_tex_inserts_charges_nothing_to_the_macro_that_set_it_off() {
    // `\everypar` starts at the paragraph `\twice` begins (tex.web § 1091),
    // and its last token is read after the list has ended (§ 390).
    let analysis = analyze(r"\def\foo{}\def\bar{}\everypar{\foo\bar}\def\twice#1{#1#1}\twice{ab}");
    assert_eq!(explain_one(&analysis, "twice").get("expands"), None);
}

/// A macro no use typed takes the types its body's assignments and
/// scanners give its parameters ([`satex::roles`]).
#[test]
fn a_body_types_the_parameters_it_assigns() {
    let analysis = analyze(
        r"\def\setlen#1#2{#1#2\relax}\def\hsp#1{\hskip#1\relax}\def\setc#1#2{\global\csname c@#1\endcsname#2\relax}\countdef\n=10 \def\setn#1{\n=#1}\def\pair#1#2{#1#2}\def\twice#1{#1#1\relax}",
    );
    let types = |name: &str| explain_one(&analysis, name).get("types").cloned().unwrap_or_default();
    assert_eq!(types("setlen"), json!(["#1 register", "#2 quantity"]));
    assert_eq!(types("hsp"), json!(["#1 glue"]));
    assert_eq!(types("setc"), json!(["#1 name", "#2 quantity"]));
    assert_eq!(types("setn"), json!(["#1 number"]));
    // Text then text is no assignment.
    assert_eq!(types("pair"), Json::Null);
    assert_eq!(types("twice"), Json::Null);
}

/// The kernel's `\setlength` is typed though the document never calls it.
#[test]
fn an_uncalled_kernel_macro_is_typed_by_its_body() {
    let analysis = analyze_kernel(r"\relax");
    let record = explain_one(&analysis, "setlength");
    assert_eq!(record.get("types"), Some(&json!(["#1 register", "#2 quantity"])), "{record:?}");
}

/// Types name each argument by its place in the usage line, optional ones
/// counted: a loading command's options are a key list, its name a file. A
/// command that defines a control sequence reads one, a conditional for
/// `\newif`: `\if⟨name⟩`, typed `conditional`.
#[test]
fn types_count_optional_arguments_and_type_a_control_sequence() {
    if !installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze("\\documentclass{article}\n\\begin{document}\\end{document}", None, &cfg);
    for (name, ty) in [("usepackage", "package (.sty)"), ("documentclass", "class (.cls)")] {
        let records = query::explain(&analysis, &[name.to_string()], false).unwrap().0;
        assert!(
            records.iter().any(|r| r.get("types") == Some(&json!(["#1 options".to_string(), format!("#2 {ty}")]))),
            "{name}: {records:?}"
        );
    }
    let newif = explain_one(&analysis, "newif");
    assert_eq!(newif.get("signature"), Some(&json!("\\if⟨name⟩")), "{newif:?}");
    assert_eq!(newif.get("types"), Some(&json!(["#1 conditional"])), "{newif:?}");
}

/// The name `\newcommand{\x}` defines is read from the first braced group;
/// that group is no numbered argument, so its type stays off the body.
#[test]
fn the_defined_name_types_no_numbered_argument() {
    if !installed() {
        return;
    }
    let analysis = Machine::analyze(
        "\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\\greet{you}\\end{document}",
        None,
        &crate::support::config(),
    );
    let record = explain_one(&analysis, "newcommand");
    let types = record.get("types").cloned().unwrap_or_default().to_string();
    assert!(!types.contains("control sequence"), "{record:?}");
}

#[test]
fn description_for_a_primitive_and_a_kernel_macro() {
    if !installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(r"\documentclass{article}\begin{document}\end{document}", None, &cfg);
    for name in ["endcsname", "section"] {
        let does = str_field(&explain_one(&analysis, name), "does").map(str::to_owned);
        assert!(does.is_some_and(|d| d.len() > 10), "\\{name} lacks a description");
    }
}

#[test]
fn every_builtin_primitive_has_a_description() {
    use satex::config::Engine::{LuaTeX, PdfTeX, Tex, XeTeX};
    let mut missing: Vec<&str> = [Tex, PdfTeX, LuaTeX, XeTeX]
        .into_iter()
        .flat_map(|e| satex::builtins::primitive_names(e).keys().map(String::as_str))
        .filter(|n| satex::query::describe(n).is_none())
        .collect();
    missing.sort_unstable();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "{} primitives lack a line in src/describe.tsv:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn expands_splits_always_from_conditional_arms() {
    // Meanings decide: `\ifx` opens an arm and `\fi` closes it, so `\p` runs
    // whatever the test says and `\q` only in the taken arm.
    let analysis = analyze(r"\def\p{P}\def\q{Q}\def\r{R}\def\a{\p\ifx\p\p\q\else\r\fi\p}\a");
    let record = explain_one(&analysis, "a");
    let names = |field| -> Vec<&str> {
        record.get(field).and_then(Json::as_array).map_or(vec![], |a| a.iter().map(|v| v.as_str().unwrap()).collect())
    };
    assert_eq!(names("expands"), vec![r"\p"]);
    assert_eq!(names("may_expand"), vec![r"\q"]);
}
