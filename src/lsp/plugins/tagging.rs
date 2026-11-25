//! Tagging and accessibility: a manager of what a tagged PDF needs, read from the run
//! ([`crate::analysis::tagging`]). The document's record writes `\DocumentMetadata`'s
//! `tagging`, `pdfstandard` and `lang` keys and the title the PDF gets; a figure's record its
//! `alt` text; a table's record its header rows (`\tagpdfsetup{table/header-rows=…}` before it).
//! Every edit goes through the snapshot printer. Where the source is not plain enough to
//! rewrite (options from a macro), a record says so and offers nothing.

use super::super::snapshot::{self, Arg, Key, Node};
use super::{Cx, TypePlugin, page, span};
use crate::analysis::roles::Ty;
use crate::analysis::tagging::{self, Tagging, Val};
use crate::project::config::TypePlugins;
use serde_json::{Value as Json, json};

pub(super) struct Tagged;

const TITLE: &str = "Accessibility…";

impl TypePlugin for Tagged {
    fn name(&self) -> &'static str {
        "tagging"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "PDF tagging: on or off, the standard, language and title, figures' alternative text, tables' header rows"
    }
    // the main file's `\DocumentMetadata` triggers it, not a type of the text
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "manage"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.tagging.enabled
    }
    fn opened_by(&self) -> Option<(&'static [&'static str], &'static str)> {
        // only where the document asks for PDF management: every other document would get a lens
        Some((&["DocumentMetadata"], TITLE))
    }
    fn manage(&self, cx: &Cx) -> Option<Json> {
        let text = cx.doc.body();
        // the main file: its `\documentclass` is where `\DocumentMetadata` goes
        let class = page::calls(text, "documentclass", 0, text.len()).into_iter().next()?;
        let file = super::super::type_at::file_of(cx.analysis, cx.path)?;
        (file == cx.analysis.main_file).then_some(())?;
        let t = tagging::of(cx.analysis);
        let mut items = vec![document(cx, t, class.0)];
        items.extend(title(cx, t));
        let on = t.is_some_and(Tagging::active);
        if on {
            items.extend(
                super::super::dialogs::figures(cx.analysis, cx.doc, cx.path).into_iter().map(|f| figure(cx, f)),
            );
            if cx.analysis.defined("tagpdfsetup") {
                items.extend(
                    super::super::dialogs::tables(cx.analysis, cx.doc, cx.path).into_iter().map(|r| table(cx, r)),
                );
            }
        }
        Some(json!({
            "title": "Accessibility",
            "sections": [
                { "kind": "records", "placeholder": "Search the document, figures and tables", "items": items },
            ],
        }))
    }
}

/// The document: `\DocumentMetadata`'s keys as a form, written into its call, or a new call above
/// `\documentclass`; the packages the status list marks, as a table.
fn document(cx: &Cx, t: Option<&Tagging>, class: usize) -> Json {
    let text = cx.doc.body();
    let call = page::calls(text, "DocumentMetadata", 0, class).into_iter().next();
    let node = match &call {
        Some((at, ..)) => snapshot::read(cx.doc, *at, &[0], false),
        None => {
            Some(Node::new_call("DocumentMetadata", vec![Arg::keys('{', Vec::new())], span(cx.doc, class, class).start))
        }
    };
    // the keys as written: a key the call writes through a macro is not offered
    let written = |k: &str| node.as_ref().and_then(|n| snapshot::list(n).iter().find(|x| x.key == k)?.value.clone());
    let on = t.is_some_and(Tagging::active);
    let standards = t.map(|t| t.standards.join(", ")).unwrap_or_default();
    let lang = t.and_then(|t| t.lang.text()).unwrap_or_default().to_string();
    let mut detail = vec![if on { "tagged" } else { "not tagged" }.to_string()];
    detail.extend((!standards.is_empty()).then(|| format!("PDF/{standards}")));
    detail.extend((!lang.is_empty()).then(|| lang.clone()));
    let mut badge = None;
    if on && t.is_some_and(|t| t.lang == Val::Empty) {
        badge = Some("no language");
    }
    let fields = vec![
        json!({ "name": "tagging", "label": "tagging", "value": if on { "on" } else { "off" }, "choices": ["on", "off"] }),
        json!({ "name": "pdfstandard", "label": "PDF standard", "value": written("pdfstandard").unwrap_or_default(),
                "placeholder": "none", "options": offered_standards(cx), "check": ["tex"] }),
        json!({ "name": "lang", "label": "language (BCP 47)", "value": written("lang").unwrap_or_default(),
                "placeholder": if lang.is_empty() { "en-US".to_string() } else { lang.clone() }, "check": ["tex"] }),
    ];
    let fills: Vec<snapshot::Fill> = vec![
        ("tagging", 0, Some(0), Some("tagging")),
        ("pdfstandard", 0, Some(0), Some("pdfstandard")),
        ("lang", 0, Some(0), Some("lang")),
    ];
    let mut forms = Vec::new();
    let mut why = None;
    match node {
        Some(n) => forms.push(json!({
            "title": if call.is_some() { "Save" } else { "Add \\DocumentMetadata" },
            "fields": fields,
            "print": snapshot::form(&[n], &fills),
            "note": "Writes the keys into \\DocumentMetadata, which stands before \\documentclass.",
        })),
        None => why = Some("\\DocumentMetadata is written in a way this dialog cannot rewrite: edit it in the source."),
    }
    let rows: Vec<Json> = t
        .map(|t| t.statuses.iter().filter(|s| s.flagged()).map(|s| json!([s.file, s.meaning()])).collect())
        .unwrap_or_default();
    let table =
        (!rows.is_empty()).then(|| json!({ "kind": "table", "columns": ["file", "tagging status"], "rows": rows }));
    let incompatible = t.is_some_and(|t| t.statuses.iter().any(|s| s.incompatible()));
    json!({
        "title": "Document",
        "detail": detail.join(" · "),
        "badge": if on && incompatible { Some("incompatible package") } else { badge },
        "search": format!("document tagging metadata language standard {}", detail.join(" ")),
        "facets": { "kind": "document" },
        "sites": super::sites([(cx.path, cx.doc.pos(call.as_ref().map_or(class, |c| c.0)).line)]),
        "forms": forms,
        "why": why,
        "table": table,
    })
}

/// The standards pdfmanagement takes: the choices its `document/metadata` keys define in the run,
/// lower-cased as documents write them; none before a `\DocumentMetadata` ran.
fn offered_standards(cx: &Cx) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let names = &cx.analysis.interner;
    for sym in (0..names.len() as u32).map(crate::engine::tex::Sym) {
        let name = names.name(sym);
        let Some(rest) = name.split_once("metadata/_pdfstandard/").map(|(_, r)| r) else { continue };
        if rest.is_empty() || rest == "unknown" || rest.contains([' ', '/']) || !cx.analysis.env.is_defined(sym) {
            continue;
        }
        let s = rest.to_lowercase();
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out.sort();
    out
}

/// The PDF's title (`dc:title`): written as `\title[pdftitle=…]` where latex-lab's title code
/// reads it, else into a `\hypersetup` the main file has; a record that says why otherwise.
fn title(cx: &Cx, t: Option<&Tagging>) -> Option<Json> {
    let t = t?;
    let text = cx.doc.body();
    let rendered = cx.analysis.metadata.iter().find(|m| m.field == "title").map(|m| m.text.clone());
    let lab = crate::analysis::machine::loaded(cx.analysis, "latex-lab-testphase-title");
    let target = match lab {
        true => page::calls(text, "title", 0, text.len()).into_iter().next().map(|c| (c.0, "pdftitle")),
        false => page::calls(text, "hypersetup", 0, text.len()).into_iter().next().map(|c| (c.0, "pdftitle")),
    };
    let node = target.and_then(|(at, _)| snapshot::read(cx.doc, at, &[0], lab));
    let value = node.as_ref().and_then(|n| snapshot::list(n).iter().find(|k| k.key == "pdftitle")?.value.clone());
    let missing = t.title == Val::Empty;
    let mut forms = Vec::new();
    let why = match &node {
        Some(n) => {
            let field = json!({ "name": "pdftitle", "label": "PDF title", "value": value.clone().unwrap_or_default(),
                                "placeholder": rendered.clone().unwrap_or_default(), "check": ["tex"] });
            forms.push(json!({ "title": "Save", "fields": [field], "print": snapshot::form(std::slice::from_ref(n), &[("pdftitle", 0, Some(0), Some("pdftitle"))]),
                               "note": format!("Writes pdftitle into \\{}.", n.name) }));
            None
        }
        None if lab => Some("There is no \\title call in this file to write pdftitle into."),
        None => Some(
            "Load hyperref and write \\hypersetup{pdftitle={…}}, or use tagging, whose \\title takes [pdftitle=…].",
        ),
    };
    Some(json!({
        "title": "Title",
        "detail": match &t.title { Val::Text(v) => v.clone(), Val::Empty => "none: viewers show the file name".into(), Val::Unknown => "not known: the run did not reach the end".into() },
        "badge": (missing && t.ua().is_some()).then_some("no title"),
        "search": "title pdftitle metadata",
        "facets": { "kind": "metadata" },
        "sites": super::sites(target.map(|(at, _)| (cx.path, cx.doc.pos(at).line))),
        "forms": forms,
        "why": why,
    }))
}

/// A figure: its `alt` text in the call's options, or marked as decoration (`artifact`).
fn figure(cx: &Cx, (at, end, file): (usize, usize, String)) -> Json {
    let text = cx.doc.body();
    let line = cx.doc.pos(at).line;
    let node = snapshot::read(cx.doc, at, &[0], true);
    // options that are no plain key list (a macro of options) are left to the source
    let plain = node.as_ref().is_some_and(|n| snapshot::list(n).iter().all(|k| !k.key.contains(['\\', '{', '}'])));
    let keys = node.as_ref().map(|n| snapshot::list(n).to_vec()).unwrap_or_default();
    let has = |k: &str| keys.iter().any(|x| x.key == k);
    let alt = keys.iter().find(|k| k.key == "alt").and_then(|k| k.value.clone());
    let decorative = has("artifact");
    // the file as the call writes it, not the path the run resolved
    let written =
        node.as_ref().and_then(|n| n.args.iter().rev().find(|a| a.open == '{')).map(|a| a.text.trim().to_string());
    let name =
        written.filter(|w| !w.is_empty()).unwrap_or_else(|| file.rsplit('/').next().unwrap_or(&file).to_string());
    let mut forms = Vec::new();
    let mut actions = Vec::new();
    if let Some(n) = node.filter(|_| plain) {
        let field = json!({ "name": "alt", "label": "alternative text", "value": alt.clone().unwrap_or_default(),
                            "placeholder": "what the figure shows", "long": true, "check": ["tex"] });
        forms.push(json!({ "title": "Save", "fields": [field], "print": snapshot::form(std::slice::from_ref(&n), &[("alt", 0, Some(0), Some("alt"))]),
                           "note": "Screen readers read this out in place of the figure." }));
        if !decorative {
            let mut d = n.clone();
            if let Some(list) = d.args.iter_mut().find_map(|a| a.keys.as_mut()) {
                list.retain(|k| k.key != "alt");
                list.push(Key { key: "artifact".into(), value: None });
            }
            let edits = snapshot::edits(cx.analysis, cx.doc, cx.file(), &[d]);
            actions.push(json!({ "title": "Mark as decoration (artifact)", "edit": super::edit_of(cx.path, edits) }));
        }
    }
    let why = (!plain).then_some("The options come from a macro: edit them in the source.");
    let missing = alt.as_deref().is_none_or(|a| a.trim().is_empty()) && !decorative && !has("actualtext");
    let snippet = text[at..end].lines().next().unwrap_or_default().to_string();
    json!({
        "title": format!("{name} · line {line}"),
        "detail": if decorative { "decoration".to_string() } else { alt.clone().unwrap_or_else(|| "no alternative text".into()) },
        "badge": missing.then_some("no alt text"),
        "search": format!("figure {name} {file} {} {snippet}", alt.unwrap_or_default()),
        "sort": { "line": format!("{line:06}") },
        "facets": { "kind": "figure" },
        "sites": super::sites([(cx.path, line)]),
        "forms": forms,
        "actions": actions,
        "why": why,
    })
}

/// A table: its header rows, as `\tagpdfsetup{table/header-rows=…}` on the line before it (one
/// already there is read and rewritten).
fn table(cx: &Cx, (at, name): (usize, String)) -> Json {
    let text = cx.doc.body();
    let line = cx.doc.pos(at).line;
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    // the setup the line before writes, if it is one
    let before = text[..start.saturating_sub(1)].rfind('\n').map_or(0, |i| i + 1);
    let setup = page::calls(text, "tagpdfsetup", before, start).into_iter().next();
    let node = match &setup {
        Some((s, ..)) => snapshot::read(cx.doc, *s, &[0], false),
        None => Some(Node::new_call("tagpdfsetup", vec![Arg::keys('{', Vec::new())], span(cx.doc, start, start).start)),
    };
    let key = "table/header-rows";
    let value = node.as_ref().and_then(|n| snapshot::list(n).iter().find(|k| k.key == key)?.value.clone());
    let forms: Vec<Json> = node
        .map(|n| {
            let field = json!({ "name": "rows", "label": "header rows", "value": value.clone().unwrap_or_default(),
                                "placeholder": "1", "check": ["tex"] });
            json!({ "title": "Save", "fields": [field], "print": snapshot::form(&[n], &[("rows", 0, Some(0), Some(key))]),
                    "note": "The rows (1 is the first) tagged as header cells (TH), every other cell data (TD). The setting holds for later tables in the same group too." })
        })
        .into_iter()
        .collect();
    json!({
        "title": format!("{name} at line {line}"),
        "detail": value.as_ref().map_or("no header rows: every cell is data".to_string(), |v| format!("header rows {v}")),
        "badge": value.is_none().then_some("no header"),
        "search": format!("table {name} header rows"),
        "sort": { "line": format!("{line:06}") },
        "facets": { "kind": "table" },
        "sites": super::sites([(cx.path, line)]),
        "forms": forms,
    })
}
