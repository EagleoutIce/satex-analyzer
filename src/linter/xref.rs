//! Cross-references and author names typed by hand: text the document
//! could take from a label or a bibliography entry instead.  And cleveref
//! references to a label type it has no format for.

use crate::fixed::HashSet;

use crate::analysis::facts::ListItem;
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
`Section~\\ref{...}`.  Otherwise the finding asks for a label.

An `\\item` of a numbered list counts too, labelled or not, in enumitem's
inline lists as well: `see item (b)` or `in step 2`, where one item prints
`(b)` or `Step~2`.  Its words are the list counter's (`enumi`, its cleveref
name `item`) and the word its label format writes (`Step`).  The fix puts a
`\\label` right after that `\\item` and refers to it.  A number with no word
before it, `in 3.)`, is never read.",
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
    Rule {
        code: "undefined-cref-format",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "cleveref has no name for a label's type, so `\\cref` prints `??`",
        explanation: "\
cleveref prints a reference with the format of the label's type: `\\cref`
the `cref` one, `\\Cref` the `Cref` one, `\\crefrange` the range ones.
`\\crefname` makes them all.  cleveref names the common types itself, and
a theorem's from its heading, but a counter of the document's own (a
`\\newcounter` stepped with `\\refstepcounter`) has none.  The reference
then prints `?? 1`, and cleveref warns that the `cref` reference format for
label type `task` is undefined.

The finding is that warning, raised by cleveref's own code in the run, at
the first reference of each type.  Where it was raised on only some paths
of the analysis, or after code the run could not follow, it is a hint.

Fix by naming the type after `\\usepackage{cleveref}`:
`\\crefname{task}{task}{tasks}`.",
        references: &[Ref { source: Source::Cleveref, at: "8.1.2" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: undefined_cref_format,
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

/// The document's numbered items that no `\label` names, each once:
/// the ones every run made with one text.
fn unlabelled_items(analysis: &Analysis) -> Vec<&ListItem> {
    let mut items: Vec<&ListItem> = Vec::new();
    let mut doubtful: HashSet<Span> = HashSet::default();
    for item in &analysis.facts.items {
        match items.iter().find(|i| i.span == item.span) {
            Some(i) if i.label != item.label => _ = doubtful.insert(item.span),
            Some(_) => {}
            None => items.push(item),
        }
        if !item.certain {
            doubtful.insert(item.span);
        }
    }
    items.retain(|i| !doubtful.contains(&i.span));
    items.sort_by_key(|i| (i.span.file, i.span.line, i.span.col));
    // A label between an item and the next one that prints the item's text names it.
    let labels: Vec<(Span, &str)> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Label)
        .map(|o| (o.span, o.key.as_str()))
        .collect();
    let place = |s: Span| (s.file, s.line, s.col);
    let ends: Vec<Option<Span>> = (0..items.len()).map(|k| items.get(k + 1).map(|i| i.span)).collect();
    items
        .into_iter()
        .zip(ends)
        .filter(|(item, end)| {
            !labels.iter().any(|(at, key)| {
                at.file == item.span.file
                    && place(*at) > place(item.span)
                    && end.is_none_or(|e| place(*at) < place(e))
                    && label_text(analysis, key).as_deref() == Some(item.label.as_str())
            })
        })
        .map(|(item, _)| item)
        .collect()
}

/// An item's label split into the word its format writes and the number:
/// `Step~2` into `Step` and `2`; `(b)` has no word.
fn item_parts(label: &str) -> (Option<&str>, &str) {
    let word_end = label.find(|c: char| !c.is_alphabetic()).unwrap_or(label.len());
    match label[word_end..].strip_prefix(['~', ' ', '\u{a0}']) {
        Some(number) if word_end >= 2 => (Some(&label[..word_end]), number),
        _ => (None, label),
    }
}

/// Whether `\ref` to the item prints `number`, typed in parentheses or not.
fn item_prints(item: &ListItem, number: &str, paren: bool) -> bool {
    let (_, printed) = item_parts(&item.label);
    printed == number || (paren && printed == format!("({number})"))
}

/// A typed reference to one unlabelled item: label the item, refer to it.
fn refer_to_item(
    report: &mut Report,
    (at, end): (Span, u32),
    (word, number, paren): (&str, &str, bool),
    item: &ListItem,
    cleveref: bool,
) {
    let analysis = report.analysis();
    let edits = report.edits();
    let typed = if paren { format!("{word} ({number})") } else { format!("{word} {number}") };
    let key: String = item.label.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let key = format!("item:{key}");
    let taken = analysis.facts.occurrences.iter().any(|o| o.kind == OccKind::Label && o.key == key);
    let own = item_parts(&item.label).0.is_some_and(|w| names(&w.to_lowercase(), &word.to_lowercase()));
    let call = if own {
        // The label prints the word itself: `Step~2`.
        format!(r"\ref{{{key}}}")
    } else if cleveref {
        let upper = word.chars().next().is_some_and(char::is_uppercase);
        format!("{}{{{key}}}", if upper { r"\Cref" } else { r"\cref" })
    } else if paren && !item.label.starts_with('(') {
        format!(r"{word}~(\ref{{{key}}})")
    } else {
        format!(r"{word}~\ref{{{key}}}")
    };
    // The label goes right after `\item`; never into `\item[…]`.
    let label_edit = edits.text(item.span.file).and_then(|(path, source)| {
        let line = source.line(item.span.line);
        let rest: String = line.chars().skip(item.span.col as usize - 1).collect();
        let after = rest.strip_prefix(r"\item")?;
        if after.starts_with(|c: char| c.is_alphabetic() || c == '@') || after.trim_start().starts_with('[') {
            return None;
        }
        let col = item.span.col + r"\item".len() as u32;
        Some(Edit {
            path,
            start: Pos::new(item.span.line, col),
            end: Pos::new(item.span.line, col),
            replacement: format!(r"\label{{{key}}}"),
        })
    });
    let use_edit = edits.text(at.file).map(|(path, _)| Edit {
        path,
        start: Pos::new(at.line, at.col),
        end: Pos::new(at.line, end),
        replacement: call.clone(),
    });
    let fix = match (taken, label_edit, use_edit) {
        (false, Some(label), Some(using)) => Some(vec![label, using]),
        _ => None,
    };
    let message = format!("`{typed}` is typed by hand, and the item at {} prints it", report.place(item.span));
    let fix = Fix::new(format!(r"label the item `\label{{{key}}}` and write `{call}`"), Applicability::Unsafe, fix);
    report.add_fix(at, "item", message, fix);
}

/// Whether `word` is `name`, or an abbreviation of it with a period.
fn names(name: &str, word: &str) -> bool {
    word == name || word.strip_suffix('.').is_some_and(|stem| stem.len() >= 2 && name.starts_with(stem))
}

/// The number or appendix letter at `at`, with its end: `2`, `3.1`, `2b`,
/// `A`, or `b` in parentheses.
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
        // A nested item's number: `2b`.
        if chars.get(end).is_some_and(char::is_ascii_lowercase)
            && !chars.get(end + 1).is_some_and(|c| c.is_alphanumeric())
        {
            end += 1;
        }
    } else if c.is_ascii_lowercase() && at > 0 && chars[at - 1] == '(' {
        // An item's letter or roman number in parentheses: `(b)`, `(ii)`.
        while chars.get(end).is_some_and(char::is_ascii_lowercase) {
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
    let items = unlabelled_items(analysis);
    if kinds.is_empty() && items.is_empty() {
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
        let named = match letter && !appendix && !named.contains(&"appendix") {
            true => Vec::new(),
            false => named,
        };
        let item_word = |item: &ListItem| {
            let own = item_parts(&item.label).0.is_some_and(|w| names(&w.to_lowercase(), &word.to_lowercase()));
            own || names_counter(analysis, cleveref, &item.counter, &word)
        };
        let matched: Vec<&ListItem> =
            items.iter().copied().filter(|i| item_word(i) && item_prints(i, &number, paren)).collect();
        if named.is_empty() && matched.is_empty() {
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
        let item = match (labels.as_slice(), matched.as_slice()) {
            ([], [item]) => Some(*item),
            _ => None,
        };
        findings.push((first, first.col + (end - start) as u32, word, number, paren, labels, item));
        i = end;
    }
    for (at, end, word, number, paren, labels, item) in findings {
        if let Some(item) = item {
            refer_to_item(report, (at, end), (&word, &number, paren), item, cleveref);
            continue;
        }
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

/// The label type of cleveref's warning for a reference it has no format
/// for: `cref reference format for label type `task' undefined`, and the
/// range form.
fn cref_type(message: &str) -> Option<&str> {
    let rest = &message[message.find("reference ")?..];
    let rest = &rest[rest.find(" format for label type `")? + " format for label type `".len()..];
    let (kind, tail) = rest.split_once('\'')?;
    tail.trim_start().starts_with("undefined").then_some(kind)
}

fn undefined_cref_format(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let edits = report.edits();
    let load = analysis.facts.loads.iter().find(|l| l.kind.is_package() && l.name == "cleveref" && l.by.is_none());
    // Once per type: the first site that warned on every path, else the first.
    let mut types: Vec<(&str, Span, bool)> = Vec::new();
    for (i, o) in analysis.facts.occurrences.iter().enumerate() {
        let Some(kind) = (o.kind == OccKind::Message).then(|| cref_type(&o.key)).flatten() else { continue };
        if !view.document(o.span.file) {
            continue;
        }
        let sure = o.certain && crate::linter::shared::doubt(analysis, i).is_none();
        match types.iter_mut().find(|t| t.0 == kind) {
            Some(t) if sure && !t.2 => *t = (kind, o.span, sure),
            Some(_) => {}
            None => types.push((kind, o.span, sure)),
        }
    }
    for (kind, span, sure) in types {
        let severity = if sure { Severity::Warning } else { Severity::Hint };
        let maybe = if sure { "" } else { " on some paths of the analysis" };
        // A type made of unknown text has no name to write.
        let plain = !kind.is_empty() && kind.chars().all(|c| c.is_alphanumeric() || "@*-_:.".contains(c));
        if !plain {
            let message = format!("cleveref has no format for this reference's label type{maybe}, so it prints `??`");
            report.add_as(Severity::Hint, span, "unknown", message, Some("name the type with `\\crefname`".into()));
            continue;
        }
        let name = format!("\\crefname{{{kind}}}{{{kind}}}{{{kind}s}}");
        let message = format!("cleveref has no format for label type `{kind}`{maybe}, so the reference prints `??`");
        let fix = load.and_then(|l| edits.insert_after_load(l, &name));
        let fix = Fix::new(format!("name the type: `{name}`"), Applicability::Unsafe, fix);
        report.add_fix_as(severity, span, kind, message, fix);
    }
}
