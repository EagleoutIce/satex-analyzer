//! LaTeX2e as documented: what the kernel's own interfaces do.

use crate::support::{broken_gaps, codes, marks};
use crate::support::{defined, installed, last_body as body};
use satex::builtins::OccKind;
use satex::machine::{Analysis, Machine};

fn analyze(source: &str) -> Analysis {
    let cfg = crate::support::config();
    Machine::analyze(source, None, &cfg)
}

/// Tests that the last body of `\key` after the document is `want`.
macro_rules! body_cases {
    ($($(#[$doc:meta])* $name:ident($source:literal, $key:literal, $want:literal);)*) => {$(
        $(#[$doc])*
        #[test]
        fn $name() {
            if !installed() {
                return;
            }
            let analysis = analyze($source);
            assert_eq!(body(&analysis, $key), $want);
        }
    )*};
}

fn keys(analysis: &Analysis, kind: OccKind) -> Vec<String> {
    analysis.facts.occurrences.iter().filter(|o| o.kind == kind).map(|o| o.key.clone()).collect()
}

#[test]
fn newcommand_declares_its_arguments_and_default() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newcommand{\plain}[1]{(#1)}
\newcommand{\withdefault}[2][x]{(#1,#2)}
\begin{document}\plain{a}\withdefault{b}\withdefault[c]{d}\end{document}",
    );
    for name in ["plain", "withdefault"] {
        assert!(defined(&analysis, name), "\\{name}");
    }
    let found = codes(&analysis);
    assert!(!found.contains(&"undefined-control-sequence".to_string()), "{found:?}");
}

#[test]
fn renewcommand_needs_an_existing_name_and_newcommand_refuses_one() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newcommand{\thing}{first}
\newcommand{\thing}{second}
\renewcommand{\thing}{third}
\begin{document}\thing\end{document}",
    );
    assert!(codes(&analysis).contains(&"already-defined".to_string()));
}

#[test]
fn newenvironment_defines_both_halves() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newenvironment{box}[1]{start #1}{stop}
\begin{document}\begin{box}{x}body\end{box}\end{document}",
    );
    assert!(defined(&analysis, "box"), "the opening half");
    assert!(defined(&analysis, "endbox"), "the closing half");
}

#[test]
fn a_counter_brings_its_printer_and_its_register() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newcounter{widget}
\setcounter{widget}{4}
\stepcounter{widget}
\begin{document}\edef\seen{\thewidget}\end{document}",
    );
    assert!(defined(&analysis, "c@widget"), "the register");
    assert!(defined(&analysis, "thewidget"), "the printer");
}

body_cases! {
    a_length_can_be_set_and_added_to(r"\documentclass{article}
\newlength{\gap}
\setlength{\gap}{2pt}
\addtolength{\gap}{3pt}
\begin{document}\edef\seen{\the\gap}\end{document}", "seen", "5.0pt");
}

#[test]
fn labels_and_references_are_matched_across_the_document() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\section{One}\label{sec:one}
See \ref{sec:one} and \ref{sec:missing}.
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["sec:one".to_string()]);
    assert_eq!(keys(&analysis, OccKind::Ref), vec!["sec:one".to_string(), "sec:missing".to_string()]);
    assert!(codes(&analysis).contains(&"undefined-reference".to_string()));
}

#[test]
fn a_duplicate_label_is_reported() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\section{One}\label{same}
\section{Two}\label{same}
\ref{same}
\end{document}",
    );
    assert!(codes(&analysis).contains(&"duplicate-label".to_string()));
}

/// An undefined name met on a path that approximated since the paths
/// parted (here a `\write` to a stream satex cannot pin down) is a possible
/// error, as `certain_error` judges the engine's error there, and is
/// reported as a note; one met on a lone path stays TeX's error.
#[test]
fn an_undefined_name_past_an_approximation_is_a_possible_error() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\ifnum\pdfuniformdeviate 2=0 \immediate\write\pdfuniformdeviate 16{x}\undefinedfoo\else b\fi
\undefinedbaz
\end{document}",
    );
    let mut found: Vec<String> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("undefined-control-sequence"))
        .map(|f| {
            let get = |k: &str| f.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            format!("{} {}", get("severity"), get("message"))
        })
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            "error \\undefinedbaz is never defined in this run",
            "info \\undefinedfoo is undefined on a path the analysis approximated"
        ]
    );
}

/// A name read with `{` a letter is TeX's error where the category codes
/// are known, but not where a cut made the analysis assume them back
/// (`region-catcodes`): there it is reported as possible only.
#[test]
fn a_name_read_under_assumed_category_codes_is_no_certain_error() {
    if !installed() {
        return;
    }
    let severity = |spin: &str, line_steps: u64| {
        let source = format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n\\def\\spin{{\\advance\\count2 1 \\ifnum\\count2<5000000 \\expandafter\\spin\\fi}}\n{spin}\n\\catcode`\\{{=11 \\relax\\x{{y \\catcode`\\{{=1\n\\end{{document}}\n"
        );
        let mut cfg = crate::support::config();
        cfg.limits.line_steps = line_steps;
        let analysis = Machine::analyze(&source, None, &cfg);
        satex::lint::lint(&analysis)
            .iter()
            .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("undefined-control-sequence"))
            .map(|f| {
                let get = |k: &str| f.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
                format!("{} {}", get("severity"), get("message"))
            })
            .filter(|f| f.contains("x{y"))
            .collect::<Vec<_>>()
    };
    assert_eq!(severity("", 0), ["error \\x{y is never defined in this run"]);
    let assumed = severity("\\begingroup\\catcode`\\{=11 \\spin\\endgroup", 400_000);
    assert_eq!(assumed, ["warning \\x{y is undefined where the analysis assumed category codes back after a cut"]);
}

/// Only `\newlabel` warns about a name defined twice (pdflatex: "Label
/// `sec:a' multiply defined", a warning): a tikzmark written to the `.aux`
/// file twice is not reported, and LaTeX's warning is no error.
#[test]
fn a_tikzmark_named_twice_is_no_duplicate_label() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{tikz}\usetikzlibrary{tikzmark}
\begin{document}
\section{A}\label{sec:a}
a \tikzmarknode{n-1-left}{x} b \tikzmarknode{n-1-left}{y}
\section{B}\label{sec:a}
\end{document}",
    );
    let found: Vec<(String, String)> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("duplicate-label"))
        .map(|f| {
            let get = |k: &str| f.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            (get("severity"), get("message"))
        })
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "warning");
    assert!(found[0].1.contains("sec:a"), "{found:?}");
}

#[test]
fn package_options_are_recorded_and_a_clash_is_reported() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage[draft]{graphicx}
\usepackage[final]{graphicx}
\begin{document}\end{document}",
    );
    let codes = codes(&analysis);
    assert!(
        codes.contains(&"option-clash".to_string()) || codes.contains(&"duplicate-package".to_string()),
        "{codes:?}"
    );
}

body_cases! {
    a_switch_made_by_newif_decides_a_conditional(r"\documentclass{article}
\newif\ifdraft
\drafttrue
\begin{document}\ifdraft\def\seen{on}\else\def\seen{off}\fi\end{document}", "seen", "on");
    at_begin_document_runs_before_the_body(r"\documentclass{article}
\AtBeginDocument{\def\seen{hook}}
\begin{document}\end{document}", "seen", "hook");
}

#[test]
fn ifstar_and_ifnextchar_take_the_form_that_follows() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\makeatletter
\def\form{\@ifstar{\def\seen{star}}{\def\seen{plain}}}
\def\peek{\@ifnextchar[{\def\other{bracket}}{\def\other{none}}}
\begin{document}\form*\peek[x]\end{document}",
    );
    assert_eq!(body(&analysis, "seen"), "star");
    assert_eq!(body(&analysis, "other"), "bracket");
}

#[test]
fn document_class_options_reach_the_class() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass[12pt,twocolumn]{article}
\begin{document}\end{document}",
    );
    let class = analysis.facts.loads.iter().find(|l| l.name == "article").expect("the class");
    assert!(class.options.contains(&"12pt".to_string()), "{:?}", class.options);
    assert!(class.options.contains(&"twocolumn".to_string()), "{:?}", class.options);
}

#[test]
fn hook_rules_and_the_kind_of_hook_decide_the_order_its_code_runs_in() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\NewHook{probe}
\NewReversedHook{mirror}
\NewMirroredHookPair{pair/begin}{pair/end}
\AddToHook{probe}[one]{\typeout{mark probe-one}}
\AddToHook{probe}[two]{\typeout{mark probe-two}}
\AddToHook{probe}[three]{\typeout{mark probe-three}}
\DeclareHookRule{probe}{three}{before}{one}
\AddToHook{mirror}[a]{\typeout{mark mirror-a}}
\AddToHook{mirror}[b]{\typeout{mark mirror-b}}
\AddToHook{pair/begin}[a]{\typeout{mark begin-a}}
\AddToHook{pair/begin}[b]{\typeout{mark begin-b}}
\AddToHook{pair/end}[a]{\typeout{mark end-a}}
\AddToHook{pair/end}[b]{\typeout{mark end-b}}
\NewHook{void}
\AddToHook{void}[keep]{\typeout{mark void-keep}}
\AddToHook{void}[gone]{\typeout{mark void-gone}}
\DeclareHookRule{void}{keep}{voids}{gone}
\begin{document}
\UseHook{probe}\UseHook{mirror}\UseHook{pair/begin}\UseHook{pair/end}\UseHook{void}
\end{document}",
    );
    // What pdflatex prints for this document, in this order.
    assert_eq!(
        marks(&analysis),
        [
            "mark probe-two",
            "mark probe-three",
            "mark probe-one",
            "mark mirror-b",
            "mark mirror-a",
            "mark begin-a",
            "mark begin-b",
            "mark end-b",
            "mark end-a",
            "mark void-keep",
        ]
    );
}

#[test]
fn the_document_hooks_run_around_the_body_and_what_they_define_survives() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\AddToHook{begindocument/before}{\newcommand{\fromearly}{a}\typeout{mark before}}
\AddToHook{begindocument}{\newcommand{\frombegin}{b}\typeout{mark begin}}
\AddToHook{begindocument/end}{\newcommand{\fromlate}{c}\typeout{mark end}}
\AddToHook{enddocument}{\newcommand{\fromstop}{d}\typeout{mark stop}}
\begin{document}\fromearly\frombegin\fromlate\end{document}",
    );
    assert_eq!(marks(&analysis), ["mark before", "mark begin", "mark end", "mark stop"]);
    // `defined` asks the environment the run ended in, which the document
    // group has already unwound; what the hook defined is in the facts.
    for (name, text) in [("fromearly", "a"), ("frombegin", "b"), ("fromlate", "c"), ("fromstop", "d")] {
        assert_eq!(body(&analysis, name), text, "\\{name}");
    }
    let found = codes(&analysis);
    assert!(!found.contains(&"undefined-control-sequence".to_string()), "{found:?}");
}

#[test]
fn a_one_time_hook_runs_code_added_after_it_has_fired() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\AddToHook{begindocument}{\newcommand{\late}{x}\typeout{mark late}}
\late
\end{document}",
    );
    assert_eq!(marks(&analysis), ["mark late"]);
    assert_eq!(body(&analysis, "late"), "x");
    let found = codes(&analysis);
    assert!(!found.contains(&"undefined-control-sequence".to_string()), "{found:?}");
}

#[test]
fn the_environment_hooks_fire_around_the_environment_and_its_group() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\AddToHook{env/quote/before}{\newcommand{\outside}{o}\typeout{mark before}}
\AddToHook{env/quote/begin}{\typeout{mark begin}}
\AddToHook{env/quote/end}{\typeout{mark end}}
\AddToHook{env/quote/after}{\newcommand{\later}{l}\typeout{mark after}}
\begin{document}\begin{quote}x\end{quote}\outside\later\end{document}",
    );
    assert_eq!(marks(&analysis), ["mark before", "mark begin", "mark end", "mark after"]);
    // `env/…/before` and `env/…/after` run outside the environment's group,
    // so what they define is still there afterwards.
    for (name, text) in [("outside", "o"), ("later", "l")] {
        assert_eq!(body(&analysis, name), text, "\\{name}");
    }
    let found = codes(&analysis);
    assert!(!found.contains(&"undefined-control-sequence".to_string()), "{found:?}");
}

#[test]
fn the_file_and_package_hooks_fire_around_a_load() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\AddToHook{package/color/before}{\typeout{mark package-before}}
\AddToHook{file/color.sty/before}{\typeout{mark file-before}}
\AddToHook{file/color.sty/after}{\typeout{mark file-after}}
\AddToHook{package/color/after}{\newcommand{\afterload}{a}\typeout{mark package-after}}
\usepackage{color}
\begin{document}\afterload\end{document}",
    );
    assert_eq!(marks(&analysis), ["mark package-before", "mark file-before", "mark file-after", "mark package-after"]);
    assert!(defined(&analysis, "afterload"));
}

#[test]
fn a_command_hook_wraps_the_command_it_names() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newcommand{\greet}{\typeout{mark body}}
\AddToHook{cmd/greet/before}{\newcommand{\fromcmd}{c}\typeout{mark before}}
\AddToHook{cmd/greet/after}{\typeout{mark after}}
\begin{document}\greet\fromcmd\end{document}",
    );
    assert_eq!(marks(&analysis), ["mark before", "mark body", "mark after"]);
    assert_eq!(body(&analysis, "fromcmd"), "c");
}

#[test]
fn add_to_hook_next_runs_last_and_only_once() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\NewHook{once}
\AddToHook{once}[a]{\typeout{mark regular}}
\AddToHookNext{once}{\newcommand{\fromnext}{n}\typeout{mark next}}
\begin{document}\UseHook{once}\UseHook{once}\fromnext\end{document}",
    );
    // pdflatex prints `mark regular`, `mark next`, `mark regular`.
    assert_eq!(marks(&analysis), ["mark regular", "mark next", "mark regular"]);
    assert_eq!(body(&analysis, "fromnext"), "n");
}

#[test]
fn removing_a_label_takes_its_code_out_of_the_hook() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\NewHook{probe}
\AddToHook{probe}[gone]{\typeout{mark gone}}
\AddToHook{probe}[kept]{\typeout{mark kept}}
\RemoveFromHook{probe}[gone]
\begin{document}\UseHook{probe}\end{document}",
    );
    assert_eq!(marks(&analysis), ["mark kept"]);
}

#[test]
fn the_document_body_is_not_bounded_by_the_argument_budget() {
    if !installed() {
        return;
    }
    // Comfortably past `limits.expansion_tokens`, which bounds an argument an
    // expansion fills and has no say over source read from the file.
    let filler = "word ".repeat(300_000);
    let analysis = analyze(&format!(
        r"\documentclass{{article}}
\begin{{filecontents}}{{generated.txt}}
{filler}
\end{{filecontents}}
\newcommand{{\afterbody}}{{x}}
\begin{{document}}\afterbody\end{{document}}"
    ));
    assert!(defined(&analysis, "afterbody"), "the body did not cut the run short");
    let found = codes(&analysis);
    assert!(!found.contains(&"unterminated-verbatim".to_string()), "{found:?}");
}

/// The document properties, as `satex summary` shows them: what hyperref's
/// `\pdfstringdef` makes of `\title`, `\author` and `\date`.  Every
/// expectation below is what pdflatex wrote for the same source.
fn metadata(analysis: &Analysis, field: &str) -> String {
    analysis.metadata.iter().find(|entry| entry.field == field).map(|entry| entry.text.clone()).unwrap_or_default()
}

fn metadata_source(analysis: &Analysis, field: &str) -> String {
    analysis.metadata.iter().find(|entry| entry.field == field).map(|entry| entry.source.clone()).unwrap_or_default()
}

#[test]
fn the_title_spells_a_logo_the_way_the_pdf_does() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{hyperref}
\begin{document}
\def\DocName{Document}
\title{A Sample \DocName{} for sa\TeX}
\author{satex Test Suite}
\maketitle
\end{document}",
    );
    assert_eq!(metadata(&analysis, "title"), "A Sample Document for saTeX");
    assert_eq!(metadata(&analysis, "author"), "satex Test Suite");
    // The rendering drops from the token list, so the list itself is kept.
    assert!(metadata_source(&analysis, "title").contains(r"\TeX"));
}

/// Documents and the metadata fields they record, with the text of each.
const METADATA: &[crate::support::BodyRow] = &[
    // `\pdfstringdef` expands what it can, robust commands included; a name
    // with an `@` in it is a macro like any other, so its text stands.
    (
        "an_internal_command_in_the_date_expands_like_any_macro",
        r"\documentclass{article}
\makeatletter
\protected\def\my@mark{M}
\date{September\my@mark 22, 2026}
\makeatother
\begin{document}
\title{T}\author{A}\maketitle
\end{document}",
        &[("date", "SeptemberM22, 2026")],
    ),
    (
        "texorpdfstring_gives_the_title_its_pdf_form",
        r"\documentclass{article}
\usepackage{hyperref}
\begin{document}
\title{The \texorpdfstring{$x^2$}{x squared} rule}\author{A}\maketitle
\end{document}",
        &[("title", "The x squared rule")],
    ),
    (
        "math_in_a_title_keeps_only_what_is_text",
        r"\documentclass{article}
\begin{document}
\title{The $\alpha$ Method}\author{A}\maketitle
\end{document}",
        &[("title", "The Method")],
    ),
    (
        "a_title_uses_the_documents_own_macro",
        r"\documentclass{article}
\newcommand{\proj}{Satex}
\begin{document}
\title{The \proj{} analyzer}\author{A}\maketitle
\end{document}",
        &[("title", "The Satex analyzer")],
    ),
    (
        "a_kern_and_a_penalty_leave_the_title_with_their_values",
        r"\documentclass{article}
\begin{document}
\title{X\kern2pt Y\penalty100 Z\hskip 3pt plus 1fil W\,V}\author{A}\maketitle
\end{document}",
        &[("title", "XYZWV")],
    ),
];

#[test]
fn metadata_is_recorded() {
    if !installed() {
        return;
    }
    crate::support::table(
        METADATA,
        |r| r.0,
        |&(_, source, fields)| {
            let analysis = analyze(source);
            let wrong: Vec<String> = fields
                .iter()
                .filter_map(|&(field, want)| {
                    let got = metadata(&analysis, field);
                    (got != want).then(|| format!("{field} is {got:?}, expected {want:?}"))
                })
                .collect();
            (!wrong.is_empty()).then(|| wrong.join("; "))
        },
    );
}

#[test]
fn the_sample_document_keeps_what_it_already_analyzed() {
    // A regression net for tests/fixtures/project/paper.tex: these are things satex reads
    // correctly today, and a change that loses one of them is a regression
    // even when every other test still passes.
    if !installed() {
        return;
    }
    let source = std::fs::read_to_string("tests/fixtures/project/paper.tex").expect("the sample document");
    let analysis = Machine::analyze(
        &source,
        Some(std::path::Path::new("tests/fixtures/project/paper.tex")),
        &crate::support::config(),
    );
    let defined = |name: &str| {
        analysis
            .interner
            .lookup(name)
            .is_some_and(|sym| !matches!(analysis.env.meaning(sym), satex::tex::Meaning::Undefined))
    };
    // Kernel and class commands the document uses.
    for name in ["textbf", "emph", "section", "label", "ref", "item", "title", "author", "caption"] {
        assert!(defined(name), "\\{name} must be known");
    }
    // Every label the document writes is recorded.
    let labels: Vec<&str> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == satex::builtins::OccKind::Label)
        .map(|o| o.key.as_str())
        .collect();
    for key in ["sec:intro", "sec:method", "fig:main", "tab:results"] {
        assert!(labels.contains(&key), "\\label{{{key}}} must be recorded, found {labels:?}");
    }
    // And the document's own definitions are followed.
    for name in ["highlight", "norm", "DocName"] {
        assert!(defined(name), "\\{name} must be known");
    }
    // A document that compiles has nothing undefined and no dangling
    // reference: both were false findings once, so both are pinned.
    let findings = satex::lint::lint(&analysis);
    let code_of =
        |record: &satex::query::Record| record.get("code").and_then(|c| c.as_str()).unwrap_or_default().to_string();
    for code in ["undefined-control-sequence", "undefined-reference"] {
        let from_document = |record: &satex::query::Record| {
            record.get("file").and_then(|f| f.as_str()).unwrap_or_default() == "paper.tex"
        };
        let raised: Vec<String> = findings
            .iter()
            .filter(|record| code_of(record) == code && from_document(record))
            .map(|record| record.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string())
            .collect();
        assert!(raised.is_empty(), "tests/fixtures/project/paper.tex must raise no {code}: {raised:?}");
    }
}

/// Severities of the `raised-error` findings in the document.
fn raised(analysis: &Analysis) -> Vec<String> {
    satex::lint::lint(analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("raised-error"))
        .filter_map(|f| f.get("severity").and_then(|s| s.as_str()).map(str::to_string))
        .collect()
}

/// An error any code raises reaches the lints, as pdflatex stops on it: the
/// kernel's ltcmd refuses an xparse-only argument type and a misplaced `!`,
/// and leaves the command undefined.
#[test]
fn an_error_the_run_raises_is_reported() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\NewDocumentCommand\fa{u{;}}{[#1]}
\NewDocumentCommand\fc{+m !o m}{[#1|#2|#3]}
\NewDocumentCommand\fd{m}{[#1]}
\begin{document}\fd{x}\end{document}",
    );
    assert_eq!(raised(&analysis), ["error", "error"]);
    for (name, defined) in [("fa", false), ("fc", false), ("fd", true)] {
        let sym = analysis.interner.lookup(name).expect("interned");
        assert_eq!(analysis.env.is_defined(sym), defined, "\\{name}");
    }
    // One the kernel explains itself goes to its own rule.
    let analysis = analyze(r"\documentclass{article}\begin{document}\begin{nosuchenv}\end{nosuchenv}\end{document}");
    assert!(raised(&analysis).is_empty());
    assert!(codes(&analysis).contains(&"undefined-environment".to_string()));
}

/// hyperref's `\Hy@SaveLastskip` tests `\lastskip`, which satex cannot know,
/// so a theorem's anchor runs a command of unknown meaning and the mode
/// widens: what the kernel then raises is not certain (pdflatex raises
/// nothing here).
#[test]
fn an_error_raised_after_the_mode_widened_is_no_certain_error() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{amsthm}\usepackage{hyperref}\usepackage{cleveref}\newtheorem{thm}{Theorem}
\begin{document}
\begin{thm}x\end{thm}\begin{itemize}\item a\end{itemize}
\end{document}",
    );
    assert!(raised(&analysis).iter().all(|s| s != "error"), "{:?}", raised(&analysis));
}

/// A name one call defines as another's implementation (ltcmd's `\fd code`,
/// `\newcommand`'s `\\fe` for an optional argument) is not reported as an
/// unused definition of its own: the name the document wrote is.
#[test]
fn an_unused_command_is_reported_once_not_with_its_implementation() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\NewDocumentCommand\fd{m}{[#1]}
\newcommand\fe[1][x]{#1}
\begin{document}x\end{document}",
    );
    let mut names: Vec<String> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("unused-definition"))
        .filter(|f| f.get("origin").and_then(|o| o.as_str()) == Some("document"))
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect();
    names.sort();
    assert_eq!(names, [r"\fd", r"\fe"]);
}

/// A definition is used when anything that reads it is: a `\let` copy that
/// is expanded, a `\csname` dispatch table, a token list a package keeps
/// (`\lstset` literate).  Names package code builds for the document
/// (`\coordinate`'s `\pgf@sh@…`, `\includegraphics`' `\ver@…`) are not the
/// document's definitions.  What nothing reads, even through a caller that
/// is itself unused, still is reported.
#[test]
fn a_definition_read_through_an_alias_or_a_package_is_used() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{listings,tikz,graphicx}
\newcommand\viaLet{x}\let\alias\viaLet
\newcommand\viaTable{A}\expandafter\let\csname table@a\endcsname\viaTable
\newcommand\viaLiterate{$\leftarrow$}\lstset{literate={<-}{{\viaLiterate}}2}
\newcommand\callee{a}\newcommand\caller{\callee}
\newcommand\lonely{b}\newcommand\lonelier{\lonely}
\begin{document}
\alias \csname table@a\endcsname \caller
\begin{tikzpicture}\coordinate (a) at (0,0);\draw (a) -- (1,1);\end{tikzpicture}
\includegraphics{example-image}
\end{document}",
    );
    let mut names: Vec<String> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| matches!(f.get("code").and_then(|c| c.as_str()), Some("unused-definition" | "dead-definition")))
        .filter(|f| f.get("origin").and_then(|o| o.as_str()) == Some("document"))
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect();
    names.sort();
    assert_eq!(names, [r"\lonelier", r"\lonely"]);
}

/// An overhang is between characters TeX set next to each other, not ones
/// the source lists next to each other: a `\title` set upright in the
/// heading and in italics elsewhere is two runs, each ended where LaTeX's
/// `\textit` adds `\/` itself.
#[test]
fn italic_correction_follows_the_typeset_order() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\title{Analyzing Code}\author{x}
\begin{document}
\maketitle
{\itshape f}(x) and \textit{f}(y) and {\itshape f\/}(z).
\makeatletter\textit{\@title}\makeatother
\end{document}",
    );
    let places: Vec<(u64, u64)> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("missing-italic-correction"))
        .map(|f| (f["line"].as_u64().unwrap_or(0), f["col"].as_u64().unwrap_or(0)))
        .collect();
    assert_eq!(places, [(5, 11)]);
}

/// versions.sty excludes an environment by skipping the document's text
/// with `\iffalse` until `\end`, which it lets to `\fi`.  With no gap
/// before it the skip is TeX's own and the run reads on after the
/// environment, as pdflatex does.
#[test]
fn an_excluded_version_is_skipped_and_the_document_goes_on() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{versions}
\excludeversion{showenv}
\newenvironment{corr}{\showenv\fbox\bgroup}{\endshowenv\egroup}
\begin{document}
\begin{corr}
Hidden \textbf{text} here.
\end{corr}
\def\after{}
\end{document}",
    );
    assert!(defined(&analysis, "after"));
    let gaps: Vec<_> =
        analysis.facts.diagnostics.iter().filter(|d| d.severity == satex::facts::Severity::Unsupported).collect();
    assert!(gaps.is_empty(), "{:?}", gaps.iter().map(|d| d.code).collect::<Vec<_>>());
}

#[test]
fn verbatim_bodies_are_read_line_by_line_with_an_active_end_of_line() {
    // pdflatex: `\FV@SV@V` holds `{a  b}` and `{ \c%}` (trailing spaces
    // dropped), `\r` is `a^^Mb`, and nothing inside a verbatim body is run.
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\usepackage{fancyvrb,listings}\n\\begin{document}\n\
\\begin{SaveVerbatim}{V}\na  b   \n \\c%\n\\end{SaveVerbatim}\n\
\\begin{verbatim}\n\\def\\bad{}\n\\end{verbatim}\n\\begin{lstlisting}\n\\def\\bad{}   \n\\end{lstlisting}\n\
\\begin{Verbatim}\n\\def\\bad{}\n\\end{Verbatim}\n{\\obeylines\\gdef\\r{a\nb}}\n\\def\\after{}\n\\end{document}\n",
    );
    let saved = body(&analysis, "FV@SV@V");
    assert!(
        saved.ends_with("\\FV@ProcessLine {a  b}\\advance \\c@FancyVerbLine \\@ne \\FV@ProcessLine { \\c%}"),
        "{saved}"
    );
    // tex.web § 49: the active end of line prints as `^^M`, not raw.
    assert_eq!(body(&analysis, "r"), "a^^Mb");
    assert!(!defined(&analysis, "bad"));
    assert!(defined(&analysis, "after"));
}

/// A box's size is its list's natural size: characters with their kerns and
/// ligatures, interword glue by the space factor, and the last items
/// `\lastskip` and `\unskip` see.  The expectations are what pdflatex
/// printed for the same source.
#[test]
fn a_box_has_its_natural_size() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\setbox0\hbox{x}\typeout{mark \the\wd0}
\setbox0\hbox{office}\typeout{mark \the\wd0}
\setbox0\hbox{AV Wa}\typeout{mark \the\wd0}
\setbox0\hbox{\texttt{paper.tex}}\typeout{mark \the\wd0}
\setbox0\hbox{\sffamily\scriptsize f\relax i}\typeout{mark \the\wd0}
\setbox0\hbox{``--''}\typeout{mark \the\wd0}
\setbox0\hbox{a~\texttt{b}}\typeout{mark \the\wd0}
\setbox0\hbox{a, \global\skip1=\lastskip}\typeout{mark \the\skip1}
\setbox0\hbox{a \unskip b\kern2pt\unkern\penalty5\global\count1=\lastpenalty}\typeout{mark \the\wd0,\the\count1}
\setbox0\hbox{\kern1pt\special{x}\vrule width 2pt height 3pt}\typeout{mark \the\wd0,\the\ht0}
\end{document}",
    );
    assert_eq!(
        marks(&analysis),
        [
            "mark 5.2778pt",
            "mark 22.22226pt",
            "mark 31.6667pt",
            "mark 47.24959pt",
            "mark 4.04692pt",
            "mark 15.00005pt",
            "mark 13.5833pt",
            "mark 3.33333pt plus 2.08331pt minus 0.88889pt",
            "mark 10.55559pt,5",
            "mark 3.0pt,3.0pt",
        ]
    );
}

/// The tail of an inline formula's list, and of the list after it: a
/// formula ends in a math node whatever it holds (tex.web § 1196), so
/// `\lastskip` is zero after one that took `\unskip`, as algorithm2e's
/// `\DataSty` does; in the formula, `\lastkern` sees a kern only while it
/// is last (§ 424). The expectations are what pdflatex printed for the same
/// source.
#[test]
fn an_inline_formula_list_tail_is_followed() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\def\after#1{\setbox0\hbox{a #1\global\skip1=\lastskip\global\count1=\lastpenalty\global\dimen1=\lastkern}%
\typeout{mark \the\skip1,\the\count1,\the\dimen1}}
\def\last#1{\setbox0\hbox{a $x\kern2pt#1$}\typeout{mark \the\dimen1}}
\def\width#1{\setbox0\hbox{#1}\typeout{mark \the\wd0}}
\after{$x\unskip$}\after{$\mbox{x}\unskip$}\after{$x\unpenalty$}\after{$x\hskip2pt\unskip$}
\last{\global\dimen1=\lastkern}\last{\relax\global\dimen1=\lastkern}\last{{}\global\dimen1=\lastkern}
\last{\textstyle\global\dimen1=\lastkern}\last{{\global\dimen1=\lastkern}}
\last{\mathchoice{\global\dimen1=\lastkern}{\global\dimen1=\lastkern}{}{}}
\last{\mathchoice{}{}{}{}\global\dimen1=\lastkern}\last{\special{}\global\dimen1=\lastkern}
\width{$x\unskip y$}\width{$\mbox{a}\unskip y$}
\end{document}",
    );
    assert_eq!(
        marks(&analysis),
        [
            "mark 0.0pt,0,0.0pt",
            "mark 0.0pt,0,0.0pt",
            "mark 0.0pt,0,0.0pt",
            "mark 0.0pt,0,0.0pt",
            "mark 2.0pt",
            "mark 2.0pt",
            "mark 0.0pt",
            "mark 0.0pt",
            "mark 0.0pt",
            "mark 0.0pt",
            "mark 0.0pt",
            "mark 0.0pt",
            "mark 10.97687pt",
            "mark 10.26161pt",
        ]
    );
}

/// A kern `\unkern` takes from inline math loses the formula's size, which
/// pdflatex gives as 10.97687pt, but not what ends the formula.
#[test]
fn unkern_in_inline_math_widens_its_size() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\setbox0\hbox{a $x\kern2pt\unkern y$\global\skip1=\lastskip}
\ifdim\wd0=10.97687pt \def\exact{}\fi
\ifdim\skip1=0pt \def\zero{}\else\def\nonzero{}\fi
\end{document}",
    );
    assert!(defined(&analysis, "exact"));
    assert!(defined(&analysis, "zero"));
    assert!(!defined(&analysis, "nonzero"));
}

/// An inline formula's size with scripts: a braced one-character script, a
/// box nucleus (tex.web § 756 uses the box's height and depth), and a
/// `\color` whatsit inside the script. The expectations are what pdflatex
/// printed for the same source.
#[test]
fn math_braced_script_box_nucleus_size() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{xcolor}
\begin{document}
\def\m#1{\setbox0\hbox{#1}\typeout{mark \the\wd0,\the\ht0,\the\dp0}}
\m{\(x^{0}\)}\m{\(\textrm{cellR}^0\)}\m{\(x^{\color{gray}0}\)}\m{\(\textrm{cellR}^{\color{gray}0}\)}
\m{\(\textrm{cellR}_0\)}\m{\(x_{0}\)}\m{\(x^{0}y\)}\m{\(x^0y\)}
\end{document}",
    );
    assert_eq!(
        marks(&analysis),
        [
            "mark 10.2014pt,8.14003pt,0.0pt",
            "mark 26.2917pt,8.98335pt,0.0pt",
            "mark 10.2014pt,8.14003pt,0.0pt",
            "mark 26.2917pt,8.98335pt,0.0pt",
            "mark 26.2917pt,6.94444pt,1.49998pt",
            "mark 10.2014pt,4.30554pt,1.49998pt",
            "mark 15.463pt,8.14003pt,1.94444pt",
            "mark 15.463pt,8.14003pt,1.94444pt",
        ]
    );
}

/// A variant expl3 generates is `\protected` only when its base is: the
/// kernel tells them apart with `\exp_not:N` and `\if_meaning:w`, and the
/// class a package like xkeyval finds in `\@filelist` depends on it.
#[test]
fn a_generated_variant_is_protected_like_its_base() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{xkeyval}
\ExplSyntaxOn
\cs_new:Npn \my_foo:nN #1#2 {#1}
\cs_generate_variant:Nn \my_foo:nN {o}
\edef\R{\meaning\my_foo:oN}
\ExplSyntaxOff
\begin{document}\end{document}",
    );
    assert_eq!(body(&analysis, "R").replace(' ', ""), r"\long macro:->\exp_args:No\my_foo:nN".replace(' ', ""));
    let found = codes(&analysis);
    assert!(!found.contains(&"raised-error".to_string()), "{found:?}");
}

/// The bisection behind [`aftergroup_from_an_expl3_alias_ends_the_group`]:
/// `body` is how the conditional returns, `branch` what its true branch
/// does.  `\l_my_log_tl` records every item the map sees, `+`/`-` for the
/// branch taken and `T`/`F` for the boolean the true branch set, so the text
/// it ends with says how far the loop ran.
fn map_break_document(body: &str, branch: &str) -> String {
    format!(
        r"\documentclass{{article}}
\ExplSyntaxOn
\seq_new:N \l_my_seq
\seq_put_right:Nn \l_my_seq {{LF}}
\seq_put_right:Nn \l_my_seq {{OsF}}
\tl_new:N \l_my_log_tl
\prg_new_protected_conditional:Nnn \my_exists:n {{ TF }}
  {{ {body} }}
\ExplSyntaxOff
\begin{{document}}
\ExplSyntaxOn
\seq_map_inline:Nn \l_my_seq
  {{
    \tl_put_right:Nn \l_my_log_tl {{#1}}
    \my_exists:nTF {{#1}}
      {{ \bool_set_true:N \l_tmpa_bool \tl_put_right:Nn \l_my_log_tl {{+}} {branch} }}
      {{ \tl_put_right:Nn \l_my_log_tl {{-}} }}
  }}
\bool_if:NTF \l_tmpa_bool {{ \tl_put_right:Nn \l_my_log_tl {{T}} }} {{ \tl_put_right:Nn \l_my_log_tl {{F}} }}
\ExplSyntaxOff
x
\end{{document}}"
    )
}

/// Every `Unsupported` diagnostic the run recorded, as `code at file:line`.
fn unsupported(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::facts::Severity::Unsupported)
        .map(|d| {
            let file = analysis.files.get(d.span.file as usize).map(|f| f.path.as_str()).unwrap_or("?");
            format!("{} at {file}:{}", d.code, d.span.line)
        })
        .collect()
}

/// The last replacement text `name` was given; [`body`] takes the first.
fn last_body(analysis: &Analysis, name: &str) -> String {
    analysis
        .facts
        .defs
        .iter()
        .rev()
        .find(|d| analysis.interner.name(d.name) == name && d.mac.is_some())
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

/// One row of the bisection: no gap at all, and the log text `pdflatex`
/// leaves behind.
fn map_break_row(body: &str, branch: &str, log: &str) {
    let analysis = analyze(&map_break_document(body, branch));
    assert_eq!(unsupported(&analysis), Vec::<String>::new(), "body {body:?} branch {branch:?}");
    assert_eq!(last_body(&analysis, "l_my_log_tl"), log, "body {body:?} branch {branch:?}");
}

/// The defect this pins: a `\prg_new_protected_conditional:` whose body
/// returns through `\group_insert_after:N \prg_return_true:` hands the
/// return token to the end of the group, and the `\use_i:nn` it expands to
/// reads the two branches from the input right after that group.  Reaching
/// `\aftergroup` through an expl3 copy used to hold the token back for the
/// next assignment instead, which left `\exp_end:` to start a paragraph and
/// the branches to be read as groups of their own.  `pdflatex`: `LF+T`.
#[test]
fn aftergroup_from_an_expl3_alias_ends_the_group() {
    if !installed() {
        return;
    }
    map_break_row(r"\group_begin: \group_insert_after:N \prg_return_true: \group_end:", r"\seq_map_break:", "LF+T");
}

/// The same through `\tex_aftergroup:D`, the other name expl3 reaches it by.
#[test]
fn aftergroup_from_tex_aftergroup_ends_the_group() {
    if !installed() {
        return;
    }
    map_break_row(r"\group_begin: \tex_aftergroup:D \prg_return_true: \group_end:", r"\seq_map_break:", "LF+T");
}

/// The delivery on its own, with no delimited `\seq_map_break:` scan behind
/// it: both items are seen and both take the true branch.  `pdflatex`:
/// `LF+OsF+T`.
#[test]
fn aftergroup_delivery_without_a_map_break_runs_the_whole_loop() {
    if !installed() {
        return;
    }
    map_break_row(r"\group_begin: \group_insert_after:N \prg_return_true: \group_end:", r"\typeout{T}", "LF+OsF+T");
}

/// The control rows of the bisection: a return with no group at all, and one
/// after an empty group.  Both were clean before the fix and stay clean.
#[test]
fn a_conditional_that_returns_without_aftergroup_is_clean() {
    if !installed() {
        return;
    }
    map_break_row(r"\prg_return_true:", r"\seq_map_break:", "LF+T");
    map_break_row(r"\group_begin: \group_end: \prg_return_true:", r"\seq_map_break:", "LF+T");
}

/// figureversions returns from `\figureversion` through the same idiom, and
/// it is where the defect showed on real documents.
#[test]
fn figureversions_selects_a_figure_style_without_a_gap() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{figureversions}
\begin{document}
\figureversion{lining}x
\end{document}",
    );
    assert_eq!(unsupported(&analysis), Vec::<String>::new());
}

/// Every floating point value ends in `\__fp_sep:`, and `\__fp_sanitize:Nw`
/// decides with an `\ifcase` whose number three conditionals build and close
/// only after it.  Reading those `\fi` as the end of the `\ifcase`
/// (tex.web § 500) skipped the sanitizing arm, so a zero came back as `nan`
/// and the value lost its terminator — after which every delimited scan for
/// `\__fp_sep:` ran into the `}` of `\exp_not:f`.  `pdflatex`:
/// `[3][0][0][0][1][1e9999][1e-9999][0][0]`.
#[test]
fn a_floating_point_zero_is_sanitized_and_keeps_its_terminator() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\fp_new:N \l_my_fp
\tl_new:N \l_my_tl
\cs_new_protected:Npn \myshow:n #1
  {
    \fp_set:Nn \l_my_fp {#1}
    \tl_put_right:Ne \l_my_tl { [ \fp_to_tl:N \l_my_fp ] }
  }
\ExplSyntaxOff
\begin{document}
\ExplSyntaxOn
\myshow:n { 1 + 2 }
\myshow:n { 0 }
\myshow:n { 1 - 1 }
\myshow:n { 0.0 }
\myshow:n { \c_one_fp }
\myshow:n { 1e9999 }
\myshow:n { 1e-9999 }
\myshow:n { 2 * 0 }
\myshow:n { round(0.4) }
\ExplSyntaxOff
x
\end{document}",
    );
    assert_eq!(unsupported(&analysis), Vec::<String>::new());
    assert_eq!(last_body(&analysis, "l_my_tl"), "[3][0][0][0][1][1e9999][1e-9999][0][0]");
}

/// siunitx compares each of its unit symbols against the engine's output at
/// `\begin{document}`, which sets a floating point multiplier for every one:
/// the five lines below were the smallest document that showed the defect.
#[test]
fn siunitx_declares_its_units_without_a_gap() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{siunitx}
\begin{document}
x
\end{document}",
    );
    assert_eq!(unsupported(&analysis), Vec::<String>::new());
}

/// `\DocumentMetadata` is the entry point of the PDF-management and tagging
/// code, so it is in every new document.  It loads pdfmanagement, tagpdf and
/// the latex-lab testphase files, and satex has to come out the other side
/// with a bounded run and a bounded report instead of one gap counted into
/// the millions.
mod document_metadata {
    use satex::config::Config;
    use satex::facts::Severity;
    use satex::machine::{Analysis, Machine};

    /// The bound one gap at one place is counted to.
    fn bound() -> u32 {
        Config::default().limits.repeat_failures
    }

    fn analyze_with(options: &str, cfg: &Config) -> Analysis {
        let source = format!(
            "\\DocumentMetadata{{{options}}}\n\\documentclass{{article}}\n\\begin{{document}}\nx\n\\end{{document}}\n"
        );
        Machine::analyze(&source, None, cfg)
    }

    fn analyze_document(options: &str) -> Analysis {
        analyze_with(options, &crate::support::config())
    }

    fn unsupported(analysis: &Analysis) -> Vec<(&str, u32)> {
        analysis
            .facts
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Unsupported)
            .map(|d| (d.code, d.count))
            .collect()
    }

    /// No gap at one place may be reported more than `repeat_failures`
    /// times, and the run may not end in `budget-exhausted`.  The document
    /// still leaves gaps, so the codes are not asserted away; what is
    /// asserted is that each is bounded and the run finished.
    fn stays_bounded(options: &str) {
        let analysis = analyze_document(options);
        let bound = bound();
        let over: Vec<_> = unsupported(&analysis).into_iter().filter(|&(_, n)| n > bound).collect();
        assert_eq!(over, Vec::new(), "gaps past the bound of {bound} for `{options}`");
        let exhausted: Vec<_> =
            unsupported(&analysis).into_iter().filter(|&(code, _)| code == "budget-exhausted").collect();
        let why: Vec<&str> = analysis
            .facts
            .diagnostics
            .iter()
            .filter(|d| d.code == "budget-exhausted" || d.code == "limit-reached")
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(exhausted, Vec::new(), "the run ran out of budget on `{options}`: {why:?}");
        assert!(!analysis.exhausted, "the run was exhausted on `{options}`");
        // Measured: 20 million tokens with the bound, 240 million without,
        // out of the 900 million the default budget allows.
        assert!(analysis.steps < BOUNDED_STEPS, "{} steps for `{options}`", analysis.steps);
    }

    /// Well above what the bounded run needs and well below what the
    /// unbounded one took.
    const BOUNDED_STEPS: u64 = 50_000_000;

    /// tikz, graphicx and hyperref together (epstopdf's `\InputIfFileExists`
    /// loop, pgfmath's dimension comparisons) end without running out of budget.
    #[test]
    fn a_tikz_hyperref_document_does_not_exhaust_the_budget() {
        if !super::installed() {
            return;
        }
        let source = "\\documentclass[9pt]{article}\n\\usepackage{tikz,graphicx,etoolbox}\n\\usepackage{hyperref}\n\\usetikzlibrary{decorations.pathmorphing,shadows,arrows.meta}\n\\begin{document}\n\\section{One}\\label{sec:one}\n\\begin{tikzpicture}\\node[draw,circle] (a) {x};\\node[draw] (b) at (2,1) {y};\\draw[->] (a) -- (b);\\end{tikzpicture}\n\\end{document}\n";
        let analysis = Machine::analyze(source, None, &crate::support::config());
        let exhausted: Vec<_> =
            unsupported(&analysis).into_iter().filter(|&(code, _)| code == "budget-exhausted").collect();
        assert_eq!(exhausted, Vec::new());
        assert!(!analysis.exhausted);
    }

    #[test]
    fn the_bare_five_line_document_stays_bounded() {
        if !super::installed() {
            return;
        }
        stays_bounded("lang=en");
    }

    /// The options real documents carry: `pdfversion` picks the PDF the
    /// backend writes, `testphase` loads the latex-lab code, `tagging`
    /// turns the structure tree on.
    #[test]
    fn the_common_options_stay_bounded() {
        if !super::installed() {
            return;
        }
        stays_bounded("pdfversion=2.0");
        stays_bounded("lang=en,pdfversion=1.7,testphase={phase-III}");
        stays_bounded("tagging=on");
    }

    /// The worst one gap at one place is counted.
    fn worst(analysis: &Analysis) -> u32 {
        unsupported(analysis).into_iter().map(|(_, n)| n).max().unwrap_or(0)
    }

    /// The gap the bound once held down (one counted into the hundreds of
    /// thousands at `expl3-code.tex` line 12363 under the latex-lab
    /// testphase code) is gone: the arms of a choice inside an expanding
    /// body join where they meet, so no path repeats it. Without the
    /// bound the report is the same as with it.
    #[test]
    fn lifting_the_bound_leaves_the_report_as_it_is() {
        if !super::installed() {
            return;
        }
        const OPTIONS: &str = "lang=en,pdfversion=1.7,testphase={phase-III}";
        let mut lifted = crate::support::config();
        lifted.limits.repeat_failures = 0;
        lifted.limits.steps = BOUNDED_STEPS;
        let unbounded = worst(&analyze_with(OPTIONS, &lifted));
        assert!(unbounded <= bound(), "without the bound one gap was counted {unbounded} times");
        let bounded = worst(&analyze_document(OPTIONS));
        assert!(bounded <= bound(), "one gap was counted {bounded} times, past the bound of {}", bound());
    }
}

/// `\int_rand:n` over a range wider than `\c__kernel_randint_max_int`
/// (131071) goes through expl3's wide path, where `\tex_uniformdeviate:D`
/// hands `\__fp_randint_split_o:Nw` text satex cannot know and its
/// `\if_meaning:w` cannot be decided.  The token stream comes apart there,
/// and `\DocumentMetadata` meets it in tagpdf's namespace id.  What the
/// bound has to guarantee is that the damage stays countable.
#[test]
fn a_wide_int_rand_leaves_bounded_gaps() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\tl_set:Ne \l_tmpa_tl { \int_rand:n {16777215} }
\ExplSyntaxOff
\begin{document}
x
\end{document}",
    );
    let bound = satex::config::Config::default().limits.repeat_failures;
    let over: Vec<_> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::facts::Severity::Unsupported && d.code != "repeated-failure")
        .filter(|d| d.count > bound)
        .map(|d| (d.code, d.count))
        .collect();
    assert_eq!(over, Vec::new(), "gaps past the bound of {bound}");
    assert!(!analysis.exhausted, "the run was exhausted");
}

/// The narrow path of the same function stays clean: `\int_rand:n` up to
/// `\c__kernel_randint_max_int` needs one `\tex_uniformdeviate:D` and no
/// `\if_meaning:w` on its digits, so it leaves no gap at all.
#[test]
fn a_narrow_int_rand_leaves_no_gap() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\tl_set:Ne \l_tmpa_tl { \int_rand:n {65535} }
\ExplSyntaxOff
\begin{document}
x
\end{document}",
    );
    let found: Vec<(&str, u32)> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::facts::Severity::Unsupported)
        .map(|d| (d.code, d.count))
        .collect();
    assert_eq!(found, Vec::new());
}

/// expl3 raises its errors from an expandable branch a satex run often
/// cannot decide, and the branch runs `\msg_error:nneeee`, whose line
/// wrapping plays `\tex_edef:D \l__iow_line_part_tl { \if_false: } \fi:`.
/// That is the shape every expl3 error path has, so it has to survive a run
/// that takes it by over-approximation: `\intarray_gset:Nnn` with a value
/// satex cannot know reaches `\__intarray_gset_overflow:NNnn` and must
/// still leave the token stream intact.
#[test]
fn an_expl3_error_path_taken_on_an_unknown_value_leaves_no_gap() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\intarray_new:Nn \g_my_intarray { 4 }
\intarray_gset:Nnn \g_my_intarray { 1 } { \tex_pdflastobj:D }
\ExplSyntaxOff
\begin{document}
x
\end{document}",
    );
    let found: Vec<(&str, u32)> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::facts::Severity::Unsupported)
        .map(|d| (d.code, d.count))
        .collect();
    assert_eq!(found, Vec::new());
}

/// A `.code:n` key whose body builds a `\csname` out of its own argument is
/// what tagpdf's `parent` key does.  The parameter has to stay a parameter
/// until the key is set: a raw `#1` reaching `\csname` would build a name no
/// engine ever builds.
#[test]
fn a_code_key_keeps_its_parameter_out_of_a_csname() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\prop_new:N \g_my_0_prop
\keys_define:nn { my }
  {
    parent .code:n =
      {
        \bool_lazy_and:nnTF
          { \prop_if_exist_p:c { g_my_ \int_eval:n {#1} _prop } }
          { \int_compare_p:nNn {#1} < {5} }
          { \tl_set:Ne \l_tmpa_tl { \int_eval:n {#1} } }
          { }
      }
  }
\ExplSyntaxOff
\begin{document}
x
\end{document}",
    );
    let found: Vec<(&str, u32)> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::facts::Severity::Unsupported)
        .map(|d| (d.code, d.count))
        .collect();
    assert_eq!(found, Vec::new());
}

/// The gaps the run reported in the document itself: where it could not
/// follow the document and over-approximated.
fn document_gaps(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::facts::Severity::Unsupported && d.span.file == analysis.main_file)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

/// `\@bsphack` saves `\lastskip`, which satex does not know after text:
/// `\@esphack` then runs both ways, and the paths meet again, so every
/// label after it is still read.
#[test]
fn esphack_after_an_unknown_skip_loses_nothing() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
Text \label{a} more\index{x} and \label{b} then
\hspace{1em}\label{c}. \par\vskip\parskip \label{d}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["a", "b", "c", "d"]);
    assert!(document_gaps(&analysis).is_empty(), "{:?}", document_gaps(&analysis));
}

/// Paths that leave different `\aftergroup` tokens waiting are joined into
/// one token that holds each as one of its meanings (tex.web § 326): after
/// the group, each path's token has run.
#[test]
fn different_aftergroup_tokens_join_into_one_of_them() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\def\one{\gdef\seen{1}}\def\two{\gdef\seen{2}}\def\three{\gdef\seen{3}}
\begin{document}
{\ifcase\numexpr\lastskip/65536\relax \aftergroup\one\or\aftergroup\two\else\aftergroup\three\fi}
\label{after}
\end{document}",
    );
    let bodies: Vec<String> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| analysis.interner.name(d.name) == "seen")
        .filter_map(|d| d.mac.as_ref().map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner)))
        .collect();
    for n in ["1", "2", "3"] {
        assert!(bodies.contains(&n.to_string()), "{bodies:?}");
    }
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    assert!(document_gaps(&analysis).is_empty(), "{:?}", document_gaps(&analysis));
}

/// microtype's protrusion leaves tokens waiting on one path only
/// (`\MT@prot@addtoken@next`); the paths still meet and the document is
/// read to its end.
#[test]
fn microtype_protrusion_reads_the_whole_document() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{microtype}
\begin{document}
\section{One}\label{one}
``Quoted'' text, \textit{italic} and more.\label{two}
\begin{itemize}\item An item.\label{three}\end{itemize}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["one", "two", "three"]);
    assert!(document_gaps(&analysis).is_empty(), "{:?}", document_gaps(&analysis));
}

#[test]
fn a_label_composed_by_a_counter_is_recorded_expanded() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newcommand\mylabel[1]{\label{fig:#1}}
\begin{document}
\section{One}\label{sec:\thepage-intro}
\mylabel{x}
See \ref{sec:1-intro} and \ref{fig:x}.
\end{document}",
    );
    // `\thepage` expands to `1` on the first page: the key `\newlabel`
    // gets is the expanded text, not the unexpanded `\thepage` token, and
    // the macro-composed `\mylabel{x}` key is `fig:x`.
    let mut labels = keys(&analysis, OccKind::Label);
    labels.sort();
    assert_eq!(labels, vec!["fig:x".to_string(), "sec:1-intro".to_string()]);
    assert!(!codes(&analysis).contains(&"undefined-reference".to_string()));
}

#[test]
fn cleveref_does_not_shadow_the_label_it_extends() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{cleveref}
\begin{document}
\section{One}\label{sec:one}
\ref{sec:one}
\end{document}",
    );
    // cleveref writes a second `\newlabel{sec:one@cref}{...}` for the same
    // `\label`: that must not read back as a second, unused label.
    assert_eq!(keys(&analysis, OccKind::Label), vec!["sec:one".to_string()]);
    assert!(!codes(&analysis).contains(&"unused-label".to_string()));
}

/// A `\cite` key comes from what `\cite`'s own aux round trip reads back
/// (`\citation{key}` written, `\b@key` read); an option `\documentclass` or
/// `\usepackage` was given is a different command's argument, even when a
/// package echoes it into its own `.aux` line (babel writes its language
/// list there) with the option text's original span intact.
#[test]
fn a_class_or_package_option_is_not_a_citation_key() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass[a4paper,11pt]{article}
\usepackage[english,french]{babel}
\usepackage[numbers]{natbib}
\begin{document}
\cite{knuth}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Cite), vec!["knuth".to_string()]);
    // No bibliography: `knuth` is undefined, as LaTeX warns, and no option is.
    let undefined: Vec<String> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| f["code"] == "undefined-citation")
        .map(|f| f["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(undefined, ["knuth"]);
    // The option on a line of its own below `\documentclass`, as a class
    // passes it on to babel.
    let analysis = analyze(
        r"\documentclass[
  a4paper,
  english
]{article}
\usepackage{babel}
\begin{document}
\cite{knuth}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Cite), vec!["knuth".to_string()]);
}

/// The kernel's own `\cite`, with no bibliography package, still reads its
/// key from `\b@key`, not from any option list.
#[test]
fn a_plain_kernel_citation_is_still_found() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass[a4paper,11pt]{article}
\begin{document}
\cite{knuth}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Cite), vec!["knuth".to_string()]);
}

/// Two keys given to one `\cite` (natbib) are both read; neither the class
/// option list nor the package option list leaks in alongside them.
#[test]
fn natbib_reads_every_key_of_one_cite_and_no_options() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass[a4paper,11pt,twocolumn]{article}
\usepackage[numbers,sort]{natbib}
\begin{document}
\cite{knuth,lamport}
\end{document}",
    );
    let mut found = keys(&analysis, OccKind::Cite);
    found.sort();
    assert_eq!(found, vec!["knuth".to_string(), "lamport".to_string()]);
}

/// A node of unknown height makes pgfmath divide unknown numbers. The paths
/// of that division join `\pgfmathresult` into one of several texts, which a
/// delimited argument must not read past (it once ran into the `}` of
/// `\pgf@process`, pgfcorepoints.code.tex line 24).
#[test]
fn tikz_rounded_node_of_unknown_height_stays_balanced() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{tikz}
\begin{document}
\begin{tikzpicture}
\node (0) {N};
\node[text width=4cm, rounded corners, draw] (1) {p};
\end{tikzpicture}
\label{after}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
}

/// A node of unknown height placed relative to another: the same math on
/// unknown lengths, without rounded corners, is clean.
#[test]
fn tikz_positioned_node_of_unknown_height_is_clean() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{tikz}
\usetikzlibrary{positioning}
\begin{document}
\begin{tikzpicture}
\node (0) {N};
\node[below=1mm of 0, text width=4cm, draw] (1) {p};
\end{tikzpicture}
\label{after}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
}

/// An undelimited parameter that reads unknown text takes one token that
/// stands for any number, so the parameters after it are not in step with
/// the input: they are unknown too, and a delimited one scans on without
/// taking a `}` it meets for an error (it once ran into the `}` of the
/// `\edef`, then abandoned the path).
#[test]
fn unknown_undelimited_argument_keeps_later_parameters_in_step() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\makeatletter
\def\secondof#1#2#3\stop{#2}
\begin{document}
\edef\a{\csname r@x.\number\pdfuniformdeviate9\endcsname\@empty\@empty}
\edef\b{\expandafter\secondof\a\stop}
\label{after}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
}

/// A pdf string holding wholly unknown text (a date the engine gives):
/// hyperref's loops over it once piled up loop heads, one per pass, since
/// each pass stood at a new place (the counter in its argument), and the
/// scans over the widened text left stray `\fi`s behind (extra-fi,
/// extra-else, then a memory blow-up).
#[test]
fn pdf_string_of_unknown_text_keeps_its_delimiters() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage[calc,en-US]{datetime2}
\usepackage{hyperref}
\hypersetup{pdfsubject={Thesis, \DTMnow}}
\begin{document}
x\label{after}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    let gaps = broken_gaps(&analysis);
    for code in ["extra-fi", "extra-else", "missing-endcsname", "budget-exhausted"] {
        assert!(!gaps.iter().any(|g| g.starts_with(code)), "{gaps:?}");
    }
}

/// A thesis class sets `\date{\today}` with datetime2 and hyperref loaded.
/// datetime2 takes the date from `\pdfcreationdate`, which was unknown, so
/// every digit of `\today` was unknown text and hyperref's `\pdfstringdef`
/// loops over it never converged. The clock is a known date by default
/// (`clock: now`), said once as an assumption.
#[test]
fn date_of_today_is_known_through_pdfcreationdate() {
    if !installed() {
        return;
    }
    let mut cfg = crate::support::config();
    cfg.clock = "2024-03-05T14:30+02:00".parse().unwrap();
    let analysis = Machine::analyze(
        r"\documentclass{article}
\usepackage[calc,en-US]{datetime2}
\usepackage{hyperref}
\title{T}
\date{\today}
\edef\stamp{\pdfcreationdate}
\ExplSyntaxOn
\edef\kernelday{\int_use:N \c_sys_year_int -\int_use:N \c_sys_month_int -\int_use:N \c_sys_day_int}
\ExplSyntaxOff
\begin{document}
\maketitle
x\label{after}
\end{document}",
        None,
        &cfg,
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    assert_eq!(body(&analysis, "stamp"), "D:20240305143000+02'00'");
    // The kernel's own date constants hold the pinned date, not the day the
    // kernel cache was built on.
    assert_eq!(body(&analysis, "kernelday"), "2024-3-5");
    let date = analysis.metadata.iter().find(|m| m.field == "date").map(|m| m.text.as_str());
    assert_eq!(date, Some("March 5, 2024"));
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
    let clock = analysis.facts.diagnostics.iter().filter(|d| d.code == "clock-assumed").count();
    assert_eq!(clock, 1);
}

/// microtype tests a numbered protrusion entry with `\pdfmatch`, which
/// was unknown text: the entry `013` got an unknown slot, the loop over
/// its heirs (`\ifx\@nnil#2`) could not be decided and ran until widened,
/// and a `\fi` of `\textsc` went astray.
#[test]
fn microtype_knows_the_slot_of_a_numbered_protrusion_entry() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{microtype}
\makeatletter
\g@addto@macro\MT@setupfont{\def\@tempa{013}\MT@get@slot\edef\MTslot{\MT@char}}
\makeatother
\begin{document}
a \textsc{rq} b
\end{document}",
    );
    assert_eq!(body(&analysis, "MTslot"), "13");
    let gaps = broken_gaps(&analysis);
    assert!(!gaps.iter().any(|g| g.starts_with("extra-fi")), "{gaps:?}");
    let loops: Vec<&str> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| matches!(d.code, "undecided-loop" | "paths-widened"))
        .map(|d| d.code)
        .collect();
    assert!(loops.is_empty(), "{loops:?}");
}

/// expl3 sets `\c_sys_year_int` and its kin in `\everyjob`, which TeX
/// reads when the job starts, after the format (tex.web § 1337): they are
/// the run's date, not the day the kernel cache was built on.
#[test]
fn expl3_clock_constants_are_set_at_job_start() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\edef\kernelyear{\int_use:N \c_sys_year_int}
\ExplSyntaxOff
\edef\runyear{\the\year}
\begin{document}
x
\end{document}",
    );
    let today = crate::support::config().clock.resolve().unwrap().year().to_string();
    assert_eq!(body(&analysis, "runyear"), today);
    assert_eq!(body(&analysis, "kernelyear"), today);
    assert_eq!(analysis.facts.diagnostics.iter().filter(|d| d.code == "clock-assumed").count(), 1);
}

/// The kernel cache holds no date: two runs pinned to different dates,
/// sharing one cache, each see their own in `\c_sys_year_int`.
#[test]
fn pinned_clocks_share_the_kernel_cache() {
    if !installed() {
        return;
    }
    let source = r"\documentclass{article}
\ExplSyntaxOn
\edef\kernelyear{\int_use:N \c_sys_year_int}
\ExplSyntaxOff
\begin{document}
x
\end{document}";
    for (clock, year) in [("2024-03-05T14:30+02:00", "2024"), ("2031-07-01T08:00+00:00", "2031")] {
        let mut cfg = crate::support::config();
        cfg.clock = clock.parse().unwrap();
        let analysis = Machine::analyze(source, None, &cfg);
        assert_eq!(body(&analysis, "kernelyear"), year, "clock {clock}");
    }
}

/// `clock: unknown`: the date's digits are unknown, but digit-shaped
/// (`\pdfcreationdate` parses into four unknown digits), and the run ends
/// with the package intact: no stall, no budget, no stray `\fi`. (hyperref's
/// `\pdfstringdef` over such a date still multiplies paths: open.)
#[test]
fn unknown_clock_keeps_the_date_digit_shaped() {
    if !installed() {
        return;
    }
    let mut cfg = crate::support::config();
    cfg.clock = satex::config::Clock::Unknown;
    let analysis = Machine::analyze(
        r"\documentclass{article}
\usepackage[calc,en-US]{datetime2}
\makeatletter
\edef\year@{\@dtm@currentyear}
\makeatother
\edef\stamp{\today}
\begin{document}
x\label{after}
\end{document}",
        None,
        &cfg,
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    let year = body(&analysis, "year@");
    assert_eq!(year.chars().count(), 4, "{year:?}");
    assert!(year.chars().all(|c| c == satex::tex::UNKNOWN_DIGIT), "{year:?}");
    let gaps = broken_gaps(&analysis);
    for code in ["extra-fi", "extra-else", "budget-exhausted", "line-budget-cut", "repeated-failure"] {
        assert!(!gaps.iter().any(|g| g.starts_with(code)), "{gaps:?}");
    }
    assert!(!analysis.facts.diagnostics.iter().any(|d| d.code == "clock-assumed"));
}

/// An expl3 flag raised an unknown number of times, then cleared: the
/// height loop keeps its counter in its own argument, so every pass once
/// stood at a new place and made one more name (`\flag test 12`) that no
/// widening bounded; the run grew until the memory limit. The loop's
/// passes now meet as one head, and the family of names is one wild name.
#[test]
fn expl3_flag_raised_an_unknown_number_of_times_converges() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\flag_new:n { test }
\int_step_inline:nn { \pdfuniformdeviate 9 } { \flag_raise:n { test } }
\flag_clear:n { test }
\ExplSyntaxOff
\begin{document}
x\label{after}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    let gaps = broken_gaps(&analysis);
    for code in [
        "line-budget-cut",
        "missing-endcsname",
        "budget-exhausted",
        "undecided-condition",
        "repeated-failure",
        "limit-reached",
    ] {
        assert!(!gaps.iter().any(|g| g.starts_with(code)), "{gaps:?}");
    }
}

/// An error the document makes is recovered as TeX recovers it: a flag
/// raised without `\flag_new:n` makes `\cs:w \relax` (tex.web § 373:
/// `\endcsname` is inserted and the run goes on), then an extra
/// `\endcsname` (§ 1135, ignored), over and over. The hundredth error is
/// reported (TeX would stop there, § 82) and satex reads on. No widening, no repeated-failure
/// cut, no budget cascade into `\begin{document}`.
#[test]
fn an_error_in_the_document_is_recovered_like_tex() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\int_step_inline:nn{\pdfuniformdeviate 9}{\flag_raise:n{test}}
\flag_clear:n{test}
\ExplSyntaxOff
\begin{document}x\end{document}",
    );
    let count = |code: &str| {
        analysis.facts.diagnostics.iter().filter(|d| d.code == code).map(|d| u64::from(d.count)).sum::<u64>()
    };
    assert!(count("missing-endcsname") >= 51 && count("extra-endcsname") >= 50, "{:?}", analysis.facts.diagnostics);
    assert!(count("too-many-errors") >= 1, "the hundredth error is reported");
    for code in ["repeated-failure", "limit-reached", "budget-exhausted", "line-budget-cut", "unbalanced-argument"] {
        assert_eq!(count(code), 0, "{code}");
    }
    let raised = |text: &str| analysis.facts.raised.iter().any(|r| r.text == text);
    assert!(raised("Missing \\endcsname inserted") && raised("That makes 100 errors; please try again"));
    assert!(analysis.steps < 5_000_000, "{} steps", analysis.steps);
}

/// A resized picture whose styles are set up next to forest, fontawesome
/// and hyperref: a file name the hooks expand holds unknown text, and
/// expl3's quote loop over it once ate the marker an outer scan needed
/// (unbalanced-argument, extra-right-brace at expl3-code.tex:2549, then
/// missing-endcsname and unbalanced-group by the dozen). Four gaps remain.
#[test]
#[ignore = "expl3's file-name quote loop over unknown text still leaves unbalanced-argument and extra-right-brace at expl3-code.tex:2549-2551 and a choice-budget undecided-condition at :12513"]
fn resized_picture_with_styles_keeps_file_name_scans() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r#"\documentclass{article}
\usepackage{graphicx,xcolor}
\usepackage{tikz}
\usetikzlibrary{positioning,arrows.meta,fit,calc,shapes.geometric,backgrounds}
\usepackage{tikz}
\usepackage{forest}
\usepackage{commalists-tools}
\usepackage{fontawesome}
\usepackage{hyperref}
\usepackage{csquotes}
\usetikzlibrary{decorations.pathmorphing,shadows,arrows.meta}
\pgfdeclarelayer{background}\pgfdeclarelayer{foreground}\pgfdeclarelayer{middle}
\pgfdeclarelayer{very-background}
\pgfsetlayers{very-background,background,main,middle,foreground}

\tikzset{
   Soft/.style={line join=round,line cap=round},
   All Soft/.style={every path/.append style={Soft}},
   Blob/.style={
      draw=black,
      circle,
      outer sep=2pt,
      minimum size=1.8em
   },
   Blobs/.style={
      every node/.append style={Blob}
   },
   Use/.style={Blob},
   RectRounding/.style={rounded corners=3pt},
   RawRect/.style={
      draw=black,
      rectangle,
      RectRounding,
      minimum size=1.8em,
      inner sep=5pt,
      fill=white,
   },
   Rect/.style={
      draw=black,
      rectangle,
      RectRounding,
      outer sep=2pt,
      minimum size=1.8em
   },
   Def/.style={Rect},
   Rects/.style={
      every node/.append style={Rect,minimum width=#1}
   },
   Rects/.default={1.5em},
   Link/.style={
      draw,
      Soft,
      rounded corners=2.5pt,
      -Kite
   },
   Links/.style={
      every path/.append style={Link}
   },
   % for dataflow graphs
   comm/.style={rectangle,draw=gray,text width=9mm,align=center,minimum height=5mm,font=\ttfamily,fill=white,fill opacity=1,drop shadow={fill=lightgray!50}},
   d/.style={comm,rounded corners=2pt}, % variable def
   u/.style={comm,rounded corners=2.5mm}, % variable use % rounded rectangle breaks anchors
   v/.style={comm,signal, signal to=east and west, gray, text=black, inner sep=-4pt, text width=5mm,fill=lightgray!22!white}, % values
   e/.style={comm,rounded corners=2.5mm,densely dotted,thick,fill=white}, % exit point
   F/.style={comm,draw=gray,rounded corners=2pt,fill=white,inner xsep=.5em},
   fc/.style={comm,double,rounded corners=2pt},
   w-back/.style={fill=white,inner sep=1pt},
   T/.style={font=\footnotesize\ttfamily,text=gray},
   code/.style={font=\ttfamily},
   dfidn/.style={
      circle,darkgray,fill opacity=.925,xshift=-1mm,yshift=.15mm,fill=white,draw,minimum size=1.8em,scale=.8,inner sep=-1pt
   },
   klabel/.style={font=\scriptsize\sffamily, inner sep=1pt,text=black,above,execute at begin node={\strut}},
   olabel/.style={midway,fill=white,fill opacity=.75,text opacity=1,rounded corners=2pt,inner sep=1pt,font=\tiny\sffamily},
\newsavebox\DataFlow
   \node[d] (0) at (0,0) {};
   \node[u,right=4mm] (1) at (0.east) {};
   \node[v,above=4mm] (2) at (0.north) {~~~~};
   \node[u,below=4mm] (3) at (0.south) {};
   \draw[Link,-Kite] (3) -- (0);
   \draw[Link,-Kite] (0) -- (1);
   \draw[Link,dotted] (1) to[bend right] (2);
\end{tikzpicture}}}
\newsavebox\ControlFlow
\setbox\ControlFlow=\hbox{\resizebox*!{1.25cm}{\color{lightgray!80!BaseGray}\begin{tikzpicture}[Link/.style={draw,line cap=round,gray,thick},comm/.append style={very thick}]
\begin{document}
x\label{after}
\end{document}"#,
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
}

/// The same for a cloud around two lines.
#[test]
fn tikz_cloud_node_of_unknown_height_stays_balanced() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{tikz}
\usetikzlibrary{shapes.symbols}
\begin{document}
\begin{tikzpicture}
\node[draw,align=center,shape=cloud] {Feat.\\Identif.};
\end{tikzpicture}
\label{after}
\end{document}",
    );
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after"]);
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
}

/// tex.web § 889: every line of a paragraph is packed to `\hsize`, so a
/// `\vbox` or `\parbox` holding one is exactly that wide, and `\unvbox`
/// (§ 1110) carries the width into a `\vtop`.  What TikZ's `text width`
/// nodes and `\resizebox` measure must not fall back to unknown.
#[test]
fn a_paragraph_in_a_box_is_hsize_wide() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\setbox0\vbox{\hsize9mm\noindent\hbox{}\par}\edef\vboxwd{\the\wd0}
\setbox0\hbox{\begin{minipage}[t]{9mm}\leavevmode\hbox{}x\end{minipage}}\edef\parboxwd{\the\wd0}
\setbox2\vbox{\hsize9mm\noindent\hbox{}\par}\setbox1\vtop{\unvbox2}\edef\vtopwd{\the\wd1}
\end{document}",
    );
    assert_eq!(body(&analysis, "vboxwd"), "25.60747pt");
    assert_eq!(body(&analysis, "parboxwd"), "25.60747pt");
    assert_eq!(body(&analysis, "vtopwd"), "25.60747pt");
}

/// tex.web §§ 851-859, 889: a paragraph that fits `\hsize` is one line,
/// `hpack`ed to the list's height and depth, so a `\vbox` holding it has
/// an exact height, and a TikZ `text width` node an exact north anchor.
#[test]
fn a_one_line_paragraph_in_a_box_has_its_line_height() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{tikz}
\makeatletter
\begin{document}
\setbox0\vbox{\hsize9mm\noindent\hbox{}x\par}\edef\vboxht{\the\ht0}\edef\vboxdp{\the\dp0}
\begin{tikzpicture}
\node[draw,text width=9mm] (b) at (0,0) {};
\pgfpointanchor{b}{north}\edef\northy{\the\pgf@y}
\end{tikzpicture}
\end{document}",
    );
    assert_eq!(body(&analysis, "vboxht"), "4.30554pt");
    assert_eq!(body(&analysis, "vboxdp"), "0.0pt");
    assert_eq!(body(&analysis, "northy"), "3.53297pt");
}

/// A TikZ picture in `\resizebox` whose `\draw` ends on a fitted node: pdfTeX
/// runs it without error. Paths joined after closing groups the fork stood
/// in kept the fork's save entries of those groups, and a later group end
/// put back `\tikz@shapeborder@name` as undefined: missing-endcsname at
/// pgfsys-pdftex.def:262.
#[test]
fn resized_picture_keeps_the_shape_border_name() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{tikz}
\usetikzlibrary{positioning,fit}
\begin{document}
\resizebox{\textwidth}{!}{\begin{tikzpicture}\node (n) {$N$};\node[right=of n] (a) {};\node [fit={(a)}] (c) {};\draw (n) -- (a);\end{tikzpicture}}
\end{document}",
    );
    let gaps = broken_gaps(&analysis);
    assert!(!analysis.facts.diagnostics.iter().any(|d| d.code == "missing-endcsname"), "{gaps:?}");
}

#[test]
fn tikz_pictures_with_unknown_borders_do_not_exhaust_the_budget() {
    if !installed() {
        return;
    }
    let picture = |extra: &str| {
        format!(
            r"\setbox0=\hbox{{\begin{{tikzpicture}}[Link/.style={{draw,line cap=round,gray,thick}}]
   \node[{extra}] (0) at (0,0) {{}};
   \node[right=4mm] (1) at (0.east) {{}};
   \node[above=4mm] (2) at (0.north) {{~~~~}};
   \draw[Link] (0) -- (1);
   \draw[Link,dotted] (1) to[bend right] (2);
\end{{tikzpicture}}}}"
        )
    };
    let source = format!(
        "\\documentclass{{article}}\\usepackage{{tikz}}\\usepackage{{hyperref}}\n\\begin{{document}}\n{}\n{}\n\\end{{document}}",
        picture("circle"),
        picture("draw")
    );
    let analysis = analyze(&source);
    assert!(!analysis.exhausted);
}

/// The scale factor of `\resizebox` is a `\strip@pt` of an unknown quotient:
/// number text with no `+`, no `--` and at most one `.`, which the delimited
/// scans of graphics' `\GPT@NormalizeNumber` decide instead of failing on.
#[test]
fn a_resized_box_of_unknown_width_normalizes_its_scale() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}\usepackage{graphicx}\begin{document}
\resizebox{\textwidth}{!}{\rule{\pdfuniformdeviate 100pt}{1pt}}
\resizebox{\textwidth}{!}{$N$}
x\label{after}\end{document}",
    );
    let count = |code: &str| analysis.facts.diagnostics.iter().filter(|d| d.code == code).count();
    for code in ["repeated-failure", "limit-reached", "argument-ends-in-unknown"] {
        assert_eq!(count(code), 0, "{code}: {:?}", analysis.facts.diagnostics);
    }
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after".to_string()]);
    assert!(analysis.steps < 2_500_000, "{} steps", analysis.steps);
}

/// `\strip@pt` of a dimension of unknown sign: `\the` prints a `-` or a
/// digit first, never the `.` that `\rem@pt`'s argument ends at, so no
/// path reads an empty integer part and TeX's "Missing number" (pdflatex
/// compiles this cleanly).
#[test]
fn stripping_the_unit_of_a_dimension_of_unknown_sign_is_no_error() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}\begin{document}\makeatletter
\dimen@=\pdfelapsedtime sp \advance\dimen@ by -\pdfelapsedtime sp
\edef\x{\strip@pt\dimen@}\end{document}",
    );
    assert!(!codes(&analysis).contains(&"missing-number".to_string()), "{:?}", codes(&analysis));
}

/// epstopdf-base converts an EPS file through `\write18` (`repstopdf`, which
/// `-shell-restricted` runs, or `epstopdf` under `-shell-escape`); the PDF it
/// makes has the EPS `%%BoundingBox` moved to the origin as its page, so the
/// image's box is known. Sizes as `pdflatex` prints them for the same file
/// (`%%HiResBoundingBox` unread). With no `\write18` nothing is converted
/// and pdftex.def's error is TeX's.
#[test]
fn an_eps_image_converted_through_the_shell_has_its_bounding_box() {
    if !installed() {
        return;
    }
    use satex::config::ShellEscape;
    let dir = crate::support::scratch_dir(std::env::temp_dir().join(format!("satex-eps-{}", std::process::id())));
    let eps = "%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 10 -20 295 61\n%%HiResBoundingBox: 10.2 -19.5 294.8819 60.7874\n\
               %%EndComments\nnewpath 10 -20 moveto 284 0 rlineto 0 80 rlineto closepath fill\nshowpage\n%%EOF\n";
    std::fs::write(dir.join("logo.eps"), eps).unwrap();
    // The same as a DOS EPS binary file, a preview after it (Adobe TN 5002).
    let mut dos = b"\xC5\xD0\xD3\xC6".to_vec();
    for word in [32u32, eps.len() as u32, 0, 0, 0, 0] {
        dos.extend(word.to_le_bytes());
    }
    dos.extend([0xff, 0xff, 0, 0]);
    dos.extend(eps.as_bytes());
    dos.extend(b"II*\0 preview");
    std::fs::write(dir.join("dos.eps"), dos).unwrap();
    let source = r"\documentclass{article}\usepackage{graphicx}\newcommand\here{.}
\begin{document}
\setbox0\hbox{\includegraphics{\here/logo}}\xdef\nat{\the\wd0,\the\ht0}
\setbox0\hbox{\includegraphics[width=73mm]{\here/logo}}\xdef\scaled{\the\wd0,\the\ht0}
\setbox0\hbox{\includegraphics{dos}}\xdef\dos{\the\wd0,\the\ht0}
\end{document}";
    let doc = dir.join("doc.tex");
    std::fs::write(&doc, source).unwrap();
    for mode in [ShellEscape::Restricted, ShellEscape::Full, ShellEscape::None] {
        let mut cfg = crate::support::config();
        cfg.shell_escape = Some(mode);
        let analysis = Machine::analyze(source, Some(&doc), &cfg);
        if mode == ShellEscape::None {
            assert_eq!(body(&analysis, "scaled"), "207.70511pt,207.71666pt", "a draft box, as pdflatex makes");
            assert!(raised(&analysis).contains(&"error".to_string()), "{mode:?}");
            continue;
        }
        assert_eq!(body(&analysis, "nat"), "286.06805pt,81.30354pt", "{mode:?}");
        assert_eq!(body(&analysis, "scaled"), "207.70511pt,59.0324pt", "{mode:?}");
        assert_eq!(body(&analysis, "dos"), "286.06805pt,81.30354pt", "{mode:?}");
        assert!(raised(&analysis).is_empty(), "{mode:?}: {:?}", raised(&analysis));
        assert!(!codes(&analysis).contains(&"missing-number".to_string()), "{mode:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// tex.web §§ 424, 996, 1013: on the main vertical list `\lastskip` and
/// `\lastkern` read its last item whether the page builder took it or not,
/// and an output routine leaves the items from the break on after its own
/// (`\clearpage` ends in a penalty, so beamer's `\reset@color` after it
/// reads 0pt). Values as `pdflatex` prints them.
#[test]
fn the_last_item_of_the_main_vertical_list_is_known_past_the_page_builder() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}\usepackage{xcolor}
\begin{document}
text\par\vskip 3pt\xdef\a{\the\lastskip}
\penalty 5 \xdef\b{\the\lastskip}
\clearpage\xdef\c{\the\lastskip}
\begingroup\color{red}\endgroup\xdef\d{\the\lastskip}
x\newpage\xdef\e{\the\lastskip}
\vskip 2pt plus 1fil\kern 4pt\xdef\f{\the\lastkern|\the\lastskip}
\hbox{}\vskip 6pt minus 1pt\xdef\g{\the\lastskip}
\end{document}",
    );
    let got: Vec<String> = ["a", "b", "c", "d", "e", "f", "g"].iter().map(|k| body(&analysis, k)).collect();
    assert_eq!(got, ["3.0pt", "0.0pt", "0.0pt", "0.0pt", "0.0pt", "4.0pt|0.0pt", "6.0pt minus 1.0pt"]);
}

/// tex.web §§ 668, 1105: on a vertical list glue or a kern adds the depth
/// before it into the height, and `\unskip` or `\unkern` taking it away
/// gives that depth back, so the box keeps its size (beamer's
/// `\reset@color` takes the `\topsep` after a list away and puts it back).
/// Sizes as `pdflatex` prints them.
#[test]
fn taking_glue_off_a_vertical_list_restores_its_size() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{beamer}
\begin{document}
\begin{frame}{T}
\setbox0\vbox{\hsize 5cm \begin{itemize}\item Work\end{itemize}}\xdef\a{\the\ht0,\the\dp0}
\setbox0\vbox{\hbox{\vrule depth 2pt height 3pt}\vskip 5pt\unskip}\xdef\b{\the\ht0,\the\dp0}
\setbox0\vtop{\vskip 4pt\unskip\hbox{\vrule depth 2pt height 3pt}}\xdef\c{\the\ht0,\the\dp0}
\setbox0\vbox{\hbox{\vrule depth 2pt height 3pt}\kern 5pt\unkern\kern1pt}\xdef\d{\the\ht0,\the\dp0}
\setbox0\vbox{\hbox{\vrule depth 2pt height 3pt}\penalty5\unpenalty}\xdef\f{\the\ht0,\the\dp0}
\end{frame}
\end{document}",
    );
    let got: Vec<String> = ["a", "b", "c", "d", "f"].iter().map(|k| body(&analysis, k)).collect();
    assert_eq!(got, ["13.60416pt,0.0pt", "3.0pt,2.0pt", "3.0pt,2.0pt", "6.0pt,0.0pt", "3.0pt,2.0pt"]);
}

/// A PDF file of `objects` (numbered from 1, the first the catalog) with
/// its cross-reference table.
fn pdf_file(objects: &[&str]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
    out
}

/// `\pdfximage page n` sizes page `n` of a page tree as pdfTeX does: the
/// page's crop box clipped to its media box, both and `/Rotate` inherited
/// from the `/Pages` nodes above it, a rotation by 90 or 270 swapping width
/// and height, a length through single precision (439.363bp is 28902072sp).
/// Sizes as `pdflatex` prints them for the same file; a page past the last
/// is unknown (pdfTeX stops there).
#[test]
fn a_page_of_a_pdf_file_has_its_own_box() {
    if !installed() {
        return;
    }
    let dir = crate::support::scratch_dir(std::env::temp_dir().join(format!("satex-pages-{}", std::process::id())));
    let pdf = pdf_file(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [4 0 R 5 0 R 3 0 R 8 0 R] /Count 5 /MediaBox [0 0 439.363 666.131] >>",
        "<< /Type /Pages /Parent 2 0 R /Kids [6 0 R 7 0 R] /Count 2 /Rotate 90 /CropBox [5 5 405 305] >>",
        "<< /Type /Page /Parent 2 0 R /Contents 9 0 R >>",
        "<< /Type /Page /Parent 2 0 R /CropBox [-77.9515 -83.9042 517.049 757.096] /Contents 9 0 R >>",
        "<< /Type /Page /Parent 3 0 R /CropBox [10 20 210 120] /Contents 9 0 R >>",
        "<< /Type /Page /Parent 3 0 R /Rotate 180 /MediaBox [0 0 300 100] /Contents 9 0 R >>",
        "<< /Type /Page /Parent 2 0 R /Rotate -90 /CropBox [-10 50 200 700] /Contents 9 0 R >>",
        "<< /Length 0 >>\nstream\n\nendstream",
    ]);
    std::fs::write(dir.join("multi.pdf"), pdf).unwrap();
    let source = r"\documentclass{article}
\begin{document}
\count255=1
\loop
\pdfximage page \count255 {multi.pdf}\setbox0\hbox{\pdfrefximage\pdflastximage}
\pdfximage page \count255 mediabox {multi.pdf}\setbox2\hbox{\pdfrefximage\pdflastximage}
\expandafter\xdef\csname P\romannumeral\count255\endcsname{\the\wd0,\the\ht0;\the\wd2,\the\ht2}
\ifnum\count255<5 \advance\count255 1 \repeat
\pdfximage page 1 {multi.pdf}\setbox0\hbox{\pdfrefximage\pdflastximage}\xdef\sp{\number\wd0,\number\ht0}
\pdfximage page 6 {multi.pdf}\setbox0\hbox{\pdfrefximage\pdflastximage}\xdef\past{\number\wd0}
\end{document}";
    let doc = dir.join("doc.tex");
    std::fs::write(&doc, source).unwrap();
    let analysis = Machine::analyze(source, Some(&doc), &crate::support::config());
    let _ = std::fs::remove_dir_all(&dir);
    for (page, want) in [
        ("Pi", "441.01062pt,668.62897pt;441.01062pt,668.62897pt"),
        ("Pii", "441.01062pt,668.62897pt;441.01062pt,668.62897pt"),
        ("Piii", "100.375pt,200.75pt;668.62897pt,441.01062pt"),
        ("Piv", "296.10625pt,95.35625pt;301.125pt,100.375pt"),
        ("Pv", "618.44147pt,200.75pt;668.62897pt,441.01062pt"),
        ("sp", "28902072,43819268"),
    ] {
        assert_eq!(body(&analysis, page), want, "{page}");
    }
    assert!(!body(&analysis, "past").chars().all(|c| c.is_ascii_digit()), "{}", body(&analysis, "past"));
}

/// tex.web §§ 764-766, 1155: a `\mathchardef` symbol is one atom with its
/// font's width, and relations and punctuation are spaced by their muskips.
/// The widths are what `pdflatex` prints for the same boxes.
#[test]
fn inline_math_of_relations_and_math_chars_has_its_width() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\setbox0\hbox{$\leftarrow$}\xdef\wa{\the\wd0}
\setbox0\hbox{$a\leftarrow b$}\xdef\wb{\the\wd0}
\setbox0\hbox{$(a,b)=x$}\xdef\wc{\the\wd0}
\end{document}",
    );
    assert_eq!(body(&analysis, "wa"), "10.00002pt");
    assert_eq!(body(&analysis, "wb"), "25.13298pt");
    assert_eq!(body(&analysis, "wc"), "40.84825pt");
}

/// tex.web §§ 651, 1119-1120: a discretionary's break texts do not count
/// for its box, its text without a break does. Widths as `pdflatex` prints.
#[test]
fn a_discretionary_counts_only_its_text_without_a_break() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\setbox0\hbox{a\discretionary{x}{}{}b}\xdef\da{\the\wd0}
\setbox0\hbox{a\discretionary{\rlap{x}}{}{yy}b}\xdef\db{\the\wd0}
\end{document}",
    );
    assert_eq!(body(&analysis, "da"), "10.55559pt");
    assert_eq!(body(&analysis, "db"), "21.11119pt");
}

body_cases! {
    /// A formula that loads its math fonts at a new size: the font files run in
    /// math mode (`\lowercase`, `\pdffontexpand` leave no node, tex.web §§ 1285,
    /// pdfTeX manual), so the formula keeps its width. As `pdflatex` prints.
    a_formula_that_loads_its_fonts_keeps_its_width(r"\documentclass{article}
\usepackage{microtype}
\usepackage{newtxmath}
\begin{document}
\setbox0\hbox{\footnotesize\(\leftarrow\)}\xdef\wa{\the\wd0}
\end{document}", "wa", "8.192pt");
}

/// tex.web §§ 728-729, 749, 764-766, 1156: binary atoms that turn ordinary,
/// operators, groups and `\mathrel`/`\mathinner` keep the formula's width.
/// Widths as `pdflatex` prints.
#[test]
fn binary_operator_and_group_atoms_have_their_width() {
    if !installed() {
        return;
    }
    let cases = [
        (r"$a+b$", "21.79968pt"),
        (r"$-a$", "13.06369pt"),
        (r"$a+=b$", "30.68857pt"),
        (r"$\sum x$", "17.93748pt"),
        (r"$(N_1,N_2,\dots)$", "53.37492pt"),
        (r"$a{=}b$", "17.35535pt"),
        (r"$a\mathrel{x}b$", "20.84824pt"),
        (r"$a+$", "13.06369pt"),
        (r"$f\mathinner{xy}$", "18.61575pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!("\\setbox0\\hbox{{{f}}}\\xdef\\W{}{{\\the\\wd0}}\n", (b'a' + i as u8) as char));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    let got: Vec<(String, String)> = cases
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (f.to_string(), body(&analysis, &format!("W{}", (b'a' + i as u8) as char))))
        .collect();
    let want: Vec<(String, String)> = cases.iter().map(|(f, w)| (f.to_string(), w.to_string())).collect();
    assert_eq!(got, want);
}

/// tex.web § 1080: `\lastbox` takes back an hbox that is all its list
/// holds, at its size, and the list is empty again: the indent box the
/// paragraph hooks put back, which `\item` takes. A parbox is centred with
/// `half` (§§ 100, 736) rounding up. Sizes as `pdflatex` prints.
#[test]
fn a_lone_hbox_taken_by_lastbox_keeps_the_sizes_known() {
    if !installed() {
        return;
    }
    let cases = [
        (r"\hbox{\parbox{3cm}{\centering xy}}", "85.35826pt/5.625pt/0.62498pt"),
        (r"\hbox{\parbox{3cm}{\begin{center}xy\end{center}}}", "85.35826pt/15.625pt/10.62498pt"),
        (r"\hbox{\parbox{3cm}{\trivlist\item\relax xy\endtrivlist}}", "85.35826pt/15.625pt/10.62498pt"),
        (r"\vbox{\hsize3cm\everypar{\setbox2\lastbox}xy}", "85.35826pt/4.30554pt/1.94444pt"),
        (r"\vbox{\hsize3cm\trivlist\item\relax xy\endtrivlist}", "85.35826pt/26.24998pt/0.0pt"),
        (r"\hbox{\setbox3\hbox{Ag}\copy3\setbox2\lastbox}", "0.0pt/0.0pt/0.0pt"),
        (
            r"\hbox{\setbox3\hbox{Ag}\copy3\setbox2\lastbox\global\setbox4\box2}\setbox0\box4",
            "12.50003pt/6.83331pt/1.94444pt",
        ),
        (r"\hbox{\parbox{3cm}{\noindent\setbox3\hbox{Ag}\copy3\setbox2\lastbox xy}}", "85.35826pt/5.625pt/0.62498pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0{f}\\xdef\\S{}{{\\the\\wd0/\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    for (i, (f, want)) in cases.iter().enumerate() {
        assert_eq!(body(&analysis, &format!("S{}", (b'a' + i as u8) as char)), *want, "{f}");
    }
}

/// tex.web § 1080: a forced break starts a new line, but `\unpenalty`
/// takes the penalty back and `\lastbox` the box before it, whose size the
/// new line does not have: it is never read as an empty box's.
#[test]
fn a_box_uncovered_after_a_forced_break_is_not_empty() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\begin{document}\n\\setbox0\\hbox{\\parbox{3cm}{\\noindent\\setbox3\\hbox{Ag}\\copy3\
         \\penalty-10000\\unpenalty\\setbox2\\lastbox\\xdef\\W{\\the\\wd2}}}\n\\end{document}",
    );
    let w = body(&analysis, "W");
    assert!(!w.is_empty() && w != "0.0pt", "{w}");
}

/// tex.web §§ 848, 889: a `\parshape` whose lines are all alike, as
/// `\list` sets it, packs every line to its length, shifted by its indent.
/// Sizes as `pdflatex` prints.
#[test]
fn a_uniform_parshape_keeps_the_lines_known() {
    if !installed() {
        return;
    }
    let cases = [
        (r"\vbox{\hsize3cm\linewidth3cm\list{}{}\item xy\endlist}", "85.35826pt/26.24998pt/0.0pt"),
        (r"\vbox{\hsize3cm\parshape1 10pt 50pt xy}", "60.0pt/4.30554pt/1.94444pt"),
        (r"\vbox{\hsize3cm\linewidth3cm\begin{itemize}\item xy\end{itemize}}", "85.35826pt/26.31837pt/0.0pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0{f}\\xdef\\S{}{{\\the\\wd0/\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    for (i, (f, want)) in cases.iter().enumerate() {
        assert_eq!(body(&analysis, &format!("S{}", (b'a' + i as u8) as char)), *want, "{f}");
    }
}

/// microtype's `\leftprotrusion` sets its text in a trial vbox, splits
/// the line off with `\vsplit` (tex.web §§ 970-977: no breakpoint but the
/// end, packed to the height asked), `\unvbox`es it, takes it back with
/// `\lastbox` and reads pdfTeX's `\leftmarginkern` of it: `\lpcode`
/// thousandths of the quad; of a ligature first, its own character's
/// (pdfTeX's `char_pw` reads `lig_char`). Sizes as `pdflatex` prints.
#[test]
fn left_protrusion_of_a_line_is_known() {
    if !installed() {
        return;
    }
    let src = r"\documentclass{article}
\usepackage[T1]{fontenc}
\usepackage{microtype}
\begin{document}
\makeatletter
\def\D#1#2{\setbox0\hbox{#2}\xdef#1{\the\wd0/\the\ht0/\the\dp0}}
\D\Ma{\vbox{\hsize3cm\trivlist\item\relax Ag\endtrivlist}}
\D\Mb{\leftprotrusion Ag}
\D\Mc{\leftprotrusion{T}he}
\setbox2\vbox{\hsize3cm\noindent A}\setbox4\vbox{\setbox2\vsplit2 to 12pt \xdef\Vs{\the\wd2/\the\ht2/\the\dp2}\unvbox2 \global\setbox6\lastbox}
\xdef\Mk{\leftmarginkern6/\the\wd6/\the\ht6/\the\dp6/\the\wd4/\the\ht4}
\setbox0\vbox{\hsize3cm\pdfprotrudechars=0 \noindent A}\setbox0\vbox{\setbox2\vsplit0 to 5pt \unvbox2 \global\setbox6\lastbox}\xdef\Mz{\leftmarginkern6}
\setbox0\vbox{\hsize3cm}\setbox2\vsplit0 to 7pt \xdef\Me{\the\wd2/\the\ht2/\the\dp2/\ifvoid0 void\fi}
\setbox0\vbox{\hsize3cm\noindent Ag}\splitmaxdepth=1pt \setbox2\vsplit0 to 3pt \xdef\Md{\the\wd2/\the\ht2/\the\dp2/\ifvoid0 void\fi}
\setbox0\hbox{\leftprotrusion{fi}x}\xdef\Ml{\the\wd0}
\setbox0\hbox{\leftprotrusion{--}x}\xdef\Mn{\the\wd0}
\setbox0\vbox{\hsize3cm\noindent ---}\setbox0\vbox{\setbox2\vsplit0 to 12pt \unvbox2 \global\setbox6\lastbox}\xdef\Mo{\leftmarginkern6}
\end{document}";
    let analysis = analyze(src);
    let want = [
        ("Ma", "85.35826pt/28.83118pt/0.0pt"),
        ("Mb", "12.11703pt/6.8872pt/1.94397pt"),
        ("Mc", "16.85811pt/6.8872pt/0.0pt"),
        ("Vs", "85.35826pt/12.0pt/0.0pt"),
        ("Mk", "-0.37991pt/85.35826pt/6.8872pt/0.0pt/0.0pt/0.0pt"),
        ("Mz", "0pt"),
        ("Me", "0.0pt/7.0pt/0.0pt/void"),
        ("Md", "85.35826pt/3.0pt/1.0pt/void"),
        ("Ml", "10.83069pt"),
        ("Mn", "8.27576pt"),
        ("Mo", "-2.99927pt"),
    ];
    for (name, want) in want {
        assert_eq!(body(&analysis, name), want, "{name}");
    }
}

/// tex.web §§ 909, 914-918: at a hyphenation point with a font kern to
/// the next letter or to the hyphen, the discretionary replaces the letter
/// and its kern and its pre-break text is the letter, that kern and the
/// hyphen. Palatino kerns `v` in "every" and "toward": the paragraphs
/// break as `pdflatex` breaks them, also at exactly the pre-break width.
#[test]
fn a_kern_at_a_hyphenation_point_keeps_the_lines_known() {
    if !installed() {
        return;
    }
    let src = r"\documentclass{article}
\usepackage[T1]{fontenc}
\usepackage[osf]{mathpazo}
\begin{document}
\pretolerance=-1 \tolerance=100 \parfillskip=0pt plus 1fil
\setbox0\hbox{ev-}\setbox0\vbox{\hsize=\wd0 \parindent0pt \hskip0pt every\par\xdef\Ma{\the\prevgraf}}
\setbox0\hbox{ev-}\setbox0\vbox{\hsize=\dimexpr\wd0-1sp\relax \parindent0pt \hskip0pt every\par\xdef\Mb{\the\prevgraf}}
\setbox0\hbox{tow-}\setbox0\vbox{\hsize=\wd0 \parindent0pt \hskip0pt toward\par\xdef\Md{\the\prevgraf}}
\setbox0\vbox{\hsize=1pt \tolerance=10000 \hyphenpenalty=0 \parindent0pt \hskip0pt every\par\xdef\Me{\the\prevgraf}}
\setbox0\vbox{\hsize=1pt \tolerance=10000 \hyphenpenalty=0 \parindent0pt \hskip0pt revolution\par}\xdef\Mf{\the\ht0/\the\dp0}
\end{document}";
    let analysis = analyze(src);
    for (name, want) in [("Ma", "2"), ("Mb", "2"), ("Md", "1"), ("Me", "3"), ("Mf", "48.0pt/0.215pt")] {
        assert_eq!(body(&analysis, name), want, "{name}");
    }
}

/// microtype's `\leftprotrusion` after `\item` measures an en dash
/// ligature first in the item: the margin kern it reads is known, so the
/// paragraph surely starts there and the characters after it do not each
/// fork on the mode (they did, summarizing a loop per character).
#[test]
fn an_item_starting_with_a_dash_ligature_starts_its_paragraph() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{microtype}
\begin{document}
\begin{description}
\item[X] -- Gamma
\end{description}
\end{document}",
    );
    let found = codes(&analysis);
    assert!(!found.iter().any(|c| c == "recursion-widened"), "{found:?}");
}

/// tex.web §§ 731, 763, 1171-1174: in inline math `\textstyle`,
/// `\scriptstyle` and `\scriptscriptstyle` set the atoms after them in
/// that style, and `\mathchoice` takes the list the style picks in its
/// place (amsmath's `\text` is one). `\displaystyle` is not followed.
/// Sizes as `pdflatex` prints.
#[test]
fn math_styles_and_choices_keep_the_size_known() {
    if !installed() {
        return;
    }
    let cases = [
        (r"$\mathchoice{a}{bb}{c}{ddd}$", "8.58331pt/6.94444pt/0.0pt"),
        (r"$B_{\mathchoice{a}{bb}{c}{ddd}}$", "11.65883pt/6.83331pt/1.49998pt"),
        (r"$x^{y^{\mathchoice{a}{bb}{c}{ddd}}}$", "22.11757pt/10.11893pt/0.0pt"),
        (r"$\scriptstyle x$", "4.53473pt/3.01389pt/0.0pt"),
        (r"$a+\scriptstyle b+c$", "28.51518pt/5.83333pt/0.83334pt"),
        (r"$f\scriptscriptstyle x\textstyle y$", "15.19221pt/6.94444pt/1.94444pt"),
        (r"$x_{\mathchoice{}{}{\scriptstyle a}{}b}c$", "18.39714pt/4.30554pt/1.49998pt"),
        (r"$B_{\text{x}}$", "12.2865pt/6.83331pt/1.49998pt"),
        (r"$a\mathchoice{+}{+}{+}{+}b$", "21.79968pt/6.94444pt/0.83333pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0\\hbox{{{f}}}\\xdef\\S{}{{\\the\\wd0/\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\setbox0\\hbox{$a\\displaystyle b$}\\xdef\\D{\\the\\wd0}\n\\end{document}");
    let analysis = analyze(&src);
    for (i, (f, want)) in cases.iter().enumerate() {
        assert_eq!(body(&analysis, &format!("S{}", (b'a' + i as u8) as char)), *want, "{f}");
    }
    // pdflatex: 9.57755pt; a display style in inline math is not followed.
    let d = body(&analysis, "D");
    assert!(d.contains("unknown"), "{d}");
}

/// tex.web § 1151: a script given by name, a `\mathchardef` constant or
/// `\mathchar`, is one math character, as a letter is. Sizes as `pdflatex`
/// prints.
#[test]
fn a_math_character_named_as_a_script_keeps_the_size_known() {
    if !installed() {
        return;
    }
    let cases = [
        (r"$O_\infty$", "16.1pt/6.83331pt/1.49998pt"),
        (r"$x^\alpha$", "11.41403pt/6.6428pt/0.0pt"),
        (r"$x_\alpha^\beta$", "11.41403pt/8.49002pt/2.47217pt"),
        (r#"$x^\mathchar"0231$"#, "14.18752pt/6.6428pt/0.0pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0\\hbox{{{f}}}\\xdef\\S{}{{\\the\\wd0/\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    for (i, (f, want)) in cases.iter().enumerate() {
        assert_eq!(body(&analysis, &format!("S{}", (b'a' + i as u8) as char)), *want, "{f}");
    }
}

/// tex.web §§ 777-801: every cell of an alignment holds its template,
/// `u_j` before and `v_j` after its text, so a `\unskip` in `v_j` reaches
/// only the cell's own text (array's `\hskip1sp`); columns are as wide as
/// their widest cell, rows as high and deep as their cells. Sizes as
/// `pdflatex` prints; what satex does not follow (spans, `\cline`,
/// several lines in a `p` cell) stays unknown, never another size.
#[test]
fn alignment_cells_hold_their_templates() {
    if !installed() {
        return;
    }
    let exact = [
        (r"\begin{tabular}{@{}l@{}}x\end{tabular}", "5.27782pt/8.5pt/3.5pt"),
        (r"\begin{tabular}{l}x\end{tabular}", "17.27782pt/8.5pt/3.5pt"),
        (r"\vbox{\halign{\hskip1sp#\unskip\cr x\cr}}", "5.27782pt/4.30554pt/0.0pt"),
        (r"\begin{tabular}{@{}lp{5cm}@{}}a&x\\b&y\end{tabular}", "159.81937pt/14.5pt/9.5pt"),
        (r"\begin{tabular}{|c|r|}\hline ab&c\\\hline d&efg\\\hline\end{tabular}", "48.25563pt/15.09999pt/10.09999pt"),
        (r"\begin{tabular}{ll}a&\multicolumn{1}{c}{b}\\cc&d\end{tabular}", "38.44449pt/14.5pt/9.5pt"),
        (r"\vbox{\halign{#\hfil&\hfil#\cr a&bb\cr ccc&d\cr\noalign{\kern2pt}e\cr}}", "24.44447pt/32.94444pt/0.0pt"),
        (r"\begin{tabular}{l}x\\\end{tabular}", "17.27782pt/8.5pt/3.5pt"),
        (r"\begin{tabular}{>{\bfseries}l<{!}r}a&b\\cc&dd\end{tabular}", "48.83333pt/14.5pt/9.5pt"),
        (r"\begin{tabular}{l}a\\[4pt]b\\\end{tabular}", "17.55559pt/16.5pt/11.5pt"),
        (r"\begin{tabular}{l}\begin{tabular}{c}in\\ner\end{tabular}\\x\end{tabular}", "37.91672pt/20.5pt/15.5pt"),
        (r"\begin{tabular}{ll}a&\\&b\\\end{tabular}", "34.55562pt/14.5pt/9.5pt"),
        (r"\begin{tabular}{c}\omit x\\yy\end{tabular}", "22.55562pt/10.65277pt/5.65277pt"),
        (r"\begin{tabular}{l@{ -- }l}a&b\end{tabular}", "34.22229pt/8.5pt/3.5pt"),
    ];
    let covered = [
        (
            r"\begin{tabular}{|l|c|r|}\hline a&bb&ccc\\\hline\hline d&&f\\\cline{1-2}\end{tabular}",
            "67.60007pt/16.09999pt/11.09999pt",
        ),
        (r"\begin{tabular}{ll}\multicolumn{2}{c}{wide header}\\a&b\end{tabular}", "64.25009pt/14.5pt/9.5pt"),
        (r"\begin{tabular}{p{2cm}l}some longer words in a cell&x\\y&z\end{tabular}", "86.18332pt/26.5pt/21.5pt"),
        // tex.web §§ 796, 889, 1076: `\vadjust` material follows the
        // line, the row or the box on the vertical list.
        (r"\begin{tabular}{@{}lp{3cm}@{}}&x\bigskip\end{tabular}", "97.35826pt/14.5pt/9.5pt"),
        (r"\vtop{\hsize3cm x\vadjust{\kern3pt}}", "85.35826pt/4.30554pt/3.0pt"),
        (r"\vbox{\hbox{x\vadjust{\kern3pt}}}", "5.2778pt/7.30554pt/0.0pt"),
        (r"\vbox{\halign{#\cr x\vadjust{\kern3pt}\cr}}", "5.2778pt/7.30554pt/0.0pt"),
    ];
    let all: Vec<_> = exact.iter().chain(covered.iter()).collect();
    let mut src = String::from("\\documentclass{article}\n\\usepackage{array}\n\\begin{document}\n");
    for (i, (f, _)) in all.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0\\hbox{{{f}}}\\xdef\\S{}{{\\the\\wd0/\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    for (i, (f, want)) in all.iter().enumerate() {
        let got = body(&analysis, &format!("S{}", (b'a' + i as u8) as char));
        if i < exact.len() {
            assert_eq!(got, *want, "{f}");
        } else {
            // Each of width, height and depth is the right one or unknown.
            for (g, w) in got.split('/').zip(want.split('/')) {
                assert!(g == w || g.contains("unknown"), "{f}: {got}");
            }
        }
    }
}

/// tex.web §§ 778-793: a preamble is read, not run; past its last
/// template a cell takes the periodic part's (`&&`) again, and `\everycr`
/// follows the preamble's `\cr` and each row's. pgf's matrix checks in
/// each `v` part that the cell's `&` was its own.
#[test]
fn a_periodic_preamble_is_read_and_every_cell_takes_its_template() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{tikz}
\usetikzlibrary{matrix}
\newcount\c \newcount\r
\begin{document}
\setbox0\vbox{\everycr{\noalign{\global\advance\r1 }}%
  \halign{\global\advance\c1 #&&\global\advance\c10 #\cr a&b&c\cr d\cr}}
\xdef\C{\the\c/\the\r}
\begin{tikzpicture}
\matrix (m) [matrix of nodes] {|[alias=a]| A & B \\ C & D \\};
\draw (a) -- (m-2-2);
\end{tikzpicture}
\end{document}",
    );
    assert_eq!(body(&analysis, "C"), "22/3");
    assert!(raised(&analysis).is_empty(), "{:?}", raised(&analysis));
}

/// tex.web § 785: between an alignment's rows a `{` or a `$` begins a row
/// as a letter does, so the group or formula is in the row's first cell, in
/// restricted horizontal mode, not one more pass of a row that may be in
/// vertical mode (which once read every token of `{x} &= 1` in amsmath's
/// `align` as a loop and summarized it). Sizes as `pdflatex` prints them.
#[test]
fn a_brace_or_math_shift_between_rows_begins_a_row() {
    if !installed() {
        return;
    }
    let cases = [
        (r"\vbox{\halign{#\cr {x}=1\cr}}", "18.05562pt/6.44444pt/0.0pt"),
        (r"\vbox{\halign{#\cr $x$\cr}}", "5.71527pt/4.30554pt/0.0pt"),
        (r"\vbox{\halign{#\cr x\cr {y}\cr}}", "5.2778pt/16.30554pt/1.94444pt"),
        (r"\vbox{\halign{#\cr {\count255=1 }x\cr}}", "5.2778pt/4.30554pt/0.0pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0\\hbox{{{f}}}\\xdef\\S{}{{\\the\\wd0/\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\begin{align}\n{x} &= 1, 2\n\\end{align}\n\\end{document}");
    let analysis = analyze(&src);
    for (i, (f, want)) in cases.iter().enumerate() {
        let got = body(&analysis, &format!("S{}", (b'a' + i as u8) as char));
        for (g, w) in got.split('/').zip(want.split('/')) {
            assert!(g == w || g.contains("unknown"), "{f}: {got}");
        }
    }
    let codes = codes(&analysis);
    assert!(!codes.iter().any(|c| c == "recursion-widened"), "{codes:?}");
}

/// tex.web § 342: an `&` or `\cr` that `get_next` reads at the cell's
/// brace level ends the cell where it is read, whoever reads it: a
/// `\futurelet` looking ahead meets the cell's `v` part (§ 789), here its
/// `\kern`. Inside braces or after `\omit` it meets something else.
/// What `pdflatex` records: `vvovo`. (An empty `v` part, where TeX's
/// look meets `\endtemplate`, is not modeled: satex's meets the `\cr`.)
#[test]
fn a_lookahead_at_a_cell_end_meets_its_v_part() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\def\look{\futurelet\next\record}
\def\record{\ifx\next\kern \xdef\seen{\seen v}\else\ifx\next\cr \xdef\seen{\seen c}\else\xdef\seen{\seen o}\fi\fi}
\def\seen{}
\setbox0\vbox{\halign{#\kern1pt\cr a\look\cr}}
\setbox0\vbox{\halign{#\kern1pt&#\cr a\look&b\cr}}
\setbox0\vbox{\halign{#\kern1pt\cr a{\look}\cr}}
\setbox0\vbox{\halign{#\kern1pt\cr \iffalse{\fi a\look\cr\iffalse}\fi}}
\setbox0\vbox{\halign{#\kern1pt&#\cr \omit a\look&b\cr}}
\xdef\result{\seen}
\end{document}",
    );
    assert_eq!(body(&analysis, "result"), "vvovo");
}

/// siunitx collects an `S` cell token by token with `\peek_...` until it
/// meets the `v` part, which TeX inserts where the `\cr` is looked at
/// (tex.web § 342); array's `\\` raises `align_state` with
/// `\iffalse{\fi\ifnum0=`}\fi` (§ 442 leaves a brace read as a code
/// uncounted), so its look past `\\` at an empty first cell ends nothing.
/// Each table is read to its end; sizes as `pdflatex` prints, or unknown.
#[test]
fn siunitx_table_cells_end_at_their_v_part() {
    if !installed() {
        return;
    }
    let cases = [
        (r"\begin{tabular}{S}3\\12\end{tabular}", "34.77786pt/14.5pt/9.5pt"),
        (r"\begin{tabular}{SS}3 & 4.5 \\ & 7\end{tabular}", "49.55566pt/14.5pt/9.5pt"),
        (r"\begin{tabular}{S[table-format=2.0]l}3 & x\\ {y} & z\\\end{tabular}", "39.27786pt/14.5pt/9.5pt"),
        (r"\begin{tabular}{lS}a & 1.25\\ b & \end{tabular}", "52.33345pt/14.5pt/9.5pt"),
        (r"\begin{tabular}{ll}a & b\\ & c\\\end{tabular}", "34.55562pt/14.5pt/9.5pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\usepackage{siunitx}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0\\hbox{{{f}}}\\xdef\\S{}{{\\the\\wd0/\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    let codes = codes(&analysis);
    for code in ["limit-reached", "line-budget-cut", "unbalanced-group", "mismatched-group", "unbalanced-file"] {
        assert!(!codes.iter().any(|c| c == code), "{codes:?}");
    }
    for (i, (f, want)) in cases.iter().enumerate() {
        let got = body(&analysis, &format!("S{}", (b'a' + i as u8) as char));
        for (g, w) in got.split('/').zip(want.split('/')) {
            assert!(g == w || g.contains("unknown"), "{f}: {got}");
        }
    }
}

/// tex.web §§ 1151, 1160: `\mathchar` and `\delimiter` in inline math are
/// one atom each, a delimiter its small variant, so `\langle` and `\{`
/// keep the formula's width. Widths as `pdflatex` prints.
#[test]
fn mathchar_and_delimiter_atoms_have_their_width() {
    if !installed() {
        return;
    }
    let cases = [
        (r#"$\mathchar"141$"#, "7.50002pt"),
        (r#"$\delimiter"426830A$"#, "3.8889pt"),
        (r"$\langle x\rangle$", "13.49307pt"),
        (r"$\{x\}$", "15.7153pt"),
        (r#"$a\delimiter"4162362 b$"#, "13.8692pt"),
        (r"$O_2 = \langle A_1, A_2, A_4, A_3 \rangle$", "94.50273pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!("\\setbox0\\hbox{{{f}}}\\xdef\\W{}{{\\the\\wd0}}\n", (b'a' + i as u8) as char));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    let got: Vec<(String, String)> = cases
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (f.to_string(), body(&analysis, &format!("W{}", (b'a' + i as u8) as char))))
        .collect();
    let want: Vec<(String, String)> = cases.iter().map(|(f, w)| (f.to_string(), w.to_string())).collect();
    assert_eq!(got, want);
}

/// tex.web §§ 866, 879, 881, 1196: a formula without binary or relation
/// atoms or stretchable glue is one box to the line breaker, its end a
/// breakpoint where glue follows, so a paragraph with inline math keeps its
/// lines and their height. Heights and depths as `pdflatex` prints.
#[test]
fn a_paragraph_with_inline_math_keeps_its_height() {
    if !installed() {
        return;
    }
    let cases = [
        (r"\hsize=2cm $x$", "4.30554pt/0.0pt"),
        (r"\hsize=2cm y$x$", "4.30554pt/1.94444pt"),
        (r"\hsize=2cm $\dots$", "1.05554pt/0.0pt"),
        (r"\hsize=2cm \begin{obeylines}$\dots$\end{obeylines}", "1.05554pt/0.0pt"),
        (
            r"\hsize=3cm Some words $x$ and $y_1$ then more words $\alpha\beta$ to wrap around lines $q$",
            "42.94444pt/1.94444pt",
        ),
        (
            r"\hsize=3cm \mathsurround=5pt Some words $x$ and $y$ then more words $z$ to wrap around lines",
            "42.94444pt/0.0pt",
        ),
        (r"\hsize=2cm $xxxxxxxxxxxxxxxxxxxx$ $y$", "16.30554pt/1.94444pt"),
        (r"\hsize=1cm $x$ $y$ $z$ $w$ $v$", "16.30554pt/0.0pt"),
        (r"\hsize=1cm $x$$y$$z$$w$$v$ $u$", "16.30554pt/0.0pt"),
        (r"\hsize=3cm $x^2$ and $a_{ij}$", "8.14003pt/2.86108pt"),
        (r"\hsize=3cm $def_{A_i}=b$", "6.94444pt/2.49998pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0\\vbox{{{f}}}\\xdef\\H{}{{\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    let got: Vec<(String, String)> = cases
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (f.to_string(), body(&analysis, &format!("H{}", (b'a' + i as u8) as char))))
        .collect();
    let want: Vec<(String, String)> = cases.iter().map(|(f, w)| (f.to_string(), w.to_string())).collect();
    assert_eq!(got, want);
}

/// tex.web §§ 719, 756-758: scripts of any length and depth are set in the
/// script and scriptscript fonts, shifted by the parameters of the size
/// they sit in. Widths, heights and depths as `pdflatex` prints.
#[test]
fn inline_math_with_scripts_has_its_size() {
    if !installed() {
        return;
    }
    let cases = [
        (r"x^2", "10.2014pt/8.14003pt/0.0pt"),
        (r"a_{ij}", "12.32874pt/4.30554pt/2.86108pt"),
        (r"x_{A_i}", "15.39703pt/4.30554pt/2.49998pt"),
        (r"def_{A_i}", "24.43875pt/6.94444pt/2.49998pt"),
        (r"def_{A_i}=\{\dots\}", "59.43861pt/7.5pt/2.5pt"),
        (r"a^{b^c}", "13.04968pt/8.79948pt/0.0pt"),
        (r"a_i^2", "9.77202pt/8.14003pt/2.60292pt"),
        (r"x^{2}y", "15.463pt/8.14003pt/1.94444pt"),
        (r"x^{\alpha\beta}", "16.30817pt/8.49002pt/0.0pt"),
        (r"A_{i}^{j}", "11.71358pt/9.43745pt/2.78741pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "\\setbox0\\hbox{{${f}$}}\\xdef\\S{}{{\\the\\wd0/\\the\\ht0/\\the\\dp0}}\n",
            (b'a' + i as u8) as char
        ));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    for (i, (f, want)) in cases.iter().enumerate() {
        assert_eq!(body(&analysis, &format!("S{}", (b'a' + i as u8) as char)), *want, "{f}");
    }
}

/// `\unhbox` of any hbox adds its list's natural width (tex.web § 1110),
/// whatever it was packed to, and an `\unskip` after it takes away the
/// glue the list ended in: tikz's `align` lines in an `\halign`. Widths
/// as `pdflatex` prints.
#[test]
fn an_unhboxed_list_keeps_its_natural_width() {
    if !installed() {
        return;
    }
    let cases = [
        (
            r"\setbox1\hbox{Aa b }\setbox0\vbox{\halign{\hfil#\hfil\cr\hbox{X}\cr\noalign{\vskip0pt}\unhbox1\unskip\cr}}",
            "21.38893pt",
        ),
        (r"\setbox1\hbox{word}\setbox0\hbox{\unhbox1}", "21.4167pt"),
        (r"\setbox1\hbox to 50pt{a\hfil b}\setbox0\hbox{\unhbox1}", "10.55559pt"),
        (r"\setbox1\hbox{Ab }\setbox0\hbox{x\unhbox1\unskip}", "18.33339pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!("{f}\\xdef\\W{}{{\\the\\wd0}}\n", (b'a' + i as u8) as char));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    let got: Vec<(String, String)> = cases
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (f.to_string(), body(&analysis, &format!("W{}", (b'a' + i as u8) as char))))
        .collect();
    let want: Vec<(String, String)> = cases.iter().map(|(f, w)| (f.to_string(), w.to_string())).collect();
    assert_eq!(got, want);
}

/// Glue and kerns in inline math, `\mskip` and `\mkern` in math units
/// (tex.web §§ 716-717, 732), are no atoms: the spacing passes over them
/// (§ 761) and the character before takes its italic correction (§ 755).
/// amsmath's `\dots` ends in `\,`. Widths as `pdflatex` prints.
#[test]
fn math_glue_and_kerns_have_their_width() {
    if !installed() {
        return;
    }
    let cases = [
        (r"$a\,b$", "11.24417pt"),
        (r"$a\mkern3mu b$", "11.24417pt"),
        (r"$f\kern1pt b$", "11.26392pt"),
        (r"$f\mskip 6mu plus 2mu b$", "13.59717pt"),
        (r"$(N_1,N_2,\dots)$", "55.04155pt"),
        (r"$a\!+b$", "20.13306pt"),
    ];
    let mut src = String::from("\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n");
    for (i, (f, _)) in cases.iter().enumerate() {
        src.push_str(&format!("\\setbox0\\hbox{{{f}}}\\xdef\\W{}{{\\the\\wd0}}\n", (b'a' + i as u8) as char));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    let got: Vec<(String, String)> = cases
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (f.to_string(), body(&analysis, &format!("W{}", (b'a' + i as u8) as char))))
        .collect();
    let want: Vec<(String, String)> = cases.iter().map(|(f, w)| (f.to_string(), w.to_string())).collect();
    assert_eq!(got, want);
}

/// One finding per source site: one use of a macro that writes three
/// labels is one unused-label there, not three.
#[test]
fn a_label_in_a_macro_body_is_one_finding() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newcommand\cell[1]{\section{#1}\label{nb:#1}}
\newcommand\three{\cell{A}\cell{B}\cell{R}}
\begin{document}
\three
\end{document}",
    );
    assert_eq!(codes(&analysis).iter().filter(|c| *c == "unused-label").count(), 1);
}

/// `satex lint` text lists correctness before style, grouped by rule, not
/// in file order.
#[test]
fn lint_text_puts_correctness_first_grouped_by_rule() {
    let record = |code: &str, category: &str, line: u64| {
        let mut r = serde_json::Map::new();
        for (k, v) in
            [("code", code), ("category", category), ("severity", "warning"), ("file", "a.tex"), ("message", "m")]
        {
            r.insert(k.into(), serde_json::json!(v));
        }
        r.insert("line".into(), serde_json::json!(line));
        r.insert("col".into(), serde_json::json!(1));
        r
    };
    let records = vec![
        record("zstyle", "style", 1),
        record("bug", "correctness", 9),
        record("astyle", "style", 2),
        record("bug", "correctness", 3),
    ];
    let text = satex::render::lint(&records, satex::render::Links::default(), true);
    let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("{s} missing in {text}"));
    assert!(at("bug") < at("astyle") && at("astyle") < at("zstyle"), "{text}");
}

/// `\beamer@modecount` is back at 0 after every frame (`\beamer@savemode`
/// at its start, `\beamer@resumemode` at its end, beamerbaseframe.sty), as
/// `pdflatex` says here: a width only known as an interval in a TikZ node
/// inside an overlay's colour box must not leave a path that skipped the
/// frame's end decide the count. It is 0 or unknown, never another number.
#[test]
fn the_beamer_mode_count_is_back_after_a_frame_with_an_unknown_width() {
    if !installed() {
        return;
    }
    let source = r"\documentclass{beamer}
\usepackage{tikz}
\begin{document}
\begin{frame}{T}
\uncover<2->{%
\begin{beamercolorbox}[wd=\textwidth,sep=5pt,rounded=true,shadow=true]{block body}
\tikz\node[text width=3cm,text height=2cm]{\rule{\pdfuniformdeviate 100 pt}{1cm}};
\end{beamercolorbox}%
}
\end{frame}
\makeatletter\xdef\Pmc{\the\beamer@modecount}\makeatother
\gdef\SatexReachedEnd{}
\end{document}
";
    let analysis = analyze(source);
    assert!(defined(&analysis, "SatexReachedEnd"));
    let count = last_body(&analysis, "Pmc");
    assert!(count == "0" || count.contains("unknown"), "{count:?}");
}

/// A number of unknown sign parsed by pgfmath: the path that read a unary
/// minus and the one that did not are kept apart where their loops meet,
/// so neither loop is cut and no operation of one stands on the other's
/// stack (pdflatex: no error).
#[test]
fn pgfmath_keeps_the_signs_of_an_unknown_number_apart() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{pgfmath}
\newdimen\da
\begin{document}
\da=-\pdfuniformdeviate 100 pt
\pgfmathparse{\the\da*2}\xdef\R{\pgfmathresult}
\end{document}",
    );
    let count = |code: &str| analysis.facts.diagnostics.iter().filter(|d| d.code == code).count();
    assert_eq!(count("recursion-widened"), 0, "{:?}", codes(&analysis));
    assert!(count("undecided-loop") <= 2, "{:?}", codes(&analysis));
}
