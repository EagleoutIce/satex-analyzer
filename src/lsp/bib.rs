//! Completion in a `.bib` file: the entry types after `@` and the fields
//! inside an entry, read from the bibliography style the document uses
//! (a `.bst`'s `ENTRY` and entry functions, biblatex's data model).

use lsp_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, Range, TextEdit};

use super::handlers::to_lsp;
use crate::analysis::machine::Analysis;
use crate::plugins::bib::model::{self, Model};
use crate::text::{Pos, Text};

/// The database commands every `.bib` file may use, whatever the style.
const COMMANDS: [&str; 3] = ["string", "preamble", "comment"];

/// What the cursor completes.
#[derive(PartialEq)]
enum Place {
    Type,
    Field,
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
    let model: Model = model::of(analysis, path);
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
