//! Quotation marks, read off the characters the run set rather than off the
//! source: which mark opens, which closes, whether the two agree, and what
//! the encoding and the document language make of them.

use crate::lint::fix::{Applicability, Fix};
use crate::lint::shared::{document_start, loaded, macro_text};
use crate::lint::text::{PUNCTUATION, Rewrite, View};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;

pub static RULES: &[Rule] = &[
    Rule {
        code: "wrong-quote-character",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a character that is not a quotation mark stands in for one",
        explanation: "\
TeX sets a quotation from two backticks and two apostrophes, which the
font's ligature program pairs into the marks (The TeXbook, chapter 2).

What a straight `\"` sets depends on the encoding, so the rule reads
`\\encodingdefault`: OT1 has a right double quote there, T1 and TU the
typewriter one.  `´` is an acute accent, not the apostrophe `'`.  A prime in
math is never reported, and the rule is silent outside an English
`\\languagename`.

Fix by opening with two backticks and closing with two apostrophes, or with
`\\enquote{…}` from csquotes.",
        references: &[
            Ref { source: Source::Chktex, at: "18" },
            Ref { source: Source::Chktex, at: "19" },
            Ref { source: Source::TeXbook, at: "chapter 2" },
        ],
        fixes: &[Applicability::Unsafe],
        run: wrong_quote_character,
    },
    Rule {
        code: "quote-direction",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a quotation mark points the wrong way, or a run of them mixes both directions",
        explanation: "\
A backtick opens a quotation and an apostrophe closes it (The TeXbook, chapter 2).
A mark the other way round sets the wrong glyph, and `` `' `` sets an empty
quotation.

satex reads the direction from the characters around the mark in the typeset
text.  An apostrophe inside a word, an elision such as `'90s`, and a prime in
math are left alone, and `` `{}` `` counts as two marks because the group
stops the ligature.

Fix by turning the mark the right way round.",
        references: &[
            Ref { source: Source::Chktex, at: "32" },
            Ref { source: Source::Chktex, at: "33" },
            Ref { source: Source::Chktex, at: "34" },
            Ref { source: Source::TeXbook, at: "chapter 2" },
        ],
        fixes: &[Applicability::Unsafe],
        run: quote_direction,
    },
    Rule {
        code: "quote-ligature",
        category: Category::Style,
        severity: Severity::Info,
        summary: "three quotation marks in a row set a double and a single mark with nothing between them",
        explanation: "\
Three backticks in a row set a left double quote followed by a left single
quote, glued together (The TeXbook, chapter 2).  The reader cannot tell which
quotation is the outer one.

satex counts only marks TeX really joined, so a group between two of them
already keeps them apart and the rule says nothing.

Fix by putting a thin space `\\,` between the double mark and the single one,
with the double mark on the outside.",
        references: &[Ref { source: Source::Chktex, at: "23" }, Ref { source: Source::TeXbook, at: "chapter 2" }],
        fixes: &[Applicability::Unsafe],
        run: quote_ligature,
    },
    Rule {
        code: "quote-punctuation",
        category: Category::Style,
        severity: Severity::Info,
        summary: "punctuation stands inside the closing quotation mark",
        explanation: "\
Under the logical convention the closing mark comes directly after the
quoted words, and punctuation that belongs to the sentence stands outside
it.  A full stop written before the closing mark puts it inside the
quotation, which claims the stop was part of what was quoted.  American practice is the other way round,
which is why chktex has this check off by default and satex reports it as a
suggestion.

Fix by moving the punctuation after the closing mark.",
        references: &[Ref { source: Source::Chktex, at: "38" }],
        fixes: &[Applicability::Unsafe],
        run: quote_punctuation,
    },
    Rule {
        code: "csquotes-available",
        category: Category::Style,
        severity: Severity::Info,
        summary: "the document sets quotation marks by hand instead of loading csquotes",
        explanation: "\
`\\enquote{…}` sets the marks the current language wants, nests an inner
quotation inside an outer one, and keeps one spelling for the whole document
([csquotes](https://ctan.org/pkg/csquotes) § 3.1).

The rule fires where the run set quotation marks itself and csquotes is not
loaded.  Apostrophes inside a word and primes in math do not count.

Fix by loading `\\usepackage{csquotes}` and writing `\\enquote{…}`.",
        references: &[Ref { source: Source::Chktex, at: "18" }, Ref { source: Source::Csquotes, at: "3.1" }],
        fixes: &[Applicability::Unsafe],
        run: csquotes_available,
    },
];

/// Which quotation character a mark is made of.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Left,
    Right,
    /// A run holding both.
    Mixed,
    Straight,
    /// An acute accent, U+00B4.
    Acute,
}

/// Where the mark stands in the sentence, decided by its neighbors.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Open,
    Close,
    /// Neither: an apostrophe, an elision, a mark the run cannot place.
    Inside,
}

struct Mark {
    start: usize,
    end: usize,
    kind: Kind,
    side: Side,
    /// The width of the opening run this mark closes, when one is open.
    closes: Option<usize>,
}

impl Mark {
    fn len(&self) -> usize {
        self.end - self.start
    }
}

/// babel's names for the dialects whose quotation marks are the ones `` and
/// '' set (babel manual, `\selectlanguage`).  Every other language wants
/// different marks, so the English advice is withheld.
const ENGLISH: &[&str] =
    &["english", "american", "USenglish", "canadian", "british", "UKenglish", "australian", "newzealand"];

/// Whether the quotation marks of the language in force are the English
/// ones.  Without babel the kernel leaves `\languagename` at `english`.
fn english(analysis: &Analysis) -> bool {
    match macro_text(analysis, "languagename") {
        Some(name) => ENGLISH.iter().any(|dialect| dialect.eq_ignore_ascii_case(name.trim())),
        None => true,
    }
}

/// OT1 carries a right double quote where `"` sits, T1 and TU the
/// typewriter one (fntguide § 5).
fn quote_at_34(analysis: &Analysis) -> bool {
    macro_text(analysis, "encodingdefault").as_deref() == Some("OT1")
}

/// `'90s` and `rock 'n' roll`: an elision, not the start of a quotation.
fn elision(view: &View, start: usize, end: usize) -> bool {
    if end - start != 1 || view.char_at(start) != Some('\'') {
        return false;
    }
    match view.char_at(end) {
        Some(c) if c.is_numeric() => true,
        _ => {
            let word = view.word_after(end).chars().count();
            (1..=2).contains(&word) && view.char_at(end + word) == Some('\'')
        }
    }
}

fn side(view: &View, start: usize, end: usize) -> Side {
    if elision(view, start, end) {
        return Side::Inside;
    }
    let left = start.checked_sub(1).and_then(|i| view.char_at(i));
    let right = view.char_at(end);
    let after_gap = left.is_none_or(|c| c.is_whitespace() || "([{~".contains(c));
    let before_gap = right.is_none_or(|c| c.is_whitespace() || ")]}~".contains(c) || PUNCTUATION.contains(c));
    match (after_gap, before_gap) {
        (true, false) if right.is_some_and(char::is_alphanumeric) => Side::Open,
        (false, true) if left.is_some_and(|c| c.is_alphanumeric() || PUNCTUATION.contains(c)) => Side::Close,
        _ => Side::Inside,
    }
}

/// `the boys' toys` closes nothing: a `'` closes only where a `` ` `` run of
/// the same width is still open.  `open` holds the unmatched widths.
fn pair(open: &mut Vec<usize>, mark: &mut Mark) {
    match (mark.kind, mark.side) {
        (Kind::Left, Side::Open) => open.push(mark.len()),
        // The quotation it closes says how wide the mark should have been.
        (Kind::Left, Side::Close) => mark.closes = open.pop(),
        (Kind::Right, Side::Close) => match open.iter().rposition(|width| *width == mark.len()) {
            Some(at) => open.truncate(at),
            None => mark.side = Side::Inside,
        },
        _ => {}
    }
}

/// Every quotation character the run set, in order, runs of `` ` `` and `'`
/// gathered into one mark when TeX joined them.
fn marks(view: &View) -> Vec<Mark> {
    let mut out = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < view.len() {
        let Some(c) = view.char_at(i) else { break };
        // A quotation character the run read verbatim is program text.
        if !view.text_mode(i) || view.verbatim(i) {
            i += 1;
            continue;
        }
        let kind = match c {
            '`' | '\'' => {
                let (mut left, mut right) = (c == '`', c == '\'');
                let mut end = i + 1;
                while end < view.len() && view.joined(end) && view.text_mode(end) && !view.verbatim(end) {
                    match view.char_at(end) {
                        Some('`') => left = true,
                        Some('\'') => right = true,
                        _ => break,
                    }
                    end += 1;
                }
                let kind = match (left, right) {
                    (true, true) => Kind::Mixed,
                    (true, false) => Kind::Left,
                    _ => Kind::Right,
                };
                let mut mark = Mark { start: i, end, kind, side: side(view, i, end), closes: None };
                pair(&mut open, &mut mark);
                out.push(mark);
                i = end;
                continue;
            }
            '"' => Kind::Straight,
            '\u{b4}' => Kind::Acute,
            _ => {
                i += 1;
                continue;
            }
        };
        out.push(Mark { start: i, end: i + 1, kind, side: side(view, i, i + 1), closes: None });
        i += 1;
    }
    out
}

fn emit(report: &mut Report, findings: Vec<Rewrite>) {
    let view = report.text();
    for finding in findings {
        if view.is_ours(finding.range.start) {
            report.rewrite(finding);
        }
    }
}

/// Backticks around a replacement that is itself made of backticks read
/// worse than the bare characters.
fn shown(want: &str) -> String {
    match want.contains('`') {
        true => want.to_string(),
        false => format!("`{want}`"),
    }
}

/// How the document should spell the mark instead, given what it loaded.
fn advice(enquote: bool, want: &str) -> String {
    match enquote {
        true => format!("write {}, or \\enquote{{…}} from csquotes", shown(want)),
        false => format!("write {}", shown(want)),
    }
}

/// What a run of `n` marks of `kind` is, in words.
fn named(kind: Kind, n: usize) -> String {
    let which = match kind {
        Kind::Left => "opening",
        _ => "closing",
    };
    match n {
        1 => format!("a single {which} quote"),
        2 => format!("a double {which} quote"),
        _ => format!("{n} {which} quotes in a row"),
    }
}

fn wrong_quote_character(report: &mut Report) {
    let analysis = report.analysis;
    if !english(analysis) {
        return;
    }
    let (ot1, enquote) = (quote_at_34(analysis), loaded(analysis, "csquotes"));
    let view = report.text();
    let mut findings = Vec::new();
    for mark in marks(view) {
        let finding = match mark.kind {
            Kind::Straight => {
                let sets = match ot1 {
                    true => "a straight \" sets a right double quote in OT1",
                    false => "a straight \" sets the typewriter double quote, not a quotation mark",
                };
                let (what, want) = match mark.side {
                    Side::Open => (", and this one opens a quotation", Some("``")),
                    Side::Close => (", and this one closes a quotation", Some("''")),
                    Side::Inside => ("", None),
                };
                let message = format!("{sets}{what}");
                let advice = advice(enquote, want.unwrap_or("`` … ''"));
                match want {
                    Some(want) => Rewrite::new(mark.start..mark.end, "straight", message, want).fix(advice),
                    None => Rewrite::told(mark.start..mark.end, "straight", message, advice),
                }
            }
            // chktex 19 is off by default: an acute accent for an apostrophe.
            Kind::Acute => Rewrite::new(
                mark.start..mark.end,
                "acute",
                "´ is an acute accent (U+00B4) standing in for the apostrophe ' (ASCII 39)".into(),
                "'",
            )
            .fix("write `'`")
            .at(Severity::Info),
            _ => continue,
        };
        findings.push(finding);
    }
    emit(report, findings);
}

fn quote_direction(report: &mut Report) {
    let analysis = report.analysis;
    if !english(analysis) {
        return;
    }
    let enquote = loaded(analysis, "csquotes");
    let view = report.text();
    let mut findings = Vec::new();
    for mark in marks(view) {
        let n = mark.len();
        let finding = match (mark.kind, mark.side) {
            (Kind::Right, Side::Open) => {
                let want = "`".repeat(n);
                let message =
                    format!("{} opens a quotation, where {} belongs", named(Kind::Right, n), named(Kind::Left, n));
                let advice = advice(enquote, &want);
                Rewrite::new(mark.start..mark.end, "opening", message, want).fix(advice)
            }
            (Kind::Left, Side::Close) => {
                let width = mark.closes.unwrap_or(n);
                let want = "'".repeat(width);
                let message =
                    format!("{} closes a quotation, where {} belongs", named(Kind::Left, n), named(Kind::Right, width));
                let advice = advice(enquote, &want);
                Rewrite::new(mark.start..mark.end, "closing", message, want).fix(advice)
            }
            (Kind::Mixed, side) => {
                let want = match side {
                    Side::Open => Some("`".repeat(n)),
                    Side::Close => Some("'".repeat(n)),
                    Side::Inside => None,
                };
                let message =
                    format!("{n} marks in a row mix the opening and the closing quote, which sets an empty quotation");
                let advice = advice(enquote, want.as_deref().unwrap_or("`` … ''"));
                match want {
                    Some(want) => Rewrite::new(mark.start..mark.end, "mixed", message, want).fix(advice),
                    None => Rewrite::told(mark.start..mark.end, "mixed", message, advice),
                }
            }
            _ => continue,
        };
        findings.push(finding);
    }
    emit(report, findings);
}

fn quote_ligature(report: &mut Report) {
    let analysis = report.analysis;
    if !english(analysis) {
        return;
    }
    let view = report.text();
    let mut findings = Vec::new();
    for mark in marks(view) {
        let n = mark.len();
        if n < 3 {
            continue;
        }
        // The outer quotation is the double one, so the thin space goes
        // after the first two marks when opening and before the last two
        // when closing.
        let want = match mark.kind {
            Kind::Left => format!("``\\,{}", "`".repeat(n - 2)),
            Kind::Right => format!("{}\\,''", "'".repeat(n - 2)),
            _ => continue,
        };
        let message =
            format!("{} set a double and a single quotation mark with nothing between them", named(mark.kind, n));
        let advice = format!("write {}", shown(&want));
        findings.push(Rewrite::new(mark.start..mark.end, "ligature", message, want).fix(advice));
    }
    emit(report, findings);
}

fn quote_punctuation(report: &mut Report) {
    let analysis = report.analysis;
    if !english(analysis) {
        return;
    }
    let view = report.text();
    let mut findings = Vec::new();
    for mark in marks(view) {
        if mark.kind != Kind::Right || mark.side != Side::Close {
            continue;
        }
        let Some(at) = mark.start.checked_sub(1) else { continue };
        let Some(punctuation) = view.char_at(at).filter(|c| PUNCTUATION.contains(*c)) else { continue };
        let closing = "'".repeat(mark.len());
        let want = format!("{closing}{punctuation}");
        let message =
            format!("`{punctuation}` stands inside the quotation, where the sentence it belongs to is outside it");
        let advice = format!("write {}", shown(&want));
        findings.push(Rewrite::new(at..mark.end, "punctuation", message, want).fix(advice));
    }
    emit(report, findings);
}

fn csquotes_available(report: &mut Report) {
    let analysis = report.analysis;
    if loaded(analysis, "csquotes") {
        return;
    }
    let view = report.text();
    let quotations = marks(view)
        .into_iter()
        .filter(|mark| mark.kind != Kind::Acute && mark.side != Side::Inside)
        .filter(|mark| view.is_ours(mark.start));
    let mut count = 0;
    let mut first = None;
    for mark in quotations {
        count += 1;
        first = first.or_else(|| view.span(mark.start));
    }
    let Some(span) = first else { return };
    let language = ["babel", "polyglossia"].iter().find(|name| loaded(analysis, name));
    let follows = match language {
        Some(package) => format!(", and \\enquote sets the marks {package} asks for in the language in force"),
        None => String::new(),
    };
    let edits =
        document_start(analysis).and_then(|begin| report.edits().insert_before(begin, "\\usepackage{csquotes}"));
    report.add_fix(
        span,
        "csquotes",
        format!("the document sets {count} quotation marks by hand and csquotes is not loaded{follows}"),
        Fix::new("load \\usepackage{csquotes} and write \\enquote{…}", Applicability::Unsafe, edits),
    );
}
