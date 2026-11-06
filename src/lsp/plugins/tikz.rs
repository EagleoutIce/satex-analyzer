//! Drawings: in a `tikzpicture` the run met, "Edit drawing…" opens a canvas (`views.js`
//! `drawing`). The run gives the nodes it made, with the shape, center and size pgf stored for
//! each (`pgf@sh@ns@`, `pgf@sh@np@`, `pgf@sh@nt@`); the dialog parses the picture's text, edits
//! nodes and arrows and writes back the text it did not touch as it was. On a blank line of the
//! document body, "New drawing…" and "Insert tikzpicture".

use super::{Cx, Hit, TypePlugin, rewrite};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::{Ty, braced_groups};
use crate::project::config::TypePlugins;
use crate::text::Text;
use lsp_types::{CodeAction, CodeLens, TextEdit};
use serde_json::{Value as Json, json};

pub(super) struct Tikz;

const ENV: &str = "tikzpicture";
/// Points per centimeter (TeXbook p. 57: 1in = 72.27pt = 2.54cm).
const PT_PER_CM: f64 = 72.27 / 2.54;
/// What "Insert tikzpicture" writes.
const TEMPLATE: &str = "\\node[draw] (a) at (0,0) {A};\n  \\node[draw] (b) at (2,0) {B};\n  \\draw[->] (a) -- (b);\n"; // not prose

impl TypePlugin for Tikz {
    fn name(&self) -> &'static str {
        "tikz"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "a canvas for a tikzpicture's nodes and arrows, Mermaid flowcharts in and out"
    }
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "actions", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.tikz.enabled
    }

    /// Outside a picture, on a blank line of the body: a picture to start from.
    fn actions(&self, cx: &Cx, _hit: &Hit) -> Vec<CodeAction> {
        let Some(new) = super::blank(cx, ENV, "tikz") else { return Vec::new() };
        let note = if new.preamble.is_some() { " (adds \\usepackage{tikz})" } else { "" };
        let mut edits = vec![TextEdit { range: new.at, new_text: wrap(TEMPLATE) }];
        edits.extend(new.preamble);
        vec![rewrite(cx, format!("Insert tikzpicture{note}"), edits)]
    }

    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let cfg = &cx.cfg().tikz;
        let (title, drawing) = match super::body(cx.analysis, cx.doc, cx.path, cx.pos.line, ENV, true) {
            Some(p) => (
                "Edit drawing",
                json!({ "range": p.range, "head": head(cx.doc, &p), "body": super::slice(cx.doc, p.range), "options": options(cx.doc, &p), "nodes": nodes(cx, &p) }),
            ),
            None => {
                let new = super::blank(cx, ENV, "tikz")?;
                let note = new.preamble.as_ref().map(|_| "adds `\\usepackage{tikz}` before `\\begin{document}`");
                (
                    "New drawing",
                    json!({ "range": new.at, "body": "", "nodes": [], "wrap": true, "extra": new.preamble, "note": note }),
                )
            }
        };
        let mut section = drawing;
        section["kind"] = json!("drawing");
        section["run"] = facts(cx);
        section["grid"] = json!(cfg.grid);
        section["spacing"] = json!(cfg.spacing);
        Some(json!({ "title": title, "sections": [section] }))
    }

    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        super::env_lenses(analysis, doc, path, ENV, "Edit drawing…", "tikz")
    }
}

/// A picture's text around `body`.
fn wrap(body: &str) -> String {
    format!("\\begin{{{ENV}}}\n  {body}\\end{{{ENV}}}")
}

/// The text between `\begin{tikzpicture}` and the body: the options, if any.
fn after_begin(doc: &Text, p: &super::Body) -> String {
    let start = lsp_types::Position::new(p.lines.0.saturating_sub(1), 0);
    let head = super::slice(doc, lsp_types::Range { start, end: p.range.start });
    head.rsplit_once(&format!("\\begin{{{ENV}}}")).map_or(String::new(), |(_, o)| o.to_string())
}

/// The picture's options (`every node/.style`, `node distance`), which the canvas applies.
fn options(doc: &Text, p: &super::Body) -> String {
    let head = after_begin(doc, p);
    let head = head.trim();
    head.strip_prefix('[').and_then(|o| o.strip_suffix(']')).unwrap_or_default().trim().to_string()
}

/// Where the options stand, `[…]` or nothing, when on the line of `\begin`: the canvas rewrites them.
fn head(doc: &Text, p: &super::Body) -> Option<lsp_types::Range> {
    let head = after_begin(doc, p);
    let units = head.encode_utf16().count() as u32;
    let start = lsp_types::Position::new(p.range.start.line, p.range.start.character.checked_sub(units)?);
    (!head.contains('\n')).then_some(lsp_types::Range { start, end: p.range.start })
}

/// What the run declared that a canvas offers: arrow tips (each with TeX's picture once drawn),
/// colors, node shapes, layers in their order, the keys that put a scope on a layer, the TikZ
/// libraries loaded and where `\usetikzlibrary` goes for one more.
fn facts(cx: &Cx) -> Json {
    let a = cx.analysis;
    use crate::queries::preview::{Stroke, arrow_specs, drawn};
    let tips: Vec<Json> = arrow_specs(a)
        .iter()
        .filter_map(|s| s.strip_prefix('-'))
        .map(|t| json!({ "name": t, "svg": drawn(a, Stroke::Arrows, &format!("-{t}")).map(|p| p.svg) }))
        .collect();
    let colors: Vec<Json> =
        // not xcolor's current color `.` nor pgf's own
        (super::color_items(a).into_iter())
            .filter(|c| !c.label.starts_with(['.', '-']) && !c.label.starts_with("pgf"))
            .map(|c| json!({ "name": c.label, "hex": c.detail }))
            .collect();
    let text = |name: &str| {
        let m = a.env.meaning(a.interner.lookup(name)?).as_macro().cloned()?;
        Some(crate::engine::tex::detokenize(&m.replacement_text, &a.interner))
    };
    let mut declared = Vec::new();
    let mut shapes = Vec::new();
    let mut keys = serde_json::Map::new();
    for sym in (0..a.interner.len() as u32).map(crate::engine::tex::Sym) {
        let name = a.interner.name(sym);
        if let Some(layer) = name.strip_prefix("pgf@layerbox@").filter(|_| a.env.is_defined(sym)) {
            declared.push(layer.to_string());
        } else if let Some(shape) = name.strip_prefix("pgf@sh@s@").filter(|s| !s.contains('@') && a.env.is_defined(sym))
        {
            shapes.push(shape.to_string());
        } else if let Some(key) = name.strip_prefix("pgfk@/tikz/").and_then(|k| k.strip_suffix("/.@cmd")) {
            // a style whose code opens a layer (backgrounds' `on background layer`)
            let layer = text(name).and_then(|t| {
                let rest = &t[t.find("\\pgfonlayer")? + "\\pgfonlayer".len()..];
                braced_groups(rest).first().map(|l| l.trim().to_string())
            });
            if let Some(layer) = layer.filter(|l| !l.is_empty() && !l.contains(['#', '\\'])) {
                keys.insert(key.to_string(), json!(layer));
            }
        }
    }
    let order: Vec<String> = text("pgf@layerlist")
        .map(|l| l.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    shapes.sort();
    let libraries: Vec<String> = (crate::queries::preview::libraries(a).iter())
        .filter_map(|l| l.strip_prefix("\\usetikzlibrary{")?.strip_suffix('}').map(str::to_string))
        .collect();
    let file = super::super::type_at::file_of(a, cx.path);
    let load_at = crate::linter::shared::document_start(a)
        .filter(|s| Some(s.file) == file)
        .map(|s| lsp_types::Position::new(s.line.saturating_sub(1), 0));
    json!({ "tips": tips, "colors": colors, "shapes": shapes, "libraries": libraries, "load_at": load_at, "layers": { "declared": declared, "order": order }, "layer_keys": keys })
}

/// The nodes the run made in the picture: name, shape, and where pgf put each (cm) when the
/// run computed it, last definition winning.
fn nodes(cx: &Cx, p: &super::Body) -> Vec<Json> {
    let a = cx.analysis;
    let mut out: Vec<(String, [String; 3])> = Vec::new();
    for d in &a.facts.defs {
        // pgf defines them in its own code: the run's order ties them to the picture
        if d.node <= p.ids.0 || d.node >= p.ids.1 {
            continue;
        }
        let name = a.interner.name(d.name);
        let Some((kind, node)) = ["ns", "np", "nt"]
            .iter()
            .enumerate()
            .find_map(|(i, k)| name.strip_prefix(&format!("pgf@sh@{k}@")).map(|n| (i, n.to_string())))
        else {
            continue;
        };
        let body = d.mac.as_ref().map(|m| crate::engine::tex::detokenize(&m.replacement_text, &a.interner));
        let at = out.iter().position(|(n, ..)| *n == node).unwrap_or_else(|| {
            out.push((node, Default::default()));
            out.len() - 1
        });
        out[at].1[kind] = body.unwrap_or_default();
    }
    out.into_iter()
        .filter(|(n, [shape, ..])| !n.is_empty() && !shape.is_empty())
        .map(|(name, [shape, np, nt])| {
            let place = place(&np, &nt);
            let cm = |v: f64| (v / PT_PER_CM * 1000.0).round() / 1000.0;
            let [x, y, w, h] = place.map_or([None; 4], |v| v.map(|v| Some(cm(v))));
            json!({ "name": name, "shape": shape, "x": x, "y": y, "w": w, "h": h })
        })
        .collect()
}

/// A node's center and size in pt: the shift of its transformation (`nt`) plus its center
/// anchor, else the middle of its `southwest` and `northeast` (`np`).
fn place(np: &str, nt: &str) -> Option<[f64; 4]> {
    let pt = |s: &str| s.trim().strip_suffix("pt")?.trim().parse::<f64>().ok();
    let shift = braced_groups(nt);
    let (sx, sy) = (pt(shift.get(4)?)?, pt(shift.get(5)?)?);
    let point = |anchor: &str| {
        let head = format!("\\{anchor} {{\\pgfqpoint ");
        let g = braced_groups(&np[np.find(&head)? + head.len()..]);
        Some((pt(g.first()?)?, pt(g.get(1)?)?))
    };
    let radius = np.find("\\radius ").and_then(|i| braced_groups(&np[i + 8..]).first().and_then(|r| pt(r)));
    let (cx, cy, w, h) = match (point("centerpoint"), point("southwest"), point("northeast"), radius) {
        (Some((x, y)), _, _, Some(r)) => (x, y, 2.0 * r, 2.0 * r),
        (_, Some((x0, y0)), Some((x1, y1)), _) => ((x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0),
        _ => return None,
    };
    Some([sx + cx, sy + cy, w, h])
}
