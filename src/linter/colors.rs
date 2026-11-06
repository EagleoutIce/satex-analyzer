//! Colors xcolor stores and reads through `\color@⟨name⟩` (xcolor § 2.5),
//! and the `\pdfcolorstack`s the run executed (pdfTeX manual § 8.9.8), as
//! [`crate::analysis::observe`] recorded them.

use crate::fixed::HashMap;

use crate::analysis::facts::{ColorDef, CsnameRole, StackOp};
use crate::analysis::machine::Analysis;
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};
use crate::project::config::Engine;

pub static RULES: &[Rule] = &[
    Rule {
        code: "undefined-color",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "a color is used, or aliased from, before xcolor ever defines it",
        explanation: "\
xcolor reaches every color through `\\color@⟨name⟩` (xcolor § 2.5). A name
the run never defined there raises `Undefined color` when xcolor itself
looks it up.  The rule reads the same lookup the run made: a name a
`\\color`, `\\textcolor` or `\\colorlet` reached while nothing had defined
it.

Fix by defining the color first, or by correcting its name.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        fixes: &[],
        engines: &[],
        run: undefined_color,
    },
    Rule {
        code: "unused-color",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a color the document defines is never used",
        explanation: "\
The document names a color with `\\definecolor`, `\\providecolor` or
`\\colorlet` and never reads it back through `\\color`, `\\textcolor` or a
`\\colorlet` source.

Silent for a color a package defines: a library may define more colors
than any one document uses.  So is a name with `@`, which code built and no
document text wrote, and xcolor's `.` for the current color.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        fixes: &[],
        engines: &[],
        run: unused_color,
    },
    Rule {
        code: "conflicting-color-definition",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "the same color name is defined twice with different colors",
        explanation: "\
Two `\\definecolor`-family calls store different colors under the same
name. Only the last one is ever read back.  Silent where the first is a
package's: a default the document recolors on purpose.

Fix by giving the second color its own name, or by removing the earlier
definition.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        fixes: &[],
        engines: &[],
        run: conflicting_color_definition,
    },
    Rule {
        code: "duplicate-color-value",
        category: Category::Style,
        severity: Severity::Info,
        summary: "two color names store the exact same color",
        explanation: "\
Two `\\definecolor`-family calls, the later one the document's own, store
the same model and components under different names.  Not wrong, but one
name likely means to be the other.

Silent unless the run could read both colors back exactly, and for a
`\\colorlet` given exactly another color's name: an alias, such as a role
named after the color it takes.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        fixes: &[],
        engines: &[],
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
        fixes: &[],
        engines: &[Engine::PdfTeX, Engine::LuaTeX],
        run: unbalanced_color_stack,
    },
];

/// Whether color-definition findings apply here: a document defines
/// colors for its own prose, but so does a package meant to be loaded by
/// one. Only the profiles [`crate::linter::text`] also treats as code stay
/// silent.
fn definitions_matter(analysis: &Analysis) -> bool {
    !matches!(analysis.settings.profile, Some(crate::project::config::Profile::Literate))
}

/// A name no document defines itself: one with `@`, which code built, or xcolor's `.`, the
/// current color every `\color` sets.  No finding names it.
fn internal(name: &str) -> bool {
    name.contains('@') || name == "."
}

fn undefined_color(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    // xcolor looks a whole expression (`green!50!blue`) up as a name before it parses it: only a
    // lookup the run answered with an error at that call was a color that does not exist
    let raised: Vec<crate::engine::tex::Span> = (analysis.facts.occurrences.iter())
        .filter(|o| crate::linter::shared::is_error(analysis, o))
        .map(|o| o.span)
        .collect();
    let failed = |at: &crate::engine::tex::Span| raised.iter().any(|e| (e.file, e.line) == (at.file, at.line));
    for use_ in &analysis.facts.color_uses {
        // A `\ifx`/`\@ifundefinedcolor`-style test deliberately asks
        // whether the color exists. Only reading or aliasing it assumes
        // that it does.
        if use_.defined || matches!(use_.role, CsnameRole::Test) || !failed(&use_.span) {
            continue;
        }
        let severity = if use_.certain { Severity::Error } else { Severity::Warning };
        report.add_as(
            severity,
            use_.span,
            &use_.name,
            format!("color `{}` is used but never defined", use_.name),
            None,
        );
    }
}

fn unused_color(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    let used: crate::fixed::HashSet<&str> = analysis
        .facts
        .color_uses
        .iter()
        .filter(|u| matches!(u.role, CsnameRole::Use | CsnameRole::LetTo))
        .map(|u| u.name.as_str())
        .collect();
    for def in &analysis.facts.colors {
        if def.package.is_some() || internal(&def.shown) || used.contains(def.name.as_str()) {
            continue;
        }
        report.add_absent("used", def.span, &def.shown, format!("color `{}` is never used", def.shown), None);
    }
}

fn conflicting_color_definition(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    let mut by_name: HashMap<&str, Vec<&ColorDef>> = HashMap::default();
    for def in &analysis.facts.colors {
        by_name.entry(def.name.as_str()).or_default().push(def);
    }
    for defs in by_name.into_values() {
        for pair in defs.windows(2) {
            let [first, second] = pair else { continue };
            let (Some(a), Some(b)) = (&first.value, &second.value) else { continue };
            // A package's color is a default the document may recolor.
            if !first.certain || !second.certain || a == b || first.package.is_some() || internal(&second.shown) {
                continue;
            }
            let name = &second.shown;
            report.add(
                second.span,
                name,
                format!(
                    "color `{name}` is redefined here as {}, but was {} at {}",
                    b.hull.spell(),
                    a.hull.spell(),
                    report.place(first.span)
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
    // An alias names an existing color on purpose: a role (`comments`) given a color (`orange`).
    let exact: Vec<&ColorDef> =
        analysis.facts.colors.iter().filter(|d| d.certain && d.value.is_some() && !d.alias).collect();
    let mut seen: crate::fixed::HashSet<&str> = crate::fixed::HashSet::default();
    for (i, doc) in exact.iter().enumerate() {
        if doc.package.is_some() || internal(&doc.shown) || !seen.insert(doc.name.as_str()) {
            continue;
        }
        // The first color defined before it with its value, never a package's internal one.
        let Some(other) = exact[..i].iter().find(|other| {
            other.name != doc.name && other.value == doc.value && !(other.package.is_some() && internal(&other.name))
        }) else {
            continue;
        };
        report.add(
            doc.span,
            &doc.shown,
            format!(
                "color `{}` is exactly `{}` ({})",
                doc.shown,
                other.shown,
                doc.value.as_ref().map_or_else(String::new, |v| v.hull.spell())
            ),
            None,
        );
    }
}

fn unbalanced_color_stack(report: &mut Report) {
    let analysis = report.analysis();
    let mut open: HashMap<i64, Vec<crate::engine::tex::Span>> = HashMap::default();
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
