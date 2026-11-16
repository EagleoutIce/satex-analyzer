//! `satex cache`: status of the kernel cache and the package caches, and the
//! `build`/`clear`/`prune` actions.

use std::io::Write;

use super::{Context, Format, Output};
use crate::cli::render;
use crate::project::config::Config;

pub mod archive;
pub mod background;
pub mod build;

/// The cache directory in effect: `cache.dir:`/`--set cache.dir=...`, or the
/// platform default.
fn directory(cfg: &Config) -> Option<std::path::PathBuf> {
    cfg.cache.dir.clone().or_else(crate::caches::format::default_cache_dir)
}

/// Status of the kernel cache and the package caches: what `satex cache`
/// prints without a further action.
pub fn status(context: &Context, out: &mut impl Write) -> Result<Output, String> {
    let analysis = context.analysis;
    let directory = directory(&analysis.settings);
    let as_json = context.format == Format::Json;

    let Some(fmt) = &analysis.format else {
        return Err("no format was read. Check `satex query distribution`".into());
    };
    let state = if fmt.cached { "cached" } else { "interpreted for this run" };
    if as_json {
        let json = serde_json::json!({
            "source": fmt.source,
            "definitions": fmt.definitions,
            "cached": fmt.cached,
            "state": state,
            "cache_dir": directory.as_ref().map(|d| d.display().to_string()),
            "key": crate::caches::format::cache_key(),
            "preload": analysis.plugins.preload.as_ref().map(|preload| serde_json::json!({
                "name": preload.name,
                "how": preload.how,
                "source": preload.source.as_ref().map(|p| p.display().to_string()),
            })),
            "background": directory.as_deref().map(|d| {
                let (state, last) = background::status(d);
                serde_json::json!({ "state": state, "last": last, "setting": analysis.settings.cache.background.as_str() })
            }),
            "lines_read": fmt.reached,
            "lines_total": fmt.lines,
            "complete": fmt.complete(),
        });
        let _ = writeln!(out, "{}", serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?);
        return Ok(Output::Done);
    }

    let row = |name: &str| render::label(&format!("{name:<11}"));
    let _ = writeln!(
        out,
        "{} {}\n  {}{} · {} definitions · {}",
        render::heading("cache"),
        directory.as_ref().map(|d| d.display().to_string()).unwrap_or_else(|| "disabled".into()),
        row("kernel"),
        fmt.source,
        crate::analysis::machine::compact_count(fmt.definitions as u64),
        if fmt.cached { render::good("cached") } else { state.to_string() },
    );
    if let Some(preload) = &analysis.plugins.preload {
        let _ = writeln!(
            out,
            "  {}{} ({}){}",
            row("preloaded"),
            preload.name,
            preload.how,
            match &preload.source {
                Some(path) => format!(" from {}", path.display()),
                None => ", no source found".to_string(),
            }
        );
    }
    if let Some(directory) = &directory {
        let pruned = crate::analysis::machine::prune_caches(directory, analysis.settings.limits.cache_size.as_u64());
        if pruned.files > 0 {
            let _ = writeln!(
                out,
                "  {}{} caches, {}: no build can use them, or past limits.cache_size",
                row("pruned"),
                pruned.files,
                bytesize::ByteSize(pruned.bytes)
            );
        }
        let (count, bytes) = crate::caches::format::package_caches(directory);
        let index = &analysis.settings.cache.index;
        let prepares: Vec<String> = if index.packages.is_empty() {
            let found = build::discover_expensive(directory, build::EXPENSIVE_BYTES);
            let found = found.into_iter().map(|(name, _)| name).filter(|name| !index.common.contains(name));
            index.common.iter().cloned().chain(found).collect()
        } else {
            index.packages.clone()
        };
        const SHOWN: usize = 6;
        let more = prepares.len().saturating_sub(SHOWN);
        let _ = writeln!(
            out,
            "  {}{count} stored, {}\n  {}`satex cache build` builds {} packages: {}{}",
            row("packages"),
            bytesize::ByteSize(bytes),
            row("prepare"),
            prepares.len(),
            prepares[..prepares.len().min(SHOWN)].join(", "),
            if more > 0 { format!(", … +{more}") } else { String::new() },
        );
        let (state, last) = background::status(directory);
        let _ = writeln!(
            out,
            "  {}{state} · cache.background: {}{}",
            row("background"),
            analysis.settings.cache.background.as_str(),
            last.map(|l| format!(", {l}")).unwrap_or_default()
        );
    }
    if fmt.lines > 0 {
        let _ = writeln!(
            out,
            "  {}to line {} of {}",
            row(if fmt.complete() { "read" } else { "partial" }),
            fmt.reached,
            fmt.lines
        );
    }
    Ok(Output::Done)
}

/// `satex cache export`: write the valid caches to `target`.
pub fn export(
    cfg: &Config,
    target: &std::path::Path,
    filter: (&[String], &[String]),
    json: bool,
    out: &mut impl Write,
) -> Result<(), String> {
    let Some(directory) = directory(cfg) else { return Err("no cache directory".into()) };
    archive::export(&directory, target, filter, json, out)
}

/// `satex cache import`: unpack an archive into the cache directory.
pub fn import(cfg: &Config, source: &std::path::Path, json: bool, out: &mut impl Write) -> Result<(), String> {
    let Some(directory) = directory(cfg) else { return Err("no cache directory".into()) };
    archive::import(&directory, source, json, out)
}

/// `satex cache clear`: delete every cache satex keeps.
pub fn clear(cfg: &Config, json: bool, out: &mut impl Write) -> Result<(), String> {
    let Some(directory) = directory(cfg) else { return Err("no cache directory".into()) };
    // What the directory held before, so the report can say what was freed.
    let size = |dir: &std::path::Path| -> u64 {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
            })
            .unwrap_or(0)
    };
    let before = size(&directory);
    let removed = crate::caches::format::clear(&directory).map_err(|e| e.to_string())?;
    let freed = before.saturating_sub(size(&directory));
    if json {
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "removed": removed,
            "freed_bytes": freed,
            "directory": directory.display().to_string(),
        }))
        .map_err(|e| e.to_string())?;
        let _ = writeln!(out, "{text}");
    } else {
        let _ = writeln!(
            out,
            "removed {removed} cache file(s), {} freed, from {}",
            bytesize::ByteSize(freed),
            directory.display()
        );
    }
    Ok(())
}

/// `satex cache stop`: end the background builder after its current package.
/// The parts of a segment's effect, as [`crate::analysis::machine::TreeSize::sections`] counts them.
const SECTIONS: [&str; 5] = ["rest", "vertices", "edges", "definitions", "expansions"];

/// `satex cache show`: each package's cache files, largest first, with their variants.
pub fn show(cfg: &Config, packages: &[String], json: bool, out: &mut impl Write) -> Result<(), String> {
    let Some(directory) = directory(cfg) else { return Err("no cache directory".into()) };
    let mut files: Vec<(String, std::path::PathBuf, u64)> = std::fs::read_dir(&directory)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stem = name.strip_prefix("package-")?.strip_suffix(".postcard")?;
            // `package-<name>-v<version>-…`: the name is what stands before `-v<digit>`
            let at = stem.match_indices("-v").find(|(i, _)| stem[i + 2..].starts_with(|c: char| c.is_ascii_digit()))?.0;
            let package = &stem[..at];
            (packages.is_empty() || packages.iter().any(|p| p == package))
                .then(|| (package.to_string(), e.path(), e.metadata().map_or(0, |m| m.len())))
        })
        .collect();
    files.sort_by(|a, b| b.2.cmp(&a.2));
    let mut rows = Vec::new();
    for (package, path, bytes) in &files {
        let trees = std::fs::read(path).ok().and_then(|b| crate::analysis::machine::tree_sizes(&b)).unwrap_or_default();
        if json {
            rows.push(serde_json::json!({
                "package": package, "file": path.display().to_string(), "bytes": bytes,
                "variants": trees.iter().map(|t| serde_json::json!({
                    "segments": t.segments, "steps": t.steps, "reads": t.reads, "effect": t.effect,
                    "sections": SECTIONS.iter().zip(t.sections).map(|(k, n)| (k.to_string(), n.into())).collect::<serde_json::Map<_, _>>(),
                    "root": t.root.iter().map(|(k, h)| format!("{k}:{h:016x}")).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            }));
            continue;
        }
        let _ = writeln!(
            out,
            "{} {} · {} variants · {}",
            render::heading(package),
            bytesize::ByteSize(*bytes),
            trees.len(),
            render::label(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
        );
        for (i, t) in trees.iter().enumerate() {
            // root parts that differ from the first variant
            let differs: Vec<&str> = t
                .root
                .iter()
                .filter(|(k, h)| trees[0].root.iter().find(|(k0, _)| k0 == k).is_none_or(|(_, h0)| h0 != h))
                .map(|(k, _)| k.as_str())
                .collect();
            // parts no segment depended on: any value is served
            let open: Vec<&str> = t.root.iter().filter(|(_, h)| *h == 0).map(|(k, _)| k.as_str()).collect();
            let _ = writeln!(
                out,
                "  {}{} segments · {} steps · reads {} · effect {} ({}){}{}",
                render::label(&format!("{:<4}", i + 1)),
                t.segments,
                crate::analysis::machine::compact_count(t.steps),
                bytesize::ByteSize(t.reads as u64),
                bytesize::ByteSize(t.effect as u64),
                SECTIONS
                    .iter()
                    .zip(t.sections)
                    .map(|(k, n)| format!("{k} {}", bytesize::ByteSize(n as u64)))
                    .collect::<Vec<_>>()
                    .join(", "),
                if i > 0 && !differs.is_empty() {
                    format!(" · differs in {}", differs.join(", "))
                } else {
                    String::new()
                },
                if open.is_empty() { String::new() } else { format!(" · any {}", open.join(", ")) }
            );
        }
    }
    // `document`, or no name at all: the documents an analysis is stored of.
    let documents = match packages.is_empty() || packages.iter().any(|p| p == "document") {
        true => crate::caches::document::stored(&directory),
        false => Vec::new(),
    };
    for (document, file, bytes, at) in &documents {
        let name = document.as_ref().map_or("a text without a file".to_string(), |p| p.display().to_string());
        let age = at.and_then(|at| at.elapsed().ok()).map(|d| d.as_secs());
        if json {
            rows.push(serde_json::json!({
                "document": document, "file": file.display().to_string(), "bytes": bytes, "age_seconds": age,
            }));
            continue;
        }
        let short = document.as_ref().and_then(|p| p.file_name()).map_or(name.clone(), |n| n.to_string_lossy().into());
        let ago = match age {
            Some(s) if s < 120 => format!("{s} s ago"),
            Some(s) if s < 7200 => format!("{} min ago", s / 60),
            Some(s) if s < 172_800 => format!("{} h ago", s / 3600),
            Some(s) => format!("{} days ago", s / 86_400),
            None => String::new(),
        };
        let _ = writeln!(
            out,
            "{} {} · {ago} · {}",
            render::heading(&format!("document {short}")),
            bytesize::ByteSize(*bytes),
            render::label(&name)
        );
    }
    if json {
        let _ = writeln!(out, "{}", serde_json::Value::Array(rows));
    } else if files.is_empty() && documents.is_empty() {
        let _ = match packages.is_empty() {
            true => writeln!(out, "no package caches in {}", directory.display()),
            false => writeln!(out, "no cache file for {} in {}", packages.join(", "), directory.display()),
        };
    }
    Ok(())
}

pub fn stop(cfg: &Config, json: bool, out: &mut impl Write) -> Result<(), String> {
    let Some(directory) = directory(cfg) else { return Err("no cache directory".into()) };
    let running = background::stop(&directory)?;
    if json {
        let _ = writeln!(
            out,
            "{}",
            serde_json::json!({ "running": running, "directory": directory.display().to_string() })
        );
    } else {
        let _ = writeln!(
            out,
            "{} {}. `satex cache build --background` resumes it",
            render::label("background"),
            if running { "stops after the package it is on" } else { "was not running, and will not start" }
        );
    }
    Ok(())
}

/// `satex cache prune`: apply the size and staleness pruning now.
pub fn prune(cfg: &Config, json: bool, out: &mut impl Write) -> Result<(), String> {
    let Some(directory) = directory(cfg) else { return Err("no cache directory".into()) };
    let pruned = crate::analysis::machine::prune_caches(&directory, cfg.limits.cache_size.as_u64());
    if json {
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "removed": pruned.files,
            "bytes": pruned.bytes,
            "directory": directory.display().to_string(),
        }))
        .map_err(|e| e.to_string())?;
        let _ = writeln!(out, "{text}");
    } else {
        let _ = writeln!(
            out,
            "{} {} cache file(s), {}, from {}",
            render::label("pruned"),
            pruned.files,
            bytesize::ByteSize(pruned.bytes),
            directory.display()
        );
    }
    Ok(())
}
