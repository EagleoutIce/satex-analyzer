//! Several satex processes sharing one cache directory report what one
//! process alone reports. The cache is written by whoever gets there
//! first, and the answer never depends on who that was.

use std::path::{Path, PathBuf};
use std::process::Command;

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

/// A project with its own empty cache directory.
fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-cache-concurrency-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cache")).expect("temporary directory");
    dir
}

/// `satex query diagnostics` on `source`, as JSON, against `dir`'s cache.
fn run(dir: &Path, source: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(["query", "diagnostics", "-f", source, "--format", "json"])
        .current_dir(dir)
        .env("SATEX_CACHE", dir.join("cache"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("satex runs")
}

fn wait(child: std::process::Child) -> String {
    let out = child.wait_with_output().expect("satex finishes");
    assert!(out.status.success(), "satex failed: {:?}", out.status);
    String::from_utf8(out.stdout).expect("json is utf-8")
}

/// Every document, written into `dir`, so the processes analyse different
/// preambles over one cache: a package read after two different preambles
/// is what puts two trees in one cache file.
fn documents(dir: &Path) -> Vec<&'static str> {
    let files: [(&str, &str); 3] = [
        ("plain.tex", "\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n"),
        (
            "colour.tex",
            "\\documentclass{article}\n\\usepackage{xcolor}\n\\begin{document}\n\
             \\definecolor{mine}{rgb}{1,0,0}\\textcolor{mine}{x}\n\\end{document}\n",
        ),
        (
            "tools.tex",
            "\\documentclass{article}\n\\newcommand\\before{}\n\\usepackage{xcolor}\n\
             \\usepackage{etoolbox}\n\\begin{document}\\relax\\end{document}\n",
        ),
    ];
    for (name, text) in files {
        std::fs::write(dir.join(name), text).expect("document");
    }
    files.iter().map(|(name, _)| *name).collect()
}

/// The cache directory's entries, for a corruption check afterwards.
fn caches(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("cache"))
        .expect("cache directory")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn processes_racing_on_one_cache_agree_with_a_serial_run() {
    if !installed() {
        return;
    }
    // What one process alone reports, with nothing else touching the cache.
    let alone = project("serial");
    let names = documents(&alone);
    let serial: Vec<String> = names.iter().map(|name| wait(run(&alone, name))).collect();

    // The same documents again, three processes at once on a fresh cache:
    // every one of them builds the kernel and the packages, and they race
    // to store what they built.
    let shared = project("racing");
    documents(&shared);
    let racing: Vec<std::process::Child> = names.iter().map(|name| run(&shared, name)).collect();
    let raced: Vec<String> = racing.into_iter().map(wait).collect();
    for (i, name) in names.iter().enumerate() {
        assert_eq!(raced[i], serial[i], "{name} read differently while three processes shared a cache");
    }

    // The cache they left behind is readable, and a run against it still
    // reports the same: a torn or mis-grafted entry would show here.
    for (i, name) in names.iter().enumerate() {
        assert_eq!(wait(run(&shared, name)), serial[i], "{name} after the race");
    }
    let left = caches(&shared);
    assert!(
        left.iter().any(|n| n.starts_with("format-") && n.ends_with(".postcard")),
        "a kernel cache was stored: {left:?}"
    );
    assert!(
        left.iter().any(|n| n.starts_with("package-") && n.ends_with(".postcard")),
        "a package cache was stored: {left:?}"
    );
    // No temporary is left: every write either renamed into place or died
    // with its process.
    assert_eq!(left.iter().filter(|n| n.ends_with(".tmp")).count(), 0, "{left:?}");
}

#[test]
fn one_document_in_many_processes_at_once_reports_one_answer() {
    if !installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\usepackage{xcolor}\n\\begin{document}\n\
                  \\definecolor{mine}{rgb}{1,0,0}\\textcolor{mine}{x}\n\\end{document}\n";
    // What the document reports with a cache of its own and nobody else in
    // it, which every racing process has to match exactly.
    let alone = project("same-alone");
    std::fs::write(alone.join("doc.tex"), source).expect("doc.tex");
    let expected = wait(run(&alone, "doc.tex"));

    let dir = project("same");
    std::fs::write(dir.join("doc.tex"), source).expect("doc.tex");
    // Four processes, one fresh cache, the same document: all four build
    // it, one store wins, and none of them may disagree.
    let racing: Vec<std::process::Child> = (0..4).map(|_| run(&dir, "doc.tex")).collect();
    let answers: Vec<String> = racing.into_iter().map(wait).collect();
    for (i, answer) in answers.iter().enumerate() {
        assert_eq!(*answer, expected, "process {i} of four disagreed with a run of its own");
    }
    // And the cache they left says the same thing again.
    assert_eq!(wait(run(&dir, "doc.tex")), expected, "the cache they left");
    // One package cache file, not one per process.
    let left = caches(&dir);
    let packages = left.iter().filter(|n| n.starts_with("package-") && n.ends_with(".postcard")).count();
    assert!(packages > 0, "the race stored the packages it read: {left:?}");
    assert_eq!(left.iter().filter(|n| n.ends_with(".tmp")).count(), 0, "{left:?}");
}
