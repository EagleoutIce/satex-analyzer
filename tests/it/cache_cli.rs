//! `satex cache` and its subcommands as a user runs them in a project folder.

use std::process::Stdio;

/// A project folder with one root document and the suite's cache.
fn project(tag: &str) -> std::path::PathBuf {
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join(format!("cache-cli-{tag}")));
    std::fs::write(
        dir.join("thesis.tex"),
        "\\documentclass{article}\n\\begin{document}\nThesis text.\n\\end{document}\n",
    )
    .unwrap();
    dir
}

/// `satex cache` and `satex cache status` report the kernel and the caches;
/// a project's own document is never read for that. Bug pinned: in a thesis
/// folder `satex cache` analyzed the whole thesis first, and `cache status`
/// failed with "no format was read".
#[test]
fn cache_status_never_reads_the_project() {
    if !crate::support::installed() {
        return;
    }
    let dir = project("status");
    for args in [&["cache"][..], &["cache", "status"]] {
        let out = crate::support::satex().args(args).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
        let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "{args:?}: {stderr}");
        assert!(stdout.contains("kernel"), "{args:?}: {stdout}");
        assert!(
            !stderr.contains("thesis.tex") && !stdout.contains("thesis.tex"),
            "{args:?} read the project: {stderr}"
        );
    }
}

/// `satex cache show` without a package lists every package cache, never an
/// empty "no cache file for" line.
#[test]
fn cache_show_without_a_package_lists_them_all() {
    let dir = project("show");
    let out = crate::support::satex().args(["cache", "show"]).current_dir(&dir).stdin(Stdio::null()).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(!stdout.contains("no cache file for \n") && !stdout.trim().is_empty(), "{stdout}");
}

/// The other commands still find the folder's root document: `lint` reads it.
#[test]
fn lint_still_reads_the_project() {
    if !crate::support::installed() {
        return;
    }
    let dir = project("lint");
    let out = crate::support::satex().args(["lint", "-f", "thesis.tex"]).current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("package caches:"), "{stderr}");
}
