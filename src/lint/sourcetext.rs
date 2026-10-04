//! The chktex warnings about the source a reader writes rather than about
//! what the document sets.  Each rule still checks itself against the run:
//! a bracket in math is not structure, a pattern in a comment is not text,
//! and a verbatim tail is reported only where the run set none of it.

use std::collections::{HashMap, HashSet};

use regex::Regex;

use crate::config::{CommentMarker, UserPattern};
use crate::lint::fix::{Applicability, Edit, Fix, Pos, Scan};
use crate::lint::text::View;
use crate::lint::verbatim::{Bodies, files, verbatims};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::tex::{Catcode, CatcodeTable, FileId, Span, Tok};

pub static RULES: &[Rule] = &[
    Rule {
        code: "unbalanced-brackets",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a file sets more of one bracket than of its partner",
        explanation: "\
TeX gives `(`, `)`, `[` and `]` no meaning of their own, but a document uses
them for structure, so a count that does not match lost a bracket
(chktex 17).

This rule counts the brackets the run really set, so one a macro produced
counts and one a macro read as an argument does not.  A bracket in math is a
delimiter, and one read verbatim is program text.  The finding points at the
first bracket left over.

Fix by adding the bracket that is missing.",
        references: &[Ref { source: Source::Chktex, at: "17" }, Ref { source: Source::TeXbook, at: "chapter 2" }],
        run: unbalanced_brackets,
    },
    Rule {
        code: "user-pattern",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "the source matches a pattern the project asked to be warned about",
        explanation: "\
chktex keeps two lists of its own in `chktexrc`: `UserWarn`, literal strings
to report (warning 20), and `UserWarnRegex`, regular expressions
(warning 44).

satex reads both from `lints.user-pattern` in `satex.yaml`, and both are
empty by default.  A regex is written as
[the `regex` crate](https://docs.rs/regex) reads it, and one that does not
compile is skipped.  A match inside a comment or a verbatim body is not
reported.

Fix by writing what the project's own rule asks for, or by removing the
pattern from `satex.yaml`.",
        references: &[Ref { source: Source::Chktex, at: "20" }, Ref { source: Source::Chktex, at: "44" }],
        run: user_pattern,
    },
    Rule {
        code: "unintended-escape",
        category: Category::Correctness,
        severity: Severity::Info,
        summary: "a command is followed by an escaped punctuation mark, which is another command",
        explanation: "\
`\\TeX\\. Right?` does not set `TeX. Right?`.  `\\.` is the dot accent, so it
takes the `R` and sets `TeXṘight?` (chktex 21).

The rule looks for a control word followed with nothing between by `\\.`, `\\,`,
`\\;`, `\\:`, `\\!` or `\\?`, where a space and a word follow.  It stays quiet in
math mode, where `\\,` is a thin space, inside a body the run read verbatim,
and where the run really did set that mark there.

Fix by removing the backslash.",
        references: &[Ref { source: Source::Chktex, at: "21" }, Ref { source: Source::TeXbook, at: "chapter 9" }],
        run: unintended_escape,
    },
    Rule {
        code: "comment-marker",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a comment still carries a TODO-style marker",
        explanation: "\
chktex's warning 22 prints every comment it finds, which is noise in a
document of any size.

This rule reports only the comments carrying a marker that says the note is
unfinished work.  The defaults are `TODO`, `FIXME`, `XXX` and `HACK`, matched
as whole words and case-sensitively, and `lints.comment-marker` in
`satex.yaml` sets its own list.  A `%` inside a body the run read verbatim is
a character, not a comment.

Fix by doing what the note asks and deleting it.",
        references: &[Ref { source: Source::Chktex, at: "22" }],
        run: comment_marker,
    },
    Rule {
        code: "ignored-verbatim-tail",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "text stands on the line that closes a verbatim body and never reaches the page",
        explanation: "\
A verbatim environment that reads its body line by line throws away whatever
follows `\\end{…}` on the same line.  `fancyvrb` does this and the kernel's
`verbatim` does not, so chktex reports warning 31 for both.

satex reports only where the text was really lost.  It knows which
environments the run read verbatim, because opening one changed the category
code of `\\`, and reports the tail only where the run set none of it.

Fix by moving the text to the next line.",
        references: &[Ref { source: Source::Chktex, at: "31" }],
        run: ignored_verbatim_tail,
    },
];

fn user_pattern_settings(analysis: &Analysis) -> UserPattern {
    analysis.settings.lints.user_pattern.clone()
}

fn comment_marker_settings(analysis: &Analysis) -> CommentMarker {
    analysis.settings.lints.comment_marker.clone()
}

/// Every character the run set, by file and line, with its column and its
/// index in the view, columns ascending.
fn by_line(view: &View) -> HashMap<(FileId, u32), Vec<(u32, usize)>> {
    let mut out: HashMap<(FileId, u32), Vec<(u32, usize)>> = HashMap::new();
    for i in 0..view.len() {
        if let Some(span) = view.span(i) {
            out.entry((span.file, span.line)).or_default().push((span.col, i));
        }
    }
    for cols in out.values_mut() {
        cols.sort_unstable();
    }
    out
}

/// The byte offset a 1-based column starts at.
fn byte_of(line: &str, col: u32) -> usize {
    line.char_indices().nth(col as usize - 1).map_or(line.len(), |(at, _)| at)
}

/// The 1-based column a byte offset falls in.
fn col_of(line: &str, byte: usize) -> u32 {
    line[..byte.min(line.len())].chars().count() as u32 + 1
}

// ---- chktex 17 -------------------------------------------------------------

/// The pairs TeX gives no meaning to that a document still uses for
/// structure, as chktex counts them for warning 17.
const PAIRS: [(char, char); 2] = [('(', ')'), ('[', ']')];

fn unbalanced_brackets(report: &mut Report) {
    let analysis = report.analysis;
    let view = report.text();
    let files = files(report.analysis, report.edits().sources);
    let diagnosed: HashSet<(FileId, u32)> =
        analysis.facts.diagnostics.iter().map(|d| (d.span.file, d.span.line)).collect();
    let mut counts: HashMap<FileId, [[usize; 2]; 2]> = HashMap::new();
    let mut opened: HashMap<FileId, [Vec<Span>; 2]> = HashMap::new();
    let mut surplus: HashMap<FileId, [Option<Span>; 2]> = HashMap::new();
    // A bracket the run could not place proves nothing, and neither does
    // any count in its file: its partner may be the one that was placed.
    let mut unplaced: HashSet<FileId> = HashSet::new();
    for i in view.prose_text() {
        let Some(c) = view.char_at(i) else { continue };
        let Some(pair) = PAIRS.iter().position(|(open, close)| *open == c || *close == c) else { continue };
        let Some(span) = view.span(i) else { continue };
        if !view.modes(i).is_known() {
            unplaced.insert(span.file);
            continue;
        }
        let count = counts.entry(span.file).or_default();
        let stack = opened.entry(span.file).or_default();
        if c == PAIRS[pair].0 {
            count[pair][0] += 1;
            stack[pair].push(span);
        } else {
            count[pair][1] += 1;
            if stack[pair].pop().is_none() {
                let first = surplus.entry(span.file).or_default();
                first[pair].get_or_insert(span);
            }
        }
    }
    for file in files.iter().filter(|file| !unplaced.contains(&file.id)) {
        for (pair, (open, close)) in PAIRS.iter().enumerate() {
            let [opens, closes] = counts.get(&file.id).map_or([0, 0], |count| count[pair]);
            if opens == closes {
                continue;
            }
            let at = opened
                .get(&file.id)
                .and_then(|stack| stack[pair].first().copied())
                .or_else(|| surplus.get(&file.id).and_then(|first| first[pair]));
            let Some(at) = at else { continue };
            if diagnosed.contains(&(at.file, at.line)) {
                continue;
            }
            let missing = if opens > closes { close } else { open };
            report.add(
                at,
                &format!("{open}{close}"),
                format!("the file set {opens} `{open}` and {closes} `{close}`"),
                Some(format!("add the `{missing}` that is missing")),
            );
        }
    }
}

// ---- chktex 20 and 44 ------------------------------------------------------

/// What a match came from: a literal string or a regular expression.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Pattern,
    Regex,
}

/// One match in a source line: where it starts, what stood there, and the
/// list entry that found it.
#[derive(Debug)]
struct Match {
    at: usize,
    text: String,
    entry: String,
    kind: Kind,
}

/// The two lists of [`UserPattern`], with the regexes compiled.
struct Patterns {
    patterns: Vec<String>,
    regexes: Vec<(String, Regex)>,
}

impl Patterns {
    fn of(settings: &UserPattern) -> Patterns {
        Patterns {
            patterns: settings.patterns.iter().filter(|p| !p.is_empty()).cloned().collect(),
            regexes: settings
                .regexes
                .iter()
                .filter_map(|source| Regex::new(source).ok().map(|regex| (source.clone(), regex)))
                .collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.patterns.is_empty() && self.regexes.is_empty()
    }

    /// Every match in one source line, outside whatever comment it carries.
    fn find(&self, line: &str) -> Vec<Match> {
        let code = &line[..crate::tex::comment_start(line).unwrap_or(line.len())];
        let mut out = Vec::new();
        for pattern in &self.patterns {
            let found = code.match_indices(pattern.as_str());
            out.extend(found.map(|(at, text)| Match {
                at,
                text: text.to_string(),
                entry: pattern.clone(),
                kind: Kind::Pattern,
            }));
        }
        for (source, regex) in &self.regexes {
            let found = regex.find_iter(code);
            out.extend(found.map(|m| Match {
                at: m.start(),
                text: m.as_str().to_string(),
                entry: source.clone(),
                kind: Kind::Regex,
            }));
        }
        out.sort_by_key(|m| m.at);
        out
    }
}

fn user_pattern(report: &mut Report) {
    let patterns = Patterns::of(&user_pattern_settings(report.analysis));
    if patterns.is_empty() {
        return;
    }
    let files = files(report.analysis, report.edits().sources);
    let bodies = Bodies::of(&verbatims(report.analysis, &files));
    let mut findings = Vec::new();
    for file in &files {
        for (i, line) in file.text.body().lines().enumerate() {
            let at_line = i as u32 + 1;
            for found in patterns.find(line) {
                let col = col_of(line, found.at);
                if bodies.holds(file.id, at_line, col) {
                    continue;
                }
                findings.push((Span { file: file.id, line: at_line, col }, found));
            }
        }
    }
    for (span, found) in findings {
        let message = match found.kind {
            Kind::Pattern => format!("`{}` is on the project's pattern list", found.text),
            Kind::Regex => format!("`{}` matches the project's regex `{}`", found.text, found.entry),
        };
        report.add(span, &found.entry, message, Some("write what the project's own rule asks for".into()));
    }
}

// ---- chktex 21 -------------------------------------------------------------

fn unintended_escape(report: &mut Report) {
    let view = report.text();
    let files = files(report.analysis, report.edits().sources);
    let bodies = Bodies::of(&verbatims(report.analysis, &files));
    let mut set: HashMap<(FileId, u32, u32), char> = HashMap::new();
    for i in 0..view.len() {
        if let (Some(span), Some(c)) = (view.span(i), view.char_at(i)) {
            set.insert((span.file, span.line, span.col), c);
        }
    }
    let mut findings = Vec::new();
    for file in &files {
        let Some(mut scan) = Scan::at(&file.text, Pos::new(1, 1), CatcodeTable::latex()) else { continue };
        let mut math = false;
        let mut word: Option<(u32, String)> = None;
        while let Some(lexeme) = scan.advance() {
            let name = scan.name(&lexeme).map(str::to_string);
            let single = name.as_deref().filter(|n| n.chars().count() == 1);
            match (&lexeme.tok, name.as_deref(), single) {
                (Tok::Chr(_, Catcode::Math), ..) => {
                    math = !math;
                    word = None;
                }
                (Tok::Cs(_), Some("(" | "["), _) => {
                    math = true;
                    word = None;
                }
                (Tok::Cs(_), Some(")" | "]"), _) => {
                    math = false;
                    word = None;
                }
                (Tok::Cs(_), _, Some(n)) if crate::lint::text::PUNCTUATION.contains(n) => {
                    let punct = n.chars().next().unwrap_or('.');
                    let Some((line, before)) = word.take() else { continue };
                    let at = lexeme.start;
                    if math || line != at.line || bodies.holds(file.id, at.line, at.col) {
                        continue;
                    }
                    if set.get(&(file.id, at.line, at.col)) == Some(&punct) {
                        continue;
                    }
                    if !ends_a_word(file.text.line(at.line), at.col) || !opens_a_word(file.text.line(at.line), at.col) {
                        continue;
                    }
                    findings.push((Span { file: file.id, line: at.line, col: at.col }, before, punct));
                }
                (Tok::Cs(_), Some(n), None) if !n.is_empty() && n.chars().all(char::is_alphabetic) => {
                    word = Some((lexeme.start.line, n.to_string()));
                }
                _ => word = None,
            }
        }
    }
    for (span, before, punct) in findings {
        let start = Pos::new(span.line, span.col);
        let edits = report.edits().text(span.file).map(|(path, _)| {
            vec![Edit { path, start, end: Pos::new(span.line, span.col + 1), replacement: String::new() }]
        });
        report.add_fix(
            span,
            &format!("\\{punct}"),
            format!("`\\{before}` is followed by `\\{punct}`, which is a command and not the mark `{punct}`"),
            Fix::new(format!("write `{punct}`"), Applicability::Unsafe, edits),
        );
    }
}

/// Whether the character before the 1-based `col` ends a control word, so
/// nothing stands between the command and what follows it.
fn ends_a_word(line: &str, col: u32) -> bool {
    col >= 2 && line[..byte_of(line, col)].chars().next_back().is_some_and(char::is_alphabetic)
}

/// Whether a space and then a word, or the end of the line, follows the
/// escaped mark that starts at the 1-based `col`.
fn opens_a_word(line: &str, col: u32) -> bool {
    let rest = &line[byte_of(line, col + 2).min(line.len())..];
    let trimmed = rest.trim_start();
    trimmed.is_empty() || (trimmed.len() < rest.len() && trimmed.starts_with(char::is_alphabetic))
}

// ---- chktex 22 -------------------------------------------------------------

fn comment_marker(report: &mut Report) {
    let markers = comment_marker_settings(report.analysis).markers;
    let files = files(report.analysis, report.edits().sources);
    let bodies = Bodies::of(&verbatims(report.analysis, &files));
    let mut findings = Vec::new();
    for file in &files {
        for (i, line) in file.text.body().lines().enumerate() {
            let at_line = i as u32 + 1;
            let Some(start) = crate::tex::comment_start(line) else { continue };
            if bodies.holds(file.id, at_line, col_of(line, start)) {
                continue;
            }
            let comment = &line[start + 1..];
            for marker in &markers {
                let Some(at) = whole_word(comment, marker) else { continue };
                let col = col_of(line, start + 1 + at);
                let note = comment[at + marker.len()..].trim_start_matches([':', ' ', '\t', '-']).trim_end();
                findings.push((Span { file: file.id, line: at_line, col }, marker.clone(), note.to_string()));
            }
        }
    }
    for (span, marker, note) in findings {
        let message = match note.is_empty() {
            true => format!("`{marker}` left in a comment"),
            false => format!("`{marker}` left in a comment: {note}"),
        };
        report.add(span, &marker, message, Some("do what the note asks and delete it".into()));
    }
}

/// Where `word` stands in `text` with no letter or digit against it.
fn whole_word(text: &str, word: &str) -> Option<usize> {
    let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    text.match_indices(word)
        .map(|(at, _)| at)
        .find(|at| edge(text[..*at].chars().next_back()) && edge(text[at + word.len()..].chars().next()))
}

// ---- chktex 31 -------------------------------------------------------------

fn ignored_verbatim_tail(report: &mut Report) {
    let view = report.text();
    let files = files(report.analysis, report.edits().sources);
    let verbatims = verbatims(report.analysis, &files);
    let lines = by_line(view);
    let mut findings = Vec::new();
    for v in &verbatims.bodies {
        let Some(file) = files.iter().find(|f| f.id == v.file) else { continue };
        let line = file.text.line(v.to);
        let start = byte_of(line, v.after);
        let end = crate::tex::comment_start(line).unwrap_or(line.len()).max(start);
        let tail = line[start..end].trim();
        if tail.is_empty() {
            continue;
        }
        // The run set something there, so this implementation kept it.
        if lines.get(&(v.file, v.to)).is_some_and(|cols| cols.iter().any(|(col, _)| *col >= v.after)) {
            continue;
        }
        findings.push((Span { file: v.file, line: v.to, col: v.after }, v.name.clone(), tail.to_string()));
    }
    for (span, name, tail) in findings {
        let at = Pos::new(span.line, span.col);
        let edits = report
            .edits()
            .text(span.file)
            .map(|(path, _)| vec![Edit { path, start: at, end: at, replacement: "\n".into() }]);
        report.add_fix(
            span,
            &name,
            format!("`{tail}` stands after `\\end{{{name}}}` and the run set none of it"),
            Fix::new("move the text to the next line", Applicability::Unsafe, edits),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_patterns_match_outside_comments() {
        let settings = UserPattern { patterns: vec!["ChkTeX".to_string()], regexes: vec![r"\\hline".to_string()] };
        let patterns = Patterns::of(&settings);
        assert!(!patterns.is_empty());
        let found = patterns.find(r"ChkTeX and \hline % ChkTeX again");
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].kind, Kind::Pattern);
        assert_eq!(found[0].at, 0);
        assert_eq!(found[1].kind, Kind::Regex);
        assert_eq!(found[1].text, r"\hline");
    }

    #[test]
    fn an_empty_list_matches_nothing() {
        let patterns = Patterns::of(&UserPattern::default());
        assert!(patterns.is_empty());
        assert!(patterns.find(r"ChkTeX and \hline").is_empty());
    }

    #[test]
    fn a_regex_that_does_not_compile_is_skipped() {
        let settings = UserPattern { patterns: Vec::new(), regexes: vec!["(".to_string()] };
        assert!(Patterns::of(&settings).is_empty());
    }

    #[test]
    fn markers_are_whole_words() {
        assert_eq!(whole_word(" TODO: fix", "TODO"), Some(1));
        assert_eq!(whole_word(" TODOS are fine", "TODO"), None);
        assert_eq!(whole_word(" see FIXME", "FIXME"), Some(5));
    }

    #[test]
    fn an_escaped_mark_needs_a_word_on_both_sides() {
        assert!(ends_a_word(r"A \TeX\. Right?", 7));
        assert!(opens_a_word(r"A \TeX\. Right?", 7));
        assert!(!ends_a_word(r"A \. Right?", 3));
    }
}
