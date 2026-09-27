//! What a run without a TeX distribution silently gave up.
//!
//! Most targets guard on `which::which("kpsewhich")` and return quietly
//! when there is none, so a run on a runner with no TeX looks exactly like
//! a run where every one of them passed.  This target makes the two
//! distinguishable: it reports which targets are gated that way, and with
//! `SATEX_REQUIRE_TEX` set it fails rather than let the gate pass.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// Every `tests/*.rs` that probes for `kpsewhich` before working, with how
/// many test functions it holds.  The count is an upper bound: a gated file
/// may still hold tests that need no distribution.
fn gated() -> Vec<(String, usize)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(tests_dir()) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        if !text.contains("which::which(\"kpsewhich\")") && !text.contains("Command::new(\"kpsewhich\")") {
            continue;
        }
        let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        if name == "distribution_guard" {
            continue;
        }
        found.push((name, text.matches("#[test]").count()));
    }
    found.sort();
    found
}

/// A runner that promises a distribution must have one, and it must resolve
/// the kernel: `kpsewhich` on an empty tree would let every gate pass.
#[test]
fn the_promised_distribution_is_there() {
    if std::env::var_os("SATEX_REQUIRE_TEX").is_none() {
        return;
    }
    assert!(which::which("kpsewhich").is_ok(), "SATEX_REQUIRE_TEX is set, but no kpsewhich is on PATH");
    let found = Command::new("kpsewhich").arg("latex.ltx").output().expect("kpsewhich latex.ltx");
    let path = String::from_utf8_lossy(&found.stdout);
    assert!(
        found.status.success() && !path.trim().is_empty(),
        "kpsewhich runs but resolves no latex.ltx: the tree is empty, so the gated suites would all skip"
    );
}

/// Report the gate, so the run says what it did not do.  Written to the job
/// summary when there is one, since a passing test's output is captured.
#[test]
fn report_the_distribution_gate() {
    let gated = gated();
    let upper_bound: usize = gated.iter().map(|(_, count)| count).sum();
    let present = which::which("kpsewhich").is_ok();
    let names: Vec<&str> = gated.iter().map(|(name, _)| name.as_str()).collect();

    let verdict = if present {
        format!("A TeX distribution is present: the {} gated targets ran.", gated.len())
    } else {
        format!(
            "No TeX distribution: {} gated targets returned early, up to {upper_bound} test functions.",
            gated.len()
        )
    };
    let report = format!("### Distribution gate ({})\n\n{verdict}\n\nGated targets: {}\n", os(), names.join(", "));
    eprintln!("{report}");

    if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY")
        && let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(summary)
    {
        let _ = file.write_all(report.as_bytes());
    }
}

fn os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}
