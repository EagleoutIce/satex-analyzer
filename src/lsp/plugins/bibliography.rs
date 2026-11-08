//! Bibliography entries as a reference manager (`doc/site/bib.js`): every entry of the
//! databases the run read with its snapshot, where it is cited and what the style's data model
//! finds wrong with it; the `@string` constants; requests that import, convert, merge and
//! rename (`op`). Every edit is printed by the snapshot printer. A `thebibliography` in the open
//! file edits in place and converts to a `.bib` file.

use super::{Cx, Hit, TypePlugin, call_at, edit_of, lines_of, put, span};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::linter::bibdb::{Db, Load};
use crate::linter::fix::Sources;
use crate::lsp::handlers::{path_uri, to_lsp};
use crate::lsp::snapshot;
use crate::plugins::bib::Entry;
use crate::plugins::bib::model::{self, Model};
use crate::project::config::TypePlugins;
use crate::text::Text;
use serde_json::{Value as Json, json};

pub(super) struct Bibliography;

impl TypePlugin for Bibliography {
    fn name(&self) -> &'static str {
        "bibliography"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "a reference manager: entries filtered by type and status, fields checked by the style's data model, types converted, @string constants, imports (DOI, arXiv, RIS, EndNote, CSL-JSON, Zotero) and dblp sync, thebibliography's items edited and converted to .bib"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Cite)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view", "manage", "lens", "hover"]
    }
    fn opened_by(&self) -> Option<(&'static [&'static str], &'static str)> {
        Some((&["cite", "bibliography", "printbibliography", "addbibresource"], "Manage bibliography…"))
    }
    fn entry(&self, cx: &Cx, _hit: &Hit) -> Option<String> {
        self.manage(cx).map(|_| super::open_link(cx, "Manage bibliography…", self.name()))
    }
    /// `thebibliography` (no `.bib` file): its manager, as on a `\bibitem`.
    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<lsp_types::CodeLens> {
        super::env_lenses(analysis, doc, path, "thebibliography", "Manage bibliography…", "bibliography")
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.bibliography.enabled
    }
    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let mut v = self.manage(cx)?;
        v["sections"][0]["focus"] = json!(cx.at.item.typed.trim());
        Some(v)
    }
    fn manage(&self, cx: &Cx) -> Option<Json> {
        let (loads, dbs) = crate::linter::bibdb::databases(cx.analysis, &Sources::default());
        let list = thebibliography(cx);
        if dbs.is_empty() && list.is_none() {
            return None;
        }
        let mut sections = Vec::new();
        if !dbs.is_empty() {
            sections.push(references(cx, &loads, &dbs));
        }
        if let Some(l) = &list {
            sections.push(bibitems(cx, l));
            sections.extend(convert(cx, l));
        }
        Some(json!({ "title": "Bibliography", "sections": sections }))
    }
}

/// `thebibliography`'s items as records: each edits in place, a new one goes before `\end`.
fn bibitems(cx: &Cx, l: &List) -> Json {
    let items: Vec<Json> = l.items.iter().map(|i| bibitem(cx, l, i)).collect();
    let taken: Vec<&str> = l.items.iter().map(|i| i.key.as_str()).collect();
    let key = json!({ "name": "citekey", "value": "", "placeholder": "citation key", "check": ["required", "bibkey", "unique"], "taken": taken });
    // before `\end{thebibliography}`, on a line of its own
    let at = l.end_tag;
    let lead = if cx.doc.body()[..at].ends_with('\n') { "" } else { "\n" };
    let text = format!("{lead}\\bibitem${{?label}}[${{label}}]${{/label}}{{${{citekey}}}} ${{text}}\n");
    let add = json!({ "name": "\\bibitem", "fields": [key,
        { "name": "label", "value": "", "placeholder": "label printed instead of the number (optional)", "check": ["balanced"] },
        { "name": "text", "value": "", "placeholder": "Author. Title. Publisher, year.", "long": true, "check": ["required", "tex"] }],
        "edit": edit_of(cx.path, vec![put(cx.doc, at, at, text)]) });
    json!({ "kind": "records", "placeholder": "search keys, fields, text, where cited", "sorts": ["key", "author", "year", "title", "cited"],
            "items": items, "add": { "title": "Add entry", "kinds": [add] } })
}

/// The checks of a field's value by its datatype in the style's data model, and by the
/// identifiers some fields hold (DOI, ISBN, ISSN): `dialogs.js` `CHECKS`.
fn checks(model: &Model, field: &str) -> Vec<&'static str> {
    let mut out = match model.datatype(field) {
        "name" => vec!["balanced", "names"],
        "date" => vec!["bibdate"],
        "datepart" => vec!["datepart"],
        "range" => vec!["pages"],
        "integer" => vec!["integer"],
        "uri" => vec!["url"],
        "entrykey" => vec!["bibkeyref"],
        "verbatim" => vec![],
        _ => vec!["balanced"],
    };
    match field {
        "doi" => out.push("doi"),
        "isbn" => out.push("isbn"),
        "issn" => out.push("issn"),
        _ => {}
    }
    out
}

/// The databases as a reference manager shows them: each entry with its snapshot (edited and
/// printed back), where it is cited and what is wrong with it; the `@string` constants with
/// their uses; the style's types, fields and checks; the loads read twice.
fn references(cx: &Cx, loads: &[Load], dbs: &[Db]) -> Json {
    let model = model::of(cx.analysis, cx.path);
    let twins = crate::linter::bibdb::twins(dbs);
    let all: Vec<&Entry> = dbs.iter().flat_map(|d| &d.entries).collect();
    let mut items = Vec::new();
    for db in dbs {
        for e in &db.entries {
            let Some(node) = node_of(&db.text, e) else { continue };
            let cited = cites(cx, &e.key);
            let field = |n: &str| e.fields.iter().find(|f| f.0 == n).map(|f| f.1.clone());
            let mut dups: Vec<Json> = twins
                .iter()
                .filter(|t| t.0.key == e.key || t.1.key == e.key)
                .map(|(k, d, why)| json!({ "keep": k.key, "drop": d.key, "why": why }))
                .collect();
            if all.iter().filter(|x| x.key == e.key).count() > 1 {
                dups.push(json!({ "why": "the same key" }));
            }
            let who = crate::plugins::bib::names(&e.author);
            let author = match who.len() {
                0 => field("editor").unwrap_or_default(),
                1 => who[0].0.clone(),
                2 => format!("{} and {}", who[0].0, who[1].0),
                _ => format!("{} et al.", who[0].0),
            };
            let constants: Vec<String> = e
                .fields
                .iter()
                .filter(|f| e.bare.contains(&f.0))
                .flat_map(|f| crate::linter::bibdb::words(&f.1))
                .collect();
            items.push(json!({
                "key": e.key, "type": e.kind, "file": db.path, "line": e.line, "uri": path_uri(&db.path).as_str(),
                "author": author.replace(['{', '}'], ""), "year": e.year, "title": e.title,
                "cited": cited.len(), "places": places(&cited), "sites": super::sites(cited.iter().map(|(f, l)| (f.as_str(), *l))),
                "node": node,
                "missing": crate::linter::bibdb::missing(e, &model),
                "undefined": crate::linter::bibdb::undefined(e, dbs, &model),
                "constants": constants,
                "duplicates": dups,
            }));
        }
    }
    let mut constants = Vec::new();
    for db in dbs {
        for c in &db.constants {
            let uses: Vec<&str> = all
                .iter()
                .filter(|e| {
                    e.fields.iter().any(|f| {
                        e.bare.contains(&f.0)
                            && crate::linter::bibdb::words(&f.1).iter().any(|w| w.eq_ignore_ascii_case(&c.name))
                    })
                })
                .map(|e| e.key.as_str())
                .collect();
            constants.push(json!({ "name": c.name, "value": c.value, "file": db.path, "line": c.line, "uses": uses }));
        }
    }
    // a value written braced in several entries: a constant to make
    let mut repeated: Vec<(String, String, Vec<&str>)> = Vec::new();
    for e in &all {
        for (n, v) in e
            .fields
            .iter()
            .filter(|f| !e.bare.contains(&f.0) && matches!(model.datatype(&f.0), "literal" | "list") && f.1.len() > 3)
        {
            if matches!(n.as_str(), "title" | "abstract" | "note" | "keywords") {
                continue;
            }
            match repeated.iter_mut().find(|r| r.1 == *v) {
                Some(r) => r.2.push(&e.key),
                None => repeated.push((n.clone(), v.clone(), vec![&e.key])),
            }
        }
    }
    repeated.retain(|r| r.2.len() > 1);
    let repeated: Vec<Json> =
        repeated.iter().map(|(f, v, keys)| json!({ "field": f, "value": v, "keys": keys })).collect();
    let mut types: Vec<Json> = model
        .types
        .iter()
        .map(|t| json!({ "name": t, "fields": model.fields_of(t), "required": model.required_of(t) }))
        .collect();
    for e in &all {
        if !types.iter().any(|t| t["name"] == e.kind.as_str()) {
            types.push(
                json!({ "name": e.kind, "fields": e.fields.iter().map(|f| &f.0).collect::<Vec<_>>(), "required": [] }),
            );
        }
    }
    let mut fields: Vec<&str> = model.fields.iter().map(String::as_str).collect();
    for e in &all {
        for f in &e.fields {
            if !fields.contains(&f.0.as_str()) {
                fields.push(&f.0);
            }
        }
    }
    let checks: serde_json::Map<String, Json> =
        fields.iter().map(|f| (f.to_string(), json!(checks(&model, f)))).collect();
    // a database loaded twice: the lint's quick fix
    let twice: Vec<Json> = loads
        .iter()
        .enumerate()
        .filter(|(i, l)| l.path.is_some() && loads[..*i].iter().any(|f| f.path == l.path))
        .map(|(_, l)| json!({ "name": l.name, "file": l.file, "line": l.span.line }))
        .collect();
    let style =
        cx.analysis.facts.occurrences.iter().find(|o| o.kind == OccKind::BibStyle).map(|s| format!("{}.bst", s.key));
    let biblatex = crate::linter::shared::loaded(cx.analysis, "biblatex");
    json!({
        "kind": "references",
        "files": dbs.iter().map(|d| json!({ "path": d.path, "uri": path_uri(&d.path).as_str(), "name": d.path.rsplit('/').next() })).collect::<Vec<_>>(),
        "style": style.unwrap_or_else(|| if biblatex { "biblatex".into() } else { "the databases".into() }),
        "types": types, "fields": fields, "checks": checks,
        "macros": model.macros.iter().map(|m| &m.0).collect::<Vec<_>>(),
        "items": items, "constants": constants, "repeated": repeated, "twice": twice,
    })
}

/// The snapshot of entry `e` in its database's text.
fn node_of(text: &Text, e: &Entry) -> Option<snapshot::Node> {
    let start = text.offset(crate::text::Pos::new(e.line, 1))?;
    let at = start + text.body()[start..].find('@')?;
    snapshot::entry(text, at)
}

/// Where each `key` is cited: file and line.
fn cites(cx: &Cx, key: &str) -> Vec<(String, u32)> {
    let a = cx.analysis;
    let mut out: Vec<(String, u32)> = Vec::new();
    for o in a.facts.occurrences.iter().filter(|o| o.kind == OccKind::Cite && o.key == key) {
        let place = (a.files[usize::from(o.span.file)].path.clone(), o.span.line);
        if !out.contains(&place) {
            out.push(place);
        }
    }
    out
}

/// The places as links a click opens, with what to say when there is none.
fn places(cites: &[(String, u32)]) -> String {
    match cites.is_empty() {
        true => "Not cited.".into(),
        false => {
            format!("Cited at {}", cites.iter().map(|(f, l)| format!("`{f}:{l}`")).collect::<Vec<_>>().join(" · "))
        }
    }
}

/// A `thebibliography` of the open file: its `\begin`, the byte of its `\end`, its items.
struct List {
    begin: usize,
    end_tag: usize,
    end: usize,
    items: Vec<Item>,
}

/// A `\bibitem[label]{key} text`: the call's `\`, its name's end, its groups and its text.
struct Item {
    key: String,
    at: usize,
    name_end: usize,
    label: Option<std::ops::Range<usize>>,
    key_group: std::ops::Range<usize>,
    text: std::ops::Range<usize>,
}

fn thebibliography(cx: &Cx) -> Option<List> {
    let (b, e) = super::pairs(cx.analysis, cx.path, "thebibliography").into_iter().next()?;
    let text = cx.doc.body();
    let begin = cx.doc.offset(crate::text::Pos::new(b.span.line, 1))?
        + text[cx.doc.offset(crate::text::Pos::new(b.span.line, 1))?..].find("\\begin{thebibliography}")?;
    let from = cx.doc.offset(crate::text::Pos::new(e.span.line, 1))?;
    let end_tag = from + text[from..].find("\\end{thebibliography}")?;
    let end = end_tag + "\\end{thebibliography}".len();
    let mut calls: Vec<(usize, String)> = (cx.analysis.facts.occurrences.iter())
        .filter(|o| o.kind == OccKind::BibItem)
        .filter_map(|o| Some((super::byte_of(cx, o.span)?, o.key.clone())))
        .filter(|(at, _)| (begin..end_tag).contains(at))
        .collect();
    calls.sort();
    calls.dedup();
    let mut items = Vec::new();
    for (i, (at, key)) in calls.iter().enumerate() {
        let Some((name_end, groups, call_end)) = call_at(text, *at) else { continue };
        let Some(g) = groups.iter().find(|g| g.open == '{') else { continue };
        let label = groups.iter().find(|g| g.open == '[').map(|g| g.inner.clone());
        let stop = calls.get(i + 1).map_or(end_tag, |c| c.0);
        let body = &text[call_end..stop];
        let lead = body.len() - body.trim_start().len();
        let text = call_end + lead..call_end + body.trim_end().len().max(lead);
        items.push(Item { key: key.clone(), at: *at, name_end, label, key_group: g.inner.clone(), text });
    }
    Some(List { begin, end_tag, end, items })
}

/// A `\bibitem` as a record: its label, text and key edited in place (the key in every
/// `\cite` of the open file too), deleted when uncited.
fn bibitem(cx: &Cx, l: &List, i: &Item) -> Json {
    let text = cx.doc.body();
    let cited = cites(cx, &i.key);
    let shown = text[i.text.clone()].split_whitespace().collect::<Vec<_>>().join(" ");
    let label = i.label.as_ref().map(|r| text[r.clone()].to_string()).unwrap_or_default();
    // the key: here and in each \cite of the open file
    let mut keys = vec![put(cx.doc, i.key_group.start, i.key_group.end, "${value}")];
    for o in cx.analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Cite && o.key == i.key) {
        let Some((_, groups, _)) = super::byte_of(cx, o.span).and_then(|at| call_at(text, at)) else { continue };
        for g in groups.iter().filter(|g| g.open == '{') {
            let mut at = g.inner.start;
            for part in text[g.inner.clone()].split(',') {
                let lead = part.len() - part.trim_start().len();
                if part.trim() == i.key {
                    let e = put(cx.doc, at + lead, at + lead + i.key.len(), "${value}");
                    if !keys.iter().any(|k| k.range == e.range) {
                        keys.push(e);
                    }
                }
                at += part.len() + 1;
            }
        }
    }
    let taken: Vec<&str> = l.items.iter().map(|x| x.key.as_str()).filter(|k| *k != i.key).collect();
    let label_end = i.key_group.start - 1;
    let fields = vec![
        json!({ "name": "key", "value": i.key, "check": ["required", "bibkey", "unique"], "taken": taken, "edit": edit_of(cx.path, keys) }),
        json!({ "name": "label", "value": label, "placeholder": "printed instead of the number (optional)", "check": ["balanced"],
                "edit": edit_of(cx.path, vec![put(cx.doc, i.name_end, label_end, "${?value}[${value}]${/value}")]) }),
        json!({ "name": "text", "value": text[i.text.clone()], "long": true, "check": ["required", "tex"],
                "edit": edit_of(cx.path, vec![put(cx.doc, i.text.start, i.text.end, "${value}")]) }),
    ];
    let (from, to) = lines_of(text, i.at, i.text.end);
    let reason = (!cited.is_empty()).then(|| format!("cited {}", crate::cli::render::plural(cited.len(), "time")));
    let year = year_of(&shown).unwrap_or_default();
    json!({
        "title": i.key,
        "detail": format!("{}{shown}", if label.is_empty() { String::new() } else { format!("[{label}] ") }),
        "badge": if cited.is_empty() { "uncited" } else { "bibitem" },
        "search": format!("{} {label} {shown} {}", i.key, places(&cited)),
        "sort": { "key": i.key, "author": shown, "year": year, "title": shown, "cited": format!("{:06}", cited.len()) },
        "fields": fields,
        "uses": format!("`\\bibitem` at `{}:{}`. {}", cx.path, cx.doc.pos(i.at).line, places(&cited)),
        "sites": super::sites(cited.iter().map(|(f, l)| (f.as_str(), *l))),
        "actions": [{ "title": format!("Delete {}", i.key), "edit": edit_of(cx.path, vec![put(cx.doc, from, to, "")]), "disabled": reason, "danger": true }],
    })
}

/// The last year a text names.
fn year_of(text: &str) -> Option<String> {
    let b = text.as_bytes();
    (0..b.len().saturating_sub(3))
        .rev()
        .find(|&i| {
            b[i..i + 4].iter().all(u8::is_ascii_digit)
                && matches!(&text[i..i + 2], "15" | "16" | "17" | "18" | "19" | "20")
                && (i == 0 || !b[i - 1].is_ascii_digit())
                && b.get(i + 4).is_none_or(|c| !c.is_ascii_digit())
        })
        .map(|i| text[i..i + 4].to_string())
}

/// A `\bibitem`'s text as BibTeX fields, best effort: the parts `\newblock` (else a sentence's
/// `. `) separates are author, title and how it was published; the last year is the year.
fn guessed(text: &str) -> Vec<(&'static str, String)> {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut parts: Vec<String> = if flat.contains("\\newblock") {
        flat.split("\\newblock").map(|p| p.trim().to_string()).collect()
    } else {
        // a sentence ends at `. ` outside braces
        let (mut out, mut depth, mut from) = (Vec::new(), 0i32, 0);
        for (i, c) in flat.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                // an initial (`A. Author`) ends no sentence
                '.' if depth == 0 && flat[i + 1..].starts_with(' ') && out.len() < 2 && !initial(&flat[..i]) => {
                    out.push(flat[from..i].trim().to_string());
                    from = i + 1;
                }
                _ => {}
            }
        }
        out.push(flat[from..].trim().to_string());
        out
    };
    parts.retain(|p| !p.is_empty());
    let clean = |p: &str| p.trim().trim_end_matches(['.', ',']).trim().to_string();
    let mut out = Vec::new();
    let mut it = parts.iter();
    if parts.len() > 1 {
        out.push(("author", clean(it.next().unwrap_or(&String::new()))));
    }
    if let Some(t) = it.next() {
        out.push(("title", clean(t)));
    }
    let rest: Vec<String> = it.map(|p| clean(p)).collect();
    if !rest.is_empty() {
        out.push(("howpublished", rest.join(". ")));
    }
    if let Some(y) = year_of(&flat) {
        out.push(("year", y));
    }
    out.retain(|(_, v)| !v.is_empty());
    out
}

/// Whether `before` ends in a lone capital: an initial.
fn initial(before: &str) -> bool {
    let word = before.rsplit([' ', '~', '.']).next().unwrap_or_default();
    word.chars().count() == 1 && word.chars().all(char::is_uppercase)
}

/// "Convert to a .bib file": every `\bibitem` as a `@misc` entry of a new database, the list
/// replaced by the commands that print it (biblatex's when it is loaded).
fn convert(cx: &Cx, l: &List) -> Vec<Json> {
    let text = cx.doc.body();
    let dir = std::path::Path::new(cx.path).parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let stem = std::path::Path::new(cx.path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let name = ["refs".to_string(), format!("{stem}-refs"), format!("{stem}-bibliography")]
        .into_iter()
        .find(|n| !dir.join(format!("{n}.bib")).exists())
        .unwrap_or_else(|| "refs".into());
    let bib_path = dir.join(format!("{name}.bib")).display().to_string();
    let mut bib = String::from("% Converted from \\bibitem texts: best effort. Check each entry's type and fields.\n");
    for i in &l.items {
        let fields = guessed(&text[i.text.clone()]);
        let width = fields.iter().map(|f| f.0.len()).max().unwrap_or(0);
        bib.push_str(&format!("\n@misc{{{},\n", i.key));
        for (f, v) in fields {
            bib.push_str(&format!("  {f:width$} = {{{v}}},\n"));
        }
        bib.push_str("}\n");
    }
    let biblatex = crate::linter::shared::loaded(cx.analysis, "biblatex");
    let styled = cx.analysis.facts.occurrences.iter().any(|o| o.kind == OccKind::BibStyle);
    // through the printer: the list becomes the commands that print the database
    let begin = text.find("\\begin{document}").unwrap_or(0);
    let call = |cmd: &str, args: &[&str]| snapshot::Node::call(cmd, args, span(cx.doc, begin, begin).start);
    let print = match (biblatex, styled) {
        (true, _) => vec![call("printbibliography", &[])],
        (false, true) => vec![call("bibliography", &[&name])],
        (false, false) => vec![call("bibliographystyle", &["plain"]), call("bibliography", &[&name])],
    };
    let add = biblatex.then(|| call("addbibresource", &[&format!("{name}.bib")]));
    let how: Vec<String> =
        print.iter().chain(&add).map(|n| format!("`{}`", snapshot::canonical(n).trim_end())).collect();
    let nodes: Vec<_> = std::iter::once(snapshot::replaced(cx.doc, l.begin, l.end, &print)).chain(add).collect();
    let bib_uri = super::super::handlers::path_uri(&bib_path);
    let tex_uri = cx.uri();
    let zero = json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } });
    let edit = json!({ "documentChanges": [
        { "kind": "create", "uri": bib_uri.as_str(), "options": { "ignoreIfExists": true } },
        { "textDocument": { "uri": bib_uri.as_str(), "version": null }, "edits": [{ "range": zero, "newText": bib }] },
        { "textDocument": { "uri": tex_uri.as_str(), "version": null }, "edits": snapshot::edits(cx.analysis, cx.doc, &nodes) },
    ] });
    vec![
        json!({ "kind": "text", "markdown": format!(
            "This list is `thebibliography`, with no `.bib` file. **Convert to a .bib file** writes `{name}.bib` with one `@misc` entry per `\\bibitem` and puts {} in its place. The fields are guessed from each text (author, title, year): check them. A `[label]` is not kept. The style numbers or labels the entries.", how.join(" and ") ) }),
        json!({ "kind": "actions", "items": [{ "title": "Convert to a .bib file", "edits": edit }] }),
    ]
}

/// The linter's edits (lines and columns of characters) as a WorkspaceEdit's `changes`.
fn changes(sources: &Sources, edits: &[crate::linter::fix::Edit], into: &mut serde_json::Map<String, Json>) {
    for e in edits {
        let Some(text) = sources.get(&e.path) else { continue };
        let range = lsp_types::Range::new(to_lsp(&text, e.start), to_lsp(&text, e.end));
        let edit = json!(lsp_types::TextEdit::new(range, e.replacement.clone()));
        if let Some(a) = into.entry(path_uri(&e.path).as_str().to_string()).or_insert_with(|| json!([])).as_array_mut()
        {
            a.push(edit);
        }
    }
}

/// `nodes` printed into the database `file`, the citations of a renamed key with them.
fn print(
    cx: (&Analysis, &Sources),
    file: &str,
    nodes: &[snapshot::Node],
    into: &mut serde_json::Map<String, Json>,
) -> Result<Vec<String>, String> {
    let (analysis, sources) = cx;
    let text = sources.get(file).ok_or_else(|| format!("{file} cannot be read"))?;
    let (edits, notes) = snapshot::printed(analysis, &text, nodes)?;
    let uri = path_uri(file).as_str().to_string();
    let own = json!(snapshot::lsp_edits(&text, edits));
    match into.get_mut(&uri).and_then(Json::as_array_mut) {
        Some(a) => a.extend(own.as_array().cloned().unwrap_or_default()),
        None => {
            into.insert(uri, own);
        }
    }
    for n in nodes.iter().filter(|n| n.kind == snapshot::Kind::Entry && n.range.is_some() && !n.drop) {
        let was = snapshot::entry(&Text::new(n.text.clone()), 0).and_then(|o| o.args.first().map(|a| a.text.clone()));
        let now = n.args.first().map(|a| a.text.as_str()).unwrap_or_default();
        if let Some(was) = was.filter(|w| w != now) {
            changes(sources, &crate::linter::bibdb::recite(analysis, sources, &was, now), into);
        }
    }
    Ok(notes)
}

/// A manager's request beyond its fields (`satex/dialogApply` of kind `snapshot` with an
/// object for `nodes`): `print` snapshots into a database, `import` references, `convert` an
/// entry's type, `merge` two entries, edit a `constant`. Edits come back as a WorkspaceEdit.
pub(in crate::lsp) fn op(analysis: &Analysis, p: &Json) -> Result<Json, String> {
    let sources = Sources::default();
    let (_, dbs) = crate::linter::bibdb::databases(analysis, &sources);
    let file =
        p["file"].as_str().map(String::from).or_else(|| dbs.first().map(|d| d.path.clone())).ok_or("no database")?;
    let main = analysis.file_name(analysis.main_file).to_string();
    let entry = |key: &str| {
        dbs.iter().flat_map(|d| &d.entries).find(|e| e.key == key).ok_or_else(|| format!("no entry `{key}`"))
    };
    let mut out = serde_json::Map::new();
    let mut notes = Vec::new();
    match p["bib"].as_str().unwrap_or_default() {
        "print" => {
            let nodes: Vec<snapshot::Node> = serde_json::from_value(p["nodes"].clone()).map_err(|e| e.to_string())?;
            notes = print((analysis, &sources), &file, &nodes, &mut out)?;
        }
        "import" => return import(analysis, &dbs, &file, p),
        "convert" => {
            let e = entry(p["key"].as_str().unwrap_or_default())?;
            let to = p["to"].as_str().unwrap_or_default();
            let model = model::of(analysis, &main);
            let text = sources.get(&e.file).ok_or("the database cannot be read")?;
            let mut node = node_of(&text, e).ok_or("the entry cannot be read")?;
            notes = converted(&model, &mut node, to);
            print((analysis, &sources), &e.file, &[node], &mut out)?;
        }
        "merge" => {
            let (keep, drop) =
                (entry(p["keep"].as_str().unwrap_or_default())?, entry(p["drop"].as_str().unwrap_or_default())?);
            let edits = crate::linter::bibdb::merge(analysis, &sources, &dbs, keep, drop)
                .ok_or("the entries cannot be merged")?;
            changes(&sources, &edits, &mut out);
        }
        "constant" => notes = constant(analysis, &sources, &dbs, &file, p, &mut out)?,
        other => return Err(format!("unknown bibliography request `{other}`")),
    }
    Ok(json!({ "edits": { "changes": out }, "notes": notes }))
}

/// `node`'s type changed to `to`: each field the new type does not read renamed to its alias
/// there, or to the one field the new type requires in its place (`journal` to `booktitle`
/// when that is the only one each requires the other lacks); the others kept and named.
fn converted(model: &Model, node: &mut snapshot::Node, to: &str) -> Vec<String> {
    let from = node.name.to_ascii_lowercase();
    let takes = model.fields_of(to).to_vec();
    let need = |t: &str| model.required_of(t).iter().flatten().cloned().collect::<Vec<String>>();
    let (old, new) = (need(&from), need(to));
    let gone: Vec<&String> = old.iter().filter(|f| !new.contains(f)).collect();
    let wanted: Vec<&String> = new.iter().filter(|f| !old.contains(f)).collect();
    let mut notes = Vec::new();
    let Some(arg) = node.args.get_mut(1) else { return notes };
    let keys = arg.keys.get_or_insert_with(Vec::new);
    let present: Vec<String> = keys.iter().map(|k| k.key.clone()).collect();
    for k in keys.iter_mut() {
        if takes.is_empty() || takes.contains(&k.key) {
            continue;
        }
        let alias = model.alias(&k.key).filter(|a| takes.iter().any(|t| t == a) && !present.iter().any(|p| p == a));
        let paired = (gone.len() == 1 && wanted.len() == 1 && *gone[0] == k.key && !present.contains(wanted[0]))
            .then(|| wanted[0].as_str());
        match alias.or(paired) {
            Some(n) => {
                notes.push(format!("{} becomes {n}", k.key));
                if let Some(b) = arg.bare.iter_mut().find(|b| **b == k.key) {
                    *b = n.to_string();
                }
                k.key = n.to_string();
            }
            None => notes.push(format!("@{to} does not read {}: kept", k.key)),
        }
    }
    node.name = to.to_string();
    notes
}

/// Imported references against the databases: per reference its key, the entry it updates
/// (by key, else by DOI or title) and its fields with the values they replace; a new one
/// comes with its snapshot at the end of `file`. `into`: the entry a dblp answer updates.
fn import(analysis: &Analysis, dbs: &[Db], file: &str, p: &Json) -> Result<Json, String> {
    use crate::plugins::bib::convert;
    let mut got = convert::parse(p["text"].as_str().unwrap_or_default());
    if got.is_empty() {
        return Err("no references found: BibTeX, RIS, EndNote, CSL-JSON, arXiv's Atom or dblp's JSON".into());
    }
    let all: Vec<&Entry> = dbs.iter().flat_map(|d| &d.entries).collect();
    let base = p["into"].as_str().and_then(|k| all.iter().find(|e| e.key == k).copied());
    if let Some(b) = base {
        // the hit naming the same work, else none
        let hit = got.iter().position(|h| convert::same(h, b)).ok_or("dblp found no record of this title")?;
        got = vec![got.swap_remove(hit)];
    }
    let keep = p["keys"] == "keep";
    let main = analysis.file_name(analysis.main_file).to_string();
    let model = model::of(analysis, &main);
    let known = |w: &str| {
        dbs.iter()
            .flat_map(|d| &d.constants)
            .find(|c| c.name.eq_ignore_ascii_case(w))
            .map(|c| c.name.clone())
            .or_else(|| model.macros.iter().find(|m| m.0.eq_ignore_ascii_case(w)).map(|m| m.0.clone()))
    };
    let text = Sources::default().get(file).ok_or("the database cannot be read")?;
    let end = span(&text, text.body().len(), text.body().len()).start;
    let mut taken: Vec<String> = all.iter().map(|e| e.key.clone()).collect();
    let mut rows = Vec::new();
    for e in &got {
        let matched = base.or_else(|| {
            all.iter().find(|x| keep && x.key == e.key).or_else(|| all.iter().find(|x| convert::same(x, e))).copied()
        });
        let key = match matched {
            Some(m) => m.key.clone(),
            None if keep && !e.key.is_empty() && !taken.contains(&e.key) => e.key.clone(),
            None => convert::key_for(e, &taken),
        };
        taken.push(key.clone());
        let mut fields = Vec::new();
        let (mut keys, mut bare) = (Vec::new(), Vec::new());
        for (n, v) in &e.fields {
            // the local entry's name for it (`journaltitle` for `journal`)
            let name = matched.and_then(|m| model.alias(n).filter(|a| m.fields.iter().any(|f| f.0 == *a))).unwrap_or(n);
            let constant = e.bare.contains(n).then(|| known(v)).flatten();
            let plain = v.bytes().all(|b| b.is_ascii_digit()) && !v.is_empty();
            let value = constant.clone().unwrap_or_else(|| v.clone());
            let old = matched.and_then(|m| m.fields.iter().find(|f| f.0 == name)).map(|f| f.1.clone());
            if old.as_deref() == Some(value.as_str()) {
                continue;
            }
            if constant.is_some() || plain {
                bare.push(name.to_string());
            }
            keys.push(snapshot::Key { key: name.to_string(), value: Some(value.clone()) });
            fields.push(json!({ "name": name, "value": value, "bare": constant.is_some() || plain, "old": old }));
        }
        let node = match matched {
            Some(m) => dbs.iter().find(|d| d.path == m.file).and_then(|d| node_of(&d.text, m)),
            None => Some(snapshot::Node {
                kind: snapshot::Kind::Entry,
                name: e.kind.clone(),
                args: vec![snapshot::Arg::text('{', &key), snapshot::Arg { bare, ..snapshot::Arg::keys('{', keys) }],
                at: Some(end),
                ..snapshot::Node::default()
            }),
        };
        rows.push(json!({ "key": key, "type": e.kind, "title": e.title, "match": matched.map(|m| &m.key), "file": matched.map_or(file, |m| m.file.as_str()), "fields": fields, "node": node }));
    }
    Ok(json!({ "rows": rows }))
}

/// A `@string` constant renamed (`rename`), deleted when unused (`delete`), or made from a
/// value several entries write (`make`, its `name`): the definition and every use.
fn constant(
    analysis: &Analysis,
    sources: &Sources,
    dbs: &[Db],
    file: &str,
    p: &Json,
    out: &mut serde_json::Map<String, Json>,
) -> Result<Vec<String>, String> {
    let name = p["name"].as_str().unwrap_or_default();
    let mut notes = Vec::new();
    // per database, the entries whose values change
    let mut nodes: Vec<(String, Vec<snapshot::Node>)> = Vec::new();
    let mut edit = |db: &Db, e: &Entry, f: &dyn Fn(&mut snapshot::Arg) -> bool| {
        let Some(mut n) = node_of(&db.text, e) else { return };
        if n.args.get_mut(1).is_some_and(f) {
            match nodes.iter_mut().find(|x| x.0 == db.path) {
                Some(x) => x.1.push(n),
                None => nodes.push((db.path.clone(), vec![n])),
            }
        }
    };
    let words_of = |v: &str| crate::linter::bibdb::words(v);
    if let Some(value) = p["make"].as_str() {
        if !snapshot::bib_bare(name) || name.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("`{name}` is no constant name"));
        }
        for db in dbs {
            for e in &db.entries {
                edit(db, e, &|a: &mut snapshot::Arg| {
                    let mut hit = false;
                    for k in a.keys.iter_mut().flatten().filter(|k| k.value.as_deref() == Some(value)) {
                        k.value = Some(name.to_string());
                        if !a.bare.contains(&k.key) {
                            a.bare.push(k.key.clone());
                        }
                        hit = true;
                    }
                    hit
                });
            }
        }
        // the definition after the database's last, else first
        let db = dbs.iter().find(|d| d.path == file).ok_or("no database")?;
        let at = match db.constants.last() {
            Some(c) => db.text.offset(crate::text::Pos::new(c.line + 1, 1)).unwrap_or(0),
            None => 0,
        };
        let def = snapshot::Node {
            kind: snapshot::Kind::Entry,
            name: "string".into(),
            args: vec![
                snapshot::Arg::text('{', ""),
                snapshot::Arg::keys('{', vec![snapshot::Key { key: name.into(), value: Some(value.into()) }]),
            ],
            at: Some(span(&db.text, at, at).start),
            ..snapshot::Node::default()
        };
        match nodes.iter_mut().find(|x| x.0 == db.path) {
            Some(x) => x.1.push(def),
            None => nodes.push((db.path.clone(), vec![def])),
        }
    } else {
        let def = dbs
            .iter()
            .find_map(|d| d.constants.iter().find(|c| c.name == name).map(|c| (d, c)))
            .ok_or_else(|| format!("no constant `{name}`"))?;
        let used = |e: &Entry| {
            e.fields
                .iter()
                .any(|f| e.bare.contains(&f.0) && words_of(&f.1).iter().any(|w| w.eq_ignore_ascii_case(name)))
        };
        let users: Vec<&str> =
            dbs.iter().flat_map(|d| &d.entries).filter(|e| used(e)).map(|e| e.key.as_str()).collect();
        let to = p["rename"].as_str();
        if to.is_none() && !users.is_empty() {
            return Err(format!("`{name}` is used by {}", users.join(", ")));
        }
        // the definition's name, or its lines
        let (d, c) = def;
        let line = d.text.offset(crate::text::Pos::new(c.line, 1)).unwrap_or(0);
        let open = line + d.text.body()[line..].find(['{', '(']).unwrap_or(0) + 1;
        let from = open + d.text.body()[open..].find(name).unwrap_or(0);
        let edit_def = match to {
            Some(to) => put(&d.text, from, from + name.len(), to),
            None => {
                let close = d.text.body()[open..].find('\n').map_or(d.text.body().len(), |i| open + i + 1);
                put(&d.text, line, close, "")
            }
        };
        if let Some(a) = out.entry(path_uri(&d.path).as_str().to_string()).or_insert_with(|| json!([])).as_array_mut() {
            a.push(json!(edit_def));
        }
        if let Some(to) = to {
            if !snapshot::bib_bare(to) {
                return Err(format!("`{to}` is no constant name"));
            }
            for db in dbs {
                for e in db.entries.iter().filter(|e| used(e)) {
                    edit(db, e, &|a: &mut snapshot::Arg| {
                        let bare = a.bare.clone();
                        for k in a.keys.iter_mut().flatten().filter(|k| bare.contains(&k.key)) {
                            let v = k.value.clone().unwrap_or_default();
                            let parts: Vec<String> = v
                                .split('#')
                                .map(|p| {
                                    if p.trim().eq_ignore_ascii_case(name) {
                                        p.replace(p.trim(), to)
                                    } else {
                                        p.to_string()
                                    }
                                })
                                .collect();
                            k.value = Some(parts.join("#"));
                        }
                        true
                    });
                }
            }
            notes.push(format!("{} uses renamed", users.len()));
        }
    }
    for (path, ns) in nodes {
        notes.extend(print((analysis, sources), &path, &ns, out)?);
    }
    Ok(notes)
}
