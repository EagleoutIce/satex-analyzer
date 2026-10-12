//! Spacing: what the run turned into glue, and what it swallowed.  Every
//! rule here reads `facts.spaces` and the characters the document set, so a
//! space that never reached the page is never reported.

use crate::fixed::{HashMap, HashSet};
use std::rc::Rc;

use crate::builtins::OccKind;
use crate::lint::fix::{Applicability, Edit, Edits, Fix, Pos, Text};
use crate::lint::text::{PUNCTUATION, Rewrite, View};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::mode::Modes;
use crate::tex::{Span, Sym};

pub static RULES: &[Rule] = &[
    Rule {
        code: "command-terminated-space",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a control word ate the space after it and the next word is set against what it produced",
        explanation: "\
TeX reads a control word and then discards the spaces after it (The TeXbook,
chapter 3), so `\\TeX ok` sets `TEXok`.

A space the source has after a control word and the run never turned into
glue is one TeX swallowed.  The rule reports it only where the command put a
character on the page and the next word starts against it, so `\\rm text` and
`\\noindent text` stay silent.

Fix by writing `\\TeX{} ok`, or `\\TeX\\ ok` where a space belongs.",
        references: &[Ref { source: Source::Chktex, at: "1" }, Ref { source: Source::TeXbook, at: "chapter 3" }],
        fixes: &[Applicability::Safe],
        run: command_terminated_space,
    },
    Rule {
        code: "missing-nonbreaking-space",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "an ordinary space in front of a reference lets the line break before the number",
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
        run: missing_nonbreaking_space,
    },
    Rule {
        code: "sentence-spacing",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a period set the wrong width of space for the sentence around it",
        explanation: "\
How wide a space TeX sets follows the space factor of the character before
it (tex.web § 1034).  A period raises it to 3000, which asks for the wider
space between sentences, but a period after an uppercase letter leaves it at
1000.

So `etc. and` gets a sentence space and `the ABC. Next` a word space, both
the wrong way round.  The rule reads the characters the run set and reports
only where a real space token became glue.

Fix by writing `etc.\\ and`, or `the ABC\\@. Next`.",
        references: &[
            Ref { source: Source::Chktex, at: "12" },
            Ref { source: Source::Chktex, at: "13" },
            Ref { source: Source::TeXWeb, at: "1034" },
        ],
        fixes: &[Applicability::Unsafe],
        run: sentence_spacing,
    },
    Rule {
        code: "space-before-command",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a space lets the line break between a word and the mark that belongs to it",
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
        run: space_before_command,
    },
    Rule {
        code: "space-before-punctuation",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a space stands between a word and the punctuation after it",
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
        run: space_before_punctuation,
    },
    Rule {
        code: "double-space",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "two interword spaces reached the page, one of them from a tie",
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
        run: double_space,
    },
    Rule {
        code: "parenthesis-spacing",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a parenthesis carries the space of the text inside it instead of the text around it",
        explanation: "\
A parenthesis takes the space of the text around it, never of the text
inside it: `text (inside) text`, not `text( inside )text`.

The rule reads the characters the run set, skips math, where `f(x)` is
right, and reports a space inside the parentheses only where a real space
token became glue.  A missing space is the noisier half, because `word(s)` is
ordinary English, so those findings come as notes.

Fix by moving the space to the other side of the parenthesis.",
        references: &[Ref { source: Source::Chktex, at: "36" }, Ref { source: Source::Chktex, at: "37" }],
        fixes: &[Applicability::Unsafe],
        run: parenthesis_spacing,
    },
];

/// chktex's `Linker`: the commands a tie belongs in front of.  chktex's own
/// policy, not a claim about what the names mean.
const LINKER: &[&str] = &[r"\ref", r"\pageref", r"\eqref", r"\cite"];

/// References that typeset the name in front of the number themselves
/// (cleveref, hyperref, varioref), so the space before them is the word
/// space of the sentence, not a tie.
const NAMING: &[&str] = &[
    r"\cref",
    r"\Cref",
    r"\crefrange",
    r"\Crefrange",
    r"\cpageref",
    r"\Cpageref",
    r"\autoref",
    r"\vref",
    r"\Vref",
    r"\vrefrange",
    r"\nameref",
    r"\Nameref",
];

/// The command chktex's warning 24 names beside `\label`, which the run
/// finds for itself.
const INDEXING: &[&str] = &[r"\index"];

/// chktex's `NotPreSpaced`, without `\/`, which the italics rules own.
const FOOTNOTE: &[&str] = &[r"\footnote", r"\footnotemark"];

/// tex.web § 1043: a space becomes glue only in horizontal mode.
fn glue(modes: Modes) -> bool {
    !modes.is_empty() && modes.within(Modes::ANY_HORIZONTAL)
}

/// The space token standing right before `span`: the run of spaces in front
/// of it, or the end of the line above, with the modes it was read in and
/// how many columns it covers.
fn space_before(analysis: &Analysis, source: &Text, span: Span) -> Option<(Span, u32, Modes)> {
    let line: Vec<char> = source.line(span.line).chars().collect();
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
    let at = Span { file: span.file, line: above, col: source.line(above).chars().count() as u32 + 1 };
    analysis.facts.spaces.get(&at).map(|modes| (at, 1, *modes))
}

/// [`space_before`] `span` where it became glue, with the path and text of
/// the file it stands in.
fn glue_before(analysis: &Analysis, edits: &Edits<'_>, span: Span) -> Option<(String, Rc<Text>, Span, u32)> {
    let (path, source) = edits.text(span.file)?;
    let (at, width, modes) = space_before(analysis, &source, span)?;
    glue(modes).then_some((path, source, at, width))
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
        let line: Vec<char> = source.line(span.line).chars().collect();
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
        if !view.char_at(i).is_some_and(char::is_alphanumeric) || !view.is_prose_text(i) {
            continue;
        }
        // Anything set in a box of its own stands apart from the word.
        if i == 0 || view.char_at(i - 1) == Some(' ') || view.modes(i - 1) != Modes::HORIZONTAL {
            continue;
        }
        let name = analysis.interner.cs(expansion.name);
        let set = format!("{}{}", view.word_before(i), view.word_after(i));
        findings.push((eaten, path, name, set));
    }
    for (at, path, name, set) in findings {
        let edits = write_over(&path, at, 1, "{} ");
        report.add_fix(
            at,
            "swallowed",
            format!("{name} took the space after its name, and the run set `{set}`"),
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
    names: &[&[&str]],
    wrappers: bool,
) -> Vec<(Span, String)> {
    let mut seen: HashSet<Span> = HashSet::default();
    let mut out = Vec::new();
    // A command the document defines around a listed one is a wrapper: it
    // stands where the word in front of the number does, not the number.
    let mut own: HashSet<Sym> = HashSet::default();
    let mut called: HashMap<Span, Sym> = HashMap::default();
    if !wrappers {
        own.extend(analysis.facts.defs.iter().filter(|d| view.document(d.span.file)).map(|d| d.name));
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
        if names.iter().any(|group| group.contains(&name.as_str())) && seen.insert(expansion.span) {
            out.push((expansion.span, name));
        }
    }
    out
}

fn missing_nonbreaking_space(report: &mut Report) {
    let analysis = report.analysis;
    let edits = report.edits();
    let naming: HashSet<Span> = analysis
        .facts
        .expansions
        .iter()
        .filter(|e| NAMING.contains(&analysis.interner.cs(e.name).as_str()))
        .map(|e| e.span)
        .collect();
    for (span, what) in sites(analysis, report.text(), &[OccKind::Ref, OccKind::Cite], &[LINKER], false) {
        if naming.contains(&span) {
            continue;
        }
        let Some((path, source, at, width)) = glue_before(analysis, &edits, span) else { continue };
        // A tie only helps where a word stands in front of the space.
        let line: Vec<char> = source.line(at.line).chars().collect();
        let word = at.col.checked_sub(2).and_then(|k| line.get(k as usize)).copied();
        if !word.is_some_and(char::is_alphanumeric) {
            continue;
        }
        let fix = (at.line == span.line).then(|| write_over(&path, at, width, "~"));
        report.add_fix(
            at,
            "tie",
            format!("an ordinary space stands in front of {what}, so the line may break between the word and it"),
            Fix::new("write a tie, `~`, instead of the space", Applicability::Unsafe, fix),
        );
    }
}

/// The abbreviation ending at the period at `index`, dots and all.
fn abbreviation(view: &View, index: usize) -> String {
    let mut start = index;
    while start > 0 && view.char_at(start - 1).is_some_and(|c| c.is_alphanumeric() || c == '.') {
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
        && view.is_prose_text(index)
        && report.analysis.facts.spaces.contains_key(&span)
        && glue(view.modes(index))
}

fn sentence_spacing(report: &mut Report) {
    let view = report.text();
    let mut findings = Vec::new();
    for (byte, _) in view.chars().match_indices('.') {
        let i = view.at(byte);
        if !view.is_prose(i) || !plain_space(report, i + 1) {
            continue;
        }
        let before = i.checked_sub(1).and_then(|k| view.char_at(k));
        // What follows in another box (an `\item` label) or paragraph is
        // not the rest of this sentence.
        let next = view.char_at(i + 2).filter(|_| view.beside(i + 1) && view.beside(i + 2));
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
        if capitals && after.is_uppercase() {
            let message = format!("the period after `{word}` set a word space, where a sentence ends");
            findings.push(rewrite_space(i..i + 1, "intersentence", message, r"\@."));
        } else if (before.is_lowercase() || before.is_numeric()) && after.is_lowercase() {
            let message = format!("the period in `{spelled}` set a sentence space, where the sentence runs on");
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

/// A space the document should have written some other way.  A control
/// word swallows the space after it, so the fix names the command alone.
fn rewrite_space(range: std::ops::Range<usize>, name: &str, message: String, want: &str) -> Rewrite {
    Rewrite::new(range, name, message, want).fix(format!("write `{}`", want.trim_end()))
}

fn space_before_command(report: &mut Report) {
    let analysis = report.analysis;
    let edits = report.edits();
    for (span, what) in sites(analysis, report.text(), &[OccKind::Label], &[INDEXING, FOOTNOTE], true) {
        let Some((path, _, at, width)) = glue_before(analysis, &edits, span) else { continue };
        let footnote = FOOTNOTE.contains(&what.as_str());
        let cost = match footnote {
            true => "so the mark can be set away from the word it belongs to",
            false => "so the mark can move to the next line, and \\pageref can then name the next page",
        };
        let fix = (at.line == span.line).then(|| write_over(&path, at, width, ""));
        report.add_fix(
            at,
            if footnote { "footnote" } else { "pagereference" },
            format!("a space stands in front of {what}, and the line may break there, {cost}"),
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
        if !plain_space(report, i) || !view.is_ours(i + 1) {
            continue;
        }
        let message = format!("a space stands between the word and the `{mark}` after it");
        findings.push(Rewrite::new(i..i + 1, "punctuation", message, "").fix("delete the space").safe());
    }
    for finding in findings {
        report.rewrite(finding);
    }
}

/// TeX's hard space: the tie, and the name LaTeX gives it.
fn hard_space(name: &str) -> bool {
    name == "~" || name == r"\nobreakspace"
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
        if !hard_space(&analysis.interner.cs(expansion.name)) {
            continue;
        }
        let Some((path, source)) = edits.text(span.file) else { continue };
        let line: Vec<char> = source.line(span.line).chars().collect();
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
        if let Some((at, width, _)) = found {
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
        let line: Vec<char> = source.line(first.line).chars().collect();
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
    let mut findings = Vec::new();
    for i in view.prose_text() {
        let Some(paren) = view.char_at(i).filter(|c| matches!(c, '(' | ')')) else { continue };
        let open = paren == '(';
        let inside = match open {
            true => Some(i + 1),
            false => i.checked_sub(1),
        };
        // Characters set in another box may stand elsewhere on the page.
        let near = |k: usize| view.beside(k.max(i));
        if inside.is_some_and(|k| plain_space(report, k) && near(k)) {
            let side = if open { "after" } else { "in front of" };
            let message = format!("a space stands {side} the `{paren}`, inside the parentheses");
            let at = inside.unwrap_or(i);
            findings.push(Rewrite::new(at..at + 1, "parenthesis", message, "").fix("delete the space"));
            continue;
        }
        let outside = match open {
            true => i.checked_sub(1),
            false => Some(i + 1),
        };
        // A word with a suffix in parentheses, `file(s)` or `f(x)`, is one
        // word: its parentheses hold no space.  (`(c)three` is a label.)
        let partner = |k: usize| match view.char_at(k) {
            Some(')') => Some(true),
            Some(c) if c.is_whitespace() => Some(false),
            Some(_) => None,
            None => Some(false),
        };
        let one_word = open && (i + 1..i + 32).find_map(partner).unwrap_or(false);
        if !one_word && outside.filter(|&k| near(k)).and_then(|k| view.char_at(k)).is_some_and(char::is_alphabetic) {
            let side = if open { "in front of" } else { "after" };
            let message = format!("no space stands {side} the `{paren}`, so it is set against the word");
            let want = if open { format!(" {paren}") } else { format!("{paren} ") };
            let fix = "put a space on the other side";
            findings.push(Rewrite::new(i..i + 1, "parenthesis", message, want).fix(fix).at(Severity::Info));
        }
    }
    for finding in findings {
        report.rewrite(finding);
    }
}
