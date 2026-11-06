//! Lengths: a dimension written in the source (`2.5cm`, `-3pt`) or a length
//! register (`\textwidth`). Hover shows it in every configured unit and in
//! scaled points; each value is a link that writes the literal in that unit,
//! marked exact when TeX reads it back as the same scaled points; the view is a converter.

use super::{Cx, TypePlugin, range};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::engine::tex::{Meaning, RegKind};
use crate::engine::value::{Num, UNIT, UNITS, parse_dimen};
use crate::project::config::TypePlugins;
use lsp_types::{CompletionItem, CompletionItemKind};
use serde_json::{Value as Json, json};

pub(super) struct Units;

/// A dimension written in the source: where it stands and what TeX makes of it.
struct Literal {
    line: u32,
    /// Columns (from 1) of its first character (the sign's) and past its last.
    from: u32,
    to: u32,
    written: String,
    unit: String,
    sp: i64,
}

impl TypePlugin for Units {
    fn name(&self) -> &'static str {
        "units"
    }
    fn category(&self) -> &'static str {
        "hover"
    }
    fn about(&self) -> &'static str {
        "a length in every unit, each value a click that writes it, and a converter view"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Dimen, Ty::Glue, Ty::Reg(RegKind::Dimen), Ty::Reg(RegKind::Skip)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["hover", "complete", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.units.enabled
    }

    fn hover(&self, cx: &Cx, hit: &super::Hit) -> Option<String> {
        if let Some(lit) = literal(cx) {
            return Some(card(cx, &format!("`{}`", lit.written), Num::exact(lit.sp), Some(&lit)));
        }
        let (name, num) = register(cx.analysis, hit)?;
        Some(card(cx, &format!("`\\{name}` at the end of the run"), num, None))
    }

    fn complete(&self, cx: &Cx, hit: &super::Hit) -> Option<Vec<CompletionItem>> {
        let typed = hit.typed.as_deref().unwrap_or(&cx.at.item.typed);
        let letters = typed.chars().rev().take_while(char::is_ascii_alphabetic).count();
        let (number, unit) = typed.split_at(typed.len() - letters);
        if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Some(Vec::new());
        }
        let items = UNITS.iter().filter(|(u, ..)| u.starts_with(unit) && *u != "mu").map(|(u, ..)| {
            let sp = parse_dimen(&format!("{number}{u}")).unwrap_or_default();
            let pt = sp as f64 / UNIT as f64;
            CompletionItem {
                label: u.to_string(),
                kind: Some(CompletionItemKind::UNIT),
                detail: Some(format!("{number}{u} = {}pt", trimmed(pt, 5))),
                filter_text: Some(format!("{number}{u}")),
                insert_text: Some(format!("{number}{u}")),
                ..Default::default()
            }
        });
        Some(items.collect())
    }

    fn view(&self, cx: &Cx, hit: &super::Hit) -> Option<Json> {
        let lit = literal(cx);
        let (title, num) = match &lit {
            Some(l) => (format!("Length {}", l.written), Num::exact(l.sp)),
            None => {
                let (name, num) = register(cx.analysis, hit)?;
                (format!("\\{name} at the end of the run"), num)
            }
        };
        let units: Vec<Json> = UNITS
            .iter()
            .filter(|(u, ..)| *u != "mu")
            .map(|(u, n, d)| json!({ "name": u, "sp": UNIT as f64 * *n as f64 / *d as f64 }))
            .chain(
                fonts_at(cx, lit.as_ref().map(|l| l.from))
                    .first()
                    .into_iter()
                    .flat_map(|f| [("em", f.1[0]), ("ex", f.1[1])])
                    .filter_map(|(u, sp)| Some(json!({ "name": u, "sp": sp? as f64 }))),
            )
            .collect();
        let sections = vec![
            json!({
                "kind": "converter",
                "lo": num.lo, "hi": num.hi,
                "units": units,
                "decimals": cx.cfg().units.decimals,
                "unit": lit.as_ref().map_or("pt", |l| l.unit.as_str()),
                // the client writes `<number><unit>` here, when the length is a literal
                "range": lit.as_ref().map(|l| range(cx.doc, l.line, l.from, l.to)),
            }),
            json!({ "kind": "text", "markdown": NOTE }),
        ];
        Some(json!({ "title": title, "sections": sections }))
    }
}

const NOTE: &str = "TeX stores a length in scaled points (sp), the 65536th part of a point, and rounds what it reads to them (tex.web § 458). A converted length marked *exact* reads back as the same sp.";

/// The literal under the cursor, in the source text.
fn literal(cx: &Cx) -> Option<Literal> {
    let line = cx.doc.line(cx.pos.line);
    let chars: Vec<char> = line.chars().collect();
    let at = (cx.pos.col as usize).checked_sub(1)?.min(chars.len().saturating_sub(1));
    let number = |c: char| c.is_ascii_digit() || c == '.' || c == ',';
    let letter = |c: char| c.is_ascii_alphabetic();
    let mut start = at;
    while start > 0
        && (letter(chars[start]) || number(chars[start]))
        && (letter(chars[start - 1]) || number(chars[start - 1]))
    {
        start -= 1;
    }
    while start < chars.len() && letter(chars[start]) {
        start += 1;
    }
    let digits = chars[start..].iter().take_while(|c| number(**c)).count();
    let units = chars[start + digits..].iter().take_while(|c| letter(**c)).count();
    if digits == 0 || units == 0 || start + digits + units < at || (start > 0 && chars[start - 1] == '\\') {
        return None;
    }
    let negative = start > 0 && chars[start - 1] == '-';
    let written: String = chars[start..start + digits + units].iter().collect();
    let unit = written[digits..].to_ascii_lowercase();
    // `2truecm` is `2cm` before `\mag` (§ 457)
    let unit = unit.strip_prefix("true").unwrap_or(&unit).to_string();
    if self::unit(&unit).is_none() || unit == "mu" {
        return None;
    }
    let sp = parse_dimen(&format!("{}{unit}", &written[..digits]))? * if negative { -1 } else { 1 };
    let from = start - usize::from(negative);
    Some(Literal {
        line: cx.pos.line,
        from: from as u32 + 1,
        to: (start + digits + units) as u32 + 1,
        written: if negative { format!("-{written}") } else { written },
        unit,
        sp,
    })
}

/// A length register's name and its value when the run ended.
fn register(analysis: &Analysis, hit: &super::Hit) -> Option<(String, Num)> {
    let name = hit.typed.as_deref()?.trim_start_matches('\\');
    Some((name.to_string(), register_value(analysis, name)?))
}

/// The value of the length register `\name` when the run ended.
pub(in crate::lsp) fn register_value(analysis: &Analysis, name: &str) -> Option<Num> {
    let sym = analysis.interner.lookup(name)?;
    let Meaning::Register(kind, n) = analysis.env.meaning(sym) else { return None };
    let reg = analysis.interner.lookup(&format!("\u{2}{}{n}", kind.as_str()))?;
    Some(analysis.env.num(reg)?.1)
}

/// `(n, d)` of a unit: `n/d` points.
fn unit(name: &str) -> Option<(i64, i64)> {
    UNITS.iter().find(|(u, ..)| *u == name).map(|&(_, n, d)| (n, d))
}

/// The value of `sp` in `name`, with the fewest decimals up to `decimals` that TeX reads back as
/// `sp`, else rounded to `decimals`; and how many sp the text read back is off.
fn converted(sp: i64, name: &str, decimals: u8) -> (String, i64) {
    let (n, d) = unit(name).unwrap_or((1, 1));
    let value = sp as f64 / UNIT as f64 * d as f64 / n as f64;
    let read = |s: &str| parse_dimen(&format!("{s}{name}"));
    for places in 0..=decimals {
        let s = trimmed(value, places);
        if read(&s) == Some(sp) {
            return (format!("{s}{name}"), 0);
        }
    }
    let s = trimmed(value, decimals);
    let off = read(&s).map_or(0, |v| v - sp);
    (format!("{s}{name}"), off)
}

/// Units most lengths are read in; the rest are TeX's own.
const EVERYDAY: [&str; 4] = ["pt", "mm", "cm", "in"];
/// Cells of the to-scale bar.
const BAR: usize = 20;

/// The hover: a headline in the everyday units, a to-scale bar, `em` and `ex` of the font in
/// force, and TeX's own units in a grid. With a literal, each value is a link that writes it
/// in place of the literal.
fn card(cx: &Cx, head: &str, num: Num, lit: Option<&Literal>) -> String {
    let cfg = &cx.cfg().units;
    let exact = num.lo == num.hi;
    let own = lit.map(|l| l.unit.as_str());
    // `u`'s value as (text, reads back as the same sp); an interval as a range
    let value = |u: &str, places: u8| -> Option<(String, bool)> {
        let (n, d) = unit(u)?;
        if exact {
            let (t, off) = converted(num.lo, u, places);
            return Some((t[..t.len() - u.len()].to_string(), off == 0));
        }
        let per = UNIT as f64 * n as f64 / d as f64;
        let v = |sp: i64| trimmed(sp as f64 / per, places);
        Some(if exact { (v(num.lo), true) } else { (format!("{} … {}", v(num.lo), v(num.hi)), false) })
    };
    let link = |u: &str, shown: &str, text: &str, same: bool| match lit {
        Some(l) if exact => apply_link(cx, range(cx.doc, l.line, l.from, l.to), shown, &format!("{text}{u}"), same),
        _ => shown.to_string(),
    };
    let mut md = format!("### {head}");
    for u in EVERYDAY.iter().filter(|u| Some(**u) != own && cfg.units.iter().any(|c| c == *u)) {
        if let Some((t, ok)) = value(u, cfg.decimals) {
            md.push_str(&format!(" {} {}", if ok { "=" } else { "≈" }, link(u, &format!("{t} {u}"), &t, ok)));
        }
    }
    md.push_str("\n\n");
    if let Some(bar) = bar(cx, num) {
        md.push_str(&format!("{bar}\n\n"));
    }
    if let Some(line) = font_line(cx, num, lit) {
        md.push_str(&format!("{line}\n\n"));
    }
    let mut cells = Vec::new();
    // every unit but the literal's own, the headline's included
    for (u, ..) in UNITS.iter().filter(|(u, ..)| *u != "mu" && Some(*u) != own) {
        if let Some((t, ok)) = value(u, 3) {
            cells.push((*u, link(u, &t, &t, ok)));
        }
    }
    // rows of a few units each: a card never scrolls sideways
    for chunk in cells.chunks(GRID) {
        let row = |f: &dyn Fn(&(&str, String)) -> String| chunk.iter().map(f).collect::<Vec<_>>().join(" | ");
        md.push_str(&format!(
            "| {} |\n|{}\n| {} |\n\n",
            row(&|c| c.0.to_string()),
            "---:|".repeat(chunk.len()),
            row(&|c| c.1.clone())
        ));
    }
    if let Some(line) = relative(cx, num, lit) {
        md.push_str(&format!("{line}\n"));
    }
    md
}

/// Units per row of the grid.
const GRID: usize = 5;

/// The length as a share of the widths a document sizes by (`0.4\linewidth`), each a link
/// that writes it: those the run knows one value of, at most as wide as the length.
fn relative(cx: &Cx, num: Num, lit: Option<&Literal>) -> Option<String> {
    if num.lo != num.hi || num.lo <= 0 {
        return None;
    }
    let mut parts = Vec::new();
    for name in ["linewidth", "textwidth", "columnwidth"] {
        let Some(w) = register_value(cx.analysis, name).filter(|w| w.lo == w.hi && w.lo >= num.lo) else { continue };
        let share = trimmed(num.lo as f64 / w.lo as f64, 3);
        let text = format!("{share}\\{name}");
        let shown = format!("{share}`\\{name}`");
        parts.push(match lit {
            Some(l) => apply_link(cx, range(cx.doc, l.line, l.from, l.to), &shown, &text, false),
            None => shown,
        });
    }
    (!parts.is_empty()).then(|| format!("≈ {} (widths as the run ended)", parts.join(" · ")))
}

/// `= 1.2 em ≈ 2.79 ex in cmr10 at 10pt`: the length in the `em` and `ex` of the font in force
/// where it was read; with several possible fonts, a range and each font.
fn font_line(cx: &Cx, num: Num, lit: Option<&Literal>) -> Option<String> {
    let fonts = fonts_at(cx, lit.map(|l| l.from));
    if fonts.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for (unit, k) in [("em", 0), ("ex", 1)] {
        let per: Vec<i64> = fonts.iter().filter_map(|f| f.1[k]).collect();
        let (Some(&small), Some(&big)) = (per.iter().min(), per.iter().max()) else { continue };
        let at = |sp: i64, per: i64| sp as f64 / per as f64;
        let (lo, hi) =
            (at(num.lo, if num.lo < 0 { small } else { big }), at(num.hi, if num.hi < 0 { big } else { small }));
        if num.lo == num.hi && small == big {
            let t = trimmed(lo, 2);
            let same = t.parse::<f64>().is_ok_and(|v| (v * small as f64).round() as i64 == num.lo);
            parts.push(format!("{} {t} {unit}", if same { "=" } else { "≈" }));
        } else {
            parts.push(format!("≈ {} … {} {unit}", trimmed(lo.min(hi), 2), trimmed(lo.max(hi), 2)));
        }
    }
    let names: Vec<&str> = fonts.iter().map(|f| f.0.as_str()).collect();
    Some(format!("{} in {}", parts.join(" "), names.join(", ")))
}

/// The length against `\textwidth` when known and not smaller, else against 1 cm, 10 cm or 1 m.
fn bar(cx: &Cx, num: Num) -> Option<String> {
    if num.lo != num.hi || num.lo <= 0 {
        return None;
    }
    let width = register_value(cx.analysis, "textwidth")
        .filter(|w| w.lo == w.hi && w.lo >= num.lo)
        .map(|w| (w.lo, "`\\textwidth`".to_string()));
    let (whole, name) = width.or_else(|| {
        [1, 10, 100].iter().find_map(|cm| {
            let sp = (UNIT as f64 * 72.27 / 2.54 * f64::from(*cm)) as i64;
            (num.lo <= sp).then(|| (sp, format!("{cm} cm")))
        })
    })?;
    let eighths = ((num.lo as f64 / whole as f64) * (BAR * 8) as f64).round().max(1.0) as usize;
    let mut bar = "█".repeat(eighths / 8);
    if !eighths.is_multiple_of(8) {
        bar.push(['▏', '▎', '▍', '▌', '▋', '▊', '▉'][eighths % 8 - 1]);
    }
    let pad = BAR.saturating_sub(bar.chars().count());
    Some(format!("`{bar}{}` {}% of {name}", "░".repeat(pad), (num.lo as f64 * 100.0 / whole as f64).round()))
}

/// A hover link that writes `text` over `range` (`satex.applyEdit`); `exact` tells the
/// client the new text reads back as the same value.
pub(super) fn apply_link(cx: &Cx, range: lsp_types::Range, shown: &str, text: &str, exact: bool) -> String {
    let args = json!([cx.uri(), range, text, exact]);
    format!("[{shown}](command:satex.applyEdit?{})", encode(&args.to_string()))
}

/// Percent-encoding of a command link's argument.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The fonts a length at `col` of the cursor's line was read in, each as its name and size and
/// its `em` and `ex` (`\fontdimen6`, `5`) in sp: the run's record of that length, else the font of
/// the text set last before the cursor in this file.
fn fonts_at(cx: &Cx, col: Option<u32>) -> Vec<(String, [Option<i64>; 2])> {
    let analysis = cx.analysis;
    let Some(file) = super::super::type_at::file_of(analysis, cx.path) else { return Vec::new() };
    let here = (cx.pos.line, cx.pos.col);
    let recorded = col
        .and_then(|c| analysis.facts.length_fonts.get(&crate::engine::tex::Span { file, line: cx.pos.line, col: c }));
    let states: Vec<u16> = match recorded {
        Some(r) => r.clone(),
        None => analysis
            .facts
            .text
            .at
            .iter()
            .rev()
            .find(|s| s.span.file == file && (s.span.line, s.span.col) <= here && s.font_state != 0)
            .map(|s| vec![s.font_state])
            .unwrap_or_default(),
    };
    let mut fonts = crate::linter::italics::Fonts::of(analysis);
    let mut out: Vec<(String, [Option<i64>; 2])> = Vec::new();
    for state in states.iter().filter_map(|s| analysis.font_states.get(usize::from(*s))) {
        let Some((metrics, size)) = fonts.metrics(state.font) else { continue };
        let Some(used) = analysis.fonts.get(usize::from(state.font)).and_then(Option::as_ref) else { continue };
        let entry = (
            format!("{} at {}pt", used.name, trimmed(size as f64 / UNIT as f64, 2)),
            [6, 5].map(|k| metrics.param(k, size).filter(|v| *v > 0)),
        );
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
    out
}

/// `v` with at most `places` decimals and no trailing zeros.
fn trimmed(v: f64, places: u8) -> String {
    let s = format!("{v:.*}", usize::from(places));
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
    if s == "-0" { "0".into() } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::converted;

    #[test]
    fn conversions_say_whether_they_are_exact() {
        // 24pt is exactly two picas; 72.27pt is an inch, but `1in` reads as one sp less
        let sp = crate::engine::value::parse_dimen("24pt").unwrap();
        assert_eq!(converted(sp, "pc", 2), ("2pc".into(), 0));
        let sp = crate::engine::value::parse_dimen("72.27pt").unwrap();
        assert_eq!(converted(sp, "in", 2), ("1in".into(), -1));
        // 12pt is 0.42175…cm: two decimals are off
        let sp = crate::engine::value::parse_dimen("12pt").unwrap();
        let (text, off) = converted(sp, "cm", 2);
        assert_eq!(text, "0.42cm");
        assert_ne!(off, 0);
        assert_eq!(converted(-sp, "pt", 2), ("-12pt".into(), 0));
    }
}
