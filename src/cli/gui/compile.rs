//! The playground's Compile button, on this machine's TeX: `POST
//! /__satex/compile` takes `{main, files, at?, pages?}`, writes the files into a
//! fresh directory beside the site and runs the native `satex compile` on
//! the main file, or on `satex slice --at` of it, with the page's settings
//! (`sets`: `KEY=VALUE`, each a `--set`). The answer names the
//! pages (PNG, by `pdftocairo`, none with `pages: false`), the words on them (`pdftotext -bbox`), its
//! objects with the link annotations (`qpdf --json`, parsed by the page), the
//! log, the explained messages and every command run. A file is text or `{"b64": ...}` (an
//! image). The same request again answers from the kept build, and with
//! `cached` only a kept build answers.
//! `GET /__satex/compile/ID/FILE` serves what a build wrote, `GET
//! /__satex/compile` says the server compiles. A static host has none of
//! this, so the page offers Compile only where the probe answers.
//! Under `satex gui` a build starts from a copy of the project's directory
//! (`base`), the page's texts laid over it.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering::Relaxed;

use serde_json::{Value, json};

use super::http::{content_type, send};

/// Builds kept for their pages and SyncTeX data: the newest few.
const KEEP: usize = 4;

/// One build at a time: TeX runs are heavy, the page waits anyway.
static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Whether the caches of a fresh `satex` are still being built.
static WARMING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Builds the page asked for that wait for [`ONE`]: the warm-up yields to them.
static WANTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Counts requests that supersede the build under way (another root): a build stops once it moved on.
static SUPERSEDED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Compiles every example once in the background, so the first Compile
/// finds the native caches of this `satex` built: a stale cache costs a
/// minute, a warm one a few seconds.
pub fn warm(root: &Path, examples: &Path) {
    let (root, examples) = (root.to_path_buf(), examples.to_path_buf());
    WARMING.store(true, std::sync::atomic::Ordering::Relaxed);
    std::thread::spawn(move || {
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
            // Example by example, each stopped and taken again later when the page asks for a build.
            let mut todo: std::collections::VecDeque<String> = mains(&dir).into();
            while let Some(main) = todo.pop_front() {
                while WANTED.load(Relaxed) > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
                let mut command = Command::new(&satex);
                command.args(["--format", "json", "compile", "-f", &main]).current_dir(&dir);
                let wanted = || (WANTED.load(Relaxed) > 0).then(|| "a build of the page comes first".to_string());
                if output_unless(&mut command, wanted).is_err() {
                    todo.push_back(main);
                }
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

/// Where the builds of the site `site` go: beside it.
pub fn root(site: &Path) -> PathBuf {
    site.with_file_name(format!("{}-compile", site.file_name().and_then(|n| n.to_str()).unwrap_or("site")))
}

pub fn respond(
    stream: &mut std::net::TcpStream,
    method: &str,
    rest: &str,
    body: &[u8],
    root: &Path,
    base: Option<&Path>,
) -> std::io::Result<()> {
    let root = root.to_path_buf();
    // `POST /__satex/compile/render`: a PDF's first page as PNG, for the image dialog (doc/site/dialogs.js)
    if method == "POST" && rest == "/render" {
        return match render(&root, body) {
            Some(png) => send(stream, method, "page.png", "no-cache", false, png),
            None => std::io::Write::write_all(
                stream,
                b"HTTP/1.1 422 Unprocessable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            ),
        };
    }
    if method == "POST" && rest.is_empty() {
        // A request for another root stops the build under way; the warm-up yields to any.
        if serde_json::from_slice::<Value>(body).is_ok_and(|r| r["supersedes"] == true) {
            SUPERSEDED.fetch_add(1, Relaxed);
        }
        WANTED.fetch_add(1, Relaxed);
        let one = ONE.lock().unwrap_or_else(|e| e.into_inner());
        WANTED.fetch_sub(1, Relaxed);
        let mut runs = Vec::new();
        let stop = Stop::new(stream, body);
        // asked for no longer while it waited its turn: not built at all
        let answer = match stop.why() {
            Some(why) => json!({ "error": why }),
            None => {
                build(&root, body, base, &mut runs, &stop).unwrap_or_else(|e| json!({ "error": e, "commands": runs }))
            }
        };
        drop(one);
        return send(stream, method, "answer.json", "no-cache", false, answer.to_string().into_bytes());
    }
    if rest.is_empty() {
        let warming = WARMING.load(std::sync::atomic::Ordering::Relaxed);
        let probe = json!({ "compile": true, "warming": warming });
        return send(stream, method, "probe.json", "no-cache", false, probe.to_string().into_bytes());
    }
    // `ID.zip`, `ID/slice.zip`: every file of that build, sources as compiled included
    if let Some(build) = rest.strip_suffix(".zip") {
        let dir = root.join(build.trim_start_matches('/')).canonicalize().ok();
        if let Some(dir) = dir.filter(|d| root.canonicalize().is_ok_and(|r| d.starts_with(&r) && *d != r) && d.is_dir())
        {
            let name = format!("{}.zip", dir.file_name().and_then(|n| n.to_str()).unwrap_or("build"));
            return send(stream, method, &name, "no-cache", false, zip(&dir)?);
        }
    }
    let file = root.join(rest.trim_start_matches('/')).canonicalize().ok();
    match file.filter(|f| root.canonicalize().is_ok_and(|r| f.starts_with(r)) && f.is_file()) {
        Some(f) => {
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let bytes = std::fs::read(&f)?;
            // a text file of the build (.aux, .log, .fls) shows in the browser, not as a download
            let text = content_type(&name) == "application/octet-stream" && std::str::from_utf8(&bytes).is_ok();
            send(stream, method, &if text { format!("{name}.txt") } else { name }, "no-cache", false, bytes)
        }
        None => std::io::Write::write_all(
            stream,
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ),
    }
}

/// The first page of the PDF `pdf` as PNG, by `pdftocairo`.
fn render(root: &Path, pdf: &[u8]) -> Option<Vec<u8>> {
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = root.join(format!("render-{at}"));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("in.pdf"), pdf).ok()?;
    let ok = Command::new("pdftocairo")
        .args(["-png", "-singlefile", "-f", "1", "-l", "1", "-r", "96", "in.pdf", "page"])
        .current_dir(&dir)
        .status()
        .is_ok_and(|s| s.success());
    let png = ok.then(|| std::fs::read(dir.join("page.png")).ok()).flatten();
    let _ = std::fs::remove_dir_all(&dir);
    png
}

/// One build of the request's project in a new directory under `root`.
/// Every command it runs goes into `runs`, the page shows them.
fn build(root: &Path, body: &[u8], base: Option<&Path>, runs: &mut Vec<Value>, stop: &Stop) -> Result<Value, String> {
    let mut request: Value = serde_json::from_slice(body).map_err(|e| format!("bad request: {e}"))?;
    let main = request["main"].as_str().filter(|m| safe(m)).ok_or("no main file")?.to_string();
    let main = main.as_str();
    // the project's other files, by name, size and time: a change to one is another build
    let copied: Vec<(PathBuf, String)> = base.map(project).unwrap_or_default();
    let stamps: Vec<String> = copied
        .iter()
        .filter_map(|(p, n)| {
            let m = p.metadata().ok()?;
            Some(format!("{n} {} {:?}", m.len(), m.modified().ok()?))
        })
        .collect();
    request["base"] = json!(stamps);
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
    for (file, name) in &copied {
        let to = dir.join(name);
        std::fs::create_dir_all(to.parent().unwrap_or(&dir)).map_err(|e| e.to_string())?;
        std::fs::copy(file, &to).map_err(|e| format!("{}: {e}", file.display()))?;
    }
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
    // satex's own build, explained by rules: the page's analysis explains the rest
    let flags: Vec<&str> =
        ["compile.system=satex", "compile.explain=false"].iter().chain(&sets).flat_map(|s| ["--set", s]).collect();
    // the last build of this lineage seeds this one, as latexmk's directory would
    let lineage = request_key(&json!({ "main": main, "at": request["at"], "sets": sets }));
    seed(root, &dir, &lineage);
    // A slice is a project of its own: the main file cut down, the local files it needs.
    let work = match request["at"].as_str() {
        Some(at) => {
            let out = run(
                runs,
                stop,
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
    // The whole project builds in its lineage's own directory, where the last build left an engine
    // waiting (`compile.warm`, src/cli/warm.rs); this build's directory gets what it wrote.
    let warm = (cfg!(target_os = "linux") && request["at"].is_null()).then(|| root.join(format!("{WARM}{lineage}")));
    let mut flags = flags;
    if let Some(w) = &warm {
        fill(w, &dir).map_err(|e| format!("{}: {e}", w.display()))?;
        flags.extend(["--set", "compile.warm=true"]);
    }
    let ran = warm.clone().unwrap_or_else(|| work.clone());
    let out = run(
        runs,
        stop,
        Command::new(&satex).args(&flags).args(["--format", "json", "compile", "-f", main]).current_dir(&ran),
    )?;
    if let Some(w) = &warm {
        mirror(w, &dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut records: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    for r in &mut records {
        if let Some(p) = r["path"].as_str() {
            let p = Path::new(p);
            r["path"] = json!(p.strip_prefix(&ran).unwrap_or(p).to_string_lossy().trim_start_matches("./"));
        }
    }
    let stem = Path::new(main).with_extension("");
    let pdf = work.join(stem.with_extension("pdf"));
    let (labels, keys) = if pdf.is_file() {
        // A page with pdf.js draws the pages itself, as they come into view (`pages: false`)
        if request["pages"].as_bool() != Some(false) {
            run(
                runs,
                stop,
                Command::new("pdftocairo")
                    .args(["-png", "-r", "144"])
                    .arg(&pdf)
                    .arg(work.join("page"))
                    .current_dir(&work),
            )?;
        }
        // the words, the links and the page labels are three reads of one PDF: run them side by side
        let (words, links, labels) = std::thread::scope(|scope| {
            let words = scope.spawn(|| {
                let mut r = Vec::new();
                let out = run(
                    &mut r,
                    stop,
                    Command::new("pdftotext").arg("-bbox").arg(&pdf).arg(work.join("words.html")).current_dir(&work),
                );
                (r, out)
            });
            // without qpdf the pages only lack their links. It exits 3 on mere warnings
            let links = scope.spawn(|| {
                let mut r = Vec::new();
                let _ = run(
                    &mut r,
                    stop,
                    Command::new("qpdf")
                        .args(["--json=2", "--json-key=pages", "--json-key=qpdf"])
                        .arg(&pdf)
                        .arg(work.join("links.json"))
                        .current_dir(&work),
                );
                r
            });
            let labels = scope.spawn(|| pagelabels(&pdf));
            let keys = scope.spawn(|| pagekeys(&pdf));
            (words.join(), links.join(), (labels.join(), keys.join()))
        });
        let (mut r, out) = words.map_err(|_| "pdftotext panicked".to_string())?;
        runs.append(&mut r);
        out?;
        runs.extend(links.unwrap_or_default());
        (labels.0.unwrap_or_else(|_| json!([])), labels.1.unwrap_or_default())
    } else {
        (pagelabels(&pdf), Vec::new())
    };
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
    let sent: std::collections::HashSet<&str> = request["files"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(n, _)| n.as_str())
        .chain(copied.iter().map(|(_, n)| n.as_str()))
        .collect();
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
        "pagelabels": labels,
        "pagekeys": keys,
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

/// Where a lineage's builds run, beside the builds: `w-` and the lineage.
const WARM: &str = "w-";

/// The lineage directory `w` made `dir`'s project: its files, the build state kept. The engine
/// waiting there is stopped unless it waits in its open (src/cli/warm.rs `settle`), and every
/// file goes in by rename, so the pipe in the main file's place is replaced, never fed. A file
/// gone from `dir` goes from `w`: the build sees what a fresh one would.
#[cfg(target_os = "linux")]
fn fill(w: &Path, dir: &Path) -> std::io::Result<()> {
    let keep = w.join(crate::cli::compile::KEEP);
    std::fs::create_dir_all(&keep)?;
    crate::cli::warm::settle(&keep, w);
    let file = |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file());
    let state = |name: &Path| name.components().next().is_some_and(|c| c.as_os_str() == crate::cli::compile::KEEP);
    for old in walkdir(w) {
        let Ok(name) = old.strip_prefix(w) else { continue };
        if !state(name) && file(&old) && !file(&dir.join(name)) {
            std::fs::remove_file(&old)?;
        }
    }
    for from in walkdir(dir) {
        let Ok(name) = from.strip_prefix(dir) else { continue };
        // the build state of a first build of the lineage, from the build it was seeded by
        if (state(name) && w.join(name).exists()) || !file(&from) {
            continue;
        }
        let to = w.join(name);
        std::fs::create_dir_all(to.parent().unwrap_or(w))?;
        crate::cli::warm::put(&to, &std::fs::read(&from)?)?;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn fill(_: &Path, _: &Path) -> std::io::Result<()> {
    Ok(())
}

/// What the build in `w` wrote, into `dir`: its plain files, no build state, no pipe.
fn mirror(w: &Path, dir: &Path) -> std::io::Result<()> {
    for from in walkdir(w) {
        let Ok(name) = from.strip_prefix(w) else { continue };
        let state = name.components().next().is_some_and(|c| c.as_os_str() == crate::cli::compile::KEEP);
        if state || !std::fs::symlink_metadata(&from).is_ok_and(|m| m.is_file()) || name == Path::new(LINEAGE) {
            continue;
        }
        let to = dir.join(name);
        std::fs::create_dir_all(to.parent().unwrap_or(dir))?;
        std::fs::copy(&from, &to)?;
    }
    Ok(())
}

/// Copies the build files and satex's build state of the newest build of `lineage` into `dir`.
fn seed(root: &Path, dir: &Path, lineage: &str) {
    let mut builds: Vec<PathBuf> =
        std::fs::read_dir(root).into_iter().flatten().filter_map(|e| Some(e.ok()?.path())).collect();
    builds.sort();
    let last = builds
        .into_iter()
        .rev()
        .find(|b| b != dir && std::fs::read_to_string(b.join(LINEAGE)).is_ok_and(|l| l == lineage));
    let _ = std::fs::write(dir.join(LINEAGE), lineage);
    let Some(last) = last else { return };
    for from in walkdir(&last) {
        let Ok(name) = from.strip_prefix(&last) else { continue };
        let ext = name.extension().and_then(|e| e.to_str()).unwrap_or_default();
        let state = name.components().any(|c| c.as_os_str() == crate::cli::compile::KEEP);
        // a lineage directory's waiting engine stays its own
        let engine = name.components().any(|c| c.as_os_str() == "warm");
        let plain = std::fs::symlink_metadata(&from).is_ok_and(|m| m.is_file());
        if !(state || crate::cli::compile::rests(ext)) || engine || !plain || dir.join(name).exists() {
            continue;
        }
        let to = dir.join(name);
        let _ = std::fs::create_dir_all(to.parent().unwrap_or(dir));
        // a format is only read: linked, not copied
        let _ = std::fs::hard_link(&from, &to).or_else(|_| std::fs::copy(&from, &to).map(drop));
    }
}

/// The file naming a build's lineage.
const LINEAGE: &str = "lineage.txt";

/// What a request determines: its main file, slice point and files.
fn request_key(request: &Value) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for part in [&request["main"], &request["at"], &request["files"], &request["sets"], &request["base"]] {
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
fn run(runs: &mut Vec<Value>, stop: &Stop, command: &mut Command) -> Result<std::process::Output, String> {
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
    let out = output_unless(command, || stop.why());
    runs.push(json!({
        "command": line.join(" "),
        "status": out.as_ref().ok().and_then(|o| o.status.code()),
        "ms": start.elapsed().as_millis() as u64,
        "error": out.as_ref().err().map(ToString::to_string),
    }));
    out.map_err(|e| match e.kind() {
        std::io::ErrorKind::Interrupted => e.to_string(),
        _ => format!("cannot run {}: {e}", command.get_program().to_string_lossy()),
    })
}

/// When a build of the page stops before its end: a request for another root came, the page
/// that asked is gone (its connection closed: a reload, a newer build of the same project, a
/// closed tab), or it ran longer than `compile.timeout`. No build outlives who waits for it.
struct Stop<'a> {
    since: u64,
    asker: &'a std::net::TcpStream,
    until: Option<(std::time::Instant, u64)>,
}

impl Stop<'_> {
    fn new<'a>(asker: &'a std::net::TcpStream, body: &[u8]) -> Stop<'a> {
        // the page's setting, else the default (`compile.timeout` in satex.yaml); 0: no limit
        let set = serde_json::from_slice::<Value>(body).ok().and_then(|r| {
            let sets = r["sets"].as_array()?.iter().filter_map(Value::as_str);
            sets.filter_map(|s| s.strip_prefix("compile.timeout=")).next_back()?.parse::<u64>().ok()
        });
        let secs = set.unwrap_or(u64::from(crate::project::config::CompileConfig::default().timeout));
        let until = (secs > 0).then(|| (std::time::Instant::now() + std::time::Duration::from_secs(secs), secs));
        Stop { since: SUPERSEDED.load(Relaxed), asker, until }
    }

    fn why(&self) -> Option<String> {
        if SUPERSEDED.load(Relaxed) != self.since {
            return Some("superseded by a newer request".into());
        }
        if gone(self.asker) {
            return Some("stopped: the page that asked for this build no longer waits for it".into());
        }
        let (at, secs) = self.until?;
        (std::time::Instant::now() > at)
            .then(|| format!("stopped after {secs} s: the build ran longer than `compile.timeout` allows"))
    }
}

/// Whether the other end of `stream` closed it. A request is whole before its build starts, so
/// nothing more is sent on it: a read that would not wait is the end of the connection.
fn gone(stream: &std::net::TcpStream) -> bool {
    if stream.set_nonblocking(true).is_err() {
        return false;
    }
    let closed = match stream.peek(&mut [0]) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    };
    let _ = stream.set_nonblocking(false);
    closed
}

/// `command`'s output, unless `stop` gives a reason first: then it is killed with every process
/// it started (`crate::cli::group`), and the reason is the error.
pub(super) fn output_unless(
    command: &mut Command,
    stop: impl Fn() -> Option<String>,
) -> std::io::Result<std::process::Output> {
    use std::process::Stdio;
    crate::cli::group::lead(command);
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // read both pipes meanwhile: a full pipe would stall the child
    let pipe = |p: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            p.map(|mut p| p.read_to_end(&mut bytes));
            bytes
        })
    };
    let out = pipe(child.stdout.take().map(|p| Box::new(p) as _));
    let err = pipe(child.stderr.take().map(|p| Box::new(p) as _));
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if let Some(why) = stop() {
            crate::cli::group::end(&mut child);
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, why));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    Ok(std::process::Output { status, stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
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
    name.starts_with("page-") && name.ends_with(".png")
        || ["words.html", "links.json", "answer.json", LINEAGE].contains(&name)
        || name.starts_with(crate::cli::compile::KEEP)
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

/// The files of the project in `dir` with their names in it: hidden and
/// ignored ones (`.gitignore`) left out, as a build tool sees the sources.
pub fn project(dir: &Path) -> Vec<(PathBuf, String)> {
    ignore::WalkBuilder::new(dir)
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.path().strip_prefix(dir).ok()?.to_str()?.replace('\\', "/");
            Some((e.path().to_path_buf(), name))
        })
        .collect()
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
    // builds are named by their time; a lineage's directory (`WARM`) and the warm-up's stay
    let build =
        |p: &PathBuf| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()));
    let mut builds: Vec<PathBuf> =
        std::fs::read_dir(root).into_iter().flatten().filter_map(|e| Some(e.ok()?.path())).filter(build).collect();
    builds.sort();
    for old in builds.iter().rev().skip(KEEP - 1) {
        let _ = std::fs::remove_dir_all(old);
    }
}

/// Each page's key: a hash of all a renderer draws it from, its dictionary and every object it
/// reaches (contents, resources, fonts, forms, annotations), not its `/Parent`, the attributes it
/// inherits from the page tree included (ISO 32000-1 § 7.7.3.4). Two pages with one key draw
/// alike, so the page keeps its image across builds. A font's new subset changes the key of every
/// page using it, which only redraws. Empty where qpdf cannot read the PDF.
pub fn pagekeys(pdf: &Path) -> Vec<String> {
    use std::collections::{HashSet, hash_map::DefaultHasher};
    use std::hash::{Hash, Hasher};
    let out = Command::new("qpdf")
        .args(["--json=2", "--json-key=qpdf", "--json-key=pages", "--json-stream-data=inline"])
        .arg(pdf)
        .arg("-")
        .output();
    let Some(json) = out.ok().and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok()) else { return Vec::new() };
    let objects = &json["qpdf"][1];
    /// `N G R`: an indirect object's reference, as qpdf's JSON writes it.
    fn reference(v: &Value) -> Option<&str> {
        let s = v.as_str()?.strip_suffix(" R")?;
        s.split(' ').all(|n| n.parse::<u64>().is_ok()).then_some(v.as_str()?)
    }
    fn walk<'a>(v: &'a Value, objects: &'a Value, seen: &mut HashSet<&'a str>, h: &mut DefaultHasher) {
        if let Some(r) = reference(v) {
            r.hash(h);
            if seen.insert(r) {
                walk(&objects[format!("obj:{r}")], objects, seen, h);
            }
            return;
        }
        match v {
            Value::Object(map) => map.iter().filter(|(k, _)| *k != "/Parent").for_each(|(k, v)| {
                k.hash(h);
                walk(v, objects, seen, h);
            }),
            Value::Array(items) => items.iter().for_each(|v| walk(v, objects, seen, h)),
            other => other.to_string().hash(h),
        }
    }
    let pages = json["pages"].as_array().into_iter().flatten();
    pages
        .map(|page| {
            let (mut h, mut seen) = (DefaultHasher::new(), HashSet::new());
            walk(&page["object"], objects, &mut seen, &mut h);
            // what the page inherits: the nearest ancestor's, as a reader takes it
            for key in ["/Resources", "/MediaBox", "/CropBox", "/Rotate"] {
                let mut node = reference(&page["object"]).map(|r| &objects[format!("obj:{r}")]["value"]);
                while let Some(n) = node.filter(|n| n.get(key).is_none()) {
                    node = n.get("/Parent").and_then(reference).map(|r| &objects[format!("obj:{r}")]["value"]);
                }
                key.hash(&mut h);
                walk(node.and_then(|n| n.get(key)).unwrap_or(&Value::Null), objects, &mut seen, &mut h);
            }
            format!("{:016x}", h.finish())
        })
        .collect()
}

/// The PDF's own page labels (beamer gives the overlay steps of a frame one range, so one label).
fn pagelabels(pdf: &Path) -> Value {
    Command::new("qpdf")
        .args(["--json=2", "--json-key=pagelabels"])
        .arg(pdf)
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok())
        .map_or(json!([]), |v| v["pagelabels"].clone())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    /// Whether process `pid` still runs (a zombie has ended).
    fn runs(pid: &str) -> bool {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().next()).is_some_and(|s| s != "Z")
    }

    /// A build is a tree of processes (`satex compile`, latexmk, the engine). Ending only the
    /// one the server started left the engine running without a parent: dozens gathered over a
    /// day. A stopped build must leave no process, at any depth.
    #[test]
    fn a_stopped_build_leaves_no_process() {
        let dir = std::env::temp_dir().join(format!("satex-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let started = std::time::Instant::now();
        // a child that starts a grandchild, as a build tool starts the engine; both would run a minute
        let mut build = Command::new("sh");
        build.args(["-c", "sh -c 'echo $$ > inner; exec sleep 60' & echo $$ > outer; wait"]).current_dir(&dir); // not prose
        let both = || dir.join("inner").metadata().is_ok_and(|m| m.len() > 0) && dir.join("outer").is_file();
        let out = output_unless(&mut build, || both().then(|| "stopped".to_string()));
        assert_eq!(out.unwrap_err().to_string(), "stopped");
        assert!(started.elapsed() < std::time::Duration::from_secs(30), "the stop waited for the build's end");
        for name in ["inner", "outer"] {
            let pid = std::fs::read_to_string(dir.join(name)).unwrap();
            let gone = || !runs(pid.trim());
            let waited = (0..200).any(|_| {
                gone() || {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    false
                }
            });
            assert!(waited, "the {name} process {pid} outlived the stopped build");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A build whose page reloaded or closed has nobody to answer: before, it ran to its end and
    /// held the next page's first build behind it. The closed connection is its stop.
    #[test]
    fn a_build_stops_once_its_page_is_gone() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let page = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (asker, _) = listener.accept().unwrap();
        let stop = Stop::new(&asker, br#"{"sets": ["engine=pdftex", "compile.timeout=0"]}"#);
        assert_eq!(stop.why(), None, "an open page waits for its build");
        assert!(stop.until.is_none(), "compile.timeout=0 is no limit");
        drop(page);
        let waited = (0..200).any(|_| {
            stop.why().is_some() || {
                std::thread::sleep(std::time::Duration::from_millis(10));
                false
            }
        });
        assert!(waited, "the closed connection did not stop the build");
        assert!(stop.why().unwrap().contains("no longer waits"));
    }

    /// A build that never ends (an engine waiting for input) held the one-build-at-a-time lock
    /// for good. `compile.timeout` ends it, and the answer says so.
    #[test]
    fn a_build_stops_at_its_time_limit() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let _page = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (asker, _) = listener.accept().unwrap();
        let limit = Stop::new(&asker, br#"{"sets": ["compile.timeout=1"]}"#);
        let default = u64::from(crate::project::config::CompileConfig::default().timeout);
        assert_eq!(Stop::new(&asker, b"{}").until.map(|(_, secs)| secs), Some(default));
        let out = output_unless(Command::new("sleep").arg("60"), || limit.why());
        assert!(out.unwrap_err().to_string().contains("compile.timeout"));
    }
}
