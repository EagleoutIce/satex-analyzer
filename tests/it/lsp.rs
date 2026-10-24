//! Drives `satex lsp` over real stdio pipes with hand-framed JSON-RPC, the
//! way an editor client would: `initialize`, `didOpen`, then a request per
//! feature. The document skips the kernel (`load.format: false`, as
//! `tests/it/engine.rs` does) so the run analyzes in milliseconds rather than
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
        let mut child = Command::new(env!("CARGO_BIN_EXE_satex"))
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
    satex::paths::file_url(path)
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
    let unused = diagnostics.iter().find(|d| d["code"] == "unused-definition");
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
/// the file the run resolved, at its start ([`satex::facts::Load::path`]).
mod goto_definition_on_loads {
    use super::*;

    fn kpsewhich_installed() -> bool {
        satex::paths::which("kpsewhich").is_ok()
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
    let file = dir.join("main.tex");
    std::fs::write(&file, text).unwrap();
    let uri = fixture_uri(&file);
    let cache = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-cache");
    let mut server = Server::start_with(&cache, &dir, &["--set", "limits.seconds=0"]);
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
    let found = diagnostics.iter().find(|d| d["code"] == "already-defined").expect("already-defined");
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
    assert!(hover.contains("Greets someone.") && hover.contains("`⟨arg1⟩` who is greeted"), "{hover}");
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
        let dir = crate::support::tmp_root().join(format!("satex-lsp-roots-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            std::fs::write(dir.join(name), text).unwrap();
        }
        let mut server = Server::start(&dir);
        let id = server.request("initialize", json!({ "processId": null, "rootUri": null, "capabilities": {} }));
        server.response(id);
        server.notify("initialized", json!({}));
        let uri = fixture_uri(&dir.join(open));
        let text = std::fs::read_to_string(dir.join(open)).unwrap();
        server.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "tex", "version": 1, "text": text } }),
        );
        server.notification("textDocument/publishDiagnostics");
        (server, uri, dir)
    }

    /// Where `\greet` on the last line of `uri` is defined.
    fn greet_defined_in(server: &mut Server, uri: &str) -> String {
        let path = satex::paths::from_file_url(uri).unwrap();
        let line = std::fs::read_to_string(path).unwrap().lines().count() - 1;
        let id = server.request(
            "textDocument/definition",
            json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": 2 } }),
        );
        let def = server.response(id);
        def["result"][0]["uri"].as_str().unwrap_or_else(|| panic!("no definition: {def:?}")).to_string()
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
        let quiet = |server: &mut Server| {
            std::thread::sleep(Duration::from_millis(800));
            let id = server.request("textDocument/documentSymbol", json!({ "textDocument": { "uri": uri } }));
            loop {
                let msg = server.read();
                assert_ne!(msg["method"], "textDocument/publishDiagnostics", "ran again: {msg:?}");
                if msg["id"] == id {
                    break;
                }
            }
        };
        for version in 2..5 {
            change(&mut server, version, &main);
        }
        quiet(&mut server);
        // An edit and its undo before the debounce ends: the text analyzed.
        change(&mut server, 5, "% edited\n");
        change(&mut server, 6, &main);
        quiet(&mut server);
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

/// What an argument is comes with the command ([`satex::roles`]): a
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

/// What TeX reads each argument as ([`satex::roles::Ty`]) comes with the
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
    assert!(main.contains("— preamble"), "{hover}");
    assert!(main.contains("- `⟨arg1⟩` options") && main.contains("- `⟨arg2⟩` class (.cls)"), "{hover}");
    assert!(more.contains("— document") && !more.contains("— preamble"), "{hover}");
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
    assert!(hover.contains("#### About glue") && hover.contains("plus"), "{hover}");
    let items = complete(&mut server, &uri, 4, 6);
    let detail = items.iter().find(|i| i["label"] == "setlength").and_then(|i| i["detail"].as_str());
    assert!(detail.is_some_and(|d| d.contains("{1: skip register}") && d.contains("{2: glue}")), "{detail:?}");
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
    use satex::roles::{FileKind as F, KeyKind as K, Ty};
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
    assert!(hover.contains("- `⟨arg1⟩` label") && hover.contains("About label"), "{hover}");
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
        assert!(hover.contains(&format!("- `⟨arg1⟩` {kind}")) && !hover.contains("About key"), "{hover}");
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
    assert!(!labels(&paths).contains(&"/demo/mode/fast"), "{:?}", labels(&paths));
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
    assert!(!diagnostics.iter().any(|d| d["code"] == "caused-library-error"), "{published}");
    let own = diagnostics.iter().find(|d| d["code"] == "raised-error").unwrap_or_else(|| panic!("{published}"));
    assert!(
        own["message"]
            .as_str()
            .unwrap_or_default()
            .contains("the argument of \\pgfkeys opened here never closes, the file ends inside it"),
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

/// The rows of `satex/typeset` for a root with an `\input` file and a bibliography.
fn typeset_of_a_root(tag: &str) -> Vec<Value> {
    let dir = crate::support::tmp_root().join(format!("satex-lsp-typeset-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = "\\documentclass{article}\n\\begin{document}\n\\input{chapter}\n\\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n";
    let main = dir.join("main.tex");
    std::fs::write(&main, text).unwrap();
    std::fs::write(dir.join("chapter.tex"), "Chapter text \\cite{a}.\n").unwrap();
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
    let mut rows = Vec::new();
    for _ in 0..20 {
        let id = server.request("satex/typeset", json!({ "textDocument": { "uri": uri } }));
        rows = server.response(id)["result"].as_array().cloned().unwrap_or_default();
        if rows.iter().any(|r| r["detail"].as_str().is_some_and(|d| d.contains("A Title"))) {
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
    if satex::paths::which("kpsewhich").is_err() {
        return;
    }
    let rows = typeset_of_a_root("input");
    assert!(rows.iter().any(|r| r["path"].as_str().is_some_and(|p| p.ends_with("chapter.tex"))), "{rows:?}");
}

/// To do: the server analyzes a document in one run (`Checkpoints::analyze`), so the .bbl the build's
/// bibliography tool would make is not typeset here, as `satex query typeset` has it.
#[test]
fn typeset_of_a_root_lists_the_bibliography() {
    if satex::paths::which("kpsewhich").is_err() {
        return;
    }
    let rows = typeset_of_a_root("bibliography");
    assert!(
        rows.iter().any(|r| r["path"] == "main.tex" && r["detail"].as_str().is_some_and(|d| d.contains("A Title"))),
        "{rows:?}"
    );
}
