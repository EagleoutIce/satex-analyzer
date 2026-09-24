//! `satex trace --lines FROM:TO` / `--steps FROM:TO`: only the events in
//! range are recorded, a line range keeps what the calls on its lines do,
//! and a malformed range is an error.

use std::process::{Command, Output};

fn trace(dir: &std::path::Path, range: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(["--no-config", "--no-classes", "--no-packages", "trace", "-f"])
        .arg(dir.join("main.tex"))
        .args(range)
        .args(["--format", "json"])
        .output()
        .unwrap()
}

/// `(name, file, line)` of every event.
fn events(dir: &std::path::Path, range: &[&str]) -> Vec<(String, String, u64)> {
    let output = trace(dir, range);
    assert!(output.status.success(), "{range:?}: {}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    rows.iter()
        .map(|r| (r["name"].as_str().unwrap().into(), r["file"].as_str().unwrap().into(), r["line"].as_u64().unwrap()))
        .collect()
}

fn document(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-trace-range-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("child.tex"), "\\def\\x{\\relax}\n").unwrap();
    std::fs::write(dir.join("main.tex"), "\\def\\a{\\relax}\n\\def\\b{\\a}\n\\b\n\\a\n\\input child \\x\n").unwrap();
    dir
}

fn has(events: &[(String, String, u64)], name: &str, file: &str, line: u64) -> bool {
    events.iter().any(|(n, f, l)| n == name && f == file && *l == line)
}

#[test]
fn a_line_range_keeps_its_lines_and_what_their_calls_do() {
    let dir = document("lines");
    let line3 = events(&dir, &["--lines", "3:3"]);
    // `\b` on line 3, then `\a` and `\relax` from the bodies on lines 1–2.
    assert!(has(&line3, "\\b", "main.tex", 3));
    assert!(has(&line3, "\\a", "main.tex", 2));
    assert!(has(&line3, "\\relax", "main.tex", 1));
    assert!(!line3.iter().any(|(n, _, l)| n == "\\def" || *l == 4), "{line3:?}");
    // The bodies on lines 1–2 run from lines 3 and 4: not part of `:2`.
    let head = events(&dir, &["--lines", ":2"]);
    assert_eq!(head.iter().filter(|(n, ..)| n == "\\def").count(), 2);
    assert!(!head.iter().any(|(n, ..)| n == "\\relax"), "{head:?}");
    // A file read on line 5 is part of that line.
    let line5 = events(&dir, &["--lines", "5:"]);
    assert!(has(&line5, "\\def", "child.tex", 1));
    assert!(has(&line5, "\\relax", "child.tex", 1));
    assert!(!line5.iter().any(|(_, f, l)| f == "main.tex" && (1..5).contains(l)), "{line5:?}");
}

#[test]
fn a_step_range_keeps_only_those_steps() {
    let dir = document("steps");
    let all = events(&dir, &[]);
    let some = events(&dir, &["--steps", "2:"]);
    let first = events(&dir, &["--steps", "0:1"]);
    assert!(!first.is_empty() && !some.is_empty());
    assert!(first.len() + some.len() <= all.len());
    assert_eq!(&all[..first.len()], &first[..]);
    assert_eq!(&all[all.len() - some.len()..], &some[..]);
}

#[test]
fn a_malformed_range_is_an_error() {
    let dir = document("bad");
    for range in [["--lines", "a:b"], ["--steps", "1:x"], ["--lines", "5:3"]] {
        let output = trace(&dir, &range);
        assert!(!output.status.success(), "{range:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("range"), "{range:?}");
    }
}
