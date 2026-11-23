//! LaTeX2e as documented: what the kernel's own interfaces do.

use crate::support::analyze_latex as analyze;
use crate::support::{assert_body, broken_gaps, codes, marks, maybe_defined, undefined};
use crate::support::{defined, installed, last_body as body};
use satex::analysis::machine::{Analysis, Machine};
use satex::engine::builtins::OccKind;

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
    let mut found: Vec<String> = satex::linter::lint(&analysis)
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
            "error `\\undefinedbaz` is never defined in this run",
            "info `\\undefinedfoo` is undefined on a path the analysis approximated"
        ]
    );
}

/// A misspelled command suggests the defined name nearest to it: a swapped
/// pair of letters, a letter too many, a different case.
#[test]
fn a_misspelled_command_suggests_the_nearest_name() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newcommand\mymacro{x}
\begin{document}
\sectoin{A} \mymacor \Textbf{b} \qwertzuiop
\end{document}",
    );
    let mut found: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("undefined-control-sequence"))
        .map(|f| format!("{} {}", f["message"].as_str().unwrap_or_default(), f["fix"].as_str().unwrap_or_default()))
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            "`\\Textbf` is never defined in this run did you mean `\\textbf`?",
            "`\\mymacor` is never defined in this run did you mean `\\mymacro`?",
            "`\\qwertzuiop` is never defined in this run define `\\qwertzuiop` or load the package that provides it",
            "`\\sectoin` is never defined in this run did you mean `\\section`?",
        ]
    );
}

/// expl3's `\int_mod:nn` of an unknown number that may be 0 stays below the
/// divisor: on the path where the test found it 0, `x - (0/d)*d` is 0.
#[test]
fn a_remainder_of_an_unknown_stays_below_its_divisor() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\begin{document}
\tl_set:Ne \l_tmpa_tl {\int_eval:n{\int_rand:n{65535}/16}}
\int_compare:nNnTF {\int_mod:nn{\l_tmpa_tl}{16}} < {16} {in}{out}
\end{document}",
    );
    let rows = satex::queries::query::run(
        &analysis,
        satex::queries::query::Query::Typeset,
        &satex::queries::query::Filter::Always,
    );
    let texts: Vec<&str> = rows.iter().filter_map(|r| r.get("detail").and_then(|d| d.as_str())).collect();
    assert!(texts.contains(&"in"), "{texts:?}");
}

/// An arrow tip a TikZ library declares, used without it: the error names
/// the library that declares it and the fix loads it.
#[test]
fn an_unknown_arrow_tip_names_its_library() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{tikz}
\begin{document}
\tikz\draw[-Kite] (0,0) -- (1,1);
\end{document}",
    );
    let found: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| f.get("message").and_then(|m| m.as_str()).is_some_and(|m| m.contains("'Kite'")))
        .map(|f| f["fix"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(found, ["add `\\usetikzlibrary{arrows.meta}`, which declares it"]);
}

/// Passes of the loop that the line budget cuts.
const SPIN_PASSES: u64 = 5_000_000;

/// A name read with `{` a letter is TeX's error where the category codes
/// are known, but not after a cut: the run rests on the cut from there
/// (`Diagnostic::rest`), so the name is reported as possible only, with the
/// gap it may follow from. Bug pinned: the rule looked for a
/// `region-catcodes` row alone, and a cut that raised none left a certain error. The budget is
/// the plain run's cost, so the cut falls in the loop however much
/// `\begin{document}` costs (a fixed 400000 began to cut `\begin{document}`).
#[test]
fn a_name_read_under_assumed_category_codes_is_no_certain_error() {
    if !installed() {
        return;
    }
    let severity = |spin: &str, line_steps: u64| {
        let source = format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n\\def\\spin{{\\advance\\count2 1 \\ifnum\\count2<{SPIN_PASSES} \\expandafter\\spin\\fi}}\n{spin}\n\\catcode`\\{{=11 \\relax\\x{{y \\catcode`\\{{=1\n\\end{{document}}\n"
        );
        let mut cfg = crate::support::config();
        cfg.limits.line_steps = line_steps;
        let analysis = Machine::analyze(&source, None, &cfg);
        satex::linter::lint(&analysis)
            .iter()
            .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("undefined-control-sequence"))
            .map(|f| {
                let get = |k: &str| f.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
                format!("{} {}", get("severity"), get("message"))
            })
            .filter(|f| f.contains("{y"))
            .collect::<Vec<_>>()
    };
    assert_eq!(severity("", 0), ["error `\\x{y` is never defined in this run"]);
    // The budget: what the whole run without the loop takes, so only the loop's line is cut,
    // whatever `\begin{document}` costs. The loop takes several steps per pass.
    let plain = {
        let source = "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";
        let a = Machine::analyze(source, None, &crate::support::config());
        a.steps - a.load_steps + 1
    };
    assert!(plain < SPIN_PASSES, "the loop's line must cost more than the plain run: {plain}");
    let assumed = severity("\\begingroup\\catcode`\\{=11 \\spin\\endgroup", plain);
    assert_eq!(assumed.len(), 1, "{assumed:?}");
    assert!(
        assumed[0].starts_with("info `\\x{y` is never defined in this run (may follow from the analysis gap at"),
        "{assumed:?}"
    );
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
    let found: Vec<(String, String)> = satex::linter::lint(&analysis)
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
    // Without hyperref the text is what LaTeX writes it as
    // (`\protected@edef`): a protected command stays a command, no text.
    (
        "a_protected_command_in_the_date_stays_a_command",
        r"\documentclass{article}
\makeatletter
\protected\def\my@mark{M}
\date{September\my@mark 22, 2026}
\makeatother
\begin{document}
\title{T}\author{A}\maketitle
\end{document}",
        &[("date", "September22, 2026")],
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

/// tex.web § 338: a definition that runs to the end of the main file is
/// TeX's fatal "File ended while scanning definition of \rep". Bug pinned:
/// satex read the body on as text and said only that the run was
/// incomplete, with no error. With no approximation on the way the error is
/// certain.
#[test]
fn a_definition_open_at_the_end_of_the_file_is_an_error() {
    if !installed() {
        return;
    }
    let analysis = analyze("\\documentclass{article}\n\\begin{document}\n\\def\\rep{x\ny\n\\end{document}\n");
    let errors: Vec<_> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|f| f.get("severity").and_then(|s| s.as_str()) == Some("error"))
        .filter_map(|f| f.get("message")?.as_str().map(str::to_string))
        .collect();
    assert!(errors.iter().any(|m| m.contains("ile ended while scanning definition")), "{errors:?}");
}

/// tex.web §§ 1240, 1236: `\multiply` of a dimension past `max_answer` and
/// `\divide` by zero are "Arithmetic overflow". With `\count1` one of 0 and 1
/// some runs err: pdflatex errs for one seed and not the other. Bug pinned:
/// the interval arithmetic joins the erring members in silently, so the run
/// is "complete" with no possible error. Each line must give one.
#[test]
fn an_overflow_on_some_members_of_an_interval_is_a_possible_error() {
    if !installed() {
        return;
    }
    for line in [r"\dimen0=\count1 pt \multiply\dimen0 by 16384", r"\count2=10 \divide\count2 by \count1"] {
        let source = format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n\\count1=\\pdfuniformdeviate 2\\relax\n{line}\n\\end{{document}}\n"
        );
        let found: Vec<_> = satex::linter::lint(&analyze(&source))
            .into_iter()
            .filter_map(|f| f.get("message")?.as_str().map(str::to_string))
            .collect();
        assert!(found.iter().any(|m| m.contains("Arithmetic overflow")), "{line}: {found:?}");
    }
}

/// `round` of an unknown quotient in l3fp: on an approximated path the expandable error's
/// `\???` scan runs off past `\end{document}`. Bug pinned: the run then said `missing-end`, a
/// certain error, though pdflatex ends the document. An end reached past a gap the run left is
/// the analysis' end, said as `analysis-ended-early`, never a certain error.
#[test]
fn an_approximated_fp_round_is_no_missing_end() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\ExplSyntaxOn
\fp_eval:n { round ( \pdfuniformdeviate 300 / 7 ) }
\ExplSyntaxOff
\end{document}",
    );
    let found = codes(&analysis);
    assert!(!found.iter().any(|c| c == "missing-end"), "{found:?}");
}

/// satex renders `\title` as a PDF string itself, which no TeX run does.
/// algorithm2e's `\nl` there steps a counter under hyperref's `\let`, and
/// the rendering recurses until cut. None of that reaches the run: the
/// later `\stepcounter` is the kernel's, no limit is hit, the end is read.
#[test]
fn rendering_the_title_leaves_the_run_as_it_was() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{hyperref}
\usepackage{algorithm2e}
\title{A\nl B}
\newcounter{other}
\stepcounter{other}
\begin{document}
x
\end{document}",
    );
    let found = codes(&analysis);
    assert!(!found.iter().any(|c| c == "missing-end"), "{found:?}");
    assert!(analysis.limits.is_empty(), "{:?}", broken_gaps(&analysis));
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
            .is_some_and(|sym| !matches!(analysis.env.meaning(sym), satex::engine::tex::Meaning::Undefined))
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
        .filter(|o| o.kind == satex::engine::builtins::OccKind::Label)
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
    let findings = satex::linter::lint(&analysis);
    let code_of = |record: &satex::queries::query::Record| {
        record.get("code").and_then(|c| c.as_str()).unwrap_or_default().to_string()
    };
    for code in ["undefined-control-sequence", "undefined-reference"] {
        let from_document = |record: &satex::queries::query::Record| {
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
    satex::linter::lint(analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("raised-error"))
        .filter_map(|f| f.get("severity").and_then(|s| s.as_str()).map(str::to_string))
        .collect()
}

/// tex.web § 1080: `\lastbox` after `\unvbox` takes the box a vbox ended
/// with, here a plain `\hbox`. Boxes on a list were the same item as marks
/// and whatsits, read back as no box: `\lastbox` was void and the document's
/// `\errmessage` for that case was raised although pdflatex takes the hbox.
#[test]
fn lastbox_after_unvbox_takes_a_plain_box() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}\begin{document}
\setbox0\vbox{\hbox{a}}\setbox2\vbox{\unvbox0 \global\setbox1\lastbox}
\ifvoid1 \errmessage{void}\fi \xdef\Sa{\ifvoid1 V\else B\fi}
\end{document}",
    );
    assert_eq!(body(&analysis, "Sa"), "B");
    assert!(raised(&analysis).is_empty(), "{:?}", raised(&analysis));
}

/// A program the shell runs may write files its command line does not
/// name: minted runs `latexminted` (allowed under restricted shell escape)
/// and reads back the configuration it wrote. That file may exist from then
/// on, its text unknown. Before, it was absent for certain and minted's
/// "executable is not installed" error was raised on a document pdflatex
/// compiles.
#[test]
fn a_file_a_shell_program_may_write_may_exist() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\usepackage{minted}\n\\begin{document}\n\\begin{minted}{python}\nx = 1\n\\end{minted}\n\\end{document}\n",
    );
    let errors: Vec<_> = satex::linter::lint(&analysis).into_iter().filter(|f| f["severity"] == "error").collect();
    assert!(errors.is_empty(), "{errors:?}");
}

/// LaTeX stops on a package it cannot find and on `\usepackage` after
/// `\begin{document}`: with the installation's file list known each is one
/// error finding. Before, a missing package was a warning, and a late
/// `\usepackage` gave a warning beside TeX's raised error.
#[test]
fn a_missing_package_and_a_late_usepackage_are_one_error_each() {
    if !installed() {
        return;
    }
    let found = |src: &str, line: u64| {
        let analysis = analyze(src);
        satex::linter::lint(&analysis)
            .into_iter()
            .filter(|f| f["line"] == line && matches!(f["severity"].as_str(), Some("error" | "warning")))
            .map(|f| {
                format!("{} {}", f["severity"].as_str().unwrap_or_default(), f["code"].as_str().unwrap_or_default())
            })
            .collect::<Vec<_>>()
    };
    let missing =
        found("\\documentclass{article}\n\\usepackage{nosuchpackagehere}\n\\begin{document}\nx\n\\end{document}\n", 2);
    assert_eq!(missing, ["error unresolved-file"]);
    let late = found("\\documentclass{article}\n\\begin{document}\n\\usepackage{amsmath}\n\\end{document}\n", 3);
    // `satex lint` says package-after-preamble, an in-memory source TeX's own error
    assert!(late.len() == 1 && late[0].starts_with("error "), "{late:?}");
}

/// Each TeX error the document's own line raises is an error finding, as
/// pdflatex stops on each (its first `!` line in the comment). Pins errors
/// lost two ways: one the engine raised under satex's own code (`A <box>
/// was supposed to be here`, `Missing { inserted`, `Extra \endgroup`,
/// `Missing \right. inserted`, `Parameters must be numbered consecutively`,
/// `Bad character code`, `Misplaced \noalign`) that no rule reported, and a
/// raised error hidden by a style or prose note at the same place
/// (`\count` is plain TeX, `\a` is never used).
#[test]
fn every_tex_error_of_the_document_is_an_error_finding() {
    if !installed() {
        return;
    }
    let cases = [
        r"\count1=x",                             // Missing number, treated as zero.
        r"\hskip x",                              // Missing number, treated as zero.
        r"\setbox0=x",                            // A <box> was supposed to be here.
        r"\hbox x",                               // Missing { inserted.
        r"\endgroup",                             // Extra \endgroup.
        r"$\left($",                              // Missing \right. inserted.
        r"\def\a#2{}",                            // Parameters must be numbered consecutively.
        r"\char300",                              // Bad character code (300).
        r"\hline",                                // Misplaced \noalign.
        r"\begin{itemize}\item a\end{enumerate}", // \begin{itemize} ... ended by \end{enumerate}.
    ];
    for body in cases {
        let analysis =
            analyze(&format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"));
        let errors: Vec<_> = satex::linter::lint(&analysis)
            .into_iter()
            .filter(|f| f["severity"] == "error" && f["line"] == 3)
            .map(|f| f["code"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(!errors.is_empty(), "{body}: no error, {:?}", codes(&analysis));
    }
}

/// A script with nothing after it before the math ends: `\(\sum_ \)` makes
/// pdflatex say `Missing { inserted` and `Missing } inserted` on that line
/// (the `\)` comes as the subscript). A person typing a formula in the
/// playground hits it first thing; satex reported nothing there (userrun
/// e2e u-sum-sub-errors).
#[test]
fn a_script_with_nothing_after_it_is_an_error_on_its_line() {
    if !installed() {
        return;
    }
    for body in [r"Sum: \(\sum_ \)", r"Power: $x^$"] {
        let analysis =
            analyze(&format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"));
        let errors: Vec<_> = satex::linter::lint(&analysis)
            .into_iter()
            .filter(|f| f["severity"] == "error" && f["line"] == 3)
            .map(|f| f["code"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(!errors.is_empty(), "{body}: no error on its line, {:?}", codes(&analysis));
    }
}

/// A blank line inside display math (what an empty `equation` template
/// holds) is a `\par` in math mode: pdflatex says `Missing $ inserted` on
/// the blank line itself. satex reports it (math-still-open) on the line
/// after, so the squiggle is not where TeX stops (userrun e2e
/// u-par-in-math-covered).
#[test]
fn a_blank_line_in_display_math_is_an_error_on_the_blank_line() {
    if !installed() {
        return;
    }
    let head = "\\documentclass{article}\n\\begin{document}\n\\begin{equation}\n";
    let blank = head.lines().count() as u64 + 1;
    let analysis = analyze(&format!("{head}  \n\\end{{equation}}\n\\end{{document}}\n"));
    let lines: Vec<_> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|f| f["severity"] == "error")
        .filter_map(|f| f["line"].as_u64())
        .collect();
    assert!(lines.contains(&blank), "errors on lines {lines:?}, want one on {blank}");
}

/// tex.web §§ 785, 1129: only `align_peek`, right after a `\cr`, takes a
/// `\noalign`; anywhere else it is "Misplaced \noalign", and so is an
/// `\omit` past a cell's start. A row missing its `\\` puts the
/// `\bottomrule` (or `\hline`) in its last cell. satex opened the rule's
/// material as if between rows and stayed silent; pdflatex stops there.
/// A `\noalign` right after a `\\` stays silent.
#[test]
fn a_noalign_inside_a_cell_is_misplaced() {
    if !installed() {
        return;
    }
    let head = "\\documentclass{article}\n\\usepackage{booktabs}\n\\begin{document}\n";
    let body_line = head.lines().count() as u64 + 1;
    let lines = |body: &str| {
        let analysis = analyze(&format!("{head}{body}\n\\end{{document}}\n"));
        let mut found: Vec<_> = satex::linter::lint(&analysis)
            .into_iter()
            .filter(|f| f["severity"] == "error")
            .filter_map(|f| f["line"].as_u64())
            .collect();
        found.dedup();
        found
    };
    let table = |rule: &str, end: &str| {
        format!("\\begin{{tabular}}{{llr}}\n\\toprule\na & b & c \\\\\ne & hallo{end}\n{rule}\n\\end{{tabular}}")
    };
    for rule in [r"\bottomrule", r"\hline", r"\noalign{}", r"\cline{1-2}"] {
        let at = body_line + table(rule, "").lines().position(|l| l == rule).unwrap() as u64;
        assert_eq!(lines(&table(rule, "")).first(), Some(&at), "{rule}: no error at the rule");
        let after = lines(&table(rule, r" \\"));
        assert!(after.is_empty(), "{rule} after a row: {after:?}");
    }
    let omit = lines(r"\begin{tabular}{l}a\omit\end{tabular}");
    assert_eq!(omit.first(), Some(&body_line), "\\omit in a cell");
    let start = lines(r"\begin{tabular}{l}\omit a\end{tabular}");
    assert!(start.is_empty(), "\\omit at a cell's start: {start:?}");
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
    let mut names: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("unused-definition"))
        .filter(|f| f.get("origin").and_then(|o| o.as_str()) == Some("document"))
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect();
    names.sort();
    assert_eq!(names, [r"\fd", r"\fe"]);
}

/// A `\csname` name the run could not decide (`\romannumeral` of a random
/// count) is a fresh name, never written on the line: no "`\` is never used"
/// with a fix that deletes a backslash (audit n8).
#[test]
fn a_name_the_run_could_not_decide_is_not_an_unused_definition() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\count1=\pdfuniformdeviate 12 \expandafter\def\csname\romannumeral\count1\endcsname{}
\begin{document}x\end{document}",
    );
    let unused: Vec<_> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("unused-definition"))
        .collect();
    assert!(unused.is_empty(), "{unused:?}");
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
    let mut names: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| matches!(f.get("code").and_then(|c| c.as_str()), Some("unused-definition" | "dead-definition")))
        .filter(|f| f.get("origin").and_then(|o| o.as_str()) == Some("document"))
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect();
    names.sort();
    assert_eq!(names, [r"\lonelier", r"\lonely"]);
}

/// `\meaning` reads the definition it prints: `\@onelevel@sanitize\host` is
/// `\edef\host{\expandafter\strip@prefix\meaning\host}`, so the `\def` before it is used and not
/// dead (hyperref's `\Hy@StringLocalhost`). Nor is a name declared empty and then defined:
/// `\newcommand\x{}` checks that the name is free before a `\def` with delimiters (acronym,
/// natbib, amsmath), and expl3 wants `\tl_new:N` before `\tl_set:Nn` (siunitx, tabularray).
/// A definition replaced with no read in between still is.
#[test]
fn a_definition_meaning_prints_is_read() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\makeatletter
\def\host{localhost}
\@onelevel@sanitize\host
\def\dead{a}
\def\dead{b}
\newcommand\declared{}
\def\declared#1[#2]{#1#2}
\ExplSyntaxOn
\tl_new:N \l_demo_tl
\tl_set:Nn \l_demo_tl { x }
\ExplSyntaxOff
\makeatother
\begin{document}
\host\dead\declared a[b]
\end{document}",
    );
    let names: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| f["code"] == "dead-definition" && f["origin"] == "document")
        .filter_map(|f| f["name"].as_str().map(str::to_string))
        .collect();
    assert_eq!(names, [r"\dead"]);
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
    let places: Vec<(u64, u64)> = satex::linter::lint(&analysis)
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
    let gaps: Vec<_> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::analysis::facts::Severity::Unsupported)
        .collect();
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
    assert!(undefined(&analysis, "bad"));
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
    assert!(maybe_defined(&analysis, "exact"));
    // `\nonzero` is undefined on every path, so every path set `\zero`
    assert!(defined(&analysis, "zero"));
    assert!(undefined(&analysis, "nonzero"));
}

/// A penalty in inline math is no atom and has no width (tex.web § 730),
/// so the formula keeps its size: siunitx 3.6 puts `\nobreak` between a
/// quantity's number and unit inside math, and an unknown width there sent
/// pgfmath parsing a node's anchors over unknown text. The expectations are
/// what pdflatex printed for the same source.
#[test]
fn a_penalty_in_inline_math_keeps_its_size() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\def\m#1{\setbox0\hbox{#1}\typeout{mark \the\wd0}}
\m{$1\nobreak\,m$}\m{$1\penalty10000 m$}\m{$\nobreak$}\m{$a\penalty0 +b$}\m{$a+\penalty-50 b$}
\end{document}",
    );
    assert_eq!(
        marks(&analysis),
        ["mark 15.44678pt", "mark 13.78015pt", "mark 0.0pt", "mark 21.79968pt", "mark 21.79968pt"]
    );
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
\m{\(x^{0}\)}\m{\(\textrm{toolX}^0\)}\m{\(x^{\color{gray}0}\)}\m{\(\textrm{toolX}^{\color{gray}0}\)}
\m{\(\textrm{toolX}_0\)}\m{\(x_{0}\)}\m{\(x^{0}y\)}\m{\(x^0y\)}
\end{document}",
    );
    assert_eq!(
        marks(&analysis),
        [
            "mark 10.2014pt,8.14003pt,0.0pt",
            "mark 28.93065pt,8.98335pt,0.0pt",
            "mark 10.2014pt,8.14003pt,0.0pt",
            "mark 28.93065pt,8.98335pt,0.0pt",
            "mark 28.93065pt,6.94444pt,1.49998pt",
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
        .filter(|d| d.severity == satex::analysis::facts::Severity::Unsupported)
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
        .map(|m| satex::engine::tex::detokenize(&m.replacement_text, &analysis.interner))
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

/// `exp`, `ln` and `^` divide by an integer block by block: each block's
/// `\__fp_fixed_div_int_pack:Nw` runs as one `\numexpr` of a nest closes,
/// with the same arguments when the digits repeat (`1/10`) and the last
/// block's rest left unread under it.  That looked like a recursion stacking
/// itself deeper each round, so the call was cut and every such value came
/// back unknown, then derailed the parser (tikz-sankey's layout ran 100M
/// steps a line). The scans unwind, so the chain ends. `pdflatex`:
/// `[2.718281828459045][0.6931471805599453][8][0.512][1000{0000}…]`.
#[test]
fn exponentials_divide_by_repeating_digits() {
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
\myshow:n { exp(1) }
\myshow:n { ln(2) }
\myshow:n { 2 ^ 3 }
\myshow:n { .8 ^ 3 }
\tl_put_right:Ne \l_my_tl
  { [ \__fp_fixed_div_int:wwN {10000}{0000}{0000}{0000}{0000}{0000} \__fp_sep: 10 \__fp_sep: \use:n ] }
\ExplSyntaxOff
x
\end{document}",
    );
    assert_eq!(unsupported(&analysis), Vec::<String>::new());
    assert_eq!(
        last_body(&analysis, "l_my_tl"),
        "[2.718281828459045][0.6931471805599453][8][0.512][1000{0000}{0000}{0000}{0000}{0000}\\__fp_sep: ]"
    );
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
    use satex::analysis::facts::Severity;
    use satex::analysis::machine::{Analysis, Machine};
    use satex::project::config::Config;

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
    fn stays_bounded(options: &str, steps: u64) {
        // the budget is the work a warm run executes: the first run fills the package cache, its own
        let name: String = options.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        let cfg = crate::support::warm_config(&format!("metadata-{name}"));
        analyze_with(options, &cfg);
        let analysis = analyze_with(options, &cfg);
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
        // out of the 900 million the default budget allows. Steps served from
        // the package cache are not executed, so they are not counted.
        let work = analysis.steps - analysis.served_steps;
        assert!(work < steps, "{work} steps executed ({} in all) for `{options}`", analysis.steps);
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
        let analysis = crate::support::analyze_latex(source);
        let exhausted: Vec<_> =
            unsupported(&analysis).into_iter().filter(|&(code, _)| code == "budget-exhausted").collect();
        assert_eq!(exhausted, Vec::new());
        assert!(!analysis.exhausted);
    }

    /// The options real documents carry: `pdfversion` picks the PDF the
    /// backend writes, `testphase` loads the latex-lab code, `tagging`
    /// turns the structure tree on.
    #[test]
    fn the_common_options_stay_bounded() {
        if !super::installed() {
            return;
        }
        stays_bounded("pdfversion=2.0", BOUNDED_STEPS);
        stays_bounded("lang=en,pdfversion=1.7,testphase={phase-III}", BOUNDED_STEPS);
        stays_bounded("tagging=on", BOUNDED_STEPS);
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
        // both runs get the default budget, so only the bound differs; what the run costs is
        // the_common_options_stay_bounded's to check
        let (unbounded, bounded) = (analyze_with(OPTIONS, &lifted), analyze_document(OPTIONS));
        assert!(worst(&unbounded) <= bound(), "without the bound one gap was counted {} times", worst(&unbounded));
        // the bound adds its own `repeated-failure` notes; every gap besides them is the same
        fn gaps(a: &Analysis) -> Vec<(&str, u32)> {
            unsupported(a).into_iter().filter(|&(code, _)| code != "repeated-failure").collect()
        }
        assert_eq!(gaps(&unbounded), gaps(&bounded), "lifting the bound changed the report");
    }
}

/// The gaps of a run: what it could not follow.
fn gaps_of(analysis: &satex::analysis::machine::Analysis) -> Vec<(&str, String)> {
    let unsupported =
        |d: &&satex::analysis::facts::Diagnostic| d.severity == satex::analysis::facts::Severity::Unsupported;
    analysis.facts.diagnostics.iter().filter(unsupported).map(|d| (d.code, d.message.clone())).collect()
}

/// A character of unknown text under `` ` `` has some code. It is no
/// improper constant: that made it 48 with the name read again, and
/// `\numexpr` ended there and left its `+1\relax` as text.
#[test]
fn the_code_of_a_character_of_unknown_text_is_unknown() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\edef\\u{\\romannumeral\\pdfuniformdeviate 100000}\n\\edef\\v{\\expandafter\\the\\expandafter\\numexpr\\expandafter`\\u+1\\relax}\n\\begin{document}\nx\n\\end{document}\n",
    );
    assert_eq!(gaps_of(&analysis), Vec::new());
    assert!(body(&analysis, "v").contains("unknown digits"), "{}", body(&analysis, "v"));
}

/// tagpdf's namespace id in small: a string of unknown hex digits run
/// through `\str_set_convert:Nnnn`, whose loops read it a character at a
/// time and are summarized. What each step can only over-approximate
/// must not become an error: a widened `\if` in `\int_div_truncate:nn`
/// left the operand of its `/` unknown, not missing, and a `\fi:` in the
/// body scan a cut gave up closes nothing the scan did not open: the
/// `\if_meaning:w` around `\__kernel_tl_gset:Nx` stays for its own `\fi:`.
#[test]
fn converting_a_string_of_unknown_hex_digits_leaves_no_gap() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\ExplSyntaxOn\n\\str_const:Ne \\c_x_str { \\int_to_Hex:n{\\int_rand:n {65535}} }\n\\exp_args:NNe \\str_set_convert:Nnnn \\l_tmpa_str {\\c_x_str}{default}{utf8/string}\n\\ExplSyntaxOff\n\\begin{document}\nx\n\\end{document}\n",
    );
    assert_eq!(gaps_of(&analysis), Vec::new());
}

/// The separators `\__fp_randint_split_o:Nw` writes stand in the arms of
/// its `\if_meaning:w`, inside the `\int_value:w` scan they end. Past the
/// site's budget that `\if` was skipped, and with it the separators:
/// `\__fp_randint_wide_aux:w` ran away and the number was `1\par`. What
/// code in the arms leaves now follows the value as unknown text.
#[test]
fn a_widened_if_in_a_number_scan_leaves_text_after_the_number() {
    if !installed() {
        return;
    }
    let mut cfg = crate::support::config();
    cfg.limits.site_forks = 1;
    let source = "\\documentclass{article}\n\\ExplSyntaxOn\n\\tl_gset:Ne \\g_a_tl { \\int_rand:n {16777215} }\n\\ExplSyntaxOff\n\\begin{document}\nx\n\\end{document}\n";
    let analysis = satex::analysis::machine::Machine::analyze(source, None, &cfg);
    assert_eq!(gaps_of(&analysis), Vec::new());
    assert!(body(&analysis, "g_a_tl").contains("unknown digits"), "{}", body(&analysis, "g_a_tl"));
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
    let bound = satex::project::config::Config::default().limits.repeat_failures;
    let over: Vec<_> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == satex::analysis::facts::Severity::Unsupported && d.code != "repeated-failure")
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
        .filter(|d| d.severity == satex::analysis::facts::Severity::Unsupported)
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
        .filter(|d| d.severity == satex::analysis::facts::Severity::Unsupported)
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
        .filter(|d| d.severity == satex::analysis::facts::Severity::Unsupported)
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
        .filter(|d| d.severity == satex::analysis::facts::Severity::Unsupported && d.span.file == analysis.main_file)
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
{\ifcase\pdfuniformdeviate 3 \aftergroup\one\or\aftergroup\two\else\aftergroup\three\fi}
\label{after}
\end{document}",
    );
    let bodies: Vec<String> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| analysis.interner.name(d.name) == "seen")
        .filter_map(|d| d.mac.as_ref().map(|m| satex::engine::tex::detokenize(&m.replacement_text, &analysis.interner)))
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
    let undefined: Vec<String> = satex::linter::lint(&analysis)
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
    cfg.clock = satex::project::config::Clock::Unknown;
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
    assert!(year.chars().all(|c| c == satex::engine::tex::UNKNOWN_DIGIT), "{year:?}");
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
}
\newsavebox\DataFlow
\setbox\DataFlow=\hbox{\resizebox*!{1.25cm}{\begin{tikzpicture}
   \node[d] (0) at (0,0) {};
   \node[u,right=4mm] (1) at (0.east) {};
   \node[v,above=4mm] (2) at (0.north) {~~~~};
   \node[u,below=4mm] (3) at (0.south) {};
   \draw[Link,-Kite] (3) -- (0);
   \draw[Link,-Kite] (0) -- (1);
   \draw[Link,dotted] (1) to[bend right] (2);
\end{tikzpicture}}}
\newsavebox\ControlFlow
\setbox\ControlFlow=\hbox{\resizebox*!{1.25cm}{\color{lightgray!80!gray}\begin{tikzpicture}[Link/.style={draw,line cap=round,gray,thick},comm/.append style={very thick}]
\node[d] (0) at (0,0) {};
\end{tikzpicture}}}
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

/// `\GPT@NormalizeNumber` of an unknown width: splits off the fraction,
/// reverses it, zaps zeros and reverses it again. Real TeX ends each
/// scan at its `\@nil`. Bug: a wide rest after `\GPT@ZapMinusMinus`, then
/// a copy of a rest tested non-empty still read empty, so `\GPT@Reverse`
/// took `\@nil` for a digit and ran into the end of the `\edef`.
#[test]
fn normalizing_an_unknown_width_ends_every_scan() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}\usepackage{graphicx}\makeatletter\begin{document}
\setbox0\hbox{\the\pdfrandomseed}\edef\x{\strip@pt\wd0}\GPT@NormalizeNumber\x
x\label{after}\end{document}",
    );
    let count = |code: &str| analysis.facts.diagnostics.iter().filter(|d| d.code == code).count();
    assert_eq!(count("argument-ends-in-unknown"), 0, "{:?}", analysis.facts.diagnostics);
    assert_eq!(keys(&analysis, OccKind::Label), vec!["after".to_string()]);
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
    use satex::project::config::ShellEscape;
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

/// A kept call effect replays a run as the code around it saw it: its
/// reads reach the logs of the enclosing records as the run gave them,
/// no more. Pins a replay of xcolor's `\XC@info` (in beamer's frame
/// colours) that logged every name its run read, also the ones the run
/// read without logging them, among them unknown ones. An enclosing site
/// record then looked like a run over unknown operands, its call was
/// summarized by its writes, and the frame body never ran.
#[test]
fn a_replayed_call_logs_only_the_reads_its_run_logged() {
    if !installed() {
        return;
    }
    let mut cfg = crate::support::config();
    cfg.limits.call_memo_steps = 200;
    let source = r"\documentclass{beamer}
\begin{document}
\begin{frame}{T}
\xdef\a{1}
\end{frame}
\end{document}";
    let analysis = Machine::analyze(source, None, &cfg);
    assert!(analysis.call_memo[1] > 0, "no kept effect was replayed");
    assert_eq!(body(&analysis, "a"), "1");
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

/// tex.web §§ 1080, 1110: `\unvbox` makes the box's last item the list's
/// tail, so `\lastbox` takes back the alignment row a box ended with, and
/// what is left of the row keeps its cells for `\unhbox`. A stored row
/// read back as a character gave a void `\lastbox`, so amsmath's `align`
/// measured no column and every numbered `align` had "Missing number" gaps.
#[test]
fn lastbox_after_unvbox_takes_the_row_the_box_ended_with() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}\usepackage{amsmath}\begin{document}
\setbox0\vbox{\halign{#&#\cr a&b\cr}}\setbox2\vbox{\unvbox0 \unpenalty \global\setbox1\lastbox}
\setbox1\hbox{\unhbox1 \unskip \global\setbox3\lastbox}\xdef\Sb{\ifhbox3 Y\else N\fi}
\begin{align}a &= b\\ c &= d\end{align}
\end{document}",
    );
    assert_eq!(body(&analysis, "Sb"), "Y");
    let gaps: Vec<_> = analysis.facts.diagnostics.iter().filter(|d| d.code == "missing-number").collect();
    assert!(gaps.is_empty(), "{gaps:?}");
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

/// tex.web §§ 898, 903-918: a word's ligatures hyphenate by their letters.
/// Inside "ff" or "ffi" the discretionary's pre-break text is "f-", its
/// post-break text the rest, and it replaces the ligature. Values as
/// `pdflatex` prints; before, "effects" lost its hyphens (a wrong height)
/// and "flowr" left the paragraph undecided.
#[test]
fn a_ligature_in_a_word_hyphenates_by_its_letters() {
    if !installed() {
        return;
    }
    let src = r"\documentclass{article}
\begin{document}
\pretolerance=-1 \tolerance=100 \parfillskip=0pt plus 1fil
\setbox0\hbox{ef-}\setbox0\vbox{\hsize=\wd0 \parindent0pt \hskip0pt effect\par\xdef\Ma{\the\prevgraf}}
\setbox0\hbox{suf-}\setbox0\vbox{\hsize=\wd0 \parindent0pt \hskip0pt sufficient\par\xdef\Mb{\the\prevgraf}}
\setbox0\vbox{\hsize=1pt \tolerance=10000 \hyphenpenalty=0 \parindent0pt \hskip0pt sufficient official\par\xdef\Mc{\the\prevgraf}}
\setbox0\vbox{\hsize=100pt Different effects of sufficient official offices affect efficient staffing differently.}\xdef\Md{\the\ht0/\the\dp0}
\setbox0\vbox{\hsize=90pt Some words about finding assertions, driven by \emph{flowr} and the official staff.}\xdef\Me{\the\ht0/\the\dp0}
\end{document}";
    let analysis = analyze(src);
    let want = [("Ma", "2"), ("Mb", "2"), ("Mc", "7"), ("Md", "42.94444pt/1.94444pt"), ("Me", "42.94444pt/0.0pt")];
    for (name, want) in want {
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
/// place (amsmath's `\text` is one), and so does `\displaystyle`.
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
    // `\displaystyle` is text size; its own steps (limits, `sup1`) are followed or lose.
    assert_eq!(body(&analysis, "D"), "9.57755pt");
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
    assert_body(
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
        "result",
        "vvovo",
    );
}

/// tex.web §§ 342, 780, 336: an `&` that skipped conditional text reads at
/// `align_state` zero ends the cell, and the `v` part TeX inserts ends in
/// `\endtemplate`, which counts as `\outer`: "Incomplete \iffalse", then
/// "Extra \fi" and "Extra alignment tab" (pdflatex). Bug pinned: satex skipped
/// past the `&` and said nothing, on a run it called complete. After a
/// `\relax` only: `align_peek` expands a leading `\iffalse` at `align_state`
/// 1000000, where an `&` ends nothing (pdflatex: no error).
#[test]
fn an_alignment_tab_in_skipped_text_ends_the_cell() {
    if !installed() {
        return;
    }
    let errors = |cell: &str| {
        let source = format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n\\setbox0\\vbox{{\\halign{{#&#\\cr {cell} x&y\\cr}}}}\n\\end{{document}}\n"
        );
        let analysis = analyze(&source);
        let lint = satex::linter::lint(&analysis);
        let errors = lint.iter().filter(|f| f.get("severity").is_some_and(|s| s == "error"));
        errors.filter_map(|f| f.get("message")?.as_str().map(str::to_string)).collect::<Vec<_>>()
    };
    let skipped = errors("\\relax\\iffalse a&b\\fi");
    assert_eq!(skipped.len(), 3, "{skipped:?}");
    assert!(skipped.iter().any(|m| m.contains("endtemplate")), "{skipped:?}");
    assert_eq!(errors("\\iffalse a&b\\fi"), Vec::<String>::new());
}

/// A macro that calls itself with nothing to stop it, run on one arm of an
/// undecided conditional: pdflatex never ends on the runs that take the arm
/// (seed 2), the others end clean. Some run fails, so the run reports a
/// possible finding, a warning. Bug pinned: run on every path the same loop
/// is an error (`unguarded-recursion`), on one arm the engine notes its
/// `endless-loop` only as imprecision and no rule reports it.
#[test]
fn an_endless_loop_on_one_arm_is_a_possible_finding() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\begin{document}\nx\n\\def\\r{\\r}\\ifnum\\pdfuniformdeviate2=0 \\r\\fi\n\\end{document}\n",
    );
    let lints = satex::linter::lint(&analysis);
    let severity = |s: &str| lints.iter().any(|f| f.get("severity").and_then(|v| v.as_str()) == Some(s));
    assert!(severity("warning") && !severity("error"), "{lints:?}");
}

/// [`an_alignment_tab_in_skipped_text_ends_the_cell`] in a LaTeX `tabular`:
/// pdflatex gives "Incomplete \iffalse", "Missing } inserted" and "Extra
/// \fi" for the cell `\relax\iffalse a & b \fi`. Bug pinned: satex reports
/// nothing on a run it calls complete; the skip passes the `&` as if no cell
/// ended there (the plain `\halign` case is caught).
#[test]
fn an_alignment_tab_in_skipped_text_ends_a_tabular_cell() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\begin{tabular}{cc}
\relax\iffalse a & b \fi\\
\end{tabular}
\end{document}",
    );
    let errors: Vec<_> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|f| f.get("severity").and_then(|s| s.as_str()) == Some("error"))
        .collect();
    assert!(!errors.is_empty(), "no error");
}

/// tex.web § 780: only the `\endtemplate` a cell's `v` part ends in is
/// forbidden in a scan, not a token that looks like the `&` given back
/// behind it. Here `\amp` ends the cell and the `v` part's `\edef` expands
/// `\amp` again: both `&` are the one token of `\amp`'s body. Bug pinned:
/// the put-back `&` was known by where it was read (file, line, column), so
/// the `\edef`'s `&` counted as `\endtemplate` and the scan stopped with a
/// false forbidden-outer. pdflatex raises nothing.
#[test]
fn a_cell_end_from_a_macro_is_no_endtemplate_in_its_v_part() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\def\\amp{&}\n\\begin{document}\n\\setbox0\\vbox{\\halign{#\\edef\\x{\\amp}&#\\cr a\\amp b\\cr}}\n\\end{document}\n";
    let a = analyze(source);
    let outer: Vec<_> =
        a.facts.diagnostics.iter().filter(|d| d.code == "forbidden-outer").map(|d| &d.message).collect();
    assert!(outer.is_empty(), "{outer:?}");
}

/// tex.web § 324: `align_state` is zero where a `u` part ends, whatever
/// braces it opened, so with the template `{#}` an `&` in skipped text still
/// ends the cell (pdflatex: Incomplete \iffalse, Missing }, Extra \fi, Extra
/// alignment tab). Bug pinned: satex took the cell's base before reading
/// the `u` part's `{`, so no `&` ever matched it (every LaTeX tabular cell).
/// Clean cells stay clean.
#[test]
fn an_alignment_tab_in_skipped_text_ends_a_braced_cell() {
    if !installed() {
        return;
    }
    let errors = |cell: &str| {
        let source = format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n\\setbox0\\vbox{{\\halign{{{{#}}&#\\cr {cell}\\cr}}}}\n\\end{{document}}\n"
        );
        let lint = satex::linter::lint(&analyze(&source));
        let errors = lint.iter().filter(|f| f.get("severity").is_some_and(|s| s == "error"));
        errors.filter_map(|f| f.get("message")?.as_str().map(str::to_string)).collect::<Vec<_>>()
    };
    let skipped = errors("\\relax\\iffalse a&b\\fi x&y");
    assert!(skipped.iter().any(|m| m.contains("endtemplate")), "{skipped:?}");
    assert_eq!(errors("\\relax\\iffalse a\\fi x&y"), Vec::<String>::new());
    assert_eq!(errors("\\iffalse a&b\\fi x&y"), Vec::<String>::new());
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

/// Each inline formula of `cases` boxed alone has the width/height/depth it names.
fn math_sizes(cases: &[(&str, &str)]) {
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

/// A command that puts nothing on a list puts no atom into a formula: a font's
/// settings (pdfTeX manual: `\pdfnobuiltintounicode`, `\pdffontattr`), the catalog,
/// `\afterassignment`. cmap sets the first two for every font it loads, the math fonts
/// at the first formula of a size: that formula's size was given up, and with it the
/// box around it (acmart's title box, whose unknown height ran its page loop). Sizes as
/// `pdflatex` prints.
#[test]
fn a_command_that_appends_nothing_keeps_a_formula_s_size() {
    if !installed() {
        return;
    }
    let cases = [
        (r"\pdfnobuiltintounicode\font a", "5.28589pt/4.30554pt/0.0pt"),
        (r"a\pdffontattr\font{}b", "9.57755pt/6.94444pt/0.0pt"),
        (r"a\pdfcatalog{}b", "9.57755pt/6.94444pt/0.0pt"),
        (r"a\afterassignment\relax b", "9.57755pt/6.94444pt/0.0pt"),
    ];
    math_sizes(&cases);
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
    math_sizes(&cases);
}

/// tex.web §§ 743-748, 1151, 1153: a fraction of characters sets its parts one size down,
/// shifted by the size's parameters; a character of math code "8000 (`'`) is its active
/// character; an implicit `{` (`\bgroup`) opens a script group. Sizes as `pdflatex` prints.
#[test]
fn inline_math_with_fractions_and_primes_has_its_size() {
    if !installed() {
        return;
    }
    let cases = [
        (r"\frac{1}{2}", "6.38612pt/8.44843pt/3.44841pt"),
        (r"{x\atop y}", "6.93472pt/7.45119pt/4.80951pt"),
        (r"a{12\over x}b", "19.9498pt/8.44843pt/3.44841pt"),
        (r"\frac{ab}{c}", "10.2543pt/8.79842pt/3.44841pt"),
        (r"{A\over B}", "8.78299pt/8.72066pt/3.44841pt"),
        (r"{f\over x}", "7.08408pt/9.32217pt/3.44841pt"),
        (r"x^{\frac12}", "12.0181pt/9.53845pt/0.0pt"),
        (r"\frac{1}{2}+\frac{x}{y}", "25.54298pt/8.44843pt/4.80951pt"),
        (r"V'", "10.86111pt/7.51782pt/0.0pt"),
        (r"x''", "10.82639pt/7.51782pt/0.0pt"),
        (r"x^\bgroup\prime\egroup", "8.52083pt/7.51782pt/0.0pt"),
    ];
    math_sizes(&cases);
}

/// `\unhbox` of any hbox adds its list's natural width (tex.web § 1110),
/// whatever it was packed to, and an `\unskip` after it takes away the
/// glue the list ended in: tikz's `align` lines in an `\halign`. The space
/// factor stays the one before it. Widths as `pdflatex` prints.
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
        (r"\setbox0\hbox{x.\setbox1\hbox{a}\unhbox1\relax\space b}", "23.05562pt"),
        (r"\setbox0\hbox{x\setbox1\hbox{a.}\unhbox1\relax\space b}", "21.9445pt"),
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

/// `\unhbox` puts the box's items back (tex.web § 1110), so a paragraph
/// breaks them as TeX does: an enumitem inline list boxes each item, and a
/// tikz node of text width set it. Height/depth as `pdflatex` prints.
#[test]
fn an_unhboxed_list_breaks_into_lines() {
    if !installed() {
        return;
    }
    let words = r"Some words \setbox1\hbox{in the box of words and other text}\unhbox1, finding more words that wrap.";
    let list = r"Some words in \begin{inlenum}\item reading all the input files \item writing the output and the summary report\end{inlenum}.";
    let cases = [
        (120, r"\setbox1\hbox{in}Some words \unhbox1, finding.", "6.94444pt/1.94444pt"),
        (150, words, "30.94444pt/1.94444pt"),
        (250, words, "18.94444pt/1.94444pt"),
        (200, list, "31.5pt/1.94444pt"),
        (300, list, "19.5pt/1.94444pt"),
    ];
    let mut src = String::from(
        "\\documentclass{article}\n\\usepackage[inline]{enumitem}\n\\newlist{inlenum}{enumerate*}{1}\n\
         \\setlist[inlenum]{label=\\emph{\\roman*)},itemjoin={{, }},itemjoin*={{, and }}}\n\\begin{document}\n",
    );
    for (i, (hsize, f, _)) in cases.iter().enumerate() {
        let name = (b'a' + i as u8) as char;
        src.push_str(&format!("\\setbox0\\vbox{{\\hsize={hsize}pt {f}}}\\xdef\\H{name}{{\\the\\ht0/\\the\\dp0}}\n"));
    }
    src.push_str("\\end{document}");
    let analysis = analyze(&src);
    let got: Vec<String> =
        (0..cases.len()).map(|i| body(&analysis, &format!("H{}", (b'a' + i as u8) as char))).collect();
    let want: Vec<String> = cases.iter().map(|(_, _, w)| w.to_string()).collect();
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
    let text = satex::cli::render::lint(&records, satex::cli::render::Links::default(), true);
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

/// Capabilities: unknown inputs, code as data. `\char_generate:nn` of an
/// unknown code names `c__char_<roman numeral>_tl`: the numeral is unknown
/// characters, part of the name, never a token that stops the `\csname`.
#[test]
fn char_generate_of_an_unknown_code_closes_its_csname() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\str_set:Ne \l_tmpa_str { \char_generate:nn { \int_rand:nn {32} {255} } {12} abc }
\ExplSyntaxOff
\begin{document}x\end{document}",
    );
    let stopped: Vec<_> =
        analysis.facts.diagnostics.iter().filter(|d| d.message.contains("has no \\endcsname")).collect();
    assert!(stopped.is_empty(), "{:?}", stopped.iter().map(|d| &d.message).collect::<Vec<_>>());
}

/// Intuition: a build whose second run stops at a budget reports its first run, which read no
/// `.aux`: what it says about the document is an older state, and must say so.
/// Bug: the fallback dropped every trace of the stopped run, so a test or a user saw a clean run
/// (`documents::notecolumn_notes_read_back` was green without reading its notes back).
#[test]
fn a_build_that_fell_back_to_an_earlier_run_says_so() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-build-fallback-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("spin.tex");
    // Ten calls per level: what only the second run does, with the label read.
    let name = |n: u8| format!("\\lv{}", char::from(b'a' + n));
    let levels: String =
        (1..6).map(|n| format!("\\def{}{{{}}}", name(n), format!("{} ", name(n - 1)).repeat(10))).collect();
    let source = format!(
        "\\documentclass{{article}}\\def{}{{\\relax}}{levels}\\begin{{document}}\\ref{{s}}\\makeatletter
\\@ifundefined{{r@s}}{{}}{{{}}}\\makeatother\\section{{S}}\\label{{s}}\\end{{document}}\n",
        name(0),
        name(5)
    );
    std::fs::write(&path, &source).unwrap();
    // The budget lets the first run end, as it did alone, and stops the second.
    // The spin must cost what it says: kept call effects would replay its calls.
    let mut cfg = crate::support::config();
    cfg.limits.call_memo_steps = 0;
    cfg.limits.build_runs = 1;
    let first = Machine::analyze(&source, Some(&path), &cfg);
    cfg.limits.build_runs = 5;
    cfg.limits.steps = first.steps + first.steps / 100;
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    assert_eq!(analysis.written_runs.len(), 2, "the second run is the one that stops");
    assert!(analysis.exhausted);
    assert!(analysis.limits.iter().any(|l| l.key == "steps"), "{:?}", analysis.limits);
    assert!(
        analysis.facts.diagnostics.iter().any(|d| d.code == "budget-exhausted" && d.message.contains("build run 2")),
        "{:?}",
        analysis.facts.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// The build's runs (challenge 11): a run writes its own `.aux`, and the
/// next reads it back, so a reference finds its label. A run that read
/// nothing a changed line defines needs no second one.
#[test]
fn the_build_reruns_only_for_what_it_read() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-build-runs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let run = |name: &str, source: &str| {
        let path = dir.join(name);
        std::fs::write(&path, source).unwrap();
        Machine::analyze(source, Some(&path), &crate::support::config())
    };
    let referenced = run(
        "ref.tex",
        "\\documentclass{article}\\begin{document}\\ref{s}\\makeatletter\\@ifundefined{r@s}{\\gdef\\U{}}{\\gdef\\D{}}\\makeatother\\section{S}\\label{s}\\end{document}\n",
    );
    assert_eq!(referenced.build_runs, 2);
    assert!(defined(&referenced, "D") && undefined(&referenced, "U"));
    // A `.lof` line defining a macro: read back, it matters only where the
    // run expanded that macro.
    let lof = |used: &str| {
        format!(
            "\\def\\Seen{{0}}\\immediate\\openout1=\\jobname.lof \\write1{{\\string\\gdef\\string\\{used}{{1}}}}\\openin2=\\jobname.lof \\ifeof2 \\else\\closein2 \\input\\jobname.lof \\fi\\edef\\X{{\\Seen}}\n"
        )
    };
    assert_eq!(run("seen.tex", &lof("Seen")).build_runs, 2);
    assert_eq!(run("unseen.tex", &lof("Unseen")).build_runs, 1);
    // A line whose value every run changes (a counter written back, a page
    // that moves) is widened when it changes again after the second run:
    // four runs, not the limit.
    let counting = run(
        "count.tex",
        "\\def\\N{0}\\openin2=\\jobname.lof \\ifeof2 \\else\\closein2 \\input\\jobname.lof \\fi\\immediate\\openout1=\\jobname.lof \\immediate\\write1{\\string\\gdef\\string\\N{\\number\\numexpr\\N+1\\relax}}\\edef\\X{\\N}\n",
    );
    assert_eq!(counting.build_runs, 4);
    // A line only the first run wrote shifts the rest: the join aligns on
    // what each line sets, so the line between the moving ones stays exact.
    let shifted = run(
        "shift.tex",
        "\\def\\N{0}\\openin2=\\jobname.lof \\ifeof2 \\else\\closein2 \\input\\jobname.lof \\fi\\immediate\\openout1=\\jobname.lof \\immediate\\write1{\\string\\gdef\\string\\N{\\number\\numexpr\\N+1\\relax}}\\ifnum\\N=0 \\immediate\\write1{\\string\\gdef\\string\\E{}}\\fi\\immediate\\write1{\\string\\gdef\\string\\K{7}}\\immediate\\write1{\\string\\gdef\\string\\P{\\N}}\\edef\\X{\\N\\P}\n",
    );
    assert_eq!(crate::support::body(&shifted, "K"), "7");
    // A `.gls` no `.tex` is named after is a source the document wrote, not
    // a build's: it is read as it is on disk.
    std::fs::write(dir.join("entries.gls"), "\\gdef\\G{}\n").unwrap();
    assert!(defined(&run("source.tex", "\\input{entries.gls}\n"), "G"));
    std::fs::remove_dir_all(dir).ok();
}

/// A build file named only in another (a part `.aux` the main `.aux`
/// names) is on disk after the run that wrote it, though no run read it
/// yet: the next run reads it, so two runs settle, as with LaTeX.
#[test]
fn a_build_file_another_names_is_read_in_the_next_run() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-named-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = "\\documentclass{article}\\def\\Y{0}\\InputIfFileExists{\\jobname.lof}{}{}\n\
        \\immediate\\openout1=\\jobname.lof \\immediate\\write1{\\string\\InputIfFileExists{\\jobname.lot}{}{}}\\immediate\\closeout1\n\
        \\immediate\\openout3=\\jobname.lot \\immediate\\write3{\\string\\gdef\\string\\Y{1}}\\immediate\\closeout3\n\
        \\edef\\X{\\Y}\\begin{document}\\end{document}\n";
    let path = dir.join("named.tex");
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    std::fs::remove_dir_all(dir).ok();
    assert_eq!(crate::support::body(&analysis, "X"), "1");
    assert_eq!(analysis.build_runs, 2);
}

/// The bibliography goes into the typeset pass: the build makes its own
/// `.bbl` from the entries the document cites, so `\bibliography` sets
/// them and `\cite` finds its label. An entry no `\cite` names is not set.
#[test]
fn the_build_makes_a_bbl_from_the_cited_entries() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-bbl-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("refs.bib"),
        "@article{k1,\n  author = {Doe, Jane and Roe, Richard},\n  title = {A Title},\n  journal = {A Journal},\n  year = 2001,\n}\n@book{k2, author={Poe, Edgar}, title={Unused}, year=1999}\n",
    )
    .unwrap();
    let source = "\\documentclass{article}\\begin{document}\\cite{k1}\\bibliographystyle{plain}\\bibliography{refs}\\makeatletter\\@ifundefined{b@k1}{\\gdef\\Missing{}}{\\gdef\\Found{}}\\@ifundefined{b@k2}{\\gdef\\Unset{}}{\\gdef\\Set{}}\\makeatother\\end{document}\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    assert!(defined(&analysis, "Found") && undefined(&analysis, "Missing"), "{:?}", analysis.build_runs);
    assert!(defined(&analysis, "Unset") && undefined(&analysis, "Set"));
    // biblatex reads biber's format: its toggle says the file was found.
    let source = "\\documentclass{article}\\usepackage[style=authoryear]{biblatex}\\addbibresource{refs.bib}\\begin{document}\\cite{k1}\\printbibliography\\makeatletter\\iftoggle{blx@bbldone}{\\gdef\\Found{}}{\\gdef\\Missing{}}\\makeatother\\end{document}\n";
    let path = dir.join("blx.tex");
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    assert!(defined(&analysis, "Found") && undefined(&analysis, "Missing"), "{:?}", analysis.build_runs);
    std::fs::remove_dir_all(dir).ok();
}

/// Paths that wrote different lines to the `.aux` join (tex.web § 1370):
/// the next run reads what either wrote, not one path's file.
#[test]
fn the_aux_of_two_paths_holds_what_either_wrote() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-joined-aux-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two.tex");
    let source = "\\documentclass{article}\\makeatletter\\providecommand\\foo{C}\n\\begin{document}\n\
        \\if A\\foo\\gdef\\GotA{}\\fi \\if B\\foo\\gdef\\GotB{}\\fi\n\
        \\ifodd\\pdfuniformdeviate9 \\immediate\\write\\@auxout{\\string\\gdef\\string\\foo{A}}\\else\
        \\immediate\\write\\@auxout{\\string\\gdef\\string\\foo{B}}\\fi\n\\end{document}\n";
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    std::fs::remove_dir_all(dir).ok();
    assert!(maybe_defined(&analysis, "GotA") && maybe_defined(&analysis, "GotB"));
}

/// Paths that wrote the `.aux` with different numbers of lines leave
/// two texts; the rerun reads each on its own path, also through the
/// 8-bit engine's byte decoding.
#[test]
fn the_aux_texts_of_two_paths_are_read_apart() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-aux-texts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("texts.tex");
    let source = "\\documentclass{article}\\makeatletter\\providecommand\\foo{C}\n\\begin{document}\n\
        \\if A\\foo\\gdef\\GotA{}\\fi \\if B\\foo\\gdef\\GotB{}\\fi\n\
        \\ifodd\\pdfuniformdeviate9 \\immediate\\write\\@auxout{\\string\\gdef\\string\\foo{A}}\\else\
        \\immediate\\write\\@auxout{\\string\\gdef\\string\\foo{B}}\\immediate\\write\\@auxout{\\relax}\\fi\n\\end{document}\n";
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    std::fs::remove_dir_all(dir).ok();
    assert!(maybe_defined(&analysis, "GotA") && maybe_defined(&analysis, "GotB"));
}

/// Paths that read different `.aux` texts join inside a verbatim group,
/// where one saved a category twice: leaving the group puts back the
/// first save, so the space is a space again (tex.web § 282).
#[test]
fn a_category_saved_twice_in_a_group_comes_back_as_first_saved() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-saved-twice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("twice.tex");
    let source = "\\documentclass{article}\\usepackage{verbatim}\n\\newenvironment{va}{\\verbatim}{\\endverbatim}\n\
        \\begin{document}\n\\begin{va}\nx\n\\end{va}\n\\ifnum\\catcode32=10 \\else\\gdef\\Leaked{}\\fi\n\
        \\ifodd\\pdfuniformdeviate9 \\expandafter\\enddocument\\fi\n\\section{A}\\label{a}\n\\end{document}\n";
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    std::fs::remove_dir_all(dir).ok();
    assert!(undefined(&analysis, "Leaked"));
}

/// `\romannumeral` of a number in a small interval is one of its numerals
/// (tex.web § 69): a name spelled from it is each name TeX may build.
#[test]
fn a_roman_numeral_of_a_small_interval_names_each_value() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\\count255=\\pdfuniformdeviate 2 \\advance\\count255 1 \\expandafter\\gdef\\csname R\\romannumeral\\count255\\endcsname{}\\begin{document}x\\end{document}\n";
    let a = analyze(source);
    assert!(maybe_defined(&a, "Ri") && maybe_defined(&a, "Rii"));
}

/// A join left `\Val` one of two texts. Library code that splits it at a
/// `.` runs away on the text without one: the document compiles, so that
/// arm is no run of it, whatever the scan raised before (tex.web § 396).
#[test]
fn a_runaway_in_library_code_refutes_the_joined_text_it_split() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-joined-arm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("frac.sty"),
        "\\def\\getfrac#1.#2\\@@{\\gdef\\Frac{#2}}\\def\\usefrac{\\expandafter\\getfrac\\Val\\@@}\n",
    )
    .unwrap();
    let source = "\\documentclass{article}\\usepackage{frac}\\ifnum\\pdfuniformdeviate2=0 \\gdef\\Val{1}\\else\\gdef\\Val{1.5}\\fi\\usefrac\n\n\\begin{document}x\\end{document}\n";
    let path = dir.join("main.tex");
    std::fs::write(&path, source).unwrap();
    let a = Machine::analyze(source, Some(&path), &crate::support::config());
    assert_eq!(body(&a, "Frac"), "5");
    assert!(!codes(&a).iter().any(|c| c.contains("library-error") || c.contains("runaway")), "{:?}", codes(&a));
    std::fs::remove_dir_all(dir).ok();
}

/// `\z` holds the text a join left `\x` with, one of two bodies. Library
/// code testing it against an empty macro (as LaTeX's list loops do)
/// splits per body (tex.web § 507), and each arm keeps its own, so the
/// same test inside the arm is decided.
#[test]
fn ifx_splits_a_body_a_join_left() {
    if !installed() {
        return;
    }
    let root = crate::support::tmp_root().join(format!("satex-ifx-split-{}", std::process::id()));
    let (dir, lib) = (root.join("doc"), root.join("lib"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("emptiness.sty"),
        "\\def\\check{\\ifx\\z\\empty\\gdef\\E{}\\ifx\\z\\empty\\else\\gdef\\Never{}\\fi\\else\\gdef\\N{}\\fi}\n",
    )
    .unwrap();
    let source = "\\documentclass{article}\\usepackage{../lib/emptiness}\\ifnum\\pdfuniformdeviate2=0 \\def\\x{}\\else\\def\\x{y}\\fi\\edef\\z{\\x}\\check\\begin{document}x\\end{document}\n";
    let path = dir.join("main.tex");
    std::fs::write(&path, source).unwrap();
    let a = Machine::analyze(source, Some(&path), &crate::support::config());
    assert!(maybe_defined(&a, "E") && maybe_defined(&a, "N"));
    assert!(undefined(&a, "Never"));
    std::fs::remove_dir_all(root).ok();
}

/// tex.web § 1100: an output routine in library code that would close
/// the conditional its caller opened is no run of a document that
/// compiles; the caller's conditional stays open for its own `\fi`.
#[test]
fn an_output_routine_never_closes_its_callers_conditional() {
    if !installed() {
        return;
    }
    let root = crate::support::tmp_root().join(format!("satex-output-fi-{}", std::process::id()));
    let (dir, lib) = (root.join("doc"), root.join("lib"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("outfi.sty"),
        "\\output={\\setbox0\\box255 \\ifnum\\pdfuniformdeviate 2=0 \\let\\next\\relax\\else\\def\\next{\\fi}\\fi\\next}\n\
\\def\\run{\\ifnum1=1 \\hbox{}\\par\\penalty-10000 \\gdef\\after{}\\fi}\n",
    )
    .unwrap();
    let source = "\\documentclass{article}\\usepackage{../lib/outfi}\\begin{document}\\run\\end{document}\n";
    let path = dir.join("main.tex");
    std::fs::write(&path, source).unwrap();
    let a = Machine::analyze(source, Some(&path), &crate::support::config());
    assert!(defined(&a, "after"));
    let extra: Vec<_> = a.facts.diagnostics.iter().filter(|d| d.code == "extra-fi").collect();
    assert!(extra.is_empty(), "{extra:?}");
    std::fs::remove_dir_all(root).ok();
}

/// What TeX reads each argument as ([`satex::analysis::roles::Ty`]), seen in the run
/// and handed on through wrappers.
mod types {
    use super::*;

    /// `cmd`'s argument types as signatures show them, in a run of `body`
    /// after a preamble of lengths, a box and a counter.
    fn types(body: &str, cmds: &[&str]) -> Vec<Vec<String>> {
        let dir = crate::support::tmp_root().join(format!("satex-types-{}-{}", cmds[0], std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = format!(
            "\\documentclass{{article}}\n\\newlength\\mylen\n\\newsavebox\\mybox\n\\newcounter{{foo}}\n{body}\n\\end{{document}}\n"
        );
        let path = dir.join("main.tex");
        std::fs::write(&path, &source).unwrap();
        let a = Machine::analyze(&source, Some(&path), &crate::support::config());
        let shown = |cmd: &&str| {
            let roles = satex::analysis::roles::roles(&a, cmd);
            satex::analysis::roles::arg_types(&roles).iter().map(|(arg, t)| format!("#{} {t}", arg + 1)).collect()
        };
        let found = cmds.iter().map(shown).collect();
        std::fs::remove_dir_all(dir).ok();
        found
    }

    #[test]
    fn a_setlength_wrapper_takes_glue() {
        if !installed() {
            return;
        }
        let found = types(
            "\\newcommand\\setw[1]{\\setlength{\\mylen}{#1}}\n\\begin{document}\n\\setw{3pt}",
            &["setw", "setlength"],
        );
        // The value of a skip register is glue (`\setlength`'s body: `#1 #2\relax`).
        assert_eq!(found, [vec!["#1 glue"], vec!["#1 skip register", "#2 glue"]]);
    }

    #[test]
    fn a_usebox_wrapper_takes_a_box() {
        if !installed() {
            return;
        }
        let found = types(
            "\\newcommand\\showb[1]{\\usebox{#1}}\n\\newcommand\\cp[1]{\\copy#1}\n\\begin{document}\n\\showb{\\mybox}\\cp{\\mybox}",
            &["showb", "cp"],
        );
        assert_eq!(found, [vec!["#1 box"], vec!["#1 box"]]);
    }

    #[test]
    fn a_setcounter_wrapper_takes_a_counter_and_a_number() {
        if !installed() {
            return;
        }
        let found = types("\\newcommand\\setc[2]{\\setcounter{#1}{#2}}\n\\begin{document}\n\\setc{foo}{2}", &["setc"]);
        assert_eq!(found, [vec!["#1 counter", "#2 number"]]);
    }

    /// One argument read as glue in one call and as a number in another
    /// is a quantity: the join of the two.
    #[test]
    fn uses_that_disagree_join_upward() {
        if !installed() {
            return;
        }
        let found = types(
            "\\newcommand\\amount[2]{\\ifnum#1=0 \\hspace{#2}\\else\\setcounter{foo}{#2}\\fi}\n\\begin{document}\n\\amount{0}{1em}\\amount{1}{2}",
            &["amount"],
        );
        assert_eq!(found, [vec!["#1 number", "#2 quantity"]]);
    }

    /// A wrapper that is never called gets its types from the body: the
    /// call it makes hands `#1` on to an argument that has one.
    #[test]
    fn a_wrapper_inherits_types_through_its_body() {
        if !installed() {
            return;
        }
        let found = types(
            "\\newcommand\\wrapw[1]{\\innerw{#1}}\n\\newcommand\\innerw[1]{\\setlength{\\mylen}{#1}}\n\\begin{document}\n\\innerw{1pt}",
            &["wrapw"],
        );
        assert_eq!(found, [vec!["#1 glue"]]);
    }

    #[test]
    fn the_lattice_orders_registers_below_their_values() {
        use satex::analysis::roles::Ty;
        use satex::engine::tex::RegKind;
        assert_eq!(Ty::Reg(RegKind::Dimen).join(Ty::Reg(RegKind::Skip)), Ty::Glue);
        assert_eq!(Ty::Box.join(Ty::Dimen), Ty::Quantity);
        assert_eq!(Ty::Named(RegKind::Count).join(Ty::Key(satex::analysis::roles::KeyKind::Any)), Ty::Name);
        assert_eq!(Ty::Cs.join(Ty::Number), Ty::Top);
        assert!(Ty::Dimen.leq(Ty::Glue) && !Ty::Glue.leq(Ty::Dimen));
        assert_eq!(Ty::Bottom.join(Ty::Box), Ty::Box);
    }
}

/// An implicit brace opens and closes a math group as `{` and `}` do
/// (tex.web § 1063): `\mathrm` (`\bgroup`…`\egroup`) keeps the formula's
/// width known.
#[test]
fn implicit_braces_keep_a_formula_width() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\\begin{document}\\setbox0\\hbox{${s}$}\\xdef\\A{\\the\\wd0}\\setbox0\\hbox{$\\bgroup s\\egroup$}\\xdef\\B{\\the\\wd0}\\setbox0\\hbox{$\\mathrm{s}$}\\xdef\\C{\\the\\wd0}\\end{document}\n";
    let a = analyze(source);
    assert_eq!(body(&a, "A"), body(&a, "B"));
    assert!(body(&a, "C").ends_with("pt") && !body(&a, "C").contains('\u{2}'), "{}", body(&a, "C"));
}

/// A script with no nucleus gets an empty one (tex.web § 1176): a footnote
/// mark's superscript keeps its width, the script's and `\scriptspace`.
#[test]
fn a_script_without_nucleus_keeps_its_width() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\\begin{document}\\setbox0\\hbox{$^{1}$}\\xdef\\A{\\the\\wd0}\\setbox0\\hbox{\\footnotemark}\\xdef\\B{\\the\\wd0}\\end{document}\n";
    let a = analyze(source);
    assert_eq!(body(&a, "A"), "4.48613pt");
    assert_eq!(body(&a, "B"), "4.48613pt");
}

/// tex.web §§ 991, 1000: the job starts with an empty page, and glue or a
/// penalty on it is discarded. A class's `\clearpage` (KOMA's `\onecolumn`)
/// on the empty page leaves `\pagetotal` at 0pt, as pdflatex reports.
#[test]
fn a_float_flushed_by_clearpage_leaves_the_map_depth_known() {
    if !installed() {
        return;
    }
    assert_body(
        "\\documentclass{article}\\begin{document}\\begin{figure}[h!]\\end{figure}\\clearpage\\ExplSyntaxOn\\xdef\\depth{\\int_use:N\\g__kernel_prg_map_int}\\ExplSyntaxOff\\end{document}",
        "depth",
        "0",
    );
}

/// `\include` flushes the float with its `\clearpage` from the output
/// routine, the class's as well as the kernel's.
fn include_float_depth(class: &str, tag: &str) -> String {
    let dir =
        crate::support::scratch_dir(std::env::temp_dir().join(format!("satex-incl-{tag}-{}", std::process::id())));
    std::fs::write(dir.join("p.tex"), "\\begin{figure}[h!]\n\\end{figure}\n").unwrap();
    let source = format!(
        "\\documentclass{{{class}}}\\begin{{document}}\\include{{p}}\\ExplSyntaxOn\\xdef\\depth{{\\int_use:N\\g__kernel_prg_map_int}}\\ExplSyntaxOff\\end{{document}}"
    );
    let doc = dir.join("doc.tex");
    std::fs::write(&doc, &source).unwrap();
    body(&Machine::analyze(&source, Some(&doc), &crate::support::config()), "depth")
}

#[test]
fn a_float_in_an_include_leaves_the_map_depth_known() {
    if installed() {
        assert_eq!(include_float_depth("article", "article"), "0");
    }
}

/// KOMA's float lists join to several texts; each read of them splits.
#[test]
fn a_float_in_a_koma_include_leaves_the_map_depth_known() {
    if installed() {
        assert_eq!(include_float_depth("scrartcl", "koma"), "0");
    }
}

#[test]
fn the_page_stays_empty_through_a_class_clearpage() {
    if !installed() {
        return;
    }
    for class in ["article", "scrartcl", "scrbook"] {
        let a = analyze(&format!(
            "\\documentclass{{{class}}}\\xdef\\pre{{\\the\\pagetotal}}\\begin{{document}}\\xdef\\doc{{\\the\\pagetotal}}\\end{{document}}"
        ));
        assert_eq!((body(&a, "pre").as_str(), body(&a, "doc").as_str()), ("0.0pt", "0.0pt"), "{class}");
    }
}

/// `\@doclearpage` splits the page's whatsit off (`\vsplit` to 0pt breaks
/// at the `\topskip` glue after it, § 970) and puts it back: a whatsit, no
/// box, so a second `\clearpage` ships nothing, as pdflatex counts.
#[test]
fn a_second_clearpage_ships_no_page() {
    if !installed() {
        return;
    }
    let a = analyze(
        "\\documentclass{article}\\makeatletter\\begin{document}Text\\clearpage\\xdef\\A{\\the\\c@page}\\clearpage\\xdef\\B{\\the\\c@page}\\end{document}",
    );
    assert_eq!((body(&a, "A").as_str(), body(&a, "B").as_str()), ("2", "2"));
}

/// tex.web §§ 970-977: a list with no forced break splits off whole to
/// `\maxdimen`; one with a forced break does not. A list that held items
/// is not empty after `\lastbox` took one: `\lastnodetype` is no -1.
#[test]
fn a_split_to_maxdimen_takes_a_list_without_forced_breaks() {
    if !installed() {
        return;
    }
    let a = analyze(
        r"\documentclass{article}\begin{document}\splitmaxdepth=\maxdimen
\setbox1\vbox to 100pt{\vskip 0pt plus 1fil\hbox{x}\vskip 0pt plus 1fil}
\setbox2\vbox{\unvcopy1}\setbox3\vsplit2 to\maxdimen \xdef\W{\ifvoid2 v\else n\fi}
\setbox2\vbox{\hbox{a}\penalty-10000\hbox{b}}\setbox3\vsplit2 to\maxdimen \xdef\F{\ifvoid2 v\else n\fi}
\setbox2\vbox{\unvcopy1\unskip\setbox0\lastbox\xdef\T{\ifnum\lastnodetype<0 e\else f\fi}}
\end{document}",
    );
    assert_eq!((body(&a, "W").as_str(), body(&a, "T").as_str()), ("v", "f"));
    // pdflatex: n; satex may not know which.
    assert_ne!(body(&a, "F"), "v");
}

/// tex.web §§ 890, 970-979: a list whose characters TeX does not know
/// holds no forced break, so it splits off whole to `\maxdimen`, marks or
/// a nested box's penalty notwithstanding; lines that a penalty of -10000
/// parts, or a smaller target, leave a rest.
#[test]
fn a_split_of_unknown_text_to_maxdimen_leaves_a_void_rest() {
    if !installed() {
        return;
    }
    let spaced = r"\t{} ".repeat(80);
    let a = analyze(&format!(
        r"\documentclass{{article}}\begin{{document}}\splitmaxdepth=\maxdimen \edef\t{{\pdfuniformdeviate 100000}}
\setbox2\vbox{{\t\par}}\setbox3\vsplit2 to\maxdimen \xdef\A{{\ifvoid2 v\else n\fi}}
\setbox2\vbox{{\interlinepenalty=-10000 {spaced}\par}}\setbox3\vsplit2 to\maxdimen \xdef\B{{\ifvoid2 v\else n\fi}}
\setbox2\vbox{{\t\penalty-10000\t\par}}\setbox3\vsplit2 to\maxdimen \xdef\C{{\ifvoid2 v\else n\fi}}
\setbox1\vbox{{\t\par}}\setbox2\vbox{{\unvcopy1\vskip5pt\unvcopy1}}\setbox3\vsplit2 to 10pt \xdef\D{{\ifvoid2 v\else n\fi}}
\setbox2\vbox{{\unvcopy1\mark{{a}}\unvcopy1}}\setbox3\vsplit2 to\maxdimen \xdef\M{{\ifvoid2 v\else n\fi}}
\setbox2\vbox{{\unvcopy1\vbox{{\penalty-10000}}\unvcopy1}}\setbox3\vsplit2 to\maxdimen \xdef\N{{\ifvoid2 v\else n\fi}}
\setbox2\vbox{{\unvcopy1\penalty-10000\unvcopy1}}\setbox3\vsplit2 to\maxdimen \xdef\F{{\ifvoid2 v\else n\fi}}
\end{{document}}"
    ));
    let got: Vec<String> = ["A", "B", "C", "D", "M", "N", "F"].iter().map(|n| body(&a, n)).collect();
    // pdflatex: v n v n v v n; satex may not know the n cases.
    assert_eq!([&got[0], &got[2], &got[4], &got[5]], ["v"; 4]);
    for k in [1, 3, 6] {
        assert_ne!(got[k], "v", "{k}");
    }
}

/// A float page: its marks pass (latex.ltx `\__mark_prepare_and_extract:nn`)
/// leaves the main list empty, so a second `\clearpage` ships nothing.
#[test]
fn a_float_page_then_two_clearpages_count_as_pdflatex() {
    if !installed() {
        return;
    }
    let a = analyze(
        r"\documentclass{article}\begin{document}Text\begin{figure}[p]x\end{figure}\clearpage\clearpage\xdef\P{\the\value{page}}\end{document}",
    );
    assert_eq!(body(&a, "P"), "3");
}

/// A here-float between two paragraphs, then `\clearpage`: the output
/// group closes with its own `}` on every path (pdflatex: one page).
#[test]
fn a_here_float_then_clearpage_closes_each_output_once() {
    if !installed() {
        return;
    }
    let a = analyze(
        r"\documentclass{article}\begin{document}Text\begin{figure}[h]x\end{figure}Text\clearpage\xdef\P{\the\value{page}}\end{document}",
    );
    let c = codes(&a);
    assert!(!c.iter().any(|c| c == "fault-contained" || c.contains("error")), "{c:?}");
    assert_eq!(body(&a, "P"), "2");
}

/// A float list headed by typed text of no class that may be empty, the
/// empty text (as a widened float queue of lars held it twice):
/// `\@xnext`'s `\@elt` reads past it and `\@tryfcolumn` pops each box.
/// Before, the call did not match, the list ran as code (`\@elt` =
/// `\@xtryfc` calling `\@next` again) and recursed to the depth cut.
#[test]
fn a_float_list_headed_by_empty_typed_text_pops_its_boxes() {
    if !installed() {
        return;
    }
    use satex::engine::tex::TextClass;
    let hole =
        satex::engine::env::named_text(TextClass::EMPTY, false, std::iter::empty::<&str>()).replace('\u{2}', "^^02");
    let a = analyze(&format!(
        r"\documentclass{{article}}\begin{{document}}\makeatletter
\begingroup\catcode2=12 \expandafter\gdef\expandafter\H\expandafter{{\csname {hole}\endcsname}}\endgroup
\expandafter\expandafter\expandafter\gdef\expandafter\expandafter\expandafter\L\expandafter\expandafter\expandafter{{\expandafter\H\H\@elt\bx@G\@elt\bx@L}}
\setbox0\vbox{{\@tryfcolumn\L}}\xdef\Made{{\if@fcolmade y\else n\fi}}\makeatother\end{{document}}"
    ));
    let c = codes(&a);
    assert!(!c.iter().any(|c| c == "recursion-widened"), "{c:?}");
    // Both boxes are void: no float column of height 0 is made.
    assert_eq!(body(&a, "Made"), "n");
}

/// One-letter keys referenced before and after their labels, at the
/// default build: the kernel's `\reserved@a` declares no key `a`.
#[test]
fn short_keys_stay_labels_and_references() {
    if !installed() {
        return;
    }
    let dir = crate::support::scratch_dir(std::env::temp_dir().join(format!("satex-keys-{}", std::process::id())));
    let source = "\\documentclass{article}\n\\usepackage{hyperref}\n\\begin{document}\nSee \\ref{a} and \\ref{b}.\n\\section{A}\\label{a}\\section{B}\\label{b}\n\\ref{a} and \\autoref{b}.\n\\end{document}\n";
    let doc = dir.join("doc.tex");
    std::fs::write(&doc, source).unwrap();
    let a = Machine::analyze(source, Some(&doc), &crate::support::config());
    assert_eq!(keys(&a, OccKind::Label), ["a", "b"]);
    assert_eq!(keys(&a, OccKind::Ref).len(), 4, "{:?}", keys(&a, OccKind::Ref));
}

/// tex.web § 889: `\vadjust` material follows the lines, and its forced
/// breaks fire the output routine in order; the second after the box
/// before it reached the page the first emptied.
#[test]
fn forced_breaks_in_vadjust_material_fire_the_output() {
    if !installed() {
        return;
    }
    assert_body(
        "\\documentclass{article}\\gdef\\so{}\\output{\\xdef\\so{\\so[\\the\\outputpenalty]}\\shipout\\box255}\\begin{document}x\\vadjust{\\penalty-10004\\vbox{}\\penalty-10003}y\\par\\end{document}",
        "so",
        "[-10004][-10003][-10001]",
    );
}

/// `pretex` is read before the document's first line (latexmk's
/// `-usepretex`): a switch it defines decides the document's test.
#[test]
fn pretex_code_runs_before_the_document() {
    let mut cfg = crate::support::config();
    cfg.pretex = "\\def\\draft{}".into();
    let a = Machine::analyze("\\ifdefined\\draft\\def\\mode{draft}\\else\\def\\mode{final}\\fi\n", None, &cfg);
    assert_eq!(body(&a, "mode"), "draft");
}

/// tagpdf's namespace id under `\DocumentMetadata`: `\int_rand:n` past
/// 2^17 takes `\uniformdeviate`'s digits apart (`\__fp_randint_split_o:Nw`
/// reads the first one) and puts them back together. pdflatex: no error.
#[test]
fn a_wide_random_integer_is_taken_apart_and_put_back_together() {
    crate::support::assert_clean(
        "\\documentclass{article}\n\\ExplSyntaxOn\n\\int_set:Nn \\l_tmpa_int {\\int_rand:n{16777215}}\n\\ExplSyntaxOff\n\\begin{document}x\\end{document}\n",
    );
}

/// `\\` in `\author` looks ahead for `*` when satex renders the field for
/// its text: the look-ahead ends with the field, and nothing it opened
/// reaches the document. pdflatex: no error.
#[test]
fn a_line_break_in_the_author_leaves_the_document_balanced() {
    if !installed() {
        return;
    }
    let analysis =
        analyze("\\documentclass{article}\\title{T}\\author{A\\\\ B}\\begin{document}\\maketitle\\end{document}");
    assert!(broken_gaps(&analysis).is_empty(), "{:?}", broken_gaps(&analysis));
    assert_eq!(metadata(&analysis, "author"), "A B");
}

/// BibTeX's commands under biblatex raise its errors; the finding names the
/// migration and its fix writes biblatex's commands instead.
#[test]
fn bibtex_commands_under_biblatex_migrate() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-biblatex-migrate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = "\\documentclass{article}\n\\usepackage{biblatex}\n\\begin{document}\nx\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    let path = dir.join("main.tex");
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), &crate::support::config());
    let found = satex::linter::lint(&analysis);
    let migrated: Vec<_> = found.iter().filter(|f| f["code"] == "bibtex-command-under-biblatex").collect();
    let names: Vec<&str> = migrated.iter().filter_map(|f| f["name"].as_str()).collect();
    assert_eq!(names, ["bibliographystyle", "bibliography"], "{found:?}");
    let replacements: Vec<&str> = migrated
        .iter()
        .flat_map(|f| f["edits"].as_array().into_iter().flatten())
        .filter_map(|e| e["replacement"].as_str())
        .collect();
    assert!(replacements.contains(&"\\printbibliography"), "{replacements:?}");
    assert!(replacements.contains(&"\\addbibresource{refs.bib}\n"), "{replacements:?}");
    // the errors they explain are theirs
    assert!(
        !found.iter().any(|f| f["code"] == "raised-error" && migrated.iter().any(|m| m["line"] == f["line"])),
        "{found:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `\newif` without its name takes the next token: `\newif` before
/// `\begin{document}` makes `\begin` a conditional. The call says so, and a
/// `\newif` of a fresh name says nothing.
#[test]
fn a_new_declaration_that_redefines_a_command_is_reported() {
    let named = |source: &str| -> Vec<String> {
        satex::linter::lint(&analyze(source))
            .iter()
            .filter(|f| f["code"] == "new-redefines")
            .map(|f| f["name"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    let broken = "\\documentclass{article}\n\\newif\n\\begin{document}\nx\n\\end{document}\n";
    assert_eq!(named(broken), ["begin"]);
    assert!(named("\\documentclass{article}\n\\newif\\iffancy\n\\begin{document}\nx\n\\end{document}\n").is_empty());
    // `\csname` leaves `\relax`, which `\@ifundefined` reads as undefined
    let by_name = "\\documentclass{article}\n\\expandafter\\newcommand\\csname note\\endcsname{x}\n\\begin{document}\nx\n\\end{document}\n";
    assert!(named(by_name).is_empty());
    // a plain `\def` right after a `\newif` is the document's, not the call's
    let after = "\\documentclass{article}\n\\def\\val{a}\n\\newif\\iffancy\n\\def\\val{b}\n\\begin{document}\nx\n\\end{document}\n";
    assert!(named(after).is_empty());
    // a register declared again as one of its kind (two tables' `\newcount\shownrows`): LaTeX
    // takes it silently, the name only moves to a fresh register
    let again = "\\documentclass{article}\n\\newcount\\rows\n\\newcount\\rows\n\\newcount\\section\n\\begin{document}\nx\n\\end{document}\n";
    assert_eq!(named(again), ["section"]);
}

/// A heading written to the table of contents is a sectioning unit, a part
/// too, which has no running head of its own (`\partmark`).
#[test]
fn parts_are_sections_of_the_outline() {
    if !installed() {
        return;
    }
    let analysis =
        analyze("\\documentclass{article}\n\\begin{document}\n\\part{Theory}\n\\section{Intro}\nx\n\\end{document}\n");
    assert_eq!(keys(&analysis, OccKind::Section), ["Theory", "Intro"]);
}

/// An alignment's rows go onto the enclosing list, so its cells reach the
/// page as placed text, not as box contents, and end a block of the page
/// (tex.web §§ 800-812).
#[test]
fn alignment_cells_are_placed_text() {
    if !installed() {
        return;
    }
    let analysis = crate::support::analyze_plain("\\catcode`\\&=4 x\n\n\\halign{#&#\\cr a&b\\cr c&d\\cr}\ny\n\\end\n");
    let rows = satex::queries::query::run(
        &analysis,
        satex::queries::query::Query::Typeset,
        &satex::queries::query::Filter::Always,
    );
    let texts: Vec<&str> = rows.iter().filter_map(|r| r.get("detail").and_then(|d| d.as_str())).collect();
    assert_eq!(texts, ["x", "abcd", "y"]);
}

/// The keys layer's facts: each definition and use of a key is an occurrence
/// with its role (and a use with the kind defining its target); the Typeset
/// view places each in its paragraph, a `\label` opening a theorem in the
/// theorem and one after a float's caption in the caption. A glossary entry
/// the preamble declares stands nowhere; a graphic's file name is no key.
#[test]
fn every_key_has_a_role_and_its_paragraph() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{graphicx}
\usepackage{amsthm}
\usepackage{glossaries}
\newtheorem{lemma}{Lemma}
\newglossaryentry{tex}{name=TeX,description={a system}}
\begin{document}
\section{One}\label{sec:one}
\begin{lemma}\label{lem:a}
Lemma text on \gls{tex}.
\end{lemma}
See Lemma~\ref{lem:a}.
\begin{figure}
\includegraphics{example-image}
\caption{Cap}
\label{fig:x}
\end{figure}
\end{document}",
    );
    use satex::queries::query::{Filter, Query, run};
    let text =
        |r: &satex::queries::query::Record, k: &str| r.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let occ = run(&analysis, Query::Occurrences, &Filter::Always);
    let mut roles: Vec<[String; 4]> = occ
        .iter()
        .filter(|r| r.contains_key("role"))
        .map(|r| [text(r, "kind"), text(r, "key"), text(r, "role"), text(r, "target")])
        .collect();
    roles.sort();
    roles.dedup();
    let want = [
        ["key", "tex", "def", ""],
        ["key-use", "tex", "use", "key"],
        ["label", "fig:x", "def", ""],
        ["label", "lem:a", "def", ""],
        ["label", "sec:one", "def", ""],
        ["ref", "lem:a", "use", "label"],
    ];
    assert_eq!(roles, want.map(|r| r.map(String::from)));
    // each key with the paragraph it stands in
    let mut placed: Vec<(String, String)> = run(&analysis, Query::Typeset, &Filter::Always)
        .iter()
        .flat_map(|r| {
            let detail = text(r, "detail");
            let keys = r.get("keys").and_then(|k| k.as_array()).cloned().unwrap_or_default();
            keys.into_iter().filter(|k| k.get("role").is_some_and(|r| !r.is_null())).map(move |k| {
                (
                    k["key"].as_str().unwrap_or_default().to_string(),
                    detail.split_whitespace().take(2).collect::<Vec<_>>().join(" "),
                )
            })
        })
        .collect();
    placed.sort();
    let want = [
        ("fig:x", "Figure 1:"),
        ("lem:a", "Lemma 1."),
        ("lem:a", "See Lemma"),
        ("sec:one", "1 One"),
        ("tex", "Lemma 1."),
    ];
    assert_eq!(placed, want.map(|(k, p)| (k.to_string(), p.to_string())));
}

/// The typeset record of text in `center` names the environment, though the
/// kernel defines it, and says its lines are centered; text outside is justified.
#[test]
fn typeset_names_kernel_environments_and_alignment() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\begin{document}\nplain\n\\begin{center}\nhenlo\n\\end{center}\n\\end{document}\n",
    );
    let rows = satex::queries::query::run(
        &analysis,
        satex::queries::query::Query::Typeset,
        &satex::queries::query::Filter::Always,
    );
    let of = |text: &str| rows.iter().find(|r| r.get("detail").and_then(|d| d.as_str()) == Some(text)).cloned();
    let (plain, centered) = (of("plain").expect("plain"), of("henlo").expect("henlo"));
    assert_eq!(centered.get("env").and_then(|e| e.as_str()), Some("center"));
    assert_eq!(centered.get("just").and_then(|e| e.as_str()), Some("center"));
    assert_eq!(plain.get("just").and_then(|e| e.as_str()), Some("justified"));
    assert!(plain.get("env").is_none());
}

/// Each paragraph's record says `\parindent` in force and whether it got the indent box:
/// a paragraph after `\section` has none (`\@afterheading` takes it with `\lastbox`), nor one
/// after `\noindent`; the next one does, `\parindent` wide. Pins the Typeset "paragraph indent" switch.
#[test]
fn typeset_reports_paragraph_indent() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\setlength\\parindent{12pt}\n\\begin{document}\n\\section{One}\nfirst\n\nsecond\n\n\\noindent third\n\n\\indent fourth\n\\end{document}\n",
    );
    let rows = satex::queries::query::run(
        &analysis,
        satex::queries::query::Query::Typeset,
        &satex::queries::query::Filter::Always,
    );
    let of = |text: &str| rows.iter().find(|r| r.get("detail").and_then(|d| d.as_str()) == Some(text)).cloned();
    let indented = |text: &str| of(text).unwrap_or_else(|| panic!("{text} in {rows:?}")).get("indented").cloned();
    assert_eq!(indented("first"), Some(serde_json::json!(false)), "{rows:?}");
    assert_eq!(indented("second"), Some(serde_json::json!(true)), "{rows:?}");
    assert_eq!(indented("third"), Some(serde_json::json!(false)), "{rows:?}");
    assert_eq!(indented("fourth"), Some(serde_json::json!(true)), "{rows:?}");
    let second = of("second").unwrap();
    assert_eq!(
        (second.get("parindent"), second.get("indent")),
        (Some(&serde_json::json!(12.0)), Some(&serde_json::json!(12.0))),
        "{second:?}"
    );
}

/// A paragraph inside a box or alignment that goes into an outer paragraph
/// is part of that one: `\vbox` and `\valign` do not end it.
#[test]
fn nested_lists_do_not_end_the_outer_paragraph() {
    for inner in ["\\vbox{a}", "\\valign{#\\cr a\\cr}"] {
        let analysis = crate::support::analyze_plain(&format!("x {inner} y\n\nz\n\\end\n"));
        let rows = satex::queries::query::run(
            &analysis,
            satex::queries::query::Query::Typeset,
            &satex::queries::query::Filter::Always,
        );
        let texts: Vec<&str> = rows.iter().filter_map(|r| r.get("detail").and_then(|d| d.as_str())).collect();
        assert_eq!(texts, ["x a y", "z"], "{inner}");
    }
}

/// biblatex's options are keys its `\DeclareOption*` handler hands on:
/// load-time ones (`style`) at once, the others (`sorting`, `maxnames`)
/// collected for a later `\setkeys`.
#[test]
fn package_options_follow_the_declare_option_handler() {
    if !installed() {
        return;
    }
    let analysis = analyze("\\documentclass{article}\n\\usepackage{biblatex}\n\\begin{document}\nx\n\\end{document}\n");
    let options: Vec<String> =
        satex::queries::query::package_options(&analysis, "biblatex").into_iter().map(|(k, _)| k).collect();
    for wanted in ["style", "backend", "sorting", "maxnames"] {
        assert!(options.iter().any(|o| o == wanted), "{wanted} not in {options:?}");
    }
    // An entry's own options are no package options.
    assert!(!options.iter().any(|o| o == "uniquepart"), "{options:?}");
}

/// A name an expl3 module function makes or sets is a variable of that
/// module's kind, from the run: `\tl_new:N` and `\seq_new:N` type their
/// argument, a string is a token list below it.
#[test]
fn expl3_variables_are_typed_by_the_function_that_made_them() {
    if !installed() {
        return;
    }
    use satex::analysis::roles::{Ty, VarKind};
    let a = analyze(
        "\\documentclass{article}\\ExplSyntaxOn\n\\tl_new:N \\l_my_tl\n\\seq_new:N \\l_my_seq\n\\str_new:N \\l_my_str\n\\ExplSyntaxOff\n\\begin{document}x\\end{document}\n",
    );
    let at = |line: u32| a.facts.types.iter().find(|(s, _)| s.line == line).map(|(_, t)| *t);
    assert_eq!(at(2), Some(Ty::Var(VarKind::Tl)));
    assert_eq!(at(3), Some(Ty::Var(VarKind::Seq)));
    assert_eq!(at(4), Some(Ty::Var(VarKind::Str)));
    assert!(Ty::Var(VarKind::Str).leq(Ty::Var(VarKind::Tl)) && Ty::Var(VarKind::Tl).leq(Ty::Cs));
}

/// expl3's naming conventions, checked against what the run did: a local
/// name set globally, a `_tl` name set as a sequence, a constant set again.
/// A string set as a token list, a global set globally and declarations
/// are fine.
#[test]
fn expl3_conventions_are_checked_against_the_run() {
    if !installed() {
        return;
    }
    let a = analyze(
        "\\documentclass{article}\\ExplSyntaxOn\n\
         \\tl_new:N \\l_bad_scope_tl \\tl_gset:Nn \\l_bad_scope_tl {x}\n\
         \\seq_new:N \\l_bad_type_tl \\seq_set_split:Nnn \\l_bad_type_tl {,} {a,b}\n\
         \\tl_const:Nn \\c_bad_const_tl {x} \\tl_set:Nn \\c_bad_const_tl {y}\n\
         \\str_new:N \\l_ok_str \\tl_set:Nn \\l_ok_str {x} \\tl_new:N \\g_ok_tl \\tl_gset:Nn \\g_ok_tl {x}\n\
         \\ExplSyntaxOff\n\\begin{document}x\\end{document}\n",
    );
    let found = satex::linter::lint(&a);
    let named = |code: &str| -> Vec<String> {
        found.iter().filter(|r| r["code"] == code).map(|r| r["name"].as_str().unwrap_or_default().to_string()).collect()
    };
    assert_eq!(named("expl3-scope"), vec!["\\l_bad_scope_tl"]);
    assert_eq!(named("expl3-type"), vec!["\\l_bad_type_tl"]);
    assert_eq!(named("expl3-constant-set"), vec!["\\c_bad_const_tl"]);
}

/// hyperref's page anchor runs `\pdfstringdef` on an unknown `\thepage`
/// inside a group that redefines `\@protected@testopt`. The paths that
/// parted in there saved it in that group alike, so its end puts back the
/// kernel's macro. Joined with the inner value instead, every optional-
/// argument command re-entered itself up to `limits.expansion_depth`.
#[test]
fn a_group_both_paths_saved_in_restores_the_saved_meaning() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{hyperref}
\begin{document}
\setcounter{page}{\pdfuniformdeviate 5}
a\clearpage
\makeatletter
\edef\after{\meaning\@protected@testopt}
\end{document}",
    );
    assert_eq!(
        body(&analysis, "after"),
        r"macro:#1->\ifx \protect \@typeset@protect \expandafter \@testopt \else \@x@protect #1\fi "
    );
}

/// mwe's `example-image` ships with the installation: kpathsea finds it,
/// so `\includegraphics` raises no `missing-graphic`, and its box is the
/// PDF page's size (pdflatex: 321.19922pt by 240.8994pt).
#[test]
fn an_installed_image_is_found_and_sized() {
    if !installed() {
        return;
    }
    let source = r"\documentclass{article}
\usepackage{graphicx}
\begin{document}
\setbox0\hbox{\includegraphics{example-image}}\xdef\size{\the\wd0,\the\ht0}
\end{document}";
    crate::support::assert_lint(source, "missing-graphic", 0);
    assert_eq!(body(&analyze(source), "size"), "321.19922pt,240.8994pt");
}

/// `\draw` outside a `tikzpicture`: undefined there, defined only inside the picture.
#[test]
fn a_picture_command_outside_its_picture_says_where_it_is_defined() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\begin{tikzpicture}\\draw (0,0) -- (1,1);\\end{tikzpicture}\n\\draw[-]\n\\end{document}\n",
    );
    let found: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("undefined-control-sequence"))
        .map(|f| f["message"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(found, ["`\\draw` is undefined here: the run defines it only inside `tikzpicture` (line 4)"]);
}

/// LuaLaTeX's kernel leaves `para/end` a set of meanings, which the format
/// cache keeps: a `\par` ends in vertical mode, so the next one does not
/// fork on the mode.
#[test]
fn a_blank_preamble_line_keeps_the_mode_known() {
    if !installed() {
        return;
    }
    let source = "% !TeX program = lualatex\n\\def\\probe{}\n\n\\probe\n\n\\probe\n\\documentclass{article}\n\\begin{document}\n\\end{document}\n";
    let analysis = analyze(source);
    let probe = analysis.interner.lookup("probe").expect("interned");
    let modes: Vec<_> = analysis.facts.expansions.iter().filter(|e| e.name == probe).map(|e| e.mode).collect();
    assert!(!modes.is_empty());
    assert!(modes.iter().all(|m| *m == satex::engine::mode::Modes::VERTICAL), "{modes:?}");
}

/// LuaLaTeX's `\everyjob` sets `\c_sys_shell_escape_int` from Lua's
/// `status.shell_escape`: known, it is a `\chardef` on every path, so the
/// allocation counter and the registers allocated after it stay known.
#[test]
fn a_lualatex_register_allocation_stays_known() {
    if !installed() {
        return;
    }
    let source = "% !TeX program = lualatex\n\\documentclass{article}\n\\newcount\\probecount\n\\probecount=7\n\\edef\\probe{\\the\\probecount}\n\\begin{document}\n\\end{document}\n";
    assert_eq!(body(&analyze(source), "probe"), "7");
}

/// Text the preamble sets is the document's error where it stands: `\everypar`'s `\@nodocument`
/// raises `Missing \begin{document}` at the text, and a `#` in any mode is "You can't use `macro
/// parameter character #'" (tex.web § 1045), as pdflatex reports both.
#[test]
fn text_in_the_preamble_is_an_error_where_it_stands() {
    if !installed() {
        return;
    }
    let lines = |analysis: &Analysis| -> Vec<(u32, String)> {
        satex::linter::lint(analysis)
            .iter()
            .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("raised-error"))
            .filter_map(|f| Some((f.get("line")?.as_u64()? as u32, f.get("message")?.as_str()?.to_string())))
            .collect()
    };
    let text =
        analyze("\\documentclass{article}\n\\usepackage{amsmath}\nhello\n\\begin{document}\nx\n\\end{document}\n");
    let found = lines(&text);
    assert!(found.iter().any(|(l, m)| *l == 3 && m.contains("Missing \\begin{document}")), "{found:?}");
    let param = analyze("\\documentclass{article}\n\\begin{document}\n#x\n\\end{document}\n");
    let found = lines(&param);
    assert!(
        found.iter().any(|(l, m)| *l == 3 && m.contains("macro parameter character #' in vertical mode")),
        "{found:?}"
    );
}

/// A dead definition is two statements the file reads one after the other.
/// A default set inside a function that a key handler may override, and two
/// macros that each set a name, run per call: neither is dead.
#[test]
fn a_definition_inside_a_macro_body_is_never_dead() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\tl_new:N \l_my_group_tl
\keys_define:nn { my } { group .tl_set:N = \l_my_group_tl }
\cs_new_protected:Npn \my_prepare:n #1
  {
    \tl_set:Nn \l_my_group_tl { default }
    \keys_set:nn { my } {#1}
    \tl_use:N \l_my_group_tl
  }
\NewDocumentCommand \Prep { m } { \my_prepare:n {#1} }
\ExplSyntaxOff
\newcommand\seta{\def\x{a}}\newcommand\setb{\def\x{b}}
\def\y{a}\def\y{b}
\begin{document}
\Prep{group=a} \seta\setb\x \y
\end{document}",
    );
    // The default is read whenever no key overrides it: no unused name either.
    let names: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| matches!(f.get("code").and_then(|c| c.as_str()), Some("dead-definition" | "unused-definition")))
        .filter_map(|f| Some(format!("{} {}", f.get("code")?.as_str()?, f.get("name")?.as_str()?)))
        .collect();
    assert_eq!(names, [r"dead-definition \y"]);
}

/// `\providecommand` gives a fallback that an optional input replaces on purpose.
#[test]
fn a_provided_fallback_is_never_dead() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\providecommand\measured{?}
\renewcommand\measured{42}
\begin{document}
\measured
\end{document}",
    );
    let dead: Vec<_> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("dead-definition"))
        .collect();
    assert!(dead.is_empty(), "{dead:?}");
}

/// A package that builds a name per call (`\csname` from a key the call
/// gets) redefines it at every call: the document wrote none of those
/// definitions, so none is dead. The sankey package's per-node flags and a
/// citation package's per-key macros were reported at every call.
#[test]
fn names_a_package_builds_per_call_are_never_dead() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\makeatletter
\newcommand\flag[1]{\expandafter\gdef\csname my@flag@#1\endcsname{set}}
\makeatother
\begin{filecontents*}[overwrite]{flags.sty}
\ProvidesPackage{flags}
\newcommand\pkgflag[1]{\expandafter\gdef\csname pkg@flag@#1\endcsname{set}}
\end{filecontents*}
\usepackage{flags}
\begin{document}
\pkgflag{a}
\pkgflag{a}
\end{document}",
    );
    let dead: Vec<_> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("dead-definition"))
        .collect();
    assert!(dead.is_empty(), "{dead:?}");
}

/// `\include{k}` writes `\@setckpt{k}{` to the `.aux` and reads names built
/// from `k`, the shape a citation has (`\cite{k}` writes `\citation{k}`, reads
/// `\b@k`). Bug pinned: the included file's name was a citation, so the run
/// warned "`chapa` is cited, but no bibliography is named" and "no
/// bibliography entry declares `chapa`" (pdflatex: no warning). A key that
/// names the file its call loads is no citation.
#[test]
fn an_included_file_is_no_citation() {
    if !installed() {
        return;
    }
    let root =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-include-{}", std::process::id())));
    let path = root.join("main.tex");
    let source = r"\documentclass{report}
\begin{filecontents*}[overwrite]{chapa.tex}
\chapter{A}\label{ch:a}
\end{filecontents*}
\begin{filecontents*}[overwrite]{part.tex}
\end{filecontents*}
\begin{document}
\include{chapa}
\end{document}";
    std::fs::write(&path, source).unwrap();
    // the second build run reads the `.aux` the first wrote
    let analysis =
        satex::analysis::machine::Machine::analyze_build(source, Some(&path), &crate::support::discover(&root));
    let cites: Vec<_> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|f| {
            f.get("code").and_then(|c| c.as_str()).is_some_and(|c| c.contains("citation") || c.contains("bibliography"))
        })
        .collect();
    assert!(cites.is_empty(), "{cites:?}");
}

/// counttexruns reads the run count back with `\read` from the `.ctr` file each run
/// writes: a line that names no macro. latexmk sees the file change and runs again
/// until its limit. Bug pinned (audit): the build stopped after its second run, since
/// no line it changed named a name the run read, and reported the count of a build
/// latexmk never ends with. Now each run's new line sends the build round again.
#[test]
fn a_run_count_read_back_from_a_data_line_runs_the_build_again() {
    if !installed() {
        return;
    }
    let root =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-ctr-{}", std::process::id())));
    let path = root.join("main.tex");
    let source = r"\documentclass{article}
\usepackage{counttexruns}
\begin{document}
\ifnum\value{counttexruns}=1 \errmessage{one}\fi
\ifnum\value{counttexruns}>1 \errmessage{many}\fi
\end{document}";
    std::fs::write(&path, source).unwrap();
    let analysis =
        satex::analysis::machine::Machine::analyze_build(source, Some(&path), &crate::support::discover(&root));
    let limit = crate::support::discover(&root).limits.build_runs;
    assert_eq!(analysis.build_runs, u32::from(limit));
}

/// l3mark's `\int_compare:nNnT \tex_lastnodetype:D < 0` in memoir's output routine
/// reads the node type of a list whose tail satex does not know. Bug pinned (memoir):
/// an unknown engine quantity was any word, -2^31 included, which fails e-TeX's
/// expression check, so three possible errors no run has were said. A node type
/// stays in its own range.
#[test]
fn memoir_output_reads_no_overflowing_node_type() {
    if !installed() {
        return;
    }
    let root =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-memoir-{}", std::process::id())));
    let path = root.join("main.tex");
    std::fs::write(
        root.join("chap.tex"),
        r"\chapter{Two}\label{two}
See~\ref{one}.\footnote{A note.}
\begin{figure}[h!]\centering x\caption{A figure}\label{fig}\end{figure}
\begin{itemize}\item one\end{itemize}
",
    )
    .unwrap();
    let source = r"\documentclass{memoir}
\begin{document}
\tableofcontents
\chapter{One}\label{one}
Text.
\include{chap}
\end{document}";
    std::fs::write(&path, source).unwrap();
    let analysis =
        satex::analysis::machine::Machine::analyze_build(source, Some(&path), &crate::support::discover(&root));
    let raised = raised(&analysis);
    assert!(raised.is_empty(), "{raised:?}");
}

/// `\cite[p.~3` never closed: the optional argument the document began runs to the end of the
/// file inside library code (`\@citex`), and pdflatex stops with "File ended while scanning use
/// of \@citex@checkblank". Bug pinned (errcover): library code is assumed to compile, so the run
/// gave the text back and said nothing. The document opened what never closes: its error.
#[test]
fn an_optional_argument_the_document_never_closes_is_an_error() {
    if !installed() {
        return;
    }
    let analysis = analyze("\\documentclass{article}\n\\begin{document}\nSee \\cite[p.~3 here.\n\\end{document}\n");
    let raised = raised(&analysis);
    assert!(raised.iter().any(|s| s == "error"), "{raised:?}");
}

/// A here-float the output routine puts on its own list (`\box\@currbox` in `\@addtocurcol`)
/// reaches the page with that list: its caption is typeset, not lost to the next box built.
#[test]
fn a_here_float_set_by_the_output_routine_keeps_its_caption() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\begin{figure}[ht]\centering X\caption{Alpha}\end{figure}
\begin{figure}[ht]\centering Y\caption{Beta}\end{figure}
\end{document}",
    );
    use satex::queries::query::{Filter, Query, run};
    let marked: Vec<String> = run(&analysis, Query::Typeset, &Filter::Always)
        .iter()
        .filter_map(|r| r.get("marked").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    for want in ["X", "Figure 1: Alpha", "Y", "Figure 2: Beta"] {
        assert!(marked.iter().any(|m| m == want), "{want} missing in {marked:?}");
    }
}

/// Paths that part inside one line of a package and join again leave it
/// at rest: a joined token list is no call on the stack (`within`) and
/// counts what it holds (`held_tokens`). Those two counters kept the
/// parted lists, so no later line could be a checkpoint and tagpdf, read
/// whole in every run, cost 80 million steps instead of 20.
#[test]
fn a_package_stays_cacheable_past_a_line_that_joined_paths() {
    if !installed() {
        return;
    }
    let root = crate::support::tmp_root().join(format!("satex-join-rest-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let hex = "\\int_to_Hex:n{\\int_rand:n {16777215}}";
    std::fs::write(
        root.join("joinrest.sty"),
        format!("\\ProvidesExplPackage{{joinrest}}{{2020-01-01}}{{1}}{{x}}\n\\str_const:Ne \\c_x_str {{ {hex} {hex} }}\n\\exp_args:NNe \\str_set_convert:Nnnn \\l_tmpa_str {{\\c_x_str}}{{default}}{{utf8/string}}\n\\bool_new:N \\g_x_bool\n"),
    )
    .unwrap();
    let source = "\\documentclass{article}\\usepackage{joinrest}\\begin{document}x\\end{document}\n";
    let path = root.join("main.tex");
    std::fs::write(&path, source).unwrap();
    let cfg = crate::support::warm_config("join-rest");
    satex::analysis::machine::Machine::analyze(source, Some(&path), &cfg);
    let warm = satex::analysis::machine::Machine::analyze(source, Some(&path), &cfg);
    let state = warm.package_caches.iter().find(|(name, ..)| name == "joinrest").map(|(.., state)| state.clone());
    assert_eq!(state.as_deref(), Some("cached"));
    std::fs::remove_dir_all(root).ok();
}

/// A digit walker named after no package, the shape of every number parser:
/// a group, a loop over the characters of its argument, one result out of
/// the group.
const WALKER: &str = r"\documentclass{article}
\makeatletter
\def\walk#1{\begingroup\count@=0 \edef\walkin{#1}\expandafter\walk@\walkin\relax}
\def\walk@#1{\ifx#1\relax\expandafter\walk@end\else
  \ifnum`#1<`0 \else\ifnum`#1>`9 \else\multiply\count@ by 10 \advance\count@ by `#1 \advance\count@ by -`0 \fi\fi
  \expandafter\walk@\fi}
\def\walk@end{\edef\x{\endgroup\def\noexpand\walkresult{\the\count@}}\x}
\makeatother
\setbox0\hbox{\the\pdfrandomseed}
\begin{document}
";

fn learned(analysis: &Analysis) -> usize {
    analysis.facts.diagnostics.iter().filter(|d| d.code == "call-learned-effect").count()
}

/// Two runs on known digits teach the walker's effect: over a box's printed
/// width it leaves a number without walking the digits (the walk forked per
/// digit, the bug behind every parser red), reported as the named assumption.
/// Known operands still run the real code, before and after: their results
/// are exact. The width reaches the walker only through `\the`, no argument
/// holds it as unknown text.
#[test]
fn a_walker_over_an_unknown_number_leaves_what_its_runs_left() {
    if !installed() {
        return;
    }
    let analysis = analyze(&format!(
        r"{WALKER}\relax\walk{{12}}\typeout{{mark \walkresult}}\walk{{345}}\typeout{{mark \walkresult}}
\walk{{\the\wd0}}\dimen2=\walkresult pt\gdef\AfterWalk{{}}
\walk{{678}}\typeout{{mark \walkresult}}
\end{{document}}"
    ));
    assert_eq!(marks(&analysis), ["mark 12", "mark 345", "mark 678"]);
    assert_eq!(learned(&analysis), 1);
    // The arms up to the cut, which all leave one effect.
    let arms = 1 << crate::support::config().limits.call_learn_forks;
    assert!(analysis.peak_paths <= arms, "{} paths", analysis.peak_paths);
    let wrong: Vec<_> = analysis.facts.diagnostics.iter().filter(|d| d.code == "missing-number").collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}

/// With no run to learn from there is nothing to assume: the call walks.
#[test]
fn a_walker_without_known_runs_learns_nothing() {
    if !installed() {
        return;
    }
    let analysis = analyze(&format!("{WALKER}\\walk{{\\the\\wd0}}\n\\end{{document}}"));
    assert_eq!(learned(&analysis), 0);
}

/// One argument twice proves nothing about other arguments: runs must differ
/// to teach, so a name a run derives from its argument is not taken for fixed.
#[test]
fn runs_on_one_argument_teach_nothing() {
    if !installed() {
        return;
    }
    let analysis = analyze(&format!("{WALKER}\\relax\\walk{{12}}\\walk{{12}}\\walk{{\\the\\wd0}}\n\\end{{document}}"));
    assert_eq!(learned(&analysis), 0);
}

/// A macro that writes text has no type its value could widen to: its real
/// run forks and keeps both arms, however many runs came before. The learned
/// effect here would have been the one arm the known runs took, unsound.
#[test]
fn a_choice_of_text_is_not_learned() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\def\choose#1{\ifnum#1>0 \def\a{x}\else\def\a{y}\fi\ifnum#1>5 \def\b{x}\else\def\b{y}\fi}
\setbox0\hbox{\the\pdfrandomseed}
\begin{document}
\choose{1}\choose{2}\choose{\wd0}
\def\x{x}\def\y{y}\ifx\a\x\gdef\TookX{}\fi\ifx\a\y\gdef\TookY{}\fi
\end{document}",
    );
    assert_eq!(learned(&analysis), 0);
    for name in ["TookX", "TookY"] {
        assert!(!undefined(&analysis, name), "{name} lost an arm");
    }
}

/// The assumption is the user's to switch off: then the walker walks, and
/// nothing reports an effect no run of this call had.
#[test]
fn a_learned_effect_can_be_switched_off() {
    if !installed() {
        return;
    }
    let source =
        format!("{WALKER}\\relax\\walk{{12}}\\walk{{345}}\\walk{{678}}\\walk{{\\the\\wd0}}\n\\end{{document}}");
    let mut cfg = crate::support::config();
    cfg.analysis.assume_call_learned_effect = false;
    assert_eq!(learned(&Machine::analyze(&source, None, &cfg)), 0);
}

/// A missing `.tex` file asks for a name like a missing package does. The
/// prompt counts as answered with Return, so what follows still runs.
/// Real: a thesis ended without a word at the first file its class lost.
#[test]
fn a_missing_input_file_does_not_end_the_run() {
    if !crate::support::installed() {
        return;
    }
    let analysis = crate::support::analyze_latex(
        "\\documentclass{article}\\batchmode\\input{no-such-file}\\begin{document}x\\gdef\\Done{}\\end{document}",
    );
    assert!(crate::support::defined(&analysis, "Done"), "the run stopped at the prompt");
}

/// `\LT@get@widths` walks the last row back with `\unskip` and `\lastbox`
/// until no box is left. satex does not know how many cells the row holds,
/// so each `\ifhbox` is undecided, and the arm with one box more than the
/// columns ran `\LT@max@sel` on an empty `\@tempa`. That arm rests on a box
/// the run cannot see: longtable compiles, so it is no run of the document.
/// The bug: a plain two-column longtable was a "Paragraph ended" error.
#[test]
fn a_longtable_walks_its_row_without_a_runaway() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\\usepackage{longtable}\\begin{document}\n\\begin{longtable}{ll}\na & X \\\\\n\\end{longtable}\n\\gdef\\Done{}\\end{document}",
    );
    assert!(defined(&analysis, "Done"));
    let bad: Vec<_> = codes(&analysis).into_iter().filter(|c| c.contains("error") || c.contains("runaway")).collect();
    assert!(bad.is_empty(), "{bad:?}");
}

/// tex.web § 799: a finished `\halign` row is tabskip glue, then a cell box
/// and glue per column it reached (a `\span`ned cell counts each column), so
/// `\unskip` and `\lastbox` walk it back cell by cell, as longtable's
/// `\LT@get@widths` counts its columns. The bug: a row was only "some item",
/// so the box `\lastbox` took from the list and `\unhbox` gave back had an
/// unknown tail past the first item, and the walk forked into paths with
/// more or fewer cells (`extra-alignment-tab` later). Six columns are 13
/// items, past the tail window satex keeps.
#[test]
fn a_row_box_unpacked_walks_back_cell_by_cell() {
    let rows = [("a", "#&#&#&#&#&#", "a&b&c&d&e&f", "6"), ("b", "#&#&#", "a\\span b&c", "3"), ("c", "#&#&#", "a", "1")];
    let mut src = String::from(
        "\\catcode`\\&=4 \\countdef\\n=10 \\def\\walk{\\unskip\\setbox3\\lastbox\\ifhbox3 \\global\\advance\\n1 \\expandafter\\walk\\fi}",
    );
    for (name, preamble, row, _) in rows {
        src.push_str(&format!(
            "\\setbox0\\vbox{{\\halign{{{preamble}\\cr {row}\\cr}}\\global\\setbox1\\lastbox}}\\setbox2\\hbox{{\\unhbox1 \\global\\n=0 \\walk}}\\xdef\\N{name}{{\\the\\n}}"
        ));
    }
    // A cell's mark follows its row (§ 796): `\lastbox` finds no box, never a row.
    src.push_str(
        "\\setbox0\\vbox{\\halign{#\\cr\\mark{m}a\\cr}\\global\\setbox1\\lastbox}\\xdef\\M{\\ifhbox1 row\\fi}",
    );
    let analysis = crate::support::analyze_plain(&src);
    for (name, preamble, row, want) in rows {
        assert_eq!(body(&analysis, &format!("N{name}")), want, "{preamble}: {row}");
    }
    assert!(maybe_defined(&analysis, "M") && body(&analysis, "M") != "row", "{}", body(&analysis, "M"));
}

/// A longtable with `\endfirsthead` and `\endhead` under `array` replays
/// the saved row (`\LT@save@row`, one `\LT@entry` per column) at each
/// chunk end. TeX's row has exactly the columns, so `\LT@entry`'s `&`
/// never comes extra. The bug: satex met `extra-alignment-tab` at
/// longtable.sty's `\LT@entry`, and a thesis's later `tikzpicture` lost
/// its group: the row walk forked into paths with fewer or more cells, as
/// the row box it took apart had an unknown tail (see
/// `a_row_box_unpacked_walks_back_cell_by_cell`).
#[test]
fn a_longtable_with_heads_has_no_extra_tab() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\\usepackage{array}\\usepackage{longtable}\\begin{document}\n\\begin{longtable}{ll}\na & B \\\\\n\\endfirsthead\na & B \\\\\n\\endhead\nc & X \\\\\n\\end{longtable}\n\\gdef\\Done{}\\end{document}",
    );
    assert!(defined(&analysis, "Done"));
    assert_eq!(broken_gaps(&analysis), Vec::<String>::new());
}

/// The preamble observer runs each line in one sandbox and drops what a line
/// its budget stopped left on the input. The calls it dropped must stop
/// counting as open: they once stayed counted, so after `expansion_depth`
/// lines stopped inside `\wrap`, the next `\wrap` was cut as a runaway
/// recursion and its `\gdef` never ran.
#[test]
fn lines_stopped_inside_a_call_leave_no_open_expansion() {
    if !installed() {
        return;
    }
    let analysis =
        analyze("\\documentclass{article}\n\\def\\wrap#1{#1\\gdef\\made{}}\n\\begin{document}\n\\end{document}\n");
    let mut cfg = crate::support::config();
    (cfg.limits.replay_steps, cfg.limits.expansion_depth) = (2_000, 8);
    let state = satex::analysis::probe::State::Preamble;
    let mut sandbox = Machine::sandbox(&cfg, &analysis, state).expect("a preamble state");
    for _ in 0..=cfg.limits.expansion_depth {
        sandbox.run_line("\\wrap{\\count255=0 \\loop\\advance\\count255 1 \\ifnum\\count255<100000 \\repeat}");
    }
    assert_eq!(sandbox.run_line("\\wrap{}"), vec!["made".to_string()]);
}

/// `\int_case:nn` ends its list with the value itself as a sentinel, so the
/// last test compares an unknown number with itself. Both sides read one
/// number, so the test is true. It was undecided, and the false arm ran past
/// the list into `\s__int_mark`: false library errors for every
/// `\int_case:nn` and `\int_to_alph:n` of an unknown value.
#[test]
fn a_number_compared_with_itself_is_equal() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\ExplSyntaxOn\n\\tl_set:Nx \\l_tmpa_tl { \\int_eval:n { \\pdfuniformdeviate 8 + 1 } }\n\\tl_set:Nx \\l_tmpb_tl { \\int_case:nn { \\l_tmpa_tl } { {50}{a} } }\n\\tl_set:Nx \\l_tmpb_tl { \\int_to_alph:n { \\l_tmpa_tl + 1 } }\n\\ExplSyntaxOff\n\\begin{document}\nx\n\\end{document}\n",
    );
    let errors: Vec<String> = codes(&analysis).into_iter().filter(|c| c.contains("error")).collect();
    assert!(errors.is_empty(), "{errors:?}");
}

/// tex.web §§ 977, 1091, 1110: `\vsplit` splits the list a vbox recorded,
/// so what is left after it is known. The record follows an hbox on the
/// vertical list (it moves no ins or mark), a one-line paragraph below
/// its interline glue, and `\unvbox`, which adds no interline glue and
/// keeps `\prevdepth`. Pins scrlayer-notecolumn's `\clearnotecolumns`
/// loop: the rest of a split note box was unknown, so the loop never ended.
/// Values as `pdflatex` prints them.
#[test]
fn split_rest_of_recorded_lines_and_unvboxed_lists_is_known() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\splittopskip=10pt \splitmaxdepth=4pt
\setbox1\vbox{\kern 700pt\hbox{\strut A}}\setbox0\vsplit1 to 500pt \xdef\Ra{\ifvoid1 v\else n\fi}
\setbox1\vbox{\hsize3cm\kern 700pt\strut A\par}\setbox0\vsplit1 to 500pt \xdef\Rb{\ifvoid1 v\else n,\the\ht1\fi}
\setbox1\vbox{\kern 1pt}\setbox1\vbox{\hsize3cm\unvbox1\kern 700pt\strut A\par}\setbox0\vsplit1 to 500pt \xdef\Rc{\ifvoid1 v\else n,\the\ht1\fi}
\setbox1\vbox{\hsize3cm\kern 700pt\strut A\par}\setbox0\vsplit1 to 500pt \setbox1\vbox{\pdfcolorstack0 current\relax\unvbox1}\xdef\Rd{\the\ht1}
\setbox1\vbox{\hbox{\vrule height5pt depth3pt}}\setbox2\vbox{\unvbox1\hbox{\vrule height5pt depth3pt}}\xdef\Re{\the\ht2,\the\dp2}
\end{document}",
    );
    let got: Vec<String> = ["Ra", "Rb", "Rc", "Rd", "Re"].iter().map(|k| body(&analysis, k)).collect();
    assert_eq!(got, ["v", "n,10.0pt", "n,10.0pt", "10.0pt", "13.0pt,3.0pt"]);
}

/// A `}` the document closes nothing with on one of two paths (a random
/// number decides): TeX may say "Too many }'s" there. Such an error is
/// reported as possible, a warning, never dropped. Before, only an error on
/// the one path the run followed exactly was reported.
#[test]
fn a_document_error_on_one_path_is_reported_as_possible() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        "\\documentclass{article}\n\\begin{document}\nText.\n\\ifnum\\pdfuniformdeviate 2=0 }\\fi\nMore.\n\\end{document}\n",
    );
    assert_eq!(raised(&analysis), ["warning"]);
}

/// An error TeX raises while it only expands or computes is an error
/// finding too, as pdflatex stops on each (its `!` line in the comment).
/// Pins three classes no rule reported: an undefined name inside an
/// expanding scan (`\edef`, `\message`: only the stomach reported them), a
/// call whose text does not match the parameter text (the call ran on with
/// the arguments read so far, and so did every expandable expl3 error,
/// which is raised this way), arithmetic past TeX's limits and a list's
/// own quantity read in another mode (the value was right, the error
/// silent).
#[test]
fn errors_met_while_expanding_or_computing_are_error_findings() {
    if !installed() {
        return;
    }
    let cases = [
        r"\edef\x{\undefinedzz}",                                      // Undefined control sequence.
        r"\message{\undefinedzz}",                                     // Undefined control sequence.
        r"\def\a.#1{}\a x",                                            // Use of \a doesn't match its definition.
        r"\def\a.#1{}\edef\x{\a y}",                                   // Use of \a doesn't match its definition.
        r"\ExplSyntaxOn\edef\y{\fp_eval:n{(-8)^(1/3)}}\ExplSyntaxOff", // Use of \??? doesn't match its definition.
        r"\edef\x{\the\numexpr 2147483647+1\relax}",                   // Arithmetic overflow.
        r"\edef\x{\the\dimexpr 8192pt*2\relax}",                       // Arithmetic overflow.
        r"\count1=5 \divide\count1 by 0 ",                             // Arithmetic overflow.
        r"\count1=2147483647 \multiply\count1 by 2 ",                  // Arithmetic overflow.
        r"\dimen0=10000pt \multiply\dimen0 by 2 ",                     // Arithmetic overflow.
        r"\count1=2147483648 ",                                        // Number too big.
        r"\dimen0=16384pt ",                                           // Dimension too large.
        r"\dimen0=6000mm ",                                            // Dimension too large.
        r"\par\count1=\spacefactor ",                                  // Improper \spacefactor.
        r"x\dimen0=\prevdepth ",                                       // Improper \prevdepth.
    ];
    for body in cases {
        let analysis =
            analyze(&format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"));
        let errors = satex::linter::lint(&analysis).into_iter().filter(|f| f["severity"] == "error" && f["line"] == 3);
        assert!(errors.count() > 0, "{body}: no error, {:?}", codes(&analysis));
    }
}

/// tex.web §§ 1151, 403 `scan_math`: a script, `\radical` (`\sqrt`),
/// `\mathaccent` (`\bar`), `\underline` or `\mathord` takes the next token
/// after expansion as its operand; anything but a character, a math character
/// or a `{` is "Missing { inserted", and TeX reads it again inside the math
/// group it opened, so the formula's `$` then finds that group open: "Missing
/// } inserted" (both are pdflatex's `!` lines for each case). § 1161: `\left`
/// before no delimiter is "Missing delimiter (. inserted)". Bug pinned: no
/// rule reported these (the playground's PDF said 2 errors, its findings 0),
/// the second error at the same place was dropped as a duplicate, and
/// `\left{` swallowed its `{`.
#[test]
fn a_math_operand_tex_rejects_is_a_missing_brace_error() {
    if !installed() {
        return;
    }
    let brace = "Missing { inserted";
    let closer = "Missing } inserted";
    let cases: [(&str, &[&str]); 9] = [
        (r"\(\sum_ \)", &[brace, closer]),
        // `'` is math-active: its definition starts with `^` (tex.web § 1151)
        (r"$x^'$", &[brace, closer]),
        (r"$x_$", &[brace, closer]),
        (r"$x^\hbox{a}$", &[brace]),
        (r"$x_^2$", &[brace]),
        (r"$\sqrt\hbox{a}$", &[brace]),
        (r"$\bar\hbox{a}$", &[brace]),
        (r"$\left\hbox{}\right.$", &["Missing delimiter (. inserted)"]),
        (r"$\left{a\right.$", &["Missing delimiter (. inserted)", closer]),
    ];
    for (body, expected) in cases {
        let analysis =
            analyze(&format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"));
        let errors: Vec<String> = satex::linter::lint(&analysis)
            .into_iter()
            .filter(|f| f["severity"] == "error" && f["line"] == 3)
            .filter_map(|f| f["message"].as_str().map(str::to_string))
            .collect();
        for want in expected {
            assert!(errors.iter().any(|e| e.starts_with(want)), "{body}: no {want:?} in {errors:?}");
        }
    }
}

/// A recursion that re-enters itself with the same arguments, in the same
/// state, on the last round's unread input never returns: TeX's stacks
/// overflow, "TeX capacity exceeded" (tex.web § 94), a fatal error. Typing
/// `\begin` or `see \ref` just before `\end{document}` makes LaTeX's `\end`
/// do that (pdflatex: "Missing \endcsname inserted", then "TeX capacity
/// exceeded, sorry [parameter stack size=2000]"). A recursion that counts
/// towards its end, nested deeper each round, stays clean. Bug pinned: the
/// run widened the recursion silently, and a doubted note at the same place
/// hid TeX's certain errors, so the findings showed none of them.
#[test]
fn a_recursion_that_never_returns_exceeds_tex_s_capacity() {
    if !installed() {
        return;
    }
    let doc = |body: &str| format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let errors = |body: &str| -> Vec<String> {
        satex::linter::lint(&analyze(&doc(body)))
            .into_iter()
            .filter(|f| f["severity"] == "error")
            .filter_map(|f| f["message"].as_str().map(str::to_string))
            .collect()
    };
    let capacity = "TeX capacity exceeded";
    for (body, also) in
        [(r"\begin", Some("Missing \\endcsname inserted")), (r"see \ref", None), (r"\def\a{\a x}\a", None)]
    {
        let found = errors(body);
        assert!(found.iter().any(|e| e.starts_with(capacity)), "{body}: {found:?}");
        if let Some(also) = also {
            assert!(found.iter().any(|e| e.starts_with(also)), "{body}: {found:?}");
        }
    }
    let counted = r"\newcounter{c}\def\a{\stepcounter{c}\ifnum\value{c}<9 {\a}\fi}\a";
    assert!(errors(counted).is_empty(), "{:?}", errors(counted));
}

/// Another rule's finding at the place of a raised TeX error stands for one
/// TeX error, never for all of them: `\[ \frac{a}{b} = \sqrt` before
/// `\end{document}` gives pdflatex's "Missing { inserted" beside the
/// environment mismatch. Bug pinned: any correctness finding at the place,
/// a doubted note too, hid every raised error there.
#[test]
fn a_finding_at_a_raised_error_s_place_hides_only_its_own_error() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\[ \\frac{a}{b} = \\sqrt\n\\end{document}\n";
    let findings = satex::linter::lint(&analyze(source));
    let has = |code: &str, text: &str| {
        findings.iter().any(|f| f["code"] == code && f["message"].as_str().is_some_and(|m| m.starts_with(text)))
    };
    assert!(has("raised-error", "Missing { inserted"), "{findings:?}");
    assert!(has("environment-mismatch", "LaTeX Error"), "{findings:?}");
}

/// The operands TeX takes (pdflatex: no error): expansion, spaces and
/// `\relax` come first, an implicit `{`, a `\mathchardef` or `\mathchar`, a
/// macro opening a group (`\mathrm`, amsmath's `\text`, `\frac`) and a
/// conditional's arm all count. Pins that the `scan_math` check reports none
/// of them, so the check reads meanings and expansion, not spelling.
#[test]
fn a_math_operand_tex_takes_raises_nothing() {
    if !installed() {
        return;
    }
    let body = r"$x_\mathrm{a}$ $x_\alpha$ $x^\prime$ $x_\text{a}$ $x^{ab}$ $x_\bgroup a\egroup$ $x_ 1$
$x_\relax 1$ $x^\char`a$ $x^\mathchar'141$ $\sqrt{x}^2$ $x^{}$ $x'$ $x^\protect\alpha$ $\overline{x}_a$
$\left(x\right)^2$ $\underline x$ $\mathbin x$ $\sqrt\relax x$ $\bar x \hat{y} \vec\alpha \sqrt[3]{x}$
$\left\{ x \right\} \bigl( x \bigr) \left\langle x \right\rangle \left| x \right.$
$\left\relax( x \right)$ $\frac{a}{b}^2 x_\frac12$ $x^\ifnum1=1 a\else b\fi$
\newcommand\sq[1]{#1^2}$\sq{x}\sq\alpha$ \let\up=^ $x\up2$";
    let analysis = analyze(&format!(
        "\\documentclass{{article}}\n\\usepackage{{amsmath}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"
    ));
    let findings: Vec<_> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|f| f["severity"] == "error" || f["code"] == "raised-error")
        .collect();
    assert!(findings.is_empty(), "{findings:?}");
}

/// `\@alph` is an `\ifcase` with 26 arms and `\@ctrerr` in its `\else`:
/// a counter in 27..29 takes the `\else` on every run, so "Counter too
/// large" is certain (`pdflatex`: the error for each seed). Bug pinned: an
/// interval past the last `\or` was split as undecided into its one arm, and
/// the error was only possible.
#[test]
fn an_ifcase_selector_past_its_last_or_takes_the_else_for_certain() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\newcounter{c}\n\\begin{document}\n\\setcounter{c}{\\numexpr27+\\pdfuniformdeviate 3\\relax}\\alph{c}\n\\end{document}\n";
    let analysis = analyze(source);
    let errors = satex::linter::lint(&analysis).into_iter().filter(|f| f["severity"] == "error" && f["line"] == 4);
    assert!(errors.count() > 0, "{:?}", codes(&analysis));
}

/// l3fp packs an operand into blocks `1dddd` and takes them apart with
/// `1#3\__fp_sep:`: the `1` of the delimiter matched against typed digits
/// 11000..13999 leaves exactly four places, a slice of that number.
/// `pdflatex`: `\fp_eval:n{1+x}` for x in 1..3 is 2, 3 or 4. Bug pinned: the
/// rest became digits of open length, a later `\int_value:w` of the blocks
/// was any number, read as one token by a one-digit test, and the result ran
/// into the file: `\R` was `\par` for certain, with a false "Too many }'s".
#[test]
fn an_fp_sum_of_a_bounded_unknown_integer_stays_a_number() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\ExplSyntaxOn\n\\int_set:Nn\\l_tmpa_int{\\int_rand:nn{1}{3}}\\xdef\\R{\\fp_eval:n{1+\\l_tmpa_int}}\n\\ExplSyntaxOff\n\\begin{document}\n\\end{document}\n";
    let analysis = analyze(source);
    let r = body(&analysis, "R");
    assert!(!r.contains("par") && !r.is_empty(), "{r:?}");
    let errors = satex::linter::lint(&analysis).into_iter().filter(|f| f["severity"] == "error").count();
    assert_eq!(errors, 0, "{:?}", codes(&analysis));
}

/// l3fp's own `\numexpr`s over an unknown quotient's digits may overflow on
/// satex's wide intervals, never on pdflatex's runs (0..299 / 7). Bug pinned:
/// that library-internal maybe-overflow was said at the document line as a
/// possible "Arithmetic overflow".
#[test]
fn library_arithmetic_over_an_unknown_quotient_is_no_possible_overflow() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\ExplSyntaxOn\n\\fp_eval:n { round(\\pdfuniformdeviate 300/7,2) }\n\\ExplSyntaxOff\n\\end{document}\n";
    let found: Vec<_> = satex::linter::lint(&analyze(source))
        .into_iter()
        .filter_map(|f| f.get("message")?.as_str().map(str::to_string))
        .collect();
    assert!(!found.iter().any(|m| m.contains("Arithmetic overflow")), "{found:?}");
}

/// A `\numexpr` some members of an interval overflow (0..1 times 2^31) is a
/// possible error when the document writes it, and none when library code
/// does (`\int_eval:n`'s own `\numexpr`): library code is assumed to compile.
/// Bug pinned: the test of whose code it is read the file being read, which
/// is the document's while any library macro runs from it, so l3mark's
/// `\numexpr\lastnodetype` in memoir's output routine said a possible
/// "Arithmetic overflow" at each `\chapter` (classes::memoir).
#[test]
fn a_possible_overflow_is_said_for_the_document_code_only() {
    if !installed() {
        return;
    }
    for (body, said) in [
        (r"\count2=\numexpr\pdfuniformdeviate 2*1073741824*2\relax", true),
        (r"\ExplSyntaxOn\int_set:Nn\l_tmpa_int{\pdfuniformdeviate 2*1073741824*2}\ExplSyntaxOff", false),
    ] {
        let source = format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
        let found: Vec<_> = satex::linter::lint(&analyze(&source))
            .into_iter()
            .filter_map(|f| f.get("message")?.as_str().map(str::to_string))
            .collect();
        assert_eq!(found.iter().any(|m| m.contains("Arithmetic overflow")), said, "{body}: {found:?}");
    }
}

/// Arithmetic on an interval errs on every run when every value of it does:
/// `\multiply` and `\numexpr` of 2..3 by 2^30, a `\dimexpr` past `\maxdimen`
/// for every 0..2pt (pdflatex: "Arithmetic overflow" for each seed). Where
/// some value stays in range (0..2 times 2^30) no error is certain. Bug
/// pinned: the interval lift clipped every corner to the bound and only
/// flagged "may err", so a certain overflow was silent.
#[test]
fn arithmetic_that_overflows_for_every_value_of_an_interval_is_an_error() {
    if !installed() {
        return;
    }
    let two = r"\count1=\pdfuniformdeviate 2 \advance\count1 2 ";
    let cases = [
        (format!(r"{two}\multiply\count1 by 1073741824 "), true),
        (format!(r"{two}\count2=\numexpr\count1*1073741824\relax"), true),
        (r"\dimen0=\pdfuniformdeviate 3pt \dimen1=\dimexpr 16383.99998pt+\dimen0+1sp\relax".to_string(), true),
        (r"\count1=\pdfuniformdeviate 3 \multiply\count1 by 1073741824 ".to_string(), false),
        (r"\count1=\pdfuniformdeviate 3 \count2=\numexpr\count1*1073741824\relax".to_string(), false),
    ];
    for (body, certain) in cases {
        let analysis =
            analyze(&format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"));
        let errors = satex::linter::lint(&analysis).into_iter().filter(|f| f["severity"] == "error" && f["line"] == 3);
        assert_eq!(errors.count() > 0, certain, "{body}: {:?}", codes(&analysis));
    }
}

/// tex.web § 398: a call that does not match its parameter text is dropped
/// with the token that did not match. `pdflatex`: `\v` is `Z`. The call ran
/// on with no argument and the token stayed: `XyZ`.
#[test]
fn a_call_that_does_not_match_is_dropped_with_its_token() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\def\\a.#1{X}\\edef\\v{\\a yZ}\n\\end{document}\n";
    assert_eq!(body(&analyze(source), "v"), "Z");
}

/// `%&own` names a format mylatexformat dumped from the document's own
/// preamble (`pdftex -ini -jobname=own "&pdflatex" mylatexformat.ltx own.tex`,
/// what `preamble-cost` suggests). Started from it, the document's
/// `\documentclass` is mylatexformat's, which skips the dumped preamble.
/// Bug pinned: satex read the dumped part as the format and then the whole
/// file again, so the second `\documentclass` raised "Two \documentclass"
/// on a document pdflatex builds.
#[test]
fn a_document_started_from_its_own_dumped_preamble_has_one_documentclass() {
    if !crate::support::installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-latex-own-format-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = "%&own\n\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";
    let path = dir.join("own.tex");
    std::fs::write(&path, source).unwrap();
    let cfg = satex::project::config::Config {
        load: satex::project::config::Load { classes: true, ..Default::default() },
        ..crate::support::config()
    };
    let analysis = satex::analysis::machine::Machine::analyze(source, Some(&path), &cfg);
    let errors: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|r| r["code"] == "raised-error")
        .map(|r| r["message"].to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

/// Each LaTeX error here is raised on one arm of an undecided `\if` only:
/// pdflatex with `\pdfsetrandomseed 1` raises none of them, with seed 2
/// each. The specific rules that report these errors in place of
/// `raised-error` must keep its certainty: possible, not an error. Pins:
/// `missing-graphic`, `undefined-environment`, `already-defined`,
/// `not-defined`, `option-clash`, `math-still-open` and
/// `undefined-glossary-entry` reported them as certain errors.
#[test]
fn a_latex_error_on_one_undecided_arm_is_no_certain_error() {
    if !installed() {
        return;
    }
    let arm = "\\ifnum\\pdfuniformdeviate2=0 ";
    let cases = [
        ("missing-graphic", "\\usepackage{graphicx}".to_string(), format!("{arm}\\includegraphics{{nofile}}\\fi")),
        ("undefined-environment", String::new(), format!("{arm}\\begin{{noenv}}\\end{{noenv}}\\fi")),
        ("already-defined", format!("{arm}\\newcommand\\foo{{}}\\fi \\newcommand\\foo{{}}"), String::new()),
        ("already-defined", String::new(), format!("{arm}\\newenvironment{{itemize}}{{}}{{}}\\fi")),
        ("not-defined", format!("{arm}\\renewcommand\\nothere{{}}\\fi"), String::new()),
        (
            "option-clash",
            format!("\\usepackage[a4paper]{{geometry}}{arm}\\usepackage[letterpaper]{{geometry}}\\fi"),
            String::new(),
        ),
        ("math-still-open", String::new(), format!("{arm}$x\\fi y")),
        (
            "undefined-glossary-entry",
            "\\usepackage{glossaries}\\makeglossaries".to_string(),
            format!("{arm}\\newglossaryentry{{e}}{{name=e,description=d}}\\fi\\gls{{e}}"),
        ),
    ];
    for (code, preamble, body) in cases {
        let source =
            format!("\\documentclass{{article}}\n{preamble}\n\\begin{{document}}\nx\n{body}\n\\end{{document}}\n");
        let findings = satex::linter::lint(&crate::support::analyze_latex(&source));
        let wrong: Vec<_> = findings
            .iter()
            .filter(|f| f.get("severity").and_then(|s| s.as_str()) == Some("error"))
            .map(|f| f.get("message").and_then(|m| m.as_str()).unwrap_or_default().to_string())
            .collect();
        assert!(wrong.is_empty(), "{code}: one run raises nothing: {wrong:?}");
    }
}

/// `\fp_eval:n` of `ops` in an `\xdef`, then `\ifx` of the result against each
/// of `values` (the results TeX gives on some run): the names whose test may
/// hold. A value lost from `\Ra` leaves its probe undefined.
fn fp_values_lost(set: &str, ops: &str, values: &[&str]) -> Vec<String> {
    let probes: String = values
        .iter()
        .enumerate()
        .map(|(i, v)| format!("\\def\\v{{{v}}}\\ifx\\Ra\\v\\expandafter\\gdef\\csname R{i}\\endcsname{{}}\\fi\n"))
        .collect();
    let source = format!(
        "\\documentclass{{article}}\n\\ExplSyntaxOn\n{set}\\xdef\\Ra{{\\fp_eval:n{{{ops}}}}}\n\\ExplSyntaxOff\n{probes}\\begin{{document}}\n\\end{{document}}\n"
    );
    let analysis = analyze(&source);
    (0..values.len()).map(|i| format!("R{i}")).filter(|n| !maybe_defined(&analysis, n)).collect()
}

/// Intuition: a product of two random integers 1..3 is one of 1, 2, 3, 4, 6, 9, so
/// `\Ra` may be each. Bug pinned (H5): l3fp reads its operand blocks with
/// delimited arguments over typed digits; the read forks, the last reruns run
/// away and are refuted, and the refutation flag of that last rerun stayed set
/// after the drive returned the feasible arm. The forking command ended the
/// `\int_value:w` scan it ran in, so the enclosing run took itself for refuted:
/// every path was dropped and `\Ra` was the empty text, its only value.
#[test]
fn a_product_of_two_unknown_integers_keeps_every_value() {
    if !installed() {
        return;
    }
    let set = r"\int_set:Nn\l_tmpa_int{\int_rand:nn{1}{3}}\int_set:Nn\l_tmpb_int{\int_rand:nn{1}{3}}";
    let lost = fp_values_lost(set, r"\l_tmpa_int*\l_tmpb_int", &["1", "2", "3", "4", "6", "9"]);
    assert!(lost.is_empty(), "values lost: {lost:?}");
}

/// Intuition: `1/x` for x in -1..1 is -1 or 1 (and for 0 an error and a value).
/// Bug pinned (H6): the run reruns the `\xdef` per path a look ahead left
/// apart; a rerun that ended with fewer paths than the run it repeats dropped
/// the arms past them as infeasible (explore.rs, the replayed-arm `no_arm`),
/// so only `-1` stayed.
#[test]
fn a_reciprocal_of_an_unknown_integer_keeps_every_value() {
    if !installed() {
        return;
    }
    let set = r"\int_set:Nn\l_tmpa_int{\int_rand:nn{-1}{1}}";
    let lost = fp_values_lost(set, r"1/\l_tmpa_int", &["-1", "1"]);
    assert!(lost.is_empty(), "values lost: {lost:?}");
}

/// Intuition: `\vref[see]{x}` reads `[` as the label, since cleveref's
/// `\vref` has no optional argument. Pins a finding that says so: the old fix told the
/// author to "add `\label{[}`", absurd advice for a misread argument.
#[test]
fn a_bracket_read_as_a_label_says_the_optional_argument_is_wrong() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{varioref}\usepackage{hyperref}\usepackage{cleveref}
\begin{document}
\section{A}\label{s:a}\vref[see]{s:a}.
\end{document}",
    );
    let found: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("undefined-reference"))
        .map(|f| format!("{} | {}", f["message"].as_str().unwrap_or_default(), f["fix"].as_str().unwrap_or_default()))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("no optional argument") && !found[0].contains("\\label{[}"), "{found:?}");
}

/// Intuition: a name the run could not compute (`\ref{sec:\s}` in a loop the
/// analysis summarized) is held under a mark of satex's own. Pins that no
/// finding shows that mark: the bug was `\^^Bunknown name:r@sec:^^G` and raw
/// control characters in messages meant for the author.
#[test]
fn no_finding_shows_the_mark_of_an_unknown_name() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{pgffor,enumitem,hyperref}
\begin{document}
\foreach \s in {1,...,60}{\section{S \s}\label{sec:\s}\begin{enumerate}\foreach \i in {1,...,6}{\item I \i, see~\ref{sec:\s}.}\end{enumerate}}
\end{document}",
    );
    let shown: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .flat_map(|f| {
            ["message", "name", "fix"].map(|k| f.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string())
        })
        .filter(|t| {
            t.contains("unknown name") || t.contains("^^B") || t.chars().any(|c| c.is_control() && !c.is_whitespace())
        })
        .collect();
    assert!(shown.is_empty(), "{shown:?}");
}

/// Intuition: a cell's own braces count from where its `u` template ends (tex.web § 324), whatever that
/// template's expansion read past or put back. siunitx's `S` cells peek token by token and swap `\cr`
/// behind `\if_false: { \fi:`; a base counted from the template's tokens alone came out one too high, so
/// that `\cr` ended the cell inside `\cs_set:Npn \cr` and a braced cell gave false certain errors.
#[test]
fn a_braced_siunitx_cell_ends_where_tex_ends_it() {
    if !installed() {
        return;
    }
    let text = "\\documentclass{article}\n\\usepackage{booktabs,siunitx}\n\\begin{document}\n\\begin{tabular}{lS}\n\\toprule\na & {B} \\\\ \\midrule\nb & 2.5 \\\\\n\\bottomrule\n\\end{tabular}\n\\end{document}\n";
    let path = crate::support::tmp_root().join("siunitx-braced-cell.tex");
    std::fs::write(&path, text).unwrap();
    let analysis = satex::analysis::machine::Machine::analyze(text, Some(&path), &crate::support::config());
    let errors: Vec<String> = codes(&analysis)
        .into_iter()
        .filter(|c| c.contains("error") || c.contains("mismatch") || c == "missing-end")
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}
