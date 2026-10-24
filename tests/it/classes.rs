//! The basic document classes: a small project in each, with an included
//! chapter, a float, a cross reference and a `\clearpage`, runs to its end
//! without a cut, keeps the expl3 map depth and the page counter known,
//! and raises no error of its own.

use crate::support::{broken_gaps, codes, installed, last_body as body};
use satex::machine::{Analysis, Machine};

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

fn check(class: &str, part: &str) {
    if !installed() {
        return;
    }
    let a = project(class, part);
    let cuts: Vec<_> = a.facts.diagnostics.iter().filter(|d| CUTS.contains(&d.code)).collect();
    assert!(cuts.is_empty(), "{class}: cut {:?}", cuts.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>());
    assert!(broken_gaps(&a).is_empty(), "{class}: {:?}", broken_gaps(&a));
    assert_eq!(body(&a, "depth"), "0", "{class}: map depth");
    assert!(body(&a, "pages").parse::<u32>().is_ok(), "{class}: page counter {}", body(&a, "pages"));
    let errors: Vec<_> = codes(&a).into_iter().filter(|c| c.starts_with("undefined") || c.contains("error")).collect();
    assert!(errors.is_empty(), "{class}: {errors:?}");
}

macro_rules! classes {
    ($($name:ident: $class:literal, $part:literal;)*) => {$(
        #[test]
        fn $name() {
            check($class, $part);
        }
    )*};
}

classes! {
    article: "article", "section";
    report: "report", "chapter";
    book: "book", "chapter";
    scrartcl: "scrartcl", "section";
    scrreprt: "scrreprt", "chapter";
    scrbook: "scrbook", "chapter";
    memoir: "memoir", "chapter";
    amsart: "amsart", "section";
    amsbook: "amsbook", "chapter";
}

/// A beamer frame instead of the sectioned project.
#[test]
fn beamer() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{beamer}\n\\begin{document}\n\\begin{frame}{One}\\begin{itemize}\\item a\\end{itemize}\\end{frame}\n\
                  \\ExplSyntaxOn\\xdef\\depth{\\int_use:N\\g__kernel_prg_map_int}\\ExplSyntaxOff\n\\end{document}\n";
    let a = Machine::analyze(source, None, &crate::support::config());
    assert!(!a.facts.diagnostics.iter().any(|d| CUTS.contains(&d.code)), "beamer: cut");
    assert_eq!(body(&a, "depth"), "0");
}
