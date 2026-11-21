//! Early lints. The rules whose [`Needs`] stop short of the run read a sketch of the document
//! first: its files, verbatim bodies, labels, references, citations, packages and graphics,
//! found by the commands `lints.early` lists, all with their standard meaning (assumption
//! `early-standard-names`).
//! The findings are provisional. The run's own findings replace them: what it finds too is
//! confirmed, what it does not is [`retracted`].

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::analysis::facts::{Load, LoadStatus, Occurrence};
use crate::analysis::machine::Analysis;
use crate::engine::builtins::{LoadKind, OccKind};
use crate::engine::tex::{FileId, Span};
use crate::linter::Needs;
use crate::project::config::{Config, Early};
use crate::queries::query::Record;

/// Each call of a control word in `code` with a braced argument on its line: where its backslash
/// stands, its name, the text of its first `[…]`, and the argument.
fn calls(code: &str) -> Vec<(usize, &str, Option<&str>, &str)> {
    let mut out = Vec::new();
    for (at, _) in code.match_indices('\\') {
        let rest = &code[at + 1..];
        let name = &rest[..rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len())];
        let mut tail = rest[name.len()..].trim_start_matches(['*', ' ']);
        let mut options = None;
        while let Some(inner) = tail.strip_prefix('[') {
            let Some(close) = inner.find(']') else { break };
            options.get_or_insert(&inner[..close]);
            tail = inner[close + 1..].trim_start();
        }
        let Some(arg) = tail.strip_prefix('{') else { continue };
        let mut depth = 0usize;
        let close = arg.char_indices().find(|&(_, c)| {
            depth = match c {
                '{' => depth + 1,
                '}' if depth == 0 => return true,
                '}' => depth - 1,
                _ => depth,
            };
            false
        });
        if let (false, Some((close, _))) = (name.is_empty(), close) {
            out.push((at, name, options, &arg[..close]));
        }
    }
    out
}

fn occurrence(kind: OccKind, key: &str, detail: Option<&str>, span: Span) -> Occurrence {
    Occurrence {
        kind,
        key: key.to_string(),
        detail: detail.map(str::to_string),
        span,
        expanded: None,
        call: None,
        site: None,
        package: None,
        node: 0,
        section: None,
        // The sketch follows one path: the run's findings replace its own.
        certain: true,
    }
}

fn load(name: &str, kind: LoadKind, options: Option<&str>, span: Span, status: LoadStatus) -> Load {
    let options = options.map_or_else(Vec::new, |o| o.split(',').map(|o| o.trim().to_string()).collect());
    let (by, file, path, required, provided, call) = (None, None, None, None, None, None);
    Load {
        name: name.to_string(),
        kind,
        options,
        span,
        by,
        file,
        path,
        status,
        required,
        provided,
        depth: 0,
        certain: false,
        call,
    }
}

/// What the sketch guesses of the document's files: occurrences and loads as a run records
/// them, and the files it reads, the main one first.
#[derive(Default)]
struct Guess {
    files: Vec<(PathBuf, String)>,
    occurrences: Vec<Occurrence>,
    loads: Vec<Load>,
    graphics: Vec<(Span, String)>,
    graphics_path: Vec<String>,
}

impl Guess {
    /// Reads `text`, the file `file`: the environments it begins and where a verbatim reader
    /// starts (what [`crate::linter::verbatim::verbatims`] reads), the keys its commands name,
    /// its loads, and the inputs, which join the files.
    fn read(&mut self, file: FileId, dir: &Path, early: &Early) {
        let text = self.files[file as usize].1.clone();
        let lines: Vec<&str> = text.lines().collect();
        let kinds = [
            (OccKind::Label, &early.labels),
            (OccKind::Ref, &early.references),
            (OccKind::Cite, &early.citations),
            (OccKind::Bibliography, &early.bibliographies),
        ];
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let code = &line[..crate::engine::tex::comment_start(line).unwrap_or(line.len())];
            let at =
                |byte: usize| Span { file, line: i as u32 + 1, col: crate::engine::source::col_of_byte(line, byte) };
            let mut next = i + 1;
            for name in &early.inline_verbatim {
                for (b, _) in code.match_indices(&format!("\\{name}")) {
                    if !code[b + name.len() + 1..].starts_with(|c: char| c.is_alphabetic()) {
                        self.occurrences.push(occurrence(OccKind::Catcode, "\\", Some("12"), at(b)));
                    }
                }
            }
            for (b, name, options, arg) in calls(code) {
                // a parameter or a command in it: a macro body's call, whose key only a run knows
                if name != "begin" && arg.contains(['#', '\\']) {
                    continue;
                }
                let span = at(b);
                let keys = || arg.split(',').map(str::trim).filter(|k| !k.is_empty());
                if name == "begin" {
                    self.occurrences.push(occurrence(OccKind::BeginEnvironment, arg, None, span));
                    if early.verbatim.iter().any(|v| v == arg) {
                        self.occurrences.push(occurrence(OccKind::Catcode, "\\", Some("12"), span));
                        let end = format!("\\end{{{arg}}}");
                        // the body is no TeX: nothing in it is a call
                        next = (i..lines.len()).find(|&j| lines[j].contains(&end)).map_or(lines.len(), |j| j + 1);
                        break;
                    }
                } else if let Some((kind, _)) = kinds.iter().find(|(_, names)| names.iter().any(|n| n == name)) {
                    let one = *kind == OccKind::Label;
                    for key in keys().filter(|_| !one).chain(one.then_some(arg.trim())) {
                        self.occurrences.push(occurrence(*kind, key, None, span));
                    }
                } else if early.packages.iter().any(|n| n == name) {
                    for key in keys() {
                        self.loads.push(load(key, LoadKind::Package, options, span, LoadStatus::Read));
                    }
                } else if early.graphics.iter().any(|n| n == name) {
                    self.occurrences.push(occurrence(OccKind::Graphics, arg.trim(), None, span));
                    self.graphics.push((span, arg.trim().to_string()));
                } else if name == "graphicspath" {
                    self.graphics_path
                        .extend(arg.split(['{', '}']).filter(|d| !d.trim().is_empty()).map(str::to_string));
                } else if early.inputs.iter().any(|n| n == name) {
                    let path = dir.join(arg.trim());
                    let path = if path.extension().is_none() { path.with_extension("tex") } else { path };
                    if self.files.iter().any(|(p, _)| *p == path) {
                        continue;
                    }
                    match crate::project::overlay::read_to_string(&path) {
                        Ok(text) => self.files.push((path, text)),
                        // the kernel's `\@missingfileerror`; `\include` only warns (`No file …`)
                        Err(_) if name == "input" => {
                            let file = path.file_name().map(|f| f.to_string_lossy()).unwrap_or_default();
                            let message = format!("LaTeX Error: File `{file}' not found. (Default extension: tex)");
                            self.occurrences.push(occurrence(OccKind::Message, &message, None, span));
                        }
                        Err(_) => {}
                    }
                }
            }
            i = next;
        }
    }

    /// The graphics no file is found for, in the main file's directory or `\graphicspath`'s, with
    /// one of `lints.early.graphic-extensions` when the name has none: graphics' own error.
    /// `\Gin@getbase` asks `\IfFileExists`, which searches TeX's input path too (kpathsea), so a
    /// name no folder holds is looked up there, as the run would (`example-image` of mwe).
    fn missing_graphics(&mut self, dir: &Path, early: &Early, cfg: &Config) {
        let dirs: Vec<PathBuf> =
            std::iter::once(dir.to_path_buf()).chain(self.graphics_path.iter().map(|d| dir.join(d))).collect();
        let mut resolver = None;
        for (span, name) in std::mem::take(&mut self.graphics) {
            let named = Path::new(&name).extension().is_some();
            let files: Vec<String> = named
                .then(|| name.clone())
                .into_iter()
                .chain(early.graphic_extensions.iter().map(|e| format!("{name}.{e}")))
                .collect();
            let tried = |d: &PathBuf| files.iter().any(|f| d.join(f).is_file());
            let mut installed = |files: &[String]| {
                let resolver = resolver.get_or_insert_with(|| {
                    let project = crate::project::build_files::Project::discover_with(dir, &cfg.l3build);
                    crate::engine::loader::Resolver::for_project(cfg, &project)
                });
                files.iter().any(|f| resolver.resolve(f, LoadKind::Input, dir).is_some())
            };
            if !dirs.iter().any(tried) && !installed(&files) {
                let message = format!("Package graphics Error: File `{name}' not found.");
                self.occurrences.push(occurrence(OccKind::Message, &message, None, span));
            }
        }
    }
}

/// The sketch of the document at `main` whose text is `source`: [`Analysis::sketch`] of the files
/// the input commands name with a literal argument, followed from file to file, with what
/// [`Guess`] reads of them. Each path is read once, so this ends.
pub fn sketch(source: &str, main: &Path, cfg: &Config) -> Analysis {
    let early = &cfg.lints.early;
    let dir = crate::project::paths::dir_of(main);
    let mut guess = Guess { files: vec![(main.to_path_buf(), source.to_string())], ..Default::default() };
    let mut at = 0;
    while at < guess.files.len() {
        guess.read(at as FileId, &dir, early);
        at += 1;
    }
    guess.missing_graphics(&dir, early, cfg);
    let paths: Vec<PathBuf> = guess.files.iter().map(|(p, _)| p.clone()).collect();
    let mut analysis = Analysis::sketch(source, &paths, cfg, guess.loads);
    analysis.facts.occurrences = guess.occurrences.into();
    analysis
}

/// The early findings of the document at `main`: those of the rules that need no run, on its
/// [`sketch`], each marked `provisional`. Empty when `lints.early.enabled` is off or the file is
/// literate, which only docstrip's view of it tells apart.
pub fn lint(source: &str, main: &Path, cfg: &Config) -> (Analysis, Vec<Record>) {
    let analysis = sketch(source, main, cfg);
    if !cfg.lints.early.enabled || crate::project::literate::Literate::of(main).is_some() {
        return (analysis, Vec::new());
    }
    let mut records = super::lint_run(&analysis, true, |r| r.needs() < Needs::Run);
    for record in &mut records {
        record.insert("provisional".into(), json!(true));
        record.insert("assumes".into(), json!("early-standard-names"));
    }
    (analysis, records)
}

/// A finding's code, file, line, column and message.
type Key<'a> = (Option<&'a str>, Option<PathBuf>, Option<u64>, Option<u64>, Option<&'a str>);

/// Where a finding stands and what it says: an early one and the run's are the same finding when
/// this is the same.
fn key(r: &Record) -> Key<'_> {
    let path = r.get("path").and_then(|v| v.as_str()).map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.into()));
    let text = |k: &str| r.get(k).and_then(|v| v.as_str());
    let number = |k: &str| r.get(k).and_then(|v| v.as_u64());
    (text("code"), path, number("line"), number("col"), text("message"))
}

/// The early findings the run did not make though it could have: `early` against `last`, the
/// findings of `run`. A run a budget stopped says nothing about any, and one that left part of
/// the document unread (as [`super::Report::unread`] tells) nothing about a file it did not
/// read: such a finding stays, unconfirmed.
pub fn retracted<'a>(early: &'a [Record], last: &[Record], run: &Analysis) -> Vec<&'a Record> {
    if run.stopped() {
        return Vec::new();
    }
    let unread = |d: &crate::analysis::facts::Diagnostic| {
        matches!(d.code, "file-not-read" | "analysis-ended-early" | "limit-reached")
    };
    let whole = !run.facts.diagnostics.iter().any(unread);
    let canonical = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
    let read: crate::fixed::HashSet<PathBuf> = run.file_names().iter().map(|p| canonical(p)).collect();
    let judged = |r: &Record| {
        whole
            || r.get("origin").and_then(|v| v.as_str()) != Some("document")
            || r.get("path").and_then(|v| v.as_str()).is_some_and(|p| read.contains(&canonical(p)))
    };
    let made: crate::fixed::HashSet<_> = last.iter().map(key).collect();
    early.iter().filter(|r| judged(r) && !made.contains(&key(r))).collect()
}

/// The early findings the run neither made nor withdrew: in a part it did not read. They stay
/// in the full list, still provisional.
pub fn unchecked<'a>(early: &'a [Record], last: &[Record], run: &Analysis) -> Vec<&'a Record> {
    let made: crate::fixed::HashSet<_> = last.iter().map(key).collect();
    let gone: crate::fixed::HashSet<_> = retracted(early, last, run).into_iter().map(key).collect();
    early.iter().filter(|r| !made.contains(&key(r)) && !gone.contains(&key(r))).collect()
}

/// `early main.tex:3:5 S012 comment-marker: …`: how `satex lint` prints an early finding.
pub fn line(r: &Record) -> String {
    let text = |k: &str| r.get(k).and_then(|v| v.as_str()).unwrap_or_default();
    let number = |k: &str| r.get(k).and_then(|v| v.as_u64()).unwrap_or_default();
    let place = format!("{}:{}:{}", text("file"), number("line"), number("col"));
    format!("early {place} {}: {}", super::label(text("code")), text("message"))
}

/// What the run made of the `early` findings, given its own `last`: `None` when there were none.
pub fn verdict(early: &[Record], last: &[Record], run: &Analysis) -> Option<String> {
    if early.is_empty() {
        return None;
    }
    let made: crate::fixed::HashSet<_> = last.iter().map(key).collect();
    let confirmed = early.iter().filter(|r| made.contains(&key(r))).count();
    let gone = retracted(early, last, run);
    let open = unchecked(early, last, run).len();
    let mut out = format!("early findings: {confirmed} confirmed");
    if open > 0 {
        out += &format!(", {open} unchecked (the run did not read their part)");
    }
    if !gone.is_empty() {
        let places: Vec<String> = gone.iter().map(|r| line(r).replacen("early ", "", 1)).collect();
        out += &format!(", {} withdrawn: {}", gone.len(), places.join(", "));
    }
    Some(out)
}
