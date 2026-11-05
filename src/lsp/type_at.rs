//! The type of the text at a position ([`type_at`]), from the run's facts:
//! what TeX read the text as (`Facts::types`), the kind of name a
//! `\csname` formed from it (`Facts::slots`), else the role of the
//! argument it stands in ([`roles::roles_probed`]) for text the run has
//! not seen. Completion and hover ask it, then offer what fits the type.

use super::handlers::{Arg, argument_at, uncommented};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::{self, KeyKind, RoleKind, Ty};
use crate::engine::builtins::{LoadKind, OccKind};
use crate::engine::tex::Span;
use crate::linter::fix::{Pos, Text};

/// What ends a name typed in an argument: a group, a list's comma, a key's `=`, a space.
pub(super) const ENDS: &str = "{}[](),=;\\ \t%";

/// How many lines before the cursor an argument may have opened.
pub(super) const LINES: u32 = 12;

/// The argument the cursor is in, from the `\` of its command: owned, as it may span lines.
pub(super) struct Place {
    pub cmd: String,
    pub open: char,
    pub groups: String,
    pub inside: String,
    /// Where the command's `\` stands.
    pub at: Pos,
}

impl Place {
    pub fn arg(&self) -> Arg<'_> {
        Arg { cmd: &self.cmd, open: self.open, groups: &self.groups, col: self.at.col, inside: &self.inside }
    }

    /// Which braced argument it is, from 0.
    pub fn nth(&self) -> usize {
        roles::braced_groups(&self.groups).len()
    }
}

/// One type the text at the cursor has, and the name it is part of.
pub(super) struct Hit {
    pub ty: Ty,
    /// What the code puts before and after the text to form the name it reads:
    /// a `\csname`'s `r@`, a library's `tikzlibrary` and `.code.tex`.
    pub prefix: String,
    pub suffix: String,
    /// Whether the code reads a file so named.
    pub file: bool,
    /// Whether the text defines the name rather than refers to one (`\label{…}`).
    pub defines: bool,
    /// The part of the item the name is, where only its end is (`b` of `a.b`, a node's anchor).
    pub typed: Option<String>,
}

impl Hit {
    pub fn of(ty: Ty) -> Hit {
        Hit { ty, prefix: String::new(), suffix: String::new(), file: false, defines: false, typed: None }
    }
}

/// The name at the cursor: its start and what is typed of it.
pub(super) struct Item {
    pub typed: String,
    /// The column of its first character.
    pub col: u32,
}

/// Everything [`type_at`] found: the types, the argument, the item.
pub(super) struct At {
    pub hits: Vec<Hit>,
    pub place: Option<Place>,
    pub item: Item,
}

/// The types of the text at `pos` of `path`: what the argument's command
/// takes, then what the run read there. Nothing in a
/// comment, in a control sequence's name or where the run set the
/// command as text (verbatim).
pub(super) fn type_at(analysis: &Analysis, doc: &Text, path: &str, pos: Pos) -> At {
    let line = doc.line(pos.line);
    let cut = crate::engine::source::byte_of_col(line, pos.col).min(line.len());
    let (before, after) = line.split_at(cut);
    let typed: String =
        before.chars().rev().take_while(|c| !ENDS.contains(*c)).collect::<Vec<_>>().into_iter().rev().collect();
    let item = Item { col: pos.col - typed.chars().count() as u32, typed };
    let mut at = At { hits: Vec::new(), place: None, item };
    let code = uncommented(before);
    let lead = &before[..before.len() - at.item.typed.len()];
    if code.len() < before.len() {
        return at;
    }
    if let Some(k) = overlay(analysis, doc, path, pos, &at.item) {
        let typed = at.item.typed[k + 1..].trim_end_matches('>').to_string();
        at.item = Item { col: at.item.col + at.item.typed[..=k].chars().count() as u32, typed };
        at.hits.push(Hit::of(Ty::Overlay));
        return at;
    }
    if lead.ends_with('\\') {
        return at;
    }
    at.place = place(doc, pos);
    let file = file_of(analysis, path);
    if let (Some(file), Some(p)) = (file, &at.place)
        && set_as_text(analysis, Span { file, line: p.at.line, col: p.at.col })
    {
        return at;
    }
    if let Some(p) = &at.place {
        at.hits.extend(argument_types(analysis, p));
    }
    // what the run made of this very text
    if let Some(file) = file {
        let span = Span { file, line: pos.line, col: at.item.col };
        let read = analysis.facts.types.get(&span).copied();
        if let Some(prefix) = analysis.facts.slots.get(&span) {
            let ty = read.filter(|t| matches!(t, Ty::Key(_) | Ty::File(_))).unwrap_or(Ty::Name);
            at.hits.push(Hit { prefix: prefix.clone(), ..Hit::of(ty) });
        } else if let Some(ty) = read.filter(|t| *t != Ty::Bottom) {
            at.hits.push(Hit::of(ty));
        }
    }
    // a color the run looked up or defined by this text, from a call on this line: the type
    // holds where the read went through a cached package's code and left no slot
    if let Some(file) = file {
        let name = at.item.typed.trim();
        let here = |n: &str, s: Span| n == name && s.file == file && s.line == pos.line && s.col <= pos.col;
        let colored = analysis.facts.color_uses.iter().any(|u| here(&u.name, u.span))
            || analysis
                .facts
                .colors
                .iter()
                .any(|c| here(&c.name, c.span) || c.spec_span.is_some_and(|s| here(&c.name, Span { col: 1, ..s })));
        if colored && !name.is_empty() && !at.hits.iter().any(|h| h.ty == Ty::Key(KeyKind::Color)) {
            at.hits.push(Hit::of(Ty::Key(KeyKind::Color)));
        }
    }
    at.hits.extend(super::handlers::node_hits(analysis, doc, pos));
    if dimension(before, after) {
        at.hits.push(Hit::of(Ty::Dimen));
    }
    // a type below another says more of the text: a label's slot is no bare name
    let tys: Vec<Ty> = at.hits.iter().map(|h| h.ty).collect();
    at.hits.retain(|h| !tys.iter().any(|t| *t != h.ty && t.leq(h.ty)));
    at
}

/// The byte of the `<` in `item` when the text after it is an overlay specification: the run read
/// text there as one, or the command or environment before the `<` reads one elsewhere in the file.
fn overlay(analysis: &Analysis, doc: &Text, path: &str, pos: Pos, item: &Item) -> Option<usize> {
    let k = item.typed.rfind('<')?;
    if item.typed[k + 1..].trim_end_matches('>').contains(['>', '<']) {
        return None;
    }
    let file = file_of(analysis, path)?;
    let col = item.col + item.typed[..=k].chars().count() as u32;
    let read = |line: u32, col: u32| analysis.facts.types.get(&Span { file, line, col }) == Some(&Ty::Overlay);
    if read(pos.line, col) {
        return Some(k);
    }
    // what stands before a `<`: a control word, or a `\begin`'s `{name}`
    let head = |line: &str, lt: usize| -> Option<String> {
        let before = &line[..lt];
        let word = before.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '@');
        if word.len() < before.len() && word.ends_with('\\') {
            return Some(before[word.len() - 1..].to_string());
        }
        let open = before.strip_suffix('}')?.rfind("\\begin{")?;
        Some(before[open..].to_string())
    };
    let line = doc.line(pos.line);
    let lt = crate::engine::source::byte_of_col(line, col - 1);
    let mine = head(line, lt)?;
    let seen = analysis.facts.types.iter().filter(|(s, t)| **t == Ty::Overlay && s.file == file).any(|(s, _)| {
        let l = doc.line(s.line);
        let at = crate::engine::source::byte_of_col(l, s.col);
        at > 0 && l.get(at - 1..at) == Some("<") && head(l, at - 1).as_deref() == Some(mine.as_str())
    });
    seen.then_some(k)
}

/// What the argument `p` takes, by its command's roles: per role of a
/// braced one, the key list or the options of a bracketed one.
fn argument_types(analysis: &Analysis, p: &Place) -> Vec<Hit> {
    let mut all = roles::roles_probed(analysis, &p.cmd);
    // a command the run looked colors up or defined them through: which argument names one, from its probe
    let sym = analysis.interner.lookup(&p.cmd);
    let via = |s| {
        analysis.facts.color_uses.iter().any(|u| u.via == Some(s))
            || analysis.facts.colors.iter().any(|c| c.via == Some(s))
    };
    if sym.is_some_and(via) && !all.iter().any(|r| r.kind == RoleKind::Type(Ty::Key(KeyKind::Color))) {
        let probed = sym.map(|s| crate::analysis::probe::argument_roles(analysis, s)).unwrap_or_default();
        all.extend(probed.into_iter().filter(|r| r.kind == RoleKind::Type(Ty::Key(KeyKind::Color))));
    }
    let hit = |ty, prefix: &str, suffix: &str, file| Hit {
        prefix: prefix.into(),
        suffix: suffix.into(),
        file,
        ..Hit::of(ty)
    };
    if p.open == '[' {
        let loads = all.iter().any(|r| matches!(r.kind, RoleKind::Load(LoadKind::Class | LoadKind::Package)));
        return vec![hit(if loads { Ty::Options } else { Ty::Key(KeyKind::Option) }, "", "", false)];
    }
    let nth = p.nth();
    // a role of no kind of name (a message, a write carrying the text) offers nothing
    let mine: Vec<&roles::Role> =
        all.iter().filter(|r| r.arg == nth && !matches!(r.ty(), Ty::Key(KeyKind::Any) | Ty::Bottom)).collect();
    let mut out: Vec<Hit> = mine
        .iter()
        .map(|r| Hit {
            defines: matches!(r.kind, RoleKind::Occ(OccKind::Label | OccKind::BibItem)),
            ..hit(r.ty(), &r.prefix, &r.suffix, matches!(r.kind, RoleKind::Load(_)))
        })
        .collect();
    // a key list: a command the run saw store keys, or a `/path` in the list
    let keyed = mine.iter().all(|r| match r.kind {
        RoleKind::Occ(k) => matches!(k, OccKind::Key | OccKind::KeyUse),
        RoleKind::Load(_) => false,
        RoleKind::Type(t) => !matches!(t, Ty::Key(_) | Ty::File(_)),
    });
    let path = p.inside.split(',').any(|k| k.trim_start().starts_with('/'));
    let declared = mine.iter().any(|r| r.ty() == Ty::Key(KeyKind::Declared));
    if keyed && (declared || path || !crate::queries::query::options(analysis, &p.cmd).is_empty()) {
        out.insert(0, hit(Ty::Key(KeyKind::Option), "", "", false));
    }
    out
}

/// The argument the cursor is in, opened on its line or one of the
/// [`LINES`] before; their comments left out.
fn place(doc: &Text, pos: Pos) -> Option<Place> {
    let first = pos.line.saturating_sub(LINES).max(1);
    let mut text = String::new();
    let mut starts = Vec::new();
    for n in first..pos.line {
        starts.push((text.len(), n));
        text.push_str(uncommented(doc.line(n)));
        text.push('\n');
    }
    starts.push((text.len(), pos.line));
    let line = doc.line(pos.line);
    text.push_str(&line[..crate::engine::source::byte_of_col(line, pos.col).min(line.len())]);
    let arg = argument_at(&text)?;
    // the `\`'s line and column, from its offset in the window
    let off = text.len() - arg.inside.len() - 1 - arg.groups.len();
    let off = text[..off].rfind('\\')?;
    let &(start, n) = starts.iter().rev().find(|(s, _)| *s <= off)?;
    let at = Pos::new(n, crate::engine::source::col_of_byte(&text[start..], off - start));
    Some(Place {
        cmd: arg.cmd.to_string(),
        open: arg.open,
        groups: arg.groups.to_string(),
        inside: arg.inside.to_string(),
        at,
    })
}

/// The id of the file at `path` in the run.
pub(super) fn file_of(analysis: &Analysis, path: &str) -> Option<crate::engine::tex::FileId> {
    let path = std::path::Path::new(path);
    (0..analysis.files.len())
        .filter_map(|i| crate::engine::tex::FileId::try_from(i).ok())
        .find(|&i| crate::project::paths::same_file(std::path::Path::new(analysis.file_name(i)), path))
}

/// Whether the run set the character at `span` as text: a verbatim `\`.
fn set_as_text(analysis: &Analysis, span: Span) -> bool {
    analysis.facts.text.at.iter().any(|s| s.span == span)
}

/// Whether a number with a unit's first letters stands at the cursor (`2.5c`) where a
/// quantity goes: an argument's or a key's value, or after a control word (`\\vskip 2c`).
fn dimension(before: &str, after: &str) -> bool {
    let letters = before.chars().rev().take_while(char::is_ascii_alphabetic).count();
    let head = &before[..before.len() - letters];
    let digits = head.chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').count();
    let lead = head[..head.len() - digits].trim_end_matches([' ', '-', '+']);
    let word = lead.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '@');
    digits > 0
        && (lead.ends_with(['{', '[', '(', '=', ',']) || (word.len() < lead.len() && word.ends_with('\\')))
        && !after.starts_with(|c: char| c.is_ascii_alphanumeric())
}
