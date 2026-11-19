//! Quick fixes for the errors LaTeX stops or warns on about packages, files,
//! references and citations: each MWE gets a finding with edits, applying them
//! gives the expected text, and a rerun no longer reports the finding.

use crate::fix::{BODY, check, fix, fix_files, lint_codes};
use crate::support::installed;

const BIB: &str = "@book{knuth1984,\n  author = {Donald Knuth},\n  title = {The TeXbook},\n  year = {1984}\n}\n";

/// A package name one letter off an installed one (`graphix`) is respelled:
/// `File graphix.sty not found` is gone.
#[test]
fn a_misspelled_package_is_respelled() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n\\usepackage{{graphix}}\n{BODY}");
    let fixed = fix("qf-respell", &source, "unknown-package-name", true);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n{BODY}"));
}

/// A package no installed name is near cannot be respelled: deleting its
/// load lets LaTeX go on; from a list only its name goes.
#[test]
fn an_unknown_package_is_deleted() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n\\usepackage{{zqxwvnopackage}}\n{BODY}");
    let fixed = fix("qf-delete", &source, "unresolved-file", true);
    check(&fixed, &format!("\\documentclass{{article}}\n{BODY}"));
    drop(fixed);
    let source = format!("\\documentclass{{article}}\n\\usepackage{{amsmath,zqxwvnopackage}}\n{BODY}");
    let fixed = fix("qf-delete-list", &source, "unresolved-file", true);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage{{amsmath}}\n{BODY}"));
}

/// Fixes the undefined `\\{name}` of `body` and asserts that a package of `packages` is loaded
/// and that the rerun finds the name defined.
fn loads_a_package(name: &str, body: &str, packages: &[&str]) {
    let source = format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let fixed = fix(&format!("qf-load-{name}"), &source, "undefined-control-sequence", true);
    let loaded = packages.iter().find(|p| fixed.text.contains(&format!("\\usepackage{{{p}}}\n")));
    assert!(loaded.is_some(), "\\{name}: no load of {packages:?} in\n{}", fixed.text);
    assert!(fixed.remaining.is_empty(), "\\{name} still reported: {:?}", fixed.remaining);
}

/// A command only a package defines (`\toprule`, `\mathbb`, ...) used without it is an undefined
/// control sequence: the fix loads the package most files use that defines it itself
/// (`graphics` or `graphicx` for `\includegraphics`, never tikz, which only loads it, or
/// adjustbox, which redefines it), and the rerun finds it defined.
#[test]
fn a_command_without_its_package_loads_it() {
    if !installed() {
        return;
    }
    let cases: [(&str, &str, &[&str]); 4] = [
        ("toprule", "\\begin{tabular}{l}\\toprule x\\\\\\bottomrule\\end{tabular}", &["booktabs"]),
        ("includegraphics", "\\includegraphics{example-image}", &["graphicx", "graphics"]),
        ("href", "\\href{https://example.org}{x}", &["hyperref"]),
        ("mathbb", "$\\mathbb{R}$", &["amssymb", "amsfonts"]),
    ];
    for (name, body, packages) in cases {
        loads_a_package(name, body, packages);
    }
}

/// A package that defines a command with a definer of its own or of xparse (url's
/// `\DeclareUrlCommand\url`, siunitx's `\NewDocumentCommand \SI` in expl3 syntax) is not seen
/// as its definer when the run never used that definer: the exports index then offers a package
/// that defines it plainly (`ltugboat`, lwarp's `SIunits` shim).
#[test]
fn a_command_made_by_a_package_s_own_definer_loads_its_package() {
    if !installed() {
        return;
    }
    loads_a_package("url", "\\url{https://example.org}", &["url", "hyperref"]);
    loads_a_package("SI", "\\SI{3}{\\metre}", &["siunitx"]);
}

/// An arrow tip only a TikZ library declares raises pgf's error at the
/// `\draw`: the fix adds the library's `\usetikzlibrary` to the preamble.
#[test]
fn a_tikz_arrow_tip_loads_its_library() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\tikz\\draw[-Stealth] (0,0) -- (1,0);\n\\end{document}\n";
    let fixed = fix("qf-tikzlib", source, "raised-error", false);
    check(
        &fixed,
        "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{arrows.meta}\n\\begin{document}\n\\tikz\\draw[-Stealth] (0,0) -- (1,0);\n\\end{document}\n",
    );
}

/// A reference one typo away from a label is respelled to it; a key far from
/// every label gets no edit, only the advice.
#[test]
fn a_misspelled_reference_is_respelled() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:intro}\nSee \\ref{sec:intor}.\n\\end{document}\n";
    let fixed = fix("qf-ref", source, "undefined-reference", true);
    check(&fixed, &source.replace("ref{sec:intor}", "ref{sec:intro}"));
    let far = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:intro}\nSee \\ref{fig:plot}.\n\\end{document}\n";
    let (records, dir) = lint_codes("qf-ref-far", far, "undefined-reference");
    assert!(records.len() == 1 && records[0].get("edits").is_none(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A key with `_` (a subscript character) is still a key of the list.
#[test]
fn a_reference_with_an_underscore_is_respelled() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec_intro}\nSee \\ref{sec_intor}.\n\\end{document}\n";
    let fixed = fix("qf-ref-underscore", source, "undefined-reference", true);
    check(&fixed, &source.replace("ref{sec_intor}", "ref{sec_intro}"));
}

/// A citation key one typo away from a database entry is respelled, inside a
/// key list behind an optional argument, leaving the other keys alone.
#[test]
fn a_misspelled_citation_is_respelled() {
    if !installed() {
        return;
    }
    let bib = format!(
        "{BIB}@book{{lamport1994,\n  author = {{Leslie Lamport}},\n  title = {{LaTeX}},\n  year = {{1994}}\n}}\n"
    );
    let source = "\\documentclass{article}\n\\begin{document}\n\\cite[p.~3]{lamport1994,knut1984}\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    let fixed = fix_files("qf-cite", &[("doc.tex", source), ("refs.bib", &bib)], "undefined-citation", true, false);
    check(&fixed, &source.replace("knut1984", "knuth1984"));
}

/// The second of two `\label{a}`s is renamed to a free key; no reference is
/// touched (they now resolve to the first, which the unsafe fix says).
#[test]
fn a_duplicate_label_is_renamed() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{a}\n\\section{B}\\label{a}\n\\label{a-2}See \\ref{a}.\n\\end{document}\n";
    let fixed = fix("qf-duplicate-label", source, "duplicate-label", true);
    check(&fixed, &source.replace("\\section{B}\\label{a}", "\\section{B}\\label{a-3}"));
}

/// Citations with no `\bibliography` at all: the `.bib` beside the document
/// is named at its end, with a style, so BibTeX finds its `\bibdata`.
#[test]
fn a_missing_bibliography_names_the_database() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\cite{knuth1984}\n\\end{document}\n";
    let fixed = fix_files("qf-nobib", &[("doc.tex", source), ("refs.bib", BIB)], "missing-bibliography", true, false);
    check(
        &fixed,
        "\\documentclass{article}\n\\begin{document}\n\\cite{knuth1984}\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n",
    );
}

/// Under biblatex the database goes to `\addbibresource` in the preamble and
/// the list to `\printbibliography`.
#[test]
fn a_missing_bibliography_under_biblatex_adds_the_resource() {
    if !installed() {
        return;
    }
    let source =
        "\\documentclass{article}\n\\usepackage{biblatex}\n\\begin{document}\n\\cite{knuth1984}\n\\end{document}\n";
    let fixed =
        fix_files("qf-nobib-biblatex", &[("doc.tex", source), ("refs.bib", BIB)], "missing-bibliography", true, false);
    assert_eq!(
        fixed.text,
        "\\documentclass{article}\n\\usepackage{biblatex}\n\\addbibresource{refs.bib}\n\\begin{document}\n\\cite{knuth1984}\n\\printbibliography\n\\end{document}\n"
    );
    assert!(fixed.remaining.is_empty(), "{:?}", fixed.remaining);
}

/// The playground's bundle holds a few packages: the names of all installed ones come from the
/// `exports.tsv` the site build ships in it (`satex query exports`). A run over a bundle reads
/// them there, and `\toprule` still gets `\usepackage{booktabs}`. The list stays small: no
/// internal (`@`, `_`) names, one file per name.
#[test]
fn a_bundle_offers_packages_from_its_shipped_names() {
    if !installed() {
        return;
    }
    let cfg = satex::project::config::Config {
        load: satex::project::config::Load { classes: true, ..Default::default() },
        ..crate::support::config()
    };
    let probe = "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";
    let probe = satex::analysis::machine::Machine::analyze(probe, None, &cfg);
    let names: Vec<(String, String)> = satex::queries::query::run(
        &probe,
        satex::queries::query::Query::Exports,
        &satex::queries::query::Filter::Always,
    )
    .iter()
    .map(|r| (r["name"].as_str().unwrap_or_default().to_string(), r["file"].as_str().unwrap_or_default().to_string()))
    .collect();
    assert!(names.iter().any(|(n, f)| n == "toprule" && f == "booktabs.sty"), "{} names", names.len());
    assert!(names.iter().all(|(n, _)| !n.contains(['@', '_'])));
    let mut by_file: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    names.iter().for_each(|(n, f)| by_file.entry(f).or_default().push(n));
    let text: String = by_file.iter().map(|(f, ns)| format!("{f}\t{}\n", ns.join("\t"))).collect();
    eprintln!("exports.tsv: {} names, {} bytes", names.len(), text.len());
    let dir = crate::support::tmp_root().join(format!("satex-qf-bundle-{}", std::process::id()));
    let bundle = dir.join("texmf.bundle");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(bundle.join("exports.tsv"), text).unwrap();
    let mut cfg = cfg;
    // the installation's trees and the bundle beside them, as the bundle alone would hold no kernel here
    cfg.texmf.roots = probe.distribution.roots.clone();
    cfg.texmf.bundle = Some(bundle);
    let source =
        "\\documentclass{article}\n\\begin{document}\n\\begin{tabular}{l}\\toprule x\\end{tabular}\n\\end{document}\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).unwrap();
    let analysis = satex::analysis::machine::Machine::analyze(source, Some(&path), &cfg);
    assert!(analysis.distribution.bundle.is_some());
    let records = satex::linter::lint(&analysis);
    let fix = records.iter().find(|r| {
        r["code"] == "undefined-control-sequence" && r["message"].as_str().is_some_and(|m| m.contains("toprule"))
    });
    let edits = fix.and_then(|r| r.get("edits")).map(|e| e.to_string()).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let all: Vec<_> = records.iter().filter(|r| r["code"] == "undefined-control-sequence").collect();
    assert!(edits.contains("usepackage{booktabs}"), "{all:?}");
}
