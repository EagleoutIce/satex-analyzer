use std::collections::HashMap;

use crate::lint::{Category, Report, Rule, Severity};

pub static RULES: &[Rule] = &[Rule {
    code: "analysis-imprecision",
    category: Category::Precision,
    severity: Severity::Info,
    summary: "the analysis had to widen",
    explanation: "\
Undecided conditionals are analyzed arm by arm, so they are not reported here.
This rule reports the two places where the analysis is incomplete instead:
a recursion that was widened, and a budget that was reached.  Findings that
depend on such a point may be incomplete.

There is nothing to fix in the document; raising the limits in `satex.yaml` or
narrowing the analysis with `load_packages: false` can reduce it.",
    run: imprecision,
}];

fn imprecision(report: &mut Report) {
    let mut order: Vec<(crate::tex::Span, &'static str, String)> = Vec::new();
    let mut counts: HashMap<(&'static str, String), usize> = HashMap::new();
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
