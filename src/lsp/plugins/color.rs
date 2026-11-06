//! Colors: a color's name or an xcolor expression (`red!40!blue`) where a
//! color command reads one. Swatches (`textDocument/documentColor`), the
//! colors defined at the position as completion items, a hover with the color
//! in every model, actions that convert a `\definecolor`'s model, and a picker
//! view that writes what works at the position: `\color[HTML]{…}` where an
//! xcolor command takes a model, else a `\definecolor` it adds.

use super::{Cx, Hit, TypePlugin};
use crate::analysis::facts::ColorDef;
use crate::analysis::machine::Analysis;
use crate::analysis::roles::{KeyKind, Ty};
use crate::project::config::TypePlugins;
use crate::text::{Pos, Text};
use lsp_types::{ColorInformation, CompletionItem, CompletionItemKind, Position, Range, TextEdit};
use serde_json::{Value as Json, json};

pub(super) struct Color;

/// Red, green and blue from 0 to 1.
type Rgb = [f64; 3];

impl TypePlugin for Color {
    fn name(&self) -> &'static str {
        "color"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "swatches, colors defined at the position, conversions between models and a picker"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Color)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["hover", "complete", "view", "colors"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.color.enabled
    }

    fn hover(&self, cx: &Cx, hit: &Hit) -> Option<String> {
        let name = name(cx, hit);
        let rgb = value_at(cx.analysis, cx.doc, cx.path, cx.pos, name);
        let def = defined_at(cx.analysis, cx.path, cx.pos).into_iter().find(|c| c.name == name);
        let mut md = crate::lsp::handlers::heading("color", name);
        match rgb {
            Some(rgb) => {
                md.push_str(&format!("![{}]({})\n\n", hex(rgb), swatch_uri(rgb)));
                // on a definition, each spec is a link that rewrites it in that model
                let slot = defined_here(cx, name).then(|| model_spec(cx, hit)).flatten();
                md.push_str("| model | spec |\n|---|---|\n");
                for (model, spec) in specs(rgb, xcolor(cx.analysis)) {
                    let shown = format!("`{spec}`");
                    let cell = match &slot {
                        Some((range, m)) if *m != model => {
                            super::units::apply_link(cx, *range, &shown, &format!("{{{model}}}{{{spec}}}"), true)
                        }
                        _ => shown,
                    };
                    md.push_str(&format!("| {model} | {cell} |\n"));
                }
            }
            None if name.contains('!') => md.push_str("An xcolor expression of colors the run did not fix.\n"),
            None => md.push_str("A value the run did not fix.\n"),
        }
        if let Some((model, spec)) = def.and_then(|d| d.declared.as_ref()) {
            md.push_str(&format!("\nDefined as `{{{}}}{{{spec}}}`\n", model.as_str()));
        }
        Some(md)
    }

    fn complete(&self, cx: &Cx, _hit: &Hit) -> Option<Vec<CompletionItem>> {
        Some(items(cx.analysis, cx.path, cx.pos))
    }

    fn view(&self, cx: &Cx, hit: &Hit) -> Option<Json> {
        let name = name(cx, hit).to_string();
        let rgb = value_at(cx.analysis, cx.doc, cx.path, cx.pos, &name);
        let slot = slot(cx, hit);
        let x = xcolor(cx.analysis);
        let defined: Vec<Json> = defined_at(cx.analysis, cx.path, cx.pos)
            .into_iter()
            .filter_map(|c| {
                Some(json!({ "name": c.name, "rgb": value(c)?, "css": hex(value(c)?), "group": group(cx.analysis, c) }))
            })
            .collect();
        let models: Vec<&str> =
            if x { vec!["HTML", "rgb", "RGB", "cmyk", "gray", "hsb"] } else { vec!["rgb", "cmyk", "gray"] };
        let writes = slot.map(|r| writes(cx, r));
        let sections = vec![json!({
            "kind": "color",
            "name": name,
            "rgb": rgb,
            "models": models,
            // xcolor reads `a!40!b` in a name's place
            "expressions": x,
            "defined": defined,
            "pick": slot,
            "apply": writes,
        })];
        Some(json!({ "title": format!("Edit colour {name}"), "sections": sections }))
    }

    fn colors(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<ColorInformation> {
        if !analysis.settings.type_plugins.color.swatches {
            return Vec::new();
        }
        document_colors(analysis, doc, path)
    }
}

/// Whether `name` is defined on the cursor's line.
/// Who defined the color: the document, else the file that did (`xcolor`, `dvipsnam`, `svgnam`).
fn group(analysis: &Analysis, c: &ColorDef) -> String {
    if analysis.project_file(c.span.file) {
        return "document".into();
    }
    let short = analysis.files.get(usize::from(c.span.file)).map_or("", |f| f.short.as_str());
    let base = short.rsplit('/').next().unwrap_or(short);
    match base.rsplit_once('.') {
        // a format file (latex.ltx) is the kernel
        Some((_, "ltx" | "fmt")) => "kernel".into(),
        Some((stem, _)) => stem.into(),
        None => base.into(),
    }
}

fn defined_here(cx: &Cx, name: &str) -> bool {
    let file = super::super::type_at::file_of(cx.analysis, cx.path);
    let here = |s: crate::engine::tex::Span| s.line == cx.pos.line && Some(s.file) == file;
    cx.analysis.facts.colors.iter().any(|c| c.name == name && (here(c.span) || c.spec_span.is_some_and(here)))
}

/// The color's name or expression at the cursor.
fn name<'a>(cx: &'a Cx, hit: &'a Hit) -> &'a str {
    hit.typed.as_deref().unwrap_or(&cx.at.item.typed).trim()
}

/// The range of the name at the cursor.
fn slot(cx: &Cx, hit: &Hit) -> Option<Range> {
    let typed = hit.typed.as_deref().unwrap_or(&cx.at.item.typed);
    let from = cx.at.item.col + (cx.at.item.typed.chars().count() - typed.chars().count()) as u32;
    let len = typed.chars().count() as u32;
    let line = cx.doc.line(cx.pos.line);
    // the item ends at the cursor: the hover position is the word's end
    let to = from + len;
    (from >= 1 && (to as usize) <= line.chars().count() + 1).then(|| super::range(cx.doc, cx.pos.line, from, to))
}

/// What writing a color into `slot` takes, as edits whose text holds `${model}`, `${spec}` and
/// `${name}` for the client to fill: with xcolor, the optional model argument of an xcolor command
/// whose first argument the slot is; else a `\definecolor` before `\begin{document}` and its name.
fn writes(cx: &Cx, slot: Range) -> Json {
    let x = xcolor(cx.analysis);
    let place = cx.at.place.as_ref();
    let own = place.filter(|p| p.open == '{' && p.nth() == 0 && x && by_color_package(cx.analysis, &p.cmd));
    if let Some(p) = own {
        // from the end of the command's name over `[…]{name}`
        let start =
            super::super::handlers::to_lsp(cx.doc, Pos::new(p.at.line, p.at.col + 1 + p.cmd.chars().count() as u32));
        let end = Position { character: slot.end.character + 1, ..slot.end };
        let closes = cx.doc.line(cx.pos.line).chars().nth(slot.end.character as usize) == Some('}');
        if closes {
            return json!({ "title": "Write here", "edits": [{ "range": Range { start, end }, "newText": "[${model}]{${spec}}" }] });
        }
    }
    let fresh = fresh_name(cx.analysis, &cx.cfg().color.name);
    let mut text = String::new();
    if !x && !cx.analysis.facts.loads.iter().any(|l| l.name == "color") {
        text.push_str("\\usepackage{xcolor}\n");
    }
    text.push_str("\\definecolor{${name}}{${model}}{${spec}}\n");
    let at = preamble_end(cx.doc).map(|line| Position { line, character: 0 });
    let mut edits = vec![json!({ "range": slot, "newText": "${name}" })];
    if let Some(at) = at {
        edits.insert(0, json!({ "range": Range { start: at, end: at }, "newText": text }));
    }
    json!({ "title": format!("Define {fresh}"), "name": fresh, "edits": edits })
}

/// The LSP line of `\begin{document}`, where a definition goes before.
fn preamble_end(doc: &Text) -> Option<u32> {
    doc.body().lines().position(|l| l.trim_start().starts_with("\\begin{document}")).map(|i| i as u32)
}

/// A name the picker defines that no color has: `mycolor`, `mycolor2`, …
fn fresh_name(analysis: &Analysis, base: &str) -> String {
    let taken = |n: &str| analysis.facts.colors.iter().any(|c| c.name == n);
    (1..).map(|i| if i == 1 { base.to_string() } else { format!("{base}{i}") }).find(|n| !taken(n)).unwrap_or_default()
}

/// Whether xcolor is loaded: it reads models such as `HTML` and expressions such as `red!40`.
fn xcolor(analysis: &Analysis) -> bool {
    analysis.facts.loads.iter().any(|l| l.name == "xcolor")
}

/// Whether `cmd` is defined by the color or xcolor package: it takes `[model]{spec}`.
fn by_color_package(analysis: &Analysis, cmd: &str) -> bool {
    let Some(sym) = analysis.interner.lookup(cmd) else { return false };
    analysis.facts.defs.iter().rev().find(|d| d.name == sym).is_some_and(|d| {
        let file = analysis.file_name(d.span.file);
        file.ends_with("/xcolor.sty") || file.ends_with("/color.sty")
    })
}

/// The colors defined where `pos` is: in other files (packages, read before) and before it in this one.
fn defined_at<'a>(analysis: &'a Analysis, path: &str, pos: Pos) -> Vec<&'a ColorDef> {
    let file = super::super::type_at::file_of(analysis, path);
    let mut seen = crate::fixed::HashSet::default();
    (analysis.facts.colors.iter().rev())
        .filter(|c| Some(c.span.file) != file || (c.span.line, c.span.col) <= (pos.line, pos.col))
        .filter(|c| seen.insert(c.name.as_str()) && !analysis.settings.conventions.is_internal(&c.name))
        .collect()
}

/// The color items at `pos`: the colors defined there, newest first, each with its `#rrggbb`
/// as detail and documentation, which an editor shows as a swatch.
pub(super) fn items(analysis: &Analysis, path: &str, pos: Pos) -> Vec<CompletionItem> {
    defined_at(analysis, path, pos)
        .into_iter()
        .map(|c| {
            let hex = value(c).map(hex);
            CompletionItem {
                label: c.name.clone(),
                kind: Some(CompletionItemKind::COLOR),
                detail: hex.clone(),
                documentation: hex.map(lsp_types::Documentation::String),
                sort_text: Some(format!("00000{}", c.name)),
                ..Default::default()
            }
        })
        .collect()
}

/// A definition's value in rgb.
fn value(c: &ColorDef) -> Option<Rgb> {
    let v = super::super::handlers::rgba(c.value.as_ref()?)?;
    Some([v.red, v.green, v.blue].map(f64::from))
}

/// The value of a name or an xcolor expression at `pos`: `a!p!b` mixes `p`% of `a` with the
/// rest of `b` (white when missing), left to right (xcolor § 2.3.2). `None` for a name with no
/// fixed value or an expression with other parts (`-red`, `>wheel`).
fn value_at(analysis: &Analysis, _doc: &Text, path: &str, pos: Pos, expr: &str) -> Option<Rgb> {
    let defs = defined_at(analysis, path, pos);
    let named = |n: &str| defs.iter().find(|c| c.name == n.trim()).and_then(|c| value(c));
    let mut parts = expr.split('!');
    let mut acc = named(parts.next()?)?;
    loop {
        let Some(p) = parts.next() else { return Some(acc) };
        let p: f64 = p.trim().parse().ok().filter(|p| (0.0..=100.0).contains(p))?;
        let other = match parts.next() {
            Some(n) => named(n)?,
            None => [1.0; 3],
        };
        acc = [0, 1, 2].map(|i| p / 100.0 * acc[i] + (1.0 - p / 100.0) * other[i]);
    }
}

/// `#rrggbb`.
fn hex(c: Rgb) -> String {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

/// A 16px swatch as an SVG data URI, which Markdown hovers show.
fn swatch_uri(c: Rgb) -> String {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='48' height='16'><rect width='48' height='16' rx='3' fill='{}' stroke='%23888'/></svg>",
        hex(c).replace('#', "%23")
    );
    format!(
        "data:image/svg+xml;utf8,{}",
        svg.replace(' ', "%20").replace('<', "%3C").replace('>', "%3E").replace('\'', "%27")
    )
}

/// The color in each model the document can write (xcolor's or the color package's), as xcolor's
/// conversions give it (xcolor § 6.3).
fn specs(c: Rgb, xcolor: bool) -> Vec<(&'static str, String)> {
    let n = |v: f64| {
        let s = format!("{:.4}", v.clamp(0.0, 1.0));
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s.is_empty() { "0".to_string() } else { s.to_string() }
    };
    let join = |vs: &[f64]| vs.iter().map(|v| n(*v)).collect::<Vec<_>>().join(",");
    let [r, g, b] = c;
    let k = 1.0 - r.max(g).max(b);
    let cmyk = if k >= 1.0 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [(1.0 - r - k) / (1.0 - k), (1.0 - g - k) / (1.0 - k), (1.0 - b - k) / (1.0 - k), k]
    };
    let gray = 0.3 * r + 0.59 * g + 0.11 * b;
    let mut out = vec![("rgb", join(&c)), ("cmyk", join(&cmyk)), ("gray", n(gray))];
    if xcolor {
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        out.insert(0, ("HTML", format!("{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b))));
        out.insert(2, ("RGB", format!("{},{},{}", byte(r), byte(g), byte(b))));
        out.push(("hsb", join(&hsb(c))));
    }
    out
}

fn hsb([r, g, b]: Rgb) -> [f64; 3] {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = match () {
        _ if d == 0.0 => 0.0,
        _ if max == r => ((g - b) / d).rem_euclid(6.0),
        _ if max == g => (b - r) / d + 2.0,
        _ => (r - g) / d + 4.0,
    } / 6.0;
    [h, if max == 0.0 { 0.0 } else { d / max }, max]
}

/// The range of the two groups `{model}{spec}` right after the name's closing brace, and the model.
fn model_spec(cx: &Cx, hit: &Hit) -> Option<(Range, String)> {
    let line = cx.doc.line(cx.pos.line);
    let typed = hit.typed.as_deref().unwrap_or(&cx.at.item.typed);
    let end = crate::engine::source::byte_of_col(line, cx.at.item.col) + typed.len();
    let rest = line.get(end..)?.strip_prefix('}')?;
    let mut groups = Vec::new();
    let mut at = 0;
    for _ in 0..2 {
        let open = at + rest[at..].find(|c: char| !c.is_whitespace())?;
        rest[open..].starts_with('{').then_some(())?;
        let close = open + rest[open..].find('}')?;
        groups.push(&rest[open + 1..close]);
        at = close + 1;
    }
    let col = |b: usize| crate::engine::source::col_of_byte(line, end + 1 + b);
    Some((super::range(cx.doc, cx.pos.line, col(0), col(at)), groups[0].trim().to_string()))
}

/// `textDocument/documentColor`: each color name a color command used in this file, with the
/// value the run gave it.
pub(super) fn document_colors(analysis: &Analysis, doc: &Text, path: &str) -> Vec<ColorInformation> {
    let own = super::super::type_at::file_of(analysis, path);
    let file = |i: u16| own == Some(i);
    let value = |name: &str| {
        analysis
            .facts
            .colors
            .iter()
            .rev()
            .find(|c| c.name == name)
            .and_then(|c| c.value.as_ref())
            .and_then(super::super::handlers::rgba)
    };
    let mut out: Vec<ColorInformation> = Vec::new();
    // A test (`\ifcsname color@.north\endcsname`) only asks whether a word is a color.
    let used = |u: &&crate::analysis::facts::ColorUse| {
        matches!(u.role, crate::analysis::facts::CsnameRole::Use | crate::analysis::facts::CsnameRole::LetTo)
    };
    for u in analysis.facts.color_uses.iter().filter(used).filter(|u| file(u.span.file)) {
        let Some(color) = value(&u.name) else { continue };
        let line = u.span.line.saturating_sub(1);
        let text = doc.line(u.span.line);
        let from = (u.span.col as usize).min(text.len());
        // the reader stands past the call's argument: the name is the last one before it, else the next
        let Some(at) = super::super::handlers::word_before(text, from, &u.name)
            .or_else(|| super::super::handlers::word_after(text, from, &u.name))
            .filter(|&at| super::super::handlers::argument_value(text, at, &u.name))
        else {
            continue;
        };
        let range = Range::new(Position::new(line, at as u32), Position::new(line, (at + u.name.len()) as u32));
        if !out.iter().any(|c| c.range == range) {
            out.push(ColorInformation { range, color });
        }
    }
    out
}

/// `textDocument/colorPresentation`: what VS Code's picker writes for `color` over the name at
/// `range`: the edits [`writes`] makes, filled with the color as `HTML` (xcolor) or `rgb`.
pub(super) fn presentations(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    range: Range,
    color: lsp_types::Color,
) -> Vec<lsp_types::ColorPresentation> {
    if !analysis.settings.type_plugins.color.enabled {
        return Vec::new();
    }
    let pos = super::super::handlers::to_pos(doc, range.end);
    let at = super::hover_types(analysis, doc, path, pos);
    let cx = super::Cx { analysis, doc, path, pos, at: &at };
    let Some(hit) = at.hits.iter().find(|h| Color.wants(h)) else { return Vec::new() };
    let Some(slot) = slot(&cx, hit) else { return Vec::new() };
    let w = writes(&cx, slot);
    let rgb = [color.red, color.green, color.blue].map(f64::from);
    let x = xcolor(analysis);
    let (model, spec) = specs(rgb, x).into_iter().next().unwrap_or_default();
    let fill = |t: &str| {
        t.replace("${model}", model)
            .replace("${spec}", &spec)
            .replace("${name}", w["name"].as_str().unwrap_or_default())
    };
    let mut edits: Vec<TextEdit> = (w["edits"].as_array().into_iter().flatten())
        .filter_map(|e| {
            Some(TextEdit {
                range: serde_json::from_value(e["range"].clone()).ok()?,
                new_text: fill(e["newText"].as_str()?),
            })
        })
        .collect();
    // the edit over the name is the presentation's own, the rest go along
    let Some(main) = edits.iter().position(|e| e.range.start <= range.start && e.range.end >= range.end) else {
        return Vec::new();
    };
    let own = edits.remove(main);
    vec![lsp_types::ColorPresentation {
        label: own.new_text.clone(),
        text_edit: Some(own),
        additional_text_edits: (!edits.is_empty()).then_some(edits),
    }]
}

#[cfg(test)]
mod tests {
    use super::{hsb, specs};

    #[test]
    fn models_convert_as_xcolor_does() {
        let teal = [0.0, 0.5, 0.5];
        let all = specs(teal, true);
        let get = |m: &str| all.iter().find(|(k, _)| *k == m).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get("HTML"), "008080");
        assert_eq!(get("cmyk"), "1,0,0,0.5");
        assert_eq!(get("gray"), "0.35");
        assert_eq!(get("rgb"), "0,0.5,0.5");
        // without xcolor only the color package's models
        assert!(specs(teal, false).iter().all(|(m, _)| ["rgb", "cmyk", "gray"].contains(m)));
        assert_eq!(hsb([1.0, 0.0, 0.0]), [0.0, 1.0, 1.0]);
    }
}
