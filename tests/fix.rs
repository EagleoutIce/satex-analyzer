//! Quick-fixes: every rule with a mechanical fix, applied to a document on
//! disk.  The fixed text is what the fix promises, linting it again no
//! longer reports the finding, and the result still compiles.

use std::path::{Path, PathBuf};
use std::process::Command;

use satex::config::Config;
use satex::lint::apply;
use satex::machine::Machine;
use satex::query::Record;

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

struct Fixed {
    dir: PathBuf,
    text: String,
    remaining: Vec<Record>,
}

impl Drop for Fixed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-fix-{name}-{}", std::process::id()));
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
fn fix_files(name: &str, files: &[(&str, &str)], code: &str, unsafe_fixes: bool, all: bool) -> Fixed {
    let dir = project(name, files);
    let path = dir.join("doc.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let cfg = Config { load_classes: true, ..Config::default() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let keep = select(code, all);
    let records: Vec<Record> = satex::lint::lint(&analysis).into_iter().filter(|r| keep(r)).collect();
    assert!(!records.is_empty(), "{code} reports nothing");
    assert!(records.iter().any(|r| r.get("edits").is_some()), "{code} has no edits: {records:?}");
    let main = analysis.file_name(analysis.main_file).to_string();
    let outcome = apply::fixpoint(&main, &cfg, records, &keep, unsafe_fixes);
    apply::write(&outcome).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    Fixed { dir, text, remaining: outcome.remaining }
}

fn fix(name: &str, source: &str, code: &str, unsafe_fixes: bool) -> Fixed {
    fix_files(name, &[("doc.tex", source)], code, unsafe_fixes, false)
}

/// pdflatex accepts the fixed document, when it is installed.
fn compiles(dir: &Path) {
    if which::which("pdflatex").is_err() {
        return;
    }
    let status = Command::new("pdflatex")
        .args(["-interaction=nonstopmode", "-halt-on-error", "doc.tex"])
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("pdflatex runs");
    let log = std::fs::read_to_string(dir.join("doc.log")).unwrap_or_default();
    assert!(status.success(), "pdflatex failed:\n{}", log.lines().filter(|l| l.starts_with('!')).collect::<Vec<_>>().join("\n"));
}

fn check(fixed: &Fixed, expected: &str) {
    assert_eq!(fixed.text, expected);
    assert!(fixed.remaining.is_empty(), "still reported: {:?}", fixed.remaining);
    compiles(&fixed.dir);
}

const BODY: &str = "\\begin{document}\nx\n\\end{document}\n";

#[test]
fn duplicate_package_drops_the_name_from_a_list() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\\usepackage{{amsmath,graphicx,amssymb}}\n{BODY}");
    let fixed = fix("duplicate-list", &source, "duplicate-package", false);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\\usepackage{{amsmath,amssymb}}\n{BODY}"));
}

#[test]
fn duplicate_package_deletes_a_line_of_its_own() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n  \\usepackage{{graphicx}} % again\n{BODY}");
    let fixed = fix("duplicate-line", &source, "duplicate-package", false);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n{BODY}"));
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

#[test]
fn option_clash_moves_the_options_to_the_first_load() {
    if !installed() {
        return;
    }
    // The second load is then a plain duplicate, which the next round deletes.
    let source = format!("\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\\usepackage[draft]{{graphicx}}\n{BODY}");
    let dir = project("option-clash", &[("doc.tex", &source)]);
    let path = dir.join("doc.tex");
    let cfg = Config { load_classes: true, ..Config::default() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let keep = |r: &Record| {
        matches!(r["code"].as_str(), Some("option-clash" | "duplicate-package")) && r["origin"] == "document"
    };
    let records: Vec<Record> = satex::lint::lint(&analysis).into_iter().filter(|r| keep(r)).collect();
    let outcome = apply::fixpoint(analysis.file_name(analysis.main_file), &cfg, records, &keep, true);
    apply::write(&outcome).unwrap();
    let fixed = Fixed { text: std::fs::read_to_string(&path).unwrap(), dir, remaining: outcome.remaining };
    assert_eq!(outcome.rounds, 2);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage[draft]{{graphicx}}\n{BODY}"));
}

#[test]
fn package_after_preamble_moves_into_the_preamble() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\usepackage[hyphens]{url}\nx\n\\end{document}\n";
    let fixed = fix("after-preamble", source, "package-after-preamble", true);
    check(&fixed, "\\documentclass{article}\n\\usepackage[hyphens]{url}\n\\begin{document}\nx\n\\end{document}\n");
}

#[test]
fn already_defined_becomes_renewcommand() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\newcommand{\\foo}{a}\n\\newcommand{\\foo}{b}\n\\begin{document}\n\\foo\n\\end{document}\n".to_string();
    let fixed = fix("already-defined", &source, "already-defined", true);
    check(&fixed, "\\documentclass{article}\n\\newcommand{\\foo}{a}\n\\renewcommand{\\foo}{b}\n\\begin{document}\n\\foo\n\\end{document}\n");
}

#[test]
fn not_defined_becomes_newcommand() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\renewcommand*{\\foo}{a}\n\\begin{document}\n\\foo\n\\end{document}\n";
    let fixed = fix("not-defined", source, "not-defined", false);
    check(&fixed, "\\documentclass{article}\n\\newcommand*{\\foo}{a}\n\\begin{document}\n\\foo\n\\end{document}\n");
}

#[test]
fn environment_mismatch_names_the_open_environment() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\begin{center}\nx\n\\end{flushleft}\n\\end{document}\n";
    let fixed = fix("env-mismatch", source, "environment-mismatch", true);
    check(&fixed, "\\documentclass{article}\n\\begin{document}\n\\begin{center}\nx\n\\end{center}\n\\end{document}\n");
}

#[test]
fn a_solo_end_is_deleted() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n\\end{{center}}\n{BODY}");
    let fixed = fix("env-solo", &source, "environment-mismatch", true);
    check(&fixed, &format!("\\documentclass{{article}}\n{BODY}"));
}

#[test]
fn unused_label_is_deleted_inline_and_as_a_line() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\\section{B}\n  \\label{sec:b}\nx\n\\end{document}\n";
    let fixed = fix("unused-label", source, "unused-label", true);
    check(&fixed, "\\documentclass{article}\n\\begin{document}\n\\section{A}\n\\section{B}\nx\n\\end{document}\n");
}

#[test]
fn dead_definition_before_a_def_is_a_safe_deletion() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\def\\x#1{[#1]}\n\\def\\x#1{(#1)}\n\\begin{document}\n\\x{a}\n\\end{document}\n";
    let fixed = fix("dead-definition", source, "dead-definition", false);
    check(&fixed, "\\documentclass{article}\n\\def\\x#1{(#1)}\n\\begin{document}\n\\x{a}\n\\end{document}\n");
}

#[test]
fn unused_definition_deletes_the_whole_newcommand() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\newcommand*{\\unused}[2][d]{%\n  #1 and #2%\n}\n\\begin{document}\nx\n\\end{document}\n";
    let fixed = fix("unused-definition", source, "unused-definition", true);
    check(&fixed, &format!("\\documentclass{{article}}\n{BODY}"));
}

#[test]
fn csname_becomes_the_etoolbox_command_for_its_role() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\usepackage{etoolbox}\n\\expandafter\\def\\csname my@x\\endcsname{X}\n\\expandafter\\let\\csname my@y\\endcsname\\relax\n\\begin{document}\n\\csname my@x\\endcsname \\csname my@x\\endcsname\n\\end{document}\n";
    let fixed = fix("csname", source, "primitive-tex-command", true);
    check(
        &fixed,
        "\\documentclass{article}\n\\usepackage{etoolbox}\n\\csdef{my@x}{X}\n\\cslet{my@y}\\relax\n\\begin{document}\n\\csuse{my@x}\\csuse{my@x}%\n\\end{document}\n",
    );
}

#[test]
fn microtype_is_loaded_before_the_document() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n{BODY}");
    let fixed = fix("microtype", &source, "microtype-available", true);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage{{microtype}}\n{BODY}"));
}

#[test]
fn ot1_accents_load_t1_and_latin_modern() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\\"a\n\\end{document}\n";
    // Whether the run sees the `\\accent` is the rule's business, not the fix's.
    let analysis = Machine::analyze(source, None, &Config { load_classes: true, ..Config::default() });
    if !satex::lint::lint(&analysis).iter().any(|r| r["code"] == "ot1-font-encoding") {
        return;
    }
    let fixed = fix("ot1", source, "ot1-font-encoding", true);
    check(
        &fixed,
        "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\usepackage{lmodern}\n\\begin{document}\n\\\"a\n\\end{document}\n",
    );
}

#[test]
fn computer_modern_in_t1_loads_latin_modern() {
    if !installed() {
        return;
    }
    let source = format!("\\documentclass{{article}}\n\\usepackage[T1]{{fontenc}}\n{BODY}");
    let fixed = fix("cm-t1", &source, "computer-modern-in-t1", true);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage[T1]{{fontenc}}\n\\usepackage{{lmodern}}\n{BODY}"));
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
    let cfg = Config { load_classes: true, ..Config::default() };
    let dir = project("library", &[("doc.tex", "")]);
    let source = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\nx\n\\end{document}\n";
    let analysis = Machine::analyze(source, Some(&dir.join("doc.tex")), &cfg);
    let project = dir.canonicalize().unwrap();
    for record in satex::lint::lint(&analysis) {
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
    let out: serde_json::Value = serde_json::from_str(&satex::render::lsp(&[record])).unwrap();
    let action = &out[0]["codeActions"][0];
    let edit = action["edit"]["changes"].as_object().unwrap().values().next().unwrap()[0].clone();
    // `é` is one UTF-16 unit, `𝄞` two.
    assert_eq!(edit["range"]["start"], serde_json::json!({ "line": 0, "character": 3 }));
    assert_eq!(edit["range"]["end"], serde_json::json!({ "line": 0, "character": 12 }));
    assert_eq!(out[0]["diagnostics"][0]["range"]["start"]["character"], 3);
    assert_eq!(action["isPreferred"], true);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `satex::lint::lint`'s findings of one `code`, against a fresh project
/// holding `doc.tex`.  The caller removes the returned directory.
fn lint_codes(name: &str, source: &str, code: &str) -> (Vec<Record>, PathBuf) {
    let cfg = Config { load_classes: true, ..Config::default() };
    let dir = project(name, &[("doc.tex", source)]);
    let analysis = Machine::analyze(source, Some(&dir.join("doc.tex")), &cfg);
    let records = satex::lint::lint(&analysis).into_iter().filter(|r| r["code"] == code).collect();
    (records, dir)
}

#[test]
fn stray_space_fires_and_is_fixed_by_appending_percent() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\def\\x{a\nb}\n\\begin{document}\n\\x\n\\end{document}\n";
    let fixed = fix("stray-space", source, "stray-space", false);
    check(&fixed, "\\documentclass{article}\n\\def\\x{a%\nb}\n\\begin{document}\n\\x\n\\end{document}\n");
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
    let source = "\\documentclass{article}\n\\newcommand{\\Weird}[3]{%\n  \\texttt{#1: #2}\n  \\typeout{#3}%\n}\n\\begin{document}\n\\Weird{key}{value}{magic}\n\\end{document}\n";
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
    let source = "\\documentclass{article}\n\\def\\x{a\nb}\n\\begin{document}\n\\x\n\\end{document}\n";
    let (records, dir) = lint_codes("stray-h", source, "stray-space");
    assert!(records.len() == 1 && records[0]["message"].as_str().unwrap().contains("inserts a space"), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
    let source = "\\documentclass{article}\n\\def\\x{a\nb}\n\\begin{document}\n$\\x$\n\\end{document}\n";
    let (records, dir) = lint_codes("stray-m", source, "stray-space");
    assert!(records.is_empty(), "{records:?}");
    let _ = std::fs::remove_dir_all(&dir);
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
