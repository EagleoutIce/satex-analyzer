//! Drives `satex lsp` over real stdio pipes with hand-framed JSON-RPC, the
//! way an editor client would: `initialize`, `didOpen`, then a request per
//! feature. The document skips the kernel (`load_format: false`, as
//! `tests/engine.rs` does) so the run analyzes in milliseconds rather than
//! waiting on `latex.ltx`.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

// Without a format the catcodes are INITEX's (tex.web § 232); prepended with
// no newline so line/column positions below stay on their original lines.
const FIXTURE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 \\def\\greet#1{Hello, #1!}\n\\greet{world}\n\\def\\unused{\\greet{x}}\n\\let\\alias\\greet\n";

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Server {
    fn start(cache: &std::path::Path) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_satex"))
            .args([
                "lsp",
                "--no-packages",
                "--no-classes",
                "--set",
                "load_format=false",
                "--set",
                "use_kpsewhich=false",
            ])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("SATEX_CACHE", cache)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("satex lsp starts");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server { child, stdin, stdout, next_id: 1 }
    }

    fn write(&mut self, value: &Value) {
        let body = serde_json::to_vec(value).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        self.stdin.write_all(&body).unwrap();
        self.stdin.flush().unwrap();
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

    /// Reads one framed JSON-RPC message, whichever it is; the caller knows
    /// from context whether to expect a response or a notification.
    fn read(&mut self) -> Value {
        let mut length = None;
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("a header line");
            if read == 0 {
                panic!("satex lsp closed stdout before sending {length:?} bytes");
            }
            if line == "\r\n" {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                length = rest.trim().parse::<usize>().ok();
            }
        }
        let length = length.expect("a Content-Length header");
        let mut body = vec![0u8; length];
        self.stdout.read_exact(&mut body).expect("the framed body");
        serde_json::from_slice(&body).expect("valid JSON-RPC")
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

    fn shutdown(mut self) {
        let id = self.request("shutdown", Value::Null);
        let resp = self.response(id);
        assert!(resp.get("error").is_none(), "shutdown failed: {resp:?}");
        self.notify("exit", Value::Null);
        drop(self.stdin);
        for _ in 0..100 {
            if self.child.try_wait().expect("satex lsp runs").is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        panic!("satex lsp did not exit after `exit`");
    }
}

fn fixture_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

#[test]
fn stdio_session_drives_the_feature_set() {
    let dir = std::env::temp_dir().join(format!("satex-lsp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("greet.tex");
    std::fs::write(&file, FIXTURE).unwrap();
    let uri = fixture_uri(&file);

    let mut server = Server::start(&dir);

    let init_id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
    let init = server.response(init_id);
    let capabilities = &init["result"]["capabilities"];
    assert_eq!(capabilities["hoverProvider"], json!(true));
    assert_eq!(capabilities["completionProvider"]["triggerCharacters"], json!(["\\", "{"]));
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

    // Completion right after the leading `\` of `\greet` offers it back.
    let completion_id = server.request(
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 1 } }),
    );
    let completion = server.response(completion_id);
    let items = completion["result"].as_array().cloned().unwrap_or_default();
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
