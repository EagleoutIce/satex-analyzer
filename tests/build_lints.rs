//! The `build-*` lints: a `latexmkrc` or `% arara:` directive held against
//! what the run observed the document to need.

use std::path::PathBuf;

use serde_json::Value as Json;

use satex::config::Config;
use satex::machine::Machine;

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("satex-build-lints").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// `code@file:line` for every `build-*` finding on `document` with `rc` as
/// its latexmkrc (none when `rc` is empty).
fn findings(name: &str, rc: &str, document: &str) -> Vec<String> {
    let directory = scratch(name);
    if !rc.is_empty() {
        std::fs::write(directory.join("latexmkrc"), rc).unwrap();
    }
    let main = directory.join("main.tex");
    std::fs::write(&main, document).unwrap();
    let cfg = Config::discover(&directory);
    let analysis = Machine::analyze(document, Some(&main), &cfg);
    let field = |record: &serde_json::Map<String, Json>, key: &str| {
        record.get(key).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).unwrap_or_default()
    };
    let out = satex::lint::lint(&analysis)
        .into_iter()
        .filter(|record| field(record, "code").starts_with("build-"))
        .map(|record| format!("{}@{}:{}", field(&record, "code"), field(&record, "file"), field(&record, "line")))
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

fn has(found: &[String], wanted: &str) -> bool {
    found.iter().any(|f| f == wanted)
}

const PLAIN_DOCUMENT: &str = "\\documentclass{article}\n\\begin{document}\nText.\n\\end{document}\n";

const SHELL_DOCUMENT: &str =
    "\\documentclass{article}\n\\begin{document}\n\\immediate\\write18{echo hi}\n\\end{document}\n";

#[test]
fn a_document_that_calls_the_shell_needs_shell_escape_in_the_build() {
    if !installed() {
        return;
    }
    let rc = "$pdf_mode = 1;\n$pdflatex = 'pdflatex %O %S';\n";
    let found = findings("shell-missing", rc, SHELL_DOCUMENT);
    assert!(has(&found, "build-shell-escape-missing@latexmkrc:2"), "{found:?}");
    let rc = "$pdf_mode = 1;\n$pdflatex = 'pdflatex -shell-escape %O %S';\n";
    let found = findings("shell-given", rc, SHELL_DOCUMENT);
    assert!(!found.iter().any(|f| f.starts_with("build-shell-escape")), "{found:?}");
}

#[test]
fn shell_escape_the_document_never_uses_is_criticised() {
    if !installed() {
        return;
    }
    let rc = "$pdf_mode = 1;\n$pdflatex = 'pdflatex -shell-escape %O %S';\n";
    let found = findings("shell-unneeded", rc, PLAIN_DOCUMENT);
    assert!(has(&found, "build-shell-escape-unneeded@latexmkrc:2"), "{found:?}");
    let rc = "set_tex_cmds('--shell-escape %O %S');\n";
    let found = findings("shell-unneeded-all", rc, PLAIN_DOCUMENT);
    assert!(has(&found, "build-shell-escape-unneeded@latexmkrc:1"), "{found:?}");
}

#[test]
fn arara_directives_are_held_against_the_shell_escape_too() {
    if !installed() {
        return;
    }
    let document = format!("% arara: pdflatex\n{SHELL_DOCUMENT}");
    let found = findings("arara-missing", "", &document);
    assert!(has(&found, "build-shell-escape-missing@main.tex:1"), "{found:?}");
    let document = format!("% arara: pdflatex: {{ shell: yes }}\n{PLAIN_DOCUMENT}");
    let found = findings("arara-unneeded", "", &document);
    assert!(has(&found, "build-shell-escape-unneeded@main.tex:1"), "{found:?}");
}

#[test]
fn the_engine_the_build_selects_has_to_be_the_one_the_document_needs() {
    if !installed() {
        return;
    }
    let rc = "$pdf_mode = 1;\n";
    let document = format!("% !TeX program = lualatex\n{PLAIN_DOCUMENT}");
    let found = findings("engine-magic", rc, &document);
    assert!(has(&found, "build-engine-mismatch@latexmkrc:1"), "{found:?}");
    let document = "\\documentclass{article}\n\\begin{document}\n\\directlua{tex.print('x')}\n\\end{document}\n";
    let found = findings("engine-primitive", rc, document);
    assert!(has(&found, "build-engine-mismatch@latexmkrc:1"), "{found:?}");
    let found = findings("engine-agrees", "$pdf_mode = 4;\n", document);
    assert!(!found.iter().any(|f| f.starts_with("build-engine-mismatch")), "{found:?}");
}

#[test]
fn pdfoutput_contradicting_pdf_mode_is_found() {
    if !installed() {
        return;
    }
    let document = "\\pdfoutput=0\n\\documentclass{article}\n\\begin{document}\nText.\n\\end{document}\n";
    let found = findings("pdfoutput-dvi", "$pdf_mode = 1;\n", document);
    assert!(has(&found, "build-engine-mismatch@latexmkrc:1"), "{found:?}");
    let document = "\\pdfoutput=1\n\\documentclass{article}\n\\begin{document}\nText.\n\\end{document}\n";
    let found = findings("pdfoutput-pdf", "$pdf_mode = 1;\n", document);
    assert!(!found.iter().any(|f| f.starts_with("build-engine-mismatch")), "{found:?}");
}

const BIBLIOGRAPHY: &str = "\\documentclass{article}\n\\begin{document}\n\\cite{k}\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";

#[test]
fn a_bibliography_needs_bibtex_or_biber_to_run() {
    if !installed() {
        return;
    }
    let found = findings("bib-disabled", "$bibtex_use = 0;\n", BIBLIOGRAPHY);
    assert!(has(&found, "build-bibliography-disabled@latexmkrc:1"), "{found:?}");
    let found = findings("bib-unneeded", "$bibtex_use = 2;\n", PLAIN_DOCUMENT);
    assert!(has(&found, "build-bibliography-unneeded@latexmkrc:1"), "{found:?}");
    let found = findings("bib-fine", "$bibtex_use = 2;\n", BIBLIOGRAPHY);
    assert!(!found.iter().any(|f| f.starts_with("build-bibliography")), "{found:?}");
}

const GLOSSARY: &str = "\\documentclass{article}\n\\usepackage{glossaries}\n\\makeglossaries\n\
\\newglossaryentry{x}{name=x,description=y}\n\\begin{document}\n\\gls{x}\n\\printglossaries\n\\end{document}\n";

#[test]
fn a_glossary_needs_a_custom_dependency_and_a_dependency_needs_a_use() {
    if !installed() {
        return;
    }
    let found = findings("glossary-missing", "$pdf_mode = 1;\n", GLOSSARY);
    assert!(has(&found, "build-missing-custom-dependency@latexmkrc:1"), "{found:?}");
    let rc = "add_cus_dep('glo', 'gls', 0, 'makeglossaries');\n";
    let found = findings("glossary-given", rc, GLOSSARY);
    assert!(!found.iter().any(|f| f.contains("custom-dependency")), "{found:?}");
    let found = findings("dependency-unused", rc, PLAIN_DOCUMENT);
    assert!(has(&found, "build-unused-custom-dependency@latexmkrc:1"), "{found:?}");
}

#[test]
fn engine_commands_need_their_placeholders_and_may_want_more_options() {
    if !installed() {
        return;
    }
    let found = findings("placeholders", "$pdf_mode = 1;\n$pdflatex = 'pdflatex';\n", PLAIN_DOCUMENT);
    assert!(has(&found, "build-command-placeholders@latexmkrc:2"), "{found:?}");
    assert!(has(&found, "build-engine-options@latexmkrc:2"), "{found:?}");
    let rc = "$pdf_mode = 1;\n$pdflatex = 'pdflatex -file-line-error -synctex=1 %O %S';\n";
    let found = findings("options-given", rc, PLAIN_DOCUMENT);
    assert!(found.is_empty(), "{found:?}");
}
