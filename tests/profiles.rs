//! `profiles.*` in `satex.yaml`: sensible defaults chosen from the kind of
//! input (`src/config.rs`'s `Profile`), and `--profile` overriding that
//! choice (`configure` in `src/main.rs`, exercised here through
//! `Config::apply_profile` directly, which is what it calls).

use std::path::Path;

use satex::config::{Config, Profile};
use satex::machine::Machine;

const PACKAGE: &str = "\
\\NeedsTeXFormat{LaTeX2e}
\\ProvidesPackage{demo}[2024/01/01 demo]
\\newcommand\\demoPublicCmd{public}
\\newcommand\\demo@privateCmd{private}
\\endinput
";

const DOCUMENT: &str = "\
\\documentclass{article}
\\begin{document}
hello
\\end{document}
";

const PLAIN: &str = "\
\\catcode`\\@=11
\\def\\greet{hello}
\\greet
\\bye
";

#[test]
fn a_sty_extension_detects_the_package_profile() {
    assert_eq!(Profile::detect(Some(Path::new("demo.sty")), PACKAGE), Profile::Package);
}

#[test]
fn a_cls_extension_detects_the_class_profile() {
    assert_eq!(Profile::detect(Some(Path::new("demo.cls")), "\\ProvidesClass{demo}\n"), Profile::Class);
}

#[test]
fn a_dtx_is_read_by_its_provides_line() {
    assert_eq!(Profile::detect(Some(Path::new("demo.dtx")), "\\ProvidesClass{demo}\n"), Profile::Class);
    assert_eq!(Profile::detect(Some(Path::new("demo.dtx")), "\\ProvidesPackage{demo}\n"), Profile::Package);
}

#[test]
fn a_tex_extension_is_sniffed_from_what_it_starts_with() {
    assert_eq!(Profile::detect(Some(Path::new("demo.tex")), DOCUMENT), Profile::Document);
    assert_eq!(Profile::detect(Some(Path::new("demo.tex")), PACKAGE), Profile::Package);
    assert_eq!(Profile::detect(Some(Path::new("demo.tex")), PLAIN), Profile::Plain);
    assert_eq!(Profile::detect(None, PLAIN), Profile::Plain);
}

#[test]
fn the_package_profile_turns_off_document_only_lints_and_at_letter() {
    let mut cfg = Config::default();
    cfg.apply_profile(Profile::Package).unwrap();
    assert_eq!(cfg.profile, Some(Profile::Package));
    assert!(cfg.at_letter);
    assert!(!cfg.report_public_definitions);
    assert!(cfg.lint_off.iter().any(|c| c == "unused-label"));
    assert!(cfg.lint_off.iter().any(|c| c == "build-engine-mismatch"));
    // A rule that matters for package code is not turned off.
    assert!(!cfg.lint_off.iter().any(|c| c == "expl3-signature-mismatch"));
    assert!(!cfg.lint_off.iter().any(|c| c == "unguarded-recursion"));
}

#[test]
fn the_document_profile_changes_nothing() {
    let mut cfg = Config::default();
    cfg.apply_profile(Profile::Document).unwrap();
    assert_eq!(cfg.profile, Some(Profile::Document));
    assert!(!cfg.at_letter);
    assert!(cfg.report_public_definitions);
    assert_eq!(cfg.lint_off, Config::default().lint_off);
}

#[test]
fn explicit_config_still_wins_over_the_profile() {
    // `configure` in main.rs applies the profile before `--config`/`--set`,
    // so a later `--set` on the same key overrides what the profile chose.
    let mut cfg = Config::default();
    cfg.apply_profile(Profile::Package).unwrap();
    cfg.set("at_letter", "false").unwrap();
    assert!(!cfg.at_letter);
}

fn analyze_as(profile: Profile, source: &str, name: &str) -> satex::machine::Analysis {
    let mut cfg = Config { load_classes: true, ..Config::default() };
    cfg.apply_profile(profile).unwrap();
    Machine::analyze(source, Some(Path::new(name)), &cfg)
}

#[test]
fn a_package_s_unused_definition_is_reported_only_when_private() {
    let analysis = analyze_as(Profile::Package, PACKAGE, "demo.sty");
    let findings = satex::lint::lint(&analysis);
    let names: Vec<String> = findings
        .iter()
        .filter(|r| r["code"] == "unused-definition" && r["origin"] == "document")
        .map(|r| r["name"].to_string())
        .collect();
    assert!(names.iter().any(|m| m.contains("demo@privateCmd")), "{names:?}");
    assert!(!names.iter().any(|m| m.contains("demoPublicCmd")), "{names:?}");
}

#[test]
fn a_document_s_unused_definition_is_reported_regardless() {
    let source = "\\documentclass{article}\n\\newcommand\\unused{x}\n\\begin{document}\\end{document}\n";
    let analysis = analyze_as(Profile::Document, source, "demo.tex");
    let findings = satex::lint::lint(&analysis);
    assert!(
        findings.iter().any(|r| r["code"] == "unused-definition" && r["origin"] == "document"),
        "{findings:?}"
    );
}
