//! References from other formats as `.bib` entries: BibTeX itself (a Zotero Better BibTeX
//! export, doi.org's answer, dblp's), RIS, EndNote's refer format, CSL-JSON (Zotero's,
//! Crossref's) and arXiv's Atom feed, and dblp's search answer. Each format's names for types
//! and fields are its own: the tables here are the formats' (RIS, refer, CSL), not a style's.
//! Plain text becomes TeX-safe; a citation key is kept when the source names one.

use super::{Entry, entries, safe};

/// The formats a text is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bibtex,
    Ris,
    Refer,
    CslJson,
    Atom,
    Dblp,
}

/// The format `text` is written in, by its first characters.
pub fn detect(text: &str) -> Option<Format> {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    let json = || serde_json::from_str::<serde_json::Value>(t).ok();
    match t.chars().next()? {
        '@' => Some(Format::Bibtex),
        '%' if t.lines().any(|l| l.starts_with("%0 ") || l.starts_with("%T ")) => Some(Format::Refer),
        '{' | '[' => json().map(|v| if v["result"]["hits"].is_object() { Format::Dblp } else { Format::CslJson }),
        '<' if t.contains("<feed") && t.contains("<entry") => Some(Format::Atom),
        _ if t.lines().any(|l| l.starts_with("TY  - ")) => Some(Format::Ris),
        _ if t.contains("\n@") => Some(Format::Bibtex),
        _ => None,
    }
}

/// The entries of `text` in its format, else none.
pub fn parse(text: &str) -> Vec<Entry> {
    let mut out = match detect(text) {
        Some(Format::Bibtex) => entries(text),
        Some(Format::Ris) => ris(text),
        Some(Format::Refer) => refer(text),
        Some(Format::CslJson) => csl(text),
        Some(Format::Atom) => atom(text),
        Some(Format::Dblp) => dblp(text),
        None => Vec::new(),
    };
    for e in &mut out {
        tidy(e);
    }
    out
}

/// A field set once: the first value wins, empty values are none; `and`-lists grow.
fn put(e: &mut Entry, name: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    match e.fields.iter_mut().find(|f| f.0 == name) {
        Some(f) if matches!(name, "author" | "editor") => f.1 = format!("{} and {value}", f.1),
        Some(f) if name == "keywords" => f.1 = format!("{}, {value}", f.1),
        Some(_) => {}
        None => e.fields.push((name.to_string(), value.to_string())),
    }
}

/// Text as a field writes it: TeX's specials escaped, white space one space.
fn tex(text: &str) -> String {
    safe(&text.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The derived parts of an entry: title, author, year as completion shows them; a range
/// written with `-` or an en dash as BibTeX's `--`.
fn tidy(e: &mut Entry) {
    for (name, value) in &mut e.fields {
        match name.as_str() {
            "pages" => {
                let v = value.replace('–', "--");
                *value = if v.contains("--") { v } else { v.replacen('-', "--", 1) };
            }
            "title" => e.title = value.replace(['{', '}'], ""),
            "author" => e.author = value.clone(),
            "year" => e.year = value.clone(),
            _ => {}
        }
    }
}

/// RIS (Research Information Systems): `TY  - JOUR` … `ER  -`, one tag a line.
fn ris(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut e: Option<(String, Entry)> = None;
    for line in text.lines() {
        let Some((tag, value)) = line.split_once("  -").map(|(t, v)| (t.trim(), v.trim())) else { continue };
        if tag == "TY" {
            let kind = match value {
                "JOUR" | "JFULL" | "MGZN" | "NEWS" | "EJOUR" => "article",
                "CONF" | "CPAPER" => "inproceedings",
                "BOOK" | "EBOOK" | "EDBOOK" => "book",
                "CHAP" | "ECHAP" => "incollection",
                "THES" => "phdthesis",
                "RPRT" => "techreport",
                "UNPB" => "unpublished",
                _ => "misc",
            };
            e = Some((value.to_string(), Entry { kind: kind.into(), ..Entry::default() }));
            continue;
        }
        let Some((ty, entry)) = e.as_mut() else { continue };
        let container = if matches!(entry.kind.as_str(), "article") { "journal" } else { "booktitle" };
        let v = tex(value);
        match tag {
            "ER" => out.extend(e.take().map(|(_, x)| x)),
            "TI" | "T1" | "CT" => put(entry, "title", &v),
            "T2" | "JO" | "JF" | "JA" | "BT" if ty != "BOOK" => put(entry, container, &v),
            "AU" | "A1" => put(entry, "author", &v),
            "ED" | "A2" | "A3" => put(entry, "editor", &v),
            "PY" | "Y1" => put(entry, "year", &value.chars().take(4).collect::<String>()),
            "DA" => {
                let m = value.split('/').nth(1).unwrap_or_default();
                put(entry, "month", m.trim_start_matches('0'));
            }
            "SP" => put(entry, "pages", &v),
            "EP" => {
                if let Some(p) = entry.fields.iter_mut().find(|f| f.0 == "pages" && !f.1.contains('-')) {
                    p.1 = format!("{}--{v}", p.1);
                }
            }
            "VL" => put(entry, "volume", &v),
            "IS" => put(entry, "number", &v),
            "PB" => put(entry, if ty == "THES" { "school" } else { "publisher" }, &v),
            "CY" | "PP" => put(entry, "address", &v),
            "SN" => {
                put(entry, if matches!(entry.kind.as_str(), "book" | "incollection") { "isbn" } else { "issn" }, &v)
            }
            "DO" => put(entry, "doi", value),
            "UR" | "L2" => put(entry, "url", value),
            "AB" | "N2" => put(entry, "abstract", &v),
            "KW" => put(entry, "keywords", &v),
            "N1" => put(entry, "note", &v),
            "ID" => entry.key = value.to_string(),
            _ => {}
        }
    }
    out
}

/// EndNote's refer format (`.enw`): `%0 Journal Article`, `%A Last, First`, ….
fn refer(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut e: Option<Entry> = None;
    for line in text.lines().chain(std::iter::once("")) {
        let Some((tag, value)) = line.strip_prefix('%').and_then(|l| l.split_at_checked(1)) else {
            // a blank line ends a record
            if line.trim().is_empty() {
                out.extend(e.take());
            }
            continue;
        };
        let (value, v) = (value.trim(), tex(value));
        if tag == "0" {
            out.extend(e.take());
            let kind = match value {
                "Journal Article" | "Magazine Article" | "Newspaper Article" => "article",
                "Conference Proceedings" | "Conference Paper" => "inproceedings",
                "Book" | "Edited Book" => "book",
                "Book Section" => "incollection",
                "Thesis" => "phdthesis",
                "Report" => "techreport",
                "Unpublished Work" => "unpublished",
                _ => "misc",
            };
            e = Some(Entry { kind: kind.into(), ..Entry::default() });
            continue;
        }
        let entry = e.get_or_insert_with(|| Entry { kind: "misc".into(), ..Entry::default() });
        let container = if entry.kind == "article" { "journal" } else { "booktitle" };
        match tag {
            "T" => put(entry, "title", &v),
            "J" => put(entry, "journal", &v),
            "B" => put(entry, container, &v),
            "A" => put(entry, "author", &v),
            "E" => put(entry, "editor", &v),
            "D" => put(entry, "year", &value.chars().take(4).collect::<String>()),
            "P" => put(entry, "pages", &v),
            "V" => put(entry, "volume", &v),
            "N" => put(entry, "number", &v),
            "I" => put(entry, if entry.kind == "phdthesis" { "school" } else { "publisher" }, &v),
            "C" => put(entry, "address", &v),
            "@" => put(entry, "isbn", &v),
            "R" => put(entry, "doi", value),
            "U" => put(entry, "url", value),
            "X" => put(entry, "abstract", &v),
            "K" => put(entry, "keywords", &v),
            "Z" => put(entry, "note", &v),
            "F" => entry.key = value.to_string(),
            _ => {}
        }
    }
    out
}

/// CSL-JSON: an array of items (Zotero's export, Better BibTeX's with the key as `id`), or one
/// item (doi.org's answer).
fn csl(text: &str) -> Vec<Entry> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let items = match &json {
        serde_json::Value::Array(a) => a.clone(),
        one => vec![one.clone()],
    };
    items.iter().map(csl_item).collect()
}

fn csl_item(item: &serde_json::Value) -> Entry {
    let s = |k: &str| match &item[k] {
        serde_json::Value::String(v) => tex(&jats(v)),
        serde_json::Value::Number(n) => n.to_string(),
        // Crossref gives some titles as lists
        serde_json::Value::Array(a) => a.first().and_then(|v| v.as_str()).map(|v| tex(&jats(v))).unwrap_or_default(),
        _ => String::new(),
    };
    let kind = match item["type"].as_str().unwrap_or_default() {
        "article-journal" | "article-magazine" | "article-newspaper" | "journal-article" => "article",
        "paper-conference" | "proceedings-article" => "inproceedings",
        "book" | "monograph" | "edited-book" => "book",
        "chapter" | "book-chapter" => "incollection",
        "thesis" | "dissertation" => "phdthesis",
        "report" => "techreport",
        "manuscript" => "unpublished",
        _ => "misc",
    };
    let mut e = Entry { kind: kind.into(), ..Entry::default() };
    // Better BibTeX's CSL keeps the citation key as `id`; Crossref's ids are none
    if let Some(id) = item["id"].as_str().filter(|i| !i.contains(['/', ' ']) && !i.is_empty()) {
        e.key = id.to_string();
    }
    put(&mut e, "title", &s("title"));
    put(&mut e, if kind == "article" { "journal" } else { "booktitle" }, &s("container-title"));
    for role in ["author", "editor"] {
        for n in item[role].as_array().into_iter().flatten() {
            let name = match (n["family"].as_str(), n["given"].as_str(), n["literal"].as_str()) {
                (Some(f), Some(g), _) => format!("{f}, {g}"),
                (Some(f), None, _) => f.to_string(),
                (_, _, Some(l)) => format!("{{{l}}}"),
                _ => continue,
            };
            put(&mut e, role, &tex(&name));
        }
    }
    let date = ["issued", "published-print", "published-online", "published"]
        .iter()
        .find_map(|k| item[k]["date-parts"][0].as_array());
    if let Some(parts) = date {
        put(&mut e, "year", &parts.first().map(|v| v.to_string()).unwrap_or_default());
        put(&mut e, "month", &parts.get(1).map(|v| v.to_string()).unwrap_or_default());
    }
    for (from, to) in [("page", "pages"), ("volume", "volume"), ("issue", "number"), ("publisher", "publisher")] {
        put(&mut e, to, &s(from));
    }
    put(&mut e, "address", &s("publisher-place"));
    put(&mut e, "doi", item["DOI"].as_str().unwrap_or_default());
    put(&mut e, "url", item["URL"].as_str().unwrap_or_default());
    put(&mut e, "isbn", &s("ISBN"));
    put(&mut e, "issn", &s("ISSN"));
    put(&mut e, "abstract", &s("abstract"));
    e
}

/// Text without the JATS/HTML markup Crossref wraps abstracts and titles in.
fn jats(text: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in text.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

/// The texts of each `<tag …>…</tag>` in `xml`.
fn elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        // `<tag>` or `<tag attr…>`, not `<tagger>`
        if !after.starts_with(['>', ' ', '/']) {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            out.push(&after[..gt]);
            rest = &after[gt..];
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find(&close) else { break };
        out.push(&body[..end]);
        rest = &body[end..];
    }
    out
}

/// arXiv's API answer (Atom): each `<entry>` an e-print.
fn atom(text: &str) -> Vec<Entry> {
    elements(text, "entry")
        .into_iter()
        .map(|x| {
            let one = |t: &str| elements(x, t).first().map(|v| tex(&jats(v))).unwrap_or_default();
            let mut e = Entry { kind: "misc".into(), ..Entry::default() };
            put(&mut e, "title", &one("title"));
            for a in elements(x, "author") {
                let name = elements(a, "name").first().map(|n| jats(n)).unwrap_or_default();
                // `First Last` as `Last, First`
                let name = match name.trim().rsplit_once(' ') {
                    Some((given, family)) => format!("{family}, {given}"),
                    None => name,
                };
                put(&mut e, "author", &tex(&name));
            }
            put(&mut e, "year", &one("published").chars().take(4).collect::<String>());
            let id = one("id");
            let number = id.rsplit("/abs/").next().unwrap_or_default();
            let number = number
                .rsplit_once('v')
                .filter(|(_, v)| v.chars().all(|c| c.is_ascii_digit()))
                .map_or(number, |(n, _)| n);
            put(&mut e, "eprint", number);
            put(&mut e, "archiveprefix", "arXiv");
            if let Some(class) = x.split("<arxiv:primary_category").nth(1).and_then(|r| r.split("term=\"").nth(1)) {
                put(&mut e, "primaryclass", class.split('"').next().unwrap_or_default());
            }
            put(&mut e, "doi", &one("arxiv:doi"));
            put(&mut e, "url", &format!("https://arxiv.org/abs/{number}"));
            put(&mut e, "abstract", &one("summary"));
            e
        })
        .collect()
}

/// dblp's search answer (`/search/publ/api?format=json`): each hit's record, keyed as dblp keys it.
fn dblp(text: &str) -> Vec<Entry> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let hits = json["result"]["hits"]["hit"].as_array().cloned().unwrap_or_default();
    hits.iter()
        .map(|h| {
            let info = &h["info"];
            let s = |k: &str| info[k].as_str().map(|v| tex(&jats(v))).unwrap_or_default();
            let kind = match info["type"].as_str().unwrap_or_default() {
                "Journal Articles" => "article",
                "Conference and Workshop Papers" => "inproceedings",
                "Parts in Books or Collections" => "incollection",
                "Books and Theses" => "book",
                _ => "misc",
            };
            let mut e = Entry { kind: kind.into(), key: format!("DBLP:{}", s("key")), ..Entry::default() };
            put(&mut e, "title", s("title").trim_end_matches('.'));
            let authors = match &info["authors"]["author"] {
                serde_json::Value::Array(a) => a.clone(),
                one => vec![one.clone()],
            };
            for a in authors {
                // dblp numbers homonyms: `Jane Doe 0001`
                let name = a["text"].as_str().unwrap_or_default();
                let name = name.trim_end_matches(|c: char| c.is_ascii_digit()).trim();
                put(&mut e, "author", &tex(name));
            }
            put(&mut e, if kind == "article" { "journal" } else { "booktitle" }, &s("venue"));
            for f in ["volume", "number", "pages", "year", "doi"] {
                put(&mut e, f, &s(f));
            }
            put(&mut e, "url", &s("ee"));
            e
        })
        .collect()
}

/// A DOI as written anywhere (`https://doi.org/10.…`, `doi:10.…`), lowercase, else none.
pub fn doi(text: &str) -> Option<String> {
    let at = text.find("10.")?;
    let d = text[at..].trim().trim_end_matches(['.', ',', ';']);
    (d.contains('/') && !d.contains(char::is_whitespace)).then(|| d.to_lowercase())
}

/// A title as a match compares it: letters and digits, lowercase.
pub fn title_key(title: &str) -> String {
    title.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Whether two entries name one work: the same DOI, else the same title (and year when both
/// have one).
pub fn same(a: &Entry, b: &Entry) -> bool {
    let f = |e: &Entry, n: &str| e.fields.iter().find(|f| f.0 == n).map(|f| f.1.clone());
    match (f(a, "doi").and_then(|d| doi(&d)), f(b, "doi").and_then(|d| doi(&d))) {
        (Some(x), Some(y)) => x == y,
        _ => {
            let (x, y) = (title_key(&a.title), title_key(&b.title));
            x.len() > 8 && x == y && (a.year.is_empty() || b.year.is_empty() || a.year == b.year)
        }
    }
}

/// A key for `e` none of `taken` has: the first author's family name, the year and the
/// title's first long word (`knuth1984literate`), then `a`, `b`, … .
pub fn key_for(e: &Entry, taken: &[String]) -> String {
    let ascii =
        |s: &str| s.chars().filter(char::is_ascii_alphanumeric).flat_map(|c| c.to_lowercase()).collect::<String>();
    let who = e.fields.iter().find(|f| matches!(f.0.as_str(), "author" | "editor")).map(|f| f.1.as_str());
    let family = who.and_then(|w| super::names(w).into_iter().next()).map(|(f, _)| ascii(&f)).unwrap_or_default();
    let word = e.title.split_whitespace().map(ascii).find(|w| w.len() > 3).unwrap_or_default();
    let base = format!("{}{}{word}", if family.is_empty() { "anon".into() } else { family }, ascii(&e.year));
    let mut key = base.clone();
    for c in 'a'..='z' {
        if !taken.iter().any(|t| t.eq_ignore_ascii_case(&key)) {
            break;
        }
        key = format!("{base}{c}");
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/bib/{name}", env!("CARGO_MANIFEST_DIR"))).expect(name)
    }

    fn field<'a>(e: &'a Entry, n: &str) -> &'a str {
        e.fields.iter().find(|f| f.0 == n).map_or("", |f| f.1.as_str())
    }

    /// One article as doi.org answers in BibTeX, RIS and CSL-JSON: the same fields each time.
    #[test]
    fn three_formats_of_one_doi_agree() {
        let all: Vec<Entry> = ["acm.bib", "acm.ris", "acm.json"].iter().map(|f| parse(&fixture(f)).remove(0)).collect();
        for e in &all {
            assert_eq!(e.kind, "article", "{e:?}");
            assert_eq!(field(e, "journal"), "Proceedings of the ACM on Programming Languages", "{e:?}");
            assert_eq!(field(e, "doi").to_lowercase(), "10.1145/3360588", "{e:?}");
            assert_eq!(field(e, "pages"), "1--30", "{e:?}");
            assert_eq!(e.year, "2019");
            assert!(e.author.starts_with("Li, Yi and Wang, Shaohua"), "{e:?}");
            assert_eq!(key_for(e, &[]), "li2019improving");
        }
        assert!(same(&all[0], &all[1]) && same(&all[1], &all[2]));
        // doi.org's BibTeX writes the month as a constant
        assert!(all[0].bare.contains(&"month".to_string()), "{:?}", all[0]);
        assert_eq!(field(&all[2], "issn"), "2475-1421");
    }

    #[test]
    fn arxiv_atom_and_datacite_name_one_eprint() {
        let atom = parse(&fixture("arxiv.atom")).remove(0);
        let cite = parse(&fixture("arxiv-doi.bib")).remove(0);
        assert_eq!(field(&atom, "eprint"), "1706.03762");
        assert_eq!(field(&atom, "archiveprefix"), "arXiv");
        assert_eq!(field(&atom, "primaryclass"), "cs.CL");
        assert!(atom.author.starts_with("Vaswani, Ashish and Shazeer, Noam"), "{atom:?}");
        assert_eq!(key_for(&atom, &[]), "vaswani2017attention");
        assert!(same(&atom, &cite), "the same title and year");
        assert_eq!(key_for(&atom, &["vaswani2017attention".into()]), "vaswani2017attentiona");
        let ris = parse(&fixture("arxiv-doi.ris")).remove(0);
        assert!(same(&ris, &cite), "{ris:?}");
    }

    #[test]
    fn refer_zotero_csl_and_dblp_read() {
        let e = parse(&fixture("knuth.enw")).remove(0);
        assert_eq!(
            (e.kind.as_str(), field(&e, "journal"), field(&e, "pages")),
            ("article", "The Computer Journal", "97--111")
        );
        assert_eq!(e.author, "Knuth, Donald E.");
        let z = parse(&fixture("zotero.json"));
        assert_eq!(z[0].key, "knuth1984literate", "Better BibTeX keeps the key as id");
        assert_eq!(field(&z[0], "journal"), "The Computer Journal");
        assert_eq!(field(&z[1], "title"), "Typesetting \\& layout: 50\\% faster");
        let d = parse(&fixture("dblp.json"));
        assert_eq!(d[0].key, "DBLP:journals/cj/Knuth84");
        assert_eq!(d[0].author, "Donald E. Knuth");
        assert_eq!(field(&d[0], "pages"), "97--111");
        assert!(same(&d[0], &e));
        let b = parse(&fixture("zotero.bib"));
        assert_eq!(b.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["knuth1984literate", "lamport1994latex"]);
    }
}
