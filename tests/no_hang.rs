//! Every subcommand finishes on a copy of the sample paper (tests/fixtures/project) within a bounded time.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Far above what the sample needs, far below a hang.
const LIMIT: Duration = Duration::from_secs(120);
/// Interpreting latex.ltx once, which an unoptimized build needs minutes for.
const KERNEL_LIMIT: Duration = Duration::from_secs(900);

fn finishes(args: &[&str]) {
    finishes_within(args, LIMIT)
}

fn finishes_within(args: &[&str], limit: Duration) {
    let cache = std::env::temp_dir().join(format!("satex-no-hang-{}", std::process::id()));
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SATEX_CACHE", &cache)
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
    let file = ["-f", "tests/fixtures/project/paper.tex"];
    finishes_within(&[&["cache", "build"][..], &file[..]].concat(), KERNEL_LIMIT);
    let queries = String::from_utf8(
        Command::new(env!("CARGO_BIN_EXE_satex"))
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
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("satex-no-hang-{}", std::process::id())));
}
