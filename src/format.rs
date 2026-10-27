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
const ENCODING_VERSION: u32 = 5;
/// The encoding of the package caches, versioned apart from the kernel's.
pub(crate) const PACKAGE_ENCODING_VERSION: u32 = 5;

/// What a cache means, bumped by hand whenever the interpreter would
/// analyse a cached file differently. Caches are keyed on it, not on the
/// source, so builds of one version share them across machines.
/// `tests/it/cache_semantics.rs` fails when the analysis changes without a bump.
pub const SEMANTIC_VERSION: u32 = 1;

/// The key every cache file name carries: `s⟨SEMANTIC_VERSION⟩`. A build
/// outside the `release` profile appends its source digest (build.rs), so a
/// developer's edits between bumps never replay a stale local cache.
pub(crate) fn cache_key() -> &'static str {
    static KEY: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| format!("s{SEMANTIC_VERSION}{}", env!("SATEX_CACHE_GUARD")));
    &KEY
}

/// [`Severity`] by index, so a cache stores a byte and not a name.
pub(crate) const SEVERITIES: [Severity; 5] =
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
    /// The kernel file, as [`Roots::encode`] writes it.
    source: String,
    names: Vec<String>,
    bindings: Vec<(u32, Meaning, Value)>,
    catcodes: Vec<(char, u8)>,
    /// The files the kernel was read from, in the order they were read.  The
    /// spans inside the cached macro bodies carry those file numbers, so a
    /// run that installs this cache has to hand out the same numbers again or
    /// every kernel definition would point at whatever file took the number.
    files: Vec<(String, LoadKind)>,
    /// [`content_hash`] of each of `files`, which must all still hold.
    hashes: Vec<Option<u64>>,
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
    /// The commands the kernel wrote its definitions with, the name right
    /// after them ([`crate::exports::written`]).
    definers: Vec<String>,
}

/// The run's diagnostic codes, fact tags and metadata fields are
/// `&'static str`; a cached one is interned once per process.
pub(crate) fn leak(text: &str) -> &'static str {
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

/// A fast digest of a file's bytes, the validity criterion of every cache:
/// a copied or unpacked tree with the same content stays valid, any edit
/// does not. Memoized per [`stamp`], in this process and in the cache
/// directory ([`load_hashes`]), so an unchanged file is read once.
pub(crate) fn content_hash(path: &Path) -> Option<u64> {
    let stamp = stamp(path)?;
    let memo = HASHES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((known, hash)) = memo.known.get(path)
        && *known == stamp
    {
        return Some(*hash);
    }
    drop(memo);
    let hash = bytes_hash(&std::fs::read(path).ok()?);
    let mut memo = HASHES.lock().unwrap_or_else(|e| e.into_inner());
    memo.known.insert(path.to_path_buf(), (stamp, hash));
    memo.dirty = true;
    Some(hash)
}

/// Eight bytes a step: the kernel's files hash in milliseconds.
pub(crate) fn bytes_hash(bytes: &[u8]) -> u64 {
    const K: u64 = 0x9e37_79b9_7f4a_7c15;
    let mix = |h: u64, w: u64| (h ^ w).wrapping_mul(K).rotate_left(29);
    let (chunks, remainder) = bytes.as_chunks::<8>();
    let mut h = (bytes.len() as u64).wrapping_mul(K);
    for &chunk in chunks {
        h = mix(h, u64::from_le_bytes(chunk));
    }
    for &byte in remainder {
        h = mix(h, u64::from(byte) | 0x100);
    }
    h ^ (h >> 31)
}

#[derive(Default)]
struct Hashes {
    known: crate::fixed::HashMap<PathBuf, ((u64, u64), u64)>,
    loaded: crate::fixed::HashSet<PathBuf>,
    dirty: bool,
}

static HASHES: std::sync::LazyLock<std::sync::Mutex<Hashes>> = std::sync::LazyLock::new(Default::default);

pub(crate) const HASHES_FILE: &str = "content-hashes-v1.postcard";

type StoredHashes = Vec<(String, (u64, u64), u64)>;

/// Take in the hashes an earlier run left in `dir`, once per process.
pub(crate) fn load_hashes(dir: &Path) {
    let mut memo = HASHES.lock().unwrap_or_else(|e| e.into_inner());
    if !memo.loaded.insert(dir.to_path_buf()) {
        return;
    }
    let Ok(bytes) = std::fs::read(dir.join(HASHES_FILE)) else { return };
    let Ok(stored) = postcard::from_bytes::<StoredHashes>(&bytes) else { return };
    for (path, stamp, hash) in stored {
        memo.known.entry(PathBuf::from(path)).or_insert((stamp, hash));
    }
}

/// Leave this process's hashes in `dir` for the next run, if it made any,
/// and keep `dir` within `bound` with them.
pub(crate) fn save_hashes(dir: &Path, bound: u64) {
    let stored: StoredHashes = {
        let mut memo = HASHES.lock().unwrap_or_else(|e| e.into_inner());
        if !std::mem::take(&mut memo.dirty) {
            return;
        }
        let mut all: StoredHashes =
            memo.known.iter().map(|(p, (stamp, hash))| (p.display().to_string(), *stamp, *hash)).collect();
        all.sort();
        all
    };
    let Ok(bytes) = postcard::to_allocvec(&stored) else { return };
    let target = dir.join(HASHES_FILE);
    let temporary = temporary_for(&target);
    if std::fs::write(&temporary, bytes).is_ok() && std::fs::rename(&temporary, &target).is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    crate::machine::prune_caches(dir, bound);
}

/// The trees a cached path is written against: each TeX root under a tag
/// (`TEXMF-DIST`, `TEXMF-LOCAL`, ...) and the project as `PROJECT`. A path
/// is stored as `$TAG/relative`, and a load resolves it against this run's
/// roots, so a cache moves with an unpacked or copied tree.
/// A bundle root ([`crate::config::Texmf::bundle`]) is laid out like
/// `texmf-dist` and stands in for it: it takes that tag when no other root
/// has it, so caches built against an installation serve the bundle, and
/// `SATEX-BUNDLE` otherwise.
pub(crate) struct Roots(Vec<(String, PathBuf)>, Vec<PathBuf>);

impl Roots {
    pub(crate) fn new(texmf: &[PathBuf], bundle: Option<&Path>, project: Option<&Path>) -> Roots {
        let mut roots: Vec<(String, PathBuf)> = Vec::new();
        let search: Vec<PathBuf> = texmf.iter().cloned().chain(bundle.map(Path::to_path_buf)).collect();
        for (i, root) in texmf.iter().enumerate().filter(|(_, r)| Some(r.as_path()) != bundle) {
            let leaf = root.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_uppercase();
            let mut tag = match leaf.as_str() {
                "TEXMF-DIST" | "TEXMF-LOCAL" | "TEXMF-VAR" | "TEXMF-CONFIG" | "TEXMF-SYSVAR" | "TEXMF-SYSCONFIG" => {
                    leaf
                }
                "TEXMF" => "TEXMF-HOME".into(),
                _ => format!("TEXMF{i}"),
            };
            if roots.iter().any(|(t, _)| *t == tag) {
                tag = format!("{tag}{i}");
            }
            roots.push((tag, root.clone()));
        }
        if let Some(bundle) = bundle {
            let dist = roots.iter().all(|(t, _)| t != "TEXMF-DIST");
            roots.push((if dist { "TEXMF-DIST" } else { "SATEX-BUNDLE" }.into(), bundle.to_path_buf()));
        }
        roots.extend(project.map(|p| ("PROJECT".to_string(), p.to_path_buf())));
        // The deepest root claims a path first.
        roots.sort_by_key(|(_, root)| std::cmp::Reverse(root.components().count()));
        Roots(roots, search)
    }

    pub(crate) fn encode(&self, path: &str) -> String {
        let at = Path::new(path);
        for (tag, root) in &self.0 {
            if let Ok(rest) = at.strip_prefix(root) {
                let rest: Vec<_> = rest.components().map(|c| c.as_os_str().to_string_lossy()).collect();
                return format!("${tag}/{}", rest.join("/"));
            }
        }
        path.to_string()
    }

    /// A path as a digest takes it: relative to its TeX root, so that which
    /// roots exist and how they are tagged is no part of it. Project paths
    /// keep their tag.
    pub(crate) fn canonical(&self, path: &str) -> String {
        let stored = self.encode(path);
        match stored.strip_prefix('$').and_then(|s| s.split_once('/')) {
            Some((tag, rest)) if tag != "PROJECT" => rest.to_string(),
            _ => stored,
        }
    }

    /// The local path of a stored one. A tag this run has no root for names
    /// the file as the first TeX root holding it does, as a lookup would;
    /// none holding it leaves the path as written, which names no file.
    pub(crate) fn decode(&self, stored: &str) -> String {
        let Some((tag, rest)) = stored.strip_prefix('$').and_then(|s| s.split_once('/')) else {
            return stored.to_string();
        };
        match self.0.iter().find(|(t, _)| t == tag) {
            Some((_, root)) => root.join(rest).display().to_string(),
            None => match self.1.iter().map(|root| root.join(rest)).find(|p| p.exists()) {
                Some(found) if tag != "PROJECT" => found.display().to_string(),
                _ => stored.to_string(),
            },
        }
    }
}

/// Held while a run interprets the kernel to cache it.
pub static BUILDING: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn default_cache_dir() -> Option<PathBuf> {
    match std::env::var_os("SATEX_CACHE").filter(|value| !value.is_empty()) {
        Some(value) => Some(PathBuf::from(value)),
        None => crate::paths::cache_dir().map(|base| base.join("satex")),
    }
}

/// The cache directory to use, when `cfg.cache.enabled`: `cfg.cache.dir` or
/// the platform default.
pub fn enabled_cache_dir(cfg: &crate::config::Config) -> Option<PathBuf> {
    cfg.cache.enabled.then(|| cfg.cache.dir.clone().or_else(default_cache_dir)).flatten()
}

/// 64-bit FNV-1a, the hash every cache name and digest is built from.
pub(crate) struct Fnv(pub(crate) u64);

impl Fnv {
    pub(crate) fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    pub(crate) fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
    /// A value as its length-prefixed postcard encoding.
    pub(crate) fn put<T: Serialize + ?Sized>(&mut self, value: &T) {
        let bytes = postcard::to_allocvec(value).unwrap_or_default();
        self.write(&(bytes.len() as u64).to_le_bytes());
        self.write(&bytes);
    }
}

pub(crate) fn digest(bytes: &[u8]) -> u64 {
    let mut hash = Fnv::new();
    hash.write(bytes);
    hash.0
}

/// Cache file per installation, engine, semantic version and preload. The
/// kernel source is keyed as its root writes it ([`Roots::encode`]), so a
/// copied tree finds the same cache; outside every root it is canonicalized
/// first, so one `latex.ltx` reached by two paths is one cache.
pub(crate) fn cache_file(dir: &Path, source: &Path, engine: &str, preload: u64, roots: &Roots) -> PathBuf {
    let shown = source.display().to_string();
    let encoded = roots.encode(&shown);
    let named = match encoded != shown {
        true => encoded,
        false => std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf()).display().to_string(),
    };
    let mut hash = digest(named.as_bytes());
    hash ^= digest(engine.as_bytes());
    hash ^= preload;
    dir.join(format!("{}{}-{hash:08x}.postcard", family(source, engine), cache_key()))
}

/// What a kernel cache the run wanted and did not find can be told from the
/// ones that are there: whether another satex build wrote them, or this
/// build wrote one under another key.  A rebuild takes minutes, so it says
/// which.
fn miss_reason(dir: &Path, wanted: &Path) -> String {
    let none = || "none built yet for this satex build and engine".to_string();
    let Some((family, _, hash)) = wanted.file_name().and_then(|n| n.to_str()).and_then(split_cache_name) else {
        return none();
    };
    let (mut others, mut mine) = (0usize, 0usize);
    let Ok(entries) = std::fs::read_dir(dir) else { return none() };
    for entry in entries.flatten() {
        let found = entry.file_name();
        let Some(found) = found.to_str().filter(|f| f.starts_with("format-")) else { continue };
        match split_cache_name(found) {
            Some((f, key, h)) if f == family && key == cache_key() => mine += usize::from(h != hash),
            Some(_) => others += 1,
            None => {}
        }
    }
    match (mine, others) {
        (0, 0) => none(),
        (0, n) => format!("the {n} kernel cache(s) found were built by another satex build"),
        (n, _) => format!(
            "this satex build has {n} kernel cache(s) under another key: the kernel file, the \
engine or the preloaded format differs"
        ),
    }
}

/// A cache file name as `(family, key, hash)`: `⟨family⟩-⟨key⟩-⟨hash⟩.postcard`,
/// the family ending in `-v⟨encoding⟩`.  The key is the whole segment, so
/// `s1` is not the key of `s1d0123…`.
fn split_cache_name(name: &str) -> Option<(&str, &str, &str)> {
    let (rest, hash) = name.strip_suffix(".postcard")?.rsplit_once('-')?;
    let (family, key) = rest.rsplit_once('-')?;
    Some((family, key, hash))
}

/// The package or class a cache file holds, `None` for a kernel or an index.
pub(crate) fn cache_holds(name: &str) -> Option<&str> {
    let (family, ..) = split_cache_name(name)?;
    family.strip_prefix("package-")?.rsplit_once("-v").map(|(held, _)| held)
}

/// Whether a cache file name is one this build can use: its key and encoding.
pub(crate) fn is_current_cache(name: &str) -> bool {
    let version = if name.starts_with("package-") { PACKAGE_ENCODING_VERSION } else { ENCODING_VERSION };
    (name.starts_with("format-") || name.starts_with("package-"))
        && split_cache_name(name)
            .is_some_and(|(family, key, _)| key == cache_key() && family.ends_with(&format!("-v{version}")))
}

/// The part of a cache name every interpreter version shares.
fn family(source: &Path, engine: &str) -> String {
    format!("format-{engine}-{}-v{ENCODING_VERSION}-", release_name(source))
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
    prune_family(dir, &family, cache_key(), bound);
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with("format-") && cache_name_parts(name).is_some_and(|(_, v, _)| older(v, ENCODING_VERSION)) {
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
            name.ends_with(".postcard")
                && name.strip_prefix(family).is_some_and(|rest| rest.split('-').next() != Some(interpreter))
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

/// Whether a cache's encoding `version` is older than `ours`.  A newer one
/// belongs to a newer satex sharing the directory and is left to it.
fn older(version: &str, ours: u32) -> bool {
    version.parse::<u32>().map_or(true, |v| v < ours)
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
        // A run killed between write and rename leaves its temporary behind.
        if name.ends_with(".tmp") && stamp.elapsed().is_ok_and(|age| age.as_secs() > 3600) {
            let _ = std::fs::remove_file(entry.path());
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
            kernels.push((!older(version, ENCODING_VERSION), item));
        } else if family.starts_with("package-") {
            packages.push((!older(version, package_encoding), item));
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
    let mut live: crate::fixed::HashSet<String> = [cache_key().to_string()].into_iter().collect();
    let mut current = Vec::new();
    for (ok, item) in kernels {
        if ok { current.push(item) } else { remove(&item.2, item.1) }
    }
    current.sort_by_key(|(stamp, ..)| std::cmp::Reverse(*stamp));
    let mut foreign = 0;
    // (stamp, bytes, path, may be dropped for size)
    let mut kept: Vec<(std::time::SystemTime, u64, PathBuf, bool)> = Vec::new();
    for (stamp, bytes, path, interpreter) in current {
        if interpreter == cache_key() {
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
        // What stays whatever its age counts first.
        let mut total: u64 = kept.iter().filter(|k| !k.3).map(|k| k.1).sum();
        for (_, bytes, path, _) in kept.into_iter().filter(|k| k.3) {
            total += bytes;
            if total > bound {
                remove(&path, bytes);
            }
        }
    }
    // A lock whose cache is gone: a writer creates the lock before its cache,
    // so only locks older than a minute that nothing holds go.
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    for (stamp, path) in locks {
        let cache = path.with_extension("postcard");
        if stamp < old
            && !cache.exists()
            && let Some(lock) = StoreLock::try_take(&cache)
        {
            drop(lock);
            let _ = std::fs::remove_file(&path);
        }
    }
    pruned
}

/// Held while this process writes `target`: another instance doing the same
/// finds it taken and can skip. Released on drop or process death.
pub(crate) struct StoreLock(#[allow(dead_code)] std::fs::File);

impl StoreLock {
    pub(crate) fn try_take(target: &Path) -> Option<StoreLock> {
        // Read and write, not append: Windows locks only a handle opened so.
        let file =
            std::fs::File::options().read(true).write(true).create(true).truncate(false).open(lock_for(target)).ok()?;
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

/// The release a kernel file belongs to: the year of its own `\fmtversion`,
/// so a tree at any path, such as a bundle, is named as the distribution
/// names it. Without one, [`tree_name`].
pub fn release_name(source: &Path) -> String {
    static MEMO: std::sync::LazyLock<std::sync::Mutex<crate::fixed::HashMap<PathBuf, String>>> =
        std::sync::LazyLock::new(Default::default);
    if let Some(known) = MEMO.lock().unwrap_or_else(|e| e.into_inner()).get(source) {
        return known.clone();
    }
    let year = std::fs::File::open(source).ok().and_then(|file| {
        use std::io::Read;
        let mut head = Vec::new();
        file.take(1 << 16).read_to_end(&mut head).ok()?;
        let version = crate::distribution::braced_after(&String::from_utf8_lossy(&head), "\\fmtversion")?;
        let year = version.get(..4)?;
        (year.chars().all(|c| c.is_ascii_digit()) && version.get(4..5) == Some("-")).then(|| year.to_string())
    });
    let name = year.unwrap_or_else(|| tree_name(source));
    MEMO.lock().unwrap_or_else(|e| e.into_inner()).insert(source.to_path_buf(), name.clone());
    name
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
        files: Vec<(String, LoadKind)>,
        sites: Vec<(u32, u16, u32)>,
        wild: &crate::machine::Wild,
        roots: &Roots,
    ) -> Option<Format> {
        content_hash(source)?;
        let hashes = files.iter().map(|(path, _)| content_hash(Path::new(path))).collect();
        let files = files.into_iter().map(|(path, kind)| (roots.encode(&path), kind)).collect();
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
            source: roots.encode(&source.display().to_string()),
            names: (0..interner.len()).map(|i| interner.name(Sym(i as u32)).to_string()).collect(),
            bindings,
            catcodes: Vec::new(),
            files,
            hashes,
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
            definers: Vec::new(),
        })
    }

    /// Keep what the kernel build diagnosed, and the gaps it recorded, so a
    /// run that installs this cache reports them again.
    pub(crate) fn diagnosed(
        mut self,
        diagnostics: &[Diagnostic],
        gaps: &crate::fixed::HashSet<(&'static str, Sym)>,
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

    /// The kernel cache for `source`, its paths resolved against `roots`,
    /// if every file it was read from still has the content it had.
    pub(crate) fn load(
        dir: &Path,
        source: &Path,
        engine: &str,
        preload: u64,
        bound: u64,
        roots: &Roots,
    ) -> Result<Format, String> {
        load_hashes(dir);
        let path = cache_file(dir, source, engine, preload, roots);
        let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => miss_reason(dir, &path),
            _ => format!("{}: {e}", path.display()),
        })?;
        let mut format: Format = postcard::from_bytes(&bytes).map_err(|e| format!("cannot decode the cache: {e}"))?;
        for ((file, _), hash) in format.files.iter_mut().zip(&format.hashes) {
            *file = roots.decode(file);
            if content_hash(Path::new(file)) != *hash {
                return Err(format!("content changed: {file}"));
            }
        }
        save_hashes(dir, bound);
        touch(&path);
        Ok(format)
    }

    pub(crate) fn store(
        &self,
        dir: &Path,
        source: &Path,
        engine: &str,
        preload: u64,
        bound: u64,
        roots: &Roots,
    ) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let bytes = postcard::to_allocvec(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        save_hashes(dir, bound);
        let target = cache_file(dir, source, engine, preload, roots);
        let Some(_lock) = StoreLock::try_take(&target) else { return Ok(()) };
        let temporary = temporary_for(&target);
        if let Err(e) = std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, target)) {
            let _ = std::fs::remove_file(&temporary);
            return Err(e);
        }
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
    pub fn definers(&self) -> &[String] {
        &self.definers
    }

    pub(crate) fn with_definers(mut self, definers: Vec<String>) -> Format {
        self.definers = definers;
        self
    }

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
            names: (0..interner.len()).map(|i| interner.name(Sym(i as u32)).to_string()).collect(),
            bindings: (0..interner.len() as u32)
                .filter_map(|i| {
                    let binding = env.get(Sym(i))?;
                    Some((i, binding.meaning.clone(), binding.value.clone()))
                })
                .collect(),
            catcodes: vec![('@', 11)],
            files: vec![("latex.ltx".to_string(), LoadKind::Input)],
            hashes: vec![Some(1)],
            sites: Vec::new(),
            aliases: Vec::new(),
            diagnostics: vec![(1, "invalid-code".into(), "invalid code (8217)".into(), Span::new(3, 2, 1), 2)],
            gaps: vec![("unsupported-primitive".into(), 0)],
            definers: vec!["def".into()],
            wild: {
                let mut wild = crate::machine::Wild::default();
                wild.add("MT@inh@".into(), String::new(), false, Some(crate::tex::Meaning::Unknown));
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
    fn a_cache_key_is_a_whole_name_segment() {
        let key = cache_key();
        let name = |key: &str| format!("format-pdftex-2026-v{ENCODING_VERSION}-{key}-0123.postcard");
        assert!(is_current_cache(&name(key)));
        assert!(!is_current_cache(&name(&format!("{key}d0123456789abcdef"))));
        assert!(!is_current_cache(&name(&key[..key.len() - 1])));
        let package = PACKAGE_ENCODING_VERSION;
        assert!(is_current_cache(&format!("package-a-v{package}-{key}-0123.postcard")));
        assert!(!is_current_cache(&format!("package-a-v{}-{key}-0123.postcard", package + 1)));
        // A build whose key extends this one's is another build to `miss_reason`.
        let dir = std::env::temp_dir().join(format!("satex-miss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name(&format!("{key}x"))), b"").unwrap();
        let reason = miss_reason(&dir, &dir.join(name(key)));
        assert!(reason.contains("another satex build"), "{reason}");
        std::fs::write(dir.join(name(key).replace("0123", "4567")), b"").unwrap();
        let reason = miss_reason(&dir, &dir.join(name(key)));
        assert!(reason.contains("under another key"), "{reason}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pruning_keeps_what_a_build_can_read_within_the_bound() {
        let key = cache_key();
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
        file(&format!("format-pdftex-2026-v{ENCODING_VERSION}-{key}-1.postcard"), 10, 0);
        file(&format!("format-pdftex-2026-v{}-{key}-1.postcard", ENCODING_VERSION - 1), 10, 0);
        // A newer satex's kernel is its own to prune.
        let newer = format!("format-pdftex-2026-v{}-fedcba9876543210-1.postcard", ENCODING_VERSION + 1);
        file(&newer, 10, 0);
        // Current encoding, live interpreter: kept while the bound allows.
        file(&format!("package-a-v5-{key}-1.postcard"), 100, 0);
        file(&format!("package-b-v5-{key}-1.postcard"), 100, 50);
        file(&format!("package-c-v5-{key}-1.postcard"), 100, 100);
        // Another encoding, or an interpreter without a kernel cache: dead.
        file(&format!("package-a-v4-{key}-1.postcard"), 100, 0);
        file(&format!("package-a-v5-{other}-1.postcard"), 100, 0);
        let pruned = prune_caches(&dir, 5, 250);
        let mut left: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(pruned.files, 4, "{left:?}");
        assert_eq!(
            left,
            [
                format!("format-pdftex-2026-v{ENCODING_VERSION}-{key}-1.postcard"),
                newer,
                format!("package-a-v5-{key}-1.postcard"),
                format!("package-b-v5-{key}-1.postcard"),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
