//! Snapshots: a source region read losslessly into calls, key lists, environments and text, and
//! the one printer every widget writes through. A view edits a snapshot's values; the printer
//! turns it into minimal edits: untouched bytes stay, a key list keeps its order and style, new
//! nodes print in one canonical form, and the packages they need are loaded.

use lsp_types::{Position, Range};
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

use super::handlers::to_pos;
use super::plugins::{call_at, ensure_package, item_cut, lines_of, span, unbraced};
use crate::analysis::machine::Analysis;
use crate::text::Text;

/// A key list's item: `key=value`, or a bare `key` (value `None`). The value is unbraced.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Key {
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
}

/// An argument: its delimiter, its inner bytes in the node's `text` (none: not written), and
/// its content, a key list or text.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Arg {
    pub open: char,
    #[serde(default)]
    pub inner: Option<(usize, usize)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<Key>>,
    #[serde(default)]
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Call,
    Env,
    Text,
}

/// The package a new node needs when the run does not define it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Package {
    pub name: String,
    #[serde(default)]
    pub options: String,
}

/// A call, an environment or text as written (`range`, `text`), or new (`at`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Node {
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub star: bool,
    #[serde(default)]
    pub args: Vec<Arg>,
    /// An environment's body, a text node's text.
    #[serde(default)]
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_at: Option<(usize, usize)>,
    #[serde(default)]
    pub range: Option<Range>,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Position>,
    /// A new node ends its line.
    #[serde(default)]
    pub line: bool,
    #[serde(default)]
    pub drop: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<Package>,
}

impl Node {
    /// A new call at `at`, on a line of its own.
    pub fn new_call(name: &str, args: Vec<Arg>, at: Position) -> Node {
        Node { name: name.into(), args, at: Some(at), line: true, ..Node::default() }
    }
}

impl Arg {
    pub fn keys(open: char, keys: Vec<Key>) -> Arg {
        Arg { open, inner: None, keys: Some(keys), text: String::new() }
    }
    pub fn text(open: char, text: &str) -> Arg {
        Arg { open, inner: None, keys: None, text: text.into() }
    }
    fn empty(&self) -> bool {
        self.keys.as_ref().map_or(self.text.is_empty(), Vec::is_empty)
    }
}

/// An item of a key list as written: its bytes (comments and spaces around left out), its key's end, its value's bytes.
struct Item {
    from: usize,
    to: usize,
    key_end: usize,
    value: Option<(usize, usize)>,
}

/// The items of the key list `text[list]`: split at top-level commas, comments skipped.
fn items(text: &str, list: std::ops::Range<usize>) -> Vec<Item> {
    let trim = |s: usize, e: usize| {
        let v = &text[s..e];
        (s + v.len() - v.trim_start().len(), s + v.trim_end().len())
    };
    let (mut out, mut depth, mut i) = (Vec::new(), 0i32, list.start);
    // the item's first code byte, the end of its last, its first top-level `=`
    let (mut first, mut end, mut eq): (Option<usize>, usize, Option<usize>) = (None, 0, None);
    loop {
        let c = text.as_bytes().get(i).copied().filter(|_| i < list.end);
        if c.is_none() || (c == Some(b',') && depth == 0) {
            if let Some(from) = first.take() {
                let key_end = trim(from, eq.unwrap_or(end)).1;
                out.push(Item { from, to: end, key_end, value: eq.map(|q| trim(q + 1, end)) });
            }
            eq = None;
            if c.is_none() {
                return out;
            }
            i += 1;
            continue;
        }
        let c = c.unwrap_or_default();
        match c {
            b'%' => {
                i = text[i..list.end].find('\n').map_or(list.end, |k| i + k);
                continue;
            }
            _ if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'=' if depth == 0 && eq.is_none() => eq = Some(i),
            _ => {}
        }
        first.get_or_insert(i);
        let len = |at: usize| text[at..].chars().next().map_or(1, char::len_utf8);
        i += if c == b'\\' && i + 1 < list.end { 1 + len(i + 1) } else { len(i) };
        end = i;
    }
}

/// The `key=value` items of a key list written in `text[range]`: each key and the bytes of its
/// value as written (braces kept), top-level commas only.
pub(super) fn key_values(text: &str, range: std::ops::Range<usize>) -> Vec<(String, std::ops::Range<usize>)> {
    items(text, range)
        .into_iter()
        .filter_map(|i| Some((text[i.from..i.key_end].to_string(), i.value.map(|(s, e)| s..e)?)))
        .filter(|(k, _)| !k.is_empty() && !k.contains(['{', '}', '\\']))
        .collect()
}

pub(super) fn read_keys(text: &str, from: usize, to: usize) -> Vec<Key> {
    let value = |(s, e): (usize, usize)| unbraced(&text[s..e]).to_string();
    items(text, from..to)
        .into_iter()
        .map(|i| Key { key: text[i.from..i.key_end].to_string(), value: i.value.map(value) })
        .collect()
}

/// The call or environment at byte `at` of `doc`, read with the arguments of index in `keyed`
/// as key lists; `options` gives a call an unwritten `[…]` first when it has none.
pub fn read(doc: &Text, at: usize, keyed: &[usize], options: bool) -> Option<Node> {
    let text = doc.body();
    let (env, end) = match text[at..].strip_prefix("\\begin{") {
        Some(rest) => {
            let name = &rest[..rest.find('}')?];
            (Some(name.to_string()), super::tabular::extent(text, at, name)?.1)
        }
        None => (None, call_at(text, at)?.2),
    };
    let (name_end, groups, args_end) = call_at(text, at)?;
    let mut node = Node {
        range: Some(span(doc, at, end)),
        text: text[at..end].to_string(),
        name: text[at + 1..name_end].trim_end_matches('*').to_string(),
        star: text[..name_end].ends_with('*'),
        ..Node::default()
    };
    let mut groups = groups.into_iter();
    if let Some(name) = env {
        node.kind = Kind::Env;
        node.name = name;
        groups.next();
        let body_start = args_end.max(at + node.name.len() + 8);
        let body_end = end - node.name.len() - 6;
        node.body_at = Some((body_start - at, body_end - at));
        node.body = text[body_start..body_end].to_string();
    }
    for (k, g) in groups.enumerate() {
        let inner = (g.inner.start - at, g.inner.end - at);
        let keys = keyed.contains(&k).then(|| read_keys(text, g.inner.start, g.inner.end));
        node.args.push(Arg { open: g.open, inner: Some(inner), keys, text: text[g.inner.clone()].to_string() });
    }
    if options && node.args.first().is_none_or(|a| a.open != '[') {
        node.args.insert(0, Arg::keys('[', Vec::new()));
    }
    Some(node)
}

/// The environment at byte `at` with the body a plugin found at `body`: no argument read past it.
pub fn env(doc: &Text, at: usize, body: std::ops::Range<usize>) -> Option<Node> {
    let mut n = read(doc, at, &[], false)?;
    n.args.retain(|a| a.inner.is_some_and(|(_, e)| at + e < body.start));
    n.body_at = Some((body.start - at, body.end - at));
    n.body = doc.body().get(body)?.to_string();
    Some(n)
}

/// A new environment at `at` that loads `package` when the run lacks it.
pub fn new_env(name: &str, at: Position, package: &str) -> Node {
    let package = Some(Package { name: package.into(), options: String::new() });
    Node { kind: Kind::Env, name: name.into(), at: Some(at), package, ..Node::default() }
}

fn close(open: char) -> char {
    if open == '[' { ']' } else { '}' }
}

/// A value as a key list writes it: braced when it would split the list or lose its spaces.
fn value_text(v: &str, open: char) -> String {
    let mut depth = 0;
    let splits = v.bytes().any(|c| {
        match c {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        depth == 0 && (c == b',' || c == b'=' || (open == '[' && c == b']'))
    });
    if splits || v != v.trim() { format!("{{{v}}}") } else { v.to_string() }
}

fn item_text(k: &Key, open: char) -> String {
    match &k.value {
        Some(v) => format!("{}={}", k.key, value_text(v, open)),
        None => k.key.clone(),
    }
}

fn content(a: &Arg) -> String {
    match &a.keys {
        Some(keys) => keys.iter().map(|k| item_text(k, a.open)).collect::<Vec<_>>().join(", "),
        None => a.text.clone(),
    }
}

fn arg_text(a: &Arg) -> String {
    format!("{}{}{}", a.open, content(a), close(a.open))
}

/// A new node in its one canonical form.
pub fn canonical(n: &Node) -> String {
    let args: String = n.args.iter().filter(|a| a.open == '{' || !a.empty()).map(arg_text).collect();
    let star = if n.star { "*" } else { "" };
    let out = match n.kind {
        Kind::Call => format!("\\{}{star}{args}", n.name),
        Kind::Env => format!("\\begin{{{}}}{star}{args}\n{}\n\\end{{{}}}", n.name, n.body.trim_matches('\n'), n.name),
        Kind::Text => n.body.clone(),
    };
    if n.line { out + "\n" } else { out }
}

/// A byte edit: `from..to` becomes `text`.
pub(super) type Edit = (usize, usize, String);

/// The edits of a key list from `was` (in `text[list]`) to `keys`: values changed in place,
/// removed runs cut with their separator, added keys after the last item in the list's style.
fn key_edits(text: &str, list: std::ops::Range<usize>, open: char, keys: &[Key]) -> Vec<Edit> {
    let was = items(text, list.clone());
    let mut used = vec![false; keys.len()];
    let mut out = Vec::new();
    // each item written: the edited key it became, else none
    let mut kept: Vec<Option<usize>> = Vec::new();
    for item in &was {
        let key = &text[item.from..item.key_end];
        let m = (0..keys.len()).find(|&j| !used[j] && keys[j].key == key);
        if let Some(j) = m {
            used[j] = true;
            let old = item.value.map(|(s, e)| unbraced(&text[s..e]).to_string());
            match (&keys[j].value, item.value) {
                (new, _) if *new == old => {}
                (Some(v), Some((s, e))) => out.push((s, e, value_text(v, open))),
                (Some(v), None) => out.push((item.to, item.to, format!("={}", value_text(v, open)))),
                (None, _) => out.push((item.key_end, item.to, String::new())),
            }
        }
        kept.push(m);
    }
    let added: Vec<String> = (0..keys.len()).filter(|&j| !used[j]).map(|j| item_text(&keys[j], open)).collect();
    // the list's separator: what follows its first comma up to the next item, comments dropped
    let sep = was.windows(2).next().map_or(", ".to_string(), |w| {
        let gap = &text[w[0].to..w[1].from];
        gap.split('\n').map(|l| l.split('%').next().unwrap_or_default()).collect::<Vec<_>>().join("\n")
    });
    // kept keys moved (graphicx reads them in order): the list written again in its separator
    let moved = kept.iter().flatten().zip(kept.iter().flatten().skip(1)).any(|(a, b)| a > b);
    if moved || kept.iter().all(Option::is_none) {
        if was.is_empty() && added.is_empty() {
            return Vec::new();
        }
        let (from, to) = was.first().map_or((list.start, list.end), |f| (f.from, was[was.len() - 1].to));
        let all: Vec<String> = keys.iter().map(|k| item_text(k, open)).collect();
        return vec![(from, to, all.join(&sep))];
    }
    let mut k = 0;
    while k < was.len() {
        if kept[k].is_some() {
            k += 1;
            continue;
        }
        let run = k;
        while k < was.len() && kept[k].is_none() {
            k += 1;
        }
        // a run at the start goes up to the next kept item, else from the comma before it
        let (from, to) = match run {
            0 => (was[0].from, was[k].from),
            _ => item_cut(text, list.clone(), was[run].from..was[k - 1].to),
        };
        out.push((from, to, String::new()));
    }
    if !added.is_empty() {
        let last = was.iter().zip(&kept).rfind(|(_, m)| m.is_some()).map_or(list.start, |(i, _)| i.to);
        out.push((last, last, format!("{sep}{}", added.join(&sep))));
    }
    out
}

/// The edits from node `n` as written at `at` (its `text`) to `n` as edited.
fn node_edits(text: &str, at: usize, n: &Node) -> Vec<Edit> {
    let mut out = Vec::new();
    let was = &text[at..at + n.text.len()];
    // where an unwritten argument goes: after the one before it, else after the name
    let mut after = at
        + match n.kind {
            Kind::Env => was.find('}').map_or(0, |k| k + 1),
            _ => 1 + n.name.len() + usize::from(n.star),
        };
    // an environment renamed: its `\begin` and `\end`
    let old = was.get(7..after - at - 1).filter(|_| n.kind == Kind::Env).unwrap_or(&n.name);
    if old != n.name {
        out.push((at + 7, after - 1, n.name.clone()));
        out.push((at + was.len() - 1 - old.len(), at + was.len() - 1, n.name.clone()));
    }
    for a in &n.args {
        let Some((s, e)) = a.inner.map(|(s, e)| (at + s, at + e)) else {
            if !a.empty() {
                out.push((after, after, arg_text(a)));
            }
            continue;
        };
        after = e + 1;
        match &a.keys {
            // an emptied `[…]` goes with its brackets
            _ if a.open == '[' && a.empty() && a.keys.as_ref().map_or(s < e, |_| !items(text, s..e).is_empty()) => {
                out.push((s - 1, e + 1, String::new()))
            }
            Some(keys) => out.extend(key_edits(text, s..e, a.open, keys)),
            None if text[s..e] != a.text => out.push(tight(text, s, e, &a.text)),
            None => {}
        }
    }
    if let Some((s, e)) = n.body_at
        && text[at + s..at + e] != n.body
    {
        out.push(tight(text, at + s, at + e, &n.body));
    }
    out
}

/// `text[s..e]` becoming `new`, cut to the bytes that differ: the rest stays untouched.
fn tight(text: &str, s: usize, e: usize, new: &str) -> Edit {
    let (old, n) = (&text[s..e], new.as_bytes());
    let mut p = old.as_bytes().iter().zip(n).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(p) {
        p -= 1;
    }
    let mut q = old.as_bytes()[p..].iter().rev().zip(n[p..].iter().rev()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(old.len() - q) {
        q -= 1;
    }
    (s + p, e - q, new[p..new.len() - q].to_string())
}

/// `node` as written, edited to read as `printed`, a text of the same call or environment:
/// arguments matched from the last, those `printed` lacks emptied, its new ones unwritten.
pub fn retarget(node: &Node, printed: &str) -> Option<Node> {
    let fresh = read(&Text::new(printed.to_string()), 0, &[], false)?;
    let mut out = Node { name: fresh.name, star: fresh.star, body: fresh.body, ..node.clone() };
    let mut was = node.args.iter().rev().peekable();
    let mut args: Vec<Arg> = Vec::new();
    for a in fresh.args.iter().rev() {
        let old = was.next_if(|o| o.open == a.open);
        args.push(Arg { inner: old.and_then(|o| o.inner), ..a.clone() });
    }
    args.extend(was.map(|o| Arg { text: String::new(), keys: o.keys.as_ref().map(|_| Vec::new()), ..o.clone() }));
    args.reverse();
    out.args = args;
    Some(out)
}

/// The byte edits that write `n`: none when untouched, its lines when dropped, its canonical
/// form at its place when new (none while its key lists are empty). A node whose range no longer holds its text is refused.
fn edits_of(doc: &Text, n: &Node) -> Result<Vec<Edit>, String> {
    let text = doc.body();
    let byte = |p: Position| doc.offset(to_pos(doc, p)).ok_or("a place outside the text");
    let Some(r) = n.range else {
        let at = byte(n.at.ok_or("a new node without a place")?)?;
        // a new call whose key lists are all empty is a place offered, not yet written
        let offered = n.args.iter().any(|a| a.keys.is_some()) && n.args.iter().all(|a| a.keys.is_none() || a.empty());
        return Ok(if n.drop || offered { Vec::new() } else { vec![(at, at, canonical(n))] });
    };
    let (from, to) = (byte(r.start)?, byte(r.end)?);
    if text.get(from..to) != Some(n.text.as_str()) {
        return Err("the text changed since the dialog opened: open it again".into());
    }
    if n.drop {
        let (a, b) = lines_of(text, from, to);
        return Ok(vec![(a, b, String::new())]);
    }
    Ok(node_edits(text, from, n))
}

/// `edits` in order, those that touch joined into one (several at one place keep their order).
fn merged(mut edits: Vec<Edit>) -> Result<Vec<Edit>, String> {
    edits.sort_by_key(|e| (e.0, e.1));
    let mut out: Vec<Edit> = Vec::new();
    for e in edits {
        match out.last_mut() {
            Some(m) if m.1 > e.0 => return Err("two edits overlap".into()),
            Some(m) if m.1 == e.0 => {
                m.1 = e.1;
                m.2.push_str(&e.2);
            }
            _ => out.push(e),
        }
    }
    Ok(out)
}

/// Whether the run defines `\name` (an environment's `\name` too).
pub(super) fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis
        .interner
        .lookup(name)
        .is_some_and(|s| !matches!(analysis.env.meaning(s), crate::engine::tex::Meaning::Undefined))
}

/// The control sequences and environments `text` writes, in order.
fn names(text: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for (i, _) in text.match_indices('\\') {
        let rest = &text[i + 1..];
        let mut name = &rest[..rest.find(|c: char| !c.is_ascii_alphabetic() && c != '@').unwrap_or(rest.len())];
        if name == "begin" && rest[5..].starts_with('{') {
            name = rest[6..].split('}').next().unwrap_or_default();
        }
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// `text[from..to]` with the `edits` inside it made.
pub(super) fn reads(text: &str, from: usize, to: usize, edits: &[Edit]) -> String {
    let mut out = text[from..to].to_string();
    for (a, b, t) in edits.iter().rev().filter(|e| from <= e.0 && e.1 <= to) {
        out.replace_range(a - from..b - from, t);
    }
    out
}

/// The edits that write `nodes`, each as edited, over the text they were read from, and notes.
/// A name a written node adds that the run does not define loads its package: a new node's
/// `package` hint for its own name, else the installation's provider. A written node's hint
/// is loaded too (what the run cannot name, as a column letter).
pub fn printed(analysis: &Analysis, doc: &Text, nodes: &[Node]) -> Result<(Vec<Edit>, Vec<String>), String> {
    let (mut edits, mut notes) = (Vec::new(), Vec::new());
    // packages wanted in order, and names asked of the installation, by the node that wants them
    let (mut wanted, mut asked): (Vec<Package>, Vec<(String, bool)>) = Default::default();
    for n in nodes {
        let own = edits_of(doc, n)?;
        if !own.is_empty() && !n.drop {
            let (was, now) = match n.range.and_then(|r| doc.offset(to_pos(doc, r.start))) {
                Some(at) => (n.text.clone(), reads(doc.body(), at, at + n.text.len(), &own)),
                None => (String::new(), canonical(n)),
            };
            let new = n.range.is_none();
            for name in names(&now).into_iter().filter(|m| !names(&was).contains(m) && !defined(analysis, m)) {
                match &n.package {
                    Some(p) if new && name == n.name => wanted.push(p.clone()),
                    _ => asked.push((name.to_string(), new && name == n.name)),
                }
            }
            wanted.extend(n.package.clone().filter(|_| !new));
        }
        edits.extend(own);
    }
    for (name, own) in asked {
        match crate::linter::shared::provider(analysis, &name) {
            Some(p) => wanted.push(Package { name: p, options: String::new() }),
            None if own => notes.push(format!("\\{name} is not defined in the run: load its package")),
            None => {}
        }
    }
    let mut loads: Vec<Edit> = Vec::new();
    let mut done: Vec<String> = Vec::new();
    let all: Vec<String> = wanted.iter().map(|p| p.name.clone()).collect();
    for p in wanted {
        // one another wanted package loads goes with it (tabularx loads array)
        let by = |q: &String| crate::linter::shared::package_loads(analysis, q, &p.name);
        if done.contains(&p.name) || all.iter().any(by) {
            continue;
        }
        done.push(p.name.clone());
        match ensure_package(analysis, doc, &p.name, &p.options) {
            Ok(Some(e)) => {
                let at = doc.offset(to_pos(doc, e.range.start)).unwrap_or_default();
                notes.push(format!("adds \\usepackage{{{}}}", p.name));
                loads.push((at, at, e.new_text));
            }
            Ok(None) => {}
            Err(why) => notes.push(why),
        }
    }
    // a load goes first where it shares a place
    loads.append(&mut edits);
    Ok((merged(loads)?, notes))
}

/// Byte edits as LSP edits.
pub fn lsp_edits(doc: &Text, edits: Vec<Edit>) -> Vec<lsp_types::TextEdit> {
    edits.into_iter().map(|(a, b, t)| lsp_types::TextEdit::new(span(doc, a, b), t)).collect()
}

/// The edits that write `nodes` as LSP edits: `{edits, notes}`.
pub fn print(analysis: &Analysis, doc: &Text, nodes: &[Node]) -> Result<Json, String> {
    let (edits, notes) = printed(analysis, doc, nodes)?;
    Ok(json!({ "edits": lsp_edits(doc, edits), "notes": notes }))
}

/// A `satex/dialogApply` of kind `snapshot`: the edits that write its `nodes`.
pub fn apply(analysis: &Analysis, doc: &Text, params: &Json) -> Result<Json, String> {
    let nodes: Vec<Node> = serde_json::from_value(params["nodes"].clone()).map_err(|e| e.to_string())?;
    print(analysis, doc, &nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real option lists, the byte where each call starts, the arguments read as key lists.
    const LISTS: [(&str, &[usize]); 6] = [
        ("\\usepackage[margin=2cm, a4paper]{geometry}", &[0]),
        ("\\documentclass[DIV=10,BCOR=5mm,twoside]{scrartcl}", &[0]),
        ("\\geometry{\n  left=2cm,% inner\n  right={3cm},\n  top=1in\n}", &[0]),
        ("\\usepackage{geometry}", &[0]),
        ("\\begin{tabular}[t]{l|c>{\\bfseries}r@{ }S}\na & b\\\\\n\\end{tabular}", &[0]),
        ("\\KOMAoptions{headsepline=true, footsepline, DIV=calc}", &[0]),
    ];

    fn write(text: &str, edits: &[Edit]) -> String {
        let mut out = text.to_string();
        for (a, b, t) in edits.iter().rev() {
            out.replace_range(*a..*b, t);
        }
        out
    }

    /// Text before and after each list, so a cut cannot reach past it.
    fn doc(call: &str) -> Text {
        Text::new(format!("% before\n{call}\nafter\n"))
    }

    fn opened(call: &str, keyed: &[usize]) -> (Text, Node) {
        let d = doc(call);
        let n = read(&d, 9, keyed, options(call)).expect("a call");
        (d, n)
    }

    /// The loads get an unwritten `[…]`.
    fn options(call: &str) -> bool {
        call.starts_with("\\usepackage") || call.starts_with("\\documentclass")
    }

    fn print_one(d: &Text, n: &Node) -> String {
        write(d.body(), &merged(edits_of(d, n).unwrap()).unwrap())
    }

    #[test]
    fn untouched_is_no_edit() {
        for (call, keyed) in LISTS {
            let (d, n) = opened(call, keyed);
            assert_eq!(edits_of(&d, &n).unwrap(), Vec::new(), "{call}");
        }
    }

    #[test]
    fn styles_kept() {
        let (d, mut n) = opened(LISTS[2].0, LISTS[2].1);
        let keys = n.args[0].keys.as_mut().unwrap();
        keys.remove(0);
        keys.push(Key { key: "bottom".into(), value: Some("2cm".into()) });
        keys[0].value = Some("4cm".into());
        assert_eq!(print_one(&d, &n), "% before\n\\geometry{\n  right=4cm,\n  top=1in,\n  bottom=2cm\n}\nafter\n");
        let (d, mut n) = opened(LISTS[1].0, LISTS[1].1);
        n.args[0].keys.as_mut().unwrap()[0].value = Some("12".into());
        assert_eq!(print_one(&d, &n), "% before\n\\documentclass[DIV=12,BCOR=5mm,twoside]{scrartcl}\nafter\n");
        // an unwritten `[…]` gains keys, an emptied one goes
        let (d, mut n) = opened(LISTS[3].0, LISTS[3].1);
        n.args[0].keys = Some(vec![Key { key: "pass".into(), value: None }]);
        assert_eq!(print_one(&d, &n), "% before\n\\usepackage[pass]{geometry}\nafter\n");
        let (d, mut n) = opened(LISTS[0].0, LISTS[0].1);
        n.args[0].keys = Some(Vec::new());
        assert_eq!(print_one(&d, &n), "% before\n\\usepackage{geometry}\nafter\n");
        // a value that would split the list is braced
        n.args[0].keys = Some(vec![Key { key: "a".into(), value: Some("1,2".into()) }]);
        assert!(print_one(&d, &n).contains("[a={1,2}]"));
    }

    /// Random adds, removals and changes on real lists: the printed text reads back as edited,
    /// and nothing outside the node moves.
    #[test]
    fn mutations_round_trip() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let pool = ["landscape", "paper=a5paper", "head={1cm, x}", "foot=", "DIV=calc", "left=\\oddsidemargin"];
        for (call, keyed) in LISTS {
            for _ in 0..300 {
                let (d, mut n) = opened(call, keyed);
                let k = n.args.iter().position(|a| a.keys.is_some()).unwrap();
                let keys = n.args[k].keys.as_mut().unwrap();
                let mut edited = Vec::new();
                for key in keys.drain(..) {
                    match next(3) {
                        0 => {}
                        1 => edited.push(Key { value: Some(format!("{}x", next(9))), ..key }),
                        _ => edited.push(key),
                    }
                }
                for _ in 0..next(3) {
                    let (key, value) =
                        pool[next(pool.len())].split_once('=').map_or((pool[0], None), |(k, v)| (k, Some(v)));
                    let value = value.map(|v| unbraced(v).to_string());
                    edited.push(Key { key: key.into(), value });
                }
                *keys = edited.clone();
                let out = print_one(&d, &n);
                assert!(out.starts_with("% before\n") && out.ends_with("\nafter\n"), "{out}");
                let back = read(&Text::new(out.clone()), 9, keyed, options(call)).unwrap();
                // a key added again takes its old place: compared as sets
                let open = n.args[k].open;
                let mut got =
                    back.args.iter().find(|a| a.open == open).and_then(|a| a.keys.clone()).unwrap_or_default();
                let key = |k: &Key| (k.key.clone(), k.value.clone());
                got.sort_by_key(key);
                edited.sort_by_key(key);
                assert_eq!(got, edited, "{call} -> {out}");
                assert_eq!(back.body, n.body, "{out}");
            }
        }
    }

    #[test]
    fn new_nodes_print_canonically() {
        let d = doc("x");
        let at = Position::new(1, 0);
        let mut n = Node::new_call(
            "newgeometry",
            vec![Arg::keys(
                '{',
                vec![
                    Key { key: "left".into(), value: Some("3cm".into()) },
                    Key { key: "top".into(), value: Some("2cm".into()) },
                ],
            )],
            at,
        );
        let restore = Node::new_call("restoregeometry", Vec::new(), at);
        let edits = merged([edits_of(&d, &n).unwrap(), edits_of(&d, &restore).unwrap()].concat()).unwrap();
        assert_eq!(
            write(d.body(), &edits),
            "% before\n\\newgeometry{left=3cm, top=2cm}\n\\restoregeometry\nx\nafter\n"
        );
        n.drop = true;
        assert!(edits_of(&d, &n).unwrap().is_empty());
        // an offered call, its key list empty, writes nothing
        let offered = Node::new_call("geometry", vec![Arg::keys('{', Vec::new())], at);
        assert!(edits_of(&d, &offered).unwrap().is_empty());
        // a dropped node takes its line
        let (d, mut n) = opened(LISTS[0].0, LISTS[0].1);
        n.drop = true;
        assert_eq!(print_one(&d, &n), "% before\nafter\n");
    }

    /// The table dialog's text through the printer: one cell changed writes that cell only; an X
    /// column renames the environment and adds its width; the rest of the bytes stay.
    #[test]
    fn a_table_prints_only_its_change() {
        use crate::lsp::tabular;
        let table = "\\begin{tabular}[t]{|l|c|} % keep\n  a  &  b \\\\ \\hline\n  c & d \\\\ % note\n\\end{tabular}";
        let (d, n) = opened(table, &[]);
        let mut t = tabular::parse(table).unwrap();
        assert!(edits_of(&d, &retarget(&n, &tabular::emit(&t)).unwrap()).unwrap().is_empty());
        t.rows[1].cells[1].text = "e".into();
        let edits = edits_of(&d, &retarget(&n, &tabular::emit(&t)).unwrap()).unwrap();
        assert_eq!(edits.iter().map(|e| e.2.as_str()).collect::<Vec<_>>(), ["e"], "{edits:?}");
        t.cols[1].align = "X".into();
        assert!(tabular::to_tabularx(&mut t));
        let out = print_one(&d, &retarget(&n, &tabular::emit(&t)).unwrap());
        assert_eq!(
            out,
            "% before\n\\begin{tabularx}{\\linewidth}[t]{|l|X|} % keep\n  a  &  b \\\\ \\hline\n  c & e \\\\ % note\n\\end{tabularx}\nafter\n"
        );
    }

    /// The float dialog: placement letters replaced, added after the name, dropped with their brackets.
    #[test]
    fn a_float_prints_its_placement() {
        let fig = "\\begin{figure}[ht] % here\n\\centering x\n\\end{figure}";
        let (d, mut n) = opened(fig, &[]);
        n.args[0].text = "tb!".into();
        assert_eq!(print_one(&d, &n), format!("% before\n{}\nafter\n", fig.replace("[ht]", "[tb!]")));
        n.args[0].text = String::new();
        assert_eq!(print_one(&d, &n), format!("% before\n{}\nafter\n", fig.replace("[ht]", "")));
        let (d, mut n) = opened(&fig.replace("[ht]", ""), &[]);
        n.args.insert(0, Arg::text('[', "p"));
        assert_eq!(print_one(&d, &n), format!("% before\n{}\nafter\n", fig.replace("[ht]", "[p]")));
    }

    /// The image dialog: a value changed in place, its list's separator kept; moved keys (graphicx
    /// reads them in order) rewrite the list in that separator.
    #[test]
    fn an_image_prints_its_keys() {
        let img = "\\includegraphics[trim=1cm 2 3 4,clip,angle=90,width=3cm]{fig}";
        let (d, mut n) = opened(img, &[0]);
        n.args[0].keys.as_mut().unwrap()[2].value = Some("180".into());
        assert_eq!(edits_of(&d, &n).unwrap().iter().map(|e| e.2.as_str()).collect::<Vec<_>>(), ["180"]);
        n.args[0].keys.as_mut().unwrap().swap(2, 3);
        assert!(print_one(&d, &n).contains("[trim=1cm 2 3 4,clip,width=3cm,angle=180]{fig}"));
    }

    /// The tree dialog: the environment's body set, its bracket tree no argument, only the
    /// bytes that differ written.
    #[test]
    fn a_tree_prints_its_body() {
        let tree = "\\begin{forest}\n  [VP [DP] [V]] % s\n\\end{forest}";
        let d = doc(tree);
        let body = 9 + "\\begin{forest}".len()..9 + tree.len() - "\\end{forest}".len();
        let mut n = env(&d, 9, body).unwrap();
        assert!(n.args.is_empty(), "{n:?}");
        n.body = n.body.replace("[V]", "[V [x]]");
        assert_eq!(edits_of(&d, &n).unwrap().iter().map(|e| e.2.as_str()).collect::<Vec<_>>(), [" [x]"]);
        assert_eq!(print_one(&d, &n), format!("% before\n{}\nafter\n", tree.replace("[V]", "[V [x]]")));
    }

    /// The color picker: a name in its place as a text node, the command's `[model]` added.
    #[test]
    fn a_color_prints_in_place() {
        let d = doc("\\color{brand} x");
        let name = Node {
            kind: Kind::Text,
            range: Some(span(&d, 16, 21)),
            text: "brand".into(),
            body: "red!40".into(),
            body_at: Some((0, 5)),
            ..Node::default()
        };
        assert_eq!(print_one(&d, &name), "% before\n\\color{red!40} x\nafter\n");
        let mut own = read(&d, 9, &[], true).unwrap();
        own.args[0] = Arg::text('[', "HTML");
        own.args[1].text = "FF0000".into();
        assert_eq!(print_one(&d, &own), "% before\n\\color[HTML]{FF0000} x\nafter\n");
    }
}
