//! Index terms: every term a written index line carried in the run (`\index{graph!directed}`
//! writes `\indexentry`), one record per entry path (a term, a subentry below it), sorted as
//! makeindex sorts. Each edits its level's text and sort key in every call below it, the page
//! format or cross reference of its own calls, deletes them; a new call goes at the cursor.

use super::{Cx, Hit, TypePlugin, call_at, edit_of, put};
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::project::config::TypePlugins;
use serde_json::{Value as Json, json};
use std::ops::Range;

pub(super) struct Index;

impl TypePlugin for Index {
    fn name(&self) -> &'static str {
        "index"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "manage index terms: edit levels, sort keys, page formats and see, add, delete, jump to uses"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Entry)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view", "manage", "lens", "hover"]
    }
    fn opened_by(&self) -> Option<(&'static [&'static str], &'static str)> {
        Some((&["index", "makeindex", "printindex"], "Manage index…"))
    }
    fn entry(&self, cx: &Cx, _hit: &Hit) -> Option<String> {
        self.manage(cx).map(|_| super::open_link(cx, "Manage index…", self.name()))
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.index.enabled
    }
    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let mut v = self.manage(cx)?;
        let typed = cx.at.item.typed.trim();
        let key = parse(typed);
        v["sections"][0]["focus"] = json!(key.levels.iter().map(|l| l.shown(typed)).collect::<Vec<_>>().join(" › "));
        Some(v)
    }
    fn manage(&self, cx: &Cx) -> Option<Json> {
        let uses = uses(cx);
        if uses.is_empty() {
            return None;
        }
        // every path a use names, its parents too
        let mut paths: Vec<Vec<String>> = Vec::new();
        for u in &uses {
            for n in 1..=u.key.levels.len() {
                let p = u.path(n);
                if !paths.contains(&p) {
                    paths.push(p);
                }
            }
        }
        paths.sort_by_key(|p| p.iter().map(|l| sort_of(l).to_lowercase()).collect::<Vec<_>>());
        let items: Vec<Json> = paths.iter().map(|p| record(cx, &uses, p)).collect();
        // a new call: like the document's own, at the cursor
        let head = uses.iter().find_map(|u| u.head.clone()).unwrap_or_else(|| "index".into());
        let level = |n: &str, more: bool| json!({ "name": n, "value": "", "placeholder": n, "check": ["index", "balanced"], "more": more });
        let term =
            json!({ "name": "entry", "value": "", "placeholder": "term", "check": ["required", "index", "balanced"] });
        let page = format!(
            "\\{head}{{${{?sort}}${{sort}}@${{/sort}}${{entry}}${{?subentry}}!${{subentry}}${{/subentry}}${{?format}}|${{format}}${{/format}}}}"
        );
        let see = format!("\\{head}{{${{entry}}|${{how}}{{${{target}}}}}}");
        let add = json!({ "title": "Add term", "kinds": [
            { "name": "page", "insert": page, "fields": [term, level("subentry", false), level("sort", true), level("format", true)] },
            { "name": "cross reference", "insert": see, "fields": [term, { "name": "how", "value": "see", "choices": ["see", "seealso"] },
              { "name": "target", "value": "", "placeholder": "term it points to", "check": ["required", "balanced"] }] },
        ] });
        Some(json!({
            "title": "Index terms",
            "sections": [{ "kind": "records", "placeholder": "search terms, subentries, see, files", "sorts": ["term", "uses"],
                           "items": items, "add": add }],
        }))
    }
}

/// One level of an index key: all of it and its text after a sort key's `@`, as byte ranges.
#[derive(Clone)]
struct Level {
    whole: Range<usize>,
    text: Range<usize>,
}

impl Level {
    fn shown<'a>(&self, key: &'a str) -> &'a str {
        &key[self.text.clone()]
    }
}

/// A key: its levels and the page's encapsulator after `|`.
struct Key {
    levels: Vec<Level>,
    encap: Option<Range<usize>>,
}

/// makeindex's reading of a key: `!` separates levels, `@` a sort key, `|` starts the
/// encapsulator; `"` quotes the next character, braces hide all three.
fn parse(key: &str) -> Key {
    let (mut levels, mut start, mut at, mut depth, mut quoted) = (Vec::new(), 0, None, 0i32, false);
    let mut encap = None;
    let level = |start: usize, at: Option<usize>, end: usize| Level {
        whole: start..end,
        text: at.map_or(start, |a| a + 1)..end,
    };
    for (i, c) in key.char_indices() {
        match c {
            _ if quoted => quoted = false,
            '"' => quoted = true,
            '{' => depth += 1,
            '}' => depth -= 1,
            '!' if depth == 0 => {
                levels.push(level(start, at.take(), i));
                start = i + 1;
            }
            '@' if depth == 0 && at.is_none() => at = Some(i),
            '|' if depth == 0 => {
                encap = Some(i + 1..key.len());
                levels.push(level(start, at.take(), i));
                return Key { levels, encap };
            }
            _ => {}
        }
    }
    levels.push(level(start, at, key.len()));
    Key { levels, encap }
}

/// A level as makeindex sorts it: its sort key, else its text.
fn sort_of(level: &str) -> &str {
    level.split_once('@').map_or(level, |(s, _)| s)
}

/// An index call: its key, where the key's bytes stand in the open file, its command and
/// line, and its file.
struct Use {
    raw: String,
    key: Key,
    at: Option<usize>,
    call: Option<Range<usize>>,
    head: Option<String>,
    line: u32,
    file: String,
}

impl Use {
    /// Its first `n` levels as written.
    fn path(&self, n: usize) -> Vec<String> {
        self.key.levels[..n].iter().map(|l| self.raw[l.whole.clone()].to_string()).collect()
    }
}

/// Every index call of the run, read from the open file's text where it stands there.
fn uses(cx: &Cx) -> Vec<Use> {
    let occ = &cx.analysis.facts.occurrences;
    let text = cx.doc.body();
    let mut out: Vec<Use> = Vec::new();
    for o in occ.iter().filter(|o| o.kind == OccKind::Entry) {
        let here = super::byte_of(cx, o.span).and_then(|at| Some((at, call_at(text, at)?)));
        let group = here.as_ref().and_then(|(at, (name, g, end))| {
            let g = g.iter().find(|g| g.open == '{')?;
            Some((g.inner.clone(), *at..*end, text[*at + 1..*name].to_string()))
        });
        let (raw, at, call, head) = match group {
            Some((g, call, head)) => (text[g.clone()].to_string(), Some(g.start), Some(call), Some(head)),
            None => (o.key.clone(), None, None, None),
        };
        let file = cx.analysis.files[usize::from(o.span.file)].path.clone();
        if out.iter().any(|u| u.line == o.span.line && u.file == file && u.at == at && u.raw == raw) {
            continue;
        }
        out.push(Use { key: parse(&raw), raw, at, call, head, line: o.span.line, file });
    }
    out
}

fn record(cx: &Cx, uses: &[Use], path: &[String]) -> Json {
    let n = path.len();
    let below: Vec<&Use> = uses.iter().filter(|u| u.key.levels.len() >= n && u.path(n) == path).collect();
    let own: Vec<&Use> = below.iter().copied().filter(|u| u.key.levels.len() == n).collect();
    let here = |u: &&Use| u.at.is_some();
    let last = &path[n - 1];
    let (sort, text) = last.split_once('@').map_or(("", last.as_str()), |(s, t)| (s, t));
    let mut fields = vec![
        json!({ "name": "entry", "value": text, "check": ["required", "index", "balanced"] }),
        json!({ "name": "sort", "value": sort, "placeholder": "sort key (optional)", "check": ["index", "balanced"] }),
    ];
    // the level in every call below it: sort key and text
    let mut edits: Vec<_> = below
        .iter()
        .filter(|u| here(u))
        .map(|u| {
            let l = &u.key.levels[n - 1];
            let at = u.at.unwrap_or_default();
            put(cx.doc, at + l.whole.start, at + l.whole.end, "${?sort}${sort}@${/sort}${entry}")
        })
        .collect();
    // its own calls' page format and cross reference: when they agree
    let encaps: Vec<(&Use, &str)> =
        own.iter().map(|u| (*u, u.key.encap.as_ref().map_or("", |e| &u.raw[e.clone()]))).collect();
    let see =
        |e: &str, how: &str| e.strip_prefix(how).and_then(|r| r.strip_prefix('{')?.strip_suffix('}')).map(String::from);
    let pages: Vec<&(&Use, &str)> =
        encaps.iter().filter(|(_, e)| see(e, "see").is_none() && see(e, "seealso").is_none()).collect();
    let format = |e: &str| e.trim_start_matches(['(', ')']).to_string();
    let formats: Vec<String> = pages.iter().filter(|(_, e)| !e.starts_with(')')).map(|(_, e)| format(e)).collect();
    if !pages.is_empty() && formats.windows(2).all(|w| w[0] == w[1]) && pages.iter().all(|(u, _)| here(u)) {
        fields.push(json!({ "name": "format", "value": formats.first().cloned().unwrap_or_default(), "placeholder": "page format, as textbf (optional)", "check": ["index", "balanced"] }));
        for (u, e) in &pages {
            let at = u.at.unwrap_or_default();
            let end = at + u.raw.len();
            match u.key.encap.clone() {
                // a range keeps its parenthesis; its end carries no format
                Some(r) if e.starts_with('(') => edits.push(put(cx.doc, at + r.start + 1, end, "${format}")),
                Some(_) if e.starts_with(')') => {}
                Some(r) => edits.push(put(cx.doc, at + r.start - 1, end, "${?format}|${format}${/format}")),
                None => edits.push(put(cx.doc, end, end, "${?format}|${format}${/format}")),
            }
        }
    }
    for how in ["see", "seealso"] {
        let refs: Vec<(&Use, String)> = encaps.iter().filter_map(|(u, e)| Some((*u, see(e, how)?))).collect();
        if refs.is_empty() || !refs.windows(2).all(|w| w[0].1 == w[1].1) || !refs.iter().all(|(u, _)| here(u)) {
            continue;
        }
        fields.push(json!({ "name": how, "value": refs[0].1, "check": ["required", "balanced"] }));
        for (u, _) in &refs {
            let r = u.key.encap.clone().unwrap_or_default();
            let at = u.at.unwrap_or_default();
            edits.push(put(cx.doc, at + r.start + how.len() + 1, at + r.end - 1, format!("${{{how}}}")));
        }
    }
    let editable = below.iter().all(here);
    let mut forms = Vec::new();
    if editable {
        let what = if n > 1 { "subentry" } else { "term" };
        forms.push(json!({ "title": "Save", "fields": fields, "edit": edit_of(cx.path, edits),
                           "note": format!("Writes the {what} in its {} below it.", crate::cli::render::plural(below.len(), "call")) }));
    }
    let mut actions = Vec::new();
    if !own.is_empty() && own.iter().all(|u| u.call.is_some()) {
        let cuts = own.iter().filter_map(|u| u.call.clone()).map(|c| put(cx.doc, c.start, c.end, "")).collect();
        let title = format!("Delete {}", crate::cli::render::plural(own.len(), "call"));
        let sub = (below.len() > own.len()).then(|| format!("its subentries keep {} calls", below.len() - own.len()));
        actions.push(json!({ "title": title, "edit": edit_of(cx.path, cuts), "danger": true, "note": sub }));
    }
    let places: Vec<String> = below.iter().map(|u| format!("`{}:{}`", u.file, u.line)).fold(Vec::new(), |mut v, p| {
        if !v.contains(&p) {
            v.push(p);
        }
        v
    });
    let rows: Vec<Json> = below
        .iter()
        .map(|u| {
            let rest = u.path(u.key.levels.len())[n..].join(" › ");
            let e = u.key.encap.as_ref().map_or("", |e| &u.raw[e.clone()]);
            json!([
                if rest.is_empty() { "(this entry)".to_string() } else { rest },
                e,
                format!("{}:{}", u.file.rsplit('/').next().unwrap_or_default(), u.line)
            ])
        })
        .collect();
    let shown: Vec<&str> = path.iter().map(|l| l.split_once('@').map_or(l.as_str(), |(_, t)| t)).collect();
    let title = shown.join(" › ");
    let subs = below.len() - own.len();
    json!({
        "title": title,
        "detail": format!("{}{}", crate::cli::render::plural(own.len(), "use"), if subs > 0 { format!(", {subs} below") } else { String::new() }),
        "search": format!("{} {} {}", path.join(" "), below.iter().map(|u| u.raw.as_str()).collect::<Vec<_>>().join(" "), places.join(" ")),
        "sort": { "term": path.iter().map(|l| sort_of(l).to_lowercase()).collect::<Vec<_>>().join("\u{1}"), "uses": format!("{:06}", 999_999 - below.len()) },
        "forms": forms,
        "uses": format!("Used at {}", places.join(" · ")),
        "why": (!editable).then_some("Some calls stand in another file: open it to edit them."),
        "table": { "columns": ["below", "format", "where"], "rows": rows },
        "actions": actions,
    })
}
