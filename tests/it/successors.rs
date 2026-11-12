//! `package-successor`: a package that a successor continues is named with
//! what moving changes, and the fix loads the successor in its place; the
//! rule stays quiet where the move would break.

use crate::support::installed;
use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};

/// The `package-successor` findings, each with its edits' replacements, on a
/// document with `preamble`; `files` are written beside it.
fn found(name: &str, preamble: &str, files: &[(&str, &str)]) -> Vec<(String, Vec<String>)> {
    let directory = crate::support::tmp_root().join("satex-successors").join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (file, body) in files {
        std::fs::write(directory.join(file), body).unwrap();
    }
    let document = format!("\\documentclass{{article}}\n{preamble}\\begin{{document}}\nx\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let out = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|r| r["code"] == "package-successor")
        .map(|r| {
            let edits = r["edits"].as_array().into_iter().flatten();
            let replacements = edits.map(|e| e["replacement"].as_str().unwrap_or_default().to_string()).collect();
            (r["message"].as_str().unwrap_or_default().to_string(), replacements)
        })
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
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
