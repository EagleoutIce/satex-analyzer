//! The basic document classes: a small project in each, with an included
//! chapter, a float, a cross reference and a `\clearpage`, runs to its end
//! without a cut, keeps the expl3 map depth, ends on the page pdflatex
//! ends on, and raises no error of its own.

use crate::support::{analyze_latex, broken_gaps, codes, installed, last_body as body};
use satex::analysis::machine::{Analysis, Machine};

/// Cuts that end or blur a run: none may happen in a plain document.
const CUTS: &[&str] = &["budget-exhausted", "line-budget-cut", "analysis-ended-early", "fault-contained"];

fn project(class: &str, part: &str) -> Analysis {
    let dir = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-class-{class}-{}", std::process::id())),
    );
    let chapter = format!(
        "\\{part}{{Two}}\\label{{two}}\nSee~\\ref{{one}}.\\footnote{{A note.}}\n\
         \\begin{{figure}}[h!]\\centering x\\caption{{A figure}}\\label{{fig}}\\end{{figure}}\n\
         \\begin{{itemize}}\\item one\\end{{itemize}}\n"
    );
    std::fs::write(dir.join("chap.tex"), chapter).unwrap();
    let source = format!(
        "\\documentclass{{{class}}}\n\\begin{{document}}\n\\tableofcontents\n\\{part}{{One}}\\label{{one}}\nText.\n\
         \\include{{chap}}\n\\clearpage\n\
         \\ExplSyntaxOn\\xdef\\depth{{\\int_use:N\\g__kernel_prg_map_int}}\\ExplSyntaxOff\n\
         \\xdef\\pages{{\\the\\value{{page}}}}\n\\end{{document}}\n"
    );
    let doc = dir.join("main.tex");
    std::fs::write(&doc, &source).unwrap();
    Machine::analyze(&source, Some(&doc), &crate::support::config())
}

fn check(class: &str, part: &str, pages: &str) {
    if !installed() {
        return;
    }
    let a = project(class, part);
    let cuts: Vec<_> = a.facts.diagnostics.iter().filter(|d| CUTS.contains(&d.code)).collect();
    assert!(cuts.is_empty(), "{class}: cut {:?}", cuts.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>());
    assert!(broken_gaps(&a).is_empty(), "{class}: {:?}", broken_gaps(&a));
    assert_eq!(body(&a, "depth"), "0", "{class}: map depth");
    assert_eq!(body(&a, "pages"), pages, "{class}: the page counter pdflatex ends with");
    let errors: Vec<_> = codes(&a).into_iter().filter(|c| c.starts_with("undefined") || c.contains("error")).collect();
    assert!(errors.is_empty(), "{class}: {errors:?}");
}

macro_rules! classes {
    ($($name:ident: $class:literal, $part:literal, $pages:literal;)*) => {$(
        #[test]
        fn $name() {
            check($class, $part, $pages);
        }
    )*};
}

classes! {
    article: "article", "section", "3";
    report: "report", "chapter", "4";
    book: "book", "chapter", "6";
    scrartcl: "scrartcl", "section", "3";
    scrreprt: "scrreprt", "chapter", "4";
    scrbook: "scrbook", "chapter", "6";
    memoir: "memoir", "chapter", "6";
    amsart: "amsart", "section", "3";
    amsbook: "amsbook", "chapter", "6";
}

/// A beamer frame instead of the sectioned project.
#[test]
fn beamer() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{beamer}\n\\begin{document}\n\\begin{frame}{One}\\begin{itemize}\\item a\\end{itemize}\\end{frame}\n\
                  \\ExplSyntaxOn\\xdef\\depth{\\int_use:N\\g__kernel_prg_map_int}\\ExplSyntaxOff\n\\end{document}\n";
    let a = analyze_latex(source);
    assert!(!a.facts.diagnostics.iter().any(|d| CUTS.contains(&d.code)), "beamer: cut");
    assert_eq!(body(&a, "depth"), "0");
}

/// A scan library code opened that reads on through the rest of the
/// document (here a library `\iffalse` the document never closes) ran past
/// `\end{document}` without running it: the run says it ended early, at the
/// scan's start, never only that the document lacks its end.
#[test]
fn a_library_scan_past_the_end_says_the_analysis_ended_early() {
    if !installed() {
        return;
    }
    let root =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-ran-off-{}", std::process::id())));
    let (lib, doc) = (root.join("lib"), root.join("doc"));
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::create_dir_all(&doc).unwrap();
    std::fs::write(lib.join("hide.tex"), "\\def\\hide{\\iffalse}\n").unwrap();
    let source = "\\documentclass{article}\n\\input{../lib/hide}\n\\begin{document}\nA\\hide B\n\\end{document}\n";
    let main = doc.join("main.tex");
    std::fs::write(&main, source).unwrap();
    let a = Machine::analyze(source, Some(&main), &crate::support::config());
    let early: Vec<_> = a.facts.diagnostics.iter().filter(|d| d.code == "analysis-ended-early").collect();
    assert!(early.iter().any(|d| d.message.contains("hide.tex")), "{:?}", codes(&a));
    assert!(!codes(&a).contains(&"missing-end".to_string()), "{:?}", codes(&a));
}

/// Only a scan past the main file's end hides the document's end: one a
/// chapter's end stopped (TeX inserts `\fi` and reads on) leaves a
/// document that really lacks `\end{document}` said so.
#[test]
fn a_scan_a_chapter_end_stopped_hides_no_missing_end() {
    if !installed() {
        return;
    }
    let root =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-no-end-{}", std::process::id())));
    let (lib, doc) = (root.join("lib"), root.join("doc"));
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::create_dir_all(&doc).unwrap();
    std::fs::write(lib.join("hide.tex"), "\\def\\hide{\\iffalse}\n").unwrap();
    std::fs::write(doc.join("ch.tex"), "A\\hide B\n").unwrap();
    let source = "\\documentclass{article}\n\\input{../lib/hide}\n\\begin{document}\n\\input{ch}\n";
    let main = doc.join("main.tex");
    std::fs::write(&main, source).unwrap();
    let a = Machine::analyze(source, Some(&main), &crate::support::config());
    let codes: Vec<&str> = a.facts.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&"missing-end") && !codes.contains(&"analysis-ended-early"), "{codes:?}");
}
