//! Format cache: interpret `latex.ltx` once, cache for reuse.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::builtins::LoadKind;
use crate::env::{Binding, Env};
use crate::facts::{Diagnostic, Severity};
use crate::machine::Machine;
use crate::tex::{Interner, Meaning, Span, Sym};
use crate::value::Value;

/// Cache version: different encodings get different files.
const ENCODING_VERSION: u32 = 3;

/// [`Severity`] by index, so a cache stores a byte and not a name.
const SEVERITIES: [Severity; 5] =
    [Severity::Info, Severity::Warning, Severity::Imprecision, Severity::Unsupported, Severity::Assumption];

/// One diagnostic the kernel build raised: severity index, code, message,
/// span and how often it was raised.
type CachedDiagnostic = (u8, String, String, Span, u32);

/// A cached LaTeX2e kernel: the [`Meaning`]s and catcodes `latex.ltx` leaves
/// behind, captured once so later runs skip interpreting it again. The
/// interpreter reads and writes it while it starts a run.
#[derive(Serialize, Deserialize)]
pub struct Format {
    version: u32,
    source: String,
    size: u64,
    modified: u64,
    names: Vec<String>,
    bindings: Vec<(u32, Meaning, Value)>,
    catcodes: Vec<(char, u8)>,
    /// The files the kernel was read from, in the order they were read.  The
    /// spans inside the cached macro bodies carry those file numbers, so a
    /// run that installs this cache has to hand out the same numbers again or
    /// every kernel definition would point at whatever file took the number.
    files: Vec<(String, LoadKind)>,
    /// Where the kernel defined each name, so that `explain` can point at
    /// `latex.ltx:1234` for a name the cache brought in.
    sites: Vec<(u32, u16, u32)>,
    /// [`Env::aliases`]: which primitive each copy the kernel made is.
    aliases: Vec<(u32, u32)>,
    /// The patterns of names the kernel may have defined from unknown text.
    wild: crate::machine::Wild,
    /// What was diagnosed while the kernel was interpreted.  A run that
    /// installs the cache raises them again, so the answer does not depend
    /// on whether the cache was there.
    diagnostics: Vec<CachedDiagnostic>,
    /// The gaps the kernel build recorded, so a document that reaches the
    /// same gap is deduplicated against them on both paths.
    gaps: Vec<(String, u32)>,
}

/// A cached diagnostic code is `&'static str` in the run: interned once per
/// process.
fn leak(text: &str) -> &'static str {
    static SEEN: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(found) = seen.iter().find(|s| **s == text) {
        return found;
    }
    let leaked: &'static str = Box::leak(text.to_string().into_boxed_str());
    seen.push(leaked);
    leaked
}

/// A file's size and when it last changed, to the nanosecond: an edit that
/// keeps the size within the second a cache was built still shows. On Unix
/// the inode change time joins in, which no tool can set back (`cp -p`,
/// `touch -r` restore only the modification time).
pub(crate) fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as u64;
    #[cfg(unix)]
    let modified = {
        use std::os::unix::fs::MetadataExt;
        let changed = (meta.ctime() as u64).wrapping_mul(1_000_000_000).wrapping_add(meta.ctime_nsec() as u64);
        modified.rotate_left(32) ^ changed
    };
    Some((meta.len(), modified))
}

/// Held while a run interprets the kernel to cache it.
pub static BUILDING: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn default_cache_dir() -> Option<PathBuf> {
    match std::env::var_os("SATEX_CACHE").filter(|value| !value.is_empty()) {
        Some(value) => Some(PathBuf::from(value)),
        None => dirs::cache_dir().map(|base| base.join("satex")),
    }
}

/// The cache directory to use, when `cfg.cache.enabled`: `cfg.cache.dir` or
/// the platform default.
pub fn enabled_cache_dir(cfg: &crate::config::Config) -> Option<PathBuf> {
    cfg.cache.enabled.then(|| cfg.cache.dir.clone().or_else(default_cache_dir)).flatten()
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

pub(crate) fn digest(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// The interpreter's source: a cache written by different code is stale.
const INTERPRETER: &str = env!("SATEX_SOURCE_DIGEST");

/// Cache file per installation, engine, interpreter and preload.  The
/// kernel source is canonicalized first: one `latex.ltx` reached by two
/// paths is one cache, not two, so a run started from another directory
/// reuses what the last one built.
pub fn cache_file(dir: &Path, source: &Path, engine: &str, preload: u64) -> PathBuf {
    let canonical = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let mut hash = digest(canonical.as_os_str().as_encoded_bytes());
    hash ^= digest(engine.as_bytes());
    hash ^= preload;
    dir.join(format!("{}{INTERPRETER}-{hash:08x}.postcard", family(source, engine)))
}

/// What a kernel cache the run wanted and did not find can be told from the
/// ones that are there: whether another satex build wrote them, or this
/// build wrote one under another key.  A rebuild takes minutes, so it says
/// which.
fn miss_reason(dir: &Path, wanted: &Path) -> String {
    let Some(name) = wanted.file_name().and_then(|n| n.to_str()) else {
        return "none built yet for this satex build and engine".into();
    };
    let Some((family, rest)) = name.split_once(INTERPRETER) else {
        return "none built yet for this satex build and engine".into();
    };
    let (mut others, mut mine) = (0usize, Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return "none built yet for this satex build and engine".into();
    };
    for entry in entries.flatten() {
        let found = entry.file_name();
        let Some(found) = found.to_str().filter(|f| f.starts_with("format-") && f.ends_with(".postcard")) else {
            continue;
        };
        match found.strip_prefix(family).and_then(|r| r.strip_prefix(INTERPRETER)) {
            Some(key) if key != rest => mine.push(key.to_string()),
            Some(_) => {}
            None => others += 1,
        }
    }
    match (mine.is_empty(), others) {
        (true, 0) => "none built yet for this satex build and engine".into(),
        (true, n) => format!("the {n} kernel cache(s) found were built by another satex build"),
        (false, _) => format!(
            "this satex build has {} kernel cache(s) under another key: the kernel file, the \
engine or the preloaded format differs",
            mine.len()
        ),
    }
}

/// The part of a cache name every interpreter version shares.
fn family(source: &Path, engine: &str) -> String {
    format!("format-{engine}-{}-v{ENCODING_VERSION}-", tree_name(source))
}

/// How many caches of other interpreter versions a family keeps whatever
/// the size bound says.  Two builds of satex used side by side — a release
/// and a test binary, or two checkouts — would otherwise delete each
/// other's cache on every run.
pub(crate) const KEEP_OTHER_VERSIONS: usize = 4;

/// Remove the caches other interpreter versions left for this family, once
/// they no longer fit within `bound` bytes, and everything of an older
/// encoding.  A kernel cache costs minutes to build, so the rule is the
/// configured size and not a count: a count evicts caches a run would have
/// reused while the cache directory is nearly empty.
fn prune(dir: &Path, source: &Path, engine: &str, bound: u64) {
    let family = family(source, engine);
    prune_family(dir, &family, INTERPRETER, bound);
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with("format-") && !name.contains(&format!("-v{ENCODING_VERSION}-")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Delete `path` and the lock beside it: a lock left behind outlives every
/// cache it ever guarded, and they pile up one per key.
fn remove_with_lock(path: &Path) -> bool {
    // The lock comes first.  A cache another process holds is left alone:
    // unlinking it under that process would let the next one take a lock on
    // an inode nothing else can reach, and two writers would then both
    // believe they hold it.
    let Some(lock) = StoreLock::try_take(path) else { return false };
    let removed = std::fs::remove_file(path).is_ok();
    // The handle goes first: Windows refuses to unlink a file still open without it.
    drop(lock);
    let _ = std::fs::remove_file(lock_for(path));
    removed
}

/// Keep the caches whose name starts with `family` and were written by this
/// interpreter, the [`KEEP_OTHER_VERSIONS`] most recent other ones, and any
/// further ones that still fit within `bound` bytes (0: no bound).
pub(crate) fn prune_family(dir: &Path, family: &str, interpreter: &str, bound: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut others: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".postcard") && name.strip_prefix(family).is_some_and(|rest| !rest.starts_with(interpreter))
        })
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), entry.path()))
        })
        .collect();
    others.sort_by_key(|(modified, ..)| std::cmp::Reverse(*modified));
    let mut total = 0u64;
    for (i, (_, bytes, path)) in others.into_iter().enumerate() {
        total += bytes;
        if i >= KEEP_OTHER_VERSIONS && (bound == 0 || total > bound) {
            let _ = remove_with_lock(&path);
        }
    }
}

/// What [`prune_caches`] removed.
#[derive(Default, Debug, Clone, Copy)]
pub struct Pruned {
    pub files: usize,
    pub bytes: u64,
}

/// The encoding version and interpreter digest a cache file name carries:
/// `…-v⟨encoding⟩-⟨interpreter⟩-⟨key⟩.postcard`.
fn cache_name_parts(name: &str) -> Option<(&str, &str, &str)> {
    let stem = name.strip_suffix(".postcard")?;
    let mut parts = stem.rsplitn(3, '-');
    let (_, interpreter, rest) = (parts.next()?, parts.next()?, parts.next()?);
    let (family, version) = rest.rsplit_once("-v")?;
    Some((family, version, interpreter))
}

/// Delete the caches no build can use any more: kernel caches of another
/// encoding or beyond the [`KEEP_OTHER_VERSIONS`] newest of other
/// interpreters, package caches of another encoding (`package_encoding` is
/// current) or of an interpreter that has no kernel cache left, lock files
/// no cache and no process holds, and then the least recently used caches
/// beyond `bound` bytes over the whole directory (0: no bound).  This
/// build's own kernel cache is never dropped for size.
pub fn prune_caches(dir: &Path, package_encoding: u32, bound: u64) -> Pruned {
    let mut pruned = Pruned::default();
    let Ok(entries) = std::fs::read_dir(dir) else { return pruned };
    let mut kernels = Vec::new();
    let mut packages = Vec::new();
    let mut locks = Vec::new();
    let mut others = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(meta) = entry.metadata().ok() else { continue };
        let stamp = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        if name.ends_with(".lock") {
            locks.push((stamp, entry.path()));
            continue;
        }
        let Some((family, version, interpreter)) = cache_name_parts(&name) else {
            if name.ends_with(".postcard") {
                others.push((stamp, meta.len(), entry.path()));
            }
            continue;
        };
        let item = (stamp, meta.len(), entry.path(), interpreter.to_string());
        if family.starts_with("format-") {
            kernels.push((version == ENCODING_VERSION.to_string(), item));
        } else if family.starts_with("package-") {
            packages.push((version == package_encoding.to_string(), item));
        } else {
            others.push((stamp, meta.len(), entry.path()));
        }
    }
    let mut remove = |path: &Path, bytes: u64| {
        if remove_with_lock(path) {
            pruned.files += 1;
            pruned.bytes += bytes;
        }
    };
    let mut live: std::collections::HashSet<String> = [INTERPRETER.to_string()].into();
    let mut current = Vec::new();
    for (ok, item) in kernels {
        if ok { current.push(item) } else { remove(&item.2, item.1) }
    }
    current.sort_by_key(|(stamp, ..)| std::cmp::Reverse(*stamp));
    let mut foreign = 0;
    // (stamp, bytes, path, may be dropped for size)
    let mut kept: Vec<(std::time::SystemTime, u64, PathBuf, bool)> = Vec::new();
    for (stamp, bytes, path, interpreter) in current {
        if interpreter == INTERPRETER {
            kept.push((stamp, bytes, path, false));
        } else if foreign < KEEP_OTHER_VERSIONS {
            foreign += 1;
            live.insert(interpreter);
            kept.push((stamp, bytes, path, true));
        } else {
            remove(&path, bytes);
        }
    }
    for (ok, (stamp, bytes, path, interpreter)) in packages {
        if ok && live.contains(&interpreter) {
            kept.push((stamp, bytes, path, true));
        } else {
            remove(&path, bytes);
        }
    }
    kept.extend(others.into_iter().map(|(s, b, p)| (s, b, p, true)));
    if bound > 0 {
        kept.sort_by_key(|(stamp, ..)| std::cmp::Reverse(*stamp));
        let mut total = 0u64;
        for (_, bytes, path, droppable) in kept {
            total += bytes;
            if droppable && total > bound {
                remove(&path, bytes);
            }
        }
    }
    // A lock whose cache is gone: a writer creates the lock before its cache,
    // so only locks older than a minute that nothing holds go.
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    for (stamp, path) in locks {
        let cache = path.with_extension("postcard");
        if stamp < old && !cache.exists() {
            if let Some(lock) = StoreLock::try_take(&cache) {
                drop(lock);
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    pruned
}

/// Held while this process writes `target`: another instance doing the same
/// finds it taken and can skip. Released on drop or process death.
pub(crate) struct StoreLock(#[allow(dead_code)] std::fs::File);

impl StoreLock {
    pub(crate) fn try_take(target: &Path) -> Option<StoreLock> {
        let file = std::fs::File::options().create(true).append(true).open(lock_for(target)).ok()?;
        file.try_lock().ok()?;
        Some(StoreLock(file))
    }
}

fn lock_for(target: &Path) -> PathBuf {
    target.with_extension("lock")
}

/// Mark a cache as just used, for [`prune_caches`]' least-recently-used order.
pub(crate) fn touch(path: &Path) {
    if let Ok(file) = std::fs::File::options().append(true).open(path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
}

/// Installation name from TeX Live version directory or file stem.
pub fn tree_name(source: &Path) -> String {
    source
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .find(|part| part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .unwrap_or_else(|| source.file_stem().and_then(|s| s.to_str()).unwrap_or("format").to_string())
}

/// A temporary file beside `target`, unique to this process and thread, to
/// write a cache into and then rename: concurrent writers and readers never
/// see half a file.
pub(crate) fn temporary_for(target: &Path) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    target.with_extension(format!("{}-{n}.tmp", std::process::id()))
}

/// How many package caches the directory holds, and their total size.
pub fn package_caches(directory: &Path) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(directory) else { return (0, 0) };
    entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with("package-") && n.ends_with(".postcard")))
        .filter_map(|e| e.metadata().ok())
        .fold((0, 0), |(n, bytes), meta| (n + 1, bytes + meta.len()))
}

/// Clear satex caches to re-index on next run.
pub fn clear(directory: &Path) -> std::io::Result<usize> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Ok(0) };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "postcard") {
            std::fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

impl Format {
    /// Capture environment state after `latex.ltx` interpretation.
    pub(crate) fn capture(
        source: &Path,
        interner: &Interner,
        env: &Env,
        catcodes: &[(char, u8)],
        files: Vec<(String, LoadKind)>,
        sites: Vec<(u32, u16, u32)>,
        wild: &crate::machine::Wild,
    ) -> Option<Format> {
        let (size, modified) = stamp(source)?;
        let bindings = (0..interner.len() as u32)
            .filter_map(|i| {
                let sym = Sym(i);
                let binding = env.get(sym)?;
                match binding.meaning {
                    Meaning::Undefined => None,
                    _ => Some((i, binding.meaning.clone(), binding.value.clone())),
                }
            })
            .collect();
        Some(Format {
            version: ENCODING_VERSION,
            source: source.display().to_string(),
            size,
            modified,
            names: (0..interner.len()).map(|i| interner.name(Sym(i as u32)).to_string()).collect(),
            bindings,
            catcodes: catcodes.to_vec(),
            files,
            sites,
            // Sorted, because a map's order is the process's: two runs that
            // built the same format would otherwise write different bytes.
            aliases: {
                let mut aliases: Vec<(u32, u32)> = env.aliases.iter().map(|(a, b)| (a.0, b.0)).collect();
                aliases.sort_unstable();
                aliases
            },
            wild: wild.clone(),
            diagnostics: Vec::new(),
            gaps: Vec::new(),
        })
    }

    /// Keep what the kernel build diagnosed, and the gaps it recorded, so a
    /// run that installs this cache reports them again.
    pub(crate) fn diagnosed(
        mut self,
        diagnostics: &[Diagnostic],
        gaps: &std::collections::HashSet<(&'static str, Sym)>,
    ) -> Format {
        self.diagnostics = diagnostics
            .iter()
            .map(|d| {
                let severity = SEVERITIES.iter().position(|s| *s == d.severity).unwrap_or(0) as u8;
                (severity, d.code.to_string(), d.message.clone(), d.span, d.count)
            })
            .collect();
        // Sorted: a cache file must not depend on hash order.
        self.gaps = gaps.iter().map(|(code, sym)| ((*code).to_string(), sym.0)).collect();
        self.gaps.sort();
        self
    }

    pub fn load(dir: &Path, source: &Path, engine: &str, preload: u64) -> Result<Format, String> {
        let path = cache_file(dir, source, engine, preload);
        let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => miss_reason(dir, &path),
            _ => format!("{}: {e}", path.display()),
        })?;
        let format: Format = postcard::from_bytes(&bytes).map_err(|e| format!("cannot decode the cache: {e}"))?;
        let (size, modified) = stamp(source).ok_or("the format source has no metadata")?;
        if format.size != size || format.modified != modified {
            return Err("latex.ltx changed".into());
        }
        touch(&path);
        Ok(format)
    }

    pub fn store(&self, dir: &Path, source: &Path, engine: &str, preload: u64, bound: u64) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let bytes = postcard::to_allocvec(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        let target = cache_file(dir, source, engine, preload);
        let Some(_lock) = StoreLock::try_take(&target) else { return Ok(()) };
        let temporary = temporary_for(&target);
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(temporary, target)?;
        prune(dir, source, engine, bound);
        Ok(())
    }

    /// Reinstate cached meanings (cache has own interner; catalog applied on top).
    pub fn install(self, interner: &mut Interner, env: &mut Env) -> Vec<(char, u8)> {
        *interner = Interner::from_names(self.names);
        *env = Env::with_capacity(interner.len());
        for (index, meaning, value) in self.bindings {
            let mut binding = Binding::builtin(meaning);
            binding.value = value;
            env.set(Sym(index), binding, true);
        }
        env.aliases = self.aliases.iter().map(|&(a, b)| (Sym(a), Sym(b))).collect();
        self.catcodes
    }

    pub fn definitions(&self) -> usize {
        self.bindings.len()
    }

    /// The files the kernel was read from, for a run that installs it.
    pub fn files(&self) -> &[(String, LoadKind)] {
        &self.files
    }

    /// Where the kernel defined each name: symbol, file and line.
    pub fn sites(&self) -> &[(u32, u16, u32)] {
        &self.sites
    }

    pub(crate) fn wild(&self) -> &crate::machine::Wild {
        &self.wild
    }
}

impl Machine<'_> {
    /// Raise again what the kernel build diagnosed, so a run that installs
    /// the cache reports what the run that built it reported.
    pub(crate) fn restore_format_diagnostics(&mut self, format: &Format) {
        for (severity, code, message, span, count) in &format.diagnostics {
            let code = leak(code);
            let severity = SEVERITIES[usize::from(*severity).min(SEVERITIES.len() - 1)];
            let before = self.out.facts.diagnostics.len();
            self.diagnose(severity, code, *span, message.clone());
            // The row it landed on: a new one, or the one it was merged
            // into.  Either way it now counts once, and the build counted
            // it `count` times.
            let at = match self.out.facts.diagnostics.len() > before {
                true => Some(before),
                false => self
                    .out
                    .facts
                    .diagnostics
                    .iter()
                    .position(|d| d.code == code && d.span == *span && d.message == *message),
            };
            if let Some(row) = at.and_then(|at| self.out.facts.diagnostics.get_mut(at)) {
                row.count = row.count.saturating_sub(1).saturating_add(*count);
            }
        }
        for (code, sym) in &format.gaps {
            self.gaps.insert((leak(code), Sym(*sym)));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::tex::{ArgSpec, Catcode, MacroDef, ParameterText, Span, Tok, Token};
    use crate::value::Value;

    fn sample() -> Format {
        let mut interner = Interner::default();
        let name = interner.intern("mymacro");
        let body: Vec<Token> = vec![
            Token::new(Tok::Cs(name), Span::new(1, 2, 3)),
            Token::new(Tok::Chr('x', Catcode::Letter), Span::new(1, 2, 4)),
            Token::new(Tok::Param(1), Span::new(1, 2, 5)),
        ];
        let mac = MacroDef {
            parameter_text: ParameterText::from_tokens(&body, |_| false),
            arg_spec: Some(ArgSpec {
                items: vec![crate::tex::ArgType::Optional(Some("d".into())), crate::tex::ArgType::Mandatory],
                raw: "O{d}m".into(),
            }),
            replacement_text: Rc::from(body),
            long: true,
            outer: false,
            protected: true,
        };
        let mut env = Env::with_capacity(interner.len());
        env.set(name, Binding::builtin(Meaning::Macro(Rc::new(mac))), true);
        env.set_value(name, Value::Dimen(65536), true);
        Format {
            version: ENCODING_VERSION,
            source: "latex.ltx".into(),
            size: 1,
            modified: 2,
            names: (0..interner.len()).map(|i| interner.name(Sym(i as u32)).to_string()).collect(),
            bindings: (0..interner.len() as u32)
                .filter_map(|i| {
                    let binding = env.get(Sym(i))?;
                    Some((i, binding.meaning.clone(), binding.value.clone()))
                })
                .collect(),
            catcodes: vec![('@', 11)],
            files: vec![("latex.ltx".to_string(), LoadKind::Input)],
            sites: Vec::new(),
            aliases: Vec::new(),
            diagnostics: vec![(1, "invalid-code".into(), "invalid code (8217)".into(), Span::new(3, 2, 1), 2)],
            gaps: vec![("unsupported-primitive".into(), 0)],
            wild: {
                let mut wild = crate::machine::Wild::default();
                wild.add("MT@inh@".into(), String::new(), true);
                wild
            },
        }
    }

    #[test]
    fn a_format_survives_the_round_trip() {
        let bytes = postcard::to_allocvec(&sample()).expect("encodes");
        let back: Format = postcard::from_bytes(&bytes).expect("decodes");
        assert_eq!(back.names, sample().names);
        assert_eq!(back.bindings.len(), sample().bindings.len());
        assert_eq!(back.catcodes, sample().catcodes);
        assert_eq!(back.diagnostics, sample().diagnostics);
        assert_eq!(back.gaps, sample().gaps);
        // The names the kernel may have defined from unknown text.
        assert_eq!(back.wild, sample().wild);
        assert!(!back.wild.is_empty());
    }

    #[test]
    fn a_pruned_family_leaves_no_lock_behind() {
        let dir = std::env::temp_dir().join(format!("satex-locks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let family = "format-pdftex-2026-v9-";
        let make = |interpreter: &str, key: &str, bytes: u64| {
            for extension in ["postcard", "lock"] {
                let path = dir.join(format!("{family}{interpreter}-{key}.{extension}"));
                let file = std::fs::File::create(&path).unwrap();
                if extension == "postcard" {
                    file.set_len(bytes).unwrap();
                }
            }
        };
        make("mine", "1", 10);
        for i in 0..6 {
            make(&format!("other{i}"), "1", 100);
        }
        // Four other builds are kept whatever the bound says, and the rest
        // only while they fit: 250 bytes leaves none of the last two.
        prune_family(&dir, family, "mine", 250);
        let mut left: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        let caches = left.iter().filter(|n| n.ends_with(".postcard")).count();
        let locks = left.iter().filter(|n| n.ends_with(".lock")).count();
        assert_eq!(caches, 5, "{left:?}");
        assert_eq!(locks, caches, "every cache that went took its lock: {left:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cache_another_process_holds_is_not_evicted() {
        let dir = std::env::temp_dir().join(format!("satex-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let family = "format-pdftex-2026-v9-";
        let path = |interpreter: &str| dir.join(format!("{family}{interpreter}-1.postcard"));
        // Six other builds, newest first: the four most recent are kept
        // whatever the bound says, so `held` and `also` are the two the
        // bound would evict.
        let others = ["one", "two", "three", "four", "held", "also"];
        for (age, interpreter) in others.iter().enumerate() {
            let file = std::fs::File::create(path(interpreter)).unwrap();
            file.set_len(1000).unwrap();
            file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(age as u64 + 1)).unwrap();
        }
        std::fs::File::create(path("mine")).unwrap().set_len(1000).unwrap();
        // `held` is being written by someone: the bound says it must go,
        // the lock says it must not.  Unlinking it would let the next
        // writer lock an inode its holder cannot see.
        let lock = StoreLock::try_take(&path("held")).expect("the lock is free to take here");
        prune_family(&dir, family, "mine", 1);
        assert!(path("held").exists(), "a locked cache was evicted");
        assert!(path("mine").exists(), "this build's own cache was evicted");
        assert!(!path("also").exists(), "a free cache beyond the bound was kept");
        drop(lock);
        // Once it is free, the same rule removes it.
        prune_family(&dir, family, "mine", 1);
        assert!(!path("held").exists(), "a cache kept only by its lock was kept after the lock went");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pruning_keeps_what_a_build_can_read_within_the_bound() {
        let dir = std::env::temp_dir().join(format!("satex-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = |name: &str, bytes: u64, age: u64| {
            let path = dir.join(name);
            let f = std::fs::File::create(&path).unwrap();
            f.set_len(bytes).unwrap();
            f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(age)).unwrap();
        };
        let other = "0123456789abcdef";
        file(&format!("format-pdftex-2026-v{ENCODING_VERSION}-{INTERPRETER}-1.postcard"), 10, 0);
        file(&format!("format-pdftex-2026-v{}-{INTERPRETER}-1.postcard", ENCODING_VERSION + 1), 10, 0);
        // Current encoding, live interpreter: kept while the bound allows.
        file(&format!("package-a-v5-{INTERPRETER}-1.postcard"), 100, 0);
        file(&format!("package-b-v5-{INTERPRETER}-1.postcard"), 100, 50);
        file(&format!("package-c-v5-{INTERPRETER}-1.postcard"), 100, 100);
        // Another encoding, or an interpreter without a kernel cache: dead.
        file(&format!("package-a-v4-{INTERPRETER}-1.postcard"), 100, 0);
        file(&format!("package-a-v5-{other}-1.postcard"), 100, 0);
        let pruned = prune_caches(&dir, 5, 250);
        let mut left: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(pruned.files, 4, "{left:?}");
        assert_eq!(
            left,
            [
                format!("format-pdftex-2026-v{ENCODING_VERSION}-{INTERPRETER}-1.postcard"),
                format!("package-a-v5-{INTERPRETER}-1.postcard"),
                format!("package-b-v5-{INTERPRETER}-1.postcard"),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
