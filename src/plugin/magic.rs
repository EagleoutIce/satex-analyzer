/// A single magic comment found in a source file, such as `% !TeX program =
/// lualatex`. Collected into [`crate::plugin::Plugins::magic`].
#[derive(Clone, Debug)]
pub struct Magic {
    pub key: String,
    pub value: String,
    pub line: usize,
}

/// Scan a source file for magic comments.
///
/// Recognizes:
/// - TeXworks/TeXShop: `% !TeX program = <engine>`, `% !TeX root = <file>`,
///   `% !TeX encoding = <encoding>`, `% !TeX spellcheck = <language>`
/// - BibTeX selection: `% !BIB program = <tool>`
/// - Arara directives: `% arara: <rule> { <options> }`
///
/// Returns magic comments found only in the leading comment block plus any
/// line matching the pattern, in order. No execution.
pub fn scan(source: &str) -> Vec<Magic> {
    let mut results = Vec::new();
    let mut non_comment_seen = false;

    for (line_num, line) in source.lines().enumerate() {
        let line_idx = line_num + 1;
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if !trimmed.starts_with('%') {
            non_comment_seen = true;
            continue;
        }

        let comment_content = &trimmed[1..].trim_start();

        if non_comment_seen && !is_magic_pattern(comment_content) {
            continue;
        }

        if let Some((key, value)) = parse_magic_comment(comment_content) {
            results.push(Magic { key, value, line: line_idx });
        }
    }

    results
}

/// Check if a comment line matches a known magic pattern.
fn is_magic_pattern(content: &str) -> bool {
    directive(content, "!TeX ").is_some() || directive(content, "!BIB ").is_some() || content.starts_with("arara:")
}

/// `content` after `prefix`, ignoring case: editors write `!TeX`, `!TEX` and
/// `!tex` alike.
fn directive<'a>(content: &'a str, prefix: &str) -> Option<&'a str> {
    let head = content.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &content[prefix.len()..])
}

/// Parse a magic comment line into (key, value) if it matches a known pattern.
fn parse_magic_comment(content: &str) -> Option<(String, String)> {
    if let Some(rest) = directive(content, "!TeX ")
        && let Some((key, value)) = rest.split_once('=')
    {
        // TeXShop's `TS-program` is TeXworks' `program`.
        let key = key.trim();
        let key = if key.eq_ignore_ascii_case("TS-program") { "program" } else { key };
        let value = value.trim();
        return Some((format!("TeX {}", key), value.to_string()));
    }

    if let Some(rest) = directive(content, "!BIB ")
        && let Some((key, value)) = rest.split_once('=')
    {
        let key = key.trim();
        let value = value.trim();
        return Some((format!("BIB {}", key), value.to_string()));
    }

    if let Some(directive) = content.strip_prefix("arara:") {
        let directive = directive.trim();
        return Some(("arara".to_string(), directive.to_string()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_tex_program() {
        let source = "% !TeX program = lualatex\n\\documentclass{article}\n";
        let magics = scan(source);
        assert_eq!(magics.len(), 1);
        assert_eq!(magics[0].key, "TeX program");
        assert_eq!(magics[0].value, "lualatex");
    }

    #[test]
    fn scans_tex_root() {
        let source = "% !TeX root = main.tex\n\\input{other.tex}\n";
        let magics = scan(source);
        assert_eq!(magics.len(), 1);
        assert_eq!(magics[0].key, "TeX root");
        assert_eq!(magics[0].value, "main.tex");
    }

    #[test]
    fn scans_any_case_and_texshop_program() {
        for source in ["% !TEX program = xelatex\n", "% !tex TS-program = xelatex\n"] {
            let magics = scan(source);
            assert_eq!(magics.len(), 1, "{source}");
            assert!(magics[0].key.eq_ignore_ascii_case("TeX program"), "{source}");
            assert_eq!(magics[0].value, "xelatex");
        }
    }

    #[test]
    fn scans_bib_program() {
        let source = "% !BIB program = biber\n\\usepackage{biblatex}\n";
        let magics = scan(source);
        assert_eq!(magics.len(), 1);
        assert_eq!(magics[0].key, "BIB program");
        assert_eq!(magics[0].value, "biber");
    }

    #[test]
    fn scans_arara_directive() {
        let source = "% arara: pdflatex\n\\documentclass{article}\n";
        let magics = scan(source);
        assert_eq!(magics.len(), 1);
        assert_eq!(magics[0].key, "arara");
        assert_eq!(magics[0].value, "pdflatex");
    }

    #[test]
    fn finds_magic_comments_after_document() {
        let source = "% !TeX program = lualatex\n\\documentclass{article}\n% !TeX root = main.tex\n";
        let magics = scan(source);
        assert_eq!(magics.len(), 2);
        assert_eq!(magics[0].key, "TeX program");
        assert_eq!(magics[1].key, "TeX root");
    }
}
