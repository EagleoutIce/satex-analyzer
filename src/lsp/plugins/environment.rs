//! Environments: on the name of a `\begin` or `\end` the run met, "Change environment…" switches
//! that one occurrence to another environment available there, rewriting both ends and adding or
//! dropping the required arguments. The view lists every environment there with its arguments;
//! every client also gets one action per environment the document already uses.

use super::{Cx, Hit, TypePlugin, button, rewrite};
use crate::analysis::roles::{KeyKind, Ty, braced_groups};
use crate::engine::builtins::OccKind;
use crate::lsp::handlers;
use crate::project::config::TypePlugins;
use lsp_types::{CodeAction, TextEdit};
use serde_json::{Value as Json, json};

pub(super) struct Environment;

impl TypePlugin for Environment {
    fn name(&self) -> &'static str {
        "environment"
    }
    fn about(&self) -> &'static str {
        "change one environment to another, both ends and its arguments"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Environment)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["actions", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.environment.enabled
    }

    fn actions(&self, cx: &Cx, _hit: &Hit) -> Vec<CodeAction> {
        let Some(pair) = pair(cx) else { return Vec::new() };
        // the environments the document already uses, as plain actions any client applies
        let used: std::collections::BTreeSet<&str> = (cx.analysis.facts.occurrences.iter())
            .filter(|o| o.kind == OccKind::BeginEnvironment && o.key != pair.name)
            .filter(|o| super::super::type_at::file_of(cx.analysis, cx.path) == Some(o.span.file))
            .map(|o| o.key.as_str())
            .collect();
        // the dialog with every environment is the view: VS Code's light bulb and the playground open it
        used.into_iter()
            .map(|env| rewrite(cx, format!("Change {} to {env}", pair.name), edits(cx, &pair, env)))
            .collect()
    }

    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let pair = pair(cx)?;
        let items: Vec<Json> = handlers::environments(cx.analysis, cx.pos)
            .into_iter()
            .filter(|(env, _)| *env != pair.name)
            .map(|(env, _)| {
                let n = handlers::environment_arguments(cx.analysis, &env);
                let args: String = (1..=n).map(|i| format!("{{⟨{i}⟩}}")).collect();
                let action = rewrite(cx, env.clone(), edits(cx, &pair, &env));
                json!({ "title": env, "detail": args, "edits": button(&action)["edits"] })
            })
            .collect();
        Some(json!({
            "title": format!("Change environment {}", pair.name),
            "sections": [
                { "kind": "choices", "placeholder": "environment", "items": items },
                { "kind": "text", "markdown": "Changes this occurrence only: its `\\begin` and `\\end`, and its required arguments (kept where both take them, empty where the new one takes more)." },
            ],
        }))
    }
}

/// The `\begin` and `\end` of the environment whose name is at the cursor: lines and the column
/// (from 1) of each name, and the required arguments the `\begin` line gives after it.
struct Pair {
    name: String,
    begin: (u32, u32),
    end: (u32, u32),
}

/// The pair the run met around the cursor, matched by nesting in this file.
fn pair(cx: &Cx) -> Option<Pair> {
    let name = cx.at.item.typed.trim().to_string();
    let file = super::super::type_at::file_of(cx.analysis, cx.path)?;
    let mut ends: Vec<&crate::analysis::facts::Occurrence> = (cx.analysis.facts.occurrences.iter())
        .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment))
        .filter(|o| o.key == name && o.span.file == file)
        .collect();
    ends.sort_by_key(|o| (o.span.line, o.span.col));
    ends.dedup_by_key(|o| (o.span.line, o.span.col, o.kind));
    let mut open = Vec::new();
    for o in ends {
        match o.kind {
            OccKind::BeginEnvironment => open.push(o),
            _ => {
                let Some(b) = open.pop() else { continue };
                if b.span.line == cx.pos.line || o.span.line == cx.pos.line {
                    let at = |line: u32, from: u32, word: &str| name_col(cx.doc.line(line), from, word, &name);
                    return Some(Pair {
                        begin: (b.span.line, at(b.span.line, b.span.col, "begin")?),
                        end: (o.span.line, at(o.span.line, o.span.col, "end")?),
                        name,
                    });
                }
            }
        }
    }
    None
}

/// The column of `name` in `\word{name}` at or after column `from` of `line`.
fn name_col(line: &str, from: u32, word: &str, name: &str) -> Option<u32> {
    let start = crate::engine::source::byte_of_col(line, from.max(1)).min(line.len());
    let head = format!("\\{word}{{{name}}}");
    let at = line[start..].find(&head).map(|i| start + i).or_else(|| line.find(&head))?;
    Some(crate::engine::source::col_of_byte(line, at + word.len() + 2))
}

/// The edits that make the pair `env`: both names, and the `\begin`'s required arguments, kept
/// where both take them, dropped or added empty.
fn edits(cx: &Cx, pair: &Pair, env: &str) -> Vec<TextEdit> {
    let len = pair.name.chars().count() as u32;
    let line = cx.doc.line(pair.begin.0);
    let after = crate::engine::source::byte_of_col(line, pair.begin.1 + len + 1).min(line.len());
    let had = usize::from(handlers::environment_arguments(cx.analysis, &pair.name));
    let wants = usize::from(handlers::environment_arguments(cx.analysis, env));
    let given: Vec<String> = braced_groups(&line[after..]).into_iter().take(had).collect();
    // the text of the arguments the old environment read, through the last one
    let mut taken = 0;
    let mut rest = &line[after..];
    for _ in 0..given.len() {
        let trimmed = rest.trim_start();
        let Some(close) = trimmed.find('}') else { break };
        let used = rest.len() - trimmed.len() + close + 1;
        taken += used;
        rest = &rest[used..];
    }
    let args: String = (0..wants).map(|i| format!("{{{}}}", given.get(i).map_or("", String::as_str))).collect();
    let name = |(l, c): (u32, u32)| super::range(cx.doc, l, c, c + len);
    let tail = super::range(
        cx.doc,
        pair.begin.0,
        crate::engine::source::col_of_byte(line, after),
        crate::engine::source::col_of_byte(line, after + taken),
    );
    let mut out = vec![TextEdit { range: name(pair.begin), new_text: env.to_string() }];
    if taken > 0 || !args.is_empty() {
        out.push(TextEdit { range: tail, new_text: args });
    }
    out.push(TextEdit { range: name(pair.end), new_text: env.to_string() });
    out
}
