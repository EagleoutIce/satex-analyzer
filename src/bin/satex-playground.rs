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
    say("1/4 the toolchain: Rust's wasm32-unknown-emscripten and the Emscripten SDK");
    wasm_target()?;
    let path = emscripten_path(root)?;
    // Both builds from one source and profile: a cache is keyed on the build that wrote it.
    say("2/4 satex for this machine");
    step(Command::new("cargo").args(["build", "--release", "--bin", "satex"]).current_dir(root))?;
    say("3/4 satex for the browser");
    step(
        Command::new("cargo")
            .args(["build", "--release", "--target", "wasm32-unknown-emscripten", "--bin", "satex"])
            .current_dir(root)
            .env("PATH", &path),
    )?;
    say("4/4 the site: the pages, the TeX files and caches the playground loads");
    step(
        Command::new("python3")
            .arg("doc/site/build.py")
            .args(["--satex", "target/release/satex"])
            .args(["--wasm", "target/wasm32-unknown-emscripten/release", "--precompress", "--out"])
            .arg(&out)
            .current_dir(root),
    )?;
    let site = root.join(&out);
    let port = free_port(&port)?;
    println!(
        "satex-playground: serving http://127.0.0.1:{port}/play/ (the site at http://127.0.0.1:{port}/); Ctrl+C stops"
    );
    serve(&site, port.parse().map_err(|_| "no port".to_string())?)
}

/// Serves `site` on 127.0.0.1 the way a static host such as GitHub Pages
/// does: the type by extension, the `.gz` beside a file to a client that
/// takes gzip (`build.py --precompress`), and pages revalidated while
/// assets, whose names carry their content's hash, are kept for good.
fn serve(site: &Path, port: u16) -> Result<(), String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    let site = site.canonicalize().map_err(|e| format!("{}: {e}", site.display()))?;
    for stream in listener.incoming().flatten() {
        let site = site.clone();
        std::thread::spawn(move || {
            let _ = respond(stream, &site);
        });
    }
    Ok(())
}

fn respond(mut stream: std::net::TcpStream, site: &Path) -> std::io::Result<()> {
    use std::io::{BufRead, Write};
    let mut reader = std::io::BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut gzip = false;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? <= 2 {
            break;
        }
        let line = line.to_ascii_lowercase();
        gzip |= line.starts_with("accept-encoding:") && line.contains("gzip");
    }
    let mut words = request.split_whitespace();
    let (method, target) = (words.next().unwrap_or_default(), words.next().unwrap_or("/"));
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let path = percent_encoding::percent_decode_str(path).decode_utf8_lossy().into_owned();
    let mut file = site.join(path.trim_start_matches('/'));
    if file.is_dir() && !path.ends_with('/') {
        let head = format!(
            "HTTP/1.1 301 Moved Permanently\r\nLocation: {path}/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        return stream.write_all(head.as_bytes());
    }
    if file.is_dir() {
        file.push("index.html");
    }
    // Nothing outside the site, whatever the path spells.
    let Some(file) = file.canonicalize().ok().filter(|f| f.starts_with(site) && f.is_file()) else {
        let head = "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: 10\r\nConnection: close\r\n\r\nnot found\n";
        return stream.write_all(head.as_bytes());
    };
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    let zipped = file.with_file_name(format!("{name}.gz"));
    let encoded = gzip && zipped.is_file();
    let body = std::fs::read(if encoded { &zipped } else { &file })?;
    let cache = cache_control(&name);
    let mut head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: {cache}\r\nVary: Accept-Encoding\r\nConnection: close\r\n",
        content_type(&name),
        body.len(),
    );
    if encoded {
        head.push_str("Content-Encoding: gzip\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if method != "HEAD" {
        stream.write_all(&body)?;
    }
    stream.flush()
}

/// How long a browser may keep a file: for good when its name carries its
/// content's hash (`site-d8283f8c8028.js`, as `build.py` names assets), else
/// asking again each time, so a rebuilt page shows at once.
fn cache_control(name: &str) -> &'static str {
    let hashed = name
        .split('.')
        .next()
        .and_then(|stem| stem.rsplit_once('-'))
        .is_some_and(|(_, h)| h.len() == 12 && h.bytes().all(|b| b.is_ascii_hexdigit()));
    if hashed { "public, max-age=31536000, immutable" } else { "no-cache" }
}

fn say(what: &str) {
    println!("satex-playground: {what}");
}

/// The media type a static host gives a file by its extension.
fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "gz" => "application/gzip",
        "woff2" => "font/woff2",
        "pdf" => "application/pdf",
        "txt" | "tex" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashed_assets_are_kept_and_pages_asked_again() {
        assert_eq!(cache_control("site-d8283f8c8028.js"), "public, max-age=31536000, immutable");
        assert_eq!(cache_control("satex-aa1e200b2fdc.wasm"), "public, max-age=31536000, immutable");
        assert_eq!(cache_control("index.html"), "no-cache");
        assert_eq!(cache_control("guide.html"), "no-cache");
    }

    #[test]
    fn the_wasm_is_served_as_wasm() {
        assert_eq!(content_type("satex-aa1e200b2fdc.wasm"), "application/wasm");
        assert_eq!(content_type("pdftex-e876c77dca82.bin.gz"), "application/gzip");
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
    }
}
