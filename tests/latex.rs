//! LaTeX2e as documented: what the kernel's own interfaces do.

use satex::builtins::OccKind;
use satex::config::Config;
use satex::machine::{Analysis, Machine};

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

fn analyze(source: &str) -> Analysis {
    let cfg = Config { load_classes: true, ..Config::default() };
    Machine::analyze(source, None, &cfg)
}

fn body(analysis: &Analysis, name: &str) -> String {
    analysis
        .facts
        .defs
        .iter()
        .rev()
        .find(|d| analysis.interner.name(d.name) == name)
        .and_then(|d| d.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
}

fn keys(analysis: &Analysis, kind: OccKind) -> Vec<String> {
    analysis.facts.occurrences.iter().filter(|o| o.kind == kind).map(|o| o.key.clone()).collect()
}

/// The findings for the document itself; `satex lint` needs `--all` to show
/// the ones that come from the packages it read.
fn codes(analysis: &Analysis) -> Vec<String> {
    satex::lint::lint(analysis)
        .iter()
        .filter(|f| f.get("origin").and_then(|o| o.as_str()) == Some("document"))
        .filter_map(|f| f.get("code").and_then(|c| c.as_str()).map(str::to_string))
        .collect()
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

#[test]
fn a_length_can_be_set_and_added_to() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newlength{\gap}
\setlength{\gap}{2pt}
\addtolength{\gap}{3pt}
\begin{document}\edef\seen{\the\gap}\end{document}",
    );
    assert_eq!(body(&analysis, "seen"), "5.0pt");
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

#[test]
fn a_switch_made_by_newif_decides_a_conditional() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newif\ifdraft
\drafttrue
\begin{document}\ifdraft\def\seen{on}\else\def\seen{off}\fi\end{document}",
    );
    assert_eq!(body(&analysis, "seen"), "on");
}

#[test]
fn at_begin_document_runs_before_the_body() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\AtBeginDocument{\def\seen{hook}}
\begin{document}\end{document}",
    );
    assert_eq!(body(&analysis, "seen"), "hook");
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

/// The `\typeout` texts the run produced, in the order it produced them: the
/// execution order of whatever wrote them.
fn marks(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Message && o.key.starts_with("mark "))
        .map(|o| o.key.clone())
        .collect()
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

#[test]
fn an_internal_command_in_the_date_expands_like_any_macro() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\makeatletter
\protected\def\my@mark{M}
\date{September\my@mark 22, 2026}
\makeatother
\begin{document}
\title{T}\author{A}\maketitle
\end{document}",
    );
    // `\pdfstringdef` expands what it can, robust commands included; a name
    // with an `@` in it is a macro like any other, so its text stands.
    assert_eq!(metadata(&analysis, "date"), "SeptemberM22, 2026");
}

#[test]
fn texorpdfstring_gives_the_title_its_pdf_form() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{hyperref}
\begin{document}
\title{The \texorpdfstring{$x^2$}{x squared} rule}\author{A}\maketitle
\end{document}",
    );
    assert_eq!(metadata(&analysis, "title"), "The x squared rule");
}

#[test]
fn math_in_a_title_keeps_only_what_is_text() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\title{The $\alpha$ Method}\author{A}\maketitle
\end{document}",
    );
    assert_eq!(metadata(&analysis, "title"), "The Method");
}

#[test]
fn a_title_uses_the_documents_own_macro() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\newcommand{\proj}{Satex}
\begin{document}
\title{The \proj{} analyser}\author{A}\maketitle
\end{document}",
    );
    assert_eq!(metadata(&analysis, "title"), "The Satex analyser");
}

#[test]
fn a_kern_and_a_penalty_leave_the_title_with_their_values() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\title{X\kern2pt Y\penalty100 Z\hskip 3pt plus 1fil W\,V}\author{A}\maketitle
\end{document}",
    );
    assert_eq!(metadata(&analysis, "title"), "XYZWV");
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
    let analysis =
        Machine::analyze(&source, Some(std::path::Path::new("tests/fixtures/project/paper.tex")), &Config::default());
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
        analyze_with(options, &Config::default())
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
        assert_eq!(exhausted, Vec::new(), "the run ran out of budget on `{options}`");
        assert!(!analysis.exhausted, "the run was exhausted on `{options}`");
        // Measured: 20 million tokens with the bound, 240 million without,
        // out of the 900 million the default budget allows.
        assert!(analysis.steps < BOUNDED_STEPS, "{} steps for `{options}`", analysis.steps);
    }

    /// Well above what the bounded run needs and well below what the
    /// unbounded one took.
    const BOUNDED_STEPS: u64 = 50_000_000;

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

    /// The bound is what holds the report down, not something else about the
    /// document: lifting it puts the same run back into the millions, at
    /// `\exp_args:Ne` and `\exp_args:Nf` in `expl3-code.tex`.  Should the
    /// gaps themselves ever be fixed, this asks to be rewritten around a
    /// document that still has one.
    #[test]
    fn lifting_the_bound_puts_the_same_document_back_into_the_millions() {
        if !super::installed() {
            return;
        }
        let mut lifted = Config::default();
        lifted.limits.repeat_failures = 0;
        let unbounded = worst(&analyze_with("lang=en", &lifted));
        assert!(unbounded > 1_000_000, "the unbounded run counted one gap only {unbounded} times");
        let bounded = worst(&analyze_document("lang=en"));
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
