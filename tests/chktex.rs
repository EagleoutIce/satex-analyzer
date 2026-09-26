//! satex's coverage of chktex's warnings; see doc/chktex-coverage.md for the
//! full mapping.  Both rules exercised here work at the macro-expansion
//! level, so no TeX installation is needed to run them.

use satex::config::Config;
use satex::machine::{Analysis, Machine};

fn analyze(source: &str) -> Analysis {
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    };
    Machine::analyze(source, None, &cfg)
}

fn findings(analysis: &Analysis, code: &str) -> Vec<String> {
    satex::lint::lint(analysis)
        .into_iter()
        .filter(|record| record["code"] == code)
        .filter_map(|record| record["message"].as_str().map(str::to_string))
        .collect()
}

// chktex warning 9: `'%s' expected, found '%s'`.
/// `\end{…}` is latex.ltx's, so these run the kernel: its `\@checkend`
/// raises `\begin{x} on input line n ended by \end{y}`.
fn latex(source: &str) -> Option<Analysis> {
    which::which("kpsewhich").ok()?;
    let cfg = Config { load_classes: true, ..Config::default() };
    Some(Machine::analyze(source, None, &cfg))
}

#[test]
fn end_naming_the_wrong_environment_is_reported() {
    let Some(analysis) =
        latex(r"\documentclass{article}\begin{document}\begin{itemize}\item one\end{enumerate}\end{document}")
    else {
        return;
    };
    let found = findings(&analysis, "environment-mismatch");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("itemize"), "{found:?}");
    assert!(found[0].contains("enumerate"), "{found:?}");
}

#[test]
fn a_solo_end_is_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}\end{itemize}\end{document}") else {
        return;
    };
    let found = findings(&analysis, "environment-mismatch");
    assert!(
        found.iter().any(|f| f.contains(r"\begin{document}") && f.contains(r"ended by \end{itemize}")),
        "{found:?}"
    );
}

#[test]
fn an_environment_never_closed_is_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}\begin{itemize}\item one\end{document}") else {
        return;
    };
    let found = findings(&analysis, "environment-mismatch");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains(r"\begin{itemize}") && found[0].contains(r"ended by \end{document}"), "{found:?}");
}

#[test]
fn properly_nested_environments_are_not_reported() {
    let analysis = analyze(r"\begin{document}\begin{itemize}\item one\end{itemize}\end{document}");
    assert!(findings(&analysis, "environment-mismatch").is_empty());
}

// chktex warning 41: `You ought to not use primitive TeX in LaTeX code.`
#[test]
fn plain_tex_primitives_from_the_default_list_are_reported() {
    let analysis = analyze(r"\catcode`\@=11 \csname foo\endcsname");
    let found = findings(&analysis, "primitive-tex-command");
    assert!(found.iter().any(|m| m.contains("\\catcode")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("\\csname")), "{found:?}");
}

#[test]
fn a_latex_style_document_is_not_reported() {
    let analysis = analyze(r"\newcommand{\greeting}{hello}\greeting");
    assert!(findings(&analysis, "primitive-tex-command").is_empty());
}
