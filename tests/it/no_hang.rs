//! Every subcommand finishes on a copy of the sample paper (tests/fixtures/project) within a bounded time.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// Far above what the sample needs, far below a hang.
const LIMIT: Duration = Duration::from_secs(120);
/// Interpreting latex.ltx once, which an unoptimized build needs minutes for.
const KERNEL_LIMIT: Duration = Duration::from_secs(900);

fn finishes(args: &[&str]) {
    finishes_within(args, LIMIT)
}

fn finishes_within(args: &[&str], limit: Duration) {
    let cache = crate::support::tmp_root().join(format!("satex-no-hang-{}", std::process::id()));
    finishes_in(Path::new(env!("CARGO_MANIFEST_DIR")), &cache, args, limit)
}

/// `satex args` in `dir` on the caches in `cache` ends within `limit`.
fn finishes_in(dir: &Path, cache: &Path, args: &[&str], limit: Duration) {
    let start = Instant::now();
    let mut child = crate::support::satex()
        .args(args)
        .current_dir(dir)
        .env("SATEX_CACHE", cache)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("satex starts");
    loop {
        if child.try_wait().expect("satex runs").is_some() {
            return;
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            panic!("`satex {}` did not finish within {limit:?}", args.join(" "));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn no_subcommand_hangs() {
    let file = ["-f", "tests/fixtures/project/paper.tex", "--set", "limits.seconds=0"];
    finishes_within(&[&["cache", "build"][..], &file[..]].concat(), KERNEL_LIMIT);
    let queries = String::from_utf8(
        crate::support::satex()
            .args(["query", "--list", "--format", "csv"])
            .output()
            .expect("satex lists its queries")
            .stdout,
    )
    .unwrap_or_default();
    for name in queries.lines().skip(1).filter_map(|line| line.split(',').next()).filter(|n| !n.is_empty()) {
        finishes(&[&["query", name][..], &file[..]].concat());
    }
    for args in [
        &["lint"][..],
        &["summary"],
        &["scope"],
        &["explain", "highlight"],
        &["slice", "highlight"],
        &["controls"],
        &["dependencies"],
        &["trace"],
        &["cache"],
        &["tokens"],
    ] {
        finishes(&[args, &file[..]].concat());
    }
    let _ = std::fs::remove_dir_all(crate::support::tmp_root().join(format!("satex-no-hang-{}", std::process::id())));
}

/// chemfig.sty runs `\input chemfig.tex`, and a document named `chemfig.tex`
/// (as the playground names its probe for the package's manual) is found
/// first: the document reads itself inside the package, whose load the job
/// then ends in. The first build run leaves that load's cache unstored and
/// its build lock held by the run's state. The second run took the same
/// lock again, and a lock is per open file, so the process waited on itself
/// forever. Skipped where chemfig is not installed.
#[test]
fn a_package_that_reads_the_document_again_finishes() {
    let Ok(out) = std::process::Command::new("kpsewhich").arg("chemfig.sty").output() else { return };
    if out.stdout.is_empty() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-no-hang-self-input-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a project directory");
    std::fs::write(
        dir.join("chemfig.tex"),
        "\\documentclass{article}\n\\usepackage{chemfig}\n\\begin{document}\nText.\n\\end{document}\n",
    )
    .expect("the document");
    // Without its cache the first run records chemfig and takes its build lock.
    let cache = crate::support::test_cache();
    for entry in std::fs::read_dir(&cache).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with("package-chemfig-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    finishes_in(&dir, &cache, &["lint", "-f", "chemfig.tex", "--set", "cache.document=false"], LIMIT);
    let _ = std::fs::remove_dir_all(&dir);
}
