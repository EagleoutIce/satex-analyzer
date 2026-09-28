//! A numbered display nothing refers to: the star form sets the same
//! mathematics without spending a number.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::builtins::OccKind;
use crate::lint::fix::{Applicability, Edit, Fix, Pos, Scan, Text};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::tex::{CatcodeTable, Span};

pub static RULES: &[Rule] = &[Rule {
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
    run: unreferenced_numbered_display,
}];

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
    let mut numbered: HashSet<&str> = HashSet::new();
    let mut inner: HashSet<&str> = HashSet::new();
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
    let mut seen: HashSet<Span> = HashSet::new();
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
    let mut open: HashMap<&str, Vec<Span>> = HashMap::new();
    let mut out = HashMap::new();
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
