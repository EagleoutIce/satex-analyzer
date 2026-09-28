//! Recommendations for the typeset result, decided by what the run observed:
//! the font encoding in force, the accents it had to build, the engine and
//! the output target.

use std::collections::HashSet;

use crate::builtins::OccKind;
use crate::config::Engine;
use crate::lint::fix::{Applicability, Fix};
use crate::lint::shared::document_start;
use crate::lint::text::View;
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::plugin::Output;
use crate::tex::{FileId, Span};

pub static RULES: &[Rule] = &[
    Rule {
        code: "ot1-font-encoding",
        category: Category::Style,
        severity: Severity::Info,
        summary: "accented letters are built with \\accent because the text is set in OT1",
        explanation: "\
OT1, LaTeX's default text encoding, has no accented letters: it builds each
one with the `\\accent` primitive (ot1enc.def, fntguide § 5).  TeX does not
hyphenate a word holding an `\\accent` ([The TeXbook](https://ctan.org/pkg/texbook), appendix H), and the PDF's text
layer holds an accent and a letter instead of the character.

Every character carries the encoding it was set in, so a document whose
default is T1 but which sets one stretch in OT1 is reported for that stretch
alone.

Fix by loading `\\usepackage[T1]{fontenc}`.",
        references: &[
            Ref { source: Source::Source2e, at: "ot1enc.def" },
            Ref { source: Source::Fntguide, at: "5" },
            Ref { source: Source::TeXbook, at: "appendix H" },
        ],
        run: ot1_font_encoding,
    },
    Rule {
        code: "computer-modern-in-t1",
        category: Category::Style,
        severity: Severity::Info,
        summary: "T1 text in Computer Modern relies on cm-super for scalable fonts",
        explanation: "\
Computer Modern in T1 is the EC family, which TeX Live ships as scalable
Type 1 fonts only through cm-super.  Without it the PDF gets bitmap fonts
that look blurred on screen.  [Latin Modern](https://ctan.org/pkg/lm) is the
same design with T1 glyphs of its own.

The rule reads the encoding and the family of every character the document
set, so it fires on the characters really set in T1 `cmr`.

Fix by loading `\\usepackage{lmodern}`, or by making sure cm-super is
installed.",
        references: &[Ref { source: Source::Lmodern, at: "Latin Modern" }, Ref { source: Source::Fntguide, at: "5" }],
        run: computer_modern_in_t1,
    },
    Rule {
        code: "microtype-available",
        category: Category::Style,
        severity: Severity::Info,
        summary: "the engine can protrude characters and expand fonts, but microtype is not loaded",
        explanation: "\
pdfTeX and LuaTeX writing PDF support character protrusion and font expansion
(pdfTeX manual, `\\pdfprotrudechars` and `\\pdfadjustspacing`), which make
margins look straighter and let paragraphs break with fewer hyphens and less
uneven spacing.  microtype turns both on (microtype manual § 1).  The rule
fires when both are still off at the end of the run.  XeTeX only
protrudes, and DVI output supports neither, so the rule stays silent there.

Fix by loading `\\usepackage{microtype}`.",
        references: &[
            Ref { source: Source::Microtype, at: "1" },
            Ref { source: Source::Pdftex, at: "\\pdfprotrudechars" },
        ],
        run: microtype_available,
    },
];

/// The kernel's roman family, which a font package replaces (fntguide § 2.3).
const KERNEL_ROMAN: &str = "cmr";

/// The characters a document set in one NFSS shape: how many there are,
/// where the first one stands, and the lines they cover.
#[derive(Default)]
struct Set {
    count: usize,
    first: Option<Span>,
    lines: HashSet<(FileId, u32)>,
    /// Whether one of them was set in the kernel's roman family.
    kernel_roman: bool,
}

/// The characters the document itself certainly set in `encoding`, with
/// `family` when one is asked for (fntguide § 2).
fn set_in(report: &Report, view: &View, encoding: &str, family: Option<&str>) -> Set {
    let index = report.index();
    let mut out = Set::default();
    for i in 0..view.len() {
        let Some(font) = view.font(i) else { continue };
        if &*font.encoding != encoding || family.is_some_and(|want| &*font.family != want) {
            continue;
        }
        // An undecided path never proves what the reader gets.
        if !view.certain(i) {
            continue;
        }
        let Some(span) = view.span(i) else { continue };
        if index.origin(span.file) != "document" {
            continue;
        }
        out.count += 1;
        out.first.get_or_insert(span);
        out.lines.insert((span.file, span.line));
        out.kernel_roman |= &*font.family == KERNEL_ROMAN;
    }
    out
}

fn ot1_font_encoding(report: &mut Report) {
    let analysis = report.analysis;
    let ot1 = set_in(report, report.text(), "OT1", None);
    let Some(first) = ot1.first else { return };
    let occurrences = &analysis.facts.occurrences;
    let Some(start) = occurrences.iter().position(|o| o.kind == OccKind::BeginEnvironment && o.key == "document")
    else {
        return;
    };
    // Only an accent standing where OT1 text stands is one OT1 had to build.
    let accents: Vec<_> = occurrences[start..]
        .iter()
        .filter(|o| o.kind == OccKind::Accent && ot1.lines.contains(&(o.span.file, o.span.line)))
        .collect();
    let Some(accent) = accents.first() else { return };
    let span = document_start(analysis).unwrap_or(first);
    let packages = match ot1.kernel_roman {
        true => "\\usepackage[T1]{fontenc}\n\\usepackage{lmodern}",
        false => "\\usepackage[T1]{fontenc}",
    };
    let edits = document_start(analysis).and_then(|begin| report.edits().insert_before(begin, packages));
    report.add_fix(
        span,
        "OT1",
        format!(
            "the document set {} characters in OT1, first at {}, which builds accented letters with \\accent \
             ({} in that text, first at {}): TeX does not hyphenate words containing them and the PDF text holds \
             accent and letter apart",
            ot1.count,
            report.place(first),
            accents.len(),
            report.place(accent.span)
        ),
        Fix::new(
            if ot1.kernel_roman {
                "load \\usepackage[T1]{fontenc} and \\usepackage{lmodern}"
            } else {
                "load \\usepackage[T1]{fontenc}"
            },
            Applicability::Unsafe,
            edits,
        ),
    );
}

fn computer_modern_in_t1(report: &mut Report) {
    let analysis = report.analysis;
    let ec = set_in(report, report.text(), "T1", Some(KERNEL_ROMAN));
    let Some(first) = ec.first else { return };
    let span = document_start(analysis).unwrap_or(first);
    let edits = report.edits().insert_before(span, "\\usepackage{lmodern}");
    report.add_fix(
        span,
        "T1",
        format!(
            "the document set {} characters in T1 Computer Modern, first at {}, which is the EC fonts: they \
             are scalable only when cm-super is installed",
            ec.count,
            report.place(first)
        ),
        Fix::new("load \\usepackage{lmodern}, or install cm-super", Applicability::Unsafe, edits),
    );
}

fn microtype_available(report: &mut Report) {
    let analysis = report.analysis;
    let plugins = &analysis.plugins;
    if plugins.output != Output::Pdf || !matches!(plugins.engine, Engine::PdfTeX | Engine::LuaTeX) {
        return;
    }
    // Either feature switched on, by microtype or by hand, is enough.  So is
    // a value the run could not decide.
    let [protrude, adjust] = match plugins.engine {
        Engine::LuaTeX => ["protrudechars", "adjustspacing"],
        _ => ["pdfprotrudechars", "pdfadjustspacing"],
    };
    let off = |name: &str| analysis.interner.lookup(name).is_none_or(|sym| analysis.env.value(sym).as_int() == Some(0));
    if !(off(protrude) && off(adjust)) {
        return;
    }
    let Some(span) = document_start(analysis) else { return };
    let edits = report.edits().insert_before(span, "\\usepackage{microtype}");
    report.add_fix(
        span,
        "microtype",
        format!(
            "{} writes PDF and can protrude characters and expand fonts, but microtype is not loaded",
            plugins.engine.as_str()
        ),
        Fix::new("load \\usepackage{microtype}", Applicability::Unsafe, edits),
    );
}
