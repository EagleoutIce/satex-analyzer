//! Type plugins: editor features triggered by a type of the lattice at a
//! position ([`super::type_at`]), never by a command's name or a text pattern.
//! Each feature is optional: hover, completion, code actions, a view (a
//! dialog as JSON that the playground and the VS Code webview render with one
//! component), document colors and inlay hints. `satex.yaml`'s
//! `type_plugins.<name>` switches and tunes each; `satex query type-plugins`
//! lists them.

mod bibliography;
mod color;
mod environment;
mod glossary;
mod image;
mod index;
mod keys;
mod savebox;
mod table;
mod theorem;
mod tikz;
mod units;

use super::type_at::{At, Hit};
use crate::analysis::facts::Occurrence;
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::linter::fix::{Pos, Text};
use crate::project::config::TypePlugins;
use lsp_types::{CodeAction, CodeLens, ColorInformation, CompletionItem, InlayHint};
use serde_json::{Value as Json, json};

/// What a feature sees: the run, the document and the types at the position.
pub(super) struct Cx<'a> {
    pub analysis: &'a Analysis,
    pub doc: &'a Text,
    pub path: &'a str,
    pub pos: Pos,
    pub at: &'a At,
}

impl Cx<'_> {
    pub fn cfg(&self) -> &TypePlugins {
        &self.analysis.settings.type_plugins
    }

    /// The LSP URI of the document, for edits.
    pub fn uri(&self) -> lsp_types::Uri {
        super::handlers::path_uri(self.path)
    }
}

/// A plugin. Every feature has a default that adds nothing.
pub(super) trait TypePlugin: Sync {
    /// Its key under `type_plugins:`.
    fn name(&self) -> &'static str;
    /// One line for `satex query type-plugins` and the wiki.
    fn about(&self) -> &'static str;
    /// The types of the lattice it triggers on; none: the run's place triggers it (an
    /// environment the run met), asked with a [`Ty::Bottom`] hit.
    fn types(&self) -> &'static [Ty];
    /// The features it implements: hover, complete, actions, view, colors, inlay.
    fn features(&self) -> &'static [&'static str];
    fn enabled(&self, cfg: &TypePlugins) -> bool;
    /// The trigger: whether `hit` is text of one of its types.
    fn wants(&self, hit: &Hit) -> bool {
        self.types().contains(&hit.ty)
    }
    fn hover(&self, _cx: &Cx, _hit: &Hit) -> Option<String> {
        None
    }
    /// `None` leaves the hit to the next plugin or the core.
    fn complete(&self, _cx: &Cx, _hit: &Hit) -> Option<Vec<CompletionItem>> {
        None
    }
    fn actions(&self, _cx: &Cx, _hit: &Hit) -> Vec<CodeAction> {
        Vec::new()
    }
    /// A dialog, in the view JSON of `doc/wiki/writing-type-plugins.md`.
    fn view(&self, _cx: &Cx, _hit: &Hit) -> Option<Json> {
        None
    }
    /// A manager of the whole document's names of its kind, opened without a position (a sidebar).
    fn manage(&self, _cx: &Cx) -> Option<Json> {
        None
    }
    /// Whole-document features: colors (`textDocument/documentColor`).
    fn colors(&self, _analysis: &Analysis, _doc: &Text, _path: &str) -> Vec<ColorInformation> {
        Vec::new()
    }
    fn inlays(&self, _analysis: &Analysis, _doc: &Text, _path: &str) -> Vec<InlayHint> {
        Vec::new()
    }
    fn lenses(&self, _analysis: &Analysis, _doc: &Text, _path: &str) -> Vec<CodeLens> {
        Vec::new()
    }
}

/// Every plugin, in the order they answer.
pub(super) static PLUGINS: &[&dyn TypePlugin] = &[
    &keys::Keys,
    &units::Units,
    &color::Color,
    &environment::Environment,
    &table::Table,
    &image::Image,
    &tikz::Tikz,
    &savebox::Savebox,
    &glossary::Glossary,
    &bibliography::Bibliography,
    &index::Index,
    &theorem::Theorem,
];

/// The hit a plugin without types is asked with.
static PLACE: std::sync::LazyLock<Hit> = std::sync::LazyLock::new(|| Hit::of(Ty::Bottom));

/// The enabled plugins.
fn on(analysis: &Analysis) -> impl Iterator<Item = &'static dyn TypePlugin> + '_ {
    PLUGINS.iter().copied().filter(|p| p.enabled(&analysis.settings.type_plugins))
}

/// Each enabled plugin that wants a hit at `cx`, with the hit, the most precise type first; then
/// those the run's place triggers.
fn triggered<'a>(cx: &'a Cx<'a>) -> impl Iterator<Item = (&'static dyn TypePlugin, &'a Hit)> + 'a {
    let typed = cx.at.hits.iter().flat_map(move |h| on(cx.analysis).filter(move |p| p.wants(h)).map(move |p| (p, h)));
    typed.chain(on(cx.analysis).filter(|p| p.types().is_empty()).map(|p| (p, &*PLACE)))
}

/// What a feature at `pos` sees.
fn context<'a>(analysis: &'a Analysis, doc: &'a Text, path: &'a str, pos: Pos, at: &'a At) -> Cx<'a> {
    Cx { analysis, doc, path, pos, at }
}

/// The first plugin hover at `pos`: `at` from [`super::type_at::type_at`] at the end of the word.
pub(super) fn hover(analysis: &Analysis, doc: &Text, path: &str, pos: Pos, at: &At) -> Option<String> {
    let cx = context(analysis, doc, path, pos, at);
    triggered(&cx).find_map(|(p, h)| p.hover(&cx, h))
}

/// The plugin items for `hit`, `None` when no enabled plugin takes it.
pub(super) fn complete(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    pos: Pos,
    at: &At,
    hit: &Hit,
) -> Option<Vec<CompletionItem>> {
    let cx = context(analysis, doc, path, pos, at);
    on(analysis).filter(|p| p.wants(hit)).find_map(|p| p.complete(&cx, hit))
}

/// Every plugin's actions at `pos`, deduplicated by title.
pub(super) fn actions(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> Vec<CodeAction> {
    let at = hover_types(analysis, doc, path, pos);
    let cx = context(analysis, doc, path, pos, &at);
    let mut out: Vec<CodeAction> = Vec::new();
    for (p, h) in triggered(&cx) {
        for a in p.actions(&cx, h) {
            if !out.iter().any(|o| o.title == a.title) {
                out.push(a);
            }
        }
    }
    out
}

/// `satex/view`: the views at `pos`, each `{plugin, title, range, sections}`.
pub(super) fn views(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> Vec<Json> {
    let at = hover_types(analysis, doc, path, pos);
    let cx = context(analysis, doc, path, pos, &at);
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (p, h) in triggered(&cx) {
        if seen.contains(&p.name()) {
            continue;
        }
        if let Some(mut v) = p.view(&cx, h) {
            v["plugin"] = json!(p.name());
            seen.push(p.name());
            out.push(v);
        }
    }
    out
}

/// `satex/view` with a plugin and no position: its manager.
pub(super) fn manager(analysis: &Analysis, doc: &Text, path: &str, name: &str) -> Option<Json> {
    let at = At { hits: Vec::new(), place: None, item: super::type_at::Item { typed: String::new(), col: 1 } };
    let cx = context(analysis, doc, path, Pos::new(1, 1), &at);
    let mut v = on(analysis).find(|p| p.name() == name)?.manage(&cx)?;
    v["plugin"] = json!(name);
    Some(v)
}

pub(super) fn colors(analysis: &Analysis, doc: &Text, path: &str) -> Vec<ColorInformation> {
    on(analysis).flat_map(|p| p.colors(analysis, doc, path)).collect()
}

pub(super) fn inlays(analysis: &Analysis, doc: &Text, path: &str) -> Vec<InlayHint> {
    on(analysis).flat_map(|p| p.inlays(analysis, doc, path)).collect()
}

pub(super) fn lenses(analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
    on(analysis).flat_map(|p| p.lenses(analysis, doc, path)).collect()
}

/// The `\begin` and `\end` occurrences of each `name` environment the run met in `path`,
/// matched by nesting, in the order they close.
pub(super) fn pairs<'a>(analysis: &'a Analysis, path: &str, name: &str) -> Vec<(&'a Occurrence, &'a Occurrence)> {
    use crate::engine::builtins::OccKind;
    let Some(file) = super::type_at::file_of(analysis, path) else { return Vec::new() };
    let mut ends: Vec<&Occurrence> = (analysis.facts.occurrences.iter())
        .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment))
        .filter(|o| o.key == name && o.span.file == file)
        .collect();
    ends.sort_by_key(|o| (o.span.line, o.span.col));
    ends.dedup_by_key(|o| (o.span.line, o.span.col, o.kind));
    let mut open = Vec::new();
    let mut out = Vec::new();
    for o in ends {
        if o.kind == OccKind::BeginEnvironment {
            open.push(o);
        } else if let Some(b) = open.pop() {
            out.push((b, o));
        }
    }
    out
}

/// The types of the whole word at `pos`, asked at its end, as hover asks.
pub(super) fn hover_types(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> At {
    let line = doc.line(pos.line);
    let at = crate::engine::source::byte_of_col(line, pos.col).min(line.len());
    let rest = line[at..].chars().take_while(|c| !super::type_at::ENDS.contains(*c)).count() as u32;
    super::type_at::type_at(analysis, doc, path, Pos::new(pos.line, pos.col + rest))
}

/// `satex query type-plugins`: one record per plugin.
pub fn list(cfg: &TypePlugins) -> Vec<crate::queries::query::Record> {
    PLUGINS
        .iter()
        .map(|p| {
            let mut r = crate::queries::query::Record::new();
            r.insert("name".into(), json!(p.name()));
            r.insert("enabled".into(), json!(p.enabled(cfg)));
            let mut types: Vec<String> = p.types().iter().map(Ty::to_string).collect();
            if types.is_empty() {
                types.push("environment the run met".into());
            }
            r.insert("types".into(), json!(types));
            r.insert("features".into(), json!(p.features()));
            r.insert("about".into(), json!(p.about()));
            r
        })
        .collect()
}

/// A code action that replaces `range` of the document with `text`.
pub(super) fn rewrite(cx: &Cx, title: String, edits: Vec<lsp_types::TextEdit>) -> CodeAction {
    let mut changes = std::collections::HashMap::new();
    changes.insert(cx.uri(), edits);
    CodeAction {
        title,
        kind: Some(lsp_types::CodeActionKind::REFACTOR_REWRITE),
        edit: Some(lsp_types::WorkspaceEdit { changes: Some(changes), ..Default::default() }),
        ..Default::default()
    }
}

/// A view's button: an action's title and its edits, as the view JSON writes them.
pub(super) fn button(action: &CodeAction) -> Json {
    let edits = action.edit.as_ref().and_then(|e| e.changes.as_ref()).and_then(|c| c.values().next().cloned());
    json!({ "title": action.title, "edits": edits.unwrap_or_default() })
}

/// A WorkspaceEdit of `edits` on the file `path`, as views write them.
pub(super) fn edit_of(path: &str, edits: Vec<lsp_types::TextEdit>) -> Json {
    json!({ "changes": { super::handlers::path_uri(path).as_str(): edits } })
}

/// The LSP range of characters `from..to` (columns from 1) of line `line`.
pub(super) fn range(doc: &Text, line: u32, from: u32, to: u32) -> lsp_types::Range {
    lsp_types::Range {
        start: super::handlers::to_lsp(doc, Pos::new(line, from)),
        end: super::handlers::to_lsp(doc, Pos::new(line, to)),
    }
}

/// Every color the run defined, for a key's value: none when the color plugin is off.
pub(super) fn color_items(analysis: &Analysis) -> Vec<CompletionItem> {
    let on = analysis.settings.type_plugins.color.enabled;
    if on { color::items(analysis, "", Pos::new(u32::MAX, u32::MAX)) } else { Vec::new() }
}

pub(super) fn color_presentations(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    range: lsp_types::Range,
    color: lsp_types::Color,
) -> Vec<lsp_types::ColorPresentation> {
    color::presentations(analysis, doc, path, range, color)
}

/// What a plugin adds to a control sequence's hover, by the type of its meaning: a length
/// register's value in every unit.
pub(super) fn command_hover(analysis: &Analysis, doc: &Text, path: &str, pos: Pos, name: &str) -> Option<String> {
    let sym = analysis.interner.lookup(crate::engine::tex::bare_name(name))?;
    let ty = Ty::of_meaning(&analysis.env.meaning(sym))?;
    let hit = Hit { typed: Some(name.to_string()), ..Hit::of(ty) };
    let at = At { hits: vec![hit], place: None, item: super::type_at::Item { typed: name.to_string(), col: pos.col } };
    hover(analysis, doc, path, pos, &at)
}

/// One argument of a call as written: its delimiter and the bytes inside it.
#[derive(Clone, Debug)]
pub(super) struct Group {
    pub open: char,
    pub inner: std::ops::Range<usize>,
}

/// The call whose `\` stands at byte `at` of `text`: the end of its name, its `[…]`/`{…}`
/// groups as written (spaces, one line end and comments between them), and its end.
pub(super) fn call_at(text: &str, at: usize) -> Option<(usize, Vec<Group>, usize)> {
    let b = text.as_bytes();
    if b.get(at) != Some(&b'\\') {
        return None;
    }
    let mut i = at + 1;
    while b.get(i).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'@') {
        i += 1;
    }
    if b.get(i) == Some(&b'*') {
        i += 1;
    }
    let name_end = i;
    let mut groups = Vec::new();
    let mut end = i;
    loop {
        let mut j = end;
        let mut breaks = 0;
        while let Some(&c) = b.get(j) {
            match c {
                b' ' | b'\t' => j += 1,
                b'\n' if breaks == 0 => (breaks, j) = (1, j + 1),
                b'%' => j = text[j..].find('\n').map_or(b.len(), |k| j + k),
                _ => break,
            }
        }
        let open = match b.get(j) {
            Some(b'{') => '{',
            Some(b'[') => '[',
            _ => break,
        };
        let close = closing(text, j, open)?;
        groups.push(Group { open, inner: j + 1..close });
        end = close + 1;
    }
    Some((name_end, groups, end))
}

/// The byte of the delimiter closing the group opened at `at` (`{` nests, `[` ends at the
/// first `]` outside braces), skipping escaped characters and comments.
pub(super) fn closing(text: &str, at: usize, open: char) -> Option<usize> {
    let b = text.as_bytes();
    let mut depth = 0i32;
    let mut i = at + 1;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 1,
            b'%' => i = text[i..].find('\n').map_or(b.len(), |k| i + k),
            b'{' => depth += 1,
            b'}' if depth == 0 && open == '{' => return Some(i),
            b'}' => depth -= 1,
            b']' if depth == 0 && open == '[' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// The `key=value` items of a key list written in `text[range]`: each key and the bytes of its
/// value as written (braces kept), top-level commas only.
pub(super) fn key_values(text: &str, range: std::ops::Range<usize>) -> Vec<(String, std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = range.start;
    let b = text.as_bytes();
    for i in range.start..=range.end {
        let c = if i == range.end { b',' } else { b[i] };
        match c {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                let item = &text[start..i];
                if let Some(eq) = item.find('=') {
                    let key = item[..eq].trim().to_string();
                    let v = &item[eq + 1..];
                    let lead = v.len() - v.trim_start().len();
                    let from = start + eq + 1 + lead;
                    let to = from + v.trim().len();
                    if !key.is_empty() && !key.contains(['{', '}', '\\']) {
                        out.push((key, from..to));
                    }
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    out
}

/// A value without the one pair of braces around all of it.
pub(super) fn unbraced(value: &str) -> &str {
    let v = value.trim();
    match v.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        Some(inner) if closing(v, 0, '{') == Some(v.len() - 1) => inner,
        _ => v,
    }
}

/// The LSP range of bytes `from..to` of `doc`.
pub(super) fn span(doc: &Text, from: usize, to: usize) -> lsp_types::Range {
    lsp_types::Range {
        start: super::handlers::to_lsp(doc, doc.pos(from)),
        end: super::handlers::to_lsp(doc, doc.pos(to)),
    }
}

/// An edit of bytes `from..to` of `doc`.
pub(super) fn put(doc: &Text, from: usize, to: usize, text: impl Into<String>) -> lsp_types::TextEdit {
    lsp_types::TextEdit { range: span(doc, from, to), new_text: text.into() }
}

/// The bytes of `from..to` widened to whole lines when nothing else stands on them: what a
/// deletion removes.
pub(super) fn lines_of(text: &str, from: usize, to: usize) -> (usize, usize) {
    let start = text[..from].rfind('\n').map_or(0, |i| i + 1);
    let end = text[to..].find('\n').map_or(text.len(), |i| to + i + 1);
    let alone = text[start..from].trim().is_empty() && text[to..end].trim().is_empty();
    if alone { (start, end) } else { (from, to) }
}

/// The byte of the occurrence at `span` in `doc`, when it stands in the file at `path`.
pub(super) fn byte_of(cx: &Cx, s: crate::engine::tex::Span) -> Option<usize> {
    (super::type_at::file_of(cx.analysis, cx.path) == Some(s.file)).then(|| cx.doc.offset(Pos::new(s.line, s.col)))?
}

/// A record's `fields` entry: a name, its value as written and the edit that writes a new one.
pub(super) fn field(cx: &Cx, name: &str, value: &str, from: usize, to: usize, template: &str) -> Json {
    json!({ "name": name, "value": value, "long": value.len() > 40, "edit": edit_of(cx.path, vec![put(cx.doc, from, to, template)]) })
}
