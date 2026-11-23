//! Quick-fixes: every rule with a mechanical fix, applied to a document on
//! disk.  The fixed text is what the fix promises, linting it again no
//! longer reports the finding, and the result still compiles.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::installed;
use satex::analysis::machine::Machine;
use satex::linter::apply;
use satex::project::config::{Config, Load};
use satex::queries::query::Record;

pub(crate) struct Fixed {
    dir: PathBuf,
    pub(crate) text: String,
    pub(crate) remaining: Vec<Record>,
}

impl Drop for Fixed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-fix-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn select(code: &str, all: bool) -> impl Fn(&Record) -> bool + '_ {
    move |record: &Record| record["code"] == code && (all || record["origin"] == "document")
}

/// Fix the findings of one rule in `doc.tex` of a fresh project, the way
/// `satex lint --fix` does, and read the file back.
pub(crate) fn fix_files(name: &str, files: &[(&str, &str)], code: &str, unsafe_fixes: bool, all: bool) -> Fixed {
    let dir = project(name, files);
    let path = dir.join("doc.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let keep = select(code, all);
    let records: Vec<Record> = satex::linter::lint(&analysis).into_iter().filter(|r| keep(r)).collect();
    assert!(!records.is_empty(), "{code} reports nothing");
    assert!(records.iter().any(|r| r.get("edits").is_some()), "{code} has no edits: {records:?}");
    let main = analysis.file_name(analysis.main_file).to_string();
    let outcome = apply::fixpoint(&main, &cfg, records, &keep, unsafe_fixes);
    apply::write(&outcome).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    Fixed { dir, text, remaining: outcome.remaining }
}

pub(crate) fn fix(name: &str, source: &str, code: &str, unsafe_fixes: bool) -> Fixed {
    fix_files(name, &[("doc.tex", source)], code, unsafe_fixes, false)
}

/// pdflatex accepts the fixed document, when it is installed.
fn compiles(dir: &Path) {
    if satex::project::paths::which("pdflatex").is_err() {
        return;
    }
    let status = Command::new("pdflatex")
        .args(["-interaction=nonstopmode", "-halt-on-error", "doc.tex"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("pdflatex runs");
    let log = std::fs::read_to_string(dir.join("doc.log")).unwrap_or_default();
    assert!(
        status.success(),
        "pdflatex failed:\n{}",
        log.lines().filter(|l| l.starts_with('!')).collect::<Vec<_>>().join("\n")
    );
}

pub(crate) fn check(fixed: &Fixed, expected: &str) {
    assert_eq!(fixed.text, expected);
    assert!(fixed.remaining.is_empty(), "still reported: {:?}", fixed.remaining);
    compiles(&fixed.dir);
}

/// Tests that fixing `code` in `source` (`unsafe`: with the unsafe fixes too)
/// gives exactly the expected text.
macro_rules! fix_cases {
    ($($(#[$doc:meta])* $name:ident($dir:literal, $source:expr, $code:literal, $unsafe:literal, $want:expr);)*) => {$(
        $(#[$doc])*
        #[test]
        fn $name() {
            if !installed() {
                return;
            }
            let source = $source;
            let fixed = fix($dir, AsRef::<str>::as_ref(&source), $code, $unsafe);
            check(&fixed, $want);
        }
    )*};
}

pub(crate) const BODY: &str = "\\begin{document}\nx\n\\end{document}\n";
const PROSE: &str = "Running text that fills the line. ";

fix_cases! {
    /// A document every path reads to its end without TeX's `\end` gets the
    /// ending of its kernel appended.
    missing_end_document_is_appended("missing-end-document", "\\documentclass{article}\n\\begin{document}\nx\n", "missing-end", false, "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n");
    duplicate_package_drops_the_name_from_a_list("duplicate-list", format!(
        "\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\\usepackage{{amsmath,graphicx,amssymb}}\n{BODY}"
    ), "duplicate-package", false, &format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\\usepackage{{amsmath,amssymb}}\n{BODY}"));
    duplicate_package_deletes_a_line_of_its_own("duplicate-line", format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n  \\usepackage{{graphicx}} % again\n{BODY}"), "duplicate-package", false, &format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n{BODY}"));
}

#[test]
fn unused_package_is_an_unsafe_fix() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n\\usepackage{{amssymb,graphicx}}\n{BODY}");
    let safe = fix("unused-package-safe", &source, "unused-package", false);
    assert_eq!(safe.text, source, "a safe run leaves an unsafe fix alone");
    drop(safe);
    let fixed = fix("unused-package", &source, "unused-package", true);
    // Both names are unused: their edits overlap, so the second waits a round
    // and then deletes what is left of the line.
    check(&fixed, &format!("\\documentclass{{article}}\n{BODY}"));
}

/// How many findings of `code` the document `source` gets.
fn reported(name: &str, source: &str, code: &str) -> usize {
    let dir = project(name, &[("doc.tex", source)]);
    let path = dir.join("doc.tex");
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(source, Some(&path), &cfg);
    satex::linter::lint(&analysis).iter().filter(|r| r.get("code").and_then(|c| c.as_str()) == Some(code)).count()
}

/// Intuition: a run a budget stopped never read the rest: a reference to a label there is "maybe
/// defined later", a precision note, never "no `\\label` anywhere" (the label is made by a macro: the
/// source text does not spell its key). Pins 3 false undefined-reference
/// errors on a 60-section article `limits.seconds` stopped at section 9.
#[test]
fn a_reference_past_a_stopped_run_is_only_a_note() {
    if !installed() {
        return;
    }
    let filler = "\\relax".repeat(4000);
    let source = format!(
        "\\documentclass{{article}}\n\\newcommand\\mark[1]{{\\label{{sec:#1}}}}\n\\begin{{document}}\nSee \\ref{{sec:late}}.\n{filler}\n\\section{{Late}}\\mark{{late}}\n\\end{{document}}\n"
    );
    let dir = project("reference-stopped", &[("doc.tex", &source)]);
    let mut cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    cfg.limits.steps =
        Machine::analyze("\\documentclass{article}\\begin{document}x\\end{document}", None, &cfg).steps + 2000;
    let analysis = Machine::analyze(&source, Some(&dir.join("doc.tex")), &cfg);
    assert!(analysis.exhausted, "the budget did not stop the run");
    let found: Vec<serde_json::Value> = (satex::linter::lint(&analysis).into_iter())
        .filter(|r| r["code"] == "undefined-reference")
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    assert!(!found.is_empty() && found.iter().all(|r| r["category"] == "precision"), "{found:?}");
}

/// Intuition: a run that did not read everything may not have met a package's use: "unused" is then a
/// precision note with no fix that deletes the load. Pins `xlistings` (used past the line a time limit
/// stopped the run at) offered for deletion.
#[test]
fn an_unused_package_in_an_incomplete_run_is_only_a_note() {
    if !installed() {
        return;
    }
    // the chapter stays unread (`load.inputs: false`): it may use the package
    let source =
        "\\documentclass{article}\n\\usepackage{needspace}\n\\begin{document}\nA\n\\input{chap}\n\\end{document}\n";
    let dir = project("unused-package-unread", &[("doc.tex", source), ("chap.tex", "\\needspace{2\\baselineskip}B\n")]);
    let mut cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    cfg.load.inputs = false;
    let analysis = Machine::analyze(source, Some(&dir.join("doc.tex")), &cfg);
    let found: Vec<serde_json::Value> = (satex::linter::lint(&analysis).into_iter())
        .filter(|r| r["code"] == "unused-package")
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    assert!(
        !found.is_empty()
            && found.iter().all(|r| r["category"] == "precision" && r.get("edits").is_none_or(|e| e.is_null())),
        "{found:?}"
    );
}

/// A package that sets a font default the text is set in acts on every character, though no name
/// it defines is called: `\\usepackage[T1]{fontenc}` (the kernel's `\\normalfont` reads
/// `\\encodingdefault`). Pins the false finding on nearly every thesis. Without an option fontenc
/// selects OT1, the default already, and is unused.
#[test]
fn a_package_setting_the_text_font_is_used() {
    if !installed() {
        return;
    }
    let doc = |opt: &str| {
        format!(
            "\\documentclass{{article}}\n\\usepackage{opt}{{fontenc}}\n\\begin{{document}}\nHello.\n\\end{{document}}\n"
        )
    };
    assert_eq!(reported("unused-package-t1", &doc("[T1]"), "unused-package"), 0);
    assert_eq!(reported("unused-package-ot1", &doc(""), "unused-package"), 1);
}

/// A package whose command the text of a macro the document runs calls is
/// used, through any number of macros, though the path calling it was not
/// taken. A macro nothing runs uses nothing.
#[test]
fn a_package_a_running_macro_calls_is_used() {
    if !installed() {
        return;
    }
    let defs = "\\newif\\ifpic\n\\newcommand\\pic{\\includegraphics{x}}\n\\newcommand\\maybe{\\ifpic\\pic\\fi}\n";
    let head = format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n{defs}");
    let used = format!("{head}\\begin{{document}}\n\\maybe\n\\end{{document}}\n");
    assert_eq!(reported("unused-package-called", &used, "unused-package"), 0);
    let idle = format!("{head}{BODY}");
    assert_eq!(reported("unused-package-idle", &idle, "unused-package"), 1);
}

/// A command right after `\begin{document}`, or one in the document's own
/// preamble, is a use of its package.
#[test]
fn the_documents_first_command_and_its_preamble_are_uses() {
    if installed() {
        let source = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\\includegraphics{x}\n\\end{document}\n";
        assert_eq!(reported("unused-package-first", source, "unused-package"), 0);
        let source = "\\documentclass{article}\n\\usepackage{etoolbox}\n\\AtEndPreamble{\\def\\foo{x}}\n\\begin{document}\\foo\n\\end{document}\n";
        assert_eq!(reported("unused-package-preamble", source, "unused-package"), 0);
    }
}

/// The class sets the page (`\textheight`), which the kernel reads for every page.
#[test]
fn the_document_class_is_never_unused() {
    if installed() {
        let source = "\\documentclass{article}\n\\begin{document}\n\\end{document}\n";
        assert_eq!(reported("unused-class", source, "unused-package"), 0);
    }
}

/// A package that only changes settings acts without any of its names
/// being used.
#[test]
fn a_package_that_changes_settings_is_used() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n\\usepackage{{parskip}}\n{BODY}");
    assert_eq!(reported("unused-package-parskip", &source, "unused-package"), 0);
}

#[test]
fn option_clash_moves_the_options_to_the_first_load() {
    if !installed() {
        return;
    }
    // The second load is then a plain duplicate, which the next round deletes.
    let source =
        format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\\usepackage[draft]{{graphicx}}\n{BODY}");
    let dir = project("option-clash", &[("doc.tex", &source)]);
    let path = dir.join("doc.tex");
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let keep = |r: &Record| {
        matches!(r["code"].as_str(), Some("option-clash" | "duplicate-package")) && r["origin"] == "document"
    };
    let records: Vec<Record> = satex::linter::lint(&analysis).into_iter().filter(|r| keep(r)).collect();
    let outcome = apply::fixpoint(analysis.file_name(analysis.main_file), &cfg, records, &keep, true);
    apply::write(&outcome).unwrap();
    let fixed = Fixed { text: std::fs::read_to_string(&path).unwrap(), dir, remaining: outcome.remaining };
    assert_eq!(outcome.rounds, 2);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage[draft]{{graphicx}}\n{BODY}"));
}

fix_cases! {
    package_after_preamble_moves_into_the_preamble("after-preamble", "\\documentclass{article}\n\\begin{document}\n\\usepackage[hyphens]{url}\nx\n\\end{document}\n", "package-after-preamble", true, "\\documentclass{article}\n\\usepackage[hyphens]{url}\n\\begin{document}\nx\n\\end{document}\n");
    already_defined_becomes_renewcommand("already-defined", "\\documentclass{article}\n\\newcommand{\\foo}{a}\n\\newcommand{\\foo}{b}\n\\begin{document}\n\\foo\n\\end{document}\n".to_string(), "already-defined", true, "\\documentclass{article}\n\\newcommand{\\foo}{a}\n\\renewcommand{\\foo}{b}\n\\begin{document}\n\\foo\n\\end{document}\n");
    /// The same definition again needs no `\renew…`: `\providecommand` keeps the first, safely.
    same_definition_again_becomes_providecommand("already-defined-same", "\\documentclass{article}\n\\newcommand{\\foo}[1]{(#1)}\n\\newcommand{\\foo}[1]{ (#1) }\n\\begin{document}\n\\foo{x}\n\\end{document}\n".to_string(), "already-defined", false, "\\documentclass{article}\n\\newcommand{\\foo}[1]{(#1)}\n\\providecommand{\\foo}[1]{ (#1) }\n\\begin{document}\n\\foo{x}\n\\end{document}\n");
    not_defined_becomes_newcommand("not-defined", "\\documentclass{article}\n\\renewcommand*{\\foo}{a}\n\\begin{document}\n\\foo\n\\end{document}\n", "not-defined", false, "\\documentclass{article}\n\\newcommand*{\\foo}{a}\n\\begin{document}\n\\foo\n\\end{document}\n");
    environment_mismatch_names_the_open_environment("env-mismatch", "\\documentclass{article}\n\\begin{document}\n\\begin{center}\nx\n\\end{flushleft}\n\\end{document}\n", "environment-mismatch", true, "\\documentclass{article}\n\\begin{document}\n\\begin{center}\nx\n\\end{center}\n\\end{document}\n");
    a_solo_end_is_deleted("env-solo", format!("\\documentclass{{article}}\n\\end{{center}}\n{BODY}"), "environment-mismatch", true, &format!("\\documentclass{{article}}\n{BODY}"));
    unused_label_is_deleted_inline_and_as_a_line("unused-label", "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\\section{B}\n  \\label{sec:b}\nx\n\\end{document}\n", "unused-label", true, "\\documentclass{article}\n\\begin{document}\n\\section{A}\n\\section{B}\nx\n\\end{document}\n");
    /// amsmath writes the `\newlabel` at the end of a display, and reads an
    /// `align` body as one argument: the label is recorded at its own key, so
    /// the deletion finds the `\label` that carries it. The display has a referenced
    /// row: one with none is `unreferenced-numbered-display`'s.
    unused_label_is_deleted_inside_a_display("unused-label-display", "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\begin{align}\n  c &= d \\label{eq:a} \\\\\n  e &= f \\label{eq:e}\n\\end{align}\n\\eqref{eq:e}\n\\end{document}\n", "unused-label", true, "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\begin{align}\n  c &= d  \\\\\n  e &= f \\label{eq:e}\n\\end{align}\n\\eqref{eq:e}\n\\end{document}\n");
    dead_definition_before_a_def_is_a_safe_deletion("dead-definition", "\\documentclass{article}\n\\def\\x#1{[#1]}\n\\def\\x#1{(#1)}\n\\begin{document}\n\\x{a}\n\\end{document}\n", "dead-definition", false, "\\documentclass{article}\n\\def\\x#1{(#1)}\n\\begin{document}\n\\x{a}\n\\end{document}\n");
    unused_definition_deletes_the_whole_newcommand("unused-definition", "\\documentclass{article}\n\\newcommand*{\\unused}[2][d]{%\n  #1 and #2%\n}\n\\begin{document}\nx\n\\end{document}\n", "unused-definition", true, &format!("\\documentclass{{article}}\n{BODY}"));
    csname_becomes_the_etoolbox_command_for_its_role("csname", "\\documentclass{article}\n\\usepackage{etoolbox}\n\\expandafter\\def\\csname my@x\\endcsname{X}\n\\expandafter\\let\\csname my@y\\endcsname\\relax\n\\begin{document}\n\\csname my@x\\endcsname \\csname my@x\\endcsname\n\\end{document}\n", "primitive-tex-command", true, "\\documentclass{article}\n\\usepackage{etoolbox}\n\\csdef{my@x}{X}\n\\cslet{my@y}\\relax\n\\begin{document}\n\\csuse{my@x}\\csuse{my@x}%\n\\end{document}\n");
    /// Prefixes the `\expandafter` chain reaches past stay in front of the
    /// replacement: TeX expands after a prefix, so they apply as before.
    csname_keeps_the_prefixes_of_its_definition("csname-prefixes", "\\documentclass{article}\n\\usepackage{etoolbox}\n\\expandafter\\protected\\expandafter\\def\\csname my@p\\endcsname{P}\n\\long\\expandafter\\def\\csname my@l\\endcsname#1{#1}\n\\begin{document}\n\\csuse{my@p}\\csuse{my@l}{x}\n\\end{document}\n", "primitive-tex-command", true, "\\documentclass{article}\n\\usepackage{etoolbox}\n\\protected\\csdef{my@p}{P}\n\\long\\csdef{my@l}#1{#1}\n\\begin{document}\n\\csuse{my@p}\\csuse{my@l}{x}\n\\end{document}\n");
    microtype_is_loaded_before_the_document("microtype", format!("\\documentclass{{article}}\n\\begin{{document}}\n{}\n\\end{{document}}\n", PROSE.repeat(150)), "microtype-available", true, &format!("\\documentclass{{article}}\n\\usepackage{{microtype}}\n\\begin{{document}}\n{}\n\\end{{document}}\n", PROSE.repeat(150)));
}

#[test]
fn ot1_accents_load_t1_and_latin_modern() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\\"a\n\\end{document}\n";
    // Whether the run sees the `\\accent` is the rule's business, not the fix's.
    let analysis = Machine::analyze(
        source,
        None,
        &Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() },
    );
    if !satex::linter::lint(&analysis).iter().any(|r| r["code"] == "ot1-font-encoding") {
        return;
    }
    let fixed = fix("ot1", source, "ot1-font-encoding", true);
    check(
        &fixed,
        "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\usepackage{lmodern}\n\\begin{document}\n\\\"a\n\\end{document}\n",
    );
}

fix_cases! {
    computer_modern_in_t1_loads_latin_modern("cm-t1", format!("\\documentclass{{article}}\n\\usepackage[T1]{{fontenc}}\n{BODY}"), "computer-modern-in-t1", true, &format!("\\documentclass{{article}}\n\\usepackage[T1]{{fontenc}}\n\\usepackage{{lmodern}}\n{BODY}"));
}

#[test]
fn dependency_file_gains_and_loses_packages() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\nx\n\\end{document}\n";
    let files = [("doc.tex", source), ("DEPENDS.txt", "hard latex\nhard xcolor\n")];
    let fixed = fix_files("depends-missing", &files, "missing-dependency", false, true);
    let depends = std::fs::read_to_string(fixed.dir.join("DEPENDS.txt")).unwrap();
    assert!(depends.starts_with("hard latex\nhard xcolor\nhard graphics\n"), "{depends}");
    assert!(fixed.remaining.is_empty(), "{:?}", fixed.remaining);
    drop(fixed);
    let fixed = fix_files("depends-unused", &files, "unused-dependency", true, true);
    let depends = std::fs::read_to_string(fixed.dir.join("DEPENDS.txt")).unwrap();
    assert_eq!(depends, "hard latex\n");
    assert!(fixed.remaining.is_empty(), "{:?}", fixed.remaining);
}

#[test]
fn edits_outside_the_project_are_never_offered() {
    if !installed() {
        return;
    }
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let dir = project("library", &[("doc.tex", "")]);
    let source = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\nx\n\\end{document}\n";
    let analysis = Machine::analyze(source, Some(&dir.join("doc.tex")), &cfg);
    let project = dir.canonicalize().unwrap();
    for record in satex::linter::lint(&analysis) {
        for edit in record.get("edits").and_then(|e| e.as_array()).into_iter().flatten() {
            let path = PathBuf::from(edit["path"].as_str().unwrap()).canonicalize().unwrap();
            assert!(path.starts_with(&project), "{record:?}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lsp_positions_count_utf16_code_units() {
    let dir = project("lsp", &[("doc.tex", "é𝄞\\label{x}\n")]);
    let path = dir.join("doc.tex").display().to_string();
    let record: Record = serde_json::from_value(serde_json::json!({
        "code": "unused-label", "severity": "info", "message": "m", "fix": "delete", "path": path,
        "file": "doc.tex", "line": 1, "col": 3, "applicability": "safe",
        "edits": [{ "path": path, "start": { "line": 1, "col": 3 }, "end": { "line": 1, "col": 12 }, "replacement": "" }],
    }))
    .unwrap();
    let out: serde_json::Value = serde_json::from_str(&satex::cli::render::lsp(&[record])).unwrap();
    let action = &out[0]["codeActions"][0];
    let edit = action["edit"]["changes"].as_object().unwrap().values().next().unwrap()[0].clone();
    // `é` is one UTF-16 unit, `𝄞` two.
    assert_eq!(edit["range"]["start"], serde_json::json!({ "line": 0, "character": 3 }));
    assert_eq!(edit["range"]["end"], serde_json::json!({ "line": 0, "character": 12 }));
    assert_eq!(out[0]["diagnostics"][0]["range"]["start"]["character"], 3);
    assert_eq!(action["isPreferred"], true);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two safe fixes of one rule in a file also come as one action that applies
/// both, for the rule and for the whole file.
#[test]
fn lsp_offers_one_action_for_all_fixes_of_a_rule() {
    let dir = project("lsp-all", &[("doc.tex", "\\label{x}\n\\label{y}\n")]);
    let path = dir.join("doc.tex").display().to_string();
    let record = |line: u32| -> Record {
        serde_json::from_value(serde_json::json!({
            "code": "unused-label", "severity": "info", "message": "m", "fix": "delete", "path": path,
            "file": "doc.tex", "line": line, "col": 1, "applicability": "safe",
            "edits": [{ "path": path, "start": { "line": line, "col": 1 }, "end": { "line": line, "col": 10 }, "replacement": "" }],
        }))
        .unwrap()
    };
    let out: serde_json::Value = serde_json::from_str(&satex::cli::render::lsp(&[record(1), record(2)])).unwrap();
    let actions = out[0]["codeActions"].as_array().unwrap();
    let all = actions.iter().find(|a| a["data"]["scope"] == "rule").expect("a fix-all action");
    let edits = all["edit"]["changes"].as_object().unwrap().values().next().unwrap().as_array().unwrap();
    let file = actions.iter().filter(|a| a["data"]["scope"] == "all").count();
    assert_eq!((actions.len(), edits.len(), file), (4, 2, 1));
    let _ = std::fs::remove_dir_all(&dir);
}

/// `satex::linter::lint`'s findings of one `code`, against a fresh project
/// holding `doc.tex`.  The caller removes the returned directory.
pub(crate) fn lint_codes(name: &str, source: &str, code: &str) -> (Vec<Record>, PathBuf) {
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let dir = project(name, &[("doc.tex", source)]);
    let analysis = Machine::analyze(source, Some(&dir.join("doc.tex")), &cfg);
    let records = satex::linter::lint(&analysis).into_iter().filter(|r| r["code"] == code).collect();
    (records, dir)
}

fix_cases! {
    stray_space_fires_and_is_fixed_by_appending_percent("stray-space", "\\documentclass{article}\n\\def\\x{a\n\\textbf{b}}\n\\begin{document}\n\\x\n\\end{document}\n", "stray-space", false, "\\documentclass{article}\n\\def\\x{a%\n\\textbf{b}}\n\\begin{document}\n\\x\n\\end{document}\n");
    /// A body whose `}` starts the next line gets that `}` moved up, and the
    /// blanks the line ends with go with the line end.
    stray_space_before_a_closing_brace_moves_the_brace_up("stray-brace", "\\documentclass{article}\n\\def\\x{\\textbf{b}  \n  }\n\\begin{document}\n\\x z\n\\end{document}\n", "stray-space", false, "\\documentclass{article}\n\\def\\x{\\textbf{b}}\n\\begin{document}\n\\x z\n\\end{document}\n");
}

#[test]
fn stray_space_is_not_reported_when_every_line_ends_with_percent() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\newcommand{\\Weird}[3]{%\n  \\texttt{#1: #2}%\n  \\typeout{#3}%\n}\n\\begin{document}\n\\Weird{key}{value}{magic}\n\\end{document}\n";
    let (records, dir) = lint_codes("weird-ok", source, "stray-space");
    assert!(records.is_empty(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stray_space_fires_when_a_line_end_is_missing_its_percent() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\newcommand{\\Weird}[3]{%\n  \\texttt{#1: #2}\n  \\typeout{#3}%\n}\n\\begin{document}\n\\Weird{key}{value}{magic} more\n\\end{document}\n";
    let (records, dir) = lint_codes("weird-bad", source, "stray-space");
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["line"], 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stray_space_follows_the_mode_the_space_is_read_in() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\def\\x{a\n\\textbf{b}}\n\\begin{document}\n\\x\n\\end{document}\n";
    let (records, dir) = lint_codes("stray-h", source, "stray-space");
    assert!(records.len() == 1 && records[0]["message"].as_str().unwrap().contains("inserts a space"), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let source = "\\documentclass{article}\n\\def\\x{a\nb}\n\\begin{document}\n$\\x$\n\\end{document}\n";
    let (records, dir) = lint_codes("stray-m", source, "stray-space");
    assert!(records.is_empty(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A line end before `\end{center}` sets nothing: `\endtrivlist` takes the
/// space away with `\unskip` (pdflatex's box is the same with a `%` there).
/// The same line end before more text of the paragraph is a real space.
#[test]
fn stray_space_needs_material_after_it() {
    if !installed() {
        return;
    }
    let env = "\\newenvironment{nb}{%\n\\begin{center}\\fbox{x}}{%\n\\fbox{y}\n\\end{center}\n}\n";
    let doc = |body: &str| format!("\\documentclass{{article}}\n{env}\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let (records, dir) = lint_codes("stray-center", &doc("Text \\begin{nb}\\end{nb} after."), "stray-space");
    assert!(records.is_empty(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let inline = env.replace("\\begin{center}", "").replace("\\end{center}\n", "");
    let source = format!(
        "\\documentclass{{article}}\n{inline}\\begin{{document}}\nText \\begin{{nb}}\\end{{nb}}after.\n\\end{{document}}\n"
    );
    let (records, dir) = lint_codes("stray-inline", &source, "stray-space");
    let lines: Vec<_> = records.iter().map(|r| r["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, [4], "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Primitives are plain TeX's own interface: a document run with plain
/// TeX's format (`% !TeX program = tex`) gets no primitive-tex-command,
/// a LaTeX document still does.
#[test]
fn primitive_tex_command_is_latex_advice() {
    if !installed() {
        return;
    }
    let plain = "% !TeX program = tex\n\\def\\x{a\\hskip 1pt b}\\x\n\\end\n";
    crate::support::assert_lint(plain, "primitive-tex-command", 0);
    let latex = "\\documentclass{article}\n\\begin{document}\na\\hskip 1pt b\n\\end{document}\n";
    let found = satex::linter::lint(&crate::support::analyze_latex(latex));
    let hskip = found.iter().filter(|r| r["code"] == "primitive-tex-command" && r["line"] == 3);
    assert_eq!(hskip.count(), 1, "{found:?}");
}

#[test]
fn disable_next_line_suppresses_only_the_line_after_it() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n% satex-disable-next-line unused-label\n\\label{a}\n\\label{b}\nx\n\\end{document}\n";
    let (records, dir) = lint_codes("disable-next", source, "unused-label");
    let lines: Vec<_> = records.iter().map(|r| r["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, vec![5], "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn disable_line_suppresses_only_its_own_trailing_line() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\label{a} % satex-disable-line unused-label\n\\label{b}\nx\n\\end{document}\n";
    let (records, dir) = lint_codes("disable-line", source, "unused-label");
    let lines: Vec<_> = records.iter().map(|r| r["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, vec![4], "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn disable_enable_region_bounds_what_it_suppresses() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n% satex-disable unused-label\n\\label{a}\n% satex-enable unused-label\n\\label{b}\nx\n\\end{document}\n";
    let (records, dir) = lint_codes("disable-region", source, "unused-label");
    let lines: Vec<_> = records.iter().map(|r| r["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, vec![6], "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn disable_file_suppresses_everywhere_in_the_file() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n% satex-disable-file unused-label\n\\begin{document}\n\\label{a}\n\\label{b}\nx\n\\end{document}\n";
    let (records, dir) = lint_codes("disable-file", source, "unused-label");
    assert!(records.is_empty(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_suppress_code_is_a_warning() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n% satex-disable-next-line not-a-real-code\n\\label{a}\nx\n\\end{document}\n";
    let (records, dir) = lint_codes("disable-typo", source, "unknown-suppress-code");
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["name"], "not-a-real-code");
    assert_eq!(records[0]["severity"], "warning");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A separator with no digit after it ends the number: `$N_1,N_2$` lists
/// two names, it sets no `1,` newton.
#[test]
fn a_list_in_math_is_not_a_hand_set_quantity() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n$(N_1,N_2)$ and $(N_4,N_3)$\n\\end{document}\n";
    let (records, dir) = lint_codes("quantity-list", source, "hand-set-quantity");
    let _ = std::fs::remove_dir_all(dir);
    assert!(records.is_empty(), "{records:?}");
}

/// A quantity only some paths set may be one no real run sets, so it is
/// left alone; the same text outside the conditional is reported.
#[test]
fn a_quantity_set_on_only_some_paths_is_not_reported() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\ifnum\\pdfelapsedtime>5 a 3cm b\\fi\n\n c 2ms d\n\\end{document}\n";
    let (records, dir) = lint_codes("quantity-paths", source, "hand-set-quantity");
    let _ = std::fs::remove_dir_all(dir);
    assert_eq!(records.len(), 1, "{records:?}");
}

fix_cases! {
    hand_set_quantity_becomes_a_siunitx_call("hand-set-quantity", "\\documentclass{article}\n\\begin{document}\nA delay of 2ms and growth of 5\\%.\n\\end{document}\n", "hand-set-quantity", true, "\\documentclass{article}\n\\usepackage{siunitx}\n\\begin{document}\nA delay of \\qty{2}{\\milli\\second} and growth of \\qty{5}{\\percent}.\n\\end{document}\n");
    hand_set_quantity_rewrites_a_macro_that_holds_the_unit("hand-set-quantity-macro", "\\documentclass{article}\n\\newcommand{\\ms}{\\,\\mathrm{ms}}\n\\begin{document}\nA delay of \\(3\\ms\\).\n\\end{document}\n", "hand-set-quantity", true, "\\documentclass{article}\n\\usepackage{siunitx}\n\\newcommand{\\ms}{\\,\\mathrm{ms}}\n\\begin{document}\nA delay of \\(\\qty{3}{\\milli\\second}\\).\n\\end{document}\n");
}

/// Documents exercising the typography lints that could plausibly disagree
/// with one another: quotes, dashes, ellipses, spacing, math shifts,
/// italics and units. `satex lint --fix` applies every rule's fixes
/// together and re-lints until nothing fixable is left, so a contradiction
/// between two rules shows up here as either a fixpoint that never settles
/// (`apply::fixpoint` hits `MAX_ROUNDS`) or a finding still left with an
/// edit once it does.
const TYPOGRAPHY_MWES: &[(&str, &str)] = &[
    (
        "quotes",
        "This is a \"straight quote\" test and a \u{b4}wrong\u{b4} apostrophe.\n\
         He said 'no way to open this' wrongly and ``this closes' wrongly too.\n\
         He said \"it's over,\" she replied.",
    ),
    (
        "dashes-units",
        "Section 12-13 has a range, and pages 5-9 too.\n\
         A distance of 10-20 km and a temperature of -5-5 degrees.\n\
         Sizes are 640x480 and 1920x1080.\n\
         A quantity of 5 km and 3.5 kg.",
    ),
    (
        "ellipsis-math",
        "$$a_1 + a_2 + ... + a_n = b$$\n\
         Also $x = 1 + 2 + ...$ and $y = ...$.",
    ),
    (
        "spacing",
        "He works at the ABC. Next sentence starts here.\n\
         He arrived etc. and stayed the night.\n\
         See Figure 3 for details, and \\cite{foo} too.\n\
         Long words like \\LaTeX and \\TeX are fine.",
    ),
    ("italics", "The word na\\\"ive uses italics like \\textit{slanted} next to upright."),
];

#[test]
fn typography_fixes_introduce_nothing_new() {
    if !installed() {
        return;
    }
    for (name, body) in TYPOGRAPHY_MWES {
        let source =
            format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\cite{{foo}}\n\\end{{document}}\n");
        // `\cite{foo}` with no bibliography is its own, unrelated finding;
        // what matters here is that fixing converges and leaves nothing
        // with an edit still attached.
        let files: &[(&str, &str)] = &[("doc.tex", &source)];
        let dir = project(name, files);
        let path = dir.join("doc.tex");
        let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
        let analysis = Machine::analyze(&source, Some(&path), &cfg);
        let select = |record: &Record| record["origin"] == "document";
        let records: Vec<Record> = satex::linter::lint(&analysis).into_iter().filter(|r| select(r)).collect();
        let main = analysis.file_name(analysis.main_file).to_string();
        let outcome = apply::fixpoint(&main, &cfg, records, &select, true);
        assert!(
            outcome.rounds < apply::MAX_ROUNDS,
            "{name}: fixing never settled in {} rounds, so two rules cycle",
            apply::MAX_ROUNDS
        );
        let stuck: Vec<&Record> = outcome.remaining.iter().filter(|r| r.get("edits").is_some()).collect();
        assert!(stuck.is_empty(), "{name}: a fix left findings still offering an edit: {stuck:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A kernel command a package redefines (amsmath's `\[`) is a use of it
/// where the document writes it.
#[test]
fn a_redefined_kernel_command_in_use_keeps_its_package() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\[ x \\]\n\\end{document}\n";
    assert_eq!(reported("unused-package-redefined", source, "unused-package"), 0);
}

/// A plain TeX document without `\bye` is told so and gets it appended; a
/// chapter, no document of its own, is not.
#[test]
fn missing_bye_is_appended() {
    if !installed() {
        return;
    }
    let (records, dir) = lint_codes("missing-bye", "% !TeX program = tex\nHello.\n", "missing-end");
    let replacement = records.first().and_then(|r| r["edits"][0]["replacement"].as_str().map(str::to_string));
    assert_eq!(replacement.as_deref(), Some("\\bye\n"), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let (records, dir) = lint_codes("missing-bye-chapter", "\\section{Two}\nText.\n", "missing-end");
    assert!(records.is_empty(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// ConTeXt's kernel is not read: how its documents end and which category
/// codes they start with are not known, so neither is reported.
#[test]
fn a_context_document_is_not_held_to_latex_conventions() {
    if !installed() {
        return;
    }
    let source = "% !TeX program = context\n\\starttext\nHello\tthere.\n\\stoptext\n";
    for code in ["missing-end", "catcode-escapes-group"] {
        let (records, dir) = lint_codes(&format!("context-{code}"), source, code);
        assert!(records.is_empty(), "{code}: {records:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A name a package defined is named with the package and its line, and the
/// fix warns that overriding it changes the package's own uses: amsmath's
/// `\text` is what `\newcommand{\text}` most often runs into.
#[test]
fn a_package_s_command_is_named_with_its_package() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\usepackage{amsmath}\n\\newcommand{\\text}[1]{#1}\n\\begin{document}\nx\n\\end{document}\n";
    let dir = project("already-defined-package", &[("doc.tex", source)]);
    let path = dir.join("doc.tex");
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(source, Some(&path), &cfg);
    let record =
        satex::linter::lint(&analysis).into_iter().find(|r| r["code"] == "already-defined").expect("a finding");
    let message = record["message"].as_str().unwrap_or_default();
    assert!(message.starts_with("\\text is already defined by amstext (amstext.sty:"), "{message}");
    assert!(record["fix"].as_str().unwrap_or_default().contains("pick another name"), "{record:?}");
}

/// An article whose body is `body`.
fn article(body: &str) -> String {
    format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\\end{{document}}\n")
}

// TeX's own errors, each with the edit that repairs it (`raised-error`): the
// fixed text runs without the error and pdflatex accepts it.
fix_cases! {
    /// An argument that never closes is closed where its paragraph ends,
    /// before the `\end` that ends it here.
    runaway_argument_closes_at_the_paragraph_end("err-runaway", article("\\textbf{Note: the cache is warm.\n"), "raised-error", true, &article("\\textbf{Note: the cache is warm.}\n"));
    /// The argument is closed at its line's end first; the paragraph's end is the alternative.
    paragraph_ended_closes_at_the_line_end("err-par", article("\\textbf{Note: the cache\nis warm.\n\nNext.\n"), "raised-error", true, &article("\\textbf{Note: the cache}\nis warm.\n\nNext.\n"));
    /// Every group still open there closes, and a `}` in a comment is none.
    runaway_closes_every_open_group("err-nested", article("\\textbf{a \\emph{b % c}\n"), "raised-error", true, &article("\\textbf{a \\emph{b}} % c}\n"));
    /// TeX ignores a `}` that closes no group: deleting it is safe.
    extra_brace_is_deleted("err-brace", article("Some text} more.\n"), "raised-error", false, &article("Some text more.\n"));
    /// A script in text sets its nucleus and argument as a formula, one at a time.
    math_only_tokens_become_formulas("err-dollar", article("Let x^{10}_i be and \\alpha too.\n"), "raised-error", true, &article("Let $x^{10}_i$ be and $\\alpha$ too.\n"));
    /// The file's own spelling of formulas wins.
    math_follows_the_files_delimiters("err-paren", article("\\(a\\) and \\(b\\), then x_1.\n"), "raised-error", true, &article("\\(a\\) and \\(b\\), then \\(x_1\\).\n"));
    /// An `&` past the last column adds a column of the last one's kind.
    extra_tab_adds_a_column("err-tab", article("\\begin{tabular}{|l|c|}\na & b & c \\\\\n\\end{tabular}\n"), "raised-error", true, &article("\\begin{tabular}{|l|cc|}\na & b & c \\\\\n\\end{tabular}\n"));
    /// A row without its `\\` before a `\hline` puts the rule's `\noalign`
    /// in a cell (tex.web § 1129): the row ends before it.
    misplaced_noalign_ends_the_row("err-noalign", article("\\begin{tabular}{ll}\na & b \\\\\nc & d\n\\hline\n\\end{tabular}\n"), "raised-error", true, &article("\\begin{tabular}{ll}\na & b \\\\\nc & d\n\\\\ \\hline\n\\end{tabular}\n"));
    /// An `&` in running text is the character.
    misplaced_tab_is_escaped("err-amp", article("Text & here.\n"), "raised-error", true, &article("Text \\& here.\n"));
    /// Text after the preamble, with no `\begin{document}` anywhere, starts the document.
    missing_begin_document_is_inserted("err-begin", "\\documentclass{article}\nHello.\n\\end{document}\n", "raised-error", true, "\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n");
    /// Items outside a list go in an `itemize`, up to the `\end` after them.
    lonely_items_get_a_list("err-item", article("\\item a\n\n\\item b\n"), "raised-error", true, &article("\\begin{itemize}\n\\item a\n\n\\item b\n\\end{itemize}\n"));
    /// `\\` with no line to end: TeX drops it, and so does the fix.
    no_line_to_end_is_deleted("err-newline", article("\\\\ Start.\n"), "raised-error", false, &article("Start.\n"));
    /// The space it asks for is `\vspace`'s.
    no_line_to_end_with_space_becomes_vspace("err-newline-space", article("\\\\[1em] Start.\n"), "raised-error", true, &article("\\vspace{1em} Start.\n"));
    /// A length without a unit gets the `pt` TeX assumes.
    illegal_unit_gets_pt("err-unit", article("A\\vspace{3}\n\nB\n"), "raised-error", false, &article("A\\vspace{3pt}\n\nB\n"));
    /// `\csname` ends where TeX ends it, before the first command that does not expand.
    missing_endcsname_is_inserted("err-csname", article("\\csname foo\\relax\n"), "raised-error", false, &article("\\csname foo\\endcsname\\relax\n"));
    /// An undefined command one edit from a known one is respelled.
    undefined_command_is_respelled("err-undefined", article("\\textbff{x}\n"), "undefined-control-sequence", true, &article("\\textbf{x}\n"));
}

/// An argument that may end at its line or its paragraph gets both fixes:
/// the line's first, which `--fix` applies, the paragraph's as an
/// alternative, its own code action that no fix-all takes.
#[test]
fn runaway_offers_the_line_end_as_an_alternative() {
    if !installed() {
        return;
    }
    let (records, dir) = lint_codes("err-alt", &article("\\textbf{a\nb.\n\nc\n"), "raised-error");
    let record = records.first().expect("a raised error");
    assert!(record["fix"].as_str().unwrap().ends_with("line"), "{record:?}");
    let alternatives = record["alternatives"].as_array().expect("an alternative");
    assert_eq!(alternatives[0]["edits"][0]["start"], serde_json::json!({ "line": 4, "col": 3 }));
    let out: serde_json::Value = serde_json::from_str(&satex::cli::render::lsp(&records)).unwrap();
    let actions = out[0]["codeActions"].as_array().unwrap();
    assert_eq!(actions.iter().filter(|a| a["data"]["alternative"] == true).count(), 1);
    // a fix-all takes the first fix only
    let changes = |a: &serde_json::Value| {
        a["edit"]["changes"].as_object().unwrap().values().next().unwrap().as_array().unwrap().len()
    };
    assert!(actions.iter().filter(|a| !a["data"]["scope"].is_null()).all(|a| changes(a) == 1));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `}` that closes an argument is no runaway, and a verbatim `}` is not stray.
#[test]
fn balanced_and_verbatim_braces_get_no_brace_fix() {
    if !installed() {
        return;
    }
    let (records, dir) = lint_codes("err-verb", &article("\\textbf{a} \\verb|}| and \\texttt{\\}}\n"), "raised-error");
    assert!(records.is_empty(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

fix_cases! {
    /// A second script of one kind on a nucleus is TeX's `Double superscript`:
    /// the first scripted nucleus is braced, one reading of the formula.
    double_superscript_braces_the_first("err-double-sup", article("$x^{a}^3$ and $\\alpha_1 _2$\n"), "raised-error", true, &article("${x^{a}}^3$ and ${\\alpha_1} _2$\n"));
    /// The last column is copied with its argument.
    extra_tab_copies_a_column_with_its_width("err-tab-p", article("\\begin{tabular}{lp{2cm}}\na & b & c \\\\\n\\end{tabular}\n"), "raised-error", true, &article("\\begin{tabular}{lp{2cm}p{2cm}}\na & b & c \\\\\n\\end{tabular}\n"));
    /// `\right` inside an open `{`: TeX closes the group right before it.
    missing_brace_before_right_closes_the_group("err-right", article("$\\left( {a \\right)$\n"), "raised-error", false, &article("$\\left( {a }\\right)$\n"));
    /// `\end` inside an open `{`: closed before the `\end`, which runs the
    /// environment's end code outside the group, so a review.
    missing_brace_before_end_closes_the_group("err-end-group", article("\\begin{center}\n{\\bfseries x\n\\end{center}\n"), "raised-error", true, &article("\\begin{center}\n{\\bfseries x\n}\\end{center}\n"));
}

/// Scripts of both kinds, a script after `{}`, or after a space-separated
/// atom are no double script.
#[test]
fn single_scripts_per_nucleus_are_no_error() {
    if !installed() {
        return;
    }
    let (records, dir) =
        lint_codes("err-double-none", &article("$x^2_3$ $x_3^2$ $x^2{}^3$ $x^2 y^3$ ${x^2}^3$\n"), "raised-error");
    assert!(records.is_empty(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The paragraph-end fix of a runaway stops before a command that ends a
/// paragraph through its own code (`\section`, whose `\@startsection` runs
/// `\par`): the bold text must not take the next heading. Bug pinned: the
/// scan saw only `\par`, unmatched `\end` and `}`. Now the run's
/// `paragraph_enders` fact (the first `\section` ran `\par`) ends it.
#[test]
fn runaway_paragraph_fix_stops_before_a_heading() {
    if !installed() {
        return;
    }
    let (records, dir) =
        lint_codes("err-heading", &article("\\section{A}\n\\textbf{a\nb\n\\section{B}\nc\n"), "raised-error");
    let record = records.first().expect("a raised error");
    let paragraph = &record["alternatives"][0]["edits"][0]["start"];
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(paragraph, &serde_json::json!({ "line": 5, "col": 2 }), "{record:?}");
}

fix_cases! {
    /// `\end{document}` while an `itemize` is open: the list lacks its end, so the fix
    /// closes it before the line. Pinned: the fix renamed `\end{document}` to
    /// `\end{itemize}`, and the document lost its end.
    an_open_list_at_the_document_s_end_is_closed_before_it("env-close", "\\documentclass{article}\n\\begin{document}\nText.\n\\begin{itemize}\n\\item one\n\\end{document}\n", "environment-mismatch", true, "\\documentclass{article}\n\\begin{document}\nText.\n\\begin{itemize}\n\\item one\n\\end{itemize}\n\\end{document}\n");
    /// A misspelled environment gets the defined one nearest to its name
    /// (an environment is a pair `\X`/`\endX`). Pinned: no suggestion, and the
    /// lonely `\item` in it was wrapped into a new `itemize`.
    a_misspelled_environment_is_renamed("env-typo", "\\documentclass{article}\n\\begin{document}\n\\begin{itemze}\n\\item a\n\\end{itemize}\n\\end{document}\n", "undefined-environment", true, "\\documentclass{article}\n\\begin{document}\n\\begin{itemize}\n\\item a\n\\end{itemize}\n\\end{document}\n");
}

/// Every unsafe fix at once: deleting the unused `\label{s:a}` and turning
/// `\ref{s:b}` into `\ref{s:a}` each hold alone, not together. Pinned: one
/// round applied both and left a dangling reference; a round whose result
/// has a finding none had before is undone and its gravest fix goes in alone.
#[test]
fn fixes_of_one_round_that_clash_go_in_one_at_a_time() {
    if !installed() {
        return;
    }
    let dir = project(
        "clash",
        &[(
            "doc.tex",
            "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{s:a} text \\ref{s:b}\n\\end{document}\n",
        )],
    );
    let path = dir.join("doc.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let keep = |r: &Record| r["origin"] == "document";
    let records: Vec<Record> = satex::linter::lint(&analysis).into_iter().filter(|r| keep(r)).collect();
    let main = analysis.file_name(analysis.main_file).to_string();
    let outcome = apply::fixpoint(&main, &cfg, records, &keep, true);
    apply::write(&outcome).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\\label{s:a}") && text.contains("\\ref{s:a}"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The slow-preamble fix writes a `%&` format line only once the format is
/// dumped: before, the line names a format TeX cannot find. Pinned: the
/// line went in on every `--fix --unsafe-fixes`. The message counts the
/// `\usepackage` lines only, never the class.
#[test]
fn a_format_line_waits_for_its_format() {
    if !installed() {
        return;
    }
    let dir = project(
        "fmt",
        &[(
            "doc.tex",
            "\\documentclass{article}\n\\usepackage{hyperref}\n\\usepackage{cleveref}\n\\begin{document}\nx\n\\end{document}\n",
        )],
    );
    let path = dir.join("doc.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let cost: Vec<Record> =
        satex::linter::lint(&analysis).into_iter().filter(|r| r["code"] == "preamble-cost").collect();
    assert!(!cost.is_empty(), "no preamble-cost finding");
    // The class is no `\usepackage`: two packages, not three. Pinned: the count took the class in.
    assert!(cost.iter().all(|r| r["message"].as_str().is_some_and(|m| m.starts_with("2 packages "))), "{cost:?}");
    assert!(cost.iter().all(|r| r.get("edits").is_none_or(|e| e.as_array().is_none_or(Vec::is_empty))), "{cost:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
