//! A numbered display nothing refers to: the star form sets the same
//! mathematics without spending a number.

use crate::fixed::{HashMap, HashSet};
use std::rc::Rc;

use crate::builtins::OccKind;
use crate::lint::fix::{Applicability, Edit, Fix, Pos, Scan, Text};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::tex::{CatcodeTable, Span};

pub static RULES: &[Rule] = &[
    Rule {
        code: "prefer-cleveref",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a hand-written name in front of `\\ref` is what cleveref sets from the label itself",
        explanation: "\
`Section~\\ref{sec:x}` writes the name of the thing by hand, so renaming a
section into an appendix leaves the prose behind.  cleveref's `\\cref` and
`\\Cref` read the name from the label's counter.

The rule reads the `\\ref` sites the run recorded and takes a word, or
abbreviation with a period, tied or spaced directly in front of one.  The
word must name the label's kind, read from the counter the label was set
after: the counter's name, its `\\<counter>name` macro or a `\\crefname` text,
in singular, plural or as a prefix.  `see~\\ref{x}` stays silent, and so does
a label the run never found.  `\\eqref`, `\\pageref` and `(\\ref{x})` have no word in
front and stay silent.  `Sections~\\ref{a} and~\\ref{b}` is one finding.

Whether the document loaded cleveref is read from the run.  If it did, the
fix writes `\\Cref{sec:x}`, or `\\cref` after a lowercase word, and the rule
speaks only where cleveref would set the very word written (`Sec.` stays
where it sets `Section`).  If it did
not, the finding asks to load it after hyperref and has no fix.

Fix by writing `\\Cref{sec:x}` and loading cleveref.",
        references: &[Ref { source: Source::Cleveref, at: "2" }],
        fixes: &[Applicability::Unsafe],
        run: prefer_cleveref,
    },
    Rule {
        code: "unreferenced-numbered-display",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a display that stepped a counter is never referenced, so its number is spent for nothing",
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
        run: unreferenced_numbered_display,
    },
];

/// amsmath § 3.2's own switches for a display's number: a display that
/// touches one already decides for itself and is left alone.
const TAG_COMMANDS: &[&str] = &["notag", "nonumber", "tag"];

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
        let Some(body) = read_body(&text, from, Pos::new(end.line, end.col)) else { continue };
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
        if let Some(note) = report.unread("referenced") {
            report.add(span, &name, format!("{message}{note}"), None);
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
fn read_body(text: &Rc<Text>, from: Pos, to: Pos) -> Option<Body> {
    let mut scan = Scan::at(text, from, CatcodeTable::latex())?;
    let mut body = Body { labels: Vec::new(), tagged: false };
    while let Some(lexeme) = scan.advance() {
        if lexeme.start >= to {
            break;
        }
        let (label, tag) = match scan.name(&lexeme) {
            Some(cs) => (cs == "label", TAG_COMMANDS.contains(&cs)),
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
    file: crate::tex::FileId,
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
fn label_counters(analysis: &Analysis) -> HashMap<&str, String> {
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
fn names_counter(analysis: &Analysis, cleveref: bool, counter: &str, word: &str) -> bool {
    use crate::lint::shared::macro_text;
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
    let view = report.text();
    let edits = report.edits();
    let cleveref = crate::lint::shared::loaded(analysis, "cleveref");
    let counters = label_counters(analysis);
    let all_counters: std::collections::BTreeSet<&str> =
        analysis.facts.defs.iter().filter_map(|d| analysis.interner.name(d.name).strip_prefix("c@")).collect();
    let mut sites: Vec<Span> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Ref && view.document(o.span.file))
        .map(|o| o.span)
        .collect();
    sites.sort_by_key(|s| (s.file, s.line, s.col));
    sites.dedup();
    let mut found: Vec<Named> = Vec::new();
    for span in sites {
        let Some((path, text)) = edits.text(span.file) else { continue };
        let line: Vec<char> = text.line(span.line).chars().collect();
        let at = span.col as usize - 1;
        let Some((label, end)) = ref_at(&line, at) else { continue };
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
            crate::lint::shared::macro_text(analysis, &format!("{form}@{c}@name{tail}")).is_some_and(|t| t == n.word)
        });
        if cleveref && !same {
            continue;
        }
        if cleveref {
            let edit = Edit {
                path: n.path,
                start: Pos::new(n.line, n.start as u32 + 1),
                end: Pos::new(n.line, n.end as u32 + 1),
                replacement: format!("{command}{{{labels}}}"),
            };
            let message = format!(
                "`{}` names the reference by hand, where `{command}{{{labels}}}` reads the name from the label",
                n.source
            );
            let fix = Fix::new(format!("write `{command}{{{labels}}}`"), Applicability::Unsafe, Some(vec![edit]));
            report.add_fix(at, "cleveref", message, fix);
        } else {
            let message = format!(
                "`{}` names the reference by hand: load cleveref (after hyperref) and write `{command}{{{labels}}}`",
                n.source
            );
            let fix = Fix::new("load cleveref after hyperref", Applicability::Unsafe, None);
            report.add_fix(at, "load", message, fix);
        }
    }
}
