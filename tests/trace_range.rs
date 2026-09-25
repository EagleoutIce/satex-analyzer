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

/// `trace --interactive`, fed `input` on stdin: `(line, name)` of every
/// event header printed.
fn stepped(dir: &std::path::Path, input: &str) -> Vec<(u64, String)> {
    stepped_with(dir, &[], input)
}

fn stepped_with(dir: &std::path::Path, args: &[&str], input: &str) -> Vec<(u64, String)> {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(["--no-config", "--no-classes", "--no-packages", "trace", "-i", "-f"])
        .arg(dir.join("main.tex"))
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap();
    out.lines()
        .filter(|l| l.starts_with('['))
        .filter_map(|l| {
            let mut words = l.split_whitespace().skip(2);
            let name = words.next()?.to_string();
            let line = words.next()?.rsplit(':').nth(1)?.parse().ok()?;
            Some((line, name))
        })
        .collect()
}

#[test]
fn interactive_l_goes_to_the_next_main_file_line() {
    let dir = document("line");
    let all = stepped(&dir, "");
    assert!(all.iter().any(|(l, n)| *l == 3 && n == "\\b"), "{all:?}");
    let steps = stepped(&dir, "\nl\nl\nq\n");
    let main_lines: Vec<u64> = steps.iter().map(|(l, _)| *l).collect();
    // Enter reaches the second event, each `l` then leaves the line it is on.
    assert_eq!(steps.len(), 4, "{steps:?}");
    assert!(main_lines[2] != main_lines[1] && main_lines[3] != main_lines[2], "{steps:?}");
}

#[test]
fn interactive_s_skips_to_a_line() {
    let dir = document("skip");
    let steps = stepped(&dir, "s 3\nq\n");
    assert_eq!(steps.first().map(|(l, _)| *l), Some(1), "{steps:?}");
    assert_eq!(steps.get(1), Some(&(3, "\\b".to_string())), "{steps:?}");
    // A line past the end runs the trace out.
    assert_eq!(stepped(&dir, "s 99\nq\n").len(), 1);
}

#[test]
fn interactive_o_steps_over_a_call() {
    let dir = std::env::temp_dir().join(format!("satex-trace-range-over-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), "\\def\\a{\\relax}\n\\def\\b{\\a\\relax}\n\\b\n\\a\n").unwrap();
    let into = stepped(&dir, "s 3\n\nq\n");
    assert_eq!(into.get(2), Some(&(2, "\\a".to_string())), "{into:?}");
    // Over `\b` skips `\a` and `\relax` from its body and lands on line 4.
    let over = stepped(&dir, "s 3\no\nq\n");
    assert_eq!(over.get(1), Some(&(3, "\\b".to_string())), "{over:?}");
    assert_eq!(over.get(2), Some(&(4, "\\a".to_string())), "{over:?}");
}

#[test]
fn ranges_can_name_a_file() {
    let dir = document("file");
    // `child.tex` holds line 1 only; `\x` on line 5 of main.tex reads it.
    let rows = |args: &[&str]| events(&dir, args);
    let in_child = rows(&["--lines", "child.tex:1:1"]);
    assert!(in_child.iter().any(|(n, f, l)| n == "\\def" && f == "child.tex" && *l == 1), "{in_child:?}");
    assert!(in_child.iter().all(|(_, f, _)| f == "child.tex"), "{in_child:?}");
    // Without the extension, and as positions.
    assert_eq!(rows(&["--lines", "child:1:1"]), in_child);
    let from = rows(&["--from", "child.tex:1", "--to", "child.tex:1"]);
    assert_eq!(from, in_child);
    // Two files in one range are an error.
    let output = trace(&dir, &["--from", "child.tex:1", "--to", "main.tex:3"]);
    assert!(!output.status.success());
}

#[test]
fn interactive_l_and_s_follow_the_named_file() {
    let dir = document("ifile");
    let steps = stepped_with(&dir, &["--from", "child.tex:1"], "l\nq\n");
    assert!(!steps.is_empty(), "{steps:?}");
}

#[test]
fn trace_shows_only_the_documents_own_files_unless_asked() {
    let dir = document("internal");
    let files = |extra: &[&str]| -> std::collections::BTreeSet<String> {
        events(&dir, extra).into_iter().map(|(_, f, _)| f).collect()
    };
    let own = files(&[]);
    assert!(own.iter().all(|f| f == "main.tex" || f == "child.tex"), "{own:?}");
    assert!(files(&["--include-internal"]).contains("latex.ltx"));
}

#[test]
fn interactive_reports_skipped_internal_steps_and_the_output_of_a_range() {
    let dir = std::env::temp_dir().join(format!("satex-trace-range-out-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), "\\documentclass{article}\n\\def\\a{world}\nhello \\a\n").unwrap();
    let text = |args: &[&str]| -> String {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_satex"))
            .args(["--no-config", "--no-classes", "--no-packages", "trace", "-i", "-f"])
            .arg(dir.join("main.tex"))
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"c\n").unwrap();
        String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
    };
    let hidden = text(&[]);
    assert!(hidden.contains("internal steps skipped (use --include-internal)"), "{hidden}");
    assert!(hidden.contains("-- end of the trace"), "{hidden}");
    // No range, no output line; a range gets the text its lines typeset.
    assert!(!hidden.contains("output:"), "{hidden}");
    let ranged = text(&["--from", "3"]);
    assert!(ranged.contains("output:\nhello world"), "{ranged}");
    assert!(!text(&["--include-internal"]).contains("internal steps skipped"));
}
