//! Checkpoints of a document's run, for analyzing it again after an edit:
//! the run takes its whole state now and then while it reads the user's
//! files, and a later run of the same document resumes from the last one
//! whose input is unchanged. Everything a checkpoint saw is the same, so the
//! resumed run is the full run from there on.

use super::explore::Drive;
use super::{Analysis, Frame, Machine};
use crate::engine::tex::Mouth;
use crate::fixed::HashMap;
use crate::project::config::Config;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Steps between two checkpoints at first; more than [`KEPT`] of them
/// thin out and double it.
const EVERY: u64 = 1 << 23;
const KEPT: usize = 8;
/// In the body, between two of its top-level lines (a frame, a paragraph): steps
/// between checkpoints are this share of the current spacing.
const BODY_SHARE: u64 = 8;

/// A run's whole state at a line start of a user's file, its paths joined.
struct Checkpoint<'a> {
    machine: Machine<'a>,
    drive: Drive,
    /// How many entries of [`Checkpoints::log`] it read.
    reads: usize,
    /// Taken where the last edit began, or just before: kept while the list thins out.
    pinned: bool,
    /// The last one before the edit's line so far: the next one on the way replaces it.
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
    /// The steps at the last one.
    last: u64,
    /// Whether this run pinned one for its edit yet.
    pinned: bool,
    /// Lines from the last one on the way to the edit's line to that line.
    toward: u32,
    /// Whether `\begin{document}` had been read at the last one.
    document: bool,
    /// The file and line where the last edit began: a checkpoint is due
    /// there, for the next edit, which is likely near.
    edit: Option<Edit>,
}

/// A file, and the line where it changed.
type Edit = (PathBuf, u32);

/// A copy of a run is no run that takes checkpoints.
impl Clone for Taker<'_> {
    fn clone(&self) -> Self {
        Taker::default()
    }
}

impl Taker<'_> {
    /// The drive a resumed run goes on with, once.
    pub(super) fn resume(&mut self) -> Option<Drive> {
        self.resume.take().map(|d| *d)
    }
}

impl Machine<'_> {
    /// Whether the run stands where a checkpoint is due: reading a user's
    /// file at a line start, in no scan, past the steps
    /// since the last one or just into the document.
    pub(super) fn checkpoint_due(&self) -> bool {
        let Some(taking) = &self.checkpoints.on else { return false };
        let Some((mouth, lookahead)) = reading_file(&self.input) else { return false };
        let document = self.out.document_depth.is_some();
        let gap = self.edit_gap(taking, mouth);
        let edit = gap == Some(0);
        // before the line that opens the document reads the `.aux` a rerun changed
        let opens = !document && opens_document(mouth);
        let since = self.out.steps - taking.last;
        // On the way to the edit's line, each time half as far from it as the last (a few per run):
        // the next edit, likely there again, resumes at the last line start before it.
        let toward = gap.is_some_and(|g| g > 0 && g.saturating_mul(2) <= taking.toward);
        // A top-level line of the body (a frame, a paragraph): an edit there resumes close by.
        let top = || {
            since >= taking.every / BODY_SHARE
                && self.out.document_depth.is_some_and(|d| usize::from(d) == self.env.group_level())
                && !self.env.many_groups()
        };
        let due = self.out.steps >= taking.next || document != taking.document || edit || opens || toward || top();
        // not while a path through an undecided conditional runs: a resume there numbers its meanings anew
        if self.halted || !due || self.env.forking() {
            return false;
        }
        // a preamble command's lookahead (`\@ifnextchar[`) holds the next line's first token
        let settled = match lookahead {
            true => opens,
            false => mouth.line_mark().is_some_and(|m| !m.ended),
        };
        // Below the file, what TeX reads after it: `\include` goes on after its file.
        !self.input.iter().any(|f| matches!(f, Frame::Boundary))
            && settled
            && self.project_file(mouth.file)
            && self.scans.is_empty()
            && self.scan_starts.is_empty()
            && self.cacheable_load.is_none()
            && self.recording_package().is_none()
    }

    /// Keeps the run's state, with `drive`, as a checkpoint.
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
        // The first for this run's edit unpins the last edit's; one on the way replaces the one before.
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
            // Every other one goes; the last one and the pinned one stay.
            let mut n = taking.list.len();
            taking.list.retain(|c| {
                n -= 1;
                c.pinned || n % 2 == 0
            });
            taking.every *= 2;
        }
        (taking.next, taking.last, taking.document) = (steps + taking.every, steps, document);
    }

    /// Lines from the run's place to the last edit's line in its file, 0 at or past it;
    /// `None` elsewhere or without one.
    fn edit_gap(&self, taking: &Taking, mouth: &Mouth) -> Option<u32> {
        let (path, line) = taking.edit.as_ref()?;
        (Path::new(&self.out.files[usize::from(mouth.file)].path) == path)
            .then(|| line.saturating_sub(mouth.position().0))
    }
}

/// The file the run reads, under tokens a lookahead gave back, and whether there are any.
/// A token list read to its end (a macro's last token run) is read no more: TeX drops it
/// at the next read.
fn reading_file(input: &super::Input) -> Option<(&Mouth, bool)> {
    let mut lookahead = false;
    for frame in input.iter().rev() {
        match frame {
            Frame::Returned(_) => lookahead = true,
            Frame::File { mouth, .. } => return Some((mouth, lookahead)),
            _ => return None,
        }
    }
    None
}

/// Whether `\begin{document}` stands on the line `mouth` is on or the next one: a
/// lookahead from this line (`\usepackage`'s `\@ifnextchar`) may read past the next line's start.
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

/// The checkpoints of one document's last run, and what they rest on.
pub struct Checkpoints<'a> {
    cfg: &'a Config,
    /// The document, and what its run read before its first line.
    setup: Option<(PathBuf, u64)>,
    list: Vec<Checkpoint<'a>>,
    /// The files the run read or looked for, in order.
    log: Vec<crate::project::overlay::Read>,
    /// The document's text at the last run: where it changed is the edit's line.
    source: String,
    every: u64,
    /// The line the last run resumed at, if it did.
    pub resumed: Option<(String, u32)>,
    /// When the build's first run had its kernel: every run's `limits.seconds` counts from it.
    pub build_started: Option<std::time::Instant>,
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
            build_started: None,
        }
    }

    /// The files the last run read or looked for.
    pub fn looked(&self) -> Vec<PathBuf> {
        self.log.iter().map(|r| r.path.clone()).collect()
    }

    /// Checkpoints at most every `steps` steps, not [`EVERY`].
    pub fn every(self, steps: u64) -> Self {
        Checkpoints { every: steps.max(1), ..self }
    }

    /// `source`, the document at `path`, analyzed: resumed from the last
    /// checkpoint whose input is still the same, else from the start.
    /// Either way the analysis is that of a full run.
    pub fn analyze(&mut self, source: &str, path: &Path) -> Analysis {
        let (_run, _entered) = crate::analysis::budget::Run::start();
        let setup = Some((path.to_path_buf(), setup_key(self.cfg, source, path)));
        if setup != self.setup {
            (self.list, self.log) = (Vec::new(), Vec::new());
        }
        self.setup = setup;
        self.resumed = None;
        let clock = std::time::Instant::now();
        let (resumed, edit) = self.resume(source);
        let resumed_in = clock.elapsed();
        // The document's own first changed line says best where the edit is.
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
        };
        m.checkpoints.on = Some(Box::new(taking));
        let (clock, from) = (std::time::Instant::now(), m.out.steps);
        m.run_guarded();
        let ran = (clock.elapsed(), m.out.steps - from);
        if let Some(taking) = m.checkpoints.on.take() {
            (self.list, self.log, self.every) = (taking.list, taking.log, taking.every);
        }
        // Reads past the last checkpoint: no resume rests on them, the build's reruns do.
        self.log.extend(crate::project::overlay::log(false));
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

    /// The machine of the last checkpoint that still holds, made to read
    /// the files as they are now, and the main file's id; and where an
    /// open file of a later one changed first.
    fn resume(&mut self, source: &str) -> (Option<(Machine<'a>, crate::engine::tex::FileId)>, Option<Edit>) {
        let (mut holds, mut edit) = (HashMap::default(), None);
        while let Some(checkpoint) = self.list.pop() {
            let (mut m, mut drive) = (checkpoint.machine.clone(), checkpoint.drive.clone());
            if let Err(changed) = reread(&mut m, &mut drive, &self.log[..checkpoint.reads], source, &mut holds) {
                if self.cfg.verbose >= 2 {
                    let at = reading_file(&m.input).map(|(m, _)| m.position());
                    crate::util::progress::note!("satex: checkpoint at {at:?} does not hold: {changed:?}");
                }
                // Where in the file is known best from a checkpoint that read in it.
                edit = edit.into_iter().chain(changed).max_by_key(|(_, line)| *line);
                continue;
            }
            // `limits.seconds` bounds the build: a resumed run has what the
            // runs before it left (it only stops one, never steers it).
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
            return (Some((m, id)), edit);
        }
        (None, edit)
    }
}

/// Makes every path of `m` and `drive` read its files as they are now, if
/// each file `log` read is the same and those still open begin with what
/// was read of them; else where such a file changed, when that is known.
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
    // A file every path that may run still reads: what they read of it is
    // checked below, by its prefix. Any other was read through by a path.
    let mut live = inputs.iter().filter(|(_, live)| *live).map(|(i, _)| files(i));
    let mut open = live.next().unwrap_or_default();
    for other in live {
        open.retain(|f| other.contains(f));
    }
    // The document itself is in no log: a path past its end read all of it.
    if !open.contains(&main) {
        return Err(None);
    }
    // Read once: a file opened again was read through before.
    let reads = |path: &PathBuf| log.iter().filter(|r| !r.probe && r.path == *path).count();
    let open: Vec<&PathBuf> = open.into_iter().map(|f| &paths[usize::from(f)]).filter(|p| reads(p) <= 1).collect();
    for read in log.iter().filter(|r| r.probe || !open.contains(&&r.path)) {
        if !*holds.entry((read.path.clone(), read.probe)).or_insert_with(|| read.holds()) {
            // Changed somewhere: the next run takes a checkpoint at its start.
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
            // One of several texts: which one the path read is its choice, so
            // it reads the file again from its start.
            (super::texts_of(&text).len() < 2).then(|| Mouth::file(&text, file))
        });
        let now = now.as_ref().ok_or_else(|| Some((paths[usize::from(file)].clone(), 1)))?;
        let read = mouth.retext(now).map_err(|line| Some((paths[usize::from(file)].clone(), line)))?;
        if let Frame::File { mouth, .. } = Rc::make_mut(frame) {
            *mouth = read;
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

/// What a run reads before the document's first line, besides the
/// configuration: the project around it, and what the whole source says
/// at once (magic comments, the kernel it asks for, a `%&` format line).
fn setup_key(cfg: &Config, source: &str, path: &Path) -> u64 {
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
