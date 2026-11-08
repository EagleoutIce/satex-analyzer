//! Theorem-like environments: the environments a document call defined together with their
//! number (`\the⟨env⟩`), whatever the package (`\newtheorem`, `\declaretheorem`,
//! `\newtcbtheorem`). Kinds with their counter, the counter they share or the one they are
//! numbered within, and what sets them apart (a style); every instance with its labels and
//! references. Renaming a kind rewrites its definition, every `\begin`/`\end` and the kinds
//! that share its counter; numbering edits the group the document's own calls of that command
//! use for it.

use super::{Cx, Group, Hit, TypePlugin, call_at, edit_of, key_values, unbraced};
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::engine::tex::{Span, Tok};
use crate::lsp::snapshot::{self, Node};
use crate::project::config::TypePlugins;
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

pub(super) struct Theorem;

impl TypePlugin for Theorem {
    fn name(&self) -> &'static str {
        "theorem"
    }
    fn category(&self) -> &'static str {
        "widget"
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
            // a positional slot by the braced groups before it: the optional ones before it vary
            if let Some((i, key)) = c.as_ref().and_then(|c| slot_of(text, call, c))
                && !slots.iter().any(|x| x.0 == k.head && x.1 == what)
            {
                let n = if key.is_some() { i } else { call.1[..i].iter().filter(|g| g.open == '{').count() };
                slots.push((k.head.clone(), what, (n, key)));
            }
        }
    }
    // the counters a document names: a package's internal ones (`pgf@counta`) not
    let counters: Vec<String> = (a.facts.defs.iter())
        .filter(|d| d.tag == "counter")
        .filter_map(|d| a.interner.name(d.name).strip_prefix("c@").map(String::from))
        .filter(|c| !c.contains('@'))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let envs: Vec<&str> = all.iter().map(|k| k.env.as_str()).collect();
    let file_name = |s: Span| a.files.get(usize::from(s.file)).map_or(cx.path, |f| f.path.as_str());
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
        let mut places: Vec<String> = Vec::new();
        let mut found: Vec<String> = Vec::new();
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
            places.push(format!(
                "`{}:{b}`{}",
                cx.path,
                if labels.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", labels.iter().copied().collect::<Vec<_>>().join(", "))
                }
            ));
            found.extend(labels.iter().map(|l| l.to_string()));
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
            let mut places = Vec::new();
            if let Some(g) = call.1.iter().find(|g| text[g.inner.clone()].trim() == k.env) {
                places.push((g.inner.start, g.inner.end));
            }
            for o in occ
                .iter()
                .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment) && o.key == k.env)
            {
                let Some(at) = super::byte_of(cx, o.span) else { continue };
                let word = if o.kind == OccKind::BeginEnvironment { "begin" } else { "end" };
                if text[at..].starts_with(&format!("\\{word}{{{}}}", k.env)) {
                    let from = at + word.len() + 2;
                    places.push((from, from + k.env.len()));
                }
            }
            for other in
                all.iter().filter(|o| o.shared.as_deref() == Some(&k.env) || o.within.as_deref() == Some(&k.env))
            {
                if let Some((_, g, _)) = &other.call
                    && let Some(g) = g.iter().skip(1).find(|g| text[g.inner.clone()].trim() == k.env)
                {
                    places.push((g.inner.start, g.inner.end));
                }
            }
            places.sort_unstable();
            places.dedup();
            let taken: Vec<&str> = envs.iter().copied().filter(|e| *e != k.env).collect();
            let mut f = snapshot::field(cx.doc, "environment", &k.env, &places);
            f["check"] = json!(["required", "key", "unique"]);
            f["taken"] = json!(taken);
            fields.push(f);
            // the title: the first braced group after the name that is not a counter
            if let Some(g) = call
                .1
                .iter()
                .skip_while(|g| text[g.inner.clone()].trim() != k.env)
                .skip(1)
                .find(|g| g.open == '{' && !counters.iter().any(|c| *c == text[g.inner.clone()].trim()))
            {
                let mut f = snapshot::field(cx.doc, "title", &text[g.inner.clone()], &[(g.inner.start, g.inner.end)]);
                f["check"] = json!(["required", "tex"]);
                fields.push(f);
            }
            // numbering: one choice of own, within a counter, shared with an earlier kind; written
            // in the slots the document's own calls of this command use, the other one cleared
            let slot = |what: &str| slots.iter().find(|s| s.0 == k.head && s.1 == what).map(|s| &s.2);
            let now = |c: &Option<String>| c.as_ref().and_then(|c| slot_of(text, call, c));
            // the call with the groups a key slot names read as key lists
            let keyed: Vec<usize> = [now(&k.shared), now(&k.within), slot("within").cloned(), slot("shared").cloned()]
                .into_iter()
                .flatten()
                .filter_map(|(i, key)| key.map(|_| i))
                .collect();
            // unread, nothing prints and no choice is offered
            let base = snapshot::read(cx.doc, call.0, &keyed, false).unwrap_or_default();
            let print =
                |n: Node| snapshot::printed(cx.analysis, cx.doc, &[n]).map(|(e, _)| snapshot::lsp_edits(cx.doc, e));
            let mut choices: Vec<(String, Vec<lsp_types::TextEdit>)> = Vec::new();
            let mut own = base.clone();
            clear(&mut own, now(&k.shared));
            clear(&mut own, now(&k.within));
            choices.extend(print(own).ok().map(|e| ("own counter".to_string(), e)));
            let set = |what: &str, was: &Option<String>, other: &Option<String>, c: &str| {
                let mut n = base.clone();
                set_slot(&mut n, slot(what)?, now(was), c)?;
                clear(&mut n, now(other));
                print(n).ok()
            };
            for c in counters.iter().filter(|c| !envs.contains(&c.as_str())) {
                choices.extend(set("within", &k.within, &k.shared, c).map(|e| (format!("within {c}"), e)));
            }
            // a kind declared before this one
            for o in all.iter().filter(|o| o.env != k.env && o.span.line < k.span.line && o.span.file == k.span.file) {
                choices.extend(set("shared", &k.shared, &k.within, &o.env).map(|e| (format!("shares {}", o.env), e)));
            }
            let now = match (&k.shared, &k.within) {
                (Some(s), _) => format!("shares {s}"),
                (_, Some(w)) => format!("within {w}"),
                _ => "own counter".into(),
            };
            if choices.len() > 1 && (k.own || k.shared.is_some()) {
                let labels: Vec<&str> = choices.iter().map(|c| c.0.as_str()).collect();
                let edits: serde_json::Map<String, Json> =
                    choices.iter().map(|(l, e)| (l.clone(), edit_of(cx.path, e.clone()))).collect();
                forms.push(json!({ "title": "Set numbering", "fields": [{ "name": "numbering", "value": now, "choices": labels }], "by": "numbering", "edits": edits }));
            }
        }
        items.push(json!({
            "title": k.env,
            "detail": format!("\\{} · {numbering}{}", k.head, if style.is_empty() { String::new() } else { format!(" · {}", style.join(" ")) }),
            "badge": begins.is_empty().then_some("unused"),
            "search": format!("{} {} {numbering} {} {}", k.env, k.head, found.join(" "), places.join(" ")),
            "uses": if places.is_empty() { format!("Defined at `{}:{}`. No instance.", file_name(k.span), k.span.line) } else { format!("Defined at `{}:{}`. Instances: {}", file_name(k.span), k.span.line, places.join(" · ")) },
            "sites": super::sites(begins.iter().map(|b| (cx.path, *b))),
            "facets": { "numbering": numbering },
            "sort": { "name": k.env, "line": format!("{:06}", k.span.line), "uses": format!("{:06}", begins.len()) },
            "fields": fields,
            "forms": forms,
            "table": { "columns": ["instance", "labels", "referenced on lines"], "rows": rows },
        }));
    }
    Some(json!({
        "title": "Theorem-like environments",
        "sections": [{ "kind": "records", "placeholder": "search kinds, labels, files", "sorts": ["line", "name", "uses"], "items": items,
                       "empty": "no theorem-like environment", "add": add(cx, all, &slots, &counters) }],
    }))
}

/// A new kind, written like the document's first call of each command with positional
/// numbering slots, after the last kind: its name, title and numbering.
fn add(cx: &Cx, all: &[Kind], slots: &[(String, &str, Slot)], counters: &[String]) -> Option<Json> {
    let text = cx.doc.body();
    let last = all.iter().filter_map(|k| k.call.as_ref()).map(|c| super::lines_of(text, c.0, c.2).1).max()?;
    let envs: Vec<String> = (cx.analysis.facts.defs.iter())
        .filter(|d| d.tag == "environment")
        .map(|d| cx.analysis.interner.name(d.name).to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut kinds = Vec::new();
    let mut heads: Vec<&str> = Vec::new();
    for k in all {
        let Some(call) = &k.call else { continue };
        if heads.contains(&k.head.as_str()) {
            continue;
        }
        heads.push(&k.head);
        let Some(mut base) = snapshot::read(cx.doc, call.0, &[], false) else { continue };
        let at = Some(super::span(cx.doc, last, last).start);
        base = Node { range: None, text: String::new(), at, line: true, ..base };
        // the arguments as written, a numbering one left out, the name and title the form's
        let numbering: Vec<usize> = [&k.shared, &k.within]
            .iter()
            .filter_map(|c| slot_of(text, call, c.as_ref()?))
            .filter(|s| s.1.is_none())
            .map(|s| s.0)
            .collect();
        let title = call.1.iter().skip(1).position(|g| g.open == '{').map(|i| i + 1);
        let mut roles: Vec<Option<&str>> = Vec::new();
        for (i, a) in std::mem::take(&mut base.args).into_iter().enumerate() {
            let role = match i {
                0 if a.text.trim() == k.env => Some("env"),
                _ if Some(i) == title => Some("title"),
                _ if numbering.contains(&i) => continue,
                _ => None,
            };
            let text = if role.is_some() { String::new() } else { a.text };
            base.args.push(snapshot::Arg { inner: None, text, ..a });
            roles.push(role);
        }
        let form = |n: &Node, roles: &[Option<&str>]| {
            let fills: Vec<snapshot::Fill> =
                roles.iter().enumerate().filter_map(|(i, r)| Some(((*r)?, 0, Some(i), None))).collect();
            snapshot::form(std::slice::from_ref(n), &fills)
        };
        let positional =
            |what: &str| slots.iter().find(|s| s.0 == k.head && s.1 == what && s.2.1.is_none()).map(|s| s.2.0);
        let mut choices: Vec<(String, Json)> = vec![("own counter".into(), form(&base, &roles))];
        let mut with = |what: &str, label: String, c: &str| {
            let (mut n, mut r) = (base.clone(), roles.clone());
            let Some(j) = positional(what).and_then(|i| set_slot(&mut n, &(i, None), None, c)) else { return };
            r.insert(j, None);
            choices.push((label, form(&n, &r)));
        };
        for c in counters.iter().filter(|c| !all.iter().any(|o| o.env == **c)) {
            with("within", format!("within {c}"), c);
        }
        for o in all {
            with("shared", format!("shares {}", o.env), &o.env);
        }
        let labels: Vec<&str> = choices.iter().map(|c| c.0.as_str()).collect();
        let prints: serde_json::Map<String, Json> = choices.iter().cloned().collect();
        kinds.push(json!({ "name": format!("\\{}", k.head), "by": "numbering", "prints": prints, "fields": [
            { "name": "env", "label": "environment", "value": "", "placeholder": "proposition", "check": ["required", "key", "unique"], "taken": envs },
            { "name": "title", "value": "", "placeholder": "Proposition", "check": ["required", "tex"] },
            { "name": "numbering", "value": "own counter", "choices": labels },
        ] }));
    }
    (!kinds.is_empty()).then(|| json!({ "title": "New environment", "kinds": kinds }))
}

/// `n`'s numbering slot `slot` set to counter `c`: the `[…]` it writes (`was`), else a new `[c]`
/// after its `slot.0`th braced argument, or `key=c` in the key list of argument `slot.0`. The
/// index of the argument written.
fn set_slot(n: &mut Node, slot: &Slot, was: Option<Slot>, c: &str) -> Option<usize> {
    let Some(key) = &slot.1 else {
        if let Some((j, _)) = was.filter(|s| s.1.is_none()) {
            n.args.get_mut(j)?.text = c.into();
            return Some(j);
        }
        let mut braced = n.args.iter().enumerate().filter(|a| a.1.open == '{').map(|a| a.0 + 1);
        let at = if slot.0 == 0 { 0 } else { braced.nth(slot.0 - 1)? };
        n.args.insert(at, snapshot::Arg::text('[', c));
        return Some(at);
    };
    let keys = n.args.get_mut(slot.0)?.keys.as_mut()?;
    match keys.iter_mut().find(|k| k.key == *key) {
        Some(k) => k.value = Some(c.into()),
        None => keys.push(snapshot::Key { key: key.clone(), value: Some(c.into()) }),
    }
    Some(slot.0)
}

/// The slot `s` of `n` emptied: its `[…]` dropped, its key removed.
fn clear(n: &mut Node, s: Option<Slot>) {
    let Some((i, key)) = s else { return };
    let Some(a) = n.args.get_mut(i) else { return };
    match (key, a.keys.as_mut()) {
        (None, _) => a.drop = true,
        (Some(key), Some(keys)) => keys.retain(|k| k.key != key),
        _ => {}
    }
}
