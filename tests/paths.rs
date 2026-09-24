//! kpathsea search-path variables (TEXINPUTS etc.): the fixtures satex reads
//! them from, in the order `src/paths.rs` tries them — environment,
//! `latexmkrc`, Makefile, `satex.yaml` (`--set` included).

use std::path::{Path, PathBuf};
use std::process::Command;

use satex::config::Config;
use satex::machine::Machine;

fn installed() -> bool {
    which::which("kpsewhich").is_ok()
}

/// A project directory with `libs/sub/foolocal.sty`, reachable only through
/// a `//`-recursive search of `libs`.
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-paths-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("libs/sub")).unwrap();
    std::fs::write(dir.join("libs/sub/foolocal.sty"), "\\ProvidesPackage{foolocal}\n").unwrap();
    dir
}

fn resolves_foolocal(dir: &Path, cfg: &Config) -> bool {
    let doc = dir.join("x.tex");
    let source = "\\documentclass{article}\n\\usepackage{foolocal}\n\\begin{document}hi\\end{document}\n";
    std::fs::write(&doc, source).unwrap();
    let analysis = Machine::analyze(source, Some(&doc), cfg);
    analysis.files.iter().any(|f| f.path.ends_with("foolocal.sty"))
}

#[test]
fn latexmkrc_ensure_path_recurses_into_a_subdirectory() {
    if !installed() {
        return;
    }
    let dir = fixture("latexmkrc");
    std::fs::write(dir.join("latexmkrc"), "$pdf_mode = 1;\nensure_path('TEXINPUTS', './libs//');\n").unwrap();
    assert!(
        resolves_foolocal(&dir, &Config { load_classes: true, ..Config::default() }),
        "ensure_path('TEXINPUTS', './libs//') should reach libs/sub/foolocal.sty"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn makefile_export_texinputs_is_read() {
    if !installed() {
        return;
    }
    let dir = fixture("makefile");
    std::fs::write(dir.join("Makefile"), "export TEXINPUTS=./libs//\nall:\n\tpdflatex x.tex\n").unwrap();
    assert!(
        resolves_foolocal(&dir, &Config { load_classes: true, ..Config::default() }),
        "export TEXINPUTS=./libs// should reach libs/sub/foolocal.sty"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn set_paths_texinputs_reaches_a_configured_directory() {
    if !installed() {
        return;
    }
    let dir = fixture("set");
    let mut cfg = Config { load_classes: true, ..Config::default() };
    cfg.set("paths.texinputs", "[\"./libs//\"]").unwrap();
    assert!(
        resolves_foolocal(&dir, &cfg),
        "--set paths.texinputs=[./libs//] should reach libs/sub/foolocal.sty"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `satex --version --format json` needs no document, so it is the cheap way
/// to check the process environment (which the library API cannot safely
/// fake in a parallel test binary) against a project's own `latexmkrc`.
#[test]
fn environment_variable_outranks_latexmkrc() {
    let dir = fixture("env");
    std::fs::write(dir.join("latexmkrc"), "ensure_path('TEXINPUTS', './other//');\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(["--version", "--format", "json"])
        .current_dir(&dir)
        .env("TEXINPUTS", "./libs//")
        .output()
        .expect("satex runs");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid json");
    let texinputs = json["paths"]
        .as_array()
        .expect("paths array")
        .iter()
        .find(|p| p["variable"] == "TEXINPUTS")
        .expect("TEXINPUTS entry");
    assert_eq!(texinputs["source"], "environment");
    let dirs: Vec<&str> = texinputs["dirs"].as_array().unwrap().iter().map(|d| d.as_str().unwrap()).collect();
    assert!(dirs.iter().any(|d| d.ends_with("libs/sub")), "{dirs:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
