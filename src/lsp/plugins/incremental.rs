//! Incremental plugins: whether the last run still holds after an edit, so
//! `satex lsp` answers from it instead of running again. Each decides from
//! what the run read (`Facts::lines`, its expansions and definitions), never
//! from the text alone; the text only says where the edit is. Whatever none
//! of them takes is analyzed again.

use super::TypePlugin;
use crate::analysis::machine::Analysis;
use crate::engine::tex::{DIRECTIVE, FileId, LineRead};
use crate::project::config::TypePlugins;

/// One edit of a document against the text its last run read: the one
/// replaced stretch that both texts differ in, on line `line`.
pub(in crate::lsp) struct Edit<'a> {
    pub analysis: &'a Analysis,
    pub old: &'a str,
    pub new: &'a str,
    pub file: FileId,
    /// The line (from 1) and the satex column of the first changed character.
    pub line: u32,
    pub col: u32,
    /// Byte offsets: where the stretch starts, and where it ends in each text.
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

/// What a plugin's yes means for the run's results.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::lsp) enum Reuse {
    /// The run holds, and every position outside the edit is where it was:
    /// the rules run again on the new text.
    Fresh,
    /// The run holds with the columns after the edit on its line moved: the
    /// rules run on the text the run read, their positions are moved.
    Shifted,
    /// The run may not hold: its results stand, moved, until typing pauses.
    Provisional,
}

impl<'a> Edit<'a> {
    /// The edit from `old` to `new` in the file of `path`, the stretch put as
    /// far left as the texts allow. `None` when the texts are the same or the
    /// run did not read the file.
    pub fn new(analysis: &'a Analysis, path: &str, old: &'a str, new: &'a str) -> Option<Self> {
        if old == new {
            return None;
        }
        let file = super::super::type_at::file_of(analysis, path)?;
        let (a, b) = (old.as_bytes(), new.as_bytes());
        let mut suffix = a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count();
        let mut start = a.iter().zip(b).take(a.len().min(b.len()) - suffix).take_while(|(x, y)| x == y).count();
        while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
            start -= 1;
        }
        while !old.is_char_boundary(a.len() - suffix) || !new.is_char_boundary(b.len() - suffix) {
            suffix -= 1;
        }
        let line_start = old[..start].rfind('\n').map_or(0, |i| i + 1);
        let line = old[..start].matches('\n').count() as u32 + 1;
        let col = crate::engine::source::with(analysis.plugins.engine.encoding(), || {
            crate::engine::source::col_of_byte(&old[line_start..], start - line_start)
        });
        Some(Edit { analysis, old, new, file, line, col, start, old_end: a.len() - suffix, new_end: b.len() - suffix })
    }

    pub fn removed(&self) -> &str {
        &self.old[self.start..self.old_end]
    }

    pub fn inserted(&self) -> &str {
        &self.new[self.start..self.new_end]
    }

    /// Whether neither side of the edit ends a line.
    fn in_line(&self) -> bool {
        !self.removed().contains(['\n', '\r']) && !self.inserted().contains(['\n', '\r'])
    }

    /// The edited line, in the old text and the new.
    fn lines(&self) -> (&str, &str) {
        let from = self.old[..self.start].rfind('\n').map_or(0, |i| i + 1);
        let end = |t: &'a str, at: usize| t[at..].find('\n').map_or(t.len(), |i| at + i);
        (&self.old[from..end(self.old, self.old_end)], &self.new[from..end(self.new, self.new_end)])
    }

    /// How every read of line `line` went, when the run read it to its end.
    fn read(&self, line: u32) -> Option<&'a LineRead> {
        self.analysis.facts.lines.get(&(self.file, line))
    }

    /// The edit's offsets in LSP terms: its line (from 0), the UTF-16
    /// column it starts at and its old and new end columns.
    pub fn shift(&self) -> Shift {
        let from = self.old[..self.start].rfind('\n').map_or(0, |i| i + 1);
        let units = |s: &str| s.encode_utf16().count() as u32;
        let at = units(&self.old[from..self.start]);
        Shift { line: self.line - 1, at, old_end: at + units(self.removed()), new_end: at + units(self.inserted()) }
    }
}

/// Where an edit moved the rest of its line.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub(in crate::lsp) struct Shift {
    pub line: u32,
    pub at: u32,
    pub old_end: u32,
    pub new_end: u32,
}

impl Shift {
    /// A position of the old text in the new: one inside the stretch goes to its start.
    pub fn moved(&self, line: u32, character: u32) -> u32 {
        match () {
            _ if line != self.line || character <= self.at => character,
            _ if character >= self.old_end => character - self.old_end + self.new_end,
            _ => self.at,
        }
    }

    /// Moves every LSP position (`{line, character}`) in `value`.
    pub fn apply(&self, value: &mut serde_json::Value) {
        use serde_json::Value;
        match value {
            Value::Object(map) => {
                if let (Some(line), Some(Value::Number(c))) =
                    (map.get("line").and_then(Value::as_u64), map.get("character"))
                    && let Some(c) = c.as_u64()
                {
                    let moved = self.moved(line as u32, c as u32);
                    map.insert("character".into(), moved.into());
                    return;
                }
                map.values_mut().for_each(|v| self.apply(v));
            }
            Value::Array(items) => items.iter_mut().for_each(|v| self.apply(v)),
            _ => {}
        }
    }
}

/// `type_plugins.comment_edit`: an edit inside a comment.
pub(super) struct CommentEdit;

impl TypePlugin for CommentEdit {
    fn name(&self) -> &'static str {
        "comment_edit"
    }
    fn about(&self) -> &'static str {
        "an edit inside text every read of its line skipped as a comment, no \\satex@ directive or magic comment"
    }
    fn category(&self) -> &'static str {
        "incremental"
    }
    fn types(&self) -> &'static [crate::analysis::roles::Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["reuse"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.comment_edit.enabled
    }
    /// Sound because the mouth drops a comment's characters unread (tex.web
    /// § 347). The edit lies after the comment character of every read of its
    /// line, in one line, so no read tokenized it. The two readers of a
    /// comment's text that change a run, `%\satex@` directives and magic
    /// comments, must be the same before and after.
    fn reuse(&self, e: &Edit) -> Option<Reuse> {
        let comment = e.read(e.line)?.comment?;
        if !e.in_line() || e.col <= comment {
            return None;
        }
        let (old, new) = e.lines();
        let directive = |line: &str| {
            let at = crate::engine::source::with(e.analysis.plugins.engine.encoding(), || {
                crate::engine::source::byte_of_col(line, comment)
            });
            line.get(at + 1..).is_some_and(|body| body.trim_start_matches([' ', '\t']).starts_with(DIRECTIVE))
        };
        if directive(old) || directive(new) {
            return None;
        }
        let magic = |text: &str| format!("{:?}", crate::plugins::magic::scan_with(text, &e.analysis.settings.magic));
        (magic(e.old) == magic(e.new)).then_some(Reuse::Fresh)
    }
}

/// `type_plugins.whitespace_edit`: spaces the run skipped, and lines it never read.
pub(super) struct WhitespaceEdit;

impl TypePlugin for WhitespaceEdit {
    fn name(&self) -> &'static str {
        "whitespace_edit"
    }
    fn about(&self) -> &'static str {
        "spaces added beside a space every read skipped, and white space past the last line the run read"
    }
    fn category(&self) -> &'static str {
        "incremental"
    }
    fn types(&self) -> &'static [crate::analysis::roles::Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["reuse"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.whitespace_edit.enabled
    }
    /// Two cases. Copies of a space character put into a run of it where
    /// every read skipped one of the run in state N or S (tex.web § 347). The
    /// copies are read at the moment that one was, under the same categories,
    /// so they are skipped too and the state stays. No `\par` can come of it:
    /// no line end is added. Or white space where no read got to: the line
    /// before the edit was never read to its end, the run did not stop at a
    /// limit, so nothing at or after the edit was ever read.
    fn reuse(&self, e: &Edit) -> Option<Reuse> {
        if !e.removed().is_empty() || !e.inserted().chars().all(char::is_whitespace) {
            return None;
        }
        let read_past = e.analysis.facts.lines.keys().any(|&(f, l)| f == e.file && l + 1 >= e.line);
        if !read_past && !e.analysis.exhausted && e.analysis.limits.is_empty() {
            return Some(Reuse::Fresh);
        }
        let space = e.inserted().chars().next()?;
        let patterns = &e.analysis.settings.lints.user_pattern;
        if !e.in_line() || !matches!(space, ' ' | '\t') || e.inserted().chars().any(|c| c != space) {
            return None;
        }
        // A user pattern may match spaces: the rules must see the new text.
        if !patterns.patterns.is_empty() || !patterns.regexes.is_empty() {
            return None;
        }
        let skipped = e.read(e.line)?.skipped;
        let run = e.old[e.start..].chars().take_while(|&c| c == space).count() as u32;
        (e.col..e.col + run).any(|col| col <= 128 && skipped & (1 << (col - 1)) != 0).then_some(Reuse::Shifted)
    }
}

/// `type_plugins.typing`: a control word being typed that the run had undefined.
pub(super) struct Typing;

impl TypePlugin for Typing {
    fn name(&self) -> &'static str {
        "typing"
    }
    fn about(&self) -> &'static str {
        "letters typed into a control word the run met undefined: the last results stand until typing pauses"
    }
    fn category(&self) -> &'static str {
        "incremental"
    }
    fn types(&self) -> &'static [crate::analysis::roles::Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["reuse"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.typing.enabled
    }
    /// Provisional, never claimed sound: the run met the old name undefined
    /// at this place on every path and nothing defines the new one. The
    /// results most likely stand. The run follows once typing pauses.
    fn reuse(&self, e: &Edit) -> Option<Reuse> {
        let letters = |s: &str| s.chars().all(|c| c.is_ascii_alphabetic());
        if !e.in_line() || !letters(e.removed()) || !letters(e.inserted()) {
            return None;
        }
        let (old, new) = e.lines();
        let line_at = e.start - e.old[..e.start].rfind('\n').map_or(0, |i| i + 1);
        let word = old[..line_at].trim_end_matches(|c: char| c.is_ascii_alphabetic());
        let slash = word.len().checked_sub(1).filter(|&i| word[i..].starts_with('\\'))?;
        let name = |line: &str| -> String { line[slash + 1..].chars().take_while(char::is_ascii_alphabetic).collect() };
        let (was, now) = (name(old), name(new));
        if now.is_empty() || was == now {
            return None;
        }
        let a = e.analysis;
        let col =
            crate::engine::source::with(a.plugins.engine.encoding(), || crate::engine::source::col_of_byte(old, slash));
        let at = crate::engine::tex::Span::new(e.file, e.line, col);
        let undefined = |m: crate::analysis::facts::MeaningKind| {
            matches!(
                m,
                crate::analysis::facts::MeaningKind::Undefined
                    | crate::analysis::facts::MeaningKind::UndefinedApproximated
            )
        };
        let met: Vec<_> = a.facts.expansions.iter().filter(|x| x.span == at).collect();
        if met.is_empty() || !met.iter().all(|x| undefined(x.meaning)) {
            return None;
        }
        let defined = a
            .interner
            .lookup(&now)
            .is_some_and(|sym| a.env.is_defined(sym) || a.facts.defs.iter().any(|d| d.name == sym));
        (!defined).then_some(Reuse::Provisional)
    }
}

/// The first enabled incremental plugin that takes `edit`, with its name.
pub(in crate::lsp) fn decide(edit: &Edit) -> Option<(&'static str, Reuse)> {
    let cfg = &edit.analysis.settings.type_plugins;
    (super::PLUGINS.iter())
        .filter(|p| p.category() == "incremental" && p.enabled(cfg))
        .find_map(|p| p.reuse(edit).map(|r| (p.name(), r)))
}

/// How often each plugin's reuse answered, for `satex/caches` and the log.
static COUNTS: std::sync::Mutex<std::collections::BTreeMap<&'static str, u64>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

pub(in crate::lsp) fn count(name: &'static str) {
    if let Ok(mut counts) = COUNTS.lock() {
        *counts.entry(name).or_default() += 1;
    }
}

/// `reused <plugin> <times>` lines.
pub(in crate::lsp) fn counts() -> Vec<String> {
    COUNTS.lock().map(|c| c.iter().map(|(n, k)| format!("reused {n} {k}")).collect()).unwrap_or_default()
}
