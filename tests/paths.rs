//! kpathsea search-path variables (TEXINPUTS etc.): the fixtures satex reads
//! them from, in the order `src/paths.rs` tries them — environment,
//! `latexmkrc`, Makefile, `satex.yaml` (`--set` included).  Also the paths a
//! document writes itself, like the `../template/` a class is kept in.

use std::path::{Path, PathBuf};
use std::process::Command;

use satex::config::Config;
use satex::facts::Severity;
use satex::machine::{Analysis, Machine};

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
    assert!(resolves_foolocal(&dir, &cfg), "--set paths.texinputs=[./libs//] should reach libs/sub/foolocal.sty");
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
    // Compared by components: the reported path is `\`-separated on Windows.
    assert!(dirs.iter().any(|d| Path::new(d).ends_with("libs/sub")), "{dirs:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// What the run could not follow: one row per cause, the way
/// `satex query gaps` reports them.
fn gaps(analysis: &Analysis) -> Vec<(&'static str, u32)> {
    analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Unsupported)
        .map(|d| (d.code, d.count))
        .collect()
}

fn defined(analysis: &Analysis, name: &str) -> bool {
    analysis.interner.lookup(name).is_some_and(|sym| analysis.env.is_defined(sym))
}

/// A document directory beside a template directory, the shape
/// `\IfFileExists{../template/x.cls}` tests for.
fn template_tree(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-template-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("doc")).unwrap();
    std::fs::create_dir_all(dir.join("template")).unwrap();
    std::fs::write(
        dir.join("template").join("mycls.cls"),
        "\\ProvidesClass{mycls}\n\\LoadClass{article}\n\\newif\\ifexam \\examtrue\n",
    )
    .unwrap();
    dir
}

fn analyze_in(dir: &Path, name: &str, source: &str) -> Analysis {
    let doc = dir.join("doc").join(name);
    std::fs::write(&doc, source).unwrap();
    Machine::analyze(source, Some(&doc), &Config { load_classes: true, ..Config::default() })
}

#[test]
fn a_class_named_by_a_relative_path_is_read_and_defines_its_commands() {
    if !installed() {
        return;
    }
    let dir = template_tree("relative");
    let analysis = analyze_in(
        &dir,
        "a.tex",
        "\\documentclass{../template/mycls}\n\\begin{document}\n\\ifexam\\clearpage\\fi\nx\n\\end{document}\n",
    );
    let wanted = dir.join("doc").join("../template/mycls.cls").display().to_string();
    let read: Vec<&str> = analysis.files.iter().map(|f| f.path.as_str()).collect();
    assert!(read.contains(&wanted.as_str()), "read {wanted}, not {read:?}");
    assert!(defined(&analysis, "ifexam"), "the class defines \\ifexam");
    assert_eq!(gaps(&analysis), Vec::new(), "the class is read, so \\fi has its \\if");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The template idiom: `\IfFileExists` picks the directory and
/// `\documentclass` is given the name the macro builds.
#[test]
fn a_class_found_through_an_if_file_exists_probe_is_read() {
    if !installed() {
        return;
    }
    let dir = template_tree("probe");
    let analysis = analyze_in(
        &dir,
        "b.tex",
        "\\IfFileExists{../template/mycls.cls}{\\def\\templatepath{../template/}}{\\def\\templatepath{}}\n\
         \\documentclass{\\templatepath mycls}\n\\begin{document}\n\\ifexam\\clearpage\\fi\nx\n\\end{document}\n",
    );
    assert!(defined(&analysis, "ifexam"), "the class defines \\ifexam");
    assert_eq!(gaps(&analysis), Vec::new());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A probe for a file that is not there is an answer, not a gap.
#[test]
fn an_if_file_exists_probe_that_finds_nothing_leaves_no_gap() {
    if !installed() {
        return;
    }
    let dir = template_tree("absent");
    let analysis = analyze_in(
        &dir,
        "c.tex",
        "\\documentclass{../template/mycls}\n\
         \\IfFileExists{nowhere.sty}{\\def\\found{}}{\\def\\missing{}}\n\
         \\InputIfFileExists{nowhere.sty}{}{}\n\\begin{document}\nx\n\\end{document}\n",
    );
    assert!(defined(&analysis, "missing"), "the probe took its second arm");
    assert_eq!(gaps(&analysis), Vec::new());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A class that is nowhere defines nothing, which the run has to say.
#[test]
fn a_class_that_cannot_be_found_is_recorded_as_a_gap() {
    if !installed() {
        return;
    }
    let dir = template_tree("missing");
    let analysis =
        analyze_in(&dir, "d.tex", "\\documentclass{../template/nosuchcls}\n\\begin{document}\nx\n\\end{document}\n");
    assert_eq!(gaps(&analysis), vec![("unresolved-class", 1)]);
    let message = analysis
        .facts
        .diagnostics
        .iter()
        .find(|d| d.code == "unresolved-class")
        .map(|d| d.message.clone())
        .unwrap_or_default();
    assert!(message.contains("nosuchcls.cls"), "{message}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A package that is nowhere defines nothing either.
#[test]
fn a_package_that_cannot_be_found_is_recorded_as_a_gap() {
    if !installed() {
        return;
    }
    let dir = template_tree("missing-package");
    let analysis = analyze_in(
        &dir,
        "e.tex",
        "\\documentclass{../template/mycls}\n\\usepackage{nosuchpkg}\n\\begin{document}\nx\n\\end{document}\n",
    );
    assert_eq!(gaps(&analysis), vec![("unresolved-package", 1)]);
    let _ = std::fs::remove_dir_all(&dir);
}
