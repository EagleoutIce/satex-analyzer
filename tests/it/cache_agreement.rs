//! The answer does not depend on what is cached. A run that builds the
//! kernel and package caches and a run that reads them report the same
//! diagnostics, in the same order, with the same spans, messages and
//! counts.

use std::path::{Path, PathBuf};

use crate::support::installed;
use satex::config::{Cache, Config};
use satex::machine::{Analysis, Machine};

/// A project directory with its own empty cache: nothing this test reads
/// was built by another.
fn project(name: &str) -> PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-cache-agreement-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cache")).expect("temporary directory");
    dir
}

/// `project` with the kernel cache already there: for tests about packages,
/// whose kernel agreement `twice` and the others below already cover cold.
fn seeded(name: &str) -> PathBuf {
    let dir = project(name);
    crate::support::seed_kernel(&config(&dir));
    dir
}

fn config(dir: &Path) -> Config {
    Config { cache: Cache { dir: Some(dir.join("cache")), ..Default::default() }, ..crate::support::config() }
}

fn analyze(dir: &Path, source: &str, cfg: &Config) -> Analysis {
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).expect("doc.tex");
    Machine::analyze(source, Some(&path), cfg)
}

fn run(dir: &Path, source: &str) -> Analysis {
    analyze(dir, source, &config(dir))
}

/// Every diagnostic as `query diagnostics` prints it: code, severity,
/// place, count and message, in the order the run raised them.
fn rows(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "{} {} {}:{}:{} x{} {}",
                d.code,
                d.severity.as_str(),
                analysis.file_name(d.span.file),
                d.span.line,
                d.span.col,
                d.count,
                d.message
            )
        })
        .collect()
}

fn count_of(analysis: &Analysis, code: &str) -> usize {
    analysis.facts.diagnostics.iter().filter(|d| d.code == code).count()
}

/// The first row two runs disagree on, for a readable failure.
fn same_rows(a: &Analysis, b: &Analysis, what: &str) {
    let (x, y) = (rows(a), rows(b));
    let at = x.iter().zip(&y).take_while(|(p, q)| p == q).count();
    assert!(
        x == y,
        "{what}: diagnostics differ at {at} of {}/{}\n  uncached {:?}\n  cached   {:?}",
        x.len(),
        y.len(),
        x.get(at),
        y.get(at)
    );
}

const PLAIN: &str = "\\documentclass{article}\n\\begin{document}\nHello.\n\\end{document}\n";

const PACKAGES: &str = "\\documentclass{article}\n\\usepackage{xcolor}\n\\usepackage{etoolbox}\n\
\\begin{document}\n\\definecolor{mine}{rgb}{1,0,0}\\textcolor{mine}{x}\n\\end{document}\n";

/// A package that leaves a gap: the file it requires is not installed.
const GAPPY: &str = "\\ProvidesPackage{gappy}[2020/01/01 v1]\n\
\\RequirePackage{satex-no-such-package}\n\\def\\gappymacro{g}\n";

const GAPS: &str = "\\documentclass{article}\n\\usepackage{gappy}\n\
\\begin{document}\\gappymacro\\end{document}\n";

#[test]
fn a_run_that_builds_the_caches_and_one_that_reads_them_agree() {
    if !installed() {
        return;
    }
    let dir = project("twice");
    std::fs::write(dir.join("gappy.sty"), GAPPY).expect("gappy.sty");
    // The first document pays for the kernel; the later ones still read
    // their packages for the first time.
    for (what, source) in [("plain", PLAIN), ("packages", PACKAGES), ("gaps", GAPS)] {
        let cold = run(&dir, source);
        let warm = run(&dir, source);
        same_rows(&cold, &warm, what);
        let again = run(&dir, source);
        same_rows(&warm, &again, what);
        assert_eq!(colors(&cold), colors(&warm), "{what}: color facts");
    }
    // The packages really were served the second time, or the test would
    // compare two cold runs.
    let warm = run(&dir, PACKAGES);
    let served = |package: &str| {
        warm.package_caches.iter().any(|(name, .., state)| name == package && state.as_str() == "cached")
    };
    assert!(served("xcolor"), "xcolor was replayed: {:?}", warm.package_caches);
    assert!(served("etoolbox"), "etoolbox was replayed: {:?}", warm.package_caches);
}

/// Every color fact, rendered so two runs compare: the definitions, the
/// `\color@⟨name⟩` uses and the `\pdfcolorstack` operations.
fn colors(analysis: &Analysis) -> Vec<String> {
    let mut out: Vec<String> = analysis
        .facts
        .colors
        .iter()
        .map(|c| {
            format!(
                "def {} {:?} {}:{} {:?}",
                c.name,
                c.value,
                analysis.file_name(c.span.file),
                c.span.line,
                c.package.map(|p| analysis.interner.name(p).to_string())
            )
        })
        .collect();
    out.extend(analysis.facts.color_uses.iter().map(|u| {
        format!("use {} {:?} {} {}:{}", u.name, u.role, u.defined, analysis.file_name(u.span.file), u.span.line)
    }));
    out.extend(
        analysis.facts.color_stack.iter().map(|e| {
            format!("stack {:?} {} {}:{}", e.stack, e.op.as_str(), analysis.file_name(e.span.file), e.span.line)
        }),
    );
    out
}

#[test]
fn colors_a_package_defines_are_reported_cached_or_not() {
    if !installed() {
        return;
    }
    let dir = seeded("colors");
    let cold = run(&dir, PACKAGES);
    let warm = run(&dir, PACKAGES);
    let named = |a: &Analysis| -> Vec<String> {
        let mut names: Vec<String> =
            a.facts.colors.iter().filter(|c| c.package.is_some()).map(|c| c.name.clone()).collect();
        names.sort();
        names.dedup();
        names
    };
    // xcolor's own base colors are facts of the run, not of the cache.
    for base in ["red", "green", "blue", "white", "black"] {
        assert!(named(&cold).iter().any(|n| n == base), "{base} from the cold run: {:?}", named(&cold));
    }
    assert_eq!(named(&cold), named(&warm));
    assert_eq!(colors(&cold), colors(&warm));
}

#[test]
fn a_run_without_the_format_cache_and_one_with_it_agree() {
    if !installed() {
        return;
    }
    let dir = project("format");
    // No cache at all: the kernel is interpreted and the packages are read.
    let uncached =
        analyze(&dir, PACKAGES, &Config { cache: Cache { enabled: false, ..Default::default() }, ..config(&dir) });
    let stored = run(&dir, PACKAGES);
    let loaded = run(&dir, PACKAGES);
    same_rows(&uncached, &stored, "no cache against a cache being built");
    same_rows(&uncached, &loaded, "no cache against a cache being read");
    assert!(uncached.format.as_ref().is_some_and(|f| !f.cached), "the first run interpreted the kernel");
    assert!(loaded.format.as_ref().is_some_and(|f| f.cached), "the third run installed the cached kernel");
}

#[test]
fn a_gap_inside_a_package_survives_being_cached() {
    if !installed() {
        return;
    }
    let dir = seeded("gap");
    std::fs::write(dir.join("gappy.sty"), GAPPY).expect("gappy.sty");
    let cold = run(&dir, GAPS);
    let warm = run(&dir, GAPS);
    assert_eq!(count_of(&cold, "unresolved-package"), 1, "the cold run records the gap once: {:?}", rows(&cold));
    assert_eq!(count_of(&warm, "unresolved-package"), 1, "the replay records it once too: {:?}", rows(&warm));
    let gap = |a: &Analysis| {
        a.facts
            .diagnostics
            .iter()
            .find(|d| d.code == "unresolved-package")
            .map(|d| (d.severity.as_str(), d.message.clone(), d.count))
    };
    assert_eq!(
        gap(&cold),
        Some((
            "unsupported",
            "package file `satex-no-such-package.sty` was not found, so nothing it defines is in this analysis"
                .to_string(),
            1
        ))
    );
    assert_eq!(gap(&cold), gap(&warm));
    same_rows(&cold, &warm, "a package that leaves a gap");
}

/// A preamble interpreted as the project's format, so the diagnostics it
/// raises are the kernel build's and not the document's. The count is
/// exact: three bad `\lccode`s, whether the kernel was interpreted or
/// installed from its cache.
const PRELOAD: &str = "\\lccode\"2019=\"2019\n\\lccode\"201C=\"201C\n\\lccode\"201D=\"201D\n";

#[test]
fn what_the_kernel_build_diagnosed_is_reported_on_both_paths() {
    if !installed() {
        return;
    }
    let dir = project("preload");
    let preload = dir.join("preload.tex");
    std::fs::write(&preload, PRELOAD).expect("preload.tex");
    let with = Config { preload: Some(preload), ..config(&dir) };
    let built = analyze(&dir, PLAIN, &with);
    let installed = analyze(&dir, PLAIN, &with);
    assert!(built.format.as_ref().is_some_and(|f| !f.cached), "the first run interpreted the kernel");
    assert!(installed.format.as_ref().is_some_and(|f| f.cached), "the second run installed it");
    // Three from the preload, whether the kernel was interpreted here or
    // installed from its cache.
    let from_preload = |a: &Analysis| {
        a.facts
            .diagnostics
            .iter()
            .filter(|d| d.code == "invalid-code" && a.file_name(d.span.file).ends_with("preload.tex"))
            .count()
    };
    assert_eq!(from_preload(&built), 3, "{:?}", rows(&built));
    assert_eq!(from_preload(&installed), 3, "{:?}", rows(&installed));
    assert_eq!(count_of(&installed, "invalid-code"), count_of(&built, "invalid-code"));
    same_rows(&built, &installed, "a preloaded format");
}

/// A package and its documentation in one project: the kernel is the same
/// for both, so the second run must install the cache the first built.
#[test]
fn a_package_and_a_document_in_one_project_share_the_kernel_cache() {
    if !installed() {
        return;
    }
    let dir = project("shared-kernel");
    std::fs::create_dir_all(dir.join("tex")).expect("tex");
    std::fs::create_dir_all(dir.join("doc")).expect("doc");
    std::fs::write(dir.join("tex/mypkg.sty"), "\\ProvidesPackage{mypkg}[2020/01/01 v1]\n\\def\\mymacro{x}\n")
        .expect("mypkg.sty");
    let document = "\\documentclass{article}\n\\begin{document}\\relax\\end{document}\n";
    std::fs::write(dir.join("doc/mydoc.tex"), document).expect("mydoc.tex");
    let at = |path: &str, source: &str| {
        let path = dir.join(path);
        Machine::analyze(source, Some(&path), &config(&dir))
    };
    let package = at("tex/mypkg.sty", "\\ProvidesPackage{mypkg}[2020/01/01 v1]\n\\def\\mymacro{x}\n");
    assert_eq!(
        package.format.as_ref().map(|f| f.cached),
        Some(false),
        "the first run builds the kernel cache: {:?}",
        package.format.as_ref().map(|f| f.source.clone())
    );
    // The same file again, then the document beside it: neither rebuilds.
    for (path, source) in
        [("tex/mypkg.sty", "\\ProvidesPackage{mypkg}[2020/01/01 v1]\n\\def\\mymacro{x}\n"), ("doc/mydoc.tex", document)]
    {
        let again = at(path, source);
        assert_eq!(again.format.as_ref().map(|f| f.cached), Some(true), "{path} rebuilt the kernel cache");
    }
    // One cache, and no lock left over for a cache that is gone.
    let names: Vec<String> = std::fs::read_dir(dir.join("cache"))
        .expect("cache")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("format-"))
        .collect();
    let caches = names.iter().filter(|n| n.ends_with(".postcard")).count();
    let locks = names.iter().filter(|n| n.ends_with(".lock")).count();
    assert_eq!(caches, 1, "{names:?}");
    assert_eq!(locks, 1, "a lock per cache and no more: {names:?}");
}
