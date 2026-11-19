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

/// `satex lint` on a file whose line 1 is `Grüße \foo`, with `args` after it.
fn lint_after_umlaut(tag: &str, args: &[&str]) -> String {
    let dir = crate::support::tmp_root().join(format!("satex-positions-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), "Grüße \\foo\n").unwrap();
    let out = crate::support::satex()
        .args(["--no-config", "--no-classes", "--no-packages", "--set", "limits.seconds=0", "--color", "never"])
        .args(["lint", "-f"])
        .arg(dir.join("main.tex"))
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The terminal view marks a finding's source under its column.  Under
/// pdfTeX a column counts bytes, so after `ü` the column is one past the
/// character it names; the renderer counted characters with it and put the
/// carets under `oo` instead of under `\foo`.
#[test]
fn the_text_view_marks_a_command_after_a_non_ascii_letter() {
    let text = lint_after_umlaut("caret", &[]);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines.iter().position(|l| l.ends_with("Grüße \\foo")).unwrap_or_else(|| panic!("{text}"));
    let (shown, marks) = (lines[at], lines[at + 1]);
    let column = |l: &str, s: &str| l.find(s).map(|b| l[..b].chars().count());
    assert_eq!(column(marks, "^^^^"), column(shown, "\\foo"), "{text}");
}

/// GitHub annotations and SARIF (`columnKind: unicodeCodePoints`) count a
/// column in characters, as editors do.
/// Bug pinned: they carried pdfTeX's byte column, two past `\foo`.
#[test]
fn github_and_sarif_count_columns_in_characters() {
    let column = "Grüße ".chars().count() + 1;
    let github = lint_after_umlaut("github", &["--format", "github"]);
    assert!(github.contains(&format!("line=1,col={column},")), "{github}");
    let sarif: serde_json::Value = serde_json::from_str(&lint_after_umlaut("sarif", &["--format", "sarif"])).unwrap();
    let region = &sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"];
    assert_eq!(region["startColumn"], column, "{sarif}");
}

/// A query's name and the flags only some queries take are checked before
/// the document is read, as `--format` is.
/// Bug pinned: `satex query nosuch` and `query options` without `--for` ran
/// the whole analysis first and only then said what was wrong.
#[test]
fn a_query_s_arguments_are_checked_before_the_document_is_read() {
    let missing = crate::support::tmp_root().join("no-such-document.tex");
    for (args, says) in [
        (&["query", "nosuch"][..], "`satex query --list`"),
        (&["query", "options"][..], "--for NAME"),
        (&["query", "files", "--text", "x"][..], "`--text`"),
    ] {
        let out = crate::support::satex().args(args).arg("-f").arg(&missing).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && stderr.contains(says) && !stderr.contains("No such file"),
            "{args:?}: {stderr}"
        );
    }
}

/// A CSV or Markdown table names where each row stands, as the text table's
/// `@ file:line:col` does.
/// Bug pinned: both kept the text table's columns only, so a CSV of
/// findings named the file but not the line.
#[test]
fn csv_and_markdown_findings_carry_their_line_and_column() {
    let csv = lint_after_umlaut("csv", &["--format", "csv"]);
    // `file,line,col` close the header; a quoted cell before them may hold commas.
    assert!(csv.lines().next().is_some_and(|h| h.ends_with(",file,line,col")), "{csv}");
    let row: Vec<&str> = csv.lines().nth(1).unwrap_or_default().rsplit(',').take(2).collect();
    assert_eq!(row.get(1), Some(&"1"), "{csv}");
    assert!(row.first().is_some_and(|c| c.parse::<u32>().is_ok_and(|c| c > 1)), "{csv}");
    let markdown = lint_after_umlaut("markdown", &["--format", "markdown"]);
    assert!(markdown.lines().next().is_some_and(|h| h.contains("| file | line | col |")), "{markdown}");
}
