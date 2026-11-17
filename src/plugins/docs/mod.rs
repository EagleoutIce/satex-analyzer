//! What a package's own documentation says about a command, for hover.
//! Each established doc convention is a [`DocPlugin`]: it reads the
//! package's documented sources (`texmf-dist/source/…`, `…/doc/…`) and
//! adds entries or links. Mining is best effort and runs off the analysis
//! thread: a missing or broken file yields nothing, and a lookup never
//! waits. A package's entries are mined once per process.

mod l3doc;
mod labels;
mod ltxdoc;
pub(crate) mod markup;
pub mod mined;
mod texdoc;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::project::config::DocsConfig;

/// Whether an entry documents a command for users or its implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    User,
    Code,
}

/// One documented command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Without the backslash.
    pub name: String,
    /// The usage line, `\name{⟨arg⟩}…`, when the documentation gives one.
    pub usage: Option<String>,
    /// The first sentence of its description, Markdown.
    pub text: Option<String>,
    pub audience: Audience,
    /// The stem of the file it was documented in.
    pub from: String,
    /// The manual that shows it: label and URL.
    pub link: Option<(String, String)>,
    /// Where it is documented.
    pub at: Option<Place>,
}

/// A place in a documented source: its file, 1-based line and the heading it stands under.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub path: String,
    pub line: usize,
    pub section: Option<String>,
}

/// A package's documented sources: its `source/…` and `doc/…` directories.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Dirs {
    pub source: Option<PathBuf>,
    pub doc: Option<PathBuf>,
}

/// One doc convention.
pub trait DocPlugin: Sync {
    /// Adds the entries this convention documents in `text` of `file`.
    fn mine(&self, _config: &DocsConfig, _file: &Path, _text: &str, _out: &mut Vec<Entry>) {}
    /// Links the entries mined from `dirs` to their manual.
    fn link(&self, _config: &DocsConfig, _dirs: &Dirs, _entries: &mut [Entry]) {}
}

/// The doc conventions, in order: a new one is one more line.
pub const PLUGINS: &[&dyn DocPlugin] =
    &[&ltxdoc::Ltxdoc, &l3doc::L3doc, &labels::Labels, &texdoc::Texdoc, &mined::Mined];

impl Dirs {
    /// The documented sources of the package `file` belongs to: for
    /// `⟨root⟩/tex/⟨path⟩/f.sty`, the deepest `⟨root⟩/source/⟨path…⟩` and
    /// `⟨root⟩/doc/⟨path…⟩` that exist (TDS). Its format folder may differ
    /// there: pgf runs from `tex/latex/pgf`, its manual is in `doc/generic/pgf`.
    /// Another format's folder counts only when no third has one by that
    /// name: `doc/latex/base` and `doc/etex/base` are no plain TeX's.
    pub fn of(config: &DocsConfig, file: &Path) -> Option<Dirs> {
        let parts: Vec<_> = file.parent()?.components().collect();
        let at = parts.iter().rposition(|c| c.as_os_str() == "tex")?;
        let root: PathBuf = parts[..at].iter().collect();
        let rest = &parts[at + 1..];
        let (format, package) = rest.split_first()?;
        let find = |top: &str| {
            let top = root.join(top);
            let under = |f: &Path, n: usize| package[..n].iter().fold(f.to_path_buf(), |p, c| p.join(c));
            let own = (1..=package.len()).rev().map(|n| under(&top.join(format), n)).find(|d| d.is_dir());
            own.or_else(|| {
                let formats: Vec<PathBuf> =
                    std::fs::read_dir(&top).into_iter().flatten().flatten().map(|e| e.path()).collect();
                (1..=package.len()).rev().find_map(|n| {
                    let mut found = formats.iter().map(|f| under(f, n)).filter(|d| d.is_dir());
                    found.next().filter(|_| found.next().is_none())
                })
            })
        };
        let dirs = Dirs { source: find(&config.source_root), doc: find(&config.doc_root) };
        (dirs.source.is_some() || dirs.doc.is_some()).then_some(dirs)
    }

    fn files(&self, config: &DocsConfig) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = [&self.source, &self.doc]
            .into_iter()
            .flatten()
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| config.extensions.iter().any(|m| e == m.as_str())) && p.is_file())
            .collect();
        out.sort();
        out
    }
}

/// Every entry `plugins` find in `dirs`, by name: a user entry before an
/// implementation one, then the first found.
pub fn mine(plugins: &[&dyn DocPlugin], config: &DocsConfig, dirs: &Dirs) -> HashMap<String, Entry> {
    let mut entries = Vec::new();
    for file in dirs.files(config) {
        let Ok(bytes) = std::fs::read(&file) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        for plugin in plugins {
            // A plugin that fails on one file loses only that file.
            let mut found = Vec::new();
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plugin.mine(config, &file, &text, &mut found)))
                .is_ok()
            {
                entries.append(&mut found);
            }
        }
    }
    for plugin in plugins {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| plugin.link(config, dirs, &mut entries)));
    }
    // A file implements a name where its code documents it (`\begin{macro}`):
    // its own manual speaks for it, not another that only mentions it.
    let implements: std::collections::HashSet<(String, String)> =
        entries.iter().filter(|e| e.audience == Audience::Code).map(|e| (e.name.clone(), e.from.clone())).collect();
    let rank = |e: &Entry| {
        let own = implements.contains(&(e.name.clone(), e.from.clone()));
        (own || !implements.iter().any(|(n, _)| *n == e.name), e.audience == Audience::User)
    };
    let mut out: HashMap<String, Entry> = HashMap::new();
    for e in entries {
        match out.get_mut(&e.name) {
            Some(old) => {
                // Implementation prose never stands in for a user's description.
                let better = rank(&e) > rank(old)
                    || (rank(&e) == rank(old) && old.text.is_none() && e.text.is_some() && e.audience == old.audience);
                if better {
                    let usage = old.usage.take();
                    *old = e;
                    old.usage = old.usage.take().or(usage);
                } else if old.usage.is_none() {
                    old.usage = e.usage;
                }
            }
            None => {
                out.insert(e.name.clone(), e);
            }
        }
    }
    out
}

/// What the package `file` is, in a line, best source first: the
/// installation's catalogue (`tlpkg/texlive.tlpdb`'s `shortdesc`), the
/// first sentence of its README, the text of its `\ProvidesPackage`.
pub fn about(config: &DocsConfig, file: &Path) -> Option<String> {
    catalogue_of(file)
        .and_then(|c| c.line(file).map(str::to_string))
        .or_else(|| readme(Dirs::of(config, file)?.doc.as_deref()?))
        .or_else(|| provided(&std::fs::read_to_string(file).ok()?))
}

/// A TeX Live database's `shortdesc` of each run file's package, by the
/// file's path below the directory that holds `tlpkg`.
pub struct Catalogue {
    base: PathBuf,
    lines: HashMap<String, String>,
}

impl Catalogue {
    /// The description of the package `file` belongs to: the one that lists that very
    /// file, so a development pre-release listing the same name does not answer.
    pub fn line(&self, file: &Path) -> Option<&str> {
        let rel = file.strip_prefix(&self.base).ok()?;
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        self.lines.get(&rel).map(String::as_str)
    }
}

/// The catalogue of the installation `path` belongs to.
pub fn catalogue_of(path: &Path) -> Option<Arc<Catalogue>> {
    path.ancestors().find_map(catalogue)
}

/// The catalogue in `base`'s `tlpkg/texlive.tlpdb`, read once.
fn catalogue(base: &Path) -> Option<Arc<Catalogue>> {
    static READ: Mutex<Option<HashMap<PathBuf, Option<Arc<Catalogue>>>>> = Mutex::new(None);
    let mut read = READ.lock().ok()?;
    let read = read.get_or_insert_with(HashMap::new);
    read.entry(base.to_path_buf())
        .or_insert_with(|| {
            let text = std::fs::read_to_string(base.join("tlpkg/texlive.tlpdb")).ok()?;
            let mut lines = HashMap::new();
            for block in text.split("\n\n") {
                let Some(desc) = block.lines().find_map(|l| l.strip_prefix("shortdesc ")) else { continue };
                for file in block.lines().filter(|l| l.starts_with(' ')) {
                    let rel = file.split_whitespace().next().unwrap_or_default();
                    lines.entry(rel.to_string()).or_insert_with(|| desc.trim().to_string());
                }
            }
            Some(Arc::new(Catalogue { base: base.to_path_buf(), lines }))
        })
        .clone()
}

/// The first sentence of a README's first paragraph of prose.
fn readme(doc: &Path) -> Option<String> {
    let path =
        std::fs::read_dir(doc).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("readme")) && p.is_file()
        })?;
    let text = std::fs::read_to_string(path).ok()?;
    let para = text
        .split("\n\n")
        .map(|p| p.lines().map(str::trim).collect::<Vec<_>>().join(" "))
        .find(|p| p.split_whitespace().count() > 5 && p.starts_with(char::is_alphabetic))?;
    markup::sentence(&para)
}

/// The text after date and version in `\ProvidesPackage{x}[date version text]`.
fn provided(code: &str) -> Option<String> {
    let at = code.find("\\Provides")?;
    let (rest, _) = markup::group(code[at..].split_once('}')?.1, '[', ']')?;
    let version = |w: &str| w.contains('/') || w.trim_start_matches('v').starts_with(|c: char| c.is_ascii_digit());
    let words: Vec<&str> = rest.split_whitespace().skip_while(|w| version(w)).collect();
    (!words.is_empty()).then(|| words.join(" "))
}

type Entries = Arc<HashMap<String, Entry>>;
type Subscriber = Box<dyn Fn(&[String]) + Send + Sync>;

#[derive(Default)]
struct State {
    /// Each directory's package, found once.
    dirs: HashMap<PathBuf, Option<Dirs>>,
    /// Each package: `None` while queued or mined.
    slots: HashMap<Dirs, Option<Entries>>,
    queue: std::collections::VecDeque<Dirs>,
    working: bool,
}

/// The mined entries of each package, filled by one background thread.
pub struct Index {
    plugins: &'static [&'static dyn DocPlugin],
    config: DocsConfig,
    state: Mutex<State>,
    subscribers: Mutex<Vec<Subscriber>>,
}

impl Index {
    pub fn new(plugins: &'static [&'static dyn DocPlugin], config: DocsConfig) -> Arc<Index> {
        Arc::new(Index { plugins, config, state: Mutex::default(), subscribers: Mutex::default() })
    }

    /// Calls `f` with the names of each package once it is mined.
    pub fn subscribe(&self, f: Subscriber) {
        if let Ok(mut s) = self.subscribers.lock() {
            s.push(f);
        }
    }

    pub fn request(self: &Arc<Self>, file: &Path) {
        if let Some(dirs) = self.dirs(file) {
            self.start(dirs, false);
        }
    }

    /// What the package of `file` documents about `name`, if mined by now;
    /// else nothing, and mining starts.
    pub fn lookup(self: &Arc<Self>, file: &Path, name: &str) -> Option<Entry> {
        self.lookup_in(self.dirs(file)?, name, false)
    }

    /// [`Index::lookup`], mining the package here when no one has yet.
    pub fn lookup_now(self: &Arc<Self>, file: &Path, name: &str) -> Option<Entry> {
        self.lookup_in(self.dirs(file)?, name, true)
    }

    /// What the manuals in `dirs` document about `name`; `wait` mines them here when no one has yet.
    pub fn lookup_in(self: &Arc<Self>, dirs: Dirs, name: &str, wait: bool) -> Option<Entry> {
        if !wait {
            return self.start(dirs, true)?.get(name).cloned();
        }
        {
            let mut state = self.state.lock().ok()?;
            match state.slots.get(&dirs) {
                Some(slot) => return slot.as_ref()?.get(name).cloned(),
                None => state.slots.insert(dirs.clone(), None),
            };
        }
        self.fill(dirs).get(name).cloned()
    }

    /// Whether someone asks again once a package is mined: a lookup then need not wait.
    pub fn listened(&self) -> bool {
        self.subscribers.lock().is_ok_and(|s| !s.is_empty())
    }

    /// The documented sources of `file`'s package, found once per directory.
    pub fn dirs(&self, file: &Path) -> Option<Dirs> {
        let mut state = self.state.lock().ok()?;
        state.dirs.entry(file.parent()?.to_path_buf()).or_insert_with(|| Dirs::of(&self.config, file)).clone()
    }

    pub fn knows(&self, name: &str) -> bool {
        let Ok(state) = self.state.lock() else { return false };
        state.slots.values().flatten().any(|e| e.contains_key(name))
    }

    /// The entries of `dirs` when mined; else queues them. Where no
    /// thread starts (the browser) a lookup (`inline`) mines them at once.
    fn start(self: &Arc<Self>, dirs: Dirs, inline: bool) -> Option<Entries> {
        {
            let mut state = self.state.lock().ok()?;
            if let Some(slot) = state.slots.get(&dirs) {
                return slot.clone();
            }
            state.slots.insert(dirs.clone(), None);
            state.queue.push_back(dirs.clone());
            if state.working {
                return None;
            }
            state.working = true;
        }
        let me = self.clone();
        if std::thread::Builder::new().name("satex-docs".into()).spawn(move || me.work()).is_ok() {
            return None;
        }
        let mut state = self.state.lock().ok()?;
        state.working = false;
        state.queue.retain(|d| *d != dirs);
        state.slots.remove(&dirs);
        drop(state);
        inline.then(|| self.fill(dirs))
    }

    fn work(&self) {
        loop {
            let next = match self.state.lock() {
                Ok(mut state) => {
                    let next = state.queue.pop_front();
                    state.working = next.is_some();
                    next
                }
                Err(_) => None,
            };
            let Some(dirs) = next else { return };
            self.fill(dirs);
        }
    }

    fn fill(&self, dirs: Dirs) -> Entries {
        let entries = Arc::new(mine(self.plugins, &self.config, &dirs));
        if let Ok(mut state) = self.state.lock() {
            state.slots.insert(dirs, Some(entries.clone()));
        }
        let names: Vec<String> = entries.keys().cloned().collect();
        if let Ok(subscribers) = self.subscribers.lock() {
            for f in subscribers.iter() {
                f(&names);
            }
        }
        entries
    }
}

/// The index of this process, over [`PLUGINS`]. The first use fixes its settings.
pub fn index(config: &DocsConfig) -> &'static Arc<Index> {
    static INDEX: OnceLock<Arc<Index>> = OnceLock::new();
    INDEX.get_or_init(|| Index::new(PLUGINS, config.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn tree(tag: &str) -> PathBuf {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("target/scratch/docs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["tex/latex/pkg", "source/latex/pkg", "doc/latex/pkg"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::write(root.join("tex/latex/pkg/pkg.sty"), "").unwrap();
        root
    }

    fn wait(index: &Arc<Index>, file: &Path, name: &str) -> Option<Entry> {
        (0..500).find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(10));
            index.lookup(file, name)
        })
    }

    static RELEASED: AtomicBool = AtomicBool::new(false);
    struct Slow;
    impl DocPlugin for Slow {
        fn mine(&self, _: &DocsConfig, file: &Path, _: &str, out: &mut Vec<Entry>) {
            while !RELEASED.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            out.push(Entry {
                name: "foo".into(),
                usage: None,
                text: Some("Slow.".into()),
                audience: Audience::User,
                from: file.to_string_lossy().into(),
                link: None,
                at: None,
            });
        }
    }

    #[test]
    fn a_lookup_never_waits_for_mining() {
        let root = tree("slow");
        std::fs::write(root.join("source/latex/pkg/pkg.dtx"), "").unwrap();
        const SLOW: &[&dyn DocPlugin] = &[&Slow];
        let index = Index::new(SLOW, DocsConfig::default());
        let sty = root.join("tex/latex/pkg/pkg.sty");
        assert_eq!(index.lookup(&sty, "foo"), None);
        RELEASED.store(true, Ordering::SeqCst);
        assert_eq!(wait(&index, &sty, "foo").and_then(|e| e.text), Some("Slow.".into()));
        let _ = std::fs::remove_dir_all(&root);
    }

    struct Panics;
    impl DocPlugin for Panics {
        fn mine(&self, _: &DocsConfig, _: &Path, text: &str, _: &mut Vec<Entry>) {
            assert!(!text.contains("boom"));
        }
    }
    const BROKEN: &[&dyn DocPlugin] = &[&Panics, &ltxdoc::Ltxdoc, &l3doc::L3doc, &texdoc::Texdoc];

    #[test]
    fn broken_sources_yield_what_they_can() {
        let root = tree("broken");
        let src = root.join("source/latex/pkg");
        std::fs::write(src.join("a.dtx"), b"% \\begin{function}{\\x:n\n% \\begin{syntax}\xff\xfe boom {{{").unwrap();
        std::fs::create_dir_all(src.join("dir.dtx")).unwrap();
        std::fs::write(src.join("b.dtx"), "% \\DescribeMacro{\\bar}\n% Draws a bar.\n").unwrap();
        let index = Index::new(BROKEN, DocsConfig::default());
        let sty = root.join("tex/latex/pkg/pkg.sty");
        index.lookup(&sty, "bar");
        assert_eq!(wait(&index, &sty, "bar").and_then(|e| e.text), Some("Draws a bar.".into()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
