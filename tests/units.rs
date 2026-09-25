//! `hand-set-quantity`: what the run has to have typeset for a number and a
//! unit beside it to be worth reporting, and what must not be reported.

use std::path::PathBuf;

use satex::config::Config;
use satex::machine::Machine;

const CODE: &str = "hand-set-quantity";

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join("satex-units").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// The `name` of every `hand-set-quantity` finding on a document whose body
/// is `body`, in the order they were reported.
fn findings(name: &str, body: &str) -> Vec<String> {
    findings_with(name, "", body)
}

fn findings_with(name: &str, preamble: &str, body: &str) -> Vec<String> {
    let directory = scratch(name);
    let document = format!("\\documentclass{{article}}\n{preamble}\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = Config { load_classes: true, ..Config::default() };
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let out = satex::lint::lint(&analysis)
        .into_iter()
        .filter(|record| record["code"] == CODE)
        .map(|record| record["message"].as_str().unwrap_or_default().split(' ').next().unwrap_or_default().to_string())
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

#[test]
fn a_number_and_a_unit_set_as_one_word_are_reported() {
    if !installed() {
        return;
    }
    assert_eq!(findings("glued", "A delay of 2ms."), ["2ms"]);
    assert_eq!(findings("decimal", "It weighs 1.5kg."), ["1.5kg"]);
    assert_eq!(findings("prefix", "Up to 50Hz."), ["50Hz"]);
}

#[test]
fn a_thin_space_before_the_unit_is_still_hand_made() {
    if !installed() {
        return;
    }
    assert_eq!(findings("thin", "A delay of \\(2\\,\\mathrm{ms}\\)."), ["2ms"]);
    assert_eq!(findings("tie", "A delay of 2~ms."), ["2ms"]);
}

#[test]
fn a_macro_that_holds_the_unit_is_reported_where_it_is_used() {
    if !installed() {
        return;
    }
    let found = findings_with("helper", "\\newcommand{\\ms}{\\,\\mathrm{ms}}\n", "A delay of \\(3\\ms\\).");
    assert_eq!(found, ["3ms"]);
}

#[test]
fn a_percentage_is_a_quantity_too() {
    if !installed() {
        return;
    }
    assert_eq!(findings("percent", "Growth of 5\\% per year."), ["5%"]);
}

#[test]
fn a_space_between_the_number_and_the_letters_is_left_alone() {
    if !installed() {
        return;
    }
    assert!(findings("space", "A delay of 2 ms.").is_empty());
}

#[test]
fn words_that_only_look_like_quantities_are_left_alone() {
    if !installed() {
        return;
    }
    // A decade, a letter before the digits, an ordinal, a letter that is no
    // unit, and a prefix with nothing under it.
    let body = "The 1980s, H2O, the 2nd of 3D printing on 5G, 100s of them.";
    assert!(findings("words", body).is_empty(), "{:?}", findings("words2", body));
}

#[test]
fn a_script_is_not_a_unit() {
    if !installed() {
        return;
    }
    assert!(findings("script", "\\(\\sum_{i=1}^{N} x_i\\) and \\(\\frac{1}{2}m\\).").is_empty());
}

#[test]
fn a_document_that_uses_siunitx_is_left_alone() {
    if !installed() {
        return;
    }
    let found = findings_with("siunitx", "\\usepackage{siunitx}\n", "A delay of \\qty{2}{\\milli\\second}.");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn siunitx_is_still_recommended_where_the_document_sets_a_unit_by_hand() {
    if !installed() {
        return;
    }
    let found = findings_with("siunitx-hand", "\\usepackage{siunitx}\n", "A delay of 2ms.");
    assert_eq!(found, ["2ms"]);
}
