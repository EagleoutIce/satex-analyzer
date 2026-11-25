//! `satex updates` (src/cli/updates.rs): installed releases against the newest, and what changed.
//! Offline: a folder stands in for CTAN, its JSON records and its archives read over `file://`.

use satex::analysis::machine::Machine;
use satex::cli::Output;
use satex::project::config::{Config, Load};
use satex::queries::query::Record;
use std::path::{Path, PathBuf};

const OLD: &str = "\\ProvidesPackage{demo}[2020/01/01 v1.0 Demo]
\\DeclareOption{draft}{}
\\ProcessOptions
\\newcommand\\demoa{old}
\\newcommand\\demob[1]{#1}
\\newenvironment{demobox}{[}{]}
\\def\\demo@x{1}
";

const NEW: &str = "\\ProvidesPackage{demo}[2024/05/01 v2.0 Demo]
\\DeclareOption{draft}{}
\\DeclareOption{final}{}
\\ProcessOptions
\\newcommand\\demoa[1]{new #1}
\\newcommand\\democ{}
\\newenvironment{demobox}{[}{]}
\\def\\demo@x{2}
";

/// A project loading `demo` v1.0, a fake CTAN whose record says `version` of `date`, and a
/// mirror holding the v2.0 sources. Returns the folder and a config reading them.
fn setup(tag: &str, version: &str, date: &str) -> (PathBuf, Config) {
    let dir = crate::support::tmp_root().join(format!("updates-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["ctan", "mirror/demo"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    std::fs::write(dir.join("demo.sty"), OLD).unwrap();
    std::fs::write(dir.join("mirror/demo/demo.sty"), NEW).unwrap();
    let record =
        format!(r#"{{"id":"demo","version":{{"number":"{version}","date":"{date}"}},"ctan":{{"path":"/demo"}}}}"#);
    std::fs::write(dir.join("ctan/demo.json"), record).unwrap();
    let mut cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let url = |p: &str| format!("file://{}", crate::support::tex_path(&dir.join(p)));
    cfg.updates.index = url("ctan/{name}.json");
    cfg.updates.archives = vec![url("mirror{path}")];
    (dir, cfg)
}

/// Where the suite's cache keeps records and releases.
fn updates() -> PathBuf {
    crate::support::test_cache().join("updates")
}

/// The `demo` row of `satex updates` on a document loading it.
fn demo(dir: &Path, cfg: &Config) -> Record {
    let source = "\\documentclass{article}\n\\usepackage{demo}\n\\begin{document}\n\\end{document}\n";
    let analysis = Machine::analyze(source, Some(&dir.join("main.tex")), cfg);
    let Ok(Output::Records(rows)) = satex::cli::updates::run(&analysis, cfg) else { panic!("records") };
    rows.into_iter().find(|r| r["name"] == "demo").expect("a row for demo")
}

fn field(row: &Record, key: &str) -> String {
    row.get(key).map(|v| v.as_str().map_or(v.to_string(), str::to_string)).unwrap_or_default()
}

/// Intuition: the installed version is what `\ProvidesPackage` set when run, the newest what
/// the index record says, and an older date is outdated. Pins: both versions in the finding.
#[test]
fn an_older_release_is_outdated_with_both_versions() {
    if !crate::support::installed() {
        return;
    }
    let (dir, cfg) = setup("outdated", "2.0", "2024-05-01");
    let row = demo(&dir, &cfg);
    assert_eq!(field(&row, "status"), "outdated", "{row:?}");
    assert_eq!(field(&row, "installed"), "2020-01-01 v1.0", "what `\\ProvidesPackage` set");
    assert_eq!(field(&row, "newest"), "2024-05-01 v2.0");
    assert!(!row.contains_key("added"), "changes are opt-in (`updates.changes`)");
}

/// Intuition: the same version as the record is current, whatever the date says.
#[test]
fn the_same_version_is_current() {
    if !crate::support::installed() {
        return;
    }
    let (dir, cfg) = setup("current", "1.0", "2021-03-04");
    assert_eq!(field(&demo(&dir, &cfg), "status"), "current");
}

/// Intuition: no record (offline, fetching off, not on CTAN) knows nothing. Pins: a missing
/// record never reads as up to date.
#[test]
fn without_a_record_a_package_is_unknown() {
    if !crate::support::installed() {
        return;
    }
    let (dir, mut cfg) = setup("unknown", "1.0", "2020-01-01");
    cfg.updates.index = "https://ctan.invalid/{name}".into();
    let row = demo(&dir, &cfg);
    assert_eq!(field(&row, "status"), "unknown");
    assert!(field(&row, "note").contains("updates.fetch"), "{row:?}");
}

/// Intuition: a cached record past `refresh_days` may miss a newer release. Pins: a stale
/// cache saying "same version" gives unknown, not current.
#[test]
fn a_stale_record_is_never_current() {
    if !crate::support::installed() {
        return;
    }
    let (dir, mut cfg) = setup("stale", "1.0", "2020-01-01");
    // its own URL: a record is cached per URL, so no other test reads this one
    cfg.updates.index = "https://ctan.invalid/stale/{name}".into();
    let record = r#"{"version":{"number":"1.0","date":"2020-01-01"}}"#;
    let entry = format!(r#"{{"url":"https://ctan.invalid/stale/demo","fetched":0,"record":{record}}}"#);
    std::fs::create_dir_all(updates()).unwrap();
    std::fs::write(updates().join("demo.json"), entry).unwrap();
    let row = demo(&dir, &cfg);
    assert_eq!(field(&row, "status"), "unknown", "{row:?}");
}

/// Intuition: both releases run alone with the package, and their definitions are compared by
/// meaning: a new command and option are added, a dropped one removed, a new body or arity
/// changed, an unchanged environment absent. Pins: changes from runs, not from a text diff
/// (both files differ on every `\ProvidesPackage` line).
#[test]
fn changes_come_from_running_both_releases() {
    if !crate::support::installed() {
        return;
    }
    let (dir, mut cfg) = setup("changes", "2.0", "2024-05-01");
    cfg.updates.changes = true;
    // unpacked once per version: a copy from an earlier run of the suite would serve
    let _ = std::fs::remove_dir_all(updates().join("demo"));
    let row = demo(&dir, &cfg);
    assert_eq!(field(&row, "status"), "outdated", "{row:?}");
    assert_eq!(field(&row, "added"), "[final] \\democ", "{row:?}");
    assert_eq!(field(&row, "removed"), "\\demob");
    assert_eq!(field(&row, "changed"), "\\demoa");
    assert!(row["changed_internal"].as_u64() >= Some(1), "\\demo@x changed: {row:?}");
    assert!(updates().join("demo/2.0/demo.sty").is_file(), "the release is cached by version");
}
