//! Cross-references and author names typed by hand: text the document
//! could take from a label or a bibliography entry instead.

use crate::fixed::HashSet;

use crate::analysis::machine::Analysis;
use crate::engine::builtins::OccKind;
use crate::engine::tex::{Catcode, Span, Tok};
use crate::linter::fix::{Applicability, Edit, Fix, Pos};
use crate::linter::numbering::{label_counters, names_counter};
use crate::linter::shared::{loaded, macro_text};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};

pub static RULES: &[Rule] = &[
    Rule {
        code: "hand-written-reference",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a number typed after a reference name, where `\\ref` would keep it right",
        explanation: "\
`see Section 2` stays `2` when a section is added in front of it.  `\\ref`
prints the number the label has in this run, so it never goes stale.

A finding needs a word that names a kind of thing the document labels (a
counter's name, its `\\<counter>name` text such as `Figure`, a cleveref
name, or an abbreviation with a period such as `Eq.`), then a number or a
letter typed in the source, as in `Figure 2`, `Eq. (5)` or `Appendix A`.
The words come from the run, so babel's names count too.  Text in math,
verbatim or a comment is never read, and neither is a number `\\ref` set.

Where one label of that kind prints that very number, the fix names it:
`\\cref` with cleveref loaded, else `\\autoref` with hyperref, else
`Section~\\ref{...}`.  Otherwise the finding asks for a label.",
        references: &[Ref { source: Source::Companion, at: "2.4" }, Ref { source: Source::Cleveref, at: "2" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: hand_written_reference,
    },
    Rule {
        code: "hand-written-author",
        category: Category::Style,
        severity: Severity::Info,
        summary: "an author name typed before a citation that prints it from the bibliography",
        explanation: "\
In `Knuth \\cite{knuth84}` the name is typed by hand.  natbib's `\\citet` and
biblatex's `\\textcite` print the author from the entry and the citation
after it, so the two always agree.

A finding needs a citation of one key whose entry, in a database the run
read, has that author's last name right before it.  `Knuth and Plass` and
`Knuth et al.` count, as does a citation in brackets, `Knuth~[\\cite{k}]`.
It needs natbib or biblatex too: without `\\textcite`, `\\citet` or
`\\citeauthor`, there is nothing better to write.

Fix by writing `\\textcite{knuth84}`, `\\citet{knuth84}` or
`\\citeauthor{knuth84}~\\cite{knuth84}`.",
        references: &[Ref { source: Source::Biblatex, at: "3.9.1" }, Ref { source: Source::Companion, at: "16.3" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: hand_written_author,
    },
];

fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
}

/// What `\ref{key}` prints: the first group of `\r@key`, read from the
/// `.aux` file the run wrote.
fn label_text(analysis: &Analysis, key: &str) -> Option<String> {
    let sym = analysis.interner.lookup(&format!("r@{key}"))?;
    let crate::engine::tex::Meaning::Macro(m) = analysis.env.meaning(sym) else { return None };
    let toks = &m.replacement_text;
    if !toks.first()?.is_cat(Catcode::Begin) {
        return None;
    }
    let mut depth = 0;
    let end = toks.iter().position(|t| {
        match t.tok {
            Tok::Chr(_, Catcode::Begin) => depth += 1,
            Tok::Chr(_, Catcode::End) => depth -= 1,
            _ => {}
        }
        depth == 0
    })?;
    Some(crate::engine::tex::text_of(&toks[1..end], &analysis.interner))
}

/// Whether `word` is `name`, or an abbreviation of it with a period.
fn names(name: &str, word: &str) -> bool {
    word == name || word.strip_suffix('.').is_some_and(|stem| stem.len() >= 2 && name.starts_with(stem))
}

/// The number or appendix letter at `at`, with its end: `2`, `3.1`, `A`.
fn number_at(chars: &[char], at: usize) -> Option<usize> {
    let c = *chars.get(at)?;
    let mut end = at + 1;
    if c.is_ascii_digit() {
        while end < chars.len()
            && (chars[end].is_ascii_digit()
                || (chars[end] == '.' && chars.get(end + 1).is_some_and(char::is_ascii_digit)))
        {
            end += 1;
        }
    } else if !c.is_ascii_uppercase() {
        return None;
    }
    (!chars.get(end).is_some_and(|c| c.is_alphanumeric())).then_some(end)
}

fn hand_written_reference(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let edits = report.edits();
    let counters = label_counters(analysis);
    let cleveref = loaded(analysis, "cleveref");
    // Only a kind the document labels is one it could refer to.
    let kinds: HashSet<&str> = counters.values().map(String::as_str).collect();
    if kinds.is_empty() {
        return;
    }
    let chars: Vec<char> = view.chars().chars().collect();
    let mut findings = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        // A word starts where no letter of the same source text stands before it.
        let inside = i > 0
            && chars[i - 1].is_alphanumeric()
            && view
                .span(i - 1)
                .zip(view.span(i))
                .is_some_and(|(a, b)| a.file == b.file && a.line == b.line && a.col + 1 == b.col);
        if !chars[i].is_alphabetic() || inside {
            i += 1;
            continue;
        }
        while i < chars.len() && chars[i].is_alphabetic() {
            i += 1;
        }
        let letters = i - start;
        let mut word_end = i;
        if chars.get(word_end) == Some(&'.') {
            word_end += 1;
        }
        if letters < 2 || chars.get(word_end) != Some(&' ') {
            continue;
        }
        let mut at = word_end + 1;
        let paren = chars.get(at) == Some(&'(');
        at += paren as usize;
        let Some(number_end) = number_at(&chars, at) else { continue };
        let end = match paren {
            true if chars.get(number_end) == Some(&')') => number_end + 1,
            true => continue,
            false => number_end,
        };
        let number: String = chars[at..number_end].iter().collect();
        let word: String = chars[start..word_end].iter().collect();
        // Typed in one line of the source, character by character: never
        // a number `\ref` set, and never text in math or verbatim.
        let Some(first) = view.span(start) else { continue };
        // The space between is the source's ` ` or `~`, whatever the run
        // set for it.
        let typed = (start..end).filter(|&k| k != word_end).all(|k| {
            view.is_prose_text(k)
                && view.span(k).is_some_and(|s| {
                    s.file == first.file && s.line == first.line && s.col == first.col + (k - start) as u32
                })
        });
        let gap = edits
            .text(first.file)
            .and_then(|(_, source)| source.line(first.line).chars().nth((first.col as usize - 1) + word_end - start));
        if !typed || !matches!(gap, Some(' ' | '~')) {
            continue;
        }
        // An appendix is a section or chapter under `\appendixname`.
        let appendix = macro_text(analysis, "appendixname").is_some_and(|n| names(&n, &word));
        let named: Vec<&str> = kinds
            .iter()
            .copied()
            .filter(|c| {
                names_counter(analysis, cleveref, c, &word) || (appendix && matches!(*c, "section" | "chapter"))
            })
            .collect();
        let letter = number.chars().all(char::is_alphabetic);
        if named.is_empty() || (letter && !appendix && !named.contains(&"appendix")) {
            continue;
        }
        let mut labels: Vec<&str> = counters
            .iter()
            .filter(|(key, c)| {
                named.contains(&c.as_str()) && label_text(analysis, key).as_deref() == Some(number.as_str())
            })
            .map(|(key, _)| *key)
            .collect();
        labels.sort_unstable();
        findings.push((first, first.col + (end - start) as u32, word, number, paren, labels));
        i = end;
    }
    for (at, end, word, number, paren, labels) in findings {
        let typed = match paren {
            true => format!("{word} ({number})"),
            false => format!("{word} {number}"),
        };
        let [key] = labels.as_slice() else {
            let message = format!("`{typed}` is typed by hand, so the number does not follow the document");
            report.add(at, "typed", message, Some("label the target and write `\\ref` to it".into()));
            continue;
        };
        let call = if cleveref {
            let upper = word.chars().next().is_some_and(char::is_uppercase);
            format!("{}{{{key}}}", if upper { r"\Cref" } else { r"\cref" })
        } else if loaded(analysis, "hyperref") {
            format!(r"\autoref{{{key}}}")
        } else if paren && defined(analysis, "eqref") {
            format!(r"{word}~\eqref{{{key}}}")
        } else if paren {
            format!(r"{word}~(\ref{{{key}}})")
        } else {
            format!(r"{word}~\ref{{{key}}}")
        };
        let fix = edits.text(at.file).map(|(path, _)| {
            vec![Edit {
                path,
                start: Pos::new(at.line, at.col),
                end: Pos::new(at.line, end),
                replacement: call.clone(),
            }]
        });
        let message = format!("`{typed}` is typed by hand, and the label `{key}` has that number");
        report.add_fix(at, "labelled", message, Fix::new(format!("write `{call}`"), Applicability::Unsafe, fix));
    }
}

/// The last names of an entry's `author` field, in order.
fn last_names(author: &str) -> Vec<String> {
    author
        .split(" and ")
        .map(|name| {
            let name: String = name.chars().filter(|c| !matches!(c, '{' | '}')).collect();
            match name.split_once(',') {
                Some((last, _)) => last.trim().to_string(),
                None => name.split_whitespace().last().unwrap_or_default().to_string(),
            }
        })
        .collect()
}

/// How the text may name the authors before a citation.
fn spellings(names: &[String]) -> Vec<String> {
    let Some(first) = names.first().filter(|n| !n.is_empty() && !n.contains('\\')) else { return Vec::new() };
    let mut out = vec![first.clone()];
    match names {
        [_, second] => out.extend([format!("{first} and {second}"), format!("{first} \\& {second}")]),
        [_, _, _, ..] => out.extend([format!("{first} et al."), format!("{first} et~al.")]),
        _ => {}
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.len()));
    out
}

fn hand_written_author(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let edits = report.edits();
    let command = ["textcite", "citet", "citeauthor"].into_iter().find(|c| defined(analysis, c));
    let Some(command) = command else { return };
    let mut authors: std::collections::BTreeMap<String, String> = Default::default();
    let mut read = HashSet::default();
    for o in analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Bibliography) {
        let Some(path) = crate::plugins::bib::resolve(analysis, &o.key) else { continue };
        if !read.insert(path.clone()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for entry in crate::plugins::bib::entries(&text) {
            authors.entry(entry.key).or_insert(entry.author);
        }
    }
    // One citation per call: a call citing several keys names no one author.
    let mut keys: crate::fixed::HashMap<Span, Vec<&str>> = Default::default();
    for o in &analysis.facts.occurrences {
        if o.kind == OccKind::Cite && view.document(o.span.file) {
            keys.entry(o.span).or_default().push(&o.key);
        }
    }
    let mut keys: Vec<_> = keys.into_iter().collect();
    keys.sort_by_key(|(s, _)| (s.file, s.line, s.col));
    for (span, mut keys) in keys {
        keys.dedup();
        let [key] = keys.as_slice() else { continue };
        let Some(author) = authors.get(*key) else { continue };
        let Some((path, source)) = edits.text(span.file) else { continue };
        let line = source.line(span.line);
        let Some(call) = line.char_indices().nth(span.col as usize - 1).map(|(b, _)| b) else { continue };
        let rest = &line[call..];
        let name_len = rest[1..].find(|c: char| !c.is_ascii_alphabetic()).map_or(rest.len(), |k| k + 1);
        let name = &rest[1..name_len];
        if !rest.starts_with('\\') || !matches!(name, "cite" | "citep" | "parencite" | "autocite") {
            continue;
        }
        // The arguments as written, up to the key's closing brace.
        let Some(close) = rest.find(&format!("{{{key}}}")).map(|k| k + key.len() + 2) else { continue };
        let args = &rest[name_len..close];
        let before = line[..call].trim_end_matches([' ', '~']);
        let (before, open) = match before.strip_suffix(['[', '(']) {
            Some(b) => (b.trim_end_matches([' ', '~']), true),
            None => (before, false),
        };
        let mut end = call + close;
        if open {
            match line[end..].chars().next() {
                Some(']' | ')') => end += 1,
                _ => continue,
            }
        }
        let names = last_names(author);
        let Some(typed) = spellings(&names).into_iter().find(|s| {
            before.strip_suffix(s.as_str()).is_some_and(|b| !b.chars().last().is_some_and(char::is_alphanumeric))
        }) else {
            continue;
        };
        let from = before.len() - typed.len();
        let col = |byte: usize| line[..byte].chars().count() as u32 + 1;
        let at = Span { file: span.file, line: span.line, col: col(from) };
        // The name is prose the document set, not math, verbatim or a heading's argument read twice.
        if !view.at_span(at).is_some_and(|i| view.is_prose_text(i)) {
            continue;
        }
        let replacement = match command {
            "citeauthor" => format!(r"\citeauthor{{{key}}}~\{name}{args}"),
            _ => format!(r"\{command}{args}"),
        };
        let edit = Edit { path, start: Pos::new(span.line, at.col), end: Pos::new(span.line, col(end)), replacement };
        let message = format!("`{typed}` is typed by hand before the citation of `{key}`");
        let fix = Fix::new(
            format!("write `{}` to print the author from the entry", edit.replacement),
            Applicability::Unsafe,
            Some(vec![edit]),
        );
        report.add_fix(at, "author", message, fix);
    }
}
