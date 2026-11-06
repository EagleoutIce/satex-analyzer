//! Bibliography entries: every entry of the databases the run read, and every `\bibitem` of a
//! `thebibliography` in the open file, searchable by key, field, text and where cited. Fields
//! are edited, added and emptied away in the `.bib` file, a new entry has the fields the
//! document's style reads of its type, an uncited entry can be deleted; a `thebibliography`
//! edits in place and converts to a `.bib` file.

use super::{Cx, Hit, TypePlugin, call_at, closing, edit_of, key_values, lines_of, put, unbraced};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
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
        "manage .bib entries or thebibliography's items: search, edit and add fields, add per type, delete uncited, convert to .bib"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Cite)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view", "manage", "lens"]
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
        let a = cx.analysis;
        let occ = &a.facts.occurrences;
        let mut files: Vec<(String, Text)> = Vec::new();
        for o in occ.iter().filter(|o| o.kind == OccKind::Bibliography) {
            let Some(path) = crate::plugins::bib::resolve(a, &o.key) else { continue };
            let path = path.display().to_string();
            if files.iter().any(|f| f.0 == path) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            files.push((path, Text::new(text)));
        }
        let list = thebibliography(cx);
        let mut items = Vec::new();
        let mut seen_types: Vec<(String, Vec<String>)> = Vec::new();
        let mut kinds = crate::lsp::bib::type_fields(a, cx.path);
        for (path, bib) in &files {
            for e in crate::plugins::bib::entries(bib.body()) {
                match seen_types.iter_mut().find(|t| t.0 == e.kind) {
                    Some(t) => t
                        .1
                        .extend(e.fields.iter().map(|f| f.0.clone()).filter(|f| !t.1.contains(f)).collect::<Vec<_>>()),
                    None => seen_types.push((e.kind.clone(), e.fields.iter().map(|f| f.0.clone()).collect())),
                }
                let style = kinds.iter().find(|k| k.0 == e.kind).map(|k| k.1.as_slice()).unwrap_or_default();
                items.push(record(path, bib, &e, &cites(cx, &e.key), style));
            }
        }
        if let Some(l) = &list {
            items.extend(l.items.iter().map(|i| bibitem(cx, l, i)));
        }
        if items.is_empty() && list.is_none() {
            return None;
        }
        // a new entry: the types and fields the style reads, else those the databases use
        for (t, fs) in seen_types {
            if !kinds.iter().any(|k| k.0 == t) {
                kinds.push((t, fs));
            }
        }
        let taken: Vec<String> = items.iter().filter_map(|r| r["title"].as_str().map(String::from)).collect();
        let key = || json!({ "name": "citekey", "value": "", "placeholder": "citation key", "check": ["required", "bibkey", "unique"], "taken": taken });
        let mut add: Vec<Json> = Vec::new();
        if let Some(l) = &list {
            // before `\end{thebibliography}`, on a line of its own
            let at = l.end_tag;
            let lead = if cx.doc.body()[..at].ends_with('\n') { "" } else { "\n" };
            let text = format!("{lead}\\bibitem${{?label}}[${{label}}]${{/label}}{{${{citekey}}}} ${{text}}\n");
            add.push(json!({ "name": "\\bibitem", "fields": [key(),
                { "name": "label", "value": "", "placeholder": "label printed instead of the number (optional)", "check": ["balanced"] },
                { "name": "text", "value": "", "placeholder": "Author. Title. Publisher, year.", "long": true, "check": ["required", "tex"] }],
                "edit": edit_of(cx.path, vec![put(cx.doc, at, at, text)]) }));
        }
        if let Some((path, bib)) = files.first() {
            let end = bib.body().len();
            let sep = if bib.body().ends_with('\n') || bib.body().is_empty() { "\n" } else { "\n\n" };
            add.extend(kinds.iter().map(|(t, fs)| {
                // a field left empty is not written
                let body: String = fs.iter().map(|f| format!("${{?{f}}},\n  {f} = {{${{{f}}}}}${{/{f}}}")).collect();
                let mut form = vec![key()];
                form.extend(fs.iter().map(|f| json!({ "name": f, "value": "", "placeholder": f, "check": ["balanced"] })));
                json!({ "name": t, "fields": form, "edit": edit_of(path, vec![put(bib, end, end, format!("{sep}@{t}{{${{citekey}}{body}\n}}\n"))]) })
            }));
        }
        let mut sections = vec![
            json!({ "kind": "records", "placeholder": "search keys, fields, text, where cited", "sorts": ["key", "author", "year", "title", "cited"],
                                         "items": items, "add": { "title": "Add entry", "kinds": add } }),
        ];
        if let Some(l) = &list {
            sections.extend(convert(cx, l));
        }
        Some(json!({ "title": "Bibliography", "sections": sections }))
    }
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
    let print = match (biblatex, styled) {
        (true, _) => "\\printbibliography".to_string(),
        (false, true) => format!("\\bibliography{{{name}}}"),
        (false, false) => format!("\\bibliographystyle{{plain}}\n\\bibliography{{{name}}}"),
    };
    let mut tex = vec![put(cx.doc, l.begin, l.end, print)];
    if biblatex {
        let begin = text.find("\\begin{document}").unwrap_or(0);
        tex.push(put(cx.doc, begin, begin, format!("\\addbibresource{{{name}.bib}}\n")));
    }
    let bib_uri = super::super::handlers::path_uri(&bib_path);
    let tex_uri = cx.uri();
    let zero = json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } });
    let edit = json!({ "documentChanges": [
        { "kind": "create", "uri": bib_uri.as_str(), "options": { "ignoreIfExists": true } },
        { "textDocument": { "uri": bib_uri.as_str(), "version": null }, "edits": [{ "range": zero, "newText": bib }] },
        { "textDocument": { "uri": tex_uri.as_str(), "version": null }, "edits": tex },
    ] });
    let how = if biblatex {
        format!("`\\printbibliography` and `\\addbibresource{{{name}.bib}}`")
    } else {
        format!("`\\bibliography{{{name}}}`")
    };
    vec![
        json!({ "kind": "text", "markdown": format!(
            "This list is `thebibliography`, with no `.bib` file. **Convert to a .bib file** writes `{name}.bib` with one `@misc` entry per `\\bibitem` and puts {how} in its place. The fields are guessed from each text (author, title, year): check them. A `[label]` is not kept. The style numbers or labels the entries." ) }),
        json!({ "kind": "actions", "items": [{ "title": "Convert to a .bib file", "edits": edit }] }),
    ]
}

fn record(path: &str, bib: &Text, e: &crate::plugins::bib::Entry, cites: &[(String, u32)], style: &[String]) -> Json {
    let text = bib.body();
    // the entry's bytes: from its `@` to the brace closing it
    let start = bib.offset(crate::text::Pos::new(e.line, 1)).unwrap_or(0);
    let at = text[start..].find('@').map(|i| start + i);
    let open = at.and_then(|at| text[at..].find('{').map(|i| at + i));
    let close = open.and_then(|o| closing(text, o, '{'));
    let mut fields = Vec::new();
    let mut actions = Vec::new();
    let mut more = Json::Null;
    if let (Some(at), Some(open), Some(close)) = (at, open, close) {
        let comma = text[open..close].find(',').map_or(close, |i| open + i);
        let list = comma + 1..close;
        let kv = key_values(text, list.clone());
        for (name, r) in &kv {
            let written = &text[r.clone()];
            let value = unbraced(written.trim_matches('"'));
            // a number written bare stays bare
            let bare = written.bytes().all(|c| c.is_ascii_digit());
            let (template, check) =
                if bare { ("${value}", json!(["integer"])) } else { ("{${value}}", json!(["balanced"])) };
            let (from, to) = super::item_cut(text, list.clone(), r.clone());
            fields.push(json!({ "name": name, "value": value, "long": value.len() > 40, "check": check,
                                "edit": edit_of(path, vec![put(bib, r.start, r.end, template)]),
                                "remove": edit_of(path, vec![put(bib, from, to, "")]) }));
        }
        // a new field after the last, in the file's indent
        let after = kv.last().map_or(comma + 1, |(_, r)| r.end);
        let rest = &text[after..close];
        let (at_new, text_new) = match rest.trim_start().starts_with(',') {
            true => (after + rest.find(',').unwrap_or(0) + 1, "\n  ${name} = {${value}},"),
            false => (after, ",\n  ${name} = {${value}}"),
        };
        let present: Vec<&str> = kv.iter().map(|(n, _)| n.as_str()).collect();
        let names: Vec<&String> = style.iter().filter(|f| !present.contains(&f.as_str())).collect();
        more = json!({ "title": "Add field", "names": names, "check": "balanced", "edit": edit_of(path, vec![put(bib, at_new, at_new, text_new)]) });
        let (from, to) = lines_of(text, at, close + 1);
        let reason = (!cites.is_empty()).then(|| format!("cited {}", crate::cli::render::plural(cites.len(), "time")));
        actions.push(json!({ "title": format!("Delete {}", e.key), "edit": edit_of(path, vec![put(bib, from, to, "")]), "disabled": reason, "danger": true }));
    }
    let file = std::path::Path::new(path).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let all: Vec<&str> = e.fields.iter().map(|f| f.1.as_str()).collect();
    json!({
        "title": e.key,
        "detail": format!("{} · {} · {}", e.author, e.year, e.kind),
        "badge": cites.is_empty().then_some("uncited"),
        "search": format!("{} {} {} {}", e.key, e.kind, all.join(" "), places(cites)),
        "sort": { "key": e.key, "author": e.author, "year": e.year, "title": e.title, "cited": format!("{:06}", cites.len()) },
        "fields": fields,
        "more": more,
        "uses": format!("In `{path}:{}`. {}", e.line, places(cites)),
        "actions": actions,
        "file": format!("{file}:{}", e.line),
    })
}
