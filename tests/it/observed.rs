//! What the run observes rather than looks up: citations from the
//! `\citation` lines written to the `.aux`, `\bibitem`s from theirs, what a
//! `\csname` is formed for, the accents OT1 builds, and the payloads a
//! document hands the PDF writer.

use std::path::{Path, PathBuf};

use crate::support::analyze_latex as analyze;
use crate::support::installed;
use satex::builtins::OccKind;
use satex::config::Config;
use satex::machine::{Analysis, Machine};

/// A document written to a directory of its own, beside the files it names;
/// the lints read those files too, so the directory outlives the analysis.
fn analyze_in(name: &str, source: &str, beside: &[(&str, &str)]) -> Analysis {
    analyze_in_with(name, source, beside, &crate::support::config())
}

fn analyze_in_with(name: &str, source: &str, beside: &[(&str, &str)], cfg: &Config) -> Analysis {
    let dir: PathBuf = crate::support::tmp_root().join(format!("satex-observed-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temporary directory");
    for (file, text) in beside {
        std::fs::write(dir.join(file), text).expect("write beside");
    }
    let main = dir.join(format!("{name}.tex"));
    std::fs::write(&main, source).expect("write document");
    Machine::analyze(source, Some(Path::new(&main)), cfg)
}

fn keys(analysis: &Analysis, kind: OccKind) -> Vec<(String, u32)> {
    analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == kind && o.span.file == analysis.main_file)
        .map(|o| (o.key.clone(), o.span.line))
        .collect()
}

/// Every occurrence of `kind` in the main file: key, line and column.
fn places(analysis: &Analysis, kind: OccKind) -> Vec<(String, u32, u32)> {
    analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == kind && o.span.file == analysis.main_file)
        .map(|o| (o.key.clone(), o.span.line, o.span.col))
        .collect()
}

fn findings(analysis: &Analysis, code: &str) -> Vec<(String, Option<String>)> {
    satex::lint::lint(analysis)
        .into_iter()
        .filter(|record| record["code"] == code)
        .map(|record| {
            (record["message"].as_str().unwrap_or_default().to_string(), record["fix"].as_str().map(str::to_string))
        })
        .collect()
}

#[test]
fn natbib_citations_are_observed_with_their_notes_and_stars() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{natbib}
\begin{document}
\citep[see][p.~3]{knuth84,lamport94}
\citet*{knuth84}
\citealp{other}
\nocite{extra}
\end{document}",
    );
    let cited = keys(&analysis, OccKind::Cite);
    for (key, line) in [("knuth84", 4), ("lamport94", 4), ("knuth84", 5), ("other", 6), ("extra", 7)] {
        assert!(cited.contains(&(key.to_string(), line)), "{key} at {line}: {cited:?}");
    }
    assert!(cited.iter().all(|(key, _)| !key.contains('[') && key != "*"), "{cited:?}");
}

#[test]
fn citations_are_checked_against_the_bibitems_and_the_database() {
    if !installed() {
        return;
    }
    let analysis = analyze_in(
        "cite",
        r"\documentclass{article}
\begin{document}
\cite{knuth84,lamport94} \cite[p.~2]{missing} \cite{indatabase}
\bibliography{refs}
\begin{thebibliography}{9}
\bibitem{knuth84} Knuth.
\bibitem[L94]{lamport94} Lamport.
\bibitem{uncited} Nobody.
\bibitem{knuth84} Again.
\end{thebibliography}
\end{document}",
        &[("refs.bib", "@string{x = \"y\"}\n@book{indatabase, title={T}}\n@misc{dup,}\n@misc{dup,}\n")],
    );
    assert_eq!(keys(&analysis, OccKind::Bibliography), vec![("refs".to_string(), 4)]);
    let items: Vec<String> = keys(&analysis, OccKind::BibItem).into_iter().map(|(k, _)| k).collect();
    assert_eq!(items, ["knuth84", "lamport94", "uncited", "knuth84"]);
    assert!(
        keys(&analysis, OccKind::Cite).iter().all(|(_, line)| *line == 3),
        "a \\bibitem is no citation: {:?}",
        keys(&analysis, OccKind::Cite)
    );

    let undefined = findings(&analysis, "undefined-citation");
    assert_eq!(undefined.len(), 1, "{undefined:?}");
    assert!(undefined[0].0.contains("missing"), "{undefined:?}");

    let unused = findings(&analysis, "unused-bibitem");
    assert_eq!(unused.len(), 1, "{unused:?}");
    assert!(unused[0].0.contains("uncited"), "{unused:?}");

    let duplicates = findings(&analysis, "duplicate-bibliography-entry");
    assert_eq!(duplicates.len(), 2, "{duplicates:?}");
    assert!(duplicates.iter().any(|(m, _)| m.contains("knuth84")), "{duplicates:?}");
    assert!(duplicates.iter().any(|(m, _)| m.contains("dup") && m.contains("refs.bib:4")), "{duplicates:?}");
    cleanup("cite");
}

#[test]
fn an_unreadable_database_keeps_citations_unjudged() {
    if !installed() {
        return;
    }
    let analysis = analyze_in(
        "nodb",
        r"\documentclass{article}
\begin{document}
\cite{anything}
\bibliography{nowhere}
\end{document}",
        &[],
    );
    assert!(findings(&analysis, "undefined-citation").is_empty());
    cleanup("nodb");
}

fn cleanup(name: &str) {
    let _ = std::fs::remove_dir_all(
        crate::support::tmp_root().join(format!("satex-observed-{name}-{}", std::process::id())),
    );
}

#[test]
fn csname_is_matched_to_the_interface_for_what_it_was_formed_for() {
    if !installed() {
        return;
    }
    let body = r"\expandafter\def\csname foo\endcsname{x}
\expandafter\gdef\csname bar\endcsname{x}
\expandafter\let\csname baz\endcsname\relax
\expandafter\let\expandafter\qux\csname foo\endcsname
\expandafter\ifx\csname foo\endcsname\relax a\else b\fi
\begin{document}
\csname foo\endcsname
\end{document}";
    let fixes = |analysis: &Analysis| -> Vec<Option<String>> {
        satex::lint::lint(analysis)
            .into_iter()
            .filter(|r| r["code"] == "primitive-tex-command" && r["name"] == "\\csname")
            .filter(|r| r["origin"] == "document")
            .map(|r| r["fix"].as_str().map(str::to_string))
            .collect()
    };
    let with = analyze(&format!("\\documentclass{{article}}\n\\usepackage{{etoolbox}}\n{body}"));
    let expected = ["\\csdef", "\\csgdef", "\\cslet", "\\letcs", "\\ifcsundef", "\\csuse"];
    let found = fixes(&with);
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (fix, want) in found.iter().zip(expected) {
        let fix = fix.as_deref().unwrap_or_default();
        assert!(fix.starts_with(&format!("use `{want}`")) && fix.contains("etoolbox"), "{fix} for {want}");
    }

    // `\gdef`, `\let` and `\let`-to-a-name-formed-by-\csname have no kernel
    // fallback, only etoolbox's: with no fix to offer, satex now reports
    // none of the three at all rather than repeating "this is plain TeX"
    // back with nothing to act on.
    let without = analyze(&format!("\\documentclass{{article}}\n{body}"));
    let found = fixes(&without);
    let expected = [Some("use `\\@namedef`"), Some("use `\\@ifundefined`"), Some("use `\\@nameuse`")];
    assert_eq!(found.iter().map(Option::as_deref).collect::<Vec<_>>(), expected);
}

#[test]
fn accents_built_in_ot1_recommend_t1() {
    if !installed() {
        return;
    }
    let accented = analyze(
        r#"\documentclass{article}
\begin{document}
Sch\"on und gr\'e\`e.
\end{document}"#,
    );
    let found = findings(&accented, "ot1-font-encoding");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].0.contains("hyphenate"), "{found:?}");
    assert!(found[0].1.as_deref().is_some_and(|fix| fix.contains("[T1]{fontenc}")), "{found:?}");

    let plain = analyze(
        r"\documentclass{article}
\begin{document}
Plain words.
\end{document}",
    );
    assert!(findings(&plain, "ot1-font-encoding").is_empty());

    let t1 = analyze(
        r#"\documentclass{article}
\usepackage[T1]{fontenc}
\begin{document}
Sch\"on.
\end{document}"#,
    );
    assert!(findings(&t1, "ot1-font-encoding").is_empty());
    assert_eq!(findings(&t1, "computer-modern-in-t1").len(), 1);

    let lmodern = analyze(
        r#"\documentclass{article}
\usepackage[T1]{fontenc}
\usepackage{lmodern}
\begin{document}
Sch\"on.
\end{document}"#,
    );
    assert!(findings(&lmodern, "computer-modern-in-t1").is_empty());

    // Vertical mode ignores a space (tex.web § 1043): the preamble sets nothing.
    let preamble = analyze(
        r"\documentclass{article}
\usepackage[T1]{fontenc}
\def\foo{x}
\begin{document}
\end{document}",
    );
    assert!(findings(&preamble, "computer-modern-in-t1").is_empty());
}

/// After `\clearpage` ships the page out, the page is empty again and
/// `\pagegoal` reads `\maxdimen` (tex.web §§ 987, 1017). article.cls runs
/// `\onecolumn`, so `\clearpage`, at its end.
#[test]
fn the_page_is_empty_after_a_forced_page_break() {
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\ifdim\pagegoal=\maxdimen \gdef\PageEmpty{}\else\gdef\PageTouched{}\fi
\end{document}",
    );
    assert!(crate::support::defined(&analysis, "PageEmpty"));
    assert!(!crate::support::defined(&analysis, "PageTouched"), "the page room is not known");
}

/// tex.web § 370: an undefined name is an error and adds nothing, so the
/// formula is empty and the box is 0pt wide.
#[test]
fn an_undefined_name_in_math_sets_nothing() {
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\setbox0\hbox{$\nosuchmacro$}\ifdim\wd0=0pt \gdef\WdZero{}\else\gdef\WdOther{}\fi
\end{document}",
    );
    assert!(crate::support::defined(&analysis, "WdZero"));
    assert!(!crate::support::defined(&analysis, "WdOther"), "the width is not known");
}

#[test]
fn microtype_is_recommended_only_for_justified_text_where_the_engine_supports_it() {
    if !installed() {
        return;
    }
    let prose = "Running text that fills the line. ".repeat(150);
    let doc = |preamble: &str, body: &str| {
        format!("\\documentclass{{article}}\n{preamble}\\begin{{document}}\n{body}\n\\end{{document}}")
    };
    let source = doc("", &prose);
    assert_eq!(findings(&analyze(&source), "microtype-available").len(), 1);
    for quiet in [
        doc("\\usepackage{microtype}\n", &prose),
        doc("", "Text."),
        doc("", &format!("\\raggedright\n{prose}")),
        doc("", &format!("\\mbox{{{prose}}}")),
    ] {
        assert!(findings(&analyze(&quiet), "microtype-available").is_empty(), "{quiet}");
    }
    let xetex = Config { engine: Some(satex::config::Engine::XeTeX), ..crate::support::config() };
    assert!(findings(&Machine::analyze(&source, None, &xetex), "microtype-available").is_empty());
}

/// Macros that take about a million tokens per `\a`: a cost of known size.
const COSTLY: &str = r"\def\e{\relax}\def\d{\e\e\e\e\e\e\e\e\e\e}\def\c{\d\d\d\d\d\d\d\d\d\d}
\def\b{\c\c\c\c\c\c\c\c\c\c}\def\a{\b\b\b\b\b\b\b\b\b\b}";

#[test]
fn preamble_cost_speaks_only_for_a_preamble_that_is_most_of_a_costly_build() {
    if !installed() {
        return;
    }
    let calls = r"\a".repeat(200);
    let doc = |preamble: &str, body: &str| {
        format!("\\documentclass{{article}}\n{COSTLY}\n{preamble}\n\\begin{{document}}\n{body}\n\\end{{document}}")
    };
    assert_eq!(findings(&analyze(&doc(&calls, "x")), "preamble-cost").len(), 1);
    for quiet in [doc("", "x"), doc(&calls, &format!("{calls}{calls}"))] {
        assert!(findings(&analyze(&quiet), "preamble-cost").is_empty(), "{quiet}");
    }
}

#[test]
fn includeonly_is_suggested_for_costly_included_parts_built_whole() {
    if !installed() {
        return;
    }
    let calls = r"\a".repeat(200);
    let parts = [("part-a.tex", calls.as_str()), ("part-b.tex", calls.as_str())];
    let doc = |preamble: &str| {
        format!(
            "\\documentclass{{report}}\n{COSTLY}\n{preamble}\\begin{{document}}\n\\include{{part-a}}\n\\include{{part-b}}\n\\end{{document}}\n"
        )
    };
    // Two parts of no text are no book: quiet by default.
    let short = analyze_in("includeonly-short", &doc(""), &parts);
    assert!(findings(&short, "includeonly-available").is_empty());
    let mut cfg = crate::support::config();
    cfg.lints.build_cost.characters = 0;
    let whole = analyze_in_with("includeonly-whole", &doc(""), &parts, &cfg);
    assert_eq!(findings(&whole, "includeonly-available").len(), 1);
    // A part's checkpoint in its own `.aux` file is no label.
    assert!(findings(&whole, "unused-label").is_empty());
    let only = analyze_in_with("includeonly-only", &doc("\\includeonly{part-a}\n"), &parts, &cfg);
    assert!(findings(&only, "includeonly-available").is_empty());
    let cheap = [("part-a.tex", "A."), ("part-b.tex", "B.")];
    assert!(
        findings(&analyze_in_with("includeonly-cheap", &doc(""), &cheap, &cfg), "includeonly-available").is_empty()
    );
}

#[test]
fn marked_content_in_literals_has_to_balance() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\pdfpageresources{/Properties << /oc1 5 0 R >>}
\begin{document}
\pdfliteral{/OC /oc1 BDC}shown\pdfliteral{EMC}
\pdfliteral{/OC /oc9 BDC}lost
\pdfliteral{q 1 0 0 1 0 0 cm}\pdfliteral{Q Q}
\special{pdf:content EMC}
\end{document}",
    );
    let pdf = keys(&analysis, OccKind::Pdf);
    assert!(pdf.iter().any(|(k, _)| k.contains("/OC /oc1 BDC")), "{pdf:?}");
    let unbalanced = findings(&analysis, "unbalanced-pdf-content");
    let messages: Vec<&str> = unbalanced.iter().map(|(m, _)| m.as_str()).collect();
    // The special closes the BDC the second literal left open.
    assert_eq!(
        messages,
        ["`\\pdfliteral` closes the graphics state (`Q`) that nothing here opened: \
                            no matching `q` precedes it"]
    );
    let undefined = findings(&analysis, "undefined-optional-content");
    assert_eq!(undefined.len(), 1, "{undefined:?}");
    assert!(undefined[0].0.contains("oc9"), "{undefined:?}");
}

#[test]
fn marked_content_left_open_is_reported_where_it_opens() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\pdfliteral{/OC /oc1 BDC}never closed
\end{document}",
    );
    let unbalanced = findings(&analysis, "unbalanced-pdf-content");
    assert_eq!(unbalanced.len(), 1, "{unbalanced:?}");
    assert!(
        unbalanced[0].0.starts_with("`\\pdfliteral` opens marked content (`BDC`) and never closes it (`EMC`)"),
        "{unbalanced:?}"
    );
    assert!(findings(&analysis, "undefined-optional-content").is_empty(), "no resources were observed");
}

/// One macro's `EMC` closes the `BDC` at every call site, not only the
/// first: `\EndAccSupp` is one definition used many times.
#[test]
fn accsupp_used_twice_is_balanced() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{accsupp}
\newcommand\alt[1]{\BeginAccSupp{method=escape,ActualText={}}#1\EndAccSupp{}}
\begin{document}
One \alt{a} and two \alt{b}.
\end{document}",
    );
    assert!(findings(&analysis, "unbalanced-pdf-content").is_empty());
}

/// The wrapper's `BDC` and the one inside a listings literate replacement
/// share the call site: `B B E E` is balanced, not a repeat of one pair.
#[test]
fn nested_accsupp_at_one_call_site_is_balanced() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{listings,accsupp,amssymb,amsmath}
\def\Arrow{\text{\BeginAccSupp{method=escape,ActualText={<-}}\(\leftarrow\)\EndAccSupp{}}}
\lstset{literate={<-}{{{\!\Arrow\!}}}2}
\newcommand*\code[1]{\BeginAccSupp{method=escape,ActualText={#1}}\lstinline`#1`\EndAccSupp{}}
\begin{document}
Use \code{x <- 2} and \code{y <- 3}.
\end{document}",
    );
    assert!(findings(&analysis, "unbalanced-pdf-content").is_empty());
}

#[test]
fn ocg_p_layers_are_balanced() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{ocg-p}
\begin{document}
\begin{ocg}{Layer}{l1}{1}shown\end{ocg}
\end{document}",
    );
    assert!(findings(&analysis, "unbalanced-pdf-content").is_empty());
}

/// A graphics-state save that is never restored: exactly one finding, at the
/// `q` itself, naming the construct that wrote it and what leaks.
#[test]
fn unclosed_graphics_state_is_reported_once() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\pdfliteral{q}
\end{document}",
    );
    let unbalanced = findings(&analysis, "unbalanced-pdf-content");
    assert_eq!(unbalanced.len(), 1, "{unbalanced:?}");
    assert_eq!(
        unbalanced[0].0,
        "`\\pdfliteral` opens the graphics state (`q`) and never closes it (`Q`): color, line width and the \
         current transform leak into everything typeset after it"
    );
    assert_eq!(unbalanced[0].1.as_deref(), Some("add the matching `Q` in the same group before it ends"));
}

/// A `\special{pdf:…}` save that is closed in the same group: no finding.
#[test]
fn balanced_graphics_state_is_not_reported() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\special{pdf:content q}\special{pdf:content Q}
\end{document}",
    );
    assert!(findings(&analysis, "unbalanced-pdf-content").is_empty());
}

/// `\special{pdf:…}` is named as the emitting construct, not `\pdfliteral`.
#[test]
fn the_special_form_is_named_in_its_own_finding() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\begin{document}
\special{pdf:content q}
\end{document}",
    );
    let unbalanced = findings(&analysis, "unbalanced-pdf-content");
    assert_eq!(unbalanced.len(), 1, "{unbalanced:?}");
    assert!(unbalanced[0].0.starts_with("`\\special{pdf:…}` opens"), "{unbalanced:?}");
}

/// The interpreter can record the same content-stream operator twice under
/// the same graph node (once directly, once by way of an `expanded` site):
/// [`payloads`] in `src/lint/pdf.rs` keeps only the first.  Reverting that
/// dedup makes this test fail with two findings instead of one.
#[test]
fn a_literal_recorded_twice_under_the_same_node_is_reported_once() {
    if !installed() {
        return;
    }
    let mut analysis = analyze(
        r"\documentclass{article}
\begin{document}
\pdfliteral{q}
\end{document}",
    );
    let duplicate = analysis
        .facts
        .occurrences
        .iter()
        .find(|o| o.kind == OccKind::Pdf && o.span.file == analysis.main_file)
        .cloned()
        .expect("the run recorded the \\pdfliteral");
    let mut duplicate = duplicate;
    duplicate.expanded = Some(satex::tex::Span::default());
    analysis.facts.occurrences.push(duplicate);
    let unbalanced = findings(&analysis, "unbalanced-pdf-content");
    assert_eq!(unbalanced.len(), 1, "{unbalanced:?}");
}

#[test]
fn content_stream_operators_skip_strings_and_dictionaries() {
    use satex::plugin::pdf::{Operator, operators};
    let ops = operators("q (a Q in a string) Tj /Span << /ActualText (EMC) >> BDC /OC /l1 BDC EMC EMC Q % Q\n");
    let names: Vec<&str> = ops.iter().map(Operator::name).collect();
    assert_eq!(names, ["q", "BDC", "BDC", "EMC", "EMC", "Q"]);
    assert_eq!(ops[2], Operator::BeginMarked { operator: "BDC", optional_content: Some("l1".into()) });
}

#[test]
fn bib_entries_skip_strings_and_comments() {
    let entries = satex::plugin::bib::entries(
        "@comment{x}\n@String{a = b}\n@Article{one,\n title={@ not an entry}}\n@book ( two , )\n",
    );
    let keys: Vec<(&str, u32)> = entries.iter().map(|e| (e.key.as_str(), e.line)).collect();
    assert_eq!(keys, [("one", 3), ("two", 5)]);
}

/// amsmath defers a display's `\label` to the end of the environment, so
/// the `\newlabel` line is written at the `\end` and, for an absorbed body,
/// at the `\begin`.  The key's own tokens still stand where the `\label`
/// put them.
#[test]
fn a_label_in_a_display_stands_where_the_document_wrote_it() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{amsmath}
\begin{document}
Text \label{sec:a} more.
\begin{equation}
  g = h
  \label{eq:b}
\end{equation}
\begin{align}
  c &= d \label{eq:a} \\
  e &= f \label{eq:c}
\end{align}
\end{document}",
    );
    let labels = places(&analysis, OccKind::Label);
    assert_eq!(
        labels,
        [("sec:a".into(), 4, 6), ("eq:b".into(), 7, 10), ("eq:a".into(), 10, 17), ("eq:c".into(), 11, 17)],
        "{labels:?}"
    );
    // The write site stays, as what the key was reached through.
    let written: Vec<(String, Option<u32>)> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Label && o.span.file == analysis.main_file)
        .map(|o| (o.key.clone(), o.expanded.map(|e| e.line)))
        .collect();
    assert_eq!(
        written,
        [("sec:a".into(), None), ("eq:b".into(), Some(8)), ("eq:a".into(), Some(9)), ("eq:c".into(), Some(9))],
        "{written:?}"
    );
}

/// amsmath reads a display's body as one argument (`\collect@@body#1\end#2`),
/// so the `\end` is matched as a delimiter inside the call the `\begin`
/// started.  The delimiter token still stands in the file.
#[test]
fn an_absorbed_body_ends_at_its_own_end() {
    if !installed() {
        return;
    }
    let analysis = analyze(
        r"\documentclass{article}
\usepackage{amsmath}
\begin{document}
\begin{align}
  c &= d \\
  e &= f
\end{align}
\begin{equation}
  g = h
\end{equation}
\end{document}",
    );
    let ends = places(&analysis, OccKind::EndEnvironment);
    assert_eq!(ends, [("align".into(), 7, 1), ("equation".into(), 10, 1), ("document".into(), 11, 1)], "{ends:?}");
    let begins = places(&analysis, OccKind::BeginEnvironment);
    assert_eq!(begins, [("document".into(), 3, 1), ("align".into(), 4, 1), ("equation".into(), 8, 1)], "{begins:?}");
}

/// tex.web § 465: the text a primitive produces stands where its operand
/// did, so `\string\x` gives back a whole name at one span.  Counting each
/// of those pushed the position of a token the main file had peeked and
/// given back out of the window, and `\begin{document}` was then read as a
/// macro body's and attributed to the `\usepackage` before it.  hyperref
/// produces enough such text to do it, so it is last in every case here.
#[test]
fn begin_document_stands_at_its_own_line_after_any_number_of_packages() {
    if !installed() {
        return;
    }
    for packages in [&["hyperref"][..], &["amsmath", "hyperref"][..], &["amsmath", "xcolor", "hyperref"][..]] {
        let loads: String = packages.iter().map(|p| format!("\\usepackage{{{p}}}\n")).collect();
        let source = format!("\\documentclass{{article}}\n{loads}\\begin{{document}}\nx\n\\end{{document}}\n");
        let analysis = analyze(&source);
        let begin: Vec<(String, u32, u32)> = places(&analysis, OccKind::BeginEnvironment);
        assert_eq!(begin, [("document".to_string(), 2 + packages.len() as u32, 1)], "{packages:?}: {begin:?}");
    }
}

/// BibTeX writes the `.bbl`: its typography is the style's, not the
/// author's, so no warning stands there; the document's own text still has
/// them.
#[test]
fn a_bbl_file_has_no_findings_of_its_own() {
    if !installed() {
        return;
    }
    let analysis = analyze_in(
        "bbl",
        "\\documentclass{article}\n\\begin{document}\nSee \\cite{a} , e.g. this.\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n",
        &[
            ("refs.bib", "@book{a, title={T}, author={A}}\n"),
            (
                "bbl.bbl",
                "\\begin{thebibliography}{1}\n\\bibitem{a} A. Author , e.g. a title.\n\\end{thebibliography}\n",
            ),
        ],
    );
    let all = satex::lint::lint(&analysis);
    let place = |r: &satex::query::Record| r["file"].as_str().unwrap_or_default().to_string();
    assert!(!all.iter().any(|r| place(r).ends_with(".bbl")), "{all:?}");
    assert!(all.iter().any(|r| place(r).ends_with("bbl.tex")), "{all:?}");
    cleanup("bbl");
}
