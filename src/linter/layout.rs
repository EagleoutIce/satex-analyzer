//! Preflight checks of the typeset page: lines and boxes TeX packs wider
//! than they may be. Reported only where the run knew every width, on
//! every visit ([`crate::analysis::facts::Overfull`]); never guessed.

use crate::analysis::facts::Wide;
use crate::engine::value::render_dimen;
use crate::linter::fix::{Applicability, Fix};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};

pub static RULES: &[Rule] = &[
    Rule {
        code: "overfull-line",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a line of the paragraph is wider than the text block",
        explanation: "\
TeX breaks a paragraph into lines and packs each to the line width (tex.web
§ 889). A line whose natural width, less all its glue can shrink, still
passes the width sticks out into the right margin, and TeX writes
`Overfull \\hbox (…pt too wide) in paragraph` to the log (§ 666).

satex runs TeX's own line breaker (§§ 813-890) on the paragraph, so it
reports exactly the lines TeX reports, at the user's line that ended the
paragraph, by the same amount. It stays silent where any width, font or
parameter is not known, where character protrusion or font expansion
(microtype) is on, and where the paragraph may be set differently on
another path.

Fix the text, not the measure: allow a hyphenation point in the long word
(`\\-`, or `\\hyphenation{…}`), let the paragraph space out
(`\\begin{sloppypar}` or `\\emergencystretch`), or rephrase it. satex
never rewrites the text itself.",
        references: &[
            Ref { source: Source::TeXWeb, at: "889" },
            Ref { source: Source::TeXWeb, at: "666" },
            Ref { source: Source::TeXbook, at: "chapter 14" },
        ],
        fixes: &[],
        engines: &[],
        run: overfull_line,
    },
    Rule {
        code: "overfull-box",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "an `\\hbox to` holds more than its width",
        explanation: "\
`\\hbox to ⟨width⟩{…}`, and the LaTeX boxes built on it such as
`\\makebox[⟨width⟩][s]{…}`, pack their material to a given width (tex.web
§ 649). Material wider than that, after all its glue shrank, sticks out of
the box, and TeX writes `Overfull \\hbox (…pt too wide) detected at line …`
to the log (§ 666). The rule reports that line's box by the same amount,
only where its material and width were known on every visit.

Fix by widening the box, shortening its material, or giving it infinitely
shrinkable glue (`\\hss`) where sticking out is meant.",
        references: &[Ref { source: Source::TeXWeb, at: "664" }, Ref { source: Source::TeXWeb, at: "666" }],
        fixes: &[],
        engines: &[],
        run: overfull_box,
    },
    Rule {
        code: "wider-than-line",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a box set in a paragraph is wider than its lines",
        explanation: "\
A box or rule set in a paragraph, such as an `\\includegraphics`, a
`tabular` or a `tikzpicture`, that is wider than the line width
(`\\linewidth`) cannot fit on any line: it sticks out into the margin
however the paragraph breaks (tex.web § 889). The rule reports the call
that set it, by how much it passes the line, where its width and the line
width were known on every visit.

An `\\includegraphics` whose `width=` is the reason gets the fix
`width=\\linewidth`, where every other key leaves that its final width (no
`height`, `scale`, `angle`, `bb` or a key satex does not know, no comment); it changes the picture's size, so it is unsafe. Otherwise
scale the material down (`\\resizebox`, a smaller font), or let it stick
out on purpose (`\\makebox[\\linewidth]{…}`), which TeX does not report.",
        references: &[Ref { source: Source::TeXWeb, at: "889" }, Ref { source: Source::Graphics, at: "4.4" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: wider_than_line,
    },
];

/// The places of `kind` that were surely overfull, in source order.
fn sure(report: &Report, kind: Wide) -> Vec<(crate::engine::tex::Span, crate::analysis::facts::Overfull)> {
    let facts = &report.analysis().facts.overfull;
    let mut found: Vec<_> = facts
        .iter()
        .filter(|((_, k), o)| *k == kind && !o.unsure && o.count > 0)
        .map(|((span, _), o)| (*span, *o))
        .collect();
    found.sort_by_key(|(s, _)| (s.file, s.line, s.col));
    found
}

fn overfull_line(report: &mut Report) {
    for (span, o) in sure(report, Wide::Line) {
        let (target, excess) = (render_dimen(o.target), render_dimen(o.excess));
        let message = match o.count {
            1 => format!("a line of the paragraph ending here is {excess} too wide for its {target} line"),
            n => {
                format!("{n} lines of the paragraph ending here are too wide for their {target} line, one by {excess}")
            }
        };
        let hint = "allow a hyphenation point (`\\-`), set the paragraph in `sloppypar`, or rephrase it";
        report.add(span, "overfull-line", message, Some(hint.to_string()));
    }
}

fn overfull_box(report: &mut Report) {
    for (span, o) in sure(report, Wide::Box) {
        let message = format!(
            "the box closed here is {} too wide for its {} width",
            render_dimen(o.excess),
            render_dimen(o.target)
        );
        report.add(span, "overfull-box", message, None);
    }
}

fn wider_than_line(report: &mut Report) {
    for (span, o) in sure(report, Wide::Content) {
        let message = format!(
            "this is {} wider than the {} line and sticks out into the margin",
            render_dimen(o.excess),
            render_dimen(o.target)
        );
        // The keys that leave `width=` the picture's final width (grfguide § 4.4).
        let allowed = [
            "width",
            "keepaspectratio",
            "clip",
            "trim",
            "viewport",
            "page",
            "pagebox",
            "draft",
            "interpolate",
            "alt",
            "type",
            "ext",
            "read",
            "command",
            "quiet",
            "decodearray",
            "hiresbb",
        ];
        let edits = report.edits().replace_key(span, "includegraphics", "width", "\\linewidth", &allowed);
        match edits {
            Some(edits) => {
                let fix = Fix::new("set its `width=\\linewidth`", Applicability::Unsafe, Some(edits));
                report.add_fix(span, "wider-than-line", message, fix);
            }
            None => report.add(span, "wider-than-line", message, Some("scale it to `\\linewidth`".to_string())),
        }
    }
}
