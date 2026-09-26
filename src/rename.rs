//! Renaming a name the document defines.  One declaration defines a
//! family of names from one key (`\newcounter{step}`: `\c@step`,
//! `\thestep`, `\p@step`, `\cl@step`, `\theHstep`), so they move
//! together with every site that spells the key.  The family is the
//! definitions recorded at one declaration span, the key their subject.
//! A site the source does not spell stops the rename.

use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use crate::builtins::{LoadKind, OccKind, Primitive};
use crate::facts::{Definition, MeaningKind};
use crate::lint::fix::{Edit, Lexeme, Pos, Scan, Sources, Text, editable};
use crate::machine::Analysis;
use crate::tex::{Catcode, CatcodeTable, FileId, Span, Sym, Tok};

/// A stretch of source that spells the key.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Site {
    pub path: String,
    pub start: Pos,
    pub end: Pos,
}

/// What a rename would touch.
pub struct Plan {
    pub key: String,
    /// The subject is a key (`{step}`), not a control sequence (`\greet`).
    pub keyed: bool,
    pub sites: Vec<Site>,
    names: Vec<String>,
}

impl Plan {
    /// The site the cursor stands in, if any.
    pub fn at(&self, path: &str, at: Pos) -> Option<&Site> {
        self.sites.iter().find(|s| s.path == path && s.start <= at && at <= s.end)
    }
}

fn cats(analysis: &Analysis, file: FileId) -> CatcodeTable {
    let mut cats = CatcodeTable::latex();
    if matches!(analysis.files.get(file as usize).map(|f| f.kind), Some(LoadKind::Package | LoadKind::Class)) {
        cats.set('@', Catcode::Letter);
    }
    cats
}

/// The path of a span's file, when a rename may write to it.
fn writable(analysis: &Analysis, file: FileId) -> Option<String> {
    let path = analysis.file_name(file).to_string();
    editable(analysis, &path).then_some(path)
}

fn pos(span: Span) -> Pos {
    Pos::new(span.line, span.col)
}

fn scan_at(sources: &Sources, analysis: &Analysis, span: Span) -> Option<(String, Rc<Text>, Scan)> {
    let path = writable(analysis, span.file)?;
    let text = sources.get(&path)?;
    let scan = Scan::at(&text, pos(span), cats(analysis, span.file))?;
    Some((path, text, scan))
}

/// Where `key` stands inside `name`, when it stands there once.
fn offset(name: &str, key: &str) -> Option<u32> {
    let mut found = name.match_indices(key);
    let (at, _) = found.next()?;
    found.next().is_none().then(|| name[..at].chars().count() as u32)
}

fn site(path: &str, at: Pos, key: &str) -> Site {
    Site { path: path.to_string(), start: at, end: Pos::new(at.line, at.col + key.chars().count() as u32) }
}

/// The definitions one declaration made, and the key it declared.
fn family<'a>(
    analysis: &'a Analysis,
    path: &str,
    at: Pos,
    name: &str,
    is_command: bool,
) -> Result<(Vec<&'a Definition>, String, bool), String> {
    let defs: Vec<&Definition> =
        analysis.facts.defs.iter().filter(|d| writable(analysis, d.span.file).is_some()).collect();
    let sym = analysis.interner.lookup(name);
    let spans: HashSet<Span> = if is_command {
        let named: Vec<&&Definition> = defs.iter().filter(|d| Some(d.name) == sym).collect();
        if named.is_empty() {
            return Err(format!("`\\{name}` is not defined in this project"));
        }
        // The binding under the cursor, not every macro of the name.
        let here = |span: Span| analysis.file_name(span.file) == path && pos(span) == at;
        let bound = named.iter().find(|d| here(d.span)).map(|d| d.span).or_else(|| {
            let call = analysis.facts.expansions.iter().find(|e| Some(e.name) == sym && here(e.span))?;
            let MeaningKind::Macro(node) = call.meaning else { return None };
            named.iter().find(|d| d.node == node).map(|d| d.span)
        });
        match bound {
            Some(span) => [span].into_iter().collect(),
            None => named.iter().map(|d| d.span).collect(),
        }
    } else {
        defs.iter().filter(|d| sym.is_some() && d.subject == sym).map(|d| d.span).collect()
    };
    let members: Vec<&Definition> = defs.into_iter().filter(|d| spans.contains(&d.span)).collect();
    // A key no declaration turned into a name: a label, a citation.
    if members.is_empty() {
        return Ok((members, name.to_string(), true));
    }
    let subject = members.iter().find_map(|d| d.subject).map(|s| analysis.interner.name(s).to_string());
    let (key, keyed) = match subject {
        Some(key) => (key, true),
        None if is_command => (name.to_string(), false),
        None => return Err(format!("`{name}` is not a name this project declares")),
    };
    for member in &members {
        let spelled = analysis.interner.name(member.name);
        if offset(spelled, &key).is_none() {
            return Err(format!("`\\{spelled}` is declared with `{key}` but does not spell it once"));
        }
    }
    Ok((members, key, keyed))
}

/// Where a declaration spells its key.  The span is the name when a call
/// read it (`\newcommand{\greet}`), the defining command otherwise.
fn declaration(
    sources: &Sources,
    analysis: &Analysis,
    span: Span,
    name: &str,
    key: &str,
    keyed: bool,
) -> Result<Site, String> {
    let unwritten = || format!("`{key}` is built while the document runs, not spelled at {}:{}", span.line, span.col);
    let (path, text, mut scan) = scan_at(sources, analysis, span)
        .ok_or_else(|| format!("`{key}` is declared in a file this project may not edit"))?;
    let inside = offset(name, key).unwrap_or(0);
    let spelled = |at: Pos, text: &Text| {
        let end = Pos::new(at.line, at.col + key.chars().count() as u32);
        (text.slice(at, end) == Some(key)).then(|| site(&path, at, key))
    };
    let head = scan.peek().cloned();
    if head.as_ref().is_some_and(|l| scan.name(l) == Some(name)) {
        let at = head.map(|l| l.start).unwrap_or(pos(span));
        scan.advance();
        return spelled(Pos::new(at.line, at.col + 1 + inside), &text).ok_or_else(unwritten);
    }
    if let Some(site) = keyed.then(|| spelled(pos(span), &text)).flatten() {
        return Ok(site);
    }
    // The span is the defining call: the name is one of its arguments.
    scan.advance();
    for _ in 0..4 {
        scan.star();
        scan.optional();
        let Some(group) = scan.argument() else { break };
        if let [only] = &group.inner[..]
            && scan.name(only) == Some(name)
        {
            return spelled(Pos::new(only.start.line, only.start.col + 1 + inside), &text).ok_or_else(unwritten);
        }
        let written = text.slice(group.inner_start, group.inner_end).unwrap_or_default();
        if keyed && written.trim() == key {
            let skip = (written.len() - written.trim_start().len()) as u32;
            return Ok(site(&path, Pos::new(group.inner_start.line, group.inner_start.col + skip), key));
        }
    }
    Err(unwritten())
}

/// Every use of a family member the source spells out.
fn uses(
    analysis: &Analysis,
    sources: &Sources,
    members: &[&Definition],
    key: &str,
    keyed: bool,
) -> Result<Vec<Site>, String> {
    let names: BTreeSet<Sym> = members.iter().map(|d| d.name).collect();
    let mut out = Vec::new();
    for use_ in analysis.facts.expansions.iter().filter(|e| names.contains(&e.name)) {
        let spelled = analysis.interner.name(use_.name);
        let Some((path, text, mut scan)) = scan_at(sources, analysis, use_.span) else { continue };
        let inside = offset(spelled, key).unwrap_or(0);
        // A `\csname` site is recorded at the name's first character.
        let at = pos(use_.span);
        let end = Pos::new(at.line, at.col + spelled.chars().count() as u32);
        if text.slice(at, end) == Some(spelled) {
            out.push(site(&path, Pos::new(at.line, at.col + inside), key));
            continue;
        }
        let Some(lexeme) = scan.advance() else { continue };
        match (scan.name(&lexeme), lexeme.tok) {
            (Some(found), _) if found == spelled => {
                out.push(site(&path, Pos::new(lexeme.start.line, lexeme.start.col + 1 + inside), key));
            }
            // Out of an expansion: nothing of the name is written here.
            (Some(_), _) => {}
            // A keyed family's names come out of the key, edited above.
            (None, Tok::Chr(_, Catcode::Letter | Catcode::Other)) if !keyed => {
                return Err(format!(
                    "`\\{spelled}` is built while the document runs at {}:{}, where a rename cannot reach it",
                    use_.span.line, use_.span.col
                ));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The kinds of occurrence that carry a name a document can rename.
fn renamable(kind: OccKind) -> bool {
    matches!(
        kind,
        OccKind::Label
            | OccKind::Ref
            | OccKind::Cite
            | OccKind::BibItem
            | OccKind::BeginEnvironment
            | OccKind::EndEnvironment
            | OccKind::Key
            | OccKind::KeyUse
    )
}

/// Every call that reads the key, and the commands that read it.
fn keys(analysis: &Analysis, sources: &Sources, key: &str) -> (Vec<Site>, BTreeSet<String>) {
    let mut out = Vec::new();
    let mut readers = BTreeSet::new();
    for occ in analysis.facts.occurrences.iter().filter(|o| o.key == key && renamable(o.kind)) {
        let Some((path, text, mut scan)) = scan_at(sources, analysis, occ.span) else { continue };
        let Some(call) = scan.advance() else { continue };
        let Some(reader) = scan.name(&call).map(str::to_string) else { continue };
        // `\newtheorem{thm}[x]{T}`: optionals around the key.
        for _ in 0..3 {
            scan.star();
            scan.optional();
            let Some(group) = scan.argument() else { break };
            let Some(written) = text.slice(group.inner_start, group.inner_end) else { break };
            if written.trim() == key {
                let skip = (written.len() - written.trim_start().len()) as u32;
                out.push(site(&path, Pos::new(group.inner_start.line, group.inner_start.col + skip), key));
                readers.insert(reader);
                break;
            }
        }
    }
    (out, readers)
}

/// Leaves the scan right after the name a declaration declares.
fn skip_head(scan: &mut Scan, text: &Text, span: Span, name: &str, subject: Option<&str>) {
    let head = scan.peek().cloned();
    if head.as_ref().is_some_and(|l| scan.name(l) == Some(name)) {
        scan.advance();
        return;
    }
    if let Some(key) = subject {
        let at = pos(span);
        let width = key.chars().count() as u32;
        if text.slice(at, Pos::new(at.line, at.col + width)) == Some(key) {
            while scan.peek().is_some_and(|l| l.start.line == at.line && l.start.col < at.col + width) {
                scan.advance();
            }
            return;
        }
    }
    scan.advance();
    for _ in 0..4 {
        scan.star();
        scan.optional();
        let Some(group) = scan.argument() else { break };
        let single = matches!(&group.inner[..], [only] if scan.name(only) == Some(name));
        let written = text.slice(group.inner_start, group.inner_end).unwrap_or_default();
        if single || subject.is_some_and(|key| written.trim() == key) {
            break;
        }
    }
}

/// What a declaration defines, as the file writes it: the token `\let`
/// copies, `\def`'s replacement text, or the groups after the name.
fn body_tokens(analysis: &Analysis, scan: &mut Scan, by: Sym) -> Vec<Lexeme> {
    match analysis.env.meaning(by).prim() {
        Some(Primitive::Let { .. }) => {
            scan.skip_spaces();
            scan.advance().into_iter().collect()
        }
        Some(Primitive::Def { .. }) => {
            while scan.peek().is_some_and(|l| !matches!(l.tok, Tok::Chr(_, Catcode::Begin))) {
                scan.advance();
            }
            scan.group().map(|g| g.inner).unwrap_or_default()
        }
        _ => {
            let mut out = Vec::new();
            loop {
                scan.star();
                if let Some(group) = scan.optional() {
                    out.extend(group.inner);
                    continue;
                }
                scan.skip_spaces();
                if !scan.peek().is_some_and(|l| matches!(l.tok, Tok::Chr(_, Catcode::Begin))) {
                    break;
                }
                match scan.group() {
                    Some(group) => out.extend(group.inner),
                    None => break,
                }
            }
            out
        }
    }
}

/// Family names inside a replacement text: a body that never expanded
/// records no site of its own.
fn bodies(
    analysis: &Analysis,
    sources: &Sources,
    members: &[&Definition],
    key: &str,
    keyed: bool,
    readers: &BTreeSet<String>,
) -> Vec<Site> {
    let spelled: BTreeSet<&str> = members.iter().map(|d| analysis.interner.name(d.name)).collect();
    let mut seen: HashSet<Span> = HashSet::new();
    let mut out = Vec::new();
    for def in analysis.facts.defs.iter().filter(|d| d.mac.is_some()) {
        if !seen.insert(def.span) {
            continue;
        }
        // Whichever name carries the subject says how the head is written.
        let at = analysis.facts.defs.iter().filter(|d| d.span == def.span);
        let head = at.clone().find(|d| d.subject.is_some()).unwrap_or(def);
        let name = analysis.interner.name(head.name).to_string();
        let subject = head.subject.map(|s| analysis.interner.name(s).to_string());
        let Some((path, text, mut scan)) = scan_at(sources, analysis, def.span) else { continue };
        skip_head(&mut scan, &text, def.span, &name, subject.as_deref());
        let tokens = body_tokens(analysis, &mut scan, def.by);
        let mut reader: Option<String> = None;
        for lexeme in &tokens {
            match lexeme.tok {
                Tok::Cs(_) => {
                    let Some(name) = scan.name(lexeme).map(str::to_string) else { continue };
                    if spelled.contains(name.as_str()) {
                        let inside = offset(&name, key).unwrap_or(0);
                        out.push(site(&path, Pos::new(lexeme.start.line, lexeme.start.col + 1 + inside), key));
                    }
                    reader = Some(name);
                }
                // `\stepcounter{step}` in a body that never ran.
                Tok::Chr(_, Catcode::Begin) => {
                    if keyed && reader.as_deref().is_some_and(|r| readers.contains(r)) {
                        let at = lexeme.end;
                        let end = Pos::new(at.line, at.col + key.chars().count() as u32);
                        if text.slice(at, end) == Some(key) {
                            out.push(site(&path, at, key));
                        }
                    }
                    reader = None;
                }
                Tok::Chr(_, Catcode::Space) => {}
                _ => reader = None,
            }
        }
    }
    out
}

/// What a rename of the name at `at` would touch.
pub fn plan(
    analysis: &Analysis,
    sources: &Sources,
    path: &str,
    at: Pos,
    name: &str,
    is_command: bool,
) -> Result<Plan, String> {
    let (members, key, keyed) = family(analysis, path, at, name, is_command)?;
    let mut sites = Vec::new();
    let spans: HashSet<Span> = members.iter().map(|d| d.span).collect();
    for span in &spans {
        let named = members.iter().find(|d| d.span == *span).map(|d| analysis.interner.name(d.name)).unwrap_or(name);
        sites.push(declaration(sources, analysis, *span, named, &key, keyed)?);
    }
    sites.extend(uses(analysis, sources, &members, &key, keyed)?);
    let (key_sites, readers) = if keyed { keys(analysis, sources, &key) } else { (Vec::new(), BTreeSet::new()) };
    sites.extend(key_sites);
    sites.extend(bodies(analysis, sources, &members, &key, keyed, &readers));
    sites.sort();
    sites.dedup();
    if sites.is_empty() {
        return Err(format!("nothing in this project spells `{key}`"));
    }
    if sites.windows(2).any(|pair| pair[0].path == pair[1].path && pair[1].start < pair[0].end) {
        return Err(format!("the sites of `{key}` overlap, so renaming it is not safe"));
    }
    let names = members.iter().map(|d| analysis.interner.name(d.name).to_string()).collect();
    Ok(Plan { key, keyed, sites, names })
}

/// The edits the plan carries out, once `new_name` is known to be free.
pub fn edits(plan: &Plan, analysis: &Analysis, new_name: &str) -> Result<Vec<Edit>, String> {
    let new_key = new_name.trim_start_matches('\\');
    let valid = if plan.keyed {
        !new_key.is_empty()
            && !new_key.chars().any(|c| c.is_whitespace() || matches!(c, '{' | '}' | ',' | '%' | '\\' | '#'))
    } else {
        !new_key.is_empty() && new_key.chars().all(|c| c.is_ascii_alphabetic() || c == '@')
    };
    if !valid {
        return Err(format!("`{new_key}` is not a valid name here"));
    }
    if new_key == plan.key {
        return Ok(Vec::new());
    }
    // A name the rename builds has to be free.
    let held: BTreeSet<&str> = plan.names.iter().map(String::as_str).collect();
    for name in &plan.names {
        let renamed = name.replacen(&plan.key, new_key, 1);
        if held.contains(renamed.as_str()) {
            continue;
        }
        if analysis.interner.lookup(&renamed).is_some_and(|sym| analysis.env.is_defined(sym)) {
            return Err(format!("`\\{renamed}` is already defined"));
        }
    }
    if plan.names.is_empty()
        && analysis
            .facts
            .occurrences
            .iter()
            .any(|o| o.key == new_key && matches!(o.kind, OccKind::Label | OccKind::BibItem))
    {
        return Err(format!("`{new_key}` is already used as a key"));
    }
    Ok(plan
        .sites
        .iter()
        .map(|s| Edit { path: s.path.clone(), start: s.start, end: s.end, replacement: new_key.to_string() })
        .collect())
}
