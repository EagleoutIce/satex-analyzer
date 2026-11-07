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

/// The fields `e` lacks that its type requires (an alternative given counts), none for an
/// entry that inherits through `crossref` or `xref`. A field counts under its alias too.
pub fn missing(e: &Entry, model: &Model) -> Vec<String> {
    if e.fields.iter().any(|f| matches!(f.0.as_str(), "crossref" | "xref")) {
        return Vec::new();
    }
    let has = |n: &str| e.fields.iter().any(|f| (f.0 == n || model.alias(&f.0) == Some(n)) && !f.1.trim().is_empty());
    let kind = model
        .type_aliases
        .iter()
        .find(|t| t.0 == e.kind && model.required_of(&e.kind).is_empty())
        .map_or(e.kind.as_str(), |t| t.1.as_str());
    model.required_of(kind).iter().filter(|alt| !alt.iter().any(|f| has(f))).map(|alt| alt.join(" or ")).collect()
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
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a database is loaded twice",
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
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "two bibliography entries name one work under different keys",
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
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a cited entry lacks a field its type requires",
        explanation: "\
The style's data model requires fields per entry type: the `.bst`'s
`output.check`s (BibTeX warns `empty journal in knuth84`), or biblatex's mandatory
constraints (biber warns under `--validate-datamodel`). An entry with `crossref`
or `xref` inherits fields and is not reported. Only cited entries are checked.

Fix by adding the field.",
        references: &[Ref { source: Source::Bibtex, at: "Using BibTeX" }, Ref { source: Source::Biblatex, at: "§2.1" }],
        fixes: &[],
        engines: &[],
        run: missing_field,
    },
    Rule {
        code: "undefined-bibliography-string",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a `.bib` value uses a constant no `@string` defines",
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
        category: Category::Style,
        severity: Severity::Info,
        summary: "a database entry is never cited",
        explanation: "\
An entry no `\\cite` names is left out of the bibliography. That is what a shared
database is for, so this is only a note. `\\nocite{*}` cites everything.

Fix by citing the entry, or by deleting it.",
        references: &[Ref { source: Source::Bibtex, at: "Using BibTeX" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: uncited_entry,
    },
];

/// The keys cited, and whether `\nocite{*}` cites all.
fn cited(analysis: &Analysis) -> (BTreeSet<&str>, bool) {
    let keys: BTreeSet<&str> =
        analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Cite).map(|o| o.key.as_str()).collect();
    let all = keys.contains("*");
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
    let analysis = report.analysis;
    let sources = report.edits().sources;
    let (loads, _) = databases(analysis, sources);
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
    let (loads, dbs) = databases(analysis, sources);
    for (keep, drop, why) in twins(&dbs) {
        let Some(load) = at_load(&loads, &drop.file).or_else(|| loads.first()) else { continue };
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
            merge(analysis, sources, &dbs, keep, drop),
        );
        report.add_fix(load.span, &format!("{},{}", keep.key, drop.key), message, fix);
    }
}

fn missing_field(report: &mut Report) {
    let analysis = report.analysis;
    let (loads, dbs) = databases(analysis, report.edits().sources);
    let (keys, all) = cited(analysis);
    if dbs.is_empty() {
        return;
    }
    let main = analysis.file_name(analysis.main_file).to_string();
    let model = crate::plugins::bib::model::of(analysis, &main);
    for e in dbs.iter().flat_map(|d| &d.entries).filter(|e| all || keys.contains(e.key.as_str())) {
        let lacks = missing(e, &model);
        if lacks.is_empty() {
            continue;
        }
        let site = analysis
            .facts
            .occurrences
            .iter()
            .find(|o| o.kind == OccKind::Cite && o.key == e.key)
            .map(|o| report.site(o).0);
        let Some(span) = site.or_else(|| at_load(&loads, &e.file).map(|l| l.span)) else { continue };
        let named = lacks.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ");
        let message =
            format!("`{}` ({}:{}) lacks {named}, required for `@{}`", e.key, file_name(&e.file), e.line, e.kind);
        report.add(span, &e.key, message, Some(format!("add {named} to `{}`", e.key)));
    }
}

fn undefined_string(report: &mut Report) {
    let analysis = report.analysis;
    let (loads, dbs) = databases(analysis, report.edits().sources);
    if dbs.is_empty() {
        return;
    }
    let main = analysis.file_name(analysis.main_file).to_string();
    let model = crate::plugins::bib::model::of(analysis, &main);
    for e in dbs.iter().flat_map(|d| &d.entries) {
        let names = undefined(e, &dbs, &model);
        let Some(load) = at_load(&loads, &e.file) else { continue };
        if names.is_empty() {
            continue;
        }
        let named = names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ");
        let message =
            format!("`{}` ({}:{}) uses {named}, which no `@string` defines", e.key, file_name(&e.file), e.line);
        report.add(load.span, &e.key, message, Some("define it with `@string`, or write the value in braces".into()));
    }
}

fn uncited_entry(report: &mut Report) {
    let analysis = report.analysis;
    let sources = report.edits().sources;
    let (loads, dbs) = databases(analysis, sources);
    let (keys, all) = cited(analysis);
    if all || dbs.is_empty() {
        return;
    }
    for db in &dbs {
        let Some(load) = at_load(&loads, &db.path) else { continue };
        for e in db.entries.iter().filter(|e| !keys.contains(e.key.as_str())) {
            let edits = entry_lines(&db.text, e).filter(|_| sources.editable(analysis, &db.path)).map(|(from, to)| {
                vec![Edit { path: db.path.clone(), start: from, end: to, replacement: String::new() }]
            });
            let message = format!("`{}` ({}:{}) is never cited", e.key, file_name(&db.path), e.line);
            report.add_fix_as(
                Severity::Info,
                load.span,
                &e.key,
                message,
                Fix::new(format!("delete `{}`", e.key), Applicability::Unsafe, edits),
            );
        }
    }
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
}
