//! Format cache: interpret `latex.ltx` once, cache for reuse.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::builtins::LoadKind;
use crate::env::{Binding, Env};
use crate::tex::{Interner, Meaning, Sym};
use crate::value::Value;

/// Cache version: different encodings get different files.
const ENCODING_VERSION: u32 = 1;

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
}

pub(crate) fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
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

/// Cache file per installation, engine, interpreter and preload.
pub fn cache_file(dir: &Path, source: &Path, engine: &str, preload: u64) -> PathBuf {
    let mut hash = digest(source.as_os_str().as_encoded_bytes());
    hash ^= digest(engine.as_bytes());
    hash ^= preload;
    dir.join(format!("{}{INTERPRETER}-{hash:08x}.postcard", family(source, engine)))
}

/// The part of a cache name every interpreter version shares.
fn family(source: &Path, engine: &str) -> String {
    format!("format-{engine}-{}-v{ENCODING_VERSION}-", tree_name(source))
}

/// How many caches of other interpreter versions a family keeps.  Two
/// builds of satex used side by side — a release and a test binary, or two
/// checkouts — would otherwise delete each other's cache on every run.
pub(crate) const KEEP_OTHER_VERSIONS: usize = 4;

/// Remove the caches other interpreter versions left for this family,
/// beyond the most recent [`KEEP_OTHER_VERSIONS`].
fn prune(dir: &Path, source: &Path, engine: &str) {
    let family = family(source, engine);
    prune_family(dir, &family, INTERPRETER);
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with("format-") && !name.contains(&format!("-v{ENCODING_VERSION}-")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Keep the caches whose name starts with `family` and were written by this
/// interpreter, and the [`KEEP_OTHER_VERSIONS`] most recent other ones.
pub(crate) fn prune_family(dir: &Path, family: &str, interpreter: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut others: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".postcard") && name.strip_prefix(family).is_some_and(|rest| !rest.starts_with(interpreter))
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    others.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in others.into_iter().skip(KEEP_OTHER_VERSIONS) {
        let _ = std::fs::remove_file(path);
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
/// encoding, package caches of another encoding (`package_encoding` is
/// current) or of an interpreter that has no kernel cache left, and then the
/// least recently used package caches beyond `bound` bytes (0: no bound).
pub fn prune_caches(dir: &Path, package_encoding: u32, bound: u64) -> Pruned {
    let mut pruned = Pruned::default();
    let Ok(entries) = std::fs::read_dir(dir) else { return pruned };
    let mut kernels = Vec::new();
    let mut packages = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(meta) = entry.metadata().ok() else { continue };
        let Some((family, version, interpreter)) = cache_name_parts(&name) else { continue };
        let stamp = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        let item = (stamp, meta.len(), entry.path(), interpreter.to_string());
        if family.starts_with("format-") {
            kernels.push((version == ENCODING_VERSION.to_string(), item));
        } else if family.starts_with("package-") {
            packages.push((version == package_encoding.to_string(), item));
        }
    }
    let mut remove = |path: &Path, bytes: u64| {
        if std::fs::remove_file(path).is_ok() {
            pruned.files += 1;
            pruned.bytes += bytes;
            if let Some(lock) = StoreLock::try_take(path) {
                let _ = std::fs::remove_file(lock_for(path));
                drop(lock);
            }
        }
    };
    let mut live: std::collections::HashSet<String> = [INTERPRETER.to_string()].into();
    for (current, (_, bytes, path, interpreter)) in kernels {
        if current {
            live.insert(interpreter);
        } else {
            remove(&path, bytes);
        }
    }
    let mut kept = Vec::new();
    for (current, (stamp, bytes, path, interpreter)) in packages {
        if current && live.contains(&interpreter) {
            kept.push((stamp, bytes, path));
        } else {
            remove(&path, bytes);
        }
    }
    if bound > 0 {
        kept.sort_by_key(|(stamp, ..)| std::cmp::Reverse(*stamp));
        let mut total = 0u64;
        for (_, bytes, path) in kept {
            total += bytes;
            if total > bound {
                remove(&path, bytes);
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
            aliases: env.aliases.iter().map(|(a, b)| (a.0, b.0)).collect(),
            wild: wild.clone(),
        })
    }

    pub fn load(dir: &Path, source: &Path, engine: &str, preload: u64) -> Result<Format, String> {
        let path = cache_file(dir, source, engine, preload);
        let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "none built yet for this satex build and engine".to_string(),
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

    pub fn store(&self, dir: &Path, source: &Path, engine: &str, preload: u64) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let bytes = postcard::to_allocvec(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        let target = cache_file(dir, source, engine, preload);
        let Some(_lock) = StoreLock::try_take(&target) else { return Ok(()) };
        let temporary = temporary_for(&target);
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(temporary, target)?;
        prune(dir, source, engine);
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
            wild: {
                let mut wild = crate::machine::Wild::default();
                wild.add("MT@inh@".into(), String::new());
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
        // The names the kernel may have defined from unknown text.
        assert_eq!(back.wild, sample().wild);
        assert!(!back.wild.is_empty());
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
