//! Declared keys (glossary entries, acronyms, abbreviations): a manager of every key a document
//! call declared in the run, with the fields its call writes and the values the run stored, its
//! uses, and edits that write back the call: a field, a new entry made like the document's own,
//! deleting an unused one or replacing its uses first. The commands are the run's, not a list.

use super::{Cx, Hit, TypePlugin, call_at, edit_of, field, key_values, lines_of, put, unbraced};
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::project::config::TypePlugins;
use serde_json::{Value as Json, json};

pub(super) struct Glossary;

impl TypePlugin for Glossary {
    fn name(&self) -> &'static str {
        "glossary"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "manage declared keys (glossary entries, acronyms): search, edit, add, delete, uses"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Declared)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view", "manage"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.glossary.enabled
    }
    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let mut v = self.manage(cx)?;
        v["sections"][0]["focus"] = json!(cx.at.item.typed.trim());
        Some(v)
    }
    fn manage(&self, cx: &Cx) -> Option<Json> {
        let all = entries(cx);
        if all.is_empty() {
            return None;
        }
        let keys: Vec<&str> = all.iter().map(|e| e.key.as_str()).collect();
        let items: Vec<Json> = all.iter().map(|e| record(cx, e, &all, &keys)).collect();
        Some(json!({
            "title": "Glossary entries",
            "sections": [
                { "kind": "records", "placeholder": "search keys, names, descriptions", "sorts": ["key", "name", "uses", "line"],
                  "items": items, "add": add(cx, &all) },
            ],
        }))
    }
}

/// A declared key: the call that declared it in the open file, its groups, its uses there.
struct Entry {
    key: String,
    head: String,
    at: usize,
    end: usize,
    groups: Vec<super::Group>,
    /// Where it stands when not in the open file: file and line.
    elsewhere: Option<(String, u32)>,
    span: crate::engine::tex::Span,
    /// Each use: line, the head, and the bytes of the key in the open file.
    uses: Vec<(u32, String, Option<std::ops::Range<usize>>)>,
}

/// Every key a document call declared, but the databases a bibliography reads.
fn entries(cx: &Cx) -> Vec<Entry> {
    let occ = &cx.analysis.facts.occurrences;
    let text = cx.doc.body();
    // a database a bibliography reads and a color are keys of their own kinds, below a declared key
    let databases: Vec<&str> = (occ.iter().filter(|o| o.kind == OccKind::Bibliography).map(|o| o.key.as_str()))
        .chain(cx.analysis.facts.colors.iter().map(|c| c.name.as_str()))
        .collect();
    let mut out: Vec<Entry> = Vec::new();
    for o in occ.iter().filter(|o| o.kind == OccKind::Key && !databases.contains(&o.key.as_str())) {
        if out.iter().any(|e| e.key == o.key) {
            continue;
        }
        let head = o.detail.clone().unwrap_or_default();
        let here = super::byte_of(cx, o.span).and_then(|at| Some((at, call_at(text, at)?)));
        let file = &cx.analysis.files[usize::from(o.span.file)].path;
        let elsewhere = here.is_none().then(|| (file.clone(), o.span.line));
        let (at, (_, groups, end)) = here.unwrap_or((0, (0, Vec::new(), 0)));
        out.push(Entry { key: o.key.clone(), head, at, end, groups, elsewhere, span: o.span, uses: Vec::new() });
    }
    for o in occ.iter().filter(|o| o.kind == OccKind::KeyUse) {
        let Some(e) = out.iter_mut().find(|e| e.key == o.key) else { continue };
        let range = super::byte_of(cx, o.span).and_then(|at| key_in(text, at, &o.key));
        let head = o.detail.clone().unwrap_or_default();
        if !e.uses.iter().any(|u| u.0 == o.span.line && u.2 == range) {
            e.uses.push((o.span.line, head, range));
        }
    }
    out.sort_by_key(|e| e.span.line);
    out
}

/// The bytes of the group of the call at `at` that holds just `key`.
fn key_in(text: &str, at: usize, key: &str) -> Option<std::ops::Range<usize>> {
    let (_, groups, _) = call_at(text, at)?;
    groups.into_iter().find(|g| text[g.inner.clone()].trim() == key).map(|g| g.inner)
}

/// The fields the call writes: a key list's keys, else each group by the field the run stored
/// its text in (`\glo@⟨key⟩@⟨field⟩`), else its number. Of several fields holding the text,
/// the one the other entries leave empty most often: the field only this command sets.
fn fields(cx: &Cx, e: &Entry, all: &[Entry]) -> Vec<(String, std::ops::Range<usize>)> {
    let text = cx.doc.body();
    let others: Vec<Vec<(String, String)>> = all.iter().filter(|o| o.key != e.key).map(|o| stored(cx, o)).collect();
    let empty = |f: &str| others.iter().filter(|s| s.iter().any(|(n, v)| n == f && v.trim().is_empty())).count();
    let stored = stored(cx, e);
    let mut out = Vec::new();
    for (n, g) in e.groups.iter().enumerate() {
        let inner = &text[g.inner.clone()];
        if inner.trim() == e.key {
            continue;
        }
        let kv = key_values(text, g.inner.clone());
        if !kv.is_empty() && g.open == '{' {
            out.extend(kv);
            continue;
        }
        if g.open == '[' {
            continue;
        }
        let name = (stored.iter().rev())
            .filter(|(_, v)| v.trim() == unbraced(inner).trim())
            .max_by_key(|(f, _)| empty(f))
            .map(|(f, _)| f.clone());
        out.push((name.unwrap_or_else(|| format!("#{}", n + 1)), g.inner.start - 1..g.inner.end + 1));
    }
    out
}

/// The macros the declaring call defined with the key in their name, by the rest of the name,
/// and their replacement text: what the run stored of the entry.
fn stored(cx: &Cx, e: &Entry) -> Vec<(String, String)> {
    let a = cx.analysis;
    let mut out: Vec<(String, String)> = Vec::new();
    // the call's lines: its definitions stand at its arguments
    let last = if e.elsewhere.is_none() { cx.doc.pos(e.end).line } else { e.span.line };
    let at_call = |s: crate::engine::tex::Span| s.file == e.span.file && (e.span.line..=last).contains(&s.line);
    for d in a.facts.defs.iter().filter(|d| at_call(d.span)) {
        let name = a.interner.name(d.name);
        let Some(i) = name.find(e.key.as_str()) else { continue };
        let what = name[i + e.key.len()..].trim_matches('@');
        let Some(mac) = &d.mac else { continue };
        if what.is_empty() || what.contains(['@', ' ']) || out.iter().any(|(f, _)| f == what) {
            continue;
        }
        out.push((what.to_string(), crate::engine::tex::detokenize(&mac.replacement_text, &a.interner)));
    }
    out
}

fn record(cx: &Cx, e: &Entry, all: &[Entry], keys: &[&str]) -> Json {
    let text = cx.doc.body();
    let fs = if e.elsewhere.is_none() { fields(cx, e, all) } else { Vec::new() };
    let value = |name: &str| fs.iter().find(|(f, _)| f == name).map(|(_, r)| unbraced(&text[r.clone()]).to_string());
    let stored = stored(cx, e);
    let name = value("name").or_else(|| stored.iter().find(|s| s.0 == "name").map(|s| s.1.clone()));
    let search: String = std::iter::once(e.key.clone())
        .chain(fs.iter().map(|(_, r)| text[r.clone()].to_string()))
        .chain(stored.iter().map(|s| s.1.clone()))
        .collect::<Vec<_>>()
        .join(" ");
    let fields: Vec<Json> =
        fs.iter().map(|(f, r)| field(cx, f, unbraced(&text[r.clone()]), r.start, r.end, "{${value}}")).collect();
    let mut rows: Vec<Json> =
        e.uses.iter().map(|(l, h, _)| json!(["use", format!("line {l}"), format!("\\{h}")])).collect();
    rows.extend(stored.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(f, v)| json!(["stored", f, v])));
    let place = e.elsewhere.as_ref().map_or(format!("line {}", e.span.line), |(f, l)| format!("{f}:{l}"));
    let mut actions = Vec::new();
    let mut forms = Vec::new();
    if e.elsewhere.is_none() {
        let (from, to) = lines_of(text, e.at, e.end);
        let delete = edit_of(cx.path, vec![put(cx.doc, from, to, "")]);
        let reason = (!e.uses.is_empty())
            .then(|| format!("used {}: replace the uses first", crate::cli::render::plural(e.uses.len(), "time")));
        actions.push(json!({ "title": format!("Delete {}", e.key), "edit": delete, "disabled": reason }));
        let others: Vec<&str> = keys.iter().copied().filter(|k| *k != e.key).collect();
        let movable = e.uses.iter().all(|u| u.2.is_some());
        if !e.uses.is_empty() && !others.is_empty() && movable {
            let mut edits: Vec<_> =
                e.uses.iter().filter_map(|u| u.2.clone()).map(|r| put(cx.doc, r.start, r.end, "${key}")).collect();
            edits.push(put(cx.doc, from, to, ""));
            forms.push(json!({ "title": "Replace uses and delete", "fields": [{ "name": "key", "choices": others }],
                               "edit": edit_of(cx.path, edits) }));
        }
    }
    json!({
        "title": e.key,
        "detail": format!("{} · \\{} · {place}", name.unwrap_or_default(), e.head),
        "badge": e.uses.is_empty().then_some("unused"),
        "search": search,
        "sort": { "key": e.key, "name": value("name").unwrap_or_default(), "uses": format!("{:06}", e.uses.len()), "line": format!("{:06}", e.span.line) },
        "fields": fields,
        "forms": forms,
        "table": { "columns": ["", "where", "what"], "rows": rows },
        "actions": actions,
    })
}

/// A new entry made like the first one of each declaring command in the open file, written
/// after the last declaration.
fn add(cx: &Cx, all: &[Entry]) -> Option<Json> {
    let text = cx.doc.body();
    let last = all.iter().filter(|e| e.elsewhere.is_none()).map(|e| lines_of(text, e.at, e.end).1).max()?;
    let mut kinds = Vec::new();
    let mut heads: Vec<&str> = Vec::new();
    for e in all.iter().filter(|e| e.elsewhere.is_none()) {
        if heads.contains(&e.head.as_str()) {
            continue;
        }
        heads.push(&e.head);
        let fs = fields(cx, e, all);
        // the call's text with the key and each field's value made a template
        let mut cuts: Vec<(std::ops::Range<usize>, String)> =
            fs.iter().map(|(f, r)| (r.clone(), format!("{{${{{}}}}}", slot(f)))).collect();
        if let Some(g) = e.groups.iter().find(|g| text[g.inner.clone()].trim() == e.key) {
            cuts.push((g.inner.clone(), "${key}".into()));
        }
        cuts.sort_by_key(|c| c.0.start);
        let mut out = String::new();
        let mut at = e.at;
        for (r, t) in cuts {
            out.push_str(&text[at..r.start]);
            out.push_str(&t);
            at = r.end;
        }
        out.push_str(&text[at..e.end]);
        let mut form = vec![json!({ "name": "key", "value": "", "placeholder": "key" })];
        form.extend(fs.iter().map(|(f, _)| json!({ "name": slot(f), "value": "", "placeholder": f })));
        let line = if text[..last].ends_with('\n') { format!("{out}\n") } else { format!("\n{out}") };
        kinds.push(json!({ "name": format!("\\{}", e.head), "fields": form, "edit": edit_of(cx.path, vec![put(cx.doc, last, last, line)]) }));
    }
    Some(json!({ "title": "Add entry", "kinds": kinds }))
}

/// A field's name as a template variable: `#2` is `n2`.
fn slot(field: &str) -> String {
    field.replace('#', "n")
}
