//! Which TeX installation to analyze against.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::format::{default_cache_dir, digest, stamp};
use crate::plugin::{Platform, Provider};

const TEXMF_VARIABLES: [&str; 8] =
    ["TEXMFHOME", "TEXMFCONFIG", "TEXMFVAR", "TEXMFLOCAL", "TEXMFSYSCONFIG", "TEXMFSYSVAR", "TEXMFDIST", "TEXMFMAIN"];

/// How the TeX installation's roots were found: configured in `satex.yaml`,
/// reported by `kpsewhich`, guessed at a well-known path, or not found at
/// all. Part of [`Distribution`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Discovery {
    /// Roots named in `satex.yaml`.
    Configured,
    /// Reported by `kpsewhich`.
    Kpsewhich,
    /// Found at a well-known installation path.
    Fallback,
    /// Nothing found.
    None,
}

impl Discovery {
    pub fn as_str(self) -> &'static str {
        match self {
            Discovery::Configured => "configured",
            Discovery::Kpsewhich => "kpsewhich",
            Discovery::Fallback => "fallback",
            Discovery::None => "none",
        }
    }
}

/// The TeX installation a run resolved files against: its roots, the year
/// and LaTeX2e format version it reports, and how many files [`discover`]
/// indexed. [`crate::loader::Resolver`] uses it to find packages and classes.
#[derive(Clone, Debug)]
pub struct Distribution {
    pub roots: Vec<PathBuf>,
    pub discovery: Discovery,
    pub year: Option<u32>,
    pub format_version: Option<String>,
    pub indexed: usize,
    pub index_cached: bool,
    /// The file the index was read from or written to.
    pub index_cache: Option<PathBuf>,
}

impl Distribution {
    pub fn describe(&self) -> String {
        let year = self.year.map_or_else(|| "unknown".into(), |y| y.to_string());
        let format = self.format_version.as_deref().unwrap_or("unknown");
        format!(
            "TeX Live {year}, LaTeX2e {format}, {} trees via {}, {} files indexed{}",
            self.roots.len(),
            self.discovery.as_str(),
            self.indexed,
            if self.index_cached { " (cached)" } else { "" }
        )
    }
}

/// What to look for when resolving a TeX installation: configured roots, a
/// `texlive_root`/year pair, and whether to ask `kpsewhich`. Passed to
/// [`discover`], which returns a [`Trees`].
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub roots: Vec<PathBuf>,
    pub install_root: Option<PathBuf>,
    pub year: Option<u32>,
    pub use_kpsewhich: bool,
    /// Whether an unresolved installation may fall back to well-known system
    /// paths (`Platform::roots`, `$HOME/texmf`). Off simulates a machine
    /// with no TeX installation at all, for testing that path without
    /// uninstalling one.
    pub probe_fallback: bool,
    pub provider: Option<Provider>,
    pub platform: Platform,
    pub cache: bool,
    pub cache_dir: Option<PathBuf>,
    /// TEXINPUTS and the other kpathsea search-path variables satex
    /// resolved ([`crate::paths::effective`]), passed to `kpsewhich` as its
    /// own environment so a lookup it makes honors the same search.
    pub kpse_env: Vec<(String, String)>,
}

/// A resolved TeX installation: its [`Distribution`] description plus the
/// file-name index [`crate::loader::Resolver`] searches.
pub struct Trees {
    pub distribution: Distribution,
    index: Arc<HashMap<String, PathBuf>>,
}

impl Trees {
    pub fn get(&self, file: &str) -> Option<&PathBuf> {
        self.index.get(file)
    }
}

/// Where an installation would resolve from, without indexing its files:
/// the roots [`discover`] would pick, cheap enough for `satex --version` to
/// call on every run.
pub fn locate(request: &Request) -> (Vec<PathBuf>, Discovery) {
    let cache = request.cache.then(|| request.cache_dir.clone().or_else(default_cache_dir)).flatten();
    roots_for(request, cache.as_deref())
}

/// Find the trees for `request`, reusing the index when the roots repeat.
pub fn discover(request: &Request) -> Trees {
    let cache = request.cache.then(|| request.cache_dir.clone().or_else(default_cache_dir)).flatten();
    let (roots, discovery) = roots_for(request, cache.as_deref());
    let (index, index_cached) = index_for(&roots, cache.as_deref());
    let index_cache = cache.as_deref().map(|dir| index_cache_file(dir, &roots));
    let distribution = Distribution {
        year: request.year.or_else(|| roots.iter().find_map(|r| year_in(r))),
        format_version: format_version(&index),
        indexed: index.len(),
        index_cached,
        index_cache,
        roots,
        discovery,
    };
    Trees { distribution, index }
}

type Roots = (Vec<PathBuf>, Discovery);

fn roots_for(request: &Request, cache: Option<&Path>) -> Roots {
    if !request.roots.is_empty() {
        let roots: Vec<PathBuf> = request.roots.iter().filter(|r| r.is_dir()).cloned().collect();
        if !roots.is_empty() {
            return (roots, Discovery::Configured);
        }
    }
    if let (Some(install), Some(year)) = (&request.install_root, request.year) {
        let roots: Vec<PathBuf> = ["texmf-dist", "texmf-local", "../texmf-local"]
            .iter()
            .map(|tree| install.join(year.to_string()).join(tree))
            .filter(|p| p.is_dir())
            .collect();
        if !roots.is_empty() {
            return (roots, Discovery::Configured);
        }
    }
    if request.use_kpsewhich
        && let Some(provider) = request.provider
    {
        let roots = kpse_roots(provider.locator(), cache, &request.kpse_env);
        if !roots.is_empty() {
            return (roots, Discovery::Kpsewhich);
        }
    }
    if !request.probe_fallback {
        return (Vec::new(), Discovery::None);
    }
    let roots = fallback_roots(request.year, request.platform);
    let discovery = if roots.is_empty() { Discovery::None } else { Discovery::Fallback };
    (roots, discovery)
}

fn fallback_roots(year: Option<u32>, platform: Platform) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(personal) = platform.home_tree()
        && personal.is_dir()
    {
        roots.push(personal);
    }
    for pattern in platform.roots() {
        let Ok(matches) = glob::glob(pattern) else { continue };
        // A pattern matches one directory per installed version: the wanted
        // year wins, otherwise the newest.
        let mut found: Vec<PathBuf> = matches.flatten().filter(|path| path.is_dir()).collect();
        found.sort();
        let chosen = match year {
            Some(year) => found.into_iter().find(|path| year_in(path) == Some(year)),
            None => found.pop(),
        };
        if let Some(path) = chosen {
            roots.push(path);
        }
    }
    roots
}

fn year_in(path: &Path) -> Option<u32> {
    path.components().find_map(|component| {
        let text = component.as_os_str().to_str()?;
        let year: u32 = text.parse().ok()?;
        (1990..=2100).contains(&year).then_some(year)
    })
}

type Index = Arc<HashMap<String, PathBuf>>;

/// The stamp of what a tree was indexed from, so that a cached index is
/// discarded once the installation changes.
fn tree_stamp(root: &Path) -> (u64, u64) {
    stamp(&root.join("ls-R")).or_else(|| stamp(&root.join("tex"))).unwrap_or((0, 0))
}

#[derive(Serialize, Deserialize)]
struct CachedIndex {
    version: u32,
    stamps: Vec<(String, u64, u64)>,
    entries: Vec<(String, PathBuf)>,
}

/// Part of the cache file name, so an index written by an older encoding is
/// simply a different file.
const INDEX_VERSION: u32 = 1;

fn index_cache_file(dir: &Path, roots: &[PathBuf]) -> PathBuf {
    let joined: Vec<u8> = roots.iter().flat_map(|r| r.as_os_str().as_encoded_bytes().to_vec()).collect();
    let tree = roots.first().map_or_else(|| "tree".to_string(), |r| crate::format::tree_name(r));
    dir.join(format!("index-{tree}-v{INDEX_VERSION}-{:08x}.postcard", digest(&joined)))
}

fn stamps_for(roots: &[PathBuf]) -> Vec<(String, u64, u64)> {
    roots
        .iter()
        .map(|root| {
            let (size, modified) = tree_stamp(root);
            (root.display().to_string(), size, modified)
        })
        .collect()
}

fn read_cached_index(dir: &Path, roots: &[PathBuf]) -> Option<HashMap<String, PathBuf>> {
    let bytes = std::fs::read(index_cache_file(dir, roots)).ok()?;
    let cached: CachedIndex = postcard::from_bytes(&bytes).ok()?;
    (cached.version == INDEX_VERSION && cached.stamps == stamps_for(roots))
        .then(|| cached.entries.into_iter().collect())
}

fn write_cached_index(dir: &Path, roots: &[PathBuf], index: &HashMap<String, PathBuf>) {
    let cached = CachedIndex {
        version: INDEX_VERSION,
        stamps: stamps_for(roots),
        entries: index.iter().map(|(name, path)| (name.clone(), path.clone())).collect(),
    };
    let Ok(bytes) = postcard::to_allocvec(&cached) else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let path = index_cache_file(dir, roots);
    let temporary = path.with_extension("tmp");
    if std::fs::write(&temporary, &bytes).is_ok() {
        let _ = std::fs::rename(&temporary, &path);
    }
}

/// One process usually analyzes against one installation, so the last index
/// is kept rather than rebuilt per run, and a cache file keeps it between
/// runs: indexing a full TeX Live is far more work than reading it back.
fn index_for(roots: &[PathBuf], cache: Option<&Path>) -> (Index, bool) {
    if let Ok(entries) = cache_entries().lock()
        && let Some((_, index)) = entries.iter().find(|(known, _)| known == roots)
    {
        return (index.clone(), true);
    }
    if let Some(dir) = cache
        && let Some(index) = read_cached_index(dir, roots)
    {
        let index: Index = Arc::new(index);
        remember(cache_entries(), roots, &index);
        return (index, true);
    }
    let mut index = HashMap::new();
    // Later roots must not shadow earlier ones, so they are read in order and
    // only fill in names that are still missing.
    for root in roots {
        let mut tree: HashMap<String, (u8, PathBuf)> = HashMap::new();
        if !read_ls_r(root, &mut tree) {
            walk(&root.join("tex"), &mut tree);
        }
        for (name, (_, path)) in tree {
            index.entry(name).or_insert(path);
        }
    }
    if let Some(dir) = cache {
        write_cached_index(dir, roots, &index);
    }
    let index: Index = Arc::new(index);
    remember(cache_entries(), roots, &index);
    (index, false)
}

type Cached = Vec<(Vec<PathBuf>, Index)>;

/// The indexes this process has already built.
fn cache_entries() -> &'static Mutex<Cached> {
    static CACHE: OnceLock<Mutex<Cached>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

/// How many installations one process keeps in memory at once.
const REMEMBERED: usize = 4;

fn remember(cache: &Mutex<Cached>, roots: &[PathBuf], index: &Index) {
    if let Ok(mut entries) = cache.lock() {
        entries.push((roots.to_vec(), index.clone()));
        if entries.len() > REMEMBERED {
            entries.remove(0);
        }
    }
}

/// TeX searches TEXINPUTS; `latex` outranks `generic` (kpathsea manual, "Supported file formats").
fn rank(relative: &str) -> Option<u8> {
    let rest = relative.strip_prefix("tex")?;
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    Some(format_rank(rest.split('/').next().unwrap_or_default()))
}

fn format_rank(directory: &str) -> u8 {
    match directory {
        "latex" => 0,
        "generic" => 1,
        _ => 2,
    }
}

fn offer(index: &mut HashMap<String, (u8, PathBuf)>, name: &str, rank: u8, path: PathBuf) {
    match index.get(name) {
        Some((known, _)) if *known <= rank => {}
        _ => {
            index.insert(name.to_string(), (rank, path));
        }
    }
}

/// `ls-R` lists a directory as `./path:` followed by its entries.
fn read_ls_r(root: &Path, index: &mut HashMap<String, (u8, PathBuf)>) -> bool {
    let Ok(text) = std::fs::read_to_string(root.join("ls-R")) else { return false };
    let mut current: Option<(PathBuf, u8)> = None;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('%') {
            continue;
        }
        match line.strip_suffix(':') {
            Some(relative) => {
                let relative = relative.trim_start_matches("./");
                current = rank(relative).map(|rank| (root.join(relative), rank));
            }
            None => {
                if let Some((directory, rank)) = &current {
                    offer(index, line, *rank, directory.join(line));
                }
            }
        }
    }
    true
}

/// Deep enough for `tex/latex/<bundle>/<package>/<file>` and then some.
const MAX_WALK_DEPTH: usize = 8;

/// Index a tree that ships no `ls-R`.
fn walk(directory: &Path, index: &mut HashMap<String, (u8, PathBuf)>) {
    for entry in walkdir::WalkDir::new(directory)
        .max_depth(MAX_WALK_DEPTH)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let Some(name) = entry.file_name().to_str() else { continue };
        // The directory directly under `tex` names the format whose files it
        // holds, and so gives the whole subtree its rank.
        let rank = entry
            .path()
            .strip_prefix(directory)
            .ok()
            .and_then(|relative| relative.components().next())
            .and_then(|component| component.as_os_str().to_str())
            .map_or(2, format_rank);
        offer(index, name, rank, entry.path().to_path_buf());
    }
}

/// `latex.ltx` carries the format date, written as `\edef\fmtversion` with
/// the value on the following line, plus a patch level.
fn format_version(index: &HashMap<String, PathBuf>) -> Option<String> {
    let text = std::fs::read_to_string(index.get("latex.ltx")?).ok()?;
    let version = braced_after(&text, "\\fmtversion")?;
    Some(match braced_after(&text, "\\def\\patch@level") {
        Some(patch) if patch != "0" => format!("{version} patch level {patch}"),
        _ => version,
    })
}

/// The contents of the first `{…}` after `marker`.
fn braced_after(text: &str, marker: &str) -> Option<String> {
    let at = text.find(marker)? + marker.len();
    let rest = &text[at..];
    let open = rest.find('{')?;
    if rest[..open].contains('\n') && rest[..open].trim_start().starts_with(char::is_alphabetic) {
        return None;
    }
    let close = rest[open + 1..].find('}')?;
    Some(rest[open + 1..open + 1 + close].trim().to_string())
}

/// A separator kpathsea does not give a meaning of its own, so that every
/// tree can be expanded in one call: starting the engine costs far more than
/// the expansion itself.
const KPSE_SEPARATOR: char = '|';

/// What a locator program answered, kept so that a warm run does not start a
/// subprocess: `kpsewhich` costs more than reading the whole index back.  The
/// answer is tied to the program's own path and stamp, so an installation
/// that is upgraded is asked again.
#[derive(Serialize, Deserialize)]
struct CachedProbe {
    version: u32,
    program: String,
    size: u64,
    modified: u64,
    answer: Vec<String>,
}

const PROBE_VERSION: u32 = 1;

fn probe_cache_file(dir: &Path, program: &Path, topic: &str) -> PathBuf {
    let hash = digest(program.as_os_str().as_encoded_bytes());
    dir.join(format!("probe-{topic}-v{PROBE_VERSION}-{hash:08x}.postcard"))
}

/// Run `ask` unless the answer for this program is already cached, in which
/// case the cached lines come back.  A program that cannot be found on
/// `PATH`, or a run without a cache directory, simply asks every time.
pub(crate) fn probed(
    program: &str,
    topic: &str,
    cache: Option<&Path>,
    ask: impl FnOnce() -> Vec<String>,
) -> Vec<String> {
    let path = which::which(program).ok();
    let stamped = path.as_deref().and_then(|path| stamp(path).map(|s| (path, s)));
    let file = match (cache, &stamped) {
        (Some(dir), Some((path, _))) => Some(probe_cache_file(dir, path, topic)),
        _ => None,
    };
    if let (Some(file), Some((path, (size, modified)))) = (&file, &stamped)
        && let Ok(bytes) = std::fs::read(file)
        && let Ok(probe) = postcard::from_bytes::<CachedProbe>(&bytes)
        && probe.version == PROBE_VERSION
        && probe.program == path.display().to_string()
        && probe.size == *size
        && probe.modified == *modified
    {
        return probe.answer;
    }
    let answer = ask();
    if let (Some(file), Some((path, (size, modified))), false) = (&file, &stamped, answer.is_empty()) {
        let probe = CachedProbe {
            version: PROBE_VERSION,
            program: path.display().to_string(),
            size: *size,
            modified: *modified,
            answer: answer.clone(),
        };
        if let Ok(bytes) = postcard::to_allocvec(&probe) {
            let _ = std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")));
            let temporary = file.with_extension("tmp");
            if std::fs::write(&temporary, &bytes).is_ok() {
                let _ = std::fs::rename(&temporary, file);
            }
        }
    }
    answer
}

fn kpse_roots(locator: &str, cache: Option<&Path>, env: &[(String, String)]) -> Vec<PathBuf> {
    let fields = probed(locator, "roots", cache, || {
        let request: Vec<String> = TEXMF_VARIABLES.iter().map(|name| format!("${name}")).collect();
        let Ok(output) = std::process::Command::new(locator)
            .arg(format!("--expand-var={}", request.join(&KPSE_SEPARATOR.to_string())))
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .output()
        else {
            return Vec::new();
        };
        let Ok(text) = String::from_utf8(output.stdout) else { return Vec::new() };
        text.trim().split(KPSE_SEPARATOR).map(|field| field.trim().to_string()).collect()
    });
    let mut roots: Vec<PathBuf> = Vec::new();
    for field in fields {
        // `!!` tells kpathsea to trust `ls-R` alone; it is not part of the path.
        let path = PathBuf::from(field.trim().trim_start_matches("!!"));
        if path.is_dir() && !roots.contains(&path) {
            roots.push(path);
        }
    }
    roots
}
