//! `scope --at`, `explain --at` and `slice --at` take `[FILE:]LINE[:COL]`
//! the way `trace --from` does: a subfile by a trailing part of its path,
//! with or without `.tex`.

fn run(dir: &std::path::Path, args: &[&str]) -> Result<Vec<serde_json::Value>, String> {
    let out = crate::support::satex()
        .args(["--no-config", "--no-classes", "--no-packages", "--set", "limits.seconds=0"])
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
    let dir = crate::support::tmp_root().join(format!("satex-positions-{tag}-{}", std::process::id()));
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

/// A completion list needs a name's tag, shape and package only, so a `lean` scope answers
/// with just those: the same names in the same order as the full scope, each its full row's
/// lean part. Pins the slow `scope` request: every row built its usage and documentation
/// (5 s and 5 MB on the thesis example) when a completion read two fields of each.
#[test]
fn a_lean_scope_is_the_full_scope_cut_to_what_completion_shows() {
    use std::io::Write;
    let dir = document("lean");
    let ask = |lean: bool| {
        let mut child = crate::support::satex()
            .args(["--no-config", "--no-classes", "--no-packages", "--set", "limits.seconds=0"])
            .args(["query", "--request", "-"])
            .current_dir(&dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let body =
            serde_json::json!({ "file": "main.tex", "queries": [{ "type": "scope", "all": true, "lean": lean }] });
        child.stdin.take().unwrap().write_all(body.to_string().as_bytes()).unwrap();
        let out: serde_json::Value = serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
        out[0]["records"].as_array().cloned().unwrap_or_default()
    };
    let (lean, full) = (ask(true), ask(false));
    assert!(names(&lean).contains(&"\\a") && names(&lean).contains(&"\\relax"), "{:?}", names(&lean));
    assert_eq!(names(&lean), names(&full));
    let keys = ["name", "tag", "effective", "package", "args", "math"];
    for (l, f) in lean.iter().zip(&full) {
        let cut: serde_json::Map<_, _> =
            keys.iter().filter_map(|k| Some((k.to_string(), f.get(*k)?.clone()))).collect();
        assert_eq!(l, &serde_json::Value::Object(cut), "{f}");
    }
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
