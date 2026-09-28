//! The content the document writes into the PDF itself: `\pdfliteral`,
//! `\pdfobj`, `\pdfpageresources` and `pdf:` specials, as the run observed
//! them.

use std::collections::BTreeSet;

use crate::builtins::OccKind;
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::plugin::pdf::{Operator, Payload, classify, names, operators};
use crate::tex::Span;

pub static RULES: &[Rule] = &[
    Rule {
        code: "unbalanced-pdf-content",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a PDF literal opens marked content or a graphics state that is never closed",
        explanation: "\
`\\pdfliteral` and `\\special{pdf:…}` (what ocgx2, ocg-p and pdfbase use to
write raw PDF) drop `q`/`Q` and `BMC`/`BDC`/`EMC` straight into the page's
content stream.  `q`/`Q` save and restore the graphics state — colour, line
width, the current transform; `BMC`/`BDC`/`EMC` bracket marked content, an
optional content group among them.  Both have to nest within the page (PDF
32000, §§ 8.4.2, 14.6).  One left open leaks its state into everything
typeset after it; a viewer may also just reject the page.

satex does not break pages into content streams itself, so it checks the
order the run executes the literals in.

Fix by closing what is opened in the same group, or by using one environment
for both ends.",
        references: &[Ref { source: Source::Pdf, at: "8.4.2" }, Ref { source: Source::Pdf, at: "14.6" }],
        run: unbalanced_pdf_content,
    },
    Rule {
        code: "undefined-optional-content",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "marked content refers to an optional content group no resource names",
        explanation: "\
`/OC /name BDC` refers to `/name` in the page's `/Properties` resources, which
maps it to an optional content group (PDF 32000, § 8.11.3.2).  A name that no
`\\pdfpageresources`, `\\pdfobj` or `pdf:` object mentions is unknown to the
viewer, and the content is shown or hidden at its whim.  The rule is silent
while the run observed no such resources at all.

Fix by declaring the group, or by correcting the name.",
        references: &[Ref { source: Source::Pdf, at: "8.11.3.2" }],
        run: undefined_optional_content,
    },
];

/// The construct that hands a payload to the PDF writer, in the document's
/// own spelling, read off the same `detail` [`classify`] itself switches on.
fn emitter(detail: &str) -> &'static str {
    match detail {
        "special" => "\\special{pdf:…}",
        _ => "\\pdfliteral",
    }
}

/// Every payload in run order, parsed by what it is, alongside the construct
/// that wrote it.  A literal the interpreter reaches by more than one path is
/// recorded once per path, all under the same graph node: only the first of
/// those is real content.
fn payloads(analysis: &Analysis) -> impl Iterator<Item = (Span, &'static str, Payload<'_>)> {
    let mut seen = BTreeSet::new();
    analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Pdf).filter(move |o| seen.insert(o.node)).map(|o| {
        let detail = o.detail.as_deref().unwrap_or("");
        (o.span, emitter(detail), classify(detail, &o.key))
    })
}

/// What a `q`/`Q` or `BMC`/`BDC`/`EMC` pair holds, in words a document
/// author writes TeX rather than PDF would recognize.
fn holds(operator: &Operator) -> &'static str {
    match operator {
        Operator::Save | Operator::Restore => "the graphics state",
        Operator::BeginMarked { .. } | Operator::EndMarked => "marked content",
    }
}

/// What is left in force everywhere after `operator`, once it is known to
/// leak past where it should have closed (PDF 32000 §§ 8.4.2, 14.6).
fn leaks(operator: &Operator) -> &'static str {
    match operator {
        Operator::Save | Operator::Restore => {
            "colour, line width and the current transform leak into everything typeset after it"
        }
        Operator::BeginMarked { .. } | Operator::EndMarked => {
            "its optional content layer stays applied to everything typeset after it"
        }
    }
}

fn unbalanced_pdf_content(report: &mut Report) {
    let mut open: Vec<(Operator, Span, &'static str)> = Vec::new();
    let mut findings = Vec::new();
    for (span, emitter, payload) in payloads(report.analysis) {
        let Payload::Content(content) = payload else { continue };
        for operator in operators(content) {
            if operator.opens() {
                open.push((operator, span, emitter));
                continue;
            }
            match open.iter().rposition(|(o, _, _)| o.closer() == operator.closer()) {
                Some(at) if at == open.len() - 1 => {
                    open.pop();
                }
                Some(at) => {
                    let (inner, inner_span, inner_emitter) = &open[open.len() - 1];
                    findings.push((
                        span,
                        operator.name(),
                        format!(
                            "{emitter} closes {} (`{}`), but {inner_emitter} at {inner_span} opened {} (`{}`) that is \
                             still open: {}",
                            holds(&operator),
                            operator.name(),
                            holds(inner),
                            inner.name(),
                            leaks(inner)
                        ),
                        format!(
                            "close `{}` before this `{}`, or move this `{}` after it",
                            inner.name(),
                            operator.name(),
                            operator.name()
                        ),
                    ));
                    open.truncate(at);
                }
                None => findings.push((
                    span,
                    operator.name(),
                    format!(
                        "{emitter} closes {} (`{}`) that nothing here opened: no matching {} precedes it",
                        holds(&operator),
                        operator.name(),
                        opener(&operator)
                    ),
                    format!("remove this `{}`, or add the {} it should close", operator.name(), opener(&operator)),
                )),
            }
        }
    }
    for (operator, span, emitter) in open {
        findings.push((
            span,
            operator.name(),
            format!(
                "{emitter} opens {} (`{}`) and never closes it (`{}`): {}",
                holds(&operator),
                operator.name(),
                operator.closer(),
                leaks(&operator)
            ),
            format!("add the matching `{}` in the same group before it ends", operator.closer()),
        ));
    }
    for (span, name, message, fix) in findings {
        report.add(span, name, message, Some(fix));
    }
}

fn opener(operator: &Operator) -> &'static str {
    match operator.closer() {
        "EMC" => "`BMC` or `BDC`",
        _ => "`q`",
    }
}

fn undefined_optional_content(report: &mut Report) {
    let analysis = report.analysis;
    let mut known = BTreeSet::new();
    let mut references = Vec::new();
    for (span, _, payload) in payloads(analysis) {
        match payload {
            Payload::Object(object) => known.extend(names(object)),
            Payload::Content(content) => {
                for operator in operators(content) {
                    if let Operator::BeginMarked { optional_content: Some(name), .. } = operator {
                        references.push((span, name));
                    }
                }
            }
            Payload::Other => {}
        }
    }
    if known.is_empty() {
        return;
    }
    for (span, name) in references {
        if known.contains(&name) {
            continue;
        }
        report.add(
            span,
            &name,
            format!("/OC /{name} names no optional content group the page resources declare"),
            Some(format!("declare /{name} in the page's /Properties, or correct the name")),
        );
    }
}
