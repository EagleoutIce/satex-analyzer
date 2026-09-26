use std::cell::OnceCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::builtins::LoadKind;
use crate::distribution::{self, Distribution, Request, Trees};

fn extensions(kind: LoadKind) -> &'static [&'static str] {
    match kind {
        LoadKind::Class => &["cls"],
        LoadKind::Package | LoadKind::Inherited => &["sty", "cls"],
        LoadKind::Lua => &["lua"],
        _ => &["tex", "ltx", "sty", "def", "cfg", "clo", "fd"],
    }
}

/// Finds a package, class or input file: first beside the document, then in
/// the [`Trees`] of the configured [`Distribution`].
/// [`crate::machine::Machine::load`] calls it once per `\usepackage`-family
/// command.
pub struct Resolver {
    local: Vec<PathBuf>,
    request: Request,
    /// Built on first lookup (lazy: resolving nothing avoids indexing cost).
    trees: OnceCell<Trees>,
    cache: HashMap<(String, LoadKind), Option<PathBuf>>,
    /// Files the build generates from a `.dtx` before TeX runs, by name:
    /// the `.dtx` and the docstrip guards that extract each.
    unpacked: std::collections::BTreeMap<String, (PathBuf, Vec<String>)>,
}

impl Resolver {
    pub fn new(local: Vec<PathBuf>, request: Request) -> Self {
        Self { local, request, trees: OnceCell::new(), cache: HashMap::new(), unpacked: Default::default() }
    }

    pub fn with_unpacked(mut self, unpacked: std::collections::BTreeMap<String, (PathBuf, Vec<String>)>) -> Self {
        self.unpacked = unpacked;
        self
    }

    /// How a resolved `.dtx` is read: with the guards that generate the file
    /// asked for, or, when nothing says, as all its code but the driver.
    pub fn guards(&self, name: &str, kind: LoadKind) -> crate::literate::Guards {
        candidates(name, kind)
            .iter()
            .find_map(|file| self.unpacked.get(file))
            .map(|(_, guards)| crate::literate::Guards::Options(guards.clone()))
            .unwrap_or(crate::literate::Guards::AllButDriver)
    }

    fn trees(&self) -> &Trees {
        self.trees.get_or_init(|| distribution::discover(&self.request))
    }

    pub fn distribution(&self) -> &Distribution {
        &self.trees().distribution
    }

    pub fn resolve(&mut self, name: &str, kind: LoadKind, base: &Path) -> Option<PathBuf> {
        if let Some(hit) = self.cache.get(&(name.to_string(), kind)) {
            return hit.clone();
        }
        let found = self.search(name, kind, base);
        self.cache.insert((name.to_string(), kind), found.clone());
        found
    }

    /// Every name looked up so far and what it resolved to.
    pub fn lookups(&self) -> impl Iterator<Item = (&(String, LoadKind), &Option<PathBuf>)> {
        self.cache.iter()
    }

    /// Look a name up afresh, bypassing what earlier lookups found.
    pub fn search_again(&self, name: &str, kind: LoadKind, base: &Path) -> Option<PathBuf> {
        self.search(name, kind, base)
    }

    fn search(&self, name: &str, kind: LoadKind, base: &Path) -> Option<PathBuf> {
        let candidates = candidates(name, kind);
        for file in &candidates {
            for directory in std::iter::once(base).chain(self.local.iter().map(PathBuf::as_path)) {
                let path = directory.join(file);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        if let Some((dtx, _)) = candidates.iter().find_map(|file| self.unpacked.get(file)) {
            return Some(dtx.clone());
        }
        candidates.iter().find_map(|file| self.trees().get(file).cloned())
    }
}

fn candidates(name: &str, kind: LoadKind) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::with_capacity(4);
    if Path::new(name).extension().is_some() {
        candidates.push(name.to_string());
    }
    for extension in extensions(kind) {
        candidates.push(format!("{name}.{extension}"));
    }
    candidates
}
