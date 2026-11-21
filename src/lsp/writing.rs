//! The document's own LaTeX in what a widget writes, for the one printer ([`super::snapshot`]) and
//! the code actions ([`super::plugins::rewrite`]). Inserted text that is the whole body of a
//! document command without arguments, standing where the run typeset prose, is written as that
//! command (`\model` for `TinyNet`). A written call that then stands twice gets a note that
//! offers to extract it. Only the inserted text changes: the bytes around it stay.
//!
//! Where TeX typesets text is a run fact, not a guess from the source's shape: an occurrence is
//! rewritten only when plain characters lead from it back to a character the run typeset as the
//! document's word prose (no math, code, verbatim, typewriter) on every path, and the command's
//! one definition ran before that place. A comment, a verbatim body, an argument the run never
//! typeset (`\label`, `\href`'s URL, an unknown role) has no such character, so nothing changes
//! there. Named assumption: a call that typesets its argument's characters unchanged typesets a
//! command expanding to them the same way (`conventions.writing.macros`, off by default).

use super::snapshot::Edit;
use crate::analysis::facts::Definition;
use crate::analysis::machine::Analysis;
use crate::engine::tex::{FileId, Span};
use crate::text::Text;

/// The document's commands without arguments whose body is plain enough to stand for: name,
/// body as written and its definition, the longest body first. Only a name with this one
/// definition in the whole run, made on every path in a document file's own text (no group, no
/// environment, no macro body that runs it later), and still its meaning when the run ended.
pub(super) fn own_macros(a: &Analysis) -> Vec<(String, String, &Definition)> {
    let mut count: crate::fixed::HashMap<_, u32> = Default::default();
    for d in &a.facts.defs {
        *count.entry(d.name).or_default() += 1;
    }
    let mut out: Vec<(String, String, &Definition)> = Vec::new();
    for d in &a.facts.defs {
        // cheap tests first: a grouped definition is never one in force for the document
        let Some(m) = d.mac.as_ref().filter(|m| m.arity() == 0 && d.depth == 0) else { continue };
        let name = a.interner.name(d.name);
        let in_force = a.env.meaning(d.name).as_macro().is_some_and(|f| f.replacement_text == m.replacement_text);
        let own = d.context.is_empty() && crate::queries::query::origin(a, d.span.file) == "document";
        let once = count[&d.name] == 1 && d.certain && d.stated;
        if !own || !once || !in_force || !name.bytes().all(|c| c.is_ascii_alphabetic()) {
            continue;
        }
        let body = written(&crate::engine::tex::detokenize(&m.replacement_text, &a.interner));
        // a body of one command is an alias, not text the document names
        let alias = body.starts_with('\\') && body[1..].bytes().all(|c| c.is_ascii_alphabetic());
        if body.chars().count() < 2 || body != body.trim() || body.contains(['#', '%']) || alias || !balanced(&body) {
            continue;
        }
        out.push((name.to_string(), body, d));
    }
    out.sort_by_key(|(_, b, _)| std::cmp::Reverse(b.len()));
    out
}

/// A detokenized body as a source writes it: the space TeX shows after a control word dropped
/// before anything but a letter (`\mathbb {R}` is `\mathbb{R}`).
fn written(body: &str) -> String {
    let mut out = String::new();
    let mut word = false;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ' ' && word && chars.peek().is_some_and(|n| !n.is_ascii_alphabetic()) {
            word = false;
            continue;
        }
        word = match c {
            '\\' => chars.peek().is_some_and(char::is_ascii_alphabetic),
            c if c.is_ascii_alphabetic() => word,
            _ => false,
        };
        out.push(c);
    }
    out
}

fn balanced(t: &str) -> bool {
    let mut depth = 0i32;
    for c in t.chars() {
        depth += match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        };
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// A character that changes no state in running text: a letter or other character that is no
/// shorthand under standard catcodes, or a space.
fn plain(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, ' ' | '\n' | '.' | ',' | '-')
}

/// What decides a rewrite in one file: its typeset characters and the load tree.
struct Site<'a> {
    a: &'a Analysis,
    text: &'a Text,
    file: FileId,
    /// Per source position of `file`: the character every run typeset there as word prose, `None`
    /// where some run set something else or not as prose.
    prose: crate::fixed::HashMap<Span, Option<char>>,
}

impl<'a> Site<'a> {
    fn new(a: &'a Analysis, text: &'a Text, file: FileId) -> Site<'a> {
        let view = crate::linter::text::View::of(a, &crate::linter::fix::Sources::default());
        let mut prose: crate::fixed::HashMap<Span, Option<char>> = Default::default();
        for i in 0..view.len() {
            let Some(span) = view.span(i).filter(|s| s.file == file) else { continue };
            let c = view.char_at(i).filter(|_| view.is_word_prose(i) && view.certain(i));
            let e = prose.entry(span).or_insert(c);
            if *e != c {
                *e = None;
            }
        }
        Site { a, text, file, prose }
    }

    fn span(&self, offset: usize) -> Span {
        let p = self.text.pos(offset);
        Span { file: self.file, line: p.line, col: p.col }
    }

    /// The place of the character before `chars` (inserted, read backwards) and then before byte
    /// `old` of the file, when only plain characters lead back to one the run typeset as the
    /// character written there in word prose.
    fn anchor(&self, chars: impl Iterator<Item = char>, old: usize) -> Option<Span> {
        if !chars.into_iter().all(plain) {
            return None;
        }
        for (at, c) in self.text.body()[..old].char_indices().rev() {
            if !plain(c) {
                return None;
            }
            if c.is_ascii_alphanumeric() {
                let span = self.span(at);
                return (self.prose.get(&span) == Some(&Some(c))).then_some(span);
            }
        }
        None
    }

    /// Whether the definition at `def` ran before `site`: both read straight from their files,
    /// compared where their chains of `\input`s from the main file part.
    fn before(&self, def: Span, site: Span) -> bool {
        let (Some(d), Some(s)) = (self.chain(def), self.chain(site)) else { return false };
        for (x, y) in d.iter().zip(&s) {
            if x.file != y.file {
                return false;
            }
            if (x.line, x.col) != (y.line, y.col) {
                return (x.line, x.col) < (y.line, y.col);
            }
        }
        false
    }

    /// The places from the main file down to `at`: each file's one load site, then `at`.
    fn chain(&self, at: Span) -> Option<Vec<Span>> {
        let mut out = vec![at];
        while out[0].file != self.a.main_file {
            let mut loads = self.a.facts.loads.iter().filter(|l| l.file == Some(out[0].file));
            let load = loads.next().filter(|l| l.certain)?;
            if loads.next().is_some() || out.len() > self.a.files.len() {
                return None;
            }
            out.insert(0, load.span);
        }
        Some(out)
    }
}

/// The bytes of `t` that replace `old`: what is left between their common prefix and suffix.
fn inserted(old: &str, t: &str) -> (usize, usize) {
    let pre = old.chars().zip(t.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum::<usize>();
    let room = old.len().min(t.len()) - pre;
    let suf = old.chars().rev().zip(t.chars().rev()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8());
    let suf = suf.scan(0, |n, l| {
        *n += l;
        (*n <= room).then_some(*n)
    });
    (pre, t.len() - suf.last().unwrap_or(0))
}

/// `t`, replacing bytes `from..to` of the file, with each document command's body that the
/// edit inserts as a whole word where the run typeset prose written as that command.
fn with_macros(site: &Site, macros: &[(String, String, &Definition)], from: usize, to: usize, t: &str) -> String {
    let text = site.text.body();
    let (lo, hi) = inserted(&text[from..to], t);
    let mut chosen: Vec<(usize, usize, &str)> = Vec::new();
    for (name, body, def) in macros {
        if !body.chars().all(plain) || body.contains('\n') {
            continue;
        }
        for (k, _) in t[lo..hi].match_indices(body.as_str()) {
            let (k, end) = (lo + k, lo + k + body.len());
            if end > hi || chosen.iter().any(|&(a, b, _)| k < b && a < end) {
                continue;
            }
            let prev = t[..k].chars().next_back().or_else(|| text[..from].chars().next_back());
            let next = t[end..].chars().next().or_else(|| text[to..].chars().next());
            let letter = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
            if letter(prev) || letter(next) {
                continue;
            }
            // the old text the edit keeps before `lo` counts as the file's
            let Some(anchor) = site.anchor(t[lo..k].chars().rev(), from + lo) else { continue };
            if site.before(def.span, anchor) {
                chosen.push((k, end, name));
            }
        }
    }
    chosen.sort();
    let mut out = String::with_capacity(t.len());
    let mut last = 0;
    for (k, end, name) in chosen {
        let next = t[end..].chars().next().or_else(|| text[to..].chars().next());
        out.push_str(&t[last..k]);
        out.push('\\');
        out.push_str(name);
        if next.is_some_and(|c| c.is_ascii_alphabetic() || c.is_whitespace()) {
            out.push_str("{}");
        }
        last = end;
    }
    out.push_str(&t[last..]);
    out
}

/// The calls (`\name` with its groups) in `t` of `min` bytes or more.
fn pieces(t: &str, min: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, _) in t.match_indices('\\') {
        if let Some((name_end, groups, end)) = super::plugins::call_at(t, i)
            && !groups.is_empty()
            && name_end > i + 1
            && !matches!(&t[i + 1..name_end], "begin" | "end")
            && end - i >= min
        {
            out.push(t[i..end].to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `edits` over `doc`, the run's file `file`, with the document's own commands written in, and
/// notes for a written call that now stands twice. Without the file no command is written in.
pub(super) fn clean(a: &Analysis, doc: &Text, file: Option<FileId>, edits: Vec<Edit>) -> (Vec<Edit>, Vec<String>) {
    let cfg = &a.settings.conventions.writing;
    let text = doc.body();
    let macros = if cfg.macros && file.is_some() { own_macros(a) } else { Vec::new() };
    let edits: Vec<Edit> = match file.filter(|_| !macros.is_empty()) {
        Some(f) => {
            let site = Site::new(a, doc, f);
            edits.into_iter().map(|(s, e, t)| (s, e, with_macros(&site, &macros, s, e, &t))).collect()
        }
        None => edits,
    };
    let mut after = String::new();
    let mut last = 0;
    for (s, e, t) in &edits {
        after.push_str(&text[last..*s]);
        after.push_str(t);
        last = *e;
    }
    after.push_str(&text[last..]);
    let mut notes = Vec::new();
    for p in edits.iter().flat_map(|(.., t)| pieces(t, cfg.repeat_min)) {
        let (was, now) = (text.matches(p.as_str()).count(), after.matches(p.as_str()).count());
        let note = format!("`{p}` now stands {now} times: Extract to \\newcommand names it once");
        if now >= 2 && now > was && !notes.contains(&note) {
            notes.push(note);
        }
    }
    (edits, notes)
}

/// [`clean`] of LSP edits: a code action's. Edits that overlap stay as they are.
pub(super) fn clean_lsp(
    a: &Analysis,
    doc: &Text,
    file: Option<FileId>,
    edits: Vec<lsp_types::TextEdit>,
) -> Vec<lsp_types::TextEdit> {
    let byte = |p: lsp_types::Position| doc.offset(super::handlers::to_pos(doc, p));
    let bytes: Option<Vec<Edit>> =
        edits.iter().map(|e| Some((byte(e.range.start)?, byte(e.range.end)?, e.new_text.clone()))).collect();
    let Some(mut b) = bytes else { return edits };
    b.sort_by_key(|e| (e.0, e.1));
    if b.windows(2).any(|w| w[0].1 > w[1].0) {
        return edits;
    }
    super::snapshot::lsp_edits(doc, clean(a, doc, file, b).0)
}
