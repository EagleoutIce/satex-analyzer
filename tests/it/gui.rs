//! `satex gui`: the server on a project directory, driven as the page's
//! worker drives it (`doc/site/worker.js`, `LocalEngine`).

use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};

struct Gui {
    child: Child,
    port: u16,
    token: String,
    dir: PathBuf,
    root: PathBuf,
}

impl Drop for Gui {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Gui {
    /// A server on a fresh project holding `files`, with a site of one page.
    fn start(name: &str, files: &[(&str, &str)]) -> Gui {
        let root = crate::support::scratch_dir(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/scratch/gui-test")
                .join(format!("{}-{name}", std::process::id())),
        );
        let (dir, site) = (root.join("project"), root.join("site"));
        // the checkout's own satex.yaml stays out of the project's cascade
        std::fs::write(root.join("satex.yaml"), "project: {ignore_other_configs: true}\n").unwrap();
        for (file, text) in files {
            std::fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
            std::fs::write(dir.join(file), text).unwrap();
        }
        std::fs::create_dir_all(site.join("play")).unwrap();
        let page = r#"<main><script id="satex-play" type="application/json">{"packages":[{"id":"x"}]}</script></main>"#;
        std::fs::write(site.join("play/index.html"), page).unwrap();
        let mut child = crate::support::satex()
            .arg("gui")
            .arg(&dir)
            .arg("--site")
            .arg(&site)
            .args(["--no-open", "--port", "0"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("satex gui starts");
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut url = String::new();
        while !url.contains("token=") {
            url.clear();
            assert!(out.read_line(&mut url).unwrap() > 0, "satex gui printed no address");
        }
        let url = url.trim();
        let port = url["http://127.0.0.1:".len()..].split('/').next().unwrap().parse().unwrap();
        let token = url.rsplit_once("token=").unwrap().1.to_string();
        Gui { child, port, token, dir: dir.canonicalize().unwrap(), root }
    }

    /// One request: the status line, the headers and the body.
    fn ask(&self, method: &str, target: &str, headers: &str, body: &str) -> (String, String) {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        let head = format!("{method} {target} HTTP/1.1\r\n{headers}Content-Length: {}\r\n\r\n", body.len());
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(body.as_bytes()).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
        (head.to_string(), body.to_string())
    }

    /// A request as the page sends it: its own host, the token's cookie.
    fn page(&self, method: &str, target: &str, body: &str) -> (String, String) {
        let headers = format!("Host: 127.0.0.1:{}\r\nCookie: satex-gui={}\r\n", self.port, self.token);
        self.ask(method, target, &headers, body)
    }

    /// One language server message: the notifications of runs that ended
    /// meanwhile, in order, then its replies. Status lines left out.
    fn lsp(&self, message: serde_json::Value) -> Vec<serde_json::Value> {
        let (head, body) = self.page("POST", "/__satex/lsp?s=t", &message.to_string());
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        let last = body.lines().last().unwrap_or_default();
        let replies: Vec<serde_json::Value> =
            serde_json::from_str(last).unwrap_or_else(|_| panic!("no replies: {body}"));
        let streamed = body.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok());
        streamed.filter(|m| m.is_object() && m["method"] != "satex/status").chain(replies).collect()
    }
}

#[test]
fn gui_serves_only_its_own_browser() {
    let gui = Gui::start("token", &[("main.tex", "hello\\bye\n")]);
    let host = format!("Host: 127.0.0.1:{}\r\n", gui.port);
    // no cookie, a wrong one, another host name (DNS rebinding): refused
    assert!(gui.ask("GET", "/__satex/local", &host, "").0.contains("403"));
    assert!(gui.ask("GET", "/__satex/local", &format!("{host}Cookie: satex-gui=0\r\n"), "").0.contains("403"));
    let rebound = format!("Host: evil.example:{}\r\nCookie: satex-gui={}\r\n", gui.port, gui.token);
    assert!(gui.ask("GET", "/__satex/local", &rebound, "").0.contains("403"));
    // the printed address sets the cookie
    let (head, _) = gui.ask("GET", &format!("/play/?token={}", gui.token), &host, "");
    assert!(
        head.contains("303") && head.contains(&format!("satex-gui={}; HttpOnly; SameSite=Strict", gui.token)),
        "{head}"
    );
    let (_, local) = gui.page("GET", "/__satex/local", "");
    let local: serde_json::Value = serde_json::from_str(&local).unwrap();
    assert_eq!(local["dir"], gui.dir.to_string_lossy().as_ref());
    assert_eq!(local["files"][0][0], "main.tex");
    // the page says it is local and fetches no package
    let (_, page) = gui.page("GET", "/play/", "");
    assert!(page.contains("\"local\"") && page.contains("\"packages\":[]"), "{page}");
}

#[test]
fn gui_reads_and_writes_inside_the_project_only() {
    let gui = Gui::start("guard", &[("main.tex", "hello\\bye\n"), ("crlf.tex", "a\r\nb\r\n")]);
    let file = |p: &Path| format!("/__satex/file?path={}", p.to_string_lossy());
    let outside = gui.dir.parent().unwrap().join("outside.tex");
    std::fs::write(&outside, "secret").unwrap();
    assert!(gui.page("GET", &file(&gui.dir.join("main.tex")), "").1.contains("hello"));
    assert!(gui.page("GET", &file(&outside), "").0.contains("404"));
    assert!(gui.page("GET", &file(&gui.dir.join("../outside.tex")), "").0.contains("404"));
    assert!(gui.page("PUT", &file(&outside), "changed").0.contains("404"));
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "secret");
    // a link inside the project to a file outside leads nowhere
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, gui.dir.join("link.tex")).unwrap();
        assert!(gui.page("GET", &file(&gui.dir.join("link.tex")), "").0.contains("404"));
        assert!(gui.page("PUT", &file(&gui.dir.join("link.tex")), "changed").0.contains("404"));
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "secret");
    }
    // a new file in a new folder, and an edit that keeps the file's line ends
    assert!(gui.page("PUT", &file(&gui.dir.join("sub/new.tex")), "new").0.contains("200"));
    assert_eq!(std::fs::read_to_string(gui.dir.join("sub/new.tex")).unwrap(), "new");
    gui.page("PUT", &file(&gui.dir.join("crlf.tex")), "a\nc\n");
    assert_eq!(std::fs::read_to_string(gui.dir.join("crlf.tex")).unwrap(), "a\r\nc\r\n");
}

#[test]
fn gui_language_server_answers_through_the_page_transport() {
    let gui = Gui::start("lsp", &[("main.tex", "\\undefinedname\\bye\n")]);
    let uri = format!("file://{}", gui.dir.join("main.tex").to_string_lossy());
    let flags = ["--no-packages", "--no-classes", "--set", "load.format=false", "--set", "texmf.kpsewhich=false"];
    let args = [&flags[..], &["-f", "main.tex"][..]].concat();
    let reset = serde_json::json!({ "args": args, "cwd": gui.dir });
    assert!(gui.page("POST", "/__satex/lsp/reset?s=t", &reset.to_string()).0.contains("204"));
    let init = gui.lsp(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    assert!(init[0]["result"]["capabilities"].is_object(), "{init:?}");
    let text = "\\undefinedname\\bye\n";
    let open = serde_json::json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": uri, "languageId": "latex", "version": 1, "text": text } } });
    assert_eq!(gui.lsp(open), Vec::<serde_json::Value>::new());
    let out = gui.lsp(serde_json::json!({ "jsonrpc": "2.0", "method": "satex/analyze", "params": {} }));
    let published = out
        .iter()
        .find(|m| m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri.as_str())
        .unwrap_or_else(|| panic!("no diagnostics for {uri}: {out:?}"));
    let said = published["params"]["diagnostics"].to_string();
    assert!(said.contains("undefinedname"), "{said}");
}

#[test]
fn gui_runs_the_shown_document_first_and_no_other_unasked() {
    // documents named before `main.tex`: a folder of them must not run them all, nor start from the first
    let doc = |n: &str| format!("\\documentclass{{article}}\\begin{{document}}\\undefined{n}\\end{{document}}\n");
    let names = ["a.tex", "b.tex", "c.tex", "d.tex", "main.tex"];
    let texts: Vec<String> = names.iter().map(|n| doc(&n[..1])).collect();
    let files: Vec<(&str, &str)> = names.iter().zip(&texts).map(|(n, t)| (*n, t.as_str())).collect();
    let gui = Gui::start("order", &files);
    let local: serde_json::Value = serde_json::from_str(&gui.page("GET", "/__satex/local", "").1).unwrap();
    assert_eq!(local["root"], "main.tex", "the root is the one the command line would take");
    let flags = ["--no-packages", "--no-classes", "--set", "load.format=false", "--set", "texmf.kpsewhich=false"];
    let args = [&flags[..], &["-f", "main.tex"][..]].concat();
    let reset = serde_json::json!({ "args": args, "cwd": gui.dir });
    gui.page("POST", "/__satex/lsp/reset?s=t", &reset.to_string());
    gui.lsp(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    let uri = |n: &str| format!("file://{}", gui.dir.join(n).to_string_lossy());
    let open = |n: &str| {
        let doc = serde_json::json!({ "uri": uri(n), "languageId": "latex", "version": 1, "text": doc(&n[..1]) });
        gui.lsp(serde_json::json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": { "textDocument": doc } }));
    };
    // The files published, in the order their runs ended: streamed lines first, then the replies.
    let analyze = |first: &str| -> Vec<String> {
        let all = gui
            .lsp(serde_json::json!({ "jsonrpc": "2.0", "method": "satex/analyze", "params": { "first": uri(first) } }));
        let published = all.iter().filter(|m| m["method"] == "textDocument/publishDiagnostics");
        published
            .filter_map(|m| m["params"]["uri"].as_str())
            .map(|u| u.rsplit('/').next().unwrap().to_string())
            .collect()
    };
    open("main.tex");
    assert_eq!(analyze("main.tex"), ["main.tex"], "only the opened document runs");
    // opened in turn, the shown one named: it runs first, then the latest opened
    for n in ["a.tex", "b.tex", "c.tex"] {
        open(n);
    }
    assert_eq!(analyze("a.tex"), ["a.tex", "c.tex", "b.tex"]);
}
