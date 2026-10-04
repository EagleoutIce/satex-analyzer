//! Hyphen, en-dash and em-dash, read off the characters the run set rather
//! than off the source: a dash a macro produced is checked like any other.

use std::collections::HashSet;

use crate::config::Engine;
use crate::lint::text::{Rewrite, View};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};

pub static RULES: &[Rule] = &[Rule {
    code: "wrong-dash",
    category: Category::Style,
    severity: Severity::Warning,
    summary: "a run of hyphens sets a dash of the wrong length for what stands around it",
    explanation: "\
TeX's text fonts turn `-` into a hyphen, `--` into an en-dash and `---` into
an em-dash ([The TeXbook](https://ctan.org/pkg/texbook), chapter 2).

The rule reads the characters the document set, following chktex's counts:
one or three hyphens between letters, two between digits, three between
spaces.  `lints.wrong-dash` in `satex.yaml` changes them and its `except`
list names spellings to leave alone.  A source scan misses that `-{}-` sets
two hyphens, the group having broken the ligature, and that every `-` in
math is a minus sign.

Fix by writing the run of hyphens the surrounding text calls for.",
    references: &[Ref { source: Source::Chktex, at: "8" }, Ref { source: Source::TeXbook, at: "chapter 2" }],
    run: wrong_dash,
}];

/// What stands on both sides of a run of hyphens, which decides how long
/// the dash should be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Between {
    Letters,
    Digits,
    Spaces,
    Other,
}

impl Between {
    fn of(left: Option<char>, right: Option<char>) -> Between {
        match (left, right) {
            (Some(l), Some(r)) if l.is_alphabetic() && r.is_alphabetic() => Between::Letters,
            (Some(l), Some(r)) if l.is_numeric() && r.is_numeric() => Between::Digits,
            (Some(' '), Some(' ')) => Between::Spaces,
            _ => Between::Other,
        }
    }

    fn what(self) -> &'static str {
        match self {
            Between::Letters => "between letters",
            Between::Digits => "between numbers",
            Between::Spaces => "between words",
            Between::Other => "here",
        }
    }
}

/// `one hyphen`, `2 hyphens`.
fn hyphens(n: usize) -> String {
    match n {
        1 => "one hyphen".to_string(),
        _ => format!("{n} hyphens"),
    }
}

fn dash(n: usize) -> &'static str {
    match n {
        1 => "a hyphen",
        2 => "an en-dash",
        3 => "an em-dash",
        _ => "a row of hyphens",
    }
}

fn wrong_dash(report: &mut Report) {
    let analysis = report.analysis;
    let settings = &analysis.settings.lints.wrong_dash;
    let unicode = matches!(analysis.plugins.engine, Engine::XeTeX | Engine::LuaTeX);
    let view = report.text();
    // Where a number with a unit stands, so a lone hyphen beside one can
    // be named for what it is.
    let quantities: HashSet<crate::tex::Span> = analysis.facts.quantities.iter().map(|q| q.span).collect();
    let siunitx = crate::lint::shared::loaded(analysis, "siunitx");
    let mut findings = Vec::new();
    let mut i = 0;
    while i < view.len() {
        // A hyphen the run read verbatim is program text, not a dash.
        if view.verbatim(i) {
            i += 1;
            continue;
        }
        match view.char_at(i) {
            Some('-') => {
                let end = view.run_of(i, '-');
                findings.extend(run(view, settings, &quantities, siunitx, i, end));
                i = end;
            }
            Some(c @ ('\u{2013}' | '\u{2014}')) if !unicode => {
                let spelling = match c {
                    '\u{2013}' => "--",
                    _ => "---",
                };
                let message = format!(
                    "{} is set from the character itself, which only some encodings carry",
                    dash(spelling.len())
                );
                findings.push(Rewrite::new(i..i + 1, "unicode", message, spelling));
                i += 1;
            }
            _ => i += 1,
        }
    }
    for finding in findings {
        report.rewrite(finding);
    }
}

/// Whether a number stands on either side of the run, so the hyphen is a
/// minus rather than a dash.
fn numeric_around(view: &View, start: usize, end: usize) -> bool {
    let numeric = |word: &str| word.chars().any(char::is_numeric);
    let left = view.before(start).map(|(i, _)| view.word_before(i + 1)).unwrap_or_default();
    let right = view.after(end).map(|(i, _)| view.word_after(i)).unwrap_or_default();
    numeric(left) && numeric(right)
}

/// Whether the run really has a quantity on both sides.
fn quantity_around(view: &View, quantities: &HashSet<crate::tex::Span>, start: usize, end: usize) -> bool {
    let holds = |range: std::ops::Range<usize>| range.filter_map(|i| view.span(i)).any(|s| quantities.contains(&s));
    let left = view.before(start).is_some_and(|(i, _)| {
        let word = view.word_before(i + 1);
        holds(i + 1 - word.chars().count()..i + 1)
    });
    let right = view.after(end).is_some_and(|(i, _)| {
        let word = view.word_after(i);
        holds(i..i + word.chars().count())
    });
    left && right
}

/// The finding a run of hyphens from `start` to `end` deserves, if any.
fn run(
    view: &View,
    settings: &crate::config::WrongDash,
    quantities: &HashSet<crate::tex::Span>,
    siunitx: bool,
    start: usize,
    end: usize,
) -> Option<Rewrite> {
    let n = end - start;
    let left = start.checked_sub(1).and_then(|i| view.char_at(i));
    let right = view.char_at(end);
    // Every `-` in math mode is a minus sign, and no ligature joins two.
    if view.math(start) {
        let message = format!("{} in math mode set {n} minus signs, not {}", hyphens(n), dash(n));
        return (n > 1).then(|| Rewrite::new(start..end, "math", message, "-".repeat(n.min(1))));
    }
    let between = Between::of(left, right);
    // `2ms - 3s` reads as a subtraction, and a minus sign is a math
    // character (tex.web § 1046), not an em-dash.
    if between == Between::Spaces && n == 1 && numeric_around(view, start, end) {
        let both = quantity_around(view, quantities, start, end);
        let more = match (both, siunitx) {
            (true, true) => ", or write the whole range with siunitx's \\qtyrange",
            (true, false) => ", and siunitx sets a range of quantities with \\qtyrange",
            _ => "",
        };
        let message = format!("a hyphen between numbers is read as a minus sign, which is set in math mode{more}");
        return Some(Rewrite::new(start..end, "minus", message, "\\(-\\)"));
    }
    // Digits joined by more than one run of hyphens, or with letters or
    // slashes in the same word, are an identifier (an ISBN, a DOI, an ISO
    // date), whose hyphens are part of it: a range has one dash.
    if between == Between::Digits && identifier(view, start, end) {
        return None;
    }
    let allowed: &[usize] = match between {
        Between::Letters => &settings.letters,
        Between::Digits => &settings.digits,
        Between::Spaces => &settings.words,
        Between::Other => return None,
    };
    if allowed.contains(&n) {
        return None;
    }
    let spelling = format!("{}{}{}", view.word_before(start), "-".repeat(n), view.word_after(end));
    if settings.except.iter().any(|keep| keep == &spelling) {
        return None;
    }
    let want = "-".repeat(*allowed.first()?);
    let message = format!(
        "{} {} {} {}, where {} belongs",
        hyphens(n),
        if n == 1 { "sets" } else { "set" },
        dash(n),
        between.what(),
        dash(want.len())
    );
    Some(Rewrite::new(start..end, between.what(), message, want))
}

/// Whether the word around the hyphens `start..end` is an identifier:
/// another run of hyphens, a letter or a `/` stands in it.
fn identifier(view: &View, start: usize, end: usize) -> bool {
    let part = |c: char| !c.is_whitespace() && !matches!(c, ',' | ';' | '(' | ')' | '[' | ']');
    let odd = |c: char| c == '-' || c == '/' || c.is_alphabetic();
    let mut i = start;
    while let Some(c) = i.checked_sub(1).and_then(|j| view.char_at(j)).filter(|&c| part(c)) {
        if odd(c) {
            return true;
        }
        i -= 1;
    }
    let mut i = end;
    while let Some(c) = view.char_at(i).filter(|&c| part(c)) {
        if odd(c) {
            return true;
        }
        i += 1;
    }
    false
}
