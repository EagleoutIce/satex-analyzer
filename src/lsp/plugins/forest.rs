//! Trees: in a `forest` environment the run met, "Edit tree…" opens the tree dialog (`views.js`
//! `forest`). The dialog parses the bracket notation itself; the run adds what forest computed
//! per node (`\forestOset` stores option `o` of node `n` as `fRsTn/o`): content, name and the
//! position its packing found, its level, and the anchors of the shapes its nodes have. In a tree,
//! "Strut every node"; on a blank line of the document body, "Insert forest tree".

use super::{Cx, Hit, TypePlugin, rewrite_rooted};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::lsp::handlers::to_pos;
use crate::lsp::snapshot;
use crate::project::config::TypePlugins;
use crate::text::Text;
use lsp_types::{CodeAction, CodeLens};
use serde_json::{Value as Json, json};

pub(super) struct Forest;

const ENV: &str = "forest";
/// The options of a node the dialog shows from the run.
const SHOWN: [&str; 5] = ["content", "name", "x", "y", "level"];
/// What "Insert forest tree" writes.
const TEMPLATE: &str = "[root\n    [child]\n    [child]\n  ]\n"; // not prose

impl TypePlugin for Forest {
    fn name(&self) -> &'static str {
        "forest"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "a tree dialog for a forest environment, indented outlines in and out"
    }
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "actions", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.forest.enabled
    }

    /// In a tree: its nodes strutted. Outside, on a blank line of the body: a tree to start from.
    fn actions(&self, cx: &Cx, _hit: &Hit) -> Vec<CodeAction> {
        if let Some(b) = super::body(cx.analysis, cx.doc, cx.path, cx.pos.line, ENV, false) {
            return strut(cx, &b).into_iter().collect();
        }
        let Some(new) = super::blank(cx, ENV, ENV) else { return Vec::new() };
        let node = snapshot::Node { body: format!("  {TEMPLATE}"), ..snapshot::new_env(ENV, new.at.start, ENV) };
        let Ok((edits, notes, roots)) = snapshot::printed(cx.analysis, cx.doc, cx.file(), &[node]) else {
            return Vec::new();
        };
        let note: String = notes.iter().map(|n| format!(" ({n})")).collect();
        vec![rewrite_rooted(cx, format!("Insert forest tree{note}"), snapshot::lsp_edits(cx.doc, edits), roots)]
    }

    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        // the environment as a snapshot (`node`) the view sets the body of; the printer writes it
        let (title, mut section) = match super::body(cx.analysis, cx.doc, cx.path, cx.pos.line, ENV, false) {
            Some(b) => {
                let (doc, begin) = (cx.doc, format!("\\begin{{{ENV}}}"));
                let body = cx.doc.offset(to_pos(doc, b.range.start))?..cx.doc.offset(to_pos(doc, b.range.end))?;
                let node = snapshot::env(doc, doc.body()[..body.start].rfind(&begin)?, body, false);
                let nodes = nodes(cx, &b);
                let anchors = anchors(cx.analysis, &nodes);
                (
                    "Edit tree",
                    json!({ "body": super::slice(cx.doc, b.range), "nodes": nodes, "anchors": anchors, "node": node }),
                )
            }
            None => {
                let new = super::blank(cx, ENV, ENV)?;
                let note = new.preamble.as_ref().map(|_| "adds `\\usepackage{forest}` before `\\begin{document}`");
                let node = snapshot::new_env(ENV, new.at.start, ENV);
                ("New tree", json!({ "body": "", "nodes": [], "wrap": true, "note": note, "node": node }))
            }
        };
        section["kind"] = json!("forest");
        section["strut"] = json!(super::tikz::strut_key(cx.analysis));
        Some(json!({ "title": title, "sections": [section] }))
    }

    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        super::env_lenses(analysis, doc, path, ENV, "Edit tree…", "forest")
    }
}

/// The nodes forest made from this tree's `\begin` on to the next tree's, by id (creation order,
/// so the bracket order), with the options of [`SHOWN`] the run knows. Forest defines them in
/// its own code, so the run's order, not the definition's place, ties them to the tree.
fn nodes(cx: &Cx, b: &super::Body) -> Vec<Json> {
    let a = cx.analysis;
    let begins: Vec<&crate::analysis::facts::Occurrence> =
        super::pairs(a, cx.path, ENV).into_iter().map(|(b, _)| b).collect();
    let Some(from) = begins.iter().find(|o| o.span.line == b.lines.0).map(|o| o.node) else { return Vec::new() };
    let to = begins.iter().map(|o| o.node).filter(|&n| n > from).min().unwrap_or(u32::MAX);
    let mut out: std::collections::BTreeMap<u64, serde_json::Map<String, Json>> = Default::default();
    for d in &a.facts.defs {
        if d.node <= from || d.node >= to {
            continue;
        }
        let Some((id, option)) = a.interner.name(d.name).strip_prefix("fRsT").and_then(|n| n.split_once('/')) else {
            continue;
        };
        let (Ok(id), Some(m)) = (id.parse::<u64>(), d.mac.as_ref()) else { continue };
        if !SHOWN.contains(&option) {
            continue;
        }
        let value = crate::engine::tex::detokenize(&m.replacement_text, &a.interner);
        let node = out.entry(id).or_default();
        node.insert("id".into(), json!(id));
        match option {
            "level" => {
                node.insert(option.into(), json!(value.trim().parse::<i64>().ok()));
            }
            "x" | "y" => {
                let pt = value.trim().strip_suffix("pt").and_then(|v| v.trim().parse::<f64>().ok());
                node.insert(option.into(), json!(pt));
            }
            _ => {
                node.insert(option.into(), json!(value));
            }
        }
    }
    out.into_values().filter(|n| n.contains_key("content")).map(Json::Object).collect()
}

/// "Strut every node": TikZ's strut key ([`super::tikz::strut_key`]) in the tree's `for tree={…}`, which
/// gets one when the preamble has none. None when the run lacks the key or the preamble struts already.
fn strut(cx: &Cx, b: &super::Body) -> Option<CodeAction> {
    let key = super::tikz::strut_key(cx.analysis)?;
    let doc = cx.doc;
    let body = doc.offset(to_pos(doc, b.range.start))?..doc.offset(to_pos(doc, b.range.end))?;
    let mut node = snapshot::env(doc, doc.body()[..body.start].rfind(&format!("\\begin{{{ENV}}}"))?, body, false)?;
    let pre = &node.body[..tree_start(&node.body)?];
    if pre.contains("\\strut") {
        return None;
    }
    let pre = match pre.find("for tree").and_then(|i| Some(i + pre[i..].find('{')?)) {
        Some(open) => {
            let close = super::closing(pre, open, '{')?;
            let sep = if pre[open + 1..close].trim().is_empty() { "" } else { ", " };
            format!("{}{sep}{key}{}", pre[..close].trim_end(), &pre[close..])
        }
        // white space only: the key on a line of its own, the tree after it as before
        None if pre.trim().is_empty() => format!("{pre}for tree={{{key}}}{pre}"),
        None => {
            let lead = pre.len() - pre.trim_start().len();
            format!("{}for tree={{{key}}}, {}", &pre[..lead], &pre[lead..])
        }
    };
    node.body = pre + &node.body[tree_start(&node.body)?..];
    let (edits, ..) = snapshot::printed(cx.analysis, doc, cx.file(), &[node]).ok()?;
    Some(super::rewrite(cx, "Strut every node".into(), snapshot::lsp_edits(doc, edits)))
}

/// The byte of a tree's root `[` in an environment body: the first outside braces and comments.
fn tree_start(body: &str) -> Option<usize> {
    let (b, mut depth, mut i) = (body.as_bytes(), 0i32, 0);
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 1,
            b'%' => i = body[i..].find('\n').map_or(b.len(), |k| i + k),
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'[' if depth == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// The anchors of the shapes the run gave the tree's named nodes (`pgf@sh@ns@⟨name⟩`), as pgf
/// declared them (`pgf@anchor@⟨shape⟩@⟨anchor⟩`); rectangle's, forest's default, when it gave none.
/// `border` is pgf's own (it computes the angle anchors), never written.
fn anchors(a: &Analysis, nodes: &[Json]) -> Vec<String> {
    let text = |name: &str| {
        let m = a.env.meaning(a.interner.lookup(name)?).as_macro().cloned()?;
        Some(crate::engine::tex::detokenize(&m.replacement_text, &a.interner))
    };
    let mut shapes: std::collections::BTreeSet<String> = (nodes.iter())
        .filter_map(|n| text(&format!("pgf@sh@ns@{}", n["name"].as_str()?)))
        .map(|s| s.trim().to_string())
        .collect();
    if shapes.is_empty() {
        shapes.insert("rectangle".into());
    }
    let mut out: Vec<String> = Vec::new();
    for sym in (0..a.interner.len() as u32).map(crate::engine::tex::Sym) {
        let name = a.interner.name(sym);
        let Some((shape, anchor)) = name.strip_prefix("pgf@anchor@").and_then(|r| r.split_once('@')) else { continue };
        if shapes.contains(shape) && anchor != "border" && !out.iter().any(|o| o == anchor) && a.env.is_defined(sym) {
            out.push(anchor.to_string());
        }
    }
    out
}
