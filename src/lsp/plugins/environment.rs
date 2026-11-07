//! Environments: on the name of a `\begin` or `\end` the run met, "Change environment…" switches
//! that one occurrence to another environment available there, rewriting both ends and adding or
//! dropping the required arguments. The view lists the environments there that make the same
//! kind of material ([`handlers::material`]), with their arguments.

use super::{Cx, Hit, TypePlugin, button, rewrite};
use crate::analysis::roles::{KeyKind, Ty};
use crate::lsp::{handlers, snapshot};
use crate::project::config::TypePlugins;
use lsp_types::TextEdit;
use serde_json::{Value as Json, json};

pub(super) struct Environment;

impl TypePlugin for Environment {
    fn name(&self) -> &'static str {
        "environment"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "change one environment to another, both ends and its arguments"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Environment)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.environment.enabled
    }

    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let pair = pair(cx)?;
        // only environments that make the same kind of material: a tabular becomes an alignment
        let kind = handlers::material(cx.analysis, &pair.name);
        let items: Vec<Json> = (handlers::environments(cx.analysis, cx.pos).into_iter())
            .filter(|(env, _)| kind.is_some() && *env != pair.name && handlers::material(cx.analysis, env) == kind)
            .map(|(env, _)| {
                let n = handlers::environment_arguments(cx.analysis, &env);
                let args: String = (1..=n).map(|i| format!("{{⟨{i}⟩}}")).collect();
                let action = rewrite(cx, env.clone(), edits(cx, &pair, &env));
                json!({ "title": env, "detail": args, "edits": button(&action)["edits"] })
            })
            .collect();
        if items.is_empty() {
            return None;
        }
        Some(json!({
            "title": "Change environment",
            "sections": [
                { "kind": "choices", "placeholder": "environment", "items": items },
                { "kind": "text", "markdown": format!("Changes this `{}` only, to an environment making the same kind of material: its `\\begin` and `\\end`, and its required arguments (kept where both take them, empty where the new one takes more).", pair.name) },
            ],
        }))
    }
}

/// The `\begin` and `\end` of the environment whose name is at the cursor: lines and the column
/// (from 1) of each name, and the required arguments the `\begin` line gives after it.
pub(super) struct Pair {
    pub name: String,
    pub begin: (u32, u32),
    pub end: (u32, u32),
}

/// The pair the run met around the cursor, matched by nesting in this file.
pub(super) fn pair(cx: &Cx) -> Option<Pair> {
    let name = cx.at.item.typed.trim().to_string();
    let (b, o) = super::pairs(cx.analysis, cx.path, &name)
        .into_iter()
        .find(|(b, o)| b.span.line == cx.pos.line || o.span.line == cx.pos.line)?;
    let at = |line: u32, from: u32, word: &str| name_col(cx.doc.line(line), from, word, &name);
    Some(Pair {
        begin: (b.span.line, at(b.span.line, b.span.col, "begin")?),
        end: (o.span.line, at(o.span.line, o.span.col, "end")?),
        name,
    })
}

/// The column of `name` in `\word{name}` at or after column `from` of `line`.
fn name_col(line: &str, from: u32, word: &str, name: &str) -> Option<u32> {
    let start = crate::engine::source::byte_of_col(line, from.max(1)).min(line.len());
    let head = format!("\\{word}{{{name}}}");
    let at = line[start..].find(&head).map(|i| start + i).or_else(|| line.find(&head))?;
    Some(crate::engine::source::col_of_byte(line, at + word.len() + 2))
}

/// The edits that make the pair `env`, through the printer: both names, and the `\begin`'s
/// arguments the old one read (through its last `{…}`) made the new one's, kept where both take
/// them, dropped or added empty.
fn edits(cx: &Cx, pair: &Pair, env: &str) -> Vec<TextEdit> {
    let had = usize::from(handlers::environment_arguments(cx.analysis, &pair.name));
    let wants = usize::from(handlers::environment_arguments(cx.analysis, env));
    let at = cx.doc.offset(crate::text::Pos::new(pair.begin.0, pair.begin.1)).and_then(|b| b.checked_sub(7));
    let Some(mut n) = at.and_then(|at| snapshot::read(cx.doc, at, &[], false)) else { return Vec::new() };
    let mut braced = 0;
    let read = n.args.iter().take_while(|a| {
        let more = braced < had;
        braced += usize::from(a.open == '{');
        more
    });
    let read = read.count();
    n.args.truncate(read);
    let mut kept = 0;
    for a in &mut n.args {
        kept += usize::from(a.open == '{');
        a.drop = a.open == '[' || kept > wants;
    }
    n.args.extend((kept..wants).map(|_| snapshot::Arg::text('{', "")));
    n.name = env.into();
    snapshot::edits(cx.analysis, cx.doc, &[n])
}
