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
    /// The body as the engine reads it, recoded once per encoding, with each line's first character.
    chars: std::cell::RefCell<Option<Recoded>>,
}

/// What one input character is: one byte, or one UTF-8 character.
/// `Engine::encoding` picks it, and `crate::engine::source` holds the choice for
/// the run.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Encoding {
    /// Every byte is its own character, code 0..=255.
    Bytes,
    #[default]
    Utf8,
}

type Recoded = (Encoding, std::rc::Rc<[char]>, Vec<usize>);

impl Text {
    pub fn new(body: String) -> Text {
        let mut starts = vec![0];
        starts.extend(body.match_indices('\n').map(|(i, _)| i + 1));
        Text { body, starts, chars: Default::default() }
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

    /// The line as the engine reads it, one `char` per column: under an
    /// 8-bit engine each byte ([`crate::engine::source`]).  Index it with
    /// a [`Pos`]'s column, never [`Self::line`]'s characters.
    pub fn columns(&self, line: u32) -> Vec<char> {
        crate::engine::source::recode(self.line(line)).chars().collect()
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

    /// The characters the engine reads, shared, and the index of `pos` in them:
    /// what [`crate::engine::source::recode`] of the body from `pos` on reads.
    /// `None` on a line that holds a run's marker, which a cut could split.
    pub fn chars_at(&self, pos: Pos) -> Option<(std::rc::Rc<[char]>, usize)> {
        let (start, offset) = (*self.starts.get(pos.line.checked_sub(1)? as usize)?, self.offset(pos)?);
        let encoding = crate::engine::source::encoding();
        let bytes = encoding == Encoding::Bytes;
        if bytes && self.line(pos.line).as_bytes().contains(&0xF4) {
            return None;
        }
        let mut cache = self.chars.borrow_mut();
        if cache.as_ref().is_none_or(|c| c.0 != encoding) {
            let chars: std::rc::Rc<[char]> = crate::engine::source::recode(&self.body).chars().collect();
            let mut firsts = vec![0];
            firsts.extend(chars.iter().enumerate().filter(|c| *c.1 == '\n').map(|(i, _)| i + 1));
            *cache = Some((encoding, chars, firsts));
        }
        let (_, chars, firsts) = cache.as_ref()?;
        let into = &self.body[start..offset];
        let at = firsts[pos.line as usize - 1] + if bytes { into.len() } else { into.chars().count() };
        Some((chars.clone(), at))
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

    /// The column of `pos` (1-based) in Unicode code points, as editors,
    /// GitHub and SARIF's `unicodeCodePoints` count it: under an 8-bit
    /// engine a satex column counts bytes.
    pub fn code_point_col(&self, pos: Pos) -> Option<u32> {
        let (start, _) = self.line_range(pos.line)?;
        Some(self.body[start..self.offset(pos)?].chars().count() as u32 + 1)
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
