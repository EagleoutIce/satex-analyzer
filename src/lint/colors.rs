//! Colours xcolor stores and reads through `\color@⟨name⟩` (xcolor § 2.5),
//! and the `\pdfcolorstack`s the run executed (pdfTeX manual § 8.9.8), as
//! [`crate::observe`] recorded them.

use std::collections::HashMap;

use crate::facts::{ColorDef, CsnameRole, StackOp};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;

pub static RULES: &[Rule] = &[
    Rule {
        code: "undefined-color",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "a colour is used, or aliased from, before xcolor ever defines it",
        explanation: "\
xcolor reaches every colour through `\\color@⟨name⟩` (xcolor § 2.5). A name
the run never defined there raises `Undefined color` when xcolor itself
looks it up.  The rule reads the same lookup the run made: a name a
`\\color`, `\\textcolor` or `\\colorlet` reached while nothing had defined
it.

Fix by defining the colour first, or by correcting its name.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        run: undefined_color,
    },
    Rule {
        code: "unused-color",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a colour the document defines is never used",
        explanation: "\
The document names a colour with `\\definecolor`, `\\providecolor` or
`\\colorlet` and never reads it back through `\\color`, `\\textcolor` or a
`\\colorlet` source.  Harmless, but likely a leftover.

Silent for a colour a package defines: a library may define more colours
than any one document uses.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        run: unused_color,
    },
    Rule {
        code: "conflicting-color-definition",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "the same colour name is defined twice with different colours",
        explanation: "\
Two `\\definecolor`-family calls store different colours under the same
name. Only the last one is ever read back.  Usually a copy-paste slip.

Fix by giving the second colour its own name, or by removing the earlier
definition.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        run: conflicting_color_definition,
    },
    Rule {
        code: "duplicate-color-value",
        category: Category::Style,
        severity: Severity::Info,
        summary: "two colour names store the exact same colour",
        explanation: "\
Two `\\definecolor`-family calls, at least one of them the document's own,
store the same model and components under different names.  Not wrong, but
one name likely means to be the other.

Silent unless the run could read both colours back exactly.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        run: duplicate_color_value,
    },
    Rule {
        code: "unbalanced-color-stack",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a `\\pdfcolorstack` push is never popped, or a pop has nothing to pop",
        explanation: "\
`\\pdfcolorstack⟨stack⟩ push` and `pop` nest like `q`/`Q` in the page's own
content stream (pdfTeX manual, § 8.9.8): a viewer that reads the stack back
after the page still expects it empty.  A stack number the run could not
decide is left out, since satex would otherwise be guessing which stack a
push or pop belongs to.

Fix by balancing every push with a pop in the same group.",
        references: &[Ref { source: Source::Pdftex, at: "8.9.8" }],
        run: unbalanced_color_stack,
    },
];

/// Whether colour-definition findings apply here: a document defines
/// colours for its own prose, but so does a package meant to be loaded by
/// one. Only the profiles [`crate::lint::text`] also treats as code stay
/// silent.
fn definitions_matter(analysis: &Analysis) -> bool {
    !matches!(analysis.settings.profile, Some(crate::config::Profile::Literate))
}

fn undefined_color(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    for use_ in &analysis.facts.color_uses {
        // A `\ifx`/`\@ifundefinedcolor`-style test deliberately asks
        // whether the colour exists. Only reading or aliasing it assumes
        // that it does.
        if use_.defined || matches!(use_.role, CsnameRole::Test) {
            continue;
        }
        let severity = if use_.certain { Severity::Error } else { Severity::Warning };
        report.add_as(
            severity,
            use_.span,
            &use_.name,
            format!("colour `{}` is used but never defined", use_.name),
            None,
        );
    }
}

fn unused_color(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    let used: std::collections::HashSet<&str> = analysis
        .facts
        .color_uses
        .iter()
        .filter(|u| matches!(u.role, CsnameRole::Use | CsnameRole::LetTo))
        .map(|u| u.name.as_str())
        .collect();
    for def in &analysis.facts.colors {
        if def.package.is_some() || used.contains(def.name.as_str()) {
            continue;
        }
        report.add(def.span, &def.name, format!("colour `{}` is never used", def.name), None);
    }
}

fn conflicting_color_definition(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    let mut by_name: HashMap<&str, Vec<&ColorDef>> = HashMap::new();
    for def in &analysis.facts.colors {
        by_name.entry(def.name.as_str()).or_default().push(def);
    }
    for (name, defs) in by_name {
        for pair in defs.windows(2) {
            let [first, second] = pair else { continue };
            let (Some(a), Some(b)) = (&first.value, &second.value) else { continue };
            if !first.certain || !second.certain || a == b {
                continue;
            }
            report.add(
                second.span,
                name,
                format!(
                    "colour `{name}` is redefined here as {}, but was {} at {}",
                    b.hull.spell(),
                    a.hull.spell(),
                    first.span
                ),
                None,
            );
        }
    }
}

fn duplicate_color_value(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    let exact: Vec<&ColorDef> = analysis.facts.colors.iter().filter(|d| d.certain && d.value.is_some()).collect();
    let mut seen: std::collections::HashSet<(&str, &str)> = std::collections::HashSet::new();
    for (i, a) in exact.iter().enumerate() {
        if a.package.is_none() {
            continue;
        }
        for b in exact.iter().skip(i + 1) {
            if a.name == b.name || (a.package.is_some() && b.package.is_some()) {
                continue;
            }
            if a.value != b.value {
                continue;
            }
            let key =
                if a.name < b.name { (a.name.as_str(), b.name.as_str()) } else { (b.name.as_str(), a.name.as_str()) };
            if !seen.insert(key) {
                continue;
            }
            let (doc, other) = if a.package.is_none() { (a, b) } else { (b, a) };
            report.add(
                doc.span,
                &doc.name,
                format!(
                    "colour `{}` is exactly `{}` ({})",
                    doc.name,
                    other.name,
                    doc.value.as_ref().unwrap().hull.spell()
                ),
                None,
            );
        }
    }
}

fn unbalanced_color_stack(report: &mut Report) {
    let analysis = report.analysis();
    let mut open: HashMap<i64, Vec<crate::tex::Span>> = HashMap::new();
    let mut findings = Vec::new();
    for entry in &analysis.facts.color_stack {
        let Some(stack) = entry.stack else { continue };
        match entry.op {
            StackOp::Push => open.entry(stack).or_default().push(entry.span),
            StackOp::Pop => match open.get_mut(&stack) {
                Some(pushes) if !pushes.is_empty() => {
                    pushes.pop();
                }
                _ => findings.push((entry.span, format!("stack {stack} is popped with nothing pushed to it"))),
            },
            StackOp::Set | StackOp::Current => {}
        }
    }
    for (stack, pushes) in open {
        for span in pushes {
            findings.push((span, format!("stack {stack} is pushed here and never popped")));
        }
    }
    for (span, message) in findings {
        report.add(span, "pdfcolorstack", message, None);
    }
}
