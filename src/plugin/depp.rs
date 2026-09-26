//! depp, the Dependency Printer for TeX Live
//! (<https://gitlab.com/islandoftex/texmf/depp>, `depp.dtx`).  depp hooks
//! every file LaTeX reads (`file/before`) and names the TeX Live package it
//! belongs to by where it sits in the TDS tree, then writes the packages to a
//! dependency file, `DEPENDS.txt` by default, in TeX Live's format for them.
//!
//! satex reads the same files, so it names the same packages with depp's
//! rule and holds them against the dependency file the project keeps: a
//! package the run needs that the file does not list is missing, and one the
//! file lists that no file of the run comes from is unused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::facts::LoadStatus;
use crate::machine::Analysis;
use crate::tex::Span;

/// How files map to TeX Live packages, from `satex.yaml`'s `depp:`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeppConfig {
    /// Patterns over a file's directory whose last capture names the
    /// package, tried in order.  depp's own: a file under
    /// `tex/⟨format⟩/⟨package⟩/` belongs to `⟨package⟩`, and a font file to
    /// the last directory under `fonts/` (depp.dtx,
    /// `\_@@_guess_package_from_path:nnn`).
    pub rules: Vec<String>,
    /// Directory names that are not the package's name: `latex/base` is the
    /// `latex` package (depp.dtx, `\_@@_list_deps:`).
    pub renames: BTreeMap<String, String>,
    /// The dependency file, when it is not the one depp's options name.
    pub file: Option<PathBuf>,
}

impl Default for DeppConfig {
    fn default() -> Self {
        DeppConfig {
            rules: vec![".+/tex/[^/]+/([^/]+)".into(), ".+/fonts/.+/([^/]+)".into()],
            renames: [("base".to_string(), "latex".to_string())].into_iter().collect(),
            file: None,
        }
    }
}

impl DeppConfig {
    fn compiled(&self) -> Vec<regex::Regex> {
        self.rules.iter().filter_map(|rule| regex::Regex::new(rule).ok()).collect()
    }

    /// The TeX Live package a file belongs to, when it is in a TDS tree.
    pub fn package_of(&self, path: &str) -> Option<String> {
        package_of(path, &self.compiled(), &self.renames)
    }
}

fn package_of(path: &str, rules: &[regex::Regex], renames: &BTreeMap<String, String>) -> Option<String> {
    let path = path.replace('\\', "/");
    let dir = Path::new(&path).parent()?.to_str()?.to_string();
    let found = rules.iter().find_map(|rule| {
        rule.captures(&dir).and_then(|c| c.iter().skip(1).flatten().last().map(|m| m.as_str().to_string()))
    })?;
    Some(renames.get(&found).cloned().unwrap_or(found))
}

/// A package the run needs: where it is first loaded, and which of its
/// files it reads.
#[derive(Clone, Debug)]
pub struct Needed {
    pub package: String,
    pub span: Span,
    pub files: Vec<String>,
}

/// One line of the dependency file (TeX Live's `DEPENDS.txt`: an optional
/// `hard` or `soft` directive, package names, `#` comments).
#[derive(Clone, Debug, PartialEq)]
pub struct Declared {
    pub package: String,
    pub directive: Option<String>,
    pub line: usize,
}

/// What depp would write, and what the project's dependency file says.
#[derive(Clone, Debug, Default)]
pub struct Depp {
    /// Where depp is loaded, with its options, when it is.
    pub loaded: Option<(Span, Vec<String>)>,
    pub needed: Vec<Needed>,
    /// The dependency file and its entries, when it exists.
    pub file: Option<PathBuf>,
    pub declared: Vec<Declared>,
    /// What depp's options leave out, and what they add.
    pub ignored: Vec<String>,
    pub binaries: Vec<String>,
}

impl Depp {
    /// depp's view of a run: always the packages, and the dependency file
    /// when depp is loaded or the file is beside the document.
    pub fn read(analysis: &Analysis) -> Option<Depp> {
        let config = &analysis.settings.depp;
        let rules = config.compiled();
        let loaded = analysis
            .facts
            .loads
            .iter()
            .find(|load| load.name == "depp" && load.kind.is_package())
            .map(|load| (load.span, load.options.clone()));
        let options = loaded.as_ref().map(|(_, options)| options.clone()).unwrap_or_default();
        let option = |key: &str| {
            options.iter().find_map(|o| match o.split_once('=') {
                Some((k, v)) if k.trim() == key => Some(v.trim().trim_matches(['{', '}']).to_string()),
                None if o.trim() == key => Some("true".to_string()),
                _ => None,
            })
        };

        let mut needed: Vec<Needed> = Vec::new();
        for load in &analysis.facts.loads {
            if load.status != LoadStatus::Read {
                continue;
            }
            let Some(path) = &load.path else { continue };
            let Some(package) = package_of(path, &rules, &config.renames) else { continue };
            let span = visible_at(analysis, load.span);
            match needed.iter_mut().find(|n| n.package == package) {
                Some(entry) => {
                    if !entry.files.contains(path) {
                        entry.files.push(path.clone());
                    }
                    if entry.span.file != analysis.main_file && span.file == analysis.main_file {
                        entry.span = span;
                    }
                }
                None => needed.push(Needed { package, span, files: vec![path.clone()] }),
            }
        }
        // `package` mode leaves out what every LaTeX installation has
        // (depp.dtx, option `package`).
        if option("package").is_some_and(|v| v != "false") {
            needed.retain(|n| n.package != "latex");
        }
        let ignored: Vec<String> = option("ignore")
            .map(|v| crate::tex::comma_split(&v).into_iter().map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();
        needed.retain(|n| !ignored.contains(&n.package));

        // `add-binaries` adds `latex-bin`, and `luatex` under LuaTeX, or the
        // packages it names; `set-binaries` replaces that list (depp.dtx).
        let mut binaries = Vec::new();
        if let Some(value) = option("add-binaries").filter(|v| v != "false") {
            binaries = match option("set-binaries") {
                Some(list) => crate::tex::comma_split(&list).into_iter().map(|s| s.trim().to_string()).collect(),
                None => {
                    let mut list = vec!["latex-bin".to_string()];
                    if analysis.plugins.engine.has_luatex() {
                        list.push("luatex".into());
                    }
                    list
                }
            };
            if value != "true" {
                binaries.extend(crate::tex::comma_split(&value).into_iter().map(|s| s.trim().to_string()));
            }
        }

        let base =
            Path::new(analysis.file_name(analysis.main_file)).parent().map(Path::to_path_buf).unwrap_or_default();
        let jobname = Path::new(analysis.file_name(analysis.main_file))
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("texput")
            .to_string();
        // `dependency-file=jobname` writes `\jobname-DEPENDS.txt`, and the
        // default, `CTAN`, writes `DEPENDS.txt` (depp.dtx).
        let name = match option("dependency-file").as_deref() {
            None | Some("CTAN") => "DEPENDS.txt".to_string(),
            Some("jobname") => format!("{jobname}-DEPENDS.txt"),
            Some(name) => name.to_string(),
        };
        let file = config.file.clone().map(|f| base.join(f)).unwrap_or_else(|| base.join(name));
        let (file, declared) = match crate::overlay::read_to_string(&file) {
            Ok(text) => (Some(file), parse(&text)),
            Err(_) => (None, Vec::new()),
        };
        if loaded.is_none() && file.is_none() {
            return None;
        }
        Some(Depp { loaded, needed, file, declared, ignored, binaries })
    }

    /// Needed, and not in the dependency file.
    pub fn missing(&self) -> Vec<&Needed> {
        if self.file.is_none() {
            return Vec::new();
        }
        self.needed.iter().filter(|n| !self.declared.iter().any(|d| d.package == n.package)).collect()
    }

    /// In the dependency file, and nothing the run reads comes from it.
    pub fn unused(&self) -> Vec<&Declared> {
        self.declared
            .iter()
            .filter(|d| {
                !self.needed.iter().any(|n| n.package == d.package)
                    && !self.binaries.contains(&d.package)
                    && !self.ignored.contains(&d.package)
            })
            .collect()
    }

    pub fn describe(&self) -> String {
        let mut text = format!("{} TeX Live packages", self.needed.len());
        if let Some(file) = &self.file {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            text.push_str(&format!(
                "; {name} lists {}, {} missing, {} unused",
                self.declared.len(),
                self.missing().len(),
                self.unused().len()
            ));
        }
        text
    }
}

/// Where a load becomes visible in the document: its own site when that is
/// in the main file, else the site that loaded the file it stands in.
fn visible_at(analysis: &Analysis, span: Span) -> Span {
    if span.file == analysis.main_file {
        return span;
    }
    analysis.entry.get(span.file as usize).copied().flatten().unwrap_or(span)
}

/// The entries of a dependency file: `hard` and `soft` lines name packages
/// with that directive, a bare line names packages without one, and
/// `package` lines start another package's entries, which depp does not
/// write (TeX Live's `tlpkg/doc/dependencies`; depp.dtx,
/// `\_@@_depp_append_dependencies:n`).
pub fn parse(text: &str) -> Vec<Declared> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default();
        let mut words = line.split_whitespace().peekable();
        let directive = match words.peek() {
            Some(&"hard") | Some(&"soft") => words.next().map(str::to_string),
            Some(&"package") => break,
            _ => None,
        };
        for package in words {
            out.push(Declared { package: package.to_string(), directive: directive.clone(), line: index + 1 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_package_by_its_place_in_the_tree() {
        let config = DeppConfig::default();
        assert_eq!(
            config.package_of("/usr/local/texlive/2026/texmf-dist/tex/latex/base/article.cls").as_deref(),
            Some("latex")
        );
        assert_eq!(
            config.package_of("/usr/share/texmf-dist/tex/generic/pgf/frontendlayer/tikz/tikz.code.tex").as_deref(),
            Some("pgf")
        );
        assert_eq!(config.package_of("/home/me/paper/mymacros.sty"), None);
    }

    #[test]
    fn reads_the_dependency_file() {
        let declared = parse("# generated\nhard amsmath graphics\nxcolor # for colors\npackage other\nhard never\n");
        let names: Vec<&str> = declared.iter().map(|d| d.package.as_str()).collect();
        assert_eq!(names, ["amsmath", "graphics", "xcolor"]);
        assert_eq!(declared[0].directive.as_deref(), Some("hard"));
        assert_eq!(declared[2].line, 3);
    }
}
