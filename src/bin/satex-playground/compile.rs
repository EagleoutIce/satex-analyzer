//! The playground's Compile button, on this machine's TeX: `POST
//! /__satex/compile` takes `{main, files, at?}`, writes the files into a
//! fresh directory beside the site and runs the native `satex compile` on
//! the main file, or on `satex slice --at` of it, with the page's settings
//! (`sets`: `KEY=VALUE`, each a `--set`). The answer names the
//! pages (PNG, by `pdftocairo`), the words on them (`pdftotext -bbox`), its
//! objects with the link annotations (`qpdf --json`, parsed by the page), the
//! log, the explained messages and every command run. A file is text or `{"b64": ...}` (an
//! image). The same request again answers from the kept build, and with
//! `cached` only a kept build answers.
//! `GET /__satex/compile/ID/FILE` serves what a build wrote, `GET
//! /__satex/compile` says the server compiles. A static host has none of
//! this, so the page offers Compile only where the probe answers.
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

/// Builds kept for their pages and SyncTeX data: the newest few.
const KEEP: usize = 4;

/// One build at a time: TeX runs are heavy, the page waits anyway.
static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Whether the caches of a fresh `satex` are still being built.
static WARMING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Compiles every example once in the background, so the first Compile
/// finds the native caches of this `satex` built: a stale cache costs a
/// minute, a warm one a few seconds.
pub fn warm(site: &Path, examples: &Path) {
    let (root, examples) = (compile_root(site), examples.to_path_buf());
    WARMING.store(true, std::sync::atomic::Ordering::Relaxed);
    std::thread::spawn(move || {
        let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = root.join("warm");
        let _ = std::fs::remove_dir_all(&dir);
        let copied = std::fs::create_dir_all(&dir).is_ok()
            && std::fs::read_dir(&examples)
                .into_iter()
                .flatten()
                .flatten()
                .all(|e| std::fs::copy(e.path(), dir.join(e.file_name())).is_ok());
        let satex = std::env::current_exe().map(|e| e.with_file_name("satex"));
        if let (true, Ok(satex)) = (copied, satex) {
            for main in mains(&dir) {
                let _ =
                    Command::new(&satex).args(["--format", "json", "compile", "-f", &main]).current_dir(&dir).output();
            }
        }
        WARMING.store(false, std::sync::atomic::Ordering::Relaxed);
    });
}

/// The root documents among `dir`'s `.tex` files: those with a `\documentclass`.
fn mains(dir: &Path) -> Vec<String> {
    let mut mains: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| std::fs::read_to_string(e.path()).is_ok_and(|t| t.contains("\\documentclass")))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".tex"))
        .collect();
    mains.sort();
    mains
}

fn compile_root(site: &Path) -> PathBuf {
    site.with_file_name(format!("{}-compile", site.file_name().and_then(|n| n.to_str()).unwrap_or("site")))
}

pub fn respond(
    stream: &mut std::net::TcpStream,
    method: &str,
    rest: &str,
    body: &[u8],
    site: &Path,
) -> std::io::Result<()> {
    let root = compile_root(site);
    if method == "POST" && rest.is_empty() {
        let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        let mut runs = Vec::new();
        let answer = build(&root, body, &mut runs).unwrap_or_else(|e| json!({ "error": e, "commands": runs }));
        return super::send(stream, method, "answer.json", "no-cache", false, answer.to_string().into_bytes());
    }
    if rest.is_empty() {
        let warming = WARMING.load(std::sync::atomic::Ordering::Relaxed);
        let probe = json!({ "compile": true, "warming": warming });
        return super::send(stream, method, "probe.json", "no-cache", false, probe.to_string().into_bytes());
    }
    // `ID.zip`, `ID/slice.zip`: every file of that build, sources as compiled included
    if let Some(build) = rest.strip_suffix(".zip") {
        let dir = root.join(build.trim_start_matches('/')).canonicalize().ok();
        if let Some(dir) = dir.filter(|d| root.canonicalize().is_ok_and(|r| d.starts_with(&r) && *d != r) && d.is_dir())
        {
            let name = format!("{}.zip", dir.file_name().and_then(|n| n.to_str()).unwrap_or("build"));
            return super::send(stream, method, &name, "no-cache", false, zip(&dir)?);
        }
    }
    let file = root.join(rest.trim_start_matches('/')).canonicalize().ok();
    match file.filter(|f| root.canonicalize().is_ok_and(|r| f.starts_with(r)) && f.is_file()) {
        Some(f) => {
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let bytes = std::fs::read(&f)?;
            // a text file of the build (.aux, .log, .fls) shows in the browser, not as a download
            let text = super::content_type(&name) == "application/octet-stream" && std::str::from_utf8(&bytes).is_ok();
            super::send(stream, method, &if text { format!("{name}.txt") } else { name }, "no-cache", false, bytes)
        }
        None => std::io::Write::write_all(
            stream,
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ),
    }
}

/// One build of the request's project in a new directory under `root`.
/// Every command it runs goes into `runs`, the page shows them.
fn build(root: &Path, body: &[u8], runs: &mut Vec<Value>) -> Result<Value, String> {
    let request: Value = serde_json::from_slice(body).map_err(|e| format!("bad request: {e}"))?;
    let main = request["main"].as_str().filter(|m| safe(m)).ok_or("no main file")?;
    let key = request_key(&request);
    if let Some(kept) = kept(root, &key) {
        return Ok(kept);
    }
    if request["cached"].as_bool() == Some(true) {
        return Ok(json!({ "none": true }));
    }
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .to_string();
    prune(root);
    let dir = root.join(&id);
    for (name, text) in request["files"].as_object().into_iter().flatten() {
        let bytes = text.as_str().map(|t| t.as_bytes().to_vec()).or_else(|| Some(unb64(text["b64"].as_str()?)));
        let (Some(bytes), true) = (bytes, safe(name)) else { continue };
        let file = dir.join(name);
        std::fs::create_dir_all(file.parent().unwrap_or(&dir)).map_err(|e| e.to_string())?;
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    }
    let satex = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("satex");
    // The page's settings: `--set KEY=VALUE` pairs only.
    let sets: Vec<&str> = request["sets"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let flags: Vec<&str> = sets.iter().flat_map(|s| ["--set", s]).collect();
    // A slice is a project of its own: the main file cut down, the local files it needs.
    let work = match request["at"].as_str() {
        Some(at) => {
            let out = run(
                runs,
                Command::new(&satex)
                    .args(&flags)
                    .args(["slice", "--at", at, "--out", "slice", "-f", main])
                    .current_dir(&dir),
            )?;
            if !out.status.success() || !dir.join("slice").join(main).is_file() {
                return Err(format!("satex slice --at {at} wrote no {main} ({}): {}", out.status, said(&out)));
            }
            dir.join("slice")
        }
        None => dir.clone(),
    };
    let out = run(
        runs,
        Command::new(&satex).args(&flags).args(["--format", "json", "compile", "-f", main]).current_dir(&work),
    )?;
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
        run(
            runs,
            Command::new("pdftocairo").args(["-png", "-r", "144"]).arg(&pdf).arg(work.join("page")).current_dir(&work),
        )?;
        run(runs, Command::new("pdftotext").arg("-bbox").arg(&pdf).arg(work.join("words.html")).current_dir(&work))?;
        // without qpdf the pages only lack their links. It exits 3 on mere warnings
        let _ = run(
            runs,
            Command::new("qpdf")
                .args(["--json=2", "--json-key=pages", "--json-key=qpdf"])
                .arg(&pdf)
                .arg(work.join("links.json"))
                .current_dir(&work),
        );
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
    // What the build wrote beside the sources: the page shows each, the logs (BibTeX's too) as text.
    let sent: std::collections::HashSet<&str> =
        request["files"].as_object().into_iter().flatten().map(|(n, _)| n.as_str()).collect();
    let mut outputs = Vec::new();
    let mut logs = serde_json::Map::new();
    for entry in walkdir(&work) {
        let Ok(name) = entry.strip_prefix(&work).map(|n| n.to_string_lossy().into_owned()) else { continue };
        if ours(&name) || sent.contains(name.as_str()) || name.starts_with("slice/") {
            continue;
        }
        if [".log", ".blg", ".ilg"].iter().any(|e| name.ends_with(e)) {
            logs.insert(name.clone(), json!(String::from_utf8_lossy(&std::fs::read(&entry).unwrap_or_default())));
        }
        outputs.push(json!({ "name": name, "size": entry.metadata().map(|m| m.len()).unwrap_or(0) }));
    }
    let answer = json!({
        "key": key,
        "base": format!("{base}/"),
        "pdf": pdf.is_file().then(|| stem.with_extension("pdf")),
        "synctex": work.join(stem.with_extension("synctex.gz")).is_file().then(|| stem.with_extension("synctex.gz")),
        "pages": pages,
        "words": work.join("words.html").is_file().then_some("words.html"),
        "links": work.join("links.json").is_file().then_some("links.json"),
        "dpi": 144,
        "records": records,
        "log": log,
        "stderr": String::from_utf8_lossy(&out.stderr),
        "commands": runs,
        "outputs": outputs,
        "logs": logs,
    });
    std::fs::write(dir.join("answer.json"), answer.to_string()).map_err(|e| e.to_string())?;
    Ok(answer)
}

/// What a request determines: its main file, slice point and files.
fn request_key(request: &Value) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for part in [&request["main"], &request["at"], &request["files"], &request["sets"]] {
        part.to_string().hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// The answer of a kept build with this key.
fn kept(root: &Path, key: &str) -> Option<Value> {
    std::fs::read_dir(root).ok()?.find_map(|d| {
        let mut answer: Value =
            serde_json::from_slice(&std::fs::read(d.ok()?.path().join("answer.json")).ok()?).ok()?;
        (answer["key"] == key).then(|| {
            answer["cached"] = json!(true);
            answer
        })
    })
}

/// Bytes of base64 text, skipping what is not in the alphabet.
fn unb64(text: &str) -> Vec<u8> {
    let (mut out, mut bits, mut have) = (Vec::new(), 0u32, 0);
    for c in text.bytes() {
        let Some(v) = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".iter().position(|&a| a == c)
        else {
            continue;
        };
        (bits, have) = (bits << 6 | v as u32, have + 6);
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
            bits &= (1 << have) - 1;
        }
    }
    out
}

/// Runs `command`, noted in `runs` as the page shows it: its words, paths
/// inside the build directory relative, exit status and time.
fn run(runs: &mut Vec<Value>, command: &mut Command) -> Result<std::process::Output, String> {
    let dir = command.get_current_dir().map(Path::to_path_buf).unwrap_or_default();
    let word = |w: &std::ffi::OsStr| {
        let p = Path::new(w);
        let p = p.strip_prefix(&dir).unwrap_or(p);
        // the native satex beside this server is plain `satex` to the reader
        let w = if p.file_name() == Some("satex".as_ref()) { "satex".into() } else { p.to_string_lossy() };
        if w.contains([' ', '\\']) { format!("'{w}'") } else { w.into_owned() }
    };
    let line: Vec<String> = std::iter::once(command.get_program()).chain(command.get_args()).map(word).collect();
    let start = std::time::Instant::now();
    let out = command.output();
    runs.push(json!({
        "command": line.join(" "),
        "status": out.as_ref().ok().and_then(|o| o.status.code()),
        "ms": start.elapsed().as_millis() as u64,
        "error": out.as_ref().err().map(ToString::to_string),
    }));
    out.map_err(|e| format!("cannot run {}: {e}", command.get_program().to_string_lossy()))
}

/// What a run printed, stderr then stdout, for an error message.
fn said(out: &std::process::Output) -> String {
    let text = [&out.stderr, &out.stdout].map(|b| String::from_utf8_lossy(b).trim().to_string());
    let text = text.into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n");
    if text.is_empty() { "it printed nothing".into() } else { text }
}

/// A relative path that stays inside the build directory.
fn safe(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty() && path.components().all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// The page's own files beside a build: its page images, words, links and answer.
fn ours(name: &str) -> bool {
    name.starts_with("page-") && name.ends_with(".png") || ["words.html", "links.json", "answer.json"].contains(&name)
}

/// The build in `dir` as a zip (stored, not compressed: the PDF and
/// `.synctex.gz` are compressed already), the page's own files and a slice's
/// directory left out.
fn zip(dir: &Path) -> std::io::Result<Vec<u8>> {
    let crc = |bytes: &[u8]| {
        !bytes
            .iter()
            .fold(!0u32, |c, &b| (0..8).fold(c ^ u32::from(b), |c, _| c >> 1 ^ (0xEDB8_8320 & (c & 1).wrapping_neg())))
    };
    let (mut out, mut central, mut count) = (Vec::new(), Vec::new(), 0u16);
    for path in walkdir(dir) {
        let Ok(name) = path.strip_prefix(dir).map(|n| n.to_string_lossy().replace('\\', "/")) else { continue };
        if ours(&name) || name.starts_with("slice/") {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let (sum, size, at) = (crc(&bytes), bytes.len() as u32, out.len() as u32);
        // version 2.0, no flags, stored, no time; the sizes, the name
        let common = |head: &mut Vec<u8>| {
            for v in [20u16, 0, 0, 0, 0x21] {
                head.extend(v.to_le_bytes());
            }
            for v in [sum, size, size] {
                head.extend(v.to_le_bytes());
            }
            head.extend((name.len() as u16).to_le_bytes());
            head.extend(0u16.to_le_bytes());
        };
        out.extend(0x0403_4b50u32.to_le_bytes());
        common(&mut out);
        out.extend(name.as_bytes());
        out.extend(&bytes);
        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend(20u16.to_le_bytes());
        common(&mut central);
        // no comment, disk 0, no attributes; where its header stands
        central.extend([0u8; 10]);
        central.extend(at.to_le_bytes());
        central.extend(name.as_bytes());
        count += 1;
    }
    let (start, len) = (out.len() as u32, central.len() as u32);
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    for v in [0u16, 0, count, count] {
        out.extend(v.to_le_bytes());
    }
    out.extend(len.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Ok(out)
}

/// Every file under `dir`, sorted.
fn walkdir(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() { out.extend(walkdir(&path)) } else { out.push(path) }
    }
    out.sort();
    out
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
