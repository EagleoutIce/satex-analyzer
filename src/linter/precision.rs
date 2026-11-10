use crate::fixed::HashMap;

use crate::linter::{Category, Report, Rule, Severity};

pub static RULES: &[Rule] = &[
    Rule {
        code: "expected-load",
        category: Category::Correctness,
        severity: Severity::Info,
        summary: "the file was read after a package it expects loaded",
        explanation: "\
A package or class read on its own may use names that another one defines,
as a beamer theme uses beamer's.  When `load.expect_detect` is on, SaTeX looks
up which classes and packages on disk define those names, and reads the file
again after the one that defines most of them.  It keeps that run when it
leaves fewer names undefined.

Fix by naming what the file expects in `load.expect`, or by loading it with
`\\RequirePackage` in the file itself.",
        references: &[],
        fixes: &[],
        engines: &[],
        run: expected_load,
    },
    Rule {
        code: "unread-document",
        category: Category::Precision,
        severity: Severity::Error,
        summary: "the analysis did not read all of the document",
        explanation: "\
A file the document's own `\\input`, `\\include` or `\\subfile` names exists but
the run never read it, or a scan read on past the document's end.  Every finding
about what that part defines, typesets or references is then missing, and
findings that something is never defined or never used may be wrong.

There is nothing to fix in the document: this is a gap of the analysis.  Report
it.  Raising the limits in `satex.yaml` may close it.",
        references: &[],
        fixes: &[],
        engines: &[],
        run: unread_document,
    },
    Rule {
        code: "analysis-imprecision",
        category: Category::Precision,
        severity: Severity::Info,
        summary: "the analysis had to widen",
        explanation: "\
Undecided conditionals are analyzed arm by arm, so they are not reported here.
This rule reports the two places where the analysis is incomplete instead:
a recursion that was widened, and a budget that was reached.  Findings that
depend on such a point may be incomplete.

There is nothing to fix in the document.  Raising the limits in `satex.yaml` or
narrowing the analysis with `load.packages: false` can reduce it.",
        references: &[],
        fixes: &[],
        engines: &[],
        run: imprecision,
    },
];

fn expected_load(report: &mut Report) {
    let found: Vec<_> = report
        .analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.code == "expected-load")
        .map(|d| (d.span, d.message.clone()))
        .collect();
    for (span, message) in found {
        report.add(span, "expected-load", message, None);
    }
}

/// Each part of the document the run did not read, at the place that names it.
fn unread_document(report: &mut Report) {
    let found: Vec<_> = (report.analysis.facts.diagnostics.iter())
        .filter(|d| matches!(d.code, "file-not-read" | "analysis-ended-early"))
        .map(|d| (d.span, d.message.clone()))
        .collect();
    for (span, message) in found {
        report.add(span, "unread-document", message, None);
    }
}

fn imprecision(report: &mut Report) {
    let mut order: Vec<(crate::engine::tex::Span, &'static str, String)> = Vec::new();
    let mut counts: HashMap<(&'static str, String), usize> = HashMap::default();
    for diagnostic in &report.analysis.facts.diagnostics {
        if matches!(diagnostic.code, "undefined-environment" | "already-defined" | "environment-mismatch")
            || crate::analysis::machine::directive::CODES.contains(&diagnostic.code)
        {
            continue;
        }
        let key = (diagnostic.code, diagnostic.message.clone());
        let count = counts.entry(key).or_default();
        if *count == 0 {
            order.push((diagnostic.span, diagnostic.code, diagnostic.message.clone()));
        }
        *count += 1;
    }
    for (span, code, message) in order {
        let count = counts.get(&(code, message.clone())).copied().unwrap_or(1);
        let message = if count > 1 { format!("{message} ({count} sites)") } else { message };
        report.add(span, code, message, None);
    }
}
