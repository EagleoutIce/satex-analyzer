//! The rules on `.bib` entries, against what BibTeX 0.99d (`plain.bst`) and biber 2.21
//! (`--validate-datamodel`) say of the same entries. Each test quotes the tool's message.

use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};
use satex::queries::query::Record;

/// The findings on `refs.bib` = `bib`, read by BibTeX with `plain.bst` or by biber with
/// biblatex. `body` is the citing text, a `\cite` per line. Empty without a TeX installation.
fn lint(name: &str, biblatex: bool, body: &str, bib: &str) -> Vec<Record> {
    if !crate::support::installed() {
        return Vec::new();
    }
    let dir = crate::support::tmp_root().join(format!("satex-bibdb-{name}-{biblatex}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.tex");
    let source = match biblatex {
        true => format!(
            "\\documentclass{{article}}\n\\usepackage{{biblatex}}\n\\addbibresource{{refs.bib}}\n\\begin{{document}}\n{body}\n\\printbibliography\n\\end{{document}}\n"
        ),
        false => format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\bibliographystyle{{plain}}\n\\bibliography{{refs}}\n\\end{{document}}\n"
        ),
    };
    std::fs::write(&path, &source).unwrap();
    std::fs::write(dir.join("refs.bib"), bib).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    let out = satex::linter::lint(&analysis);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// A `\cite` per line for each key: each entry's findings at a site of their own.
fn cite(keys: &[&str]) -> String {
    keys.iter().map(|k| format!("\\cite{{{k}}}\n")).collect()
}

/// The messages of `code`, each without its `` `key` (refs.bib:n) `` head: (key, what is said).
fn said(found: &[Record], code: &str) -> Vec<(String, String)> {
    let of = |r: &&Record| r["code"] == code;
    let split = |r: &Record| {
        let m = r["message"].as_str().unwrap_or_default().to_string();
        let (head, rest) = m.split_once(") ").unwrap_or(("", &m));
        (head.split('`').nth(1).unwrap_or_default().to_string(), rest.to_string())
    };
    found.iter().filter(of).map(split).collect()
}

fn keys(found: &[Record], code: &str) -> Vec<String> {
    said(found, code).into_iter().map(|s| s.0).collect()
}

/// `bib` after the edits of `code`'s findings, each with the applicability it claims.
fn fixed(found: &[Record], code: &str, bib: &str) -> (String, Vec<String>) {
    let offset = |p: &serde_json::Value| {
        let (line, col) = (p["line"].as_u64().unwrap() as usize, p["col"].as_u64().unwrap() as usize);
        let start: usize = bib.split_inclusive('\n').take(line - 1).map(str::len).sum();
        start + bib[start..].char_indices().nth(col - 1).map_or(bib.len() - start, |c| c.0)
    };
    let of = |r: &&Record| r["code"] == code && r.contains_key("edits");
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for e in found.iter().filter(of).flat_map(|r| r["edits"].as_array().cloned().unwrap_or_default()) {
        edits.push((offset(&e["start"]), offset(&e["end"]), e["replacement"].as_str().unwrap().to_string()));
    }
    edits.sort();
    let mut out = bib.to_string();
    for (from, to, text) in edits.into_iter().rev() {
        out.replace_range(from..to, &text);
    }
    let claims = found.iter().filter(of).map(|r| r["applicability"].as_str().unwrap_or_default().to_string());
    (out, claims.collect())
}

const MISSING: &str = "bibliography-entry-missing-field";

/// Intuition: what a type requires is what its reader checks, under the names it reads.
/// BibTeX: `Warning--empty year in noyear`, `empty year in emptyyear` (an empty field is
/// missing), `empty author and editor in bk`, `empty school in th`, and `empty journal`
/// where the entry writes biblatex's `journaltitle`. Pins the `journal` alias counted for
/// BibTeX, which renames nothing.
#[test]
fn bibtex_requires_what_the_bst_checks() {
    let bib = "@article{noyear, author={A. Uthor}, title={T}, journal={J}}\n\
               @article{emptyyear, author={A. Uthor}, title={T}, journal={J}, year={}}\n\
               @book{bk, title={T}, publisher={P}, year=2000}\n\
               @phdthesis{th, author={A}, title={T}, year=2000}\n\
               @article{jt, author={A. Uthor}, title={T}, journaltitle={J}, year=2000}\n\
               @article{fine, author={A. Uthor}, title={T}, journal={J}, year=2000}\n";
    let found = lint("bst-missing", false, &cite(&["noyear", "emptyyear", "bk", "th", "jt", "fine"]), bib);
    if found.is_empty() {
        return;
    }
    let lacks: Vec<(String, String)> = said(&found, MISSING);
    let of = |k: &str| lacks.iter().find(|l| l.0 == k).map(|l| l.1.as_str()).unwrap_or_default().to_string();
    assert_eq!(keys(&found, MISSING), ["noyear", "emptyyear", "bk", "th", "jt"]);
    assert_eq!(of("noyear"), "lacks `year`, required for `@article`");
    assert_eq!(of("bk"), "lacks `author or editor`, required for `@book`");
    assert_eq!(of("th"), "lacks `school`, required for `@phdthesis`");
    assert_eq!(of("jt"), "lacks `journal`, required for `@article`");
}

/// Intuition: biber validates what it has read, after its renames. biber:
/// `Missing mandatory field - one of 'date, year' must be defined`, `Missing mandatory
/// field 'institution'` for a `@phdthesis` and a `@techreport` (never `type`, which biber
/// sets itself), `'author'` for a `@book` with an editor, `one of 'url, doi, eprint'` for
/// an `@online`. `school` is `institution`, `@www` is `@online`. Pins `type` reported for
/// every `@phdthesis`, and a `@set` asked for a year its members carry.
#[test]
fn biber_requires_what_the_data_model_declares() {
    let bib = "@article{noyear, author={A. Uthor}, title={T}, journal={J}}\n\
               @phdthesis{th, author={A}, title={T}, year=2000}\n\
               @phdthesis{th2, author={A}, title={T}, year=2000, school={S}}\n\
               @techreport{tr, author={A}, title={T}, year=2000}\n\
               @book{edt, editor={A. Uthor}, title={T}, year={2001}, publisher={P}}\n\
               @www{web, author={A. Uthor}, title={T}, year={2001}}\n\
               @set{both, entryset={noyear,th2}}\n";
    let found = lint("bl-missing", true, &cite(&["noyear", "th", "th2", "tr", "edt", "web", "both"]), bib);
    if found.is_empty() {
        return;
    }
    let lacks = said(&found, MISSING);
    let all: Vec<(&str, &str)> = lacks.iter().map(|l| (l.0.as_str(), l.1.as_str())).collect();
    assert_eq!(
        all,
        [
            ("noyear", "lacks `date or year`, required for `@article`"),
            ("th", "lacks `institution`, required for `@phdthesis`"),
            ("tr", "lacks `institution`, required for `@techreport`"),
            ("edt", "lacks `author`, required for `@book`"),
            ("web", "lacks `url or doi or eprint`, required for `@www`"),
        ]
    );
}

/// Intuition: a `crossref` parent hands its fields down, by each reader's rules. BibTeX
/// copies every field and skips the checks of its `crossref missing$` branch: no warning
/// for `ckid`. biber renames by type: the parent's `title` arrives as `booktitle`, so
/// `Missing mandatory field 'title'` for `ckid` and nothing else. `xref` hands down nothing
/// with either (`empty booktitle in xkid`, `empty year in xkid`), and a parent that is
/// not there leaves the child as it is (`empty journal in orphan`). Pins every entry with
/// `crossref` or `xref` going unchecked.
#[test]
fn a_crossref_parent_hands_fields_down() {
    let bib = "@inproceedings{ckid, author={A. Uthor}, crossref={cparent}}\n\
               @inproceedings{xkid, author={A. Uthor}, title={T}, xref={cparent}}\n\
               @article{orphan, author={A. Uthor}, title={T}, year=2000, crossref={nosuch}}\n\
               @proceedings{cparent, title={P}, year=2000, editor={E. D.}}\n";
    let body = cite(&["ckid", "xkid", "orphan"]);
    let (bst, bl) = (lint("crossref", false, &body, bib), lint("crossref", true, &body, bib));
    if bst.is_empty() {
        return;
    }
    let pairs = |found: &[Record]| said(found, MISSING);
    let own = |k: &str, what: &str| (k.to_string(), what.to_string());
    assert_eq!(
        pairs(&bst),
        [
            own("xkid", "lacks `booktitle`, `year`, required for `@inproceedings`"),
            own("orphan", "lacks `journal`, required for `@article`"),
        ]
    );
    assert_eq!(
        pairs(&bl),
        [
            own("ckid", "lacks `title`, required for `@inproceedings`"),
            own("xkid", "lacks `date or year`, `booktitle`, required for `@inproceedings`"),
            own("orphan", "lacks `journaltitle`, required for `@article`"),
        ]
    );
}

/// Intuition: a site keeps one finding per rule, so the entries of one `\cite{a,b}` share
/// one finding that names each. Pins the second entry's finding lost.
#[test]
fn one_cite_of_two_broken_entries_names_both() {
    let bib = "@article{a, author={A. Uthor}, title={T}, journal={J}}\n\
               @article{b, author={A. Uthor}, title={T}, year=2000}\n";
    let found = lint("two-keys", false, "\\cite{a,b}\n", bib);
    if found.is_empty() {
        return;
    }
    let lacks: Vec<&Record> = found.iter().filter(|r| r["code"] == MISSING).collect();
    assert_eq!(lacks.len(), 1, "{lacks:?}");
    let message = lacks[0]["message"].as_str().unwrap();
    assert_eq!(
        message,
        "`a` (refs.bib:1) lacks `year`, required for `@article`. `b` (refs.bib:2) lacks `journal`, required for `@article`"
    );
}

/// Intuition: an entry that breaks the grammar loses what follows the break, cited or not.
/// BibTeX: `I was expecting a `,' or a `}'---line 2`, then `line 4` for the brace that
/// never closes (it takes the entry's own close, so the reader gives up at the next `@`).
/// biber: `syntax error: found "title", expected end of entry`. A lone key is fine to
/// BibTeX and `syntax error: found "}", expected ","` to biber: the fix adds the comma.
#[test]
fn a_malformed_entry_is_reported_where_the_reader_gives_up() {
    let bib = "@article{ok, author={A. Uthor}, title={T}, journal={J}, year=2001}\n\
               @article{nocomma, author={A. Uthor} title={T}, journal={J}, year=2001}\n\
               @article{open, author={A. {Uthor}, title={T}, journal={J}, year=2001}\n\
               @article{next, author={A. Uthor}, title={T}, journal={J}, year=2001}\n\
               @misc{lone}\n";
    let code = "malformed-bibliography-entry";
    let (bst, bl) = (lint("malformed", false, &cite(&["ok"]), bib), lint("malformed", true, &cite(&["ok"]), bib));
    if bst.is_empty() {
        return;
    }
    let message = |found: &[Record]| {
        found.iter().find(|r| r["code"] == code).map(|r| r["message"].as_str().unwrap().to_string()).unwrap_or_default()
    };
    assert_eq!(
        message(&bst),
        "`nocomma` (refs.bib:2) breaks off at line 2, where BibTeX expects `,` or the entry's close. \
         `open` (refs.bib:3) breaks off at line 4, where BibTeX expects `,` or the entry's close"
    );
    assert!(message(&bl).ends_with("`lone` (refs.bib:5) has no comma after its key, which biber takes as an error"));
    assert!(message(&bl).contains("where biber expects"), "{}", message(&bl));
    assert!(bst.iter().chain(&bl).filter(|r| r["code"] == code).all(|r| r["severity"] == "error"));
    let (after, claims) = fixed(&bl, code, bib);
    assert!(after.ends_with("@misc{lone,}\n"), "{after}");
    assert_eq!(claims, ["safe"]);
}

/// Intuition: of a field given twice one is read, and which one depends on the reader.
/// BibTeX: `Warning--I'm ignoring dup's extra "title" field` (the first is printed). biber
/// prints the last and says nothing. The fix deletes the one not read, so the output stays.
#[test]
fn a_field_given_twice_keeps_the_one_its_reader_reads() {
    let bib = "@article{dup,\n  author = {A. Uthor},\n  title = {First},\n  journal = {J},\n  year = 2001,\n  title = {Second}\n}\n";
    let code = "duplicate-bibliography-field";
    let (bst, bl) = (lint("dupfield", false, &cite(&["dup"]), bib), lint("dupfield", true, &cite(&["dup"]), bib));
    if bst.is_empty() {
        return;
    }
    assert_eq!(said(&bst, code), [("dup".to_string(), "gives `title` twice. BibTeX reads the first".to_string())]);
    assert_eq!(said(&bl, code), [("dup".to_string(), "gives `title` twice. biber reads the last".to_string())]);
    let kept_first = "@article{dup,\n  author = {A. Uthor},\n  title = {First},\n  journal = {J},\n  year = 2001\n}\n";
    let kept_last = "@article{dup,\n  author = {A. Uthor},\n  journal = {J},\n  year = 2001,\n  title = {Second}\n}\n";
    assert_eq!(fixed(&bst, code, bib), (kept_first.to_string(), vec!["safe".to_string()]));
    assert_eq!(fixed(&bl, code, bib), (kept_last.to_string(), vec!["safe".to_string()]));
}

/// Intuition: a name list is judged where its reader parses it. BibTeX: `Too many commas
/// in name 1 of "Last, First, Jr, X"`, `Name 2 in "…" has a comma at the end`. An `and`
/// at an edge is printed as a name (`A. Uthor, {and}, and B. Other and.`). biber adds
/// `Name "Uthor,, A." is malformed (consecutive commas)`. BibTeX parses no `editor` of an
/// `@article`: silence there. The fix drops stray `and`s and closing commas only.
#[test]
fn name_lists_are_judged_where_they_are_parsed() {
    let bib = "@article{commas, author={Last, First, Jr, X}, title={T}, journal={J}, year=2001}\n\
               @article{trail, author={Uthor, A. and Other, B.,}, title={T}, journal={J}, year=2001}\n\
               @article{ands, author={and A. Uthor and and B. Other and}, title={T}, journal={J}, year=2001}\n\
               @article{row, author={Uthor,, A.}, title={T}, journal={J}, year=2001}\n\
               @article{ed, author={A. Uthor}, editor={Last, First, Jr, X}, title={T}, journal={J}, year=2001}\n\
               @article{fine, author={Knuth, Donald E. and {Smith and Sons} and others}, title={T}, journal={J}, year=2001}\n";
    let code = "invalid-bibliography-field-value";
    let body = cite(&["commas", "trail", "ands", "row", "ed", "fine"]);
    let (bst, bl) = (lint("names", false, &body, bib), lint("names", true, &body, bib));
    if bst.is_empty() {
        return;
    }
    assert_eq!(keys(&bst, code), ["commas", "trail", "ands"]);
    assert_eq!(keys(&bl, code), ["commas", "trail", "ands", "row", "ed"]);
    let what = said(&bst, code);
    assert!(what[0].1.ends_with("where the name `Last, First, Jr, X` has more than two commas"), "{what:?}");
    assert!(what[1].1.ends_with("where the name `Other, B.,` ends in a comma"), "{what:?}");
    assert!(what[2].1.ends_with("where an `and` joins no name"), "{what:?}");
    let (after, _) = fixed(&bst, code, bib);
    assert!(after.contains("author={Uthor, A. and Other, B.}") && after.contains("author={A. Uthor and B. Other}"));
    assert!(after.contains("author={Last, First, Jr, X}"), "{after}");
}

/// Intuition: with biblatex a value must fit its field's datatype, as the data model
/// declares it. biber: `Invalid format 'Spring 2001' of date field 'date'`, `legacy year
/// field 'in press' … is not an integer`, `legacy month field 'jan' … is not an integer`,
/// `ISBN '0-306-40615-3' … is invalid`. Under `--validate-datamodel` only (a note):
/// `Invalid value of field 'volume' must be datatype 'integer'`, `Invalid ISSN`, `Invalid
/// value (pattern match fails) for field 'gender'`. A bare `jan` is the month. BibTeX
/// checks none of this: silence with `plain.bst`.
#[test]
fn biber_values_fit_their_datatypes() {
    let bib = "@article{v-date, author={A. Uthor}, title={T}, journal={J}, date={Spring 2001}}\n\
               @article{v-year, author={A. Uthor}, title={T}, journal={J}, year={in press}}\n\
               @article{v-month, author={A. Uthor}, title={T}, journal={J}, year={2001}, month={jan}}\n\
               @book{v-isbn, author={A. Uthor}, title={T}, year={2001}, isbn={0-306-40615-3}}\n\
               @book{v-volume, author={A. Uthor}, title={T}, year={2001}, volume={IV}}\n\
               @article{v-issn, author={A. Uthor}, title={T}, journal={J}, year={2001}, issn={12}}\n\
               @article{v-gender, author={A. Uthor}, title={T}, journal={J}, year={2001}, gender={xx}}\n\
               @book{v-fine, author={A. Uthor}, title={T}, date={2001-05-21/2001-05-23}, month=jan, volume={4},\n  isbn={978-3-16-148410-0}, issn={0317-8471}, gender={sf}, urldate={2001?}, pages={12-}}\n";
    let code = "invalid-bibliography-field-value";
    let body = cite(&["v-date", "v-year", "v-month", "v-isbn", "v-volume", "v-issn", "v-gender", "v-fine"]);
    let (bst, bl) = (lint("values", false, &body, bib), lint("values", true, &body, bib));
    if bst.is_empty() {
        return;
    }
    assert_eq!(keys(&bst, code), Vec::<String>::new());
    let severity = |k: &str| {
        let of = |r: &&Record| r["code"] == code && r["name"] == k;
        bl.iter().find(of).map(|r| r["severity"].as_str().unwrap().to_string()).unwrap_or_default()
    };
    let severities: Vec<String> =
        ["v-date", "v-year", "v-month", "v-isbn", "v-volume", "v-issn", "v-gender", "v-fine"].map(severity).to_vec();
    assert_eq!(severities, ["warning", "warning", "warning", "warning", "info", "info", "info", ""]);
    let what = said(&bl, code);
    assert_eq!(what[0].1, "has the `date` `Spring 2001`, which biber cannot read as a date");
    assert_eq!(what[1].1, "has the `year` `in press`, which is no number");
    assert_eq!(what[3].1, "has the `isbn` `0-306-40615-3`, which is no valid ISBN");
    assert_eq!(what[6].1, "has the `gender` `xx`, which the data model's pattern `(?:sf|sm|sn|pf|pm|pn|pp)` rejects");
    // the month constant loses its braces
    let (after, claims) = fixed(&bl, code, bib);
    assert!(after.contains("year={2001}, month=jan}") && claims == ["safe"], "{after} {claims:?}");
}

/// Intuition: fields that say one thing twice, by the style's own pairs. BibTeX: `can't
/// use both author and editor fields in bk`, `can't use both volume and number fields in
/// vn`, and neither for an entry with `crossref` (the check is in the other branch). biber:
/// `Field 'journal' invalid in data model - ignoring` beside `journaltitle`, and
/// `Overwriting field 'year' with year value from field 'date'` where they differ. The
/// same year twice is no conflict.
#[test]
fn fields_that_exclude_each_other() {
    let bib = "@book{bk, author={A}, editor={B}, title={T}, publisher={P}, year=2000}\n\
               @book{vn, author={A}, title={T}, publisher={P}, year=2000, volume={1}, number={2}}\n\
               @article{jj, author={A. Uthor}, title={T}, journal={J}, journaltitle={J}, year={2001}}\n\
               @article{yd, author={A. Uthor}, title={T}, journal={J}, year={2001}, date={2002-01}}\n\
               @article{same, author={A. Uthor}, title={T}, journal={J}, year={2001}, date={2001-05}}\n";
    let code = "conflicting-bibliography-fields";
    let body = cite(&["bk", "vn", "jj", "yd", "same"]);
    let (bst, bl) = (lint("conflicts", false, &body, bib), lint("conflicts", true, &body, bib));
    if bst.is_empty() {
        return;
    }
    let own = |k: &str, what: &str| (k.to_string(), what.to_string());
    assert_eq!(
        said(&bst, code),
        [
            own("bk", "gives both `author` and `editor`. The style takes one"),
            own("vn", "gives both `volume` and `number`. The style takes one"),
        ]
    );
    assert_eq!(
        said(&bl, code),
        [
            own("jj", "gives both `journal` and `journaltitle`. biber reads `journaltitle` and drops `journal`"),
            own("yd", "has the `year` `2001`, but its `date` says `2002`. biber takes the date's"),
        ]
    );
    let (after, claims) = fixed(&bl, code, bib);
    assert!(after.contains("@article{jj, author={A. Uthor}, title={T}, journaltitle={J}, year={2001}}"), "{after}");
    assert_eq!(claims, ["safe"]);
}

/// Intuition: a style reads a fixed set of fields per type, and what it knows but does not
/// read of a type is dropped in silence. biber: `Invalid field 'booktitle' for entrytype
/// 'article'`, `Invalid field 'isbn' for entrytype 'article'`. BibTeX says nothing and
/// prints nothing. A date is read as its parts, a parent's fields are its children's, and
/// a field no part of the style knows (`abstract` to `plain.bst`, `myfield`) is the
/// user's own. With BibTeX `journaltitle` is renamed to the `journal` the style reads.
#[test]
fn a_field_the_style_never_reads_of_the_type() {
    let bib = "@article{art, author={A. Uthor}, title={T}, journal={J}, year=2001, booktitle={B}, isbn={0-306-40615-2},\n  abstract={A}, myfield={x}, date={2001-05}, urldate={2001-05-06}, key={k}}\n\
               @article{jt, author={A. Uthor}, title={T}, journaltitle={J}, year=2001}\n\
               @inproceedings{kid, author={A. Uthor}, title={T}, crossref={parent}}\n\
               @proceedings{parent, title={P}, booktitle={P}, year=2000, editor={E. D.}}\n";
    let code = "ignored-bibliography-field";
    let body = cite(&["art", "jt", "kid", "parent"]);
    let (bst, bl) = (lint("ignored", false, &body, bib), lint("ignored", true, &body, bib));
    if bst.is_empty() {
        return;
    }
    let own = |k: &str, what: &str| (k.to_string(), what.to_string());
    // `isbn` is no field of plain.bst: not its to read
    assert_eq!(
        said(&bst, code),
        [
            own("art", "has `booktitle`, which the style never reads of an `@article`"),
            own("jt", "has `journaltitle`. The style reads `journal`"),
        ]
    );
    assert_eq!(
        said(&bl, code),
        [own(
            "art",
            "has `booktitle`, which the style never reads of an `@article`. `art` (refs.bib:1) has `isbn`, which the style never reads of an `@article`"
        )]
    );
    assert!(bst.iter().chain(&bl).filter(|r| r["code"] == code).all(|r| r["severity"] == "info"));
    let (after, claims) = fixed(&bst, code, bib);
    assert!(after.contains("@article{jt, author={A. Uthor}, title={T}, journal={J}, year=2001}"), "{after}");
    assert_eq!(claims, ["safe"]);
}

/// Intuition: a type is the style's to define. BibTeX: `Warning--entry type for "web"
/// isn't style-file defined` (plain.bst has no `@online`). biber reads `@www` as `@online`
/// and says `Invalid entry type 'frobnicate' - defaulting to 'misc'`.
#[test]
fn an_entry_type_the_style_does_not_define() {
    let bib = "@online{web, author={A. Uthor}, title={T}, year={2001}, url={http://example.org}}\n\
               @www{web2, author={A. Uthor}, title={T}, year={2001}, url={http://example.org}}\n\
               @frobnicate{unk, author={A. Uthor}, title={T}, year=2000}\n\
               @phdthesis{th, author={A}, title={T}, year=2000, school={S}}\n";
    let code = "unknown-bibliography-entry-type";
    let body = cite(&["web", "web2", "unk", "th"]);
    let (bst, bl) = (lint("types", false, &body, bib), lint("types", true, &body, bib));
    if bst.is_empty() {
        return;
    }
    assert_eq!(keys(&bst, code), ["web", "web2", "unk"]);
    assert_eq!(
        said(&bl, code),
        [("unk".to_string(), "is an `@frobnicate`, a type the style does not define".to_string())]
    );
}

/// Intuition: `crossref` must name an entry its reader can still find. Both: `A bad cross
/// reference---entry "orphan" refers to entry "nosuch", which doesn't exist`, biber's `I
/// didn't find a database entry for crossref 'nosuch'`. BibTeX reads once, in order: a
/// parent before its child and not cited itself is gone (`entry "late" refers to entry
/// "early", which doesn't exist`), fine once cited too. BibTeX folds the key's case, biber
/// does not (`crossref 'EARLY' in entry 'upper'`).
#[test]
fn a_crossref_names_an_entry_its_reader_finds() {
    let bib = "@proceedings{early, title={P}, booktitle={P}, year=2000, editor={E. D.}}\n\
               @inproceedings{late, author={A. Uthor}, title={T}, crossref={early}}\n\
               @inproceedings{upper, author={A. Uthor}, title={T}, crossref={LATER}}\n\
               @inproceedings{orphan, author={A. Uthor}, title={T}, crossref={nosuch}}\n\
               @proceedings{later, title={P}, booktitle={P}, year=2000, editor={E. D.}}\n";
    let code = "broken-bibliography-crossref";
    let body = cite(&["late", "upper", "orphan"]);
    let (bst, bl) = (lint("xref", false, &body, bib), lint("xref", true, &body, bib));
    if bst.is_empty() {
        return;
    }
    let own = |k: &str, what: &str| (k.to_string(), what.to_string());
    assert_eq!(
        said(&bst, code),
        [
            own("late", "inherits from `early`, which stands before it. BibTeX has passed it by then"),
            own("orphan", "names the `crossref` `nosuch`, which no database declares"),
        ]
    );
    assert_eq!(keys(&bl, code), ["upper", "orphan"]);
    // the parent cited too: BibTeX keeps it
    let both = lint("xref-cited", false, &cite(&["late", "early"]), bib);
    assert_eq!(keys(&both, code), Vec::<String>::new());
}

/// Intuition: a source map rewrites entries before biber judges them, so what the
/// database says is no longer what biber sees. Nothing of the data model is reported
/// then. Soundness: no finding biber would not give.
#[test]
fn a_source_map_keeps_the_data_model_rules_silent() {
    let bib = "@article{noyear, author={A. Uthor}, title={T}, journal={J}, booktitle={B}, volume={IV}}\n";
    let body = "\\cite{noyear}\n";
    let plain = lint("map-none", true, body, bib);
    if plain.is_empty() {
        return;
    }
    let map = "\\DeclareSourcemap{\\maps[datatype=bibtex]{\\map{\\step[fieldset=year, fieldvalue=2000]}}}\n";
    let dir = crate::support::tmp_root().join(format!("satex-bibdb-map-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = format!(
        "\\documentclass{{article}}\n\\usepackage{{biblatex}}\n{map}\\addbibresource{{refs.bib}}\n\\begin{{document}}\n{body}\\printbibliography\n\\end{{document}}\n"
    );
    std::fs::write(dir.join("doc.tex"), &source).unwrap();
    std::fs::write(dir.join("refs.bib"), bib).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let mapped = satex::linter::lint(&Machine::analyze(&source, Some(&dir.join("doc.tex")), &cfg));
    let _ = std::fs::remove_dir_all(&dir);
    let codes = [MISSING, "ignored-bibliography-field", "invalid-bibliography-field-value"];
    for code in codes {
        assert_eq!((keys(&plain, code), keys(&mapped, code)), (vec!["noyear".to_string()], Vec::new()), "{code}");
    }
}
