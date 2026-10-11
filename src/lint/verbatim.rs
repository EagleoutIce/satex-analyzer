//! What the run read verbatim, derived from the run itself: opening a
//! verbatim environment changes the category code of `\`, so an
//! `OccKind::Catcode` occurrence for `\` at the span of a
//! `OccKind::BeginEnvironment` is a body nothing in which was read as TeX.
//! The same occurrence without a begin is an inline reader (`\verb`,
//! `\lstinline`), which has to finish on its own line.
//!
//! Text read this way is program text, not prose, so every rule about
//! typography has to leave it alone.  [`crate::lint::text::View::verbatim`]
//! is how a rule asks, and it is built once per lint run.

use crate::fixed::{HashMap, HashSet};
use std::rc::Rc;

use crate::builtins::OccKind;
use crate::lint::fix::{Sources, Text};
use crate::machine::Analysis;
use crate::tex::{FileId, Span};

/// A file of the document itself, with its source.
pub struct File {
    pub id: FileId,
    pub text: Rc<Text>,
}

/// Every file of the document the run read, each path once.
pub fn files(analysis: &Analysis, sources: &Sources) -> Vec<File> {
    let mut seen = HashSet::default();
    let mut out = Vec::new();
    for id in 0..analysis.files.len() as FileId {
        if crate::query::origin(analysis, id) != "document" {
            continue;
        }
        let path = analysis.file_name(id).to_string();
        if !seen.insert(path.clone()) {
            continue;
        }
        let Some(text) = sources.get(&path) else { continue };
        out.push(File { id, text });
    }
    out
}

/// An environment the run read verbatim, found by the category code of `\`
/// changing as it opened, so nothing in its body was read as TeX.
pub struct Verbatim {
    pub file: FileId,
    pub name: String,
    pub from: u32,
    /// The line of the `\end{…}` that closed it.
    pub to: u32,
    /// The column just past that `\end{…}`.
    pub after: u32,
}

/// What the run read verbatim: the environments, and the places an inline
/// reader such as `\verb` started.
#[derive(Default)]
pub struct Verbatims {
    pub bodies: Vec<Verbatim>,
    /// A `\verb` and its kin: from this column to the closing delimiter
    /// (to the end of the line when none is found) nothing was read as TeX.
    /// Such a reader has to finish on its line.
    pub inline: Vec<(FileId, u32, u32, u32)>,
}

pub fn verbatims(analysis: &Analysis, files: &[File]) -> Verbatims {
    let texts: HashMap<FileId, Rc<Text>> = files.iter().map(|f| (f.id, f.text.clone())).collect();
    let mut escaped: HashSet<Span> = HashSet::default();
    let mut begins: HashMap<Span, &str> = HashMap::default();
    for occ in &analysis.facts.occurrences {
        match occ.kind {
            OccKind::Catcode if verbatim_reader(occ) && texts.contains_key(&occ.span.file) => {
                escaped.insert(occ.span);
            }
            OccKind::BeginEnvironment if texts.contains_key(&occ.span.file) => {
                begins.insert(occ.span, &occ.key);
            }
            _ => {}
        }
    }
    let mut out = Verbatims::default();
    for span in escaped {
        let Some(text) = texts.get(&span.file) else { continue };
        let Some(name) = begins.get(&span) else {
            let end = reader_end(text.line(span.line), span.col).unwrap_or(u32::MAX);
            out.inline.push((span.file, span.line, span.col, end));
            continue;
        };
        if let Some((to, after)) = closing(text, name, span.line) {
            out.bodies.push(Verbatim { file: span.file, name: (*name).to_string(), from: span.line, to, after });
        }
    }
    out
}

/// The column just past an inline reader's argument: `\name`, a star,
/// `[…]`, then `{…}` or a text between two equal delimiters.  `None` when
/// the line does not spell it so.
fn reader_end(line: &str, col: u32) -> Option<u32> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = col.checked_sub(1)? as usize;
    (chars.get(i) == Some(&'\\')).then_some(())?;
    i += 1;
    while chars.get(i).is_some_and(|c| c.is_alphabetic() || *c == '@') {
        i += 1;
    }
    while chars.get(i).is_some_and(|c| *c == '*' || *c == ' ') {
        i += 1;
    }
    if chars.get(i) == Some(&'[') {
        i += chars[i..].iter().position(|c| *c == ']')? + 1;
    }
    let open = *chars.get(i)?;
    let mut depth = 0usize;
    for (j, c) in chars.iter().enumerate().skip(i + 1) {
        match *c {
            '}' if open == '{' && depth == 0 => return Some(j as u32 + 2),
            '}' if open == '{' => depth -= 1,
            '{' if open == '{' => depth += 1,
            c if c == open && open != '{' => return Some(j as u32 + 2),
            _ => {}
        }
    }
    None
}

/// Where the `\end{name}` that closes a body opened on `from` stands: its
/// line, and the column just past it.
fn closing(text: &Text, name: &str, from: u32) -> Option<(u32, u32)> {
    let needle = format!("\\end{{{name}}}");
    let last = text.body().lines().count() as u32;
    for line in from..=last {
        let body = text.line(line);
        // On the opening line the body starts after `\begin{name}`.
        let start = match line == from {
            true => body.find(&format!("\\begin{{{name}}}")).map_or(0, |at| at + name.len() + 8),
            false => 0,
        };
        if start > body.len() {
            continue;
        }
        if let Some(at) = body[start..].find(&needle) {
            let end = start + at + needle.len();
            return Some((line, body[..end].chars().count() as u32 + 1));
        }
    }
    None
}

/// The source lines every verbatim body covers, for asking whether a
/// position stands in one.
#[derive(Default)]
pub struct Bodies {
    inside: HashSet<(FileId, u32)>,
    /// The closing line of each body, with the column its text ends at.
    last: HashMap<(FileId, u32), u32>,
    /// A line inline readers ran on, with the columns each covered.
    from: HashMap<(FileId, u32), Vec<(u32, u32)>>,
}

impl Bodies {
    pub fn of(verbatims: &Verbatims) -> Bodies {
        let mut bodies = Bodies::default();
        for v in &verbatims.bodies {
            for line in v.from + 1..v.to {
                bodies.inside.insert((v.file, line));
            }
            let at = bodies.last.entry((v.file, v.to)).or_insert(v.after);
            *at = (*at).max(v.after);
        }
        for (file, line, col, end) in &verbatims.inline {
            bodies.from.entry((*file, *line)).or_default().push((*col, *end));
        }
        bodies
    }

    pub fn holds(&self, file: FileId, line: u32, col: u32) -> bool {
        self.inside.contains(&(file, line))
            || self.last.get(&(file, line)).is_some_and(|after| col < *after)
            || self
                .from
                .get(&(file, line))
                .is_some_and(|runs| runs.iter().any(|&(from, end)| (from..end).contains(&col)))
    }

    pub fn is_empty(&self) -> bool {
        self.inside.is_empty() && self.last.is_empty() && self.from.is_empty()
    }
}

/// Whether the run read any source verbatim at all, asked before the
/// document's files are read from disk for a document that has none.
fn any(analysis: &Analysis) -> bool {
    analysis.facts.occurrences.iter().any(verbatim_reader)
}

/// A category code change that stops `\` from being an escape: setting it
/// to 0 (`\ref`'s sanitizing restores it) reads nothing verbatim.
fn verbatim_reader(occ: &crate::facts::Occurrence) -> bool {
    occ.kind == OccKind::Catcode && occ.key == "\\" && occ.detail.as_deref() != Some("0")
}

/// Every position the run read verbatim, built once per lint run.
pub fn map(analysis: &Analysis, sources: &Sources) -> Bodies {
    match any(analysis) {
        true => Bodies::of(&verbatims(analysis, &files(analysis, sources))),
        false => Bodies::default(),
    }
}
