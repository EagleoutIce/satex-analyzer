//! The spacing rules, checked as a cycle: a clean document gets one known
//! mistake, the rule finds it, and its fix gives the clean document back,
//! byte for byte. A fix that only silences its finding, or writes another
//! spelling than the one the rule asks for, fails here.

use satex::analysis::machine::{Analysis, Machine};
use satex::linter::apply;
use satex::project::config::{Config, Load};
use satex::queries::query::Record;

/// Every construct the spacing rules look at, written the way they ask for.
const CLEAN: &str = "\\documentclass{article}
\\begin{document}
\\section{Intro}\\label{sec:intro}
We use tools, e.g.\\ hammers and saws, as section~\\ref{sec:intro} says.
It came from NASA\\@. Then we went on.
\\LaTeX{} is nice, and so is \\TeX{} here.
A note (see below) and more text (see above) too.
A word\\footnote{A note.} after, and a tie~here.
\\end{document}
";

/// The rules' names, what the clean text holds, and the mistake put in its place.
const MISTAKES: &[(&str, &str, &str)] = &[
    ("missing-nonbreaking-space", "section~\\ref", "section \\ref"),
    ("sentence-spacing", "e.g.\\ hammers", "e.g. hammers"),
    ("sentence-spacing", "NASA\\@. Then", "NASA. Then"),
    ("space-before-punctuation", "saws, as", "saws , as"),
    ("command-terminated-space", "\\LaTeX{} is", "\\LaTeX is"),
    ("parenthesis-spacing", "below) and", "below)and"),
    ("parenthesis-spacing", "text (see above)", "text(see above)"),
    ("space-before-command", "word\\footnote", "word \\footnote"),
    ("double-space", "tie~here", "tie~ here"),
];

fn analyze(path: &std::path::Path, cfg: &Config) -> Analysis {
    Machine::analyze(&std::fs::read_to_string(path).unwrap(), Some(path), cfg)
}

/// The findings of `code` the document itself raises.
fn found(analysis: &Analysis, code: &str) -> Vec<Record> {
    satex::linter::lint(analysis).into_iter().filter(|r| r["code"] == code && r["origin"] == "document").collect()
}

/// The clean text raises none of the rules, so a finding below is the mistake's alone.
#[test]
fn the_clean_document_raises_no_spacing_finding() {
    if !crate::support::installed() {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("spacing-cycle-clean"));
    let path = dir.join("doc.tex");
    std::fs::write(&path, CLEAN).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = analyze(&path, &cfg);
    for (code, _, _) in MISTAKES {
        assert!(found(&analysis, code).is_empty(), "{code}: {:?}", found(&analysis, code));
    }
}

/// Each mistake is found once, and fixing it restores the clean text exactly.
#[test]
fn each_spacing_fix_restores_the_clean_text() {
    if !crate::support::installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    for (k, (code, clean, wrong)) in MISTAKES.iter().enumerate() {
        assert_eq!(CLEAN.matches(clean).count(), 1, "{clean} stands once");
        let dir = crate::support::scratch_dir(crate::support::tmp_root().join(format!("spacing-cycle-{k}")));
        let path = dir.join("doc.tex");
        std::fs::write(&path, CLEAN.replace(clean, wrong)).unwrap();
        let analysis = analyze(&path, &cfg);
        let records = found(&analysis, code);
        assert_eq!(records.len(), 1, "{code} on `{wrong}`: {records:?}");
        let keep = |r: &Record| r["code"] == *code && r["origin"] == "document";
        let main = analysis.file_name(analysis.main_file).to_string();
        let outcome = apply::fixpoint(&main, &cfg, records, &keep, true);
        apply::write(&outcome).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), CLEAN, "{code}: fixing `{wrong}`");
    }
}
