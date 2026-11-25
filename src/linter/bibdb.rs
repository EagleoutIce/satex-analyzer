//! The `.bib` databases a document loads, read as the bibliography tool reads them: which
//! calls load which file (twice is a slip), the entries and `@string`s each holds, and what
//! is wrong with them against the style's data model. The bibliography manager shows the
//! same findings as badges.

use std::collections::BTreeSet;
use std::rc::Rc;

use crate::analysis::machine::Analysis;
use crate::engine::builtins::OccKind;
use crate::engine::tex::Span;
use crate::linter::fix::{Applicability, Edit, Fix, Pos, Sources, Text};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};
use crate::plugins::bib::{Entry, convert, model::Model};

/// The calls that name databases: BibTeX's and biblatex's.
const LOADERS: [&str; 4] = ["bibliography", "addbibresource", "addglobalbib", "addsectionbib"];

/// One database name of a loading call: the call, the name's bytes in its argument, the file.
pub struct Load {
    pub span: Span,
    pub name: String,
    pub path: Option<String>,
    /// The call's file, its `\`, the argument's braces, and the name's bytes in it.
    pub file: String,
    pub call: (usize, usize),
    pub item: (usize, usize),
    /// How many names the call gives.
    pub names: usize,
}

/// A `@string` constant: its name, value and line.
pub struct Constant {
    pub name: String,
    pub value: String,
    pub line: u32,
}

/// A database the run loads: its path, text, entries and constants.
pub struct Db {
    pub path: String,
    pub text: Rc<Text>,
    pub entries: Vec<Entry>,
    pub constants: Vec<Constant>,
}

/// The databases with the style's model: built once per lint run ([`Report::bib`]).
pub struct Bib {
    pub loads: Vec<Load>,
    pub dbs: Vec<Db>,
    pub model: Model,
}

impl Bib {
    pub fn of(analysis: &Analysis, sources: &Sources) -> Bib {
        let (loads, dbs) = databases(analysis, sources);
        let main = analysis.file_name(analysis.main_file).to_string();
        let model = if dbs.is_empty() { Model::default() } else { crate::plugins::bib::model::of(analysis, &main) };
        Bib { loads, dbs, model }
    }
}

/// The entry `key` names, as the style's reader compares keys: BibTeX folds case.
fn entry<'a>(dbs: &'a [Db], model: &Model, key: &str) -> Option<&'a Entry> {
    let same = |e: &&Entry| if model.biblatex { e.key == key } else { e.key.eq_ignore_ascii_case(key) };
    dbs.iter().flat_map(|d| &d.entries).find(same)
}

/// The document's loading calls and the databases they read, each once.
pub fn databases(analysis: &Analysis, sources: &Sources) -> (Vec<Load>, Vec<Db>) {
    let mut loads: Vec<Load> = Vec::new();
    let mut seen = BTreeSet::new();
    for x in &analysis.facts.expansions {
        let name = analysis.interner.name(x.name);
        if !LOADERS.contains(&name) || !seen.insert(x.span) {
            continue;
        }
        let file = analysis.file_name(x.span.file).to_string();
        // the document's own calls, not a package's
        let kind = analysis.files.get(usize::from(x.span.file)).map(|f| f.kind);
        if kind.is_none_or(|k| {
            matches!(k, crate::engine::builtins::LoadKind::Package | crate::engine::builtins::LoadKind::Class)
        }) {
            continue;
        }
        let Some(text) = sources.get(&file) else { continue };
        let Some(at) = text.offset(Pos::new(x.span.line, x.span.col)) else { continue };
        let Some((open, close)) = argument(text.body(), at + 1 + name.len()) else { continue };
        let inner = &text.body()[open + 1..close];
        let parts: Vec<(usize, &str)> = inner
            .split(',')
            .scan(open + 1, |at, p| {
                let here = *at;
                *at += p.len() + 1;
                Some((here, p))
            })
            .collect();
        let names = parts.iter().filter(|p| !p.1.trim().is_empty()).count();
        for (from, part) in parts {
            let n = part.trim();
            if n.is_empty() {
                continue;
            }
            let lead = part.len() - part.trim_start().len();
            let path = crate::plugins::bib::resolve(analysis, n).map(|p| p.display().to_string());
            let item = (from + lead, from + lead + n.len());
            loads.push(Load {
                span: x.span,
                name: n.into(),
                path,
                file: file.clone(),
                call: (at, close + 1),
                item,
                names,
            });
        }
    }
    loads.sort_by_key(|l| (l.span, l.item));
    // the databases the run read, also through what the loading calls do not show (a `.bbl`'s `\bibdata`)
    let mut paths: Vec<String> = loads.iter().filter_map(|l| l.path.clone()).collect();
    for o in analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Bibliography) {
        paths.extend(crate::plugins::bib::resolve(analysis, &o.key).map(|p| p.display().to_string()));
    }
    let mut dbs: Vec<Db> = Vec::new();
    for path in paths {
        if dbs.iter().any(|d| d.path == path) {
            continue;
        }
        let Some(text) = sources.get(&path) else { continue };
        let mut entries = crate::plugins::bib::entries(text.body());
        for e in &mut entries {
            e.file = path.clone();
        }
        let constants = constants(text.body());
        dbs.push(Db { path, text, entries, constants });
    }
    (loads, dbs)
}

/// The braced argument after optional spaces and `[…]` from byte `at`: its braces' bytes.
fn argument(text: &str, mut at: usize) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    loop {
        while b.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        match b.get(at)? {
            b'[' => at += text[at..].find(']')? + 1,
            b'{' => break,
            _ => return None,
        }
    }
    let mut depth = 0;
    for (i, c) in text[at..].bytes().enumerate() {
        match c {
            b'{' => depth += 1,
            b'}' if depth == 1 => return Some((at, at + i)),
            b'}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The `@string{name = value}` constants of a database, in order.
pub fn constants(text: &str) -> Vec<Constant> {
    let mut out = Vec::new();
    let lower = text.to_ascii_lowercase();
    for (at, _) in lower.match_indices("@string") {
        let rest = &text[at + 7..];
        let Some(open) = rest.find(['{', '(']).filter(|o| rest[..*o].trim().is_empty()) else { continue };
        let body = &rest[open + 1..];
        let Some(eq) = body.find('=') else { continue };
        let name = body[..eq].trim().to_string();
        let value = body[eq + 1..].trim_start();
        let close = if rest[open..].starts_with('(') { ')' } else { '}' };
        let (mut depth, mut end) = (0i32, value.len());
        for (i, c) in value.char_indices() {
            match c {
                '{' => depth += 1,
                '}' if depth > 0 => depth -= 1,
                c if c == close && depth == 0 => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        let v = value[..end].trim();
        let v = v
            .strip_prefix('{')
            .and_then(|v| v.strip_suffix('}'))
            .or_else(|| v.strip_prefix('"').and_then(|v| v.strip_suffix('"')))
            .unwrap_or(v);
        let line = 1 + text[..at].matches('\n').count() as u32;
        out.push(Constant { name, value: v.to_string(), line });
    }
    out
}

/// The constants a bare value uses: its words, numbers and quoted or braced parts left out.
pub fn words(value: &str) -> Vec<String> {
    let (mut out, mut depth, mut quoted, mut word) = (Vec::new(), 0i32, false, String::new());
    for c in value.chars().chain(std::iter::once(' ')) {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            '"' if depth == 0 => quoted = !quoted,
            c if depth == 0 && !quoted && (c.is_whitespace() || c == '#') => {
                if !word.is_empty() && !word.bytes().all(|b| b.is_ascii_digit()) {
                    out.push(std::mem::take(&mut word));
                }
                word.clear();
                continue;
            }
            c if depth == 0 && !quoted => word.push(c),
            _ => {}
        }
    }
    out
}

/// The constants `e` uses that no database of `dbs` and no style constant defines.
pub fn undefined(e: &Entry, dbs: &[Db], model: &Model) -> Vec<String> {
    let known = |w: &str| {
        dbs.iter().flat_map(|d| &d.constants).any(|c| c.name.eq_ignore_ascii_case(w))
            || model.macros.iter().any(|m| m.0.eq_ignore_ascii_case(w))
    };
    let mut out: Vec<String> = Vec::new();
    for (_, value) in e.fields.iter().filter(|f| e.bare.contains(&f.0)) {
        out.extend(words(value).into_iter().filter(|w| !known(w) && !out.contains(w)).collect::<Vec<_>>());
    }
    out
}

/// `e`'s first `name` field with text.
fn field<'a>(e: &'a Entry, name: &str) -> Option<&'a str> {
    e.fields.iter().find(|f| f.0 == name && !f.1.trim().is_empty()).map(|f| f.1.as_str())
}

/// The fields `e` lacks that its type requires (an alternative given counts). A field
/// counts under the name its reader gives it, and so does what a `crossref` parent hands
/// down: everything with BibTeX, what the inheritance rules say with biber. `xref`
/// inherits nothing.
pub fn missing(e: &Entry, dbs: &[Db], model: &Model) -> Vec<String> {
    let parent = field(e, "crossref").and_then(|k| entry(dbs, model, k));
    // a set takes its fields from its members (biblatex manual, §2.2.3 `entryset`)
    let set = model.biblatex && field(e, "entryset").is_some();
    if set || (model.custom && (parent.is_some() || model.biblatex)) {
        return Vec::new();
    }
    fn given<'a>(of: &'a Entry, model: &'a Model) -> impl Iterator<Item = &'a str> {
        of.fields.iter().filter(|f| !f.1.trim().is_empty()).map(|f| model.read_as(&f.0))
    }
    let has = |n: &str| {
        given(e, model).any(|f| f == n)
            || parent.is_some_and(|p| match model.biblatex {
                true => given(p, model).any(|f| model.inherited(&p.kind, &e.kind, f).iter().any(|to| to == n)),
                false => given(p, model).any(|f| f == n),
            })
    };
    // biber sets some fields itself, and a `.bst` asks less of an entry that inherits
    let set = |n: &str| model.biblatex && model.type_sets.iter().any(|s| s.0 == e.kind && s.1 == n);
    let spared = model.uncrossed.iter().find(|u| parent.is_some() && u.0 == e.kind).map(|u| u.1.as_slice());
    let needs = model.required_of(model.kind(&e.kind)).iter().filter(|alt| !spared.unwrap_or_default().contains(alt));
    needs.filter(|alt| !alt.iter().any(|f| has(f) || set(f))).map(|alt| alt.join(" or ")).collect()
}

/// Two entries of the databases that name one work under different keys: (kept, dropped, why).
/// The kept one has more fields.
pub fn twins(dbs: &[Db]) -> Vec<(&Entry, &Entry, &'static str)> {
    let all: Vec<&Entry> = dbs.iter().flat_map(|d| &d.entries).collect();
    let mut out = Vec::new();
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            if a.key == b.key || !convert::same(a, b) {
                continue;
            }
            let doi = |e: &Entry| e.fields.iter().any(|f| f.0 == "doi");
            let why = if doi(a) && doi(b) { "the same DOI" } else { "the same title" };
            let (keep, drop) = if b.fields.len() > a.fields.len() { (*b, *a) } else { (*a, *b) };
            out.push((keep, drop, why));
        }
    }
    out
}

pub static RULES: &[Rule] = &[
    Rule {
        code: "duplicate-bibliography-load",
        since: "0.2.0",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "database loaded twice",
        explanation: "\
A `.bib` file named twice (`\\bibliography{a,a}`, `\\addbibresource` twice, or
`\\bibliography` and `\\addbibresource` both) is read twice. BibTeX reports
every entry as a `Repeated entry`. biber warns about each duplicate key.

Fix by dropping the second load.",
        references: &[Ref { source: Source::Bibtex, at: "Using BibTeX" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: duplicate_load,
    },
    Rule {
        code: "duplicate-bibliography-work",
        since: "0.2.0",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "one work under two keys",
        explanation: "\
Two entries with the same DOI, or the same title and year, are one work under two
keys, in one database or in two. Citing both prints it twice in the bibliography.

Fix by merging them: the entry with more fields stays, gains the fields only the
other has, and the citations of the other key cite the kept one.",
        references: &[Ref { source: Source::Bibtex, at: "Using BibTeX" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: duplicate_work,
    },
    Rule {
        code: "bibliography-entry-missing-field",
        since: "0.2.0",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "cited entry lacks a required field",
        explanation: "\
The style's data model requires fields per entry type: the `.bst`'s
`output.check`s (BibTeX warns `empty journal in knuth84`), or biblatex's mandatory
constraints (biber warns under `--validate-datamodel`). An empty field counts as
missing. A field counts under the name biber gives it (`journal` is
`journaltitle`, `school` is `institution`), and a type under the one biber reads it
as (`@phdthesis` is a `@thesis`). BibTeX renames nothing.

A `crossref` parent hands its fields down: all of them with BibTeX, what
biblatex's inheritance rules say with biber (a `@proceedings`' `title` arrives as
`booktitle`, not as `title`). `xref` hands down nothing. Nothing is reported for
biber while the document declares a source map or inheritance rules of its own.
Only cited entries are checked.

Fix by adding the field.",
        references: &[Ref { source: Source::Bibtex, at: "Using BibTeX" }, Ref { source: Source::Biblatex, at: "§2.1" }],
        fixes: &[],
        engines: &[],
        run: missing_field,
    },
    Rule {
        code: "undefined-bibliography-string",
        since: "0.2.0",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "undefined @string constant",
        explanation: "\
A bare word in a field value is a constant: an `@string` of the databases, or
one the style predefines (`jan`). Any other is undefined: BibTeX warns `I don't
have a string` and writes nothing for it.

Fix by defining the constant, or by writing the value in braces.",
        references: &[Ref { source: Source::Bibtex, at: "The database files" }],
        fixes: &[],
        engines: &[],
        run: undefined_string,
    },
    Rule {
        code: "uncited-bibliography-entry",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "database entry never cited",
        explanation: "\
An entry no `\\cite` names is left out of the bibliography. That is what a shared
database is for, so this is only a note. `\\nocite{*}` cites everything.

Fix by citing the entry, or by deleting it.",
        references: &[Ref { source: Source::Bibtex, at: "Using BibTeX" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: uncited_entry,
    },
    Rule {
        code: "malformed-bibliography-entry",
        since: "0.2.6",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "entry breaks the database's grammar",
        explanation: "\
An entry is `@type{key, name = value, …}`. A value is a number, a constant,
`{…}` or `\"…\"` with balanced braces, or several joined by `#`. Where an entry
breaks this (a missing comma, a brace or quote that never closes, a `}` too
many inside quotes), BibTeX says `I was expecting a `,' or a `}'` and skips the
rest of the entry: the fields after the break are lost, and an unclosed brace
swallows the entries that follow. biber stops with a syntax error. biber also
wants the comma after the key: `@misc{key}` is an error to it, not to BibTeX.

Every entry of a loaded database is checked, cited or not. The message names
the line where the reader gives up, which may be after the slip.

Fix by repairing the entry. The quick fix adds the comma after a lone key.",
        references: &[Ref { source: Source::Bibtex, at: "The database files" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: malformed_entry,
    },
    Rule {
        code: "duplicate-bibliography-field",
        since: "0.2.6",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "field given twice in one entry",
        explanation: "\
An entry that gives a field twice keeps one of the two. BibTeX reads the first
and warns `I'm ignoring knuth84's extra \"title\" field`. biber reads the last
and says nothing. A database moved between the two changes what is printed.

Fix by deleting one. The quick fix deletes the one the document's reader does
not read, so the output stays as it is.",
        references: &[Ref { source: Source::Bibtex, at: "The database files" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: duplicate_field,
    },
    Rule {
        code: "invalid-bibliography-field-value",
        since: "0.2.6",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "value is not what the field takes",
        explanation: "\
A name list is names joined by `and`, each `First Last`, `Last, First` or
`Last, Jr, First`. BibTeX warns `Too many commas in name` at a third comma and
`has a comma at the end` at a closing one. biber skips the entry at a third
comma or at two in a row. An `and` at the start, at the end or twice in a row
joins no name: BibTeX prints it as part of a name, biber skips the entry.
`others` stands for the rest only as the last name.

With biblatex a value must also fit the datatype the data model declares for
its field. biber warns about a `date` it cannot read (`2001-05-21`, a `/`
range, ISO 8601-2), about a `year` or `month` that is no number (`month =
{jan}` in braces is a word, bare it is the month constant), and about an ISBN
whose check digit fails. Under `--validate-datamodel` it also rejects an
`integer` field that is no number (`volume = {IV}`), an ISSN or ISMN with a
wrong check digit and a value outside its field's pattern: these are notes.
A value with a macro or braces in it is not judged.

The quick fix drops stray `and`s and closing commas, and unbraces a month
constant.",
        references: &[Ref { source: Source::Bibtex, at: "The entries" }, Ref { source: Source::Biblatex, at: "§2.3" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: field_value,
    },
    Rule {
        code: "conflicting-bibliography-fields",
        since: "0.2.6",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "fields that exclude each other",
        explanation: "\
Some fields say the same thing twice, and the reader takes one. A `.bst` checks
its pairs itself (`either.or.check`): BibTeX warns `can't use both author and
editor fields` for a `@book`, and `can't use both volume and number fields`.
biber renames BibTeX's field names to biblatex's (`journal` to `journaltitle`):
where an entry gives both, the old name is dropped. A `date` carries the year
and the month: biber warns `Overwriting field 'year' with year value from
field 'date'` where a `year` or `month` beside it says something else.

The pairs come from the style: the `.bst`'s checks, biblatex's source map.

Fix by keeping one. The quick fix deletes the field biber drops, safely where
both say the same.",
        references: &[
            Ref { source: Source::Bibtex, at: "The entries" },
            Ref { source: Source::Biblatex, at: "§2.2.5" },
        ],
        fixes: &[Applicability::Safe, Applicability::Unsafe],
        engines: &[],
        run: conflicting_fields,
    },
    Rule {
        code: "unknown-bibliography-entry-type",
        since: "0.2.6",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "entry type the style does not define",
        explanation: "\
A `.bst` defines a function per entry type. For any other type BibTeX warns
`entry type for \"key\" isn't style-file defined` and formats the entry with
the style's fallback, if it has one. The standard styles know no `@online`.
biblatex's data model declares its types, and biber reads some BibTeX types
as one of them (`@phdthesis` as `@thesis`): any other becomes a `@misc`, with a
warning under `--validate-datamodel`.

Fix by using a type of the style.",
        references: &[Ref { source: Source::Bibtex, at: "Entry types" }, Ref { source: Source::Biblatex, at: "§2.1" }],
        fixes: &[],
        engines: &[],
        run: unknown_type,
    },
    Rule {
        code: "broken-bibliography-crossref",
        since: "0.2.6",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "crossref to no entry, or to an earlier one",
        explanation: "\
`crossref` names the entry a cited one inherits its missing fields from. Where
no database declares that key, BibTeX stops with `A bad cross reference` and
biber warns `I didn't find a database entry for crossref`: nothing is
inherited. BibTeX compares keys without case, biber with it.

BibTeX reads the databases once, in order. A parent that stands before its
child and is not cited itself is passed by before the child names it, and
BibTeX reports the same bad cross reference. btxdoc asks for the parent after
every entry that refers to it. biber has no such order.

Fix by adding the entry, by correcting the key, or by moving the parent below
its children.",
        references: &[
            Ref { source: Source::Bibtex, at: "The entries" },
            Ref { source: Source::Biblatex, at: "§2.4.1" },
        ],
        fixes: &[],
        engines: &[],
        run: broken_crossref,
    },
    Rule {
        code: "ignored-bibliography-field",
        since: "0.2.6",
        category: Category::Style,
        severity: Severity::Info,
        summary: "field the style never reads of this type",
        explanation: "\
A style reads a fixed set of fields per entry type: what the `.bst`'s function
for the type and the functions it calls name, or what biblatex's data model
lists for the type. A field the style knows but does not read of this type is
left out without a word: the `booktitle` of an `@article`, the `journal` of a
`@book`. biber says `Invalid field 'booktitle' for entrytype 'article'` under
`--validate-datamodel`. Often the entry has the wrong type.

With BibTeX a field under biblatex's name is not read either (`journaltitle`
where the style reads `journal`). A field no part of the style knows (`abstract`,
`file`, a reference manager's own) is a note to yourself and not reported.

Fix by deleting the field or by changing the type. The quick fix renames a
field to the name the style reads.",
        references: &[Ref { source: Source::Bibtex, at: "The entries" }, Ref { source: Source::Biblatex, at: "§2.1" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: ignored_field,
    },
];

/// The keys cited, and whether `\nocite{*}` cites all.
fn cited<'a>(analysis: &'a Analysis, sources: &Sources) -> (BTreeSet<&'a str>, bool) {
    let keys: BTreeSet<&str> =
        analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Cite).map(|o| o.key.as_str()).collect();
    // biblatex hands its `*` to biber alone, in no `\citation` line: the call says it
    let star = |x: &crate::analysis::facts::Expansion| {
        let text = sources.get(analysis.file_name(x.span.file))?;
        let at = text.offset(Pos::new(x.span.line, x.span.col))?;
        let (open, close) = argument(text.body(), at + "\\nocite".len())?;
        (text.body()[open + 1..close].trim() == "*").then_some(())
    };
    let nocite = |x: &&crate::analysis::facts::Expansion| analysis.interner.name(x.name) == "nocite";
    let all = keys.contains("*") || analysis.facts.expansions.iter().filter(nocite).any(|x| star(x).is_some());
    (keys, all)
}

/// Where a finding about a database's entry stands: the call that loads its file.
fn at_load<'a>(loads: &'a [Load], path: &str) -> Option<&'a Load> {
    loads.iter().find(|l| l.path.as_deref() == Some(path))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The lines of `e` in its database's text: what deleting it removes.
fn entry_lines(text: &Text, e: &Entry) -> Option<(Pos, Pos)> {
    let body = text.body();
    let start = text.offset(Pos::new(e.line, 1))?;
    let at = start + body[start..].find('@')?;
    let open = at + body[at..].find(['{', '('])?;
    let (mut depth, mut end) = (0i32, None);
    for (i, c) in body[open..].bytes().enumerate() {
        match c {
            b'{' | b'(' => depth += 1,
            b'}' | b')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let stop = body[end..].find('\n').map_or(body.len(), |i| end + i + 1);
    // a blank line after it goes too
    let stop = if body[stop..].starts_with('\n') { stop + 1 } else { stop };
    Some((Pos::new(e.line, 1), text.pos(stop)))
}

fn duplicate_load(report: &mut Report) {
    let sources = report.edits().sources;
    let loads = &report.bib().loads;
    for (i, l) in loads.iter().enumerate() {
        let Some(path) = &l.path else { continue };
        let Some(first) = loads[..i].iter().find(|f| f.path.as_ref() == Some(path)) else { continue };
        let Some(text) = sources.get(&l.file) else { continue };
        // the name with its comma, else the whole call
        let body = text.body();
        let (from, to) = match l.names {
            1 => (l.call.0, l.call.1),
            _ if body[..l.item.0].trim_end().ends_with(',') => (body[..l.item.0].trim_end().len() - 1, l.item.1),
            _ => (l.item.0, l.item.1 + body[l.item.1..].find(',').map_or(0, |c| c + 1)),
        };
        let mut edit =
            Edit { path: l.file.clone(), start: text.pos(from), end: text.pos(to), replacement: String::new() };
        // a call alone on its line goes with the line
        let line = text.line(edit.start.line);
        if l.names == 1 && line.trim() == body[from..to].trim() {
            (edit.start, edit.end) = (Pos::new(edit.start.line, 1), Pos::new(edit.start.line + 1, 1));
        }
        let message = format!("`{}` is loaded already at {}", file_name(path), report.place(first.span));
        report.add_fix(
            l.span,
            &l.name,
            message,
            Fix::new("drop the second load", Applicability::Safe, Some(vec![edit])),
        );
    }
}

/// The edits that rewrite each `\cite` of `from` to `to`: the key in each citing line.
pub fn recite(analysis: &Analysis, sources: &Sources, from: &str, to: &str) -> Vec<Edit> {
    let mut out: Vec<Edit> = Vec::new();
    for o in analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Cite && o.key == from) {
        let path = analysis.file_name(o.span.file).to_string();
        let Some(text) = sources.get(&path) else { continue };
        let line = text.line(o.span.line);
        let bound = |c: Option<char>| c.is_none_or(|c| matches!(c, '{' | '}' | ',' | ' ' | '\t'));
        for (i, _) in line.match_indices(from) {
            if bound(line[..i].chars().next_back()) && bound(line[i + from.len()..].chars().next()) {
                let col = |b: usize| line[..b].chars().count() as u32 + 1;
                let e = Edit {
                    path: path.clone(),
                    start: Pos::new(o.span.line, col(i)),
                    end: Pos::new(o.span.line, col(i + from.len())),
                    replacement: to.into(),
                };
                if !out.iter().any(|x| x.path == e.path && x.start == e.start) {
                    out.push(e);
                }
            }
        }
    }
    out
}

/// Merging `drop` into `keep`: its fields `keep` lacks added before `keep`'s close, `drop`
/// deleted, its citations rewritten.
pub fn merge(analysis: &Analysis, sources: &Sources, dbs: &[Db], keep: &Entry, drop: &Entry) -> Option<Vec<Edit>> {
    let db = |e: &Entry| dbs.iter().find(|d| d.path == e.file);
    let (kt, dt) = (db(keep)?, db(drop)?);
    let (_, end) = entry_lines(&kt.text, keep)?;
    // the close: the last `}` or `)` before the entry's end
    let close = kt.text.body()[..kt.text.offset(end)?].rfind(['}', ')'])?;
    let before = kt.text.body()[..close].trim_end();
    let comma = if before.ends_with(',') { "" } else { "," };
    let added: String = drop
        .fields
        .iter()
        .filter(|f| !keep.fields.iter().any(|k| k.0 == f.0))
        .map(|(n, v)| if drop.bare.contains(n) { format!("\n  {n} = {v},") } else { format!("\n  {n} = {{{v}}},") })
        .collect();
    let mut edits = Vec::new();
    if !added.is_empty() {
        let at = kt.text.pos(before.len());
        edits.push(Edit {
            path: kt.path.clone(),
            start: at,
            end: at,
            replacement: format!("{comma}{}", added.trim_end_matches(',')),
        });
    }
    let (from, to) = entry_lines(&dt.text, drop)?;
    edits.push(Edit { path: dt.path.clone(), start: from, end: to, replacement: String::new() });
    edits.extend(recite(analysis, sources, &drop.key, &keep.key));
    Some(edits)
}

fn duplicate_work(report: &mut Report) {
    let analysis = report.analysis;
    let sources = report.edits().sources;
    let Bib { loads, dbs, .. } = report.bib();
    for (keep, drop, why) in twins(dbs) {
        let Some(load) = at_load(loads, &drop.file).or_else(|| loads.first()) else { continue };
        let message = format!(
            "`{}` ({}:{}) and `{}` ({}:{}) have {why}",
            keep.key,
            file_name(&keep.file),
            keep.line,
            drop.key,
            file_name(&drop.file),
            drop.line
        );
        let fix = Fix::new(
            format!("merge `{}` into `{}`", drop.key, keep.key),
            Applicability::Unsafe,
            merge(analysis, sources, dbs, keep, drop),
        );
        report.add_fix(load.span, &format!("{},{}", keep.key, drop.key), message, fix);
    }
}

/// A finding about one entry, before those of one site are joined.
struct Found {
    span: Span,
    key: String,
    severity: Severity,
    message: String,
    advice: String,
    edits: Vec<Edit>,
    applicability: Applicability,
}

/// What a check says of one entry: the message after the entry's name, the advice, the
/// edits of its fix and whether they keep the meaning.
type Verdict = (Severity, String, String, Vec<Edit>, Applicability);

fn verdict(severity: Severity, message: String, advice: impl Into<String>) -> Verdict {
    (severity, message, advice.into(), Vec::new(), Applicability::Safe)
}

/// Runs `check` on the entries the reader reads in full, the cited ones (every one with
/// `all`), and reports what it says. A finding stands at the entry's first citation, else
/// at its database's load. A site keeps one finding per rule, so those of one site (a
/// `\cite` of two keys, `\nocite{*}`'s load) are joined into one.
fn judge(report: &mut Report, all: bool, check: impl Fn(&Bib, &Db, &Entry) -> Vec<Verdict>) {
    let bib = report.bib();
    let (keys, every) = cited(report.analysis, report.edits().sources);
    let mut found: Vec<Found> = Vec::new();
    let mut first = std::collections::BTreeMap::new();
    for o in report.analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Cite) {
        first.entry(o.key.as_str()).or_insert(o);
    }
    for (db, e) in bib.dbs.iter().flat_map(|d| d.entries.iter().map(move |e| (d, e))) {
        if !(all || every || keys.contains(e.key.as_str())) {
            continue;
        }
        let verdicts = check(bib, db, e);
        if verdicts.is_empty() {
            continue;
        }
        let site = first.get(e.key.as_str()).map(|o| report.site(o).0);
        let Some(span) = site.or_else(|| at_load(&bib.loads, &e.file).map(|l| l.span)) else { continue };
        let editable = report.edits().sources.editable(report.analysis, &db.path);
        for (severity, message, advice, edits, applicability) in verdicts {
            let message = format!("`{}` ({}:{}) {message}", e.key, file_name(&e.file), e.line);
            let edits = if editable { edits } else { Vec::new() };
            found.push(Found { span, key: e.key.clone(), severity, message, advice, edits, applicability });
        }
    }
    while let Some(first) = found.first().map(|f| f.span) {
        let (here, rest): (Vec<Found>, Vec<Found>) = found.into_iter().partition(|f| f.span == first);
        found = rest;
        let rank = |s: Severity| [Severity::Error, Severity::Warning, Severity::Info].iter().position(|x| *x == s);
        let severity = here.iter().map(|f| f.severity).min_by_key(|s| rank(*s)).unwrap_or(Severity::Warning);
        let join = |texts: Vec<&str>| {
            let mut texts = texts;
            texts.dedup();
            texts.join(". ")
        };
        let message = join(here.iter().map(|f| f.message.as_str()).collect());
        let key = here[0].key.clone();
        let fixed: Vec<&Found> = here.iter().filter(|f| !f.edits.is_empty()).collect();
        if fixed.is_empty() {
            let advice = join(here.iter().map(|f| f.advice.as_str()).collect());
            report.add_as(severity, first, &key, message, Some(advice));
        } else {
            let advice = join(fixed.iter().map(|f| f.advice.as_str()).collect());
            let applicability = fixed.iter().map(|f| f.applicability).max().unwrap_or(Applicability::Unsafe);
            let edits = fixed.iter().flat_map(|f| f.edits.iter().cloned()).collect();
            report.add_fix_as(severity, first, &key, message, Fix::new(advice, applicability, Some(edits)));
        }
    }
}

fn missing_field(report: &mut Report) {
    judge(report, false, |bib, _, e| {
        let lacks = missing(e, &bib.dbs, &bib.model);
        if lacks.is_empty() {
            return Vec::new();
        }
        let named = lacks.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ");
        let message = format!("lacks {named}, required for `@{}`", e.kind);
        vec![verdict(Severity::Warning, message, format!("add {named} to `{}`", e.key))]
    });
}

fn undefined_string(report: &mut Report) {
    judge(report, true, |bib, _, e| {
        let names = undefined(e, &bib.dbs, &bib.model);
        if names.is_empty() {
            return Vec::new();
        }
        let named = names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ");
        let message = format!("uses {named}, which no `@string` defines");
        vec![verdict(Severity::Warning, message, "define it with `@string`, or write the value in braces")]
    });
}

fn uncited_entry(report: &mut Report) {
    let analysis = report.analysis;
    let sources = report.edits().sources;
    let Bib { loads, dbs, .. } = report.bib();
    let (keys, all) = cited(analysis, sources);
    if all || dbs.is_empty() {
        return;
    }
    // One finding per database: its entries all stand at the one load, where a site keeps one finding.
    for db in dbs {
        let Some(load) = at_load(loads, &db.path) else { continue };
        let uncited: Vec<_> = db.entries.iter().filter(|e| !keys.contains(e.key.as_str())).collect();
        let Some(first) = uncited.first() else { continue };
        let editable = sources.editable(analysis, &db.path);
        let edits: Option<Vec<Edit>> = (uncited.iter())
            .map(|e| entry_lines(&db.text, e).filter(|_| editable))
            .map(|lines| {
                lines.map(|(from, to)| Edit { path: db.path.clone(), start: from, end: to, replacement: String::new() })
            })
            .collect();
        let named: Vec<String> =
            uncited.iter().map(|e| format!("`{}` ({}:{})", e.key, file_name(&db.path), e.line)).collect();
        let (message, title) = match named.len() {
            1 => (format!("{} is never cited", named[0]), format!("delete `{}`", first.key)),
            n => (format!("{n} entries are never cited: {}", named.join(", ")), format!("delete the {n} entries")),
        };
        report.add_fix_as(
            Severity::Info,
            load.span,
            &first.key,
            message,
            Fix::new(title, Applicability::Unsafe, edits),
        );
    }
}

/// Who reads the databases, for a message.
fn reader(model: &Model) -> &'static str {
    if model.biblatex { "biber" } else { "BibTeX" }
}

/// The edit that puts `text` for bytes `from..to` of `db`.
fn put(db: &Db, from: usize, to: usize, text: impl Into<String>) -> Edit {
    Edit { path: db.path.clone(), start: db.text.pos(from), end: db.text.pos(to), replacement: text.into() }
}

/// The edit that deletes field `i` of `e` with its comma, and its line when it has one alone.
fn drop_field(db: &Db, e: &Entry, i: usize) -> Edit {
    let body = db.text.body();
    let (mut from, _, mut to) = e.at[i];
    let after = body[to..].trim_start_matches([' ', '\t']);
    if let Some(rest) = after.strip_prefix(',') {
        let (line, rest) = (body[..from].rfind('\n').map_or(0, |n| n + 1), rest.trim_start_matches([' ', '\t']));
        to = body.len() - rest.len();
        if body[line..from].trim().is_empty() && rest.starts_with('\n') {
            (from, to) = (line, to + 1);
        }
    } else {
        // the last field: the comma before it goes
        from = body[..from].trim_end().strip_suffix(',').map_or(from, |b| b.len());
    }
    put(db, from, to, "")
}

/// The edit that writes `value` for field `i` of `e`, in the delimiters it has.
fn set_value(db: &Db, e: &Entry, i: usize, value: &str) -> Edit {
    let (_, from, to) = e.at[i];
    let quoted = db.text.body()[from..].starts_with('"');
    put(db, from, to, if quoted { format!("\"{value}\"") } else { format!("{{{value}}}") })
}

/// `e`'s fields with text and their places: (index, name, value). A constant or a
/// concatenation is left out unless `bare`: its text is not what the reader sees.
fn written(e: &Entry, bare: bool) -> impl Iterator<Item = (usize, &str, &str)> {
    let fields = e.fields.iter().enumerate().map(|(i, f)| (i, f.0.as_str(), f.1.as_str()));
    fields.filter(move |f| !f.2.trim().is_empty() && (bare || !e.bare.iter().any(|b| b == f.1)))
}

/// The names of a name list as BibTeX splits it: at each `and` outside braces, in any
/// case. An empty one stands where an `and` joins nothing.
pub fn name_list(value: &str) -> Vec<String> {
    let (mut out, mut depth, mut word, mut name) = (Vec::new(), 0i32, String::new(), Vec::<String>::new());
    for c in value.chars().chain([' ']) {
        depth += i32::from(c == '{') - i32::from(c == '}');
        if !c.is_whitespace() || depth > 0 {
            word.push(c);
        } else if word.eq_ignore_ascii_case("and") {
            out.push(std::mem::take(&mut name).join(" "));
            word.clear();
        } else if !word.is_empty() {
            name.push(std::mem::take(&mut word));
        }
    }
    out.push(name.join(" "));
    out
}

/// What is wrong with a name list, and the list repaired where that is plain: stray
/// `and`s and closing commas dropped. BibTeX reads `First Last`, `Last, First` and
/// `Last, Jr, First` (btxhak, "Name formats"): a third comma is too many, a closing one
/// names nothing. biber stops at two commas in a row too.
pub fn name_faults(value: &str, biber: bool) -> (Vec<String>, Option<String>) {
    let commas = |n: &str| {
        let mut depth = 0i32;
        n.chars().filter(|c| (depth += i32::from(*c == '{') - i32::from(*c == '}'), *c == ',' && depth == 0).1).count()
    };
    let faults = |names: &[String]| {
        let mut faults = Vec::new();
        if names.iter().any(String::is_empty) {
            faults.push("an `and` joins no name".to_string());
        }
        for (i, n) in names.iter().enumerate().filter(|n| !n.1.is_empty()) {
            if n == "others" && i + 1 < names.len() {
                faults.push("`others` is not the last name".into());
            } else if n.ends_with(',') {
                faults.push(format!("the name `{n}` ends in a comma"));
            } else if commas(n) > NAME_COMMAS {
                faults.push(format!("the name `{n}` has more than two commas"));
            } else if biber && n.split(',').skip(1).any(|part| part.trim().is_empty()) {
                faults.push(format!("the name `{n}` has two commas in a row"));
            }
        }
        faults
    };
    let names = name_list(value);
    let found = faults(&names);
    let kept: Vec<&str> = names.iter().map(|n| n.trim_end_matches([',', ' '])).filter(|n| !n.is_empty()).collect();
    let fixed = kept.join(" and ");
    let plain = !found.is_empty() && !fixed.is_empty() && faults(&name_list(&fixed)).is_empty();
    (found, plain.then_some(fixed))
}

/// The commas of BibTeX's longest name shape, `Last, Jr, First`.
const NAME_COMMAS: usize = 2;

/// Whether `value` reads as one of biblatex's dates (ISO 8601-2 level 1: `2001-05-21`, a
/// `/` range, an open end, seasons, `?` and `~`, unspecified `X` digits, a time). Never
/// false for a date biber takes: what is not understood here counts as a date.
pub fn dated(value: &str) -> bool {
    let digits = |p: &str, n: usize| p.len() == n && p.bytes().all(|b| b.is_ascii_digit() || b == b'X');
    let one = |end: &str| {
        let end = end.trim().trim_end_matches(['?', '~', '%']);
        let day = end.split('T').next().unwrap_or_default();
        if day.is_empty() || day == ".." || day.starts_with('Y') {
            return true;
        }
        let mut parts = day.strip_prefix('-').unwrap_or(day).split('-');
        let (year, month, day) = (parts.next().unwrap_or_default(), parts.next(), parts.next());
        let number = |p: Option<&str>| p.and_then(|p| p.parse::<u32>().ok());
        // the calendar: months, then ISO 8601-2's seasons, and each month's days
        let month_fits =
            number(month).is_none_or(|m| (1..=12).contains(&m) || (day.is_none() && (21..=41).contains(&m)));
        let longest = match number(month) {
            Some(2) => 29,
            Some(4 | 6 | 9 | 11) => 30,
            _ => 31,
        };
        digits(year, 4)
            && month.is_none_or(|m| digits(m, 2) && month_fits)
            && day.is_none_or(|d| digits(d, 2) && number(Some(d)).is_none_or(|d| (1..=longest).contains(&d)))
            && parts.next().is_none()
    };
    let mut ends = value.split('/');
    one(ends.next().unwrap_or_default()) && ends.next().is_none_or(one) && ends.next().is_none()
}

/// Whether `value` passes the check digit of its kind (`isbn`, `issn`, `ismn`): an ISBN-10
/// and an ISSN weigh their digits down from their length modulo 11 with `X` for ten, the
/// 13-digit forms weigh 1 and 3 in turn modulo 10. Hyphens and spaces do not count. A
/// kind or length not known here passes.
pub fn check_digit(kind: &str, value: &str) -> bool {
    if !matches!(kind, "isbn" | "issn" | "ismn") {
        return true;
    }
    let code: Vec<char> = value.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
    let digit = |c: char, last: bool| match c {
        'X' | 'x' if last => Some(10),
        c => c.to_digit(10),
    };
    let n = code.len();
    let Some(d) = code.iter().enumerate().map(|(i, c)| digit(*c, i + 1 == n)).collect::<Option<Vec<u32>>>() else {
        // an ISMN of the old form starts with `M`
        return kind == "ismn" && code.first().is_some_and(|c| c.eq_ignore_ascii_case(&'m'));
    };
    let down = || d.iter().enumerate().map(|(i, x)| x * (n - i) as u32).sum::<u32>() % 11 == 0; // not prose
    let ean = || n == 13 && d.iter().enumerate().map(|(i, x)| x * [1, 3][i % 2]).sum::<u32>() % 10 == 0;
    match kind {
        "isbn" => (n == 10 && down()) || ean(),
        "issn" => n == 8 && down(),
        _ => ean(),
    }
}

/// What biber checks of a value without `--validate-datamodel` (biber manual, "Data
/// model validation"): the rest is a note.
const BIBER_ALWAYS: [&str; 3] = ["date", "datepart", "isbn"];

/// The values of `e` that are not what their fields take: name lists either reader
/// stumbles on, and with biber what the data model's datatypes and data constraints
/// reject.
fn bad_values(bib: &Bib, db: &Db, e: &Entry) -> Vec<Verdict> {
    let model = &bib.model;
    let mut out = Vec::new();
    let plain = |v: &str| !v.contains(['{', '\\']);
    for (i, name, value) in written(e, false) {
        let ty = model.datatype(model.read_as(name));
        let said = |what: String| format!("has the `{name}` `{value}`, {what}");
        // BibTeX parses a name where the type's functions format it
        if ty == "name" && (model.biblatex || model.fields_of(&e.kind).iter().any(|f| f == name)) {
            let (faults, fixed) = name_faults(value, model.biblatex);
            if !faults.is_empty() {
                let mut v =
                    verdict(Severity::Warning, said(format!("where {}", faults.join(" and "))), "repair the name list");
                v.3.extend(fixed.map(|f| set_value(db, e, i, &f)));
                out.push(v);
            }
        }
        if !model.biblatex || model.custom || !plain(value) {
            continue;
        }
        let number = value.trim().bytes().all(|b| b.is_ascii_digit());
        let fault = match ty {
            "date" if !dated(value) => Some("which biber cannot read as a date".to_string()),
            // biber reads a legacy `year` that is a date as one
            "datepart" if !number && !(name.ends_with("year") && dated(value)) => Some("which is no number".into()),
            "integer" if !number => Some("which is no number".into()),
            _ => None,
        };
        let checked = model.checks.iter().filter(|c| c.0 == model.read_as(name)).filter_map(|(_, kind, pattern)| {
            let fails = match kind.as_str() {
                "pattern" => regex::Regex::new(pattern).is_ok_and(|re| !re.is_match(value)),
                kind => !check_digit(kind, value),
            };
            fails.then(|| match kind.as_str() {
                "pattern" => (kind.as_str(), format!("which the data model's pattern `{pattern}` rejects")),
                kind => (kind, format!("which is no valid {}", kind.to_uppercase())),
            })
        });
        for (kind, what) in fault.map(|f| (ty, f)).into_iter().chain(checked) {
            let severity = if BIBER_ALWAYS.contains(&kind) { Severity::Warning } else { Severity::Info };
            let mut v = verdict(severity, said(what), format!("correct `{name}`"));
            // a month constant in braces is a word to biber: bare, it is the month
            if kind == "datepart" && model.macros.iter().any(|m| m.0.eq_ignore_ascii_case(value.trim())) {
                (v.2, v.3) = (
                    format!("write `{name} = {}` without braces", value.trim()),
                    vec![put(db, e.at[i].1, e.at[i].2, value.trim())],
                );
            }
            out.push(v);
        }
    }
    out
}

/// The fields of `e` its reader takes one of only: a `.bst`'s `either.or.check`s, and
/// with biber a field beside the one it is renamed to (`journal`, `journaltitle`) and a
/// legacy `year` or `month` that disagrees with `date`.
fn conflicts(bib: &Bib, db: &Db, e: &Entry) -> Vec<Verdict> {
    let model = &bib.model;
    let mut out = Vec::new();
    let place = |n: &str| written(e, true).find(|f| f.1 == n);
    // BibTeX's styles check a pair only where the entry inherits nothing
    let sets = model.exclusive.iter().filter(|x| !model.biblatex && x.0 == e.kind && field(e, "crossref").is_none());
    for set in sets.flat_map(|x| &x.1).filter(|set| set.iter().all(|f| place(f).is_some())) {
        let named = set.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(" and ");
        out.push(verdict(Severity::Warning, format!("gives both {named}. The style takes one"), "keep one of them"));
    }
    if !model.biblatex || model.custom {
        return out;
    }
    for (from, to) in model.aliases.iter().filter(|a| !model.fields.contains(&a.0)) {
        let (Some((i, _, was)), Some((_, _, is))) = (place(from), place(to)) else { continue };
        let message = format!("gives both `{from}` and `{to}`. biber reads `{to}` and drops `{from}`");
        let mut v = verdict(Severity::Warning, message, format!("delete `{from}`"));
        (v.3, v.4) = (vec![drop_field(db, e, i)], if was == is { Applicability::Safe } else { Applicability::Unsafe });
        out.push(v);
    }
    // `date` carries the year and the month: biber overwrites the legacy fields with them
    let date =
        place("date").filter(|d| model.datatype("date") == "date" && dated(d.2) && !d.2.contains(['/', 'X', '?', '~']));
    let mut parts = date.map(|d| d.2.trim().trim_start_matches('-').split(['-', 'T'])).into_iter().flatten();
    for legacy in ["year", "month"] {
        let (Some(part), Some((_, _, own))) = (parts.next().and_then(|p| p.parse::<u32>().ok()), place(legacy)) else {
            continue;
        };
        if model.datatype(legacy) == "datepart" && own.trim().parse::<u32>().is_ok_and(|own| own != part) {
            let message = format!("has the `{legacy}` `{own}`, but its `date` says `{part}`. biber takes the date's");
            out.push(verdict(Severity::Warning, message, format!("delete `{legacy}`, or correct `date`")));
        }
    }
    out
}

/// The fields of `e` the style knows but never reads of its type (`booktitle` of an
/// `@article`), and with BibTeX a field written under the name biblatex gives it.
fn ignored(bib: &Bib, db: &Db, e: &Entry) -> Vec<Verdict> {
    let model = &bib.model;
    let kind = if model.biblatex { model.kind(&e.kind) } else { e.kind.as_str() };
    // a parent's fields are its children's to inherit
    let names = |k: &str| if model.biblatex { k == e.key } else { k.eq_ignore_ascii_case(&e.key) };
    let parent = bib.dbs.iter().flat_map(|d| &d.entries).any(|x| field(x, "crossref").is_some_and(names));
    if parent || model.custom || !(model.biblatex || model.bibtex) || !model.types.iter().any(|t| t == kind) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, name, _) in written(e, true).filter(|f| !model.reads(kind, f.1)) {
        // the first of a field given twice speaks for both
        if e.fields.iter().position(|f| f.0 == name) != Some(i) {
            continue;
        }
        let known = model.fields.iter().any(|f| f == model.read_as(name));
        let other = model.alias(name).filter(|o| !model.biblatex && !known && model.reads(kind, o));
        if let Some(other) = other {
            let mut v = verdict(
                Severity::Info,
                format!("has `{name}`. The style reads `{other}`"),
                format!("rename `{name}` to `{other}`"),
            );
            v.3.extend(field(e, other).is_none().then(|| put(db, e.at[i].0, e.at[i].0 + name.len(), other)));
            out.push(v);
        } else if known {
            let message = format!("has `{name}`, which the style never reads of an `@{}`", e.kind);
            out.push(verdict(Severity::Info, message, format!("delete `{name}`, or change the entry's type")));
        }
    }
    out
}

fn malformed_entry(report: &mut Report) {
    judge(report, true, |bib, db, e| {
        let (body, tool) = (db.text.body(), reader(&bib.model));
        let line = |at: usize| db.text.pos(at.min(body.len())).line;
        if let Some((at, expected)) = e.fault {
            let message = format!("breaks off at line {}, where {tool} expects {expected}", line(at));
            return vec![verdict(
                Severity::Error,
                message,
                "repair the entry: a missing comma, an unclosed brace or quote",
            )];
        }
        // btparse wants the comma after the key, BibTeX does not
        if bib.model.biblatex && e.fields.is_empty() && !body[e.span.0..e.span.1].contains(',') {
            let mut v = verdict(
                Severity::Error,
                "has no comma after its key, which biber takes as an error".into(),
                "add the comma",
            );
            v.3.push(put(db, e.span.1 - 1, e.span.1 - 1, ","));
            return vec![v];
        }
        Vec::new()
    });
}

fn duplicate_field(report: &mut Report) {
    judge(report, false, |bib, db, e| {
        let mut out = Vec::new();
        for (i, f) in e.fields.iter().enumerate() {
            let Some(first) = e.fields[..i].iter().position(|g| g.0 == f.0) else { continue };
            if e.fields[..i].iter().filter(|g| g.0 == f.0).count() > 1 {
                continue;
            }
            // BibTeX keeps the first and warns, biber keeps the last and says nothing
            let (read, unread) = if bib.model.biblatex { ("last", first) } else { ("first", i) };
            let message = format!("gives `{}` twice. {} reads the {read}", f.0, reader(&bib.model));
            let mut v = verdict(Severity::Warning, message, format!("delete the `{}` that is not read", f.0));
            v.3.push(drop_field(db, e, unread));
            out.push(v);
        }
        out
    });
}

fn field_value(report: &mut Report) {
    judge(report, false, bad_values);
}

fn conflicting_fields(report: &mut Report) {
    judge(report, false, conflicts);
}

fn ignored_field(report: &mut Report) {
    judge(report, false, ignored);
}

fn unknown_type(report: &mut Report) {
    judge(report, false, |bib, _, e| {
        let model = &bib.model;
        // BibTeX takes any function of the style as a type, biber the data model's types and its renames
        let known = match model.biblatex {
            true => model.types.iter().any(|t| t == model.kind(&e.kind)),
            false => !model.bibtex || model.defined.iter().any(|f| f.eq_ignore_ascii_case(&e.kind)),
        };
        if known || model.custom || model.types.is_empty() {
            return Vec::new();
        }
        let message = format!("is an `@{}`, a type the style does not define", e.kind);
        vec![verdict(Severity::Warning, message, "use a type of the style")]
    });
}

fn broken_crossref(report: &mut Report) {
    let (keys, every) = cited(report.analysis, report.edits().sources);
    let keys: Vec<String> = keys.into_iter().map(str::to_string).collect();
    judge(report, false, |bib, _, e| {
        let Some(key) = field(e, "crossref") else { return Vec::new() };
        let all = || bib.dbs.iter().flat_map(|d| &d.entries);
        let Some(parent) = entry(&bib.dbs, &bib.model, key) else {
            let message = format!("names the `crossref` `{key}`, which no database declares");
            return vec![verdict(Severity::Warning, message, "add the entry, or correct the key")];
        };
        // BibTeX reads the databases once: a parent it passed uncited is gone when the child names it
        let before = all().position(|x| std::ptr::eq(x, parent)) < all().position(|x| std::ptr::eq(x, e));
        if before && !bib.model.biblatex && !every && !keys.iter().any(|k| k.eq_ignore_ascii_case(&parent.key)) {
            let message =
                format!("inherits from `{}`, which stands before it. BibTeX has passed it by then", parent.key);
            return vec![verdict(Severity::Warning, message, format!("move `{}` below `{}`", parent.key, e.key))];
        }
        Vec::new()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_and_their_uses() {
        let c = constants("@String{cacm = {Comm. ACM}}\n@string(tods = \"TODS\")\n");
        assert_eq!(
            c.iter().map(|c| (c.name.as_str(), c.value.as_str(), c.line)).collect::<Vec<_>>(),
            [("cacm", "Comm. ACM", 1), ("tods", "TODS", 2)]
        );
        assert_eq!(words("jan # \" 1\" # cacm"), ["jan", "cacm"]);
        assert_eq!(words("1984"), Vec::<String>::new());
        assert_eq!(argument("\\bibliography [x] {a, b}", 13), Some((18, 23)));
    }

    /// Intuition: BibTeX splits a name list at `and` outside braces, in any case, so an `and`
    /// at an edge or doubled leaves an empty name. Each fault is one BibTeX or biber reports
    /// (`Too many commas`, `has a comma at the end`, biber's `consecutive commas`), and the
    /// repair is offered only where it leaves no fault. Pins `{and}` and `Last, Jr, First` as fine.
    #[test]
    fn name_lists_are_split_and_judged_as_bibtex_does() {
        assert_eq!(name_list("A. Uthor AND {Smith and Sons} and"), ["A. Uthor", "{Smith and Sons}", ""]);
        let faults = |v: &str, biber: bool| name_faults(v, biber).0.len();
        for fine in
            ["Knuth, Donald E. and Plass, Michael F.", "{and} and B", "de la Vall{\\'e}e, Jr, Charles", "A and others"]
        {
            assert_eq!(name_faults(fine, true), (Vec::new(), None), "{fine}");
        }
        assert_eq!(name_faults("and A and and B and", false).1.as_deref(), Some("A and B"));
        assert_eq!(name_faults("Uthor, A. and Other, B.,", false).1.as_deref(), Some("Uthor, A. and Other, B."));
        assert_eq!((faults("Last, First, Jr, X", false), name_faults("Last, First, Jr, X", false).1), (1, None));
        assert_eq!((faults("Uthor,, A.", false), faults("Uthor,, A.", true)), (0, 1));
        assert_eq!(faults("others and A", false), 1);
    }

    /// Intuition: only a date biber rejects is no date. Each value is one biber 2.21 was run
    /// on: the first row it takes, the second it reports as `Invalid format … of date field`.
    #[test]
    fn dates_are_judged_as_biber_does() {
        let taken = [
            "2001",
            "2001-05",
            "2001-05-21",
            "2001/2002",
            "2001-21",
            "2001-25",
            "199X",
            "2001?/..",
            "2001~",
            "-0044",
            "/2001",
            "2001-05-01T12:00:00",
            "2001-05-06/2001-05-08",
        ];
        let refused = [
            "2001-13-40",
            "Spring 2001",
            "yesterday",
            "2020-02-30",
            "2020-1-5",
            "01.05.2001",
            "2001-5",
            "20010501",
            "2001-04-31",
            "2001-00",
            "2001-12-00",
            "99",
        ];
        assert_eq!(taken.iter().filter(|d| !dated(d)).collect::<Vec<_>>(), Vec::<&&str>::new());
        assert_eq!(refused.iter().filter(|d| dated(d)).collect::<Vec<_>>(), Vec::<&&str>::new());
    }

    /// Intuition: a check digit catches a mistyped number. The codes are the ones biber was
    /// run on. A form not known here passes: no finding without a reason, so the
    /// `datatype=date` constraint of TeX Live 2019's biblatex flags no date range.
    #[test]
    fn check_digits() {
        for (kind, code, valid) in [
            ("isbn", "978-3-16-148410-0", true),
            ("isbn", "0-306-40615-2", true),
            ("isbn", "0-306-40615-3", false),
            ("isbn", "123", false),
            ("isbn", "080442957X", true),
            ("issn", "0317-8471", true),
            ("issn", "12", false),
            ("ismn", "979-0-2600-0043-8", true),
            ("ismn", "M-2306-7118-7", true),
            ("ean", "1", true),
            ("date", "2001-05-21/2001-05-23", true),
        ] {
            assert_eq!(check_digit(kind, code), valid, "{kind} {code}");
        }
    }
}
