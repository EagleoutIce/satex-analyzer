//! Citations against the entries that declare them: `\bibitem`s the run
//! observed, and the `.bib` databases `\bibliography` names.

use std::collections::{BTreeMap, BTreeSet};

use crate::builtins::OccKind;
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::tex::Span;

pub static RULES: &[Rule] = &[
    Rule {
        code: "undefined-citation",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a citation names a key no bibliography entry declares",
        explanation: "\
No `\\bibitem` and no entry of the `.bib` databases `\\bibliography` names
declares this key, so LaTeX prints `?` and warns `Citation … undefined`.
Citations are observed rather than listed: a citation command writes its key
to the `.aux` for the bibliography program (`\\citation{⟨key⟩}`) and reads
the name the entry defines when the file is read back (`\\b@⟨key⟩`).  Any line
written so is a citation.  Without any bibliography every citation is
undefined.  Nothing is reported while a named database cannot be found, or
while a file of the document was not read.

Fix by adding the entry, or by correcting the key.",
        references: &[Ref { source: Source::Source2e, at: "ltbibl.dtx" }],
        fixes: &[],
        run: undefined_citation,
    },
    Rule {
        code: "duplicate-bibliography-entry",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "the same key is declared by two bibliography entries",
        explanation: "\
Two `\\bibitem`s with one key make LaTeX warn `Label … multiply defined` and
every citation resolves to the later one.  Two `.bib` entries with one key make
BibTeX stop with `Repeated entry`.

Fix by renaming or deleting one of them.",
        references: &[Ref { source: Source::Bibtex, at: "Using BibTeX" }],
        fixes: &[],
        run: duplicate_bibliography_entry,
    },
    Rule {
        code: "unused-bibitem",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a `\\bibitem` written in the document is never cited",
        explanation: "\
A `thebibliography` written by hand lists what it lists, cited or not.  A
database only contributes what is cited, so its entries are not reported.
`\\nocite{*}` cites everything.

Fix by citing the entry, or by deleting it.",
        references: &[Ref { source: Source::Bibtex, at: "Using BibTeX" }],
        fixes: &[],
        run: unused_bibitem,
    },
];

/// What declares a key: a `\bibitem` at a span, or a line of a database.
enum Declared {
    Item(Span),
    Entry { file: String, line: u32, at: Span },
}

struct Bibliography {
    declared: BTreeMap<String, Vec<Declared>>,
    /// Whether every database a `\bibliography` names was read.
    complete: bool,
}

fn bibliography(analysis: &Analysis) -> Bibliography {
    let mut declared: BTreeMap<String, Vec<Declared>> = BTreeMap::new();
    let mut complete = true;
    let mut read = BTreeSet::new();
    for occurrence in &analysis.facts.occurrences {
        match occurrence.kind {
            OccKind::BibItem => {
                declared.entry(occurrence.key.clone()).or_default().push(Declared::Item(occurrence.span))
            }
            OccKind::Bibliography => {
                let Some(path) = crate::plugin::bib::resolve(analysis, &occurrence.key) else {
                    complete = false;
                    continue;
                };
                if !read.insert(path.clone()) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    complete = false;
                    continue;
                };
                let file = path.file_name().map_or_else(String::new, |f| f.to_string_lossy().into_owned());
                for entry in crate::plugin::bib::entries(&text) {
                    declared.entry(entry.key).or_default().push(Declared::Entry {
                        file: file.clone(),
                        line: entry.line,
                        at: occurrence.span,
                    });
                }
            }
            _ => {}
        }
    }
    Bibliography { declared, complete }
}

/// `\nocite{*}` cites every entry (btxdoc, "Using BibTeX").
const EVERY_ENTRY: &str = "*";

fn undefined_citation(report: &mut Report) {
    let analysis = report.analysis;
    let bibliography = bibliography(analysis);
    // With no entry at all, every citation is undefined, as LaTeX warns;
    // unless the entries may stand in what the run did not read.
    if !bibliography.complete || bibliography.declared.is_empty() && report.unread("declared").is_some() {
        return;
    }
    let missing: Vec<(Span, Option<String>, String)> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Cite && o.key != EVERY_ENTRY)
        .filter(|o| !bibliography.declared.contains_key(&o.key))
        .map(|o| {
            let (span, via) = report.site(o);
            (span, via, o.key.clone())
        })
        .collect();
    for (span, via, key) in missing {
        report.add(
            span,
            &key,
            format!("no bibliography entry declares `{key}`{}", via.unwrap_or_default()),
            Some(format!("add an entry `{key}`, or correct the key")),
        );
    }
}

fn duplicate_bibliography_entry(report: &mut Report) {
    let bibliography = bibliography(report.analysis);
    // A `.bbl` repeats the database's entries as `\bibitem`s, so each kind
    // is compared with its own.
    for (key, sites) in &bibliography.declared {
        let mut first_item = None;
        let mut first_entry = None;
        for site in sites {
            let (span, message) = match site {
                Declared::Item(span) => match first_item.replace(*span) {
                    None => continue,
                    Some(at) => {
                        first_item = Some(at);
                        (*span, format!("`{key}` is already declared by the `\\bibitem` at {}", report.place(at)))
                    }
                },
                Declared::Entry { file, line, at } => {
                    let here = format!("{file}:{line}");
                    match first_entry.replace(here.clone()) {
                        None => continue,
                        Some(earlier) => {
                            first_entry = Some(earlier.clone());
                            (*at, format!("`{key}` at {here} is already declared at {earlier}"))
                        }
                    }
                }
            };
            report.add(span, key, message, Some("rename or delete one of the two entries".into()));
        }
    }
}

fn unused_bibitem(report: &mut Report) {
    let analysis = report.analysis;
    let cited: BTreeSet<&str> =
        analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Cite).map(|o| o.key.as_str()).collect();
    if cited.contains(EVERY_ENTRY) {
        return;
    }
    let unused: Vec<(Span, String)> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::BibItem && o.span.file == analysis.main_file)
        .filter(|o| !cited.contains(o.key.as_str()))
        .map(|o| (o.span, o.key.clone()))
        .collect();
    for (span, key) in unused {
        let fix = Some(format!("cite `{key}`, or delete its `\\bibitem`"));
        report.add_absent("cited", span, &key, format!("`\\bibitem` `{key}` is never cited"), fix);
    }
}
