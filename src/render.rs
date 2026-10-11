//! Terminal output: aligned tables, color, and OSC 8 hyperlinks.

use crate::fixed::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use anstyle::{AnsiColor, Color, Style};
use serde_json::Value as Json;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::machine::Step;
use crate::query::Record;

const BOLD: Style = Style::new().bold();
const DIM: Style = Style::new().dimmed();
const NAME: Style = fg(AnsiColor::Cyan);
const ERROR: Style = fg(AnsiColor::Red).bold();
const WARNING: Style = fg(AnsiColor::Yellow);
const INFO: Style = fg(AnsiColor::Blue);
const IMPRECISION: Style = fg(AnsiColor::Magenta);
const GOOD: Style = fg(AnsiColor::Green);
const CACHED: Style = Style::new().underline();

const fn fg(color: AnsiColor) -> Style {
    Style::new().fg_color(Some(Color::Ansi(color)))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Links(pub bool);

fn severity_style(value: &str) -> Style {
    match value {
        "error" => ERROR,
        "warning" => WARNING,
        "info" => INFO,
        "imprecision" => IMPRECISION,
        _ => Style::new(),
    }
}

/// The color of a trace step, in the terminal's own palette.
pub(crate) fn step_style(step: Step) -> Style {
    match step {
        Step::Expand => fg(AnsiColor::Cyan),
        Step::Execute => fg(AnsiColor::Blue),
        Step::Define | Step::Assign => fg(AnsiColor::Green),
        Step::OpenGroup | Step::CloseGroup => fg(AnsiColor::Magenta),
        Step::Condition | Step::Branch => fg(AnsiColor::Yellow),
        Step::OpenFile | Step::CloseFile => DIM,
        Step::Widen | Step::Undefined => fg(AnsiColor::Red),
    }
}

fn column_style(column: &str) -> Style {
    match column {
        "name" | "callee" | "key" | "subject" | "package" | "class" | "for" => NAME,
        "code" | "category" | "by" | "status" | "kind" | "tag" | "step" | "file" | "root" | "when" | "context" => DIM,
        _ => Style::new(),
    }
}

pub(crate) fn hyperlink(text: &str, target: &str, links: Links) -> String {
    if !links.0 || target.is_empty() {
        return text.to_string();
    }
    format!("\x1b]8;;{target}\x1b\\{text}\x1b]8;;\x1b\\")
}

pub fn plain(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        Json::Array(items) => items.iter().map(plain).collect::<Vec<_>>().join(", "),
        // `takes`: `{"min": N, "max": M}` prints as `N-M`, `N` when they
        // agree, `N+` when `max` is null (unbounded, e.g. `\halign`).
        Json::Object(map) if map.contains_key("min") && map.contains_key("max") => {
            let min = map.get("min").and_then(Json::as_u64).unwrap_or(0);
            match map.get("max").and_then(Json::as_u64) {
                Some(max) if max == min => min.to_string(),
                Some(max) => format!("{min}-{max}"),
                None => format!("{min}+"),
            }
        }
        other => other.to_string(),
    }
}

/// The page a record's `documentation` names, for the link on it.
fn reference(record: &Record) -> &str {
    record.get("reference").and_then(Json::as_str).unwrap_or_default()
}

/// The link target of a record's place: the file, as a `file://` URL. A
/// `file:` URL has no standard way to name a line, so the line stays in the
/// link's text instead of a fragment.
fn location(record: &Record) -> String {
    let Some(Json::String(path)) = record.get("path") else { return String::new() };
    file_url(path)
}

const COLUMNS: [&str; 43] = [
    "index",
    "warning",
    "covered",
    "rules",
    "id",
    "step",
    "severity",
    "category",
    "code",
    "kind",
    "status",
    "tag",
    "name",
    "callee",
    "key",
    "subject",
    "for",
    "when",
    "context",
    "redefined",
    "effective",
    "error",
    "signature",
    "parameters",
    "arity",
    "takes",
    "uses",
    "count",
    "value",
    "default",
    "governs",
    "package",
    "class",
    "by",
    "detail",
    "fix",
    "message",
    "documentation",
    "expands",
    "body",
    "reference",
    "root",
    "file",
];

fn cell(column: &str, record: &Record, links: Links) -> (String, String) {
    let Some(value) = record.get(column) else { return (String::new(), String::new()) };
    let text = plain(value);
    if text.is_empty() {
        return (text.clone(), text);
    }
    let style = match column {
        "severity" => severity_style(&text),
        "step" => Step::named(&text).map_or_else(Style::new, step_style),
        _ => column_style(column),
    };
    let shown = match column {
        "file" => hyperlink(&text, &location(record), links),
        // The documentation cell carries the link, so the reference it points
        // at does not need a column of its own.
        "documentation" => hyperlink(&text, reference(record), links),
        _ => text.clone(),
    };
    (text, format!("{style}{shown}{style:#}"))
}

const DEFAULT_WIDTH: usize = 120;
const MIN_COLUMN: usize = 8;
const GUTTER: usize = 2;

fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(terminal_size::Width(columns), _)| columns as usize)
        .or_else(|| std::env::var("COLUMNS").ok()?.parse().ok())
        .filter(|columns| *columns > MIN_COLUMN)
        .unwrap_or(DEFAULT_WIDTH)
}

const SHRINK_FIRST: [&str; 9] =
    ["body", "detail", "parameters", "signature", "effective", "documentation", "fix", "expands", "path"];

/// Free text gives way before the columns that identify a row.
fn shrink_rank(column: &str) -> usize {
    match column {
        "message" => SHRINK_FIRST.len(),
        _ => SHRINK_FIRST.iter().position(|c| *c == column).unwrap_or(SHRINK_FIRST.len() + 1),
    }
}

fn fit(columns: &[&str], widths: &mut [usize], budget: usize) {
    let separators = widths.len() * GUTTER;
    loop {
        let total: usize = widths.iter().sum::<usize>() + separators;
        if total <= budget {
            return;
        }
        let Some(victim) = widths
            .iter()
            .enumerate()
            .filter(|(_, width)| **width > MIN_COLUMN)
            .min_by_key(|(index, width)| (shrink_rank(columns[*index]), std::cmp::Reverse(**width)))
            .map(|(index, _)| index)
        else {
            return;
        };
        let excess = total - budget;
        let room = widths[victim] - MIN_COLUMN;
        widths[victim] -= excess.min(room);
    }
}

fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn elide(text: &str, limit: usize) -> String {
    if width(text) <= limit {
        return text.to_string();
    }
    let mut kept = String::new();
    let mut used = 0;
    for c in text.chars() {
        let next = UnicodeWidthChar::width(c).unwrap_or(0);
        if used + next > limit.saturating_sub(1) {
            break;
        }
        kept.push(c);
        used += next;
    }
    kept.push('…');
    kept
}

/// The columns these records carry, in the order the table shows them.
fn present_columns(records: &[Record]) -> Vec<&'static str> {
    COLUMNS
        .iter()
        .copied()
        .filter(|c| records.iter().any(|r| r.get(*c).is_some_and(|v| !plain(v).is_empty())))
        .collect()
}

/// The same, for a terminal that follows links: the reference then rides on
/// the `documentation` cell instead of taking a column.
fn shown_columns(records: &[Record], links: Links) -> Vec<&'static str> {
    let mut present = present_columns(records);
    if links.0 {
        present.retain(|column| *column != "reference");
    }
    present
}

/// RFC 4180: a field with a comma, a quote or a newline is quoted, and a
/// quote inside it is doubled.
pub fn csv(records: &[Record]) -> String {
    let present = present_columns(records);
    let escape = |text: &str| {
        if text.contains([',', '"', '\n']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text.to_string() }
    };
    let mut out = present.join(",");
    out.push('\n');
    for record in records {
        let row: Vec<String> = present.iter().map(|c| escape(&record.get(*c).map(plain).unwrap_or_default())).collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

/// A GitHub-flavoured Markdown table.
pub fn markdown(records: &[Record]) -> String {
    let present = present_columns(records);
    if present.is_empty() {
        return String::new();
    }
    let mut out = format!("| {} |\n", present.join(" | "));
    out.push_str(&format!("|{}\n", present.iter().map(|_| " --- |").collect::<String>()));
    for record in records {
        let row: Vec<String> =
            present.iter().map(|c| record.get(*c).map(plain).unwrap_or_default().replace('|', "\\|")).collect();
        out.push_str(&format!("| {} |\n", row.join(" | ")));
    }
    out
}

/// A record with the contexts it holds in, each with the error raised there.
type Grouped = (Record, Vec<(String, Option<String>)>);

/// Records that differ only in the context they hold in and the error they
/// raise there, as one record and its contexts.
fn by_context(records: &[Record]) -> Vec<Grouped> {
    let shared = |r: &Record| {
        let mut r = r.clone();
        let context = r.remove("context").map(|v| plain(&v));
        let error = r.remove("error").map(|v| plain(&v));
        (r, context, error)
    };
    let mut groups: Vec<Grouped> = Vec::new();
    for record in records {
        let (rest, context, error) = shared(record);
        match (groups.last_mut(), context) {
            (Some((first, contexts)), Some(context)) if !contexts.is_empty() && *first == rest => {
                contexts.push((context, error))
            }
            (_, Some(context)) => groups.push((rest, vec![(context, error)])),
            (_, None) => groups.push((record.clone(), Vec::new())),
        }
    }
    // A context of its own is shown as the record's fields.
    groups
        .into_iter()
        .map(|(mut record, contexts)| {
            if let [(context, error)] = contexts.as_slice() {
                record.insert("context".into(), Json::from(context.clone()));
                if let Some(error) = error {
                    record.insert("error".into(), Json::from(error.clone()));
                }
                return (record, Vec::new());
            }
            if !contexts.is_empty() {
                record.insert("context".into(), Json::from(""));
            }
            (record, contexts)
        })
        .collect()
}

/// One block per record instead of one row, with nothing cut to fit: the
/// detail views, whose fields are too wide for a table.
pub fn fields(records: &[Record], links: Links) -> String {
    let mut out = String::new();
    for (index, (record, contexts)) in by_context(records).iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let name = record.get("name").map(plain).unwrap_or_default();
        let _ = writeln!(out, "{NAME}{name}{NAME:#} {}", position(record));
        let present: Vec<&str> = COLUMNS
            .iter()
            .copied()
            .filter(|c| *c != "name")
            // `takes` carries it, since the two numbers only mean something
            // side by side.
            .filter(|c| *c != "arity")
            .filter(|c| !(links.0 && *c == "reference"))
            .filter(|c| {
                record.get(*c).is_some_and(|v| !plain(v).is_empty()) || (*c == "context" && !contexts.is_empty())
            })
            .collect();
        let label = present.iter().map(|c| width(c)).max().unwrap_or(0);
        let indent = 2 + label + 2;
        let room = terminal_width().saturating_sub(indent).max(MIN_COLUMN);
        for column in present {
            if column == "context" && !contexts.is_empty() {
                // Each context, and under it the first line of what it raises.
                for (i, (context, error)) in contexts.iter().enumerate() {
                    let head = if i == 0 { column } else { "" };
                    for (line, part) in wrap(context, room).into_iter().enumerate() {
                        let head = if line == 0 { head } else { "" };
                        let _ = writeln!(out, "  {DIM}{head:label$}{DIM:#}  {part}");
                    }
                    let error = error.as_deref().and_then(|e| e.lines().next()).unwrap_or("no error");
                    let style = if error == "no error" { DIM } else { column_style("error") };
                    for part in wrap(error, room.saturating_sub(2).max(MIN_COLUMN)) {
                        let _ = writeln!(out, "{:indent$}  {style}{part}{style:#}", "");
                    }
                }
                continue;
            }
            let text = match column {
                "file" => path_of(record),
                "uses" => match record.get(column).and_then(Json::as_u64) {
                    Some(1) => "1 time, in this run".to_string(),
                    Some(n) => format!("{n} times, in this run"),
                    None => plain(&record[column]),
                },
                _ => record.get(column).map(plain).unwrap_or_default(),
            };
            let style = match column {
                "severity" => severity_style(&text),
                "step" => Step::named(&text).map_or_else(Style::new, step_style),
                _ => column_style(column),
            };
            for (line, part) in wrap(&text, room).into_iter().enumerate() {
                let shown = match (column, line) {
                    ("file", 0) => hyperlink(&part, &location(record), links),
                    ("documentation", 0) => hyperlink(&part, reference(record), links),
                    _ => part,
                };
                match line {
                    0 => {
                        let _ = writeln!(out, "  {DIM}{column:label$}{DIM:#}  {style}{shown}{style:#}");
                    }
                    _ => {
                        let _ = writeln!(out, "{:indent$}{style}{shown}{style:#}", "");
                    }
                }
            }
        }
    }
    out
}

/// The full path a record names, with its line, falling back to the short
/// name for a file that is not on this machine.
fn path_of(record: &Record) -> String {
    let path = match record.get("path").and_then(Json::as_str) {
        Some(path) => std::fs::canonicalize(path).map(|p| p.display().to_string()).unwrap_or_else(|_| path.to_string()),
        None => record.get("file").map(plain).unwrap_or_default(),
    };
    match record.get("line").and_then(Json::as_u64).filter(|line| *line > 0) {
        Some(line) => format!("{path}:{line}"),
        None => path,
    }
}

/// Break text into lines of at most `limit` columns, at a space where there
/// is one and inside a word where there is not.
fn wrap(text: &str, limit: usize) -> Vec<String> {
    // A column narrower than one character still has to make progress, so the
    // narrowest line holds one character.
    let limit = limit.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let start = lines.len();
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let mut word = word;
            while width(word) > limit {
                let used = width(&line) + usize::from(!line.is_empty());
                match split_at_width(word, limit.saturating_sub(used)) {
                    ("", _) if !line.is_empty() => lines.push(std::mem::take(&mut line)),
                    // The character itself is wider than the column: it goes
                    // on a line of its own rather than nowhere.
                    ("", _) => {
                        let mut rest = word.chars();
                        let first = rest.next().expect("a word wider than the column");
                        lines.push(first.to_string());
                        word = rest.as_str();
                    }
                    (head, tail) => {
                        if !line.is_empty() {
                            line.push(' ');
                        }
                        line.push_str(head);
                        lines.push(std::mem::take(&mut line));
                        word = tail;
                    }
                }
            }
            if !line.is_empty() && width(&line) + 1 + width(word) > limit {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() || lines.len() == start {
            lines.push(line);
        }
    }
    lines
}

fn split_at_width(text: &str, limit: usize) -> (&str, &str) {
    let mut used = 0;
    for (index, c) in text.char_indices() {
        let next = UnicodeWidthChar::width(c).unwrap_or(0);
        if used + next > limit {
            return text.split_at(index);
        }
        used += next;
    }
    (text, "")
}

/// Columns that repeat another or say least, in the order a narrow terminal
/// gives them up.
const DROP_FIRST: [&str; 10] =
    ["subject", "arity", "takes", "package", "class", "by", "redefined", "context", "uses", "reference"];

/// The width free text is squeezed to before a column is dropped instead.
const READABLE: usize = 24;

/// Drops [`DROP_FIRST`] columns, one at a time, until the rest fit with the
/// free-text columns cut to [`READABLE`].
fn drop_for_width(records: &[Record], present: &mut Vec<&'static str>, budget: usize) {
    let natural = |column: &str| {
        let cells = records.iter().filter_map(|r| r.get(column)).map(|v| width(&plain(v)));
        let widest = cells.max().unwrap_or(0).max(width(column));
        if SHRINK_FIRST.contains(&column) || column == "message" { widest.min(READABLE) } else { widest }
    };
    for column in DROP_FIRST {
        let total: usize = present.iter().map(|c| natural(c) + GUTTER).sum();
        if total <= budget {
            return;
        }
        present.retain(|c| *c != column);
    }
}

pub fn table(records: &[Record], links: Links) -> String {
    if records.is_empty() {
        return String::new();
    }
    let mut present = shown_columns(records, links);
    let position_width = records.iter().map(|record| width(&position(record))).max().unwrap_or(0);
    drop_for_width(records, &mut present, terminal_width().saturating_sub(position_width));
    let rows: Vec<Vec<(String, String)>> =
        records.iter().map(|r| present.iter().map(|c| cell(c, r, links)).collect()).collect();

    let mut widths: Vec<usize> = present.iter().map(|c| width(c)).collect();
    for row in &rows {
        for (slot, (text, _)) in widths.iter_mut().zip(row) {
            *slot = (*slot).max(width(text));
        }
    }
    let suffix = records.iter().map(|record| width(&position(record))).max().unwrap_or(0);
    fit(&present, &mut widths, terminal_width().saturating_sub(suffix));

    let mut out = String::new();
    for (column, slot) in present.iter().zip(&widths) {
        let shown = elide(column, *slot);
        let pad = slot - width(&shown);
        let _ = write!(out, "{BOLD}{shown}{BOLD:#}{:pad$}  ", "");
    }
    out.push('\n');
    for (row, record) in rows.iter().zip(records) {
        for ((text, styled), slot) in row.iter().zip(&widths) {
            let shown = if width(text) > *slot {
                let cut = elide(text, *slot);
                styled.replacen(text.as_str(), &cut, 1)
            } else {
                styled.clone()
            };
            let pad = slot - width(text).min(*slot);
            let _ = write!(out, "{shown}{:pad$}  ", "");
        }
        out.push_str(&position(record));
        out.push('\n');
    }
    out
}

/// `file:line:col`, or just the file for a record with no place (a widened
/// recursion, say, which has none).
fn place_text(record: &Record) -> String {
    let file = record.get("file").map(plain).unwrap_or_default();
    match record.get("line").and_then(Json::as_u64).filter(|line| *line > 0) {
        Some(line) => format!("{file}:{line}:{}", record.get("col").and_then(Json::as_u64).unwrap_or(0)),
        None => file,
    }
}

fn place_key(record: &Record) -> (String, u64, u64) {
    (
        record.get("file").map(plain).unwrap_or_default(),
        record.get("line").and_then(Json::as_u64).unwrap_or(0),
        record.get("col").and_then(Json::as_u64).unwrap_or(0),
    )
}

fn plural(count: usize, word: &str) -> String {
    format!("{count} {word}{}", if count == 1 { "" } else { "s" })
}

/// One finding, never cut: `place  severity  code  message`, wrapped at the
/// terminal width rather than elided, with `fix: ...` indented below it when
/// the rule suggests one.
fn finding(out: &mut String, record: &Record, links: Links) {
    let severity_text = record.get("severity").map(plain).unwrap_or_default();
    let code = record.get("code").map(plain).unwrap_or_default();
    let message = record.get("message").map(plain).unwrap_or_default();
    let place = place_text(record);
    let head = format!("  {place}  {severity_text:<7}  {code}  ");
    let indent = width(&head);
    let room = terminal_width().saturating_sub(indent).max(MIN_COLUMN);
    let sev_style = severity_style(&severity_text);
    let shown_place = hyperlink(&place, &location(record), links);
    let mut lines = wrap(&message, room).into_iter();
    let _ = writeln!(
        out,
        "  {DIM}{shown_place}{DIM:#}  {sev_style}{severity_text:<7}{sev_style:#}  {DIM}{code}{DIM:#}  {}",
        lines.next().unwrap_or_default()
    );
    for line in lines {
        let _ = writeln!(out, "{:indent$}{line}", "");
    }
    let Some(fix) = record.get("fix").and_then(Json::as_str).filter(|f| !f.is_empty()) else { return };
    // `satex lint --fix` applies a `[fix]`, `--unsafe-fixes` an `[unsafe fix]`.
    let (tag, style) = match record.get("applicability").and_then(Json::as_str) {
        Some("safe") => ("[fix]", GOOD),
        Some("unsafe") => ("[unsafe fix]", WARNING),
        _ => ("fix:", DIM),
    };
    let indent = 5 + width(tag);
    let fix_room = terminal_width().saturating_sub(indent).max(MIN_COLUMN);
    let mut lines = wrap(fix, fix_room).into_iter();
    let _ = writeln!(out, "    {style}{tag}{style:#} {}", lines.next().unwrap_or_default());
    for line in lines {
        let _ = writeln!(out, "{:indent$}{line}", "");
    }
}

/// How many findings one rule shows before the rest become a count.  A rule
/// that fires on every quantity in a document says the same thing each time,
/// and the places are what `--filter` is for.
const SHOWN_PER_RULE: usize = 3;

fn findings_section(out: &mut String, title: &str, records: &[&Record], links: Links, group: bool) {
    if records.is_empty() {
        return;
    }
    let code = |r: &&Record| r.get("code").map(plain).unwrap_or_default();
    let mut total: HashMap<String, usize> = HashMap::default();
    for record in records {
        *total.entry(code(record)).or_default() += 1;
    }
    let _ = writeln!(out, "{}", heading(&format!("{title} ({})", records.len())));
    let mut shown: HashMap<String, usize> = HashMap::default();
    for record in records {
        let this = code(record);
        let seen = shown.entry(this.clone()).or_default();
        *seen += 1;
        let count = total[&this];
        if !group || count <= SHOWN_PER_RULE || *seen <= SHOWN_PER_RULE {
            finding(out, record, links);
        } else if group && *seen == SHOWN_PER_RULE + 1 {
            let rest = count - SHOWN_PER_RULE;
            let _ = writeln!(out, "  {}", label(&format!("{rest} more {this}, shown by --no-group")));
        }
    }
    out.push('\n');
}

/// What the analysis could not fully resolve, collapsed to a count and the
/// distinct causes rather than one line per site. It is a widened recursion
/// or a reached budget, never a defect in the document.
fn precision_line(records: &[&Record]) -> String {
    let mut causes: Vec<String> = records.iter().map(|r| r.get("name").map(plain).unwrap_or_default()).collect();
    causes.sort();
    causes.dedup();
    format!(
        "{} {}",
        heading("precision"),
        label(&format!(
            "{} where the analysis had to widen ({}) \u{2014} `satex lint --filter category=precision` shows them, `satex query gaps` lists what it could not parse at all",
            plural(records.len(), "note"),
            causes.join(", ")
        ))
    )
}

/// `satex lint`'s text output: findings grouped by severity, with build-cost
/// observations and unresolved cases kept apart rather than mixed into one
/// flat, truncated table.
pub fn lint(records: &[Record], links: Links, group: bool) -> String {
    let mut document: Vec<&Record> = Vec::new();
    let mut performance: Vec<&Record> = Vec::new();
    let mut precision: Vec<&Record> = Vec::new();
    for record in records {
        match record.get("category").map(plain).as_deref() {
            Some("performance") => performance.push(record),
            Some("precision") => precision.push(record),
            _ => document.push(record),
        }
    }
    // Most important first: by category (correctness before style), then
    // grouped by rule, each rule's findings in source order.
    let rank = |r: &Record| match r.get("category").map(plain).as_deref() {
        Some("correctness") => 0,
        Some("security") => 1,
        _ => 2,
    };
    document.sort_by_key(|r| (rank(r), r.get("code").map(plain).unwrap_or_default(), place_key(r)));
    performance.sort_by_key(|r| place_key(r));

    let severity = |r: &&Record| r.get("severity").map(plain).unwrap_or_default();
    let errors: Vec<&Record> = document.iter().copied().filter(|r| severity(r) == "error").collect();
    let warnings: Vec<&Record> = document.iter().copied().filter(|r| severity(r) == "warning").collect();
    let suggestions: Vec<&Record> = document.iter().copied().filter(|r| severity(r) == "info").collect();

    let mut out = String::new();
    findings_section(&mut out, "errors", &errors, links, group);
    findings_section(&mut out, "warnings", &warnings, links, group);
    findings_section(&mut out, "suggestions", &suggestions, links, group);
    findings_section(&mut out, "performance", &performance, links, group);
    if !precision.is_empty() {
        let _ = writeln!(out, "{}", precision_line(&precision));
        out.push('\n');
    }
    let _ = writeln!(
        out,
        "{}",
        label(&format!(
            "{}, {}, {}, {}, {}",
            plural(errors.len(), "error"),
            plural(warnings.len(), "warning"),
            plural(suggestions.len(), "suggestion"),
            plural(performance.len(), "performance note"),
            plural(precision.len(), "precision note"),
        ))
    );
    let fixable = |applicability: &str| {
        records.iter().filter(|r| r.get("applicability").and_then(Json::as_str) == Some(applicability)).count()
    };
    let (safe, unsafe_) = (fixable("safe"), fixable("unsafe"));
    if safe + unsafe_ > 0 {
        let hint = match (safe, unsafe_) {
            (0, _) => format!("{unsafe_} fixable with `satex lint --fix --unsafe-fixes`"),
            (_, 0) => format!("{safe} fixable with `satex lint --fix`"),
            _ => format!("{safe} fixable with `satex lint --fix`, {unsafe_} more with `--unsafe-fixes`"),
        };
        let _ = writeln!(out, "{}", label(&hint));
    }
    out
}

fn position(record: &Record) -> String {
    let (Some(line), Some(col)) = (record.get("line"), record.get("col")) else {
        return String::new();
    };
    if line.is_null() {
        return String::new();
    }
    match record.get("file").map(plain).filter(|f| !f.is_empty()) {
        Some(file) => format!("{DIM}@ {file}:{line}:{col}{DIM:#}"),
        None => format!("{DIM}@ {line}:{col}{DIM:#}"),
    }
}

pub fn summary(analysis: &crate::machine::Analysis, root: &crate::query::Node, detail: Detail, links: Links) -> String {
    let identity = crate::query::identity(analysis);
    let mut out = String::new();
    let _ = writeln!(out, "{}", heading(&root.name));
    let class = match (&identity.class, identity.class_options.as_slice()) {
        (Some(class), []) => format!(", class {}", subject(class)),
        (Some(class), options) => {
            format!(", class {}{}", subject(class), label(&format!("[{}]", options.join(","))))
        }
        (None, _) => String::new(),
    };
    let _ = writeln!(out, "  {} {}{class}", label("identified"), identity.kind);
    for entry in &analysis.metadata {
        let _ = writeln!(out, "  {} {}", label(entry.field), entry.text);
    }
    let _ =
        writeln!(out, "  {} {} {}", label("engine"), identity.engine, label(&format!("({})", identity.engine_source)));
    for (kind, value, source) in analysis.plugins.rows() {
        if kind == crate::plugin::Kind::Engine {
            continue;
        }
        let room = terminal_width().saturating_sub(kind.as_str().len() + source.len() + 8);
        let _ = writeln!(out, "  {} {} {}", label(kind.as_str()), elide(&value, room), label(&format!("({source})")));
    }
    let _ = writeln!(out, "  {} {}", label("installation"), analysis.distribution.describe());
    if let Some(format) = &analysis.format {
        let _ = writeln!(
            out,
            "  {} {} definitions from {}{}",
            label("format"),
            format.definitions,
            format.source,
            if format.cached { format!(" {CACHED}(cached){CACHED:#}") } else { String::new() }
        );
    }
    let _ = writeln!(
        out,
        "  {} {}",
        label("config"),
        analysis
            .config
            .as_ref()
            .map_or_else(|| "built-in defaults".to_string(), |path| linked_file(&path.display().to_string(), links))
    );
    let caches: Vec<String> = [
        analysis.format.as_ref().and_then(|f| f.cache.clone()),
        analysis.distribution.index_cache.as_ref().map(|p| p.display().to_string()),
    ]
    .into_iter()
    .flatten()
    .map(|path| linked_file(&path, links))
    .collect();
    if !caches.is_empty() {
        let _ = writeln!(out, "  {} {}", label("caches"), caches.join(", "));
    }
    if !analysis.package_caches.is_empty() {
        // What became of each package in the end, grouped by that: a cache
        // found stale and stored again counts as stored.
        let mut last: Vec<(&str, &str)> = Vec::new();
        for (name, _, state) in &analysis.package_caches {
            match last.iter_mut().find(|(n, _)| n == name) {
                Some(entry) => entry.1 = state,
                None => last.push((name, state)),
            }
        }
        let group = |test: &dyn Fn(&str) -> bool, show: &dyn Fn(&str, &str) -> String| -> Vec<String> {
            last.iter().filter(|(_, s)| test(s)).map(|(n, s)| show(n, s)).collect()
        };
        let parts: Vec<String> = [
            ("cached", group(&|s| s == "cached", &|n, _| n.to_string())),
            (
                "partly cached",
                group(&|s| s.contains("% cached"), &|n, s| format!("{n} {}", s.split(" cached").next().unwrap_or(s))),
            ),
            ("stored", group(&|s| s == "stored", &|n, _| n.to_string())),
            ("stale", group(&|s| s.starts_with("stale"), &|n, s| format!("{n} ({})", s.trim_start_matches("stale: ")))),
            (
                "not stored",
                group(&|s| s.starts_with("not stored"), &|n, s| {
                    format!("{n} ({})", s.trim_start_matches("not stored: "))
                }),
            ),
        ]
        .into_iter()
        .filter(|(_, names)| !names.is_empty())
        .map(|(what, names)| format!("{what}: {}", names.join(", ")))
        .collect();
        let _ = writeln!(out, "  {} {}", label("package caches"), parts.join("; "));
    }
    let _ = writeln!(out, "  {} {} files, {} tokens", label("read"), analysis.files.len(), analysis.steps);
    if let Some(defines) = defines_line(root) {
        let _ = writeln!(out, "  {} {defines}", label("defines"));
    }
    if let Some(uses) = environment_uses_line(analysis, root) {
        let _ = writeln!(out, "  {} {uses}", label("environments used"));
    }
    contents(&mut out, root, "  ", detail);
    for (i, child) in root.children.iter().enumerate() {
        node(&mut out, child, "", i + 1 == root.children.len(), 1, detail);
    }
    if detail.depth < usize::MAX {
        let _ = writeln!(out, "{}", label("  `--all` shows every file and what it defines"));
    }
    out
}

#[derive(Clone, Copy)]
pub struct Detail {
    pub depth: usize,
    pub contents: bool,
}

impl Detail {
    pub fn brief() -> Detail {
        Detail { depth: 1, contents: false }
    }
    pub fn full() -> Detail {
        Detail { depth: usize::MAX, contents: true }
    }
}

fn node(out: &mut String, node: &crate::query::Node, prefix: &str, last: bool, depth: usize, detail: Detail) {
    let branch = if last { "└─ " } else { "├─ " };
    let hidden = node.children.len();
    let options = if node.options.is_empty() { String::new() } else { format!("[{}]", node.options.join(",")) };
    let status = if node.status == "read" { String::new() } else { format!(" {}", label(node.status)) };
    let _ = writeln!(
        out,
        "{prefix}{branch}{}{}{status}{}",
        subject(&node.name),
        label(&options),
        node.identification
            .as_ref()
            .filter(|text| !text.is_empty())
            .map(|text| format!("  {}", label(text)))
            .unwrap_or_default()
    );
    let inner = format!("{prefix}{}", if last { "   " } else { "│  " });
    if depth >= detail.depth {
        if hidden > 0 {
            let _ = writeln!(out, "{inner}{}", label(&format!("{hidden} more")));
        }
        return;
    }
    contents(out, node, &inner, detail);
    for (i, child) in node.children.iter().enumerate() {
        self::node(out, child, &inner, i + 1 == node.children.len(), depth + 1, detail);
    }
}

/// The word a definition tag reads as in a sentence: LaTeX calls a macro a
/// "command"; every other tag (`environment`, `switch`, `counter`,
/// `length`, `option`, …) already reads as one, whatever the facts hand it.
fn concept(tag: &str) -> &str {
    if tag == "macro" { "command" } else { tag }
}

/// "3 commands, 2 environments, 1 switch": what the document's own file
/// defines, one clause per tag the facts recorded. Generic, so a tag with
/// no word of its own still reads, pluralized plainly.
fn defines_line(root: &crate::query::Node) -> Option<String> {
    let parts: Vec<String> = root
        .provides
        .iter()
        .filter(|(_, names)| !names.is_empty())
        .map(|(tag, names)| {
            let count = names.len();
            let word = concept(tag);
            // Usage tags are recorded already plural.
            if matches!(word, "labels" | "references" | "citations" | "environments used") {
                return format!("{count} {word}");
            }
            // `plural`'s plain "+s" reads wrong for the two tags that need
            // more than that.
            let irregular = match word {
                "alias" => Some("aliases"),
                "switch" => Some("switches"),
                _ => None,
            };
            match irregular {
                Some(many) if count != 1 => format!("{count} {many}"),
                _ => plural(count, word),
            }
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// How often the document's own environments (`root`'s `environment`
/// provides) were actually opened in the main file, from the run's own
/// `\begin{name}` occurrences, not a count of the definitions themselves.
fn environment_uses_line(analysis: &crate::machine::Analysis, root: &crate::query::Node) -> Option<String> {
    let owned = root.provides.get("environment")?;
    if owned.is_empty() {
        return None;
    }
    let mut counts: Vec<(&str, usize)> = owned
        .iter()
        .map(|name| {
            let count = analysis
                .facts
                .occurrences
                .iter()
                .filter(|o| {
                    o.kind == crate::builtins::OccKind::BeginEnvironment
                        && o.key == *name
                        && o.span.file == analysis.main_file
                })
                .count();
            (name.as_str(), count)
        })
        .filter(|(_, count)| *count > 0)
        .collect();
    if counts.is_empty() {
        return None;
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    Some(counts.iter().map(|(name, count)| format!("{name} ({count}×)")).collect::<Vec<_>>().join(", "))
}

const SHOWN: usize = 8;

fn contents(out: &mut String, node: &crate::query::Node, prefix: &str, detail: Detail) {
    if !detail.contents {
        return;
    }
    for (tag, names) in &node.provides {
        if names.is_empty() {
            continue;
        }
        let shown: Vec<&str> = names.iter().take(SHOWN).map(String::as_str).collect();
        let rest = names.len().saturating_sub(shown.len());
        let more = if rest > 0 { format!(" +{rest}") } else { String::new() };
        let _ =
            writeln!(out, "{prefix}{} {}{}", label(&format!("{tag}({})", names.len())), shown.join(" "), label(&more));
    }
    if !node.catcodes.is_empty() {
        let _ = writeln!(out, "{prefix}{} {}", label("catcodes"), node.catcodes.join(" "));
    }
}

const TIME_WIDTH: usize = 8 + 3;
const TOKENS_WIDTH: usize = 9 + 7;

pub fn timings(analysis: &crate::machine::Analysis, links: Links) -> String {
    let mut out = String::from("\n");
    let _ = writeln!(out, "{}", heading("phase"));
    for (phase, elapsed) in analysis.timings.phases() {
        let note = match phase {
            crate::timing::Phase::Discovery => {
                let index = &analysis.distribution;
                let source = marker(index.index_cached, "cache hit", "indexed");
                detail(&format!(
                    "{source}: {} files in {} trees via {}",
                    index.indexed,
                    index.roots.len(),
                    index.discovery.as_str()
                ))
            }
            crate::timing::Phase::Format => match &analysis.format {
                Some(format) => {
                    let source = marker(format.cached, "cache hit", "interpreted");
                    detail(&format!("{source}: {}", format.source))
                }
                None => detail("no format"),
            },
            crate::timing::Phase::Document => detail(&format!("{} files read", analysis.files.len())),
            crate::timing::Phase::Lint => detail(&format!("{} rules", crate::lint::rules().len())),
            crate::timing::Phase::Hooks | crate::timing::Phase::Fix => String::new(),
        };
        let _ = writeln!(out, "  {:<10} {:>8.1} ms{note}", phase.as_str(), elapsed.as_secs_f64() * 1000.0);
    }

    let root = crate::query::summary(analysis);
    let mut rows: Vec<(String, &crate::query::Node)> = Vec::new();
    flatten(&root, String::new(), true, true, &mut rows);
    if rows.is_empty() {
        return out;
    }
    // `flatten` folds repeat reads of a file, even under a different parent,
    // into one line. A frame this run opened counts at least the token that
    // finds its end, so zero tokens on `read` means no frame.
    let mut again: Vec<&str> = Vec::new();
    rows.retain(|(_, node)| {
        let repeat = node.tokens == 0 && matches!(node.status, "already-loaded" | "read");
        if repeat {
            again.push(node.name.as_str());
        }
        !repeat
    });
    let name_width = rows.iter().map(|(name, _)| width(name)).max().unwrap_or(0);
    // Indent, name, time, tokens and the gutters between them, as the row is printed.
    let fixed = 2 + name_width + 1 + TIME_WIDTH + 2 + TOKENS_WIDTH + 2;
    let path_room = terminal_width().saturating_sub(fixed);

    let _ = writeln!(out, "{}", heading("file"));
    for (name, node) in &rows {
        let pad = name_width - width(name);
        let path_width = if node.path.is_empty() {
            0
        } else if node.tokens == 0 {
            path_room.saturating_sub(width(why_empty(node)) + 3)
        } else if node.cached {
            path_room.saturating_sub(" (from its cache)".len())
        } else {
            path_room
        }
        .max(MIN_COLUMN);
        let path = if node.path.is_empty() {
            let reason = why_empty(node);
            if reason.is_empty() { note(node) } else { reason.to_string() }
        } else if node.tokens == 0 {
            format!("{} ({})", tail(&node.path, path_width), why_empty(node))
        } else if node.cached {
            format!("{} ({CACHED}from its cache{CACHED:#})", tail(&node.path, path_width))
        } else {
            tail(&node.path, path_width)
        };
        let shown =
            if node.path.is_empty() { label(&path) } else { label(&hyperlink(&path, &file_url(&node.path), links)) };
        let _ = writeln!(out, "  {name}{:pad$} {:>8.1} ms  {:>9} tokens  {shown}", "", node.millis, node.tokens);
    }
    if !again.is_empty() {
        again.sort_unstable();
        again.dedup();
        let _ = writeln!(out, "  {}", label(&format!("read again, no further cost: {}", again.join(", "))));
    }
    out
}

fn detail(text: &str) -> String {
    format!("  {}", label(text))
}

fn marker(cached: bool, hit: &str, miss: &'static str) -> String {
    if cached { format!("{CACHED}{hit}{CACHED:#}{DIM}") } else { miss.to_string() }
}

fn flatten<'a>(
    node: &'a crate::query::Node,
    prefix: String,
    root: bool,
    last: bool,
    rows: &mut Vec<(String, &'a crate::query::Node)>,
) {
    let name = if root {
        node.name.clone()
    } else {
        format!("{prefix}{}{}", if last { "└─ " } else { "├─ " }, node.name)
    };
    rows.push((name, node));
    let inner = if root { String::new() } else { format!("{prefix}{}", if last { "   " } else { "│  " }) };
    // A file read once is listed once. Repeats, in any order, are gathered
    // into the row of the first of them.
    let mut shown: Vec<usize> = Vec::new();
    let mut repeats: Vec<&str> = Vec::new();
    for (i, child) in node.children.iter().enumerate() {
        if child.tokens == 0 && node.children[..i].iter().any(|seen| seen.name == child.name) {
            repeats.push(child.name.as_str());
            continue;
        }
        shown.push(i);
    }
    for (position, index) in shown.iter().enumerate() {
        let child = &node.children[*index];
        let last = position + 1 == shown.len();
        flatten(child, inner.clone(), false, last && repeats.is_empty(), rows);
    }
    if !repeats.is_empty() {
        repeats.sort_unstable();
        repeats.dedup();
        let first = node.children.first().expect("a repeat implies a child");
        let name = format!("{inner}└─ {} read again", repeats.join(", "));
        rows.push((name, first));
    }
}

fn note(node: &crate::query::Node) -> String {
    match node.status {
        "read" => String::new(),
        other => other.replace('-', " "),
    }
}

/// Why a file cost nothing: it was read before, or never read at all.
fn why_empty(node: &crate::query::Node) -> &'static str {
    match node.status {
        "already-loaded" => "read once, above",
        "not-found" => "not found",
        "skipped" => "skipped by the configuration",
        "not-followed" => "not followed",
        "unreadable" => "could not be read",
        "too-deep" => "past the file_depth limit",
        _ => "requested again",
    }
}

fn tail(text: &str, limit: usize) -> String {
    let length = width(text);
    if length <= limit {
        return text.to_string();
    }
    let dropped = length - limit + 1;
    format!("…{}", text.chars().skip(dropped).collect::<String>())
}

/// A path shown by its name and linked to the file itself, when the terminal
/// takes links. The whole path otherwise.
fn linked_file(path: &str, links: Links) -> String {
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
    if links.0 { hyperlink(name, &file_url(path), links) } else { path.to_string() }
}

pub(crate) fn file_url(path: &str) -> String {
    // Absolute, not canonical: resolving symlinks would name `/private/var`
    // for the `/var` an editor opened on macOS. The URL must match what the
    // client knows.
    let absolute = std::path::absolute(path).unwrap_or_else(|_| Path::new(path).to_path_buf());
    crate::paths::file_url(&absolute)
}

pub fn github(records: &[Record]) -> String {
    let mut out = String::new();
    for record in records {
        let level = match record.get("severity").and_then(Json::as_str) {
            Some("error") => "error",
            Some("warning") => "warning",
            _ => "notice",
        };
        let message = plain(record.get("message").unwrap_or(&Json::Null));
        let title = plain(record.get("code").unwrap_or(&Json::Null));
        let file = plain(record.get("path").unwrap_or(&Json::Null));
        let line = record.get("line").and_then(Json::as_u64).unwrap_or(1);
        let column = record.get("col").and_then(Json::as_u64).unwrap_or(1);
        let _ = writeln!(
            out,
            "::{level} file={file},line={line},col={column},title={title}::{}",
            message.replace('%', "%25").replace('\n', "%0A").replace('\r', "%0D")
        );
    }
    out
}

pub fn sarif(records: &[Record], version: &str) -> String {
    let rules: Vec<serde_json::Value> = {
        let mut codes: Vec<String> = records.iter().map(|r| plain(r.get("code").unwrap_or(&Json::Null))).collect();
        codes.sort();
        codes.dedup();
        codes.into_iter().map(|code| serde_json::json!({ "id": code })).collect()
    };
    let results: Vec<serde_json::Value> = records
        .iter()
        .map(|record| {
            serde_json::json!({
                "ruleId": plain(record.get("code").unwrap_or(&Json::Null)),
                "level": match record.get("severity").and_then(Json::as_str) {
                    Some("error") => "error",
                    Some("warning") => "warning",
                    _ => "note",
                },
                "message": { "text": plain(record.get("message").unwrap_or(&Json::Null)) },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": plain(record.get("path").unwrap_or(&Json::Null)) },
                        "region": {
                            "startLine": record.get("line").and_then(Json::as_u64).unwrap_or(1),
                            "startColumn": record.get("col").and_then(Json::as_u64).unwrap_or(1),
                        }
                    }
                }],
                "fixes": sarif_fixes(record),
                "properties": { "fix": record.get("fix") },
            })
        })
        .collect();
    let report = serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": { "name": "satex", "version": version, "rules": rules } },
            "columnKind": "unicodeCodePoints",
            "results": results,
        }],
    });
    serde_json::to_string_pretty(&report).unwrap_or_default()
}

/// The edits a finding's fix carries, grouped by file in the order given.
fn edits_by_file(record: &Record) -> Vec<(String, Vec<crate::lint::fix::Edit>)> {
    let mut out: Vec<(String, Vec<crate::lint::fix::Edit>)> = Vec::new();
    let edits = record.get("edits").and_then(Json::as_array).into_iter().flatten();
    for edit in edits.filter_map(crate::lint::fix::Edit::from_json) {
        match out.iter_mut().find(|(path, _)| *path == edit.path) {
            Some((_, list)) => list.push(edit),
            None => out.push((edit.path.clone(), vec![edit])),
        }
    }
    out
}

/// SARIF 2.1.0 § 3.55: a fix is the changes it makes, so a finding whose
/// fix is prose only has none; its prose is under `properties.fix`.
fn sarif_fixes(record: &Record) -> Json {
    let changes: Vec<Json> = edits_by_file(record)
        .into_iter()
        .map(|(path, edits)| {
            let replacements: Vec<Json> = edits
                .iter()
                .map(|edit| {
                    serde_json::json!({
                        "deletedRegion": {
                            "startLine": edit.start.line,
                            "startColumn": edit.start.col,
                            "endLine": edit.end.line,
                            "endColumn": edit.end.col,
                        },
                        "insertedContent": { "text": edit.replacement },
                    })
                })
                .collect();
            serde_json::json!({ "artifactLocation": { "uri": path }, "replacements": replacements })
        })
        .collect();
    if changes.is_empty() {
        return Json::Null;
    }
    serde_json::json!([{
        "description": { "text": record.get("fix").map(plain).unwrap_or_default() },
        "artifactChanges": changes,
        "properties": { "applicability": record.get("applicability") },
    }])
}

/// LSP severities (`DiagnosticSeverity`): 1 error, 2 warning, 3 information.
fn lsp_severity(record: &Record) -> u8 {
    match record.get("severity").and_then(Json::as_str) {
        Some("error") => 1,
        Some("warning") => 2,
        _ => 3,
    }
}

/// Findings as a language server publishes them: per file, the
/// `Diagnostic`s and the `quickfix` `CodeAction`s that repair them, with
/// positions 0-based and columns in UTF-16 code units, as the protocol
/// counts them. An editor integration hands these on unchanged.
pub fn lsp(records: &[Record]) -> String {
    lsp_with(records, file_url)
}

/// The column just past what a finding at `col` is about: a command with
/// the `{…}`/`[…]` groups that follow it on its line (to the line's end
/// when one stays open, as a runaway argument does), else the word there.
fn call_end(line: &str, col: u32) -> u32 {
    let start = crate::source::byte_of_col(line, col).min(line.len());
    let rest = &line[start..];
    let mut chars = rest.char_indices().peekable();
    let word = |c: char| c.is_alphanumeric() || c == '@';
    let mut end = match chars.next() {
        Some((_, '\\')) => {
            let first = chars.next().map_or(1, |(i, c)| i + c.len_utf8());
            let mut end = first;
            while let Some(&(i, c)) = chars.peek().filter(|(_, c)| word(*c) && rest[1..first].chars().all(word)) {
                end = i + c.len_utf8();
                chars.next();
            }
            end
        }
        Some((_, c)) if word(c) => rest.find(|c: char| !word(c)).unwrap_or(rest.len()),
        Some((_, c)) => c.len_utf8(),
        None => 0,
    };
    if rest.starts_with('\\') {
        let base = end;
        let (mut depth, mut closed) = (0u32, base);
        for (i, c) in rest[base..].char_indices() {
            match c {
                '{' | '[' => depth += 1,
                '}' | ']' if depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        closed = base + i + 1;
                    }
                }
                '*' | ' ' if depth == 0 => {}
                _ if depth == 0 => break,
                _ => {}
            }
        }
        end = if depth > 0 { rest.len() } else { closed };
    }
    crate::source::col_of_byte(line, start + end)
}

/// [`lsp`] with the URI each path is published under.
pub fn lsp_with(records: &[Record], uri: impl Fn(&str) -> String) -> String {
    let sources = crate::lint::fix::Sources::default();
    let position = |path: &str, line: u64, col: u64| -> Json {
        let pos = crate::lint::fix::Pos::new(line.max(1) as u32, col.max(1) as u32);
        let (line, character) =
            sources.get(path).and_then(|text| text.utf16(pos)).unwrap_or((pos.line - 1, pos.col - 1));
        serde_json::json!({ "line": line, "character": character })
    };
    let mut files: Vec<(String, Vec<Json>, Vec<Json>)> = Vec::new();
    for record in records {
        let path = plain(record.get("path").unwrap_or(&Json::Null));
        let line = record.get("line").and_then(Json::as_u64).unwrap_or(1);
        let col = record.get("col").and_then(Json::as_u64).unwrap_or(1);
        let at = position(&path, line, col);
        let end = sources.get(&path).map_or(col, |text| call_end(text.line(line.max(1) as u32), col as u32) as u64);
        let diagnostic = serde_json::json!({
            "range": { "start": at, "end": position(&path, line, end) },
            "severity": lsp_severity(record),
            "code": record.get("code"),
            "source": "satex",
            "message": plain(record.get("message").unwrap_or(&Json::Null)),
            "data": { "fix": record.get("fix"), "applicability": record.get("applicability") },
        });
        let changes: serde_json::Map<String, Json> = edits_by_file(record)
            .into_iter()
            .map(|(file, edits)| {
                let edits: Vec<Json> = edits
                    .iter()
                    .map(|edit| {
                        serde_json::json!({
                            "range": {
                                "start": position(&file, edit.start.line.into(), edit.start.col.into()),
                                "end": position(&file, edit.end.line.into(), edit.end.col.into()),
                            },
                            "newText": edit.replacement,
                        })
                    })
                    .collect();
                (uri(&file), Json::Array(edits))
            })
            .collect();
        let action = (!changes.is_empty()).then(|| {
            serde_json::json!({
                "title": plain(record.get("fix").unwrap_or(&Json::Null)),
                "kind": "quickfix",
                "diagnostics": [diagnostic.clone()],
                "isPreferred": record.get("applicability").and_then(Json::as_str) == Some("safe"),
                "edit": { "changes": changes },
            })
        });
        let slot = match files.iter().position(|(p, _, _)| *p == path) {
            Some(index) => index,
            None => {
                files.push((path.clone(), Vec::new(), Vec::new()));
                files.len() - 1
            }
        };
        files[slot].1.push(diagnostic);
        files[slot].2.extend(action);
    }
    let out: Vec<Json> = files
        .into_iter()
        .map(|(path, diagnostics, actions)| {
            serde_json::json!({ "uri": uri(&path), "diagnostics": diagnostics, "codeActions": actions })
        })
        .collect();
    serde_json::to_string_pretty(&out).unwrap_or_default()
}

/// What the run could not follow, in a few lines: every cause with how often
/// it happened and where it first did. `gaps.report: false` turns it off.
pub fn gaps(analysis: &crate::machine::Analysis) -> String {
    /// Enough to show the causes that matter without burying the answer.
    const SHOWN: usize = 5;
    let records = crate::query::run(analysis, crate::query::Query::Gaps, &crate::query::Filter::Always);
    if records.is_empty() {
        return String::new();
    }
    let mut out = format!("{}\n", heading("recognized gaps"));
    for record in records.iter().take(SHOWN) {
        let count = record.get("count").and_then(Json::as_u64).unwrap_or(0);
        let code = record.get("code").map(plain).unwrap_or_default();
        let line = record.get("line").and_then(Json::as_u64).unwrap_or(0);
        let file = record.get("file").map(plain).unwrap_or_default();
        let _ = writeln!(out, "  {} {count}× first at {file}:{line}", label(&code));
    }
    if records.len() > SHOWN {
        let _ = writeln!(out, "  {}", footer(&format!("`satex query gaps` lists {} more", records.len() - SHOWN)));
    }
    out
}

pub fn heading(text: &str) -> String {
    format!("{BOLD}{text}{BOLD:#}")
}

/// A line under a table naming what it leaves out.
pub fn footer(text: &str) -> String {
    label(&format!("  {text}"))
}

pub fn label(text: &str) -> String {
    format!("{DIM}{text}{DIM:#}")
}

pub fn subject(text: &str) -> String {
    format!("{NAME}{text}{NAME:#}")
}

pub fn good(text: &str) -> String {
    format!("{GOOD}{text}{GOOD:#}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_with(provides: &[(&'static str, &[&str])]) -> crate::query::Node {
        crate::query::Node {
            name: "root".into(),
            path: String::new(),
            kind: "input",
            status: "read",
            options: Vec::new(),
            identification: None,
            span: None,
            provides: provides
                .iter()
                .map(|(tag, names)| (*tag, names.iter().map(|n| (*n).to_string()).collect()))
                .collect(),
            catcodes: Vec::new(),
            tokens: 0,
            cached: false,
            millis: 0.0,
            children: Vec::new(),
        }
    }

    #[test]
    fn defines_line_counts_each_tag_separately_and_generically() {
        let node =
            node_with(&[("macro", &["a", "b"]), ("environment", &["e"]), ("switch", &["s"]), ("bespoke", &["x"])]);
        let line = defines_line(&node).unwrap();
        assert!(line.contains("2 commands"), "{line}");
        assert!(line.contains("1 environment"), "{line}");
        assert!(line.contains("1 switch"), "{line}");
        // A tag this has no special word for still reads, plainly pluralized.
        assert!(line.contains("1 bespoke"), "{line}");
    }

    #[test]
    fn defines_line_pluralizes_the_two_irregulars() {
        let node = node_with(&[("alias", &["a", "b"]), ("switch", &["s", "t"])]);
        let line = defines_line(&node).unwrap();
        assert!(line.contains("2 aliases"), "{line}");
        assert!(line.contains("2 switches"), "{line}");
    }

    #[test]
    fn defines_line_is_none_when_nothing_is_defined() {
        assert!(defines_line(&node_with(&[])).is_none());
    }

    #[test]
    fn narrow_columns_terminate() {
        assert_eq!(wrap("abc de", 1), vec!["a", "b", "c", "d", "e"]);
        assert_eq!(wrap("漢字", 1), vec!["漢", "字"]);
        assert_eq!(wrap("ab", 0), vec!["a", "b"]);
    }

    #[test]
    fn no_line_is_wider_than_the_column() {
        for limit in 2..12 {
            for text in ["hello world foo", "漢字漢字漢字 abc", "aaaaaaaaaaaaaaaaaaaa", ""] {
                for line in wrap(text, limit) {
                    assert!(width(&line) <= limit, "{text:?} at {limit}: {line:?}");
                }
            }
        }
    }

    fn finding_record(severity: &str, category: &str, code: &str, message: &str, fix: Option<&str>) -> Record {
        serde_json::json!({
            "file": "paper.tex", "path": "paper.tex", "line": 1, "col": 1,
            "severity": severity, "category": category, "code": code, "name": code,
            "message": message, "fix": fix, "origin": "document",
        })
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn lint_groups_by_severity_and_never_truncates() {
        let long_message = "x".repeat(200);
        let records = vec![
            finding_record("error", "correctness", "some-code", &long_message, Some("fix it")),
            finding_record("warning", "correctness", "other-code", "short", None),
            finding_record("info", "style", "style-code", "a suggestion", None),
            finding_record("warning", "performance", "perf-code", "5 files read", Some("precompile")),
            finding_record("info", "precision", "analysis-imprecision", "widened", None),
        ];
        let out = lint(&records, Links(false), true);
        assert!(out.contains("errors (1)"));
        assert!(out.contains("warnings (1)"));
        assert!(out.contains("suggestions (1)"));
        assert!(out.contains("performance (1)"));
        assert!(out.contains("precision"));
        // wrapped, not cut: no ellipsis, and the tail of the long message
        // still shows up somewhere in the output.
        assert!(!out.contains('…'));
        assert!(out.contains(&long_message[long_message.len() - 10..]));
        assert!(out.contains("1 error, 1 warning, 1 suggestion, 1 performance note, 1 precision note"));
    }
}

fn paint(style: Style, text: &str) -> String {
    format!("{style}{text}{style:#}")
}

/// Colors one `--stats`/`-v` status line (cache, stats, hotspot, limit),
/// keeping its text. `anstream` drops the codes where color is off.
pub fn status(line: &str) -> String {
    let (lead, body) = line.split_at(line.len() - line.trim_start().len());
    let colored = if let Some(rest) = body.strip_prefix("satex: ") {
        format!("satex: {}", status(rest))
    } else if let Some(rest) = body.strip_prefix("hot ") {
        let (name, rows) = rest.split_once(' ').unwrap_or((rest, ""));
        let label = match name {
            "steps" => fg(AnsiColor::Blue).bold(),
            "forks" => fg(AnsiColor::Magenta).bold(),
            "joins" => fg(AnsiColor::Green).bold(),
            _ => fg(AnsiColor::Yellow).bold(),
        };
        let rows = rows.split("  ").map(|r| match r.split_once(' ') {
            Some((n, at)) => format!("{} {}", paint(BOLD, n), paint(NAME, at)),
            None => r.to_string(),
        });
        format!("hot {} {}", paint(label, name), rows.collect::<Vec<_>>().join("  "))
    } else if let Some(rest) = body.strip_prefix("limits.") {
        let Some((key, rest)) = rest.split_once(" = ") else { return line.to_string() };
        let (value, rest) = rest.split_once(" was reached ").unwrap_or((rest, ""));
        let (count, rest) = rest.split_once(" (first at ").unwrap_or((rest, ""));
        let (at, advice) = rest.split_once("); ").unwrap_or((rest, ""));
        format!(
            "limits.{} = {} was reached {} (first at {}); {}",
            paint(WARNING, key),
            paint(BOLD, value),
            paint(BOLD, count),
            paint(NAME, at),
            paint(DIM, advice)
        )
    } else if body.starts_with("cache ") {
        match body.split_once("rebuilt (") {
            Some((head, why)) => format!("{head}{} {}", paint(WARNING, "rebuilt"), paint(DIM, &format!("({why}"))),
            None => match body.rsplit_once("reused") {
                Some((head, tail)) => format!("{head}{}{tail}", paint(GOOD, "reused")),
                None => body.to_string(),
            },
        }
    } else if body.starts_with(|c: char| c.is_ascii_digit()) && body.contains(" files") {
        let mut parts = body.split("  |  ");
        let first = parts.next().unwrap_or("");
        let rest = parts.map(|p| match p {
            "budget exhausted" => paint(ERROR, p),
            _ => p.to_string(),
        });
        let all = std::iter::once(paint(BOLD, first)).chain(rest);
        all.collect::<Vec<_>>().join("  |  ")
    } else {
        body.to_string()
    };
    format!("{lead}{colored}")
}
