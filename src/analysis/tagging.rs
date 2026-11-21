//! PDF tagging as the run set it up: `\DocumentMetadata` (latex-lab), pdfmanagement and tagpdf.
//! Nothing here is a table of the tagging code's names. The facts are what the kernel's own
//! accessors return in a sandbox at the run's end: `\GetDocumentProperties` for what
//! `\DocumentMetadata` and hyperref recorded, `\pdf_version:` for the PDF version, and the
//! installation's `latex-tagging-status.ltx` run as TeX for each loaded file's status (what
//! `check-tagging-status` prints).

use crate::analysis::facts::Load;
use crate::analysis::machine::{Analysis, Machine};
use crate::analysis::probe::State;
use crate::engine::builtins::LoadKind;
use crate::engine::tex::{Catcode, Meaning, Tok};

/// What an accessor returned.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase", tag = "kind", content = "text")]
pub enum Val {
    /// Nothing: the property was never set.
    Empty,
    /// Characters only.
    Text(String),
    /// Text the run did not know, or commands that survived expansion.
    Unknown,
}

impl Val {
    pub fn text(&self) -> Option<&str> {
        match self {
            Val::Text(t) => Some(t),
            _ => None,
        }
    }
    /// Set to something, known or not.
    pub fn set(&self) -> bool {
        *self != Val::Empty
    }
}

/// The tagging state of a run that called `\DocumentMetadata`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Tagging {
    /// `document/tagging`: `active` or `inactive`.
    pub tagging: Val,
    /// The standards claimed, as pdfmanagement names them: `A-4F`, `UA-2`, `X-4`.
    pub standards: Vec<String>,
    /// `document/lang`: the `/Lang` of the catalog.
    pub lang: Val,
    /// `document/lang` at `\begin{document}`: what the document set, before LaTeX guesses one
    /// from babel (warning `meta/lang-missing`).
    pub lang_set: Val,
    /// `hyperref/pdftitle`: what the XMP `dc:title` is made of.
    pub title: Val,
    /// `\pdf_version:`: the version the backend writes.
    pub pdfversion: Val,
    /// Each loaded package and class with its tagging status, by `latex-tagging-status.ltx`.
    pub statuses: Vec<Status>,
}

impl Tagging {
    /// Whether the structure tree is written.
    pub fn active(&self) -> bool {
        self.tagging.text() == Some("active")
    }
    /// Whether a PDF/UA standard is claimed: what makes a missing title or language an error.
    pub fn ua(&self) -> Option<&str> {
        self.standards.iter().map(String::as_str).find(|s| s.starts_with("UA-"))
    }
}

/// A loaded file's status: the code `latex-tagging-status.ltx` gives it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Status {
    /// `hyperref.sty`.
    pub file: String,
    /// The index of its load in [`crate::analysis::facts::Facts::loads`].
    pub load: usize,
    pub code: u8,
}

impl Status {
    /// The status file's own words for a code (its header).
    pub fn meaning(&self) -> &'static str {
        match self.code {
            4 => "compatible",
            3 => "partially compatible",
            2 => "currently incompatible",
            1 => "not supported",
            _ => "unchecked",
        }
    }
    /// Incompatible now (2) or for good (1).
    pub fn incompatible(&self) -> bool {
        matches!(self.code, 1 | 2)
    }
}

/// The installation's tagging-status data (latex-tagging-status bundle).
const STATUS_FILE: &str = "latex-tagging-status.ltx";

/// The run's tagging state, or `None` when the run never called `\DocumentMetadata`
/// (`document/tagging` is set by every call). Computed once per analysis.
pub fn of(analysis: &Analysis) -> Option<&Tagging> {
    analysis.tagging.get_or_init(|| read(analysis).map(Box::new)).as_deref()
}

/// The properties `\DocumentMetadata`, pdfmanagement and hyperref recorded, read with
/// `\GetDocumentProperties` (pdfmanagement's documented accessor).
const PROPERTIES: [&str; 6] = [
    "document/tagging",
    "document/pdfstandard",
    "document/pdfstandard-UA",
    "document/pdfstandard-X",
    "document/lang",
    "hyperref/pdftitle",
];

fn read(analysis: &Analysis) -> Option<Tagging> {
    if !analysis.defined("GetDocumentProperties") {
        return None;
    }
    let mut exprs: Vec<String> = PROPERTIES.iter().map(|p| format!("\\GetDocumentProperties{{{p}}}")).collect();
    // No property set at all (no `\DocumentMetadata`): nothing to look up, and the status file is costly.
    if evaluate(analysis, State::End, None, &exprs)?.iter().all(|v| *v == Val::Empty) {
        return None;
    }
    exprs.push("\\ifcsname pdf_version:\\endcsname\\csname pdf_version:\\endcsname\\fi".into());
    // `\begin{document}`'s state: what `\DocumentMetadata` and the preamble set, before the
    // kernel guesses a language, and the loads to look up (every package is loaded by then);
    // the end's: what the PDF gets. A run without `\begin{document}` has only the end.
    let loads: Vec<(usize, String)> = files(analysis);
    let mut with_status = exprs.clone();
    with_status.extend(loads.iter().map(|(_, f)| status(f)));
    let pre = evaluate(analysis, State::Preamble, Some(STATUS_FILE), &with_status);
    let end = match &pre {
        Some(_) => evaluate(analysis, State::End, None, &exprs)?,
        None => evaluate(analysis, State::End, Some(STATUS_FILE), &with_status)?,
    };
    let codes = pre.as_ref().unwrap_or(&end).clone();
    let at = |i: usize| end.get(i).cloned().unwrap_or(Val::Unknown);
    let before = |i: usize| pre.as_ref().and_then(|p| p.get(i).cloned());
    // `\DocumentMetadata`'s own facts hold from the preamble on: unknown at the end (a path the
    // run lost), the preamble's value stands
    let fixed = |i: usize| match at(i) {
        Val::Unknown => before(i).unwrap_or(Val::Unknown),
        v => v,
    };
    let tagging = fixed(0);
    if tagging == Val::Empty {
        return None;
    }
    let (a, ua, x) = (fixed(1), fixed(2), fixed(3));
    let mut standards: Vec<String> = Vec::new();
    standards.extend(a.text().map(str::to_string));
    // `{2}{2024}`: the part, then the year
    standards.extend(ua.text().and_then(|t| t.trim_start_matches('{').split('}').next()).map(|n| format!("UA-{n}")));
    standards.extend(x.text().map(|t| t.trim_start_matches("PDF/").to_uppercase()));
    let lang_set = before(4).unwrap_or(Val::Unknown);
    // a language the document set stays: LaTeX only adds one where none is
    let lang = match (at(4), &lang_set) {
        (Val::Unknown, Val::Text(_)) => lang_set.clone(),
        (v, _) => v,
    };
    let (title, pdfversion) = (at(5), fixed(6));
    let statuses = loads
        .into_iter()
        .enumerate()
        .filter_map(|(i, (load, file))| {
            let code = codes.get(PROPERTIES.len() + 1 + i)?.text()?.parse().ok()?;
            // 8: the tagging code's own files, 100: a part its package's entry covers
            (code <= 4).then_some(Status { file, load, code })
        })
        .collect();
    Some(Tagging { tagging, standards, lang, lang_set, title, pdfversion, statuses })
}

/// The status codes of `files` (`geometry.sty`), loaded or not, by the installation's status
/// file: what loading one would bring. A loaded file's is the run's; `None` where unlisted,
/// a part its package's entry covers, or no sandbox could be made.
pub fn statuses(analysis: &Analysis, files: &[String]) -> Vec<Option<u8>> {
    let known = of(analysis).map(|t| t.statuses.as_slice()).unwrap_or_default();
    let exprs: Vec<String> = files.iter().map(|f| status(f)).collect();
    let codes = evaluate(analysis, State::Preamble, Some(STATUS_FILE), &exprs)
        .or_else(|| evaluate(analysis, State::End, Some(STATUS_FILE), &exprs))
        .unwrap_or_default();
    files
        .iter()
        .enumerate()
        .map(|(i, f)| match known.iter().find(|s| s.file == *f) {
            Some(s) => Some(s.code),
            None => codes.get(i)?.text()?.parse().ok().filter(|c| *c <= 4),
        })
        .collect()
}

/// The expression that expands to a file's status code, empty where the status file lists none.
fn status(file: &str) -> String {
    format!("\\ifcsname t@status@{file}\\endcsname\\number\\csname t@status@{file}\\endcsname\\fi")
}

/// The packages and classes the run read, as `\@filelist` names them, once each.
fn files(analysis: &Analysis) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for (i, load) in analysis.facts.loads.iter().enumerate() {
        let Some(file) = file_name(load) else { continue };
        if !out.iter().any(|(_, f)| *f == file) {
            out.push((i, file));
        }
    }
    out
}

fn file_name(load: &Load) -> Option<String> {
    let ext = match load.kind {
        LoadKind::Class => "cls",
        LoadKind::Package => "sty",
        _ => return None,
    };
    (load.status == crate::analysis::facts::LoadStatus::Read).then(|| format!("{}.{ext}", load.name))
}

/// Each of `exprs` expanded (`\edef`) in the run's `state`, after `prelude` (a file the
/// installation has) ran there; `None` when no sandbox can be made.
fn evaluate(analysis: &Analysis, state: State, prelude: Option<&str>, exprs: &[String]) -> Option<Vec<Val>> {
    // a sandbox that loads nothing but the prelude, with the probes' step budget per line; the
    // cache stays on: with nothing loaded it only serves the installation's roots, which a fresh
    // `kpsewhich` probe takes 0.3 s for
    let mut cfg = analysis.settings.clone();
    (cfg.load.packages, cfg.load.classes, cfg.load.format) = (false, false, false);
    (cfg.record_arguments, cfg.verbose, cfg.trace, cfg.timings) = (false, 0, false, false);
    (cfg.limits.steps, cfg.limits.replay_steps) = (cfg.limits.probe_steps, cfg.limits.probe_steps);
    let mut m = Machine::sandbox(&cfg, analysis, state)?;
    let prelude = prelude.and_then(|name| m.input_bytes(name)).flatten();
    if let Some(bytes) = prelude {
        // the file's own `\makeatletter` and `\catcode` restore are part of it
        m.run_line(&String::from_utf8_lossy(&bytes));
    }
    // letters only: a digit would end the name
    let name = |i: usize| {
        let mut n = String::from("satex@tagging@");
        let mut i = i;
        loop {
            n.push(char::from(b'a' + (i % 26) as u8));
            i /= 26;
            if i == 0 {
                break n;
            }
        }
    };
    let line: String = exprs.iter().enumerate().map(|(i, e)| format!("\\edef\\{}{{{e}}}", name(i))).collect();
    m.run_line(&line);
    let vals = (0..exprs.len()).map(|i| {
        let sym = m.out.interner.lookup(&name(i));
        match sym.map(|s| m.env.meaning(s)) {
            Some(Meaning::Macro(mac)) => value(&m, &mac.replacement_text),
            _ => Val::Unknown,
        }
    });
    Some(vals.collect())
}

/// The characters of an expansion, braces kept; a command or unknown text in it makes it
/// unknown. Text a join left one of several (`one of`, paths that set the property apart) is
/// the value all members share, else unknown.
fn value(m: &Machine, toks: &[crate::engine::tex::Token]) -> Val {
    if let [one] = toks
        && m.one_of_members(*one).is_some()
    {
        // A text that holds itself, or nested too deep: unknown.
        let crate::analysis::machine::explore::Nested::Open(inner) = m.one_of_nested(*one) else { return Val::Unknown };
        let mut vals = inner.texts.iter().map(|t| value(m, t));
        let first = vals.next().unwrap_or(Val::Unknown);
        return if vals.all(|v| v == first) { first } else { Val::Unknown };
    }
    let mut out = String::new();
    for t in toks {
        match t.tok {
            Tok::Chr(_, Catcode::Begin) => out.push('{'),
            Tok::Chr(_, Catcode::End) => out.push('}'),
            Tok::Chr(c, _) => out.push(c),
            Tok::Cs(_) | Tok::Param(_) => return Val::Unknown,
        }
    }
    match out.trim() {
        "" => Val::Empty,
        t => Val::Text(t.to_string()),
    }
}
