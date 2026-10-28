//! The table rules: numbers that do not line up, mixed precision, a unit in
//! every cell, and rules booktabs sets better; tables that are fine stay
//! quiet.

use crate::support::installed;
use satex::config::{Config, Load};
use satex::machine::Machine;

/// `code: message` and the group of every table finding on a document with `body`.
fn found(name: &str, body: &str) -> Vec<(String, Option<String>)> {
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
        .map(|r| {
            let group = r.get("group").and_then(|g| g.as_str()).map(str::to_string);
            (format!("{}: {}", r["code"].as_str().unwrap(), r["message"].as_str().unwrap()), group)
        })
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

fn findings(name: &str, body: &str) -> Vec<String> {
    found(name, body).into_iter().map(|(f, _)| f).collect()
}

#[test]
fn a_ragged_column_of_numbers_is_reported() {
    if !installed() {
        return;
    }
    let found = found(
        "ragged",
        "\\begin{tabular}{|l|c|r|}\n\\hline\nName & Time & Share \\\\\n\\hline\na & 1.5 ms & 12.5 \\\\\nb & 12.25 ms & 3.75 \\\\\nc & 3 ms & 100.0 \\\\\n\\hline\n\\end{tabular}",
    );
    let messages: Vec<&str> = found.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(
        messages,
        [
            "numbers-not-aligned: numeric column 2 is set `c`: align its decimal separators with siunitx `S`, or at least use `r`",
            "numbers-not-aligned: numeric column 3 is right-aligned, not on the decimal separator: use siunitx `S`",
            "inconsistent-precision: numeric column 2 mixes 0 to 2 decimal places: give every number one precision",
            "inconsistent-precision: numeric column 3 mixes 1 to 2 decimal places: give every number one precision",
            "unit-in-cells: every number in column 2 repeats the unit `ms`: name it once in the header",
            "table-rules: vertical rules and `\\hline` instead of booktabs rules: drop the `|` and use `\\toprule`, `\\midrule`, `\\bottomrule`",
        ]
    );
    // One table: its findings share one group, at its `\begin`.
    assert!(found.iter().all(|(_, g)| g.as_deref() == Some("table@3:1")), "{found:?}");
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

/// Two numbers already stand apart: a centred column of 0.92 and 1.5.
#[test]
fn two_numbers_make_a_column() {
    if !installed() {
        return;
    }
    let found = findings(
        "two",
        "\\begin{tabular}{lc}\nMethod & Score \\\\ \\hline\nOurs & 0.92 \\\\\nLol & 1.5 \\\\\n\\end{tabular}",
    );
    for code in ["numbers-not-aligned", "inconsistent-precision", "table-rules"] {
        assert!(found.iter().any(|f| f.starts_with(code)), "{code}: {found:?}");
    }
}

/// Rules matter from `lints.table-rules.rows` rows on: two rows are left alone.
#[test]
fn a_short_table_keeps_its_rules() {
    if !installed() {
        return;
    }
    let found = findings("short", "\\begin{tabular}{|l|l|}\n\\hline\na & b \\\\\nc & d \\\\\n\\hline\n\\end{tabular}");
    assert!(found.is_empty(), "{found:?}");
}
