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

/// A package is worth pre-warming once ordinary runs on this machine have
/// left at least this much cache behind it: what a package's stored
/// segments weigh tracks what it costs to hold and replay them, and on this
/// project's own cache directory this cut falls cleanly between routine
/// packages (graphicx, article: under half a megabyte) and the ones whose
/// first load noticeably stalls an interactive run (amsmath, hyperref,
/// xcolor's own load site: several megabytes). Below it, `cache.index.auto`
/// catching a package on first use is good enough; `cache build` exists for
/// the packages where that first use is itself the problem.
pub(crate) const EXPENSIVE_BYTES: u64 = 1_000_000;

/// The package name a package cache's file name was built from (see
/// `package_cache::family`/`cache_file`): the sanitized name between
/// `package-` and the first `-v<digits>-` marker.
fn package_name_of(file_name: &str) -> Option<&str> {
    let rest = file_name.strip_prefix("package-")?;
    let rest = rest.strip_suffix(".postcard")?;
    let mut at = 0;
    while let Some(found) = rest[at..].find("-v") {
        let start = at + found;
        let digits = rest[start + 2..].bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest.as_bytes().get(start + 2 + digits) == Some(&b'-') {
            return Some(&rest[..start]);
        }
        at = start + 2;
    }
    None
}

/// What `cache build` prepares when nothing is configured or named: the
/// packages whose cache files in `dir` already total at least `threshold`
/// bytes, most expensive first. A measurement of what ordinary runs
/// (`cache.index.auto`) already found worth storing on this machine, not a
/// list of names; a fresh cache directory has nothing to discover, which is
/// the honest answer until some document has loaded a package for real.
pub(crate) fn discover_expensive(dir: &Path, threshold: u64) -> Vec<(String, u64)> {
    let mut totals: crate::fixed::HashMap<String, u64> = crate::fixed::HashMap::default();
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else { continue };
        let Some(package) = package_name_of(file_name) else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        *totals.entry(package.to_string()).or_default() += meta.len();
    }
    let mut found: Vec<(String, u64)> = totals.into_iter().filter(|(_, bytes)| *bytes >= threshold).collect();
    found.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    found
}

/// Analyze one package in its canonical preamble, on the calling thread.
fn prepare_one(run: &Config, base: &Path, directory: &Path, package: &str) -> Prepared {
    let started = Instant::now();
    let source = preamble_of(&run.cache.index.class, package);
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
/// `cache.index`'s configured ones when `packages` is empty — for every
/// given or configured engine.  A package cache is only rebuilt when it is
/// missing or stale, unless `refresh` rebuilds all of them.  `preamble`
/// builds a format cache from that file on top of the kernel instead of any
/// packages, the way `mylatexformat` dumps one.  `progress` hears the plan
/// and each item as it is done.
/// The stack of a thread that interprets the kernel: the main thread's.
const KERNEL_STACK: usize = 8 << 20;

/// What building one engine's kernel came to.
fn kernel_outcome(kernel: &crate::machine::Analysis, seconds: f64) -> Prepared {
    let (outcome, reason) = match &kernel.format {
        Some(f) if f.cached => ("fresh", None),
        Some(f) if f.complete() && !kernel.facts.diagnostics.iter().any(|d| d.code == "format-truncated") => {
            ("built", None)
        }
        Some(_) => ("failed", Some("latex.ltx was not read to the end".to_string())),
        None => ("failed", Some("no latex.ltx was found".to_string())),
    };
    Prepared {
        engine: kernel.plugins.engine.as_str(),
        package: "kernel".into(),
        outcome,
        reason,
        state: String::new(),
        seconds,
        bytes: 0,
    }
}

pub fn prepare(
    cfg: &Config,
    refresh: bool,
    packages: &[String],
    engines: &[Engine],
    preamble: Option<&Path>,
    progress: &mut dyn FnMut(&str),
) -> Result<Vec<Prepared>, String> {
    let directory = cfg.cache.dir.clone().or_else(crate::format::default_cache_dir).ok_or("no cache directory")?;
    let base = directory.join("index");
    std::fs::create_dir_all(&base).map_err(|e| format!("{}: {e}", base.display()))?;
    // Nothing configured or named: prepare what ordinary runs on this
    // machine (`cache.index.auto`) already found expensive, discovered from
    // their caches rather than a list of names. A fresh cache directory has
    // nothing to discover yet; `cache build` then has nothing to warm until
    // some document has loaded a package for real.
    let discovered;
    let packages: &[String] = match preamble {
        Some(_) => &[],
        None if packages.is_empty() && cfg.cache.index.packages.is_empty() => {
            let found = discover_expensive(&directory, EXPENSIVE_BYTES);
            if found.is_empty() {
                progress(
                    "nothing configured and nothing measured yet: run satex over your documents \
                     first (cache.index.auto stores what they load), then `cache build` will warm \
                     what turned out to be expensive",
                );
            } else {
                progress(&format!(
                    "discovered from what ran here already, at least {} each: {}",
                    bytesize::ByteSize(EXPENSIVE_BYTES),
                    found
                        .iter()
                        .map(|(name, bytes)| format!("{name} ({})", bytesize::ByteSize(*bytes)))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            discovered = found.into_iter().map(|(name, _)| name).collect::<Vec<_>>();
            &discovered[..]
        }
        None if packages.is_empty() => &cfg.cache.index.packages[..],
        None => packages,
    };
    let engines: Vec<Option<Engine>> = match (engines, &cfg.cache.index.engines[..]) {
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
    let runs: Vec<Config> = engines
        .iter()
        .map(|engine| {
            let mut run = cfg.clone();
            run.engine = engine.or(cfg.engine);
            run.cache.dir = Some(directory.clone());
            run.cache.index.building = true;
            run.cache.index.refresh = refresh;
            if let Some(preamble) = preamble {
                run.preload = Some(preamble.to_path_buf());
            }
            run
        })
        .collect();
    let mut prepared = Vec::new();
    let mut done = 0;
    // The kernels first, one thread per engine: they share nothing. Each
    // replaces its cache, since a run that interprets `latex.ltx` leaves a
    // state that differs from one that installs its cache, so packages read
    // in it would be keyed for no later run. Then the class every package
    // is prepared after, so the workers find it cached.
    let class = format!("\\documentclass{{{}}}\n\\begin{{document}}\n\\end{{document}}\n", cfg.cache.index.class);
    let (send, receive) = std::sync::mpsc::channel::<Prepared>();
    std::thread::scope(|scope| {
        for run in &runs {
            let (base, class, send) = (&base, &class, send.clone());
            std::thread::Builder::new()
                .stack_size(KERNEL_STACK)
                .spawn_scoped(scope, move || {
                    let mut rebuild = run.clone();
                    rebuild.cache.rebuild = true;
                    let started = Instant::now();
                    let kernel = Machine::analyze("", Some(&base.join("kernel.tex")), &rebuild);
                    if !packages.is_empty() {
                        let _ = Machine::analyze(class, Some(&base.join("class.tex")), run);
                    }
                    let _ = send.send(kernel_outcome(&kernel, started.elapsed().as_secs_f64()));
                })
                .expect("a kernel thread");
        }
        drop(send);
        // Each kernel is reported as it finishes.
        for entry in receive {
            done += 1;
            progress(&format!(
                "[{done}/{total}] kernel ({}) … {}{} in {:.1} s",
                entry.engine,
                entry.outcome,
                entry.reason.as_ref().map(|r| format!(": {r}")).unwrap_or_default(),
                entry.seconds
            ));
            prepared.push(entry);
        }
    });
    if packages.is_empty() {
        return Ok(prepared);
    }
    // Every package of every engine from one pool of workers.
    let items: Vec<(&Config, &String)> = runs.iter().flat_map(|run| packages.iter().map(move |p| (run, p))).collect();
    let workers = workers(cfg).min(items.len()).max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (send, receive) = std::sync::mpsc::channel::<Prepared>();
    let (base, directory, items) = (&base, &directory, &items);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let send = send.clone();
            let next = &next;
            std::thread::Builder::new()
                .stack_size(KERNEL_STACK)
                .spawn_scoped(scope, move || {
                    while let Some((run, package)) = items.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                    {
                        let _ = send.send(prepare_one(run, base, directory, package));
                    }
                })
                .expect("a package thread");
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
            crate::progress::note!("{line}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_name_is_read_back_out_of_its_cache_file_name() {
        assert_eq!(package_name_of("package-xcolor-v1-a1b2c3d4e5f6a7b8-0123456789abcdef.postcard"), Some("xcolor"));
        assert_eq!(package_name_of("package-koma-script-v1-abcd1234-1234abcd.postcard"), Some("koma-script"));
        assert_eq!(package_name_of("format-pdftex-2026-v1-abcd.postcard"), None, "not a package cache");
        assert_eq!(package_name_of("package-xcolor-v1-abcd.lock"), None, "a lock file, not a cache");
    }

    #[test]
    fn discovery_finds_only_what_exceeds_the_threshold_and_sums_a_packages_files() {
        let dir = std::env::temp_dir().join(format!("satex-cache-discover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        // Two cache files for the same package (different resolved paths or
        // engines) add up.
        std::fs::write(dir.join("package-big-v1-aaaa-1111.postcard"), vec![0u8; 600]).unwrap();
        std::fs::write(dir.join("package-big-v1-aaaa-2222.postcard"), vec![0u8; 600]).unwrap();
        std::fs::write(dir.join("package-small-v1-bbbb-3333.postcard"), vec![0u8; 100]).unwrap();
        std::fs::write(dir.join("package-small-v1-bbbb-3333.lock"), vec![]).unwrap();

        let found = discover_expensive(&dir, 1000);
        assert_eq!(found, vec![("big".to_string(), 1200)], "only the package over the threshold, summed: {found:?}");

        let none = discover_expensive(&dir, 10_000);
        assert!(none.is_empty(), "nothing on a fresh or light machine: {none:?}");
    }
}
