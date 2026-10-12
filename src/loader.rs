use crate::fixed::HashMap;
use std::cell::{OnceCell, RefCell};
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
    /// The build's output directory, searched first for a relative name.
    output: Option<PathBuf>,
    request: Request,
    /// Built on first lookup (lazy: resolving nothing avoids indexing cost).
    trees: OnceCell<Trees>,
    cache: HashMap<(String, LoadKind), Option<PathBuf>>,
    /// Files the build generates from a `.dtx` before TeX runs, by name:
    /// the `.dtx` and the docstrip guards that extract each.
    unpacked: std::collections::BTreeMap<String, (PathBuf, Vec<String>)>,
    /// Case mismatches found: (asked_name, found_name on disk) for each lookup.
    case_mismatches: RefCell<HashMap<(String, LoadKind), (String, String)>>,
}

impl Resolver {
    pub fn new(local: Vec<PathBuf>, request: Request) -> Self {
        Self {
            local,
            output: None,
            request,
            trees: OnceCell::new(),
            cache: HashMap::default(),
            unpacked: Default::default(),
            case_mismatches: RefCell::new(HashMap::default()),
        }
    }

    /// The resolver a run of `cfg` on `project` searches with: TEXINPUTS
    /// and the rest in kpathsea's precedence, then the distribution.
    pub fn for_project(cfg: &crate::config::Config, project: &crate::project::Project) -> Self {
        let kpse = crate::paths::effective_all(project, &cfg.paths, &project.root);
        let texinputs = kpse.iter().find(|e| e.variable == "TEXINPUTS").map(|e| e.dirs.clone()).unwrap_or_default();
        let search = cfg.search_paths.iter().cloned().chain(project.search_paths()).chain(texinputs).collect();
        let mut request = cfg.distribution_request();
        request.kpse_env = crate::paths::env_vars(&kpse);
        let output = crate::paths::output_dir(project, &cfg.paths.output_directory, &project.root).map(|o| o.dir);
        Self::new(search, request).with_output(output).with_unpacked(project.unpacked())
    }

    /// Like web2c with `-output-directory`: a relative name is tried in `dir` first.
    pub fn with_output(mut self, dir: Option<PathBuf>) -> Self {
        self.output = dir;
        self
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
        let found = self.search_uncached(name, kind, base);
        self.cache.insert((name.to_string(), kind), found.clone());
        found
    }

    /// Every name looked up so far and what it resolved to.
    pub fn lookups(&self) -> impl Iterator<Item = (&(String, LoadKind), &Option<PathBuf>)> {
        self.cache.iter()
    }

    /// Returns the case mismatch detected for a name, if any: (asked, found).
    pub fn case_mismatch(&self, name: &str, kind: LoadKind) -> Option<(String, String)> {
        self.case_mismatches.borrow().get(&(name.to_string(), kind)).cloned()
    }

    /// Look a name up afresh, bypassing what earlier lookups found.
    pub(crate) fn search_uncached(&self, name: &str, kind: LoadKind, base: &Path) -> Option<PathBuf> {
        let candidates = candidates(name, kind);
        for file in &candidates {
            let output = self.output.as_deref().filter(|_| !Path::new(file).is_absolute());
            for directory in
                output.into_iter().chain(std::iter::once(base)).chain(self.local.iter().map(PathBuf::as_path))
            {
                if let Some(path) = exact_file(directory, file, HOST) {
                    return Some(path);
                }
                if let Some(found_name) = case_folded_name(directory, file) {
                    self.case_mismatches.borrow_mut().insert((name.to_string(), kind), (file.to_string(), found_name));
                }
            }
        }
        if let Some((dtx, _)) = candidates.iter().find_map(|file| self.unpacked.get(file)) {
            return Some(dtx.clone());
        }
        // The distribution index is keyed by bare file names, so a name that
        // spells a path names nothing in it and must not be searched for.
        candidates.iter().filter(|file| !spells_a_path(file)).find_map(|file| self.trees().get(file).cloned())
    }
}

/// Whether a name spells a path, like `../template/x.cls`, rather than the
/// file name a distribution indexes.
fn spells_a_path(name: &str) -> bool {
    Path::new(name).parent().is_some_and(|parent| !parent.as_os_str().is_empty())
}

/// How a host's filesystem matches a name: NTFS and default APFS open a
/// file whatever case it is asked for, ext4 does not.  Named rather than
/// `cfg!`-tested inline, so one test run covers both.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Fs {
    folding: bool,
}

/// The host answers for itself, so nothing here has to model its case rule.
pub(crate) const HOST: Fs = Fs { folding: false };
/// A folding filesystem, for testing that branch away from Windows and macOS.
#[cfg(test)]
pub(crate) const FOLDING: Fs = Fs { folding: true };

impl Fs {
    fn is_file(self, path: &Path) -> bool {
        path.is_file() || (self.folding && folded_file(path))
    }
}

/// Whether `path` names a file once case is ignored, which is what the
/// filesystem call itself does on a folding host.
fn folded_file(path: &Path) -> bool {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return false };
    let Some(name) = name.to_str() else { return false };
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries
        .flatten()
        .any(|e| e.file_name().to_str().is_some_and(|f| f.eq_ignore_ascii_case(name)) && e.path().is_file())
}

/// When an exact match fails, check if a case-insensitive match exists by
/// reading the directory and comparing case-insensitively. Returns the name
/// as found on disk if one exists, or None. This only checks the immediate
/// parent directory, not nested directories.
fn case_folded_name(dir: &Path, rel: &str) -> Option<String> {
    let path = Path::new(rel);
    let name_str = path.file_name()?.to_str()?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let Ok(entries) = std::fs::read_dir(dir.join(parent)) else { return None };
    entries.flatten().find_map(|e| {
        if !e.path().is_file() {
            return None;
        }
        let file_name = e.file_name();
        let file_str = file_name.to_str()?;
        if file_str.eq_ignore_ascii_case(name_str) && !file_str.eq(name_str) {
            Some(file_str.to_string())
        } else {
            None
        }
    })
}

/// One step of a name, as the walk that checks its spelling takes it.
enum Step {
    /// A directory entry, which the parent directory has to list verbatim.
    Name(std::ffi::OsString),
    /// `..`, which names no entry of the directory it is written in.
    Up,
    /// The root an absolute name starts from, which names no entry either.
    Root(std::ffi::OsString),
}

/// `rel` lexically normalized: `.` drops out and `..` pops the name before
/// it.  A `..` with nothing to pop stays, because the directory it leads to
/// is the caller's to reach, not an entry to check.
fn steps(rel: &Path) -> Vec<Step> {
    use std::path::Component;
    let mut steps = Vec::new();
    for component in rel.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(steps.last(), Some(Step::Name(_))) {
                    steps.pop();
                } else {
                    steps.push(Step::Up);
                }
            }
            Component::Normal(name) => steps.push(Step::Name(name.to_os_string())),
            root => steps.push(Step::Root(root.as_os_str().to_os_string())),
        }
    }
    steps
}

/// `dir/rel`, when every component of it that names a directory entry is
/// spelled on disk exactly as written.  kpathsea matches its `ls-R` index
/// case-sensitively, and so does [`Trees::get`], so a name that only opens
/// because NTFS or APFS folds case would resolve here and nowhere else,
/// including on the Linux machine that builds the document for real.
/// `.` and `..` name no entry, so they carry no spelling to check, and `..`
/// leading out of `dir` is what TeX itself does with `../template/x.cls`.
fn exact_file(dir: &Path, rel: &str, fs: Fs) -> Option<PathBuf> {
    let path = dir.join(rel);
    if !fs.is_file(&path) {
        return None;
    }
    let mut at = dir.to_path_buf();
    let mut rooted = false;
    for step in steps(Path::new(rel)) {
        match step {
            Step::Up => at.push(".."),
            // An absolute name starts from its own root, not from `dir`.
            // Windows spells that root in two steps, a prefix and a `\`.
            Step::Root(root) => {
                if !std::mem::replace(&mut rooted, true) {
                    at = PathBuf::new();
                }
                at.push(root);
            }
            Step::Name(name) => {
                let listed =
                    std::fs::read_dir(&at).is_ok_and(|mut e| e.any(|e| e.is_ok_and(|e| e.file_name() == name)));
                if !listed {
                    return None;
                }
                at.push(name);
            }
        }
    }
    Some(path)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("satex-loader-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("MyPkg.sty"), "").unwrap();
        std::fs::write(dir.join("sub").join("Deep.tex"), "").unwrap();
        dir
    }

    /// A document directory beside a template directory: the shape
    /// `\documentclass{../template/x}` names.
    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("satex-loader-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("doc").join("deeper")).unwrap();
        std::fs::create_dir_all(dir.join("template")).unwrap();
        std::fs::write(dir.join("template").join("MyCls.cls"), "").unwrap();
        std::fs::write(dir.join("doc").join("Local.sty"), "").unwrap();
        dir
    }

    #[test]
    fn a_name_resolves_only_when_it_is_spelled_the_way_the_disk_spells_it() {
        let dir = scratch("exact");
        for fs in [HOST, FOLDING] {
            assert_eq!(exact_file(&dir, "MyPkg.sty", fs), Some(dir.join("MyPkg.sty")), "{fs:?}");
            assert_eq!(exact_file(&dir, "sub/Deep.tex", fs), Some(dir.join("sub/Deep.tex")), "{fs:?}");
            // What `Trees::get` would miss, the local directory must miss too.
            assert_eq!(exact_file(&dir, "mypkg.sty", fs), None, "{fs:?}");
            assert_eq!(exact_file(&dir, "MYPKG.STY", fs), None, "{fs:?}");
            assert_eq!(exact_file(&dir, "sub/deep.tex", fs), None, "{fs:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_folding_filesystem_this_is_tested_against_really_does_fold() {
        // Without this the case-sensitive assertions above would pass on
        // Linux for the wrong reason: the open never succeeding at all.
        let dir = scratch("folding");
        assert!(FOLDING.is_file(&dir.join("mypkg.sty")));
        assert!(!HOST.is_file(&dir.join("mypkg.sty")) || cfg!(any(windows, target_os = "macos")));
        assert!(!FOLDING.is_file(&dir.join("other.sty")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_resolves_on_no_filesystem() {
        let dir = scratch("missing");
        for fs in [HOST, FOLDING] {
            assert_eq!(exact_file(&dir, "nothing.sty", fs), None, "{fs:?}");
            assert_eq!(exact_file(&dir, "sub", fs), None, "{fs:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_leading_out_of_the_document_directory_resolves() {
        let dir = tree("parent");
        let doc = dir.join("doc");
        let deeper = doc.join("deeper");
        for fs in [HOST, FOLDING] {
            assert_eq!(
                exact_file(&doc, "../template/MyCls.cls", fs),
                Some(doc.join("../template/MyCls.cls")),
                "{fs:?}"
            );
            assert_eq!(exact_file(&doc, "./Local.sty", fs), Some(doc.join("./Local.sty")), "{fs:?}");
            assert_eq!(exact_file(&doc, "deeper/../Local.sty", fs), Some(doc.join("deeper/../Local.sty")), "{fs:?}");
            assert_eq!(
                exact_file(&deeper, "../../template/MyCls.cls", fs),
                Some(deeper.join("../../template/MyCls.cls")),
                "{fs:?}"
            );
            // `..` names a directory, not a file, and leads nowhere alone.
            assert_eq!(exact_file(&doc, "../template", fs), None, "{fs:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_leading_out_of_the_document_directory_is_still_spelled_exactly() {
        let dir = tree("parent-case");
        let doc = dir.join("doc");
        // Without this the assertions below would pass on Linux for the
        // wrong reason: the open never succeeding at all.
        assert!(FOLDING.is_file(&doc.join("../template/mycls.cls")));
        for fs in [HOST, FOLDING] {
            assert_eq!(exact_file(&doc, "../template/mycls.cls", fs), None, "{fs:?}");
            assert_eq!(exact_file(&doc, "../Template/MyCls.cls", fs), None, "{fs:?}");
            assert_eq!(exact_file(&doc, "./local.sty", fs), None, "{fs:?}");
            assert_eq!(exact_file(&doc, "deeper/../local.sty", fs), None, "{fs:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_that_spells_a_path_is_not_a_distribution_file_name() {
        assert!(spells_a_path("../template/MyCls.cls"));
        assert!(spells_a_path("sub/deep.tex"));
        assert!(!spells_a_path("article.cls"));
        assert!(!spells_a_path("MyPkg.sty"));
    }

    #[test]
    fn a_case_mismatch_is_found_through_a_parent_directory_too() {
        if cfg!(any(target_os = "windows", target_os = "macos")) {
            return;
        }
        let dir = tree("parent-mismatch");
        let doc = dir.join("doc");
        assert_eq!(case_folded_name(&doc, "../template/MyCls.cls"), None);
        assert_eq!(case_folded_name(&doc, "../template/mycls.cls"), Some("MyCls.cls".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn case_mismatch_is_detected_when_file_exists_with_different_case() {
        let dir = std::env::temp_dir().join("satex-loader-case-mismatch");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Create files with specific cases: MyPkg.sty and Deep.tex
        std::fs::write(dir.join("MyPkg.sty"), "").unwrap();
        std::fs::write(dir.join("Deep.tex"), "").unwrap();

        // Check that exact matches work.
        assert_eq!(case_folded_name(&dir, "MyPkg.sty"), None, "exact match should not be detected as mismatch");
        assert_eq!(case_folded_name(&dir, "Deep.tex"), None, "exact match should not be detected as mismatch");

        // Check that case-insensitive matches are detected only on case-insensitive systems.
        // On Linux (case-sensitive), a mismatch should be found.
        // On Windows/macOS (case-insensitive), the exact case might match differently.
        let mypkg_lower = case_folded_name(&dir, "mypkg.sty");
        let deep_lower = case_folded_name(&dir, "deep.tex");

        if !cfg!(any(target_os = "windows", target_os = "macos")) {
            // On case-sensitive systems, mismatches should be detected.
            assert_eq!(mypkg_lower, Some("MyPkg.sty".to_string()), "case mismatch should be detected");
            assert_eq!(deep_lower, Some("Deep.tex".to_string()), "case mismatch should be detected");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
