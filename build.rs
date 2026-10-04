//! Records when and from what this binary was built, for `satex --version`
//! and for the format cache, which is keyed on the interpreter's source.

use std::process::Command;

fn main() {
    lua();
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
const REPORTING: &[&str] = &[
    "src/lint",
    "src/cmd",
    "src/bin",
    "src/lsp",
    "src/render.rs",
    "src/main.rs",
    "src/progress.rs",
    "src/query.rs",
    "src/rename.rs",
    "src/timing.rs",
];

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
    for byte in files.iter().filter_map(|f| std::fs::read(f).ok()).flat_map(|text| code_only(&text)) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The bytes that make up the code: `//` comments and whitespace outside
/// string literals dropped, so a comment or a reformat keeps the caches.
fn code_only(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let (mut i, mut string) = (0, false);
    while i < text.len() {
        let c = text[i];
        if string {
            out.push(c);
            if c == b'\\' && i + 1 < text.len() {
                out.push(text[i + 1]);
                i += 1;
            } else if c == b'"' {
                string = false;
            }
        } else if c == b'/' && text.get(i + 1) == Some(&b'/') {
            while i < text.len() && text[i] != b'\n' {
                i += 1;
            }
            continue;
        } else if c == b'"' && !(i > 0 && text[i - 1] == b'\'' && text.get(i + 1) == Some(&b'\'')) {
            string = true;
            out.push(c);
        } else if !c.is_ascii_whitespace() {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Lua 5.3 with satex's unknown value made untestable (vendor/lua/SATEX.patch).
/// `mlua` runs in `module` mode and links this build, since a `[patch]` on a
/// Lua crate does not survive publishing.
fn lua() {
    let dir = std::path::Path::new("vendor/lua");
    let skip = ["lua.c", "luac.c"];
    let mut build = cc::Build::new();
    build.include(dir).warnings(false);
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target.as_str() {
        "macos" | "ios" => build.define("LUA_USE_MACOSX", None),
        // `mlua`'s `module` mode would declare Lua as a raw-dylib import and
        // duplicate these symbols; `.cargo/config.toml` names this library
        // instead, so nothing here has to force the link through.
        "windows" => &mut build,
        _ => build.define("LUA_USE_LINUX", None),
    };
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("vendor/lua")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "c"))
        .filter(|p| !skip.iter().any(|s| p.file_name().is_some_and(|n| n == *s)))
        .collect();
    files.sort();
    build.files(files).compile("lua53");
    println!("cargo:rerun-if-changed=vendor/lua");
}

/// LPeg, which LuaTeX builds in, compiled against the Lua 5.3 headers of
/// the Lua built above.
fn lpeg() {
    let dir = std::path::Path::new("vendor/lpeg");
    let files = ["lpcap.c", "lpcode.c", "lpcset.c", "lpprint.c", "lptree.c", "lpvm.c"];
    cc::Build::new().include(dir).files(files.iter().map(|f| dir.join(f))).warnings(false).compile("lpeg");
    println!("cargo:rerun-if-changed=vendor/lpeg");
}
