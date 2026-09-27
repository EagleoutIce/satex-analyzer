//! Lints over the colours xcolor stores through `\color@⟨name⟩` and the
//! `\pdfcolorstack`s the run executed.

use std::path::PathBuf;

use satex::config::Config;
use satex::machine::Machine;

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("satex-colors").join(format!("{name}-{}", std::process::id()));
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
    let cfg = Config::default();
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let out = satex::lint::lint(&analysis)
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
