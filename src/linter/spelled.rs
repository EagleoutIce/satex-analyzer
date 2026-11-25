//! A word the document's own command sets, typed out by hand elsewhere: the
//! run knows which characters a macro's body set, so the word is found
//! wherever the author wrote it again instead of the command.

use crate::engine::tex::{Span, Tok};
use crate::fixed::HashMap;
use crate::linter::fix::Applicability;
use crate::linter::text::Rewrite;
use crate::linter::{Category, Report, Rule, Severity};
use crate::util::equiv;

pub static RULES: &[Rule] = &[Rule {
    code: "spelled-out-macro",
    since: "0.2.6",
    category: Category::Style,
    severity: Severity::Info,
    summary: "word typed out that a command of the document sets",
    explanation: "\
A name the document keeps in one place, `\\newcommand\\model{TinyNet}` or a
glossary entry `name={cellR}`, should be written through it everywhere. A
copy typed out by hand stays behind when the name changes, and a glossary
misses the use.

The rule reads the characters the run set. A word whose characters come from
the replacement text of one of the document's own macros is that macro's
word, when the macro sets that one word and nothing else. The same word typed
in the document's prose is reported, once per place, when it looks like a name
(`TinyNet`, `cellR`, `CI`): an everyday word a macro holds (`main`) is not. \"The same\" is a level of
the one equivalence ladder every comparison of text uses
(`lints.spelled-out-macro.equivalence`): `exact`, `glyphs` (the default: a
ligature is its letters, any spaces one space), `case` or `letters`. Words
shorter than `min-length` (3) are left alone.

Fix by writing the command there: `\\model{}` or `\\gls{cellr}`. It is unsafe:
the command may set more than its word in another context.",
    references: &[],
    fixes: &[Applicability::Unsafe],
    engines: &[],
    run: spelled_out,
}];

/// Whether `word` looks like a name a document keeps in one place: a capital after its first letter
/// (`TinyNet`, `SaTeX`, `cellR`), a digit beside letters, or all capitals (`CI`). An everyday word a macro
/// happens to hold (`main`, `Integer`) is typed in prose for its own sake.
fn distinctive(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    let upper = word.chars().filter(|c| c.is_uppercase()).count();
    letters > 0
        && (word.chars().skip(1).any(char::is_uppercase)
            || word.chars().any(|c| c.is_ascii_digit())
            || upper == letters)
}

/// The command that writes a macro's word: a glossary entry's name macro
/// (`\glo@cellr@name`) is written `\gls{cellr}`, any other macro by its name.
fn command(name: &str) -> String {
    match name.strip_prefix("glo@").and_then(|k| k.strip_suffix("@name")) {
        Some(key) => format!("\\gls{{{key}}}"),
        None => format!("\\{name}{{}}"),
    }
}

fn spelled_out(report: &mut Report) {
    let analysis = report.analysis;
    let view = report.text();
    let ours = |span: Span| view.document(span.file);
    let cfg = &analysis.settings.lints.spelled_out_macro;
    let same = |w: &str| equiv::key(w, cfg.equivalence);
    // the document's one-word macros: each character token of the body, by its source position
    // the macros the document's own text calls
    let used: crate::fixed::HashSet<crate::engine::tex::Sym> =
        analysis.facts.expansions.iter().filter(|e| ours(e.span)).map(|e| e.name).collect();
    let mut body: HashMap<Span, usize> = HashMap::default();
    let mut words: Vec<(String, String)> = Vec::new();
    for d in analysis.facts.defs.iter() {
        let Some(mac) = &d.mac else { continue };
        let toks = &mac.replacement_text;
        let word: Option<String> = toks
            .iter()
            .map(|t| match t.tok {
                Tok::Chr(c, _) if c.is_alphanumeric() => Some(c),
                _ => None,
            })
            .collect();
        let Some(word) = word.filter(|w| w.chars().count() >= cfg.min_length && distinctive(w)) else {
            continue;
        };
        if !toks.iter().all(|t| ours(t.span)) {
            continue;
        }
        let name = analysis.interner.name(d.name).to_string();
        // a command of the document's own: its name written in the document's files (`\\newcommand\\model`)
        // and writable there, never internal (`\\reserved@a`, `\\@tempa`), or a glossary entry's name,
        // which glossaries builds
        let glossary = name.starts_with("glo@") && name.ends_with("@name");
        if !glossary && (!d.spelled || !crate::linter::shared::writable(&name) || !used.contains(&d.name)) {
            continue;
        }
        // a glossary entry's name macro wins over its other copies (`text`, `first`)
        let k = match words.iter().position(|(w, _)| *w == same(&word)) {
            Some(k) if !name.starts_with("glo@") || name.ends_with("@name") => {
                if name.ends_with("@name") {
                    words[k].1 = name;
                }
                k
            }
            Some(k) => k,
            None => {
                words.push((same(&word), name));
                words.len() - 1
            }
        };
        for t in toks.iter() {
            body.insert(t.span, k);
        }
    }
    if words.is_empty() {
        return;
    }
    // the text the run wrote to a file from a line of the document (`\\caption`, `\\section`)
    let written: Vec<(crate::engine::tex::FileId, u32, &str)> = (analysis.facts.occurrences.iter())
        .filter(|o| o.kind == crate::engine::builtins::OccKind::Write)
        .filter_map(|o| o.call.map(|(_, at)| (at.file, at.line, o.key.as_str())))
        .collect();
    let by_word: HashMap<&str, usize> = words.iter().enumerate().map(|(k, (w, _))| (w.as_str(), k)).collect();
    let mut findings = Vec::new();
    let mut i = 0;
    while i < view.len() {
        if !view.is_word_prose(i) || !view.char_at(i).is_some_and(char::is_alphanumeric) {
            i += 1;
            continue;
        }
        let start = i;
        while i < view.len() && view.char_at(i).is_some_and(char::is_alphanumeric) && (i == start || view.joined(i)) {
            i += 1;
        }
        let text = view.slice(start..i);
        let Some(&k) = by_word.get(same(text).as_str()) else { continue };
        // typed here: no character of the word came from a macro body, and it stands literally in a file of ours
        let typed = (start..i).all(|j| view.span(j).is_some_and(|s| !body.contains_key(&s)));
        if !typed || view.literal(start..i).is_none_or(|(first, _)| !ours(first)) {
            continue;
        }
        // a word the run also wrote out (a caption's or heading's text goes to the `.lof`/`.toc`) stands in
        // a moving argument, where glossaries advises `\\glsentrytext` (no first-use, no link)
        let at = view.span(start).unwrap_or_default();
        let moving = written.iter().any(|(file, line, key)| *file == at.file && *line == at.line && key.contains(text));
        let want = match (moving, words[k].1.strip_prefix("glo@").and_then(|n| n.strip_suffix("@name"))) {
            (true, Some(key)) => format!("\\glsentrytext{{{key}}}"),
            _ => command(&words[k].1),
        };
        let message =
            format!("`{text}` is what `{want}` sets: write the command, so a renamed {text} changes here too");
        findings.push(Rewrite::new(start..i, "typed", message, want.clone()).fix(format!("write `{want}`")));
    }
    for finding in findings {
        report.rewrite(finding);
    }
}
