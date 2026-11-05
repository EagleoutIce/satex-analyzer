//! Completion in a `.bib` file: the entry types after `@` and the fields
//! inside an entry, read from the bibliography style the document uses
//! (a `.bst`'s `ENTRY` and entry functions, biblatex's data model).

use lsp_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, Range, TextEdit};

use super::handlers::{style_file, to_lsp};
use crate::analysis::machine::Analysis;
use crate::engine::builtins::OccKind;
use crate::text::{Pos, Text};

/// The database commands every `.bib` file may use, whatever the style.
const COMMANDS: [&str; 3] = ["string", "preamble", "comment"];

/// What the cursor completes.
#[derive(PartialEq)]
enum Place {
    Type,
    Field,
}

/// The entry types and field names of the document's style.
#[derive(Default)]
struct Model {
    types: Vec<String>,
    fields: Vec<String>,
    /// Per entry type, the fields its style reads of it.
    of_type: Vec<(String, Vec<String>)>,
}

/// Per entry type of the document's style, the fields the style reads of it: what a new
/// entry of that type is made with.
pub(super) fn type_fields(analysis: &Analysis, path: &str) -> Vec<(String, Vec<String>)> {
    model(analysis, path).of_type
}

/// The place the cursor is in and the word typed so far, or `None` in a
/// value, a key, a comment or before a command.
fn place(before: &str) -> Option<(Place, &str)> {
    let (mut depth, mut comma, mut equals, mut start) = (0u32, false, false, 0);
    let mut chars = before.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '@' if depth == 0 => (comma, equals, start) = (false, false, i + 1),
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 1 => (comma, equals) = (true, false),
            '=' if depth == 1 => equals = true,
            _ => {}
        }
    }
    let word = before.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    match depth {
        0 if word.ends_with('@') && word.len() == start => Some((Place::Type, &before[word.len()..])),
        1 if comma && !equals && word.trim_end().ends_with(['{', ',']) => Some((Place::Field, &before[word.len()..])),
        _ => None,
    }
}

/// The entry types and fields offered at `pos` of the `.bib` file `path`, or
/// `None` where the command completion applies.
pub fn completion(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> Option<Vec<CompletionItem>> {
    if !path.ends_with(".bib") {
        return None;
    }
    let before = &doc.body()[..doc.offset(pos)?];
    let Some((place, typed)) = place(before) else {
        // a command after `\` completes as in a `.tex` file, no other word does
        return (!super::handlers::cs_re().is_match(before)).then(Vec::new);
    };
    let model = model(analysis, path);
    let (names, detail) = match place {
        Place::Type => (model.types.iter().map(String::as_str).chain(COMMANDS).collect::<Vec<_>>(), "entry type"),
        Place::Field => (model.fields.iter().map(String::as_str).collect(), "field"),
    };
    let start = to_lsp(doc, Pos::new(pos.line, pos.col - typed.chars().count() as u32));
    let range = Range { start, end: to_lsp(doc, pos) };
    let lower = typed.to_lowercase();
    Some(
        names
            .into_iter()
            .filter(|n| n.starts_with(&lower))
            .map(|n| CompletionItem {
                label: n.into(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(detail.into()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text: n.into() })),
                ..Default::default()
            })
            .collect(),
    )
}

/// The model of the `\bibliographystyle`'s `.bst` file, else of biblatex.
fn model(analysis: &Analysis, path: &str) -> Model {
    let read = |file: Option<std::path::PathBuf>| file.and_then(|f| std::fs::read_to_string(f).ok());
    let style = analysis.facts.occurrences.iter().find(|o| o.kind == OccKind::BibStyle);
    if let Some(text) = style.and_then(|s| read(style_file(analysis, path, &format!("{}.bst", s.key), None))) {
        return bst_model(&text);
    }
    let biblatex = analysis.facts.loads.iter().find(|l| l.name == "biblatex");
    let dm = biblatex.and_then(|l| l.path.as_deref()).and_then(|p| std::path::Path::new(p).parent());
    read(dm.map(|d| d.join("blx-dm.def"))).map(|t| datamodel(&t)).unwrap_or_default()
}

/// `text` without its `%` comments.
fn uncommented(text: &str) -> String {
    text.lines().map(|l| l.split('%').next().unwrap_or_default()).collect::<Vec<_>>().join("\n")
}

/// The braced group opening at the first `{` of `text`, and the rest after it.
fn group(text: &str) -> Option<(&str, &str)> {
    let open = text.find('{')?;
    let mut depth = 0;
    for (i, c) in text[open..].char_indices() {
        depth += match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        };
        if depth == 0 {
            return Some((&text[open + 1..open + i], &text[open + i + 1..]));
        }
    }
    None
}

/// A `.bst`: the fields its `ENTRY` command lists and the entry types, the
/// functions that start a bibitem.
fn bst_model(text: &str) -> Model {
    let text = uncommented(text);
    let fields: Vec<String> = text
        .split("ENTRY")
        .nth(1)
        .and_then(group)
        .map(|(g, _)| g.split_whitespace().map(str::to_lowercase).collect())
        .unwrap_or_default();
    let mut types = Vec::new();
    let mut bodies: Vec<(&str, &str)> = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("FUNCTION") {
        rest = &rest[at + "FUNCTION".len()..];
        let Some((name, after)) = group(rest) else { break };
        let Some((body, _)) = group(after) else { break };
        let name = name.trim();
        bodies.push((name, body));
        if body.contains("output.bibitem") && !name.contains(['.', '$']) {
            types.push(name.to_string());
        }
    }
    // the fields a type's function reads, itself or through the functions it calls
    let words = |body: &str| {
        body.split(|c: char| c.is_whitespace() || "{}\"#'".contains(c))
            .filter(|w| !w.is_empty())
            .map(String::from)
            .collect::<Vec<_>>()
    };
    let of_type = types
        .iter()
        .map(|t| {
            let mut seen = vec![t.clone()];
            let mut out: Vec<String> = Vec::new();
            let mut todo = vec![t.clone()];
            while let Some(f) = todo.pop() {
                let Some((_, body)) = bodies.iter().find(|b| b.0 == f) else { continue };
                for w in words(body) {
                    if fields.contains(&w) && !out.contains(&w) {
                        out.push(w);
                    } else if bodies.iter().any(|b| b.0 == w) && !seen.contains(&w) {
                        seen.push(w.clone());
                        todo.push(w);
                    }
                }
            }
            (t.clone(), out)
        })
        .collect();
    Model { types, fields, of_type }
}

/// biblatex's `blx-dm.def`: the names its `\DeclareDatamodelEntrytypes` and
/// `\DeclareDatamodelFields` declare.
fn datamodel(text: &str) -> Model {
    let text = uncommented(text);
    let mut model = Model::default();
    for (command, names) in [("Entrytypes", &mut model.types), ("Fields", &mut model.fields)] {
        for part in text.split(&format!("\\DeclareDatamodel{command}")).skip(1) {
            let part = part.trim_start();
            let part = if part.starts_with('[') { part.split_once(']').map_or(part, |(_, r)| r) } else { part };
            let Some((list, _)) = group(part) else { continue };
            let list = list.split(',').map(str::trim).filter(|n| !n.is_empty() && !n.contains(['\\', '#', ' ']));
            for n in list.map(String::from) {
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
    }
    // `\DeclareDatamodelEntryfields[type]{fields}`: per type
    for part in text.split("\\DeclareDatamodelEntryfields[").skip(1) {
        let Some((kinds, rest)) = part.split_once(']') else { continue };
        let Some((list, _)) = group(rest) else { continue };
        let names: Vec<String> = list.split(',').map(str::trim).filter(|n| !n.is_empty()).map(String::from).collect();
        for kind in kinds.split(',').map(str::trim) {
            match model.of_type.iter_mut().find(|t| t.0 == kind) {
                Some(t) => t.1.extend(names.iter().cloned()),
                None => model.of_type.push((kind.to_string(), names.clone())),
            }
        }
    }
    model
}
