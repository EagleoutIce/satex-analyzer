//! Document caches: a run's whole [`Analysis`], one file per document
//! (a run under other settings or of an edited text replaces it), and served again while the document's text, the settings, the
//! installation's trees and every file the run read or looked for are what
//! they were. A served run executes nothing. A build `limits.seconds` cut
//! between two runs is kept with the files its next run reads, and the next
//! process goes on from there.
//! A finished build is kept with the files it left (`.aux`, `.toc`, `.bbl`):
//! once the document is edited (or `--stats` measures it), its next build starts from them, as latexmk
//! starts from the files on disk. It ends when a run writes what it read, the
//! same test as for a build from nothing, mostly after one run.

use crate::analysis::machine::{Analysis, BuildPending, Unfinished};
use crate::caches::format::{self, Fnv};
use crate::project::config::{Clock, Config};
use crate::project::overlay::Read;
use std::path::{Path, PathBuf};

/// The encoding of the files, apart from the interpreter's [`format::cache_key`].
const VERSION: u32 = 4;

/// What a listing shows of a file: it comes first, so a listing reads only the file's start.
#[derive(serde::Serialize, serde::Deserialize)]
struct Label {
    version: u32,
    /// The document's path, `None` for a text without a file.
    document: Option<PathBuf>,
}

/// A stored document: its path, its cache file, the file's bytes and when it was stored.
pub type Stored = (Option<PathBuf>, PathBuf, u64, Option<std::time::SystemTime>);

/// The documents `dir` holds an analysis of, the latest first.
pub fn stored(dir: &Path) -> Vec<Stored> {
    use std::io::Read as _;
    let mut found: Vec<Stored> = (std::fs::read_dir(dir).into_iter().flatten().flatten())
        .filter(|e| own_name(&e.file_name().to_string_lossy()))
        .filter_map(|e| {
            let mut start = Vec::new();
            std::fs::File::open(e.path()).ok()?.take(1 << 16).read_to_end(&mut start).ok()?;
            let (label, _): (Label, _) = postcard::take_from_bytes(&start).ok()?;
            let meta = e.metadata().ok()?;
            (label.version == VERSION).then(|| (label.document, e.path(), meta.len(), meta.modified().ok()))
        })
        .collect();
    found.sort_by(|a, b| (b.3, &a.1).cmp(&(a.3, &b.1)));
    found
}

fn own_name(name: &str) -> bool {
    name.starts_with("document-") && name.ends_with(".postcard")
}

/// `document-⟨file name⟩-⟨hash of the whole path⟩.postcard`: one per document, named so a listing can say which.
fn file_for(dir: &Path, absolute: Option<&Path>) -> PathBuf {
    let name = absolute.and_then(Path::file_name).map_or("text".into(), |n| n.to_string_lossy());
    let plain: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' }).collect();
    let mut hash = Fnv::new();
    hash.put(&absolute);
    dir.join(format!("document-{plain}-{:016x}.postcard", hash.0))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Head {
    version: u32,
    key: u64,
    /// Each tree's stamp ([`crate::caches::distribution::stamps_for`]).
    trees: Vec<(String, u64, u64)>,
    /// What the run read or looked for outside its own output.
    reads: Vec<Read>,
    /// Set for a build to go on with, not to serve.
    pending: Option<BuildPending>,
    /// The files a finished build left: where a build of the document starts once this one
    /// is no longer served (an edit), so it settles in one run where a build from nothing takes three.
    files: Vec<(PathBuf, String)>,
}

/// A run that stores its analysis once it ends.
pub(crate) struct Recording {
    file: PathBuf,
    key: u64,
    document: Option<PathBuf>,
}

type BuildFiles = Vec<(PathBuf, String)>;

pub(crate) enum Lookup {
    Served(Box<Analysis>),
    /// A run to make and store, on from a build cut between two runs if one is kept.
    Record(Recording, Option<Unfinished>),
    Off,
}

/// Environment variables the installation's trees and programs are found by.
fn environment() -> Vec<(String, String)> {
    let mut vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| k.starts_with("TEX") || k.starts_with("SATEX") || k.contains("KPSE") || k == "PATH")
        .collect();
    vars.sort();
    vars
}

/// The served analysis, or the recording a run makes, for `source` at `path`.
pub(crate) fn lookup(source: &str, path: Option<&Path>, cfg: &Config) -> Lookup {
    let hot = crate::analysis::machine::HOT.load(std::sync::atomic::Ordering::Relaxed);
    // `satex cache build` prepares packages; another run's files in the overlay
    // (`lint --fix`, the language server) are no input a key holds. `--stats`
    // measures a run, so nothing is served, but it builds from the last build's files.
    let off = !cfg.cache.enabled || !cfg.cache.document || cfg.cache.index.building || cfg.trace;
    if off || !crate::project::overlay::is_empty() {
        return Lookup::Off;
    }
    let Some(dir) = cfg.cache.dir.clone().or_else(format::default_cache_dir) else { return Lookup::Off };
    let absolute = path.map(|p| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
    let settings = serde_json::to_string(cfg).unwrap_or_default();
    let mut key = Fnv::new();
    key.put(&(VERSION, format::cache_key(), &absolute, std::env::current_dir().ok(), environment(), &settings));
    key.put(source);
    // The project around it: build files, plugin discovery, magic comments.
    key.put(&crate::analysis::machine::setup_key(cfg, source, path.unwrap_or(Path::new(""))));
    // One file per document: an edit or another setting replaces it.
    let file = file_for(&dir, absolute.as_deref());
    let mut pending = None;
    if !cfg.cache.rebuild && !cfg.cache.index.refresh {
        match serve(&file, key.0, cfg, hot) {
            Ok((analysis, None)) => {
                format::touch(&file);
                return Lookup::Served(Box::new(analysis));
            }
            Ok((mut analysis, Some(build))) => {
                if cfg.verbose >= 1 {
                    crate::util::progress::note!("satex: the build goes on after its run {}", build.run);
                }
                // The cut is the last process's: this one has its own time.
                analysis.limits.retain(|hit| hit.key != "seconds" || hit.at.line != 0);
                pending = Some((Some(Box::new(analysis)), build));
            }
            Err((why, files)) => {
                if cfg.verbose >= 1 {
                    let from = if files.is_empty() { "" } else { ", the build starts from the last one's files" };
                    crate::util::progress::note!("satex: document cache not used: {why}{from}");
                }
                if !files.is_empty() {
                    pending = Some((None, BuildPending { files, ..Default::default() }));
                }
            }
        }
    }
    if !crate::project::overlay::record() {
        return Lookup::Off;
    }
    Lookup::Record(Recording { file, key: key.0, document: absolute }, pending)
}

/// The stored analysis, or why it is not served (`measure`: `--stats`) and the build files it was stored with.
fn serve(
    file: &Path,
    key: u64,
    cfg: &Config,
    measure: bool,
) -> Result<(Analysis, Option<BuildPending>), (String, BuildFiles)> {
    let none = |why: String| (why, Vec::new());
    let bytes = std::fs::read(file).map_err(|_| none("none stored".into()))?;
    let (label, rest): (Label, &[u8]) = postcard::take_from_bytes(&bytes).map_err(|e| none(e.to_string()))?;
    if label.version != VERSION {
        return Err(none("stored by another version".into()));
    }
    let (head, rest): (Head, &[u8]) = postcard::take_from_bytes(rest).map_err(|e| none(e.to_string()))?;
    let roots: Vec<PathBuf> = head.trees.iter().map(|(root, ..)| PathBuf::from(root)).collect();
    let why = match () {
        _ if head.version != VERSION || head.key != key => Some("the text or the settings changed".to_string()),
        _ if crate::caches::distribution::stamps_for(&roots) != head.trees => Some("the installation changed".into()),
        _ => head.reads.iter().find(|r| !r.holds()).map(|read| format!("{} changed", read.path.display())),
    }
    .or_else(|| measure.then(|| "`--stats` measures a run".to_string()));
    if let Some(why) = why {
        return Err((why, head.pending.map_or(head.files, |p| p.files)));
    }
    let mut analysis: Analysis =
        crate::fixed::shared::with(|| postcard::from_bytes(rest)).map_err(|e| none(e.to_string()))?;
    analysis.served_steps = analysis.steps;
    analysis.cached_files = (0..analysis.files.len()).filter_map(|i| i.try_into().ok()).collect();
    analysis.settings.source = cfg.source.clone();
    analysis.package_caches = vec![("document".into(), file.display().to_string(), "cached".into())];
    // As a package cache that read the clock does under `clock: now`.
    if cfg.clock == Clock::Now
        && let Some(d) = analysis.facts.diagnostics.iter_mut().find(|d| d.code == "clock-assumed")
    {
        d.message.push_str(". the document cache keeps the date of the run that cached it");
    }
    Ok((analysis, head.pending))
}

/// What `analysis` stands on outside the overlay: the reads [`crate::project::overlay::record`]
/// logged, found and absent files alike, and every file it opened, those a package cache served
/// included. It stands while each [`Read::holds`].
pub(crate) fn reads_of(analysis: &Analysis, mut reads: Vec<Read>) -> Vec<Read> {
    for file in &analysis.files {
        let path = Path::new(&file.path);
        if path.is_file() && !crate::project::overlay::holds(path) {
            let print = crate::project::overlay::fingerprint(path);
            reads.push(Read { path: path.to_path_buf(), probe: false, own: false, print });
        }
    }
    reads.sort_by(|a, b| (&a.path, a.probe, a.print).cmp(&(&b.path, b.probe, b.print)));
    reads.dedup();
    reads
}

impl Drop for Recording {
    fn drop(&mut self) {
        crate::project::overlay::recorded();
    }
}

impl Recording {
    /// Stores `analysis`, and makes it the copy a later run is served: a map
    /// iterates in the order its decoding inserted, so both runs see the
    /// same. A run a budget stopped is stored too, unless it built caches:
    /// then the next run gets further in the same time.
    pub(crate) fn store(self, analysis: &mut Analysis) {
        let reads = reads_of(analysis, crate::project::overlay::recorded());
        let pending = analysis.build_pending.take().map(|p| *p);
        // A run `limits.memory` stopped is not encoded: that holds it twice, past the limit.
        if analysis.stopped() && pending.is_none() && analysis.caches_built() > 0
            || analysis.limits.iter().any(|l| l.key == "memory")
            // What a run reads past a stack cut depends on its thread, not on its key.
            || analysis.facts.diagnostics.iter().any(|d| d.code == "nesting-widened")
        {
            return;
        }
        let head = Head {
            version: VERSION,
            key: self.key,
            trees: crate::caches::distribution::stamps_for(&analysis.distribution.roots),
            reads,
            pending: pending.clone(),
            files: std::mem::take(&mut analysis.build_files),
        };
        let label = Label { version: VERSION, document: self.document.clone() };
        let (Ok(mut bytes), Ok(head)) = (postcard::to_allocvec(&label), postcard::to_allocvec(&head)) else { return };
        bytes.extend_from_slice(&head);
        // The body is encoded in place: one encoded copy beside the analysis, not two.
        let start = bytes.len();
        let Ok(bytes) = crate::fixed::shared::with(|| postcard::to_extend(&*analysis, bytes)) else { return };
        let Ok(mut copy) = crate::fixed::shared::with(|| postcard::from_bytes::<Analysis>(&bytes[start..])) else {
            return;
        };
        let temporary = format::temporary_for(&self.file);
        if self.file.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok())
            && std::fs::write(&temporary, &bytes).is_ok()
            && std::fs::rename(&temporary, &self.file).is_ok()
        {
            // Files named by the settings too (before one file per document) are never read again.
            let legacy = |name: &str| own_name(name) && name.len() == "document-.postcard".len() + 16;
            for old in self.file.parent().and_then(|d| std::fs::read_dir(d).ok()).into_iter().flatten().flatten() {
                if legacy(&old.file_name().to_string_lossy()) {
                    let _ = std::fs::remove_file(old.path());
                }
            }
            // What the encoding leaves out is this run's own.
            copy.timings = std::mem::take(&mut analysis.timings);
            (copy.started, copy.hot) = (analysis.started, analysis.hot.take());
            copy.package_caches.push(("document".into(), self.file.display().to_string(), "stored".into()));
            *analysis = copy;
        }
        analysis.build_pending = pending.map(Box::new);
    }
}
