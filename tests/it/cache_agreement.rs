//! The answer does not depend on what is cached. A run that builds the
//! kernel and package caches and a run that reads them report the same
//! diagnostics, in the same order, with the same spans, messages and
//! counts.

use std::path::{Path, PathBuf};

use crate::support::installed;
use satex::analysis::machine::{Analysis, Machine};
use satex::project::config::{Cache, Config};

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
    Config {
        cache: Cache { dir: Some(dir.join("cache")), document: false, ..Default::default() },
        ..crate::support::config()
    }
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

/// A run with no cache, one that builds the caches and ones that read them report the same.
/// Pins cache-only facts: a kernel or package replay that loses or adds a diagnostic or a color.
#[test]
fn a_run_that_builds_the_caches_and_one_that_reads_them_agree() {
    if !installed() {
        return;
    }
    let dir = project("twice");
    std::fs::write(dir.join("gappy.sty"), GAPPY).expect("gappy.sty");
    // No cache at all: the kernel is interpreted and the packages are read.
    let uncached =
        analyze(&dir, PACKAGES, &Config { cache: Cache { enabled: false, ..Default::default() }, ..config(&dir) });
    assert!(uncached.format.as_ref().is_some_and(|f| !f.cached), "the uncached run interpreted the kernel");
    // The first document pays for the kernel; the later ones still read
    // their packages for the first time.
    for (what, source) in [("plain", PLAIN), ("packages", PACKAGES), ("gaps", GAPS)] {
        let cold = run(&dir, source);
        let warm = run(&dir, source);
        same_rows(&cold, &warm, what);
        let again = run(&dir, source);
        same_rows(&warm, &again, what);
        assert_eq!(colors(&cold), colors(&warm), "{what}: color facts");
        if source == PACKAGES {
            same_rows(&uncached, &cold, "no cache against a cache being built");
            same_rows(&uncached, &warm, "no cache against a cache being read");
            assert!(warm.format.as_ref().is_some_and(|f| f.cached), "the warm run installed the cached kernel");
        }
    }
    // The packages really were served the second time, or the test would
    // compare two cold runs.
    let warm = run(&dir, PACKAGES);
    let served = |package: &str| {
        warm.package_caches
            .iter()
            .any(|(name, .., state)| name == package && satex::analysis::machine::cache_served(state))
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
    let caches: Vec<&str> = names.iter().filter_map(|n| n.strip_suffix(".postcard")).collect();
    assert_eq!(caches.len(), 1, "{names:?}");
    // The store lock and the build lock name their cache.
    let stem = |n: &str| n.strip_suffix(".building.lock").or_else(|| n.strip_suffix(".lock")).map(str::to_string);
    let orphans: Vec<&String> =
        names.iter().filter(|n| stem(n).is_some_and(|s| !caches.contains(&s.as_str()))).collect();
    assert!(orphans.is_empty(), "locks of no cache: {orphans:?}");
}

/// Reads what the main vertical list holds after the class: whether the
/// kernel and the class were read or replayed, a job reads the same.
const LISTS: &str = "\\documentclass{article}\n\
\\ifnum\\lastnodetype=-1 \\gdef\\listmark{empty}\\else\\gdef\\listmark{items}\\fi\n\
\\begin{document}x\\end{document}\n";

/// A format holds no lists (tex.web § 1299), and a replayed load leaves
/// them as reading it did: no cache makes `\lastnodetype` more decided.
#[test]
fn the_lists_a_job_reads_are_the_same_whether_the_kernel_was_read_or_loaded() {
    if !installed() {
        return;
    }
    let dir = project("lists");
    let marks = |a: &Analysis| {
        let mark = a.interner.lookup("listmark");
        a.facts.defs.iter().filter(|d| Some(d.name) == mark).map(|d| (d.span.line, d.span.col)).collect::<Vec<_>>()
    };
    let uncached =
        analyze(&dir, LISTS, &Config { cache: Cache { enabled: false, ..Default::default() }, ..config(&dir) });
    let stored = run(&dir, LISTS);
    let loaded = run(&dir, LISTS);
    assert!(loaded.format.as_ref().is_some_and(|f| f.cached), "the third run installed the cached kernel");
    assert!(!marks(&uncached).is_empty(), "the mark is defined");
    assert_eq!(marks(&uncached), marks(&stored), "interpreting the kernel into the cache");
    assert_eq!(marks(&uncached), marks(&loaded), "installing the kernel and replaying the class");
    same_rows(&uncached, &loaded, "the lists a job reads");
}

/// `\DocumentMetadata` loads the PDF management and tagging code, whose
/// conditionals fork at a few hot sites.
const METADATA: &str = "\\DocumentMetadata{lang=en}\n\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";

/// The fork counts that widen a hot site are kept per load, so a load read
/// and the same load replayed in part widen at the same choices.
#[test]
fn a_tagged_document_reads_the_same_with_its_loads_replayed() {
    if !installed() {
        return;
    }
    let dir = seeded("metadata");
    let cold = run(&dir, METADATA);
    let warm = run(&dir, METADATA);
    same_rows(&cold, &warm, "a tagged document");
}

/// xcolor loaded inside a group (LaTeX errors, then reads it anyway, its
/// local definitions one level deeper). A cold run and a run whose xcolor
/// cache was built by a top-level load must agree. Pins: the replay keeps
/// each definition's group depth from the run that built the cache (0 and 1
/// instead of 1 and 2), so `unused-package` counted 17 names cold and 19
/// names warm, and warm alone said `^^A` keeps category code 14 at the end.
#[test]
fn a_load_in_a_group_reports_the_same_cold_and_from_a_top_level_cache() {
    if !installed() {
        return;
    }
    const TOP: &str =
        "\\documentclass{article}\n\\usepackage{xcolor}\n\\begin{document}\n\\textcolor{red}{x}\n\\end{document}\n";
    const GROUP: &str =
        "\\documentclass{article}\n\\begingroup\\usepackage{xcolor}\\endgroup\n\\begin{document}\nx\n\\end{document}\n";
    let lint = |a: &Analysis| -> Vec<String> {
        let s = |f: &satex::queries::query::Record, k: &str| f.get(k).map(|v| v.to_string()).unwrap_or_default();
        satex::linter::lint(a).iter().map(|f| format!("{} {}", s(f, "code"), s(f, "message"))).collect()
    };
    let cold = run(&seeded("group-load-cold"), GROUP);
    let dir = seeded("group-load-warm");
    run(&dir, TOP);
    let warm = run(&dir, GROUP);
    let (c, w) = (lint(&cold), lint(&warm));
    let only = |a: &[String], b: &[String]| a.iter().filter(|x| !b.contains(x)).cloned().collect::<Vec<_>>();
    assert!(c == w, "cold only {:?}, warm only {:?}", only(&c, &w), only(&w, &c));
    same_rows(&cold, &warm, "group load");
}

/// Which files a project takes as given (`project.frozen`) only steers reports, and its globs
/// are made absolute to the project folder. A package cache keyed by them was built anew for
/// every folder: the same project checked out elsewhere, or a second project with such a
/// setting, never reused a package the machine had already analyzed.
#[test]
fn a_package_cache_is_shared_by_projects_that_freeze_different_files() {
    if !installed() {
        return;
    }
    let dir = seeded("frozen-globs");
    let source = "\\documentclass{article}\n\\usepackage{keyval}\n\\begin{document}x\\end{document}\n";
    let run = |project: &str| {
        let home = dir.join(project);
        std::fs::create_dir_all(&home).expect("project folder");
        let mut cfg = config(&dir);
        cfg.project.frozen = vec![format!("{}/lib/**", home.display())];
        let path = home.join("doc.tex");
        std::fs::write(&path, source).expect("doc.tex");
        Machine::analyze(source, Some(&path), &cfg)
    };
    let (first, second) = (run("one"), run("two"));
    let stored = |a: &Analysis| a.package_caches.iter().filter(|(_, _, state)| state.ends_with("stored")).count();
    assert!(stored(&first) > 0, "the first project builds the caches: {:?}", first.package_caches);
    assert_eq!(stored(&second), 0, "the second project built them again: {:?}", second.package_caches);
    same_rows(&first, &second, "frozen globs");
}

/// Runs `group` in one project: cold, then once after each of `firsts` (with
/// the package's own caches dropped first, so `first` builds them). Each warm
/// run must say what the cold one said; returns the last warm run.
fn in_group_runs(name: &str, package: &str, sty: &str, group: &str, firsts: &[&str]) -> Analysis {
    let dir = seeded(name);
    let said = |a: &Analysis| {
        let lint = satex::linter::lint(a).iter().map(|f| format!("{:?}", f.get("message"))).collect::<Vec<_>>();
        [crate::coldwarm::said(a), lint].concat()
    };
    // Only the kernel and other packages' caches stay: outputs and this package's trees go.
    let fresh = || {
        for e in std::fs::read_dir(&dir).expect("dir").flatten() {
            if e.file_name() != "cache" {
                let _ = std::fs::remove_file(e.path());
            }
        }
        for e in std::fs::read_dir(dir.join("cache")).expect("cache").flatten() {
            if e.file_name().to_string_lossy().starts_with(&format!("package-{package}")) {
                let _ = std::fs::remove_file(e.path());
            }
        }
        std::fs::write(dir.join(format!("{package}.sty")), sty).expect("sty");
    };
    fresh();
    let cold = run(&dir, group);
    let leaked: Vec<_> =
        cold.facts.diagnostics.iter().filter(|d| d.message.contains("leaked") || d.message.contains("lost")).collect();
    assert!(leaked.is_empty(), "cold: {leaked:?}");
    let mut last = cold;
    for (i, first) in firsts.iter().enumerate() {
        fresh();
        run(&dir, first);
        let warm = run(&dir, group);
        let diff = crate::coldwarm::difference(&said(&last), &said(&warm));
        assert!(diff.is_empty(), "primed by run {i}:\n{diff}");
        last = warm;
    }
    last
}

/// A tiny package that writes a macro, a `\let`, a global macro, a scratch
/// register and a category code, loaded inside a group. Cold, from a cache a
/// top-level load built, and from a cache a load in a group built, the runs
/// must say the same, and after the group only the global macro is left.
/// Pins: a top-level recording replayed in a group left the load's locals in
/// place (the tree's key had no group level), and a replay wrote a local by
/// the binding's sticky `global` flag (a local `\dimen@=` keeps the flag that
/// the register's `\newdimen` set), so the register kept its value past the group.
#[test]
fn a_tiny_package_in_a_group_says_the_same_cold_and_from_either_cache() {
    if !installed() {
        return;
    }
    const STY: &str = "\\ProvidesPackage{grp}\n\\def\\grpdef{1}\n\\let\\grplet\\relax\n\\gdef\\grpglobal{1}\n\
                       \\dimen@=1pt\n\\catcode`\\^^A=14\n";
    const TOP: &str = "\\documentclass{article}\n\\usepackage{grp}\n\\begin{document}\nx\n\\end{document}\n";
    const GROUP: &str = "\\documentclass{article}\n\\begingroup\\usepackage{grp}\\endgroup\n\
                         \\ifdefined\\grplet\\errmessage{let leaked}\\fi\n\\ifdefined\\grpdef\\errmessage{def leaked}\\fi\n\
                         \\ifdefined\\grpglobal\\else\\errmessage{global lost}\\fi\n\
                         \\ifnum\\catcode`\\^^A=14 \\errmessage{catcode leaked}\\fi\n\
                         \\makeatletter\\ifdim\\dimen@=1pt \\errmessage{dimen leaked}\\fi\\makeatother\n\
                         \\begin{document}\nx\n\\end{document}\n";
    in_group_runs("tiny-group", "grp", STY, GROUP, &[TOP, GROUP]);
}

/// A package in a group writes a name globally, then locally. At the group's
/// end TeX restores the global value, not the one from before the load
/// (tex.web § 283: a save entry made after a global write holds that value).
/// Guards the open case of the write kind a replay takes from the save stack:
/// one local write would make the group's end restore the value from before
/// the load. Green so far (the case was not reached by this replay).
#[test]
fn a_global_then_local_write_in_a_group_load_leaves_the_global_value() {
    if !installed() {
        return;
    }
    const STY: &str = "\\ProvidesPackage{mix}\n\\gdef\\mixname{G}\n\\def\\mixname{L}\n";
    const GROUP: &str = "\\documentclass{article}\n\\begingroup\\usepackage{mix}\\endgroup\n\
                         \\def\\mixG{G}\\ifx\\mixname\\mixG\\else\\errmessage{global lost}\\fi\n\
                         \\begin{document}\nx\n\\end{document}\n";
    in_group_runs("mix-group", "mix", STY, GROUP, &[GROUP]);
}

/// Intuition: a kernel cache holds what the kernel's code did under the
/// configuration that read it, so another configuration never loads it. A
/// kernel built under `limits.choices: 1` first, then a run under the
/// default configuration over the same cache reads a kernel file of its
/// own and gives what a fresh default run gives. Pins the kernel cache keyed without its configuration: the
/// first test of a CI shard to build it decided what every later one
/// loaded (a cut l3fp loop took 5.96M steps there, 1.89M alone).
#[test]
fn a_kernel_built_under_another_configuration_is_never_loaded() {
    if !installed() {
        return;
    }
    // the l3fp loop a line budget cuts, as in `documents::cut_inside_library_call_drops_the_call`
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/documents/cut_inside_library_call/main.tex");
    let source = std::fs::read_to_string(fixture).expect("the fixture");
    let (shared, fresh) = (project("kernel-config-shared"), project("kernel-config-fresh"));
    let mut other = config(&shared);
    other.limits.choices = 1;
    let first = analyze(&shared, "\\relax\n", &other);
    let cut = |dir: &Path| Config {
        limits: satex::project::config::Limits { line_steps: 1_000_000, ..config(dir).limits },
        ..config(dir)
    };
    let after = analyze(&shared, &source, &cut(&shared));
    let alone = analyze(&fresh, &source, &cut(&fresh));
    let steps = |a: &Analysis| a.steps - a.load_steps;
    let codes = |a: &Analysis| a.facts.diagnostics.iter().map(|d| (d.code, d.span, d.count)).collect::<Vec<_>>();
    let kernel = |a: &Analysis| a.format.as_ref().and_then(|f| f.cache.clone());
    assert!(kernel(&first).is_some(), "the first run left no kernel cache");
    assert_ne!(kernel(&after), kernel(&first), "the other configuration's kernel was loaded");
    assert_eq!(steps(&after), steps(&alone));
    assert_eq!(codes(&after), codes(&alone));
    let _ = (std::fs::remove_dir_all(shared), std::fs::remove_dir_all(fresh));
}
