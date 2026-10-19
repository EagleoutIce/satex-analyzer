//! `satex cache export` and `import`: the valid caches of a cache directory
//! in one file, so a machine without the time or the TeX installation to
//! build them can use them.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::format::{HASHES_FILE, SEMANTIC_VERSION, cache_key, is_current_cache};

const MAGIC: &str = "satex-cache-archive";

/// The file a bundle directory keeps its caches in.
pub const BUNDLE_ARCHIVE: &str = "satex-cache.bin";

#[derive(Serialize, Deserialize)]
struct Archive {
    magic: String,
    semantic: u32,
    key: String,
    entries: Vec<(String, Vec<u8>)>,
}

/// A directory names its [`BUNDLE_ARCHIVE`].
fn archive_path(path: &Path) -> PathBuf {
    if path.is_dir() { path.join(BUNDLE_ARCHIVE) } else { path.to_path_buf() }
}

/// Whether a cache directory file belongs in an archive.
fn exportable(name: &str) -> bool {
    name == HASHES_FILE || (name.ends_with(".postcard") && is_current_cache(name))
}

pub fn export(directory: &Path, target: &Path, json: bool, out: &mut impl Write) -> Result<(), String> {
    let target = archive_path(target);
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if exportable(&name)
            && let Ok(bytes) = std::fs::read(entry.path())
        {
            entries.push((name, bytes));
        }
    }
    entries.sort();
    let count = entries.len();
    let archive = Archive { magic: MAGIC.into(), semantic: SEMANTIC_VERSION, key: cache_key().into(), entries };
    let bytes = postcard::to_allocvec(&archive).map_err(|e| e.to_string())?;
    std::fs::write(&target, &bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    if json {
        let value =
            serde_json::json!({ "archive": target.display().to_string(), "entries": count, "bytes": bytes.len() });
        let _ = writeln!(out, "{value}");
    } else {
        let _ = writeln!(
            out,
            "exported {count} cache file(s), {}, to {}",
            bytesize::ByteSize(bytes.len() as u64),
            target.display()
        );
    }
    Ok(())
}

pub fn import(directory: &Path, source: &Path, json: bool, out: &mut impl Write) -> Result<(), String> {
    let source = archive_path(source);
    let bytes = std::fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut skipped: Vec<String> = Vec::new();
    let mut imported = 0usize;
    match postcard::from_bytes::<Archive>(&bytes) {
        Err(e) => skipped.push(format!("{}: not a cache archive ({e})", source.display())),
        Ok(a) if a.magic != MAGIC => skipped.push(format!("{}: not a cache archive", source.display())),
        Ok(a) if a.semantic != SEMANTIC_VERSION || a.key != cache_key() => skipped.push(format!(
            "{}: built for semantic version {} (key {}), this build is {SEMANTIC_VERSION} (key {})",
            source.display(),
            a.semantic,
            a.key,
            cache_key()
        )),
        Ok(a) => {
            std::fs::create_dir_all(directory).map_err(|e| format!("{}: {e}", directory.display()))?;
            for (name, data) in a.entries {
                let target = directory.join(&name);
                let reason = if Path::new(&name).file_name().is_none_or(|n| n.to_string_lossy() != name) {
                    Some("not a plain file name".to_string())
                } else if !exportable(&name) {
                    Some("not a cache this build can use".to_string())
                } else if name == HASHES_FILE && target.exists() {
                    Some("this directory has its own".to_string())
                } else {
                    let temporary = crate::format::temporary_for(&target);
                    match std::fs::write(&temporary, data).and_then(|()| std::fs::rename(&temporary, &target)) {
                        Ok(()) => None,
                        Err(e) => {
                            let _ = std::fs::remove_file(&temporary);
                            Some(e.to_string())
                        }
                    }
                };
                match reason {
                    Some(reason) => skipped.push(format!("skipped {name}: {reason}")),
                    None => imported += 1,
                }
            }
        }
    }
    if json {
        let value = serde_json::json!({ "imported": imported, "skipped": skipped });
        let _ = writeln!(out, "{value}");
    } else {
        for line in &skipped {
            let _ =
                writeln!(out, "{}", if line.starts_with("skipped") { line.clone() } else { format!("skipped {line}") });
        }
        let _ = writeln!(out, "imported {imported} cache file(s) into {}", directory.display());
    }
    Ok(())
}
