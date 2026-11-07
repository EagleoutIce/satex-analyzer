//! Listing bodies: an environment the run read verbatim and listings set itself up in. Outside its escapes only what the
//! listing reads applies: the replacement sequences of its `literate=` key. Inside an escape
//! (`escapeinside`, `escapechar`, `mathescape`) TeX runs, so the usual completion applies. The
//! sequences and the delimiters are the run's options in effect at the listing.

use super::TypePlugin;
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::engine::builtins::OccKind;
use crate::engine::tex::{Sym, detokenize};
use crate::project::config::TypePlugins;
use crate::text::{Pos, Text};
use lsp_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, Range, TextEdit};

pub(super) struct Listing;

impl TypePlugin for Listing {
    fn name(&self) -> &'static str {
        "listing"
    }
    fn category(&self) -> &'static str {
        "completion"
    }
    fn about(&self) -> &'static str {
        "a listing's literate sequences outside its escapes, LaTeX inside them"
    }
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["complete"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.listing.enabled
    }
}

/// The items at `pos` in a listing's body outside its escapes: its literate sequences. `None`
/// outside a listing or inside an escape, where completion goes on as anywhere.
pub(crate) fn items(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> Option<Vec<CompletionItem>> {
    if !analysis.settings.type_plugins.listing.enabled {
        return None;
    }
    let (body, ids) = listing_at(analysis, doc, path, pos)?;
    let at = doc.offset(pos)?;
    let before = doc.body().get(body..at)?;
    let option = |name: &str| setting(analysis, name, ids);
    let mut pairs = escapes(option("lst@DefEsc").as_deref().unwrap_or_default());
    let math = analysis.interner.lookup("lst@ifmathescape").map(|s| analysis.env.meaning(s));
    let on = analysis.interner.lookup("iftrue").map(|s| analysis.env.meaning(s));
    let set_here = defined_in(analysis, "lst@ifmathescape", ids).is_some();
    if set_here || (math.is_some() && math == on) {
        pairs.push(("$".into(), "$".into()));
    }
    if pairs.iter().any(|(open, close)| inside(before, open, close)) {
        return None;
    }
    let typed: String =
        before.chars().rev().take_while(|c| !c.is_whitespace()).collect::<Vec<_>>().into_iter().rev().collect();
    let start = super::super::handlers::to_lsp(doc, doc.pos(at - typed.len()));
    let range = Range { start, end: super::super::handlers::to_lsp(doc, pos) };
    let literate = option("lst@literate").unwrap_or_default();
    let items = sequences(&literate).into_iter().filter(|(seq, _)| seq.starts_with(&typed)).map(|(seq, shown)| {
        CompletionItem {
            label: seq.clone(),
            kind: Some(CompletionItemKind::TEXT),
            detail: Some(format!("literate: {shown}")),
            filter_text: Some(seq.clone()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text: seq })),
            ..Default::default()
        }
    });
    Some(items.collect())
}

/// The byte where the body of the listing around `pos` starts, and the run's ids of its
/// `\begin` and `\end`: an environment whose `\begin` stopped `\` being an escape.
fn listing_at(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> Option<(usize, (u32, u32))> {
    let file = super::super::type_at::file_of(analysis, path)?;
    let verbatim = |s: crate::engine::tex::Span| {
        analysis.facts.occurrences.iter().any(|o| o.kind == OccKind::Catcode && o.key == "\\" && o.span == s)
    };
    let begins = analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::BeginEnvironment);
    let mut names: Vec<&str> =
        begins.filter(|o| o.span.file == file && verbatim(o.span)).map(|o| o.key.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    // listings ran there: it set its own state between `\begin` and `\end`, which a frame's or a
    // verbatim environment's body never does
    let listings = |(b, e): (u32, u32)| {
        let ran = |d: &crate::analysis::facts::Definition| b < d.node && d.node < e;
        analysis.facts.defs.iter().any(|d| ran(d) && analysis.interner.name(d.name).starts_with("lst@"))
    };
    let mut bodies = names.into_iter().filter_map(|env| super::body(analysis, doc, path, pos.line, env, true));
    let body = bodies.find(|b| listings(b.ids))?;
    let start = doc.offset(super::super::handlers::to_pos(doc, body.range.start))?;
    (start <= doc.offset(pos)?).then_some((start, body.ids))
}

/// The run's definition of `name` made inside the listing (its own options), else the meaning
/// it had when the run ended: what the listing reads.
fn setting(analysis: &Analysis, name: &str, ids: (u32, u32)) -> Option<String> {
    let text = |m: &crate::engine::tex::MacroDef| detokenize(&m.replacement_text, &analysis.interner);
    match defined_in(analysis, name, ids) {
        Some(d) => d.mac.as_deref().map(text),
        None => analysis.env.meaning(analysis.interner.lookup(name)?).as_macro().map(|m| text(m)),
    }
}

/// The last definition of `name` the run made between the listing's `\begin` and `\end`.
fn defined_in<'a>(
    analysis: &'a Analysis,
    name: &str,
    (begin, end): (u32, u32),
) -> Option<&'a crate::analysis::facts::Definition> {
    let sym: Sym = analysis.interner.lookup(name)?;
    analysis.facts.defs.iter().filter(|d| d.name == sym && begin < d.node && d.node < end).max_by_key(|d| d.node)
}

/// The `{open}{close}` pairs of each `\lst@Escape` in `text`, as `\lst@DefEsc` holds them.
fn escapes(text: &str) -> Vec<(String, String)> {
    let plain = |s: &str| s.strip_prefix('\\').filter(|r| r.chars().count() == 1).unwrap_or(s).to_string();
    text.split("\\lst@Escape")
        .skip(1)
        .filter_map(|rest| {
            let groups = crate::analysis::roles::braced_groups(rest);
            let (open, close) = (plain(groups.first()?), plain(groups.get(1)?));
            Some((open, close)).filter(|(o, c)| !o.is_empty() && !c.is_empty())
        })
        .collect()
}

/// Whether the end of `text` stands between `open` and `close`.
fn inside(text: &str, open: &str, close: &str) -> bool {
    if open == close {
        return text.matches(open).count() % 2 == 1;
    }
    match (text.rfind(open), text.rfind(close)) {
        (Some(o), Some(c)) => o + open.len() > c + close.len(),
        (Some(_), None) => true,
        _ => false,
    }
}

/// The `{sequence}{replacement}{length}` items of a `literate=` value, each sequence with what it
/// becomes.
fn sequences(literate: &str) -> Vec<(String, String)> {
    let mut groups = Vec::new();
    let mut rest = literate;
    while let Some(open) = rest.find('{') {
        let Some(close) = super::closing(rest, open, '{') else { break };
        groups.push(rest[open + 1..close].trim().to_string());
        rest = &rest[close + 1..];
    }
    let shown = |g: &str| g.strip_prefix('{').and_then(|r| r.strip_suffix('}')).unwrap_or(g).to_string();
    let pairs = groups.chunks(2).filter(|p| p.len() == 2);
    pairs.map(|p| (p[0].clone(), shown(&p[1]))).filter(|(s, _)| !s.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn literate_and_escapes_read_as_listings_stores_them() {
        let lit = super::sequences("{:yields:}{{$\\Rightarrow$}}1 {->}{$\\to$}2");
        assert_eq!(lit, [(":yields:".into(), "$\\Rightarrow$".into()), ("->".into(), "$\\to$".into())]);
        assert_eq!(super::escapes("\\lst@Escape {(*}{*)}{}{}"), [("(*".into(), "*)".into())]);
        assert!(super::inside("x (* \\ref{", "(*", "*)"));
        assert!(!super::inside("x (* a *) \\ref{", "(*", "*)"));
        assert!(super::inside("a $\\al", "$", "$"));
    }
}
