//! Hover over typed text: the type [`super::type_at::type_at`] finds says
//! what the text names, one small helper per kind says what the run knows of it.

use super::handlers::{cite_hover, command_hover, key_hover, load_hover};
use crate::analysis::facts::Occurrence;
use crate::analysis::machine::Analysis;
use crate::analysis::roles::{FileKind, KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::engine::tex::RegKind;
use crate::linter::fix::{Pos, Text};
use std::path::Path;

/// What the text under `pos` names, by its types, the most precise first.
pub(super) fn typed_hover(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> Option<String> {
    let line = doc.line(pos.line);
    // the whole name: asked at its end
    let at = crate::engine::source::byte_of_col(line, pos.col).min(line.len());
    let rest = line[at..].chars().take_while(|c| !super::type_at::ENDS.contains(*c)).count() as u32;
    let found = super::type_at::type_at(analysis, doc, path, Pos::new(pos.line, pos.col + rest));
    let key = found.item.typed.trim();
    found.hits.iter().find_map(|hit| {
        let key = hit.typed.as_deref().unwrap_or(key);
        match hit.ty {
            _ if key.is_empty() => None,
            Ty::Key(KeyKind::Cite) => cite_hover(analysis, key),
            Ty::Key(KeyKind::Label) => label_hover(analysis, &format!("{}{key}", hit.prefix)),
            Ty::Key(KeyKind::Environment) => environment_hover(analysis, path, pos, key),
            Ty::Key(KeyKind::Option | KeyKind::Pgf | KeyKind::Declared) => key_hover(analysis, line, pos.col),
            Ty::Key(KeyKind::Color) => color_hover(analysis, key),
            Ty::File(FileKind::Cls | FileKind::Sty) => load_hover(analysis, path, pos, key),
            Ty::File(kind) => file_hover(analysis, path, pos, key, kind),
            Ty::Named(RegKind::Count) => command_hover(analysis, &format!("c@{key}"), None),
            Ty::Dimen | Ty::Glue => super::units::hover(line, pos.col),
            _ => None,
        }
    })
}

/// A color's name: its value where the run defined it.
fn color_hover(analysis: &Analysis, key: &str) -> Option<String> {
    let c = analysis.facts.colors.iter().rev().find(|c| c.name == key)?;
    let value = c.value.as_ref().and_then(super::handlers::rgba);
    let shown = value.map(|v| format!("rgb {:.2} {:.2} {:.2}", v.red, v.green, v.blue));
    Some(format!("### `{key}`: color\n\n{}\n", shown.unwrap_or_else(|| "a value the run did not fix".into())))
}

/// A file argument: the load the run made of it, else the file beside the document when there is one.
fn file_hover(analysis: &Analysis, path: &str, pos: Pos, key: &str, kind: FileKind) -> Option<String> {
    if let Some(md) = load_hover(analysis, path, pos, key) {
        return Some(md);
    }
    let beside = Path::new(path).parent()?;
    let named = beside.join(key);
    let found =
        [Some(named.clone()), kind.ext().map(|e| named.with_extension(e))].into_iter().flatten().find(|p| p.is_file());
    Some(format!("### `{key}`: {}\n\n`{}`\n", kind.label(), found?.display()))
}

/// The `\begin`/`\end` pair of the environment `key` the cursor's line opens or closes, and its definition.
fn environment_hover(analysis: &Analysis, path: &str, pos: Pos, key: &str) -> Option<String> {
    let mut delimiters: Vec<&Occurrence> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment))
        .filter(|o| o.key == key && analysis.file_name(o.span.file) == path)
        .collect();
    delimiters.sort_by_key(|o| (o.span.line, o.span.col));
    let mut open: Vec<&Occurrence> = Vec::new();
    let mut pairs: Vec<(&Occurrence, &Occurrence)> = Vec::new();
    for o in delimiters {
        match o.kind {
            OccKind::BeginEnvironment => open.push(o),
            _ => pairs.extend(open.pop().map(|b| (b, o))),
        }
    }
    let mut md = format!("### `{key}`: environment\n\n");
    if let Some((b, e)) = pairs.iter().find(|(b, e)| b.span.line == pos.line || e.span.line == pos.line) {
        md.push_str(&format!("`\\begin` on line {}, `\\end` on line {}\n\n", b.span.line, e.span.line));
    }
    match command_hover(analysis, key, None) {
        Some(definition) => md.push_str(&definition),
        None => md.push_str("No definition recorded by the run.\n"),
    }
    Some(md)
}

/// A label or reference key: where it is defined, how often it is referenced and what a reference typesets.
fn label_hover(analysis: &Analysis, key: &str) -> Option<String> {
    let named = |kind| analysis.facts.occurrences.iter().filter(move |o| o.key == key && o.kind == kind);
    let defined = named(OccKind::Label).next();
    let refs: Vec<&Occurrence> = named(OccKind::Ref).collect();
    if defined.is_none() && refs.is_empty() {
        return None;
    }
    let place = |o: &Occurrence| format!("`{}:{}`", analysis.short_name(o.span.file), o.span.line);
    let mut md = format!("### `{key}`: label\n\n");
    match defined {
        Some(o) => md.push_str(&format!("Defined at {}\n\n", place(o))),
        None => md.push_str("Not defined by the run.\n\n"),
    }
    md.push_str(&format!("{} reference(s)\n\n", refs.len()));
    if let Some(set) = refs.iter().find_map(|o| crate::queries::query::typeset_by(analysis, o.span)) {
        md.push_str(&format!("A reference typesets {set}\n\n"));
    }
    Some(md)
}
