//! The content the document writes into the PDF itself: `\pdfliteral`,
//! `\pdfobj`, `\pdfpageresources` and `pdf:` specials, as the run observed
//! them.

use std::collections::BTreeSet;

use crate::builtins::OccKind;
use crate::lint::{Category, Report, Rule, Severity};
use crate::machine::Analysis;
use crate::plugin::pdf::{classify, names, operators, Operator, Payload};
use crate::tex::Span;

pub static RULES: &[Rule] = &[
    Rule {
        code: "unbalanced-pdf-content",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a PDF literal opens marked content or a graphics state that is never closed",
        explanation: "\
Marked content (`BMC`/`BDC` … `EMC`) and saved graphics states (`q` … `Q`)
have to nest within a page's content stream (PDF 32000, §§ 8.4.2, 14.6).  An
`EMC` or `Q` without its opening operator, or one left open, makes viewers
reject the page or show an optional content layer on every page after it.
Optional content packages (ocgx2, ocg-p, pdfbase) write these operators with
`\\pdfliteral` or `\\special{pdf:…}` at the start and end of an environment.

satex does not break pages, so it checks the order in which the run executes
the literals; a pair split by a page break is not seen.

Fix by closing what is opened in the same group, or by using one environment
for both ends.",
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
        run: undefined_optional_content,
    },
];

/// Every payload in run order, parsed by what it is.
fn payloads(analysis: &Analysis) -> impl Iterator<Item = (Span, Payload<'_>)> {
    analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Pdf)
        .map(|o| (o.span, classify(o.detail.as_deref().unwrap_or(""), &o.key)))
}

fn unbalanced_pdf_content(report: &mut Report) {
    let mut open: Vec<(Operator, Span)> = Vec::new();
    let mut findings = Vec::new();
    for (span, payload) in payloads(report.analysis) {
        let Payload::Content(content) = payload else { continue };
        for operator in operators(content) {
            if operator.opens() {
                open.push((operator, span));
                continue;
            }
            match open.iter().rposition(|(o, _)| o.closer() == operator.closer()) {
                Some(at) if at == open.len() - 1 => {
                    open.pop();
                }
                Some(at) => {
                    let (inner, inner_span) = &open[open.len() - 1];
                    findings.push((
                        span,
                        operator.name(),
                        format!(
                            "{} closes the {} at {} while the {} at {inner_span} is still open",
                            operator.name(),
                            open[at].0.name(),
                            open[at].1,
                            inner.name()
                        ),
                    ));
                    open.truncate(at);
                }
                None => findings.push((
                    span,
                    operator.name(),
                    format!("{} has no {} before it to close", operator.name(), opener(&operator)),
                )),
            }
        }
    }
    for (operator, span) in open {
        findings.push((
            span,
            operator.name(),
            format!("{} is never closed by {}", operator.name(), operator.closer()),
        ));
    }
    for (span, name, message) in findings {
        let fix = format!("balance {name} within the same group");
        report.add(span, name, message, Some(fix));
    }
}

fn opener(operator: &Operator) -> &'static str {
    match operator.closer() {
        "EMC" => "BMC or BDC",
        _ => "q",
    }
}

fn undefined_optional_content(report: &mut Report) {
    let analysis = report.analysis;
    let mut known = BTreeSet::new();
    let mut references = Vec::new();
    for (span, payload) in payloads(analysis) {
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
