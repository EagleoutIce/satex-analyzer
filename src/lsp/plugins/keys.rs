//! Key lists: pgfkeys, l3keys, kvoptions/xkeyval and package options, from
//! the keys the run stored. Completion opens at `{`, `,` and `=` with nothing
//! typed; a family goes on with its keys (`/demo` → `/demo/`, completion
//! reopened); hover shows a key's handler kind, initial and default value,
//! where it was defined and the comment written next to the definition.

use super::{Cx, Hit, TypePlugin};
use crate::analysis::roles::{KeyKind, Ty};
use crate::lsp::handlers;
use crate::project::config::TypePlugins;
use lsp_types::CompletionItem;

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
        &["hover", "complete"]
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
