//! `satex compile`: the build's errors and warnings, each where TeX says it
//! arose and explained by satex's finding there.

use std::process::Command;

#[test]
fn the_build_log_is_explained_where_it_arose() {
    if !crate::support::installed() || satex::paths::which("pdflatex").is_err() {
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
    let out = Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(["compile", "-f", "doc.tex", "--format", "json"])
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
