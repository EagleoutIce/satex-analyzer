//! `stubs:` in `satex.yaml`: an effect summary in place of a macro's
//! internals, said once as an `assumption`.

use satex::builtins::OccKind;
use satex::config::{Config, Load, Stub, StubKind, StubResult, StubSets};
use satex::facts::Severity;
use satex::machine::{Analysis, Machine};
use std::path::PathBuf;

const PACKAGE: &str = "\\ProvidesPackage{stubpkg}\n\\newcommand\\stubnum[1]{\\number 1#1}\n\
\\newcommand\\stubset{\\def\\stubresult{known}}\n";

fn stub(args: &str, result: StubResult, sets: &[&str]) -> Stub {
    let sets = StubSets::Names(sets.iter().map(|s| s.to_string()).collect());
    Stub { args: Some(args.into()), until: None, result, sets }
}

fn plain(kind: StubKind) -> StubResult {
    StubResult::Plain(kind)
}

fn run(name: &str, stubs: Vec<(&str, Stub)>, source: &str) -> Option<Analysis> {
    if !crate::support::installed() {
        return None;
    }
    let dir =
        crate::support::scratch_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/stubs").join(name));
    std::fs::write(dir.join("stubpkg.sty"), PACKAGE).unwrap();
    let mut cfg = Config {
        load: Load { packages: true, classes: true, ..Default::default() },
        search_paths: vec![dir.clone()],
        ..crate::support::config()
    };
    cfg.cache.dir = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/stubs-cache"));
    cfg.stubs = stubs.into_iter().map(|(n, s)| (n.to_string(), s)).collect();
    let source = format!(
        "\\documentclass{{article}}\\usepackage{{graphicx,stubpkg}}\n\\begin{{document}}\n{source}\n\\end{{document}}\n"
    );
    Some(Machine::analyze(&source, Some(&dir.join("doc.tex")), &cfg))
}

fn stubbed(a: &Analysis) -> usize {
    a.facts.diagnostics.iter().filter(|d| d.severity == Severity::Assumption && d.code == "stubbed").count()
}

#[test]
fn a_stubbed_includegraphics_keeps_labels_and_says_so_once() {
    let source = "\\begin{figure}\\includegraphics[width=1cm]{x}\\caption{c}\\label{fig}\\end{figure}\n\
\\begin{figure}\\includegraphics*{y}\\end{figure}\\ref{fig}";
    let Some(a) = run("graphics", vec![("\\includegraphics", stub("som", plain(StubKind::Box), &[]))], source) else {
        return;
    };
    let has = |kind| a.facts.occurrences.iter().any(|o| o.kind == kind && o.key == "fig");
    assert!(has(OccKind::Label) && has(OccKind::Ref));
    assert_eq!(stubbed(&a), 1, "{:#?}", a.facts.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>());
}

#[test]
fn a_stubbed_number_macro_in_an_edef_is_typed_digits() {
    let Some(a) =
        run("number", vec![("\\stubnum", stub("m", plain(StubKind::Number), &[]))], "\\edef\\r{\\stubnum{1}}")
    else {
        return;
    };
    assert!(crate::support::body(&a, "r").contains("unknown digits"), "{}", crate::support::body(&a, "r"));
}

#[test]
fn sets_makes_the_name_unknown() {
    let source = "\\def\\stubresult{a}\\stubset\\edef\\r{\\stubresult}";
    let Some(a) = run("sets", vec![("\\stubset", stub("", plain(StubKind::Nothing), &["\\stubresult"]))], source)
    else {
        return;
    };
    assert_ne!(crate::support::body(&a, "r"), "a");
    assert_eq!(stubbed(&a), 1);
}

#[test]
fn a_document_redefinition_disables_the_stub() {
    let source = "\\renewcommand\\stubnum[1]{7}\\edef\\r{\\stubnum{1}}";
    let Some(a) = run("redefined", vec![("\\stubnum", stub("m", plain(StubKind::Number), &[]))], source) else {
        return;
    };
    assert_eq!(crate::support::body(&a, "r"), "7");
    assert_eq!(stubbed(&a), 0);
}

#[test]
fn a_ranged_number_decides_a_following_test() {
    let source = "\\edef\\r{\\ifnum\\stubnum{1}<100 Y\\else N\\fi}";
    let ranged = StubResult::Number { number: [0, 99] };
    let Some(a) = run("ranged", vec![("\\stubnum", stub("m", ranged, &[]))], source) else { return };
    assert_eq!(crate::support::body(&a, "r"), "Y");
    let Some(b) = run("unranged", vec![("\\stubnum", stub("m", plain(StubKind::Number), &[]))], source) else { return };
    assert_ne!(crate::support::body(&b, "r"), "Y", "an untyped number decides nothing");
}

#[test]
fn a_typed_text_class_and_typed_sets_carry_their_type() {
    let source = "\\stubset\\edef\\r{\\ifnum\\stubresult<100 Y\\else N\\fi}";
    let typed = StubSets::Typed([("\\stubresult".to_string(), Some(StubResult::Number { number: [1, 9] }))].into());
    let mut s = stub("", plain(StubKind::Nothing), &[]);
    s.sets = typed;
    let Some(a) = run("typedsets", vec![("\\stubset", s)], source) else { return };
    assert_eq!(crate::support::body(&a, "r"), "Y");
}
