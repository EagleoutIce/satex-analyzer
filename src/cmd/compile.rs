//! `satex compile`: build the document with the tools this machine has —
//! latexmk where it is installed, else the engine the analysis found — and
//! explain each error and warning the build log reports: by the finding
//! satex made at that place, else by the rule that describes that message.
//!
//! The build reads the project's own latexmkrc; satex only adds what keeps
//! the log readable (`-file-line-error`, long lines) and never opens a
//! viewer (`-pv-`).
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Context, Output};
use crate::query::Record;
use serde_json::json;

/// One message of the build log, where TeX says it arose.
#[derive(Debug, PartialEq)]
struct Logged {
    path: Option<PathBuf>,
    line: Option<u64>,
    error: bool,
    message: String,
}

pub fn run(context: &Context) -> Result<Output, String> {
    let analysis = context.analysis;
    let main = PathBuf::from(analysis.file_name(analysis.main_file));
    let main = main.canonicalize().unwrap_or(main);
    let dir = main.parent().map(Path::to_path_buf).filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| ".".into());
    let file = main.file_name().and_then(|f| f.to_str()).ok_or("the document has no file name")?;
    let job = main.file_stem().and_then(|f| f.to_str()).unwrap_or(file).to_string();
    let project = &analysis.project;
    let out_dir = project.output_dir().map(|(d, _)| dir.join(d));
    let log_path = out_dir.clone().unwrap_or_else(|| dir.clone()).join(format!("{job}.log"));
    let engine = analysis.plugins.engine;
    let status = match which::which("latexmk") {
        // latexmk reruns as often as the document needs and reads the
        // latexmkrc; the engine is ours only where that names none.
        Ok(_) => {
            let mut args = vec!["-interaction=nonstopmode", "-file-line-error", "-pv-", "-pvc-"];
            if !["pdf_mode", "pdflatex", "lualatex", "xelatex", "latex"].iter().any(|k| project.setting(k).is_some()) {
                args.push(match engine {
                    crate::config::Engine::LuaTeX => "-lualatex",
                    crate::config::Engine::XeTeX => "-xelatex",
                    _ => "-pdf",
                });
            }
            build("latexmk", &args, file, &dir)?
        }
        // The engine alone: again while the log asks for a rerun, as
        // LaTeX's cross-references need.
        Err(_) => {
            let output = out_dir.as_ref().map(|d| format!("-output-directory={}", d.display()));
            let mut args = vec!["-interaction=nonstopmode", "-file-line-error"];
            args.extend(output.as_deref());
            let mut status = build(engine.program(), &args, file, &dir)?;
            for _ in 0..2 {
                let log = std::fs::read_to_string(&log_path).unwrap_or_default();
                if !log.contains("Rerun to get") {
                    break;
                }
                status = build(engine.program(), &args, file, &dir)?;
            }
            status
        }
    };
    let log = std::fs::read(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    let logged = parse_log(&String::from_utf8_lossy(&log), &dir);
    let findings = crate::lint::lint(analysis);
    let mut records: Vec<Record> = logged.iter().map(|l| explain(l, &findings, &main)).collect();
    if !status.success() && !logged.iter().any(|l| l.error) {
        records.push(record(&main, None, true, "build", format!("the build failed ({status}) without a TeX error in the log"), None));
    }
    Ok(Output::Lint(records))
}

/// The errors (`file:line: message`, as `-file-line-error` writes them) and
/// the LaTeX, class and package warnings of a log, each with the file TeX
/// was reading: the innermost `(file` the log opened and has not closed.
/// One run of `program` on `file` in `dir`. Its output is progress on
/// stderr; the findings are the result.
fn build(program: &str, args: &[&str], file: &str, dir: &Path) -> Result<std::process::ExitStatus, String> {
    Command::new(program)
        .args(args)
        .arg(file)
        .current_dir(dir)
        // Unwrapped log lines keep paths and messages whole.
        .env("max_print_line", "100000")
        .stdout(std::process::Stdio::from(std::io::stderr()))
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))
}

fn parse_log(log: &str, dir: &Path) -> Vec<Logged> {
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut lines = log.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some((path, number, message)) = file_line_error(line) {
            let mut message = message.to_string();
            // A package error goes on in lines that start with `(package)`.
            while let Some(next) = lines.peek().filter(|l| l.starts_with('(') && l.contains(')')) {
                message.push(' ');
                message.push_str(next.split_once(')').map_or("", |(_, rest)| rest).trim());
                lines.next();
            }
            out.push(Logged { path: Some(dir.join(path)), line: Some(number), error: true, message });
            continue;
        }
        if let Some(at) = ["LaTeX Warning: ", "Package ", "Class "].iter().find_map(|p| line.find(p))
            && line[at..].contains("Warning: ")
        {
            let mut message = line[at..].to_string();
            while let Some(next) = lines.peek().filter(|l| l.starts_with('(') || l.starts_with("  ")) {
                message.push(' ');
                message.push_str(next.split_once(')').map_or(*next, |(_, rest)| rest).trim());
                lines.next();
            }
            // The end-of-run summaries repeat what was said where it arose.
            if message.starts_with("LaTeX Warning: There were") || message.contains("Label(s) may have changed") {
                continue;
            }
            let number = message.rsplit_once("on input line ").and_then(|(_, n)| n.trim_end_matches('.').parse().ok());
            out.push(Logged { path: files.last().cloned(), line: number, error: false, message });
            continue;
        }
        track_files(line, dir, &mut files);
    }
    out
}

/// `./chapter.tex:12: Undefined control sequence.`
fn file_line_error(line: &str) -> Option<(&str, u64, &str)> {
    let (head, message) = line.split_once(": ")?;
    let (path, number) = head.rsplit_once(':')?;
    let number = number.parse().ok()?;
    (path.ends_with(".tex") || path.ends_with(".sty") || path.ends_with(".cls") || path.contains('/'))
        .then_some((path, number, message))
}

/// The files a log line opens with `(path` and closes with `)`.
fn track_files(line: &str, dir: &Path, files: &mut Vec<PathBuf>) {
    let mut rest = line;
    while let Some(i) = rest.find(['(', ')']) {
        if rest[i..].starts_with(')') {
            files.pop();
            rest = &rest[i + 1..];
            continue;
        }
        let name: String = rest[i + 1..].chars().take_while(|c| !c.is_whitespace() && !"()".contains(*c)).collect();
        let opened = name.contains('.') && (name.starts_with('.') || name.starts_with('/') || name.contains('/'));
        files.push(if opened { dir.join(&name) } else { PathBuf::new() });
        rest = &rest[i + 1 + name.len()..];
    }
}

/// The record for one logged message, with satex's finding at that place or
/// the summary of the rule the message is named by.
fn explain(logged: &Logged, findings: &[Record], main: &Path) -> Record {
    let path = logged.path.clone().filter(|p| p.is_file()).unwrap_or_else(|| main.to_path_buf());
    let same = |p: &str| Path::new(p).canonicalize().ok() == path.canonicalize().ok();
    let near = |r: &&Record| {
        r.get("path").and_then(|p| p.as_str()).is_some_and(same)
            && match (logged.line, r.get("line").and_then(|l| l.as_u64())) {
                (Some(a), Some(b)) => a.abs_diff(b) <= 1,
                _ => false,
            }
    };
    let code = code_of(&logged.message);
    let finding = findings.iter().filter(near).min_by_key(|r| r.get("code").and_then(|c| c.as_str()) != code);
    let why = match finding {
        Some(f) => {
            let text = f.get("message").and_then(|m| m.as_str()).unwrap_or_default();
            let fix = f.get("fix").and_then(|m| m.as_str()).map(|fix| format!(" — {fix}")).unwrap_or_default();
            Some(format!("{text}{fix}"))
        }
        None => code.and_then(|c| crate::lint::all().find(|r| r.code == c)).map(|r| r.summary.to_string()),
    };
    let code = code.unwrap_or(if logged.error { "tex-error" } else { "tex-warning" });
    record(&path, logged.line, logged.error, code, logged.message.clone(), why)
}

/// The lint rule a TeX or LaTeX message is.
fn code_of(message: &str) -> Option<&'static str> {
    const NAMED: [(&str, &str); 7] = [
        ("Undefined control sequence", "undefined-control-sequence"),
        ("Environment ", "undefined-environment"),
        ("' not found", "unresolved-file"),
        ("multiply defined", "duplicate-label"),
        ("Citation `", "undefined-citation"),
        ("Reference `", "undefined-reference"),
        ("Missing number", "missing-number"),
    ];
    NAMED.iter().find(|(text, _)| message.contains(text)).map(|(_, code)| *code)
}

fn record(path: &Path, line: Option<u64>, error: bool, code: &str, message: String, why: Option<String>) -> Record {
    let mut r = Record::new();
    let file = path.file_name().and_then(|f| f.to_str()).unwrap_or_default();
    r.insert("file".into(), json!(file));
    r.insert("path".into(), json!(path.display().to_string()));
    r.insert("line".into(), json!(line));
    r.insert("col".into(), json!(1));
    r.insert("code".into(), json!(code));
    r.insert("category".into(), json!("correctness"));
    r.insert("severity".into(), json!(if error { "error" } else { "warning" }));
    r.insert("origin".into(), json!("build"));
    r.insert("message".into(), json!(message));
    r.insert("fix".into(), json!(why));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_gives_its_errors_and_warnings_where_they_arose() {
        let log = "This is pdfTeX\n(./main.tex (/usr/share/article.cls)\n(./intro.tex\n\
                   ./intro.tex:3: Undefined control sequence.\nl.3 \\foo\n\n\
                   LaTeX Warning: Reference `x' on page 1 undefined on input line 5.\n)\n\
                   ./main.tex:9: Package foo Error: bad\n(foo)                more here.\n)";
        let got = parse_log(log, Path::new("/d"));
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!(got[0].path.as_deref(), Some(Path::new("/d/./intro.tex")));
        assert_eq!((got[0].line, got[0].error), (Some(3), true));
        assert_eq!(got[1].path.as_deref(), Some(Path::new("/d/./intro.tex")));
        assert_eq!((got[1].line, got[1].error), (Some(5), false));
        assert_eq!(got[2].message, "Package foo Error: bad more here.");
        assert_eq!(code_of(&got[0].message), Some("undefined-control-sequence"));
        assert_eq!(code_of(&got[1].message), Some("undefined-reference"));
    }
}
