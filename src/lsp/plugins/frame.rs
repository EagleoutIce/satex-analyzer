//! "Compile only this frame": a lens on each frame ([`super::overlay::frames`]) opening a view
//! whose `compile` is the setup a client compiles with: `\includeonlyframes{label}` before
//! `\begin{document}`, and a `label=` in the frame's options when it has none. Offered only
//! where the run defined `\includeonlyframes`.

use super::overlay::{Pair, frame_at, frames};
use super::{Cx, Hit, TypePlugin, closing, edit_of, key_values, put, rewrite};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::engine::builtins::OccKind;
use crate::linter::fix::{Pos, Text};
use crate::project::config::TypePlugins;
use lsp_types::{CodeAction, CodeLens, Command, Range, TextEdit};
use serde_json::{Value as Json, json};

pub(super) struct Frame;

const COMMAND: &str = "includeonlyframes";

impl TypePlugin for Frame {
    fn name(&self) -> &'static str {
        "frame"
    }
    fn category(&self) -> &'static str {
        "build"
    }
    fn about(&self) -> &'static str {
        "compile only this frame: \\includeonlyframes with its label, one added when it has none"
    }
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["actions", "view", "lens"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.frame.enabled
    }
    fn actions(&self, cx: &Cx, _hit: &Hit) -> Vec<CodeAction> {
        setup(cx.analysis, cx.doc, cx.path, cx.pos.line)
            .map(|s| rewrite(cx, format!("Compile only this frame ({})", s.label), s.edits))
            .into_iter()
            .collect()
    }
    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let s = setup(cx.analysis, cx.doc, cx.path, cx.pos.line)?;
        let title = format!("Compile only frame {}", s.label);
        let note = match s.added {
            true => format!("The frame has no label: `label={}` is added to its options.", s.label),
            false => format!("`\\{COMMAND}{{{}}}` before `\\begin{{document}}` leaves the other frames out.", s.label),
        };
        Some(json!({
            "title": title,
            "compile": { "engine": "pdflatex", "label": s.label, "lines": [s.lines.0, s.lines.1], "edit": edit_of(cx.path, s.edits.clone()) },
            "sections": [
                { "kind": "text", "markdown": note },
                { "kind": "actions", "items": [{ "title": title, "edits": s.edits }] },
            ],
        }))
    }
    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        if !defined(analysis) {
            return Vec::new();
        }
        let uri = super::super::handlers::path_uri(path);
        frames(analysis, doc, path)
            .into_iter()
            .map(|(b, _)| {
                let at = super::super::handlers::to_lsp(doc, Pos::new(b.span.line, 1));
                let command = Command {
                    title: "Compile only this frame".into(),
                    command: "satex.openView".into(),
                    arguments: Some(vec![json!(uri.as_str()), json!(at), json!("frame")]),
                };
                CodeLens { range: Range { start: at, end: at }, command: Some(command), data: None }
            })
            .collect()
    }
}

fn defined(a: &Analysis) -> bool {
    a.interner.lookup(COMMAND).is_some_and(|s| a.env.is_defined(s))
}

struct Setup {
    label: String,
    added: bool,
    lines: (u32, u32),
    edits: Vec<TextEdit>,
}

/// The setup for the frame around `line`.
fn setup(a: &Analysis, doc: &Text, path: &str, line: u32) -> Option<Setup> {
    if !defined(a) {
        return None;
    }
    let frame = frame_at(a, doc, path, line)?;
    let text = doc.body();
    let (begin, end) = (frame.0, frame.1);
    let line_start = doc.offset(Pos::new(begin.span.line, 1))?;
    let opening = format!("\\begin{{{}}}", begin.key);
    let name_end = line_start + text[line_start..].find(&opening)? + opening.len();
    let mut edits = Vec::new();
    let (label, added) = match options(text, name_end) {
        Some(inner) => match key_values(text, inner.clone()).into_iter().find(|(k, _)| k == "label") {
            Some((_, v)) => (super::unbraced(&text[v]).to_string(), false),
            None => {
                let label = fresh(a, doc, path, &frame);
                let sep = if text[inner.clone()].trim().is_empty() { "" } else { "," };
                edits.push(put(doc, inner.end, inner.end, format!("{sep}label={label}")));
                (label, true)
            }
        },
        None => {
            let label = fresh(a, doc, path, &frame);
            edits.push(put(doc, after_spec(text, name_end), after_spec(text, name_end), format!("[label={label}]")));
            (label, true)
        }
    };
    let document = super::pairs(a, path, "document").into_iter().next()?.0;
    let at = doc.offset(Pos::new(document.span.line, 1))?;
    let call = format!("\\{COMMAND}{{{label}}}");
    // an earlier restriction is replaced
    let old = text[..at].find(&format!("\\{COMMAND}{{"));
    match old.and_then(|o| Some((o, closing(text, o + COMMAND.len() + 1, '{')?))) {
        Some((o, c)) => edits.push(put(doc, o, c + 1, call)),
        None => edits.push(put(doc, at, at, format!("{call}\n"))),
    }
    Some(Setup { label, added, lines: (begin.span.line, end.span.line), edits })
}

/// The bytes after an overlay specification `<…>` right after byte `at`, else `at`.
fn after_spec(text: &str, at: usize) -> usize {
    match text[at..].starts_with('<') {
        true => text[at..].find('>').map_or(at, |k| at + k + 1),
        false => at,
    }
}

/// The inside of the frame's `[…]` options after its name (and its overlay specification).
fn options(text: &str, at: usize) -> Option<std::ops::Range<usize>> {
    let at = after_spec(text, at);
    text[at..].starts_with('[').then(|| Some(at + 1..closing(text, at, '[')?)).flatten()
}

/// A label no other label of the run has: `frame` and the frame's place.
fn fresh(a: &Analysis, doc: &Text, path: &str, frame: &Pair) -> String {
    let n = frames(a, doc, path).iter().position(|f| f.0.span == frame.0.span).map_or(1, |k| k + 1);
    let taken = |l: &str| a.facts.occurrences.iter().any(|o| o.kind == OccKind::Label && o.key == l);
    let base = format!("frame{n}");
    (0..).map(|k| if k == 0 { base.clone() } else { format!("{base}-{k}") }).find(|l| !taken(l)).unwrap_or(base)
}
