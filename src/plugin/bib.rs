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
/// What the bibliography tool would hand the typesetting pass: a `.bbl`
/// of the cited `entries`, written by satex itself and never read from a
/// build. With `datalist` (biblatex's list name, from its `.bcf`) it is
/// biber's format, else BibTeX's `thebibliography` (`natbib` labels).
pub fn virtual_bbl(entries: &[&Entry], datalist: Option<&str>, natbib: bool) -> String {
    match datalist {
        Some(list) => biber_bbl(entries, list),
        None => bibtex_bbl(entries, natbib),
    }
}

/// The `Last, First and …` names of a field, as (family, given).
fn names(value: &str) -> Vec<(String, String)> {
    value
        .split(" and ")
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| match n.split_once(',') {
            Some((last, first)) => (last.trim().to_string(), first.trim().to_string()),
            None => match n.rsplit_once(' ') {
                Some((first, last)) => (last.to_string(), first.trim().to_string()),
                None => (n.to_string(), String::new()),
            },
        })
        .collect()
}

/// A TeX-safe field value: the characters TeX reads as syntax escaped.
fn safe(value: &str) -> String {
    let mut out = String::new();
    let mut prev = ' ';
    for c in value.chars() {
        if matches!(c, '%' | '&' | '#' | '_') && prev != '\\' {
            out.push('\\');
        }
        out.push(c);
        prev = c;
    }
    out
}

fn field<'a>(e: &'a Entry, names: &[&str]) -> Option<&'a str> {
    e.fields.iter().find(|(n, v)| names.contains(&n.as_str()) && !v.is_empty()).map(|(_, v)| v.as_str())
}

fn bibtex_bbl(entries: &[&Entry], natbib: bool) -> String {
    let mut out = format!("\\begin{{thebibliography}}{{{}}}\n", "9".repeat(entries.len().to_string().len()));
    for e in entries {
        let who = names(field(e, &["author", "editor"]).unwrap_or_default());
        let year = field(e, &["year", "date"]).map(|y| y.chars().take(4).collect::<String>()).unwrap_or_default();
        let author = who
            .iter()
            .map(|(l, f)| if f.is_empty() { l.clone() } else { format!("{f} {l}") })
            .collect::<Vec<_>>()
            .join(", ");
        if natbib {
            let last = who.first().map_or(e.key.as_str(), |(l, _)| l.as_str());
            out.push_str(&format!("\\bibitem[{}({year}){}]{{{}}}\n", safe(last), safe(last), e.key));
        } else {
            out.push_str(&format!("\\bibitem{{{}}}\n", e.key));
        }
        out.push_str(&format!("{}.\n", safe(&author)));
        if let Some(title) = field(e, &["title"]) {
            out.push_str(&format!("\\newblock {}.\n", safe(title)));
        }
        let venue = field(e, &["journal", "journaltitle", "booktitle", "publisher", "school", "institution"]);
        let tail = [venue.map(safe), Some(year).filter(|y| !y.is_empty())].into_iter().flatten().collect::<Vec<_>>();
        if !tail.is_empty() {
            out.push_str(&format!("\\newblock {}.\n", tail.join(", ")));
        }
        out.push('\n');
    }
    out.push_str("\\end{thebibliography}\n");
    out
}

/// A 32-digit hex string standing for `text`, for the hashes biber gives names.
fn hash(text: &str) -> String {
    let fnv = |seed: u64| text.bytes().fold(seed, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
    format!("{:016x}{:016x}", fnv(0xcbf2_9ce4_8422_2325), fnv(0x8422_2325_cbf2_9ce4))
}

/// Fields biber hands over in a form of their own, or not at all.
const OWN_FORM: [&str; 17] = [
    "author",
    "editor",
    "translator",
    "publisher",
    "location",
    "address",
    "institution",
    "school",
    "organization",
    "url",
    "doi",
    "eprint",
    "file",
    "abstract",
    "keywords",
    "date",
    "month",
];

fn biber_bbl(entries: &[&Entry], list: &str) -> String {
    let mut out = String::from(
        "% $ biblatex auxiliary file $\n% $ biblatex bbl format version 3.3 $\n% Do not modify the above lines!\n\
\\begingroup\n\\makeatletter\n\\@ifundefined{ver@biblatex.sty}\n  {\\@latex@error\n     {Missing 'biblatex' package}\n     \
{The bibliography requires the 'biblatex' package.}\n      \\aftergroup\\endinput}\n  {}\n\\endgroup\n\n\
\\refsection{0}\n",
    );
    out.push_str(&format!("  \\datalist[entry]{{{list}}}\n"));
    let initial = |s: &str| s.chars().next().map(|c| format!("{c}\\bibinitperiod")).unwrap_or_default();
    for e in entries {
        out.push_str(&format!("    \\entry{{{}}}{{{}}}{{}}{{}}\n", e.key, e.kind));
        let mut sortinit = String::new();
        for role in ["author", "editor", "translator"] {
            let who = names(field(e, &[role]).unwrap_or_default());
            if who.is_empty() {
                continue;
            }
            out.push_str(&format!("      \\name{{{role}}}{{{}}}{{}}{{%\n", who.len()));
            for (family, given) in &who {
                let given_initials: Vec<String> = given.split_whitespace().map(initial).collect();
                out.push_str(&format!(
                    "        {{{{hash={}}}{{%\n           family={{{}}},\n           familyi={{{}}},\n           given={{{}}},\n           giveni={{{}}}}}}}%\n",
                    hash(&format!("{family},{given}")),
                    safe(family),
                    initial(family),
                    safe(given),
                    given_initials.join("")
                ));
            }
            out.push_str("      }\n");
            if sortinit.is_empty() {
                sortinit = who[0].0.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            }
            let all = hash(&who.iter().map(|(f, g)| format!("{f},{g}")).collect::<Vec<_>>().join(";"));
            out.push_str(&format!("      \\strng{{namehash}}{{{all}}}\n      \\strng{{fullhash}}{{{all}}}\n"));
        }
        let lists = [("publisher", "publisher"), ("location", "location"), ("address", "location")]
            .into_iter()
            .chain([("institution", "institution"), ("school", "institution"), ("organization", "organization")]);
        for (given, made) in lists {
            if let Some(value) = field(e, &[given]) {
                out.push_str(&format!("      \\list{{{made}}}{{1}}{{%\n        {{{}}}%\n      }}\n", safe(value)));
            }
        }
        if sortinit.is_empty() {
            sortinit = field(e, &["title"])
                .and_then(|t| t.chars().next())
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
        }
        out.push_str(&format!(
            "      \\field{{sortinit}}{{{sortinit}}}\n      \\field{{sortinithash}}{{{}}}\n",
            hash(&sortinit)
        ));
        for (name, value) in e.fields.iter().filter(|(n, v)| !v.is_empty() && !OWN_FORM.contains(&n.as_str())) {
            let name = if name == "journal" { "journaltitle" } else { name.as_str() };
            let value = safe(value);
            let value = if name == "pages" { value.replace("--", "\\bibrangedash ") } else { value };
            out.push_str(&format!("      \\field{{{name}}}{{{value}}}\n"));
        }
        if let Some(date) = field(e, &["date"]).filter(|_| field(e, &["year"]).is_none()) {
            out.push_str(&format!("      \\field{{year}}{{{}}}\n", date.chars().take(4).collect::<String>()));
        }
        for (name, value) in e.fields.iter().filter(|(n, v)| matches!(n.as_str(), "doi" | "url") && !v.is_empty()) {
            out.push_str(&format!("      \\verb{{{name}}}\n      \\verb {value}\n      \\endverb\n"));
        }
        out.push_str("    \\endentry\n");
    }
    out.push_str("  \\enddatalist\n\\endrefsection\n\\endinput\n");
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
