//! A numbered display nothing refers to: the star form sets the same
//! mathematics without spending a number.

use crate::fixed::{HashMap, HashSet};
use std::rc::Rc;

use crate::analysis::machine::Analysis;
use crate::engine::builtins::OccKind;
use crate::engine::tex::{CatcodeTable, Span};
use crate::linter::fix::{Applicability, Edit, Fix, Pos, Scan, Text};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};

pub static RULES: &[Rule] = &[
    Rule {
        code: "prefer-cleveref",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "hand-written name before a reference",
        explanation: "\
In `Section~\\ref{sec:x}` the word `Section` is typed by hand.  If the
section becomes an appendix, the word is wrong.  cleveref's `\\cref` and
`\\Cref` print the word from the label's counter.

A finding needs a word, or an abbreviation with a period, right before a
`\\ref`.  The word must name the kind of the label: its counter, the
counter's `\\<counter>name` or a `\\crefname` text, singular, plural or a
prefix.  So `see~\\ref{x}` gives no finding, and neither do a label the run
never found, `\\eqref`, `\\pageref` and `(\\ref{x})`.
`Sections~\\ref{a} and~\\ref{b}` is one finding.

With cleveref loaded, a finding needs cleveref to print the same word
(`Sec.` gives none where it prints `Section`), and the fix writes
`\\Cref{sec:x}`, or `\\cref` after a lowercase word.  Without cleveref, the
fix also loads it after the last `\\usepackage` of the preamble.

Fix by writing `\\Cref{sec:x}` and loading cleveref.",
        references: &[Ref { source: Source::Cleveref, at: "2" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: prefer_cleveref,
    },
    Rule {
        code: "bare-reference",
        since: "0.2.4",
        category: Category::Style,
        severity: Severity::Info,
        summary: "reference without a name",
        explanation: "\
`see \\ref{sec:x}` sets `see 2`: a number, not what it numbers.

A finding needs a `\\ref` to a label of a named kind (a sectioning unit, a
figure, anything with a `\\<counter>name` or cleveref name) with no word,
`\\S` or section sign before it.  Equations in parentheses, items, footnotes and the
next `\\ref` of a list never count.

The fix writes what the document uses: its own one-argument reference
command (as last defined), else `\\cref` (`\\Cref` opening a sentence), else
`\\autoref`, else `\\<counter>name~\\ref`, else `\\S~\\ref` for a section.",
        references: &[Ref { source: Source::Cleveref, at: "2" }, Ref { source: Source::Hyperref, at: "4" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: bare_reference,
    },
    Rule {
        code: "reference-name-mismatch",
        since: "0.2.4",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "name does not match the label",
        explanation: "\
`Subsection~\\ref{sec:a}` or `Chapter~\\ref{sec:a}` for a section, or
`Figure~\\ref{tab:b}`, names the wrong kind of thing.  The general word for a
deeper unit is fine (a subsection is a section), and so is `Appendix` for a
section or chapter.  Names come from the counter, `\\<counter>name` (babel)
and cleveref.

Fix by writing the label's own name, or `\\Cref`.",
        references: &[Ref { source: Source::Cleveref, at: "2" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: reference_name_mismatch,
    },
    Rule {
        code: "unreferenced-numbered-display",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "numbered display never referenced",
        explanation: "\
`\\begin{equation}` and its relatives step a counter and print a number
beside the display.  The starred forms set the same mathematics and spend no
number.

satex decides *numbered* by running the document: `\\refstepcounter`
redefines `\\@currentlabel` (clsguide § counters), so an environment whose
body redefined it stepped a counter.
`\\@captype` and `\\@listctr` say the number went to something inside, and
amsmath's `\\notag`, `\\nonumber` and `\\tag` say the display governs itself.

Fix by writing the starred environment and dropping the label that nothing
reads.",
        references: &[Ref { source: Source::Amsmath, at: "3.1" }, Ref { source: Source::Clsguide, at: "counters" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: unreferenced_numbered_display,
    },
];

/// What the source says about one display: the `\label`s in its body, and
/// whether it governs its own number.  Neither leaves a fact.
struct Body {
    labels: Vec<(Pos, String)>,
    tagged: bool,
}

fn unreferenced_numbered_display(report: &mut Report) {
    let analysis = report.analysis();
    // clsguide § counters: `\refstepcounter` redefines `\@currentlabel`, so
    // an environment whose body redefined it stepped a counter.  `\@captype`
    // and `\@listctr` say the kernel gave that number to a caption or an
    // item inside, not to the environment itself.
    let mut numbered: HashSet<&str> = HashSet::default();
    let mut inner: HashSet<&str> = HashSet::default();
    for def in &analysis.facts.defs {
        let Some(env) = def.context.last() else { continue };
        let set = match analysis.interner.name(def.name) {
            "@currentlabel" => &mut numbered,
            "@captype" | "@listctr" => &mut inner,
            _ => continue,
        };
        set.insert(analysis.interner.name(*env));
    }
    let referenced: HashSet<&str> =
        analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Ref).map(|o| o.key.as_str()).collect();
    let declared = report.index().labels();
    let view = report.text();
    let edits = report.edits();
    let closes = closes(analysis);
    let starred =
        |name: &str| analysis.interner.lookup(&format!("{name}*")).is_some_and(|sym| analysis.env.is_defined(sym));
    let mut findings: Vec<(Span, String, String, Vec<Edit>)> = Vec::new();
    // One body scan per candidate, and the candidates are the displays the
    // document wrote: the whole rule stays linear in the source.
    let mut seen: HashSet<Span> = HashSet::default();
    for occurrence in &analysis.facts.occurrences {
        if occurrence.kind != OccKind::BeginEnvironment || !occurrence.certain {
            continue;
        }
        let name = occurrence.key.as_str();
        if name.ends_with('*') || !numbered.contains(name) || inner.contains(name) || !starred(name) {
            continue;
        }
        if !view.document(occurrence.span.file) || !seen.insert(occurrence.span) {
            continue;
        }
        let Some((path, text)) = edits.text(occurrence.span.file) else { continue };
        let Some((begin_star, from)) = star_of(&text, occurrence.span, "begin", name) else { continue };
        let Some(end) = closes.get(&occurrence.span) else { continue };
        let Some((end_star, _)) = star_of(&text, *end, "end", name) else { continue };
        let Some(body) = read_body(&text, from, Pos::new(end.line, end.col), &analysis.settings.conventions.math.tags)
        else {
            continue;
        };
        if body.tagged {
            continue;
        }
        if body.labels.iter().any(|(_, key)| referenced.contains(key.as_str())) {
            continue;
        }
        let here: Vec<&str> =
            body.labels.iter().map(|(_, key)| key.as_str()).filter(|key| declared.contains_key(key)).collect();
        let message = match here.as_slice() {
            [] => format!("{name} stepped a counter and carries no label, so its number cannot be referenced"),
            [one] => format!("{name} is numbered and its label `{one}` is never referenced"),
            many => format!(
                "{name} is numbered and none of its labels ({}) is referenced",
                many.iter().map(|key| format!("`{key}`")).collect::<Vec<_>>().join(", ")
            ),
        };
        let mut edit = vec![
            Edit { path: path.clone(), start: begin_star, end: begin_star, replacement: "*".into() },
            Edit { path: path.clone(), start: end_star, end: end_star, replacement: "*".into() },
        ];
        for (at, key) in &body.labels {
            let span = Span { file: occurrence.span.file, line: at.line, col: at.col };
            edit.extend(edits.delete_command(span, key).unwrap_or_default());
        }
        findings.push((occurrence.span, name.to_string(), message, edit));
    }
    for (span, name, message, edit) in findings {
        if report.unread("referenced").is_some() {
            report.add_absent("referenced", span, &name, message, None);
            continue;
        }
        report.add_fix(
            span,
            &name,
            message,
            Fix::new(format!("write the starred {name}"), Applicability::Unsafe, Some(edit)),
        );
    }
}

/// Which `\end` closes each `\begin`, paired the way the run met them, so a
/// display nested in one of its own kind still finds its own.
fn closes(analysis: &Analysis) -> HashMap<Span, Span> {
    let mut open: HashMap<&str, Vec<Span>> = HashMap::default();
    let mut out = HashMap::default();
    for occurrence in &analysis.facts.occurrences {
        match occurrence.kind {
            OccKind::BeginEnvironment => open.entry(occurrence.key.as_str()).or_default().push(occurrence.span),
            OccKind::EndEnvironment => {
                if let Some(begin) = open.get_mut(occurrence.key.as_str()).and_then(Vec::pop) {
                    out.insert(begin, occurrence.span);
                }
            }
            _ => {}
        }
    }
    out
}

/// Where a `*` belongs in the `\begin{name}` or `\end{name}` at `span`, and
/// where the command ends, when the source really spells it there.
fn star_of(text: &Rc<Text>, span: Span, cs: &str, name: &str) -> Option<(Pos, Pos)> {
    let mut scan = Scan::at(text, Pos::new(span.line, span.col), CatcodeTable::latex())?;
    let head = scan.advance()?;
    if scan.name(&head)? != cs {
        return None;
    }
    let group = scan.group()?;
    (text.slice(group.inner_start, group.inner_end)?.trim() == name).then_some((group.inner_end, group.end))
}

/// The body between a display's two halves, read the way TeX's mouth reads
/// the file.
fn read_body(text: &Rc<Text>, from: Pos, to: Pos, tags: &[String]) -> Option<Body> {
    let mut scan = Scan::at(text, from, CatcodeTable::latex())?;
    let mut body = Body { labels: Vec::new(), tagged: false };
    while let Some(lexeme) = scan.advance() {
        if lexeme.start >= to {
            break;
        }
        let (label, tag) = match scan.name(&lexeme) {
            Some(cs) => (cs == "label", tags.iter().any(|t| *t == cs)),
            None => (false, false),
        };
        body.tagged |= tag;
        if label {
            let key = scan.group()?;
            body.labels.push((lexeme.start, text.slice(key.inner_start, key.inner_end)?.trim().to_string()));
        }
    }
    Some(body)
}

/// A hand-written name and the `\ref`s it stands in front of.
struct Named {
    file: crate::engine::tex::FileId,
    path: String,
    line: u32,
    /// Character columns, 0-based: the name's start and the end of the last `}`.
    start: usize,
    end: usize,
    word: String,
    labels: Vec<String>,
    source: String,
}

/// The `\ref{label}` at `at`: the label and the column after its `}`.
fn ref_at(line: &[char], at: usize) -> Option<(String, usize)> {
    let head: Vec<char> = r"\ref{".chars().collect();
    if line.get(at..at + head.len())? != head.as_slice() {
        return None;
    }
    let close = at + head.len() + line[at + head.len()..].iter().position(|&c| c == '}')?;
    Some((line[at + head.len()..close].iter().collect(), close + 1))
}

/// One `\ref{label}` typed in the document's own text.
struct Site {
    span: Span,
    path: String,
    /// The source line, and the columns (0-based) of the `\` and after the `}`.
    line: Vec<char>,
    at: usize,
    end: usize,
    label: String,
}

/// Every `\ref{label}` of the document's own files, once per place, in order.
fn ref_sites(report: &Report) -> Vec<Site> {
    let (analysis, view, edits) = (report.analysis(), report.text(), report.edits());
    let mut spans: Vec<Span> = (analysis.facts.occurrences.iter())
        .filter(|o| o.kind == OccKind::Ref && view.prose_at(o.span))
        .map(|o| o.span)
        .collect();
    spans.sort_by_key(|s| (s.file, s.line, s.col));
    spans.dedup();
    spans
        .into_iter()
        .filter_map(|span| {
            let (path, text) = edits.text(span.file)?;
            let line: Vec<char> = text.line(span.line).chars().collect();
            let at = span.col as usize - 1;
            let (label, end) = ref_at(&line, at)?;
            // The word before it decides: one set in math, code or
            // typewriter type (`\texttt{see \ref{a}}`) is no prose.
            let before = line[..at].iter().rposition(|c| !c.is_whitespace() && *c != '~');
            let set = before.and_then(|k| view.at_span(Span { col: k as u32 + 1, ..span }));
            if set.is_some_and(|i| !view.is_text(i)) {
                return None;
            }
            Some(Site { span, path, line, at, end, label })
        })
        .collect()
}

/// LaTeX's sectioning units, outermost first (`\part` to `\subparagraph`).
const SECTIONING: [&str; 7] =
    ["part", "chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph"];

/// The name tied or spaced directly before column `at`: letters with an
/// optional period, and where it starts.
fn name_before(line: &[char], at: usize) -> Option<(usize, String)> {
    let sep = at.checked_sub(1).filter(|&k| matches!(line[k], '~' | ' '))?;
    let mut start = sep;
    if start > 0 && line[start - 1] == '.' {
        start -= 1;
    }
    let letters = start;
    while start > 0 && line[start - 1].is_alphabetic() {
        start -= 1;
    }
    let word: String = line[start..sep].iter().collect();
    let odd = start > 0 && (line[start - 1].is_alphanumeric() || matches!(line[start - 1], '\\' | '@' | '_' | ':'));
    (letters - start >= 2 && !odd).then_some((start, word))
}

/// The counter each label was set after, read off what `\label` wrote
/// (scope-correct, unlike a counter register read later): cleveref's
/// `\newlabel{k@cref}{{[section][1][]1}...}`, else hyperref's anchor
/// `section.1` at the end of `\newlabel{k}`.
pub(crate) fn label_counters(analysis: &Analysis) -> HashMap<&str, String> {
    let lines: Vec<&str> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Write && o.key.starts_with("\\newlabel"))
        .map(|o| o.key.as_str())
        .collect();
    let counter =
        |name: &str| analysis.interner.lookup(&format!("c@{name}")).is_some_and(|sym| analysis.env.is_defined(sym));
    let labels: Vec<&str> =
        analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Label).map(|o| o.key.as_str()).collect();
    let mut out = HashMap::default();
    for &label in &labels {
        let cref = format!("\\newlabel{label}@cref[");
        let plain = format!("\\newlabel{label}");
        let found = lines.iter().find_map(|l| {
            let rest = l.strip_prefix(&cref)?;
            Some(rest[..rest.find(']')?].to_string())
        });
        let found = found.or_else(|| {
            // The anchor is glued to the text before it, so the counter is
            // the longest tail of that text some `\c@` register is named by.
            // A label that is a prefix of another is not told apart.
            if labels.iter().any(|o| o.len() > label.len() && o.starts_with(label)) {
                return None;
            }
            let rest = lines.iter().find_map(|l| l.strip_prefix(&plain).filter(|r| !r.starts_with('@')))?;
            let tail = rest.len() - rest.chars().rev().take_while(|c| c.is_ascii_alphanumeric() || *c == '.').count();
            let name = &rest[tail..rest[tail..].find('.')? + tail];
            (0..name.len())
                .filter(|&k| name.is_char_boundary(k))
                .map(|k| &name[k..])
                .find(|n| counter(n))
                .map(str::to_string)
        });
        if let Some(c) = found.filter(|c| !c.is_empty()) {
            out.entry(label).or_insert(c);
        }
    }
    out
}

/// Whether `word` names the kind of thing `counter` counts: the counter's
/// own name, its `\<counter>name` macro, or a cleveref name for it, in the
/// singular, the plural or as an abbreviation with a period.
pub(crate) fn names_counter(analysis: &Analysis, cleveref: bool, counter: &str, word: &str) -> bool {
    use crate::linter::shared::macro_text;
    let mut names = vec![counter.to_string()];
    names.extend(macro_text(analysis, &format!("{counter}name")));
    if cleveref {
        for form in ["cref", "Cref"] {
            for tail in ["", "@plural"] {
                names.extend(macro_text(analysis, &format!("{form}@{counter}@name{tail}")));
            }
        }
    }
    let lower = word.to_lowercase();
    let (stem, abbreviated) = lower.strip_suffix('.').map_or((lower.as_str(), false), |w| (w, true));
    names.iter().map(|n| n.to_lowercase()).any(|n| {
        if abbreviated {
            n.starts_with(stem)
        } else {
            n == stem || n.strip_suffix('s') == Some(stem) || stem.strip_suffix('s') == Some(n.as_str())
        }
    })
}

fn prefer_cleveref(report: &mut Report) {
    let analysis = report.analysis();
    let edits = report.edits();
    let cleveref = crate::linter::shared::loaded(analysis, "cleveref");
    let counters = label_counters(analysis);
    let all_counters: std::collections::BTreeSet<&str> =
        analysis.facts.defs.iter().filter_map(|d| analysis.interner.name(d.name).strip_prefix("c@")).collect();
    let mut found: Vec<Named> = Vec::new();
    for Site { span, path, line, at, end, label } in ref_sites(report) {
        // After a plural name, `, ` `and~` or `or~` carry the list on.
        let joined = found.last_mut().filter(|n| {
            let gap: String = line.get(n.end..at).unwrap_or_default().iter().collect();
            let bare: String = gap.chars().filter(|c| !matches!(c, ' ' | ',' | '~')).collect();
            let plural = n.word.to_lowercase().ends_with('s');
            let same = counters.get(label.as_str()) == counters.get(n.labels[0].as_str());
            n.file == span.file
                && n.line == span.line
                && n.end <= at
                && plural
                && same
                && (matches!(bare.as_str(), "and" | "or") || (bare.is_empty() && gap.contains(',')))
        });
        if let Some(last) = joined {
            last.end = end;
            last.labels.push(label);
            last.source = line[last.start..end].iter().collect();
            continue;
        }
        let Some((start, word)) = name_before(&line, at) else { continue };
        // Only a word that names what the label counts is a reference name.
        // A label of unknown kind may be named for any counter the document has.
        let named = match counters.get(label.as_str()) {
            Some(c) => names_counter(analysis, cleveref, c, &word),
            None => all_counters.iter().any(|c| names_counter(analysis, cleveref, c, &word)),
        };
        if !named {
            continue;
        }
        let source = line[start..end].iter().collect();
        found.push(Named { file: span.file, path, line: span.line, start, end, word, labels: vec![label], source });
    }
    for n in found {
        let at = Span { file: n.file, line: n.line, col: n.start as u32 + 1 };
        let labels = n.labels.join(",");
        let command = if n.word.chars().next().is_some_and(char::is_uppercase) { r"\Cref" } else { r"\cref" };
        // The fix changes nothing on the page only where cleveref sets
        // the very word written: `Fig.` stays when it would set `Figure`.
        let form = &command[1..];
        let tail = if n.labels.len() > 1 { "@plural" } else { "" };
        let same = counters.get(n.labels[0].as_str()).is_some_and(|c| {
            crate::linter::shared::macro_text(analysis, &format!("{form}@{c}@name{tail}")).is_some_and(|t| t == n.word)
        });
        if cleveref && !same {
            continue;
        }
        let call = format!("{command}{{{labels}}}");
        let start = Pos::new(n.line, n.start as u32 + 1);
        let replace = Edit { path: n.path, start, end: Pos::new(n.line, n.end as u32 + 1), replacement: call.clone() };
        // Loaded, it asks for nothing more.
        let mut fixed = edits.ensure_package("cleveref");
        fixed.iter_mut().for_each(|e| e.push(replace.clone()));
        let (name, message, load) = match cleveref {
            true => ("cleveref", format!("`{call}` prints `{}` itself", n.source), ""),
            false => (
                "load",
                format!("with cleveref, `{call}` prints `{}` itself", n.source),
                "load cleveref (after hyperref) and ",
            ),
        };
        let fix = Fix::new(format!("{load}write `{call}`"), Applicability::Unsafe, fixed);
        report.add_fix(at, name, message, fix);
    }
}

/// Kinds a bare number never says enough for: those with a name, but not an
/// equation (set in parentheses by convention), an item or a footnote.
fn has_name(analysis: &Analysis, cleveref: bool, kind: &str) -> bool {
    use crate::linter::shared::macro_text;
    let quiet = matches!(kind, "equation" | "subequation" | "footnote" | "mpfootnote" | "page" | "line")
        || kind.starts_with("enum");
    !quiet
        && (SECTIONING.contains(&kind)
            || macro_text(analysis, &format!("{kind}name")).is_some()
            || (cleveref && macro_text(analysis, &format!("cref@{kind}@name")).is_some()))
}

/// The document's own one-argument commands that refer to a label: by the
/// meaning the run gave them last, `Some(kind)` for one that writes the name
/// of a kind before `\ref{#1}`, `None` for one that hands it to `\cref`,
/// `\Cref` or `\autoref`.
fn own_ref_commands(analysis: &Analysis, cleveref: bool, kinds: &[&str]) -> Vec<(String, Option<String>)> {
    use crate::engine::tex::{Catcode, Meaning, Tok, text_of};
    let mut names: Vec<crate::engine::tex::Sym> =
        analysis.facts.defs.iter().filter(|d| d.package.is_none()).map(|d| d.name).collect();
    names.sort_unstable_by_key(|n| n.0);
    names.dedup();
    let mut out = Vec::new();
    for sym in names {
        let Meaning::Macro(m) = analysis.env.meaning(sym) else { continue };
        let toks = &m.replacement_text;
        if m.arity() != 1 {
            continue;
        }
        let call = toks.windows(4).position(|w| {
            matches!(w[1].tok, Tok::Chr(_, Catcode::Begin))
                && matches!(w[2].tok, Tok::Param(1))
                && matches!(w[3].tok, Tok::Chr(_, Catcode::End))
                && matches!(w[0].tok, Tok::Cs(c) if matches!(analysis.interner.name(c), "ref" | "cref" | "Cref" | "autoref"))
        });
        let Some(k) = call else { continue };
        let Tok::Cs(c) = toks[k].tok else { continue };
        let kind = match analysis.interner.name(c) {
            "ref" => {
                let before = text_of(&toks[..k], &analysis.interner);
                let word = before.trim_end_matches([' ', '~', '\u{a0}']).rsplit([' ', '~']).next().unwrap_or_default();
                match kinds.iter().find(|c| names_counter(analysis, cleveref, c, word)) {
                    Some(c) => Some(c.to_string()),
                    None => continue,
                }
            }
            _ => None,
        };
        out.push((analysis.interner.name(sym).to_string(), kind));
    }
    out
}

/// Whether the text before column `at` ends a sentence, or the line holds
/// nothing before it and the line above ends one (or there is none).
fn sentence_starts(lines: (&[char], Option<&str>), at: usize) -> bool {
    let before: String = lines.0[..at].iter().collect();
    let before = before.trim_end_matches([' ', '~']);
    let last = match before.trim_start().is_empty() {
        true => lines.1.map_or(Some('.'), |l| l.trim_end().chars().last()),
        false => before.chars().last(),
    };
    last.is_none_or(|c| matches!(c, '.' | '!' | '?'))
}

fn bare_reference(report: &mut Report) {
    use crate::linter::shared::{loaded, macro_text};
    let analysis = report.analysis();
    let edits = report.edits();
    let counters = label_counters(analysis);
    if counters.is_empty() {
        return;
    }
    let (cleveref, hyperref) = (loaded(analysis, "cleveref"), loaded(analysis, "hyperref"));
    let mut kinds: Vec<&str> = counters.values().map(String::as_str).chain(SECTIONING).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let own = own_ref_commands(analysis, cleveref, &kinds);
    let mut last: Option<(Span, usize)> = None;
    let mut found = Vec::new();
    for site in ref_sites(report) {
        let previous = last.replace((site.span, site.end));
        let Some(kind) = counters.get(site.label.as_str()) else { continue };
        if !has_name(analysis, cleveref, kind) {
            continue;
        }
        let before: String = site.line[..site.at].iter().collect();
        let before = before.trim_end_matches([' ', '~']);
        if before.ends_with('(') || before.ends_with("\\S") || before.ends_with('§') {
            continue;
        }
        // A name before it, right or wrong, is the other rules' business.
        if name_before(&site.line, site.at)
            .is_some_and(|(_, w)| kinds.iter().any(|c| names_counter(analysis, cleveref, c, &w)))
        {
            continue;
        }
        // The next `\ref` of a list: `Sections~\ref{a} and~\ref{b}`, `\ref{a}--\ref{b}`.
        if let Some((span, end)) = previous
            && (span.file, span.line) == (site.span.file, site.span.line)
        {
            let gap: String = site.line[end..site.at].iter().filter(|c| !matches!(c, ' ' | '~' | ',')).collect();
            if matches!(gap.as_str(), "" | "and" | "or" | "to" | "-" | "--") {
                continue;
            }
        }
        let above = (site.span.line > 1).then(|| edits.text(site.span.file)).flatten();
        let above = above.as_ref().map(|(_, t)| t.line(site.span.line - 1).to_string());
        let upper = sentence_starts((&site.line, above.as_deref()), site.at);
        found.push((site, kind.clone(), upper));
    }
    for (site, kind, upper) in found {
        let key = &site.label;
        // What the document itself writes for this kind, as the run left it.
        let own_kind = own.iter().find(|(_, k)| k.as_deref() == Some(kind.as_str()));
        let own_any = own.iter().find(|(_, k)| k.is_none());
        let name_command = macro_text(analysis, &format!("{kind}name")).map(|_| format!("\\{kind}name~\\ref{{{key}}}"));
        let call = match (own_kind.or(own_any), cleveref, hyperref) {
            (Some((name, _)), ..) => Some(format!("\\{name}{{{key}}}")),
            (None, true, _) => Some(format!("{}{{{key}}}", if upper { r"\Cref" } else { r"\cref" })),
            (None, false, true) => Some(format!("\\autoref{{{key}}}")),
            _ => name_command.or_else(|| SECTIONING.contains(&kind.as_str()).then(|| format!("\\S~\\ref{{{key}}}"))),
        };
        let typed: String = site.line[site.at..site.end].iter().collect();
        let message = format!("`{typed}` sets only the number of a {kind}, not what it numbers");
        let Some(call) = call else {
            report.add(site.span, "bare", message, Some(format!("write the name of the {kind} before it")));
            continue;
        };
        let edit = Edit {
            path: site.path,
            start: Pos::new(site.span.line, site.at as u32 + 1),
            end: Pos::new(site.span.line, site.end as u32 + 1),
            replacement: call.clone(),
        };
        let fix = Fix::new(format!("write `{call}`"), Applicability::Unsafe, Some(vec![edit]));
        report.add_fix(site.span, "bare", message, fix);
    }
}

/// The name a kind is called by on the page: its `\<kind>name` text, else
/// cleveref's, else the counter's own name, capitalized like `word`.
fn kind_name(analysis: &Analysis, cleveref: bool, kind: &str, word: &str) -> String {
    use crate::linter::shared::macro_text;
    let name = macro_text(analysis, &format!("{kind}name"))
        .or_else(|| cleveref.then(|| macro_text(analysis, &format!("Cref@{kind}@name"))).flatten())
        .unwrap_or_else(|| kind.to_string());
    let mut chars = name.chars();
    let first = chars.next().map(|c| match word.chars().next().is_some_and(char::is_uppercase) {
        true => c.to_uppercase().collect::<String>(),
        false => c.to_lowercase().collect(),
    });
    first.unwrap_or_default() + chars.as_str()
}

fn reference_name_mismatch(report: &mut Report) {
    use crate::linter::shared::{loaded, macro_text};
    let analysis = report.analysis();
    let counters = label_counters(analysis);
    let cleveref = loaded(analysis, "cleveref");
    let comparable: Vec<&str> = SECTIONING.iter().copied().chain(["figure", "table"]).collect();
    let rank = |c: &str| SECTIONING.iter().position(|s| *s == c);
    let appendix = macro_text(analysis, "appendixname");
    let mut found = Vec::new();
    for site in ref_sites(report) {
        let Some(kind) = counters.get(site.label.as_str()).filter(|k| comparable.contains(&k.as_str())) else {
            continue;
        };
        let Some((start, word)) = name_before(&site.line, site.at) else { continue };
        if names_counter(analysis, cleveref, kind, &word) {
            continue;
        }
        // An appendix is a section or chapter under `\appendixname`.
        if matches!(kind.as_str(), "section" | "chapter")
            && appendix.as_deref().is_some_and(|a| crate::linter::xref::names(a, &word))
        {
            continue;
        }
        let Some(said) = comparable.iter().copied().find(|c| names_counter(analysis, cleveref, c, &word)) else {
            continue;
        };
        // A deeper unit may go by the general word: a subsection is a section.
        let general = matches!(said, "section" | "subsection") && rank(said) < rank(kind);
        if general {
            continue;
        }
        found.push((site, start, word, said, kind.clone()));
    }
    for (site, start, word, said, kind) in found {
        let right = kind_name(analysis, cleveref, &kind, &word);
        let message = format!("`{word}` names a {said}, but `{}` labels a {kind}", site.label);
        let edit = Edit {
            path: site.path,
            start: Pos::new(site.span.line, start as u32 + 1),
            end: Pos::new(site.span.line, (start + word.chars().count()) as u32 + 1),
            replacement: right.clone(),
        };
        let fix = Fix::new(format!("write `{right}`"), Applicability::Unsafe, Some(vec![edit]));
        let at = Span { col: start as u32 + 1, ..site.span };
        report.add_fix(at, "mismatch", message, fix);
    }
}
