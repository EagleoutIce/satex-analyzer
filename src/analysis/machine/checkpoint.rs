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
        self.resumed = None;
        let clock = std::time::Instant::now();
        let (resumed, edit) = self.resume(source);
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
        // Reads after the last checkpoint. No resume depends on them, but the build's reruns do.
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

    /// The machine of the last checkpoint that is still valid, set up to read
    /// the files as they are now, with the main file's id. Also returns where
    /// a file open in a later, invalid checkpoint first changed.
    fn resume(&mut self, source: &str) -> (Option<(Machine<'a>, crate::engine::tex::FileId)>, Option<Edit>) {
        let (mut holds, mut edit) = (HashMap::default(), None);
        while let Some(checkpoint) = self.list.pop() {
            let (mut m, mut drive) = (checkpoint.machine.clone(), checkpoint.drive.clone());
            if let Err(changed) = reread(&mut m, &mut drive, &self.log[..checkpoint.reads], source, &mut holds) {
                if self.cfg.verbose >= 2 {
                    let at = reading_file(&m.input).map(|(m, _)| m.position());
                    crate::util::progress::note!("satex: checkpoint at {at:?} does not hold: {changed:?}");
                }
                // Keep the furthest changed line any invalid checkpoint found.
                edit = edit.into_iter().chain(changed).max_by_key(|(_, line)| *line);
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
            return (Some((m, id)), edit);
        }
        (None, edit)
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
