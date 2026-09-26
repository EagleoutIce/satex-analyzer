//! `satex cache build`: interpret the kernel, replacing its cache, and the
//! configured or named packages once, so that later runs start from their
//! caches.  `--preamble` builds a format cache from a preamble file instead.

use std::io::Write;
use std::path::Path;
use std::time::Instant;

use crate::config::{Config, Engine};
use crate::machine::Machine;
use crate::render;

/// What one item came to: the kernel or a package, for one engine.
pub struct Prepared {
    pub engine: &'static str,
    pub package: String,
    /// `fresh`, `built`, `rebuilt` or `failed`.
    pub outcome: &'static str,
    /// Why a cache was rejected or could not be stored.
    pub reason: Option<String>,
    /// The package cache's state as the run reported it.
    pub state: String,
    pub seconds: f64,
    pub bytes: u64,
}

/// The preamble a package cache is prepared in: the configured class and
/// nothing else before the package.
fn preamble_of(class: &str, package: &str) -> String {
    format!("\\documentclass{{{class}}}\n\\usepackage{{{package}}}\n\\begin{{document}}\n\\end{{document}}\n")
}

/// The package caches a directory holds for `package`, and their size.
fn stored(directory: &Path, package: &str) -> (usize, u64) {
    let prefix = format!("package-{package}-");
    let Ok(entries) = std::fs::read_dir(directory) else { return (0, 0) };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .filter_map(|e| e.metadata().ok())
        .fold((0, 0), |(n, bytes), meta| (n + 1, bytes + meta.len()))
}

/// What a run's reports for `package` say happened to its cache.
fn outcome_of(states: &[&str], exhausted: bool) -> (&'static str, Option<String>) {
    let stale = states
        .iter()
        .find_map(|s| {
            let partial = s.strip_suffix("), rest stored").and_then(|s| s.split_once(" (")).map(|(_, why)| why);
            s.strip_prefix("stale: ").or(partial)
        })
        .map(str::to_string);
    let failed = states.iter().find_map(|s| s.strip_prefix("not stored: ")).map(str::to_string);
    if states.contains(&"cached") {
        ("fresh", None)
    } else if states.iter().any(|s| s.ends_with("rest stored")) {
        ("rebuilt", Some(stale.unwrap_or_else(|| "part of it no longer fits".into())))
    } else if let Some(reason) = failed {
        ("failed", Some(reason))
    } else if states.contains(&"stored") {
        match stale {
            Some(reason) => ("rebuilt", Some(reason)),
            None => ("built", None),
        }
    } else if exhausted {
        ("failed", Some("the analysis ran out of budget".into()))
    } else {
        ("failed", Some("it is not loaded where a cache can serve it".into()))
    }
}

/// How many packages are prepared at once: `limits.threads`, or every
/// processor when it is 0, but no more than `limits.memory` holds at
/// [`MEMORY_PER_WORKER`] each, since the budget counts the whole process.
fn workers(cfg: &Config) -> usize {
    let threads = match cfg.limits.threads {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get()),
        n => n,
    };
    let memory = cfg.limits.memory.as_u64();
    let fit = if memory == 0 { threads } else { (memory / MEMORY_PER_WORKER).max(1) as usize };
    threads.min(fit).max(1)
}

/// What one package's analysis is allowed to hold, when several share
/// `limits.memory`.
const MEMORY_PER_WORKER: u64 = 1 << 30;

/// Analyze one package in its canonical preamble, on the calling thread.
fn prepare_one(run: &Config, base: &Path, directory: &Path, package: &str) -> Prepared {
    let started = Instant::now();
    let source = preamble_of(&run.cache_index.class, package);
    let safe: String = package.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let path = base.join(format!("cache-index-{safe}.tex"));
    if let Err(e) = std::fs::write(&path, &source) {
        return Prepared {
            engine: "",
            package: package.to_string(),
            outcome: "failed",
            reason: Some(format!("{}: {e}", path.display())),
            state: String::new(),
            seconds: 0.0,
            bytes: 0,
        };
    }
    let analysis = Machine::analyze(&source, Some(&path), run);
    let states: Vec<&str> =
        analysis.package_caches.iter().filter(|(name, ..)| name == package).map(|(.., state)| state.as_str()).collect();
    let (outcome, reason) = outcome_of(&states, analysis.exhausted);
    let (_, bytes) = stored(directory, package);
    Prepared {
        engine: analysis.plugins.engine.as_str(),
        package: package.to_string(),
        outcome,
        reason,
        state: states.join(", then "),
        seconds: started.elapsed().as_secs_f64(),
        bytes,
    }
}

/// Interpret the kernel, replacing its cache, and the given packages — or
/// `cache_index`'s configured ones when `packages` is empty — for every
/// given or configured engine.  A package cache is only rebuilt when it is
/// missing or stale, unless `refresh` rebuilds all of them.  `preamble`
/// builds a format cache from that file on top of the kernel instead of any
/// packages, the way `mylatexformat` dumps one.  `progress` hears the plan
/// and each item as it is done.
pub fn prepare(
    cfg: &Config,
    refresh: bool,
    packages: &[String],
    engines: &[Engine],
    preamble: Option<&Path>,
    progress: &mut dyn FnMut(&str),
) -> Result<Vec<Prepared>, String> {
    let directory = cfg.cache_dir.clone().or_else(crate::format::default_cache_dir).ok_or("no cache directory")?;
    let base = directory.join("index");
    std::fs::create_dir_all(&base).map_err(|e| format!("{}: {e}", base.display()))?;
    let packages: &[String] = match preamble {
        Some(_) => &[],
        None if packages.is_empty() => &cfg.cache_index.packages[..],
        None => packages,
    };
    let engines: Vec<Option<Engine>> = match (engines, &cfg.cache_index.engines[..]) {
        ([], []) => vec![cfg.engine],
        ([], configured) => configured.iter().copied().map(Some).collect(),
        (given, _) => given.iter().copied().map(Some).collect(),
    };
    let total = engines.len() * (packages.len() + 1);
    let name_of = |engine: Option<Engine>| engine.map_or("the engine in effect", |e| e.as_str());
    progress(&format!(
        "plan: the kernel{} and {} package(s) for {} engine(s), {total} item(s), into {}",
        preamble.map(|p| format!(" with preamble {}", p.display())).unwrap_or_default(),
        packages.len(),
        engines.len(),
        directory.display()
    ));
    for engine in &engines {
        for package in packages {
            let (count, _) = stored(&directory, package);
            let state = match (refresh, count) {
                (true, _) => "rebuild",
                (false, 0) => "missing",
                (false, _) => "cached, checked when reached",
            };
            progress(&format!("  {package} ({}): {state}", name_of(*engine)));
        }
    }
    let mut prepared = Vec::new();
    let mut done = 0;
    for engine in engines {
        let mut run = cfg.clone();
        run.engine = engine.or(cfg.engine);
        run.cache_dir = Some(directory.clone());
        run.cache_index.building = true;
        run.cache_index.refresh = refresh;
        if let Some(preamble) = preamble {
            run.preload = Some(preamble.to_path_buf());
        }
        // The kernel first, always replacing its cache: a run that
        // interprets `latex.ltx` leaves a state that differs from one that
        // installs its cache, so packages read in it would be keyed for no
        // later run.
        run.rebuild_format = true;
        let started = Instant::now();
        let kernel = Machine::analyze("", Some(&base.join("kernel.tex")), &run);
        run.rebuild_format = false;
        done += 1;
        let (outcome, reason) = match &kernel.format {
            Some(f) if f.cached => ("fresh", None),
            Some(f) if f.complete() && !kernel.facts.diagnostics.iter().any(|d| d.code == "format-truncated") => {
                ("built", None)
            }
            Some(_) => ("failed", Some("latex.ltx was not read to the end".to_string())),
            None => ("failed", Some("no latex.ltx was found".to_string())),
        };
        let engine_name = kernel.plugins.engine.as_str();
        progress(&format!(
            "[{done}/{total}] kernel ({engine_name}) … {outcome}{} in {:.1} s",
            reason.as_ref().map(|r| format!(": {r}")).unwrap_or_default(),
            started.elapsed().as_secs_f64()
        ));
        prepared.push(Prepared {
            engine: engine_name,
            package: "kernel".into(),
            outcome,
            reason,
            state: String::new(),
            seconds: started.elapsed().as_secs_f64(),
            bytes: 0,
        });
        if packages.is_empty() {
            continue;
        }
        // The class every package is prepared after, once, so that the
        // workers find it cached instead of each storing it.
        let class = format!("\\documentclass{{{}}}\n\\begin{{document}}\n\\end{{document}}\n", cfg.cache_index.class);
        let _ = Machine::analyze(&class, Some(&base.join("class.tex")), &run);
        let workers = workers(cfg).min(packages.len()).max(1);
        let next = std::sync::atomic::AtomicUsize::new(0);
        let (send, receive) = std::sync::mpsc::channel::<Prepared>();
        let run = &run;
        let base = &base;
        let directory = &directory;
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let send = send.clone();
                let next = &next;
                scope.spawn(move || {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(package) = packages.get(i) else { break };
                        let _ = send.send(prepare_one(run, base, directory, package));
                    }
                });
            }
            drop(send);
            for entry in receive {
                done += 1;
                let seconds = entry.seconds;
                let detail = match (entry.outcome, &entry.reason) {
                    ("fresh", _) => "fresh, skipped".to_string(),
                    ("built", _) => format!("built in {seconds:.1} s, {}", bytesize::ByteSize(entry.bytes)),
                    ("rebuilt", Some(r)) => {
                        format!("rejected: {r}; rebuilt in {seconds:.1} s, {}", bytesize::ByteSize(entry.bytes))
                    }
                    (_, Some(r)) => format!("failed: {r}"),
                    (outcome, None) => outcome.to_string(),
                };
                progress(&format!("[{done}/{total}] {} ({}) … {detail}", entry.package, entry.engine));
                prepared.push(entry);
            }
        });
    }
    Ok(prepared)
}

pub fn run(
    cfg: &Config,
    refresh: bool,
    packages: &[String],
    engines: &[Engine],
    preamble: Option<&Path>,
    json: bool,
    out: &mut impl Write,
) -> Result<(), String> {
    let started = Instant::now();
    let mut progress = |line: &str| {
        if !json {
            anstream::eprintln!("{line}");
        }
    };
    let prepared = prepare(cfg, refresh, packages, engines, preamble, &mut progress)?;
    let count = |outcome: &str| prepared.iter().filter(|p| p.outcome == outcome).count();
    let bytes: u64 = prepared.iter().map(|p| p.bytes).sum();
    if json {
        let items: Vec<serde_json::Value> = prepared
            .iter()
            .map(|p| {
                serde_json::json!({
                    "engine": p.engine,
                    "item": p.package,
                    "outcome": p.outcome,
                    "reason": p.reason,
                    "seconds": p.seconds,
                    "bytes": p.bytes,
                })
            })
            .collect();
        let summary = serde_json::json!({
            "items": items,
            "built": count("built") + count("rebuilt"),
            "fresh": count("fresh"),
            "failed": count("failed"),
            "bytes": bytes,
            "seconds": started.elapsed().as_secs_f64(),
        });
        let _ = writeln!(out, "{}", serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?);
        return Ok(());
    }
    let _ = writeln!(
        out,
        "{} {} built, {} fresh, {} failed; {} of package caches; {:.1} s",
        render::label("cache build"),
        count("built") + count("rebuilt"),
        count("fresh"),
        count("failed"),
        bytesize::ByteSize(bytes),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
