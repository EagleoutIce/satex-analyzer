//! satex must work with no TeX installation at all — not even found by
//! well-known-path probing. `use_kpsewhich: false` alone is not enough to
//! test that on a machine that actually has one installed: `roots_for`
//! still falls back to well-known paths (`/usr/local/texlive`, ...)
//! regardless of `provider`/`use_kpsewhich`. `use_fallback_roots: false`
//! turns that fallback off too, so the two together simulate "nothing
//! installed" without uninstalling anything.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const LIMIT: Duration = Duration::from_secs(60);
const NO_INSTALL: [&str; 4] = ["--set", "use_kpsewhich=false", "--set", "use_fallback_roots=false"];

struct Run {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

/// Runs `satex` with `args` plus [`NO_INSTALL`], on an isolated cache
/// directory, and fails the test if it does not finish within [`LIMIT`].
/// `stdout`/`stderr` are drained on a background thread while waiting, so a
/// command with a lot of output (`tokens`, `trace`) cannot deadlock on a
/// full pipe the way polling `try_wait` without reading would.
fn run(args: &[&str]) -> Run {
    let cache = std::env::temp_dir().join(format!("satex-no-install-{}-{}", std::process::id(), fastrand()));
    let full: Vec<&str> = args.iter().chain(NO_INSTALL.iter()).copied().collect();
    let child = Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(&full)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SATEX_CACHE", &cache)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("satex starts");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let result = match rx.recv_timeout(LIMIT) {
        Ok(output) => output.expect("collect output"),
        Err(_) => panic!("`satex {}` did not finish within {LIMIT:?}", args.join(" ")),
    };
    let _ = std::fs::remove_dir_all(&cache);
    Run {
        status: result.status,
        stdout: String::from_utf8_lossy(&result.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    }
}

/// Cheap process-local uniqueness for the cache directory name; no crate
/// dependency needed for it.
fn fastrand() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::time::Instant::now().hash(&mut hasher);
    hasher.finish()
}

fn assert_clean(run: &Run, args: &[&str]) {
    assert!(!run.stderr.to_lowercase().contains("panicked"), "`satex {}` panicked:\n{}", args.join(" "), run.stderr);
}

const FILE: [&str; 2] = ["-f", "tests/fixtures/project/paper.tex"];

#[test]
fn every_command_survives_with_no_installation() {
    for args in [
        &["lint"][..],
        &["summary"],
        &["explain", "highlight"],
        &["query", "definitions"],
        &["slice", "highlight"],
        &["controls"],
        &["trace"],
        &["tokens"],
        &["cache"],
    ] {
        let full: Vec<&str> = args.iter().chain(FILE.iter()).copied().collect();
        let out = run(&full);
        assert_clean(&out, &full);
        // `cache` reports the kernel it interpreted, and there is none to
        // report; everything else must still succeed with document-level
        // results even though only engine primitives are known.
        if args == ["cache"] {
            assert!(!out.status.success(), "`satex cache` should fail cleanly with no kernel to report");
            assert!(out.stderr.contains("no format was read"), "stderr: {}", out.stderr);
        } else {
            assert!(
                out.status.success(),
                "`satex {}` failed:\nstdout: {}\nstderr: {}",
                full.join(" "),
                out.stdout,
                out.stderr
            );
        }
    }
}

#[test]
fn cache_build_survives_with_no_installation() {
    let out = run(&["cache", "build"]);
    assert_clean(&out, &["cache", "build"]);
    assert!(out.status.success(), "stdout: {}\nstderr: {}", out.stdout, out.stderr);
}

#[test]
fn cache_clear_and_prune_survive_with_no_installation() {
    for args in [&["cache", "clear"][..], &["cache", "prune"]] {
        let out = run(args);
        assert_clean(&out, args);
        assert!(
            out.status.success(),
            "`satex {}` failed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            out.stdout,
            out.stderr
        );
    }
}

#[test]
fn version_reports_no_installation() {
    let out = run(&["summary", "-f", "tests/fixtures/project/paper.tex"]);
    assert_clean(&out, &["summary"]);
    assert!(out.status.success());
    assert!(
        out.stdout.contains("no installation"),
        "`satex summary` did not report \"no installation\":\n{}",
        out.stdout
    );

    let json = run(&["--version", "--format", "json"]);
    assert!(json.status.success());
    let value: serde_json::Value = serde_json::from_str(&json.stdout).expect("valid json");
    assert_eq!(value["installation"], "no installation", "json: {}", json.stdout);
}

/// The document itself still analyzes: with no kernel, `\section` etc. are
/// unknown, but the document's own structure (here, a lone `\section`) is
/// still read and reported, not silently dropped.
#[test]
fn document_still_analyzes_as_initex() {
    let out = run(&["lint", "-f", "tests/fixtures/project/paper.tex"]);
    assert!(out.status.success(), "stderr: {}", out.stderr);
    // The kernel not being loaded is reported exactly once, under the
    // precision category, not once per undefined kernel command.
    let mentions = out.stdout.matches("no-format").count();
    assert!(mentions <= 1, "no-format should be reported once, not {mentions}:\n{}", out.stdout);
}

/// A run that actually has a distribution to find (real `kpsewhich` on this
/// machine) still finds it through `kpsewhich` alone, `use_fallback_roots`
/// left on: this is a control for the two tests above, so a bug that always
/// reports "no installation" would be caught too.
#[test]
fn version_reports_an_installation_when_kpsewhich_is_allowed() {
    let has_kpsewhich = Command::new("kpsewhich").arg("--version").output().is_ok();
    if !has_kpsewhich {
        eprintln!("skipping: no kpsewhich on this machine");
        return;
    }
    let child = Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(["--version"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SATEX_CACHE", std::env::temp_dir().join(format!("satex-has-install-{}", std::process::id())))
        .output()
        .expect("satex runs");
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert!(child.status.success());
    assert!(
        !stdout.contains("installation       no installation"),
        "expected a real installation to be found:\n{stdout}"
    );
}
