//! A control sequence's definition history: every definition event the run recorded for the name
//! (`\def`, `\let`, `\renewcommand`, `\appto`, `\g@addto@macro`, `\patchcmd`: whatever changed its
//! meaning), newest first along the path that ends with the meaning in force, each with its place,
//! its body and what it changed. A `\let` copy links to the definition it copied. Definitions on other
//! paths (an undecided conditional's other arm, a package loaded on one path) follow, each with the
//! conditions that make it the one in force. The calls in the final body carry their own histories.

use super::{Cx, Hit, TypePlugin, open_link};
use crate::analysis::facts::Definition;
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::engine::tex::{Meaning, Sym, Token, detokenize};
use crate::project::config::TypePlugins;
use lsp_types::{CodeAction, Command};
use serde_json::{Value as Json, json};
use std::rc::Rc;

pub(super) struct History;

impl TypePlugin for History {
    fn name(&self) -> &'static str {
        "history"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "each redefinition and augment of a command, newest first, with \\let copies and definitions on other paths"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Cs]
    }
    fn features(&self) -> &'static [&'static str] {
        &["actions", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.history.enabled
    }
    fn entry(&self, cx: &Cx, hit: &Hit) -> Option<String> {
        name(cx, hit).map(|_| open_link(cx, TITLE, "history"))
    }
    fn actions(&self, cx: &Cx, hit: &Hit) -> Vec<CodeAction> {
        let Some(_) = name(cx, hit) else { return Vec::new() };
        let args =
            vec![json!(cx.uri().as_str()), json!(super::super::handlers::to_lsp(cx.doc, cx.pos)), json!("history")];
        let command = Command { title: TITLE.into(), command: "satex.openView".into(), arguments: Some(args) };
        vec![CodeAction { title: TITLE.into(), command: Some(command), ..Default::default() }]
    }
    fn view(&self, cx: &Cx, hit: &Hit) -> Option<Json> {
        let sym = name(cx, hit)?;
        let a = cx.analysis;
        let top = history(a, sym);
        // the names its final body calls and the names its copies link, each with its own history
        let mut more: Vec<Sym> = Vec::new();
        for e in top.get("entries").and_then(Json::as_array).into_iter().flatten() {
            let calls =
                e.get("calls").and_then(Json::as_array).into_iter().flatten().filter_map(|c| c["name"].as_str());
            for n in calls.chain(e.pointer("/copies/name").and_then(Json::as_str)) {
                if let Some(s) =
                    a.interner.lookup(crate::engine::tex::bare_name(n)).filter(|s| *s != sym && !more.contains(s))
                {
                    more.push(s);
                }
            }
        }
        more.truncate(a.settings.type_plugins.history.entries);
        let others: serde_json::Map<String, Json> = more.iter().map(|&s| (a.interner.cs(s), history(a, s))).collect();
        let cs = a.interner.cs(sym);
        Some(json!({
            "title": format!("Definition history of {cs}"),
            "sections": [{ "kind": "history", "name": cs, "trace": cs, "history": top, "histories": others }],
        }))
    }
}

const TITLE: &str = "Definition history…";

/// The name at the cursor, when the run defined it.
fn name(cx: &Cx, hit: &Hit) -> Option<Sym> {
    let typed = hit.typed.as_deref()?;
    let sym = cx.analysis.interner.lookup(crate::engine::tex::bare_name(typed))?;
    (cx.analysis.facts.defs.iter().any(|d| d.name == sym) || cx.analysis.kernel_sites.contains_key(&sym)).then_some(sym)
}

/// Whether two definitions can stand on one path: no conditional they both depend on took
/// different arms.
fn together(a: &Definition, b: &Definition) -> bool {
    !a.cds.iter().any(|x| b.cds.iter().any(|y| x.on == y.on && x.taken != y.taken))
}

fn same_path(a: &[crate::analysis::graph::ControlDep], b: &[crate::analysis::graph::ControlDep]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.on == y.on && x.taken == y.taken)
}

fn same(a: &Definition, b: &Definition) -> bool {
    match (&a.mac, &b.mac) {
        (Some(x), Some(y)) => Rc::ptr_eq(&x.replacement_text, &y.replacement_text),
        _ => false,
    }
}

/// `{entries, other, undefined_elsewhere}` of `sym`.
fn history(a: &Analysis, sym: Sym) -> Json {
    let cfg = &a.settings.type_plugins;
    let mut defs: Vec<&Definition> = a.facts.defs.iter().filter(|d| d.name == sym).collect();
    defs.sort_by_key(|d| d.node);
    // a site run again with the same text is one event: its last run
    let mut seen: Vec<&Definition> = Vec::new();
    for d in defs.into_iter().rev() {
        let text = |d: &Definition| d.mac.as_ref().map(|m| m.replacement_text.clone());
        if !seen
            .iter()
            .any(|s| s.span == d.span && text(s).as_deref() == text(d).as_deref() && same_path(&s.cds, &d.cds))
        {
            seen.push(d);
        }
    }
    // newest first; the meanings in force when the run ended
    let finals: Vec<Rc<[Token]>> = {
        let slot = a.env.slot(sym);
        let may = slot.and_then(|s| s.may.clone());
        std::iter::once(a.env.meaning(sym))
            .chain(may.iter().flat_map(|m| m.iter().cloned()))
            .filter_map(|m| m.as_macro().map(|m| m.replacement_text.clone()))
            .collect()
    };
    let is_final = |d: &Definition| {
        d.mac
            .as_ref()
            .is_some_and(|m| finals.iter().any(|f| Rc::ptr_eq(f, &m.replacement_text) || **f == *m.replacement_text))
    };
    let head = seen.iter().copied().find(|d| is_final(d)).or_else(|| seen.first().copied());
    let (chain, other): (Vec<&Definition>, Vec<&Definition>) =
        seen.iter().copied().partition(|d| head.is_none_or(|h| together(d, h)));
    // what each event changed: against the one before it on the chain
    let mut entries: Vec<Json> = Vec::new();
    for (i, &d) in chain.iter().enumerate() {
        let before = chain.get(i + 1).copied();
        let mut e = entry(a, d, before);
        // a later local definition its group undid stands above the one in force
        let state = match head.is_some_and(|h| std::ptr::eq(h, d)) && is_final(d) {
            true => "in force",
            false if d.depth > 0 && !d.global && head.is_some_and(|h| d.node > h.node) => "undone",
            false => "replaced",
        };
        e["state"] = json!(state);
        entries.push(e);
    }
    if let Some(&k) = a.kernel_sites.get(&sym) {
        let mut e = Json::Object(crate::queries::query::place(a, k));
        e["by"] = json!("the kernel");
        e["owner"] = json!("kernel");
        e["state"] = json!(if chain.is_empty() { "in force" } else { "replaced" });
        if chain.is_empty() {
            insert_body(a, &mut e, a.env.meaning(sym).as_macro().map(|m| &**m));
        }
        entries.push(e);
    }
    let limit = cfg.history.entries;
    let more = entries.len().saturating_sub(limit);
    entries.truncate(limit);
    let other: Vec<Json> = other
        .iter()
        .take(limit)
        .map(|d| {
            let mut e = entry(a, d, None);
            e["state"] = json!(if is_final(d) { "in force on another path" } else { "replaced on its path" });
            e
        })
        .collect();
    let undefined = a
        .env
        .slot(sym)
        .is_some_and(|s| s.may.as_ref().is_some_and(|m| m.iter().any(|m| matches!(m, Meaning::Undefined))));
    json!({ "entries": entries, "earlier": more, "other": other, "undefined_elsewhere": undefined })
}

fn insert_body(a: &Analysis, e: &mut Json, mac: Option<&crate::engine::tex::MacroDef>) {
    if let Some(m) = mac {
        e["body"] = json!(detokenize(&m.replacement_text, &a.interner));
        if let Some(p) = crate::queries::pretty::of(a, Some(m)) {
            e["pretty"] = json!(p);
        }
    }
}

/// One event: its place, who made it, its body, what it changed against `before`, the copy it
/// made, the calls in its body and the conditions it stands under.
fn entry(a: &Analysis, d: &Definition, before: Option<&Definition>) -> Json {
    let mut e = Json::Object(crate::queries::query::place(a, d.span));
    e["by"] = json!(a.interner.cs(d.by));
    e["tag"] = json!(d.tag);
    e["owner"] = json!(match d.package {
        Some(p) => a.interner.name(p).to_string(),
        None => crate::queries::query::origin(a, d.span.file).to_string(),
    });
    if d.depth > 0 && !d.global {
        e["local"] = json!(true);
    }
    insert_body(a, &mut e, d.mac.as_deref());
    let text = |d: &Definition| d.mac.as_ref().map(|m| detokenize(&m.replacement_text, &a.interner));
    if let (Some(now), Some(was)) = (text(d), before.and_then(text)) {
        e["change"] = change(&was, &now);
    }
    // a copy: the definition of another name it holds
    if d.tag == "alias"
        && let Some(src) = a
            .facts
            .defs
            .iter()
            .filter(|s| s.name != d.name && s.node < d.node && s.tag != "alias" && same(s, d))
            .max_by_key(|s| s.node)
    {
        let mut c = Json::Object(crate::queries::query::place(a, src.span));
        c["name"] = json!(a.interner.cs(src.name));
        c["by"] = json!(a.interner.cs(src.by));
        e["copies"] = c;
    }
    // the names its body calls that the run defined; a copy of an earlier meaning says whose
    let mut calls: Vec<Json> = Vec::new();
    for t in d.mac.as_ref().map_or(&[][..], |m| &m.replacement_text[..]) {
        let Some(s) = t.cs().filter(|s| *s != d.name) else { continue };
        let name = a.interner.cs(s);
        if calls.iter().any(|c| c["name"] == name.as_str()) || !a.facts.defs.iter().any(|x| x.name == s) {
            continue;
        }
        let mut c = json!({ "name": name });
        let held = a.facts.defs.iter().filter(|x| x.name == s && x.node < d.node).max_by_key(|x| x.node);
        if let Some(src) = held.filter(|h| h.tag == "alias").and_then(|h| {
            a.facts
                .defs
                .iter()
                .filter(|x| x.name != s && x.node < h.node && x.tag != "alias" && same(x, h))
                .max_by_key(|x| x.node)
        }) {
            c["holds"] = json!(a.interner.cs(src.name));
            c["at"] = json!(format!("{}:{}", a.short_name(src.span.file), src.span.line));
        }
        calls.push(c);
    }
    if !calls.is_empty() {
        e["calls"] = json!(calls);
    }
    let when: Vec<String> = d
        .cds
        .iter()
        .filter_map(|cd| {
            let v = a.graph.vertex(cd.on)?;
            let arm = if cd.taken { "true" } else { "false" };
            Some(format!("{} {arm} at {}:{}", a.interner.cs(v.name), a.short_name(v.span.file), v.span.line))
        })
        .collect();
    if !when.is_empty() {
        e["when"] = json!(when);
    }
    if !d.certain {
        e["uncertain"] = json!(true);
    }
    e
}

/// What `now` changed of `was`: appended or prepended text, else the part replaced.
fn change(was: &str, now: &str) -> Json {
    let pre = was.chars().zip(now.chars()).take_while(|(x, y)| x == y).count();
    let (was, now): (Vec<char>, Vec<char>) = (was.chars().collect(), now.chars().collect());
    let post = was[pre..].iter().rev().zip(now[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let removed: String = was[pre..was.len() - post].iter().collect();
    let added: String = now[pre..now.len() - post].iter().collect();
    let kind = match (removed.is_empty(), added.is_empty()) {
        (true, true) => "same",
        (true, false) if post == 0 => "appends",
        (true, false) if pre == 0 => "prepends",
        (true, false) => "inserts",
        (false, true) => "removes",
        (false, false) => "replaces",
    };
    // a replaced text whole: a shared `\` or brace cut off reads wrong
    let (removed, added) =
        if kind == "replaces" { (was.iter().collect(), now.iter().collect()) } else { (removed, added) };
    json!({ "kind": kind, "added": added, "removed": removed })
}
