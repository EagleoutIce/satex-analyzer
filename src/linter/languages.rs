//! Code languages a listings key names ([`crate::queries::languages`]): a value no definition of
//! the run and no language file of listings declares.

use super::{Category, Report, Rule, Severity};
use crate::analysis::roles::{KeyKind, Ty, value_key};
use crate::engine::source::{byte_of_col, col_of_byte};
use crate::engine::tex::Span;
use crate::fixed::HashSet;
use crate::queries::languages::{listings, listings_key};

pub static RULES: &[Rule] = &[Rule {
    code: "unknown-code-language",
    category: Category::Correctness,
    severity: Severity::Warning,
    summary: "a listings key names a language nothing defines",
    explanation: "\
listings finds a language through the driver `\\lstlang@⟨language⟩$⟨dialect⟩`, which
`\\lstdefinelanguage` defines and the language files (`\\lstlanguagefiles`, `lstlang1.sty`
and its siblings) declare. A name none of them defines raises `Couldn't load requested
language`, and the listing is typeset without highlighting.

The rule reads the keys whose values the run formed such a driver from (`language=`), and
checks every value of those keys against the run's definitions and the declarations of the
language files. Where the run did not know the list of language files, the finding is a note.

Fix by correcting the name, or by defining the language with `\\lstdefinelanguage`.",
    references: &[],
    fixes: &[],
    engines: &[],
    run: unknown_language,
}];

fn unknown_language(report: &mut Report) {
    let analysis = report.analysis();
    let Some((langs, whole)) = listings(analysis) else { return };
    let known: HashSet<String> = langs.iter().map(|l| listings_key(&l.name)).collect();
    let mut texts: crate::fixed::HashMap<_, Option<String>> = Default::default();
    let mut text = |file| {
        texts
            .entry(file)
            .or_insert_with(|| {
                let path = &analysis.files.get(usize::from(file))?.path;
                crate::project::overlay::read_to_string(std::path::Path::new(path)).ok()
            })
            .clone()
    };
    let line_of = |text: &str, n: u32| text.lines().nth(n.saturating_sub(1) as usize).map(String::from);
    let slots = &analysis.facts.slots;
    // each key, by the slot the run formed from its name, whose value it formed a driver from
    let mut keys: HashSet<(String, String)> = HashSet::default();
    let typed = analysis.facts.types.iter().filter(|(_, t)| **t == Ty::Key(KeyKind::Language));
    for (s, _) in typed {
        let Some(line) = text(s.file).and_then(|t| line_of(&t, s.line)) else { continue };
        let Some(key) = value_key(&line, byte_of_col(&line, s.col)) else { continue };
        let col = col_of_byte(&line, key.as_ptr() as usize - line.as_ptr() as usize);
        if let Some(prefix) = slots.get(&Span { col, ..*s }) {
            keys.insert((prefix.clone(), key.to_string()));
        }
    }
    let mut found: Vec<(Span, String)> = Vec::new();
    for (s, prefix) in slots.iter().filter(|(s, _)| analysis.project_file(s.file)) {
        let Some(line) = text(s.file).and_then(|t| line_of(&t, s.line)) else { continue };
        let at = byte_of_col(&line, s.col).min(line.len());
        let rest = &line[at..];
        let Some((_, key)) = keys.iter().find(|(p, k)| p == prefix && rest.starts_with(k.as_str())) else { continue };
        let Some(after) = rest[key.len()..].trim_start().strip_prefix('=') else { continue };
        let value = after.trim_start();
        let end = end_of_value(value);
        let written = value[..end].trim();
        let name = written.strip_prefix('{').and_then(|w| w.strip_suffix('}')).unwrap_or(written).trim();
        if name.is_empty() || name.contains(['\\', '#', '{']) || known.contains(&listings_key(name)) {
            continue;
        }
        let col = col_of_byte(&line, line.len() - value.len());
        found.push((Span { col, ..*s }, name.to_string()));
    }
    found.sort_by_key(|(s, _)| (s.file, s.line, s.col));
    found.dedup();
    let severity = if whole { Severity::Warning } else { Severity::Info };
    for (span, name) in found {
        let message = format!("listings defines no code language `{name}`");
        report.add_as(severity, span, &name, message, None);
    }
}

/// The bytes of a key's value written at the start of `value`: up to a `,`, `]` or `}` outside
/// its groups (a dialect's `[…]`).
fn end_of_value(value: &str) -> usize {
    let mut depth = 0i32;
    for (i, c) in value.char_indices() {
        match c {
            '{' | '[' => depth += 1,
            '}' | ']' if depth > 0 => depth -= 1,
            ',' | ']' | '}' if depth == 0 => return i,
            _ => {}
        }
    }
    value.len()
}
