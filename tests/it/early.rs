//! Early lints: the rules that need no run report on a sketch first, and the run confirms or
//! retracts each finding (`satex::linter::early`).

use satex::analysis::machine::{Analysis, Machine};
use satex::linter::early;
use satex::project::config::{Config, Load};
use satex::queries::query::Record;

const PART: &str = "% FIXME: say more\nText of the part.\n";

/// A document whose early-tier findings are all certain: a marker in the main file and in an
/// input, a marker inside a verbatim body (no comment), a user pattern, and a latexmkrc.
fn document(body: &str) -> String {
    format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\\end{{document}}\n")
}

struct Project(std::path::PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `files` written into a fresh directory, `doc.tex` the main one.
fn project(name: &str, files: &[(&str, &str)]) -> Project {
    let dir = crate::support::tmp_root().join(format!("satex-early-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    Project(dir)
}

fn config() -> Config {
    let mut cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    cfg.lints.user_pattern.patterns = vec!["ChkTeX".into()];
    cfg
}

/// The early findings of `doc.tex` and the run's of the early-tier rules, without the marks.
fn both(project: &Project, cfg: &Config) -> (Vec<Record>, Vec<Record>, Analysis) {
    let path = project.0.join("doc.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let (_, early) = early::lint(&source, &path, cfg);
    let analysis = Machine::analyze(&source, Some(&path), cfg);
    let last: Vec<Record> = satex::linter::lint_document(&analysis)
        .into_iter()
        .filter(|r| satex::linter::settled(r["code"].as_str().unwrap_or_default()))
        .collect();
    (early, last, analysis)
}

fn messages(records: &[Record]) -> Vec<String> {
    let mut out: Vec<String> =
        records.iter().map(|r| format!("{} {}:{} {}", r["code"], r["file"], r["line"], r["message"])).collect();
    out.sort();
    out
}

/// Intuition: a rule that needs only the text and a few standard names finds on the sketch what
/// it finds on the run, so the editor's early findings are its final ones and nothing flickers.
/// Bug pinned: the sketch missed inputs or verbatim bodies, or read a verbatim `%` as a comment.
#[test]
fn early_findings_are_the_run_s() {
    if !crate::support::installed() {
        return;
    }
    let body = "% TODO: first\n\\input{part}\n\\begin{verbatim}\n% TODO not a comment\n\\end{verbatim}\nChkTeX here.\n";
    let project = project(
        "same",
        &[("doc.tex", &document(body)), ("part.tex", PART), ("latexmkrc", "$pdflatex = 'pdflatex';\n")],
    );
    let (early, last, analysis) = both(&project, &config());
    let said = messages(&early);
    assert!(early.iter().all(|r| r["provisional"] == true), "{said:?}");
    for code in ["comment-marker", "user-pattern", "build-command-placeholders", "build-engine-options"] {
        assert!(early.iter().any(|r| r["code"] == code), "no early {code}: {said:?}");
    }
    assert!(said.iter().any(|m| m.contains("FIXME")), "the input's marker: {said:?}");
    assert!(!said.iter().any(|m| m.contains("not a comment")), "a verbatim body: {said:?}");
    assert_eq!(said, messages(&last));
    assert!(early::retracted(&early, &last, &analysis).is_empty());
}

/// Intuition: the sketch follows every `\input` it sees, and the run skips a false branch, but
/// the run says it left that existing file unread (`file-not-read`), so it cannot rule on the
/// marker there either. The finding stays, unconfirmed, and the one it did read is confirmed.
/// Bug pinned: a finding in a part the run never read was retracted as if the run had judged it.
#[test]
fn a_file_the_run_leaves_unread_keeps_its_early_finding_unconfirmed() {
    let body = "% TODO: kept\n\\iffalse\n\\input{old}\n\\fi\n";
    let project = project("unread", &[("doc.tex", &document(body)), ("old.tex", "% TODO: stale\n")]);
    let mut cfg = crate::support::bare();
    cfg.load.inputs = true;
    let (early, last, analysis) = both(&project, &cfg);
    assert!(early::retracted(&early, &last, &analysis).is_empty(), "{:?}", messages(&last));
    let verdict = early::verdict(&early, &last, &analysis).unwrap();
    assert!(verdict.contains("of 2 early findings the run confirmed 1, left 1 unconfirmed"), "{verdict}");
}

/// Intuition: a run a budget stopped read part of the document only, so it cannot tell a wrong
/// guess from a part it never reached. It retracts nothing: the early findings stay unconfirmed.
/// Bug pinned: a thesis run cut by `limits.seconds` retracted every early finding.
#[test]
fn a_stopped_run_retracts_nothing() {
    let body = "% TODO: kept\n\\iffalse\n\\input{old}\n\\fi\n";
    let project = project("stopped", &[("doc.tex", &document(body)), ("old.tex", "% TODO: stale\n")]);
    let mut cfg = crate::support::bare();
    cfg.load.inputs = true;
    cfg.limits.steps = 1;
    let (early, last, analysis) = both(&project, &cfg);
    assert!(analysis.stopped(), "the budget stops the run");
    assert!(early::retracted(&early, &last, &analysis).is_empty(), "{:?}", messages(&last));
    assert!(early::verdict(&early, &last, &analysis).unwrap().contains("unconfirmed"));
}

/// Intuition: `satex lint` prints the early findings on stderr before the run's output, and says
/// at the end which the run retracted. stdout stays the run's alone.
/// Bug pinned: early findings went to stdout, so scripts read them twice.
#[test]
fn the_cli_prints_early_findings_and_the_verdict() {
    if !crate::support::installed() {
        return;
    }
    let body = "% TODO: kept\n\\newcommand\\mk[1]{\\label{l:#1}}\\section{A}\\mk{a}\nSee \\ref{l:a}.\n";
    let project = project("cli", &[("doc.tex", &document(body))]);
    let out = crate::support::satex()
        .args(["lint", "--set", "limits.seconds=0", "-f"])
        .arg(project.0.join("doc.tex"))
        .env("SATEX_CACHE", crate::support::test_cache())
        .output()
        .unwrap();
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let early = stderr.find("early doc.tex:3:3").unwrap_or_else(|| panic!("{stderr}"));
    let verdict = stderr
        .find("of 2 early findings the run confirmed 1, retracted 1: doc.tex:5")
        .unwrap_or_else(|| panic!("{stderr}"));
    assert!(early < verdict, "{stderr}");
    assert!(
        stdout.contains("kept") && !stdout.contains("anywhere in the document") && !stdout.contains("early"),
        "{stdout}"
    );
}

/// The early findings of `codes` and the run's, for a document with the kernel and packages.
fn of_codes(name: &str, body: &str, extra: &[(&str, &str)], codes: &[&str]) -> Option<(Vec<String>, Vec<String>)> {
    if !crate::support::installed() {
        return None;
    }
    let doc = format!(
        "\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\\usepackage{{graphicx}}\n\\begin{{document}}\n{body}\\bibliographystyle{{plain}}\n\\bibliography{{refs}}\n\\end{{document}}\n"
    );
    let mut files = vec![("doc.tex", doc.as_str()), ("refs.bib", "@misc{known, title = {T}}\n")];
    files.extend_from_slice(extra);
    let project = project(name, &files);
    let mut cfg = config();
    cfg.load.packages = true;
    let (early, last) = both_all(&project, &cfg);
    let keep = |r: &[Record]| {
        messages(&r.iter().filter(|r| codes.iter().any(|c| r["code"] == *c)).cloned().collect::<Vec<_>>())
    };
    Some((keep(&early), keep(&last)))
}

/// [`both`], the run's side holding every rule's findings, not only the settled ones.
fn both_all(project: &Project, cfg: &Config) -> (Vec<Record>, Vec<Record>) {
    let path = project.0.join("doc.tex");
    let source = std::fs::read_to_string(&path).unwrap();
    let (_, early) = early::lint(&source, &path, cfg);
    let analysis = Machine::analyze(&source, Some(&path), cfg);
    (early, satex::linter::lint_document(&analysis))
}

const KEYS: [&str; 6] = [
    "duplicate-label",
    "undefined-reference",
    "undefined-citation",
    "missing-graphic",
    "unresolved-file",
    "duplicate-package",
];

/// Intuition: `\label`, `\ref`, `\cite`, `\usepackage`, `\input` and `\includegraphics` mean what
/// the kernel and graphicx make them, so the keys they name in the text are the run's keys, and
/// each rule of the `keys` tier finds early what it finds at the end.
/// Bug pinned: a guessed key differed from the run's (a list of keys, a trailing space), so an
/// editor's early finding moved or vanished when the run ended.
#[test]
fn keys_findings_are_the_run_s() {
    let body = "\\section{A}\\label{a}\\label{a}\nSee \\ref{a}, \\ref{gone} and \\cite{known,unknown}.\n\\input{nothere}\n\\includegraphics{pic}\\includegraphics{nofile}\n";
    let png = "\u{89}PNG\r\n";
    let Some((early, last)) = of_codes("keys", body, &[("pic.png", png)], &KEYS) else { return };
    for code in KEYS {
        assert!(early.iter().any(|m| m.contains(code)), "no early {code}: {early:#?}");
    }
    assert_eq!(early, last);
}

/// Intuition: a label a macro builds from its argument has a key only the run knows, so the
/// sketch reports its reference as undefined, and a graphic that only a skipped branch includes
/// is missing only to the sketch. The run retracts both.
/// Bug pinned: a provisional guess the run disagrees with stayed in the output.
#[test]
fn keys_guesses_the_run_disagrees_with_are_retracted() {
    let body = "\\newcommand\\mk[1]{\\label{l:#1}}\\section{A}\\mk{a}\nSee \\ref{l:a}.\n\\iffalse\\includegraphics{nofile}\\fi\n";
    let Some((early, last)) = of_codes("keys-retract", body, &[], &KEYS) else { return };
    assert!(early.iter().any(|m| m.contains("l:a")) && early.iter().any(|m| m.contains("nofile")), "{early:#?}");
    assert!(!last.iter().any(|m| m.contains("l:a") || m.contains("nofile")), "{last:#?}");
}
