//! Overlay specifications (`<2->`): on which slides of its frame the material after one shows,
//! from the pages the run shipped and the text it set visibly on them; completion of the steps
//! the frame has. A frame is the innermost environment whose lines the run shipped pages at.

use super::{Cx, Hit, TypePlugin, closing};
use crate::analysis::facts::Occurrence;
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::engine::builtins::OccKind;
use crate::project::config::TypePlugins;
use crate::text::{Pos, Text};
use lsp_types::{CompletionItem, CompletionItemKind};
use std::ops::Range;

pub(super) struct Overlay;

impl TypePlugin for Overlay {
    fn name(&self) -> &'static str {
        "overlay"
    }
    fn category(&self) -> &'static str {
        "hover"
    }
    fn about(&self) -> &'static str {
        "the slides of its frame the material shows on, completion of the frame's steps"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::Overlay]
    }
    fn features(&self) -> &'static [&'static str] {
        &["hover", "complete"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.overlay.enabled
    }
    fn hover(&self, cx: &Cx, _hit: &Hit) -> Option<String> {
        let a = cx.analysis;
        let frame = frame_at(a, cx.doc, cx.path, cx.pos.line)?;
        let slides = slides(a, cx.path, &frame);
        let line = cx.doc.line(cx.pos.line);
        let from = crate::engine::source::byte_of_col(line, cx.at.item.col);
        let spec = line.get(from..).and_then(|r| r.split('>').next()).unwrap_or_default();
        let (start, end) = material(cx.doc, cx.doc.offset(Pos::new(cx.pos.line, cx.at.item.col))?)?;
        let shown = shown(a, cx, &slides, start, end);
        let n = slides.len();
        Some(match shown.as_slice() {
            [] => format!("Overlay `<{spec}>`: the run set none of the material visibly on the frame's {n} slides."),
            s => format!(
                "Overlay `<{spec}>`: shows on slide{} {} of {n}.",
                if s.len() == 1 { "" } else { "s" },
                s.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(", ")
            ),
        })
    }
    fn complete(&self, cx: &Cx, _hit: &Hit) -> Option<Vec<CompletionItem>> {
        let frame = frame_at(cx.analysis, cx.doc, cx.path, cx.pos.line)?;
        let n = slides(cx.analysis, cx.path, &frame).len();
        let item = |label: String, detail: String, k: usize| CompletionItem {
            label,
            kind: Some(CompletionItemKind::VALUE),
            detail: Some(detail),
            sort_text: Some(format!("{k:04}")),
            ..Default::default()
        };
        Some(
            (1..=n)
                .flat_map(|k| {
                    [
                        item(k.to_string(), format!("slide {k} of {n}"), 2 * k),
                        item(format!("{k}-"), format!("slides {k} to the last ({n})"), 2 * k + 1),
                    ]
                })
                .collect(),
        )
    }
}

/// An environment the run met: its `\begin` and `\end`.
pub(super) type Pair<'a> = (&'a Occurrence, &'a Occurrence);

/// Every environment the run met in `path` whose `\begin` the document writes, any name.
fn environments<'a>(a: &'a Analysis, doc: &Text, path: &str) -> Vec<Pair<'a>> {
    let Some(file) = super::super::type_at::file_of(a, path) else { return Vec::new() };
    let mut names: Vec<&str> = (a.facts.occurrences.iter())
        .filter(|o| o.kind == OccKind::BeginEnvironment && o.span.file == file)
        .map(|o| o.key.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    let written = |(b, _): &Pair| doc.line(b.span.line).contains(&format!("\\begin{{{}}}", b.key));
    names.into_iter().flat_map(|n| super::pairs(a, path, n)).filter(written).collect()
}

/// The pages the run shipped while reading lines `lines` of `path`, or at the start of the line
/// after them (an `\\end` ending its line), each as the range of the characters set on it.
fn pages(a: &Analysis, path: &str, lines: (u32, u32)) -> Vec<Range<u32>> {
    let Some(file) = super::super::type_at::file_of(a, path) else { return Vec::new() };
    let all = &a.facts.text.pages;
    let within = |s: &crate::engine::tex::Span| (lines.0, 1) <= (s.line, s.col) && (s.line, s.col) <= (lines.1 + 1, 1);
    (all.iter().enumerate())
        .filter(|(_, (_, s))| s.file == file && within(s))
        .map(|(k, (end, _))| k.checked_sub(1).map_or(0, |p| all[p].0)..*end)
        .collect()
}

/// The frames of `path`: environments the run shipped pages in that hold no other such.
pub(super) fn frames<'a>(a: &'a Analysis, doc: &Text, path: &str) -> Vec<Pair<'a>> {
    let shipping: Vec<Pair> = environments(a, doc, path)
        .into_iter()
        .filter(|(b, e)| !pages(a, path, (b.span.line, e.span.line)).is_empty())
        .collect();
    let within = |(b, e): &Pair, (ib, ie): &Pair| {
        (b.span.line, b.span.col) < (ib.span.line, ib.span.col)
            && (ie.span.line, ie.span.col) < (e.span.line, e.span.col)
    };
    let mut out: Vec<Pair> = shipping.iter().filter(|f| !shipping.iter().any(|g| within(f, g))).copied().collect();
    out.sort_by_key(|(b, _)| (b.span.line, b.span.col));
    out
}

/// The frame around `line`.
pub(super) fn frame_at<'a>(a: &'a Analysis, doc: &Text, path: &str, line: u32) -> Option<Pair<'a>> {
    frames(a, doc, path).into_iter().find(|(b, e)| b.span.line <= line && line <= e.span.line)
}

/// The slides of `frame`, each as the characters set on it.
pub(super) fn slides(a: &Analysis, path: &str, frame: &Pair) -> Vec<Range<u32>> {
    pages(a, path, (frame.0.span.line, frame.1.span.line))
}

/// The bytes of the material an overlay specification opening at byte `at` stands for: the
/// braced group after its `>`, else the one right before its `<` (`\only{x}<2>`), else the rest
/// of its line (`\item<2-> text`).
fn material(doc: &Text, at: usize) -> Option<(usize, usize)> {
    let text = doc.body();
    let close = at + text[at..].find('>')?;
    let rest = &text[close + 1..];
    let skip = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    if rest[skip..].starts_with('{') {
        let open = close + 1 + skip;
        return Some((open + 1, closing(text, open, '{')?));
    }
    let lt = at.checked_sub(1)?;
    if text[..lt].ends_with('}') {
        let mut depth = 0i32;
        for (i, c) in text[..lt].char_indices().rev() {
            match c {
                '}' => depth += 1,
                '{' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                return Some((i + 1, lt - 1));
            }
        }
    }
    Some((close + 1, text[close..].find('\n').map_or(text.len(), |k| close + k)))
}

/// The slides, counted from 1, on which the run set a character read from bytes `from..to` of
/// the document outside text it moved off the page.
fn shown(a: &Analysis, cx: &Cx, slides: &[Range<u32>], from: usize, to: usize) -> Vec<usize> {
    let Some(file) = super::super::type_at::file_of(a, cx.path) else { return Vec::new() };
    let (s, e) = (cx.doc.pos(from), cx.doc.pos(to));
    let hidden = crate::queries::query::invisible(&a.facts.literals);
    let text = &a.facts.text;
    // the character's own token, wherever the code that set it read it from (a frame's body)
    let inside = |i: usize| {
        text.at.get(i).is_some_and(|spot| {
            let at = (spot.span.line, spot.span.col);
            spot.span.file == file && (s.line, s.col) <= at && at < (e.line, e.col)
        })
    };
    let visible = |i: u32| !hidden.iter().any(|r| r.contains(&i));
    (slides.iter().enumerate())
        .filter(|(_, r)| (r.start..r.end).any(|i| visible(i) && inside(i as usize)))
        .map(|(k, _)| k + 1)
        .collect()
}
