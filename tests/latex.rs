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
    assert_eq!(
        keys(&analysis, OccKind::Ref),
        vec!["sec:one".to_string(), "sec:missing".to_string()]
    );
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
        codes.contains(&"option-clash".to_string())
            || codes.contains(&"duplicate-package".to_string()),
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
    assert_eq!(
        marks(&analysis),
        ["mark package-before", "mark file-before", "mark file-after", "mark package-after"]
    );
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
    analysis
        .metadata
        .iter()
        .find(|entry| entry.field == field)
        .map(|entry| entry.text.clone())
        .unwrap_or_default()
}

fn metadata_source(analysis: &Analysis, field: &str) -> String {
    analysis
        .metadata
        .iter()
        .find(|entry| entry.field == field)
        .map(|entry| entry.source.clone())
        .unwrap_or_default()
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
    for name in ["textbf", "emph", "section", "label", "ref", "item", "title", "author", "caption"]
    {
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
    let code_of = |record: &satex::query::Record| {
        record.get("code").and_then(|c| c.as_str()).unwrap_or_default().to_string()
    };
    for code in ["undefined-control-sequence", "undefined-reference"] {
        let from_document = |record: &satex::query::Record| {
            record.get("file").and_then(|f| f.as_str()).unwrap_or_default() == "paper.tex"
        };
        let raised: Vec<String> = findings
            .iter()
            .filter(|record| code_of(record) == code && from_document(record))
            .map(|record| {
                record.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string()
            })
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
    assert!(saved.ends_with("\\FV@ProcessLine {a  b}\\advance \\c@FancyVerbLine \\@ne \\FV@ProcessLine { \\c%}"), "{saved}");
    assert_eq!(body(&analysis, "r"), "a\rb");
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
