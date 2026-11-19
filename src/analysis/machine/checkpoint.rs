//! Checkpoints make re-analysis after an edit fast. While a run reads the
//! user's files, it saves its whole state from time to time. The next run of
//! the same document resumes from the last checkpoint whose input has not
//! changed. That gives the same result as a full run.

use super::explore::Drive;
use super::provisional;
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
/// After the edit's line, one checkpoint at the first top-level line past each of this many
/// pages shipped: the page holding the edited text is where the next edit's run may meet this one.
const AFTER_EDIT: u32 = 2;

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
    /// Where the document begins, or just before: every body edit and every rerun after the
    /// `.aux` changed resumes here at the latest. Never dropped. Copied early, so small.
    anchor: bool,
    /// What copying the state allocated: the parts it does not share with the run.
    bytes: usize,
}

/// The checkpoints a run takes, and the drive it resumes with.
#[derive(Default)]
pub(super) struct Taker<'a> {
    pub(super) on: Option<Box<Taking<'a>>>,
    resume: Option<Box<Drive>>,
    /// A paragraph preview's end: the file, its last line, and whether the run has read the file yet.
    stop: Option<(crate::engine::tex::FileId, u32, bool)>,
}

/// What a paragraph preview hands on: the run up to the edited paragraph's end, the
/// paragraph's file and its first and last line.
pub type Preview = Box<dyn FnOnce(Analysis, &str, u32, u32)>;

/// What a provisional run hands on: the whole document, the last run's rest spliced in
/// where the states first agree apart from the page being built.
pub type Lax = Box<dyn FnOnce(Analysis)>;

/// What the next resumed runs hand on early, besides their result.
#[derive(Default)]
pub struct RunHooks {
    /// For an edit inside a paragraph: run that paragraph first, from the same checkpoint,
    /// and hand it on. The paragraph is broken into lines from the state at its start and
    /// its own text, so its rows are those of the full run. Taken by the next run.
    pub preview: Option<Preview>,
    /// A provisional whole analysis, as soon as the state equals the last run's apart from
    /// the page being built (the lax page model of the Typeset view): every row after is the
    /// last run's, its pages marked not exact. The run itself goes on exact. Kept until it fires.
    pub provisional: Option<Lax>,
}

/// Lines `lo..=hi` of the paragraph `text` holds at `line`: up to the blank lines around it.
/// `None` on a blank line.
fn paragraph_at(text: &str, line: u32) -> Option<(u32, u32)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let blank = |i: usize| lines.get(i).is_none_or(|l| l.trim().is_empty());
    let at = (line as usize).checked_sub(1).filter(|&i| !blank(i))?;
    let lo = (0..at).rev().find(|&i| blank(i)).map_or(0, |i| i + 1);
    let hi = (at..lines.len()).find(|&i| blank(i)).unwrap_or(lines.len());
    Some((lo as u32 + 1, hi as u32))
}

pub(super) struct Taking<'a> {
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
    /// Past the edit's line: the pages shipped at the last checkpoint, and how many more to take ([`AFTER_EDIT`]).
    after: Option<(u64, u32)>,
    /// File and line where the last edit began. The next edit is likely
    /// nearby, so a checkpoint is taken there.
    edit: Option<Edit>,
    /// The last run, whose state this one may meet again after the edit.
    pub(super) prior: Prior<'a>,
    /// Assignments and saves made when this run resumed: the runs differ only in what came after.
    pub(super) since: (u64, usize, u32),
    /// Where this run's state first equals the last run's (see [`Checkpoints::converged`]).
    converged: Option<(String, u32, u64)>,
    /// The last run's end with this run's facts before the meeting line: this run's end.
    spliced: Option<Box<Machine<'a>>>,
    /// What the last run read after the meeting line.
    tail: Vec<crate::project::overlay::Read>,
    /// The main file, its text and [`RunHooks::provisional`], until it fires.
    pub(super) provisional: Option<(crate::engine::tex::FileId, Rc<str>, Lax)>,
}

/// What a run keeps of the one before to meet its state again.
#[derive(Default)]
pub(super) struct Prior<'a> {
    /// Its checkpoints that no longer hold, in run order.
    stale: Vec<Checkpoint<'a>>,
    /// Its machine at its end, before concluding.
    pub(super) end: Option<Box<Machine<'a>>>,
    /// Every file it read, in order.
    log: Vec<crate::project::overlay::Read>,
    steps: u64,
    /// The file edited since.
    pub(super) edit: Option<PathBuf>,
    /// Its edited lines, the old text from a stale checkpoint that had it open: where the
    /// states meet after the file was read to its end ([`edited_lines`] needs it open).
    pub(super) window: Option<Window>,
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
        let document = self.out.document_depth.is_some();
        // The body's first checkpoint: `\begin{document}` ends with `\ignorespaces`, which reads
        // on into the body, so it is taken with that one token left. Once `\document` has let
        // `\@nodocument` go (latex.ltx), not while `\document` itself waits.
        let hooked = || {
            let name = self.out.interner.lookup("@nodocument");
            !name.is_some_and(|n| matches!(self.env.observed_meaning(n), crate::engine::tex::Meaning::Macro(_)))
        };
        let body = document && !taking.document && hooked();
        let Some((mouth, lookahead)) = reading_file_after(&self.input, usize::from(body)) else { return false };
        let gap = self.edit_gap(taking, mouth);
        let edit = gap == Some(0);
        // Before `\begin{document}`, which reads the `.aux` that a rerun may change.
        let opens = !document && opens_document(mouth);
        let since = self.out.steps - taking.last;
        // Approaching the edit's line, take one each time the distance halves.
        // The next edit is likely there again and resumes close to it.
        let toward = gap.is_some_and(|g| g > 0 && g.saturating_mul(2) <= taking.toward);
        // Top-level lines of the body (a frame, a paragraph), so an edit there resumes close by.
        let level = || {
            self.out.document_depth.is_some_and(|d| usize::from(d) == self.env.group_level()) && !self.env.many_groups()
        };
        let top = || since >= taking.every / BODY_SHARE && level();
        // A page shipped since the edit's line, or since the last checkpoint after it.
        let shipped = || taking.after.is_some_and(|(pages, _)| self.pages > pages) && level();
        let due = self.out.steps >= taking.next
            || document != taking.document
            || edit
            || opens
            || toward
            || top()
            || shipped();
        // Not while a path through an undecided conditional runs: resuming there would number its meanings anew.
        if self.halted || !due || self.env.forking() {
            return false;
        }
        // A lookahead in the preamble (`\@ifnextchar[`) may hold the next line's first token.
        let settled = match lookahead {
            true => opens || body,
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

    /// Whether a paragraph preview is done: past its last line, back in vertical mode, or out of its file.
    pub(super) fn preview_done(&mut self) -> bool {
        let Some((file, hi, seen)) = self.checkpoints.stop else { return false };
        let open = self.input.iter().any(|f| matches!(f, Frame::File { mouth, .. } if mouth.file == file));
        let past = reading_file(&self.input).is_some_and(|(mouth, _)| {
            mouth.file == file && mouth.position().0 > hi && self.mode == crate::engine::mode::Modes::VERTICAL
        });
        self.checkpoints.stop = Some((file, hi, seen || open));
        (seen && !open) || past
    }

    /// Saves the run's state and `drive` as a checkpoint.
    pub(super) fn take_checkpoint(&mut self, drive: Drive) {
        let before = crate::analysis::budget::in_use();
        let machine = self.clone();
        let bytes = crate::analysis::budget::in_use().saturating_sub(before);
        let budget = self.cfg.limits.checkpoint_memory.as_u64() as usize;
        let (steps, document) = (self.out.steps, self.out.document_depth.is_some());
        let opens = reading_file_after(&self.input, 1).is_some_and(|(mouth, _)| !document && opens_document(mouth));
        let gap = match (&self.checkpoints.on, reading_file_after(&self.input, 1)) {
            (Some(taking), Some((mouth, _))) => self.edit_gap(taking, mouth),
            _ => None,
        };
        let Some(taking) = self.checkpoints.on.as_mut() else { return };
        taking.log.extend(crate::project::overlay::log(true));
        let (toward, mut pinned) = (gap.is_some_and(|g| g > 0), gap.is_some());
        // The pages after the edit's line are kept as the edit's own checkpoints.
        if let Some((pages, left)) = taking.after.filter(|&(pages, _)| self.pages > pages) {
            taking.after = (left > 1).then_some((self.pages.max(pages), left - 1));
            pinned = true;
        }
        // On the way to the edit's line or at it: the pages from here on count ([`AFTER_EDIT`]).
        if gap.is_some() {
            taking.after = Some((self.pages, AFTER_EDIT));
        }
        match gap {
            Some(0) => taking.edit = None,
            Some(g) => taking.toward = g,
            None => {}
        }
        if self.cfg.verbose >= 2 {
            crate::util::progress::note!(
                "satex: checkpoint at {:?}, step {steps}",
                reading_file_after(&self.input, 1).map(|(m, _)| m.position())
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
        let anchor = opens || document != taking.document;
        taking.list.push(Checkpoint { machine, drive, reads: taking.log.len(), pinned, toward, anchor, bytes });
        // Each copy holds what grows along the document (graph, facts, sites)
        // anew, so their bytes are bounded too, not only their number.
        let over = |list: &[Checkpoint]| budget > 0 && held(list) > budget;
        let kept = |c: &Checkpoint| c.pinned || c.anchor;
        while taking.list.len() > KEPT || (over(&taking.list) && taking.list.iter().filter(|c| !kept(c)).count() > 1) {
            // Drop every other one. The last, the pinned and the anchors stay.
            // Counted among the ones that may go, so the kept ones never shield their neighbours.
            let len = taking.list.len();
            let mut n = taking.list.iter().filter(|c| !kept(c)).count();
            taking.list.retain(|c| {
                n -= usize::from(!kept(c));
                kept(c) || n % 2 == 0
            });
            taking.every *= 2;
            if taking.list.len() == len {
                break;
            }
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
        let differs = self.differs_from(&old, false).or_else(|| self.splice(&old, reads).err());
        if matches!(differs, Some("page so far" | "mode or pages" | "path state")) {
            self.lax_splice(&old);
        }
        if self.cfg.verbose >= 2 {
            crate::util::progress::note!(
                "satex: state at {path}:{line} vs the last run's: {}",
                differs.unwrap_or("alike")
            );
        }
        if self.cfg.verbose >= 2 && differs == Some("bindings") {
            let (a, b) = (&self.env.slots, &old.env.slots);
            let names: Vec<String> = (0..a.len().min(b.len()))
                .filter(|&i| !crate::engine::env::same_slot(&a[i], &b[i]))
                .take(12)
                .map(|i| self.out.interner.name(crate::engine::tex::Sym(i as u32)).to_string())
                .collect();
            crate::util::progress::note!("satex:   bindings that differ: {names:?}");
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
        let window = prior.window;
        let window = edited_lines(&self.input, &old.input, &self.out.files, &edit, window).ok_or("edit elsewhere")?;
        // Between paragraphs: one going on would be broken into lines with the edited text.
        if self.mode != crate::engine::mode::Modes::VERTICAL {
            return Err("inside a paragraph");
        }
        if (&self.diag_seen, &self.diag_raised, &self.diag_live) != (&old.diag_seen, &old.diag_raised, &old.diag_live) {
            return Err("diagnostics raised");
        }
        // Counts of tokens taken and assignments made differ by the edit's. They are compared
        // only with marks taken since, which differ alike, or before the edit, which both passed.
        let (mine, theirs) = (provisional::comparable_path(self, false), provisional::comparable_path(old, false));
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
        provisional::splice_out(&self.out, &old.out, &mut end.out, window, since.2, false)?;
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
    pub(super) fn differs_from(&self, old: &Machine, lax: bool) -> Option<&'static str> {
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
            ((self.mode, self.list) != (old.mode, old.list) || !lax && self.pages != old.pages, "mode or pages"),
            (!lax && self.natural != old.natural, "page so far"),
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

/// What copying `list` allocated, without its anchors (kept whatever they cost).
fn held(list: &[Checkpoint]) -> usize {
    list.iter().filter(|c| !c.anchor).map(|c| c.bytes).sum()
}

/// The file being read and whether a lookahead put tokens back above it.
/// Token lists already read to their end are skipped: TeX drops them at the next read.
fn reading_file(input: &super::Input) -> Option<(&Mouth, bool)> {
    reading_file_after(input, 0)
}

/// [`reading_file`], where token lists with at most `tail` tokens left count as a lookahead.
fn reading_file_after(input: &super::Input, tail: usize) -> Option<(&Mouth, bool)> {
    let mut lookahead = false;
    for frame in input.iter().rev() {
        match frame {
            Frame::Tokens { toks, pos, .. } if *pos >= toks.len() => {}
            Frame::Tokens { toks, pos, .. } if toks.len() - pos <= tail => lookahead = true,
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
pub(super) type Window = (crate::engine::tex::FileId, u32, u32);

/// The lines of `edit` that differ between the file `new` reads on top and
/// the one `old` read, when every other open file is the same. With `edit` open in
/// neither, its lines `closed` that differ, when every open file is the same.
pub(super) fn edited_lines(
    new: &super::Input,
    old: &super::Input,
    files: &[crate::analysis::machine::FileInfo],
    edit: &Path,
    closed: Option<Window>,
) -> Option<Window> {
    let mouths = |input: &super::Input| -> Vec<Rc<Mouth>> {
        (input.iter())
            .filter_map(|f| if let Frame::File { mouth, .. } = f { Some(mouth.clone()) } else { None })
            .collect()
    };
    let (new, old) = (mouths(new), mouths(old));
    let same = |a: &Rc<Mouth>, b: &Rc<Mouth>| Rc::ptr_eq(a.source(), b.source()) || a.source() == b.source();
    let edited = |m: &Rc<Mouth>| files.get(usize::from(m.file)).is_some_and(|f| Path::new(&f.path) == edit);
    if !new.iter().chain(&old).any(edited) {
        let alike = new.len() == old.len() && new.iter().zip(&old).all(|(a, b)| a.file == b.file && same(a, b));
        return closed.filter(|_| alike);
    }
    let (top, below) = new.split_last()?;
    let (was, was_below) = old.split_last()?;
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
pub(super) fn mentions(text: &str, (file, lo, hi): Window) -> bool {
    let number = |s: &str, key: &str| {
        s.strip_prefix(key).and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()?.parse::<u32>().ok())
    };
    text.split("Span { ").skip(1).any(|s| {
        let file_at = number(s, "file: ");
        let line = s.find("line: ").and_then(|i| number(&s[i..], "line: "));
        file_at == Some(u32::from(file)) && line.is_some_and(|l| (lo..=hi).contains(&l))
    })
}

/// Whether `\begin{document}` is on the current line or the next one. A lookahead
/// (`\usepackage`'s `\@ifnextchar`) may read past the start of the next line.
fn opens_document(mouth: &Mouth) -> bool {
    let source = mouth.source();
    let line_start = source[..mouth.offset().min(source.len())].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    // Asked before every command outside the body: the answer per line is kept.
    // The cache holds the text, so no other text can reuse its address.
    /// The text, a line start and whether `\\begin{document}` is near it.
    type Opens = Option<(std::rc::Rc<[char]>, usize, bool)>;
    thread_local! {
        static LAST: std::cell::RefCell<Opens> = const { std::cell::RefCell::new(None) };
    }
    if let Some(opens) = LAST.with_borrow(|last| {
        last.as_ref().filter(|(text, at, _)| std::rc::Rc::ptr_eq(text, source) && *at == line_start).map(|l| l.2)
    }) {
        return opens;
    }
    let rest = &source[line_start..];
    let end = rest.iter().enumerate().filter(|&(_, &c)| c == '\n').nth(1).map_or(rest.len(), |(i, _)| i);
    const WANTED: [char; 16] = ['\\', 'b', 'e', 'g', 'i', 'n', '{', 'd', 'o', 'c', 'u', 'm', 'e', 'n', 't', '}'];
    let opens = rest[..end].windows(WANTED.len()).any(|w| w == WANTED);
    LAST.set(Some((source.clone(), line_start, opens)));
    opens
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
    /// Steps the last run took itself (from its resume point, or all of them), and all its steps.
    pub ran: (u64, u64),
    /// The last run's machine at its end, before concluding.
    end: Option<Box<Machine<'a>>>,
    /// When the build's first run started. `limits.seconds` counts from here for every run.
    pub build_started: Option<std::time::Instant>,
    /// Called after each run of a build that is followed by another run.
    pub between_runs: Option<BetweenRuns>,
    /// A build an earlier process left between two runs: the next build goes on from it.
    pub pending: Option<super::Unfinished>,
    /// What the next runs hand on early.
    pub hooks: RunHooks,
}

impl<'a> Checkpoints<'a> {
    /// Files outside the documents landed (a package bundle). A checkpoint holds while every
    /// name its run looked up resolves as before: the first that resolves otherwise goes, and
    /// every later one with it. What a run read is checked at its resume ([`reread`]).
    pub fn outside_changed(&mut self) {
        self.end = None;
        let Some(mut now) = self.list.first().map(|c| c.machine.resolver.fresh()) else { return };
        let mut holds = |c: &mut Checkpoint<'a>| {
            let base = c.machine.base.clone();
            c.machine.resolver.renew(&mut now, &base)
        };
        let kept = self.list.iter_mut().position(|c| !holds(c)).unwrap_or(self.list.len());
        self.list.truncate(kept);
    }

    /// A trace of lines (`cfg.trace_lines`) run from the last checkpoint before them, under
    /// `cfg`, instead of from the start: the run before the lines is the last run's. `None`
    /// where no checkpoint is before them or none holds for `source`.
    /// `whole`: to the run's end, else only past the lines (the trace alone is asked).
    pub fn trace_from<'b>(&self, source: &str, cfg: &'b Config, whole: bool) -> Option<Analysis>
    where
        'a: 'b,
    {
        let (first, last) = cfg.trace_lines?;
        let traced = |m: &Machine<'a>, f: crate::engine::tex::FileId| match &cfg.trace_file {
            Some(name) => {
                m.out.files.get(usize::from(f)).is_some_and(|f| crate::project::config::names_file(&f.path, name))
            }
            None => f == m.out.main_file,
        };
        // before the lines: the file is open short of them, or not opened yet
        let before = |c: &&Checkpoint<'a>| {
            let open = c.machine.input.iter().find_map(|f| match f {
                Frame::File { mouth, .. } if traced(&c.machine, mouth.file) => Some(mouth.position().0),
                _ => None,
            });
            let opened = (0..c.machine.out.files.len()).any(|f| traced(&c.machine, f as crate::engine::tex::FileId));
            open.map_or(!opened, |line| line < first)
        };
        let c = self.list.iter().rev().find(before)?;
        let at =
            reading_file_after(&c.machine.input, 1).map(|(m, _)| (c.machine.out.short_name(m.file), m.position().0));
        crate::util::progress::note!("satex: trace from the checkpoint at {at:?}");
        let (mut input, mut drive) = (c.machine.input.clone(), c.drive.clone());
        reread(&c.machine.out, &mut input, &mut drive, &self.log[..c.reads], source, &mut HashMap::default()).ok()?;
        let mut m: Machine<'b> = c.machine.clone();
        (m.input, m.cfg, m.started) = (input, std::borrow::Cow::Borrowed(cfg), std::time::Instant::now());
        m.checkpoints.resume = Some(Box::new(drive));
        crate::engine::source::set_encoding(m.out.plugins.engine.encoding());
        let file = (0..m.out.files.len()).map(|f| f as crate::engine::tex::FileId).find(|&f| traced(&c.machine, f));
        m.checkpoints.stop = file.filter(|_| !whole).map(|f| (f, last, false));
        m.run_guarded();
        m.checkpoints.stop = None;
        let id = m.out.main_file;
        Some(m.conclude(id, source))
    }

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
            ran: (0, 0),
            end: None,
            build_started: None,
            between_runs: None,
            pending: None,
            hooks: RunHooks::default(),
        }
    }

    /// The files the last run read or looked for.
    pub fn looked(&self) -> Vec<PathBuf> {
        self.log.iter().map(|r| r.path.clone()).collect()
    }

    /// How many checkpoints the last run kept, and what copying them allocated.
    /// Without the anchors, which stay outside the budget.
    pub fn kept(&self) -> (usize, usize) {
        (self.list.iter().filter(|c| !c.anchor).count(), held(&self.list))
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
        // Nothing the last run read changed (another file opened in the editor): its end is this run's.
        if let Some(end) = self.end.as_ref().filter(|e| !e.out.exhausted && self.source == source)
            && self.log.iter().all(crate::project::overlay::Read::holds)
        {
            let m = (**end).clone();
            let id = m.out.main_file;
            self.ran = (0, m.out.steps);
            return m.conclude(id, source);
        }
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
        if let (Some(then), Some((path, line))) = (self.hooks.preview.take(), &edit) {
            self.preview(&m, id, source, path, *line, then);
        }
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
            after: None,
            edit,
            prior: Prior {
                window: edited.as_ref().and_then(|p| closed_window(&old, p, source, id)),
                stale: old,
                end: self.end.take(),
                log,
                steps: self.steps,
                edit: edited,
            },
            converged: None,
            spliced: None,
            tail: Vec::new(),
            since: (m.env.assignments, m.env.journal_len(), m.out.facts.text.at.len() as u32),
            provisional: self.hooks.provisional.take().map(|then| (id, Rc::from(source), then)),
        };
        m.checkpoints.on = Some(Box::new(taking));
        let (clock, from) = (std::time::Instant::now(), m.out.steps);
        m.run_guarded();
        let ran = (clock.elapsed(), m.out.steps - from);
        self.ran = (ran.1, m.out.steps);
        // Reads after the last checkpoint. No resume depends on them, but the build's reruns do.
        let after = crate::project::overlay::log(false);
        if let Some(taking) = m.checkpoints.on.take() {
            let mut taking = *taking;
            self.hooks.provisional = taking.provisional.take().map(|(.., then)| then);
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
        if self.resumed.is_none() {
            self.ran = (m.out.steps, m.out.steps);
        }
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

    /// Runs a copy of the resumed `m` to the end of the paragraph edited at `path`'s `line`, if that
    /// line is in one of a file the run reads, and hands the result to `then`.
    fn preview(
        &self,
        m: &Machine<'a>,
        id: crate::engine::tex::FileId,
        source: &str,
        path: &Path,
        line: u32,
        then: Preview,
    ) {
        let Some(file) = m.out.files.iter().position(|f| Path::new(&f.path) == path) else { return };
        let text = match file == usize::from(id) {
            true => source.to_string(),
            false => match crate::project::overlay::read_bytes(path) {
                Ok(bytes) => crate::engine::source::decode_counted(&bytes).0,
                Err(_) => return,
            },
        };
        let Some((lo, hi)) = paragraph_at(&text, line) else { return };
        let clock = std::time::Instant::now();
        let mut p = m.clone();
        p.checkpoints.resume = m.checkpoints.resume.clone();
        p.checkpoints.stop = Some((file as crate::engine::tex::FileId, hi, false));
        let (cloned, from) = (clock.elapsed(), p.out.steps);
        p.run_guarded();
        let (ran, steps) = (clock.elapsed(), p.out.steps - from);
        let at = p.checkpoints.stop.is_some_and(|(.., seen)| seen);
        if at && !p.out.exhausted {
            let analysis = p.conclude(id, source);
            let concluded = clock.elapsed();
            then(analysis, &m.out.files[file].path.clone(), lo, hi);
            if self.cfg.verbose >= 2 {
                crate::util::progress::note!(
                    "satex: paragraph {lo}-{hi} first: clone {:.3}s, run {:.3}s for {steps} steps, conclude {:.3}s, rows {:.3}s",
                    cloned.as_secs_f64(),
                    (ran - cloned).as_secs_f64(),
                    (concluded - ran).as_secs_f64(),
                    (clock.elapsed() - concluded).as_secs_f64()
                );
            }
        }
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
            // Only the input is reread: the machine is copied once it holds.
            let (mut input, mut drive) = (checkpoint.machine.input.clone(), checkpoint.drive.clone());
            if let Err(changed) = reread(
                &checkpoint.machine.out,
                &mut input,
                &mut drive,
                &self.log[..checkpoint.reads],
                source,
                &mut holds,
            ) {
                if self.cfg.verbose >= 2 {
                    let at = reading_file_after(&input, 1).map(|(m, _)| m.position());
                    crate::util::progress::note!("satex: checkpoint at {at:?} does not hold: {changed:?}");
                }
                // Keep the furthest changed line any invalid checkpoint found.
                edit = edit.into_iter().chain(changed).max_by_key(|(_, line)| *line);
                old.push(checkpoint);
                continue;
            }
            let mut m = checkpoint.machine.clone();
            m.input = input;
            // `limits.seconds` bounds the whole build, so a resumed run gets only
            // the time the earlier runs left. The limit only stops a run, it never changes its result.
            m.started = self.build_started.unwrap_or_else(std::time::Instant::now);
            m.checkpoints.resume = Some(Box::new(drive));
            crate::engine::source::set_encoding(m.out.plugins.engine.encoding());
            if let Some((mouth, _)) = reading_file_after(&m.input, 1) {
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

/// Points `input` and every path of `drive` at the current file contents.
/// Succeeds if each file in `log` is unchanged and each open file still starts
/// with what was read of it. Otherwise returns where a file changed, if known.
fn reread(
    out: &Analysis,
    input: &mut super::Input,
    drive: &mut Drive,
    log: &[crate::project::overlay::Read],
    source: &str,
    holds: &mut HashMap<(PathBuf, bool), bool>,
) -> Result<(), Option<Edit>> {
    let paths: Vec<PathBuf> = out.files.iter().map(|f| PathBuf::from(&f.path)).collect();
    let main = out.main_file;
    let mut inputs: Vec<(&mut super::Input, bool)> = std::iter::once((input, true)).chain(drive.inputs()).collect();
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

/// The lines of `path` that differ between a stale checkpoint's open copy and the file now,
/// when both have as many lines.
fn closed_window(stale: &[Checkpoint], path: &Path, source: &str, main: crate::engine::tex::FileId) -> Option<Window> {
    let mouth = stale.iter().find_map(|c| {
        c.machine.input.iter().find_map(|f| match f {
            Frame::File { mouth, .. } if Path::new(&c.machine.out.files[usize::from(mouth.file)].path) == path => {
                Some(mouth.clone())
            }
            _ => None,
        })
    })?;
    let now = match mouth.file == main {
        true => crate::engine::source::recode(source).into_owned(),
        false => crate::engine::source::decode_counted(&crate::project::overlay::read_bytes(path).ok()?).0,
    };
    let now: Vec<char> = now.chars().collect();
    let (a, b): (Vec<&[char]>, Vec<&[char]>) =
        (mouth.source().split(|&c| c == '\n').collect(), now.split(|&c| c == '\n').collect());
    let differ: Vec<u32> = (0..a.len()).filter(|&i| a.len() == b.len() && a[i] != b[i]).map(|i| i as u32 + 1).collect();
    Some((mouth.file, *differ.first()?, *differ.last()?))
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
