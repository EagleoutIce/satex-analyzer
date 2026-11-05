//! Trees: in a `forest` environment the run met, "Edit tree…" opens the tree dialog (`views.js`
//! `forest`). The dialog parses the bracket notation itself; the run adds what forest computed
//! per node (`\forestOset` stores option `o` of node `n` as `fRsTn/o`): content, name and the
//! position its packing found. On a blank line of the document body, "Insert forest tree".

use super::{Cx, Hit, TypePlugin, rewrite};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::linter::fix::Text;
use crate::project::config::TypePlugins;
use lsp_types::{CodeAction, CodeLens, TextEdit};
use serde_json::{Value as Json, json};

pub(super) struct Forest;

const ENV: &str = "forest";
/// The options of a node the dialog shows from the run.
const SHOWN: [&str; 4] = ["content", "name", "x", "y"];
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

    /// Outside a tree, on a blank line of the body: a tree to start from.
    fn actions(&self, cx: &Cx, _hit: &Hit) -> Vec<CodeAction> {
        let Some(new) = super::blank(cx, ENV, ENV) else { return Vec::new() };
        let note = if new.preamble.is_some() { " (adds \\usepackage{forest})" } else { "" };
        let mut edits =
            vec![TextEdit { range: new.at, new_text: format!("\\begin{{{ENV}}}\n  {TEMPLATE}\\end{{{ENV}}}") }];
        edits.extend(new.preamble);
        vec![rewrite(cx, format!("Insert forest tree{note}"), edits)]
    }

    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let (title, mut section) = match super::body(cx.analysis, cx.doc, cx.path, cx.pos.line, ENV, false) {
            Some(b) => (
                "Edit tree",
                json!({ "range": b.range, "body": super::slice(cx.doc, b.range), "nodes": nodes(cx, &b) }),
            ),
            None => {
                let new = super::blank(cx, ENV, ENV)?;
                let note = new.preamble.as_ref().map(|_| "adds `\\usepackage{forest}` before `\\begin{document}`");
                (
                    "New tree",
                    json!({ "range": new.at, "body": "", "nodes": [], "wrap": true, "extra": new.preamble, "note": note }),
                )
            }
        };
        section["kind"] = json!("forest");
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
