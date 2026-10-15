//! satex-playground: builds the site and its playground from this tree and
//! serves it on localhost, as `doc/developing.md` describes step by step.
//!
//!   cargo playground [--port 8000] [--out target/site]
//!
//! It needs what the site's workflow has: the Rust target
//! `wasm32-unknown-emscripten`, the Emscripten SDK (`emcc` on the `PATH`, or
//! `EMSDK` naming its directory), Python 3, pandoc and pdftocairo.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("satex-playground: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (mut port, mut out) = ("8000".to_string(), PathBuf::from("target/site"));
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port = args.next().ok_or("--port needs a number")?,
            "--out" => out = args.next().ok_or("--out needs a directory")?.into(),
            _ => return Err(format!("unknown argument `{arg}`; usage: cargo playground [--port N] [--out DIR]")),
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = emscripten_path()?;
    // Both builds from one source and profile: a cache is keyed on the build that wrote it.
    step(Command::new("cargo").args(["build", "--release", "--bin", "satex"]).current_dir(root))?;
    step(
        Command::new("cargo")
            .args(["build", "--release", "--target", "wasm32-unknown-emscripten", "--bin", "satex"])
            .current_dir(root)
            .env("PATH", &path),
    )?;
    step(
        Command::new("python3")
            .arg("doc/site/build.py")
            .args(["--satex", "target/release/satex"])
            .args(["--wasm", "target/wasm32-unknown-emscripten/release", "--out"])
            .arg(&out)
            .current_dir(root),
    )?;
    let site = root.join(&out);
    println!("satex-playground: serving http://127.0.0.1:{port}/play/ (the site at http://127.0.0.1:{port}/); Ctrl+C stops");
    // It serves until Ctrl+C, which ends it as it should.
    let mut serve = Command::new("python3");
    serve.args(["-m", "http.server", "-b", "127.0.0.1", "-d"]).arg(&site).arg(&port);
    serve.status().map(|_| ()).map_err(|e| format!("cannot run python3: {e}"))
}

/// The `PATH` with Emscripten's `emcc` on it: as it is, or with `$EMSDK`'s
/// compiler in front.
fn emscripten_path() -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    if which::which("emcc").is_ok() {
        return Ok(path);
    }
    let emsdk = std::env::var("EMSDK").map_err(|_| {
        "no `emcc` on the PATH and no EMSDK set: install the Emscripten SDK (the version in \
         .github/workflows/pages.yml) and `source emsdk_env.sh`, or set EMSDK to its directory"
            .to_string()
    })?;
    let compiler = Path::new(&emsdk).join("upstream/emscripten");
    if !compiler.join("emcc").exists() {
        return Err(format!("{} has no emcc: run `emsdk install` and `emsdk activate` there", compiler.display()));
    }
    Ok(format!("{}:{emsdk}:{path}", compiler.display()))
}

fn step(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| format!("cannot run {:?}: {e}", command.get_program()))?;
    status.success().then_some(()).ok_or_else(|| format!("{:?} failed ({status})", command.get_program()))
}
