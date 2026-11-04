//! Lengths: a dimension written in the source (`2.5cm`, `-3pt`) or a length
//! register (`\textwidth`). Hover shows it in every configured unit and in
//! scaled points; actions rewrite a literal in another unit, saying whether
//! TeX reads the new text as the same scaled points; the view is a converter.

use super::{Cx, TypePlugin, button, range, rewrite};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::engine::tex::{Meaning, RegKind};
use crate::engine::value::{Num, UNIT, UNITS, parse_dimen};
use crate::project::config::TypePlugins;
use lsp_types::{CodeAction, CompletionItem, CompletionItemKind, TextEdit};
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
    fn about(&self) -> &'static str {
        "a length in every unit, convert actions and a converter view"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Dimen, Ty::Glue, Ty::Reg(RegKind::Dimen), Ty::Reg(RegKind::Skip)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["hover", "complete", "actions", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.units.enabled
    }

    fn hover(&self, cx: &Cx, hit: &super::Hit) -> Option<String> {
        if let Some(lit) = literal(cx) {
            let mut md = crate::lsp::handlers::heading("length", &lit.written);
            md.push_str(&table(cx, Num::exact(lit.sp)));
            return Some(md);
        }
        let (name, num) = register(cx.analysis, hit)?;
        let mut md = crate::lsp::handlers::heading("length", &format!("\\{name}"));
        md.push_str(&format!("{} at the end of the run\n\n", shown(num)));
        md.push_str(&table(cx, num));
        Some(md)
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

    fn actions(&self, cx: &Cx, _hit: &super::Hit) -> Vec<CodeAction> {
        let Some(lit) = literal(cx) else { return Vec::new() };
        let cfg = &cx.cfg().units;
        let edit = |text: String| TextEdit { range: range(cx.doc, lit.line, lit.from, lit.to), new_text: text };
        cfg.units
            .iter()
            .filter(|u| **u != lit.unit && unit(u).is_some())
            .map(|u| {
                let (text, off) = converted(lit.sp, u, cfg.decimals);
                let note = match off {
                    0 => "exact".to_string(),
                    d => format!("≈, {d:+} sp"),
                };
                rewrite(cx, format!("Convert {} to {text} ({note})", lit.written), vec![edit(text.clone())])
            })
            .collect()
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
            .chain(font_units(cx).into_iter().map(|(u, sp)| json!({ "name": u, "sp": sp as f64 })))
            .collect();
        let mut sections = vec![json!({
            "kind": "converter",
            "lo": num.lo, "hi": num.hi,
            "units": units,
            "decimals": cx.cfg().units.decimals,
            "unit": lit.as_ref().map_or("pt", |l| l.unit.as_str()),
            // the client writes `<number><unit>` here, when the length is a literal
            "range": lit.as_ref().map(|l| range(cx.doc, l.line, l.from, l.to)),
        })];
        let actions: Vec<Json> = self.actions(cx, hit).iter().map(button).collect();
        if !actions.is_empty() {
            sections.push(json!({ "kind": "actions", "items": actions }));
        }
        sections.push(json!({ "kind": "text", "markdown": NOTE }));
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
    let sym = analysis.interner.lookup(name)?;
    let Meaning::Register(kind, n) = analysis.env.meaning(sym) else { return None };
    let reg = analysis.interner.lookup(&format!("\u{2}{}{n}", kind.as_str()))?;
    let (_, num) = analysis.env.num(reg)?;
    Some((name.to_string(), num))
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

/// The rows: each configured unit, then em and ex where the font is known, then sp.
fn table(cx: &Cx, num: Num) -> String {
    let cfg = &cx.cfg().units;
    let mut out = String::from("| unit | value |\n|---|---|\n");
    let row = |per: f64| {
        let v = |sp: i64| trimmed(sp as f64 / per, 5);
        if num.lo == num.hi { v(num.lo) } else { format!("{} … {}", v(num.lo), v(num.hi)) }
    };
    for u in &cfg.units {
        if let Some((n, d)) = unit(u) {
            out.push_str(&format!("| {u} | {} |\n", row(UNIT as f64 * n as f64 / d as f64)));
        }
    }
    let fonts = font_units(cx);
    for (u, sp) in &fonts {
        out.push_str(&format!("| {u} | {} |\n", row(*sp as f64)));
    }
    out.push_str(&format!("| sp | {} |\n\n", shown_sp(num)));
    if fonts.is_empty() {
        out.push_str("`em` and `ex` depend on the font where the length is read: none was set before it.\n");
    } else {
        out.push_str("`em` and `ex` of the font of the text set just before.\n");
    }
    out
}

/// `em` and `ex` in sp: the quad and x-height (`\fontdimen6`, `5`) of the font of the text set
/// last before the cursor in this file, when the run loaded its metrics.
fn font_units(cx: &Cx) -> Vec<(&'static str, i64)> {
    let Some(file) = super::super::type_at::file_of(cx.analysis, cx.path) else { return Vec::new() };
    let spots = &cx.analysis.facts.text.at;
    let before = spots
        .iter()
        .rev()
        .find(|s| s.span.file == file && (s.span.line, s.span.col) <= (cx.pos.line, cx.pos.col) && s.font_state != 0);
    let Some(state) = before.and_then(|s| cx.analysis.font_states.get(usize::from(s.font_state))) else {
        return Vec::new();
    };
    let mut fonts = crate::linter::italics::Fonts::of(cx.analysis);
    let Some((metrics, size)) = fonts.metrics(state.font) else { return Vec::new() };
    [("em", 6), ("ex", 5)]
        .into_iter()
        .filter_map(|(u, k)| Some((u, metrics.param(k, size).filter(|v| *v > 0)?)))
        .collect()
}

fn shown(num: Num) -> String {
    let pt = |sp: i64| format!("{}pt", trimmed(sp as f64 / UNIT as f64, 5));
    if num.lo == num.hi { pt(num.lo) } else { format!("between {} and {}", pt(num.lo), pt(num.hi)) }
}

fn shown_sp(num: Num) -> String {
    if num.lo == num.hi { num.lo.to_string() } else { format!("{} … {}", num.lo, num.hi) }
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
