//! Feature handlers: each turns an [`Analysis`] satex already built into the
//! `lsp_types` response for one request.  No analysis of its own happens
//! here — everything comes from [`crate::queries::query`] and [`crate::linter`], the
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

use crate::analysis::machine::Analysis;
use crate::linter::fix::{Pos, Sources, Text};
use crate::queries::preview::Stroke;
use crate::queries::query::{self, Filter, Query, Record};

/// LSP position (0-based line, UTF-16 column) to satex's (1-based line,
/// column as the engine counts one), against the line as it stands in `doc`.
pub fn to_pos(doc: &Text, position: Position) -> Pos {
    let line = position.line + 1;
    Pos::new(line, doc.from_utf16(line, position.character))
}

/// The reverse of [`to_pos`], through [`Text::utf16`]; falls back to a
/// 0-based, byte-counted guess when `pos` is out of range (a stale position
/// from a definition site an edit has since moved).
pub(super) fn to_lsp(doc: &Text, pos: Pos) -> Position {
    match doc.utf16(pos) {
        Some((line, character)) => Position { line, character },
        None => Position { line: pos.line.saturating_sub(1), character: pos.col.saturating_sub(1) },
    }
}

/// An expl3 function name (`\tl_set:Nn`) is one word: its `:` tells it from `\foo_x`.
fn word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\[A-Za-z@_]+:[A-Za-z]*|\\[A-Za-z@]+|\\.|[A-Za-z0-9_:.+*-]+").unwrap())
}

/// The control sequence or key under the cursor: its name (without a
/// leading `\`) and whether it had one.  `None` off the end of every match
/// on the line.
pub fn word_at(line: &str, col: u32) -> Option<(String, bool)> {
    word_span(line, col).map(|(name, is_command, _)| (name, is_command))
}

/// The same, with the 1-based column the match starts at.
pub fn word_span(line: &str, col: u32) -> Option<(String, bool, u32)> {
    let target = crate::engine::source::byte_of_col(line, col);
    word_re().find_iter(line).find(|m| m.start() <= target && target <= m.end()).map(|m| {
        let text = m.as_str();
        (
            crate::engine::tex::bare_name(text).to_string(),
            text.starts_with('\\'),
            crate::engine::source::col_of_byte(line, m.start()),
        )
    })
}

/// The path a run recorded a file under, resolved to a `file://` URI.
/// `lsp_types::Uri` (since 0.95) is a thin wrapper over a generic RFC 3986
/// URI, not a `url::Url`, so there is no `from_file_path`/`to_file_path`
/// left to reuse: [`crate::project::paths::file_url`] and [`uri_to_path`] are
/// satex's own, covering exactly the case the server needs.
pub fn path_uri(path: &str) -> Uri {
    // The same rooting [`crate::util::render::file_url`] does, so a diagnostic and the file it
    // belongs to reach the client under one URI.
    let absolute = std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path));
    let absolute = VIEW.with_borrow(|view| view.named(&absolute));
    crate::project::paths::file_url(&absolute).parse().unwrap_or_else(|_| "file:///".parse().unwrap())
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
    crate::project::paths::from_file_url(uri.as_str())
}

/// A `place`d record (`path`/`line`/`col`, as every query answer carries) as
/// an LSP [`Location`]: a zero-width range at the position, the same way
/// [`crate::util::render::lsp`] points a diagnostic at one place rather than a span.
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
    if let Some(markdown) = super::directive::hover(doc.line(pos.line), pos.col) {
        return Some(markdown_hover(markdown));
    }
    if let Some(markdown) = directive_hover(analysis, path, pos) {
        return Some(markdown_hover(markdown));
    }
    if let Some(markdown) = magic_hover(analysis, doc, pos) {
        return Some(markdown_hover(markdown));
    }
    let word = word_at(doc.line(pos.line), pos.col);
    let markdown = match word {
        Some((name, true)) => {
            placed(analysis, command_hover(analysis, &name, active_at(analysis, &name, path, pos))?, &name, path, pos)
        }
        _ => super::argument_hover::typed_hover(analysis, doc, path, pos).or_else(|| {
            let (name, _) = word?;
            load_hover(analysis, path, pos, &name).or_else(|| bib_style_hover(analysis, path, pos, &name))
        })?,
    };
    Some(markdown_hover(markdown))
}

/// The key under column `col` in the option list of a command (`\\captionsetup{aboveskip=}`):
/// the family and package that define it, its type, default and choices as the run stored them
/// ([`query::options`]), and what the mined manual says of it.
pub(super) fn key_hover(analysis: &Analysis, line: &str, col: u32) -> Option<String> {
    let at = crate::engine::source::byte_of_col(line, col).min(line.len());
    // the group the cursor stands in, still open there: a `[…]` closed before it holds no key here
    let open = argument_at(&line[..at])?;
    let from = at - open.inside.len();
    let start = from + open.inside.rfind([',', '{']).map_or(0, |i| i + 1);
    let end = at + line[at..].find(['=', ',', ']', '}']).unwrap_or(line.len() - at);
    // a value, not a key: the `=` stands before the cursor
    if line[start..at].contains('=') {
        return None;
    }
    let key = line[start..end].trim();
    if key.is_empty() || key.contains('\\') {
        return None;
    }
    // the text up to and with the bracket or comma before the key: the first key has no comma
    let arg = argument_at(&line[..start])?;
    let target = key_list_owner(&arg)?;
    // a relative key after the list changed its directory (`/demo/.cd`, a family's name)
    let switched = arg.inside.split(',').rev().map(|i| i.split('=').next().unwrap_or_default().trim()).find_map(|k| {
        let dir = k.strip_suffix("/.cd").unwrap_or(k);
        let family = query::stored_keys(analysis, Some(dir)).iter().any(|r| {
            r.get("key").and_then(Json::as_str) == Some(dir)
                && r.get("kind").and_then(Json::as_str) == Some("is family")
        });
        (dir.starts_with('/') && (k.ends_with("/.cd") || family)).then(|| format!("{dir}/{key}"))
    });
    let record = (switched.filter(|_| !key.starts_with('/')))
        .and_then(|full| query::resolve_key(analysis, &target, &full))
        .or_else(|| query::resolve_key(analysis, &target, key))?;
    let text = |r: &Record, f: &str| r.get(f).and_then(Json::as_str).filter(|v| !v.is_empty()).map(str::to_string);
    let mut markdown = format!("### Key `{key}` of `\\{}`\n\n", target.trim_start_matches('\\'));
    // where the run defined the key (the package's file), not the call that loaded it
    let defined = record.get("defined");
    let site = |f: &str| defined.and_then(|d| d.get(f));
    let defined_path = site("path").and_then(Json::as_str).map(str::to_string).or_else(|| text(&record, "path"));
    let family = text(&record, "family");
    if record.contains_key("unknown") {
        // What the handler takes it for, by what the run defined under that name.
        let color = analysis.facts.colors.iter().rev().find(|c| c.name == key);
        match color {
            Some(c) => markdown.push_str(&format!(
                "The color `{key}`{}: no key of `{}` by this name, so its `.unknown` handler takes it as the color to draw and fill with.\n\n",
                c.value.as_ref().and_then(rgba).map(|v| format!(" (rgb {:.2} {:.2} {:.2})", v.red, v.green, v.blue)).unwrap_or_default(),
                family.as_deref().unwrap_or_default()
            )),
            None => markdown.push_str(&format!(
                "No key of `{}` by this name: its `.unknown` handler reads it (a color, an arrow tip, a shape).\n\n",
                family.as_deref().unwrap_or_default()
            )),
        }
    }
    let mined = defined_path.clone().filter(|_| analysis.settings.plugins.docs).and_then(|p| {
        // pgf's manual names a key by its path (`/tikz/yshift`), other manuals by its name
        let index = crate::plugins::docs::index(&analysis.settings.docs);
        let path = std::path::Path::new(&p);
        let full = family.as_deref().map(|f| format!("{}/{key}", f.trim_end_matches('/')));
        full.and_then(|f| index.lookup(path, &f)).or_else(|| index.lookup(path, key))
    });
    if let Some((m, does)) = mined.as_ref().and_then(|m| Some((m, m.text.as_deref()?))) {
        markdown.push_str(&format!("{does} {}\n\n", extracted_from_manual(m)));
    }
    if let Some(usage) = mined.as_ref().and_then(|m| m.usage.as_deref()) {
        markdown.push_str(&format!("`{usage}`\n\n"));
    }
    if let Some(comment) = text(&record, "comment") {
        markdown.push_str(&format!("{comment} {}\n\n", extracted("from the source comment")));
    }
    let default = text(&record, "default").map(|d| match d.contains("pgfkeysvaluerequired") {
        true => "needs a value".to_string(),
        false => format!("default `{d}`"),
    });
    let kind = text(&record, "kind").filter(|k| k != "code" && k != "initial");
    let initial = (text(&record, "kind").as_deref() == Some("initial"))
        .then(|| text(&record, "value").map(|v| format!("initial `{v}`")))
        .flatten();
    let from = text(&record, "package")
        .or_else(|| defined_path.as_ref().map(|p| p.rsplit('/').next().unwrap_or(p).to_string()));
    let facts: Vec<String> = [
        text(&record, "type").map(|t| format!("*{t}* `{}`", text(&record, "key").unwrap_or_default())),
        kind,
        initial,
        default,
        family.map(|f| format!("family `{f}`")),
        from.map(|p| format!("defined in `{p}`")),
    ]
    .into_iter()
    .flatten()
    .collect();
    markdown.push_str(&format!("{}\n\n", facts.join(" · ")));
    if let Some(full) = text(&record, "key").filter(|_| !record.contains_key("unknown")) {
        let choices: Vec<String> = query::stored_keys(analysis, Some(&format!("{full}/")))
            .iter()
            .filter_map(|r| text(r, "key")?.strip_prefix(&format!("{full}/")).map(str::to_string))
            .filter(|c| !c.starts_with('.') && !c.contains('/'))
            .chain(query::key_values(analysis, &target, key))
            .collect();
        if !choices.is_empty() {
            markdown.push_str(&format!(
                "Values: {}\n\n",
                choices.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    let line = site("line").or_else(|| record.get("line")).and_then(Json::as_u64);
    if let (Some(path), Some(n)) = (defined_path, line) {
        markdown.push_str(&format!("`{path}:{n}`\n\n"));
    }
    if let Some((doc, url)) = mined.and_then(|m| m.link) {
        markdown.push_str(&format!("[{doc}]({url})\n\n"));
    }
    Some(markdown)
}

/// The command or environment whose key list `arg` is: `\draw[…]`, `\tikzset{…}`, `\begin{x}[…]`.
fn key_list_owner(arg: &Arg) -> Option<String> {
    Some(match arg.cmd {
        "begin" => arg.groups.trim().trim_start_matches('{').split('}').next()?.to_string(),
        cmd => cmd.to_string(),
    })
}

/// What satex did with the magic comment (`% !TeX program = …`, `% arara: …`)
/// on the cursor's line: the engine it ran, the root it read the file through.
fn magic_hover(analysis: &Analysis, doc: &Text, pos: Pos) -> Option<String> {
    let magic = crate::plugins::magic::scan_with(doc.line(pos.line), &analysis.settings.magic).into_iter().next()?;
    let what = match magic.key.to_ascii_lowercase().as_str() {
        "tex program" => format!(
            "satex ran this document with the {:?} engine{}.",
            analysis.plugins.engine,
            if analysis.plugins.program.is_empty() {
                String::new()
            } else {
                format!(" (`{}`)", analysis.plugins.program)
            }
        ),
        "tex root" => format!("satex reads this file through its root, `{}`.", magic.value),
        "arara" => "satex's build reads this arara directive as one of the build's steps.".into(),
        _ => "satex reads it and does not act on it.".into(),
    };
    Some(format!("**Magic comment** `{}` = `{}`\n\n{what}", magic.key, magic.value))
}

/// The outcome of the `%\satex@` directive the cursor is on, from its
/// `\satex@` to the end of its line.
fn directive_hover(analysis: &Analysis, path: &str, pos: Pos) -> Option<String> {
    let found: Vec<String> = crate::linter::assertions::verdicts(analysis)
        .into_iter()
        .filter(|(span, ..)| span.line == pos.line && span.col <= pos.col && analysis.file_name(span.file) == path)
        .map(|(_, code, message)| format!("**{}** {message}", crate::linter::label(code)))
        .collect();
    (!found.is_empty()).then(|| found.join("\n\n"))
}

/// A key's card heading: its kind first, the key as code (`### Label `sec:x``).
pub(super) fn heading(kind: &str, key: &str) -> String {
    let mut chars = kind.chars();
    let kind: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
    format!("### {kind} `{key}`\n\n")
}

/// A count and its noun, plural unless one: `0 references`, `1 reference`.
pub(super) fn counted(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

/// The database entry of the citation key `key`: what it names and every field.
pub(super) fn cite_hover(analysis: &Analysis, key: &str) -> Option<String> {
    let e = bib_entries(analysis).iter().find(|e| e.key == key.trim())?;
    let field = |names: &[&str]| e.fields.iter().find(|(n, _)| names.contains(&n.as_str())).map(|(_, v)| v.as_str());
    let venue = field(&["journal", "journaltitle", "booktitle", "publisher", "school", "institution", "howpublished"]);
    let mut markdown = format!("{}{}\n\n", heading("citation", &e.key), e.kind);
    if !e.title.is_empty() {
        markdown.push_str(&format!("**{}**\n\n", e.title));
    }
    let line: Vec<&str> =
        [e.author.as_str(), e.year.as_str(), venue.unwrap_or_default()].into_iter().filter(|s| !s.is_empty()).collect();
    if !line.is_empty() {
        markdown.push_str(&format!("{}\n\n", line.join(" · ")));
    }
    for (name, value) in &e.fields {
        markdown.push_str(&format!("- `{name}` {value}\n"));
    }
    let file = e.file.rsplit(['/', '\\']).next().unwrap_or_default();
    markdown.push_str(&format!("\n`{file}:{}`\n\n", e.line));
    Some(markdown)
}

/// The hover of the control sequence `name` apart from where it stands:
/// what may be kept.
pub fn command_markdown(analysis: &Analysis, name: &str) -> Option<String> {
    command_hover(analysis, name, None)
}

/// The record of `name` in force at `pos` of `path`, when its definitions differ.
fn active_at(analysis: &Analysis, name: &str, path: &str, pos: Pos) -> Option<Record> {
    let at = query::ExplainAt::At(query::At { file: Some(path.to_string()), line: pos.line, col: pos.col, end: None });
    query::explain_at(analysis, &[name.to_string()], false, Some(&at)).ok()?.0.into_iter().next()
}

/// The heading above the definitions that do not hold where the cursor is.
pub const MORE_DEFINITIONS: &str = "#### More definitions";

/// `markdown` with the primitive's syntax under its first heading, where
/// `name` still means the primitive at `at`.
pub fn placed(analysis: &Analysis, mut markdown: String, name: &str, path: &str, at: Pos) -> String {
    let first = markdown.find("\n\n").map_or(0, |i| i + 2);
    // What the calls typeset, the one at the cursor first: before the
    // signature line, else after the first paragraph under the heading.
    let set = typesets(analysis, name, path, at);
    let usage = markdown.find(&format!("\n`\\{name}")).map(|i| i + 1);
    let second = markdown[first..].find("\n\n").map_or(markdown.len(), |i| first + i + 2);
    markdown.insert_str(usage.unwrap_or(second).min(markdown.len()), &set);
    if let Some(syntax) = super::primitives::syntax(analysis, name, path, at.line, at.col) {
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

/// A redefinition chain as one line: `latex.ltx:12 (kernel) → hyperref.sty:3 (redefined) → …`, its
/// first steps folded into `+N earlier` past the last two.
fn chain_line(chain: &Json) -> String {
    let steps = chain.as_array().map_or(&[][..], Vec::as_slice);
    let text = |s: &Json, k: &str| s.get(k).and_then(Json::as_str).unwrap_or_default().to_string();
    let one = |s: &Json| {
        let what = match text(s, "role").as_str() {
            "kernel" => "kernel".to_string(),
            role => {
                let who =
                    if s.get("package").is_some_and(Json::is_string) { text(s, "package") } else { "document".into() };
                format!("{who}, {}", if role == "in force" { "in force here" } else { "redefined" })
            }
        };
        let saves = match s.get("saved").and_then(Json::as_array) {
            Some(names) if !names.is_empty() => {
                let names: Vec<&str> = names.iter().filter_map(Json::as_str).collect();
                format!(", saves the old meaning as `{}`", names.join("`, `"))
            }
            _ => String::new(),
        };
        format!("`{}:{}` ({what}{saves})", text(s, "file"), s.get("line").and_then(Json::as_u64).unwrap_or_default())
    };
    let keep = steps.len().saturating_sub(2);
    let mut parts: Vec<String> = Vec::new();
    if keep > 0 {
        parts.push(format!("+{keep} earlier"));
    }
    parts.extend(steps[keep..].iter().map(one));
    parts.join(" → ")
}

pub(super) fn command_hover(analysis: &Analysis, name: &str, active: Option<Record>) -> Option<String> {
    let (mut records, _) = query::explain(analysis, &[name.to_string()], false).ok()?;
    // The definition in force at the cursor first, the others after it.
    let place = |r: &Record| (r.get("path").cloned(), r.get("line").cloned());
    if let Some(active) = active {
        match records.iter().position(|r| place(r) == place(&active)) {
            Some(i) => records[..=i].rotate_right(1),
            None => records.insert(0, active),
        }
    }
    let mut markdown = String::new();
    if records.is_empty() {
        let path = analysis.file_name(analysis.main_file).to_string();
        let sites = super::unreached::sites(analysis, &path, name);
        if sites.is_empty() {
            return None;
        }
        markdown.push_str(&format!("### `\\{name}`: not reached by the analysis\n\n"));
        for site in &sites {
            markdown.push_str(&format!("`{}:{}` (source scan)\n\n", site.path, site.pos.line));
        }
        markdown.push_str(&super::knowledge::about(&topics(analysis, name, &records)));
    }
    for (n, record) in records.iter().enumerate() {
        if n == 1 {
            markdown.push_str(&super::knowledge::about(&topics(analysis, name, &records)));
            markdown.push_str(&format!("{MORE_DEFINITIONS}\n\n"));
        }
        let text = |key: &str| record.get(key).and_then(Json::as_str);
        let mined = text("path")
            .filter(|_| analysis.settings.plugins.docs)
            .and_then(|p| crate::plugins::docs::index(&analysis.settings.docs).lookup(std::path::Path::new(p), name));
        let mined = mined.as_ref();
        match text("when") {
            Some(w) => markdown.push_str(&format!("### `\\{name}`: {w}\n\n")),
            None => markdown.push_str(&format!("### `\\{name}`\n\n")),
        }
        if let Some(does) = text("does") {
            markdown.push_str(&format!("{does}\n\n"));
        } else if let Some((m, does)) = mined.and_then(|m| Some((m, m.text.as_deref()?))) {
            markdown.push_str(&format!("{does} {}\n\n", extracted_from_manual(m)));
        }
        // The manual's usage line, else the run's, its arguments named by role.
        if let Some(usage) = mined.and_then(|m| m.usage.as_deref()) {
            markdown.push_str(&format!("`{usage}`\n\n"));
        } else if let Some(usage) = text("usage").filter(|u| u.contains('⟨')) {
            markdown.push_str(&format!("`{usage}`\n\n"));
        } else if let Some(sig) = text("signature").filter(|s| !s.is_empty()) {
            markdown.push_str(&format!("`\\{name}{}`\n\n", usage(sig)));
        }
        if let Some(one_of) = record.get("one_of").and_then(Json::as_array) {
            markdown.push_str("Here it holds one of, by path:\n");
            for alt in one_of {
                let body = alt.get("body").and_then(Json::as_str).unwrap_or_default();
                let line = alt.get("line").and_then(Json::as_u64).unwrap_or_default();
                let shown = if body.is_empty() { "(empty)".to_string() } else { format!("`{body}`") };
                markdown.push_str(&format!("- {shown} (line {line})\n"));
            }
            markdown.push('\n');
        }
        let chains: Vec<&Json> = match record.get("one_of").and_then(Json::as_array) {
            Some(alts) => alts.iter().filter_map(|a| a.get("chain")).filter(|c| c.is_array()).collect(),
            None => record.get("chain").into_iter().collect(),
        };
        for chain in chains {
            markdown.push_str(&format!("Redefined by: {}\n\n", chain_line(chain)));
        }
        // Each argument once: its type and what the comment above the definition says of it.
        let doc = documented(record);
        let said = |n: &str| doc.as_ref()?.params.iter().find(|(k, _)| format!("⟨arg{k}⟩") == n).map(|(_, t)| t);
        let types = arg_types(record);
        for (arg, ty) in &types {
            match said(arg) {
                Some(text) => markdown.push_str(&format!("- `{arg}` {ty}: {text}\n")),
                None => markdown.push_str(&format!("- `{arg}` {ty}\n")),
            }
        }
        let untyped: Vec<_> = doc
            .iter()
            .flat_map(|d| &d.params)
            .filter(|(n, _)| !types.iter().any(|(a, _)| *a == format!("⟨arg{n}⟩")))
            .collect();
        for (n, text) in &untyped {
            markdown.push_str(&format!("- `⟨arg{n}⟩` {text}\n"));
        }
        if let Some(ty) = text("yields") {
            markdown.push_str(&format!("- yields {ty}\n"));
        }
        if !types.is_empty() || !untyped.is_empty() || text("yields").is_some() {
            markdown.push('\n');
        }
        if let Some(doc) = &doc {
            // the comment stands above the definition, whose place the card's foot gives
            if !doc.text.is_empty() {
                markdown.push_str(&format!("{} {}\n\n", doc.text, extracted("from the source comment")));
            }
            if !doc.example.is_empty() {
                markdown.push_str(&format!("```tex\n{}\n```\n\n", doc.example.join("\n")));
            }
        } else if let Some(body) = text("body").filter(|b| {
            // a primitive, or a name standing for itself, expands to nothing worth a line
            text("signature").is_none_or(str::is_empty)
                && text("tag") != Some("primitive")
                && *b != format!("\\{name}")
                && !b.is_empty()
                && b.chars().count() <= analysis.settings.conventions.completion.body
        }) {
            // No comment and no arguments: what it stands for says most (`\draw` is `\path [draw]`).
            markdown.push_str(&format!("Expands to `{body}`\n\n"));
        }
        let said = text("does").is_some()
            || mined.is_some_and(|m| m.text.is_some())
            || doc.as_ref().is_some_and(|d| !d.text.is_empty());
        if let (false, Some(file), Some(line)) = (said, text("file"), record.get("line").and_then(Json::as_u64)) {
            markdown.push_str(&format!("No documentation found. Defined at `{file}:{line}`.\n\n"));
        }
        let mut foot = Vec::new();
        if let (Some(file), Some(line)) = (text("file"), record.get("line").and_then(Json::as_u64)) {
            foot.push(format!("`{file}:{line}`"));
        }
        if let Some((doc, url)) = mined.and_then(|m| m.link.as_ref()) {
            foot.push(format!("[{doc}]({url})"));
        } else if let (Some(doc), Some(url)) = (text("documentation"), text("reference")) {
            foot.push(format!("[{doc}]({url})"));
        } else if let Some(doc) = text("documentation") {
            markdown.push_str(&format!("{doc}\n\n"));
        }
        if !foot.is_empty() {
            markdown.push_str(&format!("{}\n\n", foot.join(" · ")));
        }
    }
    if records.len() == 1 {
        markdown.push_str(&super::knowledge::about(&topics(analysis, name, &records)));
    }
    Some(markdown)
}

/// What the document's calls of `name` typeset: the call at `at` in `path`
/// first, marked, then at most two others, the rest counted (Explain
/// lists them all).
fn typesets(analysis: &Analysis, name: &str, path: &str, at: Pos) -> String {
    const OTHERS: usize = 2;
    let Some(sym) = analysis.interner.lookup(name) else { return String::new() };
    let mut calls: Vec<crate::engine::tex::Span> = analysis
        .facts
        .expansions
        .iter()
        .filter(|e| e.name == sym && query::origin(analysis, e.span.file) == "document")
        .map(|e| e.span)
        .collect();
    calls.sort_by_key(|s| (s.file, s.line, s.col));
    calls.dedup();
    let here = calls
        .iter()
        .rposition(|s| analysis.file_name(s.file) == path && (s.line, s.col) <= (at.line, at.col) && s.line == at.line);
    // each call that sets text, with the lines that set the same text together
    let mut sets: Vec<(String, Vec<u32>, bool)> = Vec::new();
    for (i, call) in calls.iter().enumerate() {
        let Some(set) = query::typeset_by(analysis, *call) else { continue };
        let mine = here == Some(i);
        match sets.iter_mut().find(|(s, ..)| *s == set) {
            Some((_, lines, m)) => {
                lines.push(call.line);
                *m |= mine;
            }
            None => sets.push((set, vec![call.line], mine)),
        }
    }
    sets.sort_by_key(|(.., mine)| !mine);
    let lines = |ls: &[u32]| {
        let named = ls.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
        format!("line{} {named}", if ls.len() > 1 { "s" } else { "" })
    };
    let mut out = String::new();
    let mut rest = sets.iter();
    // the call at the cursor stands out, the others are muted
    if let Some((set, _, true)) = sets.first() {
        rest.next();
        out.push_str(&format!("**Typesets {set} here**\n\n"));
    }
    let others: Vec<_> = rest.collect();
    if others.is_empty() {
        return out;
    }
    let shown: Vec<String> =
        others.iter().take(OTHERS).map(|(set, ls, _)| format!("*{set} at {}*", lines(ls))).collect();
    let more = match others.len() > OTHERS {
        true => format!(" *+{} more*", others.len() - OTHERS),
        false => String::new(),
    };
    let heading = if out.is_empty() { "Typesets" } else { "Elsewhere" };
    format!("{out}*{heading}:* {}{more}\n\n", shown.join(" · "))
}

/// The knowledge-base topics a command's hover touches: the types of its
/// arguments, and the kind of quantity a register or parameter holds.
fn topics(analysis: &Analysis, name: &str, records: &[Record]) -> Vec<String> {
    // The command itself first (`\\`: a line break), then what it reads.
    let mut out: Vec<String> = std::iter::once(format!("\\{name}"))
        .chain(
            records
                .iter()
                .flat_map(|r| r.get("types").and_then(Json::as_array).into_iter().flatten().filter_map(Json::as_str))
                .filter_map(|t| Some(t.split_once(' ')?.1.to_string())),
        )
        .collect();
    if let Some(meaning) = analysis.interner.lookup(name).map(|s| analysis.env.meaning(s)) {
        out.extend(crate::analysis::roles::Ty::of_meaning(&meaning).map(|t| t.to_string()));
        if let Some(crate::engine::builtins::Primitive::Register(k)) = meaning.prim() {
            out.push(k.as_str().to_string());
        }
    }
    out
}

/// The types a record gives its arguments ([`crate::analysis::roles::Ty`]), each by
/// its `⟨argN⟩` in the usage line.
fn arg_types(record: &Record) -> Vec<(String, String)> {
    let types = record.get("types").and_then(Json::as_array).into_iter().flatten().filter_map(Json::as_str);
    types
        .filter_map(|t| t.strip_prefix('#')?.split_once(' '))
        .map(|(n, ty)| (format!("⟨arg{n}⟩"), ty.to_string()))
        .collect()
}

/// After `#` in a definition's body: the parameters of the macro the line
/// defines, each with its type and what the comment above it says of it.
fn parameter_items(analysis: &Analysis, doc: &Text, before: &str, pos: Pos) -> Option<Vec<CompletionItem>> {
    let typed =
        before.strip_suffix('#').or_else(|| before.trim_end_matches(|c: char| c.is_ascii_digit()).strip_suffix('#'))?;
    // `##` is a parameter of a definition inside this one
    if typed.ends_with('#') {
        return None;
    }
    let path = analysis.file_name(analysis.main_file);
    let def = (analysis.facts.defs.iter())
        .filter(|d| d.span.line <= pos.line && analysis.file_name(d.span.file) == path && d.mac.is_some())
        .max_by_key(|d| (d.span.line, d.span.col))?;
    let arity = def.mac.as_ref()?.parameter_text.arity;
    let name = analysis.interner.name(def.name).to_string();
    let record = query::explain(analysis, std::slice::from_ref(&name), false).ok()?.0.into_iter().next();
    let types = record.as_ref().map(arg_types).unwrap_or_default();
    let comment = record.as_ref().and_then(documented);
    let start = to_lsp(doc, Pos::new(pos.line, pos.col - (before.len() - typed.len()) as u32));
    let range = Range { start, end: to_lsp(doc, pos) };
    let items = (1..=arity).map(|n| {
        let ty = types.iter().find(|(a, _)| *a == format!("⟨arg{n}⟩")).map(|(_, t)| t.clone());
        let about = comment.as_ref().and_then(|d| d.params.iter().find(|(k, _)| *k == n)).map(|(_, t)| t.clone());
        let label = format!("#{n}");
        CompletionItem {
            text_edit: Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text: label.clone() })),
            detail: Some(
                [ty.clone(), Some(format!("parameter {n} of \\{name}"))]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("  "),
            ),
            documentation: about.map(lsp_types::Documentation::String),
            kind: Some(CompletionItemKind::VARIABLE),
            sort_text: Some(format!("{n:02}")),
            label,
            ..Default::default()
        }
    });
    Some(items.collect())
}

/// The label that sets text satex extracted apart from what it authored.
fn extracted(origin: &str) -> String {
    format!("*({origin})*")
}

/// The same for a mined manual: linked to where it can be read, with the
/// section and the source line that document it.
fn extracted_from_manual(m: &crate::plugins::docs::Entry) -> String {
    let manual = m.link.as_ref().and_then(|(l, _)| l.strip_prefix("texdoc ")).unwrap_or(&m.from);
    let section = m.at.as_ref().and_then(|p| p.section.as_ref()).map(|s| format!(", § {s}")).unwrap_or_default();
    let origin = match &m.link {
        Some((_, url)) => format!("from the [{manual} manual]({url}){section}"),
        None => format!("from the {manual} manual{section}"),
    };
    let place = m.at.as_ref().map(|p| format!(" `{}:{}`", p.path, p.line)).unwrap_or_default();
    format!("{}{place}", extracted(&origin))
}

/// The comment block above the definition a record names.
fn documented(record: &Record) -> Option<super::doccomment::DocComment> {
    let path = record.get("path").and_then(Json::as_str)?;
    let line = record.get("line").and_then(Json::as_u64)? as usize;
    super::doccomment::above(&crate::project::overlay::read_to_string(std::path::Path::new(path)).ok()?, line)
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
    let types = arg_types(record);
    let mut parameters = Vec::new();
    for n in 1..=9u8 {
        let arg = format!("⟨arg{n}⟩");
        if !label.contains(&arg) {
            break;
        }
        let text = comment.as_ref().and_then(|d| d.params.iter().find(|(k, _)| *k == n)).map(|(_, t)| t.clone());
        let ty = types.iter().find(|(a, _)| *a == arg).map(|(_, t)| t.clone());
        let text = match (ty, text) {
            (Some(ty), Some(text)) => Some(format!("{ty}: {text}")),
            (ty, text) => ty.or(text),
        };
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
            // Required text, as written.
            '‹' => {
                let end = chars[i..].iter().position(|c| *c == '›').map_or(chars.len(), |k| i + k);
                out.extend(&chars[i..end]);
                i = end + 1;
            }
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

/// The style a `\bibliographystyle` names or biblatex's `style=` option
/// gives: its `.bst` or `.bbx` file when one is found, the databases it
/// formats and what it does.
fn bib_style_hover(analysis: &Analysis, path: &str, pos: Pos, word: &str) -> Option<String> {
    use crate::engine::builtins::OccKind;
    let here = |line: u32, file| line == pos.line && analysis.file_name(file) == path;
    let bibtex = analysis
        .facts
        .occurrences
        .iter()
        .any(|o| o.key == word && o.kind == OccKind::BibStyle && here(o.span.line, o.span.file));
    let style = |o: &String| o.split_once('=').is_some_and(|(k, v)| k.trim() == "style" && v.trim() == word);
    let load = analysis
        .facts
        .loads
        .iter()
        .find(|l| l.name == "biblatex" && here(l.span.line, l.span.file) && l.options.iter().any(style));
    if !bibtex && load.is_none() {
        return None;
    }
    let (what, ext, topic) = if bibtex { ("BibTeX", "bst", "bst") } else { ("biblatex", "bbx", "biblatex style") };
    let mut md = heading(&format!("{what} style"), &word);
    let file = format!("{word}.{ext}");
    let shipped =
        load.and_then(|l| l.path.as_deref()).and_then(|p| Path::new(p).parent()).map(|d| d.join("bbx").join(&file));
    if let Some(found) = style_file(analysis, path, &file, shipped) {
        md.push_str(&format!("`{}`\n\n", found.display()));
    }
    let databases: Vec<&str> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Bibliography)
        .map(|o| o.key.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if !databases.is_empty() {
        md.push_str(&format!(
            "It formats the cited entries of {}.\n\n",
            databases.iter().map(|d| format!("`{d}`")).collect::<Vec<_>>().join(", ")
        ));
    }
    md.push_str(&super::knowledge::about(&[format!("{topic} {word}")]));
    Some(md)
}

/// The style file `file`: beside the document `path`, in `shipped`, on BSTINPUTS, else in the installation's styles.
pub(super) fn style_file(analysis: &Analysis, path: &str, file: &str, shipped: Option<PathBuf>) -> Option<PathBuf> {
    let dirs = crate::project::paths::effective(
        "BSTINPUTS",
        &analysis.project,
        &analysis.settings.paths.bstinputs,
        &analysis.project.root,
    );
    let beside = Path::new(path).parent().map(|d| d.join(file));
    let found = beside.into_iter().chain(shipped).chain(dirs.dirs.iter().map(|d| d.join(file))).find(|p| p.is_file());
    found.or_else(|| crate::caches::distribution::file_under(&analysis.distribution.roots, "bibtex/bst", file))
}

/// A package or class name in `\usepackage`, `\RequirePackage` or
/// `\documentclass`: its file and `\Provides…` line, the options it offers
/// and the ones passed, what it defines, what it loads, and the run's
/// diagnostics inside it.
pub(super) fn load_hover(analysis: &Analysis, path: &str, pos: Pos, word: &str) -> Option<String> {
    let load = analysis
        .facts
        .loads
        .iter()
        .find(|l| l.name == word && l.span.line == pos.line && analysis.file_name(l.span.file) == path)?;
    let mut md = heading(load.kind.as_str(), &load.name);
    let path_of = load.path.as_deref().map(std::path::Path::new);
    if let Some(about) = path_of.and_then(|p| crate::plugins::docs::about(&analysis.settings.docs, p)) {
        md.push_str(&format!("{about}\n\n"));
    }
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

/// The file an argument typed [`crate::analysis::roles::Ty::File`] names under the
/// cursor, found beside the document or the project, through kpathsea for
/// databases, with the extensions of its kind.
fn file_target(analysis: &Analysis, doc: &Text, pos: Pos) -> Option<GotoDefinitionResponse> {
    let line = doc.line(pos.line);
    let (before, after) = line.split_at(crate::engine::source::byte_of_col(line, pos.col).min(line.len()));
    let arg = argument_at(before).filter(|a| a.open == '{')?;
    let at = crate::analysis::roles::braced_groups(arg.groups).len();
    let roles = crate::analysis::roles::roles(analysis, arg.cmd);
    let ty = roles
        .iter()
        .filter(|r| r.arg == at)
        .map(|r| r.kind.ty())
        .find(|t| matches!(t, crate::analysis::roles::Ty::File(_)))?;
    let name = format!(
        "{}{}",
        arg.inside.rsplit(',').next().unwrap_or_default().trim_start(),
        after.split([',', '}']).next().unwrap_or_default().trim_end()
    );
    let (exts, _) = file_exts(analysis, ty)?;
    let main = analysis
        .files
        .get(usize::from(analysis.main_file))
        .map(|f| crate::project::paths::dir_of(std::path::Path::new(&f.path)));
    let dirs = [Some(analysis.project.root.clone()), main];
    let beside = dirs.into_iter().flatten().flat_map(|d| {
        let names = std::iter::once(name.clone()).chain(exts.iter().map(|e| format!("{name}.{e}"))).collect::<Vec<_>>();
        names.into_iter().map(move |n| d.join(n))
    });
    let found = beside
        .chain(crate::plugins::bib::resolve(analysis, &name).filter(|_| exts.iter().any(|e| e == "bib")))
        .find(|p| p.is_file())?;
    let zero = Position { line: 0, character: 0 };
    Some(GotoDefinitionResponse::Array(vec![Location {
        uri: path_uri(&found.to_string_lossy()),
        range: Range { start: zero, end: zero },
    }]))
}

/// `textDocument/definition`: the load target ([`load_target`]) if the
/// cursor sits on one; else every definition site [`query::explain`] knows
/// for the control sequence under the cursor, or the `\label`, `\bibitem`
/// or bibliography entry a key names.
pub fn definition(analysis: &Analysis, doc: &Text, path: &str, position: Position) -> Option<GotoDefinitionResponse> {
    let pos = to_pos(doc, position);
    let (name, is_command) = word_at(doc.line(pos.line), pos.col)?;
    if let Some(target) = file_target(analysis, doc, pos).or_else(|| load_target(analysis, path, pos, &name)) {
        return Some(target);
    }
    let sources = Sources::default();
    // a key in an option list (`\draw[above]`, `\tikzset{x/.style=…}`): where its code is defined
    let line = doc.line(pos.line);
    let before = &line[..crate::engine::source::byte_of_col(line, pos.col).min(line.len())];
    if !is_command && let Some(arg) = argument_at(before) {
        let item = arg.inside.rsplit(',').next().unwrap_or_default().trim_start();
        let rest: String = line[before.len()..].chars().take_while(|c| !matches!(c, ',' | '=' | ']' | '}')).collect();
        let key = format!("{item}{rest}").trim().to_string();
        let resolved = key_list_owner(&arg).and_then(|owner| query::resolve_key(analysis, &owner, &key));
        let found = resolved.or_else(|| {
            query::pgfkeys(analysis, None).into_iter().find(|r| {
                r.get("key").and_then(Json::as_str).is_some_and(|k| {
                    k.rsplit_once('/').is_some_and(|(_, last)| last == key) || k.ends_with(&format!("/{key}"))
                })
            })
        });
        if let Some(at) =
            found.as_ref().and_then(|r| r.get("defined")?.as_object()).and_then(|d| record_location(d, &sources))
        {
            return Some(GotoDefinitionResponse::Array(vec![at]));
        }
    }
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
    // The kernel's own definition of a name the run redefines.
    let kernel = analysis.interner.lookup(&name).and_then(|s| analysis.kernel_sites.get(&s));
    records.extend(kernel.map(|span| query::place(analysis, *span)));
    let mut locations: Vec<Location> = Vec::new();
    for at in records.iter().filter_map(|r| record_location(r, &sources)) {
        if !locations.contains(&at) {
            locations.push(at);
        }
    }
    // The run may stop before a later file defines the name: scan the source.
    if locations.is_empty() && is_command {
        for site in super::unreached::sites(analysis, path, &name) {
            eprintln!("satex: \\{name} at {}:{} not reached by the analysis (source scan)", site.path, site.pos.line);
            let position = to_lsp(&site.text, site.pos);
            locations.push(Location { uri: path_uri(&site.path), range: Range { start: position, end: position } });
        }
    }
    // A primitive has no line: its documentation answers, else the format's source.
    if locations.is_empty() {
        let at = |r: &Record| {
            let reference = r.get("reference").and_then(Json::as_str).and_then(|u| u.parse().ok());
            let uri = reference.or_else(|| r.get("path").and_then(Json::as_str).map(path_uri))?;
            Some(Location { uri, range: Range::default() })
        };
        locations.extend(records.iter().filter_map(at));
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
    let from = crate::engine::source::byte_of_col(text_line, col);
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
    let first = crate::engine::source::col_of_byte(text_line, found) + u32::from(is_command);
    let width = crate::engine::source::columns(name) as u32;
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
fn rename_plan(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    position: Position,
) -> Result<crate::queries::rename::Plan, String> {
    let pos = to_pos(doc, position);
    let (name, is_command, col) = word_span(doc.line(pos.line), pos.col).ok_or("nothing renamable here")?;
    crate::queries::rename::plan(analysis, &Sources::default(), path, Pos::new(pos.line, col), &name, is_command)
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
    for edit in crate::queries::rename::edits(&plan, analysis, new_name)? {
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
pub fn completion(analysis: &Analysis, doc: &Text, path: &str, position: Position) -> CompletionResponse {
    completion_at(analysis, doc, path, position)
}

fn completion_at(analysis: &Analysis, doc: &Text, path: &str, position: Position) -> CompletionResponse {
    let pos = to_pos(doc, position);
    let line = doc.line(pos.line);
    let (before, after) = line.split_at(crate::engine::source::byte_of_col(line, pos.col).min(line.len()));
    if let Some(items) = super::directive::completion(before) {
        return CompletionResponse::Array(items);
    }
    match super::directive::in_comment(before) {
        Some(super::directive::InComment::Names(mut items)) => {
            // the items replace the name typed from its backslash on
            let typed = before.rsplit('\\').next().unwrap_or_default().chars().count() as u32 + 1;
            let range = Range { start: to_lsp(doc, Pos::new(pos.line, pos.col.saturating_sub(typed))), end: position };
            for it in &mut items {
                let new_text = it.insert_text.take().unwrap_or_default();
                it.text_edit = Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text }));
            }
            return CompletionResponse::Array(items);
        }
        Some(super::directive::InComment::Nothing) => return CompletionResponse::Array(Vec::new()),
        Some(super::directive::InComment::Code) | None => {}
    }
    if let Some(items) = super::directive::relations(before) {
        return CompletionResponse::Array(items);
    }
    if let Some(items) = parameter_items(analysis, doc, before, pos) {
        return CompletionResponse::Array(items);
    }
    // What the text at the cursor is ([`super::type_at`]): the names of that type.
    let at = super::type_at::type_at(analysis, doc, path, pos);
    let items = typed_items(analysis, doc, path, &at, after, pos);
    match at.place.as_ref() {
        Some(place) if !cs_re().is_match(before) => {
            let arg = place.arg();
            let mut items = items;
            // A braced name replaces the word typed and the rest of it, and closes the brace.
            let typed = arg.inside.rsplit(',').next().unwrap_or_default().trim_start();
            if arg.open == '{' && !typed.contains('=') {
                let typed = &at.item.typed;
                let rest = after.chars().take_while(|c| c.is_alphanumeric() || ":-_./*".contains(*c)).count();
                let close = if after[after.char_indices().nth(rest).map_or(after.len(), |(i, _)| i)..].starts_with('}')
                {
                    ""
                } else {
                    "}"
                };
                let range = Range {
                    start: to_lsp(doc, Pos::new(pos.line, pos.col - typed.chars().count() as u32)),
                    end: to_lsp(doc, Pos::new(pos.line, pos.col + rest as u32)),
                };
                for it in items.iter_mut().filter(|i| i.text_edit.is_none() && i.insert_text.is_none()) {
                    let new_text = format!("{}{close}", it.label);
                    it.text_edit = Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text }));
                }
            }
            // An option replaces what is typed of it, `-` and `>` too (`-Ki` for `-Kite`):
            // an editor's word would stop at them and filter the item out.
            if arg.open == '[' {
                let typed = typed.rsplit('=').next().unwrap_or_default().trim_start();
                let start = to_lsp(doc, Pos::new(pos.line, pos.col - typed.chars().count() as u32));
                let range = Range { start, end: position };
                for it in items.iter_mut().filter(|i| i.text_edit.is_none()) {
                    let new_text = it.insert_text.take().unwrap_or_else(|| it.label.clone());
                    it.filter_text.get_or_insert_with(|| it.label.clone());
                    it.text_edit = Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text }));
                }
            }
            CompletionResponse::Array(items)
        }
        // a name of a known kind outside an argument (`\draw (`, `2.5c`): no `\` typed
        _ if !items.is_empty() => CompletionResponse::Array(items),
        _ => {
            // the name typed, with the letters of that place (`_` and `:` under `\ExplSyntaxOn`)
            let letters = analysis.facts.letters_at(analysis.main_file, pos.line, pos.col);
            // a command completes only where one is being typed: its `\` right before the letters
            let word = before.chars().rev().take_while(|c| c.is_alphabetic() || letters.contains(c)).count();
            if before.chars().rev().nth(word) != Some('\\') {
                return CompletionResponse::Array(Vec::new());
            }
            let typed = before
                .rfind('\\')
                .map(|i| &before[i + 1..])
                .filter(|t| t.chars().all(|c| c.is_alphabetic() || letters.contains(&c)))
                .unwrap_or("");
            let (mut items, more) = command_items(analysis, pos, typed);
            // In an argument TeX reads as a quantity, the names holding one
            // that fits come first.
            if let Some(want) = argument_at(before).and_then(|arg| quantity_wanted(analysis, &arg)) {
                for item in &mut items {
                    let sym = analysis.interner.lookup(&item.label);
                    let ty = sym.and_then(|s| crate::analysis::roles::Ty::of_meaning(&analysis.env.meaning(s)));
                    let register = want == crate::analysis::roles::Ty::Register
                        && matches!(ty, Some(crate::analysis::roles::Ty::Reg(_)));
                    if register || ty.is_some_and(|t| t.leq(want)) {
                        item.sort_text = item.sort_text.take().map(|s| format!("!{s}"));
                    }
                }
            }
            CompletionResponse::List(lsp_types::CompletionList { is_incomplete: more, items })
        }
    }
}

/// The quantity the braced argument `arg` is read as ([`crate::analysis::roles::Ty`]),
/// a register's for a register name, or any register's.
fn quantity_wanted(analysis: &Analysis, arg: &Arg) -> Option<crate::analysis::roles::Ty> {
    use crate::analysis::roles::Ty;
    let at = crate::analysis::roles::braced_groups(arg.groups).len();
    let roles = crate::analysis::roles::roles(analysis, arg.cmd);
    let (_, ty) = crate::analysis::roles::arg_types(&roles).into_iter().find(|(a, _)| arg.open == '{' && *a == at)?;
    let ty = if matches!(ty, Ty::Reg(_)) { ty.parent() } else { ty };
    (ty.leq(Ty::Quantity) || ty == Ty::Register).then_some(ty)
}

/// A control sequence being typed at the end of the text.
pub(super) fn cs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\[A-Za-z@]*$").unwrap())
}

/// The argument the cursor is in: the command it belongs to, its opening
/// delimiter, the complete groups between them and the column of the `\`.
pub(super) struct Arg<'a> {
    pub(super) cmd: &'a str,
    pub(super) open: char,
    pub(super) groups: &'a str,
    pub(super) col: u32,
    /// The argument's text up to the cursor.
    pub(super) inside: &'a str,
}

pub(super) fn argument_at(before: &str) -> Option<Arg<'_>> {
    static HEAD: OnceLock<Regex> = OnceLock::new();
    let head = HEAD
        .get_or_init(|| Regex::new(r"\\([A-Za-z@]+)\*?((?:\s*(?:\[[^\[\]]*\]|\{[^{}]*\}|\([^()]*\)))*)\s*$").unwrap());
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
                    col: crate::engine::source::col_of_byte(before, start),
                    inside: &before[i + 1..],
                });
            }
            _ => {}
        }
    }
    None
}

/// How many name characters precede the cursor on its line.
fn before_word(doc: &Text, pos: Pos) -> u32 {
    let line = doc.line(pos.line);
    let before = &line[..crate::engine::source::byte_of_col(line, pos.col).min(line.len())];
    before.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '*').count() as u32
}

/// The entries of every database the document names, once per analysis.
fn bib_entries(analysis: &Analysis) -> &[crate::plugins::bib::Entry] {
    analysis.bib.get_or_init(|| {
        let mut seen = BTreeSet::new();
        analysis
            .facts
            .occurrences
            .iter()
            .filter(|o| o.kind == crate::engine::builtins::OccKind::Bibliography)
            .filter_map(|o| crate::plugins::bib::resolve(analysis, &o.key))
            .filter(|path| seen.insert(path.clone()))
            .filter_map(|path| Some((crate::project::overlay::read_to_string(&path).ok()?, path)))
            .flat_map(|(text, path)| {
                let file = path.to_string_lossy().into_owned();
                crate::plugins::bib::entries(&text)
                    .into_iter()
                    .map(move |e| crate::plugins::bib::Entry { file: file.clone(), ..e })
            })
            .collect()
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

/// The names that fit each type [`super::type_at::type_at`] found at the cursor, the most precise
/// type's first.
fn typed_items(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    at: &super::type_at::At,
    after: &str,
    pos: Pos,
) -> Vec<CompletionItem> {
    use crate::analysis::roles::{KeyKind, Ty};
    use crate::engine::builtins::OccKind;
    let mut out: Vec<CompletionItem> = Vec::new();
    // a name being defined is not one of those defined
    for hit in at.hits.iter().filter(|h| !h.defines) {
        let typed = hit.typed.as_deref().unwrap_or(&at.item.typed);
        let arg = at.place.as_ref().map(super::type_at::Place::arg);
        // a type plugin's items first ([`super::plugins`])
        if let Some(items) = super::plugins::complete(analysis, doc, path, pos, at, hit) {
            // after `key=` only values: nothing else fits a known key's value
            let keyed = matches!(hit.ty, Ty::Key(KeyKind::Option | KeyKind::Pgf | KeyKind::Declared));
            let valued = arg.as_ref().is_some_and(|a| {
                a.open != '['
                    && a.inside.rsplit(',').next().is_some_and(|k| k.contains('='))
                    && !query::options(analysis, a.cmd).is_empty()
            });
            if keyed && valued {
                return items;
            }
            out.extend(items.into_iter().filter(|i| !out.iter().any(|o| o.label == i.label)).collect::<Vec<_>>());
            continue;
        }
        let items = match (hit.ty, &arg) {
            (Ty::Key(KeyKind::Label), _) => {
                let labels = analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Label);
                let labels: BTreeSet<&str> = labels.filter_map(|o| o.key.strip_prefix(hit.prefix.as_str())).collect();
                let labels = labels.into_iter().filter(|k| !k.is_empty() && k.starts_with(typed));
                labels.map(|k| item(k.to_string(), CompletionItemKind::REFERENCE, None, 0)).collect()
            }
            (Ty::Key(KeyKind::Cite), _) => cite_items(analysis),
            (Ty::Key(KeyKind::Environment), Some(arg)) => environment_items(analysis, doc, arg, after, pos),
            (Ty::File(kind), _) if hit.file && !(hit.prefix.is_empty() && hit.suffix.is_empty()) => {
                let ext = match kind {
                    crate::analysis::roles::FileKind::Sty
                    | crate::analysis::roles::FileKind::Cls
                    | crate::analysis::roles::FileKind::Theme => kind.ext(),
                    _ => None,
                };
                library_items(analysis, &hit.prefix, &hit.suffix, ext.map(|e| format!(".{e}")).as_deref(), typed)
            }
            (ty @ Ty::File(_), _) => files(analysis, file_exts(analysis, ty).into_iter(), typed.trim()),
            _ if !hit.prefix.is_empty() => {
                let what = format!("named `{}…`", hit.prefix);
                (defined_with(analysis, &hit.prefix, typed).into_iter())
                    .map(|n| item(n, CompletionItemKind::REFERENCE, Some(what.clone()), 0))
                    .collect()
            }
            _ => Vec::new(),
        };
        out.extend(items.into_iter().filter(|i| !out.iter().any(|o| o.label == i.label)).collect::<Vec<_>>());
    }
    out
}

/// `\usepackage[…]{name}`: the options of the package named after the cursor, after `key=` the
/// values the run saw and the files of the kind it loaded by that value (biblatex's `style=` a `.bbx`).
pub(super) fn package_options(analysis: &Analysis, arg: &Arg, after: &str) -> Vec<CompletionItem> {
    let close = after.find(']').map_or(after.len(), |i| i + 1);
    let named = crate::analysis::roles::braced_groups(&after[close..]);
    let Some(package) =
        named.first().map(|g| g.trim_matches(['{', '}']).split(',').next().unwrap_or_default().trim().to_string())
    else {
        return Vec::new();
    };
    if let Some((key, typed)) = arg.inside.rsplit(',').next().and_then(|o| o.split_once('=')) {
        let mut items: Vec<CompletionItem> = query::option_values(analysis, &package, key.trim())
            .into_iter()
            .filter(|v| v.starts_with(typed.trim()))
            .map(|v| item(v, CompletionItemKind::VALUE, Some(format!("{} of {package}", key.trim())), 0))
            .collect();
        items.extend(files(analysis, option_files(analysis, &package, key.trim()).into_iter(), typed.trim()));
        return items;
    }
    let detail = |d: Option<String>| d.map_or(package.clone(), |d| format!("default: {d}  ({package})"));
    let options = query::package_options(analysis, &package).into_iter();
    options.map(|(key, d)| item(key, CompletionItemKind::PROPERTY, Some(detail(d)), 0)).collect()
}

/// A `[…]` key list of a command or environment: after `key=` the tips, patterns, colors or nodes
/// the key hands pgf; else the keys, or the default of an optional argument.
pub(super) fn bracket_keys(analysis: &Analysis, doc: &Text, arg: &Arg) -> Vec<CompletionItem> {
    let _ = doc;
    let target = key_list_owner(arg).unwrap_or_else(|| arg.cmd.to_string());
    if let Some((key, _)) = arg.inside.rsplit(',').next().and_then(|o| o.split_once('=')) {
        let options = query::options(analysis, &target);
        let family = options.iter().find_map(|r| r.get("family")?.as_str().filter(|f| f.starts_with('/')));
        if let Some(family) = family {
            let path = format!("{}/{}", family.trim_end_matches('/'), key.trim());
            let drawn = drawn_items(
                analysis,
                &path,
                &[Stroke::Arrows, Stroke::Pattern, Stroke::Color, Stroke::Node, Stroke::NodeOf],
            );
            if !drawn.is_empty() {
                return drawn;
            }
        }
    }
    let mut items = option_items(analysis, &target);
    // An optional argument with a default offers it; a numeric default
    // makes it a count (`\newcommand`'s `[0]`): the counts follow.
    if items.is_empty()
        && let Some(default) = optional_default(analysis, &target, arg.groups.matches('[').count())
    {
        items.push(item(default.clone(), CompletionItemKind::VALUE, Some("default".into()), 0));
        if default.trim().parse::<u8>().is_ok() {
            items.extend(
                (0..=9u8)
                    .map(|n| n.to_string())
                    .filter(|n| *n != default)
                    .map(|n| item(n, CompletionItemKind::VALUE, None, 1)),
            );
        }
    }
    items
}

/// The databases' entries, found by key, title, author or year; then keys a `\bibitem` or
/// another citation gave.
fn cite_items(analysis: &Analysis) -> Vec<CompletionItem> {
    use crate::engine::builtins::OccKind;
    let entries = bib_entries(analysis);
    let mut out: Vec<CompletionItem> = entries
        .iter()
        .map(|e| {
            let who = e.author.split(" and ").next().unwrap_or_default();
            let surname = who.split(',').next().unwrap_or(who).split_whitespace().last().unwrap_or_default();
            let detail = match (surname, e.year.as_str()) {
                ("", "") => e.kind.clone(),
                (s, "") => s.to_string(),
                ("", y) => y.to_string(),
                (s, y) => format!("{s} ({y})"),
            };
            CompletionItem {
                filter_text: Some(format!("{} {} {} {}", e.key, e.title, e.author, e.year)),
                documentation: (!e.title.is_empty())
                    .then(|| lsp_types::Documentation::String(format!("{}\n{} {}", e.title, e.author, e.year))),
                ..item(e.key.clone(), CompletionItemKind::REFERENCE, Some(detail), 0)
            }
        })
        .collect();
    let known: BTreeSet<&str> = entries.iter().map(|e| e.key.as_str()).collect();
    let occ = analysis.facts.occurrences.iter().filter(|o| matches!(o.kind, OccKind::BibItem | OccKind::Cite));
    let other: BTreeSet<&str> = occ.map(|o| o.key.as_str()).filter(|k| !k.is_empty() && !known.contains(k)).collect();
    out.extend(other.into_iter().map(|k| item(k.to_string(), CompletionItemKind::REFERENCE, None, 1)));
    out
}

/// The environments open at the `\begin`/`\end`, then every one defined at the cursor; after
/// `\begin{` as a snippet with its arguments and `\end`.
fn environment_items(analysis: &Analysis, doc: &Text, arg: &Arg, after: &str, pos: Pos) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let open = open_environments(analysis, (pos.line, arg.col));
    let snippet = arg.cmd == "begin";
    let close = after.starts_with('}');
    // A name already written after the cursor stays as it is. Text that only follows
    // it (a comment, a word after a space) is not part of the name.
    let fresh = close || !after.starts_with(|c: char| c.is_alphanumeric() || ":-_./*@".contains(c));
    let here = open.iter().rev().enumerate().filter(|_| !snippet);
    out.extend(here.map(|(i, e)| item(e.clone(), CompletionItemKind::MODULE, Some("open here".into()), i)));
    for (env, items) in environments(analysis, pos) {
        if !snippet && open.contains(&env) {
            continue;
        }
        let mut it = item(env.clone(), CompletionItemKind::MODULE, None, 100);
        if snippet && fresh {
            let body = if items { "\t\\item $0" } else { "\t$0" };
            // From the name typed so far through a `}` the editor already closed.
            let typed = before_word(doc, pos);
            let range = Range {
                start: to_lsp(doc, Pos::new(pos.line, pos.col - typed)),
                end: to_lsp(doc, Pos::new(pos.line, pos.col + close as u32)),
            };
            let args: String = (1..=environment_arguments(analysis, &env)).map(|i| format!("{{${i}}}")).collect();
            let new_text = format!("{env}}}{args}\n{body}\n\\end{{{env}}}");
            it.text_edit = Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text }));
            it.insert_text_format = Some(InsertTextFormat::SNIPPET);
        }
        out.push(it);
    }
    out
}

/// Whether a stored key is set with a value (`key=…`): not a switch, a
/// family or a key with a default, and code or a style only when it reads `#1`.
fn takes_value(r: &Record) -> bool {
    let f = |k: &str| r.get(k).and_then(Json::as_str).unwrap_or_default();
    match f("kind") {
        _ if !f("default").is_empty() => false,
        "is if" | "is family" | "bool" | "bool_set" | "bool_set_inverse" => false,
        "style" | "code" | "code args" | "" => r.get("value").and_then(Json::as_str).is_none_or(|v| v.contains("#1")),
        _ => true,
    }
}

/// A key's completion: the key, then `=` with the cursor after it where it takes a value.
fn key_item(analysis: &Analysis, mut it: CompletionItem, key: &str, r: &Record) -> CompletionItem {
    if let Some(path) = r.get("key").and_then(Json::as_str) {
        it = with_preview(it, crate::queries::preview::of_key(analysis, path, None));
    }
    if takes_value(r) {
        it.insert_text = Some(format!("{}=$1", key.replace('$', "\\$")));
        it.insert_text_format = Some(InsertTextFormat::SNIPPET);
    }
    it
}

/// `it` with its picture ([`crate::queries::preview`]): as documentation for an
/// editor, as `data.preview` for the playground.
fn with_preview(mut it: CompletionItem, preview: Option<crate::queries::preview::Preview>) -> CompletionItem {
    if let Some(p) = preview {
        it.documentation = Some(lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: p.markdown(),
        }));
        it.data = Some(serde_json::json!({ "preview": p.svg }));
    }
    it
}

/// What `path`'s code hands a setter of `kinds` that TeX draws: the arrow
/// specs or pattern names the run declared, each with its picture.
fn drawn_items(analysis: &Analysis, path: &str, kinds: &[Stroke]) -> Vec<CompletionItem> {
    let sets = crate::queries::preview::sets(analysis, path);
    let mut out = Vec::new();
    for &kind in kinds.iter().filter(|k| sets.contains(k)) {
        if matches!(kind, Stroke::Node | Stroke::NodeOf) {
            let of = if kind == Stroke::NodeOf { "of " } else { "" };
            let nodes = node_names(analysis, "").into_iter().map(|n| format!("{of}{n}"));
            out.extend(nodes.map(|n| item(n, CompletionItemKind::REFERENCE, Some("node".into()), 0)));
            continue;
        }
        // after the keys, like the tips
        if kind == Stroke::Color {
            out.extend(
                super::plugins::color_items(analysis)
                    .into_iter()
                    .map(|it| CompletionItem { sort_text: Some(format!("{:05}{}", 50_000, it.label)), ..it }),
            );
            continue;
        }
        let (names, what) = match kind {
            Stroke::Arrows => (crate::queries::preview::arrow_specs(analysis), "arrow tips"),
            _ => (crate::queries::preview::pattern_names(analysis), "pattern"),
        };
        out.extend(names.into_iter().map(|n| {
            let it = item(n.clone(), CompletionItemKind::ENUM_MEMBER, Some(what.into()), 50_000);
            with_preview(it, crate::queries::preview::drawn(analysis, kind, &n))
        }));
        // the tips a library of the installation declares, not loaded yet: picking one loads it
        if kind == Stroke::Arrows {
            let have: BTreeSet<String> = out.iter().map(|i| i.label.clone()).collect();
            let start =
                crate::linter::shared::document_start(analysis).map(|s| Position::new(s.line.saturating_sub(1), 0));
            for (tip, load) in crate::linter::shared::library_declared(analysis, "arrow") {
                let label = if tip.contains(char::is_whitespace) { format!("-{{{tip}}}") } else { format!("-{tip}") };
                if have.contains(&label) {
                    continue;
                }
                let mut it = item(label, CompletionItemKind::ENUM_MEMBER, Some(format!("arrow tip of {load}")), 60_000);
                it.additional_text_edits = start
                    .map(|at| vec![TextEdit { range: Range { start: at, end: at }, new_text: format!("{load}\n") }]);
                out.push(it);
            }
        }
    }
    out
}

/// Inside `(` (`\\draw (a) -- (`): the kinds of names that `(…)` groups in
/// statements of the same command formed elsewhere in the document
/// ([`crate::analysis::facts::Facts::slots`]). After `name.` the anchors of a node's shape, as pgf
/// keeps them (`pgf@sh@ns@⟨node⟩` holds the shape, `pgf@anchor@⟨shape⟩@⟨anchor⟩` each anchor).
pub(super) fn node_hits(analysis: &Analysis, doc: &Text, pos: Pos) -> Vec<super::type_at::Hit> {
    let hit =
        |prefix: String| super::type_at::Hit { prefix, ..super::type_at::Hit::of(crate::analysis::roles::Ty::Name) };
    let Some(before) = doc.slice(Pos::new(pos.line.saturating_sub(super::type_at::LINES).max(1), 1), pos) else {
        return Vec::new();
    };
    let before: Vec<char> = before.chars().collect();
    let Some(open) = before.iter().rposition(|c| !(c.is_alphanumeric() || " _-.:".contains(*c))) else {
        return Vec::new();
    };
    let Some(command) = (before[open] == '(').then(|| statement_command(&before[..open])).flatten() else {
        return Vec::new();
    };
    let typed: String = before[open + 1..].iter().collect();
    if let Some((node, anchor)) = typed.split_once('.') {
        let shape = analysis.interner.lookup(&format!("pgf@sh@ns@{}", node.trim()));
        let shape = shape.and_then(|s| {
            let m = analysis.env.meaning(s);
            m.as_macro().map(|m| crate::engine::tex::detokenize(&m.replacement_text, &analysis.interner))
        });
        let anchor =
            |shape| super::type_at::Hit { typed: Some(anchor.to_string()), ..hit(format!("pgf@anchor@{shape}@")) };
        return shape.map(anchor).into_iter().collect();
    }
    // the prefixes `(…)` of this command's statements formed in the run
    let mut kinds = BTreeSet::new();
    for (span, prefix) in &analysis.facts.slots {
        let Some(at) = doc.offset(Pos::new(span.line, span.col)) else { continue };
        // a statement is a few lines: what stands within a window before the slot
        let body = &doc.body()[..at];
        let from = body.char_indices().rev().nth(WINDOW).map_or(0, |(i, _)| i);
        let text: Vec<char> = body[from..].chars().collect();
        if text.last() == Some(&'(') && statement_command(&text[..text.len() - 1]).as_deref() == Some(command.as_str())
        {
            kinds.insert(prefix.clone());
        }
    }
    kinds.into_iter().map(hit).collect()
}

/// How many characters before a slot its statement may start.
const WINDOW: usize = 2000;

/// The command a statement starts with: the first control word after the
/// `;` that ends the statement before (TikZ's statements end there).
fn statement_command(before: &[char]) -> Option<String> {
    let start = before.iter().rposition(|c| *c == ';').map_or(0, |i| i + 1);
    let at = before[start..].iter().position(|c| *c == '\\')? + start + 1;
    let name: String = before[at..].iter().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!name.is_empty()).then_some(name)
}

/// The defined names that start with `prefix`, without it, those that go on with `typed`.
fn defined_with(analysis: &Analysis, prefix: &str, typed: &str) -> BTreeSet<String> {
    (0..analysis.interner.len() as u32)
        .map(crate::engine::tex::Sym)
        .filter(|s| analysis.env.is_defined(*s))
        .filter_map(|s| analysis.interner.name(s).strip_prefix(prefix).map(str::to_string))
        .filter(|n| !n.is_empty() && n.starts_with(typed) && !analysis.settings.conventions.is_internal(n))
        .collect()
}

/// The names of the nodes the document made that start with `typed`, as pgf
/// keeps them (`\pgf@sh@ns@⟨name⟩`). pgf's own (`current subpath start`) are
/// defined by its library code.
fn node_names<'a>(analysis: &'a Analysis, typed: &str) -> BTreeSet<&'a str> {
    (analysis.facts.defs.iter())
        .filter(|d| crate::queries::query::origin(analysis, d.span.file) == "document")
        .filter_map(|d| analysis.interner.name(d.name).strip_prefix("pgf@sh@ns@"))
        .filter(|n| n.starts_with(typed) && !n.is_empty() && !analysis.settings.conventions.is_internal(n))
        .collect()
}

/// The items of a key list at the cursor, from the keys the run stored
/// ([`query::stored_keys`]): after `key=`, the choices of that key; a
/// `/path` typed so far, the keys under its directory, replacing it; after
/// a `/path/.cd` or a family's name, the keys under it; else the keys of
/// the family the command sets ([`option_items`]).
pub(super) fn key_items(analysis: &Analysis, doc: &Text, arg: &Arg, pos: Pos) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut depth = 0u32;
    let mut from = 0;
    for (i, c) in arg.inside.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(&arg.inside[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    let typed = arg.inside[from..].trim_start();
    let stored = |dir: &str| query::stored_keys(analysis, Some(dir));
    let field = |r: &Record, f: &str| r.get(f).and_then(Json::as_str).unwrap_or_default().to_string();
    // The directory relative keys resolve in: the last one the list
    // changed to, else the families the command reads.
    let switched = items.iter().rev().find_map(|item| {
        let key = item.split('=').next()?.trim();
        if let Some(dir) = key.strip_suffix("/.cd") {
            return Some(dir.to_string());
        }
        let family = stored(key).into_iter().any(|r| field(&r, "key") == key && field(&r, "kind") == "is family");
        family.then(|| key.to_string())
    });
    let mut dirs: Vec<String> = match &switched {
        Some(dir) => vec![dir.clone()],
        None => query::options(analysis, arg.cmd).iter().map(|r| field(r, "family")).collect(),
    };
    dirs.dedup();
    // The keys under `dir` down to `depth` levels, as paths relative to it,
    // not its handlers (`.cd`, `.unknown`): shallower ones first.
    let below = |dir: &str, depth: usize| -> Vec<(String, Record)> {
        let named = stored(dir).into_iter().map(|r| (field(&r, "key")[dir.len()..].to_string(), r));
        let mut out: Vec<_> = named
            .filter(|(n, _)| {
                n.split('/').count() <= depth
                    && n.split('/').all(|p| !(p.is_empty() || p.starts_with('.') || p == "unknown"))
            })
            .collect();
        out.sort_by_key(|(n, _)| n.matches('/').count());
        out
    };
    let children = |dir: &str| below(dir, 1);
    let detail = |r: &Record| {
        ["kind", "default"].iter().map(|f| field(r, f)).filter(|d| !d.is_empty()).collect::<Vec<_>>().join("  ")
    };
    let under = |key: &str| -> Vec<String> {
        match key.starts_with('/') {
            true => vec![format!("{key}/")],
            false => dirs.iter().map(|d| format!("{}/{key}/", d.trim_end_matches('/'))).collect(),
        }
    };
    if let Some((key, _)) = typed.split_once('=') {
        // The values a key takes: its choices, or a switch's two.
        let mut out = Vec::new();
        for dir in under(key.trim()) {
            let path = dir.trim_end_matches('/');
            if stored(path).iter().any(|r| field(r, "key") == path && field(r, "kind") == "is if") {
                out.extend(["true", "false"].map(|v| item(v.into(), CompletionItemKind::ENUM_MEMBER, None, 0)));
            }
            out.extend(
                children(&dir).into_iter().map(|(n, _)| item(n, CompletionItemKind::ENUM_MEMBER, Some(path.into()), 0)),
            );
            out.extend(drawn_items(
                analysis,
                path,
                &[Stroke::Arrows, Stroke::Pattern, Stroke::Color, Stroke::Node, Stroke::NodeOf],
            ));
        }
        let values = query::key_values(analysis, arg.cmd, key.trim());
        out.extend(values.into_iter().map(|v| item(v, CompletionItemKind::ENUM_MEMBER, Some(key.trim().into()), 0)));
        return out;
    }
    if typed.starts_with('/') {
        let dir = &typed[..typed.rfind('/').map_or(0, |i| i + 1)];
        let start = to_lsp(doc, Pos::new(pos.line, pos.col - typed.chars().count() as u32));
        let range = Range { start, end: to_lsp(doc, pos) };
        return below(dir, analysis.settings.conventions.completion.key_depth)
            .into_iter()
            .map(|(name, r)| {
                let key = format!("{dir}{name}");
                let rank = name.matches('/').count();
                let mut it = key_item(
                    analysis,
                    item(key.clone(), CompletionItemKind::PROPERTY, Some(detail(&r)), rank),
                    &key,
                    &r,
                );
                let new_text = it.insert_text.take().unwrap_or(key);
                it.text_edit = Some(lsp_types::CompletionTextEdit::Edit(TextEdit { range, new_text }));
                it
            })
            .collect();
    }
    // A key no family stores goes to its `.unknown` handler: tips where that sets arrows (`-Kite`).
    if typed.contains('-') {
        let unknown = |d: &String| format!("{}/.unknown", d.trim_end_matches('/'));
        let tips: Vec<_> = dirs.iter().flat_map(|d| drawn_items(analysis, &unknown(d), &[Stroke::Arrows])).collect();
        if !tips.is_empty() {
            return tips;
        }
    }
    // nothing typed in a list the command reads no family of: the keys beside the paths written
    // before (`/demo/mode=fast, ` offers `/demo/…`)
    if typed.is_empty() && switched.is_none() && dirs.iter().all(String::is_empty) {
        let mut used: Vec<String> = (items.iter())
            .filter_map(|i| i.split('=').next().map(str::trim).filter(|k| k.starts_with('/')))
            .filter_map(|k| Some(k[..k.rfind('/')? + 1].to_string()))
            .collect();
        used.dedup();
        let absolute = |dir: &str, depth| -> Vec<CompletionItem> {
            (below(dir, depth).into_iter())
                .map(|(name, r)| {
                    let key = format!("{dir}{name}");
                    let rank = name.matches('/').count();
                    key_item(
                        analysis,
                        item(key.clone(), CompletionItemKind::PROPERTY, Some(detail(&r)), rank),
                        &key,
                        &r,
                    )
                })
                .collect()
        };
        if !used.is_empty() {
            let depth = analysis.settings.conventions.completion.key_depth;
            return used.iter().flat_map(|d| absolute(d, depth)).collect();
        }
    }
    if switched.is_none() {
        let mut keys = option_items(analysis, arg.cmd);
        if keys.iter().all(|k| k.detail.as_deref().is_some_and(|d| d.contains("positional"))) {
            return Vec::new();
        }
        // The subkeys of a pgfkeys family, after its own keys.
        let deeper = dirs.iter().filter(|d| d.starts_with('/')).flat_map(|d| {
            below(&format!("{}/", d.trim_end_matches('/')), analysis.settings.conventions.completion.key_depth)
                .into_iter()
                .filter(|(n, _)| n.contains('/'))
        });
        keys.extend(deeper.map(|(name, r)| {
            key_item(analysis, item(name.clone(), CompletionItemKind::PROPERTY, Some(detail(&r)), 99_999), &name, &r)
        }));
        return keys;
    }
    dirs.iter()
        .flat_map(|d| {
            below(&format!("{}/", d.trim_end_matches('/')), analysis.settings.conventions.completion.key_depth)
        })
        .map(|(name, r)| {
            let rank = name.matches('/').count();
            key_item(analysis, item(name.clone(), CompletionItemKind::PROPERTY, Some(detail(&r)), rank), &name, &r)
        })
        .collect()
}

/// The keys `name`'s optional argument feeds, from [`query::options`].
/// The default of the `nth` optional argument `name` takes, by its
/// signature ([`query::signature`]).
fn optional_default(analysis: &Analysis, name: &str, nth: usize) -> Option<String> {
    let sym = analysis.interner.lookup(crate::engine::tex::bare_name(name))?;
    let mac = analysis.env.meaning(sym).as_macro()?.clone();
    let spec = query::signature(analysis, sym, &mac)?;
    let optional = spec.items.iter().filter_map(|a| match a {
        crate::engine::tex::ArgType::Optional(d) => Some(d.clone()),
        crate::engine::tex::ArgType::Delimited { open: '[', default, .. } => Some(default.clone()),
        _ => None,
    });
    optional.into_iter().nth(nth).flatten()
}

fn option_items(analysis: &Analysis, name: &str) -> Vec<CompletionItem> {
    let options = query::options(analysis, name);
    let families: BTreeSet<&str> = options.iter().filter_map(|r| r.get("family")?.as_str()).collect();
    // A family whose unknown keys may be arrow specs takes the declared tips.
    let tips: Vec<CompletionItem> = families
        .iter()
        .filter(|f| f.starts_with('/'))
        .flat_map(|f| {
            drawn_items(analysis, &format!("{}/.unknown", f.trim_end_matches('/')), &[Stroke::Arrows, Stroke::Color])
        })
        .collect();
    options
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let family = r.get("family").and_then(Json::as_str).unwrap_or_default();
            let key = r.get("key")?.as_str()?;
            let key = key.strip_prefix(family).map_or(key, |k| k.trim_start_matches('/'));
            let default = r.get("default").and_then(Json::as_str).filter(|d| !d.is_empty());
            let kind = r.get("kind").and_then(Json::as_str).filter(|k| *k != "code");
            // pgfkeys' marker for a key that needs a value reads as words
            let default = default.map(|d| {
                if d.contains("pgfkeysvaluerequired") { "needs a value".to_string() } else { format!("default {d}") }
            });
            let ty = r.get("type").and_then(Json::as_str).map(str::to_string);
            let detail = [ty, kind.map(str::to_string), default, Some(family.to_string())];
            let detail: Vec<String> = detail.into_iter().flatten().filter(|d| !d.is_empty()).collect();
            let rank = r.get("rank").and_then(Json::as_u64).unwrap_or(0) as usize * 10_000 + i.min(9_999);
            Some(key_item(
                analysis,
                item(key.to_string(), CompletionItemKind::PROPERTY, Some(detail.join("  ")), rank),
                key,
                r,
            ))
        })
        .chain(tips)
        .collect()
}

/// Extensions of files, and the installation's tree their program searches.
type Exts = (Vec<String>, Option<&'static str>);

/// The files an argument of type `ty` names, by [`crate::analysis::roles::FileKind`].
fn file_exts(analysis: &Analysis, ty: crate::analysis::roles::Ty) -> Option<Exts> {
    use crate::analysis::roles::{FileKind, Ty};
    let Ty::File(kind) = ty else { return None };
    match kind {
        // The extensions graphics tries (`\Gin@extensions` at the end of the run) and those the run's own graphics had.
        FileKind::Graphic => {
            let occ =
                analysis.facts.occurrences.iter().filter(|o| o.kind == crate::engine::builtins::OccKind::Graphics);
            let tried = crate::linter::shared::macro_text(analysis, "Gin@extensions").unwrap_or_default();
            let tried = tried.split(',').filter_map(|e| Some(e.trim().strip_prefix('.')?.to_string()));
            let exts: BTreeSet<String> =
                occ.filter_map(|o| Some(o.key.rsplit_once('.')?.1.to_string())).chain(tried).collect();
            Some((exts.into_iter().collect(), None))
        }
        // `\input` adds `.tex` only to a name without an extension (tex.web § 537): any file goes.
        FileKind::Tex => Some((vec!["tex".into(), String::new()], None)),
        k => Some((vec![k.ext()?.to_string()], k.tree())),
    }
}

/// The extensions of the files loading `package` with option `key=v` read
/// by the name `v`, as [`file_exts`] gives them.
fn option_files(analysis: &Analysis, package: &str, key: &str) -> Option<Exts> {
    let loads = &analysis.facts.loads;
    let at = loads.iter().position(|l| l.name == package && l.kind == crate::engine::builtins::LoadKind::Package)?;
    let value = loads[at].options.iter().find_map(|o| o.split_once('=').filter(|(k, _)| k.trim() == key))?.1.trim();
    let nested = loads[at + 1..].iter().take_while(|l| l.depth > loads[at].depth);
    let exts: BTreeSet<String> = nested
        .filter_map(|l| l.name.rsplit_once('.').filter(|(stem, _)| *stem == value).map(|(_, ext)| ext.to_string()))
        .collect();
    let tree = exts.first().and_then(|e| crate::analysis::roles::FileKind::of_ext(e)).and_then(|k| k.tree());
    (!exts.is_empty()).then(|| (exts.into_iter().collect(), tree))
}

/// The files of each `(extensions, tree)` kind whose name starts with
/// `typed`: the project's first, then the installation's.
fn files(analysis: &Analysis, kinds: impl Iterator<Item = Exts>, typed: &str) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    for (exts, tree) in kinds {
        let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
        let mut names: Vec<(String, usize, Option<PathBuf>)> =
            project_files(analysis, &exts).into_iter().map(|n| (n, 0, None)).collect();
        let roots = &analysis.distribution.roots;
        for (ext, dir) in exts.iter().filter_map(|e| Some((e, tree?))) {
            let ext = format!(".{ext}");
            match dir {
                "tex" => names.extend(
                    crate::caches::distribution::files_with(roots, &ext)
                        .into_iter()
                        .filter_map(|(n, p)| Some((n.strip_suffix(&ext)?.to_string(), 1, Some(p)))),
                ),
                _ => names.extend(
                    crate::caches::distribution::names_with(roots, dir, &ext).into_iter().map(|n| (n, 1, None)),
                ),
            }
        }
        let listed: Vec<String> = exts.iter().filter(|e| !e.is_empty()).map(|e| format!(".{e}")).collect();
        let detail = |n: &str| match n.rsplit_once('.').filter(|(_, e)| !e.contains('/')) {
            Some((stem, ext)) => loader(analysis, ext, stem),
            None => Some(listed.join(", ")),
        };
        let names: Vec<_> = names.into_iter().filter(|(n, ..)| n.starts_with(typed)).collect();
        // Reading each file's identification is for a list one can read.
        let identify = names.len() <= analysis.settings.conventions.completion.identified;
        out.extend(names.into_iter().map(|(n, rank, path)| {
            let mut detail = detail(&n);
            let mut it = item(n.clone(), CompletionItemKind::FILE, None, rank);
            if let Some(path) = path {
                let (stem, ext) = (n.as_str(), path.extension().and_then(|e| e.to_str()).unwrap_or_default());
                let tree = roots.iter().find(|r| path.starts_with(r)).filter(|r| local(analysis, r));
                let tree = tree.and_then(|r| r.file_name()).map(|t| format!("local: {}", t.to_string_lossy()));
                let head = identify.then(|| head(&path)).flatten().unwrap_or_default();
                let needs = needs(&head);
                // A package this run's engine cannot load says so, and comes last.
                let other = (!needs.is_empty() && !needs.contains(&analysis.plugins.engine)).then(|| {
                    it.sort_text = Some(format!("{}~", it.sort_text.as_deref().unwrap_or_default()));
                    let names: Vec<&str> = needs
                        .iter()
                        .map(|e| if *e == crate::project::config::Engine::LuaTeX { "LuaTeX" } else { "XeTeX" })
                        .collect();
                    format!("needs {}", names.join(" or "))
                });
                let parts: Vec<String> = [detail.take(), provided(&head), tree, other].into_iter().flatten().collect();
                detail = Some(parts.join(" · "));
                if matches!(ext, "sty" | "cls") {
                    it.documentation = Some(lsp_types::Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("[documentation](https://texdoc.org/pkg/{stem})"),
                    }));
                }
            }
            it.detail = detail;
            it
        }));
    }
    out.sort_by(|a, b| (&a.label, &a.sort_text).cmp(&(&b.label, &b.sort_text)));
    out.dedup_by(|a, b| a.label == b.label);
    out
}

/// The installation's files a library argument names: the part of a file
/// name between the `prefix` and `suffix` its load puts around it (and the
/// extension `ext` a package's or class's load adds), starting with `typed`.
fn library_items(
    analysis: &Analysis,
    prefix: &str,
    suffix: &str,
    ext: Option<&str>,
    typed: &str,
) -> Vec<CompletionItem> {
    let ext = ext.unwrap_or_default();
    let all = crate::caches::distribution::files_with(&analysis.distribution.roots, "");
    let detail = format!("{prefix}…{suffix}{ext}");
    let names = all.iter().filter_map(|(name, _)| name.strip_prefix(prefix)?.strip_suffix(ext)?.strip_suffix(suffix));
    let names: BTreeSet<&str> = names.filter(|m| !m.is_empty() && m.starts_with(typed)).collect();
    names.into_iter().map(|m| item(m.to_string(), CompletionItemKind::MODULE, Some(detail.clone()), 0)).collect()
}

/// Whether `root` is a tree of the user's or the site's own rather than of
/// the distribution: TeX Live's `texmf-dist` and a configured bundle are not.
fn local(analysis: &Analysis, root: &std::path::Path) -> bool {
    root.file_name().is_none_or(|n| n != "texmf-dist") && analysis.distribution.bundle.as_deref() != Some(root)
}

/// What a file says it is (`\ProvidesPackage{x}[2024/01/01 v1.2 …]`, or
/// expl3's `\ProvidesExplPackage{x}{date}{version}{…}`): date and version.
fn provided(head: &str) -> Option<String> {
    let (at, expl) =
        ["\\ProvidesExpl", "\\Provides"].iter().find_map(|m| Some((head.find(m)?, m.ends_with("Expl"))))?;
    let rest = &head[at..];
    let after_name = rest.find('}')? + 1;
    let rest = &rest[after_name..];
    let text = if expl {
        let groups: Vec<&str> = rest.split('}').take(2).filter_map(|g| Some(g.split_once('{')?.1.trim())).collect();
        groups.join(" ")
    } else {
        let rest = rest.trim_start().strip_prefix('[')?;
        rest[..rest.find(']')?].split_whitespace().take(2).collect::<Vec<_>>().join(" ")
    };
    (!text.is_empty()).then_some(text)
}

/// A file's first lines, where it identifies itself.
fn head(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut head = Vec::new();
    std::fs::File::open(path).ok()?.take(16 * 1024).read_to_end(&mut head).ok()?;
    Some(String::from_utf8_lossy(&head).into_owned())
}

/// The engines a file says, in its code, it needs: `\RequireLuaTeX`, or a
/// message that it "requires either XeTeX or LuaTeX" (fontspec), "LuaTeX is
/// required" (luacode), "Use XeLaTeX or LuaLaTeX instead" (unicode-math); "Don't use XeTeX" is
/// no such message. A
/// convention read off its first lines; none found means any engine.
fn needs(head: &str) -> Vec<crate::project::config::Engine> {
    use crate::project::config::Engine;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        let engine = r"(Lua|Xe)(?:La)?TeX";
        Regex::new(&format!(
            r"(?i)\\Require(Lua|Xe)TeX\b|requires?[\s~]+(?:either[\s~]+)?{engine}(?:[\s~]+or[\s~]+{engine})?|use[\s~]+{engine}(?:[\s~]+or[\s~]+{engine})?[\s~]+instead|{engine}[\s~]+(?:is[\s~]+)?required"
        ))
        .unwrap()
    });
    let mut out = Vec::new();
    for line in head.lines().map(uncommented) {
        // `\def\RequireLuaTeX` (iftex) defines the test, it does not ask for it.
        let asked = |c: &regex::Captures| !line[..c.get(0).map_or(0, |m| m.start())].trim_end().ends_with("def");
        for name in re.captures_iter(line).filter(asked).flat_map(|c| c.iter().skip(1).flatten().collect::<Vec<_>>()) {
            let engine = if name.as_str().eq_ignore_ascii_case("lua") { Engine::LuaTeX } else { Engine::XeTeX };
            if !out.contains(&engine) {
                out.push(engine);
            }
        }
    }
    out
}

/// What a file of extension `ext` is, and the command this run loaded such
/// files with (`\usepackage{stem}`), if it loaded any.
fn loader(analysis: &Analysis, ext: &str, stem: &str) -> Option<String> {
    let kind = crate::analysis::roles::FileKind::of_ext(ext)?;
    let load = match kind {
        crate::analysis::roles::FileKind::Sty => crate::engine::builtins::LoadKind::Package,
        crate::analysis::roles::FileKind::Cls => crate::engine::builtins::LoadKind::Class,
        _ => return Some(kind.label().to_string()),
    };
    let by = analysis.facts.loads.iter().filter(|l| l.kind == load && l.depth == 0).find_map(|l| l.by);
    Some(match by {
        Some(cs) => format!("{}: \\{}{{{stem}}} loads it", kind.label(), analysis.interner.name(cs)),
        None => kind.label().to_string(),
    })
}

/// Files under the project root with one of `exts` (`""`: any other),
/// relative, without a listed extension, as `\input` and `\includegraphics`
/// take them.
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
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let listed = exts.contains(&ext);
                let shown = if listed { path.with_extension("") } else { path.clone() };
                if (listed || exts.contains(&""))
                    && let Ok(rel) = shown.strip_prefix(root)
                {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort();
    out
}

/// Environments open before `at` in the main file, outermost first.
fn open_environments(analysis: &Analysis, at: (u32, u32)) -> Vec<String> {
    use crate::engine::builtins::OccKind;
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

/// The names of a `\begin{…}` and its `\end{…}`, when `position` is in
/// one, to edit together (LSP linked editing). They pair by nesting, not
/// by name, so the link holds while a name is retyped. Read from the text
/// as it is now: a run would lag behind the typing.
pub fn linked_environment(text: &str, position: Position) -> Option<lsp_types::LinkedEditingRanges> {
    static ENV: OnceLock<Regex> = OnceLock::new();
    let env = ENV.get_or_init(|| Regex::new(r"\\(begin|end)\s*\{([^{}%]*)\}").unwrap());
    let mut open: Vec<Range> = Vec::new();
    for (line, body) in text.lines().enumerate() {
        let code = uncommented(body);
        for c in env.captures_iter(code) {
            let name = c.get(2)?;
            let at = |b: usize| Position { line: line as u32, character: code[..b].encode_utf16().count() as u32 };
            let range = Range { start: at(name.start()), end: at(name.end()) };
            if &c[1] == "begin" {
                open.push(range);
            } else if let Some(begin) = open.pop()
                && [begin, range].iter().any(|r| r.start <= position && position <= r.end)
            {
                return Some(lsp_types::LinkedEditingRanges { ranges: vec![begin, range], word_pattern: None });
            }
        }
    }
    None
}

/// `line` up to its first `%` that starts a comment (not `\%`).
pub(super) fn uncommented(line: &str) -> &str {
    let mut escaped = false;
    for (at, c) in line.char_indices() {
        match c {
            '%' if !escaped => return &line[..at],
            '\\' => escaped = !escaped,
            _ => escaped = false,
        }
    }
    line
}

/// Environments defined at `pos`: the run's `\X`/`\endX` pairs (what
/// `\begin` looks up), less the ones the document defines only later, each
/// with whether its code reaches what `\item` reads (a list).
pub(super) fn environments(analysis: &Analysis, pos: Pos) -> Vec<(String, bool)> {
    let later = |sym| {
        let defs: Vec<_> = analysis.facts.defs.iter().filter(|d| d.name == sym).collect();
        !defs.is_empty() && !defs.iter().any(|d| analysis.visible_in(d.span, analysis.main_file, pos.line, pos.col))
    };
    // A list environment is made with `\list`, whose items are `\item`s.
    let list = analysis.interner.lookup("list");
    let mut out = Vec::new();
    for i in 0..analysis.interner.len() as u32 {
        let end = crate::engine::tex::Sym(i);
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

/// The names the bodies of `name`'s code spell (or build with `\csname`), breadth-first through the
/// meanings the run ended with, up to `limit` macros.
pub(super) fn body_reach(analysis: &Analysis, name: &str, limit: usize) -> BTreeSet<crate::engine::tex::Sym> {
    let mut seen = BTreeSet::new();
    let mut queue: std::collections::VecDeque<_> = analysis.interner.lookup(name).into_iter().collect();
    let csname = analysis.interner.lookup("csname");
    let mut expanded = 0;
    while let Some(sym) = queue.pop_front() {
        let Some(mac) = analysis.env.meaning(sym).as_macro().cloned() else { continue };
        expanded += 1;
        if expanded > limit {
            break;
        }
        let toks = &mac.replacement_text;
        // a name built of characters, `\csname tabular*\endcsname`, is reached as well
        let built = |i: usize| {
            let name: String = (toks[i + 1..].iter())
                .map_while(|t| match t.tok {
                    crate::engine::tex::Tok::Chr(c, _) => Some(c),
                    _ => None,
                })
                .collect();
            (!name.is_empty()).then(|| analysis.interner.lookup(&name)).flatten()
        };
        for (i, cs) in toks.iter().enumerate().filter_map(|(i, t)| Some((i, t.cs()?))) {
            let named = (Some(cs) == csname).then(|| built(i)).flatten();
            for cs in std::iter::once(cs).chain(named) {
                if seen.insert(cs) {
                    queue.push_back(cs);
                }
            }
        }
    }
    seen
}

/// The kind of material an environment makes: the first kernel builder its code reaches, the most
/// specific first (an alignment preamble, a list, a trivlist, a float); `None`: no kind.
pub(super) fn material(analysis: &Analysis, name: &str) -> Option<usize> {
    const BUILDERS: [&[&str]; 4] = [&["@mkpream"], &["list"], &["@trivlist"], &["@float", "@dblfloat"]];
    let reach = body_reach(analysis, name, 400);
    BUILDERS
        .iter()
        .position(|names| names.iter().any(|n| analysis.interner.lookup(n).is_some_and(|s| reach.contains(&s))))
}

/// Builds what completion reads of `analysis` once, ahead of the first request.
pub fn warm(analysis: &Analysis) {
    takes(analysis);
    crate::util::progress::pause_now();
    crate::analysis::roles::roles(analysis, "documentclass");
    crate::util::progress::pause_now();
    query::scope_named(analysis, None, false, |_| false);
}

/// The fewest arguments each defined name takes, once per analysis.
fn takes(analysis: &Analysis) -> &crate::fixed::HashMap<String, u8> {
    analysis.takes.get_or_init(|| {
        let defs = analysis.facts.defs.iter();
        crate::util::progress::unpaused(|| {
            defs.filter_map(|d| Some((analysis.interner.cs(d.name), query::takes_min(analysis, d)?))).collect()
        })
    })
}

/// How many mandatory arguments `\begin{env}` takes.
pub(super) fn environment_arguments(analysis: &Analysis, env: &str) -> u8 {
    analysis
        .settings
        .conventions
        .completion
        .environment_arguments
        .get(env)
        .copied()
        .unwrap_or_else(|| takes(analysis).get(&format!("\\{env}")).copied().unwrap_or(0))
}

/// A name a file the build wrote defines (`\@abspage@last` from the
/// `.aux`): it holds from the second run on, which the label says.
fn written_by(analysis: &Analysis, record: &crate::queries::query::Record) -> Option<String> {
    let path = record.get("path").and_then(Json::as_str)?;
    let file = analysis.written.iter().find(|(name, _)| path.ends_with(name.as_str()))?;
    let name = std::path::Path::new(&file.0).file_name()?.to_string_lossy().into_owned();
    Some(format!("from {name}, from the second run on"))
}

/// Each carries a snippet for its mandatory arguments.
/// The commands in scope at `pos` whose name holds `typed` (case aside),
/// the best `conventions.completion.shown` of them, and whether there were more.
/// The project's own first, then documented names from packages and the
/// kernel (no `@`, `_` or `:`), more used ones first, internals last.
fn command_items(analysis: &Analysis, pos: Pos, typed: &str) -> (Vec<CompletionItem>, bool) {
    let takes = takes(analysis);
    let signature = signature_items(analysis, typed);
    let typed = typed.to_lowercase();
    let mut used: crate::fixed::HashMap<crate::engine::tex::Sym, u32> = crate::fixed::HashMap::default();
    let mut document: crate::fixed::HashMap<crate::engine::tex::FileId, bool> = crate::fixed::HashMap::default();
    for e in &analysis.facts.expansions {
        let own = *document.entry(e.span.file).or_insert_with(|| query::origin(analysis, e.span.file) == "document");
        if own && e.within.is_none() {
            *used.entry(e.name).or_default() += 1;
        }
    }
    // a name with `@` only where `@` is a letter (`\makeatletter`, a package)
    let letters = analysis.facts.letters_at(analysis.main_file, pos.line, pos.col);
    let tiers = &analysis.settings.conventions.completion.tiers;
    let typeable =
        |label: &str| label.chars().count() < 2 || label.chars().all(|c| c.is_alphabetic() || letters.contains(&c));
    let mut items: Vec<(CompletionItem, String)> = {
        let named = |name: &str| crate::engine::tex::bare_name(name).to_lowercase().contains(&typed);
        query::scope_named(analysis, Some((analysis.main_file, pos.line, pos.col)), true, named)
    }
    .into_iter()
    .filter_map(|record| {
        let name = record.get("name")?.as_str()?.to_string();
        let label = crate::engine::tex::bare_name(&name).to_string();
        if !label.to_lowercase().contains(&typed) || !typeable(&label) {
            return None;
        }
        let tag = record.get("tag").and_then(Json::as_str).unwrap_or("");
        let internal = analysis.settings.conventions.is_internal(crate::engine::tex::bare_name(&label));
        if label.is_empty() || (internal && !matches!(tag, "macro" | "primitive" | "environment" | "alias")) {
            return None;
        }
        // the usage line, its arguments named, else the bare shape
        let effective = record.get("usage").or_else(|| record.get("effective")).and_then(Json::as_str).unwrap_or("");
        let package = record.get("package").and_then(Json::as_str).unwrap_or("");
        let tier = match (package, internal) {
            ("document", _) => tiers.document,
            (_, true) => tiers.internal,
            _ if tag == "primitive" || !label.chars().any(char::is_alphabetic) => tiers.primitive,
            _ => tiers.package,
        };
        let count = analysis.interner.lookup(&label).and_then(|s| used.get(&s)).copied().unwrap_or(0);
        // The argument is a name to pick: the editor asks again inside
        // the braces, and an environment brings its `\end`.
        let pick = analysis.settings.conventions.completion.pick.contains(&label);
        let (insert_text, format) = match takes.get(&name).copied().unwrap_or(0) {
            _ if pick => (format!("{label}{{$1}}"), InsertTextFormat::SNIPPET),
            0 => (label.clone(), InsertTextFormat::PLAIN_TEXT),
            min => {
                let args: String = (1..=min).map(|i| format!("{{${i}}}")).collect();
                (format!("{label}{args}"), InsertTextFormat::SNIPPET)
            }
        };
        let it = CompletionItem {
            sort_text: Some(format!("{tier}{:06}{label}", 999_999u32.saturating_sub(count))),
            label: label.clone(),
            kind: Some(if tag == "primitive" { CompletionItemKind::KEYWORD } else { CompletionItemKind::FUNCTION }),
            detail: Some(format!("{effective}  ({})", written_by(analysis, &record).unwrap_or(package.to_string()))),
            insert_text: Some(insert_text),
            insert_text_format: Some(format),
            command: pick.then(|| lsp_types::Command {
                title: "pick".into(),
                command: "editor.action.triggerSuggest".into(),
                arguments: None,
            }),
            ..Default::default()
        };
        // the arguments a call reads: the effective usage after the name
        let args = effective.strip_prefix('\\').unwrap_or(effective).strip_prefix(label.as_str()).unwrap_or("");
        Some((it, args.to_string()))
    })
    .collect();
    // A name that starts with what is typed before one that only holds it.
    items.sort_by_cached_key(|(i, _)| (!i.label.to_lowercase().starts_with(&typed), i.sort_text.clone()));
    let shown = analysis.settings.conventions.completion.shown;
    let more = items.len() > shown;
    items.truncate(shown);
    let all = {
        let names: Vec<&str> = items.iter().map(|(i, _)| i.label.as_str()).collect();
        crate::analysis::roles::roles_many(analysis, &names)
    };
    let probes = analysis.settings.conventions.completion.signatures;
    for (i, ((item, effective), roles)) in items.iter_mut().zip(all).enumerate() {
        // a listing gives a definition's own text: what a call reads comes from the probe
        if i < probes
            && !effective.contains(['{', '['])
            && let Some(sym) = analysis.interner.lookup(&item.label)
        {
            let probed = query::effective_probed(analysis, sym);
            *effective = probed
                .strip_prefix('\\')
                .unwrap_or(&probed)
                .strip_prefix(item.label.as_str())
                .unwrap_or("")
                .to_string();
        }
        item.detail = item.detail.take().map(|d| typed_usage(&d, &roles));
        // the signature beside the label, a tab stop per argument named by its type
        if let Some((signature, snippet)) = signature_snippet(&item.label, effective, &roles) {
            item.label_details = Some(lsp_types::CompletionItemLabelDetails {
                detail: Some(signature),
                description: item
                    .detail
                    .as_deref()
                    .and_then(|d| d.rsplit_once("  ("))
                    .map(|(_, by)| by.trim_end_matches(')').to_string()),
            });
            if item.command.is_none() {
                item.insert_text = Some(snippet);
                item.insert_text_format = Some(InsertTextFormat::SNIPPET);
                // the first argument's help opens once the name is in
                item.command = Some(lsp_types::Command {
                    title: "signature".into(),
                    command: "editor.action.triggerParameterHints".into(),
                    arguments: None,
                });
            }
        }
    }
    let mut items: Vec<CompletionItem> = items.into_iter().map(|(i, _)| i).collect();
    items.extend(signature.into_iter().filter(|s| !items.iter().any(|i| i.label == s.label)).collect::<Vec<_>>());
    (items, more)
}

/// An expl3 name typed up to its signature (`tl_set:N`): each letter that may come next, with what
/// it says of the argument, as the `conventions.signatures` setting has them.
fn signature_items(analysis: &Analysis, typed: &str) -> Vec<CompletionItem> {
    let Some((_, sig)) = typed.split_once(':') else { return Vec::new() };
    if !sig.chars().all(|c| c.is_ascii_alphabetic()) {
        return Vec::new();
    }
    let signatures = &analysis.settings.conventions.signatures;
    signatures
        .iter()
        .enumerate()
        .map(|(i, (letter, about))| {
            let mut it =
                item(format!("{typed}{letter}"), CompletionItemKind::TYPE_PARAMETER, Some(about.clone()), 90_000 + i);
            it.filter_text = Some(format!("{typed}{letter}"));
            it
        })
        .collect()
}

/// A command's signature from its effective usage (`\\usepackage[1]{2}[3]`) and the argument
/// types its roles give: `[⟨options⟩]{⟨sty file⟩}[⟨date⟩]`, and the snippet with a tab stop per
/// argument, an optional one with its brackets so that one key removes it. `None` for a usage
/// with delimited arguments or none.
fn signature_snippet(label: &str, args: &str, roles: &[crate::analysis::roles::Role]) -> Option<(String, String)> {
    let types = crate::analysis::roles::arg_types(roles);
    let mut rest = args.trim_start_matches('*');
    let (mut signature, mut snippet, mut braced, mut stop) = (String::new(), label.to_string(), 0, 0);
    while let Some(open) = rest.chars().next() {
        let close = match open {
            '{' => '}',
            '[' => ']',
            _ => return None,
        };
        let end = rest.find(close)?;
        let name = match open {
            '{' => {
                let ty = types.iter().find(|(a, _)| *a == braced).map(|(_, t)| t.to_string());
                braced += 1;
                ty.unwrap_or_else(|| "arg".into())
            }
            _ => "options".into(),
        };
        let name = name.replace(['$', '}', '\\'], "");
        stop += 1;
        signature.push_str(&format!("{open}⟨{name}⟩{close}"));
        snippet.push_str(&match open {
            '{' => format!("{{${{{stop}:{name}}}}}"),
            _ => format!("${{{stop}:[{name}]}}"),
        });
        rest = &rest[end + 1..];
    }
    (stop > 0).then(|| (signature, snippet + "$0"))
}

/// A usage line with each braced argument's type after its number
/// (`{1}` is `{1: skip register}`), as far as the roles know it.
fn typed_usage(usage: &str, roles: &[crate::analysis::roles::Role]) -> String {
    let types = crate::analysis::roles::arg_types(roles);
    let mut out = String::new();
    let (mut rest, mut braced) = (usage, 0);
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}').map(|c| open + c) else { break };
        let inside = &rest[open + 1..close];
        out.push_str(&rest[..open + 1]);
        out.push_str(inside);
        if !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit()) {
            if let Some((_, ty)) = types.iter().find(|(a, _)| *a == braced) {
                out.push_str(&format!(": {ty}"));
            }
            braced += 1;
        }
        rest = &rest[close..];
    }
    out + rest
}

/// The paragraphs the document's own files set, in reading order: `satex
/// query typeset` of the run, without what libraries set. A heading (a
/// paragraph kept with what follows) carries the sectioning command of its
/// line as `heading`, else `heading`.
pub fn typeset(analysis: &Analysis) -> Vec<Json> {
    let at = |r: &query::Record| {
        (r.get("path").and_then(Json::as_str).map(str::to_owned), r.get("line").and_then(Json::as_u64))
    };
    let headings: std::collections::HashMap<_, _> = query::run(analysis, Query::Occurrences, &Filter::Always)
        .into_iter()
        .filter(|r| r.get("kind").and_then(Json::as_str) == Some("section"))
        .filter_map(|r| Some((at(&r), r.get("detail")?.clone())))
        .collect();
    query::run(analysis, Query::Typeset, &Filter::Always)
        .into_iter()
        .filter(|r| {
            r.get("origin").is_none_or(|o| o == "document")
                && r.get("kind").and_then(Json::as_str) != Some("assumption")
        })
        .map(|mut r| {
            if r.remove("kept").is_some() {
                r.insert("heading".into(), headings.get(&at(&r)).cloned().unwrap_or_else(|| Json::from("heading")));
            }
            Json::Object(r)
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
    // the facts of this file only: no record is built for the rest of the run
    let here = |f| analysis.file_name(f) == path;
    for occ in analysis.facts.occurrences.iter().filter(|o| here(o.span.file)) {
        let symbol_kind = match occ.kind.as_str() {
            "section" => SymbolKind::NAMESPACE,
            "label" => SymbolKind::CONSTANT,
            "begin-environment" => SymbolKind::MODULE,
            _ => continue,
        };
        if !occ.key.is_empty() {
            out.push(symbol(occ.key.clone(), symbol_kind, occ.span.line, occ.span.col));
        }
    }
    for def in analysis.facts.defs.iter().filter(|d| here(d.span.file)) {
        if query::origin(analysis, def.span.file) == "document" {
            out.push(symbol(analysis.interner.cs(def.name), SymbolKind::FUNCTION, def.span.line, def.span.col));
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `lint`'s findings in the document's own files. Precision notes are the
/// analysis's, not the document's: an editor hides them.
pub fn findings(analysis: &Analysis) -> Vec<Record> {
    crate::linter::lint_document(analysis)
        .into_iter()
        .filter(|r| r.get("origin").and_then(Json::as_str) == Some("document"))
        .filter(|r| r.get("category").and_then(Json::as_str) != Some("precision"))
        .collect()
}

/// `records` in `render::lsp`'s shape: `[{uri, diagnostics, codeActions}, …]`,
/// one entry per file they touch.
pub fn diagnostics_json(records: &[Record]) -> Vec<Json> {
    let text = crate::util::render::lsp_with(records, |path| path_uri(path).to_string());
    serde_json::from_str(&text).unwrap_or_default()
}

/// `textDocument/documentLink`: each name a load of this file resolved (`\usepackage{hyperref}`,
/// `\input{intro}`) links to the file the run read for it, its implementation.
pub fn document_links(analysis: &Analysis, doc: &Text, path: &str) -> Vec<lsp_types::DocumentLink> {
    let mut out: Vec<lsp_types::DocumentLink> = Vec::new();
    for load in analysis.facts.loads.iter().filter(|l| analysis.file_name(l.span.file) == path) {
        let Some(file) = load.path.as_deref() else { continue };
        let text = doc.line(load.span.line);
        let from = (load.span.col as usize).min(text.len());
        let name = load.name.as_str();
        let Some(at) = word_before(text, from, name).or_else(|| word_after(text, from, name)) else { continue };
        let line = load.span.line.saturating_sub(1);
        let range = Range::new(Position::new(line, at as u32), Position::new(line, (at + name.len()) as u32));
        if out.iter().any(|l| l.range == range) {
            continue;
        }
        out.push(lsp_types::DocumentLink {
            range,
            target: Some(path_uri(file)),
            tooltip: Some(format!("open {}", analysis.short_name(load.file.unwrap_or(analysis.main_file)))),
            data: None,
        });
    }
    out
}

/// Where `name` stands in `text` as a whole word: not inside a longer name or a key (`x` in `satex`).
fn words<'a>(text: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    let word = |c: char| c.is_alphanumeric() || matches!(c, '.' | '_' | '@' | '/');
    text.match_indices(name).map(|(i, _)| i).filter(move |&i| {
        !text[..i].chars().next_back().is_some_and(word) && !text[i + name.len()..].chars().next().is_some_and(word)
    })
}

/// Whether `name` at `at` stands as an argument's value (`{red}`, `[x=red]`,
/// `red!50`), not as prose: a package's own `\color{.}` names no full stop.
pub(super) fn argument_value(text: &str, at: usize, name: &str) -> bool {
    let before = text[..at].trim_end().chars().next_back();
    let after = text[at + name.len()..].trim_start().chars().next();
    matches!(before, Some('{' | '[' | '=' | ',' | '!')) && matches!(after, Some('}' | ']' | ',' | '!' | ';'))
}

pub(super) fn word_before(text: &str, from: usize, name: &str) -> Option<usize> {
    words(text, name).filter(|&i| i < from).last()
}

pub(super) fn word_after(text: &str, from: usize, name: &str) -> Option<usize> {
    words(text, name).find(|&i| i >= from)
}

/// A color xcolor holds exactly, as red, green and blue (xcolor § 6.3's
/// conversions from its core models).
pub(super) fn rgba(colors: &crate::engine::value::Colors) -> Option<lsp_types::Color> {
    use crate::engine::value::Model;
    let color = &colors.hull;
    let c: Vec<f32> = color.exact()?.iter().map(|&v| v as f32 / crate::engine::value::UNIT as f32).collect();
    let (r, g, b) = match (color.model.core(), &c[..]) {
        (Model::Rgb, [r, g, b]) => (*r, *g, *b),
        (Model::Cmy, [cy, m, y]) => (1.0 - cy, 1.0 - m, 1.0 - y),
        (Model::Cmyk, [cy, m, y, k]) => ((1.0 - cy) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)),
        (Model::Gray, [g]) => (*g, *g, *g),
        (Model::Hsb, [h, s, v]) => {
            let i = (h * 6.0).floor();
            let f = h * 6.0 - i;
            let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
            match i as i32 % 6 {
                0 => (*v, t, p),
                1 => (q, *v, p),
                2 => (p, *v, t),
                3 => (p, q, *v),
                4 => (t, p, *v),
                _ => (*v, p, q),
            }
        }
        _ => return None,
    };
    Some(lsp_types::Color { red: r, green: g, blue: b, alpha: 1.0 })
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
