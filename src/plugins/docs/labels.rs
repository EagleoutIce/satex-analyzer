//! Manuals in plain LaTeX: a heading or a list item labelled with one
//! command documents it in the prose below (`\subsection{\cs{pdfpagewidth}}`
//! in the pdfTeX manual, `\item[\cs{vskip}]` in TeX by Topic's glossary).

use std::path::Path;

use super::markup::{Places, description, group, quoted, render};
use super::{Audience, DocPlugin, Entry};
use crate::project::config::DocsConfig;

pub struct Labels;

/// How many paragraphs below a label its description may start: past a syntax line.
const PARAGRAPHS: usize = 3;

impl DocPlugin for Labels {
    fn mine(&self, config: &DocsConfig, file: &Path, text: &str, out: &mut Vec<Entry>) {
        let from = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (doc, places) = Places::of(file, text);
        for label in &config.labels {
            let open = format!("\\{label}");
            for (at, _) in doc.match_indices(&open).filter(|(at, _)| !quoted(&doc, *at)) {
                let rest = &doc[at + open.len()..];
                // `\item` and `\section`, not `\itemsep` or `\sectionmark`
                if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                    continue;
                }
                let rest = rest.strip_prefix('*').unwrap_or(rest);
                // `\item[label]`, `\section{title}`
                let Some((head, after)) = group(rest, '[', ']').or_else(|| group(rest, '{', '}')) else { continue };
                let Some(name) = command(head) else { continue };
                // its prose ends at the next label of its kind
                let after = after.split(open.as_str()).next().unwrap_or(after);
                // a label without prose (an option list's `\item[\cs{x}]`) says nothing of it
                let Some(text) = paragraphs(untagged(after), &name) else { continue };
                out.push(Entry {
                    name,
                    usage: None,
                    text: Some(text),
                    audience: Audience::User,
                    from: from.clone(),
                    link: None,
                    at: Some(places.at(at)),
                });
            }
        }
    }
}

/// The command a label is and nothing else, without its backslash: `\cs{x}`, `\verb|\x|`, `|\x|`.
fn command(head: &str) -> Option<String> {
    let shown = render(head, true);
    let name = shown.trim().strip_prefix('\\')?;
    let word = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphabetic() || c == '@');
    let symbol = name.chars().count() == 1 && !name.starts_with(char::is_alphanumeric);
    (word || symbol).then(|| name.to_string())
}

/// `text` past a leading tag with one argument (TeX by Topic's `\gr{primitive command}`).
fn untagged(text: &str) -> &str {
    let t = text.trim_start();
    let Some(cs) = t.strip_prefix('\\') else { return text };
    let len = cs.chars().take_while(char::is_ascii_alphabetic).count();
    match (len, group(&cs[len..], '{', '}')) {
        (1.., Some((_, rest))) if !rest.trim_start().is_empty() => rest,
        _ => text,
    }
}

/// The first of the paragraphs below a label that reads as a description of `name`.
fn paragraphs(text: &str, name: &str) -> Option<String> {
    text.trim_start_matches([' ', '\n'])
        .split("\n\n")
        .take(PARAGRAPHS)
        .find_map(|p| description(p.trim_start_matches(' '), Some(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// pdfTeX's manual heads each primitive with it and a syntax line; TeX
    /// by Topic's glossary labels items with it and a grammar tag. Both
    /// describe it in the prose after.
    #[test]
    fn headings_and_items_document_their_command() {
        let tex = "\\subsection{\\cs{pdfpagewidth}}\n\\pdftexprimitive{\\Syntax{\\cs{pdfpagewidth} \\Whatever{dimen}}}\n\nThis dimension parameter specifies the page width. More.\n\n\\section{Document info}\n\\begin{description}\n\\item [\\cs{vskip}]\n  \\gr{vertical command}\n  Skip vertically.\n\\item[\\cs{-}] Discretionary hyphen.\n\\item[two words] Not a command.\n\\itemsep\n\\end{description}\n";
        let mut out = Vec::new();
        Labels.mine(&DocsConfig::default(), Path::new("manual.tex"), tex, &mut out);
        let text = |n: &str| out.iter().find(|e| e.name == n).and_then(|e| e.text.clone());
        assert_eq!(text("pdfpagewidth").as_deref(), Some("This dimension parameter specifies the page width."));
        assert_eq!(text("vskip").as_deref(), Some("Skip vertically."));
        assert_eq!(text("-").as_deref(), Some("Discretionary hyphen."));
        // a heading of words names no command
        assert_eq!(out.len(), 3, "{out:?}");
    }
}
