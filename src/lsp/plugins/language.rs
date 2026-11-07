//! Code languages: a key's value listings reads as a language (`language=`), an argument minted
//! hands Pygments as a lexer. Completion lists the languages the run and its language files
//! define, or Pygments' lexers, the document's own first; hover says where one is defined.

use super::{Cx, Hit, TypePlugin};
use crate::analysis::roles::{KeyKind, Ty};
use crate::project::config::TypePlugins;
use crate::queries::languages::{self, Lang};
use lsp_types::{CompletionItem, CompletionItemKind};

pub(super) struct Language;

impl TypePlugin for Language {
    fn name(&self) -> &'static str {
        "language"
    }
    fn category(&self) -> &'static str {
        "completion"
    }
    fn about(&self) -> &'static str {
        "listings languages and Pygments lexers, where each is defined"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Language)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["hover", "complete"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.language.enabled
    }

    fn hover(&self, cx: &Cx, hit: &Hit) -> Option<String> {
        let name = name(cx, hit);
        let mut md = crate::lsp::handlers::heading("code language", &name);
        let langs = offered(cx);
        let key = |l: &Lang| languages::listings_key(&l.name);
        let found = match value(cx) {
            true => langs.iter().find(|l| key(l) == languages::listings_key(&name)),
            false => langs.iter().find(|l| l.name.eq_ignore_ascii_case(&name) || l.aliases.contains(&name)),
        };
        match found {
            Some(l) if !l.source.is_empty() => md.push_str(&format!("Defined in {}.\n", l.source)),
            Some(_) => md.push_str("A language the run defined.\n"),
            None => md.push_str("A language neither the run nor its language files define.\n"),
        }
        Some(md)
    }

    fn complete(&self, cx: &Cx, _hit: &Hit) -> Option<Vec<CompletionItem>> {
        let used: Vec<String> = languages::used(cx.analysis).into_iter().map(|w| w.to_lowercase()).collect();
        let items = offered(cx).into_iter().map(|l| {
            let mine = used.contains(&l.name.to_lowercase()) || l.aliases.iter().any(|a| used.contains(a));
            let filter = std::iter::once(l.name.clone()).chain(l.aliases.iter().cloned()).collect::<Vec<_>>().join(" ");
            CompletionItem {
                sort_text: Some(format!("{}{}", u8::from(!mine), l.name.to_lowercase())),
                filter_text: Some(filter),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(l.source.clone()),
                label: l.name,
                ..Default::default()
            }
        });
        let items: Vec<CompletionItem> = items.collect();
        (!items.is_empty()).then_some(items)
    }
}

/// Whether the text at the cursor is a key's value: listings reads a language there.
fn value(cx: &Cx) -> bool {
    cx.at.place.as_ref().is_some_and(|p| p.inside.rsplit(',').next().is_some_and(|i| i.contains('=')))
}

/// The languages a position takes: listings' for a key's value, else Pygments' lexers.
fn offered(cx: &Cx) -> Vec<Lang> {
    match value(cx) {
        true => languages::listings(cx.analysis).map(|(l, _)| l).unwrap_or_default(),
        false => languages::pygments(cx.analysis),
    }
}

/// The language's name at the cursor, with the dialect written before it (`[LaTeX]TeX`).
fn name(cx: &Cx, hit: &Hit) -> String {
    let typed = hit.typed.as_deref().unwrap_or(&cx.at.item.typed).trim();
    let line = cx.doc.line(cx.pos.line);
    let lead = &line[..crate::engine::source::byte_of_col(line, cx.at.item.col).min(line.len())];
    match lead.strip_suffix(']').and_then(|l| l.rfind('[')) {
        Some(i) => format!("{}{typed}", &lead[i..]),
        None => typed.to_string(),
    }
}
