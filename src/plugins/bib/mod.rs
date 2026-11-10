//! The bibliography databases a document names: where `\bibliography{refs}`
//! finds `refs.bib`, and the keys its entries declare.

use std::path::{Path, PathBuf};

use crate::analysis::machine::Analysis;

pub mod convert;
pub mod model;

named_enum! {
    /// The tool that makes a biblatex document's `.bbl` when neither its
    /// `backend=` option nor `% !BIB program` names one.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Deserialize, serde::Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Backend {
        #[default]
        Biber => "biber",
        Bibtex => "bibtex",
    }
}

/// `bib:` in `satex.yaml`.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BibConfig {
    pub backend: Backend,
}

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
    /// Every field as written, its delimiters dropped, in entry order.
    pub fields: Vec<(String, String)>,
    /// The database it was read from, when known.
    pub file: String,
    /// The fields written bare: a number, a constant or a `#` concatenation.
    pub bare: Vec<String>,
}

/// A database as BibTeX finds it: `⟨name⟩.bib` beside the document, or the
/// name as given when it carries the extension (BibTeX reads `\bibdata`
/// names with `.bib` appended; btxdoc, "Using BibTeX").  The project walk
/// is the fallback, for a database kept in a subdirectory.
pub fn resolve(analysis: &Analysis, name: &str) -> Option<PathBuf> {
    let base = analysis
        .files
        .get(usize::from(analysis.main_file))
        .map(|file| crate::project::paths::dir_of(Path::new(&file.path)))
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
    let bibinputs = crate::project::paths::effective(
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
            (entry.fields, entry.bare) = fields(&body[end..], if open == '(' { ')' } else { '}' });
            for (name, value) in &entry.fields {
                match name.as_str() {
                    "title" => entry.title = unbraced(value),
                    "author" => entry.author = unbraced(value),
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
/// value in braces or quotes without them, or a bare word. Inner braces
/// stay: `{\TeX}book` is not `\TeXbook`.
fn fields(body: &str, close: char) -> (Vec<(String, String)>, Vec<String>) {
    let (mut out, mut bare) = (Vec::new(), Vec::new());
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
        let outer = usize::from(!quoted && chars.next_if(|(_, c)| *c == '{').is_some());
        depth += outer;
        if !quoted && outer == 0 {
            bare.push(name.to_ascii_lowercase());
        }
        for (_, c) in chars.by_ref() {
            match c {
                '}' if depth == outer && outer == 1 => break,
                '{' => {
                    depth += 1;
                    value.push(c);
                }
                '}' if depth > 0 => {
                    depth -= 1;
                    value.push(c);
                }
                '"' if quoted && depth == 0 => break,
                c if !quoted && depth == 0 && (c == ',' || c == close) => break,
                c => value.push(c),
            }
        }
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        out.push((name.to_ascii_lowercase(), value));
    }
    (out, bare)
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
pub(crate) fn names(value: &str) -> Vec<(String, String)> {
    // BibTeX splits names and their parts only outside braces: `{GCC
    // Development Team}` is one family name, `Alexander {von Rhein}` a
    // first name and one family part.
    top_split(value, " and ")
        .into_iter()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| match top_split(n, ",").as_slice() {
            [last, first, ..] => (last.trim().to_string(), first.trim().to_string()),
            _ => match top_split(n, " ").last().filter(|last| last.len() < n.len()) {
                Some(last) => (last.to_string(), n[..n.len() - last.len()].trim().to_string()),
                None => (n.to_string(), String::new()),
            },
        })
        .collect()
}

/// `text` split at each `sep` outside braces.
fn top_split<'a>(text: &'a str, sep: &str) -> Vec<&'a str> {
    let (mut parts, mut depth, mut start, mut escaped) = (Vec::new(), 0i32, 0, false);
    for (i, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '{' => depth += 1,
            '}' => depth -= 1,
            _ if depth == 0 && i >= start && text[i..].starts_with(sep) => {
                parts.push(&text[start..i]);
                start = i + sep.len();
            }
            _ => {}
        }
    }
    parts.push(&text[start.min(text.len())..]);
    parts
}

/// A name part's first letter as TeX reads it: a brace group or an accent
/// command with its letter is one, never half a group or a bare escape.
fn first_letter(s: &str) -> &str {
    let mut depth = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            '\\' => {
                // the command, then the letter it applies to
                if chars.next_if(|(_, c)| !c.is_alphabetic()).is_none() {
                    while chars.next_if(|(_, c)| c.is_alphabetic()).is_some() {}
                }
                while chars.next_if(|(_, c)| *c == ' ').is_some() {}
                continue;
            }
            _ => {}
        }
        if depth <= 0 {
            return &s[..i + c.len_utf8()];
        }
    }
    s
}

/// A TeX-safe field value: the characters TeX reads as syntax escaped.
/// The letter a name or title sorts under, past braces and accents:
/// `{\\"O}sterle` sorts under `O`, not under a `{` no argument could close.
fn sort_initial(text: &str) -> String {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut rest = chars.clone();
                if rest.next().is_some_and(|n| !n.is_alphabetic()) {
                    chars = rest;
                }
            }
            c if c.is_alphanumeric() => return c.to_uppercase().to_string(),
            _ => {}
        }
    }
    String::new()
}

pub(crate) fn safe(value: &str) -> String {
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

/// A field value as completion shows it: its braces dropped.
fn unbraced(value: &str) -> String {
    value.chars().filter(|c| !matches!(c, '{' | '}')).collect()
}

/// `text` closed with a period unless it ends a sentence already, as
/// BibTeX's `add.period$` does: braces at the end do not count.
fn sentence(text: &str) -> String {
    if text.trim_end_matches('}').ends_with(['.', '?', '!']) { text.to_string() } else { format!("{text}.") }
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
        out.push_str(&format!("{}\n", sentence(&safe(&author))));
        if let Some(title) = field(e, &["title"]) {
            out.push_str(&format!("\\newblock {}\n", sentence(&safe(title))));
        }
        let venue = field(e, &["journal", "journaltitle", "booktitle", "publisher", "school", "institution"]);
        let tail = [venue.map(safe), Some(year).filter(|y| !y.is_empty())].into_iter().flatten().collect::<Vec<_>>();
        if !tail.is_empty() {
            out.push_str(&format!("\\newblock {}\n", sentence(&tail.join(", "))));
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
    let initial = |s: &str| match first_letter(s) {
        "" => String::new(),
        lead => format!("{lead}\\bibinitperiod"),
    };
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
                // biber's form: `J\bibinitperiod\bibinitdelim K\bibinitperiod`
                let given_initials: Vec<String> = given.split_whitespace().map(initial).collect();
                out.push_str(&format!(
                    "        {{{{hash={}}}{{%\n           family={{{}}},\n           familyi={{{}}},\n           given={{{}}},\n           giveni={{{}}}}}}}%\n",
                    hash(&format!("{family},{given}")),
                    safe(family),
                    initial(family),
                    safe(given),
                    given_initials.join("\\bibinitdelim ")
                ));
            }
            out.push_str("      }\n");
            if sortinit.is_empty() {
                sortinit = sort_initial(&who[0].0);
            }
            let all = hash(&who.iter().map(|(f, g)| format!("{f},{g}")).collect::<Vec<_>>().join(";")); // not prose
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
            sortinit = field(e, &["title"]).map(sort_initial).unwrap_or_default();
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

    /// A name that starts with a braced accent must not put its brace into
    /// `\\field{sortinit}`: the `.bbl` would never close it.
    #[test]
    fn a_braced_accent_sorts_under_its_letter() {
        assert_eq!(sort_initial("{\\\"O}sterle"), "O");
        assert_eq!(sort_initial("{van} Berg"), "V");
        assert_eq!(sort_initial("\\AA ngstr"), "A");
        assert_eq!(sort_initial("{}"), "");
    }

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

    /// BibTeX splits names only outside braces: a corporate author is one
    /// family name and a braced particle stays with it. Splitting inside
    /// left `\\name` arguments unbalanced, so the `.bbl` never closed them.
    #[test]
    fn braced_names_split_outside_braces() {
        let pair = |f: &str, g: &str| (f.to_string(), g.to_string());
        assert_eq!(names("{GCC Development Team}"), [pair("{GCC Development Team}", "")]);
        assert_eq!(
            names("Alexander {von Rhein} and Th{\\\"u}m, Thomas"),
            [pair("{von Rhein}", "Alexander"), pair("Th{\\\"u}m", "Thomas")]
        );
        assert_eq!(names("{Smith and Sons}, Ltd"), [pair("{Smith and Sons}", "Ltd")]);
        let e = entries("@misc{k, author = {{GCC Development Team}}, title = {T}}").remove(0);
        let bbl = biber_bbl(&[&e], "x");
        assert_eq!(bbl.matches('{').count(), bbl.matches('}').count(), "{bbl}");
    }

    #[test]
    fn the_bbl_keeps_inner_braces_and_adds_no_second_period() {
        let text = "@book{k, author = {Donald E. Knuth}, title = {The {\\TeX}book}, publisher = {A.}}\n";
        let e = entries(text);
        assert_eq!(e[0].title, "The \\TeXbook");
        let bbl = virtual_bbl(&[&e[0]], None, false);
        assert!(bbl.contains("\\newblock The {\\TeX}book.\n"), "{bbl}");
        assert!(bbl.contains("\\newblock A.\n"), "{bbl}");
    }

    #[test]
    fn biber_initials_are_whole_letters_kept_apart() {
        let text = "@book{k, author = {Doe, Ann Bea and {\\\"U}lm, \\'Eva}, title = {T}}\n";
        let bbl = virtual_bbl(&[&entries(text)[0]], Some("x"), false);
        assert!(bbl.contains("giveni={A\\bibinitperiod\\bibinitdelim B\\bibinitperiod}"), "{bbl}");
        assert!(bbl.contains("familyi={{\\\"U}\\bibinitperiod}"), "{bbl}");
        assert!(bbl.contains("giveni={\\'E\\bibinitperiod}"), "{bbl}");
    }
}
