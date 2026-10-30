//! Italic correction and accented letters, read off what the run set: the
//! font state each character was set in, the italic correction its TFM
//! gives it, which characters stand around a `\/`, and which letter the
//! `\accent` primitive really took.

use crate::fixed::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::builtins::OccKind;
use crate::facts::{Expansion, FontState, MeaningKind};
use crate::lint::fix::{Applicability, Edit, Edits, Fix, Pos, Scan};
use crate::lint::text::{self, Rewrite, View};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::mode::Modes;
use crate::tex::{CatcodeTable, Meaning, Span, Sym};
use crate::value::Scaled;

pub static RULES: &[Rule] = &[
    Rule {
        code: "stray-italic-correction",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a repeated italic correction, or one in front of a period or comma",
        explanation: "\
`\\/` appends a kern as wide as the italic correction of the last character
on the current list (The TeXbook, chapter 4).  The rule reports two places where
that kern does not belong: a second `\\/` with nothing typeset in between,
which finds a kern rather than a character (tex.web § 1113), and one in front of
a period or a comma (chktex 28).

satex reads the characters the document set, so it sees through a closing
brace: in `\\textit{word\\/}.` the next character set is the period.

Fix by deleting the `\\/`.",
        references: &[
            Ref { source: Source::Chktex, at: "5" },
            Ref { source: Source::Chktex, at: "28" },
            Ref { source: Source::TeXWeb, at: "1113" },
        ],
        fixes: &[Applicability::Safe, Applicability::Unsafe],
        run: stray_italic_correction,
    },
    Rule {
        code: "zero-italic-correction",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "the italic correction is zero here, so `\\/` changes nothing",
        explanation: "\
`\\/` appends a kern as wide as the italic correction the font gives the last
character on the current list (tex.web § 1113).  Where that correction is zero
the `\\/` does nothing.

Every character carries the font it was set in (fntguide § 2), and satex
reads the correction out of that font's TFM.  That is a measurement, where
chktex decides from a list of command names.  In math, and after a space, the
kern is zero whatever the font.

Fix by deleting the `\\/`.",
        references: &[
            Ref { source: Source::Chktex, at: "4" },
            Ref { source: Source::TeXWeb, at: "1113" },
            Ref { source: Source::TeXbook, at: "chapter 4" },
        ],
        fixes: &[Applicability::Safe],
        run: zero_italic_correction,
    },
    Rule {
        code: "missing-italic-correction",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a slanted letter runs into upright text with no `\\/` between them",
        explanation: "\
A slanted letter leans over the space after it, and `\\/` adds the kern the
font asks for (The TeXbook, chapter 4).

satex compares each character's `\\f@shape` against the kernel's
`\\itdefault` and `\\sldefault` (fntguide § 2), and the character's italic
correction in its own font says whether the kern is worth having.  So
`{\\it …}`, `\\emph{…}` and a shape a package selects are all seen alike,
where chktex knows only a list of names.

Fix by writing `\\/` after the last slanted character.",
        references: &[
            Ref { source: Source::Chktex, at: "6" },
            Ref { source: Source::TeXbook, at: "chapter 4" },
            Ref { source: Source::TeXWeb, at: "1113" },
        ],
        fixes: &[Applicability::Unsafe],
        run: missing_italic_correction,
    },
    Rule {
        code: "accent-on-dotted-letter",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "an accent sits on top of the dot of an `i` or a `j`",
        explanation: "\
An accent is placed over the whole character, so an `i` keeps its dot under
it (The TeXbook, chapter 9).  `\\i` and `\\j` are the dotless letters, and math has
`\\imath` and `\\jmath` (fntguide § 5).

The rule reads the character the `\\accent` or `\\mathaccent` primitive really
took, and how far the argument reaches from the source, so `\\^{}i` is left
alone.  In OT1 `\\'i` is a composite for `\\'\\i` and in T1 the glyph `í`, so
neither is reported.

Fix by writing the dotless letter under the accent.",
        references: &[
            Ref { source: Source::Chktex, at: "7" },
            Ref { source: Source::TeXbook, at: "chapter 9" },
            Ref { source: Source::Fntguide, at: "5" },
        ],
        fixes: &[Applicability::Unsafe],
        run: accent_on_dotted_letter,
    },
];

/// Source order over a [`Span`].
fn key(span: Span) -> (crate::tex::FileId, u32, u32) {
    (span.file, span.line, span.col)
}

/// Every character the run set in a document file, in source order.
fn ordered(view: &View) -> Vec<(Span, usize)> {
    let mut spots: Vec<(Span, usize)> = view.ours().filter_map(|i| Some((view.span(i)?, i))).collect();
    spots.sort_unstable_by_key(|(span, _)| key(*span));
    spots
}

/// The character the run set nearest after `span` in the same file.
fn next_char(spots: &[(Span, usize)], span: Span) -> Option<(Span, usize)> {
    let at = spots.partition_point(|(other, _)| key(*other) <= key(span));
    spots.get(at).copied().filter(|(other, _)| other.file == span.file)
}

/// The character the run set nearest before `span` in the same file.
fn prev_char(spots: &[(Span, usize)], span: Span) -> Option<(Span, usize)> {
    let at = spots.partition_point(|(other, _)| key(*other) < key(span));
    spots.get(at.checked_sub(1)?).copied().filter(|(other, _)| other.file == span.file)
}

/// Whether the run set any character between `from` and `to`.
fn nothing_between(spots: &[(Span, usize)], from: Span, to: Span) -> bool {
    next_char(spots, from).is_none_or(|(span, _)| key(span) > key(to))
}

/// Every `\/` the run reached in a document file, in source order.
fn corrections<'a>(analysis: &'a Analysis, view: &View) -> Vec<&'a Expansion> {
    let Some(slash) = analysis.interner.lookup("/") else { return Vec::new() };
    let mut sites: Vec<&Expansion> = analysis
        .facts
        .expansions
        .iter()
        .filter(|e| e.name == slash && e.meaning == MeaningKind::Primitive && e.package.is_none())
        .filter(|e| view.document(e.span.file))
        .collect();
    sites.sort_unstable_by_key(|e| key(e.span));
    sites
}

/// The metrics of the fonts the run set characters in, each TFM read once.
pub(crate) struct Fonts<'a> {
    analysis: &'a Analysis,
    base: PathBuf,
    read: HashMap<u16, Option<(Rc<crate::tfm::Metrics>, Scaled)>>,
}

impl<'a> Fonts<'a> {
    pub(crate) fn of(analysis: &'a Analysis) -> Fonts<'a> {
        let base = crate::paths::dir_of(Path::new(analysis.file_name(analysis.main_file)));
        Fonts { analysis, base, read: HashMap::default() }
    }

    /// The TFM the engine had loaded for a font state, and the size it was
    /// loaded at.
    pub(crate) fn metrics(&mut self, font: u16) -> Option<(Rc<crate::tfm::Metrics>, Scaled)> {
        let (analysis, base) = (self.analysis, &self.base);
        self.read
            .entry(font)
            .or_insert_with(|| {
                let used = analysis.fonts.get(usize::from(font))?.as_ref()?;
                let path = crate::tfm::locate(base, &analysis.distribution.roots, &used.name, "tfm")?;
                Some((crate::tfm::load(&path)?, used.size))
            })
            .clone()
    }

    /// The italic correction the font gives a character, which is the kern
    /// `\/` appends after it (tex.web § 1113).
    fn italic(&mut self, state: &FontState, c: char) -> Option<Scaled> {
        let (metrics, size) = self.metrics(state.font)?;
        Some(metrics.char_box(c as u32, size)?.italic)
    }

    fn name(&self, state: &FontState) -> String {
        match self.analysis.fonts.get(usize::from(state.font)).and_then(Option::as_ref) {
            Some(used) => format!("{} ({})", state.nfss(), used.name),
            None => state.nfss(),
        }
    }
}

/// The shapes LaTeX's font selection gives a slanted face: `\itdefault` and
/// `\sldefault` (fntguide § 2.5), read from the kernel rather than assumed.
fn slanted_shapes(analysis: &Analysis) -> Vec<String> {
    ["itdefault", "sldefault"].iter().filter_map(|name| crate::lint::shared::macro_text(analysis, name)).collect()
}

/// A width in scaled points, as `pt` (tex.web § 101: 65536 sp to the point).
fn points(width: Scaled) -> String {
    format!("{:.2}pt", width as f64 / 65536.0)
}

/// An edit deleting the `\/` that stands at `span`, when the source really
/// has one there and the file may be changed.
fn delete_slash(edits: &Edits, span: Span) -> Option<Vec<Edit>> {
    let (path, text) = edits.text(span.file)?;
    let (start, end) = (Pos::new(span.line, span.col), Pos::new(span.line, span.col + 2));
    (text.slice(start, end)? == "\\/").then(|| vec![Edit { path, start, end, replacement: String::new() }])
}

/// Why a `\/` the run reached sets no kern, or sets one where it hurts.
struct Stray {
    span: Span,
    name: &'static str,
    message: String,
    /// Deleting a kern the run proved to be zero keeps the output.
    applicability: Applicability,
}

fn stray_italic_correction(report: &mut Report) {
    let analysis = report.analysis;
    let view = report.text();
    let sites = corrections(analysis, view);
    if sites.is_empty() {
        return;
    }
    let spots = ordered(view);
    let mut findings = Vec::new();
    let mut previous: Option<Span> = None;
    for site in &sites {
        let span = site.span;
        // A site that fired more than once stands in a macro body, where
        // the characters around it in the source are not the ones it met.
        let once = site.count == 1;
        let repeated = once
            && previous.is_some_and(|before| {
                before.file == span.file && key(before) < key(span) && nothing_between(&spots, before, span)
            });
        previous = once.then_some(span);
        if repeated {
            findings.push(Stray {
                span,
                name: "repeated",
                message: "nothing was set since the \\/ before, so this one finds a kern at the end of the list \
                          and adds zero (tex.web § 1113)"
                    .into(),
                applicability: Applicability::Safe,
            });
            continue;
        }
        let Some((_, at)) = once.then(|| next_char(&spots, span)).flatten() else { continue };
        if let Some(c @ ('.' | ',')) = view.char_at(at) {
            findings.push(Stray {
                span,
                name: "punctuation",
                message: format!("the next character the run set is `{c}`, which the correction pushes away"),
                applicability: Applicability::Unsafe,
            });
        }
    }
    strays(report, findings);
}

/// The `\/`s a rule found, each with the edit that deletes it.
fn strays(report: &mut Report, findings: Vec<Stray>) {
    for finding in findings {
        let edits = delete_slash(&report.edits(), finding.span);
        report.add_fix(
            finding.span,
            finding.name,
            finding.message,
            Fix::new("delete the \\/", finding.applicability, edits),
        );
    }
}

fn zero_italic_correction(report: &mut Report) {
    let analysis = report.analysis;
    let view = report.text();
    let sites = corrections(analysis, view);
    if sites.is_empty() {
        return;
    }
    let spots = ordered(view);
    let mut fonts = Fonts::of(analysis);
    let mut findings = Vec::new();
    for site in &sites {
        let (span, modes) = (site.span, site.mode);
        if !modes.is_known() {
            continue;
        }
        if modes.within(Modes::ANY_MATH) {
            findings.push(Stray {
                span,
                name: "math",
                message: "in math mode \\/ appends a kern of zero (tex.web § 1113)".into(),
                applicability: Applicability::Safe,
            });
            continue;
        }
        // A site that fired more than once stands in a macro body, where
        // the character before it in the source is not the one it met.
        if modes.meets(Modes::ANY_MATH) || site.count != 1 {
            continue;
        }
        let Some((_, at)) = prev_char(&spots, span) else { continue };
        let Some(c) = view.char_at(at) else { continue };
        // tex.web § 1113: the kern is the italic correction of a character
        // at the end of the list, and a space in horizontal mode is glue.
        let (name, why) = match c {
            ' ' => ("glue", "a space stands before it, so the list ends in glue and \\/ adds a kern of zero".into()),
            _ => {
                let Some(state) = view.font(at) else { continue };
                if fonts.italic(state, c) != Some(0) {
                    continue;
                }
                (
                    "zero",
                    format!("{} gives `{c}` no italic correction, so this \\/ adds a kern of zero", fonts.name(state)),
                )
            }
        };
        findings.push(Stray { span, name, message: why, applicability: Applicability::Safe });
    }
    strays(report, findings);
}

/// A slanted character the run left behind without its kern.
struct Overhang {
    at: usize,
    letter: char,
    shape: String,
    italic: Scaled,
}

fn missing_italic_correction(report: &mut Report) {
    let analysis = report.analysis;
    let slanted = slanted_shapes(analysis);
    if slanted.is_empty() {
        return;
    }
    let view = report.text();
    let closed = &analysis.facts.text.closed;
    let mut fonts = Fonts::of(analysis);
    let mut findings = Vec::new();
    // Neighbors as TeX set them, not as the source lists them: a title set
    // once upright and once in italics is two runs of characters.
    for a in view.ours() {
        let b = a + 1;
        let (Some(from), Some(to)) = (view.span(a), view.span(b)) else { continue };
        // The `\/` goes into the source right after the letter, so only a
        // letter the next character follows there can take one.
        if from.file != to.file || closed.binary_search(&(b as u32)).is_ok() {
            continue;
        }
        let (Some(left), Some(right)) = (view.font(a), view.font(b)) else { continue };
        // The overhang shows only where a slanted shape gives way to an
        // upright one, not on a change of family or size.
        if !slanted.contains(&left.shape.to_string()) || slanted.contains(&right.shape.to_string()) {
            continue;
        }
        let (here, next) = (view.modes(a), view.modes(b));
        if !here.is_known() || !next.is_known() || here.meets(Modes::ANY_MATH) || next.meets(Modes::ANY_MATH) {
            continue;
        }
        let Some(letter) = view.char_at(a).filter(|c| *c != ' ') else { continue };
        if matches!(view.char_at(b), Some('.' | ',')) {
            continue;
        }
        let Some(italic) = fonts.italic(left, letter).filter(|width| *width > 0) else { continue };
        findings.push(Overhang { at: a, letter, shape: fonts.name(left), italic });
    }
    for finding in findings {
        let message = format!(
            "`{}` ends the text set in {}, which leans {} over what follows, and no \\/ takes that back",
            finding.letter,
            finding.shape,
            points(finding.italic)
        );
        let with = format!("{}\\/", finding.letter);
        let at = finding.at;
        report.rewrite(Rewrite::new(at..at + 1, "overhang", message, with).fix("write \\/ after it"));
    }
}

/// The control sequence a command's meaning starts with once it reaches a
/// primitive, through LaTeX's `\protect \cmd␣` indirection for a robust
/// command (source2e, ltdefns.dtx).
fn leads_with(analysis: &Analysis, sym: Sym, protect: Option<Sym>) -> Option<Sym> {
    let mut sym = sym;
    for _ in 0..4 {
        match analysis.env.meaning(sym) {
            Meaning::Primitive(_) => return Some(sym),
            Meaning::Macro(m) if m.arity() == 0 => {
                let mut body = m.replacement_text.iter().filter_map(|t| t.cs());
                let first = body.next()?;
                sym = match Some(first) == protect {
                    true => body.next()?,
                    false => first,
                };
            }
            _ => return None,
        }
    }
    None
}

/// Where the accent's argument stands in the source, so a character the run
/// set past it is not taken for the letter underneath: `\^{}i` accents
/// nothing and sets the `i` beside the accent, not under it.
fn argument(edits: &Edits, span: Span) -> Option<(Pos, Pos)> {
    let text = edits.sources.get(edits.analysis.file_name(span.file))?;
    let mut scan = Scan::at(&text, Pos::new(span.line, span.col), CatcodeTable::latex())?;
    scan.advance()?;
    let group = scan.argument()?;
    Some((group.start, group.end))
}

/// `an i`, `a j`.
fn article(letter: char) -> &'static str {
    match letter {
        'i' => "an",
        _ => "a",
    }
}

/// What stands under an accent, and what to write instead.
struct Dotted {
    span: Span,
    at: usize,
    letter: char,
    dotless: &'static str,
}

fn accent_on_dotted_letter(report: &mut Report) {
    let analysis = report.analysis;
    let sym = |name: &str| analysis.interner.lookup(name);
    let protect = sym("protect");
    let math_accents: Vec<Sym> = ["mathaccent", "Umathaccent"].iter().filter_map(|name| sym(name)).collect();
    let view = report.text();
    let ours = |span: Span| view.document(span.file);
    let mut sites: Vec<(Span, bool)> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Accent && o.package.is_none() && ours(o.span))
        .map(|o| (o.span, false))
        .collect();
    sites.extend(
        analysis
            .facts
            .expansions
            .iter()
            .filter(|e| e.mode.meets(Modes::ANY_MATH) && e.package.is_none() && ours(e.span))
            .filter(|e| leads_with(analysis, e.name, protect).is_some_and(|p| math_accents.contains(&p)))
            .map(|e| (e.span, true)),
    );
    if sites.is_empty() {
        return;
    }
    sites.sort_unstable_by_key(|(span, _)| key(*span));
    let spots = ordered(view);
    let mut findings = Vec::new();
    let edits = report.edits();
    for (span, math) in sites {
        let Some((at_span, at)) = next_char(&spots, span) else { continue };
        let Some((start, end)) = argument(&edits, span) else { continue };
        let at_pos = Pos::new(at_span.line, at_span.col);
        if at_pos < start || end <= at_pos {
            continue;
        }
        let Some(letter @ ('i' | 'j')) = view.char_at(at) else { continue };
        let dotless = match (math, letter) {
            (true, 'i') => "\\imath",
            (true, _) => "\\jmath",
            (false, 'i') => "\\i",
            (false, _) => "\\j",
        };
        findings.push(Dotted { span, at, letter, dotless });
    }
    for finding in findings {
        let edits = text::replace(&report.edits(), view, finding.at..finding.at + 1, finding.dotless);
        let letter = finding.letter;
        report.add_fix(
            finding.span,
            &letter.to_string(),
            format!("the accent sits on {} `{letter}`, whose dot stays under it", article(letter)),
            Fix::new(format!("write `{}`", finding.dotless), Applicability::Unsafe, edits),
        );
    }
}
