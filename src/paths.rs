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

/// How a host writes paths: what kpathsea splits a path list on, whether
/// `\` separates directories, and whether a leading `C:` roots a path.
/// Named rather than `cfg!`-tested inline, so one test run covers both.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Syntax {
    list: char,
    backslash: bool,
    drives: bool,
}

pub(crate) const UNIX: Syntax = Syntax { list: ':', backslash: false, drives: false };
pub(crate) const WINDOWS: Syntax = Syntax { list: ';', backslash: true, drives: true };
pub(crate) const NATIVE: Syntax = if cfg!(windows) { WINDOWS } else { UNIX };

impl Syntax {
    fn is_separator(self, c: char) -> bool {
        c == '/' || (self.backslash && c == '\\')
    }

    /// Whether `text` starts with a drive specification, which roots what
    /// follows it and so must survive trimming and rooting.
    fn has_drive(self, text: &str) -> bool {
        let bytes = text.as_bytes();
        self.drives && bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
    }
}

/// The separator kpathsea path variables use (kpathsea manual, "Path
/// searching"): `:` everywhere but Windows, where it would collide with a
/// drive letter.
pub const SEPARATOR: char = NATIVE.list;

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
    parse_in(value, NATIVE)
}

fn parse_in(value: &str, syntax: Syntax) -> Vec<Component> {
    value
        .split(syntax.list)
        .map(|raw| {
            let raw = raw.trim();
            if raw.is_empty() {
                return Component::Default;
            }
            let raw = raw.strip_prefix("!!").unwrap_or(raw);
            // The recursive marker is a doubled separator, and Windows writes it with either slash.
            let mut tail = raw.chars().rev();
            let recursive = tail.next().is_some_and(|c| syntax.is_separator(c))
                && tail.next().is_some_and(|c| syntax.is_separator(c));
            // Re-collecting components drops the trailing separators while keeping the
            // drive prefix and root that trimming the string would leave behind.
            let path: PathBuf = Path::new(raw).components().collect();
            Component::Dir { path, recursive }
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

/// What a `file://` URL's path escapes: everything but the unreserved set
/// (RFC 3986 § 2.3), the `/` separator, and `:`, whose encoded form in a
/// drive letter is what breaks the round trip back to a path.
const URL_PATH: &percent_encoding::AsciiSet =
    &percent_encoding::NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.').remove(b'~').remove(b'/').remove(b':');

/// `path` as a `file://` URL: slash-separated and percent-encoded, without
/// the extended-length prefix `fs::canonicalize` returns on Windows.  The
/// path should be absolute; a relative one is rooted as it stands.
pub fn file_url(path: &Path) -> String {
    url_in(&path.to_string_lossy(), NATIVE)
}

/// The local path a `file://` URL names, or `None` for another scheme.
/// Round-trips with [`file_url`].
pub fn from_file_url(url: &str) -> Option<PathBuf> {
    path_in(url, NATIVE).map(PathBuf::from)
}

/// Whether `file` lies in `dir` or below it, however either is written:
/// both are made absolute against the working directory first, without
/// resolving symlinks, so `-f main.tex` and a client's link name agree.
pub fn within(dir: &Path, file: &Path) -> bool {
    let absolute = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    absolute(file).starts_with(absolute(dir))
}

/// The directory `path` lives in, for a document- or project-relative
/// lookup: `Path::parent()` returns `Some("")`, not `None`, for a bare file
/// name, so code that does `.unwrap_or_default()` on it silently gets the
/// empty path instead of falling back to anything. This maps that case (and
/// `path` having no parent at all, as `/` or `.` do not) to `.`, so the
/// result always joins or lists correctly against the working directory.
pub fn dir_of(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// A drive letter upper-cased: VS Code sends `file:///c%3A/…` for a file the
/// run recorded as `C:\…`, and the two have to name the same path.
fn upper_drive(rooted: &str, syntax: Syntax) -> String {
    let body = rooted.strip_prefix('/').unwrap_or(rooted);
    if !syntax.has_drive(body) || !matches!(body.as_bytes().get(2), None | Some(b'/')) {
        return rooted.to_string();
    }
    format!("/{}{}", body[..1].to_ascii_uppercase(), &body[1..])
}

fn url_in(text: &str, syntax: Syntax) -> String {
    let mut path: String = text.chars().map(|c| if syntax.is_separator(c) { '/' } else { c }).collect();
    let mut authority = String::new();
    if syntax.drives {
        // `\\?\C:\x` and `\\?\UNC\host\share` are what `canonicalize` returns, not URL syntax.
        if let Some(rest) = path.strip_prefix("//?/UNC/") {
            path = format!("//{rest}");
        } else if let Some(rest) = path.strip_prefix("//?/") {
            path = rest.to_string();
        }
        if let Some(rest) = path.strip_prefix("//") {
            // A UNC share's host is the URL's authority, not part of its path (RFC 8089 § 3).
            let (host, tail) = rest.split_once('/').unwrap_or((rest, ""));
            authority = host.to_string();
            path = format!("/{tail}");
        }
    }
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    let path = upper_drive(&path, syntax);
    format!("file://{authority}{}", percent_encoding::utf8_percent_encode(&path, URL_PATH))
}

fn path_in(url: &str, syntax: Syntax) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let (authority, path) = match rest.find('/') {
        Some(cut) => (&rest[..cut], &rest[cut..]),
        None => (rest, "/"),
    };
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8_lossy().into_owned();
    if !syntax.drives {
        return Some(decoded);
    }
    let decoded = upper_drive(&decoded, syntax);
    if !authority.is_empty() && authority != "localhost" {
        return Some(format!(r"\\{authority}{}", decoded.replace('/', "\\")));
    }
    // `file:///C:/x` names a drive, so the URL's leading `/` is not part of the path (RFC 8089 § 2).
    let body = if syntax.has_drive(decoded.strip_prefix('/').unwrap_or(&decoded)) && decoded.starts_with('/') {
        &decoded[1..]
    } else {
        &decoded[..]
    };
    Some(body.replace('/', "\\"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs_of(value: &str, syntax: Syntax, cwd: &Path) -> Vec<PathBuf> {
        parse_in(value, syntax).iter().flat_map(|c| expand(c, cwd)).collect()
    }

    fn recursive(value: &str, syntax: Syntax) -> Vec<bool> {
        parse_in(value, syntax).iter().map(|c| matches!(c, Component::Dir { recursive: true, .. })).collect()
    }

    #[test]
    fn empty_component_is_the_default_path_and_contributes_nothing() {
        let cwd = Path::new("/project");
        for syntax in [UNIX, WINDOWS] {
            let value = format!("./tex{}", syntax.list);
            assert_eq!(dirs_of(&value, syntax, cwd), vec![PathBuf::from("/project/tex")], "{syntax:?}");
        }
    }

    #[test]
    fn bang_bang_is_stripped_and_relative_paths_join_cwd() {
        let cwd = Path::new("/project");
        for syntax in [UNIX, WINDOWS] {
            assert_eq!(dirs_of("!!./tex", syntax, cwd), vec![PathBuf::from("/project/tex")], "{syntax:?}");
        }
    }

    #[test]
    fn a_doubled_backslash_recurses_where_windows_writes_one() {
        assert_eq!(recursive(r"C:\texmf\tex\\", WINDOWS), vec![true]);
        assert_eq!(recursive(r"C:\texmf\tex", WINDOWS), vec![false]);
        // On a Unix path a backslash is an ordinary character, so it marks nothing.
        assert_eq!(recursive(r"/texmf/tex\\", UNIX), vec![false]);
        assert_eq!(recursive("/texmf/tex//", UNIX), vec![true]);
    }

    #[test]
    fn a_list_splits_only_on_its_own_separator() {
        assert_eq!(parse_in("a;b", UNIX).len(), 1);
        assert_eq!(parse_in("a:b", UNIX).len(), 2);
        assert_eq!(parse_in("C:/a;C:/b", WINDOWS).len(), 2);
    }

    #[test]
    fn a_trailing_separator_keeps_the_root() {
        // Trimming the string would leave `C:`, which is relative to the drive's own cwd.
        assert_eq!(parse_in("/", UNIX), vec![Component::Dir { path: PathBuf::from("/"), recursive: false }]);
        let root = expand(&parse_in("/", UNIX)[0], Path::new("/project"));
        assert_eq!(root, vec![PathBuf::from("/")]);
    }

    #[test]
    fn a_windows_url_keeps_its_drive_letter_and_round_trips() {
        assert_eq!(url_in(r"C:\foo bar.tex", WINDOWS), "file:///C:/foo%20bar.tex");
        assert_eq!(url_in("c:/foo.tex", WINDOWS), "file:///C:/foo.tex");
        assert_eq!(path_in("file:///C:/foo.tex", WINDOWS).unwrap(), r"C:\foo.tex");
        // VS Code encodes the colon and lower-cases the drive.
        assert_eq!(path_in("file:///c%3A/foo.tex", WINDOWS).unwrap(), r"C:\foo.tex");
        for path in [r"C:\foo.tex", r"C:\Program Files\tex\a.sty", r"\\host\share\a.tex"] {
            assert_eq!(path_in(&url_in(path, WINDOWS), WINDOWS).unwrap(), path, "{path}");
        }
    }

    #[test]
    fn the_extended_length_prefix_is_not_part_of_the_url() {
        assert_eq!(url_in(r"\\?\C:\foo.tex", WINDOWS), "file:///C:/foo.tex");
        assert_eq!(url_in(r"\\?\UNC\host\share\a.tex", WINDOWS), "file://host/share/a.tex");
    }

    #[test]
    fn a_unix_url_round_trips_and_leaves_backslashes_alone() {
        assert_eq!(url_in("/home/a b.tex", UNIX), "file:///home/a%20b.tex");
        // A backslash is an ordinary character in a Unix file name.
        assert_eq!(url_in(r"/home/a\b.tex", UNIX), "file:///home/a%5Cb.tex");
        for path in ["/home/a.tex", "/home/a b.tex", r"/home/a\b.tex", "/home/ü.tex"] {
            assert_eq!(path_in(&url_in(path, UNIX), UNIX).unwrap(), path, "{path}");
        }
    }

    #[test]
    fn dir_of_falls_back_to_the_working_directory_on_a_bare_name() {
        assert_eq!(dir_of(Path::new("thesis.tex")), PathBuf::from("."));
        assert_eq!(dir_of(Path::new("sub/thesis.tex")), PathBuf::from("sub"));
        assert_eq!(dir_of(Path::new("/proj/thesis.tex")), PathBuf::from("/proj"));
        assert_eq!(dir_of(Path::new("/proj/")), PathBuf::from("/"));
    }

    #[test]
    fn a_url_of_another_scheme_is_not_a_path() {
        assert_eq!(path_in("https://example.com/a.tex", NATIVE), None);
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
