//! `satex query produces`: everywhere a piece of text can come from.

use serde_json::Value as Json;

use satex::config::Config;
use satex::machine::{Analysis, Machine};
use satex::query::{self, Record};

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

/// No format, no packages: enough for anything the modeled primitives
/// answer without a real TeX installation.
fn analyze(source: &str) -> Analysis {
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    };
    Machine::analyze(source, None, &cfg)
}

/// The sample document, with everything it loads.
fn sample() -> (Analysis, String) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/project/paper.tex");
    let source = std::fs::read_to_string(&path).expect("tests/fixtures/project/paper.tex");
    let cfg = Config { load_classes: true, ..Config::default() };
    let analysis = Machine::analyze(&source, Some(&path), &cfg);
    (analysis, source)
}

// Without a format the catcodes are INITEX's (tex.web § 232); kept out of
// `analyze()` itself since `query::produces` below needs the exact same
// `source` text it was passed, columns included.
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

fn field<'a>(record: &'a Record, name: &str) -> &'a str {
    record.get(name).and_then(Json::as_str).unwrap_or_default()
}

fn kinds(found: &[Record]) -> Vec<&str> {
    let mut all: Vec<&str> = found.iter().map(|r| field(r, "kind")).collect();
    all.sort_unstable();
    all.dedup();
    all
}

#[test]
fn a_literal_in_the_document_is_found_where_it_stands() {
    let source = format!("{PRELUDE}\\def\\price{{3.5}}% the price is 3.5");
    let analysis = analyze(&source);
    let found = query::produces(&analysis, &source, "3.5");
    let source: Vec<&Record> = found.iter().filter(|r| field(r, "kind") == "source").collect();
    assert_eq!(source.len(), 2, "{found:?}");
    assert_eq!(source[0].get("col"), Some(&Json::from(12 + PRELUDE.len() as i64)));
    assert_eq!(field(source[0], "tag"), "input");
    // The second occurrence is commented out, so it produces nothing.
    assert_eq!(field(source[1], "tag"), "comment");
    let body = found.iter().find(|r| field(r, "kind") == "macro body").expect("the macro body");
    assert_eq!(field(body, "name"), r"\price");
}

#[test]
fn a_literal_only_a_package_writes_is_found_in_the_package() {
    if !installed() {
        return;
    }
    let (analysis, source) = sample();
    // `\setlength{\my@indent}{1.5em}` stands in tests/fixtures/project/mypackage.sty alone.
    let found = query::produces(&analysis, &source, "1.5em");
    let package: Vec<&Record> =
        found.iter().filter(|r| field(r, "kind") == "source" && field(r, "file") == "mypackage.sty").collect();
    assert_eq!(package.len(), 1, "{package:?}");
    assert_eq!(field(package[0], "tag"), "package");
    assert!(!found.iter().any(|r| field(r, "file") == "paper.tex"), "the document itself never writes 1.5em");
}

#[test]
fn text_that_only_exists_after_expansion_is_still_answered() {
    if !installed() {
        return;
    }
    let (analysis, source) = sample();
    // The title reads `A Sample \DocName{} for sa\TeX`, so the expanded text
    // stands nowhere in the source.
    let found = query::produces(&analysis, &source, "A Sample Document");
    assert_eq!(kinds(&found), vec!["metadata"], "{found:?}");
    assert_eq!(field(&found[0], "name"), r"\title");
    assert_eq!(field(&found[0], "tag"), "title");
}

#[test]
fn a_dimension_is_found_as_written_and_as_the_register_holds_it() {
    let source = r"\parindent=1.5cm\relax";
    let analysis = analyze(source);
    let written = query::produces(&analysis, source, "1.5cm");
    assert_eq!(kinds(&written), vec!["source"], "{written:?}");
    assert_eq!(written[0].get("col"), Some(&Json::from(12)));
    // A dimension prints in points, whatever unit assigned it.
    let held = query::produces(&analysis, source, "42.67912pt");
    let value = held.iter().find(|r| field(r, "kind") == "value").expect("the register value");
    assert_eq!(field(value, "name"), r"\parindent");
    assert_eq!(field(value, "detail"), "42.67912pt");
}

#[test]
fn a_text_that_occurs_nowhere_finds_nothing() {
    let source = r"\def\a{b}";
    let analysis = analyze(source);
    assert!(query::produces(&analysis, source, "nowhere-at-all").is_empty());
    // An empty search would match everything, and answers nothing.
    assert!(query::produces(&analysis, source, "").is_empty());
}

#[test]
fn the_search_text_is_a_literal_not_a_pattern() {
    let source = format!("{PRELUDE}\\def\\pattern{{(a|b)}}");
    let analysis = analyze(&source);
    let found = query::produces(&analysis, &source, "(a|b)");
    assert_eq!(kinds(&found), vec!["macro body", "source"], "{found:?}");
    // `a.b` as a regular expression would match `a|b` in that same body; as
    // a literal it matches nothing, and neither does a character class.
    assert!(query::produces(&analysis, &source, "a.b").is_empty());
    assert!(query::produces(&analysis, &source, r"[ab]").is_empty());
}
