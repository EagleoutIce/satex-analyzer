//! Bibliography entries: every entry of the databases the run read, searchable by key, author,
//! title and year, with where each is cited. Fields are edited in the `.bib` file, a new entry
//! has the fields the document's style reads of its type, an uncited entry can be deleted.

use super::{Cx, Hit, TypePlugin, closing, edit_of, key_values, lines_of, put, unbraced};
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
        "manage .bib entries: search, edit fields, add per entry type, delete uncited, cited where"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Cite)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view", "manage"]
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
        let first = files.first()?;
        let mut items = Vec::new();
        let mut seen_types: Vec<(String, Vec<String>)> = Vec::new();
        for (path, bib) in &files {
            for e in crate::plugins::bib::entries(bib.body()) {
                let cites: Vec<u32> =
                    occ.iter().filter(|o| o.kind == OccKind::Cite && o.key == e.key).map(|o| o.span.line).collect();
                match seen_types.iter_mut().find(|t| t.0 == e.kind) {
                    Some(t) => t
                        .1
                        .extend(e.fields.iter().map(|f| f.0.clone()).filter(|f| !t.1.contains(f)).collect::<Vec<_>>()),
                    None => seen_types.push((e.kind.clone(), e.fields.iter().map(|f| f.0.clone()).collect())),
                }
                items.push(record(path, bib, &e, &cites));
            }
        }
        // a new entry: the types and fields the style reads, else those the databases use
        let mut kinds = crate::lsp::bib::type_fields(a, cx.path);
        for (t, fs) in seen_types {
            if !kinds.iter().any(|k| k.0 == t) {
                kinds.push((t, fs));
            }
        }
        let taken: Vec<&str> = items.iter().filter_map(|r| r["title"].as_str()).collect();
        let (path, bib) = first;
        let end = bib.body().len();
        let sep = if bib.body().ends_with('\n') || bib.body().is_empty() { "\n" } else { "\n\n" };
        let add: Vec<Json> = kinds
            .iter()
            .map(|(t, fs)| {
                let body: String = fs.iter().map(|f| format!(",\n  {f} = {{${{{f}}}}}")).collect();
                let mut form = vec![json!({ "name": "citekey", "value": "", "placeholder": "citation key",
                                            "check": ["required", "bibkey", "unique"], "taken": taken })];
                form.extend(fs.iter().map(|f| json!({ "name": f, "value": "", "placeholder": f, "check": ["balanced"] })));
                json!({ "name": t, "fields": form, "edit": edit_of(path, vec![put(bib, end, end, format!("{sep}@{t}{{${{citekey}}{body}\n}}\n"))]) })
            })
            .collect();
        Some(json!({
            "title": "Bibliography",
            "sections": [{ "kind": "records", "placeholder": "search key, author, title, year", "sorts": ["key", "author", "year", "title", "cited"],
                           "items": items, "add": { "title": "Add entry", "kinds": add } }],
        }))
    }
}

fn record(path: &str, bib: &Text, e: &crate::plugins::bib::Entry, cites: &[u32]) -> Json {
    let text = bib.body();
    // the entry's bytes: from its `@` to the brace closing it
    let start = bib.offset(crate::text::Pos::new(e.line, 1)).unwrap_or(0);
    let at = text[start..].find('@').map(|i| start + i);
    let open = at.and_then(|at| text[at..].find('{').map(|i| at + i));
    let close = open.and_then(|o| closing(text, o, '{'));
    let mut fields = Vec::new();
    let mut actions = Vec::new();
    if let (Some(at), Some(open), Some(close)) = (at, open, close) {
        let comma = text[open..close].find(',').map_or(close, |i| open + i);
        for (name, r) in key_values(text, comma + 1..close) {
            let value = unbraced(text[r.clone()].trim_matches('"'));
            fields.push(json!({ "name": name, "value": value, "long": value.len() > 40, "check": ["balanced"],
                                "edit": edit_of(path, vec![put(bib, r.start, r.end, "{${value}}")]) }));
        }
        let (from, to) = lines_of(text, at, close + 1);
        let reason = (!cites.is_empty()).then(|| format!("cited {}", crate::cli::render::plural(cites.len(), "time")));
        actions.push(json!({ "title": format!("Delete {}", e.key), "edit": edit_of(path, vec![put(bib, from, to, "")]), "disabled": reason, "danger": true }));
    }
    let file = std::path::Path::new(path).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let rows: Vec<Json> = cites.iter().map(|l| json!(["cited", format!("line {l}")])).collect();
    json!({
        "title": e.key,
        "detail": format!("{} · {} · {}", e.author, e.year, e.kind),
        "badge": cites.is_empty().then_some("uncited"),
        "search": format!("{} {} {} {} {}", e.key, e.author, e.title, e.year, e.kind),
        "sort": { "key": e.key, "author": e.author, "year": e.year, "title": e.title, "cited": format!("{:06}", cites.len()) },
        "fields": fields,
        "table": { "columns": ["", "where"], "rows": rows },
        "actions": actions,
        "file": format!("{file}:{}", e.line),
    })
}
