//! Build-system configuration files around the document.

use std::path::{Path, PathBuf};

use crate::config::Engine;

pub const BUILD_FILES: [(&str, &str); 7] = [
    ("latexmkrc", "latexmk"),
    (".latexmkrc", "latexmk"),
    ("GNUmakefile", "make"),
    ("makefile", "make"),
    ("Makefile", "make"),
    ("Tectonic.toml", "tectonic"),
    ("tectonic.toml", "tectonic"),
];

/// One build-configuration file found beside the document (a `latexmkrc`, a
/// `Makefile`), with the settings [`Project::discover`] could read from it.
#[derive(Clone, Debug)]
pub struct BuildFile {
    pub tool: &'static str,
    pub path: PathBuf,
    pub settings: Vec<(String, String)>,
    /// kpathsea path variables this file sets (`latexmkrc`'s `ensure_path`
    /// and `$ENV{…}`, a Makefile's `export` and inline `VAR=value engine`),
    /// each with the raw value read; see [`crate::paths`].
    pub paths: Vec<(String, String)>,
}

/// The build files found around the document ([`BuildFile`]s), which is where
/// satex learns the engine and output format a `latexmk`-driven build would
/// actually use.
#[derive(Clone, Debug, Default)]
pub struct Project {
    pub root: PathBuf,
    pub files: Vec<BuildFile>,
    /// The `build.lua` of an l3build bundle, evaluated.
    pub l3build: Option<crate::plugin::l3build::L3build>,
}

impl Project {
    pub fn discover(root: &Path) -> Project {
        let mut files = Vec::new();
        for (name, tool) in BUILD_FILES {
            let path = root.join(name);
            if !path.is_file() {
                continue;
            }
            let settings = match tool {
                "latexmk" => latexmk_settings(&path),
                _ => Vec::new(),
            };
            let paths = match tool {
                "latexmk" => latexmkrc_paths(&path),
                "make" => makefile_paths(&path),
                _ => Vec::new(),
            };
            files.push(BuildFile { tool, path, settings, paths });
        }
        // The user's own `~/.latexmkrc` (latexmk manual, "Configuration/initialization
        // (rc) files"), read after the project's: a project-level `ensure_path`
        // outranks it, since [`Project::kpse_paths`] takes the first file per tool.
        if let Some(home) = dirs::home_dir() {
            let global = home.join(".latexmkrc");
            if global.is_file() && !files.iter().any(|f| f.path == global) {
                let paths = latexmkrc_paths(&global);
                if !paths.is_empty() {
                    files.push(BuildFile { tool: "latexmk", path: global, settings: Vec::new(), paths });
                }
            }
        }
        let l3build = crate::plugin::l3build::L3build::read(root);
        if let Some(config) = &l3build {
            files.push(BuildFile {
                tool: "l3build",
                path: config.path.clone(),
                settings: config.settings(),
                paths: Vec::new(),
            });
        }
        Project { root: root.to_path_buf(), files, l3build }
    }

    /// `variable`'s value from the project's build files: `latexmkrc`
    /// (`ensure_path`, `$ENV{…}`) outranks the Makefile (`export`, inline
    /// `VAR=value engine`) — the order latexmk itself would set the
    /// environment up in before running the engine command.  `None` when
    /// neither sets it, so the caller falls back to `satex.yaml`.
    pub fn kpse_paths(&self, variable: &str) -> Option<(Vec<String>, crate::paths::Source)> {
        for (tool, source) in [("latexmk", crate::paths::Source::Latexmkrc), ("make", crate::paths::Source::Make)] {
            let values: Vec<String> = self
                .files
                .iter()
                .filter(|file| file.tool == tool)
                .filter_map(|file| file.paths.iter().find(|(name, _)| name == variable).map(|(_, v)| v.clone()))
                .collect();
            if !values.is_empty() {
                return Some((values, source));
            }
        }
        None
    }

    /// Where a build finds the files the project itself provides, beyond the
    /// document's own directory.
    pub fn search_paths(&self) -> Vec<PathBuf> {
        self.l3build.as_ref().map(|config| config.search_paths()).unwrap_or_default()
    }

    /// The files a build generates from literate sources before TeX runs,
    /// each with the `.dtx` and docstrip guards it is extracted with.
    pub fn unpacked(&self) -> std::collections::BTreeMap<String, (PathBuf, Vec<String>)> {
        self.l3build.as_ref().map(|config| config.unpacked()).unwrap_or_default()
    }

    pub fn setting(&self, key: &str) -> Option<&str> {
        self.files
            .iter()
            .flat_map(|file| file.settings.iter())
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub fn engine(&self) -> Option<Engine> {
        let Some(mode) = self.setting("pdf_mode") else {
            return self.l3build.as_ref().and_then(|config| config.engine_for(None));
        };
        match mode {
            "4" => Some(Engine::LuaTeX),
            "5" => Some(Engine::XeTeX),
            _ => Some(Engine::PdfTeX),
        }
    }

    pub fn describe(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|file| {
                let name = file.path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                match file.settings.as_slice() {
                    [] => format!("{name} ({})", file.tool),
                    settings => format!(
                        "{name} ({}: {})",
                        file.tool,
                        settings.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")
                    ),
                }
            })
            .collect()
    }
}

/// The latexmk settings that change what the run is: the engine selector and
/// the programs it names (latexmk manual, "List of configuration variables").
const LATEXMK_KEYS: [&str; 5] = ["pdf_mode", "pdflatex", "lualatex", "xelatex", "latex"];

/// latexmk rc is Perl; only plain `$key = value;` can be read without running it.
fn latexmk_settings(path: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix('$') else { continue };
        let Some((key, value)) = rest.split_once('=') else { continue };
        let key = key.trim();
        if !LATEXMK_KEYS.contains(&key) {
            continue;
        }
        let value = value.trim().trim_end_matches(';').trim();
        let value = value.trim_matches(['"', '\'']).trim();
        out.push((key.to_string(), value.to_string()));
    }
    out
}

/// A `'…'`/`"…"` literal's contents, unquoted; `None` when `text` does not
/// start with a quote.  Perl string concatenation (`'a' . $ENV{…}`) is not
/// evaluated — only the leading literal is read, the same restriction as
/// [`latexmk_settings`].
fn quoted_literal(text: &str) -> Option<String> {
    let text = text.trim();
    let quote = text.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let rest = &text[1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn add_kpse_path(out: &mut Vec<(String, String)>, variable: &str, value: String) {
    if !crate::paths::VARIABLES.contains(&variable) || value.is_empty() {
        return;
    }
    match out.iter_mut().find(|(name, _)| name == variable) {
        Some((_, existing)) => {
            existing.push(crate::paths::SEPARATOR);
            existing.push_str(&value);
        }
        None => out.push((variable.to_string(), value)),
    }
}

/// The kpathsea path variables a `latexmkrc` sets: `ensure_path('VAR', dir,
/// …)` adds directories to it (latexmk manual, "ensure_path"), and
/// `$ENV{'VAR'} = '…'` sets it outright.  Only a literal call or assignment
/// on one line is read, the same restriction as [`latexmk_settings`].
fn latexmkrc_paths(path: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let mut out: Vec<(String, String)> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("$ENV{") {
            let Some((name, tail)) = rest.split_once('}') else { continue };
            let variable = quoted_literal(name).unwrap_or_else(|| name.trim().to_string());
            if let Some(value) = tail.trim_start().strip_prefix('=').and_then(quoted_literal) {
                add_kpse_path(&mut out, &variable, value);
            }
        } else if let Some(rest) = line.strip_prefix("ensure_path") {
            let Some(inner) = rest.trim_start().strip_prefix('(').and_then(|s| s.rsplit_once(')')).map(|(s, _)| s)
            else {
                continue;
            };
            let mut arguments = inner.split(',').map(str::trim);
            let Some(variable) = arguments.next().and_then(quoted_literal) else { continue };
            let dirs: Vec<String> = arguments.filter_map(quoted_literal).collect();
            if !dirs.is_empty() {
                add_kpse_path(&mut out, &variable, dirs.join(&crate::paths::SEPARATOR.to_string()));
            }
        }
    }
    out
}

/// The kpathsea path variables a Makefile sets: `export VAR=value`, or the
/// inline `VAR=value pdflatex …` form a recipe line passes to its shell (GNU
/// make manual, "Target-specific Variable Values").  A `$VAR`/`$(VAR)`
/// reference to the variable's own previous value is what the real
/// environment already carries, so only the literal part before it is read.
fn makefile_paths(path: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let assignment = line.strip_prefix("export ").unwrap_or(line);
        let Some((variable, rest)) = assignment.split_once('=') else { continue };
        let variable = variable.trim();
        if !crate::paths::VARIABLES.contains(&variable) {
            continue;
        }
        let value = rest.split_whitespace().next().unwrap_or_default();
        let value = value.split('$').next().unwrap_or(value).trim_end_matches(':').to_string();
        if !value.is_empty() {
            out.push((variable.to_string(), value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latexmkrc_selects_the_engine() {
        let dir = std::env::temp_dir().join("satex-project-test");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("latexmkrc"), "$pdf_mode = 4;\n$lualatex = 'lualatex %O %S';\n").unwrap();
        let project = Project::discover(&dir);
        assert_eq!(project.files.len(), 1);
        assert_eq!(project.files[0].tool, "latexmk");
        assert_eq!(project.engine(), Some(Engine::LuaTeX));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
