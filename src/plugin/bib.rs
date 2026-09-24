//! The bibliography databases a document names: where `\bibliography{refs}`
//! finds `refs.bib`, and the keys its entries declare.

use std::path::{Path, PathBuf};

use crate::machine::Analysis;

/// One entry of a `.bib` file: its key and the line its `@type{` opens on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub line: u32,
}

/// A database as BibTeX finds it: `⟨name⟩.bib` beside the document, or the
/// name as given when it carries the extension (BibTeX reads `\bibdata`
/// names with `.bib` appended; btxdoc, "Using BibTeX").  The project walk
/// is the fallback, for a database kept in a subdirectory.
pub fn resolve(analysis: &Analysis, name: &str) -> Option<PathBuf> {
    let base = analysis
        .files
        .get(usize::from(analysis.main_file))
        .and_then(|file| Path::new(&file.path).parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let file = if name.ends_with(".bib") { name.to_string() } else { format!("{name}.bib") };
    let beside = base.join(&file);
    if beside.is_file() {
        return Some(beside);
    }
    if let found @ Some(_) = analysis
        .plugins
        .discovery
        .bibliographies
        .iter()
        .find(|path| path.ends_with(&file))
        .cloned()
    {
        return found;
    }
    // BIBINPUTS (kpathsea manual, "Supported file formats"), resolved the
    // same way as TEXINPUTS: env, then latexmkrc/Makefile, then satex.yaml.
    // Relative directories are relative to the build's working directory,
    // not the document's own.
    let bibinputs = crate::paths::effective(
        "BIBINPUTS",
        &analysis.project,
        &analysis.settings.paths.bibinputs,
        &analysis.project.root,
    );
    bibinputs.dirs.iter().map(|dir| dir.join(&file)).find(|path| path.is_file())
}

/// The entries of a `.bib` file.  `@string`, `@preamble` and `@comment`
/// declare no key; everything else is an entry whose key runs from the
/// opening delimiter to the first comma (btxdoc, "The database files").
pub fn entries(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut line = 1u32;
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        line += count_lines(&rest[..at]);
        rest = &rest[at + 1..];
        let kind_len = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
        let kind = rest[..kind_len].to_ascii_lowercase();
        let after = rest[kind_len..].trim_start();
        let Some(open) = after.chars().next().filter(|c| *c == '{' || *c == '(') else { continue };
        if kind.is_empty() || matches!(kind.as_str(), "string" | "preamble" | "comment") {
            continue;
        }
        let body = &after[open.len_utf8()..];
        let end = body.find([',', '}', ')', '\n']).unwrap_or(body.len());
        let key = body[..end].trim();
        if !key.is_empty() {
            out.push(Entry { key: key.to_string(), line });
        }
    }
    out
}

fn count_lines(text: &str) -> u32 {
    u32::try_from(text.bytes().filter(|b| *b == b'\n').count()).unwrap_or(u32::MAX)
}
