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
/// Returns them in order. No execution.
pub fn scan(source: &str) -> Vec<Magic> {
    scan_with(source, &MagicConfig::default())
}

/// `magic:` in `satex.yaml`: which comments are magic.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MagicConfig {
    /// Read `% arara:` directives as the build's steps.
    pub arara: bool,
    /// The heads of `key = value` comments, matched ignoring case: `!TeX`
    /// gives `% !TeX program = lualatex` the key `TeX program`.
    pub prefixes: Vec<String>,
}

impl Default for MagicConfig {
    fn default() -> MagicConfig {
        MagicConfig { arara: true, prefixes: vec!["!TeX".into(), "!BIB".into()] }
    }
}

/// [`scan`] with the comments `cfg` makes magic.
pub fn scan_with(source: &str, cfg: &MagicConfig) -> Vec<Magic> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let (key, value) = parse_magic_comment(line.trim().strip_prefix('%')?.trim_start(), cfg)?;
            Some(Magic { key, value, line: i + 1 })
        })
        .collect()
}

/// `content` after `prefix`, ignoring case: editors write `!TeX`, `!TEX` and
/// `!tex` alike.
fn directive<'a>(content: &'a str, prefix: &str) -> Option<&'a str> {
    let head = content.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &content[prefix.len()..])
}

/// A magic comment's key and value, if `content` is one `cfg` knows.
fn parse_magic_comment(content: &str, cfg: &MagicConfig) -> Option<(String, String)> {
    for prefix in &cfg.prefixes {
        if let Some(rest) = directive(content, &format!("{prefix} "))
            && let Some((key, value)) = rest.split_once('=')
        {
            // TeXShop's `TS-program` is TeXworks' `program`.
            let key = key.trim();
            let key = if key.eq_ignore_ascii_case("TS-program") { "program" } else { key };
            return Some((format!("{} {key}", prefix.trim_start_matches('!')), value.trim().to_string()));
        }
    }
    let directive = content.strip_prefix("arara:").filter(|_| cfg.arara)?;
    Some(("arara".to_string(), directive.trim().to_string()))
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

    #[test]
    fn arara_off_keeps_tex_program() {
        let cfg = serde_yaml_ng::from_str::<crate::config::Config>("magic: {arara: false}").unwrap().magic;
        let magics = scan_with("% arara: pdflatex\n% !TeX program = lualatex\n", &cfg);
        assert_eq!(magics.len(), 1);
        assert_eq!(magics[0].key, "TeX program");
    }
}
