//! Splicing the last run's rest after an edit. The exact splice and the provisional one
//! (the lax page model) both reuse it once the states meet.

use super::checkpoint::{Window, edited_lines, mentions};
use super::{Analysis, Machine};
use crate::fixed::HashMap;
use std::rc::Rc;

impl Machine<'_> {
    /// [`super::RunHooks::provisional`]: the state equals `old`'s apart from the page being built, so
    /// the last run's rest, spliced in, is what a lax page model shows; its pages are marked
    /// not exact. Once per run; this run goes on exact.
    pub(super) fn lax_splice(&mut self, old: &Machine, reads: usize) {
        if !self.checkpoints.on.as_mut().is_some_and(|t| t.prior.rest_holds(reads)) {
            return;
        }
        let Some(taking) = self.checkpoints.on.as_ref() else { return };
        let (Some((id, source, _)), Some(edit), Some(end)) =
            (&taking.provisional, &taking.prior.edit, &taking.prior.end)
        else {
            return;
        };
        if self.mode != crate::engine::mode::Modes::VERTICAL || self.differs_from(old, true).is_some() {
            return;
        }
        let Some(window) = edited_lines(&self.input, &old.input, &self.out.files, edit, taking.prior.window) else {
            return;
        };
        let since = taking.since;
        let (mine, theirs) = (comparable_path(self, true, taking.parted), comparable_path(old, true, taking.parted));
        let traces =
            [&mine, &self.env.trace((since.0, since.1), &[]), &theirs, &old.env.trace((since.0, since.1), &[])];
        if mine != theirs || traces.iter().any(|t| mentions(t, window)) {
            return;
        }
        let (id, source, mut end) = (*id, source.clone(), (**end).clone());
        if splice_out(&self.out, &old.out, &mut end.out, window, since.2, true).is_err()
            || splice_records(self, old, &mut end, window).is_err()
        {
            return;
        }
        // where the rest's pages break, the lax model does not know
        for page in end.out.facts.text.pages.iter_mut().filter(|p| p.0 >= since.2) {
            page.2 = false;
        }
        end.started = self.started;
        let Some((.., then)) = self.checkpoints.on.as_mut().and_then(|t| t.provisional.take()) else { return };
        then(end.conclude(id, &source));
    }
}

/// A machine's path state as two runs that differ by an edit before here are compared:
/// the source position, the document mark and the open calls' absolute counts left out
/// ([`super::calls::CallRec::alike_view`]); `lax` also leaves out the page being built.
pub(super) fn comparable_path(m: &Machine, lax: bool, since: Parted) -> String {
    format!("{:?}", comparable_state(m, lax, since))
}

/// The source read, the groups opened and the assignments made when two runs parted.
pub(super) type Parted = (u64, u64, u64);

/// [`comparable_path`] before it is written out. A loop is named by the source read when
/// it began: one begun since the runs parted (`since`) is named by how long ago.
pub(super) fn comparable_state(m: &Machine, lax: bool, since: Parted) -> super::PathState {
    let from = since.0;
    let mut p = super::PathState { source_progress: 0, doc_mark: 0, ..m.save_path() };
    let view = |r: &Rc<super::calls::CallRec>| Rc::new(r.alike_view(lax).ending_view(m.natural, m.input.len()));
    p.call_recs = Rc::new(p.call_recs.iter().map(view).collect());
    let ago = |(at, by, read): super::LoopId| match read > from {
        true => (at, by, 1 << 63 | m.source_progress.saturating_sub(read)),
        false => (at, by, read),
    };
    p.loop_bounds = Rc::new(p.loop_bounds.iter().map(|&(id, bound)| (ago(id), bound)).collect());
    p.summary_of = Rc::new(p.summary_of.iter().map(|&id| ago(id)).collect());
    // A token list holds the group and the assignment count it was pushed at: likewise.
    let (groups, assigned) = (m.env.group_ids().0, m.env.assignments);
    for frame in p.input.0.iter_mut() {
        if let super::Frame::Tokens { at, .. } = &**frame
            && (at.1 > since.1 || at.2 > since.2)
            && let super::Frame::Tokens { at, .. } = Rc::make_mut(frame)
        {
            let ago =
                |now: u64, then: u64, from: u64| if then > from { 1 << 63 | now.saturating_sub(then) } else { then };
            (at.1, at.2) = (ago(groups, at.1, since.1), ago(assigned, at.2, since.2));
        }
    }
    if lax {
        // the output routines fired so far, and the braces they read: the page builder's.
        // `align_state` counts only inside an alignment cell (tex.web § 342).
        p.pages = 0;
        if p.cell_base.is_none() {
            p.align_state = 0;
        }
    }
    p
}

/// Serde's bytes of `t`, to compare values of any recorded type.
fn bytes<T: serde::Serialize + ?Sized>(t: &T) -> Option<Vec<u8>> {
    postcard::to_allocvec(t).ok()
}

fn same<T: serde::Serialize + ?Sized>(a: &T, b: &T) -> bool {
    matches!((bytes(a), bytes(b)), (Some(x), Some(y)) if x == y)
}

/// `end` becomes `new`'s rows, updated by what the last run changed after
/// `at`, and the rows it added. Rows are kept per index (a site's row is
/// updated in place), so a row both runs hold differently must be one the
/// rest cannot touch: placed on an edited line, of which no token is left.
/// Whether `new` holds the rows `at` holds, each alike.
fn alike<T: PartialEq>(new: &impl Rows<T>, at: &impl Rows<T>) -> bool {
    new.count() == at.count() && (0..at.count()).all(|i| new.row(i) == at.row(i))
}

fn rows<T: PartialEq + Clone>(
    new: &impl Rows<T>,
    at: &impl Rows<T>,
    end: &mut impl Rows<T>,
    edited: impl Fn(&T) -> bool,
) -> bool {
    if end.count() < at.count() {
        return false;
    }
    // This run holds the rows the last one held here: the end's stand as they are, none is copied.
    if alike(new, at) {
        return true;
    }
    let mut out = Vec::with_capacity(new.count() + end.count() - at.count());
    for i in 0..at.count() {
        let (was, mine, last) = (at.row(i).expect("in range"), new.row(i), end.row(i).expect("longer"));
        let rest_changed = last != was;
        match mine {
            Some(mine) if mine == was => out.push(last.clone()),
            Some(mine) if !rest_changed && edited(mine) && edited(was) => out.push(mine.clone()),
            None if !rest_changed && edited(was) => {}
            _ => return false,
        }
    }
    let added = (at.count()..new.count()).filter_map(|i| new.row(i));
    if !added.clone().all(&edited) {
        return false;
    }
    out.extend(added.cloned());
    out.extend((at.count()..end.count()).filter_map(|i| end.row(i)).cloned());
    end.set(out);
    true
}

/// A list [`rows`] reads by index: a `Vec`, or a [`crate::fixed::Chunked`] checkpoints share.
trait Rows<T> {
    fn row(&self, i: usize) -> Option<&T>;
    fn count(&self) -> usize;
    fn set(&mut self, rows: Vec<T>);
}

impl<T> Rows<T> for Vec<T> {
    fn row(&self, i: usize) -> Option<&T> {
        self.get(i)
    }
    fn count(&self) -> usize {
        self.len()
    }
    fn set(&mut self, rows: Vec<T>) {
        *self = rows;
    }
}

impl<T: Clone> Rows<T> for crate::fixed::Chunked<T> {
    fn row(&self, i: usize) -> Option<&T> {
        self.get(i)
    }
    fn count(&self) -> usize {
        self.len()
    }
    fn set(&mut self, rows: Vec<T>) {
        *self = rows.into();
    }
}

/// `end` becomes `new` with the entries the last run changed after `at`. An
/// entry this run holds differently from the last must be one the rest of the
/// run cannot touch: on an edited line (`window`), when no token of it is left.
fn keyed<K: std::hash::Hash + Eq + Clone, V: PartialEq + Clone>(
    new: &HashMap<K, V>,
    at: &HashMap<K, V>,
    end: &mut HashMap<K, V>,
    window: impl Fn(&K) -> bool,
) -> bool {
    let mut out = new.clone();
    for k in end.keys().chain(at.keys()) {
        if end.get(k) != at.get(k) {
            if new.get(k) != at.get(k) {
                return false;
            }
            match end.get(k) {
                Some(v) => out.insert(k.clone(), v.clone()),
                None => out.remove(k),
            };
        }
    }
    let untouched = |k: &K| end.get(k) == at.get(k);
    if new.keys().chain(at.keys()).any(|k| new.get(k) != at.get(k) && untouched(k) && !window(k)) {
        return false;
    }
    *end = out;
    true
}

/// A value holding offsets into the characters set (how many came before).
trait Offsets: Sized + Clone + PartialEq {
    /// The first offset.
    fn first(&self) -> u32;
    /// Every offset moved by [`shift`].
    fn moved(&self, by: Move) -> Option<Self>;
}

/// Where the characters the runs set differently begin and where, in the last run's count,
/// they end, and by how much the later ones moved.
type Move = (u32, u32, i64);

/// An offset before the characters set differently stays, one after them moves,
/// one among them is not known.
fn shift(x: u32, (resume, from, by): Move) -> Option<u32> {
    match x {
        _ if x < resume => Some(x),
        _ if x < from => None,
        _ => u32::try_from(i64::from(x) + by).ok(),
    }
}

impl Offsets for u32 {
    fn first(&self) -> u32 {
        *self
    }
    fn moved(&self, by: Move) -> Option<Self> {
        shift(*self, by)
    }
}

/// A character range.
impl Offsets for (u32, u32) {
    fn first(&self) -> u32 {
        self.0
    }
    fn moved(&self, by: Move) -> Option<Self> {
        Some((shift(self.0, by)?, shift(self.1, by)?))
    }
}

/// A character range and what lies under it.
impl Offsets for (u32, u32, Option<u16>) {
    fn first(&self) -> u32 {
        self.0
    }
    fn moved(&self, by: Move) -> Option<Self> {
        Some((shift(self.0, by)?, shift(self.1, by)?, self.2))
    }
}

/// An offset and what was set there.
macro_rules! offset_first {
    ($($t:ty),*) => {$(
        impl Offsets for $t {
            fn first(&self) -> u32 {
                self.0
            }
            fn moved(&self, by: Move) -> Option<Self> {
                let mut x = *self;
                x.0 = shift(x.0, by)?;
                Some(x)
            }
        }
    )*};
}
use crate::analysis::facts::{Align, Indent, Just, Rule};
use crate::engine::tex::Span;
offset_first!(
    (u32, Span),
    (u32, Span, bool),
    (u32, Align),
    (u32, u16),
    (u32, crate::engine::value::Scaled),
    (u32, Just),
    (u32, Indent),
    (u32, crate::engine::env::NodeId, bool),
    (u32, bool, u32),
    (u32, Rule, Span, u32),
    (u32, crate::engine::linebreak::Params),
    (u32, crate::analysis::facts::LineEnd)
);

/// `end` becomes `new`'s list and the rest's additions, moved by `by` (a
/// longer word moves every later character). The rest may only add, and what
/// it reads of the list, its last entry against the characters set so far,
/// must stand alike in both runs.
fn offsets<T: Offsets>(new: &[T], at: &[T], end: &mut Vec<T>, (now, then, by): (u32, u32, Move)) -> bool {
    let rel = |l: &[T], len: u32| l.last().map(|x| i64::from(x.first()) - i64::from(len));
    let reads_alike = match (rel(new, now), rel(at, then)) {
        (Some(x), Some(y)) => x == y || (x < 0 && y < 0),
        (x, y) => x == y,
    };
    let Some(rest) = end.get(at.len()..).filter(|_| reads_alike && end[..at.len()] == *at) else { return false };
    let Some(rest) = rest.iter().map(|x| x.moved(by)).collect::<Option<Vec<T>>>() else { return false };
    *end = new.iter().cloned().chain(rest).collect();
    true
}

/// [`crate::analysis::facts::Text`] spliced: per character this run's then
/// the rest's, offsets moved, and how they move. A stack the rest pops must be alike here.
fn splice_text(
    new: &crate::analysis::facts::Text,
    at: &crate::analysis::facts::Text,
    end: &mut crate::analysis::facts::Text,
    resume: u32,
) -> Result<Move, &'static str> {
    let k = at.at.len();
    // Characters both runs set alike before the edited ones keep their offsets, the ones
    // set alike after them move together.
    let from = (resume as usize).min(k).min(new.at.len());
    let resume = from + new.at[from..].iter().zip(&at.at[from..k]).take_while(|(a, b)| a == b).count();
    let (mine, theirs): (Vec<char>, Vec<char>) =
        (new.chars.chars().skip(resume).collect(), at.chars.chars().skip(resume).take(k - resume).collect());
    let same = |i: usize, j: usize| new.at[i] == at.at[j] && new.site.get(i) == at.site.get(j);
    let tail = (1..=mine.len().min(theirs.len()))
        .take_while(|&n| mine[mine.len() - n] == theirs[theirs.len() - n] && same(new.at.len() - n, k - n))
        .count();
    let resume = resume as u32;
    let by = (resume, (k - tail) as u32, new.at.len() as i64 - k as i64);
    let lens = (new.at.len() as u32, k as u32, by);
    let crate::analysis::facts::Text {
        chars,
        at: spots,
        site,
        closed,
        pars,
        par_starts,
        kept,
        open,
        arms,
        repeats,
        hyphens,
        hyphenated,
        placed,
        dropped,
        inserts,
        rules,
        align,
        math,
        skips,
        just,
        indents,
        pages,
        fills,
        open_pages: _,
        frames,
        backgrounds,
        page_drawn,
        breaks,
        lines,
    } = end;
    let per_char = spots.len() >= k && site.len() >= k && spots[..k] == at.at[..] && site[..k] == at.site[..];
    if !per_char || new.open != at.open || new.open_pages != at.open_pages {
        return Err("text");
    }
    *chars = new.chars.chars().chain(chars.chars().skip(k)).collect();
    *spots = new.at.iter().chain(&spots[k..]).copied().collect();
    *site = new.site.iter().chain(&site[k..]).copied().collect();
    *page_drawn |= new.page_drawn;
    *open = open.iter().map(|&x| shift(x, by).unwrap_or(x)).collect();
    *hyphenated = match *hyphenated == at.hyphenated {
        true => new.hyphenated,
        false => shift(*hyphenated, by).ok_or("hyphenated")?,
    };
    #[rustfmt::skip]
    let lists = [
        (offsets(&new.closed, &at.closed, closed, lens), "text closed"), (offsets(&new.pars, &at.pars, pars, lens), "text pars"),
        (offsets(&new.par_starts, &at.par_starts, par_starts, lens), "text par starts"), (offsets(&new.kept, &at.kept, kept, lens), "text kept"),
        (offsets(&new.arms, &at.arms, arms, lens), "text arms"), (offsets(&new.repeats, &at.repeats, repeats, lens), "text repeats"),
        (offsets(&new.hyphens, &at.hyphens, hyphens, lens), "text hyphens"), (offsets(&new.placed, &at.placed, placed, lens), "text placed"),
        (offsets(&new.dropped, &at.dropped, dropped, lens), "text dropped"), (offsets(&new.inserts, &at.inserts, inserts, lens), "text inserts"),
        (offsets(&new.rules, &at.rules, rules, lens), "text rules"), (offsets(&new.align, &at.align, align, lens), "text align"),
        (offsets(&new.math, &at.math, math, lens), "text math"), (offsets(&new.skips, &at.skips, skips, lens), "text skips"),
        (offsets(&new.just, &at.just, just, lens), "text just"), (offsets(&new.pages, &at.pages, pages, lens), "text pages"),
        (offsets(&new.fills, &at.fills, fills, lens), "text fills"), (offsets(&new.frames, &at.frames, frames, lens), "text frames"),
        (offsets(&new.backgrounds, &at.backgrounds, backgrounds, lens), "text backgrounds"),
        (offsets(&new.breaks, &at.breaks, breaks, lens), "text breaks"),
        (offsets(&new.lines, &at.lines, lines, lens), "text lines"),
        (offsets(&new.indents, &at.indents, indents, lens), "text indents"),
    ];
    lists.into_iter().find(|(ok, _)| !ok).map_or(Ok(by), |(_, what)| Err(what))
}

/// [`splice_out`] for what a run records outside its analysis and relates once it ends
/// ([`crate::analysis::observe::relate`]): the lines written, the names read, the names defined from keys.
pub(super) fn splice_records(
    new: &Machine,
    at: &Machine,
    end: &mut Machine,
    window: Window,
) -> Result<(), &'static str> {
    let span = |s: Span| s.file == window.0 && (window.1..=window.2).contains(&s.line);
    // The names read are indexed: left as they are where this run read what the last one had.
    let reindex = !alike(&new.name_reads.list, &at.name_reads.list);
    let mut reads = if reindex { std::mem::take(&mut end.name_reads).list } else { Default::default() };
    let checks = [
        (rows(&new.written_lines, &at.written_lines, &mut end.written_lines, |r| span(r.place())), "lines written"),
        (
            !reindex && end.name_reads.list.count() >= at.name_reads.list.count()
                || rows(&new.name_reads.list, &at.name_reads.list, &mut reads, |r| span(r.place())),
            "names read",
        ),
        (rows(&new.key_names, &at.key_names, &mut end.key_names, |r| span(r.3)), "names from keys"),
        (new.stream_files == at.stream_files, "stream files"),
    ];
    if reindex {
        end.name_reads = crate::analysis::observe::NameReads::of(reads);
    }
    checks.into_iter().find(|(ok, _)| !ok).map_or(Ok(()), |(_, what)| Err(what))
}

/// No key is on an edited line.
fn never<K>(_: &K) -> bool {
    false
}

/// [`keyed`] for a set.
fn members<K: std::hash::Hash + Eq + Clone, S>(new: &S, at: &S, end: &mut S, window: impl Fn(&K) -> bool) -> bool
where
    for<'s> &'s S: IntoIterator<Item = &'s K>,
    S: FromIterator<K>,
{
    let map = |s: &S| s.into_iter().map(|k| (k.clone(), ())).collect::<HashMap<K, ()>>();
    let mut out = map(end);
    let ok = keyed(&map(new), &map(at), &mut out, window);
    if ok {
        *end = out.into_keys().collect();
    }
    ok
}

/// The last run's end `end` with this run's record up to here (`new`) in
/// place of its own (`at`). Every field is listed: a new one must say how.
#[rustfmt::skip]
pub(super) fn splice_out(new: &Analysis, at: &Analysis, end: &mut Analysis, window: Window, resume: u32, lax: bool) -> Result<(), &'static str> {
    let span = |s: &Span| s.file == window.0 && (window.1..=window.2).contains(&s.line);
    let line = |k: &(crate::engine::tex::FileId, u32)| k.0 == window.0 && (window.1..=window.2).contains(&k.1);
    // Kept as the run's end has them: its clock and the caches filled after it.
    let Analysis { interner, graph, calls, observed, files, main_file, literate, project, plugins, distribution,
        document_depth, preamble_cost, justified_text, entry, format, config, settings, metadata, kernel_sites, fonts,
        font_states, env, catcodes, facts, trace, typeset, steps, load_steps, served_steps, limits, lines, exhausted,
        journal_size, held_tokens_left, loop_heads, peak_paths, peak_output_paths, forks, output_forks, call_memo,
        joined_slots, memory_reported, untyped, package_caches, written, written_runs, build_runs, class_caches,
        cached_files, preamble, kernel, timings: _, started: _, caching: _, hot: _, probes: _, context_probes: _, takes: _,
        role_sources: _, tagging: _, probed_roles: _, bib: _, scope_rows: _, said: _, build_pending: _, build_files: _ } = end;
    let crate::analysis::facts::Facts { defs, written_files, absent, found, shapes, types, length_fonts, vars,
        letters, expansions, loads, occurrences, literals, diagnostics, csnames, slots, groups, raised, spaces,
        kept_spaces, paragraph_enders, conditionals, quantities, colors, items, color_uses, color_stack, text,
        overfull, lines: line_reads } = facts;
    let (n, a) = (&new.facts, &at.facts);
    // What this run recorded must be what the last one had here: the rest then leaves the same.
    let alike = [
        // This run began with the last run's names ([`super::Checkpoints::analyze`]): the end takes its.
        (new.interner.holds(interner), "names"),
        // The edited line's nodes sit at other columns: the end takes this run's; the provisional one keeps the last run's.
        (new.graph.len() == at.graph.len() && (lax || graph.splice(&new.graph, &at.graph, span)), "graph"),
        (new.calls == at.calls && new.observed == at.observed, "calls"),
        (new.files == at.files, "files"),
        (same(&(new.main_file, &new.literate, &new.project, &new.plugins, &new.distribution, new.document_depth),
            &(at.main_file, &at.literate, &at.project, &at.plugins, &at.distribution, at.document_depth)), "setup"),
        (same(&(new.preamble_cost, &new.entry, &new.format, &new.config, &new.metadata, &new.kernel_sites),
            &(at.preamble_cost, &at.entry, &at.format, &at.config, &at.metadata, &at.kernel_sites)), "preamble"),
        ((&new.fonts, &new.font_states, &new.catcodes, new.exhausted) == (&at.fonts, &at.font_states, &at.catcodes, at.exhausted), "fonts"),
        ((new.journal_size, new.held_tokens_left, new.loop_heads, new.peak_paths, new.peak_output_paths, new.memory_reported)
            == (at.journal_size, at.held_tokens_left, at.loop_heads, at.peak_paths, at.peak_output_paths, at.memory_reported), "peaks"),
        ((&new.package_caches, &new.written, &new.written_runs, new.build_runs, &new.class_caches, &new.cached_files)
            == (&at.package_caches, &at.written, &at.written_runs, at.build_runs, &at.class_caches, &at.cached_files), "caches"),
        (new.preamble.is_some() == at.preamble.is_some()
            && new.kernel.is_some() == at.kernel.is_some() && same(&new.settings, &at.settings), "settings"),
    ];
    if let Some((_, what)) = alike.into_iter().find(|(ok, _)| !ok) {
        return Err(what);
    }
    *interner = new.interner.clone();
    let by = splice_text(&n.text, &a.text, text, resume)?;
    // A literal holds the characters set before it. The rest's move with them; the last
    // run's before the meeting line are compared as this run would have set them.
    let moved = |c: u32| if c < by.1 { c } else { u32::try_from(i64::from(c) + by.2).unwrap_or(u32::MAX) };
    let at_literals: Vec<_> = (a.literals.iter().cloned()).map(|l| crate::analysis::facts::Literal { chars: moved(l.chars), ..l }).collect();
    for (i, l) in literals.iter_mut().enumerate() {
        l.chars = match i < a.literals.len() { true => moved(l.chars), false => shift(l.chars, by).ok_or("literals")? };
    }
    // Counts: the rest's share added to this run's.
    for (end, new, at) in [(justified_text, new.justified_text, at.justified_text), (steps, new.steps, at.steps),
        (load_steps, new.load_steps, at.load_steps), (served_steps, new.served_steps, at.served_steps),
        (lines, new.lines, at.lines), (forks, new.forks, at.forks), (output_forks, new.output_forks, at.output_forks),
        (joined_slots, new.joined_slots, at.joined_slots), (untyped, new.untyped, at.untyped)] {
        *end = end.saturating_sub(at) + new;
    }
    for ((end, at), new) in call_memo.iter_mut().zip(at.call_memo).zip(new.call_memo) {
        *end = end.saturating_sub(at) + new;
    }
    let rest = typeset.get(at.typeset.len()..).filter(|_| typeset.starts_with(&at.typeset)).map(str::to_string);
    *typeset = format!("{}{}", new.typeset, rest.ok_or("typeset")?);
    let s = |r: &Span| span(r);
    let checks = [
        (rows(&n.defs, &a.defs, defs, |r| s(&r.span)), "defs"), (rows(&n.vars, &a.vars, vars, |r| s(&r.span)), "vars"),
        (rows(&n.letters, &a.letters, letters, |r| s(&r.0)), "letters"),
        (rows(&n.expansions, &a.expansions, expansions, |r| s(&r.span)), "expansions"),
        (rows(&n.loads, &a.loads, loads, |r| s(&r.span)), "loads"),
        // Raised in a package's file by a call on an edited line: that call's place is what moved.
        (rows(&n.occurrences, &a.occurrences, occurrences, |r| {
            s(&r.span) || [r.expanded, r.call.map(|c| c.1), r.site.map(|c| c.1)].iter().flatten().any(s)
        }), "occurrences"),
        (rows(&n.literals, &at_literals, literals, |r| s(&r.at)), "literals"),
        // A repeat is counted on its first row, which `diag_seen` names by index.
        (n.diagnostics.len() == a.diagnostics.len() && rows(&n.diagnostics, &a.diagnostics, diagnostics, |r| s(&r.span)), "diagnostics"),
        (rows(&n.groups, &a.groups, groups, |r| s(&r.0)), "groups"), (rows(&n.raised, &a.raised, raised, |r| s(&r.span)), "raised"),
        (rows(&n.conditionals, &a.conditionals, conditionals, |r| s(&r.at)), "conditionals"),
        (rows(&n.quantities, &a.quantities, quantities, |r| s(&r.span)), "quantities"),
        (rows(&n.colors, &a.colors, colors, |r| s(&r.span)), "colors"), (rows(&n.items, &a.items, items, |r| s(&r.span)), "items"),
        (rows(&n.color_uses, &a.color_uses, color_uses, |r| s(&r.span)), "color uses"),
        (rows(&n.color_stack, &a.color_stack, color_stack, |r| s(&r.span)), "color stack"),
        (rows(&new.limits, &at.limits, limits, |r| s(&r.at)), "limits"), (rows(&new.trace, &at.trace, trace, |r| s(&r.span)), "trace"),
        (members(&n.written_files, &a.written_files, written_files, never), "written files"),
        (members(&n.absent, &a.absent, absent, never), "absent"), (members(&n.found, &a.found, found, never), "found"),
        (members(&n.kept_spaces, &a.kept_spaces, kept_spaces, span), "kept spaces"),
        (members(&n.paragraph_enders, &a.paragraph_enders, paragraph_enders, never), "paragraph enders"),
        (keyed(&n.shapes, &a.shapes, shapes, never), "shapes"), (keyed(&n.types, &a.types, types, span), "types"),
        (keyed(&n.length_fonts, &a.length_fonts, length_fonts, span), "length fonts"),
        (keyed(&n.csnames, &a.csnames, csnames, span), "csnames"), (keyed(&n.slots, &a.slots, slots, span), "slots"),
        (keyed(&n.spaces, &a.spaces, spaces, span), "spaces"), (keyed(&n.overfull, &a.overfull, overfull, |k| span(&k.0)), "overfull"),
        (keyed(&n.lines, &a.lines, line_reads, line), "lines read"),
    ];
    // Kept from the end: equal here, so the rest left them as the last run's end has them.
    let _ = (interner, graph, calls, observed, files, main_file, literate, project, plugins, distribution, document_depth,
        preamble_cost, entry, format, config, settings, metadata, kernel_sites, fonts, font_states, env, catcodes,
        exhausted, journal_size, held_tokens_left, loop_heads, peak_paths, peak_output_paths, memory_reported,
        package_caches, written, written_runs, build_runs, class_caches, cached_files, preamble, kernel);
    checks.into_iter().find(|(ok, _)| !ok).map_or(Ok(()), |(_, what)| Err(what))
}
