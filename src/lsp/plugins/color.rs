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
use crate::lsp::handlers::to_pos;
use crate::lsp::snapshot::{self, Node, Package};
use crate::project::config::TypePlugins;
use crate::text::{Pos, Text};
use lsp_types::{ColorInformation, CompletionItem, CompletionItemKind, Position, Range, TextEdit};
use serde_json::{Value as Json, json};

pub(super) struct Color;

use crate::util::color::{Rgb, hex, specs};

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
        let defined = palette(cx.analysis, cx.path, cx.pos);
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

/// What writing a color into `slot` takes, as snapshots the view fills and the printer writes:
/// `name`, the slot's text; with xcolor, `own`, the xcolor command whose first argument the slot
/// is (its `[model]` and `{spec}`); else `define`, a `\definecolor{name}{model}{spec}` before
/// `\begin{document}` (xcolor loaded when the run defines none) and `name`, the fresh name.
fn writes(cx: &Cx, slot: Range) -> Json {
    let doc = cx.doc;
    let (from, to) = (doc.offset(to_pos(doc, slot.start)), doc.offset(to_pos(doc, slot.end)));
    let name = from.zip(to).map(|(a, b)| snapshot::text(doc, a, b));
    let x = xcolor(cx.analysis);
    let place = cx.at.place.as_ref();
    let own = place.filter(|p| p.open == '{' && p.nth() == 0 && x && by_color_package(cx.analysis, &p.cmd));
    // the command with its `[model]` as text, its `{spec}` the slot's group, closing right after it
    let own = own.and_then(|p| doc.offset(p.at)).and_then(|at| {
        let mut n = snapshot::read(doc, at, &[], true)?;
        n.args[0].keys = None;
        (n.args.get(1)?.inner.map(|(_, e)| at + e) == to).then_some(n)
    });
    if let Some(own) = own {
        return json!({ "title": "Write here", "nodes": { "name": name, "own": own } });
    }
    let fresh = fresh_name(cx.analysis, &cx.cfg().color.name);
    let at = preamble_end(doc).map(|line| Position { line, character: 0 });
    let define = at.map(|at| Node {
        package: Some(Package { name: "xcolor".into(), options: String::new() }),
        ..Node::call("definecolor", &["", "", ""], at)
    });
    json!({ "title": format!("Define {fresh}"), "name": fresh, "nodes": { "name": name, "define": define } })
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

/// The colors defined at `pos` with a fixed value: name, rgb, `#rrggbb` and who defined them.
pub(super) fn palette(analysis: &Analysis, path: &str, pos: Pos) -> Vec<Json> {
    (defined_at(analysis, path, pos).into_iter())
        .filter_map(|c| {
            Some(json!({ "name": c.name, "rgb": value(c)?, "css": hex(value(c)?), "group": group(analysis, c) }))
        })
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
    c.value.as_ref()?.rgb()
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

/// The range of the two groups `{model}{spec}` right after the name's closing brace, and the model.
fn model_spec(cx: &Cx, hit: &Hit) -> Option<(Range, String)> {
    let line = cx.doc.line(cx.pos.line);
    let typed = hit.typed.as_deref().unwrap_or(&cx.at.item.typed);
    let end = crate::engine::source::byte_of_col(line, cx.at.item.col) + typed.len();
    let rest = line.get(end..)?.strip_prefix('}')?;
    let groups = groups(rest, 2)?;
    let at = groups[1].end + 1;
    let col = |b: usize| crate::engine::source::col_of_byte(line, end + 1 + b);
    Some((super::range(cx.doc, cx.pos.line, col(0), col(at)), rest[groups[0].clone()].trim().to_string()))
}

/// `textDocument/documentColor`: each color a color command read in this file, over all of
/// what the document wrote for it: a used name or expression (`accent!8`) with its value at
/// that place, and a definition's spec (`{HTML}{0B3D91}`) with the color it defined.
pub(super) fn document_colors(analysis: &Analysis, doc: &Text, path: &str) -> Vec<ColorInformation> {
    use crate::engine::source::col_of_byte;
    let own = super::super::type_at::file_of(analysis, path);
    let file = |i: u16| own == Some(i);
    let rgba = |[r, g, b]: Rgb| lsp_types::Color { red: r as f32, green: g as f32, blue: b as f32, alpha: 1.0 };
    let mut out: Vec<ColorInformation> = Vec::new();
    // one swatch per place: a range overlapping one taken already is the same color written once
    let mut add = |line: u32, text: &str, from: usize, to: usize, color| {
        let at = |b| Position::new(line - 1, col_of_byte(text, b) - 1);
        let range = Range::new(at(from), at(to));
        let overlaps = |r: &Range| r.start.line == range.start.line && r.start < range.end && range.start < r.end;
        if from < to && !out.iter().any(|c| overlaps(&c.range)) {
            out.push(ColorInformation { range, color });
        }
    };
    for (c, line, _, spec) in written_specs(analysis, doc, path) {
        if let Some(rgb) = value(c) {
            add(line, doc.line(line), spec.start, spec.end, rgba(rgb));
        }
    }
    // A test (`\ifcsname color@.north\endcsname`) only asks whether a word is a color.
    let used = |u: &&crate::analysis::facts::ColorUse| {
        matches!(u.role, crate::analysis::facts::CsnameRole::Use | crate::analysis::facts::CsnameRole::LetTo)
    };
    for u in analysis.facts.color_uses.iter().filter(used).filter(|u| file(u.span.file)) {
        let text = doc.line(u.span.line);
        let from = (u.span.col as usize).min(text.len());
        // the reader stands past the call's argument: the name is the last one before it, else the next
        let Some(at) = super::super::handlers::word_before(text, from, &u.name)
            .or_else(|| super::super::handlers::word_after(text, from, &u.name))
            .filter(|&at| super::super::handlers::argument_value(text, at, &u.name))
        else {
            continue;
        };
        // the whole expression the name starts (`accent!8!black`) when it has a value, else the name
        let pos = Pos::new(u.span.line, col_of_byte(text, at));
        let end = at + expression_len(&text[at..]);
        let whole = value_at(analysis, doc, path, pos, &text[at..end]).map(|rgb| (end, rgb));
        let Some((end, rgb)) =
            whole.or_else(|| Some((at + u.name.len(), value_at(analysis, doc, path, pos, &u.name)?)))
        else {
            continue;
        };
        add(u.span.line, text, at, end, rgba(rgb));
    }
    out
}

/// The specs this file's definitions wrote (the last of `{name}{model}{spec}` on the
/// definition's line): each definition, its line, its name's start and the spec, trimmed, as
/// byte offsets in the line.
fn written_specs<'a>(
    analysis: &'a Analysis,
    doc: &'a Text,
    path: &str,
) -> impl Iterator<Item = (&'a ColorDef, u32, usize, std::ops::Range<usize>)> + 'a {
    let own = super::super::type_at::file_of(analysis, path);
    analysis.facts.colors.iter().filter(move |c| own == Some(c.span.file)).filter_map(|c| {
        let text = doc.line(c.span.line);
        // the span stands on the name or on its group's brace
        let at = crate::engine::source::byte_of_col(text, c.span.col);
        let from = text[..at].rfind('{').filter(|&b| text[b + 1..at].trim().is_empty()).unwrap_or(at);
        let [name, _, spec] = <[_; 3]>::try_from(groups(&text[from..], 3)?).ok()?;
        let inside = &text[from..][spec.clone()];
        let start = from + spec.start + (inside.len() - inside.trim_start().len());
        let name_at =
            from + name.start + (text[from..][name.clone()].len() - text[from..][name.clone()].trim_start().len());
        (text[from..][name].trim() == c.shown)
            .then(|| (c, c.span.line, name_at, start..from + spec.start + inside.trim_end().len()))
    })
}

/// A place on a definition's spec stands for the color it defines: the place of its name
/// there, where the color's views and picker work. `None` off every spec.
pub(super) fn named_at(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> Option<Pos> {
    let text = doc.line(pos.line);
    let at = crate::engine::source::byte_of_col(text, pos.col);
    written_specs(analysis, doc, path)
        .find(|(_, line, _, spec)| *line == pos.line && spec.start <= at && at <= spec.end)
        .map(|(_, line, name, _)| Pos::new(line, crate::engine::source::col_of_byte(text, name)))
}

/// The insides of the `n` brace groups `s` starts with, spaces between them allowed: byte ranges
/// in `s`. `None` when fewer stand there on the line.
fn groups(s: &str, n: usize) -> Option<Vec<std::ops::Range<usize>>> {
    let mut out = Vec::with_capacity(n);
    let mut at = 0;
    for _ in 0..n {
        let open = at + s[at..].find(|c: char| !c.is_whitespace())?;
        s[open..].starts_with('{').then_some(())?;
        let mut depth = 0usize;
        let close = open
            + 1
            + s[open + 1..].char_indices().find_map(|(i, c)| match c {
                '{' => {
                    depth += 1;
                    None
                }
                '}' if depth == 0 => Some(i),
                '}' => {
                    depth -= 1;
                    None
                }
                _ => None,
            })?;
        out.push(open + 1..close);
        at = close + 1;
    }
    Some(out)
}

/// The length of the xcolor expression `s` starts with: names, `!` and percentages, up to the
/// argument's end (`}`, `]`, `,`, `;`, `=`) or a space before one.
fn expression_len(s: &str) -> usize {
    let end = s.find(['}', ']', ',', ';', '=', '{', '\\']).unwrap_or(s.len());
    s[..end].trim_end().len()
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
    let (model, spec) = specs(rgb, xcolor(analysis)).into_iter().next().unwrap_or_default();
    // the snapshots filled as the picker fills them: the command's, else a definition and the name
    let node = |k: &str, texts: &[&str]| {
        let mut n: Node = serde_json::from_value(w["nodes"][k].clone()).ok()?;
        n.args.iter_mut().zip(texts).for_each(|(a, t)| a.text = t.to_string());
        Some(n)
    };
    let name = w["name"].as_str().unwrap_or_default();
    let nodes = match node("own", &[model, &spec]) {
        Some(own) => vec![own],
        None => vec![node("define", &[name, model, &spec]), node("name", &[]).map(|n| Node { body: name.into(), ..n })]
            .into_iter()
            .flatten()
            .collect(),
    };
    let Ok((edits, _)) = snapshot::printed(analysis, doc, &nodes) else { return Vec::new() };
    // the presentation's own edit: the slot's node (the last) from its first edit to its end, the rest go along
    let byte = |p| doc.offset(to_pos(doc, p));
    let Some(held) = nodes.last().and_then(|n| n.range).and_then(|r| Some(byte(r.start)?..byte(r.end)?)) else {
        return Vec::new();
    };
    let (own, rest): (Vec<_>, Vec<_>) = edits.into_iter().partition(|e| held.start <= e.0 && e.1 <= held.end);
    let Some(from) = own.iter().map(|e| e.0).min() else { return Vec::new() };
    let to = held.end;
    let own = TextEdit::new(super::span(doc, from, to), snapshot::reads(doc.body(), from, to, &own));
    let rest = snapshot::lsp_edits(doc, rest);
    vec![lsp_types::ColorPresentation {
        label: own.new_text.clone(),
        text_edit: Some(own),
        additional_text_edits: (!rest.is_empty()).then_some(rest),
    }]
}

#[cfg(test)]
mod tests {
    use crate::util::color::{hsb, specs};

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
