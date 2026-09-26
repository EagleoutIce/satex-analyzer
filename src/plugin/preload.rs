//! Preloaded formats: the `.fmt` files a build starts from instead of the
//! stock `latex`, such as the ones `mylatexformat` dumps from a document's
//! own preamble.

use std::path::{Path, PathBuf};

/// A format file the build preloads, and how satex learned of it: a
/// `%&⟨format⟩` line (web2c manual, "Format files"), an `-fmt=` in the build
/// configuration, or `preload:` in the configuration.  `source` is the
/// preamble that was dumped, which satex interprets in place of the `.fmt`.
#[derive(Clone, Debug)]
pub struct Preload {
    pub name: String,
    pub how: &'static str,
    pub source: Option<PathBuf>,
}

/// The `%&⟨format⟩` line: TeX honors it only as the very first line of the
/// file, before any other character.
pub fn requested(text: &str) -> Option<String> {
    let first = text.lines().next()?;
    let name = first.strip_prefix("%&")?.trim();
    // `%&latex -translate-file=…`: the format is the first word.
    let name = name.split_whitespace().next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// The `-fmt=⟨format⟩` a build configuration passes to the engine.
pub fn from_command(command: &str) -> Option<String> {
    command
        .split_whitespace()
        .find_map(|word| word.strip_prefix("-fmt=").or_else(|| word.strip_prefix("--fmt=")))
        .map(str::to_string)
}

/// Where a format dumped by `mylatexformat` stops: everything before the
/// dump point went into the `.fmt`, and the rest is the document.
/// `mylatexformat` ends the preamble at `\endofdump`, and a format dumped by
/// hand ends at `\dump` (The TeXbook, appendix A); a preamble with neither
/// ends where the document body begins.
pub fn dumped_part(text: &str) -> &str {
    let end = ["\\endofdump", "\\dump", "\\begin{document}"].iter().filter_map(|marker| text.find(marker)).min();
    match end {
        Some(at) => &text[..at],
        None => text,
    }
}

impl Preload {
    /// The format this run should start from: what the source asks for, else
    /// what the build configuration passes to the engine.
    pub fn detect(text: &str, engine_command: Option<&str>, base: &Path, found: &super::Discovery) -> Option<Preload> {
        let (name, how) = match requested(text) {
            Some(name) => (name, "%& line"),
            None => (from_command(engine_command?)?, "build configuration"),
        };
        // The stock formats are what satex already interprets from
        // `latex.ltx`; only a format of the project's own needs its source.
        if matches!(name.as_str(), "latex" | "pdflatex" | "lualatex" | "xelatex" | "tex" | "pdftex") {
            return None;
        }
        // The dumped preamble is a file of the project named after the
        // format: beside the document, or wherever the walk found it.
        let source = ["tex", "ltx"]
            .iter()
            .map(|extension| base.join(&name).with_extension(extension))
            .find(|path| path.is_file())
            .or_else(|| {
                found
                    .documents
                    .iter()
                    .chain(&found.inputs)
                    .find(|path| path.file_stem().is_some_and(|stem| stem == name.as_str()))
                    .cloned()
            });
        Some(Preload { name, how, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_nothing_for_a_stock_format() {
        let found = super::super::Discovery::default();
        assert!(Preload::detect("%&latex\n", None, Path::new("."), &found).is_none());
    }

    #[test]
    fn reads_the_format_line() {
        assert_eq!(requested("%&mypreamble\n\\section{a}\n").as_deref(), Some("mypreamble"));
        assert_eq!(requested("\n%&mypreamble\n"), None);
    }

    #[test]
    fn reads_the_engine_flag() {
        assert_eq!(from_command("pdflatex -fmt=mypreamble %S").as_deref(), Some("mypreamble"));
        assert_eq!(from_command("pdflatex %S"), None);
    }

    #[test]
    fn cuts_the_preamble_at_the_dump_point() {
        assert_eq!(dumped_part("\\usepackage{a}\n\\endofdump\n\\rest"), "\\usepackage{a}\n");
        assert_eq!(dumped_part("\\usepackage{a}\n\\begin{document}x"), "\\usepackage{a}\n");
    }

    #[test]
    fn cuts_at_the_first_marker_whichever_it_is() {
        assert_eq!(dumped_part("\\usepackage{a}\n\\begin{document}\\dump"), "\\usepackage{a}\n");
    }
}
