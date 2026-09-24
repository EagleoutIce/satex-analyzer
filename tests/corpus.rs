//! satex must survive any TeX it is pointed at.
//!
//! The installation is the corpus: plain TeX macro files, LaTeX classes and
//! packages, documented sources, expl3 code and ConTeXt modules.  Every file
//! is analyzed with a bounded budget; what the test asks of each is that the
//! run ends, that it ends without panicking, and that it reports what it gave
//! up on rather than silently producing nothing.

use std::path::{Path, PathBuf};
use std::process::Command;

use satex::config::Config;
use satex::machine::{Analysis, Machine};

fn texmf() -> Option<PathBuf> {
    let output = Command::new("kpsewhich").arg("-var-value=TEXMFDIST").output().ok()?;
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
    path.is_dir().then_some(path)
}

/// The first `count` files with this extension under `directory`, in a fixed
/// order so a failure names the same file on every machine.
fn sample(directory: &Path, extension: &str, count: usize) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = walkdir::WalkDir::new(directory)
        .max_depth(4)
        .into_iter()
        .flatten()
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.extension().is_some_and(|e| e == extension))
        .collect();
    found.sort();
    found.truncate(count);
    found
}

/// A run bounded tightly enough that the whole corpus stays quick: the point
/// is that satex copes, not that it reads every file to the end.
fn analyze(path: &Path) -> Option<Analysis> {
    let source = std::fs::read_to_string(path).ok()?;
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        limits: satex::config::Limits { steps: 2_000_000, ..Config::default().limits },
        ..Config::default()
    };
    Some(Machine::analyze(&source, Some(path), &cfg))
}

fn check(paths: &[PathBuf]) {
    for path in paths {
        let Some(analysis) = analyze(path) else { continue };
        // Whatever it could not follow, it says so: a run that stops early
        // carries an imprecision, never silence.
        if analysis.exhausted {
            assert!(
                analysis
                    .facts
                    .diagnostics
                    .iter()
                    .any(|d| d.severity == satex::facts::Severity::Imprecision || d.code == "budget-exhausted"),
                "{}: the run stopped early without saying so",
                path.display()
            );
        }
    }
}

#[test]
fn plain_tex_macro_files() {
    let Some(texmf) = texmf() else { return };
    check(&sample(&texmf.join("tex/plain"), "tex", 40));
}

#[test]
fn latex_classes_and_packages() {
    let Some(texmf) = texmf() else { return };
    check(&sample(&texmf.join("tex/latex"), "cls", 30));
    check(&sample(&texmf.join("tex/latex"), "sty", 60));
}

#[test]
fn documented_sources() {
    let Some(texmf) = texmf() else { return };
    check(&sample(&texmf.join("source/latex"), "dtx", 20));
    check(&sample(&texmf.join("source/latex"), "ins", 20));
}

#[test]
fn generic_and_context_sources() {
    let Some(texmf) = texmf() else { return };
    check(&sample(&texmf.join("tex/generic"), "tex", 40));
    check(&sample(&texmf.join("tex/context"), "mkiv", 20));
    check(&sample(&texmf.join("tex/context"), "mkxl", 20));
}
