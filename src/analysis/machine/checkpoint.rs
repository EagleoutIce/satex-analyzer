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
    /// An anchor taken before `\\begin{document}` ran: the next such one replaces it.
    opens: bool,
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
    /// What gets the preview once the run reaches `stop`, which then goes on: a copy is concluded
    /// there ([`Machine::hand_preview`]). Without it the run halts at `stop`.
    preview: Option<Box<Pending>>,
}

/// A paragraph preview the run hands on at its end: the hook, the main file and its text, the
/// paragraph's file name and its first and last line.
struct Pending(Preview, crate::engine::tex::FileId, Rc<str>, String, u32, u32);

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
    /// Past the edit's line: the pages shipped at the last checkpoint, and how many more to take (`limits.checkpoints_after_edit`).
    after: Option<(u64, u32)>,
    /// File and line where the last edit began. The next edit is likely
    /// nearby, so a checkpoint is taken there.
    edit: Option<Edit>,
    /// The edited file, the edit's last line and how many more to take
    /// (`limits.checkpoints_after_edit`): each of the first lines past it between paragraphs
    /// gets a kept checkpoint, where the next edit's run may meet this one. The first may not
    /// do: the call that ended the edited paragraph is still recorded there.
    past: Option<(PathBuf, u32, u32)>,
    /// The step at which this run was last compared with the last run's state and differed:
    /// a checkpoint kept there for the next edit's run would differ from that one alike.
    apart: Option<u64>,
    /// A run of the text before the edit that only places checkpoints around it
    /// ([`Checkpoints::plant`]): the step it stops at, at the latest.
    until: Option<u64>,
    /// The last run, whose state this one may meet again after the edit.
    pub(super) prior: Prior<'a>,
    /// Assignments and saves made and characters set when this run resumed: the runs differ
    /// only in what came after.
    pub(super) since: (u64, usize, u32),
    pub(super) parted: provisional::Parted,
    /// Where this run's state first equals the last run's (see [`Checkpoints::converged`]).
    converged: Option<(String, u32, u64)>,
    /// The last run's end with this run's facts before the meeting line: this run's end.
    spliced: Option<Box<Machine<'a>>>,
    /// What the last run read after the meeting line.
    tail: Vec<crate::project::overlay::Read>,
    /// After a splice: when this run and the last run's rest last read or wrote each slot.
    used: Option<Vec<u64>>,
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
    /// When it last read or wrote each slot ([`crate::engine::env::LastUse`]).
    used: Vec<u64>,
    /// Per file it read or looked for, whether that still holds ([`Prior::rest_holds`]).
    held: HashMap<(PathBuf, bool, bool, u64), bool>,
    /// The file edited since.
    pub(super) edit: Option<PathBuf>,
    /// Its edited lines, when they differ one for one: where the states meet after the file
    /// was read to its end ([`edited_lines`] needs it open).
    pub(super) window: Option<Window>,
}

/// A file, and the line where it changed.
type Edit = (PathBuf, u32);

/// The one file edited since the last run (`None`: the document itself), its text then, the
/// first line that differs, and the last one in the old and in the new text.
struct Change {
    path: Option<PathBuf>,
    was: Rc<str>,
    lo: u32,
    hi: (u32, u32),
}

/// A cloned machine does not take checkpoints.
impl Clone for Taker<'_> {
    fn clone(&self) -> Self {
        Taker::default()
    }
}

impl Prior<'_> {
    /// Whether every file the last run read after its first `reads` is still what it found:
    /// only then did its rest read what this run's would. An edit the rest has yet to read
    /// (a file read again, a meeting line before the edited file is opened) fails here.
    pub(super) fn rest_holds(&mut self, reads: usize) -> bool {
        let (log, held) = (&self.log, &mut self.held);
        let holds = |r: &crate::project::overlay::Read| {
            *held.entry((r.path.clone(), r.probe, r.own, r.print)).or_insert_with(|| r.holds())
        };
        log.get(reads..).unwrap_or_default().iter().all(holds)
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
        let top = || since >= taking.every / self.cfg.limits.checkpoint_body_share.max(1) && level();
        // A page shipped since the edit's line, or since the last checkpoint after it.
        let shipped = || taking.after.is_some_and(|(pages, _)| self.pages > pages) && level();
        let due = self.out.steps >= taking.next
            || document != taking.document
            || self.past_edit(taking, mouth).is_some()
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

    /// Whether the run hands a paragraph preview on here, at its end ([`Taker::preview`]).
    pub(super) fn preview_due(&mut self) -> bool {
        self.checkpoints.preview.is_some() && self.preview_done()
    }

    /// Concludes a copy of the run at the paragraph preview's end and hands it on; the run goes
    /// on. The copy halts where a separate run from the same checkpoint would have, with the
    /// same state: the run itself is that run up to here.
    pub(super) fn hand_preview(&mut self, drive: Drive) {
        let (Some(pending), stop) = (self.checkpoints.preview.take(), self.checkpoints.stop.take()) else { return };
        let Pending(then, id, source, path, lo, hi) = *pending;
        let clock = std::time::Instant::now();
        let mut p = self.clone();
        p.checkpoints.resume = Some(Box::new(drive));
        p.checkpoints.stop = stop;
        let cloned = clock.elapsed();
        p.run_guarded();
        if p.out.exhausted {
            return;
        }
        let analysis = p.conclude(id, &source);
        let concluded = clock.elapsed();
        then(analysis, &path, lo, hi);
        if self.cfg.verbose >= 2 {
            crate::util::progress::note!(
                "satex: paragraph {lo}-{hi} first: clone {:.3}s, conclude {:.3}s, rows {:.3}s",
                cloned.as_secs_f64(),
                (concluded - cloned).as_secs_f64(),
                (clock.elapsed() - concluded).as_secs_f64()
            );
        }
    }

    /// Saves the run's state and `drive` as a checkpoint.
    pub(super) fn take_checkpoint(&mut self, drive: Drive) {
        // No step since the last one (the run just resumed from it): that one stands for this state.
        let again = self
            .checkpoints
            .on
            .as_ref()
            .and_then(|t| t.list.last())
            .is_some_and(|c| c.machine.out.steps == self.out.steps);
        if std::env::var_os("SATEX_MEMPROBE").is_some() && !again {
            let o = &self.out;
            let fa = &o.facts;
            let mut v = crate::probe_sz!("o.interner" => o.interner, "o.graph" => o.graph, "o.calls" => o.calls, "o.observed" => o.observed, "o.files" => o.files, "o.facts" => o.facts, "o.trace" => o.trace, "o.env" => o.env, "o.kernel_sites" => o.kernel_sites, "o.font_states" => o.font_states, "out" => self.out,
                "f.defs" => fa.defs, "f.types" => fa.types, "f.length_fonts" => fa.length_fonts, "f.vars" => fa.vars, "f.letters" => fa.letters, "f.expansions" => fa.expansions, "f.loads" => fa.loads, "f.occurrences" => fa.occurrences, "f.literals" => fa.literals, "f.diagnostics" => fa.diagnostics, "f.csnames" => fa.csnames, "f.groups" => fa.groups, "f.raised" => fa.raised, "f.spaces" => fa.spaces, "f.kept_spaces" => fa.kept_spaces, "f.conditionals" => fa.conditionals, "f.text" => fa.text, "f.lines" => fa.lines, "f.shapes" => fa.shapes,
                "env" => self.env, "kp" => self.kp, "input" => self.input, "catcodes" => self.catcodes, "lua" => self.lua, "carried" => self.carried, "scratch" => self.scratch, "outputs" => self.outputs, "streams" => self.streams, "unknown_chars" => self.unknown_chars, "box_chars" => self.box_chars, "diag_seen" => self.diag_seen, "quantity" => self.quantity, "undo" => self.undo, "choices" => self.choices, "pending" => self.pending, "definition_sites" => self.definition_sites, "expansion_sites" => self.expansion_sites, "site_writes" => self.site_writes, "cond_sites" => self.cond_sites, "call_runs" => self.call_runs, "memos" => self.memos, "memo_codes" => self.memo_codes, "memo_seen" => self.memo_seen, "name_reads" => self.name_reads, "lines_seen" => self.lines_seen, "settings_scopes" => self.settings_scopes, "cfg" => self.cfg, "pinned" => self.pinned, "read_whole" => self.read_whole, "box_saves" => self.box_saves, "diag_raised" => self.diag_raised, "scans" => self.scans, "resolver" => self.resolver, "cds" => self.cds, "whole" => *self);
            v.extend(self.out.graph.probe());
            v.extend(self.env.probe());
            v.sort_by_key(|p| std::cmp::Reverse(p.1));
            eprintln!("PROBE step {} {:?}", self.out.steps, &v[..v.len().min(30)]);
        }
        if !again {
            // The copy shares the site tables whole.
            self.expansion_sites.settle();
            self.call_runs.settle();
            self.out.graph.settle();
        }
        let (before, clock) = (crate::analysis::budget::in_use(), std::time::Instant::now());
        let machine = (!again).then(|| self.clone());
        let (bytes, copied) = (crate::analysis::budget::in_use().saturating_sub(before), clock.elapsed());
        let budget = self.cfg.limits.checkpoint_memory.as_u64() as usize;
        let (steps, document) = (self.out.steps, self.out.document_depth.is_some());
        let opens = reading_file_after(&self.input, 1).is_some_and(|(mouth, _)| !document && opens_document(mouth));
        let (gap, past) = match (&self.checkpoints.on, reading_file_after(&self.input, 1)) {
            (Some(taking), Some((mouth, _))) => (self.edit_gap(taking, mouth), self.past_edit(taking, mouth)),
            _ => (None, None),
        };
        let spliced = self.spliced();
        let Some(taking) = self.checkpoints.on.as_mut() else { return };
        taking.log.extend(crate::project::overlay::log(true));
        let (toward, mut pinned) = (gap.is_some_and(|g| g > 0), gap.is_some() || past.is_some() || spliced);
        // The next one past the edit is on a later line of the edited file, none once that is read.
        if let Some(line) = past {
            let next = |(at, _, left): (PathBuf, u32, u32)| Some((at, line?, left.checked_sub(1).filter(|&l| l > 0)?));
            taking.past = taking.past.take().and_then(next);
        }
        // The pages after the edit's line are kept as the edit's own checkpoints.
        if let Some((pages, left)) = taking.after.filter(|&(pages, _)| self.pages > pages) {
            taking.after = (left > 1).then_some((self.pages.max(pages), left - 1));
            pinned = true;
        }
        // On the way to the edit's line or at it: the pages from here on count (`limits.checkpoints_after_edit`).
        if gap.is_some() {
            taking.after = Some((self.pages, self.cfg.limits.checkpoints_after_edit));
        }
        match gap {
            Some(0) => {
                taking.edit = None;
                // A run that only places checkpoints goes a bounded way past the edit.
                let far = steps.saturating_add(self.cfg.limits.checkpoint_meet_steps);
                taking.until = taking.until.map(|until| until.min(far));
            }
            Some(g) => taking.toward = g,
            None => {}
        }
        if self.cfg.verbose >= 2 {
            crate::util::progress::note!(
                "satex: checkpoint at {:?}, step {steps}, {} MiB in {:.3}s",
                reading_file_after(&self.input, 1).map(|(m, _)| m.position()),
                bytes >> 20,
                copied.as_secs_f64()
            );
        }
        // The first checkpoint for this edit unpins the previous edit's.
        // A checkpoint on the way to the edit replaces the previous one on the way.
        if pinned && !taking.pinned {
            taking.list.iter_mut().for_each(|c| c.pinned = false);
            taking.pinned = true;
        }
        let again = taking.list.pop_if(|_| machine.is_none());
        if toward {
            taking.list.retain(|c| !c.toward);
        } else {
            taking.list.iter_mut().for_each(|c| c.toward = false);
        }
        let anchor = opens || document != taking.document;
        // Each command on the line that opens the document is nearer to it: the latest one stays.
        if opens {
            taking.list.retain(|c| !c.opens);
        }
        let reads = taking.log.len();
        taking.list.extend(match (machine, again) {
            (Some(machine), _) => Some(Checkpoint { machine, drive, reads, pinned, toward, anchor, opens, bytes }),
            // The same state, with the files as this run reads them.
            (None, Some(last)) => {
                let machine = Machine { input: self.input.clone(), ..last.machine };
                let (pinned, anchor, opens) = (last.pinned || pinned, last.anchor || anchor, last.opens || opens);
                Some(Checkpoint { machine, drive, reads, pinned, toward, anchor, opens, bytes: last.bytes })
            }
            (None, None) => None,
        });
        // Each copy holds what grows along the document (graph, facts, sites)
        // anew, so their bytes are bounded too, not only their number.
        let over = |list: &[Checkpoint]| budget > 0 && held(list) > budget;
        let kept = |c: &Checkpoint| c.pinned || c.anchor;
        while taking.list.len() > self.cfg.limits.checkpoints.max(1)
            || (over(&taking.list) && taking.list.iter().filter(|c| !kept(c)).count() > 1)
        {
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
            self.lax_splice(&old, reads);
        }
        if self.cfg.verbose >= 2 {
            crate::util::progress::note!(
                "satex: state at {path}:{line} vs the last run's: {}",
                differs.unwrap_or("alike")
            );
        }
        if let Some(taking) = self.checkpoints.on.as_mut() {
            match differs {
                None => taking.converged = Some((path, line, total.saturating_sub(old.out.steps))),
                Some(_) => taking.apart = Some(self.out.steps),
            }
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
        if !prior.rest_holds(reads) {
            return Err("the rest reads a changed file");
        }
        let window = edited_lines(&self.input, &old.input, &self.out.files, &edit, window).ok_or("edit elsewhere")?;
        // Between paragraphs: one going on would be broken into lines with the edited text.
        if self.mode != crate::engine::mode::Modes::VERTICAL {
            return Err("inside a paragraph");
        }
        // The places the run keeps of what it read last stand past the edited lines: what the
        // rest records at one of them it records as this run would.
        let lasts = |m: &Machine| {
            let spans = [
                m.project_site,
                m.typed_at,
                m.project_call.map(|c| c.1),
                m.text_site.map(|c| c.1),
                m.built_name.map(|c| c.1),
            ];
            (spans, m.call_parts.as_ref().map(|p| (p.0, p.1)))
        };
        let (mine, theirs) = (lasts(self), lasts(old));
        let on_edit = |s: &crate::engine::tex::Span| s.file == window.0 && (window.1..=window.2).contains(&s.line);
        let places = |l: &(
            [Option<crate::engine::tex::Span>; 5],
            Option<(crate::engine::tex::Span, crate::engine::tex::Span)>,
        )| { l.0.iter().flatten().chain(l.1.iter().flat_map(|p| [&p.0, &p.1])).any(on_edit) };
        if mine != theirs || places(&mine) {
            return Err("a place on the edited lines");
        }
        // In the body: a package load replays against bindings it reads unnoted.
        if self.out.document_depth.is_none() {
            return Err("before the document");
        }
        if (&self.diag_seen, &self.diag_raised, &self.diag_live) != (&old.diag_seen, &old.diag_raised, &old.diag_live) {
            return Err("diagnostics raised");
        }
        // Counts of tokens taken and assignments made differ by the edit's. They are compared
        // only with marks taken since, which differ alike, or before the edit, which both passed.
        let parted = self.checkpoints.on.as_ref().map_or((0, 0, 0), |t| t.parted);
        let (mine, theirs) =
            (provisional::comparable_path(self, false, parted), provisional::comparable_path(old, false, parted));
        if mine != theirs {
            return Err("path state");
        }
        // The names bound otherwise than in the last run: its rest ran as this run's would
        // only if it never read or wrote one of them ([`crate::engine::env::Env::touched`]).
        // They then end as this run binds them here.
        let delta = self.env.differing(&old.env);
        let prior = self.checkpoints.on.as_ref().map(|t| &t.prior).ok_or("no checkpoints")?;
        if delta.iter().any(|&s| prior.used.get(s as usize).is_some_and(|&at| at >= old.out.steps)) {
            return Err("bindings the rest reads");
        }
        // A recorded list is named by where it was made: the runs name one list alike only
        // when this one began with every list the last one made ([`crate::engine::linebreak::Arena::grown`]).
        if !prior.end.as_ref().is_some_and(|end| self.kp.holds(&end.kp) && self.out.interner.holds(&end.out.interner)) {
            return Err("lists or names numbered apart");
        }
        // An open call's record compares without where its reads begin in the log: the logs may
        // differ in length by the edit's reads, so the rest must not have come that near the log's bound.
        let longer = self.env.read_log_len().checked_sub(old.env.read_log_len()).ok_or("read log")?;
        if self.env.logging() != old.env.logging()
            || !prior.end.as_ref().is_some_and(|e| e.env.read_log_alike(old.out.steps, longer))
        {
            return Err("read log");
        }
        let since = self.checkpoints.on.as_ref().map_or((0, 0, 0), |t| t.since);
        let traces =
            [mine, self.env.trace((since.0, since.1), &delta), theirs, old.env.trace((since.0, since.1), &delta)];
        if traces.iter().any(|t| mentions(t, window)) {
            return Err("tokens of the edited lines");
        }
        // Last: it splices `end` in place, so a failure leaves no end for a later line.
        let mut end = self.checkpoints.on.as_mut().and_then(|t| t.prior.end.take()).ok_or("no end of the last run")?;
        provisional::splice_out(&self.out, &old.out, &mut end.out, window, since.2, false)?;
        provisional::splice_records(self, old, &mut end, window)?;
        end.env.carry(&self.env, &delta);
        end.kp = self.kp.clone();
        end.started = self.started;
        // The rest used what the last run used from here on.
        let (mine, was, now) = (self.env.touched.times(), old.out.steps, self.out.steps);
        let Some(taking) = self.checkpoints.on.as_mut() else { return Err("no checkpoints") };
        taking.used = Some(crate::engine::env::LastUse::joined(mine, &taking.prior.used, was, now));
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
            // An exact splice carries the names bound otherwise, where the rest reads none ([`Machine::splice`]).
            (!self.env.alike(&old.env, (since.0, since.1), lax), "bindings"),
            (self.catcodes != old.catcodes, "catcodes"),
            (self.conds.len() != old.conds.len(), "conditionals"),
            (!Rc::ptr_eq(&self.outputs, &old.outputs) && self.outputs != old.outputs, "files written"),
            (!Rc::ptr_eq(&self.pending, &old.pending) && self.pending != old.pending, "writes held"),
            ((self.mode, self.list) != (old.mode, old.list) || !lax && self.pages != old.pages, "mode or pages"),
            (!lax && self.natural != old.natural, "page so far"),
            (self.out.graph.len() != old.out.graph.len(), "graph"),
            (self.out.files.len() != old.out.files.len(), "files read"),
            (counts(f) != counts(g), "facts"),
        ]
        .into_iter()
        .find_map(|(differs, what)| differs.then_some(what))
    }

    /// Whether the run stands between paragraphs on a line past the edit (the line), or
    /// has left the edited file (`None`): where its state may equal that of the run before
    /// the edit again.
    fn past_edit(&self, taking: &Taking, mouth: &Mouth) -> Option<Option<u32>> {
        let (path, hi, _) = taking.past.as_ref().filter(|_| taking.apart != Some(self.out.steps))?;
        let edited = |f: crate::engine::tex::FileId| Path::new(&self.out.files[usize::from(f)].path) == path;
        let open = self.input.iter().any(|f| matches!(f, Frame::File { mouth, .. } if edited(mouth.file)));
        let read = taking.edit.is_none() && (0..self.out.files.len()).any(|f| edited(f as crate::engine::tex::FileId));
        let between = self.mode == crate::engine::mode::Modes::VERTICAL
            && self.out.document_depth.is_some_and(|d| usize::from(d) == self.env.group_level());
        let line = mouth.position().0;
        match open {
            true => (between && edited(mouth.file) && line > *hi).then_some(Some(line)),
            false => (between && read).then_some(None),
        }
    }

    /// Whether a run that only places checkpoints has placed them ([`Checkpoints::plant`]).
    pub(super) fn planted(&self) -> bool {
        self.checkpoints.on.as_ref().is_some_and(|t| {
            t.until.is_some_and(|until| {
                let paged = t.after.is_none_or(|a| a.1 < self.cfg.limits.checkpoints_after_edit);
                self.out.steps >= until || (t.edit.is_none() && t.past.is_none() && paged)
            })
        })
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
    /// When the last run last read or wrote each slot ([`crate::engine::env::LastUse`]).
    used: Vec<u64>,
    /// The project's other files as the last run read them.
    texts: HashMap<PathBuf, Rc<str>>,
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
    /// Set by a server that answers between its messages: what a run kept of the one before is
    /// dropped by [`Checkpoints::rest`], after the run's results went out, not before.
    pub defer_drops: bool,
    buried: Option<Prior<'a>>,
}

impl<'a> Checkpoints<'a> {
    /// Drops what the last run kept of the one before it ([`Checkpoints::defer_drops`]).
    pub fn rest(&mut self) {
        self.buried = None;
    }

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
            every: cfg.limits.checkpoint_steps.max(1),
            resumed: None,
            converged: None,
            steps: 0,
            used: Default::default(),
            texts: HashMap::default(),
            ran: (0, 0),
            end: None,
            build_started: None,
            between_runs: None,
            pending: None,
            hooks: RunHooks::default(),
            defer_drops: false,
            buried: None,
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

    /// Use `steps` instead of `limits.checkpoint_steps` as the initial spacing.
    pub fn every(self, steps: u64) -> Self {
        Checkpoints { every: steps.max(1), ..self }
    }

    /// Analyzes `source`, the document at `path`. Resumes from the last
    /// checkpoint whose input is unchanged, else starts from the beginning.
    /// The result is the same as a full run either way.
    pub fn analyze(&mut self, source: &str, path: &Path) -> Analysis {
        let (_run, _entered) = crate::analysis::budget::Run::start();
        let setup = Some((path.to_path_buf(), setup_key(self.cfg, source, path)));
        // The last run's end depends on what its log saw: forgotten with the log.
        if setup != self.setup {
            (self.list, self.log, self.end) = (Vec::new(), Vec::new(), None);
        }
        self.setup = setup;
        (self.resumed, self.converged) = (None, None);
        // Nothing the last run read changed (another file opened in the editor): its end is this run's.
        if let Some(end) = self.end.as_ref().filter(|e| !e.out.exhausted && self.source == source)
            && self.log.iter().all(crate::project::overlay::Read::holds)
        {
            let m = (**end).clone();
            let id = m.out.main_file;
            if self.cfg.verbose >= 2 {
                crate::util::progress::note!("satex: nothing the last run read changed ({} files)", self.log.len());
            }
            self.ran = (0, m.out.steps);
            return m.conclude(id, source);
        }
        let clock = std::time::Instant::now();
        let log = self.log.clone();
        let change = self.change(source);
        let (mut resumed, mut edit, mut old) = self.resume(source);
        if let Some(change) = change.as_ref().filter(|_| resumed.is_some())
            && self.plant(change, &old, &log)
        {
            let stale = std::mem::take(&mut old);
            (resumed, edit, old) = self.resume(source);
            old.extend(stale);
        }
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
        // The last run's lists and names keep their numbers in this one, its own new ones come
        // after: what both runs make is numbered alike, whatever the edit made in between. A
        // name is only ever asked for its meaning (tex.web § 259 `id_lookup`: one never seen
        // and one undefined are the same to TeX), so more names change nothing.
        // After an edit only: a rerun of the same text meets nothing, and numbers as a run from the start does.
        if let Some(end) = self.end.as_ref().filter(|_| self.resumed.is_some() && change.is_some()) {
            if let Some(lists) = m.kp.grown(&end.kp) {
                m.kp = lists;
            }
            if end.out.interner.holds(&m.out.interner) {
                m.out.interner = end.out.interner.clone();
            }
        }
        let edit = changed.map(|line| (PathBuf::from(&m.out.files[usize::from(id)].path), line)).or(edit);
        // Known exactly when the last run's text of the edited file is.
        let main = PathBuf::from(&m.out.files[usize::from(id)].path);
        let at = |c: &Change| c.path.clone().unwrap_or_else(|| main.clone());
        let edit = change.as_ref().map(|c| (at(c), c.lo)).or(edit);
        let past = change.as_ref().map(|c| (at(c), c.hi.1, self.cfg.limits.checkpoints_after_edit));
        if let (Some(then), Some((path, line))) = (self.hooks.preview.take(), &edit) {
            self.preview(&mut m, id, source, path, *line, then);
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
            past,
            apart: None,
            until: None,
            prior: Prior {
                // Lines that differ one for one, known with the old text.
                window: change.as_ref().filter(|c| c.hi.0 == c.hi.1).and_then(|c| {
                    let file = m.out.files.iter().position(|f| Path::new(&f.path) == at(c))?;
                    Some((file as crate::engine::tex::FileId, c.lo, c.hi.0))
                }),
                stale: old,
                end: self.end.take(),
                log,
                steps: self.steps,
                used: self.used.clone(),
                held: HashMap::default(),
                edit: edited,
            },
            converged: None,
            spliced: None,
            tail: Vec::new(),
            used: None,
            since: (m.env.assignments, m.env.journal_len(), m.out.facts.text.at.len() as u32),
            parted: (m.source_progress, m.env.group_ids().0, m.env.assignments),
            provisional: self.hooks.provisional.take().map(|then| (id, Rc::from(source), then)),
        };
        m.checkpoints.on = Some(Box::new(taking));
        m.env.touched.track(std::mem::take(&mut self.used), m.out.steps);
        let (clock, from) = (std::time::Instant::now(), m.out.steps);
        m.run_guarded();
        self.used = m.env.touched.times();
        let ran = (clock.elapsed(), m.out.steps - from);
        self.ran = (ran.1, m.out.steps);
        // Reads after the last checkpoint. No resume depends on them, but the build's reruns do.
        let after = crate::project::overlay::log(false);
        if let Some(taking) = m.checkpoints.on.take() {
            let mut taking = *taking;
            self.hooks.provisional = taking.provisional.take().map(|(.., then)| then);
            (self.list, self.log, self.every) = (taking.list, taking.log, taking.every);
            self.converged = taking.converged;
            if self.defer_drops {
                self.buried = Some(std::mem::take(&mut taking.prior));
            }
            match taking.spliced.take() {
                Some(end) => {
                    m = *end;
                    self.log.extend(taking.tail);
                    self.used = taking.used.take().unwrap_or_default();
                }
                None => self.log.extend(after),
            }
        }
        self.steps = m.out.steps;
        // The project's files as this run read them: the next edit is told from them ([`Checkpoints::change`]).
        self.texts = (0..m.out.files.len() as crate::engine::tex::FileId)
            .filter(|&f| f != id && m.out.project_file(f))
            .filter_map(|f| {
                let at = PathBuf::from(&m.out.files[usize::from(f)].path);
                let text = crate::project::overlay::peek(&at)?;
                Some((at, Rc::from(text)))
            })
            .collect();
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

    /// What was edited since the last run, when that is one file whose old text is known.
    fn change(&self, source: &str) -> Option<Change> {
        let main = (!self.source.is_empty() && self.source != source)
            .then(|| (None, Rc::from(self.source.as_str()), source.to_string()));
        let others = self.texts.iter().filter_map(|(at, was)| {
            let now = crate::project::overlay::peek(at).unwrap_or_default();
            (now != **was).then(|| (Some(at.clone()), was.clone(), now))
        });
        let mut changed = main.into_iter().chain(others);
        let (path, was, now) = changed.next().filter(|_| changed.next().is_none())?;
        let (a, b): (Vec<&str>, Vec<&str>) = (was.split('\n').collect(), now.split('\n').collect());
        let same = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let rest = a.len().min(b.len()) - same;
        let tail = a.iter().rev().zip(b.iter().rev()).take(rest).take_while(|(x, y)| x == y).count();
        let lo = same as u32 + 1;
        Some(Change {
            path,
            lo,
            hi: (lo.max((a.len() - tail) as u32), lo.max((b.len() - tail) as u32)),
            was: was.clone(),
        })
    }

    /// Before the run after an edit, where the last run kept no checkpoint around the edit:
    /// runs the text before the edit again from the last checkpoint that holds, to place
    /// them. One at the edit's line, where the run of the new text resumes, then on the
    /// first lines past the edit between paragraphs and past the next page shipped, where
    /// that run may meet the last one (`limits.checkpoint_meet_steps` bounds how far). The same
    /// run as the last one, so its states are that run's: checked by the files it read.
    /// `old` are the last run's checkpoints that no longer hold, `log` what it read.
    fn plant(&mut self, change: &Change, old: &[Checkpoint<'a>], log: &[crate::project::overlay::Read]) -> bool {
        let Some(last) = self.list.last() else { return false };
        // The last one that holds stands on the line before the edit's (or on it): no line start
        // lies between for a run of the old text to place one at.
        let out = &last.machine.out;
        let adjacent = reading_file_after(&last.machine.input, 1).is_some_and(|(mouth, _)| {
            let path = Path::new(&out.files[usize::from(mouth.file)].path);
            let edited = change.path.as_deref().map_or(mouth.file == out.main_file, |p| p == path);
            edited && mouth.position().0 + 1 >= change.lo
        });
        // The last run's own checkpoints surround the edit already.
        if self.cfg.limits.checkpoint_meet_steps == 0
            || (last.pinned || last.toward || adjacent) && old.first().is_some_and(|c| c.pinned)
        {
            return false;
        }
        let clock = std::time::Instant::now();
        // The edited file as the last run read it, for the files the checkpoint has open
        // (it may hold an older text of one, alike up to where it stands) and those read later.
        let now = change.path.as_ref().map(|p| (p, crate::project::overlay::text(p)));
        if let Some((p, _)) = &now {
            crate::project::overlay::set(p, change.was.to_string());
        }
        let restore = || match &now {
            Some((p, Some(text))) => crate::project::overlay::set(p, text.clone()),
            Some((p, None)) => crate::project::overlay::remove(p),
            None => {}
        };
        let (mut input, mut drive) = (last.machine.input.clone(), last.drive.clone());
        let source = if change.path.is_none() { &*change.was } else { self.source.as_str() };
        let read = &self.log[..last.reads.min(self.log.len())];
        if reread(&last.machine.out, &mut input, &mut drive, read, source, &mut HashMap::default()).is_err() {
            restore();
            return false;
        }
        let mut m = last.machine.clone();
        let id = m.out.main_file;
        let at = change.path.clone().unwrap_or_else(|| PathBuf::from(&m.out.files[usize::from(id)].path));
        m.input = input;
        m.checkpoints.resume = Some(Box::new(drive));
        m.started = self.build_started.unwrap_or_else(std::time::Instant::now);
        crate::engine::source::set_encoding(m.out.plugins.engine.encoding());
        crate::project::overlay::log(true);
        let from = m.out.steps;
        m.checkpoints.on = Some(Box::new(Taking {
            list: Vec::new(),
            log: self.log.clone(),
            every: self.every,
            next: m.out.steps + self.every,
            last: m.out.steps,
            pinned: false,
            toward: u32::MAX,
            document: m.out.document_depth.is_some(),
            after: None,
            edit: Some((at.clone(), change.lo)),
            past: Some((at, change.hi.0, self.cfg.limits.checkpoints_after_edit)),
            apart: None,
            until: Some(old.first().map_or(u64::MAX, |c| c.machine.out.steps)),
            prior: Prior::default(),
            converged: None,
            spliced: None,
            tail: Vec::new(),
            used: None,
            since: (m.env.assignments, m.env.journal_len(), m.out.facts.text.at.len() as u32),
            parted: (m.source_progress, m.env.group_ids().0, m.env.assignments),
            provisional: None,
        }));
        m.run_guarded();
        crate::project::overlay::log(false);
        restore();
        let Some(taking) = m.checkpoints.on.take() else { return false };
        if self.cfg.verbose >= 2 {
            crate::util::progress::note!(
                "satex: placed {} checkpoints around the edit in {:.2}s, {} steps",
                taking.list.len(),
                clock.elapsed().as_secs_f64(),
                m.out.steps - from
            );
        }
        if taking.list.is_empty() || log.get(..taking.log.len()) != Some(&taking.log[..]) {
            return false;
        }
        self.log = taking.log;
        self.list.extend(taking.list);
        true
    }

    /// Sets the resumed `m` to hand on the paragraph edited at `path`'s `line` once it reaches
    /// the paragraph's end ([`Machine::hand_preview`]), if that line is in one of a file the run reads.
    fn preview(
        &self,
        m: &mut Machine<'a>,
        id: crate::engine::tex::FileId,
        source: &str,
        path: &Path,
        line: u32,
        then: Preview,
    ) {
        let Some(file) = m.out.files.iter().position(|f| Path::new(&f.path) == path) else { return };
        let text = match file == usize::from(id) {
            true => source.to_string(),
            false => match crate::project::overlay::peek_bytes(path) {
                Ok(bytes) => crate::engine::source::decode_counted(&bytes).0,
                Err(_) => return,
            },
        };
        let Some((lo, hi)) = paragraph_at(&text, line) else { return };
        // The main file's last paragraph ends with the run: the run's own result is its preview.
        if file == usize::from(id) && text.split('\n').skip(hi as usize).all(|l| l.trim().is_empty()) {
            return;
        }
        m.checkpoints.stop = Some((file as crate::engine::tex::FileId, hi, false));
        let name = m.out.files[file].path.clone();
        m.checkpoints.preview = Some(Box::new(Pending(then, id, Rc::from(source), name, lo, hi)));
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
            // The Lua state went on past this point: a run from here would not have its own.
            if !crate::plugins::lua::in_step(&checkpoint.machine) {
                old.push(checkpoint);
                continue;
            }
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
    format!("{:?}", crate::project::build_files::Project::discover_with(&base, &cfg.l3build)).hash(&mut hasher);
    if cfg.plugins.discovery {
        format!("{:?}", crate::plugins::discovery::scan_with(&base, cfg.limits.thread_count(), cfg)).hash(&mut hasher);
    }
    format!("{:?}", crate::plugins::magic::scan_with(source, &cfg.magic)).hash(&mut hasher);
    format!("{:?}", crate::plugins::Kernel::detect(source)).hash(&mut hasher);
    format!("{:?}", crate::project::config::Profile::of_name(Some(path), source)).hash(&mut hasher);
    source.lines().next().hash(&mut hasher);
    hasher.finish()
}
