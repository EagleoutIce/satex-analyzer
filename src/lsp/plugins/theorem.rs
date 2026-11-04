//! Theorem-like environments: the environments a document call defined together with their
//! number (`\the⟨env⟩`), whatever the package (`\newtheorem`, `\declaretheorem`,
//! `\newtcbtheorem`). Kinds with their counter, the counter they share or the one they are
//! numbered within, and what sets them apart (a style); every instance with its labels and
//! references. Renaming a kind rewrites its definition, every `\begin`/`\end` and the kinds
//! that share its counter; numbering edits the group the document's own calls of that command
//! use for it.

use super::{Cx, Group, Hit, TypePlugin, call_at, edit_of, key_values, put, unbraced};
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::engine::tex::{Span, Tok};
use crate::project::config::TypePlugins;
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

pub(super) struct Theorem;

impl TypePlugin for Theorem {
    fn name(&self) -> &'static str {
        "theorem"
    }
    fn about(&self) -> &'static str {
        "manage theorem-like environments: kinds, counters, instances, rename, numbering"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Environment)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view", "manage"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.theorem.enabled
    }
    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let name = cx.at.item.typed.trim();
        let all = kinds(cx);
        all.iter().any(|k| k.env == name).then_some(())?;
        let mut v = render(cx, &all)?;
        v["sections"][0]["focus"] = json!(name);
        Some(v)
    }
    fn manage(&self, cx: &Cx) -> Option<Json> {
        render(cx, &kinds(cx))
    }
}

/// A theorem-like environment and the call that defined it.
struct Kind {
    env: String,
    head: String,
    span: Span,
    /// The call in the open file: its `\`, groups and end.
    call: Option<(usize, Vec<Group>, usize)>,
    own: bool,
    shared: Option<String>,
    within: Option<String>,
    body: BTreeSet<String>,
}

/// Every environment a document call defined together with a `\the⟨env⟩`, and every other
/// environment the same command defined (unnumbered ones).
fn kinds(cx: &Cx) -> Vec<Kind> {
    let a = cx.analysis;
    use crate::engine::builtins::LoadKind;
    // the document's own files: the main one and what it inputs or includes
    let doc_file = |s: Span| {
        s.file == a.main_file
            || a.files.get(usize::from(s.file)).is_some_and(|f| matches!(f.kind, LoadKind::Input | LoadKind::Include))
    };
    let name = |s| a.interner.name(s).to_string();
    let at_call = |d: &crate::analysis::facts::Definition, s: Span| d.span == s;
    let envs: Vec<&crate::analysis::facts::Definition> = (a.facts.defs.iter())
        .filter(|d| d.tag == "environment" && doc_file(d.span) && !name(d.name).starts_with("end"))
        .collect();
    let numbered = |d: &crate::analysis::facts::Definition| {
        let the = format!("the{}", name(d.name));
        a.facts.defs.iter().any(|t| at_call(t, d.span) && name(t.name) == the)
    };
    let heads: BTreeSet<&str> = envs.iter().filter(|d| numbered(d)).map(|d| a.interner.name(d.by)).collect();
    let counters: BTreeSet<String> = (a.facts.defs.iter())
        .filter(|d| d.tag == "counter")
        .filter_map(|d| name(d.name).strip_prefix("c@").map(String::from))
        .collect();
    let text = cx.doc.body();
    let mut out: Vec<Kind> = Vec::new();
    for d in envs.iter().filter(|d| heads.contains(a.interner.name(d.by))) {
        let env = name(d.name);
        if out.iter().any(|k| k.env == env) {
            continue;
        }
        let at_d = |n: &str| {
            a.facts
                .defs
                .iter()
                .filter(|t| at_call(t, d.span) || t.span.line == d.span.line && t.span.file == d.span.file)
                .find(|t| name(t.name) == n)
        };
        let the = at_d(&format!("the{env}"))
            .and_then(|t| t.mac.as_ref())
            .map(|m| crate::engine::tex::detokenize(&m.replacement_text, &a.interner));
        let shared = the
            .as_deref()
            .and_then(|b| b.trim().strip_prefix("\\the"))
            .map(str::trim)
            .filter(|c| counters.contains(*c))
            .map(String::from);
        let within = (a.facts.defs.iter())
            .filter(|t| t.span.file == d.span.file && t.span.line == d.span.line)
            .filter_map(|t| Some((name(t.name).strip_prefix("cl@")?.to_string(), t.mac.as_ref()?)))
            .find(|(_, m)| {
                crate::engine::tex::detokenize(&m.replacement_text, &a.interner).contains(&format!("{{{env}}}"))
            })
            .map(|(c, _)| c);
        let body = d.mac.as_ref().map_or_else(BTreeSet::new, |m| {
            m.replacement_text
                .iter()
                .filter_map(|t| if let Tok::Cs(s) = t.tok { Some(name(s)) } else { None })
                .collect()
        });
        // the call: the last `\` before the definition's argument on its line
        let call = super::byte_of(cx, d.span).and_then(|b| {
            let start = text[..b].rfind('\\')?;
            call_at(text, start).map(|(_, g, e)| (start, g, e))
        });
        out.push(Kind {
            env,
            head: name(d.by).trim_start_matches('\\').to_string(),
            span: d.span,
            call,
            own: at_d(&format!("c@{}", name(d.name))).is_some(),
            shared,
            within,
            body,
        });
    }
    // what sets a kind apart: the commands of its body no other kind's body has
    let common: BTreeSet<String> = out.iter().map(|k| k.body.clone()).reduce(|x, y| &x & &y).unwrap_or_default();
    for k in &mut out {
        k.body = k.body.difference(&common).cloned().collect();
    }
    out
}

/// Where a call names a counter: a group's index, and the key when the group is a key list.
type Slot = (usize, Option<String>);

/// The group of a kind's call that names `counter`.
fn slot_of(text: &str, call: &(usize, Vec<Group>, usize), counter: &str) -> Option<Slot> {
    call.1.iter().enumerate().find_map(|(i, g)| {
        if text[g.inner.clone()].trim() == counter && i > 0 {
            return Some((i, None));
        }
        let kv = key_values(text, g.inner.clone());
        kv.into_iter().find(|(_, r)| unbraced(&text[r.clone()]) == counter).map(|(key, _)| (i, Some(key)))
    })
}

fn render(cx: &Cx, all: &[Kind]) -> Option<Json> {
    if all.is_empty() {
        return None;
    }
    let a = cx.analysis;
    let text = cx.doc.body();
    let file = super::super::type_at::file_of(a, cx.path);
    let occ = &a.facts.occurrences;
    // where each command's calls write a shared counter and the one numbered within: learned from the document
    let mut slots: Vec<(String, &str, Slot)> = Vec::new();
    for k in all {
        let Some(call) = &k.call else { continue };
        for (what, c) in [("shared", &k.shared), ("within", &k.within)] {
            if let Some(s) = c.as_ref().and_then(|c| slot_of(text, call, c))
                && !slots.iter().any(|x| x.0 == k.head && x.1 == what)
            {
                slots.push((k.head.clone(), what, s));
            }
        }
    }
    let counters: Vec<String> = (a.facts.defs.iter())
        .filter(|d| d.tag == "counter")
        .filter_map(|d| a.interner.name(d.name).strip_prefix("c@").map(String::from))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut items = Vec::new();
    for k in all {
        // the instances: each \begin…\end of this file, its labels and their references
        let mut ends: Vec<(u32, OccKind)> = (occ.iter())
            .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment))
            .filter(|o| o.key == k.env && Some(o.span.file) == file)
            .map(|o| (o.span.line, o.kind))
            .collect();
        ends.sort_by_key(|e| (e.0, e.1 == OccKind::EndEnvironment));
        ends.dedup();
        let begins: Vec<u32> = ends.iter().filter(|e| e.1 == OccKind::BeginEnvironment).map(|e| e.0).collect();
        let mut rows = Vec::new();
        for b in &begins {
            let end = ends.iter().find(|e| e.1 == OccKind::EndEnvironment && e.0 >= *b).map_or(*b, |e| e.0);
            let labels: BTreeSet<&str> = (occ.iter())
                .filter(|o| o.kind == OccKind::Label && Some(o.span.file) == file && (*b..=end).contains(&o.span.line))
                .map(|o| o.key.as_str())
                .collect();
            let refs: BTreeSet<u32> = occ
                .iter()
                .filter(|o| o.kind == OccKind::Ref && labels.contains(o.key.as_str()))
                .map(|o| o.span.line)
                .collect();
            let refs: Vec<String> = refs.iter().map(u32::to_string).collect();
            rows.push(json!([format!("line {b}"), labels.into_iter().collect::<Vec<_>>().join(", "), refs.join(", ")]));
        }
        let numbering = match (&k.shared, &k.within, k.own) {
            (Some(s), _, _) => format!("shares {s}"),
            (_, Some(w), _) => format!("own counter within {w}"),
            (_, _, true) => "own counter".into(),
            _ => "unnumbered".into(),
        };
        let style: Vec<&str> = k.body.iter().map(|s| s.as_str()).filter(|s| *s != k.env).collect();
        let mut fields = Vec::new();
        let mut forms = Vec::new();
        if let Some(call) = &k.call {
            // rename: the definition's group, every \begin and \end, the kinds sharing its counter
            let mut edits = Vec::new();
            if let Some(g) = call.1.iter().find(|g| text[g.inner.clone()].trim() == k.env) {
                edits.push(put(cx.doc, g.inner.start, g.inner.end, "${value}"));
            }
            for o in occ
                .iter()
                .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment) && o.key == k.env)
            {
                let Some(at) = super::byte_of(cx, o.span) else { continue };
                let word = if o.kind == OccKind::BeginEnvironment { "begin" } else { "end" };
                let head = format!("\\{word}{{{}}}", k.env);
                if text[at..].starts_with(&head) {
                    let from = at + word.len() + 2;
                    if !edits
                        .iter()
                        .any(|e: &lsp_types::TextEdit| e.range == super::span(cx.doc, from, from + k.env.len()))
                    {
                        edits.push(put(cx.doc, from, from + k.env.len(), "${value}"));
                    }
                }
            }
            for other in
                all.iter().filter(|o| o.shared.as_deref() == Some(&k.env) || o.within.as_deref() == Some(&k.env))
            {
                if let Some((_, g, _)) = &other.call
                    && let Some(g) = g.iter().skip(1).find(|g| text[g.inner.clone()].trim() == k.env)
                {
                    edits.push(put(cx.doc, g.inner.start, g.inner.end, "${value}"));
                }
            }
            fields.push(json!({ "name": "environment", "value": k.env, "edit": edit_of(cx.path, edits) }));
            // the title: the first braced group after the name that is not a counter
            if let Some(g) = call
                .1
                .iter()
                .skip_while(|g| text[g.inner.clone()].trim() != k.env)
                .skip(1)
                .find(|g| g.open == '{' && !counters.iter().any(|c| *c == text[g.inner.clone()].trim()))
            {
                fields.push(super::field(cx, "title", &text[g.inner.clone()], g.inner.start, g.inner.end, "${value}"));
            }
            // numbering: the slot the document's own calls of this command use
            for (what, now) in [("shared", &k.shared), ("within", &k.within)] {
                let Some((_, _, (i, key))) = slots.iter().find(|s| s.0 == k.head && s.1 == what) else { continue };
                // a key can be emptied; a positional group stays
                let mut choices = if key.is_some() { vec![String::new()] } else { Vec::new() };
                choices.extend(counters.iter().filter(|c| **c != k.env).cloned());
                let current = now.clone().unwrap_or_default();
                let edit = numbering_edit(cx, call, *i, key.as_deref(), now.as_deref());
                let Some(edit) = edit else { continue };
                forms.push(json!({ "title": format!("Set {}", if what == "shared" { "shared counter" } else { "numbered within" }),
                                   "fields": [{ "name": "counter", "value": current, "choices": choices }], "edit": edit }));
            }
        }
        items.push(json!({
            "title": k.env,
            "detail": format!("\\{} · {numbering}{}", k.head, if style.is_empty() { String::new() } else { format!(" · {}", style.join(" ")) }),
            "badge": begins.is_empty().then_some("unused"),
            "search": format!("{} {} {numbering}", k.env, k.head),
            "sort": { "name": k.env, "line": format!("{:06}", k.span.line), "uses": format!("{:06}", begins.len()) },
            "fields": fields,
            "forms": forms,
            "table": { "columns": ["instance", "labels", "referenced on lines"], "rows": rows },
        }));
    }
    Some(json!({
        "title": "Theorem-like environments",
        "sections": [{ "kind": "records", "placeholder": "search kinds", "sorts": ["line", "name", "uses"], "items": items, "empty": "no theorem-like environment" }],
    }))
}

/// The edit that writes `${counter}` into a call's numbering slot: group `i`, or `key=` of the
/// key list in group `i`; an empty counter removes it.
fn numbering_edit(
    cx: &Cx,
    call: &(usize, Vec<Group>, usize),
    i: usize,
    key: Option<&str>,
    now: Option<&str>,
) -> Option<Json> {
    let text = cx.doc.body();
    let edits = match key {
        None => match (now, call.1.get(i)) {
            (Some(_), Some(g)) if g.open == '[' => {
                vec![put(cx.doc, g.inner.start - 1, g.inner.end + 1, "[${counter}]")]
            }
            // not given: insert the group where the slot stands, after the group before it
            _ => {
                let at = if i == 0 { call.0 } else { call.1.get(i - 1).map(|g| g.inner.end + 1)? };
                let at = at.min(call.2);
                vec![put(cx.doc, at, at, "[${counter}]")]
            }
        },
        Some(key) => {
            let g = call.1.get(i)?;
            match key_values(text, g.inner.clone()).into_iter().find(|(k, _)| k == key) {
                Some((_, r)) => vec![put(cx.doc, r.start, r.end, "${counter}")],
                None => vec![put(cx.doc, g.inner.end, g.inner.end, format!(",{key}=${{counter}}"))],
            }
        }
    };
    Some(edit_of(cx.path, edits))
}
