//! Spacing: what the run turned into glue, and what it swallowed.  Every
//! rule here reads `facts.spaces` and the characters the document set, so a
//! space that never reached the page is never reported.

use crate::fixed::{HashMap, HashSet};
use std::rc::Rc;

use crate::analysis::machine::Analysis;
use crate::engine::builtins::OccKind;
use crate::engine::mode::Modes;
use crate::engine::tex::{Span, Sym};
use crate::linter::fix::{Applicability, Edit, Fix, Pos, Text};
use crate::linter::text::{PUNCTUATION, Rewrite, View};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};

pub static RULES: &[Rule] = &[
    Rule {
        code: "command-terminated-space",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "space eaten after a command",
        explanation: "\
TeX reads a control word and then discards the spaces after it (The TeXbook,
chapter 3), so `\\TeX ok` sets `TEXok`.

A space the source has after a control word and the run never turned into
glue is one TeX swallowed.  The rule reports it only where the command put a
character on the page and the next word starts against it, so `\\rm text` and
`\\noindent text` stay silent.

Fix by writing `\\TeX{} ok`, or `\\TeX\\ ok` where a space belongs.  Where the
joined word is meant (`\\TeX book` for TeXbook), write `\\TeX{}book`: no space
is swallowed there, so the rule stays silent and the intent shows.",
        references: &[Ref { source: Source::Chktex, at: "1" }, Ref { source: Source::TeXbook, at: "chapter 3" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: command_terminated_space,
    },
    Rule {
        code: "missing-nonbreaking-space",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "missing tie before a reference",
        explanation: "\
A reference should not begin a line on its own, so a tie belongs in front of
it.  `~` is an interword space TeX may not break at (The TeXbook, chapter 14).

The rule reads the reference and citation sites the run recorded, so it
follows a `\\ref` inside a macro of yours without a list of names, and it
reports only where the space really became glue (tex.web § 1043).

Fix by writing `Figure~\\ref{fig:one}`.",
        references: &[
            Ref { source: Source::Chktex, at: "2" },
            Ref { source: Source::TeXbook, at: "chapter 14" },
            Ref { source: Source::TeXWeb, at: "1043" },
        ],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: missing_nonbreaking_space,
    },
    Rule {
        code: "sentence-spacing",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "wrong space after a period",
        explanation: "\
How wide a space TeX sets follows the space factor of the character before
it (tex.web § 1034).  A period raises it to 3000, which asks for the wider
space between sentences, but a period after an uppercase letter leaves it at
1000.  Under `\\frenchspacing` a period sets 1000 too, so nothing is
reported.

So `etc. and` gets a sentence space and `the ABC. Next` a word space, both
the wrong way round.  The rule reads the characters the run set and reports
only where a real space token became glue.  A lowercase word counts as the
sentence running on only when the source writes it right after the space,
in the same line and group, unless the word before the period is a listed
abbreviation.

Fix by writing `etc.\\ and`, or `the ABC\\@. Next`.",
        references: &[
            Ref { source: Source::Chktex, at: "12" },
            Ref { source: Source::Chktex, at: "13" },
            Ref { source: Source::TeXWeb, at: "1034" },
        ],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: sentence_spacing,
    },
    Rule {
        code: "sentence-capital",
        since: "0.2.4",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "sentence starts lowercase",
        explanation: "\
In the languages of `lints.sentence-capital.languages` (by `\\languagename`), a
sentence starts with a capital.  A finding needs `.`, `!` or `?`, a space,
then a lowercase word.  No sentence ends after a number, an ellipsis or an
abbreviation: three letters or less (`etc.`), a period inside (`e.g.`), or
listed in `conventions.ties.abbreviations` or `lints.sentence-capital.exceptions`
(those go to `sentence-spacing`).  A listed word (`iPhone`) or one with a
capital inside may start a sentence.

Fix by writing the capital, or `\\Cref` for a `\\cref`.",
        references: &[],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: sentence_capital,
    },
    Rule {
        code: "space-before-command",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "line may break before a mark",
        explanation: "\
`\\label` records the page its own position falls on, and `\\index` and
`\\footnote` mark the word they stand after (source2e, ltxref.dtx).  A space
in front of one is glue TeX may break the line at, so the mark can land on
the next line.

The rule finds the label sites the run recorded, so a `\\label` your own macro
calls counts too, and reports the space only where the run turned it into
glue.

Fix by deleting the space, so the command touches the word.",
        references: &[
            Ref { source: Source::Chktex, at: "24" },
            Ref { source: Source::Chktex, at: "42" },
            Ref { source: Source::Source2e, at: "ltxref.dtx" },
        ],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: space_before_command,
    },
    Rule {
        code: "space-before-punctuation",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "space before punctuation",
        explanation: "\
Punctuation belongs against the word it follows.  A space in front of a comma
or a period is glue, and the reader sees it on the page.

The rule reports only a real space token that became glue in horizontal mode
(tex.web § 1043).  A thin space a language package inserts before `:` comes
from that package's own code, which the run keeps out of the text, so French
typography is not reported.

Fix by deleting the space.",
        references: &[Ref { source: Source::Chktex, at: "26" }, Ref { source: Source::TeXWeb, at: "1043" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: space_before_punctuation,
    },
    Rule {
        code: "double-space",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "double space",
        explanation: "\
TeX reads a run of spaces as one space token (The TeXbook, chapter 8), so two spaces
in the source set one on the page.  satex reports only the spaces that really
reached it.

Two do when a tie stands beside an ordinary space, as in `Dr.~ Smith`, and
when the run appended two glues in a row, as in `a {} b`.  That second case is
chktex's warning 30, off by default, so it comes as a note.

Fix by deleting the ordinary space.",
        references: &[
            Ref { source: Source::Chktex, at: "39" },
            Ref { source: Source::Chktex, at: "30" },
            Ref { source: Source::TeXbook, at: "chapter 8" },
        ],
        fixes: &[Applicability::Safe, Applicability::Unsafe],
        engines: &[],
        run: double_space,
    },
    Rule {
        code: "parenthesis-spacing",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "parenthesis takes the wrong space",
        explanation: "\
A parenthesis takes the space of the text around it, never of the text
inside it: `text (inside) text`, not `text( inside )text`.

The rule reads the characters the run set, skips math, where `f(x)` is
right, and reports a space inside the parentheses only where a real space
token became glue.  A missing space is the noisier half, because `word(s)` is
ordinary English, so those findings come as notes.

Code shown as text is not prose.  Every prose rule skips text in math, in
verbatim and in the typewriter family (`\\ttdefault`, as `\\texttt` and
`\\url` set), and this one a call too: a `(` right after a name, around a
list of single words or nothing, as in `plot(x, y)` or `c(1, 2, 3)`.  What an
undefined command or environment leaves behind is unknown code as well, so
every prose rule skips it: the command's arguments and the rest of its
statement or line, an environment's body up to its `\\end`.

Fix by moving the space to the other side of the parenthesis.",
        references: &[Ref { source: Source::Chktex, at: "36" }, Ref { source: Source::Chktex, at: "37" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: parenthesis_spacing,
    },
];

/// The `)` of a call `name(a, b)` whose `(` stands at `open`: right after
/// a name, around a list of single words or none, on one line.  Prose sets
/// a space in front of a parenthesis, and a parenthetical is not a list.
fn call(view: &View, open: usize) -> Option<usize> {
    let name = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    if view.char_at(open)? != '(' || !open.checked_sub(1).and_then(|k| view.char_at(k)).is_some_and(name) {
        return None;
    }
    let close = (open + 1..open + 64).find(|&k| matches!(view.char_at(k), Some(')' | '(' | '\n') | None))?;
    let args = view.slice(open + 1..close);
    let list = view.char_at(close) == Some(')') && args.split(',').all(|a| !a.trim().contains(char::is_whitespace));
    (list && (args.contains(',') || !args.contains(char::is_whitespace))).then_some(close)
}

/// tex.web § 1043: a space becomes glue only in horizontal mode.
fn glue(modes: Modes) -> bool {
    !modes.is_empty() && modes.within(Modes::ANY_HORIZONTAL)
}

/// The space token standing right before `span`: the run of spaces in front
/// of it, or the end of the line above, with the modes it was read in and
/// how many columns it covers.
fn space_before(analysis: &Analysis, source: &Text, span: Span) -> Option<(Span, u32, Modes)> {
    let line = source.columns(span.line);
    let mut start = span.col;
    while start > 1 && line.get(start as usize - 2) == Some(&' ') {
        start -= 1;
    }
    if start > 1 {
        let at = Span { file: span.file, line: span.line, col: start };
        let found = analysis.facts.spaces.get(&at);
        return found.filter(|_| start < span.col).map(|modes| (at, span.col - start, *modes));
    }
    // tex.web § 348: leading spaces give no token, so the end of the line
    // above is the space in front.
    let above = span.line.checked_sub(1).filter(|line| *line > 0)?;
    let at = Span { file: span.file, line: above, col: source.columns(above).len() as u32 + 1 };
    analysis.facts.spaces.get(&at).map(|modes| (at, 1, *modes))
}

/// [`space_before`] `span` where it became glue in prose, with the path
/// and text of the file it stands in.
fn glue_before(report: &Report, span: Span) -> Option<(String, Rc<Text>, Span, u32)> {
    let (path, source) = report.edits().text(span.file)?;
    let (at, width, modes) = space_before(report.analysis, &source, span)?;
    (glue(modes) && report.text().prose_at(at)).then_some((path, source, at, width))
}

fn write_over(path: &str, at: Span, width: u32, with: &str) -> Vec<Edit> {
    vec![Edit {
        path: path.to_string(),
        start: Pos::new(at.line, at.col),
        end: Pos::new(at.line, at.col + width),
        replacement: with.to_string(),
    }]
}

/// The column of the character right after the control word or active
/// character standing at `col`.
fn command_end(line: &[char], col: u32) -> u32 {
    let mut end = col as usize;
    if line.get(end - 1) == Some(&'\\') {
        while line.get(end).is_some_and(|c| c.is_alphabetic()) {
            end += 1;
        }
    }
    end as u32 + 1
}

fn command_terminated_space(report: &mut Report) {
    let analysis = report.analysis;
    let view = report.text();
    let edits = report.edits();
    let mut findings = Vec::new();
    for expansion in &analysis.facts.expansions {
        let span = expansion.span;
        if expansion.package.is_some() || span.col == 0 || !view.document(span.file) {
            continue;
        }
        let Some((path, source)) = edits.text(span.file) else { continue };
        let line = source.columns(span.line);
        if line.get(span.col as usize - 1) != Some(&'\\') {
            continue;
        }
        // A command that continues a word is not a word of its own.
        if span.col >= 2 && line.get(span.col as usize - 2).is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        let end = command_end(&line, span.col);
        if end <= span.col + 1 || line.get(end as usize - 1) != Some(&' ') {
            continue;
        }
        let eaten = Span { file: span.file, line: span.line, col: end };
        if analysis.facts.spaces.contains_key(&eaten) {
            continue;
        }
        let mut next = end as usize - 1;
        while line.get(next) == Some(&' ') {
            next += 1;
        }
        let after = Span { file: span.file, line: span.line, col: next as u32 + 1 };
        let Some(i) = view.at_span(after) else { continue };
        if !view.char_at(i).is_some_and(char::is_alphanumeric) || !view.is_word_prose(i) {
            continue;
        }
        // Anything set in a box of its own stands apart from the word: both sides in one
        // horizontal list (a paragraph, or an `\hbox` such as a TikZ node's text) or no finding.
        let one_list = |m: Modes| m == Modes::HORIZONTAL || m == Modes::RESTRICTED_HORIZONTAL;
        if i == 0
            || view.char_at(i - 1) == Some(' ')
            || !one_list(view.modes(i - 1))
            || view.modes(i) != view.modes(i - 1)
            || !view.beside(i)
        {
            continue;
        }
        let name = analysis.interner.cs(expansion.name);
        let set = format!("{}{}", view.word_before(i), view.word_after(i));
        findings.push((eaten, path, name, set, view.word_after(i)));
    }
    for (at, path, name, set, next) in findings {
        let edits = write_over(&path, at, 1, "{} ");
        report.add_fix(
            at,
            "swallowed",
            format!("space eaten after {name}: the run set `{set}`. If `{set}` is meant, write `{name}{{}}{next}`"),
            Fix::new(format!("write `{name}{{}}` so the space survives"), Applicability::Safe, Some(edits)),
        );
    }
}

/// The sites whose preceding space a rule checks, each with the word the
/// message calls it by.
fn sites(
    analysis: &Analysis,
    view: &View,
    kinds: &[OccKind],
    names: &[&[String]],
    wrappers: bool,
) -> Vec<(Span, String)> {
    let mut seen: HashSet<Span> = HashSet::default();
    let mut out = Vec::new();
    // A command the document defines around a listed one is a wrapper: it
    // stands where the word in front of the number does, not the number.
    // One whose body opens with the listed command sets the number first.
    let opens = |d: &&crate::analysis::facts::Definition| {
        d.mac.as_ref().and_then(|m| m.replacement_text.first()).and_then(|t| t.cs()).is_some_and(|cs| {
            let name = analysis.interner.cs(cs);
            names.iter().any(|group| group.contains(&name))
        })
    };
    let mut own: HashSet<Sym> = HashSet::default();
    let mut called: HashMap<Span, Sym> = HashMap::default();
    if !wrappers {
        own.extend(analysis.facts.defs.iter().filter(|d| view.document(d.span.file) && !opens(d)).map(|d| d.name));
        called.extend(analysis.facts.expansions.iter().map(|e| (e.span, e.name)));
    }
    for occurrence in &analysis.facts.occurrences {
        if !kinds.contains(&occurrence.kind) || !view.document(occurrence.span.file) {
            continue;
        }
        if !wrappers && called.get(&occurrence.span).is_none_or(|name| own.contains(name)) {
            continue;
        }
        let what = match occurrence.kind {
            OccKind::Cite => "the citation",
            OccKind::Label => "the label",
            _ => "the reference",
        };
        if seen.insert(occurrence.span) {
            out.push((occurrence.span, what.to_string()));
        }
    }
    for expansion in &analysis.facts.expansions {
        if expansion.package.is_some() || !view.document(expansion.span.file) {
            continue;
        }
        let name = analysis.interner.cs(expansion.name);
        if names.iter().any(|group| group.contains(&name)) && seen.insert(expansion.span) {
            out.push((expansion.span, name));
        }
    }
    out
}

fn missing_nonbreaking_space(report: &mut Report) {
    let analysis = report.analysis;
    let ties = &analysis.settings.conventions.ties;
    let naming: HashSet<Span> = analysis
        .facts
        .expansions
        .iter()
        .filter(|e| ties.naming.contains(&analysis.interner.cs(e.name)))
        .map(|e| e.span)
        .collect();
    for (span, what) in sites(analysis, report.text(), &[OccKind::Ref, OccKind::Cite], &[&ties.before], false) {
        if naming.contains(&span) {
            continue;
        }
        let Some((path, source, at, width)) = glue_before(report, span) else { continue };
        // A tie only helps where a word stands in front of the space.
        let line = source.columns(at.line);
        let word = at.col.checked_sub(2).and_then(|k| line.get(k as usize)).copied();
        if !word.is_some_and(char::is_alphanumeric) {
            continue;
        }
        let fix = (at.line == span.line).then(|| write_over(&path, at, width, "~"));
        report.add_fix(
            at,
            "tie",
            format!("missing tie before {what}: an ordinary space lets the line break there"),
            Fix::new("write a tie, `~`, instead of the space", Applicability::Unsafe, fix),
        );
    }
}

/// The abbreviation ending at the period at `index`, dots and all.
fn abbreviation(view: &View, index: usize) -> String {
    let mut start = index;
    while start > 0 && view.joined(start) && view.char_at(start - 1).is_some_and(|c| c.is_alphanumeric() || c == '.') {
        start -= 1;
    }
    view.slice(start..index + 1).to_string()
}

/// Whether the character at `index` is a space token the run turned into
/// glue in text the author wrote.
fn plain_space(report: &Report, index: usize) -> bool {
    let view = report.text();
    let Some(span) = view.span(index) else { return false };
    view.char_at(index) == Some(' ')
        && view.is_word_prose(index)
        && report.analysis.facts.spaces.contains_key(&span)
        && glue(view.modes(index))
}

/// `\frenchspacing`: the run left the period's `\sfcode` at 1000, so it
/// sets a word space everywhere.
fn french_spacing(analysis: &Analysis) -> bool {
    let name = format!("{}sfcode.", crate::engine::exec::CODE);
    analysis.interner.lookup(&name).is_some_and(|sym| analysis.env.value(sym).as_int() == Some(1000))
}

fn sentence_spacing(report: &mut Report) {
    if french_spacing(report.analysis) {
        return;
    }
    let view = report.text();
    let mut findings = Vec::new();
    for (byte, _) in view.chars().match_indices('.') {
        let i = view.at(byte);
        if !view.is_word_prose(i) || !plain_space(report, i + 1) {
            continue;
        }
        let before = i.checked_sub(1).and_then(|k| view.text_char(k));
        // What follows in another box (an `\item` label) or paragraph is
        // not the rest of this sentence.
        let next = view.text_char(i + 2).filter(|_| view.beside(i + 1) && view.beside(i + 2));
        let (Some(before), Some(after)) = (before, next) else { continue };
        // tex.web § 1034: two capitals hold the space factor below 1000, so
        // the period cannot raise it and the space stays a word space.
        let capitals = before.is_uppercase()
            && i >= 2
            && view.char_at(i - 2).is_some_and(char::is_uppercase)
            && view.joined(i - 1)
            && view.joined(i);
        let word = view.word_before(i).to_string();
        let spelled = abbreviation(view, i);
        let listed = report.analysis.settings.conventions.ties.abbreviations.contains(&spelled);
        // A lowercase word is evidence of a sentence running on only where it follows the space
        // in the same line and group: after a line break or a brace (`\textcolor{…}{henlo}`)
        // the source no longer says the two belong to one sentence.
        let runs_on = (before.is_lowercase() || before.is_numeric())
            && after.is_lowercase()
            && view.joined(i + 2)
            && !ends_sentence(report, i);
        if capitals && after.is_uppercase() && !listed {
            let message = format!("wrong space after the period: `{word}.` set a word space, where a sentence ends");
            findings.push(rewrite_space(i..i + 1, "intersentence", message, r"\@."));
        } else if listed || runs_on {
            let message =
                format!("wrong space after the period: `{spelled}` set a sentence space, where the sentence runs on");
            // `\ ` is a control space: the fix names it with its period, since
            // a lone backslash reads as a typo.
            let fix = format!(r"write `{spelled}\ ` (a control space), so the space stays a word space");
            findings.push(Rewrite::new(i + 1..i + 2, "interword", message, r"\ ").fix(fix));
        }
    }
    for finding in findings {
        report.rewrite(finding);
    }
}

/// Whether the period at `index` ends a sentence although a lowercase word
/// follows, a missing capital (`sentence-capital`) and no abbreviation: in a
/// language that capitalizes, after a word of four letters or more that
/// holds no period and that no list names.
fn ends_sentence(report: &Report, index: usize) -> bool {
    let (analysis, view) = (report.analysis, report.text());
    let spelled = abbreviation(view, index);
    let listed = |list: &[String]| list.contains(&spelled);
    capitalizes(analysis)
        && view.char_at(index) == Some('.')
        && spelled.chars().filter(|c| c.is_alphabetic()).count() > 3
        && !spelled[..spelled.len() - 1].contains('.')
        && !spelled.chars().any(|c| c.is_numeric())
        && !listed(&analysis.settings.conventions.ties.abbreviations)
        && !listed(&analysis.settings.lints.sentence_capital.exceptions)
}

/// Whether the document's language (`\languagename` at the end of the run) starts sentences with a capital.
fn capitalizes(analysis: &Analysis) -> bool {
    let languages = &analysis.settings.lints.sentence_capital.languages;
    crate::linter::shared::macro_text(analysis, "languagename")
        .is_some_and(|l| languages.iter().any(|c| c.eq_ignore_ascii_case(l.trim())))
}

fn sentence_capital(report: &mut Report) {
    let view = report.text();
    let edits = report.edits();
    let exceptions = &report.analysis.settings.lints.sentence_capital.exceptions;
    if !capitalizes(report.analysis) {
        return;
    }
    let mut findings = Vec::new();
    for (byte, mark) in view.chars().match_indices(['.', '!', '?']) {
        let i = view.at(byte);
        // `$x$` or `\texttt{x}` after `?` is no word, so no sentence start.
        let Some(next) = view.text_char(i + 2).filter(|c| c.is_lowercase()) else { continue };
        if !view.is_word_prose(i) || !plain_space(report, i + 1) || !view.beside(i + 1) || !view.beside(i + 2) {
            continue;
        }
        if mark == "." && (view.char_at(i.wrapping_sub(1)) == Some('.') || !ends_sentence(report, i)) {
            continue;
        }
        let mut end = i + 2;
        while view.char_at(end).is_some_and(char::is_alphanumeric) {
            end += 1;
        }
        let word = view.slice(i + 2..end);
        if exceptions.iter().any(|e| e == word) || word.chars().skip(1).any(char::is_uppercase) {
            continue;
        }
        findings.push((i + 2, next, word.to_string()));
    }
    for (at, next, word) in findings {
        let message = format!("the sentence starts with lowercase `{word}`");
        if view.is_ours(at) {
            let capital: String = next.to_uppercase().collect();
            report.rewrite(Rewrite::new(at..at + 1, "lowercase", message, capital));
            continue;
        }
        // A library set the word: a `\cref` typed after the space sets its name in lowercase.
        let Some(space) = view.span(at - 1) else { continue };
        let Some((path, text)) = edits.text(space.file) else { continue };
        let typed: String = text.columns(space.line).into_iter().skip(space.col as usize).take(5).collect();
        if typed == r"\cref" {
            let c = Pos::new(space.line, space.col + 2);
            let edit = Edit { path, start: c, end: Pos::new(space.line, space.col + 3), replacement: "C".into() };
            let fix = Fix::new("write `\\Cref`", Applicability::Unsafe, Some(vec![edit]));
            report.add_fix(Span { col: space.col + 1, ..space }, "lowercase", message, fix);
        }
    }
}

/// A space the document should have written some other way.  A control
/// word swallows the space after it, so the fix names the command alone.
fn rewrite_space(range: std::ops::Range<usize>, name: &str, message: String, want: &str) -> Rewrite {
    Rewrite::new(range, name, message, want).fix(format!("write `{}`", want.trim_end()))
}

fn space_before_command(report: &mut Report) {
    let analysis = report.analysis;
    let ties = &analysis.settings.conventions.ties;
    for (span, what) in sites(analysis, report.text(), &[OccKind::Label], &[&ties.unspaced, &ties.footnote], true) {
        let Some((path, _, at, width)) = glue_before(report, span) else { continue };
        let footnote = ties.footnote.contains(&what);
        let cost = match footnote {
            true => "so the mark can be set away from the word it belongs to",
            false => "so the mark can move to the next line, and `\\pageref` can then name the next page",
        };
        let fix = (at.line == span.line).then(|| write_over(&path, at, width, ""));
        report.add_fix(
            at,
            if footnote { "footnote" } else { "pagereference" },
            format!("line may break before {what}: a space stands in front of it, {cost}"),
            Fix::new("delete the space", Applicability::Unsafe, fix),
        );
    }
}

fn space_before_punctuation(report: &mut Report) {
    let view = report.text();
    let mut findings = Vec::new();
    for i in 1..view.len() {
        let Some(mark) = view.char_at(i + 1) else { continue };
        // The mark must have been set straight after the space: a command
        // between the two left the page unlike what the text reads.
        if !PUNCTUATION.contains(mark) || view.char_at(i - 1) == Some(' ') || !view.joined(i + 1) {
            continue;
        }
        // `x :: T` and `x := 1` are operators, not a colon.
        if mark == ':' && matches!(view.char_at(i + 2), Some(':' | '=')) {
            continue;
        }
        if !plain_space(report, i) || !view.is_word_prose(i + 1) {
            continue;
        }
        let message = format!("space before `{mark}`: it stands between the word and the mark");
        findings.push(Rewrite::new(i..i + 1, "punctuation", message, "").fix("delete the space").safe());
    }
    for finding in findings {
        report.rewrite(finding);
    }
}

fn double_space(report: &mut Report) {
    let analysis = report.analysis;
    let view = report.text();
    let edits = report.edits();
    let mut ties = Vec::new();
    for expansion in &analysis.facts.expansions {
        let span = expansion.span;
        if expansion.package.is_some() || span.col == 0 || !view.document(span.file) {
            continue;
        }
        if !analysis.settings.conventions.ties.hard.contains(&analysis.interner.cs(expansion.name))
            || !view.prose_at(span)
        {
            continue;
        }
        let Some((path, source)) = edits.text(span.file) else { continue };
        let line = source.columns(span.line);
        let mut found = space_before(analysis, &source, span).filter(|(_, _, modes)| glue(*modes));
        if found.is_none() {
            let end = command_end(&line, span.col);
            let at = Span { file: span.file, line: span.line, col: end };
            found = analysis
                .facts
                .spaces
                .get(&at)
                .filter(|modes| glue(**modes) && line.get(end as usize - 1) == Some(&' '))
                .map(|modes| (at, 1, *modes));
        }
        if let Some((at, width, _)) = found.filter(|(at, ..)| view.prose_at(*at)) {
            ties.push((at, width, path));
        }
    }
    for (at, width, path) in ties {
        report.add_fix(
            at,
            "tie",
            "a tie and an ordinary space stand next to each other, so two interword spaces reached the page".into(),
            Fix::new("delete the ordinary space", Applicability::Safe, Some(write_over(&path, at, width, ""))),
        );
    }
    // Two glues in a row with nothing between them in the source: chktex's
    // warning 30, which is off by default there.
    let mut pairs = Vec::new();
    for i in 1..view.len() {
        if !plain_space(report, i) || !plain_space(report, i - 1) {
            continue;
        }
        let (Some(first), Some(second)) = (view.span(i - 1), view.span(i)) else { continue };
        let Some((_, source)) = edits.text(first.file) else { continue };
        let line = source.columns(first.line);
        // Only a group stands between them: a tie is the other rule's, and
        // a command may have taken the glue back with \unskip.
        let between = line.get(first.col as usize..second.col as usize - 1);
        if first.line != second.line || !between.is_some_and(|gap| gap.iter().all(|c| matches!(c, '{' | '}'))) {
            continue;
        }
        pairs.push(i);
    }
    for i in pairs {
        let message = "the run appended two interword spaces in a row, so both reached the page".to_string();
        report.rewrite(Rewrite::new(i..i + 1, "twice", message, "").fix("delete one of the spaces").at(Severity::Info));
    }
}

fn parenthesis_spacing(report: &mut Report) {
    let view = report.text();
    let mut code = HashSet::default();
    let mut findings = Vec::new();
    for i in view.word_prose() {
        let Some(paren) = view.char_at(i).filter(|c| matches!(c, '(' | ')')) else { continue };
        // Code shown as text: a call ([`View::word_prose`] skips the typewriter family).
        if code.contains(&i) {
            continue;
        }
        if let Some(close) = call(view, i) {
            code.insert(close);
            continue;
        }
        let open = paren == '(';
        let inside = match open {
            true => Some(i + 1),
            false => i.checked_sub(1),
        };
        // Characters set in another box may stand elsewhere on the page.
        let near = |k: usize| view.beside(k.max(i));
        if inside.is_some_and(|k| plain_space(report, k) && near(k)) {
            let side = if open { "after" } else { "in front of" };
            let message = format!("space {side} the `{paren}`, inside the parentheses");
            let at = inside.unwrap_or(i);
            findings.push(Rewrite::new(at..at + 1, "parenthesis", message, "").fix("delete the space"));
            continue;
        }
        let outside = match open {
            true => i.checked_sub(1),
            false => Some(i + 1),
        };
        // A word with a part in parentheses, `file(s)`, `f(x)` or
        // `(re)executed`, is one word: its parentheses hold no space.  A
        // one-letter prefix is a label (`(c)three`).
        let partner = |k: usize| match view.char_at(k) {
            Some(')') if open => Some(true),
            Some('(') if !open => Some(i - k > 2),
            Some(c) if c.is_whitespace() => Some(false),
            Some(_) => None,
            None => Some(false),
        };
        let one_word = match open {
            true => (i + 1..i + 32).find_map(partner),
            false => (i.saturating_sub(32)..i).rev().find_map(partner),
        }
        .unwrap_or(false);
        if !one_word && outside.filter(|&k| near(k)).and_then(|k| view.text_char(k)).is_some_and(char::is_alphabetic) {
            let side = if open { "in front of" } else { "after" };
            let message = format!("missing space {side} the `{paren}`: it is set against the word");
            let want = if open { format!(" {paren}") } else { format!("{paren} ") };
            let fix = "put a space on the other side";
            findings.push(Rewrite::new(i..i + 1, "parenthesis", message, want).fix(fix).at(Severity::Info));
        }
    }
    for finding in findings {
        report.rewrite(finding);
    }
}
