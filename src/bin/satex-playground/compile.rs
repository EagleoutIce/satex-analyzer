//! The playground's Compile button, on this machine's TeX: `POST
//! /__satex/compile` takes `{main, files, at?}`, writes the files into a
//! fresh directory beside the site and runs the native `satex compile` on
//! the main file, or on `satex slice --at` of it. The answer names the
//! pages (PNG, by `pdftocairo`), the log and the explained messages.
//! `GET /__satex/compile/ID/FILE` serves what a build wrote, `GET
//! /__satex/compile` says the server compiles. A static host has none of
//! this, so the page offers Compile only where the probe answers.
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

/// Builds kept for their pages and SyncTeX data: the newest few.
const KEEP: usize = 4;

pub fn respond(
    stream: &mut std::net::TcpStream,
    method: &str,
    rest: &str,
    body: &[u8],
    site: &Path,
) -> std::io::Result<()> {
    let root = site.with_file_name(format!("{}-compile", site.file_name().and_then(|n| n.to_str()).unwrap_or("site")));
    if method == "POST" && rest.is_empty() {
        // One build at a time: TeX runs are heavy, the page waits anyway.
        static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        let answer = build(&root, body).unwrap_or_else(|e| json!({ "error": e }));
        return super::send(stream, method, "answer.json", "no-cache", false, answer.to_string().into_bytes());
    }
    if rest.is_empty() {
        return super::send(stream, method, "probe.json", "no-cache", false, b"{\"compile\":true}".to_vec());
    }
    let file = root.join(rest.trim_start_matches('/')).canonicalize().ok();
    match file.filter(|f| root.canonicalize().is_ok_and(|r| f.starts_with(r)) && f.is_file()) {
        Some(f) => {
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            super::send(stream, method, &name, "no-cache", false, std::fs::read(&f)?)
        }
        None => std::io::Write::write_all(
            stream,
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ),
    }
}

/// One build of the request's project in a new directory under `root`.
fn build(root: &Path, body: &[u8]) -> Result<Value, String> {
    let request: Value = serde_json::from_slice(body).map_err(|e| format!("bad request: {e}"))?;
    let main = request["main"].as_str().filter(|m| safe(m)).ok_or("no main file")?;
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .to_string();
    prune(root);
    let dir = root.join(&id);
    for (name, text) in request["files"].as_object().into_iter().flatten() {
        let (Some(text), true) = (text.as_str(), safe(name)) else { continue };
        let file = dir.join(name);
        std::fs::create_dir_all(file.parent().unwrap_or(&dir)).map_err(|e| e.to_string())?;
        std::fs::write(&file, text).map_err(|e| e.to_string())?;
    }
    let satex = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("satex");
    // A slice is a project of its own: the main file cut down, the local files it needs.
    let work = match request["at"].as_str() {
        Some(at) => {
            let out =
                run(Command::new(&satex).args(["slice", "--at", at, "--out", "slice", "-f", main]).current_dir(&dir))?;
            if !dir.join("slice").join(main).is_file() {
                return Err(format!("the slice failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
            }
            dir.join("slice")
        }
        None => dir.clone(),
    };
    let out = run(Command::new(&satex).args(["--format", "json", "compile", "-f", main]).current_dir(&work))?;
    let mut records: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    for r in &mut records {
        if let Some(p) = r["path"].as_str() {
            let p = Path::new(p);
            r["path"] = json!(p.strip_prefix(&work).unwrap_or(p).to_string_lossy().trim_start_matches("./"));
        }
    }
    let stem = Path::new(main).with_extension("");
    let pdf = work.join(stem.with_extension("pdf"));
    if pdf.is_file() {
        run(Command::new("pdftocairo").args(["-png", "-r", "144"]).arg(&pdf).arg(work.join("page")))?;
    }
    let mut pages: Vec<String> = std::fs::read_dir(&work)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("page-") && n.ends_with(".png"))
        .collect();
    // `page-2` before `page-10`: pdftocairo pads to the widest number, so length first.
    pages.sort_by_key(|n| (n.len(), n.clone()));
    let base = work.strip_prefix(root).unwrap_or(&work).to_string_lossy().into_owned();
    let log = std::fs::read_to_string(work.join(stem.with_extension("log"))).unwrap_or_default();
    Ok(json!({
        "base": format!("{base}/"),
        "pdf": pdf.is_file().then(|| stem.with_extension("pdf")),
        "synctex": work.join(stem.with_extension("synctex.gz")).is_file().then(|| stem.with_extension("synctex.gz")),
        "pages": pages,
        "dpi": 144,
        "records": records,
        "log": log,
        "stderr": String::from_utf8_lossy(&out.stderr),
    }))
}

fn run(command: &mut Command) -> Result<std::process::Output, String> {
    command.output().map_err(|e| format!("cannot run {}: {e}", command.get_program().to_string_lossy()))
}

/// A relative path that stays inside the build directory.
fn safe(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty() && path.components().all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Drops all but the newest [`KEEP`] builds.
fn prune(root: &Path) {
    let mut builds: Vec<PathBuf> =
        std::fs::read_dir(root).into_iter().flatten().filter_map(|e| Some(e.ok()?.path())).collect();
    builds.sort();
    for old in builds.iter().rev().skip(KEEP - 1) {
        let _ = std::fs::remove_dir_all(old);
    }
}
