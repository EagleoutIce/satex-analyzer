//! kpathsea search-path variables — `TEXINPUTS`, `BIBINPUTS`, `BSTINPUTS`,
//! `TFMFONTS`, `ENCFONTS` — resolved the way a real build would see them:
//! the process environment first, then what the project's `latexmkrc` or
//! Makefile sets, then `satex.yaml`'s `paths.*` (kpathsea manual, "Path
//! sources"; latexmk manual, "ensure_path").  [`effective`] follows that
//! order and expands kpathsea path syntax (`:`-separated, an empty
//! component for the default path, a trailing `//` for every subdirectory,
//! a leading `!!` for ls-R-only) against the build's working directory.
//!
//! [`crate::loader::Resolver`] searches the expanded directories directly;
//! [`env_vars`] hands the raw, unexpanded value to a `kpsewhich` child
//! process so kpathsea resolves it the same way a real build would.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::project::Project;

/// The kpathsea variables satex tracks.
pub const VARIABLES: [&str; 5] = ["TEXINPUTS", "BIBINPUTS", "BSTINPUTS", "TFMFONTS", "ENCFONTS"];

/// The separator kpathsea path variables use (kpathsea manual, "Path
/// searching"): `:` everywhere but Windows, where it would collide with a
/// drive letter.
#[cfg(windows)]
pub const SEPARATOR: char = ';';
#[cfg(not(windows))]
pub const SEPARATOR: char = ':';

/// Deep enough for a project's own tree; kpathsea itself has no limit, but a
/// bound keeps a `//` on a huge or cyclic tree from stalling a run.
const MAX_RECURSE_DEPTH: usize = 8;

/// Where an [`Effective`] value was read from, in the order satex tries them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Env,
    Latexmkrc,
    Make,
    Config,
    Unset,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Env => "environment",
            Source::Latexmkrc => "latexmkrc",
            Source::Make => "makefile",
            Source::Config => "satex.yaml",
            Source::Unset => "unset",
        }
    }
}

/// One kpathsea path variable, resolved: the raw value — handed to
/// `kpsewhich`'s environment as-is, so it expands `//` and `!!` itself —
/// and the directories satex's own resolver searches, already expanded
/// against the build's working directory.
#[derive(Clone, Debug)]
pub struct Effective {
    pub variable: &'static str,
    pub raw: String,
    pub source: Source,
    pub dirs: Vec<PathBuf>,
}

/// One component of a kpathsea path value.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Component {
    /// An empty component: the compile-time default path goes here.  satex
    /// has no separate default beyond the distribution trees the resolver
    /// already searches, so this contributes no extra directory of its own.
    Default,
    Dir {
        path: PathBuf,
        recursive: bool,
    },
}

/// Splits a kpathsea path value on [`SEPARATOR`], reading a leading `!!`
/// (ls-R only) and a trailing `//` (recursive) off each component.  `!!` is
/// accepted but otherwise has no separate effect: satex's own lookup always
/// stats the candidate file directly rather than trusting a directory
/// listing, so there is no faster path to fall back to.
fn parse(value: &str) -> Vec<Component> {
    value
        .split(SEPARATOR)
        .map(|raw| {
            let raw = raw.trim();
            if raw.is_empty() {
                return Component::Default;
            }
            let raw = raw.strip_prefix("!!").unwrap_or(raw);
            let recursive = raw.ends_with("//");
            let path = raw.trim_end_matches('/');
            Component::Dir { path: PathBuf::from(path), recursive }
        })
        .collect()
}

/// A component's directories, relative ones resolved against `cwd` — the
/// build's working directory, not necessarily the document's.
fn expand(component: &Component, cwd: &Path) -> Vec<PathBuf> {
    match component {
        Component::Default => Vec::new(),
        Component::Dir { path, recursive } => {
            let root = if path.as_os_str().is_empty() {
                cwd.to_path_buf()
            } else if path.is_absolute() {
                path.clone()
            } else {
                cwd.join(path)
            };
            if !*recursive {
                return vec![root];
            }
            let mut dirs = vec![root.clone()];
            for entry in walkdir::WalkDir::new(&root)
                .max_depth(MAX_RECURSE_DEPTH)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_dir() && e.path() != root)
            {
                dirs.push(entry.path().to_path_buf());
            }
            dirs
        }
    }
}

fn build(raw: String, source: Source, variable: &'static str, cwd: &Path) -> Effective {
    let mut seen = HashSet::new();
    let dirs: Vec<PathBuf> =
        parse(&raw).iter().flat_map(|c| expand(c, cwd)).filter(|d| seen.insert(d.clone())).collect();
    Effective { variable, raw, source, dirs }
}

/// `variable`'s effective value: the process environment, then the
/// project's `latexmkrc`, then its Makefile, then `config_value`
/// (`paths.<variable>` in `satex.yaml`, itself set with `--set`).  Relative
/// directories are resolved against `cwd`, the build's working directory.
pub fn effective(variable: &'static str, project: &Project, config_value: &[String], cwd: &Path) -> Effective {
    if let Ok(value) = std::env::var(variable)
        && !value.is_empty()
    {
        return build(value, Source::Env, variable, cwd);
    }
    if let Some((parts, source)) = project.kpse_paths(variable) {
        return build(parts.join(&SEPARATOR.to_string()), source, variable, cwd);
    }
    if !config_value.is_empty() {
        return build(config_value.join(&SEPARATOR.to_string()), Source::Config, variable, cwd);
    }
    Effective { variable, raw: String::new(), source: Source::Unset, dirs: Vec::new() }
}

/// Every tracked variable, resolved for `project`/`config`/`cwd`, in
/// [`VARIABLES`] order.
pub fn effective_all(project: &Project, config: &crate::config::PathsConfig, cwd: &Path) -> Vec<Effective> {
    VARIABLES.iter().map(|&variable| effective(variable, project, config.get(variable), cwd)).collect()
}

/// The variables that resolved to something, as `kpsewhich`'s own
/// environment: a lookup it makes on satex's behalf then honors the same
/// search satex used, instead of whatever happened to be in the shell.
pub fn env_vars(effective: &[Effective]) -> Vec<(String, String)> {
    effective.iter().filter(|e| e.source != Source::Unset).map(|e| (e.variable.to_string(), e.raw.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_component_is_the_default_path_and_contributes_nothing() {
        let cwd = Path::new("/project");
        let dirs: Vec<PathBuf> = parse("./tex:").iter().flat_map(|c| expand(c, cwd)).collect();
        assert_eq!(dirs, vec![PathBuf::from("/project/tex")]);
    }

    #[test]
    fn bang_bang_is_stripped_and_relative_paths_join_cwd() {
        let cwd = Path::new("/project");
        let dirs: Vec<PathBuf> = parse("!!./tex").iter().flat_map(|c| expand(c, cwd)).collect();
        assert_eq!(dirs, vec![PathBuf::from("/project/tex")]);
    }

    #[test]
    fn trailing_slash_slash_recurses_into_subdirectories() {
        let dir = std::env::temp_dir().join("satex-paths-recurse-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tex/sub")).unwrap();
        let dirs: Vec<PathBuf> = parse("tex//").iter().flat_map(|c| expand(c, &dir)).collect();
        assert!(dirs.contains(&dir.join("tex")));
        assert!(dirs.contains(&dir.join("tex/sub")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
