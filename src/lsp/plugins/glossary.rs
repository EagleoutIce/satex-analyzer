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
        // the keys the declaring command's family takes: from the run, by the keys the document wrote
        let text = cx.doc.body();
        let written: Vec<String> = (all.iter().filter_map(|e| list_of(text, e)))
            .flat_map(|l| key_values(text, l).into_iter().map(|kv| kv.0))
            .fold(Vec::new(), |mut v, k| {
                if !v.contains(&k) {
                    v.push(k);
                }
                v
            });
        let family = super::family_keys(cx.analysis, &written);
        let items: Vec<Json> = all.iter().map(|e| record(cx, e, &all, &keys, &family)).collect();
        Some(json!({
            "title": "Edit glossary",
            "sections": [
                { "kind": "records", "placeholder": "Search keys, names and descriptions", "sorts": ["key", "name", "uses", "line"],
                  "items": items, "add": add(cx, &all, &family) },
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
    /// Each use: line, its file, and the bytes of the key in the open file.
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
        let file = cx.analysis.files[usize::from(o.span.file)].path.clone();
        if !e.uses.iter().any(|u| u.0 == o.span.line && u.1 == file && u.2 == range) {
            e.uses.push((o.span.line, file, range));
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

/// The entry's key list: its first braced group of `key=value` items.
fn list_of(text: &str, e: &Entry) -> Option<std::ops::Range<usize>> {
    e.groups.iter().find(|g| g.open == '{' && !key_values(text, g.inner.clone()).is_empty()).map(|g| g.inner.clone())
}

/// The keys every entry of `e`'s command in the open file writes: what that command needs.
fn common(text: &str, e: &Entry, all: &[Entry]) -> Vec<String> {
    let lists = all.iter().filter(|o| o.head == e.head && o.elsewhere.is_none()).filter_map(|o| list_of(text, o));
    let mut sets = lists.map(|l| key_values(text, l).into_iter().map(|kv| kv.0).collect::<Vec<_>>());
    let first = sets.next().unwrap_or_default();
    sets.fold(first, |acc, s| acc.into_iter().filter(|k| s.contains(k)).collect())
}

/// How the document separates key list items: `, ` or `,`.
fn sep(text: &str, list: &std::ops::Range<usize>) -> &'static str {
    if text[list.clone()].contains(", ") { ", " } else { "," }
}

fn record(cx: &Cx, e: &Entry, all: &[Entry], keys: &[&str], family: &[String]) -> Json {
    let text = cx.doc.body();
    let fs = if e.elsewhere.is_none() { fields(cx, e, all) } else { Vec::new() };
    let value = |name: &str| fs.iter().find(|(f, _)| f == name).map(|(_, r)| unbraced(&text[r.clone()]).to_string());
    let stored = stored(cx, e);
    let search: String = std::iter::once(e.key.clone())
        .chain(fs.iter().map(|(_, r)| text[r.clone()].to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    // a key only some entries write goes when emptied; a new one joins the list
    let list = list_of(text, e).filter(|_| e.elsewhere.is_none());
    let needed = common(text, e, all);
    let mut fields: Vec<Json> = Vec::new();
    for (f, r) in &fs {
        let mut j = field(cx, f, unbraced(&text[r.clone()]), r.start, r.end, "{${value}}");
        if let Some(l) = list.clone().filter(|l| l.contains(&r.start) && !needed.contains(f)) {
            let (from, to) = super::item_cut(text, l, r.clone());
            j["remove"] = edit_of(cx.path, vec![put(cx.doc, from, to, "")]);
        } else if list.as_ref().is_some_and(|l| l.contains(&r.start)) {
            j["check"] = json!(["required", "tex"]);
        }
        fields.push(j);
    }
    let more = list.as_ref().map(|l| {
        let names: Vec<&String> = family.iter().filter(|k| !fs.iter().any(|(f, _)| f == *k)).collect();
        let at = l.end - (text[l.clone()].len() - text[l.clone()].trim_end().len());
        json!({ "title": "Add field", "names": names,
                "edit": edit_of(cx.path, vec![put(cx.doc, at, at, format!("{}${{name}}={{${{value}}}}", sep(text, l)))]) })
    });
    // what the call wrote comes first, what the run stored only when the call did not write it
    let summary = |names: &[&str]| {
        let stored = |n: &&str| stored.iter().find(|s| s.0 == *n).map(|s| s.1.clone());
        let v = names.iter().find_map(|n| value(n)).or_else(|| names.iter().find_map(stored));
        v.map(|v| v.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|v| !v.is_empty())
    };
    // what a reader of the document knows it by: name, short description, and a type other than the default
    let kind = summary(&["type"]).filter(|t| t != "main");
    let detail: Vec<String> =
        [summary(&["name", "short"]), summary(&["description", "desc", "long"]), kind].into_iter().flatten().collect();
    // each use as a place a click opens
    let uses = match e.uses.is_empty() {
        true => "Not used.".to_string(),
        false => {
            let mut places: Vec<String> = e.uses.iter().map(|(l, f, _)| format!("`{f}:{l}`")).collect();
            places.dedup();
            format!("Used at {}", places.join(" · "))
        }
    };
    let mut actions = Vec::new();
    let mut forms = Vec::new();
    if e.elsewhere.is_none() {
        let (from, to) = lines_of(text, e.at, e.end);
        let delete = edit_of(cx.path, vec![put(cx.doc, from, to, "")]);
        let reason = (!e.uses.is_empty())
            .then(|| format!("Used {}. Replace its uses first", crate::cli::render::plural(e.uses.len(), "time")));
        actions
            .push(json!({ "title": format!("Delete {}", e.key), "edit": delete, "disabled": reason, "danger": true }));
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
        "detail": detail.join(" · "),
        "badge": e.uses.is_empty().then_some("unused"),
        "search": format!("{search} {} {uses}", detail.join(" ")),
        "sort": { "key": e.key, "name": value("name").unwrap_or_default(), "uses": format!("{:06}", e.uses.len()), "line": format!("{:06}", e.span.line) },
        "fields": fields,
        "more": more,
        "forms": forms,
        "uses": format!("Declared at `{}:{}`. {uses}", e.elsewhere.as_ref().map_or(cx.path, |(f, _)| f.as_str()), e.span.line),
        "actions": actions,
    })
}

/// A new entry made like the first one of each declaring command in the open file, written
/// after the last declaration.
fn add(cx: &Cx, all: &[Entry], family: &[String]) -> Option<Json> {
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
        let list = list_of(text, e);
        let needed = common(text, e, all);
        let in_list = |r: &std::ops::Range<usize>| list.as_ref().is_some_and(|l| l.contains(&r.start));
        // the call's text with the key and each field's value made a template; its key list
        // written anew: the keys every entry writes, then each other key of the family when filled
        let mut cuts: Vec<(std::ops::Range<usize>, String)> =
            fs.iter().filter(|(_, r)| !in_list(r)).map(|(f, r)| (r.clone(), format!("{{${{{}}}}}", slot(f)))).collect();
        let mut optional: Vec<String> = Vec::new();
        if let Some(l) = &list {
            let sep = sep(text, l);
            let inner: Vec<String> = needed.iter().map(|k| format!("{k}={{${{{k}}}}}")).collect();
            optional = (fs.iter().map(|(f, _)| f.clone()).filter(|f| !needed.contains(f)))
                .chain(family.iter().filter(|k| !fs.iter().any(|(f, _)| f == *k)).cloned())
                .collect();
            let first = inner.is_empty();
            let tail: String = optional
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    let lead = if first && i == 0 { "" } else { sep };
                    format!("${{?{k}}}{lead}{k}={{${{{k}}}}}${{/{k}}}")
                })
                .collect();
            cuts.push((l.clone(), format!("{}{tail}", inner.join(sep))));
        }
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
        let taken: Vec<&str> = all.iter().map(|e| e.key.as_str()).collect();
        let mut form = vec![
            json!({ "name": "key", "value": "", "placeholder": "key", "check": ["required", "key", "unique"], "taken": taken }),
        ];
        // what the command needs first, the family's other keys folded below
        let shown = fs.iter().map(|(f, _)| f.as_str()).filter(|f| !optional.iter().any(|o| o == f));
        form.extend(
            shown.map(|f| json!({ "name": slot(f), "value": "", "placeholder": f, "check": ["required", "tex"] })),
        );
        form.extend(
            optional
                .iter()
                .map(|f| json!({ "name": f, "value": "", "placeholder": f, "check": ["tex"], "more": true })),
        );
        let line = if text[..last].ends_with('\n') { format!("{out}\n") } else { format!("\n{out}") };
        kinds.push(json!({ "name": format!("\\{}", e.head), "fields": form, "edit": edit_of(cx.path, vec![put(cx.doc, last, last, line)]) }));
    }
    Some(json!({ "title": "Add entry", "kinds": kinds }))
}

/// A field's name as a template variable: `#2` is `n2`.
fn slot(field: &str) -> String {
    field.replace('#', "n")
}
