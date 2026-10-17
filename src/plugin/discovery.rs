//! What a project directory holds, walked with git's rules: `.gitignore`,
//! `.ignore`, the global excludes and hidden files are skipped.

use std::path::{Path, PathBuf};

/// The files a project offers a run: documents that could be the root, the
/// classes and packages it writes itself, its build configurations and its
/// bibliographies.
#[derive(Clone, Debug, Default)]
pub struct Discovery {
    pub root: PathBuf,
    pub documents: Vec<PathBuf>,
    pub inputs: Vec<PathBuf>,
    pub classes: Vec<PathBuf>,
    pub packages: Vec<PathBuf>,
    pub build: Vec<PathBuf>,
    pub bibliographies: Vec<PathBuf>,
    /// The literate sources it is written as: `.dtx` files and the `.ins`
    /// batch files that extract them.
    pub literate: Vec<PathBuf>,
    /// The file a `% !TeX root` line in the project points at.
    pub declared_root: Option<PathBuf>,
    /// What the build configuration typesets: l3build's `typesetfiles`.
    pub typeset: Vec<PathBuf>,
    /// Whether ignore files were honored, which is what the walk reports.
    pub respects_ignores: bool,
}

/// How deep the walk goes.  A document tree keeps its chapters a level or
/// two down (`chapters/`, `sections/figures/`), and going deeper mostly
/// finds build output.
const MAX_DEPTH: usize = 4;

/// The build configurations satex recognizes beside a document.
const BUILD_FILES: [&str; 7] =
    ["latexmkrc", ".latexmkrc", "Makefile", "justfile", "arara.yaml", "tectonic.toml", super::l3build::FILE];

pub fn scan(root: &Path) -> Discovery {
    scan_with(root, 1)
}

/// How much of a file is read to tell a document from an input: both
/// `\documentclass` and the `% !TeX` lines stand at the top.
const HEAD: usize = 64 * 1024;

/// What one file of a project is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Document,
    Input,
    Class,
    Package,
    Build,
    Bibliography,
    Literate,
}

/// One file the walk classified, with the root its `% !TeX root` line names.
struct Found {
    kind: Kind,
    path: PathBuf,
    declares_root: Option<PathBuf>,
}

fn head_of(path: &Path) -> String {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else { return String::new() };
    let mut buffer = vec![0u8; HEAD];
    let read = file.read(&mut buffer).unwrap_or(0);
    buffer.truncate(read);
    String::from_utf8_lossy(&buffer).into_owned()
}

/// Whether `text` is a document's own: it names its class or opens it.
pub fn is_document(text: &str) -> bool {
    text.contains("\\documentclass") || text.contains("\\begin{document}")
}

/// The root a `% !TeX root` line in `text`, the file at `path`, names.
pub fn declared_root(path: &Path, text: &str) -> Option<PathBuf> {
    super::magic::scan(text)
        .into_iter()
        .find(|magic| magic.key.eq_ignore_ascii_case("TeX root"))
        .map(|magic| path.with_file_name(magic.value.trim()))
        .filter(|root| root.is_file())
}

/// Whether `text` names the file `stem` the way a load does: as a path
/// argument, `{stem}`, `{dir/stem.tex}` or `\input stem `.
fn names(text: &str, stem: &str) -> bool {
    !stem.is_empty()
        && text.match_indices(stem).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let after = text[at + stem.len()..].chars().next();
            (matches!(before, Some('{' | '/')) || text[..at].ends_with("\\input "))
                && matches!(after, None | Some('}' | '.' | ' ' | '\n'))
        })
}

fn classify(path: &Path) -> Option<Found> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let found = |kind| Some(Found { kind, path: path.to_path_buf(), declares_root: None });
    if BUILD_FILES.contains(&name) {
        return found(Kind::Build);
    }
    match path.extension().and_then(|e| e.to_str()) {
        // A `.dtx` holds its documentation and its code, and the `.ins`
        // beside it says which files docstrip makes of them; the
        // `\documentclass` in a `.dtx` belongs to the driver that typesets
        // the documentation, so neither file is a document.
        Some("dtx" | "fdd" | "ins") => found(Kind::Literate),
        Some("tex" | "ltx" | "Rnw") => {
            let head = head_of(path);
            let kind = if is_document(&head) { Kind::Document } else { Kind::Input };
            Some(Found { kind, path: path.to_path_buf(), declares_root: declared_root(path, &head) })
        }
        Some("cls") => found(Kind::Class),
        Some("sty") => found(Kind::Package),
        Some("bib") => found(Kind::Bibliography),
        _ => None,
    }
}

/// The same walk on `threads` threads.  More than one runs the parallel
/// walker, which is what makes a large project tree cheap to look at; one
/// keeps the walk on this thread.
pub fn scan_with(root: &Path, threads: usize) -> Discovery {
    // `Path::parent` of a bare file name is the empty path, which names the
    // current directory everywhere else but walks nothing here.
    let root = match root.as_os_str().is_empty() {
        true => Path::new("."),
        false => root,
    };
    let mut builder = ignore::WalkBuilder::new(root);
    builder.max_depth(Some(MAX_DEPTH)).threads(threads.max(1));
    let mut files = match threads > 1 {
        true => {
            let collected = std::sync::Mutex::new(Vec::new());
            builder.build_parallel().run(|| {
                Box::new(|entry| {
                    if let Ok(entry) = entry
                        && entry.file_type().is_some_and(|kind| kind.is_file())
                        && let Some(found) = classify(entry.path())
                        && let Ok(mut collected) = collected.lock()
                    {
                        collected.push(found);
                    }
                    ignore::WalkState::Continue
                })
            });
            collected.into_inner().unwrap_or_default()
        }
        false => builder
            .build()
            .flatten()
            .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
            .filter_map(|entry| classify(entry.path()))
            .collect(),
    };

    // The parallel walker hands files back in whatever order its threads
    // finish in, and `read_dir` is not sorted either.  `declared_root` takes
    // the first `% !TeX root` it meets, so the walk order would decide which
    // one a project with several of them starts from.
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut found = Discovery { root: root.to_path_buf(), respects_ignores: true, ..Discovery::default() };
    for file in files {
        found.declared_root = found.declared_root.take().or(file.declares_root);
        let list = match file.kind {
            Kind::Document => &mut found.documents,
            Kind::Input => &mut found.inputs,
            Kind::Class => &mut found.classes,
            Kind::Package => &mut found.packages,
            Kind::Build => &mut found.build,
            Kind::Bibliography => &mut found.bibliographies,
            Kind::Literate => &mut found.literate,
        };
        list.push(file.path);
    }
    for list in [
        &mut found.documents,
        &mut found.inputs,
        &mut found.classes,
        &mut found.packages,
        &mut found.build,
        &mut found.bibliographies,
        &mut found.literate,
    ] {
        list.sort();
    }
    if found.build.iter().any(|path| path.ends_with(super::l3build::FILE))
        && let Some(config) = super::l3build::L3build::read(root)
    {
        let base = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        found.typeset = config
            .documents()
            .into_iter()
            .map(|path| path.strip_prefix(&base).map(|p| root.join(p)).unwrap_or(path))
            .collect();
    }
    found
}

impl Discovery {
    /// The document a run starts from when the command line names none: the
    /// file a `% !TeX root` line points at, else the document closest to the
    /// directory, preferring `main.tex` and the directory's own name.
    pub fn root_document(&self) -> Option<PathBuf> {
        if let Some(root) = &self.declared_root {
            return Some(root.clone());
        }
        if let Some(first) = self.typeset.first() {
            return Some(first.clone());
        }
        let rank = |path: &&PathBuf| self.rank(path);
        self.documents
            .iter()
            .min_by_key(|path| rank(path))
            .or_else(|| self.inputs.iter().min_by_key(|path| rank(path)))
            // A `.dtx` bundle has no document to start from: what it is
            // about is the code in the `.dtx`, which is also what the `.ins`
            // beside it extracts.
            .or_else(|| {
                self.literate
                    .iter()
                    .filter(|path| path.extension().is_some_and(|e| e != "ins"))
                    .min_by_key(|path| rank(path))
            })
            .or_else(|| self.literate.iter().min_by_key(|path| rank(path)))
            .cloned()
    }

    /// How likely `path` is the root, least first: a build file or an
    /// `% arara:` line naming it, then the document closest to the
    /// directory, preferring `main.tex` and the directory's own name.
    fn rank(&self, path: &Path) -> (bool, usize, bool, bool, PathBuf) {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let folder = self.root.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        let hinted = self.build.iter().any(|b| head_of(b).contains(name)) || head_of(path).contains("% arara:");
        (!hinted, path.components().count(), stem != "main", stem != folder, path.to_path_buf())
    }

    /// The documents that may read `file`, likeliest first: those naming
    /// it, directly or through inputs that do, then the one a run starts
    /// from. Only a run tells which one reads it.
    pub fn candidate_roots(&self, file: &Path) -> Vec<PathBuf> {
        let texts: Vec<(&PathBuf, String)> = self
            .documents
            .iter()
            .chain(&self.inputs)
            .filter(|p| !crate::paths::same_file(p, file))
            .map(|p| (p, head_of(p)))
            .collect();
        let mut named = vec![file.to_path_buf()];
        let mut next = 0;
        while next < named.len() {
            let stem = named[next].file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            next += 1;
            for (path, text) in &texts {
                if names(text, &stem) && !named.contains(path) {
                    named.push(path.to_path_buf());
                }
            }
        }
        let mut roots: Vec<PathBuf> = named.into_iter().filter(|p| self.documents.contains(p)).collect();
        roots.sort_by_cached_key(|p| self.rank(p));
        roots.extend(self.root_document().filter(|r| !roots.contains(r) && !crate::paths::same_file(r, file)));
        roots
    }

    /// The documents that could be the root, when more than one could and
    /// nothing in the project says which: the caller picks the first but
    /// should say that it had to choose.
    pub fn rival_roots(&self) -> &[PathBuf] {
        match self.declared_root.is_none() && self.documents.len() > 1 {
            true => &self.documents,
            false => &[],
        }
    }

    /// The ignore files the walk honors, in the order the `ignore` crate
    /// applies them: what git itself would skip.
    pub fn honors() -> &'static [&'static str] {
        &[".gitignore", ".ignore", ".git/info/exclude", "core.excludesFile", "hidden files"]
    }

    /// One row per kind of file found, for `summary` and `query project`.
    pub fn rows(&self) -> Vec<(&'static str, Vec<&PathBuf>)> {
        vec![
            ("documents", self.documents.iter().collect()),
            ("inputs", self.inputs.iter().collect()),
            ("classes", self.classes.iter().collect()),
            ("packages", self.packages.iter().collect()),
            ("build", self.build.iter().collect()),
            ("bibliographies", self.bibliographies.iter().collect()),
            ("literate", self.literate.iter().collect()),
            ("typeset", self.typeset.iter().collect()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_documents_of_a_project() {
        let found = scan(Path::new("tests/fixtures/project"));
        assert!(found.documents.iter().any(|p| p.ends_with("paper.tex")));
        assert!(found.packages.iter().any(|p| p.ends_with("mypackage.sty")));
        assert!(found.build.iter().any(|p| p.ends_with("latexmkrc")));
        // Two documents sit in the fixture, neither named after the directory
        // and neither pointing at the other, so the choice is the first by
        // name and the caller is told it was a choice.
        assert_eq!(found.root_document(), Some(PathBuf::from("tests/fixtures/project/options.tex")));
        assert_eq!(found.rival_roots().len(), 2);
    }

    #[test]
    fn an_empty_root_is_the_current_directory() {
        assert_eq!(scan(Path::new("")).documents, scan(Path::new(".")).documents);
    }

    #[test]
    fn skips_what_git_ignores() {
        let found = scan(Path::new("."));
        assert!(
            !found.inputs.iter().any(|p| p.starts_with("./target")),
            "target/ is ignored in git, so it is ignored here"
        );
    }
}
