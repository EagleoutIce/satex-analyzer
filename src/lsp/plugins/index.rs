//! Index terms: every term a written index line carried in the run (`\index{graph!directed}`
//! writes `\indexentry`), by its top level with the subentries below it, in sorted order, with
//! where each stands; renaming a term rewrites it in every call of the open file.

use super::{Cx, Hit, TypePlugin, call_at, edit_of, put};
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::project::config::TypePlugins;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;

pub(super) struct Index;

/// A use of a term: its subentry, line and the bytes of its top level in the open file.
type Use = (String, u32, Option<std::ops::Range<usize>>);

impl TypePlugin for Index {
    fn name(&self) -> &'static str {
        "index"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "manage index terms: sorted terms with subentries, rename a term everywhere"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Entry)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view", "manage"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.index.enabled
    }
    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let mut v = self.manage(cx)?;
        let typed = cx.at.item.typed.trim();
        v["sections"][0]["focus"] = json!(level(typed).0);
        Some(v)
    }
    fn manage(&self, cx: &Cx) -> Option<Json> {
        let occ = &cx.analysis.facts.occurrences;
        let text = cx.doc.body();
        // the database lines of the `.aux` file carry keys too: their calls name bibliographies
        let other: Vec<_> = occ
            .iter()
            .filter(|o| matches!(o.kind, OccKind::Bibliography | OccKind::BibStyle))
            .map(|o| o.span)
            .collect();
        let mut terms: BTreeMap<String, Vec<Use>> = BTreeMap::new();
        for o in occ.iter().filter(|o| o.kind == OccKind::Entry && !other.contains(&o.span)) {
            let (top, rest) = level(&o.key);
            // the top level's bytes in the call's argument, where the call stands in the open file
            let range = super::byte_of(cx, o.span)
                .and_then(|at| call_at(text, at))
                .and_then(|(_, g, _)| g.into_iter().find(|g| g.open == '{' && text[g.inner.clone()] == o.key))
                .map(|g| g.inner.start..g.inner.start + top.len());
            let uses = terms.entry(top.to_string()).or_default();
            if !uses.iter().any(|u| u.1 == o.span.line && u.2 == range && u.0 == rest) {
                uses.push((rest.to_string(), o.span.line, range));
            }
        }
        if terms.is_empty() {
            return None;
        }
        let items: Vec<Json> = terms
            .iter()
            .map(|(term, uses)| {
                let rows: Vec<Json> = uses.iter().map(|(sub, line, _)| json!([sub, format!("line {line}")])).collect();
                let subs = uses.iter().filter(|u| !u.0.is_empty()).count();
                let edits: Vec<_> = uses.iter().filter_map(|u| u.2.clone()).map(|r| put(cx.doc, r.start, r.end, "${value}")).collect();
                let fields = if edits.len() == uses.len() {
                    vec![json!({ "name": "term", "value": term, "edit": edit_of(cx.path, edits) })]
                } else {
                    Vec::new()
                };
                json!({
                    "title": term,
                    "detail": format!("{}{}", crate::util::render::plural(uses.len(), "use"), if subs > 0 { format!(", {subs} with subentries") } else { String::new() }),
                    "search": format!("{term} {}", uses.iter().map(|u| u.0.as_str()).collect::<Vec<_>>().join(" ")),
                    "sort": { "term": sort_key(term), "uses": format!("{:06}", uses.len()) },
                    "fields": fields,
                    "save": "Rename everywhere",
                    "table": { "columns": ["subentry", "where"], "rows": rows },
                })
            })
            .collect();
        Some(json!({
            "title": "Index terms",
            "sections": [{ "kind": "records", "placeholder": "search terms", "sorts": ["term", "uses"], "items": items }],
        }))
    }
}

/// An index key's top level and the rest after its first level separator (makeindex's `!`;
/// `"` quotes the next character, `|` starts the page format).
fn level(key: &str) -> (&str, &str) {
    let mut quoted = false;
    for (i, c) in key.char_indices() {
        match c {
            '"' if !quoted => quoted = true,
            '!' if !quoted => return (&key[..i], &key[i + 1..]),
            // the page's format (`|textbf`) is no level
            '|' if !quoted => return (&key[..i], ""),
            _ => quoted = false,
        }
    }
    (key, "")
}

/// What makeindex sorts a level by: the part before its `@` (the rest is how it prints).
fn sort_key(term: &str) -> &str {
    term.split_once('@').map_or(term, |(s, _)| s)
}
