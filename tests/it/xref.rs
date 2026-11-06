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

const TASK: &str = "\\usepackage{amsthm}\n\\usepackage{cleveref}\n\\newcounter{task}\n\\newtheorem{claim}{Claim}\n";

/// The `undefined-cref-format` findings: severity, line and fix edits.
fn cref_findings(name: &str, preamble: &str, text: &str) -> Vec<(String, u64, Vec<String>)> {
    let body = format!(
        "{TASK}{preamble}\\begin{{document}}\n\\refstepcounter{{task}}\\label{{t:a}}\n\
         \\begin{{claim}}\\label{{c:a}}x\\end{{claim}}\\begin{{claim}}\\label{{c:b}}y\\end{{claim}}\n\
         \\begin{{figure}}\\caption{{c}}\\label{{f:a}}\\end{{figure}}\n\
         \\begin{{equation}}x\\label{{e:a}}\\end{{equation}}\n{text}\n\\end{{document}}\n"
    );
    records(name, &body, "", "undefined-cref-format")
        .iter()
        .map(|r| {
            let severity = r["severity"].as_str().unwrap_or_default().to_string();
            (severity, r["line"].as_u64().unwrap_or_default(), replacements(std::slice::from_ref(r)))
        })
        .collect()
}

/// The line `text` starts on in [`cref_findings`]'s document.
fn text_line(preamble: &str) -> u64 {
    (format!("\\documentclass{{article}}\n{TASK}{preamble}").lines().count() + 6) as u64
}

#[test]
fn a_reference_to_an_unnamed_counter_asks_for_crefname_once() {
    let found = cref_findings("cref-task", "", "See \\cref{t:a}, \\Cref{t:a} and \\cref{t:a}.");
    if !crate::support::installed() {
        return;
    }
    let fix = vec!["\n\\crefname{task}{task}{tasks}".to_string()];
    assert_eq!(found, [("warning".to_string(), text_line(""), fix)]);
}

#[test]
fn a_theorem_range_without_crefname_asks_for_it() {
    // cleveref names a theorem from its heading, but makes no range format.
    let found = cref_findings("cref-claim", "", "See \\cref{c:a}.\n\\crefrange{c:a}{c:b}.");
    if !crate::support::installed() {
        return;
    }
    let fix = vec!["\n\\crefname{claim}{claim}{claims}".to_string()];
    assert_eq!(found, [("warning".to_string(), text_line("") + 1, fix)]);
}

#[test]
fn named_and_built_in_types_need_no_crefname() {
    let preamble = "\\crefname{task}{task}{tasks}\n\\crefname{claim}{claim}{claims}\n";
    let text = "\\cref{t:a}, \\Cref{c:a}, \\crefrange{c:a}{c:b}, \\cref{f:a}, \\cref{e:a}, \\ref{t:a}.";
    assert!(cref_findings("cref-named", preamble, text).is_empty());
}

#[test]
fn a_reference_made_on_some_paths_only_is_a_hint() {
    let found = cref_findings("cref-path", "", "\\ifnum\\pdfuniformdeviate 2=0 \\cref{t:a}\\fi");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(found.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["hint"], "{found:?}");
}

#[test]
fn a_name_given_on_some_paths_only_is_never_a_warning() {
    let preamble = "\\ifnum\\pdfuniformdeviate 2=0 \\crefname{task}{task}{tasks}\\fi\n";
    let found = cref_findings("cref-maybe", preamble, "See \\cref{t:a}.");
    assert!(found.iter().all(|f| f.0 == "hint"), "{found:?}");
}
