//! Recommendations for the typeset result, decided by what the run observed:
//! the font encoding in force, the accents it had to build, the engine and
//! the output target.

use crate::builtins::OccKind;
use crate::config::Engine;
use crate::lint::fix::{Applicability, Fix};
use crate::lint::shared::document_start;
use crate::lint::{Category, Report, Rule, Severity};
use crate::machine::Analysis;
use crate::plugin::Output;
use crate::tex::Meaning;

pub static RULES: &[Rule] = &[
    Rule {
        code: "ot1-font-encoding",
        category: Category::Style,
        severity: Severity::Info,
        summary: "accented letters are built with \\accent because the text is set in OT1",
        explanation: "\
OT1, LaTeX's default text encoding, has no accented letters: the encoding
builds each one with the `\\accent` primitive (ot1enc.def; fntguide § 5).  TeX
does not hyphenate a word that contains an `\\accent` ([The TeXbook](https://ctan.org/pkg/texbook), appendix H),
and the PDF's text layer holds an accent and a letter instead of the character,
which breaks search and copy.  T1 has the accented letters as glyphs, and the
same encoding file then turns them into single characters.

The rule fires when `\\encodingdefault` is OT1 and the body really executed
`\\accent`; how the letters were typed, `\\\"a` or `ä`, does not matter.

Fix by loading `\\usepackage[T1]{fontenc}`.",
        run: ot1_font_encoding,
    },
    Rule {
        code: "computer-modern-in-t1",
        category: Category::Style,
        severity: Severity::Info,
        summary: "T1 text in Computer Modern relies on cm-super for scalable fonts",
        explanation: "\
Computer Modern in T1 is the EC family, which TeX Live provides as scalable
Type 1 fonts only through the cm-super package; without it the PDF gets
bitmap fonts that look blurred on screen.  Latin Modern is the same design
with T1 glyphs of its own (lmodern documentation).  The rule fires when
`\\encodingdefault` is T1 and `\\rmdefault` is still the kernel's `cmr`.

Fix by loading `\\usepackage{lmodern}`, or by making sure cm-super is installed.",
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
        run: microtype_available,
    },
];

/// The text a parameterless macro stands for at the end of the run.
fn macro_text(analysis: &Analysis, name: &str) -> Option<String> {
    let sym = analysis.interner.lookup(name)?;
    match analysis.env.meaning(sym) {
        Meaning::Macro(m) if m.arity() == 0 => Some(crate::tex::text_of(&m.replacement_text, &analysis.interner)),
        _ => None,
    }
}

/// The encoding `\encodingdefault` names, which `\begin{document}` selects
/// for the body (source2e, ltfssini.dtx).
fn encoding(analysis: &Analysis) -> Option<String> {
    macro_text(analysis, "encodingdefault")
}

/// The kernel's roman family, which a font package replaces (fntguide § 2.3).
const KERNEL_ROMAN: &str = "cmr";

fn ot1_font_encoding(report: &mut Report) {
    let analysis = report.analysis;
    if encoding(analysis).as_deref() != Some("OT1") {
        return;
    }
    let occurrences = &analysis.facts.occurrences;
    let Some(start) = occurrences.iter().position(|o| o.kind == OccKind::BeginEnvironment && o.key == "document")
    else {
        return;
    };
    let accents: Vec<_> = occurrences[start..].iter().filter(|o| o.kind == OccKind::Accent).collect();
    let Some(first) = accents.first() else { return };
    let span = document_start(analysis).unwrap_or(first.span);
    let lmodern = macro_text(analysis, "rmdefault").as_deref() == Some(KERNEL_ROMAN);
    let packages = match lmodern {
        true => "\\usepackage[T1]{fontenc}\n\\usepackage{lmodern}",
        false => "\\usepackage[T1]{fontenc}",
    };
    let edits = document_start(analysis).and_then(|begin| report.edits().insert_before(begin, packages));
    report.add_fix(
        span,
        "OT1",
        format!(
            "the text is set in OT1, which builds accented letters with \\accent ({} in the body, first at {}): \
             TeX does not hyphenate words containing them and the PDF text holds accent and letter apart",
            accents.len(),
            first.span
        ),
        Fix::new(
            if lmodern {
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
    if encoding(analysis).as_deref() != Some("T1") || macro_text(analysis, "rmdefault").as_deref() != Some(KERNEL_ROMAN)
    {
        return;
    }
    let Some(span) = document_start(analysis) else { return };
    let edits = report.edits().insert_before(span, "\\usepackage{lmodern}");
    report.add_fix(
        span,
        "T1",
        "T1 text in Computer Modern is set in the EC fonts, which are scalable only when cm-super is installed".into(),
        Fix::new("load \\usepackage{lmodern}, or install cm-super", Applicability::Unsafe, edits),
    );
}

fn microtype_available(report: &mut Report) {
    let analysis = report.analysis;
    let plugins = &analysis.plugins;
    if plugins.output != Output::Pdf || !matches!(plugins.engine, Engine::PdfTeX | Engine::LuaTeX) {
        return;
    }
    // Either feature switched on, by microtype or by hand, is enough; so is
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
