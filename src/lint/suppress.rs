//! Magic comments that suppress findings, read from the document's own
//! text rather than any table: `% satex-disable-next-line CODE[,CODE]`
//! suppresses the line after it, `% satex-disable-line …` (trailing on a
//! line of code) suppresses that same line, `% satex-disable CODE…` and
//! `% satex-enable CODE…` open and close a region, and `%
//! satex-disable-file CODE…` suppresses everywhere in the file.  Leaving
//! off the codes means every code.  [`apply`] runs once, after every rule
//! has produced its findings, so every output format and `--fix` (which
//! relints after each round) see the same filtered set.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::json;

use crate::lint::{Category, Rule, Severity};
use crate::machine::Analysis;
use crate::query::Record;

pub static RULES: &[Rule] = &[Rule {
    code: "unknown-suppress-code",
    category: Category::Style,
    severity: Severity::Warning,
    summary: "a satex-disable comment names a code no rule has",
    explanation: "\
A `satex-disable…`/`satex-enable` magic comment named a code that
`satex lint --rules` does not list, most often a typo.  It suppresses
nothing, since matching a finding's own code is all these comments do.

Fix by correcting the code, or removing it if the rule was renamed or
never existed.",
    // The finding is produced by `suppress::apply`, which already scans
    // every magic comment while it builds the suppression map; a second,
    // ordinary rule pass would just repeat that scan.
    run: |_| {},
}];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    NextLine,
    ThisLine,
    DisableFile,
    Disable,
    Enable,
}

/// Longest keyword first, so `satex-disable-line` is not read as
/// `satex-disable` with a stray word after it.
const KEYWORDS: &[(&str, Kind)] = &[
    ("satex-disable-next-line", Kind::NextLine),
    ("satex-disable-line", Kind::ThisLine),
    ("satex-disable-file", Kind::DisableFile),
    ("satex-disable", Kind::Disable),
    ("satex-enable", Kind::Enable),
];

struct Directive {
    kind: Kind,
    /// `None` means every code.
    codes: Option<Vec<String>>,
}

/// The text after a line's comment start.
fn comment_of(line: &str) -> Option<&str> {
    crate::tex::comment_start(line).map(|at| &line[at + 1..])
}

fn parse_directive(comment: &str) -> Option<Directive> {
    let rest = comment.trim_start();
    let (kind, rest) = KEYWORDS.iter().find_map(|(kw, kind)| rest.strip_prefix(kw).map(|r| (*kind, r)))?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None; // e.g. `satex-disable-foo`: not one of ours
    }
    let codes = rest
        .split_whitespace()
        .next()
        .map(|list| list.split(',').map(str::trim).filter(|c| !c.is_empty()).map(str::to_string).collect::<Vec<_>>())
        .filter(|v| !v.is_empty());
    Some(Directive { kind, codes })
}

#[derive(Default)]
struct FileSuppressions {
    file_all: bool,
    file_codes: HashSet<String>,
    next_line: HashMap<u32, Option<Vec<String>>>,
    this_line: HashMap<u32, Option<Vec<String>>>,
    /// `Disable`/`Enable` directives only, in source order.
    regions: Vec<(u32, Directive)>,
}

impl FileSuppressions {
    fn suppressed(&self, line: u32, code: &str) -> bool {
        let names = |codes: &Option<Vec<String>>| codes.as_ref().is_none_or(|cs| cs.iter().any(|c| c == code));
        if self.file_all || self.file_codes.contains(code) {
            return true;
        }
        if self.this_line.get(&line).is_some_and(names) || self.next_line.get(&line).is_some_and(names) {
            return true;
        }
        // A region's state as of `line`: `all` once a bare `disable` is
        // seen, `exceptions` are codes an `enable` carved back out of it;
        // outside `all`, `specific` are the codes a bare-less `disable`
        // named directly. Recomputed per query: regions are few, findings
        // are not, and a finding's own suppression never changes a run.
        let (mut all, mut specific, mut exceptions) = (false, HashSet::<&str>::new(), HashSet::<&str>::new());
        for (at, dir) in &self.regions {
            if *at > line {
                break;
            }
            match (dir.kind, &dir.codes) {
                (Kind::Disable, None) => {
                    all = true;
                    exceptions.clear();
                }
                (Kind::Disable, Some(cs)) if all => exceptions.retain(|e| !cs.iter().any(|c| c == e)),
                (Kind::Disable, Some(cs)) => specific.extend(cs.iter().map(String::as_str)),
                (Kind::Enable, None) => {
                    all = false;
                    specific.clear();
                    exceptions.clear();
                }
                (Kind::Enable, Some(cs)) if all => exceptions.extend(cs.iter().map(String::as_str)),
                (Kind::Enable, Some(cs)) => specific.retain(|s| !cs.iter().any(|c| c == s)),
                _ => {}
            }
        }
        if all { !exceptions.contains(code) } else { specific.contains(code) }
    }
}

fn unknown_finding(path: &str, line: u32, col: u32, code: &str) -> Record {
    let mut record = serde_json::Map::new();
    record.insert("file".into(), json!(Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path)));
    record.insert("path".into(), json!(path));
    record.insert("line".into(), json!(line));
    record.insert("col".into(), json!(col));
    record.insert("code".into(), json!("unknown-suppress-code"));
    record.insert("category".into(), json!(Category::Style.as_str()));
    record.insert("severity".into(), json!(Severity::Warning.as_str()));
    record.insert("name".into(), json!(code));
    record.insert("origin".into(), json!("document"));
    record.insert("message".into(), json!(format!("`{code}` is not a rule satex lint --rules knows")));
    record.insert("fix".into(), json!(null));
    record
}

fn scan(path: &str, text: &str) -> (FileSuppressions, Vec<Record>) {
    let mut sup = FileSuppressions::default();
    let mut warnings = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let ln = i as u32 + 1;
        let Some(comment) = comment_of(line) else { continue };
        let Some(dir) = parse_directive(comment) else { continue };
        if let Some(codes) = &dir.codes {
            for code in codes {
                if crate::lint::explanation(code).is_none() {
                    let col = line.find(code.as_str()).map_or(1, |b| line[..b].chars().count() as u32 + 1);
                    warnings.push(unknown_finding(path, ln, col, code));
                }
            }
        }
        match dir.kind {
            Kind::NextLine => {
                sup.next_line.insert(ln + 1, dir.codes);
            }
            Kind::ThisLine => {
                sup.this_line.insert(ln, dir.codes);
            }
            Kind::DisableFile => match dir.codes {
                None => sup.file_all = true,
                Some(cs) => sup.file_codes.extend(cs),
            },
            Kind::Disable | Kind::Enable => sup.regions.push((ln, dir)),
        }
    }
    (sup, warnings)
}

/// `records`, with everything a magic comment suppressed removed, and a
/// warning added for every code such a comment named that no rule has.
pub fn apply(analysis: &Analysis, records: Vec<Record>) -> Vec<Record> {
    let mut by_file: HashMap<String, FileSuppressions> = HashMap::new();
    let mut extra = Vec::new();
    for id in 0..analysis.files.len() as crate::tex::FileId {
        if crate::query::origin(analysis, id) != "document" {
            continue;
        }
        let path = analysis.file_name(id).to_string();
        if by_file.contains_key(&path) {
            continue;
        }
        let Ok(text) = crate::overlay::read_to_string(Path::new(&path)) else { continue };
        let (sup, warnings) = scan(&path, &text);
        extra.extend(warnings);
        by_file.insert(path, sup);
    }
    let mut out: Vec<Record> = records
        .into_iter()
        .filter(|r| {
            let path = r.get("path").and_then(|v| v.as_str());
            let code = r.get("code").and_then(|v| v.as_str());
            let line = r.get("line").and_then(|v| v.as_u64());
            match (path, code, line) {
                (Some(path), Some(code), Some(line)) => {
                    !by_file.get(path).is_some_and(|s| s.suppressed(line as u32, code))
                }
                _ => true,
            }
        })
        .collect();
    out.extend(extra);
    out
}
