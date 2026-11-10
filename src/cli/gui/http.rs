//! The little HTTP the playground's local servers speak: `satex gui` and
//! `cargo playground` (`src/bin/satex-playground.rs`). One request per
//! connection, served on 127.0.0.1 the way a static host such as GitHub
//! Pages serves the site.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

/// A request: its method, decoded path, query, headers (names lowercase) and body.
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn read(stream: &TcpStream) -> std::io::Result<Request> {
        let mut reader = std::io::BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let mut words = line.split_whitespace();
        let (method, target) = (words.next().unwrap_or_default().to_string(), words.next().unwrap_or("/"));
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let path = percent_encoding::percent_decode_str(path).decode_utf8_lossy().into_owned();
        let query = query.split('#').next().unwrap_or_default().to_string();
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? <= 2 {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
            }
        }
        let mut request = Request { method, path, query, headers, body: Vec::new() };
        let length = request.header("content-length").and_then(|n| n.parse().ok()).unwrap_or(0);
        request.body = vec![0; length];
        reader.read_exact(&mut request.body)?;
        Ok(request)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// The query parameter `key`, decoded.
    pub fn param(&self, key: &str) -> Option<String> {
        self.query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == key)
                .then(|| percent_encoding::percent_decode_str(&v.replace('+', " ")).decode_utf8_lossy().into_owned())
        })
    }

    fn gzip(&self) -> bool {
        self.header("accept-encoding").is_some_and(|e| e.contains("gzip"))
    }
}

pub fn send(
    stream: &mut TcpStream,
    method: &str,
    name: &str,
    cache: &str,
    encoded: bool,
    body: Vec<u8>,
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: {cache}\r\nVary: Accept-Encoding\r\nConnection: close\r\n",
        content_type(name),
        body.len(),
    );
    if encoded {
        head.push_str("Content-Encoding: gzip\r\n");
    }
    // Cross-origin isolated from the first load, as doc/site/sw.js makes it later: the page shares
    // a SharedArrayBuffer with its worker, which a run polls to stop for a newer text.
    head.push_str("Cross-Origin-Opener-Policy: same-origin\r\nCross-Origin-Embedder-Policy: credentialless\r\n\r\n");
    stream.write_all(head.as_bytes())?;
    if method != "HEAD" {
        stream.write_all(&body)?;
    }
    stream.flush()
}

/// An answer without a body worth a type: `status` is `404 Not Found` and the like.
pub fn status(stream: &mut TcpStream, status: &str, extra: &str) -> std::io::Result<()> {
    let head = format!("HTTP/1.1 {status}\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n");
    stream.write_all(head.as_bytes())
}

/// A file of `site`, or `site/index.html` for a directory; `fill` may
/// change a page. A `.gz` beside a file goes to a client that takes gzip
/// (`build.py --precompress`). Nothing outside `site`, whatever the path spells.
pub fn file(
    stream: &mut TcpStream,
    req: &Request,
    site: &Path,
    fill: impl Fn(&str, Vec<u8>) -> Vec<u8>,
) -> std::io::Result<()> {
    let mut file = site.join(req.path.trim_start_matches('/'));
    if file.is_dir() && !req.path.ends_with('/') {
        return status(stream, "301 Moved Permanently", &format!("Location: {}/\r\n", req.path));
    }
    if file.is_dir() {
        file.push("index.html");
    }
    let Some(file) = file.canonicalize().ok().filter(|f| f.starts_with(site) && f.is_file()) else {
        return status(stream, "404 Not Found", "");
    };
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    let zipped = file.with_file_name(format!("{name}.gz"));
    // a page `fill` changes goes out as it is
    let encoded = req.gzip() && zipped.is_file() && !name.ends_with(".html");
    let body = std::fs::read(if encoded { &zipped } else { &file })?;
    let body = if encoded { body } else { fill(&name, body) };
    send(stream, &req.method, &name, cache_control(&name), encoded, body)
}

/// How long a browser may keep a file: for good when its name carries its
/// content's hash (`site-d8283f8c8028.js`, as `build.py` names assets), else
/// asking again each time, so a rebuilt page shows at once.
pub fn cache_control(name: &str) -> &'static str {
    if hashed(name.split('.').next().unwrap_or_default()).is_some() {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

/// `stem` without the content hash `build.py` gives an asset (`site-0123456789ab`).
pub fn hashed(stem: &str) -> Option<&str> {
    stem.rsplit_once('-').filter(|(_, h)| h.len() == 12 && h.bytes().all(|b| b.is_ascii_hexdigit())).map(|(s, _)| s)
}

/// The media type a static host gives a file by its extension.
pub fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",             // not prose
        "js" | "mjs" => "text/javascript; charset=utf-8", // not prose
        "css" => "text/css; charset=utf-8",               // not prose
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "gz" => "application/gzip",
        "zip" => "application/zip",
        "woff2" => "font/woff2",
        "pdf" => "application/pdf",
        "txt" | "tex" | "md" => "text/plain; charset=utf-8", // not prose
        _ => "application/octet-stream",
    }
}

/// `port`, or the next one up that nothing listens on (port 0: one the system picks).
pub fn bind(port: u16) -> Result<std::net::TcpListener, String> {
    (port..port.saturating_add(100))
        .find_map(|p| std::net::TcpListener::bind(("127.0.0.1", p)).ok())
        .ok_or_else(|| format!("no free port from {port} on"))
}

/// The playground page `site/play/index.html` with its manifest changed by
/// `patch`. With `src` (doc/site), the page's body is `src/play.html` and
/// the worker `src/worker.js`, so a source edit shows without a site build.
pub fn page(site: &Path, src: Option<&Path>, patch: impl FnOnce(&mut serde_json::Value)) -> Option<Vec<u8>> {
    let built = std::fs::read_to_string(site.join("play/index.html")).ok()?;
    let marker = "id=\"satex-play\" type=\"application/json\">";
    let m = built.find(marker)? + marker.len();
    let end = m + built[m..].find("</script>")?;
    let mut manifest: serde_json::Value = serde_json::from_str(&built[m..end].replace("<\\/", "</")).ok()?;
    if let Some(src) = src {
        manifest["worker"] = std::fs::read_to_string(src.join("worker.js")).ok()?.into();
    }
    patch(&mut manifest);
    let json = manifest.to_string().replace("</", "<\\/");
    let Some(src) = src else {
        return Some(format!("{}{json}{}", &built[..m], &built[end..]).into_bytes());
    };
    let (lo, hi) = (built.find("<main>")? + 6, built.find("</main>")?);
    let body = std::fs::read_to_string(src.join("play.html")).ok()?;
    // `{{viewsjs}}` is `views.js`, as build.py names the scripts it emits
    let mut filled = String::new();
    let mut rest = body.as_str();
    while let Some((before, after)) = rest.split_once("{{") {
        let Some((key, after)) = after.split_once("}}") else { break };
        filled.push_str(before);
        match key {
            "manifest" => filled.push_str(&json),
            "root" => filled.push_str("../"),
            "playjs" => filled.push_str("play/playground.js"),
            _ => {
                filled.push_str(&key.strip_suffix("js").map_or_else(|| format!("{{{{{key}}}}}"), |s| format!("{s}.js")))
            }
        }
        rest = after;
    }
    filled.push_str(rest);
    Some(format!("{}{filled}{}", &built[..lo], &built[hi..]).into_bytes())
}

/// With `src` (doc/site): a hashed asset (`style-0123456789ab.css`) read
/// from its source (`src/style.css`), a script's `.ts` source transpiled
/// by esbuild as `build.py` does.
pub fn source(path: &str, src: &Path) -> Option<(String, Vec<u8>)> {
    let name = path.rsplit('/').next()?;
    let (stem, ext) = name.split_once('.')?;
    let stem = hashed(stem).unwrap_or(stem);
    let ts = src.join(format!("{stem}.ts"));
    if ext == "js" && ts.is_file() {
        let local = src.join("../../editors/vscode/node_modules/.bin/esbuild");
        let tool =
            std::env::var_os("ESBUILD").map_or(if local.is_file() { local } else { "esbuild".into() }, PathBuf::from);
        let out = std::process::Command::new(tool)
            .args(["--loader=ts", "--format=iife", "--log-level=error"])
            .stdin(std::fs::File::open(&ts).ok()?)
            .output()
            .ok()?;
        return out.status.success().then(|| (name.to_string(), out.stdout));
    }
    let file: PathBuf = src.join(format!("{stem}.{ext}"));
    (matches!(ext, "css" | "js") && file.is_file()).then(|| (name.to_string(), std::fs::read(file).unwrap_or_default()))
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
