//! The document as the reader sees it: [`crate::facts::Text`], the
//! characters the run typeset, indexed so a rule can move between a byte
//! offset in the string and the source position the character came from.
//!
//! A rule searches `chars()` with ordinary string methods and turns a match
//! back into positions with [`View::at`].  The typographic rules all work
//! this way, so none of them reads the interpreter.
//!
//! [`View::prose`] and its kin are the one answer to "may a rule report
//! this character", and [`Rewrite`] the one way to say what belongs there
//! instead.

use crate::fixed::HashMap;
use std::cell::OnceCell;

use crate::facts::{FontState, Spot};
use crate::lint::Severity;
use crate::lint::fix::{Applicability, Edit, Pos, Sources};
use crate::machine::Analysis;
use crate::mode::Modes;
use crate::tex::{FileId, Span};

/// The punctuation the typographic rules follow, as chktex lists it.
pub const PUNCTUATION: &str = ".,;:!?";

/// [`crate::facts::Text`] with a byte offset per character.
pub struct View<'a> {
    text: &'a crate::facts::Text,
    states: &'a [FontState],
    offsets: Vec<u32>,
    /// One per character: whether the run read it verbatim.  Empty when
    /// the run read nothing verbatim.
    verbatim: Vec<bool>,
    /// One per file: whether the author wrote it.
    document: Vec<bool>,
    /// Which character the run set at a source position, the first one
    /// where several share it.  Built the first time a rule asks.
    at_span: OnceCell<HashMap<Span, usize>>,
}

impl<'a> View<'a> {
    /// The characters a *document* set.  A package, a class or a literate
    /// source is code, not prose, so the view over one is empty and every
    /// rule reading it stays silent without having to ask.
    pub fn of(analysis: &'a Analysis, sources: &Sources) -> View<'a> {
        static NOTHING: std::sync::LazyLock<crate::facts::Text> = std::sync::LazyLock::new(crate::facts::Text::default);
        let prose = !matches!(
            analysis.settings.profile,
            Some(crate::config::Profile::Package | crate::config::Profile::Class | crate::config::Profile::Literate)
        );
        let text = match prose {
            true => &analysis.facts.text,
            false => &NOTHING,
        };
        let offsets = text.chars.char_indices().map(|(at, _)| at as u32).collect();
        let document =
            (0..analysis.files.len()).map(|f| crate::query::origin(analysis, f as FileId) == "document").collect();
        View {
            text,
            states: &analysis.font_states,
            offsets,
            verbatim: verbatim_of(text, analysis, sources),
            document,
            at_span: OnceCell::new(),
        }
    }

    /// Everything the document set, in order.
    pub fn chars(&self) -> &'a str {
        &self.text.chars
    }

    pub fn len(&self) -> usize {
        self.text.at.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.at.is_empty()
    }

    /// The character the run set at a source position.
    pub fn at_span(&self, span: Span) -> Option<usize> {
        let map = self.at_span.get_or_init(|| {
            let mut out: HashMap<Span, usize> = HashMap::with_capacity_and_hasher(self.len(), Default::default());
            for i in 0..self.len() {
                if let Some(span) = self.span(i) {
                    out.entry(span).or_insert(i);
                }
            }
            out
        });
        map.get(&span).copied()
    }

    /// The character index a byte offset into [`View::chars`] falls in.
    pub fn at(&self, byte: usize) -> usize {
        self.offsets.partition_point(|start| (*start as usize) < byte)
    }

    /// The byte offset a character index starts at.
    pub fn byte(&self, index: usize) -> usize {
        self.offsets.get(index).map_or(self.text.chars.len(), |at| *at as usize)
    }

    pub fn spot(&self, index: usize) -> Option<&'a Spot> {
        self.text.at.get(index)
    }

    pub fn span(&self, index: usize) -> Option<Span> {
        self.spot(index).map(|spot| spot.span)
    }

    /// The font the character was set in, as LaTeX's font selection held
    /// it (fntguide § 2).  `None` when the run did not know.
    pub fn font(&self, index: usize) -> Option<&'a FontState> {
        let state = self.spot(index)?.font_state;
        self.states.get(usize::from(state)).filter(|_| state != 0)
    }

    /// Whether every run sets this character, or only some path through an
    /// undecided conditional does.
    pub fn certain(&self, index: usize) -> bool {
        self.spot(index).is_some_and(|spot| spot.certain)
    }

    /// Whether the run read the character verbatim, in a verbatim body or
    /// after a `\verb` on its line, so it is program text and not prose.
    pub fn verbatim(&self, index: usize) -> bool {
        self.verbatim.get(index).copied().unwrap_or(false)
    }

    pub fn modes(&self, index: usize) -> Modes {
        self.spot(index).map_or(Modes::NONE, |spot| spot.modes)
    }

    pub fn math(&self, index: usize) -> bool {
        self.modes(index).meets(Modes::ANY_MATH)
    }

    /// Whether the run placed the character in text mode and knew which
    /// one: a mode it could not decide proves nothing.
    pub fn text_mode(&self, index: usize) -> bool {
        let modes = self.modes(index);
        modes.is_known() && !modes.is_empty() && !modes.meets(Modes::ANY_MATH)
    }

    /// Whether the author wrote the file, so a finding in it names their
    /// own source rather than a package's code.
    pub fn document(&self, file: FileId) -> bool {
        self.document.get(file as usize).copied().unwrap_or(false)
    }

    /// Whether the document itself set the character.
    pub fn is_ours(&self, index: usize) -> bool {
        self.span(index).is_some_and(|span| self.document(span.file))
    }

    /// [`View::is_ours`], and the run read it as prose rather than
    /// verbatim: the characters a typographic rule may report.
    pub fn is_prose(&self, index: usize) -> bool {
        self.is_ours(index) && !self.verbatim(index)
    }

    /// [`View::is_prose`], and outside mathematics.
    pub fn is_prose_text(&self, index: usize) -> bool {
        self.is_prose(index) && !self.math(index)
    }

    /// Every character the document set, in order.
    pub fn ours(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        (0..self.len()).filter(|i| self.is_ours(*i))
    }

    /// Every character [`View::is_prose`] holds for, in order.
    pub fn prose(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        (0..self.len()).filter(|i| self.is_prose(*i))
    }

    /// Every character [`View::is_prose_text`] holds for, in order.
    pub fn prose_text(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        (0..self.len()).filter(|i| self.is_prose_text(*i))
    }

    /// Whether TeX set this character straight after the one before, so a
    /// ligature between the two is the one the reader gets.
    pub fn joined(&self, index: usize) -> bool {
        self.spot(index).is_some_and(|spot| !spot.broken)
    }

    /// Whether the character stands right after the one before it on the
    /// page, as far as the run knows ([`crate::facts::Spot::apart`]).
    pub fn beside(&self, index: usize) -> bool {
        self.spot(index).is_some_and(|spot| !spot.apart)
    }

    pub fn char_at(&self, index: usize) -> Option<char> {
        self.text.chars[self.byte(index)..].chars().next()
    }

    /// The characters `range` covers.
    pub fn slice(&self, range: std::ops::Range<usize>) -> &'a str {
        &self.text.chars[self.byte(range.start)..self.byte(range.end)]
    }

    /// How far a run of `c` reaches from `index`, counting only characters
    /// TeX joined to the one before.
    pub fn run_of(&self, index: usize, c: char) -> usize {
        let mut end = index;
        while self.char_at(end) == Some(c) && (end == index || self.joined(end)) {
            end += 1;
        }
        end
    }

    /// The character before `index` that is not a space, with its index.
    pub fn before(&self, index: usize) -> Option<(usize, char)> {
        (0..index).rev().map(|i| (i, self.char_at(i))).find_map(|(i, c)| c.filter(|c| *c != ' ').map(|c| (i, c)))
    }

    /// The character at or after `index` that is not a space, with its index.
    pub fn after(&self, index: usize) -> Option<(usize, char)> {
        (index..self.len()).map(|i| (i, self.char_at(i))).find_map(|(i, c)| c.filter(|c| *c != ' ').map(|c| (i, c)))
    }

    /// The word ending right before `index`.
    pub fn word_before(&self, index: usize) -> &'a str {
        let mut start = index;
        while start > 0 && self.char_at(start - 1).is_some_and(char::is_alphanumeric) {
            start -= 1;
        }
        self.slice(start..index)
    }

    /// The word starting at `index`.
    pub fn word_after(&self, index: usize) -> &'a str {
        let mut end = index;
        while self.char_at(end).is_some_and(char::is_alphanumeric) {
            end += 1;
        }
        self.slice(index..end)
    }

    /// Whether `range` stands as those very characters in one line of one
    /// source file, so an edit over it says what the reader wrote.
    pub fn literal(&self, range: std::ops::Range<usize>) -> Option<(Span, Span)> {
        let (first, last) = (self.spot(range.start)?, self.spot(range.end.checked_sub(1)?)?);
        let contiguous = (range.start + 1..range.end).all(|i| self.joined(i));
        let same_line = (first.span.file, first.span.line) == (last.span.file, last.span.line);
        let width = last.span.col.checked_sub(first.span.col)? as usize == range.len() - 1;
        (contiguous && same_line && width).then_some((first.span, last.span))
    }
}

/// Which of `text`'s characters the run read verbatim, empty when none of
/// them was, so a document without verbatim pays one scan of the facts.
fn verbatim_of(text: &crate::facts::Text, analysis: &Analysis, sources: &Sources) -> Vec<bool> {
    if text.at.is_empty() {
        return Vec::new();
    }
    let bodies = crate::lint::verbatim::map(analysis, sources);
    if bodies.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<bool> =
        text.at.iter().map(|spot| bodies.holds(spot.span.file, spot.span.line, spot.span.col)).collect();
    // What an active character in the body sets (`\verb`'s `-`, made
    // `\kern\z@\char`-`) comes from the kernel's text: it is the body's
    // when the document's characters on both sides are.
    let document: Vec<bool> =
        (0..analysis.files.len()).map(|f| crate::query::origin(analysis, f as FileId) == "document").collect();
    let ours = |i: usize| document.get(text.at[i].span.file as usize).copied().unwrap_or(false);
    let mut i = 0;
    while i < out.len() {
        if ours(i) {
            i += 1;
            continue;
        }
        let end = (i..out.len()).find(|&k| ours(k)).unwrap_or(out.len());
        if i > 0 && out[i - 1] && out.get(end).copied().unwrap_or(false) {
            out[i..end].fill(true);
        }
        i = end;
    }
    out
}

/// An edit replacing the source `range` covers, when it stands there
/// literally and the file may be changed.
pub fn replace(
    edits: &crate::lint::fix::Edits,
    view: &View,
    range: std::ops::Range<usize>,
    with: &str,
) -> Option<Vec<Edit>> {
    let (first, last) = view.literal(range)?;
    let (path, _) = edits.text(first.file)?;
    let start = Pos::new(first.line, first.col);
    let end = Pos::new(last.line, last.col + 1);
    Some(vec![Edit { path, start, end, replacement: with.to_string() }])
}

/// A finding over the characters a run set, with the text that belongs
/// there instead.  [`crate::lint::Report::rewrite`] emits it.
pub struct Rewrite {
    pub range: std::ops::Range<usize>,
    pub name: String,
    pub message: String,
    /// What belongs there, when an edit can write it.
    pub want: Option<String>,
    /// The fix's own words, `write \`want\`` when none are given.
    pub fix: Option<String>,
    pub applicability: Applicability,
    /// Below the rule's own, for a check chktex keeps off by default.
    pub severity: Option<Severity>,
}

impl Rewrite {
    pub fn new(
        range: std::ops::Range<usize>,
        name: impl Into<String>,
        message: String,
        want: impl Into<String>,
    ) -> Rewrite {
        Rewrite {
            range,
            name: name.into(),
            message,
            want: Some(want.into()),
            fix: None,
            applicability: Applicability::Unsafe,
            severity: None,
        }
    }

    /// A finding the rule cannot write an edit for.
    pub fn told(
        range: std::ops::Range<usize>,
        name: impl Into<String>,
        message: String,
        fix: impl Into<String>,
    ) -> Rewrite {
        Rewrite {
            range,
            name: name.into(),
            message,
            want: None,
            fix: Some(fix.into()),
            applicability: Applicability::Unsafe,
            severity: None,
        }
    }

    pub fn fix(self, description: impl Into<String>) -> Rewrite {
        Rewrite { fix: Some(description.into()), ..self }
    }

    /// The edit keeps what the run set, so it may be applied unasked.
    pub fn safe(self) -> Rewrite {
        Rewrite { applicability: Applicability::Safe, ..self }
    }

    pub fn at(self, severity: Severity) -> Rewrite {
        Rewrite { severity: Some(severity), ..self }
    }
}
