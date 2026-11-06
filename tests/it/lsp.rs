//! Drives `satex lsp` over real stdio pipes with hand-framed JSON-RPC, the
//! way an editor client would: `initialize`, `didOpen`, then a request per
//! feature. The document skips the kernel (`load.format: false`, as
//! `tests/it/engine.rs` does) so the run analyzes in milliseconds rather than
//! waiting on `latex.ltx`.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

// Without a format the catcodes are INITEX's (tex.web § 232); prepended with
// no newline so line/column positions below stay on their original lines.
const FIXTURE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 \\def\\greet#1{Hello, #1!}\n\\greet{world}\n\\def\\unused{\\greet{x}}\n\\let\\alias\\greet\n";

/// The longest a test waits for one message: a server that stops answering
/// fails its test instead of hanging the suite.
const SILENCE: Duration = Duration::from_secs(600);

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    /// The framed messages, read on a thread; `None` once stdout closed.
    messages: mpsc::Receiver<Option<Value>>,
    /// Everything the server says on stderr, drained on a thread so a full pipe never blocks it.
    stderr: Option<std::thread::JoinHandle<String>>,
    next_id: i64,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One framed message from `stdout`, `None` at its end.
fn read_message(stdout: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if stdout.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if line == "\r\n" {
            break;
        }
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            length = rest.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0u8; length.expect("a Content-Length header")];
    stdout.read_exact(&mut body).ok()?;
    Some(serde_json::from_slice(&body).expect("valid JSON-RPC"))
}

impl Server {
    fn start(cache: &std::path::Path) -> Server {
        Server::start_in(cache, std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
    }

    fn start_in(cache: &std::path::Path, cwd: &std::path::Path) -> Server {
        Server::start_with(
            cache,
            cwd,
            &[
                "--no-packages",
                "--no-classes",
                "--set",
                "load.format=false",
                "--set",
                "texmf.kpsewhich=false",
                "--set",
                "limits.seconds=0",
            ],
        )
    }

    /// Like [`Server::start_in`], with the caller's own flags instead of the
    /// default `--no-packages --no-classes --set load.format=false --set
    /// texmf.kpsewhich=false` (a test of `\usepackage`/`\documentclass`
    /// goto-definition needs the real kernel and a real search).
    fn start_with(cache: &std::path::Path, cwd: &std::path::Path, args: &[&str]) -> Server {
        let mut child = crate::support::satex()
            .args(["lsp"])
            .args(args)
            .current_dir(cwd)
            .env("SATEX_CACHE", cache)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("satex lsp starts");
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let (send, messages) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                let msg = read_message(&mut stdout);
                let end = msg.is_none();
                if send.send(msg).is_err() || end {
                    break;
                }
            }
        });
        let mut pipe = child.stderr.take().unwrap();
        let stderr = Some(std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        }));
        Server { child, stdin: Some(stdin), messages, stderr, next_id: 1 }
    }

    /// What the server said on stderr, once it has exited.
    fn said(&mut self) -> String {
        self.stderr.take().map(|t| t.join().unwrap()).unwrap_or_default()
    }

    fn write(&mut self, value: &Value) {
        let body = serde_json::to_vec(value).unwrap();
        let stdin = self.stdin.as_mut().expect("stdin is open");
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        stdin.write_all(&body).unwrap();
        stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.write(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn request(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    /// The next framed JSON-RPC message, whichever it is; the caller knows
    /// from context whether to expect a response or a notification.
    fn read(&mut self) -> Value {
        match self.messages.recv_timeout(SILENCE) {
            Ok(Some(msg)) => msg,
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = self.child.kill();
                panic!("satex lsp closed stdout: {}", self.said())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                panic!("satex lsp sent nothing for {SILENCE:?}: {}", self.said())
            }
        }
    }

    /// Reads messages until one matches `id`, the way a client ignores
    /// notifications interleaved with the response it is waiting for.
    fn response(&mut self, id: i64) -> Value {
        loop {
            let msg = self.read();
            if msg.get("id").and_then(Value::as_i64) == Some(id) {
                return msg;
            }
        }
    }

    /// Reads messages until one is the named notification.
    fn notification(&mut self, method: &str) -> Value {
        loop {
            let msg = self.read();
            if msg.get("method").and_then(Value::as_str) == Some(method) {
                return msg;
            }
        }
    }

    /// Shuts the server down and returns what it said on stderr.
    fn shutdown(mut self) -> String {
        let id = self.request("shutdown", Value::Null);
        let resp = self.response(id);
        assert!(resp.get("error").is_none(), "shutdown failed: {resp:?}");
        self.notify("exit", Value::Null);
        drop(self.stdin.take());
        for _ in 0..100 {
            if self.child.try_wait().expect("satex lsp runs").is_some() {
                return self.said();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        panic!("satex lsp did not exit after `exit`");
    }
}

fn fixture_uri(path: &std::path::Path) -> String {
    satex::project::paths::file_url(path)
}

#[test]
fn stdio_session_drives_the_feature_set() {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("greet.tex");
    std::fs::write(&file, FIXTURE).unwrap();
    let uri = fixture_uri(&file);

    let mut server = Server::start(&dir);

    let init_id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    let init = server.response(init_id);
    let capabilities = &init["result"]["capabilities"];
    assert_eq!(capabilities["hoverProvider"], json!(true));
    assert_eq!(capabilities["completionProvider"]["triggerCharacters"], json!(["\\", "{", "[", ",", "="]));
    server.notify("initialized", json!({}));

    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": FIXTURE } }),
    );

    // `didOpen` always analyzes and publishes, even with nothing to report,
    // so a fixed diagnostic is cleared instead of lingering (see `analyze`).
    let published = server.notification("textDocument/publishDiagnostics");
    assert_eq!(published["params"]["uri"], json!(uri));

    // Hover over `\greet` on line 2 (`\greet{world}`, 0-based line 1).
    let hover_id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 3 } }),
    );
    let hover = server.response(hover_id);
    let text = hover["result"]["contents"]["value"].as_str().unwrap_or_default();
    assert!(text.contains("greet"), "hover did not mention `\\greet`: {text}");
    assert!(text.contains("greet.tex"), "hover did not point at its definition site: {text}");
    assert!(text.contains("`\\greet{⟨arg1⟩}`") && !text.contains("signature"), "hover shows no usage line: {text}");

    // Completion right after the leading `\` of `\greet` offers it back.
    let completion_id = server.request(
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 1 } }),
    );
    let completion = server.response(completion_id);
    let items = completion_items(&completion["result"]);
    assert!(items.iter().any(|item| item["label"] == json!("greet")), "completion did not offer `greet`: {items:?}");

    let definition_id = server.request(
        "textDocument/definition",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 3 } }),
    );
    let definition = server.response(definition_id);
    let definition = definition["result"].as_array().cloned().unwrap_or_default();
    assert!(
        definition.iter().any(|l| l["uri"].as_str().is_some_and(|u| u.ends_with("greet.tex"))),
        "definition did not point at greet.tex: {definition:?}"
    );

    let references_id = server.request(
        "textDocument/references",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": 3 },
            "context": { "includeDeclaration": true }
        }),
    );
    let references = server.response(references_id);
    assert!(
        references["result"].as_array().is_some_and(|r| !r.is_empty()),
        "references found no use of `\\greet`: {references:?}"
    );

    let highlight_id = server.request(
        "textDocument/documentHighlight",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 3 } }),
    );
    let highlight = server.response(highlight_id);
    assert!(
        highlight["result"].as_array().is_some_and(|h| h.len() >= 2),
        "highlight missed the definition or the use: {highlight:?}"
    );

    let rename_id = server.request(
        "textDocument/rename",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 3 }, "newName": "hello" }),
    );
    let rename = server.response(rename_id);
    let edits = rename["result"]["changes"][uri.as_str()].as_array().cloned().unwrap_or_default();
    assert!(
        edits.len() >= 3 && edits.iter().all(|e| e["newText"] == json!("hello")),
        "rename did not edit both sites: {rename:?}"
    );
    assert!(
        edits.iter().any(|e| e["range"]["start"]["line"] == json!(2)),
        "rename missed the use in an unexpanded body: {rename:?}"
    );
    assert!(
        edits.iter().any(|e| e["range"]["start"]["line"] == json!(3) && e["range"]["start"]["character"] == json!(11)),
        "rename missed the name `\\let` copies: {rename:?}"
    );

    // `\hello` is free, `\alias` is not: renaming onto a name the document
    // already holds would change what it means.
    let clash_id = server.request(
        "textDocument/rename",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 3 }, "newName": "alias" }),
    );
    let clash = server.response(clash_id);
    assert!(clash["error"]["message"].as_str().is_some_and(|m| m.contains("already defined")), "{clash:?}");
    assert!(
        edits
            .iter()
            .any(|e| e["range"]
                == json!({ "start": { "line": 1, "character": 1 }, "end": { "line": 1, "character": 6 } })),
        "rename range missed the use of `\\greet`: {edits:?}"
    );

    let workspace_id = server.request("workspace/symbol", json!({ "query": "gre" }));
    let workspace = server.response(workspace_id);
    assert!(
        workspace["result"].as_array().is_some_and(|s| s.iter().any(|s| s["name"] == json!("\\greet"))),
        "workspace/symbol did not find `\\greet`: {workspace:?}"
    );

    let symbols_id = server.request("textDocument/documentSymbol", json!({ "textDocument": { "uri": uri } }));
    let symbols = server.response(symbols_id);
    let symbols = symbols["result"].as_array().cloned().unwrap_or_default();
    assert!(
        symbols.iter().any(|s| s["name"] == json!("\\greet")),
        "documentSymbol did not list `\\greet`: {symbols:?}"
    );

    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Opens `text` as `name` in a fresh directory whose file on disk says
/// something else, the way the playground's buffers differ from its files.
fn open_unsaved(tag: &str, text: &str) -> (Server, String, Value, std::path::PathBuf) {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.tex");
    std::fs::write(&file, "% the saved text differs\n").unwrap();
    let uri = fixture_uri(&file);
    let mut server = Server::start(&dir);
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": text } }),
    );
    let published = server.notification("textDocument/publishDiagnostics");
    (server, uri, published, dir)
}

/// The findings of the buffer are `satex lint`'s, info ones included,
/// though the file on disk differs.
#[test]
fn unsaved_buffer_reports_unused_definition_as_info() {
    let (server, _, published, dir) = open_unsaved("unused", FIXTURE);
    let diagnostics = published["params"]["diagnostics"].as_array().cloned().unwrap_or_default();
    let unused = diagnostics.iter().find(|d| d["data"]["rule"] == "unused-definition");
    assert!(
        unused.is_some_and(|d| d["severity"] == 3 && d["message"].as_str().unwrap().contains("\\unused")),
        "{published:?}"
    );
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hover shows a primitive's syntax only where the name still means it.
#[test]
fn hover_shows_primitive_syntax_only_while_it_is_the_primitive() {
    let text = format!("{FIXTURE}\\let\\def\\relax\n\\def\n");
    let (mut server, uri, _, dir) = open_unsaved("prim", &text);
    let mut hover = |line: u32, character: u32| {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let first = FIXTURE.find("\\def").unwrap() as u32 + 2;
    let before = hover(0, first);
    assert!(
        before.contains("\\def⟨control sequence⟩⟨parameter text⟩{⟨replacement text⟩}")
            && before.contains("TeX primitive"),
        "{before}"
    );
    let after = hover(5, 2);
    assert!(!after.contains("⟨control sequence⟩") && !after.contains("TeX primitive"), "{after}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `\\` is the control symbol LaTeX defines as a line break, never the
/// null control sequence; its hover explains it, and a glue parameter's
/// hover explains glue.
#[test]
fn hover_explains_the_line_break_and_glue() {
    let text = "\\documentclass{article}\n\\begin{document}\nA\\\\ B \\setlength{\\parskip}{1pt}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_unsaved("linebreak", text);
    let mut hover = |character: u32| {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": character } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let newline = hover(2);
    assert!(newline.contains("#### About `\\\\`") && !newline.contains("null control sequence"), "{newline}");
    let glue = hover(18);
    assert!(glue.contains("#### About *glue*"), "{glue}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A hover is kept, and the one of a name the document defines is asked
/// again once an edit redefined it: the diagnostics of the edit come after.
#[test]
fn a_kept_hover_follows_the_document_s_own_definition() {
    let catcodes = "\\catcode`\\{=1 \\catcode`\\}=2 ";
    let (mut server, uri, _, dir) = open_unsaved("kept", &format!("{catcodes}\\def\\foo{{x}}\n\\foo\n"));
    let hover = |server: &mut Server, line: u32| {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 2 } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let first = hover(&mut server, 1);
    assert!(first.contains("main.tex:1"), "{first}");
    assert_eq!(hover(&mut server, 1), first);
    server.notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": format!("{catcodes}\n\\def\\foo{{y}}\n\\foo\n") }],
        }),
    );
    server.notification("textDocument/publishDiagnostics");
    let after = hover(&mut server, 2);
    assert!(after.contains("main.tex:2"), "{after}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The hovers of names the document does not define are kept for the next
/// session, in the cache, and answered from there.
#[test]
fn hovers_are_kept_between_sessions() {
    let text = "\\catcode`\\{=1 \\catcode`\\}=2 \\def\\x{\\hfil}\n\\hfil\n";
    let (mut server, uri, _, dir) = open_unsaved("store", text);
    let hover = |server: &mut Server, uri: &str| {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 2 } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let first = hover(&mut server, &uri);
    assert!(first.contains("\\hfil"), "{first}");
    server.notify("textDocument/didClose", json!({ "textDocument": { "uri": uri } }));
    server.shutdown();
    let stored: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("hovers-")))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect();
    assert!(stored.iter().any(|s| s.contains("\"hfil\"") && !s.contains("\"x\"")), "{stored:?}");
    let (mut again, uri, _, dir) = open_unsaved("store", text);
    assert_eq!(hover(&mut again, &uri), first);
    again.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A folder opened through a symlink is answered under the symlink, though
/// the server's working directory is the resolved folder (#17), and an input
/// file is analyzed with the document that reads it.
#[cfg(unix)]
#[test]
fn a_folder_opened_through_a_symlink_keeps_its_name() {
    let base = crate::support::tmp_root().join(format!("satex-lsp-link-{}", std::process::id()));
    let (real, link) = (base.join("real"), base.join("link"));
    std::fs::create_dir_all(&real).unwrap();
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let main = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\input sub\n\\greet{world}\n";
    let sub = "\\def\\greet#1{Hello, #1!}\n";
    std::fs::write(real.join("main.tex"), main).unwrap();
    std::fs::write(real.join("sub.tex"), sub).unwrap();
    let uri = fixture_uri(&link.join("sub.tex"));

    let mut server = Server::start_in(&base, &real);
    let init_id =
        server.request("initialize", json!({ "processId": null, "rootUri": fixture_uri(&link), "capabilities": {} }));
    server.response(init_id);
    server.notify("initialized", json!({}));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": sub } }),
    );
    let published = server.notification("textDocument/publishDiagnostics");
    let named = fixture_uri(&link);
    assert!(published["params"]["uri"].as_str().is_some_and(|u| u.starts_with(&named)), "{published:?}");

    let rename_id = server.request(
        "textDocument/rename",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 6 }, "newName": "salute" }),
    );
    let rename = server.response(rename_id);
    let changes = rename["result"]["changes"].as_object().cloned().unwrap_or_default();
    let mut files: Vec<&String> = changes.keys().collect();
    files.sort();
    assert_eq!(files, [&fixture_uri(&link.join("main.tex")), &uri], "{rename:?}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&base);
}

/// #5 (LSP goto-definition on a load site): `\input`, `\include` and
/// `\usepackage` name a file; the earlier behavior pointed back at the
/// `\input` line itself as if it defined the name. It should instead open
/// the file the run resolved, at its start ([`satex::analysis::facts::Load::path`]).
mod goto_definition_on_loads {
    use super::*;

    fn kpsewhich_installed() -> bool {
        satex::project::paths::which("kpsewhich").is_ok()
    }

    fn definition_at(server: &mut Server, uri: &str, line: u32, character: u32) -> Value {
        let id = server.request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
        );
        server.response(id)
    }

    fn first_uri(def: &Value) -> Option<&str> {
        def["result"].as_array()?.first()?["uri"].as_str()
    }

    #[test]
    fn input_with_and_without_extension_opens_the_resolved_file() {
        let dir = crate::support::tmp_root().join(format!("satex-lsp-input-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let prelude = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7\n";
        let main_text = format!("{prelude}\\input{{chapter}}\n\\input{{chapter.tex}}\n");
        let main = dir.join("main.tex");
        std::fs::write(&main, &main_text).unwrap();
        let chapter = dir.join("chapter.tex");
        std::fs::write(&chapter, "").unwrap();
        let uri = fixture_uri(&main);
        let chapter_uri = fixture_uri(&chapter);

        let mut server = Server::start(&dir);
        let init_id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
        server.response(init_id);
        server.notify("initialized", json!({}));
        server.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": main_text } }),
        );
        server.notification("textDocument/publishDiagnostics");

        // `chapter` in `\input{chapter}`, line 1 (0-based), no extension.
        let def = definition_at(&mut server, &uri, 1, 9);
        assert_eq!(first_uri(&def), Some(chapter_uri.as_str()), "{def:?}");
        assert_eq!(def["result"][0]["range"]["start"], json!({ "line": 0, "character": 0 }), "{def:?}");

        // `\input` itself, same site.
        let def = definition_at(&mut server, &uri, 1, 3);
        assert_eq!(first_uri(&def), Some(chapter_uri.as_str()), "{def:?}");

        // `chapter.tex`, extension spelled out, line 2 (0-based).
        let def = definition_at(&mut server, &uri, 2, 9);
        assert_eq!(first_uri(&def), Some(chapter_uri.as_str()), "{def:?}");

        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The run stops at `\endinput`, so it never reads `parts/later.tex`.
    /// Go-to-definition still finds `\deep` two loads further by scanning
    /// the source, and hover says the analysis did not reach it.
    #[test]
    fn definition_falls_back_to_a_source_scan_past_where_the_run_stopped() {
        let dir = crate::support::tmp_root().join(format!("satex-lsp-unreached-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("parts")).unwrap();
        let prelude = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6\n";
        let main_text = format!("{prelude}\\endinput\n\\input{{parts/later}}\n\\deep\n");
        let main = dir.join("main.tex");
        std::fs::write(&main, &main_text).unwrap();
        std::fs::write(dir.join("parts/later.tex"), "\\input parts/deep\n").unwrap();
        let deep = dir.join("parts/deep.tex");
        std::fs::write(&deep, "% deep\n\\def\\deep{x}\n").unwrap();
        let uri = fixture_uri(&main);

        let mut server = Server::start(&dir);
        let init_id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
        server.response(init_id);
        server.notify("initialized", json!({}));
        server.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": main_text } }),
        );
        server.notification("textDocument/publishDiagnostics");

        let def = definition_at(&mut server, &uri, 3, 2);
        assert_eq!(first_uri(&def), Some(fixture_uri(&deep).as_str()), "{def:?}");
        assert_eq!(def["result"][0]["range"]["start"], json!({ "line": 1, "character": 4 }), "{def:?}");

        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 3, "character": 2 } }),
        );
        let hover = server.response(id);
        let text = hover["result"]["contents"]["value"].as_str().unwrap_or_default();
        assert!(text.contains("not reached by the analysis") && text.contains("deep.tex:2"), "{hover:?}");

        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `\usepackage`/`\include` are LaTeX macros, not primitives, so this
    /// needs the real kernel (`load.format: true`, the default) and a real
    /// `kpsewhich` search for `article.cls`. `satex lsp` starts with no file
    /// to detect a profile from, so it otherwise defaults to `plain`, which
    /// does not follow packages or classes at all (see `Profiles::default`).
    #[test]
    fn usepackage_and_include_open_the_resolved_file() {
        if !kpsewhich_installed() {
            return;
        }
        let dir = crate::support::tmp_root().join(format!("satex-lsp-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main_text = "\\documentclass{article}\n\\usepackage[draft]{mypkg}\n\\begin{document}\n\\include{intro}\n\\end{document}\n";
        let main = dir.join("main.tex");
        std::fs::write(&main, main_text).unwrap();
        let pkg = dir.join("mypkg.sty");
        std::fs::write(
            &pkg,
            "\\ProvidesPackage{mypkg}[2024/01/01 v1.0 test]\n\\DeclareOption{draft}{}\n\\DeclareOption{final}{}\n\\ProcessOptions\\relax\n\\RequirePackage{keyval}\n\\newcommand\\mycmd{x}\n",
        )
        .unwrap();
        let intro = dir.join("intro.tex");
        std::fs::write(&intro, "Intro text.\n").unwrap();
        let uri = fixture_uri(&main);
        let pkg_uri = fixture_uri(&pkg);
        let intro_uri = fixture_uri(&intro);

        let mut server = Server::start_with(
            &dir,
            &dir,
            &["--set", "load.packages=true", "--set", "load.classes=true", "--set", "limits.seconds=0"],
        );
        let init_id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
        server.response(init_id);
        server.notify("initialized", json!({}));
        server.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": main_text } }),
        );
        server.notification("textDocument/publishDiagnostics");

        // `mypkg` in `\usepackage[draft]{mypkg}`, line 1 (0-based).
        let def = definition_at(&mut server, &uri, 1, 21);
        assert_eq!(first_uri(&def), Some(pkg_uri.as_str()), "{def:?}");

        // Hover on the same name: what the run knows about the package.
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 21 } }),
        );
        let hover = server.response(id);
        let text = hover["result"]["contents"]["value"].as_str().unwrap_or_default();
        for part in ["mypkg", "2024/01/01", "draft (given)", "final", "mycmd", "keyval", "mypkg.sty"] {
            assert!(text.contains(part), "package hover lacks {part}: {text}");
        }
        assert_eq!(first_uri(&def), Some(pkg_uri.as_str()), "{def:?}");

        // `intro` in `\include{intro}`, line 3 (0-based).
        let def = definition_at(&mut server, &uri, 3, 11);
        assert_eq!(first_uri(&def), Some(intro_uri.as_str()), "{def:?}");

        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const COMPLETE: &str = "\\documentclass{article}
\\usepackage[shortlabels]{enumitem}
\\usepackage{pgfkeys}
\\newenvironment{myenv}{}{}
\\pgfkeys{/fa/.cd, width/.initial=1, wide/.initial=2}
\\pgfkeys{/fb/.cd, wild/.initial=3}
\\pgfkeys{/fc/.cd, wimp/.initial=4}
\\pgfkeys{/fa/.search also={/fb}}
\\def\\fa[#1]{\\pgfkeys{/fa/.cd,#1}}
\\begin{document}
\\section{A}\\label{s}
\\begin{itemize}[label=x]
\\item \\ref{s} \\fa[width=2]
\\begin{myenv}
\\end{myenv}
\\end{itemize}
\\end{document}
\\begin{}
";

/// Options of commands that read them through a key family or a default,
/// with nothing else on the line to go by.
const COMPLETE_OPTIONS: &str = "\\documentclass{article}
\\usepackage{tikz}
\\newcommand{\\x}[]{x}
\\begin{document}
\\begin{tikzpicture}[]
\\draw[] (0,0) -- (1,1);
\\end{tikzpicture}
\\end{document}
";

/// What an optional argument takes, from the run: `\newcommand`'s count
/// from its numeric default, TikZ keys for a picture and a path, never the
/// keys of an unrelated package the call graph also reaches (xcolor's color
/// models through scratch macros).
#[test]
fn completion_offers_what_an_optional_argument_takes() {
    let (mut server, uri, _, dir) = open_loaded("options", COMPLETE_OPTIONS);
    let count = complete(&mut server, &uri, 2, 16);
    assert_eq!(labels(&count).first(), Some(&"0"), "{:?}", labels(&count));
    assert!((1..=9).all(|n| labels(&count).contains(&n.to_string().as_str())), "{:?}", labels(&count));
    for (line, character) in [(4, 20), (5, 6)] {
        let keys = complete(&mut server, &uri, line, character);
        let names = labels(&keys);
        assert!(names.contains(&"thick") && names.contains(&"scale"), "{line}: {names:?}");
        assert!(!names.contains(&"natural"), "{line}: {names:?}");
    }
    drop(server);
    std::fs::remove_dir_all(dir).ok();
}

/// `\includegraphics{}` offers the project's images in the formats graphics
/// tries (`\Gin@extensions`), even before the document has any graphic.
#[test]
fn completion_offers_images_graphics_can_read() {
    let text =
        "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\includegraphics{}\n\\end{document}\n";
    let dir = crate::support::tmp_root().join(format!("satex-lsp-images-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["plot.png", "notes.txt"] {
        std::fs::write(dir.join(name), "x").unwrap();
    }
    std::fs::write(dir.join("main.tex"), text).unwrap();
    let mut server = serve(&dir, true);
    let (uri, _) = open_file(&mut server, &dir, "main.tex");
    let items = complete(&mut server, &uri, 3, "\\includegraphics{".len() as u32);
    let names = labels(&items);
    assert!(names.iter().any(|n| n.starts_with("plot")), "{names:?}");
    assert!(!names.iter().any(|n| n.starts_with("notes")), "{names:?}");
    drop(server);
    std::fs::remove_dir_all(dir).ok();
}

/// TikZ draws with what its keys hand pgf: `thick` a width, `densely dashed`
/// a dash pattern, `-Stealth` a tip and `pattern=` a fill TeX draws. Each
/// item carries its picture.
#[test]
fn completion_pictures_what_tikz_keys_draw() {
    let text = COMPLETE_OPTIONS
        .replace("\\usepackage{tikz}", "\\usepackage{tikz}\n\\usetikzlibrary{arrows.meta,patterns}")
        .replace("\\end{tikzpicture}", "\\fill[pattern=] (0,0) circle (1);\n\\end{tikzpicture}");
    let (mut server, uri, _, dir) = open_loaded("pictures", &text);
    let draw = "\\draw[".len() as u32;
    let find = |items: &[Value], label: &str| items.iter().find(|i| i["label"] == label).cloned();
    let items = complete(&mut server, &uri, 6, draw);
    let thick = find(&items, "thick").expect("thick offered");
    assert!(thick["documentation"]["value"].as_str().unwrap_or("").contains("0.8pt"), "{thick}");
    assert!(thick["data"]["preview"].as_str().is_some_and(|s| s.starts_with("<svg")), "{thick}");
    let dashed = find(&items, "densely dashed").expect("densely dashed offered");
    assert!(dashed["documentation"]["value"].as_str().unwrap_or("").contains("on 3pt off 2pt"), "{dashed}");
    assert!(dashed["data"]["preview"].as_str().is_some_and(|s| s.contains("stroke-dasharray")), "{dashed}");
    // TeX draws tips and patterns in the background: a later completion has them.
    let mut pictured = |line: u32, character: u32, label: &str| {
        (0..120).any(|_| {
            let found = find(&complete(&mut server, &uri, line, character), label);
            let found = found.unwrap_or_else(|| panic!("{label} offered"));
            let svg = found["data"]["preview"].as_str().is_some_and(|s| s.contains("<svg"));
            if !svg {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            svg
        })
    };
    assert!(pictured(6, draw, "-Stealth"), "no picture of -Stealth");
    assert!(pictured(7, "\\fill[pattern=".len() as u32, "north east lines"), "no picture of north east lines");
    drop(server);
    std::fs::remove_dir_all(dir).ok();
}

/// `\draw[-Ki`: a key TikZ reads as arrows offers the tips of a library not
/// loaded yet, and picking one loads it.
#[test]
fn arrow_tips_of_unloaded_libraries() {
    let text = COMPLETE_OPTIONS.replace("\\draw[]", "\\draw[-Ki]");
    let (mut server, uri, _, dir) = open_loaded("tips", &text);
    let items = complete(&mut server, &uri, 5, "\\draw[-Ki".len() as u32);
    let kite = items.iter().find(|i| i["label"] == "-Kite").unwrap_or_else(|| panic!("{:?}", labels(&items)));
    assert!(kite["additionalTextEdits"].to_string().contains("\\\\usetikzlibrary{arrows.meta}"), "{kite}");
    // the edit covers the `-` typed, which an editor's word would leave out
    assert_eq!(kite["textEdit"]["range"]["start"]["character"], "\\draw[".len(), "{kite}");
    drop(server);
    std::fs::remove_dir_all(dir).ok();
}

/// A completion result's items, an array or a list (LSP `CompletionList`).
fn completion_items(result: &Value) -> Vec<Value> {
    result.as_array().or_else(|| result["items"].as_array()).cloned().unwrap_or_default()
}

/// The completion items at 0-based `line`:`character`, ordered as a client
/// orders them: by `sortText`, then label.
fn complete(server: &mut Server, uri: &str, line: u32, character: u32) -> Vec<Value> {
    let id = server.request(
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
    );
    let mut items = completion_items(&server.response(id)["result"]);
    let key =
        |v: &Value| (v["sortText"].as_str().unwrap_or("").to_string(), v["label"].as_str().unwrap_or("").to_string());
    items.sort_by_key(key);
    items
}

fn labels(items: &[Value]) -> Vec<&str> {
    items.iter().filter_map(|i| i["label"].as_str()).collect()
}

/// [`open_unsaved`] with the real kernel, classes and packages, on a cache
/// the completion tests share.
fn open_loaded(tag: &str, text: &str) -> (Server, String, Value, std::path::PathBuf) {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), text).unwrap();
    let mut server = serve(&dir, true);
    let (uri, published) = open_file(&mut server, &dir, "main.tex");
    (server, uri, published, dir)
}

/// An initialized server over `dir`: with the LaTeX format, classes and
/// packages when `loaded`, else plain TeX only.
fn serve(dir: &std::path::Path, loaded: bool) -> Server {
    let mut server = match loaded {
        true => Server::start_with(&crate::support::test_cache(), dir, &["--set", "limits.seconds=0"]),
        false => Server::start(dir),
    };
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    server
}

/// `name` in `dir` opened, with the diagnostics published for it.
fn open_file(server: &mut Server, dir: &std::path::Path, name: &str) -> (String, Value) {
    let uri = fixture_uri(&dir.join(name));
    let text = std::fs::read_to_string(dir.join(name)).unwrap();
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": text } }),
    );
    let published = server.notification("textDocument/publishDiagnostics");
    (uri, published)
}

/// A `\begin{` completion adds its `\end` when only a comment or a word after a space follows the cursor.
#[test]
fn begin_completion_closes_before_trailing_text() {
    let text =
        "\\documentclass{article}\n\\begin{document}\n\\begin{ite % later\n\\begin{ite and more\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("beginend", text);
    for line in [2, 3] {
        let items = complete(&mut server, &uri, line, 10);
        let new_text = items
            .iter()
            .find(|i| i["label"] == "itemize")
            .map(|i| i["textEdit"]["newText"].as_str().unwrap_or_default().to_string());
        assert_eq!(
            new_text.as_deref(),
            Some("itemize}\n\t\\item $0\n\\end{itemize}"),
            "line {line}: {:?}",
            labels(&items)
        );
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Inside an argument the list comes from what the run saw the command do.
#[test]
fn completion_knows_the_argument_it_is_in() {
    let (mut server, uri, _, dir) = open_loaded("args", COMPLETE);
    let classes = complete(&mut server, &uri, 0, 15);
    assert!(labels(&classes).contains(&"article"), "{:?}", labels(&classes));
    let options = complete(&mut server, &uri, 1, 12);
    assert!(labels(&options).contains(&"shortlabels"), "{:?}", labels(&options));
    let begin = complete(&mut server, &uri, 17, 7);
    let snippet = |name: &str| {
        let item = begin.iter().find(|i| i["label"] == name).unwrap_or_else(|| panic!("{name} not offered"));
        item["textEdit"]["newText"].as_str().unwrap_or_default().to_string()
    };
    assert_eq!(snippet("myenv"), "myenv}\n\t$0\n\\end{myenv}");
    assert_eq!(snippet("itemize"), "itemize}\n\t\\item $0\n\\end{itemize}");
    // Mandatory arguments the begin command reads through an internal macro.
    assert_eq!(snippet("tabular"), "tabular}{$1}\n\t$0\n\\end{tabular}");
    assert_eq!(snippet("minipage"), "minipage}{$1}\n\t$0\n\\end{minipage}");
    let end = complete(&mut server, &uri, 14, 5);
    assert_eq!(labels(&end).first(), Some(&"myenv"), "{:?}", labels(&end));
    assert_eq!(labels(&end).get(1), Some(&"itemize"), "{:?}", labels(&end));
    let keys = complete(&mut server, &uri, 11, 16);
    assert!(["label", "leftmargin"].iter().all(|k| labels(&keys).contains(k)), "{:?}", labels(&keys));
    let refs = complete(&mut server, &uri, 12, 11);
    assert_eq!(labels(&refs), vec!["s"]);
    // `.search also` adds /fb after /fa; /fc is not searched.
    let pgf = labels(&complete(&mut server, &uri, 12, 18)).join(" ");
    assert!(pgf.starts_with("wide width wild"), "{pgf}");
    assert!(!pgf.contains("wimp"), "{pgf}");
    // A document author's commands before the kernel's internals, which a
    // list cut to the best may not reach; what is typed narrows it.
    let commands = complete(&mut server, &uri, 10, 1);
    let at = |name: &str| labels(&commands).iter().position(|l| *l == name);
    assert!(at("section").is_some_and(|s| at("@startsection").is_none_or(|i| s < i)), "{:?}", labels(&commands));
    // `\begin` opens its braces and asks again: the environment list.
    let begin = commands.iter().find(|i| i["label"] == "begin").expect("begin offered");
    assert_eq!(begin["insertText"], "begin{$1}");
    assert_eq!(begin["command"]["command"], "editor.action.triggerSuggest");
    let typed = complete(&mut server, &uri, 10, 3);
    assert!(labels(&typed).iter().all(|l| l.to_lowercase().contains("se")), "{:?}", labels(&typed));
    assert!(labels(&typed).contains(&"section"), "{:?}", labels(&typed));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The first `\ref{` of a document, still unclosed, offers the labels
/// the run recorded, a macro's among them.
#[test]
fn first_ref_offers_labels() {
    let text = "\\documentclass{article}\n\\newcommand{\\greet}[1]{\\label{foo:#1}Hello, #1!}\n\\begin{document}\n\\greet{world}\n\\ref{\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("firstref", text);
    let refs = complete(&mut server, &uri, 4, 5);
    assert_eq!(labels(&refs), vec!["foo:world"]);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without enumitem, `itemize` takes no keys.
#[test]
fn list_keys_need_enumitem() {
    let text = COMPLETE.replace("\\usepackage[shortlabels]{enumitem}\n", "").replace("[label=x]", "[]");
    let (mut server, uri, _, dir) = open_loaded("noenum", &text);
    let keys = complete(&mut server, &uri, 10, 16);
    assert!(!labels(&keys).contains(&"leftmargin"), "{:?}", labels(&keys));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A finding underlines the call it is about, not one character: the
/// command with its arguments.
#[test]
fn a_finding_covers_its_call() {
    let text = "\\documentclass{article}\n\\newcommand{\\section}{x}\n\\begin{document}\n\\end{document}\n";
    let (server, _, published, dir) = open_loaded("range", text);
    let diagnostics = published["params"]["diagnostics"].as_array().cloned().unwrap_or_default();
    let found = diagnostics.iter().find(|d| d["data"]["rule"] == "already-defined").expect("already-defined");
    let range = &found["range"];
    assert_eq!(range["start"], json!({ "line": 1, "character": 0 }), "{found}");
    assert_eq!(range["end"], json!({ "line": 1, "character": 24 }), "{found}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The comment above a definition documents it: in the hover, and per
/// argument in the signature help while that argument is being typed.
#[test]
fn comments_above_a_definition_document_its_arguments() {
    let text = "\\documentclass{article}\n% Greets someone.\n% #1: who is greeted\n% #2: the greeting\n\\newcommand{\\greet}[2]{#2, #1!}\n\\begin{document}\n\\greet{world}{He\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("sig", text);
    let id = server.request(
        "textDocument/signatureHelp",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 6, "character": 16 } }),
    );
    let help = server.response(id)["result"].clone();
    let signature = &help["signatures"][0];
    assert_eq!(signature["label"], "\\greet{⟨arg1⟩}{⟨arg2⟩}", "{help}");
    assert_eq!(signature["documentation"], "Greets someone.");
    assert_eq!(help["activeParameter"], 1);
    assert_eq!(signature["parameters"][1]["documentation"], "the greeting");
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 6, "character": 3 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(
        hover.contains("Greets someone. *(from the source comment)*") && hover.contains("`⟨arg1⟩` who is greeted"),
        "{hover}"
    );
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// An input file is analyzed through the root that reads it, never alone.
mod roots {
    use super::*;

    const PRELUDE: &str =
        "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\def\\documentclass#1{}\\documentclass{article}\n";

    /// A project of `files` in a fresh directory, served from there with
    /// `open` opened and its diagnostics waited out.
    fn project(tag: &str, files: &[(&str, &str)], open: &str) -> (Server, String, std::path::PathBuf) {
        project_with(tag, files, open, false)
    }

    fn project_with(
        tag: &str,
        files: &[(&str, &str)],
        open: &str,
        loaded: bool,
    ) -> (Server, String, std::path::PathBuf) {
        let dir = crate::support::tmp_root().join(format!("satex-lsp-roots-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            std::fs::write(dir.join(name), text).unwrap();
        }
        let mut server = serve(&dir, loaded);
        let (uri, _) = open_file(&mut server, &dir, open);
        (server, uri, dir)
    }

    /// Where `\greet` on the last line of `uri` is defined.
    fn greet_defined_in(server: &mut Server, uri: &str) -> String {
        let path = satex::project::paths::from_file_url(uri).unwrap();
        let line = std::fs::read_to_string(path).unwrap().lines().count() - 1;
        let id = server.request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 2 } }),
        );
        let def = server.response(id);
        def["result"][0]["uri"].as_str().unwrap_or_else(|| panic!("no definition: {def:?}")).to_string()
    }

    /// A file no root reads says so; one a root reads does not.
    #[test]
    fn a_file_no_root_reads_is_marked() {
        let main = format!("{PRELUDE}\\input{{chap}}\n");
        let files = [
            ("main.tex", main.as_str()),
            ("chap.tex", "x\n"),
            ("orphan.tex", "y\n"),
            ("plain.tex", "% !TeX program = tex\nHello\n\\bye\n"),
        ];
        let marked = |open: &str| {
            let dir = crate::support::tmp_root().join(format!("satex-lsp-orphan-{open}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            files.iter().for_each(|(name, text)| std::fs::write(dir.join(name), text).unwrap());
            let mut server = serve(&dir, false);
            let (_, published) = open_file(&mut server, &dir, open);
            server.shutdown();
            let _ = std::fs::remove_dir_all(&dir);
            published.to_string().contains("unreachable-file")
        };
        assert!(marked("orphan.tex"));
        assert!(!marked("chap.tex"));
        // a plain TeX root is a document of its own
        assert!(!marked("plain.tex"));
    }

    /// A package no root loads is analyzed as a package: its `@` names are
    /// names and its preamble commands raise nothing.
    #[test]
    fn a_package_no_root_loads_is_its_own_run() {
        if !crate::support::installed() {
            return;
        }
        let sty = "\\ProvidesPackage{mine}\n\\newif\\ifmy@draft\n\\DeclareOption{draft}{\\my@drafttrue}\n\\ProcessOptions\\relax\n";
        let dir = crate::support::tmp_root().join("satex-lsp-unloaded-package");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let main = "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";
        std::fs::write(dir.join("main.tex"), main).unwrap();
        std::fs::write(dir.join("mine.sty"), sty).unwrap();
        let mut server = serve(&dir, true);
        let (_, published) = open_file(&mut server, &dir, "mine.sty");
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
        let text = published.to_string();
        assert!(!text.contains("only in preamble") && !text.contains("undefined-control-sequence"), "{text}");
    }

    #[test]
    fn an_input_file_is_read_through_its_root() {
        let main = format!("{PRELUDE}\\def\\greet#1{{Hello, #1!}}\n\\input{{chap}}\n");
        let (mut server, uri, dir) =
            project("input", &[("main.tex", &main), ("chap.tex", "\\greet{world}\n")], "chap.tex");
        assert_eq!(greet_defined_in(&mut server, &uri), fixture_uri(&dir.join("main.tex")));
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A root whose run never reaches the file (here it ends first) still
    /// lends it its preamble: the file is read after it, so what the
    /// preamble defines is defined there.
    #[test]
    fn an_input_no_root_reaches_is_read_after_its_root_s_preamble() {
        let main = "\\documentclass{article}\n\\newcommand\\greet[1]{Hello, #1!}\n\\begin{document}\nx\n\\end{document}\n\\input{chap}\n";
        let (mut server, uri, dir) =
            project_with("unreached", &[("main.tex", main), ("chap.tex", "\\greet{world}\n")], "chap.tex", true);
        assert_eq!(greet_defined_in(&mut server, &uri), fixture_uri(&dir.join("main.tex")));
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `satex/query` answers from the open buffer's analysis, not the
    /// file on disk: the playground's explain panel asks it on hover.
    #[test]
    fn a_query_answers_from_the_open_buffer() {
        let main = format!("{PRELUDE}\\def\\greet{{disk}}\n");
        let (mut server, uri, dir) = project("query", &[("main.tex", &main)], "main.tex");
        let text = format!("{PRELUDE}\\def\\greet{{buffer}}\n");
        server.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": text }] }),
        );
        let file = dir.join("main.tex").display().to_string();
        let request = json!({ "file": file, "queries": [{ "type": "explain", "names": ["\\greet"] }] });
        let id =
            server.request("workspace/executeCommand", json!({ "command": "satex/query", "arguments": [request] }));
        let reply = server.response(id);
        assert!(reply["result"][0].to_string().contains("buffer"), "{reply}");
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The playground's trace of a call: `\begin{document}` after a package,
    /// one there and one missing, still runs and records its steps.
    #[test]
    fn a_trace_of_begin_document_records_its_steps() {
        for package in ["xcolor", "nosuchpackage"] {
            let main = format!(
                "\\documentclass{{article}}\n\\usepackage{{{package}}}\n\\begin{{document}}\nx\n\\end{{document}}\n"
            );
            let (mut server, _, dir) = project_with("trace-begin", &[("main.tex", &main)], "main.tex", true);
            let line = main.lines().position(|l| l.starts_with("\\begin{document}")).unwrap() + 1;
            let config = json!({ "trace": "true", "trace_document_only": "false", "trace_lines": format!("[{line}, {line}]"),
                                 "trace_col": "1", "trace_file": "main.tex" });
            let request = json!({ "file": dir.join("main.tex").display().to_string(), "config": config, "queries": [{ "type": "trace" }] });
            let id =
                server.request("workspace/executeCommand", json!({ "command": "satex/query", "arguments": [request] }));
            let reply = server.response(id);
            let records = reply["result"][0]["records"].as_array().cloned().unwrap_or_default();
            assert!(records.len() > 1, "{package}: {reply}");
            server.shutdown();
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// `a.tex` names the file but does not read it; of `b.tex` and `c.tex`,
    /// which both read it, the likelier one answers, and keeps answering.
    #[test]
    fn of_two_roots_sharing_a_file_the_one_seen_reading_it_answers() {
        let a = format!("{PRELUDE}% \\input{{shared}}\n\\def\\greet#1{{a}}\n");
        let b = format!("{PRELUDE}\\def\\greet#1{{b}}\n\\input{{shared}}\n");
        let c = format!("{PRELUDE}\\def\\greet#1{{c}}\n\\input{{shared}}\n");
        let files = [("a.tex", a.as_str()), ("b.tex", &b), ("c.tex", &c), ("shared.tex", "\\greet{world}\n")];
        let (mut server, uri, dir) = project("shared", &files, "shared.tex");
        let b_uri = fixture_uri(&dir.join("b.tex"));
        assert_eq!(greet_defined_in(&mut server, &uri), b_uri);
        server.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": "\\greet{you}\n" }] }),
        );
        server.notification("textDocument/publishDiagnostics");
        assert_eq!(greet_defined_in(&mut server, &uri), b_uri);
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A part a package inputs by a name with a parameter
    /// (`\input{parts-#1.code}`) is read through the document that loads
    /// the package, not as a document of its own.
    #[test]
    fn a_package_part_is_read_through_the_document_using_the_package() {
        let main = format!("{PRELUDE}\\input{{parts.sty}}\n");
        let sty = "\\def\\greet#1{Hello, #1!}\n\\def\\load#1{\\input{parts-#1.code}}\n\\load{one}\n";
        let files = [("main.tex", main.as_str()), ("parts.sty", sty), ("parts-one.code", "\\greet{world}\n")];
        let (mut server, uri, dir) = project("parts", &files, "parts-one.code");
        assert_eq!(greet_defined_in(&mut server, &uri), fixture_uri(&dir.join("parts.sty")));
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Without a document that reads it, a package's part is read after
    /// the package is loaded, as its own code.
    #[test]
    fn a_package_part_without_a_document_is_read_after_its_package() {
        if !crate::support::installed() {
            return;
        }
        let sty = "\\ProvidesPackage{parts}\n\\def\\greet#1{Hello, #1!}\n\\def\\load#1{\\input{parts-#1.code}}\n";
        let files = [("parts.sty", sty), ("parts-one.code", "\\greet{world}\n")];
        let (mut server, uri, dir) = project("partsonly", &files, "parts-one.code");
        assert_eq!(greet_defined_in(&mut server, &uri), fixture_uri(&dir.join("parts.sty")));
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_magic_root_comment_wins() {
        let b = format!("{PRELUDE}\\def\\greet#1{{b}}\n\\input{{shared}}\n");
        let c = format!("{PRELUDE}\\def\\greet#1{{c}}\n\\input{{shared}}\n");
        let shared = "% !TeX root = c.tex\n\\greet{world}\n";
        let files = [("b.tex", b.as_str()), ("c.tex", &c), ("shared.tex", shared)];
        let (mut server, uri, dir) = project("magic", &files, "shared.tex");
        assert_eq!(greet_defined_in(&mut server, &uri), fixture_uri(&dir.join("c.tex")));
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The same text sent again runs nothing: no diagnostics arrive before
    /// the answer to a request sent after the debounce.
    #[test]
    fn the_same_text_again_does_not_run_again() {
        let main = format!("{PRELUDE}\\def\\greet#1{{Hello, #1!}}\n\\def\\unused{{}}\n");
        let (mut server, uri, dir) = project("same", &[("main.tex", &main)], "main.tex");
        let change = |server: &mut Server, version: i64, text: &str| {
            server.notify(
                "textDocument/didChange",
                json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }),
            )
        };
        // no run: at most the last run's findings again, now of the newest version
        let mut last: Option<Value> = None;
        let mut quiet = |server: &mut Server, newest: i64| {
            std::thread::sleep(Duration::from_millis(800));
            let id = server.request("textDocument/documentSymbol", json!({ "textDocument": { "uri": uri } }));
            loop {
                let msg = server.read();
                if msg["method"] == "textDocument/publishDiagnostics" {
                    assert_eq!(msg["params"]["version"], newest, "ran again: {msg:?}");
                    let found = &msg["params"]["diagnostics"];
                    assert!(last.as_ref().is_none_or(|l| l == found), "ran again: {msg:?}");
                    last = Some(found.clone());
                }
                if msg["id"] == id {
                    break;
                }
            }
        };
        for version in 2..5 {
            change(&mut server, version, &main);
        }
        quiet(&mut server, 4);
        // An edit and its undo before the debounce ends: the text analyzed.
        change(&mut server, 5, "% edited\n");
        change(&mut server, 6, &main);
        quiet(&mut server, 6);
        change(&mut server, 7, "% edited\n");
        server.notification("textDocument/publishDiagnostics");
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Keys complete inside a key list: a `/path` lists the keys under it, and a
/// command that hands its argument to `\pgfkeys` offers its family's keys.
#[test]
fn completion_offers_keys_in_key_lists() {
    let text = "\\documentclass{article}\n\\usepackage{pgfkeys}\n\\pgfkeys{/demo/.is family, /demo,\n  width/.initial=2cm,\n  mode/.is choice, mode/fast/.code={}, mode/safe/.code={},\n}\n\\newcommand\\demo[1]{\\pgfkeys{/demo/.cd,#1}}\n\\begin{document}\n\\pgfkeys{/demo/}\n\\demo{}\n\\demo{width=1cm}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("keys", text);
    let paths = complete(&mut server, &uri, 8, 15);
    assert!(labels(&paths).contains(&"/demo/width"), "{:?}", labels(&paths));
    assert!(labels(&paths).contains(&"/demo/mode"), "{:?}", labels(&paths));
    let family = complete(&mut server, &uri, 9, 6);
    assert!(labels(&family).contains(&"width"), "{:?}", labels(&family));
    // Subkeys come a few levels deep, to be found by any part of their path.
    assert!(labels(&paths).contains(&"/demo/mode/fast"), "{:?}", labels(&paths));
    assert!(labels(&family).contains(&"mode/safe"), "{:?}", labels(&family));
    std::fs::remove_dir_all(dir).ok();
}

/// Inside `\cite{` the databases' entries are offered, found by a word of
/// the title as well as by key, with author and year beside the key.
#[test]
fn citation_keys_come_from_the_databases() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\cite{}\n\\bibliography{refs}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("cite", text);
    std::fs::write(
        dir.join("refs.bib"),
        "@article{knuth84, author = {Donald E. Knuth}, title = {Literate Programming}, year = 1984}\n",
    )
    .unwrap();
    let items = complete(&mut server, &uri, 2, 6);
    let entry = items.iter().find(|i| i["label"] == "knuth84").unwrap_or_else(|| panic!("{:?}", labels(&items)));
    assert_eq!(entry["detail"], "Knuth (1984)");
    assert!(entry["filterText"].as_str().unwrap_or_default().contains("Literate Programming"), "{entry}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// What an argument is comes with the command ([`satex::analysis::roles`]): a
/// wrapper that hands its first argument to `\ref` behind a prefix
/// completes labels there, without the prefix, and not in its second.
#[test]
fn a_ref_wrapper_completes_labels_in_its_reference_argument_only() {
    let text = "\\documentclass{article}\n\\newcommand\\secref[2]{#2 \\ref{sec:#1}}\n\\begin{document}\n\\section{A}\\label{sec:intro}\n\\secref{}{}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("refwrap", text);
    let first = complete(&mut server, &uri, 4, 8);
    assert!(labels(&first).contains(&"intro"), "{:?}", labels(&first));
    let second = complete(&mut server, &uri, 4, 10);
    assert!(!labels(&second).iter().any(|l| l.contains("intro")), "{:?}", labels(&second));
    std::fs::remove_dir_all(dir).ok();
}

/// A `\ref` the document redefined is what its new body says: no labels.
#[test]
fn a_redefined_ref_completes_no_labels() {
    let text = "\\documentclass{article}\n\\renewcommand\\ref[1]{#1}\n\\begin{document}\n\\section{A}\\label{intro}\n\\ref{}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("refnew", text);
    let items = complete(&mut server, &uri, 4, 5);
    assert!(!labels(&items).contains(&"intro"), "{:?}", labels(&items));
    std::fs::remove_dir_all(dir).ok();
}

/// What TeX reads each argument as ([`satex::analysis::roles::Ty`]) comes with the
/// signature: the hover lists it per argument, the signature help per
/// parameter.
#[test]
fn hover_and_signature_help_show_argument_types() {
    let text = "\\documentclass{article}\n\\newlength\\mylen\n\\newcommand\\setw[1]{\\setlength{\\mylen}{#1}}\n\\begin{document}\n\\setw{3pt}\n\\setw{1em}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("types", text);
    let id = server.request(
        "textDocument/signatureHelp",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 5, "character": 7 } }),
    );
    let help = server.response(id)["result"].clone();
    assert_eq!(help["signatures"][0]["parameters"][0]["documentation"], "glue", "{help}");
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 4, "character": 2 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("- `⟨arg1⟩` glue"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A command's hover says what the call at the cursor typesets, under the
/// signature: other calls belong to their own hovers.
#[test]
fn hover_shows_what_the_call_typesets() {
    let text = "\\documentclass{article}\n\\newcommand\\hi[1]{Hello #1}\n\\begin{document}\nGar\\c{c}on \\hi{you}.\n\\hi{me} \\hi{you}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("typesets", text);
    let hover = |server: &mut Server, character: u32| {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 3, "character": character } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let hi = hover(&mut server, 12);
    assert!(hi.contains("**Typesets `Hello you` here**") && !hi.contains("Hello me"), "{hi}");
    assert!(hi.find("`\\hi") < hi.find("Typesets"), "the signature leads: {hi}");
    let c = hover(&mut server, 4);
    assert!(c.contains("Typesets `") && c.contains(" here**"), "{c}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// After an edit, a call's hover says what the new run typesets, not what
/// the run before set.
#[test]
fn a_call_hover_follows_the_edit() {
    let text = "\\documentclass{article}\n\\begin{document}\nSee \\ref{a}.\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("typesets-edit", text);
    let hover = |server: &mut Server| {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": 6 } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let before = hover(&mut server);
    assert!(before.contains("Typesets `??`"), "{before}");
    let labelled = text.replace("\\begin{document}\n", "\\begin{document}\n\\section{A}\\label{a}\n");
    server.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": labelled }] }),
    );
    server.notification("textDocument/publishDiagnostics");
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 3, "character": 6 } }),
    );
    let after = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(after.contains("Typesets `1`"), "{after}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

const BIB: &str = "@book{lamport94,\n  title = {{\\LaTeX}: A System},\n  year = 1994,\n}\n\n@\n\n@article{k,\n  \n  title = {x \\},\n}\n";

/// A `.bib` file the document reads, opened beside it.
fn open_bib(tag: &str) -> (Server, String, std::path::PathBuf) {
    let text = "\\documentclass{article}\n\\begin{document}\nSee \\cite{lamport94}.\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    let (mut server, _, _, dir) = open_loaded(tag, text);
    std::fs::write(dir.join("refs.bib"), BIB).unwrap();
    let (uri, _) = open_file(&mut server, &dir, "refs.bib");
    (server, uri, dir)
}

/// A command in a `.bib` field hovers as it does in a `.tex` file.
#[test]
fn hover_works_in_a_bib_field() {
    let (mut server, uri, dir) = open_bib("bibhover");
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 14 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("LaTeX"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// After `@` the entry types of the bibliography style, inside an entry its fields.
#[test]
fn completion_in_a_bib_file_offers_types_and_fields() {
    let (mut server, uri, dir) = open_bib("bibcomplete");
    let types = complete(&mut server, &uri, 5, 1);
    assert!(["article", "book", "inproceedings"].iter().all(|t| labels(&types).contains(t)), "{:?}", labels(&types));
    let fields = complete(&mut server, &uri, 8, 2);
    assert!(["author", "title", "year"].iter().all(|t| labels(&fields).contains(t)), "{:?}", labels(&fields));
    let commands = complete(&mut server, &uri, 9, 14);
    assert!(labels(&commands).contains(&"cite"), "{:?}", labels(&commands));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A command with several definitions shows the one in force at the cursor,
/// the others folded under a heading; each usage line carries its own types.
#[test]
fn hover_shows_the_active_definition_and_folds_the_others() {
    let text = "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("active", text);
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 4 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    let (main, more) = hover.split_once("#### More definitions").unwrap_or((&hover, ""));
    assert!(main.contains(": preamble"), "{hover}");
    assert!(main.contains("- `⟨arg1⟩` options") && main.contains("- `⟨arg2⟩` class (.cls)"), "{hover}");
    assert!(more.contains(": document") && !more.contains(": preamble"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// What a quantity is, from the knowledge base, under the hover of a command
/// that reads one; the completion detail of a command carries the types.
#[test]
fn hover_explains_quantities_and_completion_types_arguments() {
    let text = "\\documentclass{article}\n\\newlength\\mylen\n\\begin{document}\n\\setlength{\\mylen}{3pt}\n\\setle\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("about", text);
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 3, "character": 3 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("#### About *glue*") && hover.contains("plus"), "{hover}");
    let items = complete(&mut server, &uri, 4, 6);
    let detail = items.iter().find(|i| i["label"] == "setlength").and_then(|i| i["detail"].as_str());
    assert!(detail.is_some_and(|d| d.contains("{⟨skip register⟩}{⟨glue⟩}")), "{detail:?}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The style a `\bibliographystyle` names is hovered with what it does.
#[test]
fn hover_shows_the_bibliography_style() {
    let text = "\\documentclass{article}\n\\begin{document}\nx\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("bst", text);
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 3, "character": 20 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("BibTeX style") && hover.contains("sorted by author"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// biblatex's `style=` option is hovered like a BibTeX style.
#[test]
fn hover_shows_the_biblatex_style() {
    let text = "\\documentclass{article}\n\\usepackage[style=authoryear]{biblatex}\n\\addbibresource{refs.bib}\n\\begin{document}\nx\n\\printbibliography\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("blxstyle", text);
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 22 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("biblatex style") && hover.contains("author-year"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file argument completes the files of its extension that start with
/// what is typed: the project's databases, the installation's styles.
#[test]
fn file_arguments_complete_their_files_by_extension() {
    let text = "\\documentclass{article}\n\\begin{document}\nx\n\\bibliographystyle{pl}\n\\bibliography{re}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("bibfiles", text);
    for name in ["refs", "rest", "other"] {
        std::fs::write(dir.join(format!("{name}.bib")), "").unwrap();
    }
    let bibs = complete(&mut server, &uri, 4, 16);
    assert!(["refs", "rest"].iter().all(|n| labels(&bibs).contains(n)), "{:?}", labels(&bibs));
    assert!(labels(&bibs).iter().all(|l| l.starts_with("re")), "{:?}", labels(&bibs));
    let styles = complete(&mut server, &uri, 3, 21);
    assert!(labels(&styles).contains(&"plain"), "{:?}", labels(&styles));
    assert!(labels(&styles).iter().all(|l| l.starts_with("pl")), "{:?}", labels(&styles));
    server.shutdown();
    let blx =
        "\\documentclass{article}\n\\usepackage[style=authoryear]{biblatex}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir2) = open_loaded("bbxfiles", blx);
    let bbx = complete(&mut server, &uri, 1, 20);
    assert!(["authoryear", "authortitle"].iter().all(|n| labels(&bbx).contains(n)), "{:?}", labels(&bbx));
    assert!(labels(&bbx).iter().all(|l| l.starts_with("au")), "{:?}", labels(&bbx));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

/// An installed package completes with its date and version, as its
/// identification line says, and a link to its documentation.
#[test]
fn packages_complete_with_their_version_and_documentation() {
    let Ok(path) = satex::project::paths::which("kpsewhich").map_err(|_| ()).and_then(|k| {
        let out = std::process::Command::new(k).arg("amsmath.sty").output().map_err(|_| ())?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }) else {
        return;
    };
    let text =
        "\\documentclass{article}\n\\usepackage{amsm}\n\\usepackage{fonts}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("pkgversions", text);
    let items = complete(&mut server, &uri, 1, "\\usepackage{amsm".len() as u32);
    let amsmath = items.iter().find(|i| i["label"] == "amsmath").cloned().unwrap_or_default();
    // the date the file names in `\ProvidesPackage{amsmath}[…]`
    let source = std::fs::read_to_string(&path).unwrap_or_default();
    let date =
        source.split("\\ProvidesPackage{amsmath}[").nth(1).and_then(|r| r.split_whitespace().next()).unwrap_or("?");
    assert!(amsmath["detail"].as_str().is_some_and(|d| d.contains(date)), "{amsmath}");
    assert!(amsmath["documentation"].to_string().contains("texdoc.org/pkg/amsmath"), "{amsmath}");
    assert!(!amsmath["detail"].to_string().contains("needs"), "{amsmath}");
    // fontspec says it "requires either XeTeX or LuaTeX": under pdfTeX it says so, and comes last
    let items = complete(&mut server, &uri, 2, "\\usepackage{fonts".len() as u32);
    let needs = |i: &Value| i["detail"].as_str().is_some_and(|d| d.contains("needs XeTeX or LuaTeX"));
    if let Some(at) = items.iter().position(|i| i["label"] == "fontspec") {
        assert!(needs(&items[at]), "{}", items[at]);
        assert!(items[at..].iter().all(needs), "{:?}", labels(&items));
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A library argument completes the library names the installation has:
/// the run saw `\\usetikzlibrary{calc}` load `tikzlibrarycalc.code.tex`, so
/// its argument names what stands between that prefix and suffix.
#[test]
fn library_arguments_complete_their_libraries() {
    if !crate::support::installed() {
        return;
    }
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{calc,posi}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("libraries", text);
    let items = complete(&mut server, &uri, 2, "\\usetikzlibrary{calc,posi".len() as u32);
    assert!(labels(&items).contains(&"positioning"), "{:?}", labels(&items));
    assert!(labels(&items).iter().all(|l| l.starts_with("posi")), "{:?}", labels(&items));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    // the first call, empty: what the argument loads is known all the same, no pgf keys
    let text =
        "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("libraries-empty", text);
    let items = complete(&mut server, &uri, 2, "\\usetikzlibrary{".len() as u32);
    assert!(labels(&items).contains(&"positioning"), "{:?}", labels(&items));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A value the run writes into a build file another tool reads takes that
/// tool's input type: biblatex's `\addbibresource` and a user wrapper of
/// `\bibliography` complete databases, with no command named.
#[test]
fn databases_complete_by_the_build_file_they_reach() {
    let wrap = "\\documentclass{article}\n\\newcommand\\mybib[1]{\\bibliography{#1}}\n\\begin{document}\nx\n\\mybib{re}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("bibwrap", wrap);
    std::fs::write(dir.join("refs.bib"), "").unwrap();
    let items = complete(&mut server, &uri, 4, 9);
    assert!(labels(&items).contains(&"refs"), "{:?}", labels(&items));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    if !crate::support::installed() {
        return;
    }
    let blx = "\\documentclass{article}\n\\usepackage{biblatex}\n\\addbibresource{re}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("bibresource", blx);
    std::fs::write(dir.join("refs.bib"), "").unwrap();
    let items = complete(&mut server, &uri, 2, 18);
    assert!(labels(&items).iter().any(|l| l.starts_with("refs")), "{:?}", labels(&items));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Files of two kinds join to a file of no kind, and a file is a name.
#[test]
fn file_kinds_join_to_a_file() {
    use satex::analysis::roles::{FileKind as F, KeyKind as K, Ty};
    assert_eq!(Ty::File(F::Bib).join(Ty::File(F::Tex)), Ty::File(F::Any));
    assert_eq!(Ty::File(F::Bib).join(Ty::File(F::Bib)), Ty::File(F::Bib));
    assert_eq!(Ty::File(F::Bib).join(Ty::Key(K::Any)), Ty::Name);
    assert_eq!(Ty::Key(K::Label).join(Ty::Key(K::Cite)), Ty::Key(K::Any));
    assert_eq!(Ty::Key(K::Label).to_string(), "label");
    assert!(Ty::File(F::Bst).leq(Ty::File(F::Any)));
    assert_eq!(Ty::File(F::Bib).to_string(), "bibliography database (.bib)");
    assert_eq!(Ty::File(F::Any).to_string(), "file");
}

/// A reference takes labels, a citation takes entries: the signature says
/// which, and neither completion offers the other's keys.
#[test]
fn keys_are_labels_or_citations() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:intro}\\cite{a} \\ref{} \\cite{}\n\\begin{thebibliography}{9}\\bibitem{knuth} K.\\end{thebibliography}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("keykinds", text);
    let at = text.lines().nth(2).unwrap();
    let col = |pat: &str| (at.find(pat).unwrap() + pat.len()) as u32;
    let refs = complete(&mut server, &uri, 2, col("\\ref{"));
    assert!(labels(&refs).contains(&"sec:intro") && !labels(&refs).contains(&"knuth"), "{:?}", labels(&refs));
    let cites = complete(&mut server, &uri, 2, col("\\ref{} \\cite{"));
    assert!(labels(&cites).contains(&"knuth") && !labels(&cites).contains(&"sec:intro"), "{:?}", labels(&cites));
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": col("\\ref") - 2 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("- `⟨arg1⟩` label") && hover.contains("#### About *label*"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file argument leads to its file: the `\input` to `.tex`, the database
/// to `.bib`, by the argument's type and not the command's name.
#[test]
fn definition_on_a_file_argument_opens_the_file() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\bibliography{refs}\n\\input{chapter}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("filedef", text);
    std::fs::write(dir.join("chapter.tex"), "x\n").unwrap();
    std::fs::write(dir.join("refs.bib"), "").unwrap();
    for (line, col, file) in [(3, 9, "chapter.tex"), (2, 16, "refs.bib")] {
        let id = server.request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": col } }),
        );
        let result = server.response(id)["result"].clone();
        assert!(result.to_string().contains(file), "{file}: {result}");
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A TikZ key in an option list leads to where its library defines it.
#[test]
fn definition_on_a_tikz_key_opens_its_library() {
    if !crate::support::installed() {
        return;
    }
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\tikz\\draw[above] (0,0) -- (1,1);\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("tikzkey", text);
    let id = server.request(
        "textDocument/definition",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 3, "character": 14 } }),
    );
    let result = server.response(id)["result"].to_string();
    assert!(result.contains("tikz.code.tex"), "{result}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A key in `\\draw[…]` resolves to the `\\tikzset` line that defines it: hover shows its comment
/// and lattice type, go-to-definition opens the key's own line, a key no family stores is the
/// family's `.unknown` handler's.
#[test]
fn a_tikz_key_resolves_to_its_definition_and_comment() {
    if !crate::support::installed() {
        return;
    }
    let lines = [
        "\\documentclass{article}",
        "\\usepackage{tikz}",
        "% A thick red line.",
        "\\tikzset{wire/.style={thick,red}}",
        "\\tikzset{",
        "  % The arrow at the end.",
        "  tip/.style={->},",
        "}",
        "\\begin{document}",
        "\\tikz\\draw[wire, tip, blue] (0,0) -- (1,1);",
        "\\end{document}",
    ];
    let (mut server, uri, _, dir) = open_loaded("tikzkeydoc", &(lines.join("\n") + "\n"));
    let use_line = lines.iter().position(|l| l.contains("draw[")).unwrap() as u32;
    let mut ask = |method: &str, word: &str| {
        let character = (lines[use_line as usize].find(word).unwrap() + 1) as u32;
        let id = server.request(
            method,
            json!({ "textDocument": { "uri": uri }, "position": { "line": use_line, "character": character } }),
        );
        server.response(id)["result"].clone()
    };
    let hover = |r: Value| r["contents"]["value"].as_str().unwrap_or_default().to_string();
    let wire = hover(ask("textDocument/hover", "wire"));
    assert!(
        wire.contains("A thick red line. *(from the source comment)*") && wire.contains("*pgf key* `/tikz/wire`"),
        "{wire}"
    );
    assert!(hover(ask("textDocument/hover", "tip")).contains("The arrow at the end."));
    assert!(hover(ask("textDocument/hover", "blue")).contains("`.unknown` handler"));
    let at = ask("textDocument/definition", "tip");
    let tip_line = lines.iter().position(|l| l.contains("tip/.style")).unwrap();
    assert_eq!(at[0]["range"]["start"]["line"], tip_line, "{at}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pgf-style block a blank line above a definition: its parameters and example show on hover.
#[test]
fn a_block_a_blank_line_above_documents_with_its_example() {
    let lines = [
        "\\documentclass{article}",
        "% Return a point",
        "%",
        "% #1 = x-coordinate",
        "%",
        "% Example:",
        "%",
        "% \\point{2pt}",
        "",
        "\\newcommand\\point[1]{(#1)}",
        "\\begin{document}",
        "\\point{1pt}",
        "\\end{document}",
    ];
    let (mut server, uri, _, dir) = open_loaded("pgfdoc", &(lines.join("\n") + "\n"));
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 11, "character": 3 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("Return a point *(from the source comment)*"), "{hover}");
    assert!(hover.contains("`⟨arg1⟩` x-coordinate") && hover.contains("```tex\n\\point{2pt}\n```"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A braced name closes its brace unless one follows, and replaces the rest
/// of the word under the cursor.
#[test]
fn a_completed_name_closes_its_brace() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:intro}\n\\ref{sec:in\n\\ref{sec:in}\n\\ref{sec:inxx}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("closing", text);
    let edit = |server: &mut Server, line, col| {
        let items = complete(server, &uri, line, col);
        let it = items.iter().find(|i| i["label"] == "sec:intro").unwrap_or_else(|| panic!("{:?}", labels(&items)));
        (
            it["textEdit"]["newText"].as_str().unwrap_or_default().to_string(),
            it["textEdit"]["range"]["end"]["character"].clone(),
        )
    };
    assert_eq!(edit(&mut server, 3, 11).0, "sec:intro}");
    assert_eq!(edit(&mut server, 4, 11).0, "sec:intro");
    assert_eq!(edit(&mut server, 5, 11), ("sec:intro".to_string(), json!(13)));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Database and style arguments show as files in the usage, by the build
/// file the run writes them to.
#[test]
fn bibliography_arguments_are_typed_as_files() {
    let text = "\\documentclass{article}\n\\begin{document}\nx\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("bibtypes", text);
    for line in [3, 4] {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 5 } }),
        );
        let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
        let kind = if line == 3 { "bibliography style (.bst)" } else { "bibliography database (.bib)" };
        assert!(hover.contains(&format!("- `⟨arg1⟩` {kind}")) && !hover.contains("#### About *key*"), "{hover}");
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// An argument TeX reads as a length narrows what a name there may be:
/// the length registers come before the rest.
#[test]
fn a_length_argument_completes_lengths_first() {
    let text = "\\documentclass{article}\n\\newlength\\zlen\n\\newcommand\\zcmd{}\n\\begin{document}\n\\zcmd\\zcmd\\zcmd\\setlength{\\zlen}{1pt}\n\\setlength{\\z}{2pt}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("lengths", text);
    let items = complete(&mut server, &uri, 5, 13);
    assert_eq!(labels(&items).first(), Some(&"zlen"), "{:?}", labels(&items));
    assert!(labels(&items).contains(&"zcmd"), "{:?}", labels(&items));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pgfkeys list completes keys and values from the keys the run stored:
/// a `/path` the keys under it, `key=` a choice key's choices or a switch's
/// `true`/`false`, a family named before the keys under it.
#[test]
fn key_lists_complete_paths_choices_and_family_keys() {
    let text = "\\documentclass{article}\n\\usepackage{pgfkeys}\n\\newif\\ifdemodraft\n\\pgfkeys{/demo/.is family, /demo,\n  width/.initial=2cm,\n  color/.style={},\n  draft/.is if=demodraft,\n  mode/.is choice, mode/fast/.code={}, mode/safe/.code={},\n  label/.code 2 args={},\n}\n\\begin{document}\n\\pgfkeys{/demo/mode=fast, /demo/}\n\\pgfkeys{/demo/.cd, mode=}\n\\pgfkeys{/demo/draft=}\n\\pgfkeys{/demo, wi}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("keylist", text);
    let paths = complete(&mut server, &uri, 11, 32);
    for key in ["/demo/width", "/demo/color", "/demo/draft", "/demo/mode", "/demo/label"] {
        assert!(labels(&paths).contains(&key), "{key}: {:?}", labels(&paths));
    }
    // Subkeys follow, after the keys of the directory itself.
    let order = |key: &str| paths.iter().find(|i| i["label"] == key).map(|i| i["sortText"].to_string());
    assert!(order("/demo/mode/fast") > order("/demo/width"), "{paths:?}");
    assert_eq!(paths[0]["textEdit"]["range"]["start"]["character"], 26, "{}", paths[0]);
    // A key that takes a value ends in `=` with the cursor after it; a switch does not.
    let inserted = |key: &str| paths.iter().find(|i| i["label"] == key).map(|i| i["textEdit"]["newText"].clone());
    assert_eq!(inserted("/demo/width"), Some(json!("/demo/width=$1")), "{paths:?}");
    assert_eq!(inserted("/demo/draft"), Some(json!("/demo/draft")), "{paths:?}");
    let choices = complete(&mut server, &uri, 12, 25);
    assert_eq!(labels(&choices), ["fast", "safe"]);
    let switch = complete(&mut server, &uri, 13, 21);
    assert_eq!(labels(&switch), ["false", "true"]);
    let family = complete(&mut server, &uri, 14, 18);
    assert!(labels(&family).contains(&"width"), "{:?}", labels(&family));
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Hovering a key in an argument that cites shows its database entry, in
/// `\cite` and in a wrapper that hands its argument on.
#[test]
fn hovering_a_citation_shows_its_entry() {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-citehover-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("refs.bib"),
        "@article{knuth84, author = {Donald E. Knuth}, title = {Literate Programming}, journal = {The Computer Journal}, year = 1984}\n",
    )
    .unwrap();
    let text = "\\documentclass{article}\n\\newcommand\\mycite[1]{see \\cite{#1}}\n\\begin{document}\n\\cite{x, knuth84} and \\mycite{knuth84}\n\\bibliography{refs}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("citehover", text);
    let hover = |server: &mut Server, character: u32| {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 3, "character": character } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let cite = hover(&mut server, 12);
    assert!(cite.contains("**Literate Programming**"), "{cite}");
    assert!(cite.contains("Donald E. Knuth · 1984 · The Computer Journal"), "{cite}");
    assert!(cite.contains("- `journal` The Computer Journal"), "{cite}");
    assert!(cite.contains("`refs.bib:1`"), "{cite}");
    let wrapped = hover(&mut server, 33);
    assert!(wrapped.contains("**Literate Programming**"), "{wrapped}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// A runaway in package code over an argument the document leaves open is
/// the document's error, at the call that opens it.
#[test]
fn an_unclosed_argument_is_the_document_s_error() {
    let text =
        "\\documentclass{article}\n\\usepackage{pgfkeys}\n\\begin{document}\n\\pgfkeys{/demo/a=1,\n\\end{document}\n";
    let (server, _, published, dir) = open_loaded("unclosed", text);
    let diagnostics = published["params"]["diagnostics"].as_array().cloned().unwrap_or_default();
    assert!(!diagnostics.iter().any(|d| d["data"]["rule"] == "caused-library-error"), "{published}");
    let own = diagnostics.iter().find(|d| d["data"]["rule"] == "raised-error").unwrap_or_else(|| panic!("{published}"));
    assert!(
        own["message"]
            .as_str()
            .unwrap_or_default()
            .contains("the argument of `\\pgfkeys` opened here never closes, the file ends inside it"),
        "{own}"
    );
    assert_eq!(own["range"]["start"]["line"], 3, "{own}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// l3keys keys complete the same way: a command handing its argument to
/// `\keys_set:nn` offers its module's keys, and a choice key its choices.
#[test]
fn l3keys_complete_keys_and_choices() {
    let text = "\\documentclass{article}\n\\ExplSyntaxOn\n\\keys_define:nn {zmod} { size .choice:, size/big .code:n = {}, size/small .code:n = {}, width .tl_set:N = \\l_tmpa_tl }\n\\NewDocumentCommand\\zset{m}{\\keys_set:nn{zmod}{#1}}\n\\ExplSyntaxOff\n\\begin{document}\n\\zset{width=1}\n\\zset{}\n\\zset{size=}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("l3keys", text);
    let keys = complete(&mut server, &uri, 7, 6);
    assert!(labels(&keys).contains(&"width") && labels(&keys).contains(&"size"), "{:?}", labels(&keys));
    assert!(!labels(&keys).iter().any(|k| k.contains('/')), "{:?}", labels(&keys));
    let choices = complete(&mut server, &uri, 8, 11);
    assert_eq!(labels(&choices), ["big", "small"]);
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Hovering a key of an option list names its module, type and choices.
#[test]
fn hovering_a_key_shows_its_definition() {
    let text = "\\documentclass{article}\n\\ExplSyntaxOn\n\\keys_define:nn {zmod} { size .choice:, size/big .code:n = {}, size/small .code:n = {}, width .tl_set:N = \\l_tmpa_tl }\n\\NewDocumentCommand\\zset{m}{\\keys_set:nn{zmod}{#1}}\n\\ExplSyntaxOff\n\\begin{document}\n\\zset{width=1, size=big} size\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("keyhover", text);
    let mut hover = |character: u32| {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 6, "character": character } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let size = hover(15);
    assert!(size.contains("Key `size` of `\\zset`") && size.contains("`big`") && size.contains("`small`"), "{size}");
    assert!(hover(12).is_empty() || !hover(12).contains("` of `"), "a value is no key");
    // after the list closed, a word is no key of it
    assert!(!hover(27).contains("Key `"), "outside the list");
    // the first key of a list has no comma before it
    assert!(hover(8).contains("` of `\\zset`"), "{}", hover(8));
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// The rows of `satex/typeset` for a root with an `\input` file and a bibliography.
fn typeset_of_a_root(tag: &str) -> Vec<Value> {
    typeset_after_edit(tag, None, "A Title")
}

/// [`typeset_of_a_root`], the root's text then replaced by `edit`; asked until a row holds `until`.
fn typeset_after_edit(tag: &str, edit: Option<&str>, until: &str) -> Vec<Value> {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-typeset-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = "\\documentclass{article}\n\\begin{document}\n\\input{chapter}\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    let main = dir.join("main.tex");
    std::fs::write(&main, text).unwrap();
    std::fs::write(dir.join("chapter.tex"), "\\section{Intro}\nChapter text \\cite{a}.\n").unwrap();
    std::fs::write(dir.join("refs.bib"), "@book{a, author={Doe, J.}, title={A Title}, year={2020}, publisher={P}}\n")
        .unwrap();
    let uri = fixture_uri(&main);
    let mut server = Server::start_with(&dir, &dir, &["--set", "limits.seconds=0"]);
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": text } }),
    );
    server.notification("textDocument/publishDiagnostics");
    if let Some(text) = edit {
        server.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": text }] }),
        );
        server.notification("textDocument/publishDiagnostics");
    }
    let mut rows = Vec::new();
    for _ in 0..20 {
        let id = server.request("satex/typeset", json!({ "textDocument": { "uri": uri } }));
        rows = server.response(id)["result"].as_array().cloned().unwrap_or_default();
        if rows.iter().any(|r| r["detail"].as_str().is_some_and(|d| d.contains(until))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    rows
}

/// The paragraphs of an `\input` file come with the file they are set in.
#[test]
fn typeset_of_a_root_lists_an_input_file() {
    if satex::project::paths::which("kpsewhich").is_err() {
        return;
    }
    let rows = typeset_of_a_root("input");
    assert!(rows.iter().any(|r| r["path"].as_str().is_some_and(|p| p.ends_with("chapter.tex"))), "{rows:?}");
}

/// A bibliography's heading and each entry are paragraphs of their own: the
/// `\par` of an empty paragraph (LaTeX's `\everypar` runs one) ends none.
/// The starred `\section*` it sets is a heading.
#[test]
fn typeset_of_a_root_splits_the_bibliography() {
    if satex::project::paths::which("kpsewhich").is_err() {
        return;
    }
    let rows = typeset_of_a_root("bibliography-split");
    let details: Vec<&str> = rows.iter().filter_map(|r| r["detail"].as_str()).collect();
    // a heading: no page break may part it from what follows
    assert!(
        rows.iter().any(|r| r["detail"].as_str().is_some_and(|d| d.trim() == "References") && r["heading"].is_string()),
        "{rows:?}"
    );
    assert!(details.iter().any(|d| d.contains("A Title") && !d.contains("References")), "{details:?}");
    assert!(details.iter().any(|d| d.trim() == "1 Intro"), "{details:?}");
    // a citation stands where it is set: before its `[1]`
    let cited = rows.iter().find(|r| r["detail"].as_str().is_some_and(|d| d.starts_with("Chapter text")));
    let detail = cited.and_then(|r| r["detail"].as_str()).unwrap_or_default();
    let mark = detail.find("[1]").map(|i| detail[..i].chars().count() as u64);
    let key = cited.and_then(|r| r["keys"].as_array()).and_then(|k| k.first()).cloned().unwrap_or_default();
    assert_eq!(
        (key["kind"].as_str(), key["key"].as_str(), key["at"].as_u64()),
        (Some("cite"), Some("a"), mark),
        "{cited:?}"
    );
}

/// A paragraph set by a sectioning command says which.
#[test]
fn typeset_of_a_root_marks_headings() {
    if satex::project::paths::which("kpsewhich").is_err() {
        return;
    }
    let rows = typeset_of_a_root("heading");
    assert!(
        rows.iter().any(|r| r["heading"] == "section" && r["detail"].as_str().is_some_and(|d| d.contains("Intro"))),
        "{rows:?}"
    );
}

/// The server runs the build (`Machine::build`), so the .bbl the bibliography
/// tool would make is typeset in the root, as `satex query typeset` has it.
#[test]
fn typeset_of_a_root_lists_the_bibliography() {
    if satex::project::paths::which("kpsewhich").is_err() {
        return;
    }
    let rows = typeset_of_a_root("bibliography");
    assert!(
        rows.iter().any(|r| r["file"] == "main.tex" && r["detail"].as_str().is_some_and(|d| d.contains("A Title"))),
        "{rows:?}"
    );
}

/// A `\tableofcontents` added after the first analysis sets the entries:
/// the `.toc` the run looked for was empty, so its new text sends the
/// build round again, though the run never expanded `\contentsline`.
#[test]
fn typeset_of_a_root_sets_a_table_of_contents_added_later() {
    if satex::project::paths::which("kpsewhich").is_err() {
        return;
    }
    let text = "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\input{chapter}\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    let rows = typeset_after_edit("toc", Some(text), "Intro");
    let toc_line = 3;
    assert!(
        rows.iter().any(|r| r["file"] == "main.tex"
            && r["line"] == toc_line
            && r["detail"].as_str().is_some_and(|d| d.contains("Intro"))),
        "{rows:?}"
    );
}

/// A `\begin` name and its `\end` name are edited together. They pair by
/// nesting, so the link holds while a name is retyped, and a commented
/// `\end` is no partner.
#[test]
fn environment_names_are_linked_by_nesting() {
    let (mut server, uri, _, dir) = open_unsaved("linked", "\\begin{a}\n");
    let text = "\\begin{ite}\n\\begin{b}\n% \\end{x}\n\\end{b}\n\\end{itemize}\n";
    server.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": text }] }),
    );
    let lines: Vec<&str> = text.lines().collect();
    let name = |line: usize, name: &str| {
        let start = lines[line].find(&format!("{{{name}}}")).unwrap() + 1;
        json!({ "start": { "line": line, "character": start }, "end": { "line": line, "character": start + name.len() } })
    };
    let id = server.request(
        "textDocument/linkedEditingRange",
        json!({ "textDocument": { "uri": uri }, "position": name(0, "ite")["end"] }),
    );
    let linked = server.response(id);
    assert_eq!(linked["result"]["ranges"], json!([name(0, "ite"), name(4, "itemize")]), "{linked:?}");
    let id = server.request(
        "textDocument/linkedEditingRange",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": 0 } }),
    );
    assert_eq!(server.response(id)["result"], Value::Null);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `\documentclass[` offers the options of the class named after the
/// cursor, not of every class or package.
#[test]
fn class_options_are_the_named_class_s() {
    if !crate::support::installed() {
        return;
    }
    let text = "\\documentclass[]{article}\n\\usepackage[]{graphicx}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("classopts", text);
    let class = complete(&mut server, &uri, 0, "\\documentclass[".len() as u32);
    let class = labels(&class);
    assert!(class.contains(&"twocolumn") && !class.contains(&"hiresbb"), "{class:?}");
    let package = complete(&mut server, &uri, 1, "\\usepackage[".len() as u32);
    let package = labels(&package);
    assert!(package.contains(&"hiresbb") && !package.contains(&"twocolumn"), "{package:?}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A class's and a package's key=value options: the keys inside `[…]`,
/// and after `key=` the values that key takes (KOMA's `paper=`, geometry's).
#[test]
fn key_value_options_offer_keys_and_values() {
    if !crate::support::installed() {
        return;
    }
    let text =
        "\\documentclass[paper=]{scrartcl}\n\\usepackage[paper=]{geometry}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("kvopts", text);
    let at = |line: &str| line.find('=').unwrap() as u32;
    let lines: Vec<&str> = text.lines().collect();
    let class_keys = complete(&mut server, &uri, 0, "\\documentclass[".len() as u32);
    assert!(labels(&class_keys).iter().any(|l| l.starts_with("paper")), "{:?}", labels(&class_keys));
    assert!(labels(&class_keys).iter().any(|l| l.starts_with("fontsize")), "{:?}", labels(&class_keys));
    let paper = complete(&mut server, &uri, 0, at(lines[0]) + 1);
    assert!(labels(&paper).contains(&"a4"), "{:?}", labels(&paper));
    let package_keys = complete(&mut server, &uri, 1, "\\usepackage[".len() as u32);
    assert!(labels(&package_keys).iter().any(|l| l.starts_with("margin")), "{:?}", labels(&package_keys));
    let geometry = complete(&mut server, &uri, 1, at(lines[1]) + 1);
    assert!(labels(&geometry).contains(&"a4paper"), "{:?}", labels(&geometry));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A name with `@` is offered only where `@` is a letter: after
/// `\makeatletter`, not in the document's own text before it.
#[test]
fn completion_offers_at_names_only_where_at_is_a_letter() {
    let text =
        "\\documentclass{article}\n\\begin{document}\n\\@ab\n\\makeatletter\n\\@ab\n\\makeatother\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("at-letter", text);
    let at = |server: &mut Server, line: usize| {
        let items = complete(server, &uri, line as u32, 4);
        labels(&items).iter().any(|l| l.starts_with("@") && l.len() > 1)
    };
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| *l == "\\@ab").unwrap();
    let inside = lines.iter().rposition(|l| *l == "\\@ab").unwrap();
    assert!(!at(&mut server, first), "@ names offered where @ is not a letter");
    assert!(at(&mut server, inside), "no @ names after \\makeatletter");
    drop(server);
    std::fs::remove_dir_all(dir).ok();
}

/// A hover shows what the defining package's manual says (src/plugins/docs):
/// mined in the background, so a later hover shows it. The kernel's
/// internals link to source2e, expl3 to interface3, a package to its own PDF,
/// each with the section and source line that document it; an environment too.
#[test]
fn hover_shows_the_mined_manual() {
    let lines = [
        "\\documentclass{article}",
        "\\usepackage{ifthen,xspace,alltt}",
        "\\xspaceaddexceptions{\\relax}",
        "\\makeatletter\\def\\oops{\\@latex@error{a}{b}}\\makeatother",
        "\\ExplSyntaxOn \\tl_set:Nn \\l_tmpa_tl {x} \\ExplSyntaxOff",
        "\\begin{document}",
        "\\ifthenelse{1=1}{a}{b}",
        "\\begin{alltt}x\\end{alltt}",
        "\\end{document}",
    ];
    let (mut server, uri, _, dir) = open_loaded("mined", &(lines.join("\n") + "\n"));
    let at = |name: &str| {
        let line = lines.iter().position(|l| l.contains(name)).unwrap();
        (line as u32, (lines[line].find(name).unwrap() + 2) as u32)
    };
    let mut until = |name: &str, wanted: &[&str]| {
        let (line, character) = at(name);
        let mut last = String::new();
        for _ in 0..300 {
            let id = server.request(
                "textDocument/hover",
                json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
            );
            last = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
            if wanted.iter().all(|w| last.contains(w)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("{name}: {wanted:?} not in\n{last}");
    };
    if !documented_sources_installed() {
        // Without `texmf-dist/source` (TeX Live's docker image) nothing is mined: the usage and source link stay.
        until("\\@latex@error", &["`\\@latex@error{⟨arg1⟩}{⟨arg2⟩}`", "[The LaTeX2e sources, source2e]"]);
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    until(
        "\\@latex@error",
        &[
            "for use in the LaTeX core. *(from the [source2e manual](https://texdoc.org/serve/source2e/0), § General commands)* `",
            "lterror.dtx:",
            "[texdoc source2e](https://texdoc.org/serve/source2e/0)",
        ],
    );
    until("\\tl_set:Nn", &["Sets ⟨tl var⟩ to contain ⟨tokens⟩", "texdoc interface3"]);
    let package = ["you can add new commands", "*(from the [xspace manual](https://texdoc.org/", "[texdoc xspace]"];
    until("\\xspaceaddexceptions", &package);
    // An authored description stays first: the manual adds the link.
    until("\\ifthenelse", &["Tests a condition and chooses code.\n", "[texdoc ifthen]"]);
    until("{alltt}", &["*(from the [alltt manual]", "§ Introduction)*", "alltt.dtx:"]);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Whether the installation ships the kernel's documented sources to mine.
fn documented_sources_installed() -> bool {
    let out = std::process::Command::new("kpsewhich").args(["-var-value", "TEXMFDIST"]).output();
    let root = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    std::path::Path::new(&root).join("source/latex/base/lterror.dtx").is_file()
}

/// A server over `dir` with `args`, `main.tex` written there and opened.
fn open_styled(tag: &str, args: &[&str]) -> (Server, std::path::PathBuf) {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), crate::style_prompt::hskips(12)).unwrap();
    let args = [&["--set", "limits.seconds=0"], args].concat();
    let mut server = Server::start_with(&crate::support::test_cache(), &dir, &args);
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    open_file(&mut server, &dir, "main.tex");
    (server, dir)
}

/// A rule the new project breaks often is asked about once. Yes writes it
/// into the project's satex.yaml and takes its findings off at once.
#[test]
fn yes_to_the_style_prompt_turns_the_rule_off() {
    let (mut server, dir) = open_styled("style-yes", &[]);
    let prompt = server.notification("window/showMessageRequest");
    let message = prompt["params"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("hskip") && message.contains("primitive-tex-command"), "{message}");
    server.write(&json!({ "jsonrpc": "2.0", "id": prompt["id"], "result": { "title": "Yes, turn it off" } }));
    let published = server.notification("textDocument/publishDiagnostics");
    let rules: Vec<&str> = published["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["data"]["rule"].as_str())
        .collect();
    assert!(!rules.contains(&"primitive-tex-command"), "{rules:?}");
    let written = std::fs::read_to_string(dir.join("satex.yaml")).unwrap_or_default();
    assert!(written.contains("lint_off+: [primitive-tex-command]"), "{written}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// "Show first" opens the rule's first finding and asks again.
#[test]
fn show_first_opens_the_first_finding_and_asks_again() {
    let (mut server, dir) = open_styled("style-show", &[]);
    let prompt = server.notification("window/showMessageRequest");
    let titles: Vec<&str> =
        prompt["params"]["actions"].as_array().unwrap().iter().filter_map(|a| a["title"].as_str()).collect();
    assert!(titles.contains(&"Show first"), "{titles:?}");
    server.write(&json!({ "jsonrpc": "2.0", "id": prompt["id"], "result": { "title": "Show first" } }));
    let show = server.notification("window/showDocument");
    assert!(show["params"]["uri"].as_str().unwrap_or_default().ends_with("main.tex"), "{show}");
    let first = crate::style_prompt::hskips(12).lines().position(|l| l.contains("\\hskip")).unwrap();
    assert_eq!(show["params"]["selection"]["start"]["line"], json!(first), "{show}");
    let again = server.notification("window/showMessageRequest");
    assert_eq!(again["params"]["message"], prompt["params"]["message"]);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// With `style_prompt.enabled` off, nothing is asked.
#[test]
fn no_style_prompt_when_it_is_off() {
    let (mut server, dir) = open_styled("style-off", &["--set", "style_prompt.enabled=false"]);
    let id = server.request("workspace/symbol", json!({ "query": "" }));
    loop {
        let msg = server.read();
        assert_ne!(msg["method"], "window/showMessageRequest", "{msg}");
        if msg["id"] == id {
            break;
        }
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A hover over a finding shows its rule's card.
#[test]
fn a_hover_over_a_finding_shows_its_rule() {
    let (mut server, dir) = open_styled("rule-card", &["--set", "style_prompt.enabled=false"]);
    let uri = fixture_uri(&dir.join("main.tex"));
    let line = crate::style_prompt::hskips(1).lines().position(|l| l.contains("hskip")).unwrap() as u32;
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 3 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    let number = satex::linter::number("primitive-tex-command").unwrap();
    assert!(hover.contains("primitive-tex-command") && hover.contains(number), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hovers kept between sessions never carry one project's names into
/// another: a name defined in a file of the project (a `.aux` the run
/// writes, a style file beside the document) is the project's own, so the
/// second project's hover names its own definition, not the first's.
#[test]
fn kept_hovers_stay_with_their_project() {
    let base = crate::support::tmp_root().join(format!("satex-lsp-projects-{}", std::process::id()));
    let cache = base.join("cache");
    let hover = |dir: &std::path::Path, line_of_def: usize| -> String {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("lib.sty"), format!("{}\\def\\shared{{x}}\n", "\n".repeat(line_of_def - 1))).unwrap();
        let text = "\\catcode`\\{=1 \\catcode`\\}=2 \\input lib.sty\n\\shared\n";
        std::fs::write(dir.join("main.tex"), text).unwrap();
        let uri = fixture_uri(&dir.join("main.tex"));
        let mut server = Server::start_in(&cache, dir);
        let id =
            server.request("initialize", json!({ "processId": null, "rootUri": fixture_uri(dir), "capabilities": {} }));
        server.response(id);
        server.notify("initialized", json!({}));
        server.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": text } }),
        );
        server.notification("textDocument/publishDiagnostics");
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 2 } }),
        );
        let card = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
        server.notify("textDocument/didClose", json!({ "textDocument": { "uri": uri } }));
        server.shutdown();
        card
    };
    let first = hover(&base.join("a"), 1);
    assert!(first.contains("lib.sty:1"), "{first}");
    let second = hover(&base.join("b"), 3);
    assert!(second.contains("lib.sty:3") && !second.contains("lib.sty:1"), "{second}");
    let _ = std::fs::remove_dir_all(&base);
}

/// An expl3 name typed up to its signature offers each signature letter
/// with what it means, from `conventions.signatures`.
#[test]
fn an_expl3_signature_completes_its_letters() {
    let text =
        "\\documentclass{article}\n\\ExplSyntaxOn\n\\tl_set:N\n\\ExplSyntaxOff\n\\begin{document}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("signature", text);
    let items = complete(&mut server, &uri, 2, 9);
    let letter = items.iter().find(|i| i["label"] == "tl_set:Nn");
    assert!(letter.is_some(), "{:?}", labels(&items));
    // a letter no defined name ends with yet says what it means
    let w = items.iter().find(|i| i["label"] == "tl_set:Nw").map(|i| i["detail"].to_string()).unwrap_or_default();
    assert!(w.contains("delimiter"), "{w}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Inside a TikZ path the names of the picture's nodes complete.
#[test]
/// The editor closes the paren as `(` is typed: the run formed the name there.
fn a_tikz_path_completes_node_names() {
    if !crate::support::installed() {
        return;
    }
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\begin{tikzpicture}\n\\node (alpha) {a};\\node (beta) at (1,0) {b};\n\\draw (al);\n\\end{tikzpicture}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("nodes", text);
    let items = complete(&mut server, &uri, 5, 9);
    assert!(labels(&items).contains(&"alpha"), "{:?}", labels(&items));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `\tableofcontents` typesets every line of the contents, read from the
/// `.toc` file, not only the last one.
#[test]
fn a_contents_hover_shows_every_line() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{Alpha}\n\\section{Beta}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("typesets-toc", text);
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": 3 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("Alpha") && hover.contains("Beta"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// After `key=` of a keyval key whose handler selects among macros or cases, the choices are
/// offered and no command names: listings' `columns` (`\csname lst@column@#1\endcsname`).
#[test]
fn key_values_come_from_the_handler() {
    if !crate::support::installed() {
        return;
    }
    let text = "\\documentclass{article}\n\\usepackage{listings}\n\\begin{document}\n\\lstset{basicstyle=\\ttfamily\\small, columns=, aboveskip=}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("keyvalues", text);
    let line = text.lines().nth(3).unwrap();
    let items = complete(&mut server, &uri, 3, (line.find("columns=").unwrap() + "columns=".len()) as u32);
    let got = labels(&items);
    for v in ["fixed", "flexible", "fullflexible", "spaceflexible"] {
        assert!(got.contains(&v), "{v} missing: {got:?}");
    }
    assert!(got.iter().all(|l| !l.chars().next().is_some_and(|c| c.is_uppercase())), "{got:?}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same for a document's own keyval family: the macros named by `\csname fam@mode@#1\endcsname`.
#[test]
fn key_values_are_the_macros_a_csname_selects() {
    if !crate::support::installed() {
        return;
    }
    let text = "\\documentclass{article}\n\\usepackage{keyval}\n\\makeatletter\n\\define@key{fam}{mode}{\\csname fam@mode@#1\\endcsname}\n\\def\\fam@mode@draft{}\\def\\fam@mode@final{}\n\\newcommand\\famset[1]{\\setkeys{fam}{#1}}\n\\makeatother\n\\begin{document}\n\\famset{mode=}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("keyvalues-own", text);
    let line = text.lines().nth(8).unwrap();
    let items = complete(&mut server, &uri, 8, line.len() as u32 - 1);
    assert_eq!(labels(&items), ["draft", "final"], "{:?}", labels(&items));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The hover over the typed argument at `line`/`character` of `text`.
fn argument_hover(tag: &str, text: &str, line: u32, character: u32) -> String {
    let (mut server, uri, _, dir) = open_loaded(tag, text);
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
    hover
}

const TYPED: &str = "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\begin{itemize}\n\\item a\n\\end{itemize}\n\\input{chapter}\n\\end{document}\n";

#[test]
fn hover_names_the_class() {
    let hover = argument_hover("hclass", TYPED, 0, 18);
    assert!(hover.contains("article") && hover.to_lowercase().contains("class"), "{hover}");
}

#[test]
fn hover_names_the_package() {
    let hover = argument_hover("hpkg", TYPED, 1, 14);
    assert!(hover.contains("amsmath") && hover.to_lowercase().contains("package"), "{hover}");
}

#[test]
fn hover_pairs_begin_and_end() {
    let hover = argument_hover("henv", TYPED, 5, 8);
    assert!(hover.contains("itemize") && hover.to_lowercase().contains("environment"), "{hover}");
    assert!(hover.contains("`\\begin` on line 4, `\\end` on line 6"), "{hover}");
    let document = argument_hover("hdoc", TYPED, 2, 10);
    assert!(document.contains("document") && document.to_lowercase().contains("environment"), "{document}");
}

#[test]
fn hover_resolves_the_input_file() {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-hinput-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), TYPED).unwrap();
    std::fs::write(dir.join("chapter.tex"), "Hello.\n").unwrap();
    let mut server = serve(&dir, true);
    let (uri, _) = open_file(&mut server, &dir, "main.tex");
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 6, "character": 9 } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("chapter.tex"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `textDocument/documentColor` gives a swatch to a named color a color command
/// uses and to one the document defines, each with the value the run gave it.
#[test]
fn document_colors_cover_named_and_defined_colors() {
    let text = "\\documentclass{article}\n\\usepackage{xcolor}\n\\definecolor{mine}{rgb}{0,0,1}\n\\begin{document}\n\\color{red} \\textcolor{mine}{x}\n\\end{document}\n";
    let dir = crate::support::tmp_root().join(format!("satex-lsp-colors-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    std::fs::write(&main, text).unwrap();
    let uri = fixture_uri(&main);
    let mut server = Server::start_with(&dir, &dir, &["--set", "limits.seconds=0"]);
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": text } }),
    );
    server.notification("textDocument/publishDiagnostics");
    let id = server.request("textDocument/documentColor", json!({ "textDocument": { "uri": uri } }));
    let found = server.response(id)["result"].clone();
    let at = |line: u64, name: &str| {
        let start = text.lines().nth(line as usize).unwrap().find(name).unwrap() as u64;
        found
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["range"]["start"] == json!({ "line": line, "character": start }))
            .cloned()
    };
    let red = at(4, "red").unwrap_or_else(|| panic!("red has a color: {found}"));
    assert_eq!((red["color"]["red"].as_f64(), red["color"]["blue"].as_f64()), (Some(1.0), Some(0.0)), "{found}");
    let mine = at(4, "mine").unwrap_or_else(|| panic!("mine has a color: {found}"));
    assert_eq!((mine["color"]["red"].as_f64(), mine["color"]["blue"].as_f64()), (Some(0.0), Some(1.0)), "{found}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A swatch sits on a word the run used as a color, never on an anchor
/// (`satex.north`) or a key, which a color test only asked about.
#[test]
fn document_colors_skip_words_only_tested() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\begin{tikzpicture}[every node/.style={draw}]\n\\node[fill=red] (satex) {a};\n\\node at (satex.north) {b};\n\\end{tikzpicture}\n\\end{document}\n";
    let dir = crate::support::tmp_root().join(format!("satex-lsp-colors2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    std::fs::write(&main, text).unwrap();
    let uri = fixture_uri(&main);
    let mut server = Server::start_with(&dir, &dir, &["--set", "limits.seconds=0"]);
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": text } }),
    );
    server.notification("textDocument/publishDiagnostics");
    let id = server.request("textDocument/documentColor", json!({ "textDocument": { "uri": uri } }));
    let found = server.response(id)["result"].clone();
    let words: Vec<&str> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let (l, s, e) = (
                c["range"]["start"]["line"].as_u64(),
                c["range"]["start"]["character"].as_u64(),
                c["range"]["end"]["character"].as_u64(),
            );
            &text.lines().nth(l.unwrap() as usize).unwrap()[s.unwrap() as usize..e.unwrap() as usize]
        })
        .collect();
    assert!(words.contains(&"red"), "red is a color the run used: {words:?}");
    assert!(words.iter().all(|w| *w == "red"), "only colors get a swatch: {words:?}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The optional argument after the class or package is a release date: the
/// kernel compares it with the date the file provides, which the hover and
/// the signature help say, from the loader's own body.
#[test]
fn a_loading_commands_last_optional_argument_is_a_release_date() {
    let text = "\\documentclass{article}\n\\usepackage{xcolor}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("release", text);
    for line in [0, 1] {
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 4 } }),
        );
        let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
        assert!(hover.contains("- `⟨arg3⟩` date, at least"), "line {line}: {hover}");
        assert!(hover.contains("- `⟨arg1⟩` options"), "{hover}");
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// In an argument the run looked a color up for (`\textcolor{…}`), completion offers the colors
/// the run defined, each a color item whose documentation is its `#rrggbb` value.
/// A command the document never called yet: one probe call shows which
/// argument it looks up as a color.
#[test]
fn a_color_argument_completes_before_any_call() {
    let text = "\\documentclass{article}\n\\usepackage{xcolor}\n\\definecolor{mine}{rgb}{0,0,1}\n\\begin{document}\nx\n\\end{document}\n\\textcolor{}{}\n";
    let (mut server, uri, _, dir) = open_loaded("color-probe", text);
    let first = complete(&mut server, &uri, 6, 11);
    assert!(first.iter().any(|i| i["label"] == "mine" && i["kind"] == 16), "{:?}", labels(&first));
    let second = complete(&mut server, &uri, 6, 13);
    assert!(second.iter().all(|i| i["kind"] != 16), "{:?}", labels(&second));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_color_argument_completes_colors_with_their_values() {
    let text = "\\documentclass{article}\n\\usepackage{xcolor}\n\\definecolor{mine}{rgb}{0,0,1}\n\\newenvironment{boxed}{\\color{mine}}{}\n\\begin{document}\n\\textcolor{mine}{x} \\textcolor{}{y}\n\\begin{boxed}x\\end{boxed} \\begin{}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("color-items", text);
    let items = complete(&mut server, &uri, 5, 31);
    let mine = items.iter().find(|i| i["label"] == "mine").unwrap_or_else(|| panic!("{:?}", labels(&items)));
    assert_eq!(mine["kind"], 16, "{mine}");
    assert_eq!(mine["documentation"], "#0000ff", "{mine}");
    // `boxed` sets a color, but `\begin`'s argument names an environment
    let environments = complete(&mut server, &uri, 6, 33);
    assert!(environments.iter().all(|i| i["kind"] != 16), "{:?}", labels(&environments));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A comment completes no command, except the directive names after
/// `\satex@`, and code inside a directive's braces.
#[test]
fn a_comment_completes_only_directives_and_their_code() {
    let text = "\\documentclass{article}\n\\begin{document}\n% \\sec\n% \\satex@a\n% \\satex@assert{\\thepa}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("comment-completion", text);
    let prose = complete(&mut server, &uri, 2, 6);
    assert!(prose.is_empty(), "{:?}", labels(&prose));
    let names = complete(&mut server, &uri, 3, 10);
    assert_eq!(labels(&names), ["\\satex@assert", "\\satex@assume"]);
    let code = complete(&mut server, &uri, 4, 22);
    assert!(code.iter().any(|i| i["label"] == "thepage"), "{:?}", labels(&code));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// After the first operand of `\ifnum` and a space, the relations TeX
/// reads; a directive's condition adds `<=`, `>=`, `!=` and `==`.
#[test]
fn a_comparison_completes_its_relations() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\ifnum\\value{page} \\fi\n% \\satex@assert{\\value{page} }\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("relations", text);
    let tex = complete(&mut server, &uri, 2, 19);
    assert_eq!(labels(&tex), ["<", "=", ">"]);
    let directive = complete(&mut server, &uri, 3, 29);
    assert_eq!(labels(&directive), ["<", "=", ">", "<=", ">=", "!=", "=="]);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A directive's name shows its usage line and the grammar of its argument,
/// from the table its parser reads.
#[test]
fn a_directive_hover_shows_its_signature() {
    let text = "\\documentclass{article}\n\\begin{document}\n%\\satex@assert{\\thepage < 5}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("directive-hover", text);
    let h = hover_at(&mut server, &uri, text, 2, "\\satex@assert", 3);
    assert!(h.contains("`\\satex@assert{⟨condition⟩}`"), "{h}");
    assert!(h.contains("`⟨relation⟩` one of `<` `=` `>` `<=`"), "{h}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// After `clock=` in a configure directive, the values the schema names.
#[test]
fn a_configure_directive_completes_clock_values() {
    let text = "\\documentclass{article}\n\\begin{document}\n%\\satex@configure{clock=}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("directive-clock", text);
    let items = complete(&mut server, &uri, 2, 24);
    assert_eq!(labels(&items), ["now", "unknown"]);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Node names complete after `(` (the node itself too: a self loop is
/// fine), after a key whose code takes a node (`right of=`), after
/// positioning's `of`, and after `name.` its shape's anchors.
#[test]
fn node_names_complete_where_a_node_is_named() {
    if !crate::support::installed() {
        return;
    }
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{positioning}\n\\begin{document}\n\\begin{tikzpicture}\n\\node (tex) {a};\n\\node (out) [right of=] {b};\n\\node (x) [right=] {c};\n\\draw (tex) -- (;\n\\draw (tex.no;\n\\end{tikzpicture}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("node-names", text);
    let paren = complete(&mut server, &uri, 8, 16);
    assert!(["tex", "out", "x"].iter().all(|n| labels(&paren).contains(n)), "{:?}", labels(&paren));
    let of = complete(&mut server, &uri, 6, 22);
    assert!(labels(&of).contains(&"tex"), "{:?}", labels(&of));
    let placed = complete(&mut server, &uri, 7, 17);
    assert!(labels(&placed).contains(&"of tex"), "{:?}", labels(&placed));
    // after `name.` the anchors of its shape, as pgf declared them
    let anchors = complete(&mut server, &uri, 9, 13);
    assert!(labels(&anchors).contains(&"north") && !labels(&anchors).contains(&"south"), "{:?}", labels(&anchors));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// After `#` in a body: the macro's parameters, with what the comment above
/// the definition says of each.
#[test]
fn a_parameter_completes_with_its_comment() {
    let text = "\\documentclass{article}\n% Says it thrice.\n% #1 is the text\n\\newcommand{\\thrice}[1]{#1#1#1 #}\n\\begin{document}\n\\thrice{ab}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("parameters", text);
    let items = complete(&mut server, &uri, 3, 32);
    assert_eq!(labels(&items), ["#1"]);
    assert!(items[0]["documentation"].to_string().contains("the text"), "{items:?}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Where the run formed a name from the text at the cursor, the names
/// defined with the same prefix complete: labels in `\ref{`, from the
/// `.aux` too.
#[test]
fn a_name_the_run_formed_completes_its_kind() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\\section{B}\\label{sec:b}\nSee \\ref{se}.\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("slots", text);
    let items = complete(&mut server, &uri, 4, 11);
    assert!(labels(&items).contains(&"sec:a") && labels(&items).contains(&"sec:b"), "{:?}", labels(&items));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// No `\` typed, no command: a `(` or a word in prose completes nothing.
#[test]
fn prose_completes_no_commands() {
    let text = "\\documentclass{article}\n\\begin{document}\nSee (\nwor\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("prose", text);
    assert!(complete(&mut server, &uri, 2, 5).is_empty(), "after a paren in prose");
    assert!(complete(&mut server, &uri, 3, 3).is_empty(), "a word in prose");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A tikz key whose code sets a color (`fill=`) takes the run's colors, and
/// so does a bare word, which tikz's `.unknown` handler tries as a color.
#[test]
fn a_tikz_color_key_completes_colors() {
    if !crate::support::installed() {
        return;
    }
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\definecolor{mine}{rgb}{0,0,1}\n\\begin{document}\n\\tikz\\node[fill=] {x};\n\\tikz\\node[] {y};\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("tikz-colors", text);
    let valued = complete(&mut server, &uri, 4, 16);
    assert!(valued.iter().any(|i| i["label"] == "mine" && i["kind"] == 16), "{:?}", labels(&valued));
    let bare = complete(&mut server, &uri, 5, 11);
    let rank = |label: &str| {
        bare.iter().find(|i| i["label"] == label).and_then(|i| i["sortText"].as_str().map(str::to_string))
    };
    let (color, key) = (rank("mine"), rank("draw"));
    assert!(color.is_some() && key.is_some() && key < color, "{:?}", labels(&bare));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Go to definition answers every place that defines a name (a
/// redefinition, the kernel's and the document's), a primitive with its
/// documentation, an environment name with its command's places; an
/// environment's hover is its command's.
#[test]
fn definition_answers_every_place_primitives_and_environments() {
    if !crate::support::installed() {
        return;
    }
    let lines = [
        "\\documentclass{article}",
        "\\usepackage{listings}",
        "\\newcommand\\twice{a}",
        "\\renewcommand\\twice{b}",
        "\\renewcommand\\today{now}",
        "\\begin{document}",
        "\\twice \\romannumeral 4 \\today",
        "\\begin{lstlisting}",
        "x",
        "\\end{lstlisting}",
        "\\end{document}",
    ];
    let (mut server, uri, _, dir) = open_loaded("everydef", &(lines.join("\n") + "\n"));
    let mut ask = |method: &str, word: &str| {
        let line = lines.iter().rposition(|l| l.contains(word)).unwrap();
        let character = (lines[line].find(word).unwrap() + 2) as u32;
        let id = server.request(
            method,
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
        );
        server.response(id)["result"].clone()
    };
    let at_lines = |r: &Value| -> Vec<u64> {
        r.as_array().into_iter().flatten().filter_map(|l| l["range"]["start"]["line"].as_u64()).collect()
    };
    let twice = ask("textDocument/definition", "twice");
    let defining = |w: &str| lines.iter().position(|l| l.starts_with(w)).unwrap() as u64;
    let wanted = [defining("\\newcommand"), defining("\\renewcommand")];
    assert!(wanted.iter().all(|l| at_lines(&twice).contains(l)), "{twice}");
    let primitive = ask("textDocument/definition", "romannumeral");
    assert!(primitive[0]["uri"].as_str().is_some_and(|u| u.contains("romannumeral")), "{primitive}");
    let today = ask("textDocument/definition", "today");
    let files: Vec<&str> = today.as_array().into_iter().flatten().filter_map(|l| l["uri"].as_str()).collect();
    assert!(files.iter().any(|u| u.ends_with("main.tex")) && files.iter().any(|u| u.ends_with(".ltx")), "{today}");
    let env = ask("textDocument/definition", "lstlisting}");
    assert!(env.to_string().contains("listings"), "{env}");
    let hover = ask("textDocument/hover", "lstlisting}");
    let hover = hover["contents"]["value"].as_str().unwrap_or_default();
    assert!(hover.contains("`lstlisting`") && hover.contains("listings"), "{hover}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// How long a completion may take while a run goes on: far below a run of
/// the kernel, generous for a loaded machine.
const COMPLETION_BOUND: Duration = Duration::from_secs(1);

/// The completion at `line`:`character` and how long it took.
/// A completion and how long it took, and whether a run published its findings before the answer came:
/// an answer before that was given while the run went on, however slow a loaded machine made it.
fn timed_complete(server: &mut Server, uri: &str, line: u32, character: u32) -> (Vec<Value>, Duration, bool) {
    let start = std::time::Instant::now();
    let id = server.request(
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
    );
    let mut published = false;
    let answer = loop {
        let msg = server.read();
        if msg.get("id").and_then(Value::as_i64) == Some(id) {
            break msg;
        }
        published |= msg.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics");
    };
    (completion_items(&answer["result"]), start.elapsed(), published)
}

/// A completion never waits for a run: during the first one the
/// installation's classes and packages answer, during a later one the last
/// run answers each kind of place.
#[test]
fn completion_answers_while_a_run_goes_on() {
    if !crate::support::installed() {
        return;
    }
    let lines = [
        "\\documentclass[]{article}",
        "\\usepackage{amsmath}",
        "\\usepackage[]{amsmath}",
        "\\begin{document}",
        "\\begin{ite",
        "\\sec",
        "\\end{document}",
        "\\documentclass{art",
        "\\usepackage{ams",
    ];
    let text = lines.join("\n") + "\n";
    let dir = crate::support::tmp_root().join(format!("satex-lsp-running-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), &text).unwrap();
    let mut server = serve(&dir, true);
    let uri = fixture_uri(&dir.join("main.tex"));
    let at = |prefix: &str| {
        let line = lines.iter().rposition(|l| l.starts_with(prefix)).unwrap();
        let character = lines[line].find("[]").map_or(lines[line].len(), |i| i + 1);
        (line as u32, character as u32)
    };
    // (where, an item it offers)
    let places = [
        (at("\\documentclass{"), "article"),
        (at("\\usepackage{ams"), "amsmath"),
        (at("\\documentclass["), "11pt"),
        (at("\\usepackage["), "fleqn"),
        (at("\\begin{ite"), "itemize"),
        (at("\\sec"), "section"),
    ];
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": text } }),
    );
    // the first run reads the kernel: the installation answers meanwhile
    let mut ran = false;
    for ((line, character), wanted) in &places[..2] {
        let (items, took, published) = timed_complete(&mut server, &uri, *line, *character);
        // never queued behind the run: answered before it published, or fast once it had
        assert!(!published || took < COMPLETION_BOUND, "{wanted}: {took:?} after the run");
        ran |= published;
        assert!(labels(&items).contains(wanted), "{wanted}: {:?}", labels(&items));
    }
    if !ran {
        server.notification("textDocument/publishDiagnostics");
    }
    // an edit, then the run it starts once the debounce is over
    let edited = text.replace("\\begin{document}", "\\usepackage{array}\n\\begin{document}");
    server.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": edited }] }),
    );
    std::thread::sleep(Duration::from_millis(satex::project::config::LspConfig::default().debounce_ms) * 2);
    for ((line, character), wanted) in places {
        let line = if line >= 3 { line + 1 } else { line };
        let (items, took, published) = timed_complete(&mut server, &uri, line, character);
        assert!(!published || took < COMPLETION_BOUND, "{wanted}: {took:?} after the run");
        assert!(labels(&items).contains(&wanted), "{wanted}: {:?}", labels(&items));
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A document with one site of each type of name, for [`completion_follows_the_type_at_the_cursor`].
const TYPED_SITES: &str = r"\documentclass{article}
\usepackage{xcolor}
\usepackage{tikz}
\definecolor{brand}{HTML}{336699}
\begin{document}
\section{One}\label{sec:one}
\textcolor{brand}{x} \ref{sec:one} \cite{knuth}
\begin{tikzpicture}\node (alpha) {a}; \draw (alpha) -- (alpha.north);\end{tikzpicture}
\begin{verbatim}
\ref{
\end{verbatim}
% \ref{
\textbf{see \ref{} and \ref{sec:one} x}
\label{}
\color{}
\vspace{2c}
page 2c and more
\cite{knuth, }
\iffalse\colorbox{}\fi
\begin{thebibliography}{9}\bibitem{knuth} K.\end{thebibliography}
\input{part}
\end{document}
";

/// The column just after the `nth` (from 0) occurrence of `marker` on line `line` of `text`.
fn after(text: &str, line: usize, marker: &str, nth: usize) -> u32 {
    let l = text.lines().nth(line).unwrap();
    let at = l.match_indices(marker).nth(nth).unwrap_or_else(|| panic!("{marker} on {l}")).0;
    l[..at + marker.len()].encode_utf16().count() as u32
}

/// The completion items' labels at `line` after the `nth` `marker`.
fn names_after(server: &mut Server, uri: &str, text: &str, line: usize, marker: &str, nth: usize) -> Vec<String> {
    let items = complete(server, uri, line as u32, after(text, line, marker, nth));
    labels(&items).into_iter().map(str::to_string).collect()
}

/// Completion offers the names of the type the text at the cursor has, per
/// type: labels, citations, colors, nodes and anchors, units; none where the
/// text defines a name, in a comment, in verbatim text, after the argument
/// closed, or in prose. A command the run never called is probed, and
/// another file of the project is typed the same.
#[test]
fn completion_follows_the_type_at_the_cursor() {
    if !crate::support::installed() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-lsp-typed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), TYPED_SITES).unwrap();
    let part = "See \\ref{}.\n";
    std::fs::write(dir.join("part.tex"), part).unwrap();
    let mut server = serve(&dir, true);
    let (uri, _) = open_file(&mut server, &dir, "main.tex");
    let mut at = |line: usize, marker: &str, nth: usize| names_after(&mut server, &uri, TYPED_SITES, line, marker, nth);
    let has = |names: &[String], name: &str| names.iter().any(|n| n == name);
    // labels: an empty argument, nested in another group
    let refs = at(12, "\\ref{", 0);
    assert!(has(&refs, "sec:one") && !has(&refs, "brand") && !has(&refs, "knuth"), "{refs:?}");
    // a name being defined, a closed group before the cursor: no labels
    let defining = at(13, "\\label{", 0);
    assert!(!has(&defining, "sec:one"), "{defining:?}");
    let closed = at(12, " x", 0);
    assert!(!has(&closed, "sec:one"), "{closed:?}");
    // a comment, verbatim text
    assert!(at(11, "\\ref{", 0).is_empty(), "comment");
    let verbatim = at(9, "\\ref{", 0);
    assert!(!has(&verbatim, "sec:one"), "verbatim: {verbatim:?}");
    // colors, a command the run called and one it never called
    let colors = at(14, "\\color{", 0);
    assert!(has(&colors, "brand") && has(&colors, "red") && !has(&colors, "sec:one"), "{colors:?}");
    let probed = at(18, "\\colorbox{", 0);
    assert!(has(&probed, "brand"), "never called: {probed:?}");
    // a citation after a list's comma
    let cites = at(17, "knuth, ", 0);
    assert!(has(&cites, "knuth") && !has(&cites, "sec:one"), "{cites:?}");
    // nodes and a node's anchors
    let nodes = at(7, "-- (", 0);
    assert!(has(&nodes, "alpha"), "{nodes:?}");
    let anchors = at(7, "(alpha.", 0);
    assert!(has(&anchors, "north") && !has(&anchors, "alpha"), "{anchors:?}");
    // a unit after a number where a length goes, none in prose
    let units = at(15, "2c", 0);
    assert!(has(&units, "cm") && !has(&units, "pt"), "{units:?}");
    let prose = at(16, "2c", 0);
    assert!(!has(&prose, "cm"), "{prose:?}");
    // another file of the project, read through the document
    let (part_uri, _) = open_file(&mut server, &dir, "part.tex");
    let other = names_after(&mut server, &part_uri, part, 0, "\\ref{", 0);
    assert!(has(&other, "sec:one"), "{other:?}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hover names what the text is by the same types: a color's value, a label's place.
#[test]
fn hover_follows_the_type_at_the_cursor() {
    if !crate::support::installed() {
        return;
    }
    let (mut server, uri, _, dir) = open_loaded("typed-hover", TYPED_SITES);
    let mut hover = |line: usize, marker: &str| {
        let character = after(TYPED_SITES, line, marker, 0) + 1;
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
        );
        server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string()
    };
    let color = hover(6, "\\textcolor{");
    assert!(color.contains("Color `brand`") && color.contains("| rgb | `0.2,0.4,0.6` |"), "{color}");
    let label = hover(6, "\\ref{");
    assert!(label.contains("Label `sec:one`"), "{label}");
    // the same word in prose is no label
    assert!(!hover(16, "page ").contains(": label"));
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The text around a completion site varies (spaces, a line break, a
/// comment, an optional argument, a list before it): the candidates stay
/// the same.
#[test]
fn completion_candidates_survive_mutations_of_their_site() {
    if !crate::support::installed() {
        return;
    }
    let base = "\\documentclass{article}\n\\begin{document}\n\\section{One}\\label{sec:one}\\label{sec:two}\nSITE\n\\nocite{knuth,lamport}\n\\begin{thebibliography}{9}\\bibitem{knuth} K.\\bibitem{lamport} L.\\end{thebibliography}\n\\end{document}\n";
    // `|` marks the cursor
    let sites: [(&str, &[&str]); 2] = [
        (
            "\\ref{|}",
            &[
                "\\ref{|}",
                "\\ref {|}",
                "\\ref{ |}",
                "\\ref{%\n|}",
                "\\ref{\n  |}",
                "x\\ref{|} y",
                "\\ref{|}% c",
                "\\textbf{\\ref{|}}",
            ],
        ),
        (
            "\\cite{|}",
            &[
                "\\cite[p.~3]{|}",
                "\\cite{knuth, |}",
                "\\cite{knuth,% c\n|}",
                "\\cite {|}",
                "\\cite{lamport,\n|}",
                "\\emph{\\cite{|}}",
            ],
        ),
    ];
    let (mut server, uri, _, dir) = open_loaded("mutations", &base.replace("SITE", ""));
    let mut version = 1;
    let mut names = |site: &str| {
        let text = base.replace("SITE", &site.replace('|', ""));
        let offset = base.find("SITE").unwrap() + site.find('|').unwrap();
        let line = text[..offset].matches('\n').count() as u32;
        let character = text[..offset].rsplit('\n').next().unwrap().encode_utf16().count() as u32;
        version += 1;
        server.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }),
        );
        server.notification("textDocument/publishDiagnostics");
        let items = complete(&mut server, &uri, line, character);
        let mut found: Vec<String> = labels(&items).into_iter().map(str::to_string).collect();
        found.sort();
        found
    };
    for (first, variants) in sites {
        let expected = names(first);
        assert!(!expected.is_empty(), "{first}");
        for variant in variants {
            assert_eq!(names(variant), expected, "{variant} against {first}");
        }
    }
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Published diagnostics name the version of the text they are of; an
/// edit back to text already analyzed publishes them again for its version,
/// so a client can drop those of a text it no longer has.
#[test]
fn diagnostics_name_the_version_they_are_of() {
    let text = "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";
    let (mut server, uri, published, dir) = open_unsaved("versions", text);
    assert_eq!(published["params"]["version"], 1, "{published}");
    let change = |server: &mut Server, version: i32, text: &str| {
        server.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }),
        );
        server.notification("textDocument/publishDiagnostics")
    };
    let broken = text.replace("x\n", "\\usetikzlib\n");
    let two = change(&mut server, 2, &broken);
    assert_eq!(two["params"]["version"], 2, "{two}");
    // the same text again: the same run, its findings now of version 3
    let three = change(&mut server, 3, &broken);
    assert_eq!(three["params"]["version"], 3, "{three}");
    assert_eq!(three["params"]["diagnostics"], two["params"]["diagnostics"]);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

// --- type plugins (src/lsp/plugins) ---

/// [`open_loaded`] with `--set` flags for the server.
fn open_set(tag: &str, text: &str, sets: &[&str]) -> (Server, String, std::path::PathBuf) {
    open_with(tag, text, &[], sets)
}

/// [`open_set`] with more files beside `main.tex`.
fn open_with(tag: &str, text: &str, files: &[(&str, &str)], sets: &[&str]) -> (Server, String, std::path::PathBuf) {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), text).unwrap();
    for (name, body) in files {
        std::fs::write(dir.join(name), body).unwrap();
    }
    let mut args = vec!["--set", "limits.seconds=0"];
    for s in sets {
        args.extend(["--set", s]);
    }
    let mut server = Server::start_with(&crate::support::test_cache(), &dir, &args);
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    let (uri, _) = open_file(&mut server, &dir, "main.tex");
    // the first findings may come before the run settles: wait out a debounce
    std::thread::sleep(std::time::Duration::from_millis(satex::project::config::LspConfig::default().debounce_ms) * 2);
    (server, uri, dir)
}

/// The result of `method` at 0-based `line`:`character`.
fn at(server: &mut Server, uri: &str, method: &str, line: u32, character: u32) -> Value {
    let p = json!({ "line": line, "character": character });
    let params = match method {
        "textDocument/codeAction" => {
            json!({ "textDocument": { "uri": uri }, "range": { "start": p, "end": p }, "context": { "diagnostics": [] } })
        }
        _ => json!({ "textDocument": { "uri": uri }, "position": p }),
    };
    let id = server.request(method, params);
    server.response(id)["result"].clone()
}

fn hover_md(v: &Value) -> String {
    v["contents"]["value"].as_str().unwrap_or_default().to_string()
}

/// The column of the first `needle` on 0-based `line` of `text`, plus `off`.
fn col(text: &str, line: usize, needle: &str, off: usize) -> u32 {
    (text.lines().nth(line).unwrap().find(needle).unwrap() + off) as u32
}

/// `em` and `ex` of the font in force where the length is read: `12pt` in an article at 10pt is
/// 1.2 em, and 12 / the x-height of cmr10 ex.
#[test]
fn units_hover_gives_em_and_ex_of_the_font_in_force() {
    // cmr10's x-height (`\fontdimen5`) in pt, as cmr10.tfm stores it
    const XHEIGHT: f64 = 4.30554;
    let text = "\\documentclass{article}\n\\begin{document}\nText \\vspace{12pt} and\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("units-font", text, &[]);
    let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 2, col(text, 2, "12pt", 1)));
    let ex = format!("{:.2}", 12.0 / XHEIGHT);
    assert!(md.contains(&format!("≈ 1.2 em ≈ {ex} ex in cmr10 at 10pt")), "{md}");
    assert!(!md.contains("depend on the font"), "{md}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// A length in the source: hover in every unit with the sp, each value a link that writes it
/// and says whether it is exact, a converter view; `\textwidth` shows its value when the run ended.
#[test]
fn units_plugin_converts_lengths() {
    let text = "\\documentclass{article}\n\\begin{document}\nText \\vspace{12pt} and \\hspace{24pt} \\the\\textwidth\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("units", text, &["type_plugins.units.units=[pt,cm,pc]"]);
    let c = col(text, 2, "12pt", 1);
    let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 2, c));
    // everyday units in the headline (≈ where inexact), a to-scale bar, TeX's own units in a grid
    assert!(md.starts_with("### `12pt` ≈ [0.42 cm](command:satex.applyEdit?"), "{md}");
    assert!(md.contains("% of `\\textwidth`") && md.contains("| pc |") && md.contains("[786432]("), "{md}");
    assert!(!md.contains("TeX stores"), "{md}");
    // a grid row never makes the card scroll sideways
    assert!(md.lines().filter(|l| l.contains("---:")).all(|l| l.matches("---:").count() <= 5), "{md}");
    // a share of the widths the document sizes by, as links that write it
    assert!(md.contains("0.035`\\linewidth`](command:satex.applyEdit?") && md.contains("`\\columnwidth`"), "{md}");
    // each value is the action: no separate convert buttons
    let actions = at(&mut server, &uri, "textDocument/codeAction", 2, c);
    let titles: Vec<&str> = actions.as_array().unwrap().iter().filter_map(|a| a["title"].as_str()).collect();
    assert!(!titles.iter().any(|t| t.starts_with("Convert")), "{titles:?}");
    // 24pt is exactly 2pc (a pica is 12pt): its link writes `2pc`, marked exact
    let pica = hover_md(&at(&mut server, &uri, "textDocument/hover", 2, col(text, 2, "24pt", 2)));
    assert!(pica.contains("%222pc%22%2Ctrue%5D"), "{pica}");
    let views = at(&mut server, &uri, "satex/view", 2, c);
    assert_eq!(views[0]["plugin"], "units", "{views}");
    assert_eq!(views[0]["sections"][0]["kind"], "converter");
    assert_eq!(views[0]["sections"][0]["lo"], 786432);
    // a length register: its value when the run ended
    let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 2, col(text, 2, "textwidth", 2)));
    assert!(
        md.contains("### `\\textwidth` at the end of the run = 345 pt ≈ 12.13 cm")
            && md.contains("100% of `\\textwidth`"),
        "{md}"
    );
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
    // switched off: none of it
    let (mut server, uri, dir) = open_set("units-off", text, &["type_plugins.units.enabled=false"]);
    assert!(!hover_md(&at(&mut server, &uri, "textDocument/hover", 2, c)).contains("| cm |"));
    assert_eq!(at(&mut server, &uri, "textDocument/codeAction", 2, c), json!([]));
    assert_eq!(at(&mut server, &uri, "satex/view", 2, c), json!([]));
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Page layout: the view gets the page geometry the run computed, geometry's options as written,
/// and with a KOMA class typearea's `DIV` and `BCOR`; lenses on the class and geometry, a link on its keys.
#[test]
fn page_plugin_reads_the_run_geometry() {
    let text = "\\documentclass{article}\n\\usepackage[margin=2cm]{geometry}\n\\begin{document}\nText\n\n\\section{One}\nMore\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("page", text, &[]);
    let page = |server: &mut Server, line, c| {
        let views = at(server, &uri, "satex/view", line, c);
        views.as_array().unwrap().iter().find(|v| v["plugin"] == "page").map(|v| v["sections"][0].clone())
    };
    let s = page(&mut server, 1, col(text, 1, "margin", 1)).expect("a page view on geometry's load");
    let len = |k: &str| s["run"][k][0].as_i64().unwrap();
    let cm2 = satex::engine::value::parse_dimen("2cm").unwrap();
    assert!((len("paperwidth") - 2 * cm2 - len("textwidth")).abs() < 8, "{s}");
    assert_eq!(s["geometry"]["global"]["options"], "margin=2cm", "{s}");
    assert_eq!(s["koma"], json!(null));
    assert!(s["papers"].as_array().unwrap().iter().any(|p| p["name"] == "a4paper"), "{s}");
    // a sectioning command and a blank line open it, other text not
    assert!(page(&mut server, 5, 1).is_some() && page(&mut server, 4, 0).is_some());
    assert!(page(&mut server, 3, 1).is_none());
    let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 1, col(text, 1, "margin", 1)));
    assert!(md.contains("[Page layout…](command:satex.openView?"), "{md}");
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].clone();
    let lines: Vec<u64> = lenses
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["command"]["arguments"][2] == "page")
        .map(|l| l["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, [0, 1], "{lenses}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
    let text = "\\documentclass[DIV=10,BCOR=5mm]{scrartcl}\n\\begin{document}\nText\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("page-koma", text, &[]);
    let views = at(&mut server, &uri, "satex/view", 0, 2);
    let s = views.as_array().unwrap().iter().find(|v| v["plugin"] == "page").expect("a page view").clone();
    let s = &s["sections"][0];
    assert_eq!(s["koma"]["div"], 10, "{s}");
    assert_eq!(s["koma"]["bcor"], satex::engine::value::parse_dimen("5mm").unwrap(), "{s}");
    assert_eq!(s["koma"]["options"], "DIV=10,BCOR=5mm", "{s}");
    assert_eq!(s["geometry"]["loaded"], false);
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Colors with xcolor: items with their hex, hover in every model, an expression mixed, a picker
/// that writes `[model]{spec}` into `\color`, a `\definecolor` in a TikZ-like key, VS Code's
/// presentation, and `\definecolor` model conversions.
#[test]
fn color_plugin_with_xcolor() {
    let text = "\\documentclass{article}\n\\usepackage{xcolor}\n\\definecolor{brand}{rgb}{0,0.5,0.5}\n\\begin{document}\n\\color{brand} \\textcolor{red!40!blue}{x} \\color{}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("color", text, &[]);
    // a hover waits for the run, a completion does not
    let c = col(text, 4, "brand", 1);
    let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 4, c));
    assert!(md.contains("| HTML | `008080` |") && md.contains("| cmyk | `1,0,0,0.5` |"), "{md}");
    let items = complete(&mut server, &uri, 4, col(text, 4, "{}", 1));
    let brand = items.iter().find(|i| i["label"] == "brand").expect("brand offered");
    assert_eq!(brand["detail"], "#008080");
    let mixed = hover_md(&at(&mut server, &uri, "textDocument/hover", 4, col(text, 4, "red!40", 1)));
    assert!(mixed.contains("`660099`"), "{mixed}");
    let views = at(&mut server, &uri, "satex/view", 4, c);
    let color = &views[0]["sections"][0];
    assert_eq!(color["kind"], "color", "{views}");
    assert_eq!(color["models"][0], "HTML");
    assert_eq!(color["apply"]["edits"][0]["newText"], "[${model}]{${spec}}");
    let group = |name: &str| {
        color["defined"].as_array().unwrap().iter().find(|d| d["name"] == name).map(|d| d["group"].clone())
    };
    // each color names who defined it: the document's own, else the package file
    assert_eq!((group("brand"), group("red")), (Some(json!("document")), Some(json!("kernel"))), "{color}");
    let id = server.request(
        "textDocument/colorPresentation",
        json!({ "textDocument": { "uri": uri }, "color": { "red": 1.0, "green": 0.0, "blue": 0.0, "alpha": 1.0 },
                "range": { "start": { "line": 4, "character": c - 1 }, "end": { "line": 4, "character": c + 4 } } }),
    );
    let shown = server.response(id)["result"].clone();
    assert_eq!(shown[0]["textEdit"]["newText"], "[HTML]{FF0000}", "{shown}");
    let id = server.request("textDocument/documentColor", json!({ "textDocument": { "uri": uri } }));
    let colors = server.response(id)["result"].clone();
    assert!(colors.as_array().is_some_and(|c| !c.is_empty()), "{colors}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// A `\definecolor`'s spec converts to another model. Red: the name a `\definecolor` defines
/// has no color type at the cursor yet (no slot, no use), so no plugin triggers there.
#[test]
fn color_plugin_converts_a_definition() {
    let text = "\\documentclass{article}\n\\usepackage{xcolor}\n\\definecolor{brand}{rgb}{0,0.5,0.5}\n\\begin{document}\n\\color{brand} x\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("color-def", text, &[]);
    // the definition's spec in another model: a link on the hover's value
    let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 2, col(text, 2, "brand", 1)));
    assert!(md.contains("%7BHTML%7D%7B008080%7D"), "{md}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Without xcolor the picker defines a color in a model the color package reads; switched off,
/// no items, swatches or views.
#[test]
fn color_plugin_without_xcolor_and_off() {
    let text = "\\documentclass{article}\n\\usepackage{color}\n\\definecolor{brand}{rgb}{0,0.5,0.5}\n\\begin{document}\n\\color{brand} x\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("color-plain", text, &[]);
    let c = col(text, 4, "brand", 1);
    let views = at(&mut server, &uri, "satex/view", 4, c);
    let color = &views[0]["sections"][0];
    assert_eq!(color["models"], json!(["rgb", "cmyk", "gray"]), "{views}");
    let edits = color["apply"]["edits"].as_array().unwrap();
    assert!(edits[0]["newText"].as_str().unwrap().starts_with("\\definecolor{${name}}"), "{edits:?}");
    assert_eq!(edits[0]["range"]["start"]["line"], 3);
    assert_eq!(edits[1]["newText"], "${name}");
    assert_eq!(color["apply"]["name"], "mycolor");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
    let (mut server, uri, dir) = open_set("color-off", text, &["type_plugins.color.enabled=false"]);
    // the core's names a `\csname` formed stay; the plugin's color items (kind 16) go
    assert!(complete(&mut server, &uri, 4, c).iter().all(|i| i["kind"] != 16));
    assert_eq!(at(&mut server, &uri, "satex/view", 4, c), json!([]));
    let id = server.request("textDocument/documentColor", json!({ "textDocument": { "uri": uri } }));
    assert_eq!(server.response(id)["result"], json!([]));
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// After a switch in a key list, `, ` offers the family's keys and no units (a length written
/// before is no number at the cursor); after a choice key's `=` its choices, not the keys.
#[test]
fn key_list_after_a_switch_offers_keys_not_units() {
    let text = "\\documentclass{article}\n\\usepackage{pgfkeys}\n\\newif\\ifdemodraft\n\\pgfkeys{/demo/.is family, /demo,\n  width/.initial=6bp,\n  draft/.is if=demodraft,\n  mode/.is choice, mode/fast/.code={}, mode/safe/.code={},\n}\n\\begin{document}\n\\pgfkeys{/demo/mode=fast, /demo/draft, }\n\\pgfkeys{/demo/width=6bp, /demo/mode=}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("keys-switch", text, &[]);
    let after = complete(&mut server, &uri, 9, col(text, 9, ", }", 2));
    assert!(!after.iter().any(|i| i["kind"] == 11), "units offered: {:?}", labels(&after));
    assert!(labels(&after).iter().any(|l| l.starts_with("/demo/width")), "{:?}", labels(&after));
    let values = complete(&mut server, &uri, 10, col(text, 10, "}", 0));
    assert_eq!(labels(&values), ["fast", "safe"]);
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Hovering in a formula shows it as the run set it, a user macro expanded: on a letter and on
/// the macro's own name.
#[test]
fn hovering_inline_math_shows_the_formula_set() {
    let text = "\\documentclass{article}\n\\usepackage{amsmath}\n\\newcommand\\norm[1]{\\lVert#1\\rVert}\n\\begin{document}\nSo $\\norm{x}^2$ holds.\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("math-hover", text, &[]);
    for (needle, off) in [("x}", 0), ("\\norm", 2)] {
        let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 4, col(text, 4, needle, off)));
        assert!(md.contains("`‖x‖^2`") && md.contains("data:image/svg+xml"), "{needle}: {md}");
    }
    // plain text has no formula
    let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 4, col(text, 4, "holds", 1)));
    assert!(!md.contains("Formula"), "{md}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Keys of a keyval family (graphicx's `Gin`) show their docs on hover, with or without a value.
#[test]
fn hovering_a_graphics_key_shows_it() {
    let text = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\includegraphics[width=1cm, alt={a plot}, clip]{example-image}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("gin-hover", text, &[]);
    for key in ["width", "alt", "clip"] {
        let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 3, col(text, 3, key, 1)));
        assert!(md.contains(&format!("Key `{key}`")), "{key}: {md}");
    }
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// A graphics key's item says in words what it takes, and resolves to docs read off its code.
#[test]
fn graphics_key_items_explain_themselves() {
    let text = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\includegraphics[]{example-image}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("gin-complete", text, &[]);
    let items = complete(&mut server, &uri, 3, col(text, 3, "]", 0));
    let find = |label: &str| items.iter().find(|i| i["label"] == label).cloned().expect(label);
    let alt = find("alt");
    assert!(alt["detail"].as_str().is_some_and(|d| d.starts_with("needs a value") && d.ends_with("Gin")), "{alt}");
    let clip = find("clip");
    assert!(clip["detail"].as_str().is_some_and(|d| d.starts_with("default true")), "{clip}");
    let docs = |item: Value, server: &mut Server| {
        let id = server.request("completionItem/resolve", item);
        server.response(id)["result"]["documentation"]["value"].as_str().unwrap_or_default().to_string()
    };
    let width = docs(find("width"), &mut server);
    assert!(width.contains("Sets `\\Gin@ewidth` to the value") && width.contains("graphicx.sty:"), "{width}");
    let alt = docs(alt, &mut server);
    assert!(alt.contains("Key `alt`") && alt.contains("needs a value") && alt.contains("code is empty"), "{alt}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Key lists: completion with nothing typed after `,`, a family that goes on with its keys, hover
/// docs on a family; switched off, no key items.
#[test]
fn keys_plugin_completes_and_documents_keys() {
    let text = "\\documentclass{article}\n\\usepackage{pgfkeys}\n% /demo is a good family\n\\pgfkeys{/demo/.is family}\n\\pgfkeys{/demo,\n  mode/.is choice, mode/fast/.code={}, mode/safe/.code={},\n  width/.initial=2cm,\n}\n\\begin{document}\n\\pgfkeys{/demo/mode=fast, /demo}\n\\pgfkeys{/demo/mode=fast, }\n\\pgfkeys{/de}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("keys", text, &[]);
    let empty = complete(&mut server, &uri, 10, col(text, 10, ", }", 2));
    assert!(labels(&empty).iter().any(|l| l.starts_with("/demo")), "{:?}", labels(&empty));
    let family = complete(&mut server, &uri, 11, col(text, 11, "}", 0));
    let demo = family.iter().find(|i| i["label"] == "/demo").expect("the family offered");
    assert_eq!(demo["textEdit"]["newText"], "/demo/", "{demo}");
    assert_eq!(demo["command"]["command"], "editor.action.triggerSuggest");
    let md = hover_md(&at(&mut server, &uri, "textDocument/hover", 9, col(text, 9, ", /demo", 4)));
    assert!(md.contains("is family") && md.contains("good family"), "{md}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
    let (mut server, uri, dir) = open_set("keys-off", text, &["type_plugins.keys.enabled=false"]);
    let family = complete(&mut server, &uri, 11, col(text, 11, "}", 0));
    assert!(!labels(&family).contains(&"/demo"), "{:?}", labels(&family));
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// The text `range` (an LSP range) covers in `text`.
fn ranged(text: &str, range: &Value) -> String {
    let at = |p: &Value| {
        let line = p["line"].as_u64().unwrap() as usize;
        let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
        start + p["character"].as_u64().unwrap() as usize
    };
    text[at(&range["start"])..at(&range["end"])].to_string()
}

/// A float's placement: the hover says what each letter lets LaTeX do (the class's default when
/// none is written), completion offers the letters not written yet, float.sty adds `H`, and the
/// dialog writes the brackets back, dropped when empty, an unknown letter refused.
#[test]
fn a_float_placement_hovers_completes_and_writes() {
    let text = "\\documentclass{article}\n\\usepackage{float}\n\\begin{document}\n\\begin{figure}[ht]\nx\n\\end{figure}\n\\begin{table}\ny\n\\end{table}\n\\end{document}\n";
    let (mut server, uri, _, _) = open_loaded("float", text);
    let (fig, tab) = (3, 6);
    let ask = |server: &mut Server, method: &str, params: Value| {
        let id = server.request(method, params);
        server.response(id)["result"].clone()
    };
    let at = |line: u32, character: u32| json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } });
    let hover = ask(&mut server, "textDocument/hover", at(fig, 15));
    let md = hover["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(
        md.contains("Placement `[ht]`")
            && md.contains("[✓ `h`](command:satex.applyEdit?")
            && md.contains("**on**: here")
            && md.contains("**on**: at the top"),
        "{md}"
    );
    assert!(
        md.contains("[○ `b`](command:satex.applyEdit?") && md.contains(") off: at the bottom"),
        "every letter, the written ones on: {md}"
    );
    // each toggle is a link that writes the brackets with its letter switched
    let writes: Vec<String> = md
        .match_indices("command:satex.applyEdit?")
        .map(|(k, m)| {
            let arg = &md[k + m.len()..];
            let arg = &arg[..arg.find(')').unwrap()];
            let (mut bytes, mut i) = (Vec::new(), 0);
            while i < arg.len() {
                let hex = (arg.as_bytes()[i] == b'%').then(|| u8::from_str_radix(&arg[i + 1..i + 3], 16).unwrap());
                bytes.push(hex.unwrap_or(arg.as_bytes()[i]));
                i += if hex.is_some() { 3 } else { 1 };
            }
            serde_json::from_slice::<Value>(&bytes).unwrap()[2].as_str().unwrap().to_string()
        })
        .collect();
    assert_eq!(writes, ["[t]", "[h]", "[htb]", "[htp]", "[ht!]", "[H]"], "{md}");
    assert!(
        md.contains("Limits as the run ended: `topnumber` 2 · `\\topfraction` .7"),
        "the limits as the run ended: {md}"
    );
    let md = ask(&mut server, "textDocument/hover", at(tab, 3))["contents"]["value"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        md.contains("default `tbp`") && md.contains("`p`](command:satex.applyEdit?"),
        "the class's \\fps@table: {md}"
    );
    assert!(md.contains("does not matter"), "the order is lost: {md}");
    let items = complete(&mut server, &uri, fig, 16);
    assert_eq!(labels(&items), ["b", "p", "!", "H"], "the letters not written, H from float.sty");
    assert_eq!(items[0]["textEdit"]["range"]["start"]["character"], 16, "inserted at the cursor: {}", items[0]);
    assert!(items[0]["documentation"]["value"].as_str().is_some_and(|d| d.contains("does not matter")));
    assert!(!labels(&complete(&mut server, &uri, fig, 18)).contains(&"H"), "no letters after the `]`");
    let lenses = ask(&mut server, "textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let float = lenses.as_array().unwrap().iter().find(|l| l["command"]["title"] == "Edit placement…").unwrap();
    assert!(float["command"]["arguments"][0]["letters"].is_null(), "no inline chips: the card toggles: {float}");
    let view = ask(&mut server, "satex/dialog", at(fig, 2));
    assert_eq!((view["kind"].as_str(), view["state"]["spec"].as_str()), (Some("float"), Some("ht")), "{view}");
    let mut apply = |spec: &str| {
        let state = json!({ "env": "figure", "spec": spec });
        ask(
            &mut server,
            "satex/dialogApply",
            json!({ "textDocument": { "uri": uri }, "kind": "float", "range": view["range"], "state": state }),
        )
    };
    assert_eq!(apply("tb!")["edits"][0]["newText"], "\\begin{figure}[tb!]");
    assert_eq!(apply("")["edits"][0]["newText"], "\\begin{figure}");
    assert!(apply("x").is_null(), "an unknown letter is refused");
}

/// array's column injections: a spec's `>{…}` and `<{…}` reach the dialog's state, edited ones are
/// written back into the spec, and `\usepackage{array}` is added when the run lacks it.
#[test]
fn a_table_column_keeps_and_writes_its_injections() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\begin{tabular}{>{\\bfseries}l c}\na & b \\\\\n\\end{tabular}\n\\end{document}\n";
    let (mut server, uri, _, _) = open_loaded("inject", text);
    let mut ask = |method: &str, params: Value| {
        let id = server.request(method, params);
        server.response(id)["result"].clone()
    };
    let view =
        ask("satex/dialog", json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": 2 } }));
    assert_eq!(view["state"]["cols"][0]["pre"], ">{\\bfseries}", "{view}");
    let mut state = view["state"].clone();
    state["cols"][1]["pre"] = json!(">{$}");
    state["cols"][1]["post"] = json!("<{$}");
    let out = ask(
        "satex/dialogApply",
        json!({ "textDocument": { "uri": uri }, "kind": "table", "range": view["range"], "state": state }),
    );
    let written = out["edits"][0]["newText"].as_str().unwrap_or_default();
    assert!(written.contains(">{\\bfseries}l") && written.contains(">{$}c<{$}}"), "{out}");
    assert_eq!(out["edits"][1]["newText"], "\\usepackage{array}\n", "array is added: {out}");
}

/// A column made S however (the type menu, not the Numbers → S rewrite) braces its text cells, which
/// siunitx would read as numbers, and loads siunitx once, on its own line before `\begin{document}`.
#[test]
fn an_s_column_braces_its_text_and_loads_siunitx() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\begin{tabular}{lc}\nMethod & Score \\\\\nOurs & 0.92 \\\\\n\\end{tabular}\n\\end{document}\n";
    let (mut server, uri, _, _) = open_loaded("scolumn", text);
    let mut ask = |method: &str, params: Value| {
        let id = server.request(method, params);
        server.response(id)["result"].clone()
    };
    let view =
        ask("satex/dialog", json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": 2 } }));
    let mut state = view["state"].clone();
    state["cols"][1]["align"] = json!("S");
    let out = ask(
        "satex/dialogApply",
        json!({ "textDocument": { "uri": uri }, "kind": "table", "range": view["range"], "state": state }),
    );
    let written = out["edits"][0]["newText"].as_str().unwrap_or_default();
    assert!(written.contains("{lS}") && written.contains("{Score}") && written.contains("& 0.92"), "{out}");
    let loads: Vec<&Value> = out["edits"].as_array().unwrap()[1..].iter().collect();
    assert_eq!(loads.len(), 1, "{out}");
    assert_eq!(loads[0]["newText"], "\\usepackage{siunitx}\n", "{out}");
    assert_eq!(loads[0]["range"]["start"], json!({ "line": 1, "character": 0 }), "before \\begin{{document}}: {out}");
}

/// A table and each figure get a code lens and a hover link to their
/// dialog; the dialog's state, unchanged, writes the text it came from; the
/// code actions rewrite the table for booktabs and siunitx, adding the
/// packages the preamble lacks, and rotate or fit the figure.
#[test]
fn tables_and_figures_open_dialogs_that_write_them_back() {
    let text = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\begin{tabular}{|l|c|}\n  \\hline\n  Method & Score \\\\ \\hline\n  Ours & 0.92 \\\\ % best\n  \\multicolumn{2}{c}{a {&} b} \\\\\n  Base & 11.5 \\\\\n  \\hline\n\\end{tabular}\n\\includegraphics[trim=1cm 2 3 4,clip,angle=90,width=3cm]{fig}\n\\includegraphics[viewport=0 0 100 50, clip]{fig}\n\\end{document}\n";
    let dir = crate::support::tmp_root().join(format!("satex-lsp-dialogs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), text).unwrap();
    let banner = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/documents/includegraphics/title-banner-image.pdf");
    std::fs::copy(banner, dir.join("fig.pdf")).unwrap();
    let mut server = serve(&dir, true);
    let (uri, _) = open_file(&mut server, &dir, "main.tex");
    let mut ask = |method: &str, params: Value| {
        let id = server.request(method, params);
        server.response(id)["result"].clone()
    };
    let lenses = ask("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses: Vec<(u64, &str)> = (lenses.as_array().unwrap().iter())
        .map(|l| (l["range"]["start"]["line"].as_u64().unwrap(), l["command"]["title"].as_str().unwrap()))
        .filter(|(_, t)| !t.starts_with("Page layout"))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(line, t)| (line, if t.contains("table") { "table" } else { "image" }))
        .collect();
    let (table, figure) = (3, 11);
    assert_eq!(lenses, [(table, "table"), (figure, "image"), (figure + 1, "image")]);
    let hover = ask(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": table, "character": 3 } }),
    );
    assert!(hover["contents"]["value"].as_str().unwrap().contains("command:satex.openDialog"), "{hover}");
    for line in [table, figure, figure + 1] {
        let view = ask(
            "satex/dialog",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 1 } }),
        );
        let applied = ask(
            "satex/dialogApply",
            json!({ "textDocument": { "uri": uri }, "kind": view["kind"], "range": view["range"], "state": view["state"] }),
        );
        assert_eq!(applied["edits"][0]["newText"].as_str().unwrap(), ranged(text, &view["range"]), "{view}");
        if view["kind"] == "image" {
            assert!(view["state"]["natural"].is_array(), "the engine's size of the file: {view}");
            // the run's graphics driver, and that it reads `decodearray` (pdftex.def sets `\Gin@decode`)
            let ctx = &view["context"];
            assert_eq!((ctx["driver"].as_str(), ctx["decode"].as_bool()), (Some("pdftex.def"), Some(true)), "{view}");
        }
        if line == figure + 1 {
            assert_eq!(view["state"]["viewport"], true, "{view}");
        }
    }
    // an X column switches to tabularx; a \multirow cell and the X column add their packages, with notes
    let view =
        ask("satex/dialog", json!({ "textDocument": { "uri": uri }, "position": { "line": table, "character": 1 } }));
    let mut state = view["state"].clone();
    state["cols"][1]["align"] = json!("X");
    state["rows"][1]["cells"][0]["text"] = json!("\\multirow{2}{*}{Ours}");
    let applied = ask(
        "satex/dialogApply",
        json!({ "textDocument": { "uri": uri }, "kind": "table", "range": view["range"], "state": state }),
    );
    let new = applied["edits"][0]["newText"].as_str().unwrap();
    assert!(new.starts_with("\\begin{tabularx}{\\linewidth}{|l|X|}") && new.ends_with("\\end{tabularx}"), "{applied}");
    assert!(new.contains("\\multirow{2}{*}{Ours} & 0.92"), "{new}");
    let packages: Vec<&str> =
        applied["edits"].as_array().unwrap()[1..].iter().filter_map(|e| e["newText"].as_str()).collect();
    assert_eq!(packages, ["\\usepackage{tabularx}\n", "\\usepackage{multirow}\n"], "{applied}");
    assert_eq!(applied["state"]["env"], "tabularx", "{applied}");
    let at = |line: u64| json!({ "start": { "line": line, "character": 0 }, "end": { "line": line, "character": 0 } });
    let actions = ask(
        "textDocument/codeAction",
        json!({ "textDocument": { "uri": uri }, "range": at(table + 2), "context": { "diagnostics": [] } }),
    );
    let titled =
        |t: &str| actions.as_array().unwrap().iter().find(|a| a["title"].as_str().unwrap().starts_with(t)).cloned();
    assert!(titled("Edit table").is_some_and(|a| a["command"]["command"] == "satex.openDialog"), "{actions}");
    let booktabs = titled("Convert table to booktabs").expect("a booktabs rewrite");
    assert!(booktabs["title"].as_str().unwrap().contains("\\usepackage{booktabs}"), "{booktabs}");
    let edits = booktabs["edit"]["changes"][&uri].as_array().unwrap().clone();
    let new = edits[0]["newText"].as_str().unwrap();
    assert!(
        new.starts_with(
            "\\begin{tabular}{lc}\n  \\toprule\n  Method & Score \\\\ \\midrule\n  Ours & 0.92 \\\\ % best\n"
        ),
        "{new}"
    );
    assert!(new.ends_with("  \\bottomrule\n\\end{tabular}"), "{new}");
    assert_eq!(edits[1]["newText"], "\\usepackage{booktabs}\n");
    let siunitx = titled("Align numbers with siunitx").expect("an S column rewrite");
    let new = siunitx["edit"]["changes"][&uri][0]["newText"].as_str().unwrap().to_string();
    assert!(new.starts_with("\\begin{tabular}{|l|S[table-format=2.2]|}\n  \\hline\n  Method & {Score} \\\\"), "{new}");
    let actions = ask(
        "textDocument/codeAction",
        json!({ "textDocument": { "uri": uri }, "range": at(figure), "context": { "diagnostics": [] } }),
    );
    let rotate =
        (actions.as_array().unwrap().iter()).find(|a| a["title"] == "Rotate image by 90°").expect("a rotation");
    assert_eq!(
        rotate["edit"]["changes"][&uri][0]["newText"],
        "\\includegraphics[trim=1cm 2 3 4,clip,angle=180,width=3cm]{fig}"
    );
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Switched off, the table and image plugins offer no lens, hover link or action.
#[test]
fn switched_off_dialog_plugins_offer_nothing() {
    let text = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\begin{tabular}{l}\na \\\\\n\\end{tabular}\n\\includegraphics{example-image}\n\\end{document}\n";
    let off =
        ["type_plugins.table.enabled=false", "type_plugins.image.enabled=false", "type_plugins.page.enabled=false"];
    let (mut server, uri, dir) = open_set("dialogs-off", text, &off);
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    assert_eq!(server.response(id)["result"], json!([]));
    let range = json!({ "start": { "line": 3, "character": 0 }, "end": { "line": 6, "character": 0 } });
    let id = server.request(
        "textDocument/codeAction",
        json!({ "textDocument": { "uri": uri }, "range": range, "context": { "diagnostics": [] } }),
    );
    let actions = server.response(id)["result"].clone();
    assert!(!actions.to_string().contains("satex.openDialog"), "{actions}");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `satex query type-plugins` counts where each plugin shows on the run's main file (the site's
/// tiles take a plugin's demo from it), and nothing where no plugin applies.
#[test]
fn type_plugins_count_their_places_in_the_main_file() {
    let dir =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-tp-places-{}", std::process::id())));
    let table = "\\begin{tabular}{l}\na \\\\\n\\end{tabular}\n";
    let places = |name: &str, body: &str| {
        std::fs::write(
            dir.join("doc.tex"),
            format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\\end{{document}}\n"),
        )
        .unwrap();
        let out = crate::support::satex()
            .args(["query", "type-plugins", "--format", "json", "-f", "doc.tex"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let records: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(records.as_array().unwrap().iter().all(|r| r["purpose"].is_string()), "{records}");
        let of = |n: &str| records.as_array().unwrap().iter().find(|r| r["name"] == n).map(|r| r["places"].clone());
        (of(name), of("image"))
    };
    assert_eq!(places("table", &format!("{table}{table}")), (Some(json!(2)), Some(json!(0))));
    assert_eq!(places("table", "text\n"), (Some(json!(0)), Some(json!(0))));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Inside a finding's range the hover shows the finding first, then the hover of the word under
/// the cursor: the macro's card on `\label`, the label's card on its key.
#[test]
fn a_finding_s_hover_keeps_the_word_s_hover() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\section{A}\\label{sec:x} text\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("finding-hover", text, &[]);
    let card = |md: &str| md.find("unused-label");
    let on_cs = hover_md(&at(&mut server, &uri, "textDocument/hover", 2, col(text, 2, "\\label", 2)));
    let macro_at = on_cs.find("\\label").filter(|_| on_cs.contains("---"));
    assert!(card(&on_cs).is_some() && macro_at.is_some(), "{on_cs}");
    assert!(card(&on_cs) < on_cs.rfind("---"), "the finding comes first: {on_cs}");
    // the macro's full card: its signature with typed arguments
    assert!(on_cs.contains("`\\label{⟨"), "the signature: {on_cs}");
    let on_key = hover_md(&at(&mut server, &uri, "textDocument/hover", 2, col(text, 2, "sec:x", 2)));
    let key_at = on_key.find("Label `sec:x`");
    assert!(card(&on_key).is_some() && key_at.is_some() && card(&on_key) < key_at, "{on_key}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// A command item carries its signature beside the label, a tab stop per argument (an optional
/// one with its brackets), opens the signature help once accepted, and resolves to its card.
#[test]
fn command_items_carry_their_signature() {
    let text = "\\documentclass{article}\n\\newcommand\\greet[2][hi]{#1 #2}\n\\begin{document}\n\\greet{you}\n\\gre\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("signature-items", text, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 3, 2);
    let items = complete(&mut server, &uri, 4, 4);
    let greet = items.iter().find(|i| i["label"] == "greet").expect("greet offered");
    let signature = greet["labelDetails"]["detail"].as_str().unwrap_or_default();
    assert!(signature.starts_with("[⟨") && signature.matches('⟨').count() == 2, "{greet}");
    let snippet = greet["insertText"].as_str().unwrap_or_default();
    let stops = (1..=3).filter(|n| snippet.contains(&format!("${{{n}:"))).count();
    assert_eq!(stops, 2, "{snippet}");
    assert!(snippet.contains("${1:[") && snippet.ends_with("$0"), "{snippet}");
    assert_eq!(greet["command"]["command"], "editor.action.triggerParameterHints");
    let id = server.request("completionItem/resolve", greet.clone());
    let resolved = server.response(id)["result"].clone();
    let card = resolved["documentation"]["value"].as_str().unwrap_or_default();
    assert!(card.contains("\\greet"), "{resolved}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// "Change environment": one view on a `\\begin` name listing only environments making the same
/// kind of material (no flat actions); center → flushright edits that pair's two names only.
#[test]
fn change_environment_edits_both_ends_only() {
    let text = "\\documentclass{article}\n\\usepackage{longtable,tabularx}\n\\begin{document}\n\\begin{center}\nA\n\\end{center}\n\\begin{flushright}B\\end{flushright}\n\\begin{tabular}{l}x\\end{tabular}\n\\begin{itemize}\\item a\\end{itemize}\n\\begin{figure}F\\end{figure}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("change-env", text, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 3, 9);
    let actions = at(&mut server, &uri, "textDocument/codeAction", 3, 9);
    let flat =
        actions.as_array().unwrap().iter().any(|a| a["title"].as_str().is_some_and(|t| t.starts_with("Change ")));
    assert!(!flat, "{actions}");
    let items = |line: u32, col: u32, server: &mut Server| -> Vec<Value> {
        let views = at(server, &uri, "satex/view", line, col);
        let view = views.as_array().into_iter().flatten().find(|v| v["plugin"] == "environment");
        view.map(|v| v["sections"][0]["items"].as_array().cloned().unwrap_or_default()).unwrap_or_default()
    };
    let names =
        |items: &[Value]| -> Vec<String> { items.iter().map(|i| i["title"].as_str().unwrap().to_string()).collect() };
    let center = items(3, 9, &mut server);
    let to = center.iter().find(|i| i["title"] == "flushright").expect("flushright offered");
    let lines: Vec<u64> =
        to["edits"].as_array().unwrap().iter().map(|e| e["range"]["start"]["line"].as_u64().unwrap()).collect();
    assert_eq!(lines, [3, 5], "{to}");
    let center = names(&center);
    assert!(!center.iter().any(|n| ["itemize", "tabular", "figure", "document"].contains(&n.as_str())), "{center:?}");
    let tabular = names(&items(7, 9, &mut server));
    for want in ["array", "tabular*", "longtable"] {
        assert!(tabular.iter().any(|n| n == want), "{want} missing from {tabular:?}");
    }
    for not in ["center", "document", "equation", "figure", "table", "itemize"] {
        assert!(!tabular.iter().any(|n| n == not), "{not} offered for a tabular: {tabular:?}");
    }
    let itemize = names(&items(8, 9, &mut server));
    assert!(itemize.iter().any(|n| n == "enumerate") && !itemize.iter().any(|n| n == "center"), "{itemize:?}");
    let figure = names(&items(9, 9, &mut server));
    assert!(figure.iter().any(|n| n == "table") && !figure.iter().any(|n| n == "tabular"), "{figure:?}");
    assert!(items(2, 9, &mut server).is_empty(), "document is switched to nothing");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// A relative `-f` root, run first for an input it reads: the root's own lens still finds its
/// table (the run cached under the canonical key keeps the name it was started with).
#[test]
fn lenses_of_a_relative_root_run_through_its_input() {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-relroot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let main = "\\documentclass{article}\n\\input{part}\n\\begin{document}\n\\begin{tabular}{l}\nx\n\\end{tabular}\n\\end{document}\n";
    std::fs::write(dir.join("main.tex"), main).unwrap();
    std::fs::write(dir.join("part.tex"), "\\newcommand\\x{y}\n").unwrap();
    let mut server =
        Server::start_with(&crate::support::test_cache(), &dir, &["--set", "limits.seconds=0", "-f", "main.tex"]);
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    let _ = open_file(&mut server, &dir, "part.tex");
    let (uri, _) = open_file(&mut server, &dir, "main.tex");
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].clone();
    assert!(lenses.as_array().unwrap().iter().any(|l| l["command"]["title"] == "Edit table…"), "{lenses}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Each widget target has its lens: the figure's placement, its image, and a table, an environment whose
/// code reaches `\@array`: the tabular in a float gets the table lens,
/// the tikzpicture the drawing's only, though its code saves amsmath's `\matrix` (`\let\tikz@origmatrix=\matrix`).
#[test]
fn table_lens_marks_tables_not_drawings() {
    let text = "\\documentclass{article}\n\\usepackage{listings}\n\\usepackage{tikz}\n\\usepackage{glossaries}\n\\usepackage{graphicx}\n\\begin{document}\n\\begin{figure}[h]\n\\includegraphics{example-image}\n\\end{figure}\n\\begin{table}[ht]\\begin{tabular}{lc}\na & b \\\\\n\\end{tabular}\n\\end{table}\n\\begin{tikzpicture}[node distance=2cm, every node/.style={draw}]\n\\node (a) {x};\n\\end{tikzpicture}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("table-vs-tikz", text, &[]);
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].clone();
    let titles = |line: u64| -> Vec<String> {
        (lenses.as_array().unwrap().iter())
            .filter(|l| l["range"]["start"]["line"] == line)
            .map(|l| l["command"]["title"].as_str().unwrap().to_string())
            .collect()
    };
    let line = |s: &str| text.lines().position(|l| l.contains(s)).unwrap() as u64;
    assert!(titles(line("\\begin{tabular}")).contains(&"Edit table…".to_string()), "{lenses}");
    // the float and its tabular share a line: each lens opens its own dialog
    for lens in lenses.as_array().unwrap().iter().filter(|l| l["command"]["command"] == "satex.openDialog") {
        let id = server
            .request("satex/dialog", json!({ "textDocument": { "uri": uri }, "position": lens["range"]["start"] }));
        let kind = match lens["command"]["title"].as_str() {
            Some("Edit table…") => "table",
            Some("Edit image…") => "image",
            _ => "float",
        };
        assert_eq!(server.response(id)["result"]["kind"], kind, "{lens}");
    }
    assert_eq!(titles(line("\\begin{figure}")), ["Edit placement…"], "{lenses}");
    assert_eq!(titles(line("\\includegraphics")), ["Edit image…"], "{lenses}");
    let tikz = titles(line("\\begin{tikzpicture}"));
    assert!(!tikz.iter().any(|t| t.contains("table")) && !tikz.is_empty(), "{lenses}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// The way VS Code asks: lenses right at `didOpen`, before any analysis, then again on
/// `workspace/codeLens/refresh`, which the server sends once the run is in. The table, the
/// figure's placement and image, and the drawing get theirs without an edit of the document.
#[test]
fn lenses_asked_before_the_run_come_with_its_refresh() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usepackage{graphicx}\n\\begin{document}\n\\begin{figure}[ht]\n\\includegraphics{example-image}\n\\end{figure}\n\\begin{tabular}{lr}\na & b \\\\\n\\end{tabular}\n\\begin{tikzpicture}\n\\node (a) {x};\n\\end{tikzpicture}\n\\end{document}\n";
    let dir = crate::support::tmp_root().join(format!("satex-lsp-lens-refresh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), text).unwrap();
    let mut server = Server::start_with(&crate::support::test_cache(), &dir, &["--set", "limits.seconds=0"]);
    let caps = json!({ "workspace": { "codeLens": { "refreshSupport": true } } });
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": caps }));
    server.response(id);
    server.notify("initialized", json!({}));
    let uri = fixture_uri(&dir.join("main.tex"));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "latex", "version": 1, "text": text } }),
    );
    server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    // the refresh, a request from the server, comes before the run's diagnostics
    let refresh = loop {
        let msg = server.read();
        match msg["method"].as_str() {
            Some("workspace/codeLens/refresh") => break msg,
            Some("textDocument/publishDiagnostics") => panic!("the run is in, the lenses asked before it stay empty"),
            _ => {}
        }
    };
    server.write(&json!({ "jsonrpc": "2.0", "id": refresh["id"], "result": null }));
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].clone();
    let line = |s: &str| text.lines().position(|l| l.contains(s)).unwrap() as u64;
    let titles = |at: &str| -> Vec<&str> {
        (lenses.as_array().unwrap().iter())
            .filter(|l| l["range"]["start"]["line"] == line(at))
            .filter_map(|l| l["command"]["title"].as_str())
            .collect()
    };
    assert_eq!(titles("\\begin{tabular}"), ["Edit table…"], "{lenses}");
    assert_eq!(titles("\\begin{figure}"), ["Edit placement…"], "{lenses}");
    assert_eq!(titles("\\includegraphics"), ["Edit image…"], "{lenses}");
    assert!(!titles("\\begin{tikzpicture}").is_empty(), "{lenses}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

const TREE: &str = "\\documentclass{article}\n\\usepackage{forest}\n\\begin{document}\n\\begin{forest}\n  for tree={draw}\n  [VP [DP, name=dp] [V]]\n\\end{forest}\n\n\\end{document}\n";

/// "Edit tree…": a lens on the tree, a view with its body and the nodes forest made, in
/// bracket order with their content; a blank line offers a new tree.
#[test]
fn forest_plugin_shows_the_runs_nodes() {
    let (mut server, uri, dir) = open_set("forest", TREE, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 5, 3);
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].clone();
    let lens = lenses.as_array().unwrap().iter().find(|l| l["command"]["arguments"][2] == "forest").cloned();
    assert_eq!(lens.expect("a tree lens")["range"]["start"]["line"], 3, "{lenses}");
    let views = at(&mut server, &uri, "satex/view", 5, 3);
    let tree = &views.as_array().unwrap().iter().find(|v| v["plugin"] == "forest").expect("a tree")["sections"][0];
    assert_eq!(tree["kind"], "forest");
    let body = TREE.split_once("\\begin{forest}").unwrap().1.split_once("\\end{forest}").unwrap().0;
    assert_eq!(tree["body"], body);
    let contents: Vec<&str> = tree["nodes"].as_array().unwrap().iter().filter_map(|n| n["content"].as_str()).collect();
    assert_eq!(contents, ["VP", "DP", "V"], "{tree}");
    assert_eq!(tree["nodes"][1]["name"], "dp", "{tree}");
    let actions = at(&mut server, &uri, "textDocument/codeAction", 7, 0);
    assert!(actions.as_array().unwrap().iter().any(|a| a["title"] == "Insert forest tree"), "{actions}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Without forest the new tree loads it; `enabled: false` takes lens, view and action away.
#[test]
fn forest_plugin_new_tree_and_off() {
    let text = "\\documentclass{article}\n\\begin{document}\nText\n\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("forest-new", text, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 2, 1);
    let actions = at(&mut server, &uri, "textDocument/codeAction", 3, 0);
    let new =
        actions.as_array().unwrap().iter().find(|a| a["title"] == "Insert forest tree (adds \\usepackage{forest})");
    let edits = new.expect("a new tree")["edit"]["changes"][&uri].as_array().unwrap().clone();
    assert!(edits[0]["newText"].as_str().unwrap().starts_with("\\begin{forest}"), "{edits:?}");
    assert_eq!(edits[1]["newText"], "\\usepackage{forest}\n");
    let views = at(&mut server, &uri, "satex/view", 3, 0);
    assert!(views.as_array().unwrap().iter().any(|v| v["plugin"] == "forest" && v["title"] == "New tree"), "{views}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
    let (mut server, uri, dir) = open_set("forest-off", TREE, &["type_plugins.forest.enabled=false"]);
    let _ = at(&mut server, &uri, "textDocument/hover", 5, 3);
    assert!(!at(&mut server, &uri, "satex/view", 5, 3).as_array().unwrap().iter().any(|v| v["plugin"] == "forest"));
    let actions = at(&mut server, &uri, "textDocument/codeAction", 7, 0);
    assert!(!actions.as_array().unwrap().iter().any(|a| a["title"].as_str().unwrap_or_default().contains("forest")));
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].clone();
    assert!(!lenses.as_array().unwrap().iter().any(|l| l["command"]["arguments"][2] == "forest"), "{lenses}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

const PICTURE: &str = "\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\begin{tikzpicture}[x=1cm]\n  \\node[draw] (a) at (0,0) {Start};\n  \\node[draw, circle] (b) at (3,1) {End};\n  \\draw[->] (a) -- (b);\n  \\foreach \\i in {1,2} { \\node (c\\i) at (\\i,-2) {\\i}; }\n\\end{tikzpicture}\n\n\\end{document}\n";

/// "Edit drawing…": a lens on the picture, a view with its body and the nodes the run made,
/// with where pgf put them (the loop's too); a blank line offers a new picture.
#[test]
fn tikz_plugin_shows_the_runs_nodes() {
    let (mut server, uri, dir) = open_set("tikz", PICTURE, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 4, 3);
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].clone();
    assert_eq!(lenses[0]["range"]["start"]["line"], 3, "{lenses}");
    assert_eq!(lenses[0]["command"]["arguments"][2], "tikz", "{lenses}");
    let views = at(&mut server, &uri, "satex/view", 5, 3);
    let draw = &views.as_array().unwrap().iter().find(|v| v["plugin"] == "tikz").expect("a drawing")["sections"][0];
    assert_eq!(draw["kind"], "drawing");
    let body = PICTURE.split_once("[x=1cm]").unwrap().1.split_once("\\end{tikzpicture}").unwrap().0;
    assert_eq!(draw["body"], body);
    let node =
        |name: &str| draw["nodes"].as_array().unwrap().iter().find(|n| n["name"] == name).cloned().unwrap_or_default();
    assert_eq!(
        (node("a")["shape"].as_str(), node("b")["shape"].as_str()),
        (Some("rectangle"), Some("circle")),
        "{draw}"
    );
    let near = |n: &Value, x: f64, y: f64| {
        (n["x"].as_f64().unwrap() - x).abs() < 0.01 && (n["y"].as_f64().unwrap() - y).abs() < 0.01
    };
    assert!(near(&node("b"), 3.0, 1.0) && near(&node("c2"), 2.0, -2.0), "{draw}");
    let actions = at(&mut server, &uri, "textDocument/codeAction", 9, 0);
    assert!(actions.as_array().unwrap().iter().any(|a| a["title"] == "Insert tikzpicture"), "{actions}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Nodes placed relative to others (`right of=`, `right=of`): the run's centers and sizes for
/// each, the picture's options for the canvas (`every node/.style`, `node distance`).
#[test]
fn tikz_plugin_places_relative_nodes() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{positioning}\n\\begin{document}\n\\begin{tikzpicture}[node distance=2.4cm, every node/.style={draw, rounded corners}]\n  \\node (tex) {\\TeX{} file};\n  \\node (satex) [right of=tex] {SaTeX};\n  \\node (out) [right=of satex] {findings};\n  \\draw[->] (tex) -- (satex);\n\\end{tikzpicture}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("tikz-rel", text, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 5, 3);
    let views = at(&mut server, &uri, "satex/view", 6, 3);
    let draw = &views.as_array().unwrap().iter().find(|v| v["plugin"] == "tikz").expect("a drawing")["sections"][0];
    assert_eq!(draw["options"], "node distance=2.4cm, every node/.style={draw, rounded corners}", "{draw}");
    let node = |name: &str| draw["nodes"].as_array().unwrap().iter().find(|n| n["name"] == name).cloned();
    let get = |name: &str| node(name).unwrap_or_else(|| panic!("{name}: {draw}"));
    let (tex, satex, out) = (get("tex"), get("satex"), get("out"));
    let x = |n: &Value| n["x"].as_f64().unwrap();
    let w = |n: &Value| n["w"].as_f64().unwrap();
    // `right of`: centers a node distance apart; `right=of`: borders a node distance apart
    assert!(x(&tex).abs() < 0.01 && (x(&satex) - 2.4).abs() < 0.01, "{draw}");
    assert!((x(&out) - w(&out) / 2.0 - x(&satex) - w(&satex) / 2.0 - 2.4).abs() < 0.01, "{draw}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// What the canvas offers comes from the run: the arrow tips declared, the shapes, the layers in
/// pgf's order, the keys that open a layer, the libraries loaded; and where the options stand.
#[test]
fn tikz_plugin_offers_what_the_run_declared() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{arrows.meta,backgrounds,shapes.geometric}\n\\pgfdeclarelayer{fore}\n\\pgfsetlayers{background,main,fore}\n\\begin{document}\n\\begin{tikzpicture}[node distance=2cm]\n  \\node[draw] (a) {A};\n  \\begin{pgfonlayer}{fore}\\node[right of=a] (b) {B};\\end{pgfonlayer}\n  \\draw[-Stealth] (a.north) -- (b);\n\\end{tikzpicture}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("tikz-facts", text, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 7, 3);
    let views = at(&mut server, &uri, "satex/view", 7, 3);
    let draw = &views.as_array().unwrap().iter().find(|v| v["plugin"] == "tikz").expect("a drawing")["sections"][0];
    let run = &draw["run"];
    let names = |v: &Value, key: &str| -> Vec<String> {
        v.as_array().unwrap().iter().map(|t| t[key].as_str().or(t.as_str()).unwrap_or_default().to_string()).collect()
    };
    let tips = names(&run["tips"], "name");
    assert!(["Stealth", "Latex", ">", "Circle"].iter().all(|t| tips.iter().any(|x| x == t)), "{tips:?}");
    let shapes = names(&run["shapes"], "");
    assert!(["rectangle", "circle", "diamond", "ellipse"].iter().all(|t| shapes.iter().any(|x| x == t)), "{shapes:?}");
    assert_eq!(run["layers"]["order"], json!(["background", "main", "fore"]), "{run}");
    let declared = names(&run["layers"]["declared"], "");
    assert!(declared.contains(&"fore".to_string()) && declared.contains(&"background".to_string()), "{declared:?}");
    assert_eq!(run["layer_keys"]["on background layer"], "background", "{run}");
    let libs = names(&run["libraries"], "");
    assert!(libs.contains(&"backgrounds".to_string()) && !libs.contains(&"matrix".to_string()), "{libs:?}");
    assert_eq!(run["load_at"]["line"], 5, "before \\begin{{document}}");
    assert!(run["colors"].as_array().unwrap().iter().any(|c| c["name"] == "red" && c["hex"] == "#ff0000"), "{run}");
    assert!(!run["colors"].as_array().unwrap().iter().any(|c| c["name"] == "."), "not the current color: {run}");
    // the options' place: the canvas rewrites `[node distance=2cm]`
    assert_eq!(draw["head"]["start"], json!({ "line": 6, "character": "\\begin{tikzpicture}".len() }));
    assert_eq!(draw["head"]["end"], draw["range"]["start"]);
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// The property editor's keys come from the run: each `/tikz/` key with the value type its
/// handler and code give, choices with their values; a style the document defines with its body
/// and where that body and its whole entry stand.
#[test]
fn tikz_plugin_offers_the_runs_keys_and_styles() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\tikzset{box/.style={draw, rounded corners}, hot/.style={fill=red!20}}\n\\tikzset{pace/.is choice, pace/fast/.code={}, pace/slow/.code={}}\n\\begin{document}\n\\begin{tikzpicture}\n  \\node[box] (a) {A};\n\\end{tikzpicture}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("tikz-keys", text, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 6, 3);
    let views = at(&mut server, &uri, "satex/view", 6, 3);
    let draw = &views.as_array().unwrap().iter().find(|v| v["plugin"] == "tikz").expect("a drawing")["sections"][0];
    let keys = draw["run"]["keys"].as_array().unwrap();
    let key = |name: &str| keys.iter().find(|k| k["name"] == name).unwrap_or_else(|| panic!("{name}: {keys:?}"));
    for (name, ty) in
        [("fill", "color"), ("line width", "length"), ("thick", "flag"), ("pace", "choice"), ("box", "flag")]
    {
        assert_eq!(key(name)["type"], ty, "{name}: {}", key(name));
    }
    assert_eq!(key("fill")["default"], "", "fill alone fills: {}", key("fill"));
    assert!(key("line width")["default"].is_null(), "line width needs a value: {}", key("line width"));
    assert_eq!(key("pace")["choices"], json!(["fast", "slow"]), "{}", key("pace"));
    assert!(!keys.iter().any(|k| k["name"].as_str().unwrap().contains(['@', '/'])), "no internals, no subkeys");
    assert_eq!(key("box")["body"], "draw, rounded corners");
    let box_ = &key("box")["style"];
    let at_line =
        |r: &Value| (r["start"]["line"].as_u64(), r["start"]["character"].as_u64(), r["end"]["character"].as_u64());
    let line = text.lines().nth(2).unwrap();
    let col = |s: &str| line.find(s).map(|c| c as u64);
    assert_eq!(at_line(&box_["range"]), (Some(2), col("draw"), col("}, hot")), "{box_}");
    // the entry takes its comma and the space after it along
    assert_eq!(at_line(&box_["entry"]), (Some(2), col("box"), col("hot")), "{box_}");
    assert_eq!(key("hot")["body"], "fill=red!20");
    assert_eq!(at_line(&key("hot")["style"]["entry"]), (Some(2), col(", hot"), col("}}").map(|c| c + 1)));
    assert!(key("thick")["style"].is_null(), "TikZ's own styles are no document styles");
    assert!(key("thick")["body"].as_str().unwrap().starts_with("line width"), "{}", key("thick"));
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// The canvas's matrix of nodes (`views.js` `toMatrix`): steps between origins from the anchor
/// node keep every node where it stood, as the run computes them.
#[test]
fn tikz_plugin_matrix_keeps_the_nodes_places() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{matrix}\n\\begin{document}\n\\begin{tikzpicture}[every node/.style={draw}]\n  \\matrix (m) [matrix of nodes, row sep={2cm,between origins}, column sep={3cm,between origins}, matrix anchor=a.center] at (1,2) {\n    |(a)| A & |[circle] (b)| Bee \\\\\n     & |(c)| C \\\\\n  };\n  \\draw[->] (a) -- (c);\n\\end{tikzpicture}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("tikz-matrix", text, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 5, 3);
    let views = at(&mut server, &uri, "satex/view", 5, 3);
    let draw = &views.as_array().unwrap().iter().find(|v| v["plugin"] == "tikz").expect("a drawing")["sections"][0];
    let node = |name: &str| {
        let n = draw["nodes"].as_array().unwrap().iter().find(|n| n["name"] == name).cloned();
        let n = n.unwrap_or_else(|| panic!("{name}: {draw}"));
        (n["x"].as_f64().unwrap(), n["y"].as_f64().unwrap())
    };
    let near = |(x, y): (f64, f64), (ex, ey): (f64, f64)| (x - ex).abs() < 0.01 && (y - ey).abs() < 0.01;
    // the anchor node at `at`, the others whole steps from it
    let (step_x, step_y) = (3.0, 2.0);
    let a = (1.0, 2.0);
    assert!(near(node("a"), a), "{draw}");
    assert!(near(node("b"), (a.0 + step_x, a.1)), "{draw}");
    assert!(near(node("c"), (a.0 + step_x, a.1 - step_y)), "{draw}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Without tikz the new picture loads it; `enabled: false` takes lens, view and action away.
#[test]
fn tikz_plugin_new_drawing_and_off() {
    let text = "\\documentclass{article}\n\\begin{document}\nText\n\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("tikz-new", text, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 2, 1);
    let actions = at(&mut server, &uri, "textDocument/codeAction", 3, 0);
    let new = actions.as_array().unwrap().iter().find(|a| a["title"] == "Insert tikzpicture (adds \\usepackage{tikz})");
    let edits = new.expect("a new picture")["edit"]["changes"][&uri].as_array().unwrap().clone();
    assert!(edits[0]["newText"].as_str().unwrap().starts_with("\\begin{tikzpicture}"), "{edits:?}");
    assert_eq!(
        (edits[1]["range"]["start"]["line"].as_u64(), edits[1]["newText"].as_str()),
        (Some(1), Some("\\usepackage{tikz}\n"))
    );
    assert_eq!(at(&mut server, &uri, "satex/view", 3, 0)[0]["title"], "New drawing");
    assert_eq!(at(&mut server, &uri, "satex/view", 2, 1), json!([]), "not on a line with text");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
    let (mut server, uri, dir) =
        open_set("tikz-off", PICTURE, &["type_plugins.tikz.enabled=false", "type_plugins.page.enabled=false"]);
    let _ = at(&mut server, &uri, "textDocument/hover", 4, 3);
    assert_eq!(at(&mut server, &uri, "satex/view", 5, 3), json!([]));
    let actions = at(&mut server, &uri, "textDocument/codeAction", 9, 0);
    assert!(
        !actions.as_array().unwrap().iter().any(|a| a["title"].as_str().unwrap_or_default().contains("tikzpicture"))
    );
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    assert_eq!(server.response(id)["result"], json!([]));
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// A primitive, or a name that stands for itself, has no "Expands to" line.
#[test]
fn a_primitive_never_expands_to_itself() {
    for (tag, text) in [
        ("par-plain", "\\par\n\\bye\n"),
        ("par-latex", "\\documentclass{article}\n\\begin{document}\n\\par\n\\end{document}\n"),
    ] {
        let line = text.lines().position(|l| l == "\\par").unwrap() as u32;
        let (mut server, uri, _, dir) = open_unsaved(tag, text);
        let id = server.request(
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 2 } }),
        );
        let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
        assert!(!hover.contains("Expands to `\\par`"), "{tag}: {hover}");
        server.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Applies the WorkspaceEdit `edit` (with `${name}` templates filled from `vars`) to `text`,
/// the file `uri`: what the view renderer does.
fn applied(text: &str, uri: &str, edit: &Value, vars: &[(&str, &str)]) -> String {
    let mut edits: Vec<(usize, usize, String)> = edit["changes"][uri]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|e| {
            let off = |p: &Value| {
                let line = p["line"].as_u64().unwrap() as usize;
                let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
                start + p["character"].as_u64().unwrap() as usize
            };
            let mut t = e["newText"].as_str().unwrap().to_string();
            // `${?k}…${/k}`: kept when `k` has a value, as the client fills it
            while let Some(a) = t.find("${?") {
                let k = t[a + 3..].split('}').next().unwrap().to_string();
                let close = format!("${{/{k}}}");
                let b = t.find(&close).unwrap();
                let filled = vars.iter().any(|(n, v)| *n == k && !v.trim().is_empty());
                let body = if filled { t[a + k.len() + 4..b].to_string() } else { String::new() };
                t.replace_range(a..b + close.len(), &body);
            }
            for (k, v) in vars {
                t = t.replace(&format!("${{{k}}}"), v);
            }
            (off(&e["range"]["start"]), off(&e["range"]["end"]), t)
        })
        .collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_string();
    for (a, b, t) in edits {
        out.replace_range(a..b, &t);
    }
    out
}

/// The manager `plugin` opens without a position, as a sidebar asks.
fn manager(server: &mut Server, uri: &str, plugin: &str) -> Value {
    let id = server.request("satex/view", json!({ "textDocument": { "uri": uri }, "plugin": plugin }));
    server.response(id)["result"][0].clone()
}

fn record<'a>(view: &'a Value, title: &str) -> &'a Value {
    view["sections"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["title"] == title)
        .unwrap_or_else(|| panic!("{title}: {view}"))
}

fn field_of<'a>(record: &'a Value, name: &str) -> &'a Value {
    record["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap_or_else(|| panic!("{name}: {record}"))
}

const GLOSSARY: &str = "\\documentclass{article}\n\\usepackage{glossaries}\n\\newglossaryentry{graph}{name={graph},description={a set of nodes and edges},plural={graphs}}\n\\newglossaryentry{tree}{name=tree,description={a connected acyclic graph}}\n\\newacronym{cfg}{CFG}{control flow graph}\n\\begin{document}\nA \\gls{graph} and \\glspl{graph}, a \\gls{cfg}.\n\\end{document}\n";

/// The glossary manager: every declared key with its fields, uses and the run's stored forms;
/// editing a field writes the call, a new entry is made like the document's, deleting refuses
/// while used and replacing the uses first moves them.
#[test]
fn glossary_manager_round_trips() {
    let (mut server, uri, dir) = open_set("glossary", GLOSSARY, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 6, 9);
    let view = manager(&mut server, &uri, "glossary");
    let graph = record(&view, "graph");
    assert_eq!(graph["badge"], Value::Null, "{graph}");
    assert_eq!(record(&view, "tree")["badge"], "unused");
    // one line: name, short description; the uses as places, no stored internals
    assert_eq!(graph["detail"], "graph · a set of nodes and edges", "{graph}");
    let uses = graph["uses"].as_str().unwrap_or_default();
    assert!(
        uses.contains("main.tex:3`") && uses.contains("Used at `") && uses.matches("main.tex:7`").count() == 1,
        "one place per line: {uses}"
    );
    assert!(graph["table"].is_null() && !graph.to_string().contains("@gls@"), "{graph}");
    let cfg = &record(&view, "cfg")["detail"];
    assert!(
        cfg.as_str().is_some_and(|d| d.starts_with("CFG · control flow graph") && !d.contains("main")),
        "the default type unsaid: {cfg}"
    );
    // a field: the description, written back in braces
    let desc = field_of(graph, "description");
    assert_eq!(desc["value"], "a set of nodes and edges");
    let out = applied(GLOSSARY, &uri, &desc["edit"], &[("value", "nodes, edges")]);
    assert!(out.contains("description={nodes, edges},plural"), "{out}");
    // the acronym's groups are named by the fields the run stored their text in
    let cfg = record(&view, "cfg");
    let names: Vec<&str> = cfg["fields"].as_array().unwrap().iter().filter_map(|f| f["name"].as_str()).collect();
    assert!(names.contains(&"short") && names.contains(&"long"), "{names:?}");
    // delete: refused while used, allowed when unused
    let used = &graph["actions"][0];
    assert!(used["disabled"].as_str().is_some_and(|r| r.contains("Used 2 times")), "{used}");
    let tree = record(&view, "tree");
    assert_eq!(tree["actions"][0]["disabled"], Value::Null);
    let out = applied(GLOSSARY, &uri, &tree["actions"][0]["edit"], &[]);
    assert!(!out.contains("{tree}") && out.contains("{cfg}"), "{out}");
    // replace the uses of graph by tree, then delete graph
    let replace = &graph["forms"][0];
    let out = applied(GLOSSARY, &uri, &replace["edit"], &[("key", "tree")]);
    assert!(out.contains("A \\gls{tree} and \\glspl{tree}") && !out.contains("{graph}"), "{out}");
    // add: a call like the document's own
    let kinds = view["sections"][0]["add"]["kinds"].as_array().unwrap();
    let new = kinds.iter().find(|k| k["name"] == "\\newglossaryentry").expect("newglossaryentry kind");
    let vars = [("key", "dag"), ("name", "DAG"), ("description", "a directed acyclic graph"), ("plural", "DAGs")];
    let out = applied(GLOSSARY, &uri, &new["edit"], &vars);
    assert!(out.contains("\\newglossaryentry{dag}{name={DAG},description={a directed acyclic graph},plural={DAGs}}\n\\newglossaryentry{tree}") || out.contains("{dag}{name={DAG}"), "{out}");
    // every key of the family the run defined: a field to add, an optional one to remove
    let names = graph["more"]["names"].to_string();
    assert!(names.contains("\"symbol\"") && names.contains("\"first\"") && !names.contains("\"plural\""), "{names}");
    let out = applied(GLOSSARY, &uri, &graph["more"]["edit"], &[("name", "symbol"), ("value", "G")]);
    assert!(out.contains("plural={graphs},symbol={G}}"), "{out}");
    let out = applied(GLOSSARY, &uri, &field_of(graph, "plural")["remove"], &[]);
    assert!(out.contains("description={a set of nodes and edges}}\n"), "{out}");
    assert!(field_of(graph, "name")["remove"].is_null(), "every entry writes a name: {graph}");
    // a new entry writes only the fields given
    let short = [("key", "dag"), ("name", "DAG"), ("description", "d")];
    let out = applied(GLOSSARY, &uri, &new["edit"], &short);
    assert!(out.contains("\\newglossaryentry{dag}{name={DAG},description={d}}\n"), "{out}");
    let symbol = new["fields"].as_array().unwrap().iter().find(|f| f["name"] == "symbol").expect("symbol from the run");
    assert_eq!(symbol["more"], true, "{symbol}");
    // the view at a use focuses that key
    let views = at(&mut server, &uri, "satex/view", 6, 9);
    let mine = views.as_array().unwrap().iter().find(|v| v["plugin"] == "glossary").expect("glossary view at \\gls");
    assert_eq!(mine["sections"][0]["focus"], "graph");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn managers_switch_off() {
    for plugin in ["glossary", "theorem", "bibliography", "index"] {
        let set = format!("type_plugins.{plugin}.enabled=false");
        let (mut server, uri, dir) = open_set(&format!("off-{plugin}"), GLOSSARY, &[&set]);
        assert_eq!(manager(&mut server, &uri, plugin), Value::Null, "{plugin}");
        server.shutdown();
        std::fs::remove_dir_all(dir).ok();
    }
    let (mut server, uri, dir) = open_set("off-savebox", SAVEBOX, &["type_plugins.savebox.enabled=false"]);
    let actions = at(&mut server, &uri, "textDocument/codeAction", 4, 9);
    assert!(!actions.to_string().contains("in a box"), "{actions}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

const REFS: &str = "@book{knuth,\n  author = {Donald E. Knuth},\n  title = {The {\\TeX}book},\n  year = 1984,\n  publisher = {Addison-Wesley}\n}\n\n@article{lamport,\n  author = {Leslie Lamport},\n  title = {Time, Clocks},\n  journal = {CACM},\n  year = {1978}\n}\n";
const CITES: &str = "\\documentclass{article}\n\\usepackage{makeidx}\n\\makeindex\n\\begin{document}\nText \\cite{knuth} \\index{graph} \\index{graph!directed} \\index{tree}.\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";

/// The bibliography manager: the entries of the database the run read, cited where; a field
/// edit writes the `.bib` file, an uncited entry deletes, a cited one refuses, a new entry has
/// the fields the style reads of its type. The index manager: terms with subentries, rename.
#[test]
fn bibliography_and_index_managers_round_trip() {
    let (mut server, uri, dir) = open_with("bibmgr", CITES, &[("refs.bib", REFS)], &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 4, 12);
    let view = manager(&mut server, &uri, "bibliography");
    let knuth = record(&view, "knuth");
    assert!(knuth["uses"].as_str().is_some_and(|u| u.contains("main.tex:5`")), "{knuth}");
    assert!(knuth["actions"][0]["disabled"].is_string(), "{knuth}");
    let bib_uri = knuth["fields"][0]["edit"]["changes"].as_object().unwrap().keys().next().unwrap().clone();
    assert!(bib_uri.ends_with("refs.bib"), "{bib_uri}");
    let year = field_of(knuth, "year");
    assert_eq!(year["value"], "1984");
    let out = applied(REFS, &bib_uri, &year["edit"], &[("value", "1986")]);
    assert!(out.contains("year = 1986,"), "a bare number stays bare: {out}");
    // an emptied field goes with its line, a new one follows the last
    let out = applied(REFS, &bib_uri, &field_of(knuth, "year")["remove"], &[]);
    assert!(out.contains("{The {\\TeX}book},\n  publisher"), "{out}");
    let out = applied(REFS, &bib_uri, &knuth["more"]["edit"], &[("name", "address"), ("value", "Reading")]);
    assert!(out.contains("publisher = {Addison-Wesley},\n  address = {Reading}\n}"), "{out}");
    assert!(knuth["more"]["names"].to_string().contains("\"edition\""), "the style's book fields: {}", knuth["more"]);
    let lamport = record(&view, "lamport");
    assert_eq!(lamport["badge"], "uncited");
    let out = applied(REFS, &bib_uri, &lamport["actions"][0]["edit"], &[]);
    assert!(!out.contains("lamport") && out.contains("knuth"), "{out}");
    let kinds = view["sections"][0]["add"]["kinds"].as_array().unwrap();
    let article = kinds.iter().find(|k| k["name"] == "article").expect("article from plain.bst");
    let fields: Vec<&str> = article["fields"].as_array().unwrap().iter().filter_map(|f| f["name"].as_str()).collect();
    assert!(fields.contains(&"journal") && fields.contains(&"author"), "{fields:?}");
    // an empty field is not written
    let out = applied(REFS, &bib_uri, &article["edit"], &[("citekey", "new"), ("author", "A"), ("title", "T")]);
    assert!(out.ends_with("@article{new,\n  author = {A},\n  title = {T}\n}\n"), "{out}");
    let index = manager(&mut server, &uri, "index");
    let graph = record(&index, "graph");
    assert!(graph["table"]["rows"].to_string().contains("directed"), "{graph}");
    let out = applied(CITES, &uri, &graph["forms"][0]["edit"], &[("entry", "network"), ("sort", "")]);
    assert!(out.contains("\\index{network} \\index{network!directed} \\index{tree}"), "{out}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

const INDEX: &str = "\\documentclass{article}\n\\usepackage{makeidx}\n\\makeindex\n\\begin{document}\nA\\index{alpha!beta} B\\index{x@$x$|seealso{alpha}} C\\index{eps|see{alpha}}\nD\\index{Beta@\\textbf{Beta}} E\\index{zeta|(} F\\index{zeta|)} G\\index{delta|textbf}\n\\printindex\n\\end{document}\n";

/// Every index entry path is a record with its levels, sort key, page format and cross
/// reference as fields: subentries, `@` keys and `|see` edit too, an emptied format goes
/// with its `|`, a range keeps its parentheses; deleting removes the calls.
#[test]
fn index_manager_edits_every_part() {
    let (mut server, uri, dir) = open_set("idxmgr", INDEX, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 4, 3);
    let view = manager(&mut server, &uri, "index");
    let titles: Vec<&str> =
        view["sections"][0]["items"].as_array().unwrap().iter().filter_map(|r| r["title"].as_str()).collect();
    assert!(
        titles.contains(&"alpha › beta") && titles.contains(&"\\textbf{Beta}") && titles.contains(&"$x$"),
        "{titles:?}"
    );
    let edit =
        |title: &str, vars: &[(&str, &str)]| applied(INDEX, &uri, &record(&view, title)["forms"][0]["edit"], vars);
    // a subentry: its own level only
    let out = edit("alpha › beta", &[("entry", "gamma"), ("sort", "")]);
    assert!(out.contains("\\index{alpha!gamma}"), "{out}");
    // a parent renames its subentries' level too
    let out = edit("alpha", &[("entry", "omega"), ("sort", "")]);
    assert!(out.contains("\\index{omega!beta}"), "{out}");
    // a sort key: kept, changed, dropped
    let beta = record(&view, "\\textbf{Beta}");
    assert_eq!(field_of(&beta["forms"][0], "sort")["value"], "Beta");
    let out = edit("\\textbf{Beta}", &[("entry", "\\textbf{Beta}"), ("sort", "")]);
    assert!(out.contains("\\index{\\textbf{Beta}}"), "{out}");
    // cross references
    let out = edit("$x$", &[("entry", "$x$"), ("sort", "x"), ("seealso", "beta")]);
    assert!(out.contains("\\index{x@$x$|seealso{beta}}"), "{out}");
    let out = edit("eps", &[("entry", "eps"), ("sort", ""), ("see", "omega")]);
    assert!(out.contains("\\index{eps|see{omega}}"), "{out}");
    // page formats: a range keeps `(` and `)`, an emptied format drops its `|`
    let out = edit("zeta", &[("entry", "zeta"), ("sort", ""), ("format", "textit")]);
    assert!(out.contains("\\index{zeta|(textit} F\\index{zeta|)}"), "{out}");
    let out = edit("delta", &[("entry", "delta"), ("sort", ""), ("format", "")]);
    assert!(out.contains("G\\index{delta}\n"), "{out}");
    let out = applied(INDEX, &uri, &record(&view, "zeta")["actions"][0]["edit"], &[]);
    assert!(out.contains("E F G"), "{out}");
    // a new call is written at the cursor: a template the client fills
    let page = &view["sections"][0]["add"]["kinds"][0]["insert"];
    assert_eq!(
        page,
        "\\index{${?sort}${sort}@${/sort}${entry}${?subentry}!${subentry}${/subentry}${?format}|${format}${/format}}"
    );
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

const BIBITEMS: &str = "\\documentclass{article}\n\\begin{document}\nSee \\cite{knuth84} and \\cite{lamport94,knuth84}.\n\\begin{thebibliography}{9}\n\\bibitem{knuth84} D.~E. Knuth.\n\\newblock Literate programming.\n\\newblock {\\em The Computer Journal}, 27(2):97--111, 1984.\n\\bibitem[Lam94]{lamport94} Leslie Lamport. {\\LaTeX}: A Document Preparation System. Addison-Wesley, 1994.\n\\bibitem{old} U. Nused. Text.\n\\end{thebibliography}\n\\end{document}\n";

/// Without a `.bib` file the manager lists `thebibliography`'s items: label, text and key edit
/// in place (the key in its `\\cite`s too), a new item goes before `\\end`, and converting writes a
/// database of guessed `@misc` entries and `\\bibliography` in the list's place.
#[test]
fn bibliography_manager_reads_thebibliography() {
    let (mut server, uri, dir) = open_set("bibitems", BIBITEMS, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 2, 6);
    let view = manager(&mut server, &uri, "bibliography");
    let knuth = record(&view, "knuth84");
    assert!(knuth["uses"].as_str().is_some_and(|u| u.contains("main.tex:3`")), "{knuth}");
    assert!(
        knuth["actions"][0]["disabled"].is_string() && record(&view, "old")["actions"][0]["disabled"].is_null(),
        "{view}"
    );
    let out = applied(BIBITEMS, &uri, &field_of(knuth, "key")["edit"], &[("value", "knuth")]);
    assert!(out.contains("\\cite{knuth} and \\cite{lamport94,knuth}") && out.contains("\\bibitem{knuth} D."), "{out}");
    let lamport = record(&view, "lamport94");
    assert_eq!(field_of(lamport, "label")["value"], "Lam94");
    let out = applied(BIBITEMS, &uri, &field_of(lamport, "label")["edit"], &[("value", "")]);
    assert!(out.contains("\\bibitem{lamport94} Leslie"), "{out}");
    let out = applied(BIBITEMS, &uri, &field_of(knuth, "label")["edit"], &[("value", "K84")]);
    assert!(out.contains("\\bibitem[K84]{knuth84} D."), "{out}");
    let out = applied(BIBITEMS, &uri, &field_of(lamport, "text")["edit"], &[("value", "L. Lamport. LaTeX. 1994.")]);
    assert!(out.contains("{lamport94} L. Lamport. LaTeX. 1994.\n\\bibitem{old}"), "{out}");
    let kinds = view["sections"][0]["add"]["kinds"].as_array().unwrap();
    let item = kinds.iter().find(|k| k["name"] == "\\bibitem").expect("a \\bibitem kind");
    let out = applied(
        BIBITEMS,
        &uri,
        &item["edit"],
        &[("citekey", "new"), ("label", ""), ("text", "A. Author. A title. 2001.")],
    );
    assert!(out.contains("Text.\n\\bibitem{new} A. Author. A title. 2001.\n\\end{thebibliography}"), "{out}");
    // convert: a database and \bibliography in the list's place
    let action = &view["sections"][2]["items"][0];
    assert_eq!(action["title"], "Convert to a .bib file");
    let changes = action["edits"]["documentChanges"].as_array().unwrap();
    assert_eq!(changes[0]["kind"], "create");
    let bib = changes[1]["edits"][0]["newText"].as_str().unwrap();
    assert!(
        bib.contains("@misc{knuth84,\n  author       = {D.~E. Knuth},\n  title        = {Literate programming},")
            && bib.contains("year         = {1984}"),
        "{bib}"
    );
    assert!(bib.contains("@misc{lamport94,\n  author       = {Leslie Lamport},\n  title        = {{\\LaTeX}: A Document Preparation System},"), "{bib}");
    assert!(bib.contains("@misc{old,\n  author = {U. Nused},\n  title  = {Text},\n}"), "{bib}");
    let tex_uri = changes[2]["textDocument"]["uri"].as_str().unwrap();
    let edit = json!({ "changes": { tex_uri: changes[2]["edits"] } });
    let out = applied(BIBITEMS, tex_uri, &edit, &[]);
    assert!(out.contains("knuth84}.\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}"), "{out}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

const THEOREMS: &str = "\\documentclass{article}\n\\usepackage{amsthm}\n\\newtheorem{theorem}{Theorem}[section]\n\\newtheorem{lemma}[theorem]{Lemma}\n\\theoremstyle{definition}\n\\newtheorem{definition}{Definition}\n\\begin{document}\n\\section{A}\n\\begin{theorem}\\label{thm:a}\nX.\n\\end{theorem}\n\\begin{lemma}\nY.\n\\end{lemma}\nSee \\ref{thm:a}.\n\\end{document}\n";

/// The theorem manager: the kinds the run defined with a number, their counters, instances
/// with labels and references; renaming rewrites the definition, both ends and the kinds
/// sharing the counter; numbering writes the group the document's own calls use.
#[test]
fn theorem_manager_round_trips() {
    let (mut server, uri, dir) = open_set("thmmgr", THEOREMS, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 8, 9);
    let view = manager(&mut server, &uri, "theorem");
    let theorem = record(&view, "theorem");
    assert!(theorem["detail"].as_str().unwrap().contains("within section"), "{theorem}");
    assert!(record(&view, "lemma")["detail"].as_str().unwrap().contains("shares theorem"), "{view}");
    assert_eq!(record(&view, "definition")["badge"], "unused");
    let rows = theorem["table"]["rows"].to_string();
    assert!(rows.contains("thm:a") && rows.contains("15"), "{rows}");
    let out = applied(THEOREMS, &uri, &field_of(theorem, "environment")["edit"], &[("value", "thm")]);
    assert!(out.contains("\\newtheorem{thm}{Theorem}[section]\n\\newtheorem{lemma}[thm]{Lemma}"), "{out}");
    assert!(out.contains("\\begin{thm}\\label{thm:a}\nX.\n\\end{thm}"), "{out}");
    // definition: numbered within section, in the slot \newtheorem's calls use
    let numbering = |env: &str, choice: &str| {
        let r = record(&view, env);
        let f = r["forms"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["title"] == "Set numbering")
            .unwrap_or_else(|| panic!("{r}"));
        applied(THEOREMS, &uri, &f["edits"][choice], &[])
    };
    assert!(numbering("definition", "within section").contains("\\newtheorem{definition}{Definition}[section]"));
    // one numbering at a time: the other slot is cleared, a later kind is no counter to share
    assert!(numbering("lemma", "within section").contains("\\newtheorem{lemma}{Lemma}[section]\n"));
    assert!(numbering("theorem", "own counter").contains("\\newtheorem{theorem}{Theorem}\n"));
    let form = &theorem["forms"][0];
    assert_eq!(form["fields"][0]["value"], "within section", "{form}");
    let choices = form["fields"][0]["choices"].to_string();
    assert!(!choices.contains("shares definition") && !choices.contains('@'), "{choices}");
    assert!(record(&view, "definition")["forms"][0]["fields"][0]["choices"].to_string().contains("shares theorem"));
    // instances are places to open
    assert!(theorem["uses"].as_str().is_some_and(|u| u.contains("main.tex:9` (thm:a)")), "{theorem}");
    // a new kind: like the document's call, numbered as chosen
    let new = &view["sections"][0]["add"]["kinds"][0];
    let vars = [("env", "prop"), ("title", "Proposition")];
    let out = applied(THEOREMS, &uri, &new["edits"]["shares theorem"], &vars);
    assert!(out.contains("{Definition}\n\\newtheorem{prop}[theorem]{Proposition}\n\\begin{document}"), "{out}");
    let out = applied(THEOREMS, &uri, &new["edits"]["within section"], &vars);
    assert!(out.contains("\\newtheorem{prop}{Proposition}[section]\n"), "{out}");
    let out = applied(THEOREMS, &uri, &new["edits"]["own counter"], &vars);
    assert!(out.contains("\\newtheorem{prop}{Proposition}\n"), "{out}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

const SAVEBOX: &str = "\\documentclass{article}\n\\usepackage{tikz}\n\\usepackage{listings}\n\\begin{document}\n\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\\begin{lstlisting}\nx = {a % b}\n\\end{lstlisting}\n\\begin{tikzpicture}\n\\node {\\thepage};\n\\end{tikzpicture}\n\\begin{minipage}{2cm}Hi\\end{minipage}\n\\end{document}\n";

/// "Store in a box": a picture moves to the preamble in an `lrbox` (tikz changes catcodes),
/// identical copies share one box, a listing gets a minipage, a picture reading `\thepage` is
/// refused, and a box of plain text goes through `\sbox`.
#[test]
fn savebox_stores_and_refuses() {
    let (mut server, uri, dir) = open_set("savebox", SAVEBOX, &[]);
    let _ = at(&mut server, &uri, "textDocument/hover", 4, 9);
    let actions = at(&mut server, &uri, "textDocument/codeAction", 4, 9);
    let find = |title: &str| {
        actions.as_array().unwrap().iter().find(|a| a["title"].as_str().is_some_and(|t| t.starts_with(title))).cloned()
    };
    let one = find("Store tikzpicture in a box").unwrap_or_else(|| panic!("{actions}"));
    let out = applied(SAVEBOX, &uri, &one["edit"], &[]);
    // tikz makes `&` a character at its `\\begin`: the body is read as the environment reads it
    assert!(
        out.contains("\\newsavebox{\\tikzpicturebox}\n\\begin{lrbox}{\\tikzpicturebox}\n\\begin{tikzpicture}"),
        "{out}"
    );
    assert!(out.contains("\\begin{document}\n\\usebox{\\tikzpicturebox}\n\\begin{tikzpicture}"), "{out}");
    let both = find("Store 2 identical tikzpicture").unwrap_or_else(|| panic!("{actions}"));
    let out = applied(SAVEBOX, &uri, &both["edit"], &[]);
    assert_eq!(out.matches("\\usebox{\\tikzpicturebox}").count(), 2, "{out}");
    let hover = hover_md(&at(&mut server, &uri, "textDocument/hover", 4, 9));
    assert!(hover.contains("Storable in a box"), "{hover}");
    // the listing: verbatim, so through lrbox
    let listing = at(&mut server, &uri, "textDocument/codeAction", 10, 9);
    let store = listing
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str().is_some_and(|t| t.contains("in a box")))
        .cloned();
    let out = applied(SAVEBOX, &uri, &store.unwrap_or_else(|| panic!("{listing}"))["edit"], &[]);
    assert!(out.contains("\\begin{lrbox}{\\lstlistingbox}\n\\begin{minipage}"), "{out}");
    // \thepage reads where it stands
    let page = hover_md(&at(&mut server, &uri, "textDocument/hover", 13, 9));
    assert!(page.contains("Not storable") && page.contains("thepage"), "{page}");
    let none = at(&mut server, &uri, "textDocument/codeAction", 13, 9);
    assert!(!none.to_string().contains("in a box"), "{none}");
    let plain = at(&mut server, &uri, "textDocument/codeAction", 16, 9);
    let store =
        plain.as_array().unwrap().iter().find(|a| a["title"].as_str().is_some_and(|t| t.contains("in a box"))).cloned();
    let out = applied(SAVEBOX, &uri, &store.unwrap_or_else(|| panic!("{plain}"))["edit"], &[]);
    assert!(out.contains("\\sbox{\\minipagebox}{%\n\\begin{minipage}{2cm}Hi\\end{minipage}%\n}"), "{out}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

// --- incremental plugins (src/lsp/plugins/incremental.rs) ---

/// INITEX with braces and parameters, a definition, a used name and an
/// undefined one: each case edits one place of it.
const INC: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\def\\greet#1{Hello, #1!}\\def\\nopes{}\n\\greet{world}  x % a note\n% a comment line\n  \\greet{you}\n\\nope\n{\\catcode`\\%=12 y % not a comment}\n\\end\n";

/// The diagnostics after editing `INC` into `new`, the reuse lines of
/// `satex/caches`, and the diagnostics a server that opens `new` publishes.
fn edit_inc(tag: &str, new: &str) -> (Value, Vec<String>, Value) {
    let (mut server, uri, _, dir) = open_unsaved(tag, INC);
    server.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": new }] }),
    );
    let after = loop {
        let p = server.notification("textDocument/publishDiagnostics");
        if p["params"]["version"] == 2 {
            break p["params"]["diagnostics"].clone();
        }
    };
    let id = server.request("satex/caches", json!({ "textDocument": { "uri": uri } }));
    let lines = server.response(id)["result"].as_array().cloned().unwrap_or_default();
    let reused = lines.iter().filter_map(Value::as_str).filter(|l| l.starts_with("reused ")).map(String::from);
    let reused = reused.collect();
    server.shutdown();
    let (server, _, fresh, dir2) = open_unsaved(tag, new);
    server.shutdown();
    assert_eq!(dir, dir2);
    let _ = std::fs::remove_dir_all(&dir);
    (after, reused, fresh["params"]["diagnostics"].clone())
}

/// Each edit a plugin takes keeps the run, and the results equal a fresh run's.
#[test]
fn incremental_edits_keep_the_run_and_equal_a_fresh_one() {
    let cases = [
        ("comment", INC.replace("% a comment line", "% a longer comment line"), "comment_edit"),
        ("trailing-comment", INC.replace("% a note", "% a notebook"), "comment_edit"),
        ("skipped-space", INC.replace("}  x", "}   x"), "whitespace_edit"),
        ("indent", INC.replace("  \\greet{you}", "    \\greet{you}"), "whitespace_edit"),
    ];
    for (tag, new, plugin) in cases {
        let (after, reused, fresh) = edit_inc(&format!("inc-{tag}"), &new);
        assert_eq!(reused, [format!("reused {plugin} 1")], "{tag}");
        assert_eq!(after, fresh, "{tag}: the kept run's results differ from a fresh run's");
    }
}

/// Edits no plugin may take run again: a `%` of category 12, a new blank
/// line, a directive, a name typed into a defined one.
#[test]
fn incremental_edits_that_must_run_again() {
    let cases = [
        ("catcode", INC.replace("y % not a comment", "y % not a comment!")),
        // the run read the line of `\end` to its end: the new lines may be read
        ("tail", format!("{INC}\n\n")),
        ("blank-line", INC.replace("\\greet{world}  x", "\\greet{world}\n\n x")),
        ("directive", INC.replace("% a comment line", "%\\satex@assert{1 < 2}")),
        ("becomes-defined", INC.replace("\\nope\n", "\\nopes\n")),
        ("defined-name", INC.replace("\\nope", "\\greet")),
    ];
    for (tag, new) in cases {
        let (after, reused, fresh) = edit_inc(&format!("inc-no-{tag}"), &new);
        assert!(reused.is_empty(), "{tag}: {reused:?}");
        assert_eq!(after, fresh, "{tag}");
    }
}

/// Letters typed into a name the run met undefined: the results stand,
/// provisional, and the run follows once typing pauses, as a fresh one.
#[test]
fn incremental_typing_an_undefined_name_waits_for_the_pause() {
    let new = INC.replace("\\nope\n", "\\nopex\n");
    let (mut server, uri, _, dir) = open_unsaved("inc-typing", INC);
    server.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": new }] }),
    );
    let mut published = Vec::new();
    while published.is_empty() {
        let p = server.notification("textDocument/publishDiagnostics");
        if p["params"]["version"] == 2 {
            published.push(p["params"]["diagnostics"].clone());
        }
    }
    let id = server.request("satex/caches", json!({ "textDocument": { "uri": uri } }));
    let lines = server.response(id)["result"].clone();
    assert!(lines.as_array().unwrap().iter().any(|l| l == "reused typing 1"), "{lines}");
    while published.len() < 2 {
        let p = server.notification("textDocument/publishDiagnostics");
        if p["params"]["version"] == 2 {
            published.push(p["params"]["diagnostics"].clone());
        }
    }
    server.shutdown();
    let (server, _, fresh, _) = open_unsaved("inc-typing", &new);
    server.shutdown();
    assert_eq!(published[1], fresh["params"]["diagnostics"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `-f main.tex` names the root the editor opens as an absolute URI: one root, one set of
/// checkpoints, so a body edit after an input file's first run resumes past the preamble
/// (the playground passes `-f` and opened the input first: every edit ran the preamble).
#[test]
fn a_root_named_relative_resumes_after_a_body_edit() {
    let dir =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-lsp-root-{}", std::process::id())));
    let main = "\\documentclass{article}\n\\input{part}\n\\begin{document}\nOne.\n\nTwo.\n\\end{document}\n";
    std::fs::write(dir.join("main.tex"), main).unwrap();
    std::fs::write(dir.join("part.tex"), "\\def\\x{x}\n").unwrap();
    let mut server = Server::start_with(
        &crate::support::test_cache(),
        &dir,
        &["-vv", "-f", "main.tex", "--set", "limits.seconds=0"],
    );
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    open_file(&mut server, &dir, "part.tex");
    let (uri, _) = open_file(&mut server, &dir, "main.tex");
    let edited = main.replace("Two.", "Two more.");
    server.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": edited }] }),
    );
    server.notification("textDocument/publishDiagnostics");
    let said = server.shutdown();
    // the edit's run: `resuming at` precedes its `resume …, run …` line
    let runs: Vec<&str> = said.lines().filter(|l| l.starts_with("satex: resum")).collect();
    assert!(
        runs.len() >= 2 && runs[runs.len() - 2].starts_with("satex: resuming at"),
        "the edit ran from the start: {runs:?}"
    );
}

#[test]
fn hover_shows_the_redefinition_chain() {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-chain-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("chain.tex");
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \n\\def\\a{1}\n\\def\\a{2}\n\\a\n";
    std::fs::write(&file, source).unwrap();
    let uri = fixture_uri(&file);
    let mut server = Server::start(&dir);
    let init_id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(init_id);
    server.notify("initialized", json!({}));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": source } }),
    );
    server.notification("textDocument/publishDiagnostics");
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 3, "character": 1 } }),
    );
    let hover = server.response(id);
    let text = hover["result"]["contents"]["value"].as_str().unwrap_or_default();
    assert!(
        text.contains("Redefined by: `chain.tex:2` (document, redefined) → `chain.tex:3` (document, in force here)"),
        "{text}"
    );
}

/// A run under way when a newer text comes stops: only the newer text's
/// findings are published, never the stopped run's.
#[test]
fn a_run_stops_for_a_newer_text_and_publishes_only_its_findings() {
    let dir =
        crate::support::scratch_dir(crate::support::tmp_root().join(format!("satex-lsp-stop-{}", std::process::id())));
    let body = |tail: &str| {
        format!("\\documentclass{{article}}\n\\newcount\\n\n\\begin{{document}}\n{tail}\n\\end{{document}}\n")
    };
    let slow = body("\\loop\\advance\\n1 \\ifnum\\n<3000000 \\repeat\n\\undefinedfirst");
    std::fs::write(dir.join("main.tex"), &slow).unwrap();
    let mut server = Server::start_with(&crate::support::test_cache(), &dir, &["--set", "limits.seconds=0"]);
    let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    server.response(id);
    server.notify("initialized", json!({}));
    let uri = fixture_uri(&dir.join("main.tex"));
    server.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": slow } }),
    );
    // the run is under way
    server.notification("satex/status");
    server.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": body("\\undefinedsecond") }] }),
    );
    let published = server.notification("textDocument/publishDiagnostics");
    let said = published["params"].to_string();
    assert_eq!(published["params"]["version"], 2, "{said}");
    assert!(said.contains("undefinedsecond") && !said.contains("undefinedfirst"), "{said}");
    server.shutdown();
}

/// Slides: two frames, the second labelled; overlays absolute, nested, incremental and trailing.
const SLIDES: &str = "\\documentclass{beamer}\n\\usetheme{default}\n\\begin{document}\n\\begin{frame}{Steps}\n  \\only<2>{Two.}\n  \\uncover<3>{Three.}\n  \\only<2-3>{Outer \\uncover<3>{inner}}\n  \\alt<2>{Alt}{Main}\n  \\only{Trail}<1>\n\\end{frame}\n\\begin{frame}[t,label=inc]{Inc}\n  \\begin{itemize}\n    \\item<+-> Ant\n    \\item<+-> Bee\n  \\end{itemize}\n\\end{frame}\n\\begin{frame}[t]{Opt}\n  Body\n\\end{frame}\n\\end{document}\n";

fn hover_at(server: &mut Server, uri: &str, text: &str, line: usize, needle: &str, off: usize) -> String {
    hover_md(&at(server, uri, "textDocument/hover", line as u32, col(text, line, needle, off)))
}

/// The hover of `\only`, `\uncover`, `\onslide`, `\visible` and `\alt` shows the overlay
/// specification `<…>` and the bodies.
#[test]
fn beamer_signatures_show_the_overlay_and_bodies() {
    // the usage lines come from beamer's manual: an installation without documentation has none
    let dist = std::process::Command::new("kpsewhich").args(["-var-value", "TEXMFDIST"]).output();
    let manual = dist.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    if !manual.is_some_and(|d| std::path::Path::new(&d).join("doc/latex/beamer").is_dir()) {
        return;
    }
    let (mut server, uri, dir) = open_set("bmrsig", SLIDES, &[]);
    for (line, cmd) in [(4, "\\only"), (5, "\\uncover"), (7, "\\alt")] {
        let h = hover_at(&mut server, &uri, SLIDES, line, cmd, 2);
        assert!(h.contains(&format!("{cmd}<")) && h.contains(">{"), "{cmd}: {h}");
    }
    let alt = hover_at(&mut server, &uri, SLIDES, 7, "\\alt", 2);
    assert!(alt.matches('{').count() >= 2, "{alt}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// An overlay's hover lists the slides its material shows on, from the pages the run shipped:
/// absolute, covered (`\uncover`), nested, trailing and incremental (`<+->`) specifications.
#[test]
fn overlay_hover_lists_the_slides_material_shows_on() {
    let (mut server, uri, dir) = open_set("bmrovl", SLIDES, &[]);
    let h = |s: &mut Server, line, needle, off| hover_at(s, &uri, SLIDES, line, needle, off);
    assert!(h(&mut server, 4, "<2>", 1).contains("slide 2 of 3"), "{}", h(&mut server, 4, "<2>", 1));
    assert!(h(&mut server, 5, "<3>", 1).contains("slide 3 of 3"), "{}", h(&mut server, 5, "<3>", 1));
    assert!(h(&mut server, 6, "<2-3>", 1).contains("slides 2, 3 of 3"), "{}", h(&mut server, 6, "<2-3>", 1));
    assert!(h(&mut server, 6, "<3>", 1).contains("slide 3 of 3"), "{}", h(&mut server, 6, "<3>", 1));
    assert!(h(&mut server, 8, "<1>", 1).contains("slide 1 of 3"), "{}", h(&mut server, 8, "<1>", 1));
    assert!(h(&mut server, 12, "<+->", 1).contains("slides 1, 2 of 2"), "{}", h(&mut server, 12, "<+->", 1));
    assert!(h(&mut server, 13, "<+->", 1).contains("slide 2 of 2"), "{}", h(&mut server, 13, "<+->", 1));
    // the steps of the frame, not of another
    let steps = labels(&complete(&mut server, &uri, 4, col(SLIDES, 4, "<2>", 1))).join(" ");
    assert_eq!(steps, "1 1- 2 2- 3 3-");
    let steps = labels(&complete(&mut server, &uri, 12, col(SLIDES, 12, "<+->", 1))).join(" ");
    assert_eq!(steps, "1 1- 2 2-");
    // `<` that is no overlay: math
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// "Compile only this frame": a lens per frame; the setup adds `\includeonlyframes` before
/// `\begin{document}`, and a label to a frame without one; a labelled frame keeps its own.
#[test]
fn compile_only_this_frame() {
    let (mut server, uri, dir) = open_set("bmrframe", SLIDES, &[]);
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].clone();
    let frames: Vec<u64> = (lenses.as_array().unwrap().iter())
        .filter(|l| l["command"]["title"] == "Compile only this frame")
        .map(|l| l["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(frames, [3, 10, 16], "{lenses}");
    let views = at(&mut server, &uri, "satex/view", 4, 3);
    let view = views.as_array().unwrap().iter().find(|v| v["plugin"] == "frame").expect("frame view").clone();
    assert_eq!(view["compile"]["label"], "frame1", "{view}");
    let out = applied(SLIDES, &uri, &view["compile"]["edit"], &[]);
    assert!(out.contains("\\includeonlyframes{frame1}\n\\begin{document}"), "{out}");
    assert!(out.contains("\\begin{frame}[label=frame1]{Steps}"), "{out}");
    let actions = at(&mut server, &uri, "textDocument/codeAction", 12, 5);
    let act = (actions.as_array().unwrap().iter())
        .find(|a| a["title"].as_str().is_some_and(|t| t.starts_with("Compile only this frame")))
        .unwrap_or_else(|| panic!("{actions}"))
        .clone();
    let out = applied(SLIDES, &uri, &act["edit"], &[]);
    assert!(out.contains("\\includeonlyframes{inc}\n\\begin{document}"), "{out}");
    assert!(out.contains("\\begin{frame}[t,label=inc]{Inc}"), "{out}");
    // options without a label: one is added to them
    let actions = at(&mut server, &uri, "textDocument/codeAction", 17, 3);
    let act = (actions.as_array().unwrap().iter())
        .find(|a| a["title"].as_str().is_some_and(|t| t.starts_with("Compile only this frame")))
        .unwrap_or_else(|| panic!("{actions}"))
        .clone();
    let out = applied(SLIDES, &uri, &act["edit"], &[]);
    assert!(
        out.contains("\\begin{frame}[t,label=frame3]{Opt}") && out.contains("\\includeonlyframes{frame3}"),
        "{out}"
    );
    // outside any frame: nothing
    let preamble = at(&mut server, &uri, "textDocument/codeAction", 1, 2);
    assert!(!preamble.to_string().contains("Compile only"), "{preamble}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// Switched off, neither plugin answers; in an article no frame lens appears.
#[test]
fn overlay_and_frame_plugins_switch_off() {
    let sets = ["type_plugins.overlay.enabled=false", "type_plugins.frame.enabled=false"];
    let (mut server, uri, dir) = open_set("bmroff", SLIDES, &sets);
    let h = hover_at(&mut server, &uri, SLIDES, 4, "<2>", 1);
    assert!(!h.contains("Overlay"), "{h}");
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].to_string();
    assert!(!lenses.contains("Compile only"), "{lenses}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
    let article = "\\documentclass{article}\n\\begin{document}\nA $a<b$ \\begin{itemize}\\item x\\end{itemize}\n\\end{document}\n";
    let (mut server, uri, dir) = open_set("bmrart", article, &[]);
    let id = server.request("textDocument/codeLens", json!({ "textDocument": { "uri": uri } }));
    let lenses = server.response(id)["result"].to_string();
    assert!(!lenses.contains("Compile only"), "{lenses}");
    let h = hover_md(&at(&mut server, &uri, "textDocument/hover", 2, 5));
    assert!(!h.contains("Overlay"), "{h}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// A code language completes where listings reads one (`language=`, its dialects written
/// `[LaTeX]TeX`, a language the document defines) and where minted reads one (Pygments' lexers,
/// or the seed without Pygments); hover says where it is defined; listings' unknown name is a finding.
#[test]
fn code_languages_complete_hover_and_lint() {
    let text = "\\documentclass{article}\n\\usepackage{listings}\n\\usepackage{minted}\n\\lstdefinelanguage{MyLang}{morekeywords={foo}}\n\\lstset{language=Python}\n\\begin{document}\n\\begin{lstlisting}[language=]\nx\n\\end{lstlisting}\n\\begin{lstlisting}[language=Nope]\nx\n\\end{lstlisting}\n\\mintinline{}{x}\n\\end{document}\n";
    let (mut server, uri, published, dir) = open_loaded("codelang", text);
    let line = |n: usize| text.lines().nth(n).unwrap();
    let after = |n: usize, s: &str| u32::try_from(line(n).find(s).unwrap() + s.len()).unwrap();
    let listings = complete(&mut server, &uri, 6, after(6, "language="));
    for name in ["Python", "[LaTeX]TeX", "MyLang", "C"] {
        assert!(labels(&listings).contains(&name), "{name}: {:?}", labels(&listings));
    }
    // the document's languages first, each with where it is defined
    assert!(labels(&listings)[..3].contains(&"Python"), "{:?}", labels(&listings));
    let python = listings.iter().find(|i| i["label"] == "Python").unwrap();
    assert!(python["detail"].as_str().is_some_and(|d| d.ends_with(".sty")), "{python}");
    let minted = complete(&mut server, &uri, 12, after(12, "\\mintinline{"));
    assert!(labels(&minted).contains(&"python"), "{:?}", labels(&minted));
    assert!(!labels(&minted).contains(&"[LaTeX]TeX"), "{:?}", labels(&minted));
    let id = server.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 4, "character": after(4, "language=P") } }),
    );
    let hover = server.response(id)["result"]["contents"]["value"].as_str().unwrap_or_default().to_string();
    assert!(hover.contains("Code language `Python`") && hover.contains("Defined in lstlang"), "{hover}");
    let diagnostics = published["params"]["diagnostics"].as_array().cloned().unwrap_or_default();
    let unknown: Vec<&Value> = diagnostics.iter().filter(|d| d["data"]["rule"] == "unknown-code-language").collect();
    assert_eq!(unknown.len(), 1, "{diagnostics:?}");
    assert_eq!(unknown[0]["range"]["start"], json!({ "line": 9, "character": after(9, "language=") }), "{unknown:?}");
    assert_eq!(unknown[0]["severity"], 2, "a warning: {unknown:?}");
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

/// In a listing's body only its `literate=` sequences complete, each with what it becomes; inside
/// `escapeinside` and `mathescape` escapes TeX runs, so a `\ref` completes its labels there only.
#[test]
fn a_listing_completes_literate_outside_and_latex_inside_its_escapes() {
    let text = "\\documentclass{article}\n\\usepackage{listings}\n\\lstset{literate={:yields:}{{$\\Rightarrow$}}1 {->}{{$\\to$}}2, escapeinside={(*}{*)}, mathescape}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\\begin{lstlisting}\nx :y\ny (*\\ref{sec:a}*) \\ref{sec:a}\nz = $\\ref{sec:a}$\n\\end{lstlisting}\n\\end{document}\n";
    let (mut server, uri, _, dir) = open_loaded("listing", text);
    let line = |n: usize| text.lines().nth(n).unwrap();
    let after =
        |n: usize, s: &str, k: usize| u32::try_from(line(n).match_indices(s).nth(k).unwrap().0 + s.len()).unwrap();
    let literate = complete(&mut server, &uri, 6, after(6, ":y", 0));
    assert_eq!(labels(&literate), [":yields:"], "{literate:?}");
    assert!(literate[0]["detail"].as_str().is_some_and(|d| d.contains("\\Rightarrow")), "{literate:?}");
    let escaped = complete(&mut server, &uri, 7, after(7, "\\ref{", 0));
    assert!(labels(&escaped).contains(&"sec:a"), "{:?}", labels(&escaped));
    let outside = complete(&mut server, &uri, 7, after(7, "\\ref{", 1));
    assert!(!labels(&outside).contains(&"sec:a"), "{:?}", labels(&outside));
    let math = complete(&mut server, &uri, 8, after(8, "\\ref{", 0));
    assert!(labels(&math).contains(&"sec:a"), "{:?}", labels(&math));
    server.shutdown();
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn every_plugin_has_integration_support() {
    use satex::lsp::{INTEGRATIONS, plugin_features, works_in};
    let known =
        ["hover", "complete", "actions", "colors", "inlay", "lens", "view", "dialog", "manage", "reuse", "build"];
    let plugins = plugin_features();
    assert!(plugins.len() > 10);
    for (name, features) in plugins {
        assert!(
            features.iter().all(|f| known.contains(f)),
            "{name}: a feature no integration table knows: {features:?}"
        );
        let w = works_in(features);
        assert_eq!(
            w.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            INTEGRATIONS.iter().map(|i| i.0).collect::<Vec<_>>(),
            "{name}"
        );
        let editor = features.iter().any(|f| !["reuse", "build"].contains(f));
        assert_eq!(w.iter().all(|(_, l)| *l == "-"), !editor, "{name}");
        assert!(w.iter().all(|(k, l)| *k != "playground" || *l != "none"), "{name}");
    }
    assert_eq!(works_in(&["hover"]).iter().find(|(k, _)| *k == "intellij").unwrap().1, "full");
    assert_eq!(works_in(&["view"]).iter().find(|(k, _)| *k == "intellij").unwrap().1, "partial");
}
