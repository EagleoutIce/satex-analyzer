//! Records when and from what this binary was built, for `satex --version`,
//! and the cache key's development guard.

use std::process::Command;

fn main() {
    lua();
    lpeg();
    // Windows gives the main thread 1 MiB; a cold kernel build nests deeper.
    // 8 MiB as on Linux (RUST_MIN_STACK covers only spawned threads).
    let cfg = |k: &str| std::env::var(k).unwrap_or_default();
    if cfg("CARGO_CFG_TARGET_OS") == "windows" && cfg("CARGO_CFG_TARGET_ENV") == "msvc" {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
    // The playground's status callback (src/lsp/wasm.rs).
    if std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "emscripten") {
        let lib = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lsp/wasm-status.js");
        println!("cargo:rustc-link-arg-bins=--js-library={}", lib.display());
    }
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
    // A working tree with changes is not that commit: say so.
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .is_some_and(|out| out.status.success() && !out.stdout.is_empty());
    let commit = if dirty && !commit.is_empty() { format!("{commit}+changes") } else { commit };
    println!("cargo:rustc-env=SATEX_COMMIT={commit}");
    // Caches are keyed on `format::SEMANTIC_VERSION` and the source digest:
    // two builds of one version (a release and a later `cargo install` of a
    // working tree) may differ in what a cache holds, and must not read or
    // overwrite each other's.
    let src = std::path::Path::new("src");
    let guard = format!("d{:016x}", source_digest(src, REPORTING, code_only));
    println!("cargo:rustc-env=SATEX_CACHE_GUARD={guard}");
    // Every byte of src: two binaries with one id come from the same source
    // (satex-doc refuses a `satex` built from other source).
    println!("cargo:rustc-env=SATEX_SOURCE_ID={:016x}", source_digest(src, &[], |text| text.to_vec()));
    println!("cargo:rerun-if-changed=src");
}

/// Code that only reports on an analysis — lints, subcommands, rendering —
/// does not change what a cache holds, so editing it keeps the caches.
const REPORTING: &[&str] = &[
    "src/linter",
    "src/cli",
    "src/bin",
    "src/lsp",
    "src/main.rs",
    "src/knowledge",
    "src/text.rs",
    "src/util/progress.rs",
    "src/util/timing.rs",
    "src/queries/query.rs",
    "src/queries/rename.rs",
    "src/data/describe.tsv",
    "src/data/lint-numbers.txt",
    "src/data/knowledge.md",
];

/// FNV-1a over the files under `root` outside `skip`, in path order, each mapped
/// by `keep`: with `REPORTING` and `code_only`, the binary and every test
/// binary built from the same interpreter share one cache.
fn source_digest(root: &std::path::Path, skip: &[&str], keep: fn(&[u8]) -> Vec<u8>) -> u64 {
    fn walk(dir: &std::path::Path, skip: &[&str], files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if skip.iter().any(|skip| path.starts_with(skip)) {
                continue;
            }
            if path.is_dir() {
                walk(&path, skip, files);
            } else {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(root, skip, &mut files);
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in files.iter().filter_map(|f| std::fs::read(f).ok()).flat_map(|text| keep(&text)) {
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
        // Lua's errors are setjmp/longjmp; in the browser build they become
        // wasm exceptions, as Rust's panics already are.
        "emscripten" => build.flag("-sSUPPORT_LONGJMP=wasm"),
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
    compile(build.files(files), "lua53", dir);
    println!("cargo:rerun-if-changed=vendor/lua");
}

/// LPeg, which LuaTeX builds in, compiled against the Lua 5.3 headers of
/// the Lua built above.
fn lpeg() {
    let dir = std::path::Path::new("vendor/lpeg");
    let files = ["lpcap.c", "lpcode.c", "lpcset.c", "lpprint.c", "lptree.c", "lpvm.c"];
    compile(cc::Build::new().include(dir).files(files.iter().map(|f| dir.join(f))).warnings(false), "lpeg", dir);
    println!("cargo:rerun-if-changed=vendor/lpeg");
}

/// `build.compile(name)`, skipped when `OUT_DIR` holds a build of the same
/// sources (every file in `dir`), build script and compiler settings: the
/// script reruns on every edit in `src`, and the C does not change then.
fn compile(build: &mut cc::Build, name: &str, dir: &std::path::Path) {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let target = std::env::var("TARGET").unwrap_or_default();
    // What `cc` reads from the environment (it watches these itself when it compiles).
    let mut vars: Vec<String> =
        ["CC_ENABLE_DEBUG_OUTPUT", "CC_FORCE_DISABLE", "CRATE_CC_NO_DEFAULTS", "OPT_LEVEL", "DEBUG", "PROFILE"]
            .map(String::from)
            .into();
    for base in ["AR", "ARFLAGS", "CC", "CFLAGS"] {
        vars.extend([base.into(), format!("HOST_{base}"), format!("TARGET_{base}"), format!("{base}_{target}")]);
        vars.push(format!("{base}_{}", target.replace('-', "_")));
    }
    let mut key = format!("{target}\n{}\n", std::fs::read_to_string("build.rs").unwrap_or_default());
    for var in &vars {
        key += &format!("{var}={:?}\n", std::env::var_os(var));
    }
    let stamp = out.join(format!("{name}.key"));
    let key = format!("{key}{:016x}", source_digest(dir, &[], |text| text.to_vec()));
    if std::fs::read_to_string(&stamp).is_ok_and(|old| old == key) && out.join(format!("lib{name}.a")).is_file() {
        for var in &vars {
            println!("cargo:rerun-if-env-changed={var}");
        }
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-lib=static={name}");
        return;
    }
    build.compile(name);
    let _ = std::fs::write(stamp, key);
}
