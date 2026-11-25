//! What a label points at, as its completion shows it: the kind of thing
//! (the environment it stands in, `§` for a sectioning unit), its number
//! and its title (a caption, the unit's heading, hyperref's name).
//!
//! All of it comes from the lines the build wrote to its `.aux` files
//! (`\newlabel{key}{{number}{page}{title}{anchor}{}}`, and a float's
//! `\contentsline{figure}{\numberline{1}{caption}}` for the list of figures),
//! the environments the run saw open and close, and the unit the label
//! stands in.

use std::collections::HashMap;

use crate::analysis::facts::Occurrence;
use crate::analysis::machine::Analysis;
use crate::engine::builtins::OccKind;
use crate::engine::tex::Span;

pub struct Labels {
    /// Each label's groups of `\newlabel`: number, page, then hyperref's title and anchor.
    written: HashMap<String, Vec<String>>,
    /// A list entry's title by its level and number: `(figure, 2)`.
    contents: HashMap<(String, String), String>,
    /// The environments, from `\begin` to `\end`.
    environments: Vec<(Span, Span, String)>,
}

impl Labels {
    pub fn of(analysis: &Analysis) -> Labels {
        let (mut written, mut contents) = (HashMap::new(), HashMap::new());
        let mut open: Vec<&Occurrence> = Vec::new();
        let mut environments = Vec::new();
        for text in analysis.written.iter().filter(|(name, _)| name.ends_with(".aux")).filter_map(|(_, t)| t.as_ref()) {
            for line in text.lines() {
                if let Some(rest) = line.trim_start().strip_prefix("\\newlabel") {
                    if let [key, value, ..] = groups(rest).as_slice() {
                        written.insert(key.trim().to_string(), groups(value).iter().map(|g| clean(g)).collect());
                    }
                } else if let Some(at) = line.find("\\contentsline") {
                    let parts = groups(&line[at + "\\contentsline".len()..]);
                    if let [level, entry, ..] = parts.as_slice()
                        && let Some(entry) = entry.trim_start().strip_prefix("\\numberline")
                        && let Some(number) = groups(entry).first()
                    {
                        let title = entry.trim_start()[number.len() + 2..].trim();
                        let title = match groups(title).as_slice() {
                            [one] if one.len() + 2 == title.len() => clean(one),
                            _ => clean(title),
                        };
                        contents.insert((level.trim().to_string(), clean(number)), title);
                    }
                }
            }
        }
        for o in &analysis.facts.occurrences {
            match o.kind {
                OccKind::BeginEnvironment => open.push(o),
                OccKind::EndEnvironment => {
                    if let Some(b) = open.iter().rposition(|b| b.key == o.key && b.span.file == o.span.file) {
                        let b = open.remove(b);
                        environments.push((b.span, o.span, o.key.clone()));
                    }
                }
                _ => {}
            }
        }
        Labels { written, contents, environments }
    }

    /// The number `key`'s `\newlabel` wrote, as typeset.
    pub fn number(&self, key: &str) -> Option<&str> {
        self.written.get(key).and_then(|g| g.first()).map(String::as_str).filter(|n| !n.is_empty())
    }

    /// What `label` points at, in a line: `figure 2: A caption`, `§1.2: Intro`, `equation 3`.
    pub fn about(&self, label: &Occurrence) -> Option<String> {
        let groups = self.written.get(&label.key);
        let number = groups.and_then(|g| g.first()).filter(|n| !n.is_empty());
        let named = groups.and_then(|g| g.get(2)).filter(|t| !t.is_empty());
        let at = |s: &Span| (s.line, s.col);
        let mut around: Vec<&(Span, Span, String)> = (self.environments.iter())
            .filter(|(b, e, name)| {
                b.file == label.span.file && at(b) <= at(&label.span) && at(&label.span) <= at(e) && name != "document"
            })
            .collect();
        around.sort_by_key(|(b, ..)| std::cmp::Reverse(at(b)));
        // the environment whose list entry has this number (a figure around a `center`), else the innermost
        let listed = number.and_then(|n| {
            around.iter().find_map(|(.., e)| Some((e.as_str(), self.contents.get(&(e.clone(), n.clone()))?)))
        });
        let (kind, title) = match (listed, around.first()) {
            (Some((e, t)), _) => (e.to_string(), Some(t.clone())),
            (None, Some((.., e))) => (e.clone(), None),
            (None, None) => ("§".to_string(), label.section.as_deref().map(str::to_string)),
        };
        let title = named.cloned().or(title).filter(|t| !t.is_empty());
        let head = match (kind.as_str(), number) {
            ("§", Some(n)) => format!("§{n}"),
            ("§", None) => String::new(),
            (k, Some(n)) => format!("{k} {n}"),
            (k, None) => k.to_string(),
        };
        let line = match (head.is_empty(), title) {
            (_, None) => head,
            (true, Some(t)) => t,
            (false, Some(t)) => format!("{head}: {t}"),
        };
        (!line.is_empty()).then_some(line)
    }
}

/// The braced groups `text` starts with, spaces between them skipped.
fn groups(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while rest.starts_with('{') {
        let mut depth = 0;
        let Some(end) = rest.char_indices().find_map(|(i, c)| {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(i)
        }) else {
            break;
        };
        out.push(&rest[1..end]);
        rest = rest[end + 1..].trim_start();
    }
    out
}

/// TeX text as words: no control words (`\ignorespaces`), braces or ties.
pub(super) fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // a control word is dropped, a control symbol keeps its character (`\&`)
                if chars.peek().is_some_and(|c| c.is_ascii_alphabetic() || *c == '@') {
                    while chars.peek().is_some_and(|c| c.is_ascii_alphabetic() || *c == '@') {
                        chars.next();
                    }
                } else if let Some(c) = chars.next() {
                    out.push(c);
                }
            }
            '{' | '}' => {}
            '~' => out.push(' '),
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `\newlabel`'s value is groups in a group; a caption's entry has its number as `\numberline`'s argument.
    #[test]
    fn groups_and_clean_text() {
        assert_eq!(groups("{sec:a}{{1.2}{3}{Intro}{section.1.2}{}}"), ["sec:a", "{1.2}{3}{Intro}{section.1.2}{}"]);
        assert_eq!(clean("\\ignorespaces A {network}~graph \\& more"), "A network graph & more");
    }
}
