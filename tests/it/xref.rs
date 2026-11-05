//! Cross-references and author names typed by hand.

use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};
use satex::queries::query::Record;

/// The findings of `code` on `body` after `\documentclass{article}`, with
/// `refs.bib` beside it.  Empty without a TeX installation.
fn records(name: &str, body: &str, bib: &str, code: &str) -> Vec<Record> {
    if !crate::support::installed() {
        return Vec::new();
    }
    let dir = crate::support::tmp_root().join(format!("satex-xref-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.tex");
    let source = format!("\\documentclass{{article}}\n{body}");
    std::fs::write(&path, &source).unwrap();
    std::fs::write(dir.join("refs.bib"), bib).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let out = satex::linter::lint(&analysis).into_iter().filter(|r| r["code"] == code).collect();
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn replacements(found: &[Record]) -> Vec<String> {
    found
        .iter()
        .flat_map(|r| r.get("edits").and_then(|e| e.as_array()).cloned().unwrap_or_default())
        .filter_map(|e| e["replacement"].as_str().map(str::to_string))
        .collect()
}

const BIB: &str = "@book{knuth84, author = {Donald E. Knuth}, title = {The TeXbook}, year = 1984}\n\
                   @book{kp81, author = {Knuth, Donald E. and Plass, Michael F.}, title = {Breaking}, year = 1981}\n";

#[test]
fn a_typed_section_number_names_its_label() {
    let body = "\\begin{document}\n\\section{A}\\label{sec:a}\nSee Section 1 and Sec.~1 here.\n\
                \\begin{equation}x\\label{eq:x}\\end{equation}\nBy Eq. (1) and Table 9.\n\
                \\appendix\n\\section{B}\\label{app:b}\nIn Appendix A.\n\\end{document}\n";
    let found = records("typed", body, "", "hand-written-reference");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(
        replacements(&found),
        [r"Section~\ref{sec:a}", r"Sec.~\ref{sec:a}", r"Eq.~(\ref{eq:x})", r"Appendix~\ref{app:b}"],
        "{found:?}"
    );
    // `Table 9`: the document labels no table, so nothing could be referred to.
    assert_eq!(found.len(), 4, "{found:?}");
}

#[test]
fn a_typed_number_follows_the_loaded_package() {
    let body = "\\usepackage{amsmath}\n\\usepackage{hyperref}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\
                \\begin{figure}\\caption{c}\\label{fig:f}\\end{figure}\nSee Figure 1 and Section 4.\n\\end{document}\n";
    let found = records("autoref", body, "", "hand-written-reference");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(replacements(&found), [r"\autoref{fig:f}"], "{found:?}");
    // No label prints 4: the finding asks for one.
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_number_set_by_ref_or_kept_out_of_prose_is_not_typed() {
    let body = "\\begin{document}\n\\section{A}\\label{sec:a}\nSection~\\ref{sec:a}, \\verb|Section 1|,\n\
                $\\mathrm{Section}\\ 1$ % Section 1\n\\end{document}\n";
    assert!(records("untyped", body, "", "hand-written-reference").is_empty());
}

#[test]
fn an_author_typed_before_a_citation_is_fixed() {
    let body = "\\usepackage{natbib}\n\\begin{document}\nAs Knuth \\cite{knuth84} and Knuth and Plass~[\\cite{kp81}] say.\n\
                \\bibliographystyle{plainnat}\n\\bibliography{refs}\n\\end{document}\n";
    let found = records("author", body, BIB, "hand-written-author");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(replacements(&found), [r"\citet{knuth84}", r"\citet{kp81}"], "{found:?}");
}

#[test]
fn a_citation_without_its_author_or_without_natbib_is_left_alone() {
    let cited = "\\begin{document}\nAs Plass \\cite{knuth84} and $Knuth$ \\cite{knuth84} say.\n\
                 \\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    assert!(
        records("other-author", &format!("\\usepackage{{natbib}}\n{cited}"), BIB, "hand-written-author").is_empty()
    );
    let plain = "\\begin{document}\nAs Knuth \\cite{knuth84} says.\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    assert!(records("no-natbib", plain, BIB, "hand-written-author").is_empty());
}

#[test]
fn a_space_before_a_wrapper_of_ref_is_reported() {
    let body = "\\newcommand\\secref[1]{\\ref{#1}}\n\\newcommand\\Secref[1]{Section~\\ref{#1}}\n\\begin{document}\n\
                \\section{A}\\label{a}\nSection \\secref{a} and see \\Secref{a}.\n\\end{document}\n";
    let found = records("wrapper", body, "", "missing-nonbreaking-space");
    if !crate::support::installed() {
        return;
    }
    let columns: Vec<_> = found.iter().map(|r| r["col"].clone()).collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(replacements(&found), ["~"], "{columns:?}");
}
