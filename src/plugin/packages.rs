//! The distribution's package manager: whether a file the search could not
//! find is one this distribution could install, which package ships it, and
//! the command that installs it.
//!
//! `tlmgr info` and `tlmgr search --global` read the repository's package
//! database, not the local one (TeX Live manual, `tlmgr`), so both reach the
//! network.  Nothing here runs unless `plugins.packages` is on or the user
//! asked for an install, and every answer is cached against the manager's
//! own stamp the way [`crate::distribution::probed`] caches a locator's.

use std::path::{Path, PathBuf};

use crate::builtins::{LoadKind, OccKind};
use crate::config::Config;
use crate::facts::LoadStatus;
use crate::machine::Analysis;
use crate::plugin::{Platform, Provider};
use crate::tex::Span;

/// Stands for an answer of "no", so that it is cached:
/// [`crate::distribution::probed`] keeps no empty answer and would ask again
/// on every run.
const NOTHING: &str = "-";

named_enum! {
    /// Why no question was put to a package manager.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Unasked {
        /// `plugins.packages` is off, so no lint run reaches the network.
        Disabled => "plugins.packages is off",
        /// No provider was detected, or its manager is not on PATH.
        NoManager => "no package manager on this machine",
        /// Tectonic carries one bundle and takes what a document needs out of
        /// it, so nothing is installable and nothing is missing.
        Bundled => "this distribution bundles every package",
    }
}

/// What a manager answered about one file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Provided {
    /// The distribution ships the file in this package.
    By(String),
    /// The manager answered, and no package of this distribution has it.
    Nothing,
    /// Nothing was asked.
    Unasked(Unasked),
}

impl Provided {
    pub fn package(&self) -> Option<&str> {
        match self {
            Provided::By(name) => Some(name),
            _ => None,
        }
    }
}

/// How one install run ended, as the manager itself reported it.
#[derive(Clone, Debug)]
pub struct Installed {
    pub command: Vec<String>,
    pub ok: bool,
    pub status: Option<i32>,
    /// The manager's own output, stdout and stderr in the order they were
    /// captured, so a failure is reported in its words rather than ours.
    pub message: String,
}

/// The package manager of the distribution a run resolved against.
#[derive(Clone, Debug)]
pub struct Manager {
    pub provider: Provider,
    program: &'static str,
    cache: Option<PathBuf>,
}

impl Manager {
    /// The manager a lint run may ask, honoring `plugins.packages`.
    pub fn enabled(cfg: &Config, provider: Option<Provider>) -> Result<Manager, Unasked> {
        if !cfg.plugins.packages {
            return Err(Unasked::Disabled);
        }
        Manager::requested(cfg, provider)
    }

    /// The manager for a request the user made in so many words, `satex
    /// install`, which is the consent `plugins.packages` stands for.
    pub fn requested(cfg: &Config, provider: Option<Provider>) -> Result<Manager, Unasked> {
        let provider = provider.ok_or(Unasked::NoManager)?;
        if provider == Provider::Tectonic {
            return Err(Unasked::Bundled);
        }
        let program = provider.manager().ok_or(Unasked::NoManager)?;
        if crate::paths::which(program).is_err() {
            return Err(Unasked::NoManager);
        }
        let cache = crate::format::enabled_cache_dir(cfg);
        Ok(Manager { provider, program, cache })
    }

    pub fn program(&self) -> &'static str {
        self.program
    }

    /// Whether `name` is a package this distribution could install.
    pub fn knows(&self, name: &str) -> bool {
        match self.provider {
            // `tlmgr info` exits 0 either way and says so in its output
            // (TeX Live manual, `info`).
            Provider::TexLive => {
                let lines = self.ask(&format!("info-{}", slug(name)), &["info".into(), name.into()]);
                lines.iter().any(|line| line.strip_prefix("package:").is_some_and(|rest| rest.trim() == name))
            }
            Provider::MikTeX => {
                let lines = self.ask("list", &["packages".into(), "list".into()]);
                lines.iter().any(|line| line.split_whitespace().any(|field| field == name))
            }
            Provider::Tectonic => false,
        }
    }

    /// Which package ships `file`, which a LaTeX package's own name often
    /// does not give: `subcaption.sty` is in `caption`.
    pub fn provides(&self, file: &str) -> Provided {
        match self.provider {
            // A leading slash anchors the match at a path component, so
            // `/tikz.sty` does not also match `mytikz.sty`.
            Provider::TexLive => {
                let pattern = format!("/{file}");
                let lines = self.ask(
                    &format!("provides-{}", slug(file)),
                    &["search".into(), "--global".into(), "--file".into(), pattern],
                );
                match search_result(&lines, file) {
                    Some(package) => Provided::By(package),
                    None => Provided::Nothing,
                }
            }
            // MiKTeX's manager has no file search, so the only honest
            // answer is a package named after the file.
            Provider::MikTeX => {
                let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or(file);
                if self.knows(stem) { Provided::By(stem.to_string()) } else { Provided::Nothing }
            }
            Provider::Tectonic => Provided::Unasked(Unasked::Bundled),
        }
    }

    /// The command that installs `packages`, in user mode when the
    /// installation's own tree cannot be written (TeX Live manual,
    /// `--usermode`).
    pub fn install_command(&self, packages: &[String], usermode: bool) -> Vec<String> {
        let mut command = vec![self.program.to_string()];
        match self.provider {
            Provider::TexLive => {
                if usermode {
                    command.push("--usermode".into());
                }
                command.push("install".into());
            }
            Provider::MikTeX => {
                command.push("packages".into());
                command.push("install".into());
            }
            Provider::Tectonic => {}
        }
        command.extend(packages.iter().cloned());
        command
    }

    /// The same command for the user to run with the rights satex will not
    /// take: satex never elevates by itself.
    pub fn elevated(&self, packages: &[String], platform: Platform) -> String {
        let command = shell(&self.install_command(packages, false));
        match platform {
            Platform::Windows => format!("{command} (in a Command Prompt started as administrator)"),
            _ => format!("sudo {command}"),
        }
    }

    /// Run the install and report what the manager said.
    pub fn install(&self, packages: &[String], usermode: bool) -> Installed {
        let command = self.install_command(packages, usermode);
        let output = std::process::Command::new(&command[0]).args(&command[1..]).output();
        match output {
            Ok(output) => {
                let mut message = String::from_utf8_lossy(&output.stdout).into_owned();
                message.push_str(&String::from_utf8_lossy(&output.stderr));
                Installed {
                    ok: output.status.success(),
                    status: output.status.code(),
                    message: message.trim().to_string(),
                    command,
                }
            }
            Err(error) => Installed { ok: false, status: None, message: error.to_string(), command },
        }
    }

    /// Run the manager, or hand back the answer a previous run cached.
    fn ask(&self, topic: &str, args: &[String]) -> Vec<String> {
        let program = self.program;
        let answer = crate::distribution::probed(program, topic, self.cache.as_deref(), || {
            let Ok(output) = std::process::Command::new(program).args(args).output() else {
                return Vec::new();
            };
            let Ok(text) = String::from_utf8(output.stdout) else { return Vec::new() };
            let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
            if lines.is_empty() {
                lines.push(NOTHING.to_string());
            }
            lines
        });
        answer.into_iter().filter(|line| line != NOTHING).collect()
    }
}

/// A package or class the run looked for and did not find.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Missing {
    /// The file the search was for, with its extension: `caption.sty`.
    pub file: String,
    /// The name the document wrote, for the message.
    pub name: String,
    pub span: Span,
}

/// The file a `\usepackage`, `\RequirePackage` or `\documentclass` looks
/// for: the name it names plus the extension of its kind.
pub(crate) fn file_of(name: &str, kind: LoadKind) -> Option<String> {
    let name = name.trim();
    let extension = match kind {
        LoadKind::Class => "cls",
        LoadKind::Package | LoadKind::Inherited => "sty",
        _ => return None,
    };
    if name.is_empty() {
        return None;
    }
    // A package name may carry a dot of its own, so only a TeX extension counts.
    match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some("sty" | "cls" | "tex" | "def" | "ltx") => Some(name.to_string()),
        _ => Some(format!("{name}.{extension}")),
    }
}

/// The kernel's own error when `\IfFileExists` finds nothing
/// (latex.ltx, `\@missingfileerror`), which is all a `\usepackage` for a
/// package that is not there leaves behind: it never reaches the `\input`,
/// so there is no load to read the name off.
fn missing_file_error(text: &str) -> Option<String> {
    const MARKER: &str = "LaTeX Error: File `";
    let rest = &text[text.find(MARKER)? + MARKER.len()..];
    Some(rest[..rest.find("' not found")?].to_string())
}

/// Every package and class file the run looked for and did not find.
pub fn unresolved(analysis: &Analysis) -> Vec<Missing> {
    let mut out: Vec<Missing> = Vec::new();
    let mut add = |file: String, name: String, span: Span| {
        if !out.iter().any(|m| m.file == file) {
            out.push(Missing { file, name, span });
        }
    };
    for load in &analysis.facts.loads {
        if load.status != LoadStatus::NotFound {
            continue;
        }
        if let Some(file) = file_of(&load.name, load.kind) {
            add(file, load.name.clone(), load.span);
        }
    }
    for occurrence in analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Message) {
        let Some(file) = missing_file_error(&occurrence.key) else { continue };
        if !file.ends_with(".sty") && !file.ends_with(".cls") {
            continue;
        }
        let name = file.rsplit_once('.').map_or(file.clone(), |(stem, _)| stem.to_string());
        add(file, name, occurrence.span);
    }
    out
}

/// The package this distribution would install for a file the search could
/// not find, while `plugins.packages` allows the question.  `unresolved-file`
/// skips what this answers for, so the two rules never report one load twice.
pub fn installable(analysis: &Analysis, file: &str) -> Option<String> {
    if analysis.distribution.indexed == 0 {
        return None;
    }
    let manager = Manager::enabled(&analysis.settings, analysis.plugins.provider).ok()?;
    manager.provides(file).package().map(str::to_string)
}

/// Whether this load names a file the distribution could install, which
/// `uninstalled-package` reports instead of `unresolved-file`.
pub fn installable_load(analysis: &Analysis, load: &crate::facts::Load) -> bool {
    load.status == LoadStatus::NotFound
        && file_of(&load.name, load.kind).is_some_and(|file| installable(analysis, &file).is_some())
}

/// Part of a cache file's name, so that one package's answer never lands in
/// another's file.
fn slug(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}

/// `tlmgr search` prints `⟨package⟩:` and then the matching files, one per
/// line and indented (TeX Live manual, `search`).
fn search_result(lines: &[String], file: &str) -> Option<String> {
    let wanted = format!("/{file}");
    let mut package: Option<&str> = None;
    for line in lines {
        match line.strip_suffix(':') {
            Some(name) if !line.starts_with(char::is_whitespace) && !line.starts_with("tlmgr") => {
                package = Some(name.trim());
            }
            _ => {
                if line.trim_end().ends_with(&wanted)
                    && let Some(name) = package
                    && !name.is_empty()
                {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// Whether the installation's own tree can be written, which decides
/// between `tlmgr install` and `tlmgr --usermode install`.  `None` when
/// there is no tree to test.
pub fn tree_writable(roots: &[PathBuf]) -> Option<bool> {
    let tree = main_tree(roots)?;
    // Permission bits alone do not say whether this user may write, so the
    // test is a file that is made and removed again.
    let probe = tree.join(format!(".satex-write-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Some(true)
        }
        Err(_) => Some(false),
    }
}

/// The tree an install writes into: the distribution's own, not the user's.
fn main_tree(roots: &[PathBuf]) -> Option<&PathBuf> {
    roots
        .iter()
        .find(|root| root.file_name().is_some_and(|name| name == "texmf-dist"))
        .or_else(|| roots.iter().find(|root| root.is_dir()))
}

/// A command as a user would type it.
pub fn shell(command: &[String]) -> String {
    command
        .iter()
        .map(|part| if part.contains(' ') { format!("\"{part}\"") } else { part.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_package_out_of_a_search() {
        let lines: Vec<String> = [
            "tlmgr: package repository https://example.org/tlnet (verified)",
            "caption:",
            "\ttexmf-dist/tex/latex/caption/subcaption.sty",
        ]
        .iter()
        .map(|l| l.to_string())
        .collect();
        assert_eq!(search_result(&lines, "subcaption.sty").as_deref(), Some("caption"));
        assert_eq!(search_result(&lines, "caption.sty"), None);
    }

    #[test]
    fn a_leading_component_is_required() {
        let lines: Vec<String> =
            ["mytikz:", "\ttexmf-dist/tex/latex/mytikz/mytikz.sty"].iter().map(|l| l.to_string()).collect();
        assert_eq!(search_result(&lines, "tikz.sty"), None);
    }

    #[test]
    fn names_the_file_each_kind_looks_for() {
        assert_eq!(file_of("subcaption", LoadKind::Package).as_deref(), Some("subcaption.sty"));
        assert_eq!(file_of("scrartcl", LoadKind::Class).as_deref(), Some("scrartcl.cls"));
        assert_eq!(file_of("brandstyler-ru-nl.sty", LoadKind::Package).as_deref(), Some("brandstyler-ru-nl.sty"));
        assert_eq!(file_of("", LoadKind::Package), None);
        assert_eq!(file_of("data", LoadKind::Input), None);
    }

    #[test]
    fn reads_the_name_out_of_the_kernels_error() {
        let text = "LaTeX Error: File `atomicreactions.sty' not found.";
        assert_eq!(missing_file_error(text).as_deref(), Some("atomicreactions.sty"));
        assert_eq!(missing_file_error("Undefined control sequence"), None);
    }

    #[test]
    fn builds_the_install_command() {
        let manager = Manager { provider: Provider::TexLive, program: "tlmgr", cache: None };
        let packages = vec!["caption".to_string()];
        assert_eq!(manager.install_command(&packages, false), ["tlmgr", "install", "caption"]);
        assert_eq!(manager.install_command(&packages, true), ["tlmgr", "--usermode", "install", "caption"]);
        assert_eq!(manager.elevated(&packages, Platform::Linux), "sudo tlmgr install caption");
        let miktex = Manager { provider: Provider::MikTeX, program: "miktex", cache: None };
        assert_eq!(miktex.install_command(&packages, true), ["miktex", "packages", "install", "caption"]);
    }

    #[test]
    fn tectonic_installs_nothing() {
        let mut cfg = Config::default();
        cfg.plugins.packages = true;
        assert_eq!(Manager::requested(&cfg, Some(Provider::Tectonic)).err(), Some(Unasked::Bundled));
        assert_eq!(Manager::requested(&cfg, None).err(), Some(Unasked::NoManager));
    }

    #[test]
    fn nothing_is_asked_while_the_switch_is_off() {
        let cfg = Config::default();
        assert!(!cfg.plugins.packages);
        assert_eq!(Manager::enabled(&cfg, Some(Provider::TexLive)).err(), Some(Unasked::Disabled));
    }

    #[test]
    fn the_installation_tree_is_the_one_an_install_writes_to() {
        let roots = vec![PathBuf::from("/home/me/texmf"), PathBuf::from("/usr/local/texlive/2026/texmf-dist")];
        assert_eq!(main_tree(&roots), Some(&PathBuf::from("/usr/local/texlive/2026/texmf-dist")));
        assert_eq!(main_tree(&[]), None);
    }
}
