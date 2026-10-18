//! Feature handlers: each turns an [`Analysis`] satex already built into the
//! `lsp_types` response for one request.  No analysis of its own happens
//! here — everything comes from [`crate::query`] and [`crate::lint`], the
//! same calls `satex query`/`satex lint` make; this module only converts
//! between LSP's positions (0-based, UTF-16 columns) and satex's
//! ([`Pos`]: 1-based, character columns) and shapes the JSON as `lsp_types`
//! structs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
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
/// column as the engine counts one), against the line as it stands in `doc`.
pub fn to_pos(doc: &Text, position: Position) -> Pos {
    let line = position.line + 1;
    Pos::new(line, doc.from_utf16(line, position.character))
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
    let target = crate::source::byte_of_col(line, col);
    word_re().find_iter(line).find(|m| m.start() <= target && target <= m.end()).map(|m| {
        let text = m.as_str();
        (text.trim_start_matches('\\').to_string(), text.starts_with('\\'), crate::source::col_of_byte(line, m.start()))
    })
}

/// The path a run recorded a file under, resolved to a `file://` URI.
/// `lsp_types::Uri` (since 0.95) is a thin wrapper over a generic RFC 3986
/// URI, not a `url::Url`, so there is no `from_file_path`/`to_file_path`
/// left to reuse: [`crate::paths::file_url`] and [`uri_to_path`] are
/// satex's own, covering exactly the case the server needs.
pub fn path_uri(path: &str) -> Uri {
    // The same rooting [`crate::render::file_url`] does, so a diagnostic and the file it
    // belongs to reach the client under one URI.
    let absolute = std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path));
    let absolute = VIEW.with_borrow(|view| view.named(&absolute));
    crate::paths::file_url(&absolute).parse().unwrap_or_else(|_| "file:///".parse().unwrap())
}

thread_local! {
    /// The paths the client named on the engine thread, which makes every URI.
    pub static VIEW: std::cell::RefCell<ClientView> = std::cell::RefCell::default();
}

/// Paths as the client named them, against what they resolve to.  satex
/// may reach a file by its resolved path (the working directory, a file
/// found from a resolved one), and a client that opened it through a
/// symlink only knows it by that name (#17).
#[derive(Default)]
pub struct ClientView(Vec<(PathBuf, PathBuf)>);

impl ClientView {
    pub fn add(&mut self, named: &Path) {
        let Ok(resolved) = std::fs::canonicalize(named) else { return };
        if resolved != named && !self.0.iter().any(|(r, _)| *r == resolved) {
            self.0.push((resolved, named.to_path_buf()));
            // The deepest name wins.
            self.0.sort_by_key(|(r, _)| std::cmp::Reverse(r.components().count()));
        }
    }

    /// `path` in the client's spelling: unchanged unless it lies under a
    /// path the client named by another name.
    pub fn named(&self, path: &Path) -> PathBuf {
        let resolved = std::fs::canonicalize(path);
        for (from, to) in &self.0 {
            let Some(rest) = resolved.as_deref().unwrap_or(path).strip_prefix(from).ok() else { continue };
            return if rest.as_os_str().is_empty() { to.clone() } else { to.join(rest) };
        }
        path.to_path_buf()
    }
}

/// The reverse of [`path_uri`]: the local path a `file://` URI names, or
/// `None` for any other scheme.
pub fn uri_to_path(uri: &Uri) -> Option<std::path::PathBuf> {
    crate::paths::from_file_url(uri.as_str())
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

/// `textDocument/hover`: for a package or class name in its load, what the
/// run knows about that file ([`load_hover`]); for a command,
/// [`query::explain`]'s record(s): a usage line from the signature, the
/// meanings when preamble and document differ, and where it comes from.
pub fn hover(analysis: &Analysis, doc: &Text, path: &str, position: Position) -> Option<Hover> {
    let pos = to_pos(doc, position);
    let (name, is_command) = word_at(doc.line(pos.line), pos.col)?;
    let markdown = if is_command {
        placed(analysis, command_hover(analysis, &name)?, &name, path, pos)
    } else {
        load_hover(analysis, path, pos, &name)?
    };
    Some(markdown_hover(markdown))
}

/// The hover of the control sequence `name` apart from where it stands:
/// what may be kept.
pub fn command_markdown(analysis: &Analysis, name: &str) -> Option<String> {
    command_hover(analysis, name)
}

/// `markdown` with the primitive's syntax under its first heading, where
/// `name` still means the primitive at `at`.
pub fn placed(analysis: &Analysis, mut markdown: String, name: &str, path: &str, at: Pos) -> String {
    if let Some(syntax) = super::primitives::syntax(analysis, name, path, at.line, at.col) {
        let first = markdown.find("\n\n").map_or(0, |i| i + 2);
        markdown.insert_str(first, &syntax);
    }
    markdown
}

/// A hover of `markdown`.
pub fn markdown_hover(markdown: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: markdown }),
        range: None,
    }
}

fn command_hover(analysis: &Analysis, name: &str) -> Option<String> {
    let (records, _) = query::explain(analysis, &[name.to_string()], false).ok()?;
    let mut markdown = String::new();
    if records.is_empty() {
        let path = analysis.file_name(analysis.main_file).to_string();
        let sites = super::unreached::sites(analysis, &path, name);
        if sites.is_empty() {
            return None;
        }
        markdown.push_str(&format!("### `\\{name}` — not reached by the analysis\n\n"));
        for site in &sites {
            markdown.push_str(&format!("`{}:{}` (source scan)\n\n", site.path, site.pos.line));
        }
    }
    for record in &records {
        let text = |key: &str| record.get(key).and_then(Json::as_str);
        match text("when") {
            Some(w) => markdown.push_str(&format!("### `\\{name}` — {w}\n\n")),
            None => markdown.push_str(&format!("### `\\{name}`\n\n")),
        }
        if let Some(sig) = text("signature").filter(|s| !s.is_empty()) {
            markdown.push_str(&format!("`\\{name}{}`\n\n", usage(sig)));
        }
        if let Some(doc) = documented(record) {
            if !doc.text.is_empty() {
                markdown.push_str(&format!("{}\n\n", doc.text));
            }
            for (n, text) in &doc.params {
                markdown.push_str(&format!("- `⟨arg{n}⟩` {text}\n"));
            }
            if !doc.params.is_empty() {
                markdown.push('\n');
            }
        }
        let mut foot = Vec::new();
        if let (Some(file), Some(line)) = (text("file"), record.get("line").and_then(Json::as_u64)) {
            foot.push(format!("`{file}:{line}`"));
        }
        if let (Some(doc), Some(url)) = (text("documentation"), text("reference")) {
            foot.push(format!("[{doc}]({url})"));
        } else if let Some(doc) = text("documentation") {
            markdown.push_str(&format!("{doc}\n\n"));
        }
        if !foot.is_empty() {
            markdown.push_str(&format!("{}\n\n", foot.join(" · ")));
        }
    }
    Some(markdown)
}

/// The comment block above the definition a record names.
fn documented(record: &Record) -> Option<super::doccomment::DocComment> {
    let path = record.get("path").and_then(Json::as_str)?;
    let line = record.get("line").and_then(Json::as_u64)? as usize;
    super::doccomment::above(&crate::overlay::read_to_string(std::path::Path::new(path)).ok()?, line)
}

/// The command whose argument the cursor is in, on its line: its name and
/// how many arguments before it are complete.
fn argument_of(line: &str, col: usize) -> Option<(String, u32)> {
    let chars: Vec<char> = line.chars().take(col).collect();
    let mut i = chars.len();
    // Out to the group the cursor is in.
    let mut depth = 0;
    loop {
        i = i.checked_sub(1)?;
        match chars[i] {
            '}' | ']' => depth += 1,
            '{' | '[' if depth == 0 => break,
            '{' | '[' => depth -= 1,
            _ => {}
        }
    }
    // The groups before it, back to the command's name.
    let mut done = 0;
    loop {
        while i > 0 && chars[i - 1] == ' ' {
            i -= 1;
        }
        let close = *chars.get(i.checked_sub(1)?)?;
        let open = match close {
            '}' => '{',
            ']' => '[',
            _ => break,
        };
        let mut depth = 0;
        loop {
            i = i.checked_sub(1)?;
            if chars[i] == close {
                depth += 1;
            } else if chars[i] == open {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
        done += 1;
    }
    let end = i;
    while i > 0 && (chars[i - 1].is_ascii_alphabetic() || chars[i - 1] == '@' || chars[i - 1] == '*') {
        i -= 1;
    }
    (i > 0 && chars[i - 1] == '\\' && i < end).then(|| (chars[i..end].iter().collect(), done))
}

/// The usage line of the command whose argument the cursor is in, the
/// argument it is at, and what the comment above its definition says.
pub fn signature_help(analysis: &Analysis, doc: &Text, position: Position) -> Option<lsp_types::SignatureHelp> {
    let pos = to_pos(doc, position);
    let (name, done) = argument_of(doc.line(pos.line), pos.col as usize)?;
    let (records, _) = query::explain(analysis, std::slice::from_ref(&name), false).ok()?;
    let record = records.iter().find(|r| r.get("signature").and_then(Json::as_str).is_some_and(|s| !s.is_empty()))?;
    let label = format!("\\{name}{}", usage(record.get("signature").and_then(Json::as_str)?));
    let comment = documented(record);
    let mut parameters = Vec::new();
    for n in 1..=9u8 {
        let arg = format!("⟨arg{n}⟩");
        if !label.contains(&arg) {
            break;
        }
        let text = comment.as_ref().and_then(|d| d.params.iter().find(|(k, _)| *k == n)).map(|(_, t)| t.clone());
        parameters.push(lsp_types::ParameterInformation {
            label: lsp_types::ParameterLabel::Simple(arg),
            documentation: text.map(lsp_types::Documentation::String),
        });
    }
    let documentation = comment.map(|d| d.text).filter(|t| !t.is_empty()).map(lsp_types::Documentation::String);
    Some(lsp_types::SignatureHelp {
        signatures: vec![lsp_types::SignatureInformation {
            label,
            documentation,
            parameters: Some(parameters),
            active_parameter: Some(done),
        }],
        active_signature: Some(0),
        active_parameter: Some(done),
    })
}

/// A usage line for an xparse-style signature (`smO{0}om`): `*` for a star,
/// `{⟨argN⟩}` for a mandatory argument, `[⟨argN⟩]` for an optional one, the
/// delimiters of `d`/`r` types, and literal text and keywords as written.
pub fn usage(sig: &str) -> String {
    let chars: Vec<char> = sig.chars().collect();
    let (mut out, mut i, mut n) = (String::new(), 0, 0);
    // `{…}` with nested braces, starting at `chars[at]`; the index after it.
    let group = |at: usize| -> usize {
        if chars.get(at) != Some(&'{') {
            return at;
        }
        let mut depth = 0;
        for (j, c) in chars.iter().enumerate().skip(at) {
            depth += match c {
                '{' => 1,
                '}' => -1,
                _ => 0,
            };
            if depth == 0 {
                return j + 1;
            }
        }
        chars.len()
    };
    let mut arg = |open: &str, close: &str| {
        n += 1;
        format!("{open}⟨arg{n}⟩{close}")
    };
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            's' => out.push('*'),
            'm' | 'v' => out.push_str(&arg("{", "}")),
            'o' => out.push_str(&arg("[", "]")),
            'O' => {
                out.push_str(&arg("[", "]"));
                i = group(i);
            }
            't' if i < chars.len() => {
                out.push(chars[i]);
                i += 1;
            }
            'd' | 'r' | 'D' | 'R' if i + 1 < chars.len() => {
                out.push_str(&arg(&chars[i].to_string(), &chars[i + 1].to_string()));
                i += 2;
                if c.is_uppercase() {
                    i = group(i);
                }
            }
            'l' => out.push_str(&arg("", "")),
            'u' => {
                let end = group(i);
                let delimiter: String = chars[(i + 1).min(end)..end.saturating_sub(1).max(i + 1)].iter().collect();
                out.push_str(&arg("", &delimiter));
                i = end;
            }
            'e' => i = group(i),
            'E' => i = group(group(i)),
            '+' | '!' | '>' => {
                if c == '>' {
                    i = group(i);
                }
            }
            '⟨' | '[' => {
                let close = if c == '⟨' { '⟩' } else { ']' };
                let end = chars[i..].iter().position(|&x| x == close).map_or(chars.len(), |p| i + p + 1);
                out.push(c);
                out.extend(&chars[i..end]);
                i = end;
            }
            other => out.push(other),
        }
    }
    out
}

/// A package or class name in `\usepackage`, `\RequirePackage` or
/// `\documentclass`: its file and `\Provides…` line, the options it offers
/// and the ones passed, what it defines, what it loads, and the run's
/// diagnostics inside it.
fn load_hover(analysis: &Analysis, path: &str, pos: Pos, word: &str) -> Option<String> {
    let load = analysis
        .facts
        .loads
        .iter()
        .find(|l| l.name == word && l.span.line == pos.line && analysis.file_name(l.span.file) == path)?;
    let mut md = format!("### `{}` — {}\n\n", load.name, load.kind.as_str());
    if let Some(provided) = &load.provided {
        md.push_str(&format!("{provided}\n\n"));
    }
    let mut lines = Vec::new();
    // What it offers, the given ones first and marked; else what was given.
    let offered = query::switches(analysis, false);
    let offers = offered.iter().find(|r| {
        r.get("tag").and_then(Json::as_str) == Some("offers")
            && r.get("name").and_then(Json::as_str) == Some(&load.name)
    });
    match offers.and_then(|r| r.get("value")?.as_str()) {
        Some(value) => lines.push(format!("**options:** {value}")),
        None if !load.options.is_empty() => lines.push(format!("**given:** {}", load.options.join(", "))),
        None => {}
    }
    let Some(file) = load.file else {
        md.push_str(&lines.join("\n\n"));
        md.push_str(&format!("\n\n{}\n", load.path.as_deref().unwrap_or("not found")));
        return Some(md);
    };
    let mut defined: Vec<&str> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| d.span.file == file && !matches!(d.tag, "option" | "key"))
        .map(|d| analysis.interner.name(d.name))
        .filter(|n| !n.is_empty() && !analysis.settings.conventions.is_internal(n))
        .collect();
    defined.sort();
    defined.dedup();
    if !defined.is_empty() {
        let shown: Vec<String> = defined.iter().take(8).map(|n| format!("`\\{n}`")).collect();
        let more =
            if defined.len() > shown.len() { format!(", +{}", defined.len() - shown.len()) } else { String::new() };
        lines.push(format!("**defines {}:** {}{more}", defined.len(), shown.join(", ")));
    }
    let mut loads: Vec<&str> =
        analysis.facts.loads.iter().filter(|l| l.span.file == file).map(|l| l.name.as_str()).collect();
    loads.dedup();
    if !loads.is_empty() {
        lines.push(format!("**loads:** {}", loads.join(", ")));
    }
    let found: Vec<String> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.span.file == file)
        .take(3)
        .map(|d| format!("- `{}` {}", d.code, d.message.lines().next().unwrap_or("")))
        .collect();
    md.push_str(&lines.join("\n\n"));
    if !found.is_empty() {
        md.push_str(&format!("\n\n{}", found.join("\n")));
    }
    md.push_str(&format!("\n\n`{}:1`\n", analysis.short_name(file)));
    Some(md)
}

/// `\input`/`\include`/`\usepackage`/`\documentclass`: the resolved file for
/// the load whose command or argument the cursor sits on, from
/// `analysis.facts.loads` (the run's own resolution). `None` off a load line
/// or if it never resolved.
fn load_target(analysis: &Analysis, path: &str, pos: Pos, word: &str) -> Option<GotoDefinitionResponse> {
    let candidates: Vec<_> = analysis
        .facts
        .loads
        .iter()
        .filter(|l| l.span.line == pos.line && analysis.file_name(l.span.file) == path)
        .collect();
    let single = if let [one] = candidates.as_slice() { Some(one) } else { None };
    let load = *candidates.iter().find(|l| l.name == word).or(single)?;
    let zero = Position { line: 0, character: 0 };
    let uri = path_uri(load.path.as_deref()?);
    Some(GotoDefinitionResponse::Array(vec![Location { uri, range: Range { start: zero, end: zero } }]))
}

/// `textDocument/definition`: the load target ([`load_target`]) if the
/// cursor sits on one; else every definition site [`query::explain`] knows
/// for the control sequence under the cursor, or the `\label`, `\bibitem`
/// or bibliography entry a key names.
pub fn definition(analysis: &Analysis, doc: &Text, path: &str, position: Position) -> Option<GotoDefinitionResponse> {
    let pos = to_pos(doc, position);
    let (name, is_command) = word_at(doc.line(pos.line), pos.col)?;
    if let Some(target) = load_target(analysis, path, pos, &name) {
        return Some(target);
    }
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
    let mut locations: Vec<Location> = records.iter().filter_map(|r| record_location(r, &sources)).collect();
    // The run may stop before a later file defines the name: scan the source.
    if locations.is_empty() && is_command {
        for site in super::unreached::sites(analysis, path, &name) {
            eprintln!("satex: \\{name} at {}:{} not reached by the analysis (source scan)", site.path, site.pos.line);
            let position = to_lsp(&site.text, site.pos);
            locations.push(Location { uri: path_uri(&site.path), range: Range { start: position, end: position } });
        }
    }
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
    let from = crate::source::byte_of_col(text_line, col);
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
    let first = crate::source::col_of_byte(text_line, found) + u32::from(is_command);
    let width = crate::source::columns(name) as u32;
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

/// `textDocument/completion`.  Inside an argument, what the run saw the
/// command do decides the list: the files it loaded (classes, packages,
/// inputs), the keys its occurrences carry (environments, labels,
/// citations, graphics), or the key family its optional argument feeds.
/// Elsewhere, the commands in scope, ranked for a document author.
pub fn completion(analysis: &Analysis, doc: &Text, position: Position) -> CompletionResponse {
    let pos = to_pos(doc, position);
    let line = doc.line(pos.line);
    let (before, after) = line.split_at(crate::source::byte_of_col(line, pos.col).min(line.len()));
    match argument_at(before) {
        Some(arg) if !cs_re().is_match(before) => {
            CompletionResponse::Array(argument_items(analysis, doc, &arg, after, pos))
        }
        _ => {
            let typed = cs_re().find(before).map_or("", |m| &m.as_str()[1..]);
            let (items, more) = command_items(analysis, pos, typed);
            CompletionResponse::List(lsp_types::CompletionList { is_incomplete: more, items })
        }
    }
}

/// How many commands one completion answer lists; the client asks again as
/// more of the name is typed.
const COMMANDS_SHOWN: usize = 300;

fn cs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\[A-Za-z@]*$").unwrap())
}

/// The argument the cursor is in: the command it belongs to, its opening
/// delimiter, the complete groups between them and the column of the `\`.
struct Arg<'a> {
    cmd: &'a str,
    open: char,
    groups: &'a str,
    col: u32,
}

fn argument_at(before: &str) -> Option<Arg<'_>> {
    static HEAD: OnceLock<Regex> = OnceLock::new();
    let head = HEAD.get_or_init(|| Regex::new(r"\\([A-Za-z@]+)\*?((?:\s*(?:\[[^\[\]]*\]|\{[^{}]*\}))*)\s*$").unwrap());
    let mut depth = 0u32;
    for (i, c) in before.char_indices().rev() {
        match c {
            '}' | ']' => depth += 1,
            '{' | '[' if depth > 0 => depth -= 1,
            '{' | '[' => {
                let caps = head.captures(&before[..i])?;
                let start = caps.get(0)?.start();
                return Some(Arg {
                    cmd: caps.get(1)?.as_str(),
                    open: c,
                    groups: caps.get(2)?.as_str(),
                    col: crate::source::col_of_byte(before, start),
                });
            }
            _ => {}
        }
    }
    None
}

/// Whether the run recorded `fact` (at `span`) as made by `\cmd`: by the
/// call it names, or, without one, by the command spelled at that place.
fn made_by(analysis: &Analysis, doc: &Text, call: Option<crate::tex::Sym>, span: crate::tex::Span, cmd: &str) -> bool {
    call.is_some_and(|sym| analysis.interner.name(sym) == cmd) || {
        span.file == analysis.main_file
            && doc
                .line(span.line)
                .get(crate::source::byte_of_col(doc.line(span.line), span.col)..)
                .and_then(|rest| rest.strip_prefix('\\'))
                .is_some_and(|rest| {
                    rest.strip_prefix(cmd).is_some_and(|r| !r.starts_with(|c: char| c.is_ascii_alphabetic()))
                })
    }
}

/// How many name characters precede the cursor on its line.
fn before_word(doc: &Text, pos: Pos) -> u32 {
    let line = doc.line(pos.line);
    let before = &line[..crate::source::byte_of_col(line, pos.col).min(line.len())];
    before.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '*').count() as u32
}

/// Whether the call spelled at `span` names `file` in its braces, so a
/// file the call's code read on its own (`\\begin{document}` reading the
/// `.aux` file) does not count as what the command takes.
fn names_load(doc: &Text, span: crate::tex::Span, file: &str) -> bool {
    let line = doc.line(span.line);
    let rest = line.get(crate::source::byte_of_col(line, span.col)..).unwrap_or("");
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    rest.split_once('{').and_then(|(_, r)| r.split_once('}')).is_some_and(|(arg, _)| {
        arg.split(',').any(|a| {
            let a = a.trim();
            a == file || a == stem
        })
    })
}

fn item(label: String, kind: CompletionItemKind, detail: Option<String>, rank: usize) -> CompletionItem {
    CompletionItem {
        sort_text: Some(format!("{rank:05}{label}")),
        label,
        kind: Some(kind),
        detail,
        ..Default::default()
    }
}

fn seed(cmd: &str) -> (Option<crate::builtins::LoadKind>, Option<crate::builtins::OccKind>) {
    use crate::builtins::{LoadKind as L, OccKind as O};
    match cmd {
        "documentclass" | "LoadClass" => (Some(L::Class), None),
        "usepackage" | "RequirePackage" => (Some(L::Package), None),
        "input" | "subfile" => (Some(L::Input), None),
        "include" | "includeonly" => (Some(L::Include), None),
        "begin" => (None, Some(O::BeginEnvironment)),
        "end" => (None, Some(O::EndEnvironment)),
        "includegraphics" => (None, Some(O::Graphics)),
        "ref" | "eqref" | "pageref" | "autoref" | "nameref" | "cref" | "Cref" | "cpageref" | "Cpageref" | "vref"
        | "Vref" | "labelcref" | "crefrange" | "Crefrange" | "subref" => (None, Some(O::Ref)),
        c if c.contains("cite") => (None, Some(O::Cite)),
        _ => (None, None),
    }
}

fn argument_items(analysis: &Analysis, doc: &Text, arg: &Arg, after: &str, pos: Pos) -> Vec<CompletionItem> {
    use crate::builtins::{LoadKind, OccKind};
    let mut loads: Vec<LoadKind> = analysis
        .facts
        .loads
        .iter()
        .filter(|l| made_by(analysis, doc, None, l.span, arg.cmd) && names_load(doc, l.span, &l.name))
        .map(|l| l.kind)
        .collect();
    let mut occs: Vec<OccKind> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| made_by(analysis, doc, o.call.map(|c| c.0), o.span, arg.cmd))
        .map(|o| o.kind)
        .collect();
    let (seed_load, seed_occ) = seed(arg.cmd);
    loads.extend(seed_load);
    occs.extend(seed_occ);
    loads.sort_by_key(|k| *k as u8);
    loads.dedup();
    occs.sort_by_key(|k| *k as u8);
    occs.dedup();
    let first_group = || {
        static G: OnceLock<Regex> = OnceLock::new();
        let g = G.get_or_init(|| Regex::new(r"\{([^{}]*)\}").unwrap());
        g.captures(arg.groups).map(|c| c[1].trim().to_string())
    };
    if arg.open == '[' {
        if loads.iter().any(|k| matches!(k, LoadKind::Class | LoadKind::Package)) {
            // `\usepackage[…]{name}`: the name follows the cursor.
            static NAME: OnceLock<Regex> = OnceLock::new();
            let name = NAME.get_or_init(|| Regex::new(r"^[^\]]*\]\s*\{([^{},]*)").unwrap());
            let Some(package) = name.captures(after).map(|c| c[1].trim().to_string()) else { return Vec::new() };
            return query::package_options(analysis, &package)
                .into_iter()
                .map(|(key, default)| {
                    item(
                        key,
                        CompletionItemKind::PROPERTY,
                        Some(default.map_or(package.clone(), |d| format!("default: {d}  ({package})"))),
                        0,
                    )
                })
                .collect();
        }
        let target = match occs.contains(&OccKind::BeginEnvironment) {
            true => first_group().unwrap_or_default(),
            false => arg.cmd.to_string(),
        };
        return option_items(analysis, &target);
    }
    // A key list in braces (`\pgfkeys{…}`, `\tikzset{…}`, a macro that
    // hands its argument to one): a `/path` typed so far completes to the
    // keys under it, else the keys of the family the command sets.
    if arg.open == '{' && loads.is_empty() {
        let typed = arg_item(doc, pos);
        if typed.starts_with('/') {
            let paths = key_paths(analysis, doc, pos, &typed);
            if !paths.is_empty() {
                return paths;
            }
        }
    }
    if arg.open == '{' && loads.is_empty() && occs.iter().all(|k| matches!(k, OccKind::Key | OccKind::KeyUse)) {
        let keys = option_items(analysis, arg.cmd);
        if !keys.is_empty() && !keys.iter().all(|k| k.detail.as_deref().is_some_and(|d| d.contains("positional"))) {
            return keys;
        }
    }
    let mut out = Vec::new();
    let roots = &analysis.distribution.roots;
    for &kind in &loads {
        match kind {
            LoadKind::Class | LoadKind::Package => {
                let ext = if kind == LoadKind::Class { ".cls" } else { ".sty" };
                let mut names = project_files(analysis, &[&ext[1..]]);
                names.extend(crate::distribution::names_with(roots, ext));
                out.extend(names.into_iter().map(|n| item(n, CompletionItemKind::MODULE, Some(ext.into()), 1)));
            }
            LoadKind::Input | LoadKind::Include => {
                out.extend(
                    project_files(analysis, &["tex"]).into_iter().map(|n| item(n, CompletionItemKind::FILE, None, 0)),
                );
            }
            _ => {}
        }
    }
    let keys = |kinds: &[OccKind]| -> BTreeSet<String> {
        analysis
            .facts
            .occurrences
            .iter()
            .filter(|o| kinds.contains(&o.kind) && !o.key.is_empty())
            .map(|o| o.key.clone())
            .collect()
    };
    for kind in &occs {
        match kind {
            OccKind::BeginEnvironment | OccKind::EndEnvironment => {
                let open = open_environments(analysis, (pos.line, arg.col));
                let snippet = *kind == OccKind::BeginEnvironment;
                let close = after.starts_with('}');
                // A name already written after the cursor stays as it is.
                let fresh = close || after.trim().is_empty();
                if !snippet {
                    out.extend(
                        open.iter()
                            .rev()
                            .enumerate()
                            .map(|(i, e)| item(e.clone(), CompletionItemKind::MODULE, Some("open here".into()), i)),
                    );
                }
                for (env, items) in environments(analysis, pos) {
                    if !snippet && open.contains(&env) {
                        continue;
                    }
                    let mut it = item(env.clone(), CompletionItemKind::MODULE, None, 100);
                    if snippet && fresh {
                        let body = if items { "\t\\item $0" } else { "\t$0" };
                        // From the name typed so far through a `}` the
                        // editor already closed.
                        let typed = before_word(doc, pos);
                        let range = Range {
                            start: to_lsp(doc, Pos::new(pos.line, pos.col - typed)),
                            end: to_lsp(doc, Pos::new(pos.line, pos.col + close as u32)),
                        };
                        let args: String =
                            (1..=environment_arguments(analysis, &env)).map(|i| format!("{{${i}}}")).collect();
                        let new_text = format!("{env}}}{args}\n{body}\n\\end{{{env}}}");
                        it.text_edit = Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text }));
                        it.insert_text_format = Some(InsertTextFormat::SNIPPET);
                    }
                    out.push(it);
                }
            }
            OccKind::Ref => {
                out.extend(keys(&[OccKind::Label]).into_iter().map(|k| item(k, CompletionItemKind::REFERENCE, None, 0)))
            }
            OccKind::Cite => out.extend(
                keys(&[OccKind::BibItem, OccKind::Cite])
                    .into_iter()
                    .map(|k| item(k, CompletionItemKind::REFERENCE, None, 0)),
            ),
            OccKind::Graphics => {
                let exts: BTreeSet<String> =
                    keys(&[OccKind::Graphics]).iter().filter_map(|k| Some(k.rsplit_once('.')?.1.to_string())).collect();
                let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
                out.extend(
                    project_files(analysis, &exts).into_iter().map(|n| item(n, CompletionItemKind::FILE, None, 0)),
                );
            }
            _ => {}
        }
    }
    out.dedup_by(|a, b| a.label == b.label);
    out
}

/// The keys `name`'s optional argument feeds, from [`query::options`].
/// The item of a key list the cursor is in, from its `,` or `{`.
fn arg_item(doc: &Text, pos: Pos) -> String {
    let line = doc.line(pos.line);
    let before = &line[..crate::source::byte_of_col(line, pos.col).min(line.len())];
    before.rsplit([',', '{']).next().unwrap_or_default().trim_start().to_string()
}

/// Every pgf key under the directory of `typed` (`/demo/m` → `/demo/`),
/// replacing what was typed.
fn key_paths(analysis: &Analysis, doc: &Text, pos: Pos, typed: &str) -> Vec<CompletionItem> {
    let dir = &typed[..typed.rfind('/').map_or(0, |i| i + 1)];
    let range =
        Range { start: to_lsp(doc, Pos::new(pos.line, pos.col - typed.chars().count() as u32)), end: to_lsp(doc, pos) };
    query::pgfkeys(analysis, Some(dir))
        .into_iter()
        .filter_map(|r| {
            let key = r.get("key")?.as_str()?.to_string();
            if key[dir.len().min(key.len())..].contains("/.") || key.ends_with("/.unknown") {
                return None;
            }
            let detail = ["kind", "default"].iter().filter_map(|f| r.get(*f)?.as_str()).collect::<Vec<_>>().join("  ");
            let mut it = item(key.clone(), CompletionItemKind::PROPERTY, Some(detail), 0);
            it.text_edit = Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text: key }));
            Some(it)
        })
        .collect()
}

fn option_items(analysis: &Analysis, name: &str) -> Vec<CompletionItem> {
    query::options(analysis, name)
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let family = r.get("family").and_then(Json::as_str).unwrap_or_default();
            let key = r.get("key")?.as_str()?;
            let key = key.strip_prefix(family).map_or(key, |k| k.trim_start_matches('/'));
            let default = r.get("default").and_then(Json::as_str).filter(|d| !d.is_empty());
            let kind = r.get("kind").and_then(Json::as_str).filter(|k| *k != "code");
            let detail = [kind.map(str::to_string), default.map(|d| format!("default: {d}")), Some(family.to_string())];
            let detail: Vec<String> = detail.into_iter().flatten().filter(|d| !d.is_empty()).collect();
            let rank = r.get("rank").and_then(Json::as_u64).unwrap_or(0) as usize;
            let mut it = item(
                key.to_string(),
                CompletionItemKind::PROPERTY,
                Some(detail.join("  ")),
                rank * 10_000 + i.min(9_999),
            );
            if default.is_none() && kind.is_none() {
                it.insert_text = Some(format!("{key}=$1"));
                it.insert_text_format = Some(InsertTextFormat::SNIPPET);
            }
            Some(it)
        })
        .collect()
}

/// Files under the project root with one of `exts`, relative and without
/// the extension, as `\input` and `\includegraphics` take them.
fn project_files(analysis: &Analysis, exts: &[&str]) -> Vec<String> {
    let root = &analysis.project.root;
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten().take(2_000) {
            let path = entry.path();
            let hidden = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
            if hidden || path.starts_with(root.join("target")) {
                continue;
            }
            if path.is_dir() {
                if stack.len() < 64 {
                    stack.push(path);
                }
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e))
                && let Ok(rel) = path.with_extension("").strip_prefix(root)
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

/// Environments open before `at` in the main file, outermost first.
fn open_environments(analysis: &Analysis, at: (u32, u32)) -> Vec<String> {
    use crate::builtins::OccKind;
    let mut stack: Vec<String> = Vec::new();
    let mut seen: Vec<_> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.span.file == analysis.main_file && (o.span.line, o.span.col) < at)
        .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment))
        .collect();
    seen.sort_by_key(|o| (o.span.line, o.span.col));
    seen.dedup_by_key(|o| (o.span.line, o.span.col, o.kind));
    for o in seen {
        if o.kind == OccKind::BeginEnvironment {
            stack.push(o.key.clone());
        } else if let Some(i) = stack.iter().rposition(|e| *e == o.key) {
            stack.truncate(i);
        }
    }
    stack
}

/// Environments defined at `pos`: the run's `\X`/`\endX` pairs (what
/// `\begin` looks up), less the ones the document defines only later, each
/// with whether its code reaches what `\item` reads (a list).
fn environments(analysis: &Analysis, pos: Pos) -> Vec<(String, bool)> {
    let later = |sym| {
        let defs: Vec<_> = analysis.facts.defs.iter().filter(|d| d.name == sym).collect();
        !defs.is_empty() && !defs.iter().any(|d| analysis.visible_in(d.span, analysis.main_file, pos.line, pos.col))
    };
    // A list environment is made with `\list`, whose items are `\item`s.
    let list = analysis.interner.lookup("list");
    let mut out = Vec::new();
    for i in 0..analysis.interner.len() as u32 {
        let end = crate::tex::Sym(i);
        let Some(base) = analysis.interner.name(end).strip_prefix("end") else { continue };
        if base.is_empty() || base.contains(['@', ':', '\\']) || base.chars().any(char::is_control) {
            continue;
        }
        let Some(begin) = analysis.interner.lookup(base) else { continue };
        let user = |s| analysis.env.is_defined(s) && analysis.env.meaning(s).prim().is_none();
        if !user(begin) || !user(end) || later(begin) {
            continue;
        }
        let items = list.is_some_and(|l| body_reach(analysis, base, 400).contains(&l));
        out.push((base.to_string(), items));
    }
    out.sort();
    out
}

/// The names the bodies of `name`'s code spell, breadth-first through the
/// meanings the run ended with, up to `limit` macros.
fn body_reach(analysis: &Analysis, name: &str, limit: usize) -> BTreeSet<crate::tex::Sym> {
    let mut seen = BTreeSet::new();
    let mut queue: std::collections::VecDeque<_> = analysis.interner.lookup(name).into_iter().collect();
    let mut expanded = 0;
    while let Some(sym) = queue.pop_front() {
        let Some(mac) = analysis.env.meaning(sym).as_macro().cloned() else { continue };
        expanded += 1;
        if expanded > limit {
            break;
        }
        for cs in mac.replacement_text.iter().filter_map(|t| t.cs()) {
            if seen.insert(cs) {
                queue.push_back(cs);
            }
        }
    }
    seen
}

/// Command names in scope at `pos`, ranked for a document author: the
/// project's own first, then documented names from packages and the
/// kernel (no `@`, `_` or `:`), more used ones first, internals last.
/// The fewest arguments each defined name takes, from the definitions
/// query, once per analysis.
fn takes(analysis: &Analysis) -> &crate::fixed::HashMap<String, u8> {
    analysis.takes.get_or_init(|| {
        query::run(analysis, Query::Definitions, &Filter::Always)
            .into_iter()
            .filter_map(|r| {
                let name = r.get("name")?.as_str()?.to_string();
                let min = r.get("takes")?.get("min")?.as_u64()? as u8;
                Some((name, min))
            })
            .collect()
    })
}

/// Environments whose begin command reads its mandatory arguments through
/// an internal macro (`\tabular` hands them to `\@tabular`), so its own
/// definition takes none: how many they are.
const ENVIRONMENT_ARGUMENTS: &[(&str, u8)] = &[
    ("array", 1),
    ("minipage", 1),
    ("multicols", 1),
    ("tabular", 1),
    ("tabular*", 2),
    ("tabularx", 2),
    ("thebibliography", 1),
    ("wrapfigure", 2),
];

/// How many mandatory arguments `\begin{env}` takes.
fn environment_arguments(analysis: &Analysis, env: &str) -> u8 {
    ENVIRONMENT_ARGUMENTS
        .iter()
        .find(|(name, _)| *name == env)
        .map(|(_, n)| *n)
        .unwrap_or_else(|| takes(analysis).get(&format!("\\{env}")).copied().unwrap_or(0))
}

/// Each carries a snippet for its mandatory arguments.
/// The commands in scope at `pos` whose name holds `typed` (case aside),
/// the best [`COMMANDS_SHOWN`] of them, and whether there were more.
fn command_items(analysis: &Analysis, pos: Pos, typed: &str) -> (Vec<CompletionItem>, bool) {
    let takes = takes(analysis);
    let typed = typed.to_lowercase();
    let mut used: crate::fixed::HashMap<crate::tex::Sym, u32> = crate::fixed::HashMap::default();
    for e in &analysis.facts.expansions {
        if query::origin(analysis, e.span.file) == "document" && e.within.is_none() {
            *used.entry(e.name).or_default() += 1;
        }
    }
    let mut items: Vec<CompletionItem> = query::scope(analysis, Some((analysis.main_file, pos.line, pos.col)), true)
        .into_iter()
        .filter_map(|record| {
            let name = record.get("name")?.as_str()?.to_string();
            let label = name.trim_start_matches('\\').to_string();
            if !label.to_lowercase().contains(&typed) {
                return None;
            }
            let tag = record.get("tag").and_then(Json::as_str).unwrap_or("");
            let internal = analysis.settings.conventions.is_internal(label.trim_start_matches('\\'));
            if label.is_empty() || (internal && !matches!(tag, "macro" | "primitive" | "environment" | "alias")) {
                return None;
            }
            let effective = record.get("effective").and_then(Json::as_str).unwrap_or("");
            let package = record.get("package").and_then(Json::as_str).unwrap_or("");
            let tier = match (package, internal) {
                ("document", _) => 0,
                (_, true) => 3,
                _ if tag == "primitive" || !label.chars().any(char::is_alphabetic) => 2,
                _ => 1,
            };
            let count = analysis.interner.lookup(&label).and_then(|s| used.get(&s)).copied().unwrap_or(0);
            // The argument is a name to pick: the editor asks again inside
            // the braces, and an environment brings its `\end`.
            let pick =
                matches!(label.as_str(), "begin" | "end" | "usepackage" | "documentclass" | "ref" | "cref" | "cite");
            let (insert_text, format) = match takes.get(&name).copied().unwrap_or(0) {
                _ if pick => (format!("{label}{{$1}}"), InsertTextFormat::SNIPPET),
                0 => (label.clone(), InsertTextFormat::PLAIN_TEXT),
                min => {
                    let args: String = (1..=min).map(|i| format!("{{${i}}}")).collect();
                    (format!("{label}{args}"), InsertTextFormat::SNIPPET)
                }
            };
            Some(CompletionItem {
                sort_text: Some(format!("{tier}{:06}{label}", 999_999u32.saturating_sub(count))),
                label,
                kind: Some(if tag == "primitive" { CompletionItemKind::KEYWORD } else { CompletionItemKind::FUNCTION }),
                detail: Some(format!("{effective}  ({package})")),
                insert_text: Some(insert_text),
                insert_text_format: Some(format),
                command: pick.then(|| lsp_types::Command {
                    title: "pick".into(),
                    command: "editor.action.triggerSuggest".into(),
                    arguments: None,
                }),
                ..Default::default()
            })
        })
        .collect();
    // A name that starts with what is typed before one that only holds it.
    items.sort_by_cached_key(|i| (!i.label.to_lowercase().starts_with(&typed), i.sort_text.clone()));
    let more = items.len() > COMMANDS_SHOWN;
    items.truncate(COMMANDS_SHOWN);
    (items, more)
}

/// The paragraphs the document's own files set, in reading order: `satex
/// query typeset` of the run, without what libraries set.
pub fn typeset(analysis: &Analysis) -> Vec<Json> {
    query::run(analysis, Query::Typeset, &Filter::Always)
        .into_iter()
        .filter(|r| {
            r.get("origin").is_none_or(|o| o == "document")
                && r.get("kind").and_then(Json::as_str) != Some("assumption")
        })
        .map(Json::Object)
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
    let text = crate::render::lsp_with(&records, |path| path_uri(path).to_string());
    serde_json::from_str(&text).unwrap_or_default()
}

#[cfg(test)]
mod signature_tests {
    use super::argument_of;

    #[test]
    fn the_argument_the_cursor_is_in() {
        assert_eq!(argument_of("\\greet{wor", 11), Some(("greet".into(), 0)));
        assert_eq!(argument_of("x \\frac{a}{b", 13), Some(("frac".into(), 1)));
        assert_eq!(argument_of("\\cmd[o]{a{b}c}{", 16), Some(("cmd".into(), 2)));
        assert_eq!(argument_of("plain {text", 11), None);
    }
}
