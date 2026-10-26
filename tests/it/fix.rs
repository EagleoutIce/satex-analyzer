//! Quick-fixes: every rule with a mechanical fix, applied to a document on
//! disk.  The fixed text is what the fix promises, linting it again no
//! longer reports the finding, and the result still compiles.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::installed;
use satex::config::{Config, Load};
use satex::lint::apply;
use satex::machine::Machine;
use satex::query::Record;

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
fn fix_files(name: &str, files: &[(&str, &str)], code: &str, unsafe_fixes: bool, all: bool) -> Fixed {
    let dir = project(name, files);
    let path = dir.join("doc.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
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
    if satex::paths::which("pdflatex").is_err() {
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

fn check(fixed: &Fixed, expected: &str) {
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

const BODY: &str = "\\begin{document}\nx\n\\end{document}\n";
const PROSE: &str = "Running text that fills the line. ";

fix_cases! {
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
    satex::lint::lint(&analysis).iter().filter(|r| r.get("code").and_then(|c| c.as_str()) == Some(code)).count()
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

/// A command right after `\begin{document}` is the body's first use.
#[test]
fn a_command_right_after_begin_document_is_a_use() {
    if installed() {
        let source = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\\includegraphics{x}\n\\end{document}\n";
        assert_eq!(reported("unused-package-first", source, "unused-package"), 0);
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
    let records: Vec<Record> = satex::lint::lint(&analysis).into_iter().filter(|r| keep(r)).collect();
    let outcome = apply::fixpoint(analysis.file_name(analysis.main_file), &cfg, records, &keep, true);
    apply::write(&outcome).unwrap();
    let fixed = Fixed { text: std::fs::read_to_string(&path).unwrap(), dir, remaining: outcome.remaining };
    assert_eq!(outcome.rounds, 2);
    check(&fixed, &format!("\\documentclass{{article}}\n\\usepackage[draft]{{graphicx}}\n{BODY}"));
}

fix_cases! {
    package_after_preamble_moves_into_the_preamble("after-preamble", "\\documentclass{article}\n\\begin{document}\n\\usepackage[hyphens]{url}\nx\n\\end{document}\n", "package-after-preamble", true, "\\documentclass{article}\n\\usepackage[hyphens]{url}\n\\begin{document}\nx\n\\end{document}\n");
    already_defined_becomes_renewcommand("already-defined", "\\documentclass{article}\n\\newcommand{\\foo}{a}\n\\newcommand{\\foo}{b}\n\\begin{document}\n\\foo\n\\end{document}\n".to_string(), "already-defined", true, "\\documentclass{article}\n\\newcommand{\\foo}{a}\n\\renewcommand{\\foo}{b}\n\\begin{document}\n\\foo\n\\end{document}\n");
    not_defined_becomes_newcommand("not-defined", "\\documentclass{article}\n\\renewcommand*{\\foo}{a}\n\\begin{document}\n\\foo\n\\end{document}\n", "not-defined", false, "\\documentclass{article}\n\\newcommand*{\\foo}{a}\n\\begin{document}\n\\foo\n\\end{document}\n");
    environment_mismatch_names_the_open_environment("env-mismatch", "\\documentclass{article}\n\\begin{document}\n\\begin{center}\nx\n\\end{flushleft}\n\\end{document}\n", "environment-mismatch", true, "\\documentclass{article}\n\\begin{document}\n\\begin{center}\nx\n\\end{center}\n\\end{document}\n");
    a_solo_end_is_deleted("env-solo", format!("\\documentclass{{article}}\n\\end{{center}}\n{BODY}"), "environment-mismatch", true, &format!("\\documentclass{{article}}\n{BODY}"));
    unused_label_is_deleted_inline_and_as_a_line("unused-label", "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\\section{B}\n  \\label{sec:b}\nx\n\\end{document}\n", "unused-label", true, "\\documentclass{article}\n\\begin{document}\n\\section{A}\n\\section{B}\nx\n\\end{document}\n");
    /// amsmath writes the `\newlabel` at the end of a display, and reads an
    /// `align` body as one argument: the label is recorded at its own key, so
    /// the deletion finds the `\label` that carries it.
    unused_label_is_deleted_inside_a_display("unused-label-display", "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\begin{equation}\n  g = h\n  \\label{eq:b}\n\\end{equation}\n\\begin{align}\n  c &= d \\label{eq:a} \\\\\n  e &= f\n\\end{align}\n\\end{document}\n", "unused-label", true, "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\begin{equation}\n  g = h\n\\end{equation}\n\\begin{align}\n  c &= d  \\\\\n  e &= f\n\\end{align}\n\\end{document}\n");
    dead_definition_before_a_def_is_a_safe_deletion("dead-definition", "\\documentclass{article}\n\\def\\x#1{[#1]}\n\\def\\x#1{(#1)}\n\\begin{document}\n\\x{a}\n\\end{document}\n", "dead-definition", false, "\\documentclass{article}\n\\def\\x#1{(#1)}\n\\begin{document}\n\\x{a}\n\\end{document}\n");
    unused_definition_deletes_the_whole_newcommand("unused-definition", "\\documentclass{article}\n\\newcommand*{\\unused}[2][d]{%\n  #1 and #2%\n}\n\\begin{document}\nx\n\\end{document}\n", "unused-definition", true, &format!("\\documentclass{{article}}\n{BODY}"));
    csname_becomes_the_etoolbox_command_for_its_role("csname", "\\documentclass{article}\n\\usepackage{etoolbox}\n\\expandafter\\def\\csname my@x\\endcsname{X}\n\\expandafter\\let\\csname my@y\\endcsname\\relax\n\\begin{document}\n\\csname my@x\\endcsname \\csname my@x\\endcsname\n\\end{document}\n", "primitive-tex-command", true, "\\documentclass{article}\n\\usepackage{etoolbox}\n\\csdef{my@x}{X}\n\\cslet{my@y}\\relax\n\\begin{document}\n\\csuse{my@x}\\csuse{my@x}%\n\\end{document}\n");
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
    if !satex::lint::lint(&analysis).iter().any(|r| r["code"] == "ot1-font-encoding") {
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
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let dir = project(name, &[("doc.tex", source)]);
    let analysis = Machine::analyze(source, Some(&dir.join("doc.tex")), &cfg);
    let records = satex::lint::lint(&analysis).into_iter().filter(|r| r["code"] == code).collect();
    (records, dir)
}

fix_cases! {
    stray_space_fires_and_is_fixed_by_appending_percent("stray-space", "\\documentclass{article}\n\\def\\x{a\n\\textbf{b}}\n\\begin{document}\n\\x\n\\end{document}\n", "stray-space", false, "\\documentclass{article}\n\\def\\x{a%\n\\textbf{b}}\n\\begin{document}\n\\x\n\\end{document}\n");
    /// A body whose `}` starts the next line gets that `}` moved up, and the
    /// blanks the line ends with go with the line end.
    stray_space_before_a_closing_brace_moves_the_brace_up("stray-brace", "\\documentclass{article}\n\\def\\x{\\textbf{b}  \n  }\n\\begin{document}\n\\x\n\\end{document}\n", "stray-space", false, "\\documentclass{article}\n\\def\\x{\\textbf{b}}\n\\begin{document}\n\\x\n\\end{document}\n");
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
    let source = "\\documentclass{article}\n\\def\\x{a\n\\textbf{b}}\n\\begin{document}\n\\x\n\\end{document}\n";
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
    hand_set_quantity_rewrites_a_macro_that_holds_the_unit("hand-set-quantity-macro", "\\documentclass{article}\n\\newcommand{\\ms}{\\,\\mathrm{ms}}\n\\begin{document}\nA delay of \\(3\\ms\\).\n\\end{document}\n", "hand-set-quantity", true, "\\documentclass{article}\n\\newcommand{\\ms}{\\,\\mathrm{ms}}\n\\usepackage{siunitx}\n\\begin{document}\nA delay of \\(\\qty{3}{\\milli\\second}\\).\n\\end{document}\n");
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
        let records: Vec<Record> = satex::lint::lint(&analysis).into_iter().filter(|r| select(r)).collect();
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
