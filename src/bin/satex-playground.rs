//! satex-playground: builds the site and its playground from this tree and
//! serves it on localhost, as `doc/developing.md` describes step by step.
//!
//!   cargo playground [--port 8000] [--out target/site]
//!
//! It needs what the site's workflow has: Python 3, pandoc and pdftocairo,
//! git, and `rustup`. The Rust target `wasm32-unknown-emscripten` is added
//! when missing, and the Emscripten SDK the workflow pins is installed under
//! `target/emsdk` unless `emcc` is on the `PATH` or `EMSDK` names one.

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
    wasm_target()?;
    let path = emscripten_path(root)?;
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
    let port = free_port(&port)?;
    println!(
        "satex-playground: serving http://127.0.0.1:{port}/play/ (the site at http://127.0.0.1:{port}/); Ctrl+C stops"
    );
    // It serves until Ctrl+C, which ends it as it should.
    let mut serve = Command::new("python3");
    serve.args(["-m", "http.server", "-b", "127.0.0.1", "-d"]).arg(&site).arg(&port);
    serve.status().map(|_| ()).map_err(|e| format!("cannot run python3: {e}"))
}

/// `port`, or the next one up that nothing listens on.
fn free_port(port: &str) -> Result<String, String> {
    let first: u16 = port.parse().map_err(|_| format!("`{port}` is no port"))?;
    (first..first.saturating_add(100))
        .find(|p| std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok())
        .map(|p| p.to_string())
        .ok_or_else(|| format!("no free port from {first} on"))
}

/// Rust's `wasm32-unknown-emscripten` standard library, added when missing.
fn wasm_target() -> Result<(), String> {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map_err(|e| format!("cannot run rustup: {e}"))?;
    if String::from_utf8_lossy(&installed.stdout).lines().any(|t| t.trim() == "wasm32-unknown-emscripten") {
        return Ok(());
    }
    step(Command::new("rustup").args(["target", "add", "wasm32-unknown-emscripten"]))
}

/// The `PATH` with Emscripten's `emcc` on it: as it is, with `$EMSDK`'s
/// compiler in front, or with the SDK the site's workflow pins, installed
/// under `target/emsdk` the first time.
fn emscripten_path(root: &Path) -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    if which::which("emcc").is_ok() {
        return Ok(path);
    }
    let emsdk = match std::env::var("EMSDK") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => install_emsdk(root)?,
    };
    let compiler = emsdk.join("upstream/emscripten");
    if !compiler.join("emcc").exists() {
        return Err(format!("{} has no emcc: run `emsdk install` and `emsdk activate` there", compiler.display()));
    }
    Ok(format!("{}:{}:{path}", compiler.display(), emsdk.display()))
}

/// The Emscripten SDK of `.github/workflows/pages.yml`, in `target/emsdk`.
fn install_emsdk(root: &Path) -> Result<PathBuf, String> {
    let workflow = std::fs::read_to_string(root.join(".github/workflows/pages.yml")).map_err(|e| e.to_string())?;
    let version = workflow
        .split("setup-emsdk")
        .nth(1)
        .and_then(|rest| rest.lines().find_map(|line| line.trim().strip_prefix("version:")))
        .map(str::trim)
        .ok_or("the site's workflow names no Emscripten version")?
        .to_string();
    let emsdk = root.join("target/emsdk");
    if !emsdk.join("emsdk").exists() {
        println!("satex-playground: installing the Emscripten SDK {version} into {}", emsdk.display());
        step(
            Command::new("git")
                .args(["clone", "--depth", "1", "https://github.com/emscripten-core/emsdk.git"])
                .arg(&emsdk),
        )?;
    }
    if !emsdk.join("upstream/emscripten/emcc").exists() {
        step(Command::new("./emsdk").args(["install", &version]).current_dir(&emsdk))?;
        step(Command::new("./emsdk").args(["activate", &version]).current_dir(&emsdk))?;
    }
    Ok(emsdk)
}

fn step(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| format!("cannot run {:?}: {e}", command.get_program()))?;
    status.success().then_some(()).ok_or_else(|| format!("{:?} failed ({status})", command.get_program()))
}
