//! Input decoding: a file's bytes as the characters the engine reads.
//!
//! `tex` and `pdftex` are 8-bit.  They read bytes, and a byte ≥ 128 gets its
//! category from the catcode table like any other character, 12 by default
//! (tex.web § 232).  XeTeX and LuaTeX read UTF-8 characters.  The choice is
//! made once per run, from the settled [`Engine`](crate::config::Engine),
//! and every source a [`Mouth`](crate::tex::Mouth) reads passes through
//! [`decode`] or [`recode`] first.
//!
//! A decoded source is held as a Rust `String` in which one `char` is one
//! input character: under [`Encoding::Bytes`] that is one byte, so a column
//! counts bytes, and under [`Encoding::Utf8`] it is one character.
//! [`byte_of_col`] and [`col_of_byte`] convert between such a column and a
//! byte offset in the file as it is written.

use std::borrow::Cow;
use std::cell::Cell;

use crate::config::Encoding;

thread_local! {
    static ENCODING: Cell<Encoding> = const { Cell::new(Encoding::Utf8) };
}

/// The encoding in force on this thread.
pub fn encoding() -> Encoding {
    ENCODING.with(Cell::get)
}

/// Settle the encoding for the run.  [`crate::machine::Machine::analyze`]
/// calls it once the engine is known, before any source is tokenized.
pub fn set_encoding(encoding: Encoding) {
    ENCODING.with(|cell| cell.set(encoding));
}

/// Run `f` under `encoding` and put the old one back.
pub fn with<R>(encoding: Encoding, f: impl FnOnce() -> R) -> R {
    let previous = ENCODING.with(|cell| cell.replace(encoding));
    let result = f();
    ENCODING.with(|cell| cell.set(previous));
    result
}

/// What XeTeX reads a byte no UTF-8 character starts with as.
const REPLACEMENT: char = '\u{FFFD}';

/// A file's bytes as the characters the engine reads, with the count of
/// bytes that were no UTF-8.
pub fn decode_counted(bytes: &[u8]) -> (String, usize) {
    if encoding() == Encoding::Bytes {
        return (as_bytes(bytes), 0);
    }
    let mut rest = bytes;
    let mut out = String::with_capacity(bytes.len());
    let mut invalid = 0;
    loop {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                out.push_str(text);
                return (out, invalid);
            }
            Err(error) => {
                let (good, tail) = rest.split_at(error.valid_up_to());
                out.push_str(std::str::from_utf8(good).unwrap_or_default());
                // XeTeX reads one replacement character per byte sequence it
                // cannot use, however long: `E2 80 78` gives one and then
                // `x`.  LuaTeX stops with "invalid utf-8 sequence" instead,
                // which is why the caller records a gap.
                let skip = error.error_len().unwrap_or(tail.len()).clamp(1, tail.len());
                out.push(REPLACEMENT);
                invalid += skip;
                rest = &tail[skip..];
            }
        }
    }
}

/// A file's bytes as the characters the engine reads.
pub fn decode(bytes: &[u8]) -> String {
    decode_counted(bytes).0
}

/// Text already held as UTF-8 — a document read before the encoding was
/// settled, or an editor's buffer — as the characters the engine reads.
pub fn recode(text: &str) -> Cow<'_, str> {
    match encoding() {
        Encoding::Utf8 => Cow::Borrowed(text),
        Encoding::Bytes => Cow::Owned(as_bytes(text.as_bytes())),
    }
}

/// Each byte a character of its own, as an 8-bit engine reads it; only
/// the unknown text a run wrote stays one character, so it reads back as
/// unknown.
fn as_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&b) = bytes.get(at) {
        let marker = (b == 0xF4)
            .then(|| bytes.get(at..at + 4).and_then(|s| std::str::from_utf8(s).ok()))
            .flatten()
            .and_then(|s| s.chars().next())
            .filter(|c| crate::tex::is_unknown_char(*c) || crate::tex::written_class(*c).is_some());
        match marker {
            Some(c) => {
                out.push(c);
                at += 4;
            }
            None => {
                out.push(char::from(b));
                at += 1;
            }
        }
    }
    out
}

/// The byte offset in `line`, as the file writes it, of the satex column
/// `col` (1-based).  A column past the line is its end.
pub fn byte_of_col(line: &str, col: u32) -> usize {
    let skip = col.saturating_sub(1) as usize;
    match encoding() {
        Encoding::Utf8 => line.char_indices().nth(skip).map_or(line.len(), |(at, _)| at),
        // A column inside a character names the character it is part of,
        // since a byte offset must stand where a character starts.
        Encoding::Bytes => floor(line, skip.min(line.len())),
    }
}

/// `at`, moved back to where the character it is inside starts.
fn floor(line: &str, mut at: usize) -> usize {
    while at > 0 && !line.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The satex column of a byte offset in `line`, as the file writes it.
pub fn col_of_byte(line: &str, at: usize) -> u32 {
    let at = at.min(line.len());
    match encoding() {
        Encoding::Utf8 => line[..floor(line, at)].chars().count() as u32 + 1,
        // Every byte is a column of its own, the ones inside a character too.
        Encoding::Bytes => at as u32 + 1,
    }
}

/// How many columns `text` spans, as a [`Mouth`](crate::tex::Mouth) counts
/// them: bytes for an 8-bit engine, characters otherwise.
pub fn columns(text: &str) -> usize {
    match encoding() {
        Encoding::Utf8 => text.chars().count(),
        Encoding::Bytes => text.len(),
    }
}
