//! The table rules: numbers that do not line up, mixed precision, a unit in
//! every cell, and rules booktabs sets better; tables that are fine stay
//! quiet.

use crate::support::installed;
use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};

/// `code: message` and the group of every table finding on a document with `body`.
fn found(name: &str, body: &str) -> Vec<(String, Option<String>)> {
    found_with(name, "", body)
}

/// [`found`] with `preamble` before `\begin{document}`.
fn found_with(name: &str, preamble: &str, body: &str) -> Vec<(String, Option<String>)> {
    found_by(name, preamble, body, crate::support::config())
}

/// [`found_with`] under `cfg`.
fn found_by(name: &str, preamble: &str, body: &str, cfg: Config) -> Vec<(String, Option<String>)> {
    let directory = crate::support::tmp_root().join("satex-tables").join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let document = format!("\\documentclass{{article}}\n{preamble}\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..cfg };
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let codes = ["numbers-not-aligned", "inconsistent-precision", "unit-in-cells", "table-rules"];
    let out = satex::linter::lint(&analysis)
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

/// An `S` column rounds when siunitx's `round-mode` says so, globally or in
/// the column's own options: only `none` prints numbers as written.  Pins a
/// false "prints its numbers as written" on a thesis whose `\sisetup` rounds.
#[test]
fn an_s_column_that_rounds_is_quiet() {
    if !installed() {
        return;
    }
    let rows = "a & 0.057291666 & 1 \\\\\nb & 0.5 & 0.32791606 \\\\\nc & 1 & 0.6 \\\\\n";
    let table = |spec: &str| format!("\\begin{{tabular}}{{{spec}}}\nName & A & B \\\\\n{rows}\\end{{tabular}}");
    let precision =
        |f: &Vec<(String, Option<String>)>| f.iter().filter(|(m, _)| m.starts_with("inconsistent-precision")).count();
    let siunitx = "\\usepackage{siunitx}\n";
    assert_eq!(precision(&found_with("s-none", siunitx, &table("lSS"))), 2);
    let global = format!("{siunitx}\\sisetup{{round-mode=places,round-precision=2}}\n");
    assert_eq!(precision(&found_with("s-global", &global, &table("lSS"))), 0);
    assert_eq!(precision(&found_with("s-column", siunitx, &table("lS[round-mode=places]S"))), 1);
}

/// siunitx's knowledge lives in its augmentation: with `plugins.augment`
/// empty, `S` is a type the rule does not know and may round, so it stays
/// quiet, while an `l` column still mixes precisions.  Pins the switch.
#[test]
fn an_s_column_is_not_judged_without_the_siunitx_augmentation() {
    if !installed() {
        return;
    }
    let body = "\\begin{tabular}{lSl}\nName & A & B \\\\\na & 0.05 & 0.5 \\\\\nb & 0.5 & 0.125 \\\\\nc & 1 & 1 \\\\\n\\end{tabular}";
    let mut cfg = crate::support::config();
    let precision = |f: Vec<(String, Option<String>)>| -> Vec<String> {
        f.into_iter().filter(|(m, _)| m.starts_with("inconsistent-precision")).map(|(m, _)| m).collect()
    };
    let on = precision(found_by("aug-on", "\\usepackage{siunitx}\n", body, cfg.clone()));
    assert_eq!(on.len(), 2, "{on:?}");
    cfg.plugins.augment.clear();
    let off = precision(found_by("aug-off", "\\usepackage{siunitx}\n", body, cfg));
    assert_eq!(off.len(), 1, "{off:?}");
    assert!(off[0].contains("column 3"), "{off:?}");
}
