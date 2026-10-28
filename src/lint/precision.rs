use crate::fixed::HashMap;

use crate::lint::{Category, Report, Rule, Severity};

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
        run: expected_load,
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

fn imprecision(report: &mut Report) {
    let mut order: Vec<(crate::tex::Span, &'static str, String)> = Vec::new();
    let mut counts: HashMap<(&'static str, String), usize> = HashMap::default();
    for diagnostic in &report.analysis.facts.diagnostics {
        if matches!(diagnostic.code, "undefined-environment" | "already-defined" | "environment-mismatch") {
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
