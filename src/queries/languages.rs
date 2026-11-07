//! Code languages (`Ty::Key(KeyKind::Language)`): the listings languages the run defined and
//! those its language files declare, and the Pygments lexers minted hands its code to.

use crate::analysis::machine::Analysis;
use crate::analysis::roles::{KeyKind, Ty};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use crate::analysis::observe::LISTINGS_PREFIX;

/// A language a position may name.
#[derive(Clone, Debug, PartialEq)]
pub struct Lang {
    /// As written: `Python`, `[LaTeX]TeX`, `python`.
    pub name: String,
    pub aliases: Vec<String>,
    /// Where it is defined: a language file, the document's line, Pygments.
    pub source: String,
}

/// listings' key for a name as written: `[LaTeX] TeX` is `tex$latex`, as `\lst@NormedDef`
/// lowercases and drops spaces.
pub fn listings_key(name: &str) -> String {
    let n: String = name.chars().filter(|c| !c.is_whitespace() && !"{}".contains(*c)).collect();
    let n = n.to_lowercase();
    match n.strip_prefix('[').and_then(|r| r.split_once(']')) {
        Some((d, l)) => format!("{l}${d}"),
        None => format!("{n}$"),
    }
}

/// The name a listings key is written as, from the casings in `cased`.
fn written(key: &str, cased: &BTreeMap<String, String>) -> String {
    let (l, d) = key.split_once('$').unwrap_or((key, ""));
    let case = |s: &str| cased.get(s).cloned().unwrap_or_else(|| s.to_string());
    if d.is_empty() { case(l) } else { format!("[{}]{}", case(d), case(l)) }
}

/// A language file's declarations: each language's dialect, if any, and name.
type Declared = Vec<(Option<String>, String)>;

/// The `[dialect]{Language}` pairs a listings language file declares, read once per file.
fn declared(path: &std::path::Path) -> Declared {
    static READ: Mutex<BTreeMap<std::path::PathBuf, Declared>> = Mutex::new(BTreeMap::new());
    if let Some(found) = READ.lock().ok().and_then(|r| r.get(path).cloned()) {
        return found;
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut out = Vec::new();
    for (at, _) in text.match_indices("definelanguage") {
        let lead = &text[..at];
        if !(lead.ends_with("\\lst@") || lead.ends_with("\\lst")) {
            continue;
        }
        let rest = text[at + "definelanguage".len()..].trim_start();
        let (dialect, rest) = match rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
            Some((d, r)) => (Some(d.trim().to_string()).filter(|d| !d.is_empty()), r.trim_start()),
            None => (None, rest),
        };
        if let Some((name, _)) = rest.strip_prefix('{').and_then(|r| r.split_once('}'))
            && !name.contains(['\\', '#'])
        {
            out.push((dialect, name.trim().to_string()));
        }
    }
    if let Ok(mut r) = READ.lock() {
        r.insert(path.to_path_buf(), out.clone());
    }
    out
}

/// The text the document wrote at each span the run typed a code language: lowercase to as written.
fn document_casing(analysis: &Analysis) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for word in used(analysis) {
        out.entry(word.to_lowercase()).or_insert(word);
    }
    out
}

/// The words at the spans the run typed a code language in the project's files, in their order:
/// the run types one where listings formed its driver from it.
pub fn used(analysis: &Analysis) -> Vec<String> {
    let mut spans: Vec<_> = (analysis.facts.types.iter())
        .filter(|(s, t)| **t == Ty::Key(KeyKind::Language) && analysis.project_file(s.file))
        .map(|(s, _)| *s)
        .collect();
    spans.sort_by_key(|s| (s.file, s.line, s.col));
    let mut texts: BTreeMap<_, Option<String>> = BTreeMap::new();
    let mut out = Vec::new();
    for s in spans {
        let text = texts.entry(s.file).or_insert_with(|| {
            let path = &analysis.files.get(usize::from(s.file))?.path;
            crate::project::overlay::read_to_string(std::path::Path::new(path)).ok()
        });
        let Some(line) = text.as_deref().and_then(|t| t.lines().nth(s.line.saturating_sub(1) as usize)) else {
            continue;
        };
        let rest = &line[crate::engine::source::byte_of_col(line, s.col).min(line.len())..];
        let word: String = rest.chars().take_while(|c| c.is_alphanumeric() || "+-#/._ ".contains(*c)).collect();
        let word = word.trim().to_string();
        if !word.is_empty() && !out.contains(&word) {
            out.push(word);
        }
    }
    out
}

/// The listings languages: those the run defined, then those its language files
/// (`\lstlanguagefiles`) declare, with whether that set is whole (the list of files was known).
/// `None` without listings.
pub fn listings(analysis: &Analysis) -> Option<(Vec<Lang>, bool)> {
    let files = analysis.interner.lookup("lstlanguagefiles").and_then(|s| {
        let m = analysis.env.meaning(s);
        Some(crate::engine::tex::detokenize(&m.as_macro()?.replacement_text, &analysis.interner))
    });
    let defs: Vec<_> =
        (analysis.facts.defs.iter()).filter(|d| analysis.interner.name(d.name).starts_with(LISTINGS_PREFIX)).collect();
    if files.is_none() && defs.is_empty() {
        return None;
    }
    let mut cased = document_casing(analysis);
    // key, then where it is defined
    let mut keys: BTreeMap<String, String> = BTreeMap::new();
    for name in files.as_deref().unwrap_or_default().split(',').map(str::trim).filter(|n| !n.is_empty()) {
        let Some(path) = crate::caches::distribution::tex_file(&analysis.distribution.roots, name) else { continue };
        for (dialect, lang) in declared(&path) {
            cased.entry(lang.to_lowercase()).or_insert_with(|| lang.clone());
            let key = format!("{}${}", lang.to_lowercase(), dialect.as_deref().unwrap_or("").to_lowercase());
            if let Some(d) = dialect {
                cased.entry(d.to_lowercase()).or_insert(d);
            }
            keys.entry(key).or_insert_with(|| name.to_string());
        }
    }
    for d in defs {
        let key = analysis.interner.name(d.name)[LISTINGS_PREFIX.len()..].to_string();
        let file = analysis.files.get(usize::from(d.span.file)).map(|f| f.path.clone()).unwrap_or_default();
        let base = std::path::Path::new(&file).file_name().map(|n| n.to_string_lossy().to_string());
        let source = match analysis.project_file(d.span.file) {
            true => format!("{}:{}", base.unwrap_or_default(), d.span.line),
            false => base.unwrap_or_default(),
        };
        // the document's own definition says where it is, over the file that declares it too
        if analysis.project_file(d.span.file) || !keys.contains_key(&key) {
            keys.insert(key, source);
        }
    }
    // each language bare too, as its default dialect answers it
    let bare: BTreeMap<String, String> =
        keys.iter().filter_map(|(k, s)| Some((format!("{}$", k.split_once('$')?.0), s.clone()))).collect();
    for (b, source) in bare {
        keys.entry(b).or_insert(source);
    }
    let langs = keys.into_iter().map(|(k, source)| Lang { name: written(&k, &cased), aliases: Vec::new(), source });
    Some((langs.collect(), files.is_some()))
}

/// The Pygments lexers, `pygmentize -L lexers` read once (each lexer's first alias, its other
/// aliases), else the completion seed.
pub fn pygments(analysis: &Analysis) -> Vec<Lang> {
    static LEXERS: OnceLock<Option<Vec<Lang>>> = OnceLock::new();
    let program = &analysis.settings.type_plugins.language.pygmentize;
    let lexers = LEXERS.get_or_init(|| lexers(program));
    lexers.clone().unwrap_or_else(|| {
        SEED.iter().map(|n| Lang { name: n.to_string(), aliases: Vec::new(), source: "minted".into() }).collect()
    })
}

/// Common lexers, where Pygments is not installed (an editor completion seed).
const SEED: [&str; 24] = [
    "bash",
    "c",
    "cpp",
    "csharp",
    "css",
    "go",
    "haskell",
    "html",
    "java",
    "javascript",
    "json",
    "julia",
    "latex",
    "lua",
    "make",
    "matlab",
    "perl",
    "php",
    "python",
    "r",
    "ruby",
    "rust",
    "sql",
    "text",
];

fn lexers(program: &str) -> Option<Vec<Lang>> {
    if program.is_empty() {
        return None;
    }
    let out = std::process::Command::new(program).args(["-L", "lexers"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut langs = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(names) = line.strip_prefix("* ").and_then(|l| l.strip_suffix(':')) else { continue };
        let title = lines.next().unwrap_or_default().trim();
        let title = title.split(" (filenames").next().unwrap_or(title).trim();
        let mut aliases = names.split(',').map(str::trim).filter(|a| !a.is_empty()).map(String::from);
        let Some(name) = aliases.next() else { continue };
        langs.push(Lang { name, aliases: aliases.collect(), source: format!("Pygments: {title}") });
    }
    (!langs.is_empty()).then_some(langs)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_listings_name_keys_as_its_driver() {
        assert_eq!(super::listings_key("Python"), "python$");
        assert_eq!(super::listings_key("{[LaTeX] TeX}"), "tex$latex");
    }

    #[test]
    fn a_value_names_its_key() {
        fn key(t: &str) -> Option<&str> {
            crate::analysis::roles::value_key(t, t.len())
        }
        assert_eq!(key("[numbers=left, language={[LaTeX]"), Some("language"));
        assert_eq!(key("\\lstset{language="), Some("language"));
        assert_eq!(key("{python"), None);
        assert_eq!(key("language="), Some("language"));
        assert_eq!(key("a, b"), None);
    }
}
