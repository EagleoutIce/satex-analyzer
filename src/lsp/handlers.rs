//! Feature handlers: each turns an [`Analysis`] satex already built into the
//! `lsp_types` response for one request.  No analysis of its own happens
//! here — everything comes from [`crate::query`] and [`crate::lint`], the
//! same calls `satex query`/`satex lint` make; this module only converts
//! between LSP's positions (0-based, UTF-16 columns) and satex's
//! ([`Pos`]: 1-based, character columns) and shapes the JSON as `lsp_types`
//! structs.

use std::sync::OnceLock;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, DocumentHighlight, DocumentHighlightKind,
    GotoDefinitionResponse, Hover, HoverContents, InsertTextFormat, Location, MarkupContent, MarkupKind, Position,
    PrepareRenameResponse, Range, SymbolInformation, SymbolKind, TextEdit, Uri, WorkspaceEdit,
};
use regex::Regex;
use serde_json::Value as Json;

use crate::lint::fix::{Pos, Sources, Text};
use crate::machine::Analysis;
use crate::query::{self, Filter, Query, Record};

/// LSP position (0-based line, UTF-16 column) to satex's (1-based line,
/// character column), against the line as it stands in `doc`.
pub fn to_pos(doc: &Text, position: Position) -> Pos {
    let line = position.line + 1;
    let text = doc.line(line);
    let mut utf16 = 0u32;
    let mut col = 1u32;
    for ch in text.chars() {
        if utf16 >= position.character {
            break;
        }
        utf16 += ch.len_utf16() as u32;
        col += 1;
    }
    Pos::new(line, col)
}

/// The reverse of [`to_pos`], through [`Text::utf16`]; falls back to a
/// 0-based, byte-counted guess when `pos` is out of range (a stale position
/// from a definition site an edit has since moved).
fn to_lsp(doc: &Text, pos: Pos) -> Position {
    match doc.utf16(pos) {
        Some((line, character)) => Position { line, character },
        None => Position { line: pos.line.saturating_sub(1), character: pos.col.saturating_sub(1) },
    }
}

fn char_byte(line: &str, char_idx: u32) -> usize {
    line.char_indices().nth(char_idx as usize).map(|(i, _)| i).unwrap_or(line.len())
}

fn word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\[A-Za-z@]+|\\.|[A-Za-z0-9_:.+*-]+").unwrap())
}

/// The control sequence or key under the cursor: its name (without a
/// leading `\`) and whether it had one.  `None` off the end of every match
/// on the line.
pub fn word_at(line: &str, col: u32) -> Option<(String, bool)> {
    word_span(line, col).map(|(name, is_command, _)| (name, is_command))
}

/// The same, with the 1-based column the match starts at.
pub fn word_span(line: &str, col: u32) -> Option<(String, bool, u32)> {
    let target = char_byte(line, col.saturating_sub(1));
    word_re().find_iter(line).find(|m| m.start() <= target && target <= m.end()).map(|m| {
        let text = m.as_str();
        (
            text.trim_start_matches('\\').to_string(),
            text.starts_with('\\'),
            line[..m.start()].chars().count() as u32 + 1,
        )
    })
}

/// What a completion right before the cursor is for, read off the raw text:
/// `\begin{`, `\ref{`/`\cite{` and their relatives ask for a name from the
/// document rather than the command list.
enum Context {
    Environment,
    Ref,
    Cite,
    Command,
}

fn completion_context(line: &str, col: u32) -> Context {
    let prefix = &line[..char_byte(line, col.saturating_sub(1)).min(line.len())];
    static ENV: OnceLock<Regex> = OnceLock::new();
    static REF: OnceLock<Regex> = OnceLock::new();
    static CITE: OnceLock<Regex> = OnceLock::new();
    let env = ENV.get_or_init(|| Regex::new(r"\\begin\{[A-Za-z*]*$").unwrap());
    let refs = REF.get_or_init(|| Regex::new(r"\\(ref|eqref|pageref|autoref|nameref|[Cc]ref)\*?\{[^}]*$").unwrap());
    let cite = CITE.get_or_init(|| Regex::new(r"\\[a-zA-Z]*cite[a-zA-Z]*\*?(\[[^]]*])*\{[^}]*$").unwrap());
    if env.is_match(prefix) {
        Context::Environment
    } else if refs.is_match(prefix) {
        Context::Ref
    } else if cite.is_match(prefix) {
        Context::Cite
    } else {
        Context::Command
    }
}

/// What a `file://` URI's path escapes: everything but the unreserved set
/// and the `/` separator (RFC 3986 § 2.3).
const PATH: &percent_encoding::AsciiSet =
    &percent_encoding::NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~').remove(b'/');

/// The path a run recorded a file under, resolved to a `file://` URI.
/// `lsp_types::Uri` (since 0.95) is a thin wrapper over a generic RFC 3986
/// URI, not a `url::Url`, so there is no `from_file_path`/`to_file_path`
/// left to reuse: this and [`uri_to_path`] are satex's own, covering exactly
/// the absolute-path-to-`file://`-URI case the server needs.
pub fn path_uri(path: &str) -> Uri {
    let path = std::path::Path::new(path);
    let absolute =
        if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
    let mut slash_path = absolute.to_string_lossy().replace('\\', "/");
    if !slash_path.starts_with('/') {
        slash_path.insert(0, '/');
    }
    format!("file://{}", percent_encoding::utf8_percent_encode(&slash_path, PATH))
        .parse()
        .unwrap_or_else(|_| "file:///".parse().unwrap())
}

/// The reverse of [`path_uri`]: the local path a `file://` URI names, or
/// `None` for any other scheme.
pub fn uri_to_path(uri: &Uri) -> Option<std::path::PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    Some(std::path::PathBuf::from(percent_encoding::percent_decode_str(rest).decode_utf8_lossy().into_owned()))
}

/// A `place`d record (`path`/`line`/`col`, as every query answer carries) as
/// an LSP [`Location`]: a zero-width range at the position, the same way
/// [`crate::render::lsp`] points a diagnostic at one place rather than a span.
fn record_location(record: &Record, sources: &Sources) -> Option<Location> {
    let path = record.get("path").and_then(Json::as_str)?;
    let line = record.get("line").and_then(Json::as_u64)? as u32;
    let col = record.get("col").and_then(Json::as_u64).unwrap_or(1) as u32;
    let position = match sources.get(path) {
        Some(text) => to_lsp(&text, Pos::new(line, col)),
        None => Position { line: line.saturating_sub(1), character: col.saturating_sub(1) },
    };
    Some(Location { uri: path_uri(path), range: Range { start: position, end: position } })
}

/// `textDocument/hover`: [`query::explain`]'s record(s) for the name under
/// the cursor — the preamble and document meanings when they differ, the
/// signature, and where it comes from.
pub fn hover(analysis: &Analysis, doc: &Text, position: Position) -> Option<Hover> {
    let pos = to_pos(doc, position);
    let (name, is_command) = word_at(doc.line(pos.line), pos.col)?;
    if !is_command {
        return None;
    }
    let (records, _) = query::explain(analysis, std::slice::from_ref(&name), false).ok()?;
    if records.is_empty() {
        return None;
    }
    let mut markdown = String::new();
    for record in &records {
        let when = record.get("when").and_then(Json::as_str);
        match when {
            Some(w) => markdown.push_str(&format!("### `\\{name}` — {w}\n\n")),
            None => markdown.push_str(&format!("### `\\{name}`\n\n")),
        }
        if let Some(sig) = record.get("signature").and_then(Json::as_str) {
            markdown.push_str(&format!("signature: `{sig}`\n\n"));
        }
        if let Some(effective) = record.get("effective").and_then(Json::as_str) {
            markdown.push_str(&format!("{effective}\n\n"));
        }
        if let (Some(file), Some(line)) =
            (record.get("file").and_then(Json::as_str), record.get("line").and_then(Json::as_u64))
        {
            markdown.push_str(&format!("defined at `{file}:{line}`\n\n"));
        }
        if let Some(text) = record.get("documentation").and_then(Json::as_str) {
            markdown.push_str(&format!("{text}\n\n"));
        }
    }
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: markdown }),
        range: None,
    })
}

/// `textDocument/definition`: every definition site [`query::explain`]
/// knows for the control sequence under the cursor, or the `\label`,
/// `\bibitem` or bibliography entry a key names.
pub fn definition(analysis: &Analysis, doc: &Text, position: Position) -> Option<GotoDefinitionResponse> {
    let pos = to_pos(doc, position);
    let (name, is_command) = word_at(doc.line(pos.line), pos.col)?;
    let sources = Sources::default();
    let mut records = if is_command {
        query::explain(analysis, std::slice::from_ref(&name), true).ok()?.0
    } else {
        let keys: Vec<Record> = query::run(analysis, Query::Occurrences, &Filter::Always)
            .into_iter()
            .filter(|r| {
                r.get("key").and_then(Json::as_str) == Some(name.as_str())
                    && r.get("kind")
                        .and_then(Json::as_str)
                        .is_some_and(|k| matches!(k, "label" | "bibitem" | "entry" | "key"))
            })
            .collect();
        // An environment name is defined as a control sequence.
        if keys.is_empty() { query::explain(analysis, std::slice::from_ref(&name), true).ok()?.0 } else { keys }
    };
    // A key declared through a macro of the project has two places: the
    // call in the file and the text the macro expanded to.
    let expanded: Vec<Record> =
        records.iter().filter_map(|r| r.get("expanded").and_then(Json::as_object).cloned()).collect();
    records.extend(expanded);
    let locations: Vec<Location> = records.iter().filter_map(|r| record_location(r, &sources)).collect();
    (!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations))
}

/// Every expansion (and, with `declarations`, definition) of a control
/// sequence, or every occurrence of a label/citation/environment key.
fn sites(analysis: &Analysis, name: &str, is_command: bool, declarations: bool) -> Vec<Record> {
    let named = |record: &Record, field: &str, want: &str| record.get(field).and_then(Json::as_str) == Some(want);
    if !is_command {
        return query::run(analysis, Query::Occurrences, &Filter::Always)
            .into_iter()
            .filter(|r| named(r, "key", name))
            .collect();
    }
    let target = format!("\\{name}");
    let mut kinds = vec![Query::Expansions];
    if declarations {
        kinds.push(Query::Definitions);
    }
    kinds
        .into_iter()
        .flat_map(|q| query::run(analysis, q, &Filter::Always))
        .filter(|r| named(r, "name", &target))
        .collect()
}

/// The sites of the name under the cursor that share its meaning: the
/// definition the cursor is on or resolves to, and the calls that resolved
/// to that same definition.  A redefinition or a same-named macro of another
/// binding is left out.  Falls back to every site of the name when the cursor
/// is on none of them.
fn bound_sites(
    analysis: &Analysis,
    path: &str,
    position: Position,
    name: &str,
    is_command: bool,
    declarations: bool,
) -> Vec<Record> {
    let all = sites(analysis, name, is_command, declarations);
    if !is_command {
        return all;
    }
    let binding = |r: &Record| r.get("definition").or_else(|| r.get("node")).and_then(Json::as_u64);
    let sources = Sources::default();
    let here = (position.line, position.character);
    let at_cursor = sites(analysis, name, is_command, true).into_iter().find_map(|r| {
        let (p, range) = site_range(&r, &sources, name, is_command)?;
        let inside = (range.start.line, range.start.character) <= here && here <= (range.end.line, range.end.character);
        (p == path && inside).then(|| binding(&r)).flatten()
    });
    match at_cursor {
        Some(id) => all.into_iter().filter(|r| binding(r) == Some(id)).collect(),
        None => all,
    }
}

/// `textDocument/references`: every expansion of a control sequence, or
/// every occurrence of a label/citation/environment key, under the cursor.
pub fn references(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    position: Position,
    declarations: bool,
) -> Option<Vec<Location>> {
    let pos = to_pos(doc, position);
    let (name, is_command) = word_at(doc.line(pos.line), pos.col)?;
    let sources = Sources::default();
    let locations: Vec<Location> = bound_sites(analysis, path, position, &name, is_command, declarations)
        .iter()
        .filter_map(|r| record_location(r, &sources))
        .collect();
    (!locations.is_empty()).then_some(locations)
}

/// The span of the name a site record points at: `\name` without its
/// backslash, or the key after the first `{` at or past the recorded column.
fn site_range(record: &Record, sources: &Sources, name: &str, is_command: bool) -> Option<(String, Range)> {
    let path = record.get("path").and_then(Json::as_str)?;
    let line = record.get("line").and_then(Json::as_u64)? as u32;
    let col = record.get("col").and_then(Json::as_u64).unwrap_or(1) as u32;
    let text = sources.get(path)?;
    let text_line = text.line(line);
    let from = char_byte(text_line, col.saturating_sub(1));
    let needle = if is_command { format!("\\{name}") } else { name.to_string() };
    let mut search = from;
    if !is_command {
        search += text_line[from..].find('{').map_or(0, |i| i + 1);
    }
    let found = text_line[search..].match_indices(&needle).map(|(i, _)| search + i).find(|&at| {
        let after = text_line[at + needle.len()..].chars().next();
        !(is_command
            && name.chars().all(|c| c.is_ascii_alphabetic() || c == '@')
            && after.is_some_and(|c| c.is_ascii_alphabetic() || c == '@'))
    })?;
    let first = text_line[..found].chars().count() as u32 + 1 + u32::from(is_command);
    let width = name.chars().count() as u32;
    let range =
        Range { start: to_lsp(&text, Pos::new(line, first)), end: to_lsp(&text, Pos::new(line, first + width)) };
    Some((path.to_string(), range))
}

/// `textDocument/documentHighlight`: this file's spans of the name under
/// the cursor, definitions included.
pub fn highlights(analysis: &Analysis, doc: &Text, path: &str, position: Position) -> Option<Vec<DocumentHighlight>> {
    let pos = to_pos(doc, position);
    let (name, is_command) = word_at(doc.line(pos.line), pos.col)?;
    let sources = Sources::default();
    let out: Vec<DocumentHighlight> = bound_sites(analysis, path, position, &name, is_command, true)
        .iter()
        .filter_map(|r| site_range(r, &sources, &name, is_command))
        .filter(|(p, _)| p == path)
        .map(|(_, range)| DocumentHighlight { range, kind: Some(DocumentHighlightKind::TEXT) })
        .collect();
    (!out.is_empty()).then_some(out)
}

/// The rename the name under the cursor asks for, against the files as
/// they stand.
fn rename_plan(analysis: &Analysis, doc: &Text, path: &str, position: Position) -> Result<crate::rename::Plan, String> {
    let pos = to_pos(doc, position);
    let (name, is_command, col) = word_span(doc.line(pos.line), pos.col).ok_or("nothing renamable here")?;
    crate::rename::plan(analysis, &Sources::default(), path, Pos::new(pos.line, col), &name, is_command)
}

/// `textDocument/prepareRename`: the span under the cursor and its name.
pub fn prepare_rename(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let plan = rename_plan(analysis, doc, path, position).ok()?;
    let site = plan.at(path, to_pos(doc, position))?;
    let range = Range { start: to_lsp(doc, site.start), end: to_lsp(doc, site.end) };
    Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder: plan.key.clone() })
}

/// `textDocument/rename`: the name under the cursor, every name its
/// declaration built with it, and every site that spells it.
pub fn rename(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    position: Position,
    new_name: &str,
) -> Result<WorkspaceEdit, String> {
    let plan = rename_plan(analysis, doc, path, position)?;
    let sources = Sources::default();
    let mut changes: std::collections::HashMap<Uri, Vec<TextEdit>> = std::collections::HashMap::new();
    for edit in crate::rename::edits(&plan, analysis, new_name)? {
        let text = sources.get(&edit.path).ok_or_else(|| format!("{} cannot be read", edit.path))?;
        let range = Range { start: to_lsp(&text, edit.start), end: to_lsp(&text, edit.end) };
        changes.entry(path_uri(&edit.path)).or_default().push(TextEdit { range, new_text: edit.replacement });
    }
    Ok(WorkspaceEdit { changes: Some(changes), ..Default::default() })
}

/// `workspace/symbol`: the document's own macro definitions, sections,
/// labels and environments whose name contains `query`, case-insensitively.
#[allow(deprecated)]
pub fn workspace_symbols(analysis: &Analysis, query_text: &str) -> Vec<SymbolInformation> {
    let needle = query_text.to_lowercase();
    let sources = Sources::default();
    let mut out = Vec::new();
    let mut add = |record: &Record, name: &str, kind: SymbolKind| {
        if !name.is_empty()
            && name.to_lowercase().contains(&needle)
            && let Some(location) = record_location(record, &sources)
        {
            out.push(SymbolInformation {
                name: name.to_string(),
                kind,
                tags: None,
                deprecated: None,
                location,
                container_name: None,
            });
        }
    };
    for record in query::run(analysis, Query::Occurrences, &Filter::Always) {
        let kind = match record.get("kind").and_then(Json::as_str) {
            Some("section") => SymbolKind::NAMESPACE,
            Some("label") => SymbolKind::CONSTANT,
            Some("begin-environment") => SymbolKind::MODULE,
            _ => continue,
        };
        if let Some(key) = record.get("key").and_then(Json::as_str) {
            add(&record, key, kind);
        }
    }
    for record in query::run(analysis, Query::Definitions, &Filter::Always) {
        if record.get("origin").and_then(Json::as_str) == Some("document")
            && let Some(name) = record.get("name").and_then(Json::as_str)
        {
            add(&record, name, SymbolKind::FUNCTION);
        }
    }
    out
}

/// `textDocument/completion`: [`query::scope`] after a bare `\`, environment
/// names inside `\begin{`, and label/citation keys inside `\ref{`/`\cite{`.
pub fn completion(analysis: &Analysis, doc: &Text, position: Position) -> CompletionResponse {
    let pos = to_pos(doc, position);
    let line = doc.line(pos.line);
    let items = match completion_context(line, pos.col) {
        Context::Environment => key_items(analysis, &["begin-environment"], CompletionItemKind::MODULE),
        Context::Ref => key_items(analysis, &["label"], CompletionItemKind::REFERENCE),
        Context::Cite => key_items(analysis, &["cite", "bibitem"], CompletionItemKind::REFERENCE),
        Context::Command => command_items(analysis, pos),
    };
    CompletionResponse::Array(items)
}

fn key_items(analysis: &Analysis, kinds: &[&str], kind: CompletionItemKind) -> Vec<CompletionItem> {
    let mut names: Vec<String> = query::run(analysis, Query::Occurrences, &Filter::Always)
        .iter()
        .filter(|r| r.get("kind").and_then(Json::as_str).is_some_and(|k| kinds.contains(&k)))
        .filter_map(|r| r.get("key").and_then(Json::as_str).map(str::to_string))
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names.into_iter().map(|label| CompletionItem { label, kind: Some(kind), ..Default::default() }).collect()
}

/// Command names in scope at `pos`, each with a snippet for its mandatory
/// arguments when [`query::Query::Definitions`] recorded how many it takes.
fn command_items(analysis: &Analysis, pos: Pos) -> Vec<CompletionItem> {
    let takes: std::collections::HashMap<String, u8> = query::run(analysis, Query::Definitions, &Filter::Always)
        .into_iter()
        .filter_map(|r| {
            let name = r.get("name")?.as_str()?.to_string();
            let min = r.get("takes")?.get("min")?.as_u64()? as u8;
            Some((name, min))
        })
        .collect();
    query::scope(analysis, Some((analysis.main_file, pos.line, pos.col)), false)
        .into_iter()
        .filter_map(|record| {
            let name = record.get("name")?.as_str()?.to_string();
            let label = name.trim_start_matches('\\').to_string();
            if label.is_empty() {
                return None;
            }
            let tag = record.get("tag").and_then(Json::as_str).unwrap_or("");
            let effective = record.get("effective").and_then(Json::as_str).unwrap_or("");
            let package = record.get("package").and_then(Json::as_str).unwrap_or("");
            let (insert_text, format) = match takes.get(&name).copied().unwrap_or(0) {
                0 => (label.clone(), InsertTextFormat::PLAIN_TEXT),
                min => {
                    let args: String = (1..=min).map(|i| format!("{{${i}}}")).collect();
                    (format!("{label}{args}"), InsertTextFormat::SNIPPET)
                }
            };
            Some(CompletionItem {
                label,
                kind: Some(if tag == "primitive" { CompletionItemKind::KEYWORD } else { CompletionItemKind::FUNCTION }),
                detail: Some(format!("{effective}  ({package})")),
                insert_text: Some(insert_text),
                insert_text_format: Some(format),
                ..Default::default()
            })
        })
        .collect()
}

/// `textDocument/documentSymbol`: sections, labels and environments from
/// [`query::Query::Occurrences`], and this file's own macro/environment
/// definitions — a flat list (`SymbolInformation`), which needs no nesting
/// logic to stay a thin wrapper over the query records.
#[allow(deprecated)]
pub fn document_symbols(analysis: &Analysis, path: &str, doc: &Text) -> Option<Vec<SymbolInformation>> {
    let symbol = |name: String, kind: SymbolKind, line: u32, col: u32| SymbolInformation {
        name,
        kind,
        tags: None,
        deprecated: None,
        location: Location {
            uri: path_uri(path),
            range: Range { start: to_lsp(doc, Pos::new(line, col)), end: to_lsp(doc, Pos::new(line, col)) },
        },
        container_name: None,
    };
    let mut out = Vec::new();
    for record in query::run(analysis, Query::Occurrences, &Filter::Always) {
        if record.get("path").and_then(Json::as_str) != Some(path) {
            continue;
        }
        let kind = record.get("kind").and_then(Json::as_str).unwrap_or("");
        let symbol_kind = match kind {
            "section" => SymbolKind::NAMESPACE,
            "label" => SymbolKind::CONSTANT,
            "begin-environment" => SymbolKind::MODULE,
            _ => continue,
        };
        let Some(name) = record.get("key").and_then(Json::as_str).filter(|n| !n.is_empty()) else { continue };
        let line = record.get("line").and_then(Json::as_u64).unwrap_or(1) as u32;
        let col = record.get("col").and_then(Json::as_u64).unwrap_or(1) as u32;
        out.push(symbol(name.to_string(), symbol_kind, line, col));
    }
    for record in query::run(analysis, Query::Definitions, &Filter::Always) {
        if record.get("origin").and_then(Json::as_str) != Some("document")
            || record.get("path").and_then(Json::as_str) != Some(path)
        {
            continue;
        }
        let Some(name) = record.get("name").and_then(Json::as_str) else { continue };
        let line = record.get("line").and_then(Json::as_u64).unwrap_or(1) as u32;
        let col = record.get("col").and_then(Json::as_u64).unwrap_or(1) as u32;
        out.push(symbol(name.to_string(), SymbolKind::FUNCTION, line, col));
    }
    (!out.is_empty()).then_some(out)
}

/// `lint`'s findings, in `render::lsp`'s shape: `[{uri, diagnostics,
/// codeActions}, …]`, one entry per file the findings touch.
pub fn diagnostics_json(analysis: &Analysis) -> Vec<Json> {
    let records: Vec<Record> = crate::lint::lint(analysis)
        .into_iter()
        .filter(|r| r.get("origin").and_then(Json::as_str) == Some("document"))
        .collect();
    let text = crate::render::lsp(&records);
    serde_json::from_str(&text).unwrap_or_default()
}
