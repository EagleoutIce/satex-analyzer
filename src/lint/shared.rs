use std::collections::{BTreeMap, HashMap};

use crate::builtins::OccKind;
use crate::machine::Analysis;
use crate::tex::{Span, Sym};

/// Whether the command that made `def` is one that defines: `\def`,
/// `\newcommand` and their relatives.  `\section` and `\refstepcounter`
/// define `\@currentlabel` as the kernel's own bookkeeping, which is not a
/// definition the document wrote.
pub fn written_definition(analysis: &Analysis, def: &crate::facts::Definition) -> bool {
    use crate::builtins::Primitive as P;
    // Made inside a `\usepackage`/`\documentclass` call, so made by the
    // loading machinery: the `\ver@` stamp, the option lists, the hooks.
    let in_load = analysis.facts.loads.iter().any(|load| {
        load.span.file == def.span.file && load.span.line == def.span.line && load.span.col <= def.span.col
    });
    if in_load {
        return false;
    }
    matches!(analysis.env.meaning(def.by).prim(), None | Some(P::Def { .. }))
}

/// Whether the message was raised by `\\errmessage`, under whatever name
/// the code called it (`\\tex_errmessage:D`, an alias): what stops a real
/// run (tex.web § 1283).
pub fn is_error(analysis: &Analysis, o: &crate::facts::Occurrence) -> bool {
    o.kind == OccKind::Message
        && o.detail.as_deref().and_then(|d| analysis.interner.lookup(d)).and_then(|d| analysis.env.meaning(d).prim())
            == Some(crate::builtins::Primitive::Message { error: true })
}

pub fn first_loads(analysis: &Analysis) -> HashMap<&str, (Vec<String>, Span)> {
    let mut seen: HashMap<&str, (Vec<String>, Span)> = HashMap::new();
    for load in &analysis.facts.loads {
        if load.kind.is_package() {
            seen.entry(&load.name).or_insert_with(|| (load.options.clone(), load.span));
        }
    }
    seen
}

pub fn definition_order(analysis: &Analysis) -> HashMap<Sym, Span> {
    let mut first: HashMap<Sym, Span> = HashMap::new();
    for def in &analysis.facts.defs {
        first.entry(def.name).or_insert(def.span);
    }
    first
}

pub fn labels(analysis: &Analysis) -> BTreeMap<&str, Span> {
    let mut out = BTreeMap::new();
    for occurrence in &analysis.facts.occurrences {
        if occurrence.kind == OccKind::Label {
            out.entry(occurrence.key.as_str()).or_insert(occurrence.span);
        }
    }
    out
}

pub fn document_start(analysis: &Analysis) -> Option<Span> {
    analysis
        .facts
        .occurrences
        .iter()
        .find(|o| o.kind == OccKind::BeginEnvironment && o.key == "document")
        .map(|o| o.span)
}
