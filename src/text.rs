//! Document text: positions, edits and a file's line index.

use serde_json::{Value as Json, json};

use crate::engine::tex::Span;

/// A place in a file: 1-based line, and 1-based column counted in characters,
/// as every [`Span`] counts them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

impl Pos {
    pub fn new(line: u32, col: u32) -> Pos {
        Pos { line, col }
    }

    pub(crate) fn of(span: Span) -> Pos {
        Pos::new(span.line, span.col)
    }
}

/// Replace the text from `start` up to, not including, `end`.  An insertion
/// has `start == end`, a deletion an empty `replacement`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Edit {
    pub path: String,
    pub start: Pos,
    pub end: Pos,
    pub replacement: String,
}

impl Edit {
    pub fn to_json(&self) -> Json {
        json!({
            "path": self.path,
            "start": { "line": self.start.line, "col": self.start.col },
            "end": { "line": self.end.line, "col": self.end.col },
            "replacement": self.replacement,
        })
    }

    pub fn from_json(value: &Json) -> Option<Edit> {
        let pos = |key: &str| {
            let at = value.get(key)?;
            Some(Pos::new(at.get("line")?.as_u64()? as u32, at.get("col")?.as_u64()? as u32))
        };
        Some(Edit {
            path: value.get("path")?.as_str()?.to_string(),
            start: pos("start")?,
            end: pos("end")?,
            replacement: value.get("replacement")?.as_str()?.to_string(),
        })
    }
}

/// A file's text with its line starts, for moving between [`Pos`]itions,
/// byte offsets and the UTF-16 columns editors count in.
pub struct Text {
    body: String,
    starts: Vec<usize>,
}

impl Text {
    pub fn new(body: String) -> Text {
        let mut starts = vec![0];
        starts.extend(body.match_indices('\n').map(|(i, _)| i + 1));
        Text { body, starts }
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    fn line_range(&self, line: u32) -> Option<(usize, usize)> {
        let index = line.checked_sub(1)? as usize;
        let start = *self.starts.get(index)?;
        let end = self.starts.get(index + 1).map_or(self.body.len(), |next| next - 1);
        Some((start, end))
    }

    /// The line without its end-of-line character.
    pub fn line(&self, line: u32) -> &str {
        self.line_range(line).map_or("", |(start, end)| &self.body[start..end])
    }

    /// The byte offset of a position.  One column past the last character
    /// is the end of the line, and a position on the line after the last is
    /// the end of the file.  A column counts what the engine reads, so under
    /// an 8-bit engine it counts bytes ([`crate::engine::source`]).
    pub fn offset(&self, pos: Pos) -> Option<usize> {
        let (start, end) = self.line_range(pos.line)?;
        let skip = pos.col.checked_sub(1)? as usize;
        let line = &self.body[start..end];
        match crate::engine::source::columns(line).cmp(&skip) {
            std::cmp::Ordering::Less => None,
            std::cmp::Ordering::Equal => Some(end),
            std::cmp::Ordering::Greater => Some(start + crate::engine::source::byte_of_col(line, pos.col)),
        }
    }

    pub fn pos(&self, offset: usize) -> Pos {
        let index = self.starts.partition_point(|start| *start <= offset).max(1) - 1;
        let start = self.starts[index];
        let col = crate::engine::source::col_of_byte(&self.body[start..], offset - start);
        Pos::new(index as u32 + 1, col)
    }

    pub fn end(&self) -> Pos {
        self.pos(self.body.len())
    }

    pub fn slice(&self, start: Pos, end: Pos) -> Option<&str> {
        let (a, b) = (self.offset(start)?, self.offset(end)?);
        (a <= b).then(|| &self.body[a..b])
    }

    /// The position as the Language Server Protocol counts it: 0-based line,
    /// and the column in UTF-16 code units.
    pub fn utf16(&self, pos: Pos) -> Option<(u32, u32)> {
        let (start, _) = self.line_range(pos.line)?;
        let offset = self.offset(pos)?;
        Some((pos.line - 1, self.body[start..offset].encode_utf16().count() as u32))
    }

    /// The satex column of an LSP column (UTF-16 code units) on `line`.
    pub fn from_utf16(&self, line: u32, utf16: u32) -> u32 {
        let text = self.line(line);
        let mut units = 0u32;
        for (at, c) in text.char_indices() {
            if units >= utf16 {
                return crate::engine::source::col_of_byte(text, at);
            }
            units += c.len_utf16() as u32;
        }
        crate::engine::source::col_of_byte(text, text.len())
    }
}
