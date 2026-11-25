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
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BibConfig {
    pub backend: Backend,
    /// Whether a build run makes the `.bbl` itself when it first looks for it, from what it
    /// cited so far ([`crate::analysis::machine::Machine`]'s `early_tool_file`). Off, the
    /// bibliography tool runs between two runs only, as in a LaTeX build.
    pub early: bool,
}

impl Default for BibConfig {
    fn default() -> BibConfig {
        BibConfig { backend: Backend::default(), early: true }
    }
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
    /// Per field, its bytes in the database: where its name starts, and its value with
    /// the delimiters.
    pub at: Vec<(usize, usize, usize)>,
    /// The entry's bytes, from its `@` to its close or its fault.
    pub span: (usize, usize),
    /// Where the entry breaks BibTeX's grammar: the byte, and what was expected there.
    pub fault: Option<(usize, &'static str)>,
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
    if beside.is_file() || crate::project::overlay::holds(&beside) {
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

/// The entries of a `.bib` file, read as BibTeX reads them (bibtex.web, "Reading the
/// database file(s)").  `@string`, `@preamble` and `@comment` declare no key; everything
/// else is an entry whose key runs from the opening delimiter to the first comma (btxdoc,
/// "The database files").  An entry that breaks the grammar keeps the fields before the
/// break, as BibTeX does, and says where it broke.
pub fn entries(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let (mut line, mut counted, mut from) = (1u32, 0, 0);
    while let Some(at) = text[from..].find('@').map(|i| from + i) {
        from = at + 1;
        let rest = &text[from..];
        let kind_len = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
        let kind = rest[..kind_len].to_ascii_lowercase();
        let after = rest[kind_len..].trim_start();
        let Some(open) = after.chars().next().filter(|c| *c == '{' || *c == '(') else { continue };
        if kind.is_empty() || matches!(kind.as_str(), "string" | "preamble" | "comment") {
            continue;
        }
        let close = if open == '(' { ')' } else { '}' };
        let mut scan = Scan { text, at: text.len() - after.len() + 1, close };
        scan.space();
        let key = scan.take(|c| !c.is_whitespace() && c != ',' && (c != '}' || close == ')'));
        if key.is_empty() {
            continue;
        }
        line += count_lines(&text[counted..at]);
        counted = at;
        let mut entry = Entry { key: key.to_string(), line, kind, ..Default::default() };
        entry.fault = scan.fields(&mut entry).err().map(|expected| (scan.at, expected));
        entry.span = (at, scan.at);
        from = scan.at.max(from);
        for (name, value) in &entry.fields {
            match name.as_str() {
                "title" if entry.title.is_empty() => entry.title = unbraced(value),
                "author" if entry.author.is_empty() => entry.author = unbraced(value),
                "year" if entry.year.is_empty() => entry.year = value.clone(),
                _ => {}
            }
        }
        if entry.year.is_empty() {
            entry.year = field(&entry, &["date"]).unwrap_or_default().chars().take(4).collect();
        }
        out.push(entry);
    }
    out
}

/// A reader over one entry's body, after its key.
struct Scan<'a> {
    text: &'a str,
    at: usize,
    close: char,
}

impl<'a> Scan<'a> {
    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }
    fn take(&mut self, keep: impl Fn(char) -> bool) -> &'a str {
        let rest = &self.text[self.at..];
        let len = rest.find(|c| !keep(c)).unwrap_or(rest.len());
        self.at += len;
        &rest[..len]
    }
    fn space(&mut self) {
        self.take(char::is_whitespace);
    }
    fn eat(&mut self, c: char) -> bool {
        let found = self.peek() == Some(c);
        self.at += if found { c.len_utf8() } else { 0 };
        found
    }
    /// A field name or constant: BibTeX's identifier.
    fn name(&mut self) -> &'a str {
        self.take(|c| !c.is_whitespace() && !"\"#%'(),={}".contains(c))
    }
    /// A `{…}` or `"…"` piece without its delimiters. Braces balance in both, and count
    /// whatever stands before them: BibTeX knows no `\{`.
    fn delimited(&mut self, quoted: bool) -> Result<&'a str, &'static str> {
        let start = self.at + 1;
        let mut depth = 0i32;
        for (i, c) in self.text[self.at..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                '"' if quoted && depth == 0 && i > 0 => depth = -1,
                _ => continue,
            }
            if depth < 0 || (depth == 0 && !quoted) {
                self.at += i + 1;
                // a `}` that closes nothing inside quotes
                return if quoted && c == '}' { Err("balanced braces") } else { Ok(&self.text[start..self.at - 1]) };
            }
        }
        self.at = self.text.len();
        Err(if quoted { "a closing `\"`" } else { "a closing `}`" })
    }
    /// The `, name = value` fields up to the entry's close, into `entry`. `Err` names
    /// what BibTeX expected where the entry breaks.
    fn fields(&mut self, entry: &mut Entry) -> Result<(), &'static str> {
        loop {
            self.space();
            if self.eat(self.close) {
                return Ok(());
            }
            if !self.eat(',') {
                return Err("`,` or the entry's close");
            }
            self.space();
            if self.eat(self.close) {
                return Ok(());
            }
            let name_at = self.at;
            let name = self.name().to_ascii_lowercase();
            if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
                return Err("a field name");
            }
            self.space();
            if !self.eat('=') {
                return Err("`=`");
            }
            self.space();
            let value_at = self.at;
            let (mut pieces, mut inner, mut bare) = (0, "", false);
            loop {
                pieces += 1;
                match self.peek() {
                    Some(c @ ('{' | '"')) => inner = self.delimited(c == '"')?,
                    _ => {
                        bare = true;
                        if self.name().is_empty() {
                            return Err("a field value");
                        }
                    }
                }
                let end = self.at;
                self.space();
                if !self.eat('#') {
                    // a constant, a number or a concatenation stays as written
                    let value = if bare || pieces > 1 { &self.text[value_at..end] } else { inner };
                    if bare || pieces > 1 {
                        entry.bare.push(name.clone());
                    }
                    entry.fields.push((name, value.split_whitespace().collect::<Vec<_>>().join(" ")));
                    entry.at.push((name_at, value_at, end));
                    break;
                }
                self.space();
            }
        }
    }
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

    /// Intuition: the reader follows BibTeX's grammar, so an entry breaks exactly where BibTeX
    /// says `I was expecting a `,' or a `}'`, keeps the fields before the break, and the next
    /// `@` starts over. Pins the old reader, which stopped without a word at a missing comma
    /// and at `"a" # b`, losing every field after it.
    #[test]
    fn entries_break_where_bibtex_does() {
        let text = "@article{ok, journal = \"J\" # jan, year = 2001}\n\
                    @article{nocomma, author = {A} title = {T}}\n\
                    @article{quote, author = \"A }\", title = {T}}\n\
                    @article{open, author = {A {B}, title = {T}}\n\
                    @article{eaten, title = {T}}\n";
        let e = entries(text);
        let keys: Vec<&str> = e.iter().map(|e| e.key.as_str()).collect();
        // the unclosed brace takes its entry's close, so BibTeX gives up at the next `@`: `eaten` is read
        assert_eq!(keys, ["ok", "nocomma", "quote", "open", "eaten"]);
        assert_eq!(e[0].fault, None);
        assert_eq!(e[0].fields, [("journal".into(), "\"J\" # jan".into()), ("year".into(), "2001".into())]);
        assert_eq!(e[0].bare, ["journal", "year"]);
        let expected: Vec<Option<&str>> = e.iter().map(|e| e.fault.map(|f| f.1)).collect();
        let comma = Some("`,` or the entry's close");
        assert_eq!(expected, [None, comma, Some("balanced braces"), comma, None]);
        assert_eq!(e[1].fields, [("author".into(), "A".into())]);
        // the break is where the reader stands: the `t` of `title`, the `@` of the next entry
        assert_eq!(e[1].fault.map(|f| &text[f.0..f.0 + 5]), Some("title"));
        assert_eq!(e[3].fault.map(|f| &text[f.0..f.0 + 5]), Some("@arti"));
    }

    /// Intuition: a fix edits a field in place, so each field knows its bytes: where its name
    /// starts and its value with the delimiters. A lone key is an entry to BibTeX.
    #[test]
    fn fields_know_their_bytes() {
        let text = "@Book(k, Title = {A  {B}},\n  year=1984 )\n@misc{lone}";
        let e = entries(text);
        assert_eq!(e[0].fields, [("title".into(), "A {B}".into()), ("year".into(), "1984".into())]);
        let (name, from, to) = e[0].at[0];
        assert_eq!((&text[name..name + 5], &text[from..to]), ("Title", "{A  {B}}"));
        assert_eq!(&text[e[0].at[1].1..e[0].at[1].2], "1984");
        assert_eq!(&text[e[0].span.0..e[0].span.1], "@Book(k, Title = {A  {B}},\n  year=1984 )");
        assert_eq!((e[1].key.as_str(), e[1].fault, e[1].fields.len()), ("lone", None, 0));
    }
}
