//! Checkpoints of a document's run, for analyzing it again after an edit:
//! the run takes its whole state now and then while it reads the user's
//! files, and a later run of the same document resumes from the last one
//! whose input is unchanged. Everything a checkpoint saw is the same, so the
//! resumed run is the full run from there on.

use super::explore::Drive;
use super::{Analysis, Frame, Machine};
use crate::config::Config;
use crate::fixed::HashMap;
use crate::tex::Mouth;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Steps between two checkpoints at first; more than [`KEPT`] of them
/// thin out and double it.
const EVERY: u64 = 1 << 23;
const KEPT: usize = 8;

/// A run's whole state at a line start of a user's file, its paths joined.
struct Checkpoint<'a> {
    machine: Machine<'a>,
    drive: Drive,
    /// How many entries of [`Checkpoints::log`] it read.
    reads: usize,
    /// Taken where the last edit began: kept while the list thins out.
    pinned: bool,
}

/// The checkpoints a run takes, and the drive it resumes with.
#[derive(Default)]
pub(super) struct Taker<'a> {
    on: Option<Box<Taking<'a>>>,
    resume: Option<Box<Drive>>,
}

struct Taking<'a> {
    list: Vec<Checkpoint<'a>>,
    log: Vec<crate::overlay::Read>,
    every: u64,
    next: u64,
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
        let Some(Frame::File { mouth, .. }) = self.input.last() else { return false };
        let document = self.out.document_depth.is_some();
        let edit = taking.edit.as_ref().is_some_and(|(path, line)| {
            mouth.position().0 >= *line && Path::new(&self.out.files[usize::from(mouth.file)].path) == path
        });
        if self.halted || (self.out.steps < taking.next && document == taking.document && !edit) {
            return false;
        }
        // Below the file, what TeX reads after it: `\include` goes on after its file.
        !self.input.iter().any(|f| matches!(f, Frame::Boundary))
            && mouth.line_mark().is_some_and(|m| !m.ended)
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
        let Some(taking) = self.checkpoints.on.as_mut() else { return };
        taking.log.extend(crate::overlay::log(true));
        let pinned = taking.edit.take().is_some();
        if pinned {
            taking.list.iter_mut().for_each(|c| c.pinned = false);
        }
        taking.list.push(Checkpoint { machine, drive, reads: taking.log.len(), pinned });
        if taking.list.len() > KEPT {
            // Every other one goes; the last one and the pinned one stay.
            let mut n = taking.list.len();
            taking.list.retain(|c| {
                n -= 1;
                c.pinned || n % 2 == 0
            });
            taking.every *= 2;
        }
        (taking.next, taking.document) = (steps + taking.every, document);
    }
}

/// The checkpoints of one document's last run, and what they rest on.
pub struct Checkpoints<'a> {
    cfg: &'a Config,
    /// The document, and what its run read before its first line.
    setup: Option<(PathBuf, u64)>,
    list: Vec<Checkpoint<'a>>,
    /// The files the run read or looked for, in order.
    log: Vec<crate::overlay::Read>,
    every: u64,
    /// The line the last run resumed at, if it did.
    pub resumed: Option<(String, u32)>,
}

impl<'a> Checkpoints<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        Checkpoints { cfg, setup: None, list: Vec::new(), log: Vec::new(), every: EVERY, resumed: None }
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
        let (_run, _entered) = crate::budget::Run::start();
        let setup = Some((path.to_path_buf(), setup_key(self.cfg, source, path)));
        if setup != self.setup {
            (self.list, self.log) = (Vec::new(), Vec::new());
        }
        self.setup = setup;
        self.resumed = None;
        let (resumed, edit) = self.resume(source);
        crate::overlay::log(true);
        let (mut m, id) = match resumed {
            Some(resumed) => resumed,
            None => {
                self.log.clear();
                let (m, id) = Machine::begin(source, Some(path), self.cfg);
                // A literate source is read through docstrip's view.
                if m.out.literate.is_some() {
                    crate::overlay::log(false);
                    let mut m = m;
                    m.run_guarded();
                    return m.conclude(id);
                }
                (m, id)
            }
        };
        let next = m.out.steps + self.every;
        let taking = Taking {
            list: std::mem::take(&mut self.list),
            log: std::mem::take(&mut self.log),
            every: self.every,
            next,
            document: m.out.document_depth.is_some(),
            edit,
        };
        m.checkpoints.on = Some(Box::new(taking));
        m.run_guarded();
        if let Some(taking) = m.checkpoints.on.take() {
            (self.list, self.log, self.every) = (taking.list, taking.log, taking.every);
        }
        // Reads past the last checkpoint: no resume rests on them, the build's reruns do.
        self.log.extend(crate::overlay::log(false));
        m.conclude(id)
    }

    /// The machine of the last checkpoint that still holds, made to read
    /// the files as they are now, and the main file's id; and where an
    /// open file of a later one changed first.
    fn resume(&mut self, source: &str) -> (Option<(Machine<'a>, crate::tex::FileId)>, Option<Edit>) {
        let (mut holds, mut edit) = (HashMap::default(), None);
        while let Some(checkpoint) = self.list.pop() {
            let (mut m, mut drive) = (checkpoint.machine.clone(), checkpoint.drive.clone());
            if let Err(changed) = reread(&mut m, &mut drive, &self.log[..checkpoint.reads], source, &mut holds) {
                // Where in the file is known best from a checkpoint that read in it.
                edit = edit.into_iter().chain(changed).max_by_key(|(_, line)| *line);
                continue;
            }
            // `limits.seconds` bounds each run: it only stops one, never steers it.
            m.started = std::time::Instant::now();
            m.checkpoints.resume = Some(Box::new(drive));
            crate::source::set_encoding(m.out.plugins.engine.encoding());
            if let Some(Frame::File { mouth, .. }) = m.input.last() {
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
    log: &[crate::overlay::Read],
    source: &str,
    holds: &mut HashMap<(PathBuf, bool), bool>,
) -> Result<(), Option<Edit>> {
    let paths: Vec<PathBuf> = m.out.files.iter().map(|f| PathBuf::from(&f.path)).collect();
    let main = m.out.main_file;
    let mut inputs: Vec<(&mut super::Input, bool)> =
        std::iter::once((&mut m.input, true)).chain(drive.inputs()).collect();
    let files = |input: &super::Input| -> Vec<crate::tex::FileId> {
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
    let mut fresh: HashMap<crate::tex::FileId, Option<Mouth>> = HashMap::default();
    for frame in inputs.iter_mut().flat_map(|(i, _)| i.0.iter_mut()) {
        let Frame::File { mouth, .. } = &**frame else { continue };
        let file = mouth.file;
        let now = fresh.entry(file).or_insert_with(|| {
            let path = &paths[usize::from(file)];
            let text = match file == main {
                true => crate::source::recode(source).into_owned(),
                // A literate source is read through docstrip's view.
                false if crate::literate::Literate::of(path).is_some() => return None,
                false => crate::source::decode_counted(&crate::overlay::read_bytes(path).ok()?).0,
            };
            Some(Mouth::file(&text, file))
        });
        let now = now.as_ref().ok_or(None)?;
        let read = mouth.retext(now).map_err(|line| Some((paths[usize::from(file)].clone(), line)))?;
        if let Frame::File { mouth, .. } = Rc::make_mut(frame) {
            *mouth = read;
        }
    }
    Ok(())
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
    format!("{:?}", crate::project::Project::discover(&base)).hash(&mut hasher);
    if cfg.plugins.discovery {
        format!("{:?}", crate::plugin::discovery::scan_with(&base, cfg.limits.thread_count())).hash(&mut hasher);
    }
    format!("{:?}", crate::plugin::magic::scan(source)).hash(&mut hasher);
    format!("{:?}", crate::plugin::Kernel::detect(source)).hash(&mut hasher);
    format!("{:?}", crate::config::Profile::of_name(Some(path), source)).hash(&mut hasher);
    source.lines().next().hash(&mut hasher);
    hasher.finish()
}
