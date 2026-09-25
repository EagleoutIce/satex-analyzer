//! Numbers set next to their unit by hand, which siunitx does properly.

use std::collections::HashSet;

use crate::builtins::LoadKind;
use crate::facts::Quantity;
use crate::lint::fix::{Applicability, Fix};
use crate::lint::shared::document_start;
use crate::lint::{Category, Report, Rule, Severity};
use crate::machine::Analysis;
use crate::tex::FileId;

pub static RULES: &[Rule] = &[Rule {
    code: "hand-set-quantity",
    category: Category::Style,
    severity: Severity::Info,
    summary: "a number and its unit are set by hand where siunitx would set them",
    explanation: "\
`2ms` sets the number and the unit as one word, so the line may break between
them and the unit is italic in math mode.  `2\\,\\mathrm{ms}` puts the space in
by hand, which fixes neither the unit's own spacing nor the decimal marker, and
a `\\newcommand` that holds the unit only hides the same markup.  The SI
brochure (9th ed., § 5.4.3) asks for a non-breaking space between a value and
its unit, and for the unit in an upright font.  siunitx does all of it:
`\\qty{2}{\\milli\\second}` (`\\SI` before version 3).

The rule reads what the run typeset, not the source: a digit run followed by
letters, with no space token between them.  The unit has to be an SI symbol or
one of the units the SI accepts (brochure, tables 1-4 and § 4.1), the number
has to start a word, and the quantity has to be set by the document rather than
handed to a package's own command, so a document that already formats its units
with siunitx, units or physics is left alone.

Fix by loading `\\usepackage{siunitx}` and writing `\\qty{⟨number⟩}{⟨unit⟩}`.",
    run: hand_set_quantity,
}];

/// The SI prefixes (SI brochure, 9th ed., table 7), longest first so that
/// `da` is read before `d`.
const PREFIXES: &[(&str, &str)] = &[
    ("da", "\\deca"),
    ("q", "\\quecto"),
    ("r", "\\ronto"),
    ("y", "\\yocto"),
    ("z", "\\zepto"),
    ("a", "\\atto"),
    ("f", "\\femto"),
    ("p", "\\pico"),
    ("n", "\\nano"),
    ("u", "\\micro"),
    ("\u{b5}", "\\micro"),
    ("\u{3bc}", "\\micro"),
    ("m", "\\milli"),
    ("c", "\\centi"),
    ("d", "\\deci"),
    ("h", "\\hecto"),
    ("k", "\\kilo"),
    ("M", "\\mega"),
    ("G", "\\giga"),
    ("T", "\\tera"),
    ("P", "\\peta"),
    ("E", "\\exa"),
    ("Z", "\\zetta"),
    ("Y", "\\yotta"),
    ("R", "\\ronna"),
    ("Q", "\\quetta"),
];

/// The unit symbols of the SI and the ones it accepts beside them (SI
/// brochure, 9th ed., tables 1-4 and § 4.1), with siunitx's macro for each
/// and whether a prefix may stand in front of it.
const UNITS: &[(&str, &str, bool)] = &[
    ("m", "\\metre", true),
    ("g", "\\gram", true),
    ("s", "\\second", true),
    ("A", "\\ampere", true),
    ("K", "\\kelvin", true),
    ("mol", "\\mole", true),
    ("cd", "\\candela", true),
    ("Hz", "\\hertz", true),
    ("N", "\\newton", true),
    ("Pa", "\\pascal", true),
    ("J", "\\joule", true),
    ("W", "\\watt", true),
    ("C", "\\coulomb", true),
    ("V", "\\volt", true),
    ("F", "\\farad", true),
    ("S", "\\siemens", true),
    ("Wb", "\\weber", true),
    ("T", "\\tesla", true),
    ("H", "\\henry", true),
    ("lm", "\\lumen", true),
    ("lx", "\\lux", true),
    ("Bq", "\\becquerel", true),
    ("Gy", "\\gray", true),
    ("Sv", "\\sievert", true),
    ("kat", "\\katal", true),
    ("rad", "\\radian", true),
    ("sr", "\\steradian", true),
    ("min", "\\minute", false),
    ("h", "\\hour", false),
    ("d", "\\day", false),
    ("t", "\\tonne", true),
    ("L", "\\litre", true),
    ("l", "\\litre", true),
    ("eV", "\\electronvolt", true),
    ("Da", "\\dalton", true),
    ("Np", "\\neper", true),
    ("B", "\\bel", true),
    ("ha", "\\hectare", false),
    ("bar", "\\bar", true),
    ("\u{2126}", "\\ohm", true),
    ("\u{3a9}", "\\ohm", true),
    ("%", "\\percent", false),
    ("\u{b0}", "\\degree", false),
    ("\u{b0}C", "\\degreeCelsius", false),
];

/// siunitx's macros for a symbol, reading a prefix off it when it has one.
fn siunitx(unit: &str) -> Option<String> {
    if let Some((_, unit, _)) = UNITS.iter().find(|(symbol, ..)| *symbol == unit) {
        return Some((*unit).to_string());
    }
    PREFIXES.iter().find_map(|(prefix, macros)| {
        let rest = unit.strip_prefix(prefix)?;
        let (_, unit, _) = UNITS.iter().find(|(symbol, _, prefixable)| *symbol == rest && *prefixable)?;
        Some(format!("{macros}{unit}"))
    })
}

/// `1980s` and `100s` are a plural, not seconds.
fn plural(number: &str, unit: &str) -> bool {
    unit == "s" && number.len() >= 3 && number.chars().all(|c| c.is_ascii_digit())
}

fn spaced(q: &Quantity) -> bool {
    q.unit_span.file != q.number_end.file
        || q.unit_span.line != q.number_end.line
        || q.unit_span.col != q.number_end.col + 1
}

/// `\qty` is siunitx 3's name for what version 2 called `\SI`.
fn call(analysis: &Analysis, number: &str, unit: &str) -> String {
    let defined = |name: &str| analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym));
    let name = match !defined("qty") && defined("SI") {
        true => "SI",
        false => "qty",
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
    let loaded = analysis.facts.loads.iter().any(|load| load.kind.is_package() && load.name == "siunitx");
    let begin = document_start(analysis);
    let document = |file| crate::query::origin(analysis, file) == "document";
    let quantities: Vec<Quantity> = analysis
        .facts
        .quantities
        .iter()
        .filter(|q| !q.within.iter().any(|file| packages.contains(file)))
        .filter(|q| document(q.span.file) && document(q.unit_span.file))
        .filter(|q| !plural(&q.number, &q.unit))
        .cloned()
        .collect();
    // Only the first finding brings the package in; the rest would overlap it.
    let mut load = !loaded;
    for q in quantities {
        let Some(unit) = siunitx(&q.unit) else { continue };
        let call = call(analysis, &q.number, &unit);
        let via = q.via.map(|sym| report.cs(sym).trim_start_matches('\\').to_string());
        let mut edits = report.edits().quantity(q.span, &q.number, &q.unit, via.as_deref(), &call);
        if let (Some(edits), Some(begin), true) = (edits.as_mut(), begin, load) {
            edits.extend(report.edits().insert_before(begin, "\\usepackage{siunitx}").unwrap_or_default());
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
