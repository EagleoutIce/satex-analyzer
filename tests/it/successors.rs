//! Package rules. `package-successor`: a package that a successor continues
//! is named with what moving changes, and the fix loads the successor in its
//! place; the rule stays quiet where the move would break. Then the rules on
//! how loads relate: `package-load-order`, `redundant-package-load` and
//! `unknown-package-name`.

use crate::support::installed;
use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};

/// Each finding of `codes` as code, message and its edits' replacements, on
/// a document with `preamble` and `body`; `files` are written beside it.
fn lint(
    name: &str,
    codes: &[&str],
    preamble: &str,
    body: &str,
    files: &[(&str, &str)],
) -> Vec<(String, String, Vec<String>)> {
    let directory = crate::support::tmp_root().join("satex-successors").join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (file, text) in files {
        std::fs::write(directory.join(file), text).unwrap();
    }
    let document = format!("\\documentclass{{article}}\n{preamble}\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let out = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|r| codes.iter().any(|c| r["code"] == *c))
        .map(|r| {
            let edits = r.get("edits").and_then(|e| e.as_array()).into_iter().flatten();
            let replacements = edits.map(|e| e["replacement"].as_str().unwrap_or_default().to_string()).collect();
            let text = |key: &str| r[key].as_str().unwrap_or_default().to_string();
            (text("code"), text("message"), replacements)
        })
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

/// The `package-successor` findings as message and replacements.
fn found(name: &str, preamble: &str, files: &[(&str, &str)]) -> Vec<(String, Vec<String>)> {
    lint(name, &["package-successor"], preamble, "x", files).into_iter().map(|(_, m, r)| (m, r)).collect()
}

/// wrapfig in a list of packages: named, and only its name is replaced.
#[test]
fn wrapfig_is_continued_by_wrapfig2() {
    if !installed() {
        return;
    }
    let found = found("wrapfig", "\\usepackage{graphicx, wrapfig}\n", &[]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].0.starts_with("`wrapfig` is continued by `wrapfig2`: "), "{found:?}");
    assert_eq!(found[0].1, ["wrapfig2"]);
}

/// A package that loads wrapfig itself keeps it loaded, and wrapfig2 refuses
/// to load next to it: no finding. A document without wrapfig gets none either.
#[test]
fn no_successor_where_the_move_breaks() {
    if !installed() {
        return;
    }
    let loader = ("mywrap.sty", "\\ProvidesPackage{mywrap}\n\\RequirePackage{wrapfig}\n\\newcommand\\mywrap{}\n");
    let found = self::found("loaded", "\\usepackage{wrapfig}\n\\usepackage{mywrap}\n", &[loader]);
    assert!(found.is_empty(), "{found:?}");
    assert!(self::found("none", "\\usepackage{graphicx}\n", &[]).is_empty());
}

/// Each pair beyond wrapfig: named, and only its name is replaced. The old
/// packages that TeX Live no longer ships are stubbed beside the document.
#[test]
fn old_packages_are_continued_by_their_successors() {
    if !installed() {
        return;
    }
    let stub = |name: &str| (format!("{name}.sty"), format!("\\ProvidesPackage{{{name}}}\n"));
    for (old, new) in
        [("here", "float"), ("fancyheadings", "fancyhdr"), ("caption2", "caption"), ("scrpage2", "scrlayer-scrpage")]
    {
        let (file, body) = stub(old);
        let found = found(&format!("pair-{old}"), &format!("\\usepackage{{{old}}}\n"), &[(&file, &body)]);
        assert_eq!(found.len(), 1, "{old}: {found:?}");
        assert!(found[0].0.starts_with(&format!("`{old}` is continued by `{new}`: ")), "{found:?}");
        assert_eq!(found[0].1, [new], "{old}");
    }
}

/// A package file `name.sty` with `body`.
fn sty(name: &str, body: &str) -> (String, String) {
    (format!("{name}.sty"), format!("\\ProvidesPackage{{{name}}}\n{body}\n"))
}

/// The `package-load-order` findings with `pkga` and `pkgb` as given, loaded
/// in that order, and a body that uses `\greet`.
fn load_order(name: &str, a: &str, b: &str, body: &str) -> Vec<(String, String, Vec<String>)> {
    let (a, b) = (sty("pkga", a), sty("pkgb", b));
    let files = [(a.0.as_str(), a.1.as_str()), (b.0.as_str(), b.1.as_str())];
    lint(name, &["package-load-order"], "\\usepackage{pkga}\n\\usepackage{pkgb}\n", body, &files)
}

/// pkgb's `\def` silently replaces the `\greet` pkga made, and the document
/// uses it. pkga's `\def` does not check, so loading pkgb first keeps pkga's
/// version: the fix moves pkgb's line above pkga's.
#[test]
fn a_later_package_overriding_a_used_command_is_reported() {
    if !installed() {
        return;
    }
    let found = load_order("order", "\\def\\greet{A}", "\\def\\greet{B}", "\\greet");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0].1,
        "`pkgb` redefines `\\greet`, which `pkga` made: loading `pkgb` before `pkga` keeps `pkga`'s version"
    );
    assert_eq!(found[0].2, ["\\usepackage{pkgb}\n", ""], "{found:?}");
}

/// pkga's `\newcommand` would refuse a `\greet` pkgb made first: the finding
/// stays, without the promise and the move.
#[test]
fn a_checked_definition_gets_no_move() {
    if !installed() {
        return;
    }
    let found = load_order("checked", "\\newcommand\\greet{A}", "\\def\\greet{B}", "\\greet");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].1, "`pkgb` redefines `\\greet`, which `pkga` made");
    assert!(found[0].2.is_empty(), "{found:?}");
}

/// No finding where pkgb adapts pkga on purpose (it asks
/// `\@ifpackageloaded{pkga}`), and none for a name the document never uses.
#[test]
fn intended_patches_and_unused_names_stay_quiet() {
    if !installed() {
        return;
    }
    let adapts = "\\@ifpackageloaded{pkga}{\\def\\greet{B}}{}";
    assert!(load_order("adapts", "\\def\\greet{A}", adapts, "\\greet").is_empty());
    assert!(load_order("unused", "\\def\\greet{A}", "\\def\\greet{B}", "x").is_empty());
}

/// graphicx loads graphics, so the document's own `\\usepackage{graphics}`
/// after it does nothing: reported once, as redundant, not also as a
/// duplicate. The document's own load twice and a load with a new option are
/// not redundant.
#[test]
fn a_load_a_package_made_already_is_redundant() {
    if !installed() {
        return;
    }
    let codes = ["redundant-package-load", "duplicate-package"];
    let kinds = |found: &[(String, String, Vec<String>)]| found.iter().map(|f| f.0.clone()).collect::<Vec<_>>();
    let found = lint("redundant", &codes, "\\usepackage{graphicx}\n\\usepackage{graphics}\n", "x", &[]);
    assert_eq!(kinds(&found), ["redundant-package-load"], "{found:?}");
    assert!(found[0].1.starts_with("`graphicx` loads `graphics` already, by the request at main.tex:2:"), "{found:?}");
    assert_eq!(found[0].2, [""], "{found:?}");
    let twice = lint("twice", &codes, "\\usepackage{graphicx}\n\\usepackage{graphicx}\n", "x", &[]);
    assert_eq!(kinds(&twice), ["duplicate-package"], "{twice:?}");
    let draft = lint("draft", &codes, "\\usepackage{graphicx}\n\\usepackage[draft]{graphics}\n", "x", &[]);
    assert!(draft.is_empty(), "{draft:?}");
}

/// `graphix` is one letter from the installed `graphicx`: reported as a typo
/// with the name replaced, and not again as an unresolved file. A name far
/// from every package stays `unresolved-file`.
#[test]
fn a_misspelled_package_names_the_installed_one() {
    if !installed() {
        return;
    }
    let codes = ["unknown-package-name", "unresolved-file"];
    let found = lint("typo", &codes, "\\usepackage{amsmath,graphix}\n", "x", &[]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "unknown-package-name");
    assert_eq!(found[0].1, "`graphix` is not installed, but `graphicx` is");
    assert_eq!(found[0].2, ["graphicx"]);
    let far = lint("far", &codes, "\\usepackage{qzqzqzqzqzq}\n", "x", &[]);
    assert_eq!(far.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["unresolved-file"], "{far:?}");
}
