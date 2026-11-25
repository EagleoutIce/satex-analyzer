//! Labelled things: every `\label` the run met, with what it points at (the environment, its
//! number as the build wrote it, a caption or title), where it stands and where it is referenced.
//! Opened from a reference's completion ("Browse labels…"), a choice writes its label there.

use super::{Cx, Hit, TypePlugin};
use crate::analysis::facts::Occurrence;
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::project::config::TypePlugins;
use serde_json::{Value as Json, json};

pub(super) struct Labelled;

impl TypePlugin for Labelled {
    fn name(&self) -> &'static str {
        "labels"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "browse every label: kind, number, caption or title, place and references. A choice writes it into the reference"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Label)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view", "complete"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.labels.enabled
    }
    fn view(&self, cx: &Cx, hit: &Hit) -> Option<Json> {
        let a = cx.analysis;
        let about = super::super::labels::Labels::of(a);
        let path = |o: &Occurrence| a.files.get(usize::from(o.span.file)).map_or(cx.path, |f| f.path.as_str());
        // a reference's key: the text between its group's `{` or `,` and the next `,` or `}`, which a choice replaces
        let cols = cx.doc.columns(cx.pos.line);
        let at = (cx.pos.col as usize).saturating_sub(1).min(cols.len());
        let from = cols[..at].iter().rposition(|c| matches!(c, '{' | ',')).map(|i| i + 1);
        let to = cols[at..].iter().position(|c| matches!(c, '}' | ',')).map(|i| at + i);
        let slot = from.zip(to).filter(|_| !hit.defines);
        // each label's references, gathered once
        let mut refs: std::collections::HashMap<&str, Vec<&Occurrence>> = Default::default();
        for r in a.facts.occurrences.iter().filter(|r| r.kind == OccKind::Ref) {
            refs.entry(r.key.as_str()).or_default().push(r);
        }
        let mut seen = std::collections::HashSet::new();
        let mut items = Vec::new();
        for o in a.facts.occurrences.iter().filter(|o| o.kind == OccKind::Label && o.key.starts_with(&hit.prefix)) {
            if !seen.insert(o.key.as_str()) {
                continue;
            }
            let line = about.about(o).unwrap_or_default();
            // the kind: the line's first word (`figure 2: …` is a figure, `§1.2` a section)
            let kind = if line.starts_with('§') { "section" } else { line.split([' ', ':']).next().unwrap_or("") };
            let refs = refs.get(o.key.as_str()).into_iter().flatten();
            let key = &o.key[hit.prefix.len()..];
            let mut item = json!({
                "title": key,
                "detail": line,
                "search": format!("{key} {line} {}", a.short_name(o.span.file)),
                "sites": super::sites([(path(o), o.span.line)].into_iter().chain(refs.map(|r| (path(r), r.span.line)))),
                "facets": { "kind": if kind.is_empty() { "label" } else { kind } },
                "sort": { "line": format!("{:04}{:06}", o.span.file, o.span.line), "label": key, "number": about.number(&o.key).unwrap_or("") },
            });
            if let Some((from, to)) = slot {
                let range = super::range(cx.doc, cx.pos.line, from as u32 + 1, to as u32 + 1);
                let edit = lsp_types::TextEdit { range, new_text: key.to_string() };
                item["actions"] = json!([{ "title": format!("Insert {key}"), "edit": super::edit_of(cx.path, vec![edit]), "close": true }]);
            }
            items.push(item);
        }
        (!items.is_empty()).then(|| {
            json!({
                "title": "Labels",
                "sections": [{ "kind": "records", "placeholder": "search labels, numbers, captions, files", "sorts": ["line", "label", "number"],
                               "items": items, "empty": "no label" }],
            })
        })
    }
}

/// The completion item of a reference's key that opens the browser at `pos` (LSP), keeping `typed`.
pub(super) fn browse(uri: &str, pos: lsp_types::Position, typed: &str) -> lsp_types::CompletionItem {
    let command = lsp_types::Command {
        title: "Browse labels…".into(),
        command: "satex.openView".into(),
        arguments: Some(vec![json!(uri), json!(pos), json!("labels")]),
    };
    lsp_types::CompletionItem {
        label: "Browse all labels…".into(),
        kind: Some(lsp_types::CompletionItemKind::FOLDER),
        detail: Some("theorems, figures, tables, sections, equations: kind, number, caption, place".into()),
        // always listed, last, and the typed key stays
        filter_text: Some(typed.into()),
        sort_text: Some("~".into()),
        insert_text: Some(typed.into()),
        command: Some(command),
        ..Default::default()
    }
}
