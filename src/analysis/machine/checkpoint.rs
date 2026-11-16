//! Checkpoints make re-analysis after an edit fast. While a run reads the
//! user's files, it saves its whole state from time to time. The next run of
//! the same document resumes from the last checkpoint whose input has not
//! changed. That gives the same result as a full run.

use super::explore::Drive;
use super::{Analysis, Frame, Machine};
use crate::engine::tex::Mouth;
use crate::fixed::HashMap;
use crate::project::config::Config;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Initial number of steps between checkpoints. When more than [`KEPT`]
/// exist, every other one is dropped and the spacing doubles.
const EVERY: u64 = 1 << 23;
const KEPT: usize = 8;
/// In the document body, a checkpoint may also be taken between top-level
/// lines (a frame, a paragraph) after this fraction of the spacing.
const BODY_SHARE: u64 = 8;

/// The whole state of a run at the start of a line in a user's file, with all paths joined.
struct Checkpoint<'a> {
    machine: Machine<'a>,
    drive: Drive,
    /// Number of entries of [`Checkpoints::log`] read up to this checkpoint.
    reads: usize,
    /// Taken at or just before the last edit. Never dropped when the list thins out.
    pinned: bool,
    /// The latest checkpoint on the way to the edit's line. The next one on the way replaces it.
    toward: bool,
}

/// The checkpoints a run takes, and the drive it resumes with.
#[derive(Default)]
pub(super) struct Taker<'a> {
    on: Option<Box<Taking<'a>>>,
    resume: Option<Box<Drive>>,
}

struct Taking<'a> {
    list: Vec<Checkpoint<'a>>,
    log: Vec<crate::project::overlay::Read>,
    every: u64,
    next: u64,
    /// Step count at the last checkpoint.
    last: u64,
    /// Whether this run already pinned a checkpoint for its edit.
    pinned: bool,
    /// Distance in lines from the last checkpoint on the way to the edit's line.
    toward: u32,
    /// Whether `\begin{document}` had been read at the last checkpoint.
    document: bool,
    /// File and line where the last edit began. The next edit is likely
    /// nearby, so a checkpoint is taken there.
    edit: Option<Edit>,
    /// The last run, whose state this one may meet again after the edit.
    prior: Prior<'a>,
    /// Assignments and saves made when this run resumed: the runs differ only in what came after.
    since: (u64, usize, u32),
    /// Where this run's state first equals the last run's (see [`Checkpoints::converged`]).
    converged: Option<(String, u32, u64)>,
    /// The last run's end with this run's facts before the meeting line: this run's end.
    spliced: Option<Box<Machine<'a>>>,
    /// What the last run read after the meeting line.
    tail: Vec<crate::project::overlay::Read>,
}

/// What a run keeps of the one before to meet its state again.
#[derive(Default)]
struct Prior<'a> {
    /// Its checkpoints that no longer hold, in run order.
    stale: Vec<Checkpoint<'a>>,
    /// Its machine at its end, before concluding.
    end: Option<Box<Machine<'a>>>,
    /// Every file it read, in order.
    log: Vec<crate::project::overlay::Read>,
    steps: u64,
    /// The file edited since.
    edit: Option<PathBuf>,
}

/// A file, and the line where it changed.
type Edit = (PathBuf, u32);

/// A cloned machine does not take checkpoints.
impl Clone for Taker<'_> {
    fn clone(&self) -> Self {
        Taker::default()
    }
}

impl Taker<'_> {
    /// The drive a resumed run continues with. Returned only once.
    pub(super) fn resume(&mut self) -> Option<Drive> {
        self.resume.take().map(|d| *d)
    }
}

impl Machine<'_> {
    /// Whether to take a checkpoint now. Only at a line start in a user's file,
    /// outside any scan, and when enough steps passed or the document just began.
    pub(super) fn checkpoint_due(&self) -> bool {
        let Some(taking) = &self.checkpoints.on else { return false };
        let Some((mouth, lookahead)) = reading_file(&self.input) else { return false };
        let document = self.out.document_depth.is_some();
        let gap = self.edit_gap(taking, mouth);
        let edit = gap == Some(0);
        // Before `\begin{document}`, which reads the `.aux` that a rerun may change.
        let opens = !document && opens_document(mouth);
        let since = self.out.steps - taking.last;
        // Approaching the edit's line, take one each time the distance halves.
        // The next edit is likely there again and resumes close to it.
        let toward = gap.is_some_and(|g| g > 0 && g.saturating_mul(2) <= taking.toward);
        // Top-level lines of the body (a frame, a paragraph), so an edit there resumes close by.
        let top = || {
            since >= taking.every / BODY_SHARE
                && self.out.document_depth.is_some_and(|d| usize::from(d) == self.env.group_level())
                && !self.env.many_groups()
        };
        let due = self.out.steps >= taking.next || document != taking.document || edit || opens || toward || top();
        // Not while a path through an undecided conditional runs: resuming there would number its meanings anew.
        if self.halted || !due || self.env.forking() {
            return false;
        }
        // A lookahead in the preamble (`\@ifnextchar[`) may hold the next line's first token.
        let settled = match lookahead {
            true => opens,
            false => mouth.line_mark().is_some_and(|m| !m.ended),
        };
        // Not inside `\include`: TeX continues reading after the included file.
        !self.input.iter().any(|f| matches!(f, Frame::Boundary))
            && settled
            && self.project_file(mouth.file)
            && self.scans.is_empty()
            && self.scan_starts.is_empty()
            && self.cacheable_load.is_none()
            && self.recording_package().is_none()
    }

    /// Saves the run's state and `drive` as a checkpoint.
    pub(super) fn take_checkpoint(&mut self, drive: Drive) {
        let machine = self.clone();
        let (steps, document) = (self.out.steps, self.out.document_depth.is_some());
        let gap = match (&self.checkpoints.on, reading_file(&self.input)) {
            (Some(taking), Some((mouth, _))) => self.edit_gap(taking, mouth),
            _ => None,
        };
        let Some(taking) = self.checkpoints.on.as_mut() else { return };
        taking.log.extend(crate::project::overlay::log(true));
        let (toward, pinned) = (gap.is_some_and(|g| g > 0), gap.is_some());
        match gap {
            Some(0) => taking.edit = None,
            Some(g) => taking.toward = g,
            None => {}
        }
        if self.cfg.verbose >= 2 {
            crate::util::progress::note!(
                "satex: checkpoint at {:?}, step {steps}",
                reading_file(&self.input).map(|(m, _)| m.position())
            );
        }
        // The first checkpoint for this edit unpins the previous edit's.
        // A checkpoint on the way to the edit replaces the previous one on the way.
        if pinned && !taking.pinned {
            taking.list.iter_mut().for_each(|c| c.pinned = false);
            taking.pinned = true;
        }
        if toward {
            taking.list.retain(|c| !c.toward);
        } else {
            taking.list.iter_mut().for_each(|c| c.toward = false);
        }
        taking.list.push(Checkpoint { machine, drive, reads: taking.log.len(), pinned, toward });
        if taking.list.len() > KEPT {
            // Drop every other one. The last and the pinned ones stay.
            let mut n = taking.list.len();
            taking.list.retain(|c| {
                n -= 1;
                c.pinned || n % 2 == 0
            });
            taking.every *= 2;
        }
        (taking.next, taking.last, taking.document) = (steps + taking.every, steps, document);
    }

    /// At a line start, compares the state with the last run's checkpoint on
    /// the same line with the same text to come. States that differ cannot go
    /// on alike. Alike ones are where the old run's rest could be reused. Only recorded for now.
    pub(super) fn probe_convergence(&mut self) {
        let Some(taking) = &self.checkpoints.on else { return };
        if taking.prior.stale.is_empty() || taking.converged.is_some() || self.env.forking() || !self.scans.is_empty() {
            return;
        }
        let Some((mouth, false)) = reading_file(&self.input) else { return };
        if !mouth.line_mark().is_some_and(|m| !m.ended) {
            return;
        }
        let here = files_at(&self.input);
        let Some(i) = taking.prior.stale.iter().position(|c| files_at(&c.machine.input) == here) else { return };
        let (line, path) = (mouth.position().0, self.out.files[usize::from(mouth.file)].path.clone());
        let Some(taking) = self.checkpoints.on.as_mut() else { return };
        let mut old: Vec<Checkpoint> = taking.prior.stale.drain(..=i).collect();
        let total = taking.prior.steps;
        let Some(old) = old.pop() else { return };
        let (reads, old) = (old.reads, old.machine);
        let differs = self.differs_from(&old).or_else(|| self.splice(&old, reads).err());
        if self.cfg.verbose >= 2 {
            crate::util::progress::note!(
                "satex: state at {path}:{line} vs the last run's: {}",
                differs.unwrap_or("alike")
            );
        }
        if differs.is_none()
            && let Some(taking) = self.checkpoints.on.as_mut()
        {
            taking.converged = Some((path, line, total.saturating_sub(old.out.steps)));
        }
    }

    /// Makes the last run's end, with this run's facts up to here, this run's
    /// end, and halts. `old` is the last run's state here, after `reads` of its log.
    /// Sound when the states are equal here and the edited lines left no token
    /// behind: the rest then runs alike, and records alike, in both runs.
    fn splice(&mut self, old: &Machine, reads: usize) -> Result<(), &'static str> {
        let prior = self.checkpoints.on.as_mut().map(|t| &mut t.prior).ok_or("no checkpoints")?;
        let (Some(edit), true) = (prior.edit.clone(), prior.end.is_some()) else {
            return Err("no end of the last run");
        };
        let window = edited_lines(&self.input, &old.input, &self.out.files, &edit).ok_or("edit elsewhere")?;
        // Between paragraphs: one going on would be broken into lines with the edited text.
        if self.mode != crate::engine::mode::Modes::VERTICAL {
            return Err("inside a paragraph");
        }
        if (&self.diag_seen, &self.diag_raised, &self.diag_live) != (&old.diag_seen, &old.diag_raised, &old.diag_live) {
            return Err("diagnostics raised");
        }
        // Counts of tokens taken and assignments made differ by the edit's. They are compared
        // only with marks taken since, which differ alike, or before the edit, which both passed.
        let path = |m: &Machine| format!("{:?}", super::PathState { source_progress: 0, doc_mark: 0, ..m.save_path() });
        let (mine, theirs) = (path(self), path(old));
        if mine != theirs {
            return Err("path state");
        }
        let since = self.checkpoints.on.as_ref().map_or((0, 0, 0), |t| t.since);
        let traces = [mine, self.env.trace((since.0, since.1)), theirs, old.env.trace((since.0, since.1))];
        if traces.iter().any(|t| mentions(t, window)) {
            return Err("tokens of the edited lines");
        }
        // Last: it splices `end` in place, so a failure leaves no end for a later line.
        let mut end = self.checkpoints.on.as_mut().and_then(|t| t.prior.end.take()).ok_or("no end of the last run")?;
        splice_out(&self.out, &old.out, &mut end.out, window, since.2)?;
        end.started = self.started;
        let Some(taking) = self.checkpoints.on.as_mut() else { return Err("no checkpoints") };
        taking.tail = taking.prior.log.get(reads..).unwrap_or_default().to_vec();
        taking.spliced = Some(end);
        Ok(())
    }

    /// Whether the run just spliced the last run's rest: it then keeps a
    /// checkpoint here, where the next edit's run may meet it, and halts.
    pub(super) fn spliced(&self) -> bool {
        self.checkpoints.on.as_ref().is_some_and(|t| t.spliced.is_some())
    }

    /// The first part of the state that differs from `old`'s, a necessary
    /// condition for both runs to go on alike. Facts are compared by count.
    fn differs_from(&self, old: &Machine) -> Option<&'static str> {
        let since = self.checkpoints.on.as_ref().map_or((0, 0, 0), |t| t.since);
        let (f, g) = (&self.out.facts, &old.out.facts);
        let counts = |f: &crate::analysis::facts::Facts| {
            [f.diagnostics.len(), f.defs.len(), f.expansions.len(), f.occurrences.len(), f.loads.len()]
        };
        [
            (!self.env.alike(&old.env, (since.0, since.1)), "bindings"),
            (self.catcodes != old.catcodes, "catcodes"),
            (self.conds.len() != old.conds.len(), "conditionals"),
            (!Rc::ptr_eq(&self.outputs, &old.outputs) && self.outputs != old.outputs, "files written"),
            (!Rc::ptr_eq(&self.pending, &old.pending) && self.pending != old.pending, "writes held"),
            ((self.mode, self.list, self.pages) != (old.mode, old.list, old.pages), "mode or pages"),
            (self.natural != old.natural, "page so far"),
            (self.out.interner.len() != old.out.interner.len(), "names"),
            (self.out.graph.len() != old.out.graph.len(), "graph"),
            (self.out.files.len() != old.out.files.len(), "files read"),
            (counts(f) != counts(g), "facts"),
        ]
        .into_iter()
        .find_map(|(differs, what)| differs.then_some(what))
    }

    /// Lines from the current position to the last edit's line, 0 at or past it.
    /// `None` when there is no edit or it is in another file.
    fn edit_gap(&self, taking: &Taking, mouth: &Mouth) -> Option<u32> {
        let (path, line) = taking.edit.as_ref()?;
        (Path::new(&self.out.files[usize::from(mouth.file)].path) == path)
            .then(|| line.saturating_sub(mouth.position().0))
    }
}

/// The file being read and whether a lookahead put tokens back above it.
/// Token lists already read to their end are skipped: TeX drops them at the next read.
fn reading_file(input: &super::Input) -> Option<(&Mouth, bool)> {
    let mut lookahead = false;
    for frame in input.iter().rev() {
        match frame {
            Frame::Tokens { toks, pos, .. } if *pos >= toks.len() => {}
            Frame::Returned(toks) if toks.is_empty() => {}
            Frame::Returned(_) => lookahead = true,
            Frame::File { mouth, .. } => return Some((mouth, lookahead)),
            _ => return None,
        }
    }
    None
}

/// Each open file of `input` with its position and the text still to read.
fn files_at(input: &super::Input) -> Vec<(crate::engine::tex::FileId, (u32, u32), &[char])> {
    (input.iter())
        .filter_map(|f| match f {
            Frame::File { mouth, .. } => {
                Some((mouth.file, mouth.position(), &mouth.source()[mouth.offset().min(mouth.source().len())..]))
            }
            _ => None,
        })
        .collect()
}

/// Lines `lo..=hi` of a file.
type Window = (crate::engine::tex::FileId, u32, u32);

/// The lines of `edit` that differ between the file `new` reads on top and
/// the one `old` read, when every other open file is the same.
fn edited_lines(
    new: &super::Input,
    old: &super::Input,
    files: &[crate::analysis::machine::FileInfo],
    edit: &Path,
) -> Option<Window> {
    let mouths = |input: &super::Input| -> Vec<Rc<Mouth>> {
        (input.iter())
            .filter_map(|f| if let Frame::File { mouth, .. } = f { Some(mouth.clone()) } else { None })
            .collect()
    };
    let (new, old) = (mouths(new), mouths(old));
    let (top, below) = new.split_last()?;
    let (was, was_below) = old.split_last()?;
    let same = |a: &Rc<Mouth>, b: &Rc<Mouth>| Rc::ptr_eq(a.source(), b.source()) || a.source() == b.source();
    if Path::new(&files.get(usize::from(top.file))?.path) != edit
        || below.len() != was_below.len()
        || !below.iter().zip(was_below).all(|(a, b)| same(a, b))
    {
        return None;
    }
    let lines = |m: &Mouth| m.source().split(|&c| c == '\n').map(<[char]>::to_vec).collect::<Vec<_>>();
    let (now, then) = (lines(top), lines(was));
    if now.len() != then.len() {
        return None;
    }
    let differ: Vec<u32> = (0..now.len()).filter(|&i| now[i] != then[i]).map(|i| i as u32 + 1).collect();
    Some((top.file, *differ.first()?, *differ.last()?))
}

/// Whether `text`, a state written with [`std::fmt::Debug`], holds a place in `window`.
fn mentions(text: &str, (file, lo, hi): Window) -> bool {
    let number = |s: &str, key: &str| {
        s.strip_prefix(key).and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()?.parse::<u32>().ok())
    };
    text.split("Span { ").skip(1).any(|s| {
        let file_at = number(s, "file: ");
        let line = s.find("line: ").and_then(|i| number(&s[i..], "line: "));
        file_at == Some(u32::from(file)) && line.is_some_and(|l| (lo..=hi).contains(&l))
    })
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
fn rows<T: PartialEq + Clone>(new: &[T], at: &[T], end: &mut Vec<T>, edited: impl Fn(&T) -> bool) -> bool {
    if end.len() < at.len() {
        return false;
    }
    let mut out = Vec::with_capacity(new.len() + end.len() - at.len());
    for (i, was) in at.iter().enumerate() {
        let (mine, last) = (new.get(i), &end[i]);
        let rest_changed = last != was;
        match mine {
            Some(mine) if mine == was => out.push(last.clone()),
            Some(mine) if !rest_changed && edited(mine) && edited(was) => out.push(mine.clone()),
            None if !rest_changed && edited(was) => {}
            _ => return false,
        }
    }
    let added = new.get(at.len()..).unwrap_or_default();
    if !added.iter().all(&edited) {
        return false;
    }
    out.extend(added.iter().cloned());
    out.extend(end[at.len()..].iter().cloned());
    *end = out;
    true
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

/// Characters set alike in both runs, set at the meeting line, and the change in between.
type Move = (u32, u32, i64);

/// An offset before the resume stays, one set from the meeting line on moves,
/// one in between (set in the edited part) is not known.
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
use crate::analysis::facts::{Align, Just, Rule};
use crate::engine::tex::Span;
offset_first!(
    (u32, Span),
    (u32, Align),
    (u32, u16),
    (u32, crate::engine::value::Scaled),
    (u32, Just),
    (u32, crate::engine::env::NodeId, bool),
    (u32, bool, u32),
    (u32, Rule, Span, u32)
);

/// `end` becomes `new`'s list and the rest's additions, moved by `by` (a
/// longer word moves every later character). The rest may only add, and what
/// it reads of the list, its last entry against the characters set so far,
/// must stand alike in both runs.
fn offsets<T: Offsets>(new: &[T], at: &[T], end: &mut Vec<T>, (now, then, resume): (u32, u32, u32)) -> bool {
    let rel = |l: &[T], len: u32| l.last().map(|x| i64::from(x.first()) - i64::from(len));
    let reads_alike = match (rel(new, now), rel(at, then)) {
        (Some(x), Some(y)) => x == y || (x < 0 && y < 0),
        (x, y) => x == y,
    };
    let by = i64::from(now) - i64::from(then);
    let Some(rest) = end.get(at.len()..).filter(|_| reads_alike && end[..at.len()] == *at) else { return false };
    let Some(rest) = rest.iter().map(|x| x.moved((resume, then, by))).collect::<Option<Vec<T>>>() else { return false };
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
    // Characters both runs set alike before the edited ones keep their offsets.
    let from = (resume as usize).min(k).min(new.at.len());
    let resume = (from + new.at[from..].iter().zip(&at.at[from..k]).take_while(|(a, b)| a == b).count()) as u32;
    let lens = (new.at.len() as u32, k as u32, resume);
    let by = (resume, lens.1, i64::from(lens.0) - i64::from(lens.1));
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
        pages,
        fills,
        open_pages: _,
        frames,
    } = end;
    let per_char = spots.len() >= k && site.len() >= k && spots[..k] == at.at[..] && site[..k] == at.site[..];
    if !per_char || new.open != at.open || new.open_pages != at.open_pages {
        return Err("text");
    }
    *chars = new.chars.chars().chain(chars.chars().skip(k)).collect();
    *spots = new.at.iter().chain(&spots[k..]).copied().collect();
    *site = new.site.iter().chain(&site[k..]).copied().collect();
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
    ];
    lists.into_iter().find(|(ok, _)| !ok).map_or(Ok(by), |(_, what)| Err(what))
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
fn splice_out(new: &Analysis, at: &Analysis, end: &mut Analysis, window: Window, resume: u32) -> Result<(), &'static str> {
    let span = |s: &Span| s.file == window.0 && (window.1..=window.2).contains(&s.line);
    let line = |k: &(crate::engine::tex::FileId, u32)| k.0 == window.0 && (window.1..=window.2).contains(&k.1);
    // Kept as the run's end has them: its clock and the caches filled after it.
    let Analysis { interner, graph, calls, observed, files, main_file, literate, project, plugins, distribution,
        document_depth, preamble_cost, justified_text, entry, format, config, settings, metadata, kernel_sites, fonts,
        font_states, env, catcodes, facts, trace, typeset, steps, load_steps, served_steps, limits, lines, exhausted,
        journal_size, held_tokens_left, loop_heads, peak_paths, peak_output_paths, forks, output_forks, call_memo,
        joined_slots, memory_reported, untyped, package_caches, written, written_runs, build_runs, class_caches,
        cached_files, preamble, kernel, timings: _, started: _, hot: _, probes: _, context_probes: _, takes: _,
        role_sources: _, probed_roles: _, bib: _, scope_rows: _, said: _ } = end;
    let crate::analysis::facts::Facts { defs, written_files, absent, found, shapes, types, length_fonts, vars,
        letters, expansions, loads, occurrences, literals, diagnostics, csnames, slots, groups, raised, spaces,
        kept_spaces, conditionals, quantities, colors, items, color_uses, color_stack, text, overfull,
        lines: line_reads } = facts;
    let (n, a) = (&new.facts, &at.facts);
    // What this run recorded must be what the last one had here: the rest then leaves the same.
    let alike = [
        (new.interner.len() == at.interner.len() && new.interner == at.interner, "names"),
        (new.graph.len() == at.graph.len() && new.graph == at.graph, "graph"),
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
        (new.env.slots.len() == at.env.slots.len() && new.preamble.is_some() == at.preamble.is_some()
            && new.kernel.is_some() == at.kernel.is_some() && same(&new.settings, &at.settings), "settings"),
    ];
    if let Some((_, what)) = alike.into_iter().find(|(ok, _)| !ok) {
        return Err(what);
    }
    let by = splice_text(&n.text, &a.text, text, resume)?;
    // A literal holds the characters set before it. The rest's move with them; the last
    // run's before the meeting line are compared as this run would have set them.
    let moved = |c: u32| if c < by.0 { c } else { u32::try_from(i64::from(c) + by.2).unwrap_or(u32::MAX) };
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
        (rows(&n.occurrences, &a.occurrences, occurrences, |r| s(&r.span)), "occurrences"),
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

/// Whether `\begin{document}` is on the current line or the next one. A lookahead
/// (`\usepackage`'s `\@ifnextchar`) may read past the start of the next line.
fn opens_document(mouth: &Mouth) -> bool {
    let source = mouth.source();
    let line_start = source[..mouth.offset().min(source.len())].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    let mut breaks = 0;
    let two_lines: String = source[line_start..]
        .iter()
        .copied()
        .take_while(|&c| {
            breaks += usize::from(c == '\n');
            breaks < 2
        })
        .collect();
    two_lines.contains("\\begin{document}")
}

/// Callback for each run of a build that is followed by another run.
pub type BetweenRuns = Box<dyn Fn(&Analysis)>;

/// The checkpoints of a document's last run, and the inputs they depend on.
pub struct Checkpoints<'a> {
    cfg: &'a Config,
    /// The document path and a hash of what the run read before its first line.
    setup: Option<(PathBuf, u64)>,
    list: Vec<Checkpoint<'a>>,
    /// The files the run read or looked for, in order.
    log: Vec<crate::project::overlay::Read>,
    /// The document's text at the last run, compared to find the edited line.
    source: String,
    every: u64,
    /// The line the last run resumed at, if it resumed.
    pub resumed: Option<(String, u32)>,
    /// The first line after the edit where the last run's state equaled the
    /// run before's, with the steps that run took from there to its end.
    pub converged: Option<(String, u32, u64)>,
    /// Steps of the last run.
    steps: u64,
    /// The last run's machine at its end, before concluding.
    end: Option<Box<Machine<'a>>>,
    /// When the build's first run started. `limits.seconds` counts from here for every run.
    pub build_started: Option<std::time::Instant>,
    /// Called after each run of a build that is followed by another run.
    pub between_runs: Option<BetweenRuns>,
}

impl<'a> Checkpoints<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        Checkpoints {
            cfg,
            setup: None,
            list: Vec::new(),
            log: Vec::new(),
            source: String::new(),
            every: EVERY,
            resumed: None,
            converged: None,
            steps: 0,
            end: None,
            build_started: None,
            between_runs: None,
        }
    }

    /// The files the last run read or looked for.
    pub fn looked(&self) -> Vec<PathBuf> {
        self.log.iter().map(|r| r.path.clone()).collect()
    }

    /// Use `steps` instead of [`EVERY`] as the initial spacing.
    pub fn every(self, steps: u64) -> Self {
        Checkpoints { every: steps.max(1), ..self }
    }

    /// Analyzes `source`, the document at `path`. Resumes from the last
    /// checkpoint whose input is unchanged, else starts from the beginning.
    /// The result is the same as a full run either way.
    pub fn analyze(&mut self, source: &str, path: &Path) -> Analysis {
        let (_run, _entered) = crate::analysis::budget::Run::start();
        let setup = Some((path.to_path_buf(), setup_key(self.cfg, source, path)));
        if setup != self.setup {
            (self.list, self.log) = (Vec::new(), Vec::new());
        }
        self.setup = setup;
        (self.resumed, self.converged) = (None, None);
        let clock = std::time::Instant::now();
        let log = self.log.clone();
        let (resumed, edit, old) = self.resume(source);
        let resumed_in = clock.elapsed();
        // The first changed line of the document is the best guess for the edit.
        let changed = first_changed_line(&std::mem::replace(&mut self.source, source.to_string()), source);
        crate::project::overlay::log(true);
        let (mut m, id) = match resumed {
            Some(resumed) => resumed,
            None => {
                self.log.clear();
                let (m, id) = Machine::begin(source, Some(path), self.cfg);
                // A literate source is read through docstrip's view.
                if m.out.literate.is_some() {
                    crate::project::overlay::log(false);
                    let mut m = m;
                    m.run_guarded();
                    return m.conclude(id, source);
                }
                (m, id)
            }
        };
        m.started = *self.build_started.get_or_insert(m.started);
        let edit = changed.map(|line| (PathBuf::from(&m.out.files[usize::from(id)].path), line)).or(edit);
        let next = m.out.steps + self.every;
        let edited = edit.as_ref().map(|(path, _)| path.clone());
        let taking = Taking {
            list: std::mem::take(&mut self.list),
            log: std::mem::take(&mut self.log),
            every: self.every,
            next,
            last: m.out.steps,
            pinned: false,
            toward: u32::MAX,
            document: m.out.document_depth.is_some(),
            edit,
            prior: Prior { stale: old, end: self.end.take(), log, steps: self.steps, edit: edited },
            converged: None,
            spliced: None,
            tail: Vec::new(),
            since: (m.env.assignments, m.env.journal_len(), m.out.facts.text.at.len() as u32),
        };
        m.checkpoints.on = Some(Box::new(taking));
        let (clock, from) = (std::time::Instant::now(), m.out.steps);
        m.run_guarded();
        let ran = (clock.elapsed(), m.out.steps - from);
        // Reads after the last checkpoint. No resume depends on them, but the build's reruns do.
        let after = crate::project::overlay::log(false);
        if let Some(taking) = m.checkpoints.on.take() {
            let mut taking = *taking;
            (self.list, self.log, self.every) = (taking.list, taking.log, taking.every);
            self.converged = taking.converged;
            match taking.spliced.take() {
                Some(end) => {
                    m = *end;
                    self.log.extend(taking.tail);
                }
                None => self.log.extend(after),
            }
        }
        self.steps = m.out.steps;
        self.end = Some(Box::new(m.clone()));
        let (clock, verbose) = (std::time::Instant::now(), self.cfg.verbose >= 2);
        let analysis = m.conclude(id, source);
        if verbose {
            crate::util::progress::note!(
                "satex: resume {:.2}s, run {:.2}s for {} steps, conclude {:.2}s",
                resumed_in.as_secs_f64(),
                ran.0.as_secs_f64(),
                ran.1,
                clock.elapsed().as_secs_f64()
            );
        }
        analysis
    }

    /// The machine of the last checkpoint that is still valid, set up to read
    /// the files as they are now, with the main file's id. Also returns where
    /// a file open in a later, invalid checkpoint first changed.
    #[allow(clippy::type_complexity)]
    fn resume(
        &mut self,
        source: &str,
    ) -> (Option<(Machine<'a>, crate::engine::tex::FileId)>, Option<Edit>, Vec<Checkpoint<'a>>) {
        let (mut holds, mut edit, mut old) = (HashMap::default(), None, Vec::new());
        while let Some(checkpoint) = self.list.pop() {
            let (mut m, mut drive) = (checkpoint.machine.clone(), checkpoint.drive.clone());
            if let Err(changed) = reread(&mut m, &mut drive, &self.log[..checkpoint.reads], source, &mut holds) {
                if self.cfg.verbose >= 2 {
                    let at = reading_file(&m.input).map(|(m, _)| m.position());
                    crate::util::progress::note!("satex: checkpoint at {at:?} does not hold: {changed:?}");
                }
                // Keep the furthest changed line any invalid checkpoint found.
                edit = edit.into_iter().chain(changed).max_by_key(|(_, line)| *line);
                old.push(checkpoint);
                continue;
            }
            // `limits.seconds` bounds the whole build, so a resumed run gets only
            // the time the earlier runs left. The limit only stops a run, it never changes its result.
            m.started = self.build_started.unwrap_or_else(std::time::Instant::now);
            m.checkpoints.resume = Some(Box::new(drive));
            crate::engine::source::set_encoding(m.out.plugins.engine.encoding());
            if let Some((mouth, _)) = reading_file(&m.input) {
                if self.cfg.verbose >= 2 {
                    crate::util::progress::note!("satex: resuming at {:?}, step {}", mouth.position(), m.out.steps);
                }
                let at = mouth.position().0;
                self.resumed = Some((m.out.files[usize::from(mouth.file)].path.clone(), at));
            }
            self.log.truncate(checkpoint.reads);
            let id = m.out.main_file;
            self.list.push(checkpoint);
            old.reverse();
            return (Some((m, id)), edit, old);
        }
        old.reverse();
        (None, edit, old)
    }
}

/// Points every path of `m` and `drive` at the current file contents.
/// Succeeds if each file in `log` is unchanged and each open file still starts
/// with what was read of it. Otherwise returns where a file changed, if known.
fn reread(
    m: &mut Machine,
    drive: &mut Drive,
    log: &[crate::project::overlay::Read],
    source: &str,
    holds: &mut HashMap<(PathBuf, bool), bool>,
) -> Result<(), Option<Edit>> {
    let paths: Vec<PathBuf> = m.out.files.iter().map(|f| PathBuf::from(&f.path)).collect();
    let main = m.out.main_file;
    let mut inputs: Vec<(&mut super::Input, bool)> =
        std::iter::once((&mut m.input, true)).chain(drive.inputs()).collect();
    let files = |input: &super::Input| -> Vec<crate::engine::tex::FileId> {
        (input.iter())
            .filter_map(|f| match f {
                Frame::File { mouth, .. } => Some(mouth.file),
                _ => None,
            })
            .collect()
    };
    // Files that every live path still has open are checked by prefix below.
    // Every other file was read to its end by some path, so it must be unchanged.
    let mut live = inputs.iter().filter(|(_, live)| *live).map(|(i, _)| files(i));
    let mut open = live.next().unwrap_or_default();
    for other in live {
        open.retain(|f| other.contains(f));
    }
    // The main file is not in the log. If no path has it open, it was read to its end.
    if !open.contains(&main) {
        return Err(None);
    }
    // Only files opened once count as open. A file opened again was read to its end before.
    let reads = |path: &PathBuf| log.iter().filter(|r| !r.probe && r.path == *path).count();
    let open: Vec<&PathBuf> = open.into_iter().map(|f| &paths[usize::from(f)]).filter(|p| reads(p) <= 1).collect();
    for read in log.iter().filter(|r| r.probe || !open.contains(&&r.path)) {
        if !*holds.entry((read.path.clone(), read.probe)).or_insert_with(|| read.holds()) {
            // Changed: the next run takes a checkpoint at the file's start.
            return Err((!read.probe).then(|| (read.path.clone(), 1)));
        }
    }
    let mut fresh: HashMap<crate::engine::tex::FileId, Option<Mouth>> = HashMap::default();
    for frame in inputs.iter_mut().flat_map(|(i, _)| i.0.iter_mut()) {
        let Frame::File { mouth, .. } = &**frame else { continue };
        let file = mouth.file;
        let now = fresh.entry(file).or_insert_with(|| {
            let path = &paths[usize::from(file)];
            let text = match file == main {
                true => crate::engine::source::recode(source).into_owned(),
                // A literate source is read through docstrip's view.
                false if crate::project::literate::Literate::of(path).is_some() => return None,
                false => crate::engine::source::decode_counted(&crate::project::overlay::read_bytes(path).ok()?).0,
            };
            // A file with several alternative texts: which one a path read is
            // its own choice, so it rereads the file from the start.
            (super::texts_of(&text).len() < 2).then(|| Mouth::file(&text, file))
        });
        let now = now.as_ref().ok_or_else(|| Some((paths[usize::from(file)].clone(), 1)))?;
        let read = mouth.retext(now).map_err(|line| Some((paths[usize::from(file)].clone(), line)))?;
        if let Frame::File { mouth, .. } = Rc::make_mut(frame) {
            *mouth = Rc::new(read);
        }
    }
    Ok(())
}

/// The first line `now` differs from `was` on, `None` when they are the same.
fn first_changed_line(was: &str, now: &str) -> Option<u32> {
    if was.is_empty() || was == now {
        return None;
    }
    let same = was.split('\n').zip(now.split('\n')).take_while(|(a, b)| a == b).count();
    Some(same as u32 + 1)
}

/// A hash of what a run reads before the document's first line, besides
/// the configuration: the project around it, magic comments, the requested
/// kernel, and a `%&` format line.
pub(crate) fn setup_key(cfg: &Config, source: &str, path: &Path) -> u64 {
    let base = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| std::env::current_dir().unwrap_or_default(), Path::to_path_buf);
    let mut hasher = std::hash::DefaultHasher::new();
    format!("{:?}", crate::project::build_files::Project::discover(&base)).hash(&mut hasher);
    if cfg.plugins.discovery {
        format!("{:?}", crate::plugins::discovery::scan_with(&base, cfg.limits.thread_count())).hash(&mut hasher);
    }
    format!("{:?}", crate::plugins::magic::scan_with(source, &cfg.magic)).hash(&mut hasher);
    format!("{:?}", crate::plugins::Kernel::detect(source)).hash(&mut hasher);
    format!("{:?}", crate::project::config::Profile::of_name(Some(path), source)).hash(&mut hasher);
    source.lines().next().hash(&mut hasher);
    hasher.finish()
}
