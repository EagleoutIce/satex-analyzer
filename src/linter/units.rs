//! Numbers set next to their unit by hand, which siunitx does properly.

use crate::fixed::HashSet;

use crate::analysis::facts::Quantity;
use crate::analysis::machine::Analysis;
use crate::engine::builtins::LoadKind;
use crate::engine::tex::FileId;
use crate::linter::fix::{Applicability, Fix};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};

pub static RULES: &[Rule] = &[Rule {
    code: "hand-set-quantity",
    category: Category::Style,
    severity: Severity::Info,
    summary: "a number and its unit are set by hand where siunitx would set them",
    explanation: "\
`2ms` sets the number and the unit as one word, so the line may break
between them and the unit is italic in math.  The
[SI brochure](https://www.bipm.org/en/publications/si-brochure) (9th ed.,
§ 5.4.3) asks for a non-breaking space and an upright unit, and
[siunitx](https://ctan.org/pkg/siunitx) does both.

The rule reads what the run typeset: a digit run followed by letters with no
space token between them, where the unit is an SI symbol and the document
set it itself.

Fix by loading `\\usepackage{siunitx}` and writing `\\qty{⟨number⟩}{⟨unit⟩}`.",
    references: &[Ref { source: Source::SiBrochure, at: "5.4.3" }, Ref { source: Source::Siunitx, at: "\\qty" }],
    fixes: &[Applicability::Unsafe],
    engines: &[],
    run: hand_set_quantity,
}];

/// siunitx's macros for a symbol, reading a prefix off it when it has one,
/// the longest prefix first so that `da` is read before `d`.
fn siunitx(units: &crate::project::config::Units, unit: &str) -> Option<String> {
    if let Some(unit) = units.symbols.get(unit) {
        return Some(unit.clone());
    }
    let mut prefixes: Vec<_> = units.prefixes.iter().collect();
    prefixes.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
    prefixes.into_iter().find_map(|(prefix, macros)| {
        let rest = unit.strip_prefix(prefix.as_str())?;
        let unit = units.symbols.get(rest).filter(|_| !units.unprefixed.iter().any(|u| u == rest))?;
        Some(format!("{macros}{unit}"))
    })
}

/// `1980s` and `100s` are a plural, not seconds.
fn plural(units: &crate::project::config::Units, number: &str, unit: &str) -> bool {
    unit == "s" && number.len() >= units.plural && number.chars().all(|c| c.is_ascii_digit())
}

fn spaced(q: &Quantity) -> bool {
    q.unit_span.file != q.number_end.file
        || q.unit_span.line != q.number_end.line
        || q.unit_span.col != q.number_end.col + 1
}

/// `units.quantity`, or `units.legacy` where only that is defined, as
/// siunitx 2 called `\qty` `\SI`.
fn call(analysis: &Analysis, number: &str, unit: &str) -> String {
    let units = &analysis.settings.conventions.units;
    let defined = |name: &str| analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym));
    let name = match !defined(&units.quantity) && defined(&units.legacy) {
        true => &units.legacy,
        false => &units.quantity,
    };
    format!("\\{name}{{{number}}}{{{unit}}}")
}

/// The files a `\usepackage` or `\documentclass` read.
fn package_files(analysis: &Analysis) -> HashSet<FileId> {
    analysis
        .facts
        .loads
        .iter()
        .filter(|load| matches!(load.kind, LoadKind::Package | LoadKind::Class))
        .filter_map(|load| load.file)
        .collect()
}

fn hand_set_quantity(report: &mut Report) {
    let analysis = report.analysis;
    let packages = package_files(analysis);
    let loaded = crate::linter::shared::loaded(analysis, "siunitx");
    let view = report.text();
    let document = |file| view.document(file);
    let quantities: Vec<Quantity> = analysis
        .facts
        .quantities
        .iter()
        // Set on every run (a path that lost track of a key may typeset a
        // length no real run does), in the document's own text, not a count.
        .filter(|q| {
            q.certain
                && !view.code_at(q.span)
                && document(q.span.file)
                && document(q.unit_span.file)
                && !q.within.iter().any(|file| packages.contains(file))
                && !plural(&analysis.settings.conventions.units, &q.number, &q.unit)
        })
        .cloned()
        .collect();
    // Only the first finding brings the package in.  The rest would overlap it.
    let mut load = !loaded;
    for q in quantities {
        let Some(unit) = siunitx(&analysis.settings.conventions.units, &q.unit) else { continue };
        let call = call(analysis, &q.number, &unit);
        let via = q.via.map(|sym| report.cs(sym).trim_start_matches('\\').to_string());
        let mut edits = report.edits().quantity(&q, via.as_deref(), &call);
        if let (Some(edits), true) = (edits.as_mut(), load) {
            edits.extend(report.edits().ensure_package("siunitx").unwrap_or_default());
            load = false;
        }
        let set = match (q.unit.chars().all(char::is_alphabetic), spaced(&q)) {
            (false, _) => "is set by hand",
            (true, true) => "spaces its unit by hand",
            (true, false) => "sets its unit as part of the same word",
        };
        report.add_fix(
            q.span,
            &q.unit,
            format!("{}{} {set}, which siunitx does properly", q.number, q.unit),
            Fix::new(format!("write {call}"), Applicability::Unsafe, edits),
        );
    }
}
