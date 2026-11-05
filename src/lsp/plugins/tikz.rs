//! Drawings: in a `tikzpicture` the run met, "Edit drawing…" opens a canvas (`views.js`
//! `drawing`). The run gives the nodes it made, with the shape, center and size pgf stored for
//! each (`pgf@sh@ns@`, `pgf@sh@np@`, `pgf@sh@nt@`); the dialog parses the picture's text, edits
//! nodes and arrows and writes back the text it did not touch as it was. On a blank line of the
//! document body, "New drawing…" and "Insert tikzpicture".

use super::{Cx, Hit, TypePlugin, rewrite};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::{Ty, braced_groups};
use crate::linter::fix::Text;
use crate::project::config::TypePlugins;
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
                json!({ "range": p.range, "body": super::slice(cx.doc, p.range), "nodes": nodes(cx, &p) }),
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

/// The nodes the run made in the picture: name, shape, and where pgf put each (cm) when the
/// run computed it, last definition winning.
fn nodes(cx: &Cx, p: &super::Body) -> Vec<Json> {
    let a = cx.analysis;
    let Some(file) = super::super::type_at::file_of(a, cx.path) else { return Vec::new() };
    let mut out: Vec<(String, [String; 3], u32)> = Vec::new();
    for d in &a.facts.defs {
        if d.span.file != file || d.span.line < p.lines.0 || d.span.line > p.lines.1 {
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
            out.push((node, Default::default(), d.span.line));
            out.len() - 1
        });
        out[at].1[kind] = body.unwrap_or_default();
        out[at].2 = d.span.line;
    }
    out.into_iter()
        .filter(|(n, [shape, ..], _)| !n.is_empty() && !shape.is_empty())
        .map(|(name, [shape, np, nt], line)| {
            let place = place(&np, &nt);
            let cm = |v: f64| (v / PT_PER_CM * 1000.0).round() / 1000.0;
            let [x, y, w, h] = place.map_or([None; 4], |v| v.map(|v| Some(cm(v))));
            json!({ "name": name, "shape": shape, "x": x, "y": y, "w": w, "h": h, "line": line - 1 })
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
