//! Document caches: a run's whole [`Analysis`], kept per document and
//! settings, and served again while the document's text, the settings, the
//! installation's trees and every file the run read or looked for are what
//! they were. A served run executes nothing.

use crate::analysis::machine::Analysis;
use crate::caches::format::{self, Fnv};
use crate::project::config::{Clock, Config};
use crate::project::overlay::Read;
use std::path::{Path, PathBuf};

/// The encoding of the files, apart from the interpreter's [`format::cache_key`].
const VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct Head {
    version: u32,
    key: u64,
    /// Each tree's stamp ([`crate::caches::distribution::stamps_for`]).
    trees: Vec<(String, u64, u64)>,
    /// What the run read or looked for outside its own output.
    reads: Vec<Read>,
}

/// A run that stores its analysis once it ends.
pub(crate) struct Recording {
    file: PathBuf,
    key: u64,
}

pub(crate) enum Lookup {
    Served(Box<Analysis>),
    Record(Recording),
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
    // (`lint --fix`, the language server) are no input a key holds.
    let off = !cfg.cache.enabled || !cfg.cache.document || cfg.cache.index.building || cfg.trace || hot;
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
    // One file per document and settings: an edit replaces it.
    let mut name = Fnv::new();
    name.put(&(&absolute, &settings, absolute.is_none().then_some(key.0)));
    let file = dir.join(format!("document-{:016x}.postcard", name.0));
    if !cfg.cache.rebuild && !cfg.cache.index.refresh {
        match serve(&file, key.0, cfg) {
            Ok(analysis) => {
                format::touch(&file);
                return Lookup::Served(Box::new(analysis));
            }
            Err(why) if cfg.verbose >= 1 => crate::util::progress::note!("satex: document cache not used: {why}"),
            Err(_) => {}
        }
    }
    if !crate::project::overlay::record() {
        return Lookup::Off;
    }
    Lookup::Record(Recording { file, key: key.0 })
}

fn serve(file: &Path, key: u64, cfg: &Config) -> Result<Analysis, String> {
    let bytes = std::fs::read(file).map_err(|_| "none stored".to_string())?;
    let (head, rest): (Head, &[u8]) = postcard::take_from_bytes(&bytes).map_err(|e| e.to_string())?;
    let roots: Vec<PathBuf> = head.trees.iter().map(|(root, ..)| PathBuf::from(root)).collect();
    if head.version != VERSION || head.key != key {
        return Err("the text or the settings changed".into());
    }
    if crate::caches::distribution::stamps_for(&roots) != head.trees {
        return Err("the installation changed".into());
    }
    if let Some(read) = head.reads.iter().find(|r| !r.holds()) {
        return Err(format!("{} changed", read.path.display()));
    }
    let mut analysis: Analysis =
        crate::fixed::shared::with(|| postcard::from_bytes(rest)).map_err(|e| e.to_string())?;
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
    Ok(analysis)
}

impl Drop for Recording {
    fn drop(&mut self) {
        crate::project::overlay::recorded();
    }
}

impl Recording {
    /// Stores `analysis` unless a budget that depends on the machine stopped
    /// it, and makes it the copy a later run is served: a map iterates in
    /// the order its decoding inserted, so both runs see the same.
    pub(crate) fn store(self, analysis: &mut Analysis) {
        let mut reads = crate::project::overlay::recorded();
        if analysis.exhausted || analysis.limits.iter().any(|l| matches!(l.key, "seconds" | "memory")) {
            return;
        }
        // Every file it opened, those a package cache served included.
        for file in &analysis.files {
            let path = Path::new(&file.path);
            if path.is_file() && !crate::project::overlay::holds(path) {
                let print = crate::project::overlay::fingerprint(path);
                reads.push(Read { path: path.to_path_buf(), probe: false, own: false, print });
            }
        }
        reads.sort_by(|a, b| (&a.path, a.probe, a.print).cmp(&(&b.path, b.probe, b.print)));
        reads.dedup();
        let head = Head {
            version: VERSION,
            key: self.key,
            trees: crate::caches::distribution::stamps_for(&analysis.distribution.roots),
            reads,
        };
        let body = crate::fixed::shared::with(|| postcard::to_allocvec(&*analysis));
        let (Ok(mut bytes), Ok(body)) = (postcard::to_allocvec(&head), body) else { return };
        let Ok(mut copy) = crate::fixed::shared::with(|| postcard::from_bytes::<Analysis>(&body)) else { return };
        bytes.extend_from_slice(&body);
        let temporary = format::temporary_for(&self.file);
        if self.file.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok())
            && std::fs::write(&temporary, &bytes).is_ok()
            && std::fs::rename(&temporary, &self.file).is_ok()
        {
            // What the encoding leaves out is this run's own.
            copy.timings = std::mem::take(&mut analysis.timings);
            (copy.started, copy.hot) = (analysis.started, analysis.hot.take());
            copy.package_caches.push(("document".into(), self.file.display().to_string(), "stored".into()));
            *analysis = copy;
        }
    }
}
