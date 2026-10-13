//! kpathsea search-path variables (TEXINPUTS etc.): the fixtures satex reads
//! them from, in the order `src/paths.rs` tries them — environment,
//! `latexmkrc`, Makefile, `satex.yaml` (`--set` included).  Also the paths a
//! document writes itself, like the `../template/` a class is kept in.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::{defined, installed};
use satex::config::{Config, Load};
use satex::facts::Severity;
use satex::machine::{Analysis, Machine};

/// A project directory with `libs/sub/foolocal.sty`, reachable only through
/// a `//`-recursive search of `libs`.
fn fixture(name: &str) -> PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-paths-test-{name}-{}", std::process::id()));
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
        resolves_foolocal(
            &dir,
            &Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() }
        ),
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
        resolves_foolocal(
            &dir,
            &Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() }
        ),
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
    let mut cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
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

/// A document directory beside a template directory, the shape
/// `\IfFileExists{../template/x.cls}` tests for.
fn template_tree(name: &str) -> PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-template-{name}-{}", std::process::id()));
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
    Machine::analyze(
        source,
        Some(&doc),
        &Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() },
    )
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

/// A project whose root holds a stale `x.aux` and whose `build/` holds the fresh one.
fn aux_fixture(name: &str, build_file: &str, text: &str) -> (PathBuf, Analysis) {
    let dir = crate::support::tmp_root().join(format!("satex-outdir-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("build")).unwrap();
    std::fs::write(dir.join("x.aux"), "\\relax\n\\gdef\\stalemark{1}\n").unwrap();
    std::fs::write(dir.join("build/x.aux"), "\\relax\n\\gdef\\freshmark{1}\n").unwrap();
    if !build_file.is_empty() {
        std::fs::write(dir.join(build_file), text).unwrap();
    }
    let doc = dir.join("x.tex");
    let source = "\\documentclass{article}\n\\begin{document}hi\\end{document}\n";
    std::fs::write(&doc, source).unwrap();
    let analysis = Machine::analyze(source, Some(&doc), &crate::support::config());
    (dir, analysis)
}

fn read_aux(analysis: &Analysis) -> Vec<String> {
    analysis.files.iter().filter(|f| f.path.ends_with("x.aux")).map(|f| f.path.clone()).collect()
}

#[test]
fn latexmkrc_out_dir_makes_the_build_aux_the_one_read() {
    if !installed() {
        return;
    }
    let (dir, analysis) = aux_fixture("latexmkrc", "latexmkrc", "$out_dir = 'build';\n");
    let aux = read_aux(&analysis);
    assert!(aux.iter().any(|p| p.contains("build")) && !aux.iter().any(|p| !p.contains("build")), "{aux:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn latexmkrc_aux_dir_outranks_out_dir() {
    if !installed() {
        return;
    }
    let (dir, analysis) = aux_fixture("auxdir", "latexmkrc", "$out_dir = 'elsewhere';\n$aux_dir = 'build';\n");
    assert!(read_aux(&analysis).iter().all(|p| p.contains("build")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn makefile_output_directory_flag_is_read() {
    if !installed() {
        return;
    }
    let (dir, analysis) = aux_fixture("make", "Makefile", "all:\n\tpdflatex -output-directory=build x.tex\n");
    assert!(read_aux(&analysis).iter().all(|p| p.contains("build")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_output_directory_is_the_fallback() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-outdir-test-config-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("build")).unwrap();
    std::fs::write(dir.join("x.aux"), "\\relax\n").unwrap();
    std::fs::write(dir.join("build/x.aux"), "\\relax\n").unwrap();
    let doc = dir.join("x.tex");
    let source = "\\documentclass{article}\n\\begin{document}hi\\end{document}\n";
    std::fs::write(&doc, source).unwrap();
    let mut cfg = crate::support::config();
    cfg.paths.output_directory = "build".into();
    let analysis = Machine::analyze(source, Some(&doc), &cfg);
    assert!(read_aux(&analysis).iter().all(|p| p.contains("build")));
    let _ = std::fs::remove_dir_all(&dir);
}

/// `source` run as `doc.tex` in a fresh directory holding `files`, the
/// modeled primitives and the files there only.
fn analyze_beside(name: &str, files: &[(&str, &str)], source: &str, cfg: Config) -> Analysis {
    let dir =
        crate::support::scratch_dir(std::env::temp_dir().join(format!("satex-beside-{name}-{}", std::process::id())));
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let doc = dir.join("doc.tex");
    let source = format!("{}{source}", crate::support::PRELUDE);
    std::fs::write(&doc, &source).unwrap();
    let analysis = Machine::analyze(&source, Some(&doc), &cfg);
    let _ = std::fs::remove_dir_all(&dir);
    analysis
}

/// tex.web § 526: a file name takes every character token up to category
/// 12, so `_` (category 8) does not end it (`\input x_loader.code.tex`).
#[test]
fn a_file_name_does_not_end_at_an_underscore() {
    let cfg = Config { load: Load { inputs: true, ..crate::support::bare().load }, ..crate::support::bare() };
    let source = "\\catcode`\\_=8 \\input part_two.tex \\relax\n";
    let analysis = analyze_beside("underscore", &[("part_two.tex", "\\def\\found{}\n")], source, cfg);
    assert!(defined(&analysis, "found"), "part_two.tex is read");
}

/// `\immediate\write18` of a program the engine may run makes the file its
/// `--outfile=` names when its input is there (epstopdf's `repstopdf`), so
/// `\pdffilesize` of it is not empty; without the input, or with a program
/// `-shell-restricted` does not allow, nothing is made.
#[test]
fn a_shell_escape_conversion_makes_its_output_file() {
    let probe = "\\immediate\\write18{PROG --outfile=out.pdf in.eps}\n\
                 \\expandafter\\ifx\\expandafter\\relax\\pdffilesize{out.pdf}\\relax\\def\\missing{}\\else\\def\\made{}\\fi\n";
    for (name, program, input, made) in [
        ("restricted", "repstopdf", true, true),
        ("no-input", "repstopdf", false, false),
        ("not-allowed", "epstopdf", true, false),
    ] {
        let files: &[(&str, &str)] = if input { &[("in.eps", "%!PS\n")] } else { &[] };
        let analysis = analyze_beside(name, files, &probe.replace("PROG", program), crate::support::bare());
        assert_eq!((defined(&analysis, "made"), defined(&analysis, "missing")), (made, !made), "{name}");
    }
}
