//! "Store in a box": an environment of the document's body moves to the preamble as a saved box
//! (`\newsavebox`, then `\sbox`, or `lrbox` when its `\begin` changed the catcode of a
//! character TeX's initial catcodes make a command, a minipage around a body read in vertical
//! mode), and its place gets `\usebox`. Identical copies share one box. Refused where the
//! content reads its position: a label, reference or citation the run met in it, or a command
//! whose code reads a counter or a reference (`\c@…`, `\r@…`).

use super::environment::pair;
use super::{Cx, Hit, TypePlugin, put, rewrite};
use crate::analysis::roles::{KeyKind, Ty};
use crate::engine::builtins::OccKind;
use crate::engine::tex::{Sym, Tok};
use crate::project::config::TypePlugins;
use lsp_types::CodeAction;

pub(super) struct Savebox;

impl TypePlugin for Savebox {
    fn name(&self) -> &'static str {
        "savebox"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "store an environment in a saved box in the preamble, refused where it reads its position"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Key(KeyKind::Environment)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["hover", "actions"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.savebox.enabled
    }
    fn hover(&self, cx: &Cx, _hit: &Hit) -> Option<String> {
        let plan = plan(cx)?;
        Some(match &plan.refused {
            Some(why) => format!("Not storable in a box: {why}."),
            None if plan.copies.len() > 1 => format!(
                "Storable in a box: {} identical copies can share one (`\\usebox{{\\{}}}`).",
                plan.copies.len(),
                plan.name
            ),
            None => format!("Storable in a box: \"Store in a box\" moves it to the preamble as `\\{}`.", plan.name),
        })
    }
    fn actions(&self, cx: &Cx, _hit: &Hit) -> Vec<CodeAction> {
        let Some(plan) = plan(cx) else { return Vec::new() };
        if plan.refused.is_some() {
            return Vec::new();
        }
        let mut out = vec![rewrite(
            cx,
            format!("Store {} in a box \\{}", plan.env, plan.name),
            plan.edits(cx, &plan.copies[..1]),
        )];
        if plan.copies.len() > 1 {
            let title = format!("Store {} identical {} in one box \\{}", plan.copies.len(), plan.env, plan.name);
            out.push(rewrite(cx, title, plan.edits(cx, &plan.copies)));
        }
        out
    }
}

struct Plan {
    env: String,
    name: String,
    /// The bytes of each identical occurrence, this one first.
    copies: Vec<(usize, usize)>,
    /// Where the preamble ends: the `\begin{document}`.
    preamble: usize,
    verbatim: bool,
    vertical: bool,
    refused: Option<String>,
}

impl Plan {
    fn edits(&self, cx: &Cx, copies: &[(usize, usize)]) -> Vec<lsp_types::TextEdit> {
        let text = cx.doc.body();
        let (from, to) = copies[0];
        let body = &text[from..to];
        let name = &self.name;
        let body = if self.vertical {
            format!("\\begin{{minipage}}{{\\textwidth}}\n{body}\n\\end{{minipage}}")
        } else {
            body.to_string()
        };
        let saved = if self.verbatim {
            format!("\\newsavebox{{\\{name}}}\n\\begin{{lrbox}}{{\\{name}}}\n{body}\n\\end{{lrbox}}\n")
        } else {
            format!("\\newsavebox{{\\{name}}}\n\\sbox{{\\{name}}}{{%\n{body}%\n}}\n")
        };
        let mut out = vec![put(cx.doc, self.preamble, self.preamble, saved)];
        out.extend(copies.iter().map(|&(a, b)| put(cx.doc, a, b, format!("\\usebox{{\\{name}}}"))));
        out
    }
}

/// What storing the environment at the cursor takes, when it stands in the document's body.
fn plan(cx: &Cx) -> Option<Plan> {
    let p = pair(cx)?;
    let a = cx.analysis;
    let text = cx.doc.body();
    let file = super::super::type_at::file_of(a, cx.path)?;
    let at = |line, col| cx.doc.offset(crate::text::Pos::new(line, col));
    // `\begin{` stands 7 characters before the name, `}` closes `\end{name`
    let from = at(p.begin.0, p.begin.1)?.checked_sub("\\begin{".len())?;
    let to = at(p.end.0, p.end.1)? + p.name.len() + 1;
    let start = (a.facts.occurrences.iter())
        .find(|o| o.kind == OccKind::BeginEnvironment && o.key == "document" && o.span.file == file)?;
    let preamble = at(start.span.line, 1)?;
    if p.name == "document" || from < preamble || !text[from..].starts_with("\\begin{") {
        return None;
    }
    let lines = p.begin.0..=p.end.0;
    let inside = |s: crate::engine::tex::Span| s.file == file && lines.contains(&s.line);
    let refused = (a.facts.occurrences.iter())
        .find(|o| matches!(o.kind, OccKind::Label | OccKind::Ref | OccKind::Cite) && inside(o.span))
        .map(|o| format!("its {} `{}` on line {} depends on where it stands", o.kind.as_str(), o.key, o.span.line))
        .or_else(|| {
            let mut seen = Vec::new();
            (a.facts.expansions.iter())
                .filter(|e| inside(e.span) && e.span.line > p.begin.0)
                .find(|e| positional(cx, e.name, &mut seen, 0))
                .map(|e| {
                    format!("`\\{}` on line {} reads a counter or a reference", a.interner.name(e.name), e.span.line)
                })
        });
    // the `\begin` made characters of what TeX's initial catcodes make commands (`&`, `%`, `\`):
    // its body must be read as the environment reads it, not as an argument
    let verbatim = a.facts.occurrences.iter().any(|o| {
        o.kind == OccKind::Catcode
            && o.span.file == file
            && o.span.line == p.begin.0
            && o.key.chars().count() == 1
            && o.key.chars().all(|c| "\\{}$&#^_%~".contains(c))
            && o.detail.as_deref().is_some_and(|d| d == "12" || d == "13")
    });
    // read in vertical mode inside: the body is a display, so it goes into a minipage
    let vertical = (a.facts.expansions.iter()).any(|e| {
        inside(e.span)
            && e.span.line > p.begin.0
            && e.span.line < p.end.0
            && e.mode.meets(crate::engine::mode::Modes::ANY_VERTICAL)
    });
    let body = normal(&text[from..to]);
    let mut copies = vec![(from, to)];
    let mut ends: Vec<_> = (a.facts.occurrences.iter())
        .filter(|o| o.kind == OccKind::BeginEnvironment && o.key == p.name && o.span.file == file)
        .filter_map(|o| at(o.span.line, o.span.col))
        .collect();
    ends.sort();
    ends.dedup();
    for b in ends.into_iter().filter(|b| *b != from && text[*b..].starts_with("\\begin{")) {
        let close = format!("\\end{{{}}}", p.name);
        let Some(e) = text[b..].find(&close).map(|i| b + i + close.len()) else { continue };
        if normal(&text[b..e]) == body {
            copies.push((b, e));
        }
    }
    Some(Plan { name: box_name(cx, &p.name), env: p.name, copies, preamble, verbatim, vertical, refused })
}

/// Whitespace-insensitive text, to compare copies.
fn normal(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether the code of `name` reads a counter (`\c@…`) or a reference (`\r@…`), itself or
/// through the macros it calls, a few levels down.
fn positional(cx: &Cx, name: Sym, seen: &mut Vec<Sym>, depth: u8) -> bool {
    let a = cx.analysis;
    if depth > 4 || seen.contains(&name) {
        return false;
    }
    seen.push(name);
    let crate::engine::tex::Meaning::Macro(m) = a.env.meaning(name) else { return false };
    let toks = &m.replacement_text;
    toks.iter().enumerate().any(|(i, t)| match t.tok {
        Tok::Cs(s) => {
            let n = a.interner.name(s);
            if n.starts_with("c@") || n.starts_with("r@") {
                return true;
            }
            // `\csname c@…`: the characters after it
            if n == "csname" {
                let head: String = toks[i + 1..]
                    .iter()
                    .take(2)
                    .filter_map(|t| if let Tok::Chr(c, _) = t.tok { Some(c) } else { None })
                    .collect();
                return head == "c@" || head == "r@";
            }
            positional(cx, s, seen, depth + 1)
        }
        _ => false,
    })
}

/// `\⟨letters of env⟩box`, a letter more while the name is taken.
fn box_name(cx: &Cx, env: &str) -> String {
    let base: String = env.chars().filter(char::is_ascii_alphabetic).collect::<String>() + "box";
    let taken = |n: &str| {
        cx.analysis
            .interner
            .lookup(n)
            .is_some_and(|s| !matches!(cx.analysis.env.meaning(s), crate::engine::tex::Meaning::Undefined))
            || cx.doc.body().contains(&format!("\\{n}"))
    };
    std::iter::once(base.clone()).chain(('a'..='z').map(|c| format!("{base}{c}"))).find(|n| !taken(n)).unwrap_or(base)
}
