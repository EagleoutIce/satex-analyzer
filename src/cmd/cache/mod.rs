//! `satex cache`: status of the kernel cache and the package caches, and the
//! `build`/`clear`/`prune` actions.

use std::io::Write;

use super::{Context, Format, Output};
use crate::config::Config;
use crate::render;

pub mod build;

/// The cache directory in effect: `cache_dir:`/`--set cache_dir=...`, or the
/// platform default.
fn directory(cfg: &Config) -> Option<std::path::PathBuf> {
    cfg.cache_dir.clone().or_else(crate::format::default_cache_dir)
}

/// Status of the kernel cache and the package caches: what `satex cache`
/// prints without a further action.
pub fn status(context: &Context, out: &mut impl Write) -> Result<Output, String> {
    let analysis = context.analysis;
    let directory = directory(&analysis.settings);
    let as_json = context.format == Format::Json;

    let Some(fmt) = &analysis.format else {
        return Err("no format was read; check `satex query distribution`".into());
    };
    let state = if fmt.cached { "cached" } else { "interpreted for this run" };
    if as_json {
        let json = serde_json::json!({
            "source": fmt.source,
            "definitions": fmt.definitions,
            "cached": fmt.cached,
            "state": state,
            "cache_dir": directory.as_ref().map(|d| d.display().to_string()),
            "preload": analysis.plugins.preload.as_ref().map(|preload| serde_json::json!({
                "name": preload.name,
                "how": preload.how,
                "source": preload.source.as_ref().map(|p| p.display().to_string()),
            })),
            "lines_read": fmt.reached,
            "lines_total": fmt.lines,
            "complete": fmt.complete(),
        });
        let _ = writeln!(out, "{}", serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?);
        return Ok(Output::Done);
    }

    let _ = writeln!(
        out,
        "{}\n  {} {}\n  {} {} definitions\n  {} {}\n  {} {}",
        render::heading("cache"),
        render::label("source"),
        fmt.source,
        render::label("holds"),
        fmt.definitions,
        render::label("state"),
        if fmt.cached { render::good("cached") } else { state.to_string() },
        render::label("cache"),
        directory.as_ref().map(|d| d.display().to_string()).unwrap_or_else(|| "disabled".into()),
    );
    if let Some(preload) = &analysis.plugins.preload {
        let _ = writeln!(
            out,
            "  {} {} ({}){}",
            render::label("preloaded"),
            preload.name,
            preload.how,
            match &preload.source {
                Some(path) => format!(" from {}", path.display()),
                None => ", no source found".to_string(),
            }
        );
    }
    if let Some(directory) = &directory {
        let pruned = crate::machine::prune_caches(directory, analysis.settings.limits.cache_size.as_u64());
        if pruned.files > 0 {
            let _ = writeln!(
                out,
                "  {} {} caches no build can use or beyond limits.cache_size, {}",
                render::label("pruned"),
                pruned.files,
                bytesize::ByteSize(pruned.bytes)
            );
        }
        let (count, bytes) = crate::format::package_caches(directory);
        let _ = writeln!(
            out,
            "  {} {count} stored, {} (`satex cache build` prepares {})",
            render::label("package caches"),
            bytesize::ByteSize(bytes),
            analysis.settings.cache_index.packages.join(", ")
        );
    }
    if fmt.lines > 0 {
        let _ = writeln!(
            out,
            "  {} read to line {} of {}",
            render::label(if fmt.complete() { "read" } else { "partial" }),
            fmt.reached,
            fmt.lines
        );
    }
    Ok(Output::Done)
}

/// `satex cache clear`: delete every cache satex keeps.
pub fn clear(cfg: &Config, json: bool, out: &mut impl Write) -> Result<(), String> {
    let Some(directory) = directory(cfg) else { return Err("no cache directory".into()) };
    let removed = crate::format::clear(&directory).map_err(|e| e.to_string())?;
    if json {
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "removed": removed,
            "directory": directory.display().to_string(),
        }))
        .map_err(|e| e.to_string())?;
        let _ = writeln!(out, "{text}");
    } else {
        let _ = writeln!(out, "removed {removed} cache file(s) from {}", directory.display());
    }
    Ok(())
}

/// `satex cache prune`: apply the size and staleness pruning now.
pub fn prune(cfg: &Config, json: bool, out: &mut impl Write) -> Result<(), String> {
    let Some(directory) = directory(cfg) else { return Err("no cache directory".into()) };
    let pruned = crate::machine::prune_caches(&directory, cfg.limits.cache_size.as_u64());
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
