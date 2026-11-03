//! `%\satex@configure{key=value, …}` in a comment takes the keys of
//! `satex.yaml`: completion and hover read them from the same schema an
//! editor reads for `satex.yaml` itself (`satex.schema.json`).

use std::sync::OnceLock;

use lsp_types::{CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind};
use serde_json::Value;

const OPENER: &str = "\\satex@configure{";

fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| serde_json::from_str(include_str!("../../satex.schema.json")).unwrap_or_default())
}

/// Every setting: its dotted key and its schema.
fn settings() -> Vec<(String, &'static Value)> {
    fn walk(prefix: &str, node: &'static Value, out: &mut Vec<(String, &'static Value)>) {
        match node.get("properties").and_then(Value::as_object) {
            Some(props) if node.get("type") == Some(&Value::from("object")) => {
                for (k, v) in props {
                    walk(&if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") }, v, out);
                }
            }
            _ => out.push((prefix.to_string(), node)),
        }
    }
    let mut out = Vec::new();
    walk("", schema(), &mut out);
    out
}

/// The schema of the setting `key`, written with `.` or `/`.
fn setting(key: &str) -> Option<&'static Value> {
    let key = key.trim().replace('/', ".");
    settings().into_iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

/// What a setting is, as Markdown: its description, type and default.
fn about(key: &str, node: &Value) -> String {
    let mut out = format!("### `{key}`\n\n");
    if let Some(text) = node.get("description").and_then(Value::as_str) {
        out.push_str(&format!("{text}\n\n"));
    }
    if let Some(ty) = node.get("type").and_then(Value::as_str) {
        out.push_str(&format!("type `{ty}`"));
    }
    if let Some(default) = node.get("default") {
        out.push_str(&format!(", default `{default}`"));
    }
    if let Some(example) = node.get("x-example").and_then(Value::as_str) {
        out.push_str(&format!("\n\n*{example}*"));
    }
    out
}

/// The setting being written at the end of `before`, a line up to the
/// cursor: the key typed so far, and the value typed so far after `=`.
fn typing(before: &str) -> Option<(&str, Option<&str>)> {
    let open = before.rfind(OPENER)? + OPENER.len();
    let inside = &before[open..];
    if inside.contains('}') || !before[..open].trim_start().starts_with('%') {
        return None;
    }
    let last = inside.rsplit(',').next().unwrap_or_default();
    Some(match last.split_once('=') {
        Some((key, value)) => (key.trim(), Some(value.trim_start())),
        None => (last.trim_start(), None),
    })
}

/// What a comment allows at the end of `before`, a line up to the cursor:
/// `None` outside a comment. In a comment no command completes, except the
/// directive names after `\satex@`, and code inside a directive's braces.
pub enum InComment {
    /// The directives whose name starts with what is typed.
    Names(Vec<CompletionItem>),
    /// Inside a directive's braces: completion as in code.
    Code,
    Nothing,
}

pub fn in_comment(before: &str) -> Option<InComment> {
    let code = super::handlers::uncommented(before);
    let comment = &before[code.len()..];
    if comment.is_empty() {
        return None;
    }
    if let Some(items) = magic(comment.trim_start_matches('%').trim_start()) {
        return Some(InComment::Names(items));
    }
    if let Some(at) = comment.rfind("\\satex@") {
        let rest = &comment[at + "\\satex@".len()..];
        let name_len = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
        if name_len == rest.len() {
            return Some(InComment::Names(names(rest)));
        }
        let mut depth = 0i32;
        for c in rest[name_len..].chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        if depth > 0 {
            return Some(InComment::Code);
        }
    }
    // `\sat…` on its way to a directive
    let typed = comment.rsplit('\\').next().filter(|_| comment.contains('\\')).unwrap_or_default();
    match !typed.is_empty() && "satex@".starts_with(typed) {
        true => Some(InComment::Names(names(""))),
        false => Some(InComment::Nothing),
    }
}

/// The relations `\ifnum` and `\ifdim` read (TeX § 503: `<`, `=` or `>` of
/// category 12), then the ones a directive's condition adds, each with
/// what it tests.
const RELATIONS: [(&str, &str); 6] = [
    ("<", "less than"),
    ("=", "equal to"),
    (">", "greater than"),
    ("<=", "at most (a directive's condition only)"),
    (">=", "at least (a directive's condition only)"),
    ("!=", "not equal to (a directive's condition only)"),
];

/// The relations that fit at the end of `before`: after the first operand of
/// an `\ifnum` or `\ifdim`, or of a comparison in a directive's condition,
/// with a space typed and no relation yet.
pub fn relations(before: &str) -> Option<Vec<CompletionItem>> {
    if !before.ends_with(' ') {
        return None;
    }
    let code = super::handlers::uncommented(before);
    let (operand, all) = match code.len() < before.len() {
        // a directive's condition: the comparison after the last `and`/`or` or the brace
        true => {
            let inside = &before[before.rfind("\\satex@")?..];
            let inside = &inside[inside.find('{')? + 1..];
            let atom = [" and ", " or "].iter().filter_map(|w| inside.rfind(w).map(|i| i + w.len())).max().unwrap_or(0);
            (&inside[atom..], true)
        }
        false => {
            let at = ["\\ifnum", "\\ifdim"].iter().filter_map(|w| code.rfind(w).map(|i| i + w.len())).max()?;
            // `\ifnumexpr` or the like is another name
            if code[at..].starts_with(|c: char| c.is_ascii_alphabetic()) {
                return None;
            }
            (&code[at..], false)
        }
    };
    if operand.trim().is_empty() || operand.contains(['<', '=', '>']) {
        return None;
    }
    let items =
        RELATIONS.iter().filter(|(r, _)| all || r.len() == 1).enumerate().map(|(i, (r, what))| CompletionItem {
            label: (*r).to_string(),
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some((*what).to_string()),
            sort_text: Some(format!("{i}")),
            ..Default::default()
        });
    Some(items.collect())
}

/// The magic comments editors and satex read (`crate::plugins::magic`),
/// and the bibliography programs a `% !BIB program` names.
const MAGIC: [&str; 5] =
    ["!TeX program = ", "!TeX root = ", "!TeX encoding = ", "!TeX spellcheck = ", "!BIB program = "];
const BIB_PROGRAMS: [&str; 4] = ["biber", "bibtex", "bibtex8", "bibtexu"];

/// Completion in a magic comment, `comment` after its `%`: the heads while
/// one is typed, the programs after `program =`.
fn magic(comment: &str) -> Option<Vec<CompletionItem>> {
    if !comment.starts_with('!') {
        return None;
    }
    let item = |label: &str, kind| CompletionItem { label: label.to_string(), kind: Some(kind), ..Default::default() };
    let Some((head, typed)) = comment.split_once('=') else {
        let lower = comment.to_ascii_lowercase();
        let heads = MAGIC.iter().filter(|m| m.to_ascii_lowercase().starts_with(&lower));
        return Some(heads.map(|m| item(m, CompletionItemKind::KEYWORD)).collect());
    };
    let (head, typed) = (head.trim().to_ascii_lowercase(), typed.trim());
    let programs: Vec<&str> = match head.as_str() {
        "!tex program" | "!tex ts-program" => {
            crate::project::config::Engine::PROGRAMS.iter().map(|(p, _)| *p).collect()
        }
        "!bib program" => BIB_PROGRAMS.to_vec(),
        _ => return Some(Vec::new()),
    };
    Some(programs.into_iter().filter(|p| p.starts_with(typed)).map(|p| item(p, CompletionItemKind::VALUE)).collect())
}

/// The directive items whose name starts with `typed`.
fn names(typed: &str) -> Vec<CompletionItem> {
    crate::analysis::machine::Machine::DIRECTIVES
        .iter()
        .filter(|(name, _)| name.starts_with(typed))
        .map(|(name, about)| CompletionItem {
            label: format!("\\satex@{name}"),
            filter_text: Some(format!("\\satex@{name}")),
            insert_text: Some(format!("\\satex@{name}{{$1}}")),
            insert_text_format: Some(lsp_types::InsertTextFormat::SNIPPET),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some((*about).to_string()),
            ..Default::default()
        })
        .collect()
}

/// Completion inside a configure directive: keys, or the values a key takes.
pub fn completion(before: &str) -> Option<Vec<CompletionItem>> {
    let (key, value) = typing(before)?;
    let docs = |key: &str, node: &Value| {
        Some(Documentation::MarkupContent(MarkupContent { kind: MarkupKind::Markdown, value: about(key, node) }))
    };
    let items = match value {
        Some(typed) => {
            let node = setting(key)?;
            let values: Vec<String> = match node.get("enum").and_then(Value::as_array) {
                Some(all) => all.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
                None if node.get("type") == Some(&Value::from("boolean")) => vec!["true".into(), "false".into()],
                None => node.get("default").map(|d| d.to_string().trim_matches('"').to_string()).into_iter().collect(),
            };
            values
                .into_iter()
                .filter(|v| v.starts_with(typed))
                .map(|v| CompletionItem {
                    label: v,
                    kind: Some(CompletionItemKind::VALUE),
                    documentation: docs(key, node),
                    ..Default::default()
                })
                .collect()
        }
        None => settings()
            .into_iter()
            .filter(|(k, _)| k.starts_with(&key.replace('/', ".")))
            .map(|(k, node)| CompletionItem {
                detail: node.get("description").and_then(Value::as_str).map(str::to_string),
                documentation: docs(&k, node),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some(format!("{k}=")),
                label: k,
                ..Default::default()
            })
            .collect(),
    };
    Some(items)
}

/// The setting under column `col` (characters, from 1) of a configure directive's line.
pub fn hover(line: &str, col: u32) -> Option<String> {
    let open = line.find(OPENER)? + OPENER.len();
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let at = chars.get((col as usize).saturating_sub(1))?.0;
    if at < open {
        return None;
    }
    let word = |c: char| c.is_ascii_alphanumeric() || "_./".contains(c);
    let start = line[..at].rfind(|c: char| !word(c)).map_or(0, |i| i + 1);
    let end = line[at..].find(|c: char| !word(c)).map_or(line.len(), |i| at + i);
    let key = &line[start..end];
    // a value names no setting: the key before its `=` does
    let key = match line[..start].ends_with('=') {
        true => line[..start - 1].rsplit([',', '{']).next()?.trim(),
        false => key,
    };
    Some(about(&key.replace('/', "."), setting(key)?))
}
