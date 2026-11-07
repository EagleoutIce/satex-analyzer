//! Drawings: in a `tikzpicture` the run met, "Edit drawing…" opens a canvas (`views.js`
//! `drawing`). The run gives the nodes it made, with the shape, center and size pgf stored for
//! each (`pgf@sh@ns@`, `pgf@sh@np@`, `pgf@sh@nt@`); the dialog parses the picture's text, edits
//! nodes and arrows and writes back the text it did not touch as it was. On a blank line of the
//! document body, "New drawing…" and "Insert tikzpicture".

use super::{Cx, Hit, TypePlugin, rewrite};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::{Ty, braced_groups};
use crate::lsp::handlers::to_pos;
use crate::lsp::snapshot::{self, Node};
use crate::project::config::TypePlugins;
use crate::text::Text;
use lsp_types::{CodeAction, CodeLens};
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
        let node = Node { body: format!("  {TEMPLATE}"), ..snapshot::new_env(ENV, new.at.start, "tikz") };
        let Ok((edits, notes)) = snapshot::printed(cx.analysis, cx.doc, &[node]) else { return Vec::new() };
        let note: String = notes.iter().map(|n| format!(" ({n})")).collect();
        vec![rewrite(cx, format!("Insert tikzpicture{note}"), snapshot::lsp_edits(cx.doc, edits))]
    }

    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let cfg = &cx.cfg().tikz;
        // the picture as a snapshot (`node`) the view sets the options and body of; the printer writes it
        let (title, mut section, own) = match super::body(cx.analysis, cx.doc, cx.path, cx.pos.line, ENV, true) {
            Some(p) => {
                let doc = cx.doc;
                let body = doc.offset(to_pos(doc, p.range.start))?..doc.offset(to_pos(doc, p.range.end))?;
                let at = doc.body()[..body.start].rfind(&format!("\\begin{{{ENV}}}"))?;
                let node = snapshot::env(doc, at, body.clone(), true);
                (
                    "Edit drawing",
                    json!({ "body": super::slice(doc, p.range), "nodes": nodes(cx, &p), "node": node }),
                    at..body.end,
                )
            }
            None => {
                let new = super::blank(cx, ENV, "tikz")?;
                let note = new.preamble.as_ref().map(|_| "adds `\\usepackage{tikz}` before `\\begin{document}`");
                let node = snapshot::new_env(ENV, new.at.start, "tikz");
                ("New drawing", json!({ "body": "", "nodes": [], "wrap": true, "note": note, "node": node }), 0..0)
            }
        };
        section["kind"] = json!("drawing");
        section["run"] = facts(cx, own);
        section["grid"] = json!(cfg.grid);
        section["spacing"] = json!(cfg.spacing);
        section["relative"] = json!(cfg.relative);
        Some(json!({ "title": title, "sections": [section] }))
    }

    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        super::env_lenses(analysis, doc, path, ENV, "Edit drawing…", "tikz")
    }
}

/// What the run declared that a canvas offers: arrow tips (each with TeX's picture once drawn),
/// colors, node shapes, layers in their order, the keys that put a scope on a layer, the TikZ
/// libraries loaded and where `\usetikzlibrary` goes for one more. `own`: the picture's bytes.
fn facts(cx: &Cx, own: std::ops::Range<usize>) -> Json {
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
    let mut keys = serde_json::Map::new();
    for sym in (0..a.interner.len() as u32).map(crate::engine::tex::Sym) {
        let name = a.interner.name(sym);
        if let Some(layer) = name.strip_prefix("pgf@layerbox@").filter(|_| a.env.is_defined(sym)) {
            declared.push(layer.to_string());
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
    let shapes = shapes(a, &text);
    let libraries: Vec<String> = (crate::queries::preview::libraries(a).iter())
        .filter_map(|l| l.strip_prefix("\\usetikzlibrary{")?.strip_suffix('}').map(str::to_string))
        .collect();
    let file = super::super::type_at::file_of(a, cx.path);
    let load_at = crate::linter::shared::document_start(a)
        .filter(|s| Some(s.file) == file)
        .map(|s| lsp_types::Position::new(s.line.saturating_sub(1), 0));
    let (keys_of, defs) = tikz_keys(cx, file, own);
    let sample = [crate::queries::preview::SAMPLE_W, crate::queries::preview::SAMPLE_H];
    json!({ "tips": tips, "colors": colors, "shapes": shapes, "sample": sample, "libraries": libraries, "load_at": load_at, "layers": { "declared": declared, "order": order }, "layer_keys": keys, "keys": keys_of, "defs": defs })
}

/// Every node shape: the run's (`pgf@sh@s@⟨name⟩`) with the `/pgf/` keys its code reads that
/// `rectangle`'s does not (`regular polygon sides`) and their values, then those the
/// installation's libraries declare with the `load` that brings each. Each with TeX's `svg` sample.
fn shapes(a: &Analysis, text: &dyn Fn(&str) -> Option<String>) -> Vec<Json> {
    use crate::queries::preview::{Stroke, drawn, library_shapes, shape_names};
    // the keys a shape's saved macros and background path read
    let reads = |shape: &str| -> std::collections::BTreeSet<String> {
        let code: String = ["s", "bg"].iter().filter_map(|k| text(&format!("pgf@sh@{k}@{shape}"))).collect();
        code.split("{/pgf/").skip(1).filter_map(|r| Some(r[..r.find('}')?].trim().to_string())).collect()
    };
    let base = reads("rectangle");
    let svg = |name: &str| drawn(a, Stroke::Shape, name).map(|p| p.svg);
    let own = shape_names(a).into_iter().map(|name| {
        let keys: Vec<Json> = (reads(&name).difference(&base))
            .filter_map(|k| Some(json!({ "name": k, "default": text(&format!("pgfk@/pgf/{k}"))? })))
            .collect();
        json!({ "name": name, "svg": svg(&name), "keys": keys })
    });
    let more =
        library_shapes(a).into_iter().map(|(name, load)| json!({ "name": name, "svg": svg(&name), "load": load }));
    own.chain(more).collect()
}

/// The `/tikz/` keys the run stored, as the property editor offers them: `name`, the value's
/// `type` read off the key's handler and the setters its code reaches (`color`, `length`,
/// `choice` with its `choices`, `bool`, `node`, `flag` for none, else `text`), its `default`. A
/// style has its `body`; one this file defines outside the picture (`\tikzset{name/.style={…}}`)
/// its `style`: the defining call's index in the snapshots returned, and its `key` as written.
fn tikz_keys(cx: &Cx, file: Option<crate::engine::tex::FileId>, own: std::ops::Range<usize>) -> (Vec<Json>, Vec<Node>) {
    use crate::queries::preview::{Stroke, sets};
    let a = cx.analysis;
    let records = crate::queries::query::pgfkeys(a, Some("/tikz/"));
    let str_of =
        |r: &crate::queries::query::Record, k: &str| r.get(k).and_then(Json::as_str).unwrap_or_default().to_string();
    let path_of =
        |r: &crate::queries::query::Record| str_of(r, "key").strip_prefix("/tikz/").unwrap_or_default().to_string();
    let here = file.map(|f| a.file_name(f).to_string());
    let (mut out, mut defs) = (Vec::new(), Vec::<Node>::new());
    for r in &records {
        let (name, kind, code) = (path_of(r), str_of(r, "kind"), str_of(r, "value"));
        // pgf's own internals, handlers and subkeys (a choice's, a family's) are no properties
        if name.is_empty()
            || name.contains(['@', '/'])
            || name.starts_with('.')
            || !matches!(kind.as_str(), "style" | "is if" | "is choice" | "code" | "code args")
        {
            continue;
        }
        let default = str_of(r, "default");
        let path = format!("/tikz/{name}");
        let reached = sets(a, &path);
        let ty = match kind.as_str() {
            "is if" => "bool",
            "is choice" => "choice",
            _ if reached.contains(&Stroke::Color) => "color",
            _ if reached.contains(&Stroke::Width) || code.contains("pgfmathsetlength") => "length",
            _ if reached.iter().any(|s| matches!(s, Stroke::Node | Stroke::NodeOf)) => "node",
            _ if !code.contains("#1") => "flag",
            _ => "text",
        };
        let mut key = json!({ "name": name, "type": ty });
        if ty == "choice" {
            let sub = format!("{name}/");
            let choices: Vec<String> = (records.iter().map(path_of))
                .filter_map(|p| {
                    p.strip_prefix(&sub).filter(|c| !c.contains(['/', '@']) && !c.starts_with('.')).map(str::to_string)
                })
                .collect();
            key["choices"] = json!(choices);
        }
        // a default, even an empty one (`fill` alone), lets the key stand without a value
        if r.get("default").is_some_and(Json::is_string) && !default.contains("pgfkeysvaluerequired") {
            key["default"] = json!(default);
        }
        if kind == "style" {
            let body = code.trim().strip_prefix("\\pgfkeysalso").map(|b| {
                let b = b.trim();
                b.strip_prefix('{').and_then(|b| b.strip_suffix('}')).unwrap_or(b).trim()
            });
            key["body"] = json!(body.unwrap_or_default());
        }
        // the document's own: made by a call in this file that writes `name/.style={…}` there
        let line = r.get("line").and_then(Json::as_u64).unwrap_or(1) as u32;
        let found = (kind == "style" && here.as_deref() == r.get("path").and_then(Json::as_str))
            .then(|| style_def(cx.doc, &name, line))
            .flatten()
            .filter(|(at, ..)| !own.contains(at));
        if let Some((_, node, k)) = found {
            let def = defs.iter().position(|d| d.range == node.range).unwrap_or_else(|| {
                defs.push(node);
                defs.len() - 1
            });
            key["body"] = json!(snapshot::list(&defs[def]).iter().find(|x| x.key == k).and_then(|x| x.value.clone()));
            key["style"] = json!({ "def": def, "key": k });
        }
        out.push(key);
    }
    (out, defs)
}

/// The call around `line` in `doc` whose first key list defines `name/.style` (the run's line
/// is the call's first or last): where it starts, read as a snapshot, and the key as written.
fn style_def(doc: &Text, name: &str, line: u32) -> Option<(usize, Node, String)> {
    let text = doc.body();
    let from = doc.offset(crate::text::Pos::new(line, 1))?;
    let to = doc.offset(crate::text::Pos::new(line + 1, 1)).unwrap_or(text.len());
    let defines = |k: &&snapshot::Key| k.key.strip_suffix("/.style").is_some_and(|n| n.trim() == name);
    // the places the name is written from the line on, then before its end; at each the innermost call holding it
    let after = text[from..].match_indices(name).map(|(i, _)| from + i);
    after.chain(text[..to].rmatch_indices(name).map(|(i, _)| i)).find_map(|at| {
        let c = text[..at]
            .rmatch_indices('\\')
            .map(|(c, _)| c)
            .find(|&c| super::call_at(text, c).is_some_and(|(.., end)| end > at))?;
        let node = snapshot::read(doc, c, &[0], false)?;
        let key = snapshot::list(&node).iter().find(defines)?.key.clone();
        Some((c, node, key))
    })
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
