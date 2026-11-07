//! `satex gui [DIR]`: the browser playground on this machine. It serves the
//! site's pages on 127.0.0.1 and works on the project in `DIR` (a file
//! names the root document, its folder the project): its files
//! are the playground's files, edits are saved to them, the language server
//! is the native `satex lsp --lines` (so it reads this machine's TeX
//! installation), and Compile runs the local TeX.
//!
//! The page's worker (`doc/site/worker.js`) talks to it instead of the wasm:
//!
//! - `GET /__satex/local`: the project, `{dir, name, root, files: [[name, size]]}`. The root is
//!   the file given, else the document discovery starts from (`root_document`).
//! - `GET|PUT|DELETE /__satex/file?path=P`: a file. Reads reach the project
//!   and the files kpathsea finds, writes the project only.
//! - `POST /__satex/run`: `{args, cwd}`, one `satex` run, `{lines, status, ms}`.
//! - `POST /__satex/lsp?s=S`: one message to session `S`'s server. The
//!   answer streams its status lines and the findings of each run that ended, then the replies (`batch.rs`).
//! - `POST /__satex/lsp/reset?s=S`: `{args, cwd}`, the next server's.
//! - `POST /__satex/lsp/cancel?s=S`: stops the run under way.
//! - `GET /__satex/session?s=S`: held open by the worker; its end ends `S`.
//!
//! Only this user's browser drives it: the server binds 127.0.0.1, accepts
//! only its own `Host` (no DNS rebinding), and wants the random token it
//! printed, as a `SameSite=Strict` cookie that the first page load sets.

pub mod compile;
pub mod http;

use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// The cookie that carries the token.
const COOKIE: &str = "satex-gui";

pub struct Gui {
    /// The project, canonical.
    pub dir: PathBuf,
    /// The root document, when `satex gui` was given a file.
    root: Option<String>,
    site: PathBuf,
    /// `doc/site` with `--dev`: the page's sources instead of the built ones.
    src: Option<PathBuf>,
    pub token: String,
    pub port: u16,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    /// Where the language servers' messages go: a log file, or with `-v` the terminal.
    log: Option<PathBuf>,
    /// The document the terminal's status line last named.
    said: Mutex<String>,
}

/// A language server for one page: the flags and directory of its next
/// start, the process, its input and its output.
#[derive(Default)]
struct Session {
    spec: Mutex<(Vec<String>, PathBuf)>,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    stdout: Mutex<Option<std::io::BufReader<ChildStdout>>>,
}

impl Session {
    fn stop(&self) {
        if let Some(mut child) = lock(&self.child).take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        lock(&self.stdin).take();
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `satex gui`: serves `dir` until stopped, printing the address first.
pub fn run(dir: &Path, port: u16, site: Option<&Path>, open: bool, dev: bool, verbose: bool) -> Result<(), String> {
    let listener = http::bind(port)?;
    let gui = Gui::new(dir, site, dev, listener.local_addr().map_err(|e| e.to_string())?.port(), verbose)?;
    let url = format!("http://127.0.0.1:{}/play/?token={}", gui.port, gui.token);
    let root = gui.root.as_deref().map(|r| format!(", root {r}")).unwrap_or_default();
    // stdout may close (a closed terminal, `| head`): the server keeps serving
    let _ = writeln!(std::io::stdout(), "satex gui: {}{root} at\n  {url}\nCtrl+C stops", gui.dir.display());
    if let Some(log) = &gui.log {
        let _ =
            writeln!(std::io::stdout(), "The language server's messages go to {} (-v prints them here)", log.display());
    }
    if open {
        browse(&url);
    }
    gui.serve(listener);
    Ok(())
}

impl Gui {
    /// `verbose`: the language servers' messages go to the terminal, else to a log file.
    pub fn new(dir: &Path, site: Option<&Path>, dev: bool, port: u16, verbose: bool) -> Result<Arc<Gui>, String> {
        let given = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
        // a file names the root document, its folder the project
        let (dir, root) = match given.is_file() {
            true => (
                given.parent().map(Path::to_path_buf).unwrap_or_default(),
                given.file_name().map(|n| n.to_string_lossy().into_owned()),
            ),
            // else the document the command line would start from
            false => {
                let found = crate::plugins::discovery::scan(&given).root_document();
                let root = found.and_then(|p| Some(p.strip_prefix(&given).ok()?.to_string_lossy().replace('\\', "/")));
                (given, root)
            }
        };
        let site = find_site(site)?.canonicalize().map_err(|e| e.to_string())?;
        let src = dev.then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/site"));
        // beside the builds, which are pruned
        let log = (!verbose).then(|| cache().with_extension("log"));
        if let Some(log) = &log {
            let _ = std::fs::create_dir_all(log.parent().unwrap_or(&dir)).and_then(|()| std::fs::write(log, ""));
        }
        Ok(Arc::new(Gui {
            dir,
            root,
            site,
            src,
            token: token(),
            port,
            sessions: Mutex::default(),
            log,
            said: Mutex::default(),
        }))
    }

    pub fn serve(self: Arc<Self>, listener: std::net::TcpListener) {
        for stream in listener.incoming().flatten() {
            let gui = self.clone();
            std::thread::spawn(move || {
                let _ = gui.respond(stream);
            });
        }
    }

    fn respond(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let req = http::Request::read(&stream)?;
        // Another site's page, or one reached through another host name, gets nothing.
        let host = req.header("host").unwrap_or_default();
        if ![format!("127.0.0.1:{}", self.port), format!("localhost:{}", self.port)].iter().any(|h| h == host) {
            return http::status(&mut stream, "403 Forbidden", "");
        }
        let cookie = format!("{COOKIE}={}", self.token);
        let has_cookie = req.header("cookie").is_some_and(|c| c.split(';').any(|c| c.trim() == cookie));
        if req.param("token").as_deref() == Some(&self.token) {
            // not prose
            let to = format!("Set-Cookie: {cookie}; HttpOnly; SameSite=Strict; Path=/\r\nLocation: {}\r\n", req.path);
            return http::status(&mut stream, "303 See Other", &to);
        }
        if !has_cookie {
            return http::status(&mut stream, "403 Forbidden", "");
        }
        let (method, path) = (req.method.as_str(), req.path.as_str());
        let session = || {
            let id = req.param("s").unwrap_or_default();
            lock(&self.sessions).entry(id).or_default().clone()
        };
        let json = |stream: &mut TcpStream, v: Value| {
            http::send(stream, method, "a.json", "no-cache", false, v.to_string().into_bytes())
        };
        match (method, path) {
            ("GET", "/__satex/local") => {
                let files: Vec<Value> = compile::project(&self.dir)
                    .into_iter()
                    .map(|(p, n)| json!([n, p.metadata().map(|m| m.len()).unwrap_or(0)]))
                    .collect();
                let name = self.dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                json(&mut stream, json!({ "dir": self.dir, "name": name, "root": self.root, "files": files }))
            }
            (_, "/__satex/file") => {
                let asked = PathBuf::from(req.param("path").unwrap_or_default());
                let done = match method {
                    "GET" => self.readable(&asked).and_then(|p| std::fs::read(p).ok()),
                    "PUT" => self.inside(&asked).and_then(|p| save(&p, &req.body).ok()).map(|()| vec![]),
                    "DELETE" => self.inside(&asked).and_then(|p| std::fs::remove_file(p).ok()).map(|()| vec![]),
                    _ => None,
                };
                match done {
                    Some(bytes) => http::send(&mut stream, method, "file", "no-cache", false, bytes),
                    None => http::status(&mut stream, "404 Not Found", ""),
                }
            }
            ("POST", "/__satex/run") => {
                let ask: Value = serde_json::from_slice(&req.body).unwrap_or_default();
                json(&mut stream, self.command(&ask))
            }
            ("POST", "/__satex/lsp") => self.exchange(&session(), &mut stream, &req.body),
            ("POST", "/__satex/lsp/reset") => {
                let ask: Value = serde_json::from_slice(&req.body).unwrap_or_default();
                let s = session();
                s.stop();
                lock(&s.stdout).take();
                *lock(&s.spec) = (strings(&ask["args"]), self.cwd(&ask["cwd"]));
                http::status(&mut stream, "204 No Content", "")
            }
            ("POST", "/__satex/lsp/cancel") => {
                if let Some(stdin) = lock(&session().stdin).as_mut() {
                    let _ = stdin.write_all(b"{\"cancel\":true}\n").and_then(|()| stdin.flush());
                }
                http::status(&mut stream, "204 No Content", "")
            }
            ("GET", "/__satex/session") => {
                stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\n\r\n: ok\n\n",
                )?;
                stream.flush()?;
                // the page never writes again: a read returns once it is gone
                let _ = stream.read(&mut [0; 1]);
                if let Some(s) = lock(&self.sessions).remove(&req.param("s").unwrap_or_default()) {
                    s.stop();
                }
                Ok(())
            }
            _ if path.starts_with("/__satex/compile") => {
                let root = cache();
                let rest = &path["/__satex/compile".len()..];
                compile::respond(&mut stream, method, rest, &req.body, &root, Some(&self.dir))
            }
            (_, "/play/" | "/play/index.html") => {
                let page = http::page(&self.site, self.src.as_deref(), |m| {
                    m["local"] = json!({ "dir": self.dir });
                    // this machine's TeX has the packages: none is fetched
                    m["packages"] = json!([]);
                    m["preload"] = json!([]);
                });
                match page {
                    Some(page) => http::send(&mut stream, method, "index.html", "no-cache", false, page),
                    None => http::status(&mut stream, "500 Internal Server Error", ""),
                }
            }
            _ => match self.src.as_deref().and_then(|src| http::source(path, src)) {
                Some((name, body)) => http::send(&mut stream, method, &name, "no-cache", false, body),
                None => http::file(&mut stream, &req, &self.site, |_, body| body),
            },
        }
    }

    /// One message to the session's server, started on demand: its status
    /// lines and replies streamed to the page as they come.
    fn exchange(&self, s: &Session, stream: &mut TcpStream, message: &[u8]) -> std::io::Result<()> {
        let mut out = lock(&s.stdout);
        if out.is_none() {
            let (args, cwd) = lock(&s.spec).clone();
            let started = Command::new(std::env::current_exe()?)
                .args(&args)
                .args(["lsp", "--lines"])
                .current_dir(&cwd)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(match self.log.as_ref().and_then(|l| std::fs::OpenOptions::new().append(true).open(l).ok()) {
                    Some(log) => Stdio::from(log),
                    None => Stdio::inherit(),
                })
                .spawn();
            if let Ok(mut child) = started {
                *lock(&s.stdin) = child.stdin.take();
                *out = child.stdout.take().map(std::io::BufReader::new);
                *lock(&s.child) = Some(child);
            }
        }
        let sent = lock(&s.stdin)
            .as_mut()
            .is_some_and(|i| i.write_all(message).and_then(|()| i.write_all(b"\n")).and_then(|()| i.flush()).is_ok());
        stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        )?;
        let mut alive = true;
        if let (true, Some(reader)) = (sent, out.as_mut()) {
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                // a page gone still gets the rest read, so the next answer starts at its own line
                alive = alive && stream.write_all(line.as_bytes()).and_then(|()| stream.flush()).is_ok();
                if line.starts_with('[') {
                    return Ok(());
                }
                self.tell(&line);
            }
        }
        // no answer: the server stopped. The next message starts another
        out.take();
        s.stop();
        stream.write_all(b"{\"error\":\"the language server stopped\"}\n")
    }

    /// The terminal's status: the document a run reads, once per document.
    fn tell(&self, line: &str) {
        let Ok(status) = serde_json::from_str::<Value>(line) else { return };
        // `analyzing main.tex (preamble)`: the run's own file
        let text = status["params"]["text"].as_str().unwrap_or_default();
        let Some(file) = text.strip_prefix("analyzing ").and_then(|t| t.split(" (").next()) else { return };
        let file = file.to_string();
        let mut said = lock(&self.said);
        if *said != file {
            let _ = writeln!(std::io::stdout(), "satex gui: analyzing {file} …");
            *said = file;
        }
    }

    /// One `satex` run of the page's console: its lines, status and time.
    fn command(&self, ask: &Value) -> Value {
        let started = std::time::Instant::now();
        let mut command = Command::new(std::env::current_exe().unwrap_or_else(|_| "satex".into()));
        command.args(strings(&ask["args"])).current_dir(self.cwd(&ask["cwd"]));
        let Ok(out) = compile::output_unless(&mut command, || false) else {
            return json!({ "lines": [["err", "satex did not start"]], "status": "aborted", "ms": 0 });
        };
        let lines = |kind: &str, bytes: &[u8]| -> Vec<Value> {
            String::from_utf8_lossy(bytes).lines().map(|l| json!([kind, l])).collect()
        };
        let mut all = lines("out", &out.stdout);
        all.extend(lines("err", &out.stderr));
        json!({ "lines": all, "status": out.status.code(), "ms": started.elapsed().as_millis() as u64 })
    }

    /// The page's working directory when inside the project, else the project.
    fn cwd(&self, asked: &Value) -> PathBuf {
        asked
            .as_str()
            .and_then(|p| self.inside(Path::new(p)))
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| self.dir.clone())
    }

    /// `path` resolved, when it lies in the project: symbolic links followed,
    /// `..` refused, a file not there yet allowed.
    pub fn inside(&self, path: &Path) -> Option<PathBuf> {
        if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
            return None;
        }
        let mut rest = Vec::new();
        let mut at = path;
        let real = loop {
            if let Ok(real) = at.canonicalize() {
                break real;
            }
            rest.push(at.file_name()?);
            at = at.parent()?;
        };
        let real = rest.iter().rev().fold(real, |p, n| p.join(n));
        real.starts_with(&self.dir).then_some(real)
    }

    /// `path` resolved, when it lies in the project or is the file kpathsea finds by its name.
    pub fn readable(&self, path: &Path) -> Option<PathBuf> {
        self.inside(path).or_else(|| {
            let real = path.canonicalize().ok()?;
            let found = Command::new("kpsewhich").arg(real.file_name()?).output().ok()?;
            let found = PathBuf::from(String::from_utf8_lossy(&found.stdout).trim()).canonicalize().ok()?;
            (found == real).then_some(real)
        })
    }
}

/// Writes `bytes` to `path` unless it holds them already (a build tool
/// watching it sees no change), making its directory. The page edits text
/// as UTF-8 with `\n` line ends: a file with `\r\n` keeps them, and a file
/// not in UTF-8, which the page cannot have read faithfully, is left alone.
fn save(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let old = std::fs::read(path).ok();
    let crlf = old.as_deref().is_some_and(|o| o.windows(2).any(|w| w == b"\r\n")) && !bytes.contains(&b'\r');
    let bytes = if crlf { String::from_utf8_lossy(bytes).replace('\n', "\r\n").into_bytes() } else { bytes.to_vec() };
    match old {
        Some(old) if old == bytes => Ok(()),
        Some(old) if std::str::from_utf8(&old).is_err() => Err(std::io::Error::other("not UTF-8: left as it is")),
        _ => {
            std::fs::create_dir_all(path.parent().unwrap_or(path))?;
            std::fs::write(path, bytes)
        }
    }
}

/// The server's own directory in the cache: the builds, the log.
fn cache() -> PathBuf {
    crate::project::paths::cache_dir().unwrap_or_else(std::env::temp_dir).join("satex-gui")
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(|s| s.as_str().map(String::from)).collect()
}

/// 128 random bits as hex, from the keys the standard library seeds from the system.
fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let half = || {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        h.finish()
    };
    format!("{:016x}{:016x}", half(), half())
}

/// The built site: `--site`, else `share/satex/site` beside the installed
/// binary, else the checkout's `target/site` that `cargo playground` builds.
fn find_site(given: Option<&Path>) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().ok();
    let beside = exe.as_deref().and_then(Path::parent).and_then(Path::parent).map(|p| p.join("share/satex/site"));
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/site");
    let found = match given {
        Some(site) => Some(site.to_path_buf()),
        None => beside.into_iter().chain([checkout]).find(|s| s.join("play/index.html").is_file()),
    };
    found.filter(|s| s.join("play/index.html").is_file()).ok_or_else(|| {
        "satex gui needs the playground's pages and found none. Pass `--site DIR` with a built site, \
         or run `cargo playground` once in a checkout of satex"
            .to_string()
    })
}

/// Opens `url` in the default browser, if the platform has a way.
fn browse(url: &str) {
    let opener: &[&str] = if cfg!(target_os = "macos") {
        &["open"]
    } else if cfg!(windows) {
        &["cmd", "/C", "start", ""]
    } else {
        &["xdg-open"]
    };
    let _ = Command::new(opener[0]).args(&opener[1..]).arg(url).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}
