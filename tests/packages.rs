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

fn defined(analysis: &Analysis, name: &str) -> bool {
    let sym = analysis.interner.lookup(name);
    sym.is_some_and(|sym| analysis.env.is_defined(sym))
}

fn package_of(analysis: &Analysis, name: &str) -> Option<String> {
    analysis
        .facts
        .defs
        .iter()
        .rev()
        .find(|def| analysis.interner.name(def.name) == name)
        .and_then(|def| def.package)
        .map(|sym| analysis.interner.name(sym).to_string())
}

#[test]
fn enumitem_defines_its_interface_and_the_lists_it_is_asked_for() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{enumitem}
\newlist{steps}{enumerate}{3}
\setlist[steps,1]{label=Step \arabic*.}
\begin{document}
\begin{steps}\item one\end{steps}
\begin{itemize}[label=--]\item two\end{itemize}
\end{document}",
    );
    for name in ["setlist", "newlist", "SetEnumitemKey", "SetLabelAlign"] {
        assert!(defined(&analysis, name), "enumitem defines \\{name}");
    }
    assert_eq!(package_of(&analysis, "setlist").as_deref(), Some("enumitem"));
    for name in ["steps", "endsteps"] {
        assert!(defined(&analysis, name), "\\newlist creates \\{name}");
    }
}

#[test]
fn a_package_that_redefines_a_kernel_command_keeps_the_observed_meaning() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{hyperref}
\begin{document}
\section{One}\label{sec:one}
\end{document}",
    );
    let labels = satex::query::run(&analysis, satex::query::Query::Occurrences, &satex::query::Filter::Always);
    assert!(
        labels.iter().any(|record| record.get("key").is_some_and(|k| k == "sec:one")),
        "the label survives hyperref redefining \\label"
    );
}

#[test]
fn expl3_code_keeps_its_names_and_its_catcodes() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\ExplSyntaxOn
\tl_new:N \l_demo_text_tl
\int_new:N \g_demo_count_int
\int_gincr:N \g_demo_count_int
\cs_new_protected:Npn \demo_show:n #1 { \tl_use:N #1 }
\NewDocumentCommand \DemoShow { m } { \demo_show:n {#1} }
\ExplSyntaxOff
\begin{document}\DemoShow{\l_demo_text_tl}\end{document}",
    );
    for name in ["l_demo_text_tl", "g_demo_count_int", "demo_show:n", "DemoShow"] {
        assert!(defined(&analysis, name), "expl3 defines \\{name}");
    }
    let findings = satex::lint::lint(&analysis);
    let codes: Vec<&str> = findings.iter().filter_map(|f| f.get("code").and_then(|c| c.as_str())).collect();
    assert!(!codes.contains(&"expl-syntax-left-on"), "\\ExplSyntaxOff ends it: {codes:?}");
    assert!(!codes.contains(&"catcode-left-changed"), "the catcodes go back: {codes:?}");
}

#[test]
fn verbatim_text_is_not_read_as_code() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\begin{verbatim}
\notacommand{x} #$%^&_
\end{verbatim}
\label{after}
\end{document}",
    );
    assert!(
        !analysis.facts.expansions.iter().any(|e| analysis.interner.name(e.name) == "notacommand"),
        "the verbatim body is text"
    );
}

#[test]
fn an_image_is_looked_for_where_graphicspath_says_and_in_what_the_driver_reads() {
    if !installed() {
        return;
    }
    let directory = std::env::temp_dir().join("satex-graphics-test");
    let figures = directory.join("figs");
    std::fs::create_dir_all(&figures).unwrap();
    std::fs::write(figures.join("tree.png"), b"").unwrap();
    let source = directory.join("doc.tex");
    std::fs::write(
        &source,
        r"\documentclass{article}
\usepackage{graphicx}
\graphicspath{{figs/}}
\def\figname{tree}
\begin{document}
\includegraphics{\figname}
\includegraphics{nowhere}
\end{document}",
    )
    .unwrap();
    let text = std::fs::read_to_string(&source).unwrap();
    let cfg = Config { load_classes: true, ..Config::default() };
    let analysis = Machine::analyze(&text, Some(&source), &cfg);

    let found: Vec<_> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == satex::builtins::OccKind::Graphics)
        .map(|o| (o.key.clone(), o.detail.is_some()))
        .collect();
    // pdflatex's log: `<figs/tree.png, id=1, ...>` and
    // `! LaTeX Error: File `nowhere' not found.`
    assert!(found.contains(&("figs/tree.png".to_string(), true)), "{found:?}");
    let missing: Vec<String> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("missing-graphic"))
        .filter_map(|f| f.get("name").and_then(|c| c.as_str()).map(str::to_string))
        .collect();
    assert_eq!(missing, ["nowhere"], "{found:?}");
    let _ = std::fs::remove_dir_all(&directory);
}

fn occurrences(analysis: &Analysis, kind: satex::builtins::OccKind) -> Vec<String> {
    analysis.facts.occurrences.iter().filter(|o| o.kind == kind).map(|o| o.key.clone()).collect()
}

#[test]
fn glossaries_uses_come_from_the_lines_it_writes_and_its_errors() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage[acronym]{glossaries}
\makeglossaries
\newglossaryentry{tex}{name=TeX,description={a typesetting system}}
\newacronym{api}{API}{application programming interface}
\begin{document}
\gls{tex} \acrshort{api} \gls{missing}
\end{document}",
    );
    // pdflatex writes `\glossaryentry{TeX?\glossentry{tex}|…}` to the
    // `.glo` and `api` to the `.acn`, and stops with `Glossary entry
    // `missing' has not been defined`.
    use satex::builtins::OccKind;
    let mut declared = occurrences(&analysis, OccKind::Key);
    declared.sort();
    assert_eq!(declared, ["api", "tex"]);
    // `\gls{missing}` reads a name of an entry's shape no call declared.
    let mut used = occurrences(&analysis, OccKind::KeyUse);
    used.sort();
    used.dedup();
    assert_eq!(used, ["api", "missing", "tex"]);
    let missing: Vec<String> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("undefined-glossary-entry"))
        .filter_map(|f| f.get("name").and_then(|c| c.as_str()).map(str::to_string))
        .collect();
    assert_eq!(missing, ["missing"]);
}

#[test]
fn the_acronym_package_declares_its_entries() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{acronym}
\begin{document}
\begin{acronym}
\acro{ci}[CI]{continuous integration}
\end{acronym}
\ac{ci}
\end{document}",
    );
    // `\acro` exists only inside the `acronym` environment; pdflatex writes
    // `\newacro{ci}…` and `\acronymused{ci}` to the `.aux`.
    // Read back, `\newacro{ci}` defines `\fn@ci`, which `\ac{ci}` reads.
    use satex::builtins::OccKind;
    assert_eq!(occurrences(&analysis, OccKind::Label), ["ci"]);
    let mut used = occurrences(&analysis, OccKind::Ref);
    used.dedup();
    assert_eq!(used, ["ci"]);
}

#[test]
fn listings_and_minted_keep_their_bodies_out_of_the_analysis() {
    if !installed() {
        return;
    }
    for (package, environment) in [("listings", "lstlisting"), ("minted", "minted")] {
        let source = format!(
            "\\documentclass{{article}}\n\\usepackage{{{package}}}\n\\begin{{document}}\n\
             \\begin{{{environment}}}{}\n\\undefinedinlisting\n\\end{{{environment}}}\n\
             \\label{{after}}\n\\end{{document}}",
            if environment == "minted" { "{text}" } else { "" }
        );
        let analysis = analyze(&source);
        assert!(
            !analysis.facts.expansions.iter().any(|e| analysis.interner.name(e.name) == "undefinedinlisting"),
            "{package}: the body of {environment} is text, not code"
        );
    }
}

fn body(analysis: &Analysis, name: &str) -> String {
    analysis
        .facts
        .defs
        .iter()
        .rev()
        .find(|def| analysis.interner.name(def.name) == name)
        .and_then(|def| def.mac.as_ref())
        .map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
        .unwrap_or_default()
}

/// The findings for the document itself, leaving out what the packages it
/// read contribute.
fn codes(analysis: &Analysis) -> Vec<String> {
    satex::lint::lint(analysis)
        .iter()
        .filter(|f| f.get("origin").and_then(|o| o.as_str()) == Some("document"))
        .filter_map(|f| f.get("code").and_then(|c| c.as_str()).map(str::to_string))
        .collect()
}

/// The `\typeout` texts the run produced, in the order it produced them.
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
fn etoolbox_document_hooks_are_the_kernel_hooks() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{etoolbox}
\AtEndPreamble{\newcommand{\fromendpreamble}{1}\typeout{mark endpreamble}}
\AtBeginDocument{\newcommand{\frombegin}{2}\typeout{mark begin}}
\AfterPreamble{\newcommand{\fromafterpreamble}{3}\typeout{mark afterpreamble}}
\AfterEndPreamble{\newcommand{\fromafterendpreamble}{4}\typeout{mark afterendpreamble}}
\AtEndDocument{\typeout{mark enddocument}}
\AfterEndDocument{\typeout{mark afterenddocument}}
\begin{document}
\fromendpreamble\frombegin\fromafterpreamble\fromafterendpreamble
\end{document}",
    );
    // etoolbox defines these in terms of `begindocument…` and `enddocument…`
    // from LaTeX 2020-10-01 on, so they run where those hooks run.
    assert_eq!(
        marks(&analysis),
        [
            "mark endpreamble",
            "mark begin",
            "mark afterpreamble",
            "mark afterendpreamble",
            "mark enddocument",
            "mark afterenddocument",
        ]
    );
    for (name, text) in
        [("fromendpreamble", "1"), ("frombegin", "2"), ("fromafterpreamble", "3"), ("fromafterendpreamble", "4")]
    {
        assert_eq!(body(&analysis, name), text, "\\{name}");
    }
    assert!(!codes(&analysis).contains(&"undefined-control-sequence".to_string()));
}

#[test]
fn etoolbox_environment_hooks_are_the_generic_env_hooks() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{etoolbox}
\BeforeBeginEnvironment{quote}{\newcommand{\beforequote}{b}\typeout{mark before}}
\AtBeginEnvironment{quote}{\typeout{mark begin}}
\AtEndEnvironment{quote}{\typeout{mark end}}
\AfterEndEnvironment{quote}{\newcommand{\afterquote}{a}\typeout{mark after}}
\begin{document}\begin{quote}x\end{quote}\beforequote\afterquote\end{document}",
    );
    assert_eq!(marks(&analysis), ["mark before", "mark begin", "mark end", "mark after"]);
    // Both hooks run outside the environment's group, so what they define is
    // still there when the body uses it.
    for (name, text) in [("beforequote", "b"), ("afterquote", "a")] {
        assert_eq!(body(&analysis, name), text, "\\{name}");
    }
    assert!(!codes(&analysis).contains(&"undefined-control-sequence".to_string()));
}

#[test]
fn at_end_of_package_runs_when_the_package_it_was_called_in_closes() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\AddToHook{package/color/after}{\typeout{mark after-color}}
\usepackage{color}
\usepackage{graphicx}
\begin{document}\end{document}",
    );
    assert_eq!(marks(&analysis), ["mark after-color"]);
}

#[test]
fn cleveref_and_hyperref_references_are_observed_from_the_names_they_test() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{hyperref}
\usepackage{cleveref}
\begin{document}
\section{Intro}\label{sec:intro}
\cref{sec:gone} \Cref{sec:intro} \crefrange{sec:intro}{sec:end}
\autoref{sec:intro} \nameref{sec:missing}
\end{document}",
    );
    // pdflatex, first run: `Reference `⟨key⟩' on page 1 undefined` for
    // exactly these keys.
    let mut refs = occurrences(&analysis, satex::builtins::OccKind::Ref);
    refs.sort();
    refs.dedup();
    assert_eq!(refs, ["sec:end", "sec:gone", "sec:intro", "sec:missing"], "{refs:?}");
}

#[test]
#[ignore = "cleveref's label-list loop (\\@whilesw\\if@cref@stackfull, cleveref.sty:950) does not terminate in satex yet"]
fn cleveref_reads_every_label_of_a_list() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{cleveref}
\begin{document}
\cref{a,b}\cref{c}
\end{document}",
    );
    let mut refs = occurrences(&analysis, satex::builtins::OccKind::Ref);
    refs.sort();
    refs.dedup();
    assert_eq!(refs, ["a", "b", "c"], "{refs:?}");
}

#[test]
fn biblatex_citations_and_resources_are_observed_from_what_it_writes() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage[backend=biber]{biblatex}
\addbibresource{refs.bib}
\begin{document}
\autocite{knuth} \parencite[p.~3]{knuth,missing} \textcite{knuth}
\printbibliography
\end{document}",
    );
    use satex::builtins::OccKind;
    // pdflatex writes `\abx@aux@cite{0}{knuth}` and `{0}{missing}` to the
    // `.aux`, and `refs.bib` as the datasource of the `.bcf`.
    let mut cites = occurrences(&analysis, OccKind::Cite);
    cites.sort();
    cites.dedup();
    assert_eq!(cites, ["knuth", "missing"], "{cites:?}");
    assert_eq!(occurrences(&analysis, OccKind::Bibliography), ["refs.bib"]);
}

#[test]
fn acro_acronyms_are_observed_from_what_acro_writes_and_its_errors() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{acro}
\DeclareAcronym{ci}{short=CI,long=continuous integration}
\DeclareAcronym{cd}{short=CD,long=continuous delivery}
\begin{document}
\ac{ci} \acs{ci} \acl{xx}
\end{document}",
    );
    use satex::builtins::OccKind;
    // pdflatex writes `\ACRO{recordpage}{ci}…` for each use of `ci` and
    // stops with `Package acro Error: You've requested acronym `xx'`.
    let mut declared = occurrences(&analysis, OccKind::Key);
    declared.sort();
    assert_eq!(declared, ["cd", "ci"]);
    let mut used = occurrences(&analysis, OccKind::KeyUse);
    used.sort();
    used.dedup();
    assert_eq!(used, ["ci", "xx"]);
    let missing: Vec<String> = satex::lint::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("undefined-glossary-entry"))
        .filter_map(|f| f.get("name").and_then(|c| c.as_str()).map(str::to_string))
        .collect();
    assert_eq!(missing, ["xx"]);
}
