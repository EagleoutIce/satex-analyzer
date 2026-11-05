//! Lints over the colors xcolor stores through `\color@⟨name⟩` and the
//! `\pdfcolorstack`s the run executed.

use std::path::PathBuf;

use crate::support::installed;
use satex::analysis::machine::Machine;

fn scratch(name: &str) -> PathBuf {
    let directory = crate::support::tmp_root().join("satex-colors").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// The `name` of every finding of `code` on a document whose preamble and
/// body are given, in the order reported.
fn findings(name: &str, code: &str, preamble: &str, body: &str) -> Vec<String> {
    let directory = scratch(name);
    let document = format!("\\documentclass{{article}}\n{preamble}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = crate::support::config();
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let out = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|record| record["code"] == code)
        .map(|record| record["name"].as_str().unwrap_or_default().to_string())
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

#[test]
fn a_colour_used_before_it_is_ever_defined_is_reported() {
    if !installed() {
        return;
    }
    assert_eq!(
        findings("undef", "undefined-color", "\\usepackage{xcolor}", "\\textcolor{nosuchcolor}{x}"),
        ["nosuchcolor"]
    );
}

#[test]
fn a_colour_defined_and_used_raises_nothing() {
    if !installed() {
        return;
    }
    let f = findings(
        "used",
        "undefined-color",
        "\\usepackage{xcolor}\n\\definecolor{myred}{RGB}{255,0,0}",
        "\\textcolor{myred}{x}",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// xcolor looks a mix (`green!50!blue`) up as a name before it parses it as an expression: that
/// lookup is no use of an undefined color.
#[test]
fn a_colour_mix_is_no_undefined_colour() {
    if !installed() {
        return;
    }
    let f = findings("mix", "undefined-color", "\\usepackage{xcolor}", "\\textcolor{green!50!blue}{x}");
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_defined_colour_never_read_back_is_reported() {
    if !installed() {
        return;
    }
    assert_eq!(
        findings("unused", "unused-color", "\\usepackage{xcolor}\n\\definecolor{leftover}{RGB}{1,2,3}", "hello"),
        ["leftover"]
    );
}

#[test]
fn a_colour_used_by_colorlet_is_not_reported_as_unused() {
    if !installed() {
        return;
    }
    let f = findings(
        "letused",
        "unused-color",
        "\\usepackage{xcolor}\n\\definecolor{base}{RGB}{1,2,3}\n\\colorlet{tint}{base!50}",
        "\\textcolor{tint}{x}",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn redefining_a_colour_with_a_different_value_is_reported() {
    if !installed() {
        return;
    }
    assert_eq!(
        findings(
            "conflict",
            "conflicting-color-definition",
            "\\usepackage{xcolor}\n\\definecolor{myred}{RGB}{255,0,0}\n\\definecolor{myred}{RGB}{0,0,0}",
            "hello"
        ),
        ["myred"]
    );
}

#[test]
fn redefining_a_colour_with_the_same_value_is_not_reported() {
    if !installed() {
        return;
    }
    let f = findings(
        "same",
        "conflicting-color-definition",
        "\\usepackage{xcolor}\n\\definecolor{myred}{RGB}{255,0,0}\n\\definecolor{myred}{RGB}{255,0,0}",
        "hello",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_pdfcolorstack_push_never_popped_is_reported() {
    if !installed() {
        return;
    }
    assert_eq!(
        findings("unbalanced", "unbalanced-color-stack", "", "\\pdfcolorstack 3 push {1 0 0 rg}"),
        ["pdfcolorstack"]
    );
}

#[test]
fn a_balanced_pdfcolorstack_raises_nothing() {
    if !installed() {
        return;
    }
    let f =
        findings("balanced", "unbalanced-color-stack", "", "\\pdfcolorstack 3 push {1 0 0 rg}\\pdfcolorstack 3 pop");
    assert!(f.is_empty(), "{f:?}");
}

/// beamer's color themes look their colors up (`fg`, `itemize/enumerate body.fg`) without any of
/// them being an error: no undefined color.
#[test]
fn beamers_color_lookups_are_no_undefined_colours() {
    if !installed() {
        return;
    }
    let directory = scratch("beamer");
    let document = "\\documentclass{beamer}\n\\begin{document}\n\\begin{frame}{T}\n\\begin{itemize}\n\\item<1-> A\n\\end{itemize}\n\\end{frame}\n\\end{document}\n";
    let main = directory.join("main.tex");
    std::fs::write(&main, document).unwrap();
    let analysis = Machine::analyze(document, Some(&main), &crate::support::config());
    let found: Vec<String> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|record| record["code"] == "undefined-color")
        .map(|record| record["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(found.is_empty(), "{found:?}");
    let _ = std::fs::remove_dir_all(&directory);
}
