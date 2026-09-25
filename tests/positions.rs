//! `scope --at`, `explain --at` and `slice --at` take `[FILE:]LINE[:COL]`
//! the way `trace --from` does: a subfile by a trailing part of its path,
//! with or without `.tex`.

use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> Result<Vec<serde_json::Value>, String> {
    let out = Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(["--no-config", "--no-classes", "--no-packages"])
        .args(args)
        .arg("-f")
        .arg(dir.join("main.tex"))
        .args(["--format", "json"])
        .output()
        .unwrap();
    match out.status.success() {
        true => Ok(serde_json::from_slice(&out.stdout).unwrap_or_default()),
        false => Err(String::from_utf8_lossy(&out.stderr).into()),
    }
}

fn document(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-positions-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("child.tex"), "\\def\\c{1}\n\\a\n").unwrap();
    std::fs::write(dir.join("main.tex"), "\\def\\a{1}\n\\input child\n\\def\\late{2}\n").unwrap();
    dir
}

fn names(rows: &[serde_json::Value]) -> Vec<&str> {
    rows.iter().filter_map(|r| r["name"].as_str()).collect()
}

#[test]
fn scope_at_a_position_in_a_subfile() {
    let dir = document("scope");
    for at in ["child.tex:2:1", "child:2:1"] {
        let rows = run(&dir, &["scope", "--at", at]).unwrap();
        let seen = names(&rows);
        assert!(seen.contains(&"\\a") && seen.contains(&"\\c"), "{at}: {seen:?}");
        assert!(!seen.contains(&"\\late"), "{at}: {seen:?}");
    }
    // Before `\c` is defined, in the same file.
    let rows = run(&dir, &["scope", "--at", "child.tex:1:1"]).unwrap();
    assert!(!names(&rows).contains(&"\\c"));
    // The main file still sees the child's definitions after the `\input`.
    let rows = run(&dir, &["scope", "--at", "3:1"]).unwrap();
    assert!(names(&rows).contains(&"\\c"));
    assert!(run(&dir, &["scope", "--at", "nowhere.tex:1:1"]).is_err());
}

#[test]
fn explain_and_slice_take_a_file_without_its_extension() {
    let dir = document("explain");
    for at in ["child.tex:2", "child:2"] {
        let rows = run(&dir, &["explain", "\\a", "--at", at]).unwrap();
        assert!(!rows.is_empty(), "{at}");
    }
    let rows = run(&dir, &["slice", "--at", "child:2:1", "--list"]);
    assert!(rows.is_ok(), "{rows:?}");
}
