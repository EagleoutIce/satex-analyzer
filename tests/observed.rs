//! What the run observes rather than looks up: citations from the
//! `\citation` lines written to the `.aux`, `\bibitem`s from theirs, what a
//! `\csname` is formed for, the accents OT1 builds, and the payloads a
//! document hands the PDF writer.

use std::path::{Path, PathBuf};

use satex::builtins::OccKind;
use satex::config::Config;
use satex::machine::{Analysis, Machine};

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

fn analyze(source: &str) -> Analysis {
    Machine::analyze(source, None, &Config::default())
}

/// A document written to a directory of its own, beside the files it names;
/// the lints read those files too, so the directory outlives the analysis.
fn analyze_in(name: &str, source: &str, beside: &[(&str, &str)]) -> Analysis {
    let dir: PathBuf = std::env::temp_dir().join(format!("satex-observed-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temporary directory");
    for (file, text) in beside {
        std::fs::write(dir.join(file), text).expect("write beside");
    }
    let main = dir.join(format!("{name}.tex"));
    std::fs::write(&main, source).expect("write document");
    Machine::analyze(source, Some(Path::new(&main)), &Config::default())
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
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("satex-observed-{name}-{}", std::process::id())));
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
        assert!(fix.starts_with(&format!("use {want}")) && fix.contains("etoolbox"), "{fix} for {want}");
    }

    let without = analyze(&format!("\\documentclass{{article}}\n{body}"));
    let found = fixes(&without);
    let expected = [Some("use \\@namedef"), None, None, None, Some("use \\@ifundefined"), Some("use \\@nameuse")];
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
}

#[test]
fn microtype_is_recommended_only_where_the_engine_supports_it() {
    if !installed() {
        return;
    }
    let source = r"\documentclass{article}
\begin{document}
Text.
\end{document}";
    assert_eq!(findings(&analyze(source), "microtype-available").len(), 1);
    let loaded = analyze(
        r"\documentclass{article}
\usepackage{microtype}
\begin{document}
Text.
\end{document}",
    );
    assert!(findings(&loaded, "microtype-available").is_empty());
    let xetex = Config { engine: Some(satex::config::Engine::XeTeX), ..Config::default() };
    assert!(findings(&Machine::analyze(source, None, &xetex), "microtype-available").is_empty());
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
    assert_eq!(messages, ["Q has no q before it to close"]);
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
    assert!(unbalanced[0].0.contains("BDC is never closed by EMC"), "{unbalanced:?}");
    assert!(findings(&analysis, "undefined-optional-content").is_empty(), "no resources were observed");
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
