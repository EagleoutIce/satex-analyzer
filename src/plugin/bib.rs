//! The bibliography databases a document names: where `\bibliography{refs}`
//! finds `refs.bib`, and the keys its entries declare.

use std::path::{Path, PathBuf};

use crate::machine::Analysis;

/// One entry of a `.bib` file: its key, the line its `@type{` opens on,
/// its type and what completion shows of it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Entry {
    pub key: String,
    pub line: u32,
    pub kind: String,
    pub title: String,
    pub author: String,
    pub year: String,
    /// Every field as written, braces dropped, in entry order.
    pub fields: Vec<(String, String)>,
    /// The database it was read from, when known.
    pub file: String,
}

/// A database as BibTeX finds it: `⟨name⟩.bib` beside the document, or the
/// name as given when it carries the extension (BibTeX reads `\bibdata`
/// names with `.bib` appended; btxdoc, "Using BibTeX").  The project walk
/// is the fallback, for a database kept in a subdirectory.
pub fn resolve(analysis: &Analysis, name: &str) -> Option<PathBuf> {
    let base = analysis
        .files
        .get(usize::from(analysis.main_file))
        .map(|file| crate::paths::dir_of(Path::new(&file.path)))
        .unwrap_or_default();
    let file = if name.ends_with(".bib") { name.to_string() } else { format!("{name}.bib") };
    let beside = base.join(&file);
    if beside.is_file() {
        return Some(beside);
    }
    if let found @ Some(_) =
        analysis.plugins.discovery.bibliographies.iter().find(|path| path.ends_with(&file)).cloned()
    {
        return found;
    }
    // BIBINPUTS (kpathsea manual, "Supported file formats"), resolved the
    // same way as TEXINPUTS: env, then latexmkrc/Makefile, then satex.yaml.
    // Relative directories are relative to the build's working directory,
    // not the document's own.
    let bibinputs = crate::paths::effective(
        "BIBINPUTS",
        &analysis.project,
        &analysis.settings.paths.bibinputs,
        &analysis.project.root,
    );
    bibinputs.dirs.iter().map(|dir| dir.join(&file)).find(|path| path.is_file())
}

/// The entries of a `.bib` file.  `@string`, `@preamble` and `@comment`
/// declare no key; everything else is an entry whose key runs from the
/// opening delimiter to the first comma (btxdoc, "The database files").
pub fn entries(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut line = 1u32;
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        line += count_lines(&rest[..at]);
        rest = &rest[at + 1..];
        let kind_len = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
        let kind = rest[..kind_len].to_ascii_lowercase();
        let after = rest[kind_len..].trim_start();
        let Some(open) = after.chars().next().filter(|c| *c == '{' || *c == '(') else { continue };
        if kind.is_empty() || matches!(kind.as_str(), "string" | "preamble" | "comment") {
            continue;
        }
        let body = &after[open.len_utf8()..];
        let end = body.find([',', '}', ')', '\n']).unwrap_or(body.len());
        let key = body[..end].trim();
        if !key.is_empty() {
            let mut entry = Entry { key: key.to_string(), line, kind, ..Default::default() };
            entry.fields = fields(&body[end..], if open == '(' { ')' } else { '}' });
            for (name, value) in &entry.fields {
                match name.as_str() {
                    "title" => entry.title = value.clone(),
                    "author" => entry.author = value.clone(),
                    "year" => entry.year = value.clone(),
                    "date" if entry.year.is_empty() => entry.year = value.chars().take(4).collect(),
                    _ => {}
                }
            }
            out.push(entry);
        }
    }
    out
}

/// The `name = value` fields of an entry's body, up to its `close`: a
/// value in braces or quotes with its braces dropped, or a bare word.
fn fields(body: &str, close: char) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = body.char_indices().peekable();
    loop {
        while chars.next_if(|(_, c)| c.is_whitespace() || *c == ',').is_some() {}
        let Some(&(_, c)) = chars.peek() else { break };
        if c == close {
            break;
        }
        let mut name = String::new();
        while let Some((_, c)) = chars.next_if(|(_, c)| c.is_alphanumeric() || "-_".contains(*c)) {
            name.push(c);
        }
        while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
        if name.is_empty() || chars.next_if(|(_, c)| *c == '=').is_none() {
            break;
        }
        while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
        let mut value = String::new();
        let mut depth = 0;
        let quoted = chars.next_if(|(_, c)| *c == '"').is_some();
        for (_, c) in chars.by_ref() {
            match c {
                '{' => depth += 1,
                '}' if depth > 0 => depth -= 1,
                '"' if quoted && depth == 0 => break,
                c if !quoted && depth == 0 && (c == ',' || c == close) => break,
                c => value.push(c),
            }
        }
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        out.push((name.to_ascii_lowercase(), value));
    }
    out
}

fn count_lines(text: &str) -> u32 {
    u32::try_from(text.bytes().filter(|b| *b == b'\n').count()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_carry_what_completion_shows() {
        let text = "@string{x = \"y\"}\n@Article{knuth84,\n  author = {Donald E. Knuth},\n  title = \"Literate {P}rogramming\",\n  year = 1984,\n}\n@book(lamport, title={\\LaTeX}, date={1994-01-01})\n";
        let e = entries(text);
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].key.as_str(), e[0].kind.as_str(), e[0].line), ("knuth84", "article", 2));
        assert_eq!(
            (e[0].author.as_str(), e[0].title.as_str(), e[0].year.as_str()),
            ("Donald E. Knuth", "Literate Programming", "1984")
        );
        assert_eq!((e[1].key.as_str(), e[1].title.as_str(), e[1].year.as_str()), ("lamport", "\\LaTeX", "1994"));
    }
}
