//! satex-playground: builds the site and its playground from this tree and
//! serves it on localhost, as `doc/developing.md` describes step by step.
//!
//!   cargo playground [--port 8000] [--out target/site] [--dev]
//!
//! `--dev` serves `doc/site`'s CSS, JS and the playground page straight from
//! the sources and reloads the open page when they change (CSS without a
//! reload). It reuses an existing build in `--out` and builds only when none
//! exists. The other pages and the wasm still need a rebuild.
//!
//! Without `--dev`, a last build in `--out` serves at once while the new one
//! builds beside it (`--out` with `.next`); the new one replaces it when done.
//!
//! The playground's Compile button posts to this server, which builds the
//! project with the machine's TeX (`src/cli/gui/compile.rs`). `satex gui`
//! serves the same pages on a project of this machine.
//!
//! It needs what the site's workflow has: Python 3, pandoc and pdftocairo,
//! git, and `rustup`. The Rust target `wasm32-unknown-emscripten` is added
//! when missing, and the Emscripten SDK the workflow pins is installed under
//! `target/emsdk` unless `emcc` is on the `PATH` or `EMSDK` names one.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use satex::cli::gui::{compile, http};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("playground: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (mut port, mut out, mut profile) = ("8000".to_string(), PathBuf::from("target/site"), "playground");
    let mut dev = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port = args.next().ok_or("--port needs a number")?,
            "--out" => out = args.next().ok_or("--out needs a directory")?.into(),
            // What the published site ships: slower to build, faster to run.
            "--release" => profile = "release",
            "--dev" => dev = true,
            _ => {
                return Err(format!(
                    "unknown argument `{arg}`. Usage: cargo playground [--port N] [--out DIR] [--release] [--dev]"
                ));
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dev = dev.then(|| root.join("doc/site"));
    take_over(&port);
    let port: u16 = port.parse().map_err(|_| format!("`{port}` is no port"))?;
    if dev.is_some() && root.join(&out).join("play/index.html").is_file() {
        let listener = http::bind(port)?;
        compile::warm(&compile::root(&root.join(&out)), &root.join("doc/site/examples"));
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        println!("playground: dev mode on the existing build in {} at http://127.0.0.1:{port}/play/", out.display());
        return serve(&root.join(&out), listener, dev);
    }
    let site = root.join(&out);
    let listener = http::bind(port)?;
    let url = format!("http://127.0.0.1:{}/play/", listener.local_addr().map_err(|e| e.to_string())?.port());
    // A last build serves at once; this one replaces it when done.
    if site.join("play/index.html").is_file() {
        println!("playground: serving the last build at {url} while rebuilding. Ctrl+C stops");
        let next = site.clone();
        std::thread::spawn(move || match build(root, &next, profile) {
            Ok(()) => {
                compile::warm(&compile::root(&next), &root.join("doc/site/examples"));
                say("built. Reload the page to load this build");
            }
            Err(message) => eprintln!("playground: {message}. The last build stays"),
        });
        return serve(&site, listener, dev);
    }
    build(root, &site, profile)?;
    compile::warm(&compile::root(&site), &root.join("doc/site/examples"));
    println!("playground: serving {url}. Ctrl+C stops");
    serve(&site, listener, dev)
}

/// Builds the site into `site.next`, then swaps it in for `site`.
fn build(root: &Path, site: &Path, profile: &str) -> Result<(), String> {
    say("[1/4] toolchain: wasm32-unknown-emscripten, Emscripten SDK");
    let wasm = wasm_cargo()?;
    let cargo = || {
        let mut c = Command::new(wasm.get_program());
        c.envs(wasm.get_envs().filter_map(|(k, v)| Some((k, v?)))).current_dir(root);
        c
    };
    let path = emscripten_path(root)?;
    // Where cargo puts its builds: `$CARGO_TARGET_DIR`, else `target`.
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    // One build at a time: two would share the site's work directory and its caches.
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let lock = std::fs::File::create(target.join("playground.lock")).map_err(|e| e.to_string())?;
    if lock.try_lock().is_err() {
        say("another playground build is running. Waiting for it to finish");
        lock.lock().map_err(|e| e.to_string())?;
    }
    // Both builds from one source and profile: a cache is keyed on the source digest. The native
    // one by the `cargo` running this, whose library build it reuses (rustup's `rustc` would redo it).
    say("[2/4] satex, native");
    step(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["build", "--profile", profile, "--bin", "satex"])
            .current_dir(root),
    )?;
    // The site's runs need only the native build; it reads the wasm last,
    // once `ready` names it built, so the two overlap.
    say("[3/4] satex, wasm, while [4/4] builds the site: pages, TeX files, caches");
    let wasm_dir = target.join("wasm32-unknown-emscripten").join(profile);
    let ready = target.join("playground-wasm.ready");
    let _ = std::fs::remove_file(&ready);
    let next = site.with_extension("next");
    let mut python = Command::new("python3")
        .arg("doc/site/build.py")
        .arg("--satex")
        .arg(target.join(profile).join("satex"))
        .arg("--wasm")
        .arg(&wasm_dir)
        // The edit loop: package listings outlive an interpreter change.
        .args(["--precompress", "--keep-listings", "--out"])
        .arg(&next)
        .arg("--work")
        .arg(site.with_file_name("site-work"))
        .env("SATEX_WASM_READY", &ready)
        .current_dir(root)
        .spawn()
        .map_err(|e| format!("cannot run python3: {e}"))?;
    // Quiet: its progress line would garble the site's; errors still show.
    let built = step(
        cargo()
            .args(["build", "-q", "--profile", profile, "--target", "wasm32-unknown-emscripten", "--bin", "satex"])
            .env("PATH", &path),
    );
    let _ = std::fs::write(&ready, if built.is_ok() { "ok" } else { "failed" });
    eprintln!("\r\x1b[Kplayground: [3/4] satex, wasm: {}", if built.is_ok() { "built" } else { "failed" });
    let status = python.wait().map_err(|e| e.to_string())?;
    built?;
    status.success().then_some(()).ok_or_else(|| format!("doc/site/build.py failed ({status})"))?;
    let old = site.with_extension("old");
    let _ = std::fs::remove_dir_all(&old);
    if site.exists() {
        std::fs::rename(site, &old).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&next, site).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Serves `site` on 127.0.0.1 the way a static host such as GitHub Pages
/// does: the type by extension, the `.gz` beside a file to a client that
/// takes gzip (`build.py --precompress`), and pages revalidated while
/// assets, whose names carry their content's hash, are kept for good.
fn serve(site: &Path, listener: std::net::TcpListener, dev: Option<PathBuf>) -> Result<(), String> {
    let site = site.canonicalize().map_err(|e| format!("{}: {e}", site.display()))?;
    for stream in listener.incoming().flatten() {
        let (site, dev) = (site.clone(), dev.clone());
        std::thread::spawn(move || {
            let _ = respond(stream, &site, dev.as_deref());
        });
    }
    Ok(())
}

fn respond(mut stream: std::net::TcpStream, site: &Path, dev: Option<&Path>) -> std::io::Result<()> {
    let req = http::Request::read(&stream)?;
    let (method, path) = (req.method.as_str(), req.path.as_str());
    // a newer server asks for the port ([`take_over`])
    if method == "POST" && path == "/__satex/quit" {
        http::status(&mut stream, "200 OK", "x-satex-playground: quit\r\n")?;
        let _ = std::io::Write::flush(&mut stream);
        std::process::exit(0);
    }
    if let Some(rest) = path.strip_prefix("/__satex/compile") {
        return compile::respond(&mut stream, method, rest, &req.body, &compile::root(site), None);
    }
    let Some(src) = dev else { return http::file(&mut stream, &req, site, |_, body| body) };
    if path == "/__dev/events" {
        return watch(stream, src);
    }
    let found = match path {
        "/__dev.js" => Some(("dev.js".into(), include_bytes!("../../doc/site/dev.js").to_vec())),
        // version `dev`: it asks the network first
        "/play/sw.js" => std::fs::read_to_string(src.join("sw.js"))
            .ok()
            .map(|w| ("sw.js".into(), w.replace("__VERSION__", "dev").into_bytes())),
        "/play/" | "/play/index.html" => http::page(site, Some(src), |_| ()).map(|p| ("index.html".into(), p)),
        _ => http::source(path, src),
    };
    match found {
        Some((name, body)) => http::send(&mut stream, method, &name, "no-cache", false, dev_inject(&name, body)),
        None => http::file(&mut stream, &req, site, dev_inject),
    }
}

/// Dev mode: a page gets the reload client.
fn dev_inject(name: &str, body: Vec<u8>) -> Vec<u8> {
    if !name.ends_with(".html") {
        return body;
    }
    String::from_utf8_lossy(&body).replace("</body>", "<script src=\"/__dev.js\"></script></body>").into_bytes()
}

/// Dev mode: the newest modification time under `dir`, for CSS and the rest.
fn stamps(dir: &Path) -> (std::time::SystemTime, std::time::SystemTime) {
    let (mut css, mut rest) = (std::time::UNIX_EPOCH, std::time::UNIX_EPOCH);
    for e in walkdir::WalkDir::new(dir).into_iter().flatten().filter(|e| e.file_type().is_file()) {
        let Some(t) = e.metadata().ok().and_then(|m| m.modified().ok()) else { continue };
        let slot = if e.path().extension().is_some_and(|x| x == "css") { &mut css } else { &mut rest };
        *slot = (*slot).max(t);
    }
    (css, rest)
}

/// Dev mode: an event stream that says `css` or `reload` when `dir` changes.
fn watch(mut stream: std::net::TcpStream, dir: &Path) -> std::io::Result<()> {
    use std::io::Write;
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\n\r\n: ok\n\n")?;
    let mut seen = stamps(dir);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let now = stamps(dir);
        let event = if now.1 != seen.1 {
            "data: reload\n\n"
        } else if now.0 != seen.0 {
            "data: css\n\n"
        } else {
            ": \n\n"
        };
        seen = now;
        // The idle comment also notices a closed page.
        stream.write_all(event.as_bytes())?;
        stream.flush()?;
    }
}

fn say(what: &str) {
    println!("playground: {what}");
}

/// Asks an older playground server on `port` to quit, so this one serves there.
fn take_over(port: &str) {
    use std::io::{Read, Write};
    let Ok(port) = port.parse::<u16>() else { return };
    let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) else { return };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let _ = stream.write_all(b"POST /__satex/quit HTTP/1.0\r\n\r\n");
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    if !reply.contains("x-satex-playground: quit") {
        return;
    }
    for _ in 0..40 {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            say(&format!("stopped the playground server that held port {port}"));
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// A `cargo` whose `rustc` has Rust's `wasm32-unknown-emscripten` standard
/// library: the one on the `PATH`, or else rustup's default toolchain's,
/// with the target added when missing (a distribution's `rustc` may come
/// first on the `PATH`, and a toolchain's `cargo` calls the first `rustc`).
fn wasm_cargo() -> Result<Command, String> {
    // `cargo run` names the `cargo` it is and the `rustc` it was told to use.
    let var = |name, default| std::env::var_os(name).unwrap_or_else(|| std::ffi::OsString::from(default));
    let libdir = Command::new(var("RUSTC", "rustc"))
        .args(["--print", "target-libdir", "--target", "wasm32-unknown-emscripten"])
        .output();
    if libdir.is_ok_and(|o| o.status.success() && Path::new(String::from_utf8_lossy(&o.stdout).trim()).is_dir()) {
        return Ok(Command::new(var("CARGO", "cargo")));
    }
    let rustup = |args: &[&str]| -> Result<String, String> {
        match Command::new("rustup").args(args).output() {
            Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()),
            Ok(out) => Err(format!("rustup {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim())),
            Err(e) => Err(format!("no rustc with the wasm32-unknown-emscripten target and no rustup: {e}")),
        }
    };
    let active = rustup(&["show", "active-toolchain"])?;
    let toolchain = active.split_whitespace().next().ok_or("rustup has no default toolchain")?.to_string();
    if !rustup(&["target", "list", "--installed", "--toolchain", &toolchain])?
        .lines()
        .any(|t| t == "wasm32-unknown-emscripten")
    {
        step(Command::new("rustup").args(["target", "add", "wasm32-unknown-emscripten", "--toolchain", &toolchain]))?;
    }
    let mut cargo = Command::new(rustup(&["which", "cargo", "--toolchain", &toolchain])?);
    cargo.env("RUSTC", rustup(&["which", "rustc", "--toolchain", &toolchain])?);
    Ok(cargo)
}

/// The `PATH` with Emscripten's `emcc` on it: as it is, with `$EMSDK`'s
/// compiler in front, or with the SDK the site's workflow pins, installed
/// under `target/emsdk` the first time.
fn emscripten_path(root: &Path) -> Result<String, String> {
    let path = std::env::var("PATH").unwrap_or_default();
    if satex::project::paths::which("emcc").is_ok() {
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
        println!("playground: installing the Emscripten SDK {version} into {}", emsdk.display());
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
