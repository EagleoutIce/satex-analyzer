//! `satex compile`: the build's errors and warnings, each where TeX says it
//! arose and explained by satex's finding there.

#[test]
fn the_build_log_is_explained_where_it_arose() {
    if !crate::support::installed() || satex::project::paths::which("pdflatex").is_err() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-compile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("doc.tex"),
        "\\documentclass{article}\n\\begin{document}\nSee \\ref{nope}.\n\\input{part}\n\\end{document}\n",
    )
    .unwrap();
    std::fs::write(dir.join("part.tex"), "Some \\foo text.\n").unwrap();
    let out = crate::support::satex()
        .args(["compile", "-f", "doc.tex", "--format", "json", "--set", "limits.seconds=0"])
        .current_dir(&dir)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let find = |code: &str| rows.iter().find(|r| r["code"] == code).unwrap_or_else(|| panic!("no {code}: {rows:?}"));
    let undefined = find("undefined-control-sequence");
    assert_eq!((undefined["file"].as_str(), undefined["line"].as_u64()), (Some("part.tex"), Some(1)));
    assert!(undefined["fix"].as_str().is_some_and(|f| f.contains("\\foo")), "{undefined:?}");
    let reference = find("undefined-reference");
    assert_eq!((reference["file"].as_str(), reference["line"].as_u64()), (Some("doc.tex"), Some(3)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A plain TeX file (`% !TeX program = tex`) is built with the plain format by pdfTeX, not
/// with latexmk's LaTeX: the playground's leapyear example and its slices.
#[test]
fn a_plain_tex_file_builds_with_pdftex() {
    if !crate::support::installed() || satex::project::paths::which("pdftex").is_err() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-compile-plain-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("doc.tex"), "% !TeX program = tex\nHello.\n\\bye\n").unwrap();
    let out = crate::support::satex()
        .args(["compile", "-f", "doc.tex", "--format", "json", "--set", "limits.seconds=0"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {said}"));
    assert!(rows.iter().all(|r| r["severity"] != "error"), "{rows:?}");
    // pdfTeX runs the plain format and writes the PDF a viewer shows
    assert!(said.contains("$ pdftex "), "not run by pdftex: {said}");
    assert!(dir.join("doc.pdf").is_file(), "no PDF: {said}");
    // a file that never ends is TeX's fatal error, said as one
    std::fs::write(dir.join("open.tex"), "% !TeX program = tex\nHello.\n").unwrap();
    let out = crate::support::satex()
        .args(["compile", "-f", "open.tex", "--format", "json", "--set", "limits.seconds=0"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    assert!(rows.iter().any(|r| r["message"].as_str().is_some_and(|m| m.contains("no legal \\end found"))), "{rows:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
