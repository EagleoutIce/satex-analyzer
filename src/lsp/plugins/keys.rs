//! Key lists: pgfkeys, l3keys, kvoptions/xkeyval and package options, from
//! the keys the run stored. Completion opens at `{`, `,` and `=` with nothing
//! typed; a family goes on with its keys (`/demo` → `/demo/`, completion
//! reopened); hover shows a key's handler kind, initial and default value,
//! where it was defined and the comment written next to the definition. The view
//! lists the same keys; a choice writes one where completion would.

use super::{Cx, Hit, TypePlugin};
use crate::analysis::roles::{KeyKind, Ty};
use crate::lsp::handlers;
use crate::project::config::TypePlugins;
use lsp_types::{CompletionItem, CompletionTextEdit, InsertTextFormat, TextEdit};
use serde_json::{Value as Json, json};

pub(super) struct Keys;

impl TypePlugin for Keys {
    fn name(&self) -> &'static str {
        "keys"
    }
    fn category(&self) -> &'static str {
        "completion"
    }
    fn about(&self) -> &'static str {
        "key list completion, family continuation and key docs on hover"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Options, Ty::Key(KeyKind::Option), Ty::Key(KeyKind::Pgf), Ty::Key(KeyKind::Declared)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["hover", "complete", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.keys.enabled
    }

    fn hover(&self, cx: &Cx, _hit: &Hit) -> Option<String> {
        handlers::key_hover(cx.analysis, cx.doc.line(cx.pos.line), cx.pos.col)
    }

    fn complete(&self, cx: &Cx, hit: &Hit) -> Option<Vec<CompletionItem>> {
        let arg = cx.at.place.as_ref()?.arg();
        let after = {
            let line = cx.doc.line(cx.pos.line);
            &line[crate::engine::source::byte_of_col(line, cx.pos.col).min(line.len())..]
        };
        let mut items = match hit.ty {
            Ty::Options => handlers::package_options(cx.analysis, &arg, after),
            _ if arg.open == '[' => handlers::bracket_keys(cx.analysis, cx.doc, &arg),
            _ => handlers::key_items(cx.analysis, cx.doc, &arg, cx.pos),
        };
        if cx.cfg().keys.family_continue {
            items.iter_mut().for_each(continue_family);
        }
        Some(items)
    }

    fn view(&self, cx: &Cx, hit: &Hit) -> Option<Json> {
        let at = super::super::handlers::to_lsp(cx.doc, cx.pos);
        let items: Vec<Json> = (self.complete(cx, hit)?.into_iter())
            .filter(|it| it.command.is_none())
            .map(|it| {
                let snippet = it.insert_text_format == Some(InsertTextFormat::SNIPPET);
                let (range, text) = match it.text_edit {
                    Some(CompletionTextEdit::Edit(e)) => (e.range, e.new_text),
                    Some(CompletionTextEdit::InsertAndReplace(e)) => (e.replace, e.new_text),
                    None => (lsp_types::Range::new(at, at), it.insert_text.unwrap_or_else(|| it.label.clone())),
                };
                let text = if snippet { plain(&text) } else { text };
                json!({ "title": it.label, "detail": it.detail, "edits": [TextEdit::new(range, text)] })
            })
            .collect();
        let what = if hit.ty == Ty::Options { "Options" } else { "Keys" };
        (!items.is_empty()).then(|| {
            json!({ "title": what, "sections": [
                { "kind": "choices", "placeholder": "key", "items": items },
                { "kind": "text", "markdown": "The keys the run defined here; a choice writes it at the cursor, as completion would." },
            ] })
        })
    }
}

/// A snippet's text without its tab stops: `${1:x}` is `x`, `$1` nothing.
fn plain(snippet: &str) -> String {
    let mut out = String::new();
    let mut rest = snippet;
    while let Some(i) = rest.find('$') {
        out.push_str(&rest[..i]);
        rest = &rest[i + 1..];
        if let Some(r) = rest.strip_prefix('{') {
            let end = r.find('}').unwrap_or(r.len());
            out.push_str(r[..end].split_once(':').map_or("", |(_, d)| d));
            rest = r.get(end + 1..).unwrap_or("");
        } else {
            rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        }
    }
    out + rest
}

/// A family's item inserts its path with a `/` and reopens completion on its keys.
fn continue_family(it: &mut CompletionItem) {
    let family = it.detail.as_deref().is_some_and(|d| d.contains("is family"));
    if !family || it.label.ends_with('/') {
        return;
    }
    match &mut it.text_edit {
        Some(lsp_types::CompletionTextEdit::Edit(e)) if !e.new_text.ends_with('/') => e.new_text.push('/'),
        Some(_) => {}
        None => it.insert_text = Some(format!("{}/", it.insert_text.as_deref().unwrap_or(&it.label))),
    }
    it.command = Some(lsp_types::Command {
        title: "keys of the family".into(),
        command: "editor.action.triggerSuggest".into(),
        arguments: None,
    });
}
