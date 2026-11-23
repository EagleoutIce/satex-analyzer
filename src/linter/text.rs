//! The document as the reader sees it: [`crate::analysis::facts::Text`], the
//! characters the run typeset, indexed so a rule can move between a byte
//! offset in the string and the source position the character came from.
//!
//! A rule searches `chars()` with ordinary string methods and turns a match
//! back into positions with [`View::at`].  The typographic rules all work
//! this way, so none of them reads the interpreter.
//!
//! [`View::is_word_prose`] is the one answer to "may a prose rule report
//! this character", [`View::is_text`] to "does this neighbour read as
//! prose", [`View::prose_at`] the same for a source position, and
//! [`Rewrite`] the one way to say what belongs there instead.  Rules that
//! read formulas ask [`View::is_formula`], rules that measure fonts
//! [`View::is_prose`].

use crate::fixed::HashMap;
use std::cell::OnceCell;

use crate::analysis::facts::{FontState, Spot};
use crate::analysis::machine::Analysis;
use crate::engine::mode::Modes;
use crate::engine::tex::{FileId, Span};
use crate::linter::Severity;
use crate::linter::fix::{Applicability, Edit, Pos, Sources};

/// The punctuation the typographic rules follow, as chktex lists it.
pub const PUNCTUATION: &str = ".,;:!?";

/// [`crate::analysis::facts::Text`] with a byte offset per character.
pub struct View<'a> {
    text: &'a crate::analysis::facts::Text,
    states: &'a [FontState],
    offsets: Vec<u32>,
    /// One per character: whether it is program text
    /// ([`crate::linter::verbatim::code`]).  Empty when none is.
    code: Vec<bool>,
    /// The source positions [`View::code_at`] holds for.
    code_at: crate::linter::verbatim::Bodies,
    /// One per file: whether the author wrote it.
    document: Vec<bool>,
    /// `\ttdefault`: code shown as text is set in it.
    mono: Option<String>,
    /// The mode each of the document's command sites was read in.  Built
    /// the first time a rule asks.
    modes_at: OnceCell<HashMap<Span, Modes>>,
    expansions: &'a crate::fixed::Chunked<crate::analysis::facts::Expansion>,
    /// Which character the run set at a source position, the first one
    /// where several share it.  Built the first time a rule asks.
    at_span: OnceCell<HashMap<Span, usize>>,
}

impl<'a> View<'a> {
    /// The characters a *document* set.  A package, a class or a literate
    /// source is code, not prose, so the view over one is empty and every
    /// rule reading it stays silent without having to ask.
    pub fn of(analysis: &'a Analysis, sources: &Sources) -> View<'a> {
        static NOTHING: std::sync::LazyLock<crate::analysis::facts::Text> =
            std::sync::LazyLock::new(crate::analysis::facts::Text::default);
        let prose = !matches!(
            analysis.settings.profile,
            Some(
                crate::project::config::Profile::Package
                    | crate::project::config::Profile::Class
                    | crate::project::config::Profile::Literate
            )
        );
        let text = match prose {
            true => &analysis.facts.text,
            false => &NOTHING,
        };
        let offsets = text.chars.char_indices().map(|(at, _)| at as u32).collect();
        let document = (0..analysis.files.len())
            .map(|f| crate::queries::query::origin(analysis, f as FileId) == "document")
            .collect();
        let code_at = crate::linter::verbatim::code(analysis, sources);
        View {
            text,
            states: &analysis.font_states,
            offsets,
            code: code_of(text, analysis, &code_at),
            code_at,
            document,
            mono: crate::analysis::machine::macro_text(analysis, "ttdefault"),
            modes_at: OnceCell::new(),
            expansions: &analysis.facts.expansions,
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

    /// Whether the character is program text and not prose: the run read
    /// it verbatim, in a verbatim body or after a `\verb` on its line, or
    /// an undefined command or environment left it behind.
    pub fn code(&self, index: usize) -> bool {
        self.code.get(index).copied().unwrap_or(false)
    }

    /// [`View::code`] for a source position, which need not have set anything.
    pub fn code_at(&self, span: Span) -> bool {
        self.code_at.holds(span.file, span.line, span.col)
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

    /// [`View::is_ours`], and no [`View::code`]: what a rule that measures
    /// the font (an italic correction, an accent) may report, in any font
    /// and mode.
    pub fn is_prose(&self, index: usize) -> bool {
        self.is_ours(index) && !self.code(index)
    }

    /// Whether the character is set in the typewriter family, where a
    /// document shows code, keys and URLs (`\texttt`, `\url`, listings).
    pub fn typewriter(&self, index: usize) -> bool {
        self.mono.as_deref().is_some_and(|mono| self.font(index).is_some_and(|f| &*f.family == mono))
    }

    /// Whether the character reads as prose, whoever set it: not math, not
    /// code, not typewriter.  A prose rule asks it of every neighbour it
    /// decides by, so a formula or a key beside the trigger is no word.
    pub fn is_text(&self, index: usize) -> bool {
        !self.math(index) && !self.code(index) && !self.typewriter(index)
    }

    /// The document's own prose: [`View::is_ours`] and [`View::is_text`].
    /// The characters a prose rule may report.
    pub fn is_word_prose(&self, index: usize) -> bool {
        self.is_ours(index) && self.is_text(index)
    }

    /// The document's own formula: what a rule that reads math may report.
    pub fn is_formula(&self, index: usize) -> bool {
        self.is_prose(index) && self.math(index)
    }

    /// [`View::is_word_prose`] or [`View::is_formula`]: for a rule that
    /// reads both, such as three periods or `640x480`.
    pub fn is_prose_or_formula(&self, index: usize) -> bool {
        self.is_word_prose(index) || self.is_formula(index)
    }

    /// The character at `index` when it [`View::is_text`].
    pub fn text_char(&self, index: usize) -> Option<char> {
        self.char_at(index).filter(|_| self.is_text(index))
    }

    /// Whether what the run set at `span` stands in a cell of an alignment (`\\halign`, a
    /// `tabular`'s): between an alignment's begin and its end, by the run's marks.
    pub fn in_alignment(&self, span: Span) -> bool {
        use crate::analysis::facts::Align;
        // A call's own characters may carry no place (`\\ref`'s number): the first character the
        // run set from the same line at or after it stands in the same cell.
        let near =
            || (span.col..span.col.saturating_add(200)).find_map(|c| self.at_span(Span::new(span.file, span.line, c)));
        let Some(at) = self.at_span(span).or_else(near) else { return false };
        let mut depth = 0i32;
        for (_, mark) in self.text.align.iter().take_while(|(before, _)| *before as usize <= at) {
            match mark {
                Align::Begin => depth += 1,
                Align::End => depth -= 1,
                _ => {}
            }
        }
        depth > 0
    }

    /// Whether the document wrote prose at a source position: no code, and
    /// [`View::is_text`] of the character set there, else not in math by
    /// the mode the command read there was in.
    pub fn prose_at(&self, span: Span) -> bool {
        if !self.document(span.file) || self.code_at(span) {
            return false;
        }
        if let Some(i) = self.at_span(span) {
            return self.is_text(i);
        }
        let modes = self.modes_at.get_or_init(|| {
            let mine = self.expansions.iter().filter(|e| self.document(e.span.file));
            mine.map(|e| (e.span, e.mode)).collect()
        });
        !modes.get(&span).is_some_and(|m| m.is_known() && m.meets(Modes::ANY_MATH))
    }

    /// Every character the document set, in order.
    pub fn ours(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        (0..self.len()).filter(|i| self.is_ours(*i))
    }

    /// Every character [`View::is_prose`] holds for, in order.
    pub fn prose(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        (0..self.len()).filter(|i| self.is_prose(*i))
    }

    /// Every character [`View::is_word_prose`] holds for, in order.
    pub fn word_prose(&self) -> impl DoubleEndedIterator<Item = usize> + '_ {
        (0..self.len()).filter(|i| self.is_word_prose(*i))
    }

    /// Whether TeX set this character straight after the one before, so a
    /// ligature between the two is the one the reader gets.
    pub fn joined(&self, index: usize) -> bool {
        self.spot(index).is_some_and(|spot| !spot.broken)
    }

    /// Whether the character stands right after the one before it on the
    /// page, as far as the run knows ([`crate::analysis::facts::Spot::apart`]).
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

/// Which of `text`'s characters are [`View::code`], empty when none of
/// them is, so a document without any pays one scan of the facts.
fn code_of(
    text: &crate::analysis::facts::Text,
    analysis: &Analysis,
    bodies: &crate::linter::verbatim::Bodies,
) -> Vec<bool> {
    if text.at.is_empty() || bodies.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<bool> =
        text.at.iter().map(|spot| bodies.holds(spot.span.file, spot.span.line, spot.span.col)).collect();
    // What an active character in the body sets (`\verb`'s `-`, made
    // `\kern\z@\char`-`) comes from the kernel's text: it is the body's
    // when the document's characters on both sides are.
    let document: Vec<bool> =
        (0..analysis.files.len()).map(|f| crate::queries::query::origin(analysis, f as FileId) == "document").collect();
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
    edits: &crate::linter::fix::Edits,
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
/// there instead.  [`crate::linter::Report::rewrite`] emits it.
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
