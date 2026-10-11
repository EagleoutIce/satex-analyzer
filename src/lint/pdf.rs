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
content stream.  `q`/`Q` save and restore the graphics state — color, line
width, the current transform; `BMC`/`BDC`/`EMC` bracket marked content, an
optional content group among them.  Both have to nest within the page (PDF
32000, §§ 8.4.2, 14.6).  One left open leaks its state into everything
typeset after it; a viewer may also just reject the page.

satex does not break pages into content streams itself, so it checks the
order the run executes the literals in.

Fix by closing what is opened in the same group, or by using one environment
for both ends.",
        references: &[Ref { source: Source::Pdf, at: "8.4.2" }, Ref { source: Source::Pdf, at: "14.6" }],
        fixes: &[],
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
        fixes: &[],
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
/// recorded once per path, under the same graph node and call site: only
/// the first of those is real content. One macro's literal used at many
/// call sites (accsupp's `\EndAccSupp`) is one node, but a payload at each.
/// Payloads of nested macros share the outermost call site, so a repeat is
/// recognized as a whole: a site's sequence that repeats its own prefix
/// (`B E B E`) is one pass, while `B B E E` is not a repeat of anything.
fn payloads(analysis: &Analysis) -> impl Iterator<Item = (usize, Span, &'static str, Payload<'_>)> {
    let pdf: Vec<_> = analysis.facts.occurrences.iter().enumerate().filter(|(_, o)| o.kind == OccKind::Pdf).collect();
    let mut sites: crate::fixed::HashMap<Span, Vec<usize>> = Default::default();
    for (i, (_, o)) in pdf.iter().enumerate() {
        sites.entry(o.span).or_default().push(i);
    }
    let mut real = vec![true; pdf.len()];
    for at in sites.values() {
        let same = |i: usize, j: usize| pdf[at[i]].1.node == pdf[at[j]].1.node && pdf[at[i]].1.key == pdf[at[j]].1.key;
        let Some(period) = (1..at.len()).find(|&p| (p..at.len()).all(|i| same(i, i - p))) else { continue };
        for &i in &at[period..] {
            real[i] = false;
        }
    }
    pdf.into_iter().zip(real).filter(|(_, real)| *real).map(|((index, o), _)| {
        let detail = o.detail.as_deref().unwrap_or("");
        (index, o.span, emitter(detail), classify(detail, &o.key))
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
            "color, line width and the current transform leak into everything typeset after it"
        }
        Operator::BeginMarked { .. } | Operator::EndMarked => {
            "its optional content layer stays applied to everything typeset after it"
        }
    }
}

fn unbalanced_pdf_content(report: &mut Report) {
    let analysis = report.analysis;
    let mut open: Vec<(Operator, Span, &'static str, usize)> = Vec::new();
    let mut findings = Vec::new();
    for (index, span, emitter, payload) in payloads(analysis) {
        let Payload::Content(content) = payload else { continue };
        for operator in operators(content) {
            if operator.opens() {
                open.push((operator, span, emitter, index));
                continue;
            }
            match open.iter().rposition(|(o, _, _, _)| o.closer() == operator.closer()) {
                Some(at) if at == open.len() - 1 => {
                    open.pop();
                }
                Some(at) => {
                    let (inner, inner_span, inner_emitter, inner_index) = &open[open.len() - 1];
                    findings.push((
                        crate::lint::shared::gap_between(analysis, *inner_index, index),
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
                    crate::lint::shared::doubt(analysis, index),
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
    // What the run lost after an opener may hold its closer.
    for (operator, span, emitter, index) in open {
        findings.push((
            crate::lint::shared::gap_between(analysis, index, usize::MAX),
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
    for (doubt, span, name, message, fix) in findings {
        match doubt {
            Some(cause) => report.add_doubted(span, name, message, &cause),
            None => report.add(span, name, message, Some(fix)),
        }
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
    for (_, span, _, payload) in payloads(analysis) {
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
