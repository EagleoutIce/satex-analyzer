//! Type plugins: editor features triggered by a type of the lattice at a
//! position ([`super::type_at`]), never by a command's name or a text pattern.
//! Each feature is optional: hover, completion, code actions, a view (a
//! dialog as JSON that the playground and the VS Code webview render with one
//! component), document colors and inlay hints. `satex.yaml`'s
//! `type_plugins.<name>` switches and tunes each; `satex query type-plugins`
//! lists them.

mod color;
mod environment;
mod image;
mod keys;
mod table;
mod units;

use super::type_at::{At, Hit};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::linter::fix::{Pos, Text};
use crate::project::config::TypePlugins;
use lsp_types::{CodeAction, ColorInformation, CompletionItem, InlayHint};
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
    /// The types of the lattice it triggers on.
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
    /// Whole-document features: colors (`textDocument/documentColor`).
    fn colors(&self, _analysis: &Analysis, _doc: &Text, _path: &str) -> Vec<ColorInformation> {
        Vec::new()
    }
    fn inlays(&self, _analysis: &Analysis, _doc: &Text, _path: &str) -> Vec<InlayHint> {
        Vec::new()
    }
}

/// Every plugin, in the order they answer.
pub(super) static PLUGINS: &[&dyn TypePlugin] =
    &[&keys::Keys, &units::Units, &color::Color, &environment::Environment, &table::Table, &image::Image];

/// The enabled plugins.
fn on(analysis: &Analysis) -> impl Iterator<Item = &'static dyn TypePlugin> + '_ {
    PLUGINS.iter().copied().filter(|p| p.enabled(&analysis.settings.type_plugins))
}

/// Each enabled plugin that wants a hit at `cx`, with the hit, the most precise type first.
fn triggered<'a>(cx: &'a Cx<'a>) -> impl Iterator<Item = (&'static dyn TypePlugin, &'a Hit)> + 'a {
    cx.at.hits.iter().flat_map(move |h| on(cx.analysis).filter(move |p| p.wants(h)).map(move |p| (p, h)))
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

pub(super) fn colors(analysis: &Analysis, doc: &Text, path: &str) -> Vec<ColorInformation> {
    on(analysis).flat_map(|p| p.colors(analysis, doc, path)).collect()
}

pub(super) fn inlays(analysis: &Analysis, doc: &Text, path: &str) -> Vec<InlayHint> {
    on(analysis).flat_map(|p| p.inlays(analysis, doc, path)).collect()
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
            r.insert("types".into(), json!(p.types().iter().map(Ty::to_string).collect::<Vec<_>>()));
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
