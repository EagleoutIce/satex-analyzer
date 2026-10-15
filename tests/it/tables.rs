//! The table rules: numbers that do not line up, mixed precision, a unit in
//! every cell, and rules booktabs sets better; tables that are fine stay
//! quiet.

use crate::support::installed;
use satex::config::{Config, Load};
use satex::machine::Machine;

/// `code: message` of every table finding on a document with `body`.
fn findings(name: &str, body: &str) -> Vec<String> {
    let directory = crate::support::tmp_root().join("satex-tables").join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let document = format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let codes = ["numbers-not-aligned", "inconsistent-precision", "unit-in-cells", "table-rules"];
    let out = satex::lint::lint(&analysis)
        .into_iter()
        .filter(|r| codes.contains(&r["code"].as_str().unwrap_or_default()))
        .map(|r| format!("{}: {}", r["code"].as_str().unwrap(), r["message"].as_str().unwrap()))
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

#[test]
fn a_ragged_column_of_numbers_is_reported() {
    if !installed() {
        return;
    }
    let found = findings(
        "ragged",
        "\\begin{tabular}{|l|c|r|}\n\\hline\nName & Time & Share \\\\\n\\hline\na & 1.5 ms & 12.5 \\\\\nb & 12.25 ms & 3.75 \\\\\nc & 3 ms & 100.0 \\\\\n\\hline\n\\end{tabular}",
    );
    assert_eq!(
        found,
        [
            "numbers-not-aligned: the numbers of column 2 do not line up at their decimal mark",
            "numbers-not-aligned: the numbers of column 3 do not line up at their decimal mark",
            "inconsistent-precision: the numbers of column 2 give 0 to 2 decimals",
            "inconsistent-precision: the numbers of column 3 give 1 to 2 decimals",
            "unit-in-cells: every number of column 2 carries the unit `ms`",
            "table-rules: the table is ruled with vertical rules and `\\hline`",
        ]
    );
}

#[test]
fn an_aligned_table_is_quiet() {
    if !installed() {
        return;
    }
    // Integers right-aligned, decimals of one precision, a header, a span.
    let found = findings(
        "aligned",
        "\\begin{tabular}{lrr}\nName & Count & Mean \\\\\na & 5 & 1.25 \\\\\nb & 120 & 12.50 \\\\\n\\multicolumn{3}{c}{all} \\\\\nc & 13 & 3.75 \\\\\n\\end{tabular}",
    );
    assert!(found.is_empty(), "{found:?}");
}
