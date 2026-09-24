//! Records when and from what this binary was built, for `satex --version`
//! and for the format cache, which is keyed on the interpreter's source.

use std::process::Command;

fn main() {
    lpeg();
    // `date` is the one clock available without pulling a crate into the
    // build script; the epoch is the fallback where it is not.
    let stamp = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default()
        });
    println!("cargo:rustc-env=SATEX_BUILD_TIME={stamp}");
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=SATEX_COMMIT={commit}");
    println!("cargo:rustc-env=SATEX_SOURCE_DIGEST={:016x}", source_digest());
    println!("cargo:rerun-if-changed=src");
}

/// Code that only reports on an analysis — lints, subcommands, rendering —
/// does not change what a cache holds, so editing it keeps the caches.
const REPORTING: &[&str] = &["src/lint", "src/cmd", "src/bin", "src/render.rs", "src/main.rs"];

/// FNV-1a over the interpreter's source files, in path order: the binary and
/// every test binary built from the same source share one format cache.
fn source_digest() -> u64 {
    fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if REPORTING.iter().any(|skip| path.starts_with(skip)) {
                continue;
            }
            if path.is_dir() {
                walk(&path, files);
            } else {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(std::path::Path::new("src"), &mut files);
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in files.iter().filter_map(|f| std::fs::read(f).ok()).flatten() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// LPeg, which LuaTeX builds in, compiled against the Lua 5.3 headers of
/// the vendored Lua `mlua` links.
fn lpeg() {
    let dir = std::path::Path::new("vendor/lpeg");
    let files = ["lpcap.c", "lpcode.c", "lpcset.c", "lpprint.c", "lptree.c", "lpvm.c"];
    cc::Build::new()
        .include(dir)
        .files(files.iter().map(|f| dir.join(f)))
        .warnings(false)
        .compile("lpeg");
    println!("cargo:rerun-if-changed=vendor/lpeg");
}
