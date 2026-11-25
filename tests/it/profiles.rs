//! `profiles.*` in `satex.yaml`: sensible defaults chosen from the kind of
//! input (`src/project/config.rs`'s `Profile`), and `--profile` overriding that
//! choice (`configure` in `src/main.rs`, exercised here through
//! `Config::apply_profile` directly, which is what it calls).

use std::path::Path;

use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load, Profile};

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
    assert!(!cfg.lints.unused_definition.report_public);
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
    assert!(cfg.lints.unused_definition.report_public);
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

fn analyze_as(profile: Profile, source: &str, name: &str) -> satex::analysis::machine::Analysis {
    let mut cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    cfg.apply_profile(profile).unwrap();
    Machine::analyze(source, Some(Path::new(name)), &cfg)
}

#[test]
fn a_package_s_unused_definition_is_reported_only_when_private() {
    let analysis = analyze_as(Profile::Package, PACKAGE, "demo.sty");
    let findings = satex::linter::lint(&analysis);
    let names: Vec<String> = findings
        .iter()
        .filter(|r| r["code"] == "unused-definition" && r["origin"] == "document")
        .map(|r| r["name"].to_string())
        .collect();
    assert!(names.iter().any(|m| m.contains("demo@privateCmd")), "{names:?}");
    assert!(!names.iter().any(|m| m.contains("demoPublicCmd")), "{names:?}");
}

/// The private names of `source`, read as the package `demo.sty`, that `unused-definition` reports.
fn unused_private(source: &str) -> Vec<String> {
    let analysis = analyze_as(Profile::Package, source, "demo.sty");
    let unused = |r: &&satex::queries::query::Record| r["code"] == "unused-definition" && r["origin"] == "document";
    let mut names: Vec<String> =
        satex::linter::lint(&analysis).iter().filter(unused).filter_map(|r| Some(r["name"].as_str()?.into())).collect();
    names.sort();
    names
}

/// A package read alone calls none of its commands, so no run expands their texts. A private
/// name a public command's text holds is used all the same, also one defined after that text
/// (hyperref's `\MakeLinkTarget` names `\__hyp_target_manual:nn` nine lines before it is
/// defined), and through another private name. So is one a public name copies (`\let`,
/// `\cs_new_eq:NN`). One nothing names is still reported.
#[test]
fn a_package_s_private_name_its_public_command_names_is_used() {
    let names = unused_private(
        "\\ProvidesPackage{demo}[2024/01/01 demo]
\\newcommand\\demoPublicCmd{\\demo@helper}
\\newcommand\\demo@helper{\\demo@inner}
\\newcommand\\demo@inner{x}
\\newcommand\\demo@orphan{\\demo@orphaned}
\\newcommand\\demo@orphaned{y}
\\newcommand\\demo@copied{z}
\\let\\demoPublicCopy\\demo@copied
\\endinput
",
    );
    assert_eq!(names, ["\\demo@orphan", "\\demo@orphaned"]);
}

/// A name a text builds from a parameter is any name of that shape: `\csname demo@get@#1\endcsname`
/// and expl3's `\use:c { __demo_get_#1:n }` (hyperref's `\hyperget`) use every name they can
/// build. A name of another shape is still reported.
#[test]
fn a_name_built_from_a_parameter_uses_every_name_of_its_shape() {
    let names = unused_private(
        "\\ProvidesPackage{demo}[2024/01/01 demo]
\\newcommand\\demoGet[1]{\\csname demo@get@#1\\endcsname}
\\newcommand\\demo@get@a{a}
\\newcommand\\demo@put@a{a}
\\ExplSyntaxOn
\\cs_new:Npn \\demoget #1 { \\use:c { __demo_get_#1:n } {} }
\\cs_new:Npn \\__demo_get_anchor:n #1 { a }
\\cs_new:Npn \\__demo_put_anchor:n #1 { a }
\\ExplSyntaxOff
\\endinput
",
    );
    assert_eq!(names, ["\\__demo_put_anchor:n", "\\demo@put@a"]);
}

/// What reads a library as a document is off for it, by the profile's own settings: it programs
/// in primitives, requires packages for the macros its users call, states each requirement
/// itself, defines colors for its users, lists control symbols (`\do\.`) where prose has a
/// typo, and repeats code where a macro would expand differently.
/// hyperref.sty alone gave 31 `primitive-tex-command`, 8 `unused-package` and 89 `clones`.
#[test]
fn the_library_profiles_turn_off_what_reads_a_library_as_a_document() {
    for profile in [Profile::Package, Profile::Class, Profile::Literate] {
        let mut cfg = Config::default();
        assert!(cfg.lints.clones.enabled);
        cfg.apply_profile(profile).unwrap();
        let document_only =
            ["primitive-tex-command", "unused-package", "redundant-package-load", "unused-color", "unintended-escape"];
        for code in document_only {
            assert!(cfg.lint_off.iter().any(|c| c == code), "{code} under {profile:?}");
        }
        assert!(!cfg.lints.clones.enabled, "{profile:?}");
        // What a library can get wrong stays on.
        for code in ["unused-definition", "dead-definition", "duplicate-package", "group-local-definition"] {
            assert!(!cfg.lint_off.iter().any(|c| c == code), "{code} under {profile:?}");
        }
    }
}

#[test]
fn a_document_s_unused_definition_is_reported_regardless() {
    let source = "\\documentclass{article}\n\\newcommand\\unused{x}\n\\begin{document}\\end{document}\n";
    let analysis = analyze_as(Profile::Document, source, "demo.tex");
    let findings = satex::linter::lint(&analysis);
    assert!(findings.iter().any(|r| r["code"] == "unused-definition" && r["origin"] == "document"), "{findings:?}");
    // What LaTeX writes to the `.aux` file (`\@abspage@last`) is not the
    // author's to delete.
    let aux = |r: &satex::queries::query::Record| {
        r["code"] == "unused-definition" && r["file"].as_str().is_some_and(|f| f.ends_with(".aux"))
    };
    assert!(!findings.iter().any(aux), "{findings:?}");
}

/// A project's satex.yaml may not set what only the user's own config sets
/// (`plugins.packages` reaches the network). One that writes the value in
/// effect, as a fully written reference config does, changes nothing and
/// must not warn on every run; one that tries to change it is told.
#[test]
fn a_project_repeating_a_user_setting_is_not_warned() {
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("user-keys"));
    std::fs::write(dir.join("main.tex"), "\\relax\n").unwrap();
    let warned = |yaml: &str| {
        std::fs::write(dir.join("satex.yaml"), yaml).unwrap();
        let out = crate::support::satex()
            .args(["lint", "-f", "main.tex", "--set", "kernel=plain"])
            .current_dir(&dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stderr).contains("plugins.packages ignored")
    };
    assert!(!warned("plugins:\n  packages: false\n"));
    assert!(warned("plugins:\n  packages: true\n"));
}

/// Intuition: a package is analyzed like a library of functions: each command it defines is
/// called once on unknown arguments, and an error certain for every argument is reported at the
/// definition. Unknown text may be a number, so `\count0=#1` raises nothing (sound, no guess).
/// Pins: package commands were only checked where the package itself called them.
#[test]
fn a_package_command_failing_on_every_call_is_reported() {
    let source = "\\def\\good#1{#1}\n\\def\\bad#1{\\undefinedthing #1}\n\\def\\num#1{\\count0=#1\\relax}\n";
    let analysis = analyze_as(Profile::Package, source, "calls.sty");
    let findings = satex::linter::lint(&analysis);
    let found: Vec<String> =
        findings.iter().filter(|r| r["code"] == "package-call-error").map(|r| r["message"].to_string()).collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("\\\\bad") && found[0].contains("Undefined control sequence"), "{found:?}");
}
