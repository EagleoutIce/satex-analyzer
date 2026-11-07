//! The formal semantics covers every primitive SaTeX knows: each name the
//! engines start with has a row in `semantics/primitives.tsv`, and each rule
//! a row names is a constructor in `semantics/lean/Satex/Concrete.lean`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use satex::engine::builtins::primitive_names;
use satex::project::config::Engine;

const ENGINES: [Engine; 4] = [Engine::Tex, Engine::PdfTeX, Engine::XeTeX, Engine::LuaTeX];

fn repo(path: &str) -> String {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

struct Row {
    name: String,
    rules: Vec<String>,
    kind: String,
}

/// The rows of the table; a control space is written `\ `.
fn rows() -> Vec<Row> {
    repo("semantics/primitives.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("name\t") && !l.is_empty())
        .map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            assert_eq!(cols.len(), 4, "a row has name, rule, source, kind: {l:?}");
            let name = if cols[0] == "\\ " { " ".to_string() } else { cols[0].to_string() };
            let rules = cols[1].split(',').filter(|r| *r != "-").map(str::to_string).collect();
            Row { name, rules, kind: cols[3].to_string() }
        })
        .collect()
}

fn known() -> BTreeSet<String> {
    ENGINES.iter().flat_map(|e| primitive_names(*e).keys().cloned()).collect()
}

#[test]
fn every_primitive_has_a_semantics_row() {
    let table: BTreeSet<String> = rows().into_iter().map(|r| r.name).collect();
    let known = known();
    let missing: Vec<_> = known.difference(&table).collect();
    assert!(missing.is_empty(), "primitives without a row in semantics/primitives.tsv: {missing:?}");
    let stale: Vec<_> = table.difference(&known).collect();
    assert!(stale.is_empty(), "rows for names no engine has: {stale:?}");
}

/// The source text agrees: every name `initial_meanings` registers by a
/// literal has a row (a check that does not depend on the engine tables).
#[test]
fn every_registered_literal_has_a_semantics_row() {
    let source = repo("src/engine/builtins.rs");
    let start = source.find("pub fn initial_meanings").expect("initial_meanings");
    let end = start + source[start..].find("\npub fn ").expect("end of initial_meanings");
    let table: BTreeSet<String> = rows().into_iter().map(|r| r.name).collect();
    let missing: Vec<&str> = source[start..end]
        .split("put(it, \"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|name| !table.contains(*name))
        .collect();
    assert!(missing.is_empty(), "registered in builtins.rs without a semantics row: {missing:?}");
}

/// The constructors of `inductive <name>` in the Lean source.
fn constructors(lean: &str, inductive: &str) -> Option<BTreeSet<String>> {
    let head = format!("inductive {inductive} ");
    let at = lean.find(&head).or_else(|| lean.find(&format!("inductive {inductive}\n")))?;
    let body = &lean[at + head.len()..];
    let end =
        ["\ninductive ", "\nend", "\ndef ", "\nmutual"].iter().filter_map(|m| body.find(m)).min().unwrap_or(body.len());
    Some(
        body[..end]
            .lines()
            .filter_map(|l| l.trim_start().strip_prefix("| "))
            .filter_map(|l| l.split([' ', ':']).next())
            .map(str::to_string)
            .collect(),
    )
}

#[test]
fn every_row_names_lean_rules() {
    let lean = repo("semantics/lean/Satex/Concrete.lean");
    let mut wrong = Vec::new();
    for row in rows() {
        assert!(matches!(row.kind.as_str(), "exact" | "over" | "open"), "kind of {:?}: {}", row.name, row.kind);
        assert_eq!(row.rules.is_empty(), row.kind == "open", "{:?}: an open row names no rule, others do", row.name);
        for rule in &row.rules {
            let (inductive, ctor) = rule.split_once('.').unwrap_or((rule, ""));
            if !constructors(&lean, inductive).is_some_and(|c| c.contains(ctor)) {
                wrong.push(format!("{} -> {rule}", row.name));
            }
        }
    }
    assert!(wrong.is_empty(), "rules not found in Concrete.lean: {wrong:?}");
}

/// `--list`, the wiki and the playground show the queries in this order: by name.
#[test]
fn queries_are_listed_by_name() {
    let names: Vec<&str> = satex::queries::query::Query::ALL.iter().map(|(n, _)| *n).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}
