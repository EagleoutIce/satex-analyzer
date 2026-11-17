//! `satex compile`: build the document with the tools this machine has —
//! latexmk where it is installed, else the engine the analysis found — and
//! explain each error and warning the build log reports: by the finding
//! satex made at that place, else by the rule that describes that message.
//!
//! The build reads the project's own latexmkrc; satex only adds what keeps
//! the log readable (`-file-line-error`, long lines) and never opens a
//! viewer (`-pv-`). It asks for SyncTeX data, which editors and the
//! playground's PDF view link the source by.
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Context, Output};
use crate::queries::query::Record;
use serde_json::json;

/// One message of the build log, where TeX says it arose.
#[derive(Debug, PartialEq)]
struct Logged {
    path: Option<PathBuf>,
    line: Option<u64>,
    error: bool,
    message: String,
}

pub fn run(context: &Context, system: Option<&str>, view: bool) -> Result<Output, String> {
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
    // latexmk runs LaTeX: a plain TeX file (`% !TeX program = tex`) runs its own program.
    let latex = analysis.plugins.kernel == crate::plugins::Kernel::Latex;
    let program = match latex || analysis.plugins.program.is_empty() {
        true => engine.program(),
        false => analysis.plugins.program.as_str(),
    };
    // Knuth's `tex` writes DVI only: pdfTeX runs the same plain format and writes PDF.
    let program = if program == "tex" { "pdftex" } else { program };
    let installed = |p: &str| crate::project::paths::which(p).is_ok();
    let arara = analysis.plugins.magic.iter().any(|m| m.key == "arara");
    let asked = system.unwrap_or(&analysis.settings.compile.system);
    // satex's own build yields to a build the project configures itself
    let configured = arara || project.files.iter().any(|f| f.tool == "latexmk" || f.tool == "tectonic");
    let asked = if asked == "satex" && configured { "auto" } else { asked };
    let mut preamble = String::new();
    let chosen = match asked {
        "auto" if arara && installed("arara") => "arara",
        "auto" if dir.join("Tectonic.toml").is_file() && installed("tectonic") => "tectonic",
        "auto" if latex && installed("latexmk") => "latexmk",
        "auto" => "engine",
        "engine" => "engine",
        "satex" => "satex",
        s @ ("latexmk" | "arara" | "tectonic" | "make" | "l3build") if installed(s) => s,
        s @ ("latexmk" | "arara" | "tectonic" | "make" | "l3build") => return Err(format!("{s} is not installed")),
        other => {
            return Err(format!(
                "unknown build system `{other}`: latexmk, arara, tectonic, make, l3build, engine or satex"
            ));
        }
    };
    let status = match chosen {
        // each reads its own configuration and reruns as the document needs
        "arara" => build("arara", &["--verbose"], file, &dir)?,
        "tectonic" => {
            build("tectonic", &["-X", "compile", "--synctex", "--keep-logs", "--keep-intermediates"], file, &dir)?
        }
        "make" => build("make", &[], "", &dir)?,
        "l3build" => build("l3build", &["doc"], "", &dir)?,
        // latexmk reruns as often as the document needs and reads the
        // latexmkrc; the engine is ours only where that names none.
        "latexmk" => {
            // `-f`: past an error, so the references the later runs resolve are not reported.
            let mut args = vec!["-interaction=nonstopmode", "-file-line-error", "-synctex=1", "-f", "-pv-", "-pvc-"];
            if !["pdf_mode", "pdflatex", "lualatex", "xelatex", "latex"].iter().any(|k| project.setting(k).is_some()) {
                args.push(match engine {
                    crate::project::config::Engine::LuaTeX => "-lualatex",
                    crate::project::config::Engine::XeTeX => "-xelatex",
                    _ => "-pdf",
                });
            }
            build("latexmk", &args, file, &dir)?
        }
        "satex" => {
            let at = out_dir.clone().unwrap_or_else(|| dir.clone());
            let passes = analysis.settings.compile.passes;
            let (status, dumped) = own(program, latex, context.source, file, &job, &dir, &at, passes)?;
            preamble = dumped;
            status
        }
        // The engine alone: again while the log asks for a rerun, as
        // LaTeX's cross-references need.
        _ => {
            let output = out_dir.as_ref().map(|d| format!("-output-directory={}", d.display()));
            let mut args = vec!["-interaction=nonstopmode", "-file-line-error"];
            args.push("-synctex=1");
            args.extend(output.as_deref());
            let mut status = build(program, &args, file, &dir)?;
            for _ in 0..2 {
                let log = std::fs::read_to_string(&log_path).unwrap_or_default();
                if !log.contains("Rerun to get") {
                    break;
                }
                status = build(program, &args, file, &dir)?;
            }
            status
        }
    };
    if view {
        open_viewer(&analysis.settings.compile.viewer, &log_path.with_extension("pdf"));
    }
    let log = std::fs::read(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    // a preamble read from a format said its messages while the format was dumped
    let mut logged = parse_log(&preamble, &dir);
    logged.extend(parse_log(&String::from_utf8_lossy(&log), &dir));
    let findings = if analysis.settings.compile.explain { crate::linter::lint(analysis) } else { Vec::new() };
    let mut records: Vec<Record> = logged.iter().map(|l| explain(l, &findings, &main)).collect();
    if !status.success() && !logged.iter().any(|l| l.error) {
        records.push(record(
            &main,
            None,
            true,
            "build",
            format!("the build failed ({status}) without a TeX error in the log"),
            None,
        ));
    }
    Ok(Output::Lint(records))
}

/// The build files a run reads back: equal before and after a pass, the references rest.
const OWN: [&str; 16] =
    ["aux", "toc", "lof", "lot", "out", "bbl", "ind", "nav", "snm", "vrb", "gls", "acr", "nls", "loa", "lol", "thm"];
/// Whether a file of this extension is a build file a run reads back.
pub fn rests(ext: &str) -> bool {
    OWN.contains(&ext)
}
/// Where satex's build keeps its formats and the stamps of its tools' inputs.
pub const KEEP: &str = ".satex-build";

/// satex's own build in `at`: the engine until the build files rest (at most `passes`), BibTeX,
/// Biber and makeindex where their inputs changed, a LaTeX preamble read from a format dumped once
/// per preamble. A fresh build's first pass writes no PDF. Gives the status and the dump's log.
#[allow(clippy::too_many_arguments)]
fn own(
    program: &str,
    latex: bool,
    source: &str,
    file: &str,
    job: &str,
    dir: &Path,
    at: &Path,
    passes: u32,
) -> Result<(std::process::ExitStatus, String), String> {
    let keep = at.join(KEEP);
    std::fs::create_dir_all(&keep).map_err(|e| format!("{}: {e}", keep.display()))?;
    let mut args = vec!["-interaction=nonstopmode".to_string(), "-file-line-error".into(), "-synctex=1".into()];
    if at != dir {
        args.push(format!("-output-directory={}", at.display()));
    }
    let fmt = if latex && program == "pdflatex" { format(source, file, job, dir, &keep) } else { None };
    if let Some((fmt, _)) = &fmt {
        args.push(format!("-fmt={}", fmt.display()));
    }
    let read = |ext: &str| std::fs::read(at.join(format!("{job}.{ext}"))).unwrap_or_default();
    let rested = || -> Vec<u8> { OWN.iter().flat_map(|e| [read(e), b"\0".to_vec()].concat()).collect() };
    let mut was = rested();
    // LaTeX writes an .aux: a first pass without one is never the last
    let fresh = latex && read("aux").is_empty();
    let mut status = None;
    for pass in 0..passes.max(1) {
        let draft = fresh && pass == 0;
        let mut pass_args: Vec<&str> = args.iter().map(String::as_str).collect();
        if draft {
            pass_args.retain(|a| *a != "-synctex=1");
            pass_args.push("-draftmode");
        }
        status = Some(build(program, &pass_args, file, dir)?);
        tools(job, dir, at, &keep)?;
        let now = rested();
        let log = String::from_utf8_lossy(&read("log")).into_owned();
        if !draft && now == was && !log.contains("Rerun to get") && !log.contains("Label(s) may have changed") {
            break;
        }
        was = now;
    }
    let dumped = fmt.map(|(_, log)| log).unwrap_or_default();
    Ok((status.ok_or("no pass ran")?, dumped))
}

/// BibTeX, Biber and makeindex, each where its input changed since it last ran or its output is missing.
fn tools(job: &str, dir: &Path, at: &Path, keep: &Path) -> Result<(), String> {
    let text = |ext: &str| std::fs::read_to_string(at.join(format!("{job}.{ext}"))).unwrap_or_default();
    let bib = |names: &mut dyn Iterator<Item = &str>| -> String {
        names.map(|n| std::fs::read_to_string(dir.join(n.trim()).with_extension("bib")).unwrap_or_default()).collect()
    };
    let aux = text("aux");
    // what BibTeX reads of the .aux, and the databases it names
    let cites: String =
        aux.lines().filter(|l| ["\\citation", "\\bibdata", "\\bibstyle"].iter().any(|p| l.starts_with(p))).collect();
    let data = aux.lines().filter_map(|l| l.strip_prefix("\\bibdata{")?.strip_suffix('}'));
    let cites = cites.clone() + &bib(&mut data.flat_map(|d| d.split(',')));
    let bcf = text("bcf");
    let sources =
        bcf.split("<bcf:datasource").skip(1).filter_map(|d| d.split_once('>')?.1.split_once('<').map(|(n, _)| n));
    let bcf = bcf.clone() + &bib(&mut sources.map(|n| n.trim_end_matches(".bib")));
    let idx = text("idx");
    let runs = [
        ("bibtex", aux.contains("\\bibdata{") && bcf.is_empty(), cites, "bbl"),
        ("biber", !bcf.is_empty(), bcf, "bbl"),
        ("makeindex", !idx.trim().is_empty(), idx, "ind"),
    ];
    for (tool, wanted, input, output) in runs {
        let stamp = keep.join(format!("{job}.{tool}"));
        let hash = format!("{:016x}", hash(input.as_bytes()));
        let missing = !at.join(format!("{job}.{output}")).is_file();
        if wanted && (missing || std::fs::read_to_string(&stamp).ok().as_deref() != Some(&hash)) {
            build(tool, &[], job, at)?;
            std::fs::write(&stamp, hash).map_err(|e| format!("{}: {e}", stamp.display()))?;
        }
    }
    Ok(())
}

fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// The format of `file`'s preamble (mylatexformat), dumped again only when the preamble or a file
/// it read changed, with the dump's log. None where the dump fails or reports an error: then the
/// runs read the preamble themselves and report it.
fn format(source: &str, file: &str, job: &str, dir: &Path, keep: &Path) -> Option<(PathBuf, String)> {
    let preamble = &source[..source.find("\\begin{document}")?];
    // a `%&format` line picks a format of its own
    if source.starts_with("%&") {
        return None;
    }
    let fmt = keep.join(job);
    let (key, log) = (keep.join(format!("{job}.key")), keep.join(format!("{job}.log")));
    let main = dir.join(file).canonicalize().ok();
    // every file the dump read but the main one, by content: one changed, the format is stale
    let stamp = |path: &str| format!("{path}\t{:016x}\n", hash(&std::fs::read(path).unwrap_or_default()));
    let head = format!("{:016x}\n", hash(preamble.as_bytes()));
    let stored = std::fs::read_to_string(&key).unwrap_or_default();
    let fresh = stored.strip_prefix(&head).is_some_and(|rest| {
        rest.lines().all(|l| l == "unusable" || stamp(l.split('\t').next().unwrap_or_default()) == format!("{l}\n"))
    });
    if !fresh {
        let ini = ["-ini", "-interaction=nonstopmode", "-file-line-error", "-recorder"];
        let out = format!("-output-directory={}", keep.display());
        let status = build(
            "pdftex",
            &[&ini[..], &[&out, &format!("-jobname={job}"), "&pdflatex", "mylatexformat.ltx"]].concat(),
            file,
            dir,
        );
        let said = std::fs::read_to_string(&log).unwrap_or_default();
        let fls = std::fs::read_to_string(keep.join(format!("{job}.fls"))).unwrap_or_default();
        let mut inputs: Vec<&str> = fls.lines().filter_map(|l| l.strip_prefix("INPUT ")).collect();
        inputs.sort_unstable();
        inputs.dedup();
        inputs.retain(|p| dir.join(p).canonicalize().ok() != main);
        let usable = status.is_ok_and(|s| s.success()) && !parse_log(&said, dir).iter().any(|l| l.error);
        let mut text = head.clone();
        text.extend(inputs.iter().map(|p| stamp(&dir.join(p).display().to_string())));
        if !usable {
            text.push_str("unusable\n");
        }
        let _ = std::fs::write(&key, text);
    }
    let stored = std::fs::read_to_string(&key).unwrap_or_default();
    let usable = !stored.lines().any(|l| l == "unusable") && fmt.with_extension("fmt").is_file();
    usable.then(|| (fmt, std::fs::read_to_string(&log).unwrap_or_default()))
}

/// Opens `pdf` in `viewer` (`auto`/`none`: the system's opener), detached.
fn open_viewer(viewer: &str, pdf: &Path) {
    let program = match viewer {
        "auto" | "none" | "" if cfg!(target_os = "macos") => "open",
        "auto" | "none" | "" if cfg!(windows) => "explorer",
        "auto" | "none" | "" => "xdg-open",
        other => other,
    };
    eprintln!("$ {program} {}", pdf.display());
    let spawned = Command::new(program)
        .arg(pdf)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(e) = spawned {
        eprintln!("satex: cannot open the PDF with {program}: {e}");
    }
}

/// The errors (`file:line: message`, as `-file-line-error` writes them) and
/// the LaTeX, class and package warnings of a log, each with the file TeX
/// was reading: the innermost `(file` the log opened and has not closed.
/// One run of `program` on `file` in `dir`. Its output is progress on
/// stderr; the findings are the result.
/// Each run is named on stderr first, as a shell shows it (`$ latexmk …`).
fn build(program: &str, args: &[&str], file: &str, dir: &Path) -> Result<std::process::ExitStatus, String> {
    eprintln!("$ {program} {} {file}", args.join(" "));
    Command::new(program)
        .args(args)
        .args((!file.is_empty()).then_some(file))
        .current_dir(dir)
        // Unwrapped log lines keep paths and messages whole. BibTeX
        // refuses 20000 and more ("3 is a bad bad").
        .env("max_print_line", "10000")
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
        // TeX's fatal ends without a file and line: `*** (job aborted, no legal \end found)`
        // (tex.web § 360) and `! Emergency stop.` (§ 93)
        // (an error with its place said so already: `! Emergency stop.` follows it)
        let unplaced = line.strip_prefix("! ").filter(|_| !out.iter().any(|l: &Logged| l.error));
        if let Some(message) = line.strip_prefix("*** (").and_then(|m| m.strip_suffix(')')).or(unplaced) {
            out.push(Logged { path: files.last().cloned(), line: None, error: true, message: message.to_string() });
            continue;
        }
        // `Overfull \hbox (12.3pt too wide) in paragraph at lines 4--7`: by how much, where it begins
        if line.starts_with("Overfull \\") || line.starts_with("Underfull \\") {
            let number = ["at lines ", "at line "]
                .iter()
                .find_map(|p| line.rsplit_once(p))
                .and_then(|(_, n)| n.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok());
            out.push(Logged { path: files.last().cloned(), line: number, error: false, message: line.to_string() });
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
    // A finding explains the message only when it is about the same thing:
    // the same rule, and the key the message quotes (`` `knuth84' ``).
    let key = logged.message.split_once('`').and_then(|(_, rest)| rest.split_once('\'')).map(|(key, _)| key);
    let about = |r: &&Record| {
        let text = r.get("message").and_then(|m| m.as_str()).unwrap_or_default();
        code.is_none_or(|c| r.get("code").and_then(|c| c.as_str()) == Some(c)) && key.is_none_or(|k| text.contains(k))
    };
    let finding = findings.iter().filter(near).find(about);
    let why = match finding {
        Some(f) => {
            let text = f.get("message").and_then(|m| m.as_str()).unwrap_or_default();
            let fix = f.get("fix").and_then(|m| m.as_str()).map(|fix| format!(". {fix}")).unwrap_or_default();
            Some(format!("{text}{fix}"))
        }
        None => code.and_then(|c| crate::linter::all().find(|r| r.code == c)).map(|r| r.summary.to_string()),
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

    #[test]
    fn a_bad_box_is_a_warning_at_its_first_line() {
        let log = "(./main.tex\nOverfull \\hbox (12.3pt too wide) in paragraph at lines 4--7\n[]\\OT1/cmr/m/n/10 x\n\n\
                   Underfull \\vbox (badness 10000) detected at line 9\n)";
        let got = parse_log(log, Path::new("/d"));
        assert_eq!(got.iter().map(|g| (g.line, g.error)).collect::<Vec<_>>(), [(Some(4), false), (Some(9), false)]);
        assert_eq!(got[0].path.as_deref(), Some(Path::new("/d/./main.tex")));
        assert!(got[0].message.contains("12.3pt too wide"), "{got:?}");
    }

    #[test]
    fn a_finding_explains_only_the_message_about_its_key() {
        let main = Path::new("main.tex");
        let finding = |code: &str, message: &str| record(main, Some(4), false, code, message.into(), None);
        let findings = [
            finding("space-after-command", "the space after \\TeX is lost"),
            finding("undefined-reference", "no `\\label{sec:nope}` anywhere"),
        ];
        let logged = |message: &str| Logged { path: None, line: Some(4), error: false, message: message.into() };
        let why = |message: &str| explain(&logged(message), &findings, main)["fix"].as_str().map(str::to_owned);
        assert_eq!(why("Reference `sec:nope' on page 1 undefined").as_deref(), Some("no `\\label{sec:nope}` anywhere"));
        // another key on that line: the rule's summary, not the finding about `sec:nope`
        assert!(why("Reference `sec:other' on page 1 undefined").is_some_and(|w| !w.contains("sec:nope")));
        // a citation: no finding of its rule there, so none of the others either
        assert!(why("Citation `knuth84' on page 1 undefined").is_some_and(|w| !w.contains("TeX")));
    }
}
