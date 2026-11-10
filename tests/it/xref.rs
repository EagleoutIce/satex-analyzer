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
    // the fix writes `\Cref` after a capital word and loads cleveref, which prints the word itself
    let (loads, calls): (Vec<String>, Vec<String>) =
        replacements(&found).into_iter().partition(|r| r.contains("usepackage"));
    assert_eq!(calls, [r"\Cref{sec:a}", r"\Cref{sec:a}", r"\Cref{eq:x}", r"\Cref{app:b}"], "{found:?}");
    assert!(loads.len() == 4 && loads.iter().all(|l| l.contains(r"\usepackage{cleveref}")), "{loads:?}");
    // `Table 9`: the document labels no table, so nothing could be referred to.
    assert_eq!(found.len(), 4, "{found:?}");
}

#[test]
fn a_typed_number_becomes_a_cleveref_reference() {
    let body = "\\usepackage{amsmath}\n\\usepackage{hyperref}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\
                \\begin{figure}\\caption{c}\\label{fig:f}\\end{figure}\nSee Figure 1 and Section 4.\n\\end{document}\n";
    let found = records("autoref", body, "", "hand-written-reference");
    if !crate::support::installed() {
        return;
    }
    // with hyperref loaded, cleveref still names the figure: loaded after it
    let calls: Vec<String> = replacements(&found).into_iter().filter(|r| !r.contains("usepackage")).collect();
    assert_eq!(calls, [r"\Cref{fig:f}"], "{found:?}");
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

/// The edits of `hand-written-reference` on a document with these lists.
fn item_fixes(name: &str, preamble: &str, lists: &str, text: &str) -> Vec<String> {
    let body = format!("{preamble}\\begin{{document}}\n{lists}\n{text}\n\\end{{document}}\n");
    replacements(&records(name, &body, "", "hand-written-reference"))
}

#[test]
fn a_typed_item_number_labels_the_item() {
    let lists = "\\begin{enumerate}\n\\item a\n\\item b\n\\end{enumerate}";
    let found = item_fixes("item-plain", "\\usepackage{cleveref}\n", lists, "See item 2.");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(found, [r"\label{item:2}", r"\cref{item:2}"]);
}

#[test]
fn an_inline_list_item_is_labelled_too() {
    let lists = "\\begin{enumerate*}[label=(\\alph*)]\n\\item a \\item b\n\\end{enumerate*}";
    let found =
        item_fixes("item-inline", "\\usepackage[inline]{enumitem}\n\\usepackage{cleveref}\n", lists, "In item (b).");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(found, [r"\label{item:b}", r"\cref{item:b}"]);
}

#[test]
fn a_label_format_gives_its_own_word() {
    let lists = "\\begin{enumerate}[label=Step~\\arabic*]\n\\item a\n\\item b\n\\end{enumerate}";
    let found = item_fixes("item-step", "\\usepackage{enumitem}\n", lists, "Then step 2 follows.");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(found, [r"\label{item:step2}", r"\ref{item:step2}"]);
}

#[test]
fn a_nested_item_number_names_both_levels() {
    let lists = "\\begin{enumerate}\n\\item a\n\\item b\n\\begin{enumerate}\n\\item c\n\\item d\n\\end{enumerate}\n\\end{enumerate}";
    let found = item_fixes("item-nested", "\\usepackage{cleveref}\n", lists, "See item 2b.");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(found, [r"\label{item:2b}", r"\cref{item:2b}"]);
}

#[test]
fn a_number_no_item_prints_or_an_item_with_its_own_label_is_left_alone() {
    let lists = "\\begin{enumerate}[label=Step~\\arabic*]\n\\item a\n\\item[x] b\n\\end{enumerate}\n\
                 \\begin{enumerate}\n\\item c\\label{it:c}\n\\end{enumerate}";
    let text = "See step 3, item 7 and in 1.) here.";
    let found = item_fixes("item-none", "\\usepackage{enumitem}\n\\usepackage{cleveref}\n", lists, text);
    assert!(found.is_empty(), "{found:?}");
}

const ETAL: &str =
    "@article{lps20, author = {Lamport, Leslie and Plass, Michael and Spivak, Michael}, title = {T}, year = 2020}\n";

#[test]
fn an_et_al_author_becomes_the_citation_of_the_loaded_package() {
    let tail = "\\begin{document}\nAs Lamport et al.~\\cite{lps20} say.\n";
    let natbib = format!(
        "\\usepackage{{natbib}}\n{tail}\\bibliographystyle{{plainnat}}\n\\bibliography{{refs}}\n\\end{{document}}\n"
    );
    let biblatex = format!(
        "\\usepackage{{biblatex}}\n\\addbibresource{{refs.bib}}\n{tail}\\printbibliography\n\\end{{document}}\n"
    );
    let natbib = records("etal-natbib", &natbib, ETAL, "hand-written-author");
    let biblatex = records("etal-biblatex", &biblatex, ETAL, "hand-written-author");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(replacements(&natbib), [r"\citet{lps20}"], "{natbib:?}");
    assert_eq!(replacements(&biblatex), [r"\textcite{lps20}"], "{biblatex:?}");
}

#[test]
fn an_et_al_author_the_entry_does_not_start_with_gets_a_hint() {
    let body = "\\usepackage{natbib}\n\\begin{document}\nAs Plass et al.~\\cite{lps20} and Knuth et al. \\cite{knuth84} say.\n\
                \\bibliographystyle{plainnat}\n\\bibliography{refs}\n\\end{document}\n";
    let found = records("etal-other", body, &format!("{BIB}{ETAL}"), "hand-written-author");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(replacements(&found).is_empty(), "{found:?}");
}

#[test]
fn et_al_spelled_another_way_is_fixed() {
    let body = "\\begin{document}\nBy Lamport et. al. and Plass et.al. and Knuth et~al and Spivak et alii, and Lamport et al. [3].\n\\end{document}\n";
    let found = records("etal-spelling", body, "", "et-al");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(replacements(&found), ["et al.", "et al.", "et~al.", "et al."], "{found:?}");
    // `[3]` gets a hint without an edit.
    assert_eq!(found.len(), 5, "{found:?}");
}

#[test]
fn et_al_in_math_verbatim_or_a_comment_is_not_read() {
    let body = "\\begin{document}\nBy Lamport et al. here, $et. al$, \\verb|et. al.| % et. al.\n\
                and petal, set alike, et all.\n\\end{document}\n";
    assert!(records("etal-quiet", body, "", "et-al").is_empty());
}

#[test]
fn et_al_before_a_lowercase_word_wants_a_word_space_unless_frenchspacing() {
    let body = "\\begin{document}\nLamport et al. and Plass et al. Then more.\n\\end{document}\n";
    let found = records("etal-space", body, "", "sentence-spacing");
    let french = records("etal-french", &format!("\\frenchspacing\n{body}"), "", "sentence-spacing");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(replacements(&found), [r"\ "], "{found:?}");
    assert!(french.is_empty(), "{french:?}");
}

/// `\citeauthor` and `\textcite` print words of the sentence, a name: a line
/// may break before them, so no tie is asked for. A bare `\cite` still gets one.
#[test]
fn no_tie_before_a_citation_that_prints_a_name() {
    let body = "\\usepackage{biblatex}\n\\addbibresource{refs.bib}\n\\begin{document}\n\
                As described by \\citeauthor{knuth84}~\\cite{knuth84} and by \\textcite{kp81}, see \\cite{kp81}.\n\
                \\end{document}\n";
    let found = records("naming-cite", body, BIB, "missing-nonbreaking-space");
    if !crate::support::installed() {
        return;
    }
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(replacements(&found), ["~"]);
}

/// biber predefines the month constants as BibTeX's styles do; with biblatex
/// the `.aux` names `biblatex.bst`, which defines none, so `jun` was reported.
#[test]
fn biblatex_knows_the_month_constants() {
    let bib = "@article{koop17, author = {A. Koop}, title = {T}, journal = {J}, year = {2017}, month = jun}\n";
    let body = "\\usepackage{biblatex}\n\\addbibresource{refs.bib}\n\\begin{document}\n\\cite{koop17}\n\\printbibliography\n\\end{document}\n";
    let found = records("months", body, bib, "undefined-bibliography-string");
    assert!(found.is_empty(), "{found:?}");
}
