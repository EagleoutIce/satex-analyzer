//! Mathematics, read off the modes the run was really in (tex.web § 211),
//! so a formula a macro or an environment opened counts like one a `$`
//! opened.  The source is read only where the spelling is the finding.

use crate::fixed::{HashMap, HashSet};

use crate::builtins::{ModeCmd, Primitive, Typeset};
use crate::lint::fix::{Applicability, Edit, Edits, Fix, Lexeme, Pos, Scan};
use crate::lint::text::{PUNCTUATION, Rewrite, View};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::mode::Modes;
use crate::tex::{Catcode, CatcodeTable, FileId, Meaning, Span, Sym, Tok, Token};

pub static RULES: &[Rule] = &[
    Rule {
        code: "math-shift-dollars",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "math opened with `$` or `$$` where LaTeX spells it \\( or \\[",
        explanation: "\
`$$ … $$` and `$ … $` are plain TeX's displays.  LaTeX spells them
`\\[ … \\]` and `\\( … \\)`, which obey the `fleqn` and `leqno` class options
and the class's `\\everydisplay` (source2e, ltmath.dtx).

Both spellings put the run in the same mode.  satex takes the first
character set in math mode, lexes the source before it with TeX's own mouth,
and reports the math shift characters standing there, so math an environment
opened is left alone.

Fix by writing `\\[ … \\]` and `\\( … \\)`.",
        references: &[
            Ref { source: Source::Chktex, at: "45" },
            Ref { source: Source::Chktex, at: "46" },
            Ref { source: Source::Source2e, at: "ltmath.dtx" },
        ],
        fixes: &[Applicability::Safe],
        run: math_shift_dollars,
    },
    Rule {
        code: "math-script-braces",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "`^` and `_` take one token, so what follows needs braces",
        explanation: "\
`^` and `_` attach the single next token to the atom before them (The TeXbook,
chapter 16).  `x^12` sets the exponent 1 and then a 2 of its own size, and
`(a+b)^2` puts the exponent on the closing parenthesis alone.

The rule lexes the source with TeX's own mouth where the run really was in
math mode, so a `^` under another catcode regime is not one, and
`\\left( … \\right)^2` is one atom the script attaches to.

Fix by putting the script between `{` and `}`.",
        references: &[
            Ref { source: Source::Chktex, at: "3" },
            Ref { source: Source::Chktex, at: "25" },
            Ref { source: Source::TeXbook, at: "chapter 16" },
        ],
        fixes: &[Applicability::Safe],
        run: math_script_braces,
    },
    Rule {
        code: "delimiter-not-followed",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "\\left, \\right or \\middle is followed by a brace or a math shift",
        explanation: "\
`\\left`, `\\right` and `\\middle` scan a delimiter: one character with a
`\\delcode`, or one control sequence made by `\\delimiter` (tex.web § 1161).
A `{`, a `}` or a `$` is none of those, so TeX reports `Missing delimiter
(. inserted)` and sets nothing.

The rule asks what each command means rather than matching its name, so it
covers `\\middle` and any `\\let` alias, which chktex's name list does not.

Fix by writing `\\{` and `\\}` for brace delimiters, or `\\left.` where no
delimiter is wanted.",
        references: &[Ref { source: Source::Chktex, at: "43" }, Ref { source: Source::TeXWeb, at: "1161" }],
        fixes: &[Applicability::Unsafe],
        run: delimiter_not_followed,
    },
    Rule {
        code: "math-still-open",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "the run is still in math mode where the text ends",
        explanation: "\
A `$` that is never closed leaves TeX in math mode.  The next `\\par` makes TeX
report `Missing $ inserted` and close the formula itself (tex.web § 1047
`insert_dollar_sign`), and everything up to there is set as mathematics.

The rule reports what the interpreter already saw: the `missing-dollar`
diagnostic it raised, or, where no `\\par` followed, a last character still
carrying a math mode (unless a limit stopped the run before the text's end).
It reports one finding, never both.

Fix by closing the formula.",
        references: &[Ref { source: Source::Chktex, at: "16" }, Ref { source: Source::TeXWeb, at: "1047" }],
        fixes: &[],
        run: math_still_open,
    },
    Rule {
        code: "math-mode-mismatch",
        category: Category::Correctness,
        severity: Severity::Info,
        summary: "\\(, \\), \\[ or \\] stands where the mode does not allow it",
        explanation: "\
LaTeX's four math delimiters each test the mode before they act.  `\\(` and
`\\[` require that math is off, `\\)` inner math and `\\]` display math.  Where
the test fails they call `\\@badmath`, which reports `Bad math environment
delimiter` (source2e, ltmath.dtx).

The rule reads the mode each one really ran in, so it sees a `\\(` nested in a
`$ … $` and a delimiter reached through a macro, where chktex follows the
source and misses both.

Fix by pairing each delimiter with its own partner.",
        references: &[Ref { source: Source::Chktex, at: "49" }, Ref { source: Source::Source2e, at: "ltmath.dtx" }],
        fixes: &[],
        run: math_mode_mismatch,
    },
    Rule {
        code: "math-operator-name",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "an operator name is set in math italic instead of by its command",
        explanation: "\
Letters in math mode are set in math italic, one variable per letter, with
the spacing of a product (The TeXbook, chapter 18), so `$sin(x)$` sets a product of
three variables.

satex decides this from the run, not from a list of names: the name the
letters spell has to be a command this document has whose body sets exactly
those letters.  So `\\argmax` from `\\DeclareMathOperator` is suggested too,
and `\\mathrm{sin}` is left alone.

Fix by writing the command.",
        references: &[Ref { source: Source::Chktex, at: "35" }, Ref { source: Source::TeXbook, at: "chapter 18" }],
        fixes: &[Applicability::Unsafe],
        run: math_operator_name,
    },
    Rule {
        code: "ellipsis-dots",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "three periods set three periods, not an ellipsis",
        explanation: "\
Three `.` characters are set with the spacing of three periods, which is
tighter than an ellipsis (The TeXbook, chapter 18).  `\\ldots` sets them on the
baseline and `\\cdots` at the height of a binary operator.

The rule reads the periods that reached the page, so an ellipsis a command
produced is already excluded.  In math it names `\\cdots` between the
operators chktex lists.

Fix by writing the command.",
        references: &[Ref { source: Source::Chktex, at: "11" }, Ref { source: Source::TeXbook, at: "chapter 18" }],
        fixes: &[Applicability::Unsafe],
        run: ellipsis_dots,
    },
    Rule {
        code: "dimension-times",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "the letter x stands between two numbers where a multiplication sign belongs",
        explanation: "\
`640x480` sets the letter x between the numbers.  The sign for a product is
`\\times`, a binary operator with the spacing of one (The TeXbook, chapter 16).

The rule reads the characters the run set, so it finds a dimension a macro
assembled as well as one typed out.  It names `\\times` where the letter was
already in math mode and `$\\times$` where it was in text.

Fix by writing the operator.",
        references: &[Ref { source: Source::Chktex, at: "29" }, Ref { source: Source::TeXbook, at: "chapter 16" }],
        fixes: &[Applicability::Unsafe],
        run: dimension_times,
    },
    Rule {
        code: "math-punctuation",
        category: Category::Style,
        severity: Severity::Info,
        summary: "punctuation sits on the wrong side of a formula",
        explanation: "\
A formula is part of the sentence around it.  Punctuation that ends the
sentence goes after the closing delimiter of inline math, and inside a
display, where it is set on the display's own line (The TeXbook, chapter 18).

The rule reads the mode, so it treats every display alike, whether it came
from `\\[`, from `$$` or from an `equation` environment, where chktex follows
only `\\]` and `$$`.

Fix by moving the punctuation to the other side of the delimiter.",
        references: &[Ref { source: Source::Chktex, at: "40" }, Ref { source: Source::TeXbook, at: "chapter 18" }],
        fixes: &[],
        run: math_punctuation,
    },
];

/// chktex's `CenterDots`: what an ellipsis stands between when `\cdots` is
/// the one that belongs.
const CENTER_DOTS: &str = "=+-<>";

/// How many source tokens one file is lexed for.
const LEXEMES: usize = 400_000;

/// How far from a formula its delimiter is looked for.
const NEAR: usize = 400;

/// How far past a display the sentence's punctuation is looked for.
const AFTER: usize = 24;

/// A run of characters the document set in one known math mode, from one
/// file and in ascending order there.
struct Region {
    start: usize,
    end: usize,
    mode: Modes,
    first: Span,
    last: Span,
    /// Where the character set before the region stands, when that is
    /// earlier in the same file.
    after: Option<Span>,
}

/// The math the document itself set, split wherever the mode, the file or
/// the source order breaks.
fn regions(view: &View) -> Vec<Region> {
    let mut out: Vec<Region> = Vec::new();
    for i in 0..view.len() {
        let mode = view.modes(i);
        let Some(span) = view.span(i) else { continue };
        if !mode.is_known() || !mode.meets(Modes::ANY_MATH) || !view.is_prose(i) {
            continue;
        }
        let joins = out.last().is_some_and(|last| {
            last.end == i
                && last.mode == mode
                && last.last.file == span.file
                && !span.before(last.last.line, last.last.col)
        });
        match out.last_mut().filter(|_| joins) {
            Some(last) => {
                last.end = i + 1;
                last.last = span;
            }
            None => {
                let after = i
                    .checked_sub(1)
                    .and_then(|j| view.span(j))
                    .filter(|before| before.file == span.file && before.before(span.line, span.col));
                out.push(Region { start: i, end: i + 1, mode, first: span, last: span, after });
            }
        }
    }
    out
}

fn span_at(file: FileId, at: Pos) -> Span {
    Span::new(file, at.line, at.col)
}

/// One document file lexed with TeX's mouth, once for the whole rule.
struct Lexed {
    path: String,
    scan: Scan,
    tokens: Vec<Lexeme>,
}

impl Lexed {
    fn of(edits: &Edits, file: FileId) -> Option<Lexed> {
        let (path, text) = edits.text(file)?;
        let mut scan = Scan::at(&text, Pos::new(1, 1), CatcodeTable::latex())?;
        let mut tokens = Vec::new();
        while tokens.len() < LEXEMES {
            let Some(lexeme) = scan.advance() else { break };
            tokens.push(lexeme);
        }
        Some(Lexed { path, scan, tokens })
    }

    fn at(&self, at: Pos) -> usize {
        self.tokens.partition_point(|lexeme| lexeme.start < at)
    }

    fn exactly(&self, at: Pos) -> Option<usize> {
        let index = self.at(at);
        self.tokens.get(index).filter(|lexeme| lexeme.start == at).map(|_| index)
    }

    fn name(&self, index: usize) -> Option<&str> {
        self.scan.name(self.tokens.get(index)?)
    }

    fn shift(&self, index: usize) -> bool {
        matches!(self.tokens.get(index).map(|l| l.tok), Some(Tok::Chr(_, Catcode::Math)))
    }
}

/// The document files the rule reads, each lexed once.
#[derive(Default)]
struct Files(crate::fixed::HashMap<FileId, Option<Lexed>>);

impl Files {
    fn get(&mut self, edits: &Edits, file: FileId) -> Option<&Lexed> {
        self.0.entry(file).or_insert_with(|| Lexed::of(edits, file)).as_ref()
    }
}

/// tex.web §§ 1161, 1191: the commands that scan a delimiter.
fn scans_delimiter(meaning: &Meaning) -> bool {
    matches!(meaning.prim(), Some(Primitive::Typeset(Typeset::Delimiter) | Primitive::Mode(ModeCmd::Left)))
}

fn meaning_of(analysis: &Analysis, name: &str) -> Meaning {
    analysis.interner.lookup(name).map_or(Meaning::Undefined, |sym| analysis.env.meaning(sym))
}

fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
}

// chktex warnings 45 and 46: `Use \[ ... \] instead of $$ ... $$.` and
// `Use \( ... \) instead of $ ... $.`

fn math_shift_dollars(report: &mut Report) {
    let view = report.text();
    let edits = report.edits();
    let mut files = Files::default();
    let mut seen = HashSet::default();
    let mut findings = Vec::new();
    for region in regions(view) {
        let Some(lexed) = files.get(&edits, region.first.file) else { continue };
        let Some(open) = opening(lexed, &region) else { continue };
        if !seen.insert((region.first.file, lexed.tokens[open.start].start)) {
            continue;
        }
        // A script's own group is inner math under a `$$` display, so the
        // count of math shifts, not the mode, says which display it is.
        if open.len() > 2 {
            continue;
        }
        let display = open.len() == 2;
        let (want_open, want_close) = match display {
            true => ("\\[", "\\]"),
            false => ("\\(", "\\)"),
        };
        let at = lexed.tokens[open.start].start;
        let edit = closing(lexed, &region).filter(|close| close.len() == open.len()).map(|close| {
            vec![
                Edit {
                    path: lexed.path.clone(),
                    start: at,
                    end: lexed.tokens[open.end - 1].end,
                    replacement: want_open.into(),
                },
                Edit {
                    path: lexed.path.clone(),
                    start: lexed.tokens[close.start].start,
                    end: lexed.tokens[close.end - 1].end,
                    replacement: want_close.into(),
                },
            ]
        });
        findings.push((span_at(region.first.file, at), display, want_open, want_close, edit));
    }
    for (span, display, open, close, edit) in findings {
        let (severity, typed, what) = match display {
            true => (Severity::Warning, "$$ … $$", "display"),
            false => (Severity::Info, "$ … $", "inline"),
        };
        report.add_fix_as(
            severity,
            span,
            what,
            format!("{typed} is plain TeX's {what} math, where LaTeX spells it `{open} … {close}`"),
            Fix::new(format!("write `{open} … {close}`"), Applicability::Safe, edit),
        );
    }
}

/// The last run of math shift characters standing between the character
/// set before the region and the region's own first character.
fn opening(lexed: &Lexed, region: &Region) -> Option<std::ops::Range<usize>> {
    let from = match region.after {
        Some(before) => lexed.at(Pos::of(before)),
        None => lexed.at(Pos::new(region.first.line, 1)),
    };
    let stop = lexed.at(Pos::of(region.first));
    let mut run: Option<std::ops::Range<usize>> = None;
    for index in from..stop.min(from + NEAR) {
        match (lexed.shift(index), run.take()) {
            (true, Some(range)) if range.end == index => run = Some(range.start..index + 1),
            (true, _) => run = Some(index..index + 1),
            (false, keep) => run = keep,
        }
    }
    run.filter(|range| range.end == stop || !lexed.shift(range.end))
}

/// The first run of math shift characters after the region's last
/// character.
fn closing(lexed: &Lexed, region: &Region) -> Option<std::ops::Range<usize>> {
    let from = lexed.exactly(Pos::of(region.last))? + 1;
    let start = (from..(from + NEAR).min(lexed.tokens.len())).find(|index| lexed.shift(*index))?;
    let mut end = start;
    while lexed.shift(end) {
        end += 1;
    }
    Some(start..end)
}

// chktex warnings 3 and 25: `You should enclose the previous parenthesis
// with '{}'.` and `You might wish to put this between a pair of '{}'`.

/// A script the source spells in a way TeX does not read as written.
struct Script {
    span: Span,
    name: &'static str,
    message: String,
    fix: String,
    edits: Option<Vec<Edit>>,
}

fn math_script_braces(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let edits = report.edits();
    let mut files = Files::default();
    let mut seen = HashSet::default();
    let mut findings = Vec::new();
    for region in regions(view) {
        let Some(lexed) = files.get(&edits, region.first.file) else { continue };
        let (from, to) = (lexed.at(Pos::of(region.first)), lexed.at(Pos::of(region.last)));
        for k in from..=to.min(lexed.tokens.len().saturating_sub(1)) {
            let Some(Tok::Chr(script, Catcode::Sup | Catcode::Sub)) = lexed.tokens.get(k).map(|l| l.tok) else {
                continue;
            };
            if !seen.insert((region.first.file, lexed.tokens[k].start)) {
                continue;
            }
            findings.extend(previous_bracket(analysis, lexed, k).map(|bracket| Script {
                span: span_at(region.first.file, lexed.tokens[k - 1].start),
                name: "bracket",
                message: format!(
                    "`{script}` attaches only to the `{bracket}` before it, not to what that bracket closes"
                ),
                fix: format!("enclose the bracketed group in `{{}}` before the `{script}`"),
                edits: None,
            }));
            let run = script_run(lexed, k + 1);
            if run.len() < 2 {
                continue;
            }
            let (start, end) = (lexed.tokens[run.start].start, lexed.tokens[run.end - 1].end);
            let spelling: String = run.clone().filter_map(|i| character(&lexed.tokens[i])).collect();
            findings.push(Script {
                span: span_at(region.first.file, start),
                name: "script",
                message: format!(
                    "`{script}` takes one token, so `{script}{spelling}` sets only `{}` as the script",
                    spelling.chars().next().unwrap_or(' ')
                ),
                fix: format!("write `{script}{{{spelling}}}`"),
                edits: Some(vec![
                    Edit { path: lexed.path.clone(), start, end: start, replacement: "{".into() },
                    Edit { path: lexed.path.clone(), start: end, end, replacement: "}".into() },
                ]),
            });
        }
    }
    for finding in findings {
        report.add_fix(
            finding.span,
            finding.name,
            finding.message,
            Fix::new(finding.fix, Applicability::Safe, finding.edits),
        );
    }
}

fn character(lexeme: &Lexeme) -> Option<char> {
    match lexeme.tok {
        Tok::Chr(c, Catcode::Letter | Catcode::Other) => Some(c),
        _ => None,
    }
}

/// The closing bracket standing right before the script at `k`, when it is
/// one character and no `\left`-family command set it.
fn previous_bracket(analysis: &Analysis, lexed: &Lexed, k: usize) -> Option<String> {
    let previous = lexed.tokens.get(k.checked_sub(1)?)?;
    let bracket = match previous.tok {
        Tok::Chr(c @ (')' | ']'), Catcode::Other) => c.to_string(),
        Tok::Cs(_) => format!("\\{}", lexed.name(k - 1).filter(|name| *name == "}")?),
        _ => return None,
    };
    // `\left( … \right)` is one atom, and the script attaches to all of it.
    let opened =
        k.checked_sub(2).and_then(|j| lexed.name(j)).is_some_and(|name| scans_delimiter(&meaning_of(analysis, name)));
    (!opened).then_some(bracket)
}

/// The characters TeX reads one at a time after a `^` or `_`.
fn script_run(lexed: &Lexed, from: usize) -> std::ops::Range<usize> {
    let mut start = from;
    while matches!(lexed.tokens.get(start).map(|l| l.tok), Some(Tok::Chr(_, Catcode::Space))) {
        start += 1;
    }
    let mut end = start;
    while lexed.tokens.get(end).and_then(character).is_some_and(char::is_alphanumeric) {
        end += 1;
    }
    start..end
}

// chktex warning 43: `'%s' is normally not followed by '%c'.`

fn delimiter_not_followed(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let edits = report.edits();
    let mut files = Files::default();
    let mut findings = Vec::new();
    for expansion in &analysis.facts.expansions {
        if !view.document(expansion.span.file) || !scans_delimiter(&analysis.env.meaning(expansion.name)) {
            continue;
        }
        let Some(lexed) = files.get(&edits, expansion.span.file) else { continue };
        let Some(index) = lexed.exactly(Pos::of(expansion.span)).filter(|k| lexed.name(*k).is_some()) else { continue };
        let mut next = index + 1;
        while matches!(lexed.tokens.get(next).map(|l| l.tok), Some(Tok::Chr(_, Catcode::Space))) {
            next += 1;
        }
        let Some(lexeme) = lexed.tokens.get(next) else { continue };
        let (found, replacement) = match lexeme.tok {
            Tok::Chr(c @ ('{' | '}'), Catcode::Begin | Catcode::End) => (c, Some(format!("\\{c}"))),
            Tok::Chr(c, Catcode::Math) => (c, None),
            _ => continue,
        };
        let edit = replacement.clone().map(|replacement| {
            vec![Edit { path: lexed.path.clone(), start: lexeme.start, end: lexeme.end, replacement }]
        });
        let name = report.cs(expansion.name);
        let fix = match replacement {
            Some(_) => format!("write `\\{found}`"),
            None => format!("give `{name}` a delimiter, or `.` for none"),
        };
        findings.push((
            expansion.span,
            format!("`{name}` scans a delimiter, and `{found}` is not one: TeX inserts `.` and sets nothing"),
            fix,
            edit,
        ));
    }
    for (span, message, fix, edit) in findings {
        report.add_fix(span, "delimiter", message, Fix::new(fix, Applicability::Unsafe, edit));
    }
}

// chktex warning 16: `Mathmode still on at end of LaTeX file.`

fn math_still_open(report: &mut Report) {
    use crate::facts::Severity as Seen;
    let analysis = report.analysis();
    let fix = Some("close the formula".to_string());
    // The `\par` that found math mode on, shown where the document led to
    // it, one on an exact path first. On an approximated path it may not
    // be TeX's.
    let raised = analysis.facts.diagnostics.iter().filter(|d| d.code == "missing-dollar");
    if let Some(d) = raised.min_by_key(|d| d.severity != Seen::Warning) {
        let mut message = format!("math mode was still on where the paragraph ended: {}", d.message);
        let severity = match d.severity {
            Seen::Warning => Severity::Error,
            _ => {
                message.push_str(" (the analysis approximated on the way: may not be TeX's)");
                Severity::Warning
            }
        };
        report.add_as(severity, d.origin.unwrap_or(d.span), "unclosed", message, fix);
        return;
    }
    // A run stopped by a limit did not reach the end of the text.
    if analysis.facts.diagnostics.iter().any(|d| d.code == "budget-exhausted") {
        return;
    }
    // `\end{document}` ends a paragraph: an open formula was found above, and
    // the last character sits in one that closed since.
    let ended = analysis
        .facts
        .occurrences
        .iter()
        .any(|o| o.kind == crate::builtins::OccKind::EndEnvironment && o.key == "document" && o.certain);
    if ended {
        return;
    }
    let view = report.text();
    let Some(last) = view.ours().next_back() else { return };
    if !view.modes(last).meets(Modes::ANY_MATH) {
        return;
    }
    let Some(span) = view.span(last) else { return };
    report.add(span, "unclosed", "math mode was still on where the document's text ended".into(), fix);
}

// chktex warning 49: `Expected math mode to be %s here.`

/// LaTeX's four math delimiters, the mode each one needs and whether it
/// needs to be in it (source2e, ltmath.dtx).
const DELIMITERS: [(&str, Modes, bool); 4] = [
    ("(", Modes::ANY_MATH, false),
    ("[", Modes::ANY_MATH, false),
    (")", Modes::MATH, true),
    ("]", Modes::DISPLAY_MATH, true),
];

fn math_mode_mismatch(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let wanted: Vec<(Sym, Modes, bool)> = DELIMITERS
        .iter()
        .filter(|(name, _, _)| defined(analysis, name))
        .filter_map(|(name, modes, on)| Some((analysis.interner.lookup(name)?, *modes, *on)))
        .collect();
    // Only a use that raised the error counts: an expansion-only context
    // (`\edef`, `\write`, a glossary's stored text) expands `\)` without
    // ever running `\@badmath`, whatever mode the run is in around it.
    let errors: HashMap<Span, bool> =
        analysis.facts.occurrences.iter().filter(|o| crate::lint::shared::is_error(analysis, o)).fold(
            HashMap::default(),
            |mut errors, o| {
                *errors.entry(o.span).or_insert(false) |= o.certain;
                errors
            },
        );
    let mut findings = Vec::new();
    for expansion in &analysis.facts.expansions {
        let Some((_, modes, on)) = wanted.iter().find(|(sym, _, _)| *sym == expansion.name) else { continue };
        let mode = expansion.mode;
        if !view.document(expansion.span.file) || !mode.is_known() || mode.is_empty() {
            continue;
        }
        if mode.meets(*modes) == *on || !errors.contains_key(&expansion.span) {
            continue;
        }
        let name = report.cs(expansion.name);
        let state = match on {
            true => "on",
            false => "off",
        };
        findings.push((
            expansion.span,
            format!(
                "`{name}` expects math mode to be {state} here, and the run was in {} mode: \
                 a real run reports `Bad math environment delimiter`",
                mode.names().join(" or ")
            ),
        ));
    }
    for (span, message) in findings {
        // The run reached `\@badmath` here, so this is the error itself.
        let severity = match errors.get(&span) == Some(&true) {
            true => Severity::Error,
            false => Severity::Info,
        };
        let fix = "pair each math delimiter with its own partner";
        report.add_as(severity, span, "delimiter", message, Some(fix.into()));
    }
}

// chktex warning 35: `You should perhaps use 'cmd' instead.`

fn math_operator_name(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let edits = report.edits();
    let mut findings = Vec::new();
    for region in regions(view) {
        let mut i = region.start;
        while i < region.end {
            if !view.char_at(i).is_some_and(char::is_alphabetic) {
                i += 1;
                continue;
            }
            let mut end = i + 1;
            while end < region.end && view.char_at(end).is_some_and(char::is_alphabetic) && view.joined(end) {
                end += 1;
            }
            if end - i > 1 {
                findings.extend(operator(analysis, view, &edits, i..end));
            }
            i = end;
        }
    }
    for finding in findings {
        report.rewrite(finding);
    }
}

/// The command that sets `range`'s letters as an operator, when this
/// document has one and the letters were typed rather than handed to a
/// command as its argument.
fn operator(analysis: &Analysis, view: &View, edits: &Edits, range: std::ops::Range<usize>) -> Option<Rewrite> {
    let word = view.slice(range.clone());
    let (first, _) = view.literal(range.clone())?;
    if source_char_before(edits, first) == Some('{') || !sets_letters(analysis, word) {
        return None;
    }
    let name = format!("\\{word}");
    let message = format!(
        "`{word}` is set in math italic, as a product of {} variables, where `{name}` sets it upright as one operator",
        range.len()
    );
    Some(Rewrite::new(range, "operator", message, name))
}

/// The character standing right before a span in its own source line.
fn source_char_before(edits: &Edits, span: Span) -> Option<char> {
    let (_, text) = edits.text(span.file)?;
    text.line(span.line).chars().nth(span.col.checked_sub(2)? as usize)
}

/// Whether `\word` is defined here and its body sets exactly those letters,
/// following the `\protect`ed name a robust command hides behind
/// (source2e, ltdefns.dtx).
fn sets_letters(analysis: &Analysis, word: &str) -> bool {
    [word.to_string(), format!("{word} ")].iter().any(|name| match meaning_of(analysis, name) {
        Meaning::Macro(mac) => mac.arity() == 0 && body_sets(&mac.replacement_text, word),
        _ => false,
    })
}

/// Whether a replacement text holds `word` as one run of character tokens.
fn body_sets(body: &[Token], word: &str) -> bool {
    let mut run = String::new();
    for token in body {
        match token.tok {
            Tok::Chr(c, Catcode::Letter | Catcode::Other) => run.push(c),
            _ if run == word => return true,
            _ => run.clear(),
        }
    }
    run == word
}

// chktex warning 11: `You should use '%s' to achieve an ellipsis.`

fn ellipsis_dots(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let plain = match defined(analysis, "dots") {
        true => "\\dots",
        false => "\\ldots",
    };
    let centered = defined(analysis, "cdots").then_some("\\cdots");
    let mut findings = Vec::new();
    let mut i = 0;
    while i + 2 < view.len() {
        let dots = (i..i + 3).all(|j| view.char_at(j) == Some('.') && (j == i || view.joined(j)));
        if !dots || !view.is_prose(i) {
            i += 1;
            continue;
        }
        let want = match view.math(i) {
            false => Some(plain),
            true => centered.filter(|_| beside_operator(view, i)),
        };
        findings.push((i..i + 3, want));
        i += 3;
    }
    for (range, want) in findings {
        let finding = match want {
            Some(want) => {
                let message = format!("three periods set three periods, where `{want}` sets an ellipsis");
                let with = spelled(view, range.end, want);
                Rewrite::new(range, "ellipsis", message, with).fix(format!("write `{want}`"))
            }
            None => Rewrite::told(
                range,
                "ellipsis",
                "three periods set three periods, where an ellipsis belongs".to_string(),
                "write `\\cdots` between operators, `\\ldots` between terms",
            ),
        };
        report.rewrite(finding);
    }
}

/// A control word swallows the space after it, so one that has to keep it
/// needs `{}` (tex.web § 354).
fn spelled(view: &View, after: usize, command: &str) -> String {
    match view.char_at(after) {
        Some(' ') => format!("{command}{{}}"),
        _ => command.to_string(),
    }
}

/// Whether one of chktex's `CenterDots` characters stands beside the dots.
fn beside_operator(view: &View, at: usize) -> bool {
    let sides = [view.before(at), view.after(at + 3)];
    sides.into_iter().flatten().any(|(_, c)| CENTER_DOTS.contains(c))
}

// chktex warning 29: `$\times$ may look prettier here.`

fn dimension_times(report: &mut Report) {
    let view = report.text();
    let mut findings = Vec::new();
    for i in 1..view.len().saturating_sub(1) {
        if view.char_at(i) != Some('x') || !view.is_prose(i) {
            continue;
        }
        let Some(dimension) = between_numbers(view, i) else { continue };
        findings.push((
            i,
            match view.math(i) {
                true => "\\times",
                false => "$\\times$",
            },
            dimension,
        ));
    }
    for (at, want, dimension) in findings {
        let message =
            format!("`{dimension}` sets the letter x between the numbers, where `{want}` sets a multiplication sign");
        report.rewrite(Rewrite::new(at..at + 1, "times", message, want));
    }
}

/// The first character the document set after a display, over the spaces
/// and the equation number the class put between.
fn following(view: &View, from: usize) -> Option<(usize, char)> {
    let at = (from..view.len().min(from + AFTER))
        .find(|i| view.char_at(*i).is_some_and(|c| c != ' ') && view.is_ours(*i))?;
    Some((at, view.char_at(at)?))
}

/// The number, the x and the number, when digits stand on both sides of
/// `at` and TeX set them all as one run.
fn between_numbers(view: &View, at: usize) -> Option<String> {
    let (left, _) = view.before(at).filter(|(_, c)| c.is_ascii_digit())?;
    let (right, _) = view.after(at + 1).filter(|(_, c)| c.is_ascii_digit())?;
    if !(left + 1..=right).all(|i| view.joined(i)) {
        return None;
    }
    let start = left + 1 - view.word_before(left + 1).chars().count();
    let end = right + view.word_after(right).chars().count();
    Some(view.slice(start..end).to_string())
}

// chktex warning 40: `You should put punctuation outside inner/inside
// display math mode.`

fn math_punctuation(report: &mut Report) {
    let view = report.text();
    let mut findings = Vec::new();
    for region in regions(view) {
        let last = region.end - 1;
        match region.mode {
            Modes::MATH => {
                let inside = view.char_at(last).filter(|c| PUNCTUATION.contains(*c));
                findings.extend(inside.map(|c| (last, c, false)));
            }
            Modes::DISPLAY_MATH => {
                let outside = following(view, region.end).filter(|(i, c)| PUNCTUATION.contains(*c) && !view.math(*i));
                findings.extend(outside.map(|(i, c)| (i, c, true)));
            }
            _ => {}
        }
    }
    for (at, c, display) in findings {
        let Some(span) = view.span(at) else { continue };
        let (is, belongs) = match display {
            true => ("after the display", "inside it"),
            false => ("inside the formula", "after the closing delimiter"),
        };
        report.add(
            span,
            match display {
                true => "display",
                false => "inline",
            },
            format!("`{c}` is set {is}, where it belongs {belongs}"),
            Some(format!("move the `{c}` {belongs}")),
        );
    }
}
