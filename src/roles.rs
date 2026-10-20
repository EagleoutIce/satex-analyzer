//! What a command's arguments are: a label it defines, a key it references,
//! a file it reads. Completion and signatures ask this.
//!
//! Three sources, in this order:
//! - the run's evidence: the calls that made an occurrence or a load,
//!   matched to the braced argument that held its key or name;
//! - the flow through the project's macro bodies: an argument handed on,
//!   behind a literal prefix or not, to an argument with a role has it too;
//! - a seed of the kernel's and common packages' conventions, for commands
//!   the project does not redefine.

use crate::builtins::{LoadKind, OccKind};
use crate::machine::Analysis;
use crate::tex::{Catcode, Tok, Token};

/// What one argument is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleKind {
    Occ(OccKind),
    Load(LoadKind),
}

impl RoleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RoleKind::Occ(k) => k.as_str(),
            RoleKind::Load(k) => k.as_str(),
        }
    }
}

impl std::fmt::Display for Role {
    /// `#1 ref`, or with the text a wrapper puts before it, `#1 ref after sec:`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{} {}", self.arg + 1, self.kind.as_str())?;
        if !self.prefix.is_empty() {
            write!(f, " after {}", self.prefix)?;
        }
        Ok(())
    }
}

/// Argument `arg` (counting braced arguments from 0) of a command is a
/// `kind`; its key is `prefix` followed by what the argument holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Role {
    pub arg: usize,
    pub kind: RoleKind,
    pub prefix: String,
}

/// How deep the flow follows macros calling macros.
const DEPTH: usize = 6;

/// The roles of `cmd`'s arguments.
pub fn roles(analysis: &Analysis, cmd: &str) -> Vec<Role> {
    let mut out = roles_at(analysis, cmd, 0);
    // An argument that defines a key is not also a use of it: the run
    // reading its own label back is no reference.
    let defines = |arg: usize, out: &[Role]| {
        out.iter()
            .any(|r| r.arg == arg && matches!(r.kind, RoleKind::Occ(OccKind::Label | OccKind::BibItem | OccKind::Key)))
    };
    let all = out.clone();
    out.retain(|r| !(matches!(r.kind, RoleKind::Occ(OccKind::Ref | OccKind::KeyUse)) && defines(r.arg, &all)));
    out.sort_by_key(|r| (r.arg, r.prefix.clone(), r.kind.as_str()));
    out.dedup();
    out
}

fn roles_at(analysis: &Analysis, cmd: &str, depth: usize) -> Vec<Role> {
    let mut out = evidence(analysis, cmd);
    let body = project_body(analysis, cmd);
    if let Some(body) = &body
        && depth < DEPTH
    {
        out.extend(flow(analysis, body, depth));
    }
    // A command the project defines anew is what its body says.
    if body.is_none() {
        out.extend(seed(cmd));
    }
    out
}

/// The roles the run saw `cmd`'s calls take: each occurrence or load made
/// by a call of `cmd`, at the braced argument that held its key.
fn evidence(analysis: &Analysis, cmd: &str) -> Vec<Role> {
    let occurrences = analysis
        .facts
        .occurrences
        .iter()
        .map(|o| (o.call.map_or(o.span, |c| c.1), o.key.as_str(), RoleKind::Occ(o.kind)));
    let loads = analysis.facts.loads.iter().map(|l| (l.span, l.name.as_str(), RoleKind::Load(l.kind)));
    let mut texts: crate::fixed::HashMap<crate::tex::FileId, Option<String>> = Default::default();
    occurrences
        .chain(loads)
        .filter_map(|(at, key, kind)| {
            let text = texts.entry(at.file).or_insert_with(|| {
                let path = &analysis.files.get(usize::from(at.file))?.path;
                crate::overlay::read_to_string(std::path::Path::new(path)).ok()
            });
            let args = call_arguments(text.as_deref()?, at, cmd)?;
            let (arg, prefix) = args.iter().enumerate().find_map(|(i, held)| {
                let prefix = key.strip_suffix(held.as_str()).filter(|_| !held.is_empty())?;
                Some((i, prefix.to_string()))
            })?;
            Some(Role { arg, kind, prefix })
        })
        .collect()
}

/// The braced arguments of the call of `cmd` that stands at `at` in
/// `text`; `None` where no call of `cmd` stands there.
fn call_arguments(text: &str, at: crate::tex::Span, cmd: &str) -> Option<Vec<String>> {
    let line = text.lines().nth(usize::try_from(at.line).ok()?.checked_sub(1)?)?;
    let rest = line.get(crate::source::byte_of_col(line, at.col)..)?.strip_prefix('\\')?.strip_prefix(cmd)?;
    if rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '@') {
        return None;
    }
    Some(braced_groups(rest.trim_start_matches('*')))
}

/// The `{…}` groups at the start of `text`, `[…]` ones skipped.
pub fn braced_groups(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while let Some(open) = rest.chars().next().filter(|c| matches!(c, '{' | '[')) {
        let close = if open == '{' { '}' } else { ']' };
        let mut depth = 0;
        let end = rest.char_indices().find_map(|(i, c)| {
            depth += i32::from(c == open) - i32::from(c == close);
            (c == close && depth == 0).then_some(i)
        });
        let Some(end) = end else { break };
        if open == '{' {
            out.push(rest[1..end].trim().to_string());
        }
        rest = rest[end + 1..].trim_start();
    }
    out
}

/// The body of the project's own definition of `cmd`, if it has one.
fn project_body(analysis: &Analysis, cmd: &str) -> Option<std::rc::Rc<[Token]>> {
    analysis
        .facts
        .defs
        .iter()
        .rev()
        .filter(|d| analysis.interner.name(d.name) == cmd && analysis.project_file(d.span.file))
        .find_map(|d| d.mac.as_ref().map(|m| m.replacement_text.clone()))
}

/// The roles a body gives its parameters: `#i` in a braced argument of a
/// command with a role there, after literal characters or not.
fn flow(analysis: &Analysis, body: &[Token], depth: usize) -> Vec<Role> {
    let mut out = Vec::new();
    for (at, t) in body.iter().enumerate() {
        let Tok::Cs(sym) = t.tok else { continue };
        let callee = analysis.interner.name(sym);
        let groups = body_groups(&body[at + 1..]);
        if groups.iter().all(|g| param_after(analysis, g).is_none()) {
            continue;
        }
        let known = roles_at(analysis, callee, depth + 1);
        for (j, group) in groups.iter().enumerate() {
            let Some((i, prefix)) = param_after(analysis, group) else { continue };
            for r in known.iter().filter(|r| r.arg == j) {
                out.push(Role { arg: i, kind: r.kind, prefix: format!("{}{prefix}", r.prefix) });
            }
        }
    }
    out
}

/// The braced groups that follow a command in a body, `[…]` ones skipped.
fn body_groups(tokens: &[Token]) -> Vec<&[Token]> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(t) = tokens.get(i) {
        match t.tok {
            Tok::Chr(_, Catcode::Space) => i += 1,
            Tok::Chr('[', Catcode::Other) => {
                let Some(end) = tokens[i..].iter().position(|t| t.tok == Tok::Chr(']', Catcode::Other)) else { break };
                i += end + 1;
            }
            Tok::Chr(_, Catcode::Begin) => {
                let mut depth = 0;
                let Some(end) = tokens[i..].iter().position(|t| {
                    match t.tok {
                        Tok::Chr(_, Catcode::Begin) => depth += 1,
                        Tok::Chr(_, Catcode::End) => depth -= 1,
                        _ => {}
                    }
                    depth == 0
                }) else {
                    break;
                };
                out.push(&tokens[i + 1..i + end]);
                i += end + 1;
            }
            _ => break,
        }
    }
    out
}

/// A group that is literal characters and then one parameter `#i`: the
/// parameter (from 0) and the characters before it.
fn param_after(analysis: &Analysis, group: &[Token]) -> Option<(usize, String)> {
    let (last, before) = group.split_last()?;
    let Tok::Param(n) = last.tok else { return None };
    let prefix = crate::tex::text_of(before, &analysis.interner);
    before.iter().all(|t| matches!(t.tok, Tok::Chr(..))).then(|| (usize::from(n).saturating_sub(1), prefix))
}

/// The conventions of the kernel and common packages, for a command whose
/// meaning is theirs: where its first braced argument is a key or a name.
fn seed(cmd: &str) -> Vec<Role> {
    use LoadKind as L;
    use OccKind as O;
    let first = |kind| vec![Role { arg: 0, kind, prefix: String::new() }];
    match cmd {
        "documentclass" | "LoadClass" => first(RoleKind::Load(L::Class)),
        "usepackage" | "RequirePackage" => first(RoleKind::Load(L::Package)),
        "input" | "subfile" => first(RoleKind::Load(L::Input)),
        "include" | "includeonly" => first(RoleKind::Load(L::Include)),
        "begin" => first(RoleKind::Occ(O::BeginEnvironment)),
        "end" => first(RoleKind::Occ(O::EndEnvironment)),
        "includegraphics" => first(RoleKind::Occ(O::Graphics)),
        "label" => first(RoleKind::Occ(O::Label)),
        "crefrange" | "Crefrange" => {
            (0..2).map(|arg| Role { arg, kind: RoleKind::Occ(O::Ref), prefix: String::new() }).collect()
        }
        "ref" | "eqref" | "pageref" | "autoref" | "nameref" | "cref" | "Cref" | "cpageref" | "Cpageref" | "vref"
        | "Vref" | "labelcref" | "subref" => first(RoleKind::Occ(O::Ref)),
        c if c.contains("cite") => first(RoleKind::Occ(O::Cite)),
        _ => Vec::new(),
    }
}
