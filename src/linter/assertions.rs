//! The outcomes of `%\satex@assert`, `%\satex@assume` and
//! `%\satex@configure` ([`crate::analysis::machine::directive`]), joined over every
//! path that read them.

use std::collections::BTreeMap;

use crate::analysis::machine::Analysis;
use crate::analysis::machine::directive::{ASSUMED, CODES, FAILS, HOLDS, INVALID, OPEN, SKIPPED};
use crate::engine::tex::Span;
use crate::linter::{Category, Report, Rule, Severity};

const SYNTAX: &str = "\
A directive is a comment, so TeX skips it: `%\\satex@assert{cond}`,
`%\\satex@assume{cond}` or `%\\satex@configure{key=value, ...}`.  A condition
compares two TeX numbers (`\\thepage < 5`, `\\value{section} >= 2`) or two
dimensions when one has a unit (`\\wd0 <= 3cm`), with `<`, `<=`, `=`, `!=`,
`>=` or `>`.  It may also be a TeX conditional such as `\\ifdefined\\foo`.
`and`, `or` and `not` combine tests.  `\\the<counter>` is the counter's value,
however `\\pagenumbering` prints it.";

pub static RULES: &[Rule] = &[
    Rule {
        code: "assertion-refuted",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "an assertion fails on a run of the document",
        explanation: "\
The condition of `%\\satex@assert` is false on a path that approximated
nothing, so a run of the document reaches it false.  The finding names the
values the condition read there.",
        references: &[],
        fixes: &[],
        engines: &[],
        run: report,
    },
    Rule {
        code: "assertion-unknown",
        category: Category::Precision,
        severity: Severity::Info,
        summary: "an assertion could not be decided",
        explanation: "\
The condition of `%\\satex@assert` is neither true on every path nor false on
an exact one: a value it reads is not known, a path approximated, or no path
reads the assertion.  `%\\satex@assume` can narrow what it reads.",
        references: &[],
        fixes: &[],
        engines: &[],
        run: report,
    },
    Rule {
        code: "assertion-proved",
        category: Category::Correctness,
        severity: Severity::Info,
        summary: "an assertion holds on every path",
        explanation: "\
The condition of `%\\satex@assert` is true on every path that reads it.  The
paths cover every run of the document, so the property holds, under the
assumptions the run reports.",
        references: &[],
        fixes: &[],
        engines: &[],
        run: report,
    },
    Rule {
        code: "assumption-made",
        category: Category::Correctness,
        severity: Severity::Info,
        summary: "the document assumes a condition",
        explanation: "\
`%\\satex@assume` drops the paths where its condition is false and narrows the
registers it compares on the others.  What the run proves after it holds for
the runs where the assumption does.",
        references: &[],
        fixes: &[],
        engines: &[],
        run: report,
    },
    Rule {
        code: "invalid-directive",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "a `%\\satex@` directive cannot be read",
        explanation: SYNTAX,
        references: &[],
        fixes: &[],
        engines: &[],
        run: report,
    },
];

/// Each directive's messages per outcome code, in source order.
fn outcomes(analysis: &Analysis) -> BTreeMap<Span, BTreeMap<&'static str, Vec<String>>> {
    let mut out: BTreeMap<Span, BTreeMap<&'static str, Vec<String>>> = BTreeMap::new();
    for d in &analysis.facts.diagnostics {
        if CODES.contains(&d.code) {
            let messages = out.entry(d.span).or_default().entry(d.code).or_default();
            if !messages.contains(&d.message) {
                messages.push(d.message.clone());
            }
        }
    }
    out
}

/// The path's message with "on every path" after the condition: `c` holds on every path: what it read.
fn proof(message: &str) -> String {
    match message.split_once("` holds") {
        Some((cond, rest)) => format!("{cond}` holds on every path{rest}"),
        None => format!("{message} on every path"),
    }
}

fn first(messages: Option<&Vec<String>>) -> Option<String> {
    let messages = messages?;
    let more = messages.len().saturating_sub(1);
    let first = messages.first()?.clone();
    Some(if more > 0 { format!("{first} (and on {more} more paths)") } else { first })
}

/// Every directive's findings, joined over the paths: where, the rule and
/// the message.
pub fn verdicts(analysis: &Analysis) -> Vec<(Span, &'static str, String)> {
    let mut out = Vec::new();
    for (span, by) in outcomes(analysis) {
        let unreached = || {
            let skipped = by.get(SKIPPED)?.iter().find(|t| t.starts_with("\\satex@assert"))?;
            Some(format!("no path reads `{skipped}`"))
        };
        let assertion = match (by.contains_key(FAILS), by.contains_key(OPEN), by.contains_key(HOLDS)) {
            (true, ..) => first(by.get(FAILS)).map(|m| ("assertion-refuted", m)),
            (_, true, _) => first(by.get(OPEN)).map(|m| ("assertion-unknown", m)),
            (.., true) => first(by.get(HOLDS)).map(|m| ("assertion-proved", proof(&m))),
            _ => unreached().map(|m| ("assertion-unknown", m)),
        };
        let assumed = by.get(ASSUMED).map(|m| ("assumption-made", m.join(". ")));
        let invalid = first(by.get(INVALID)).map(|m| ("invalid-directive", m));
        out.extend([assertion, assumed, invalid].into_iter().flatten().map(|(code, m)| (span, code, m)));
    }
    out
}

fn report(report: &mut Report) {
    let code = report.rule.code;
    for (span, _, message) in verdicts(report.analysis).into_iter().filter(|(_, c, _)| *c == code) {
        report.add(span, code, message, None);
    }
}
