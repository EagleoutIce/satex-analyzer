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
    assert!(text.contains("`\\greet{⟨arg1⟩}`") && !text.contains("signature"), "hover shows no usage line: {text}");

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

/// Opens `text` as `name` in a fresh directory whose file on disk says
/// something else, the way the playground's buffers differ from its files.
fn open_unsaved(tag: &str, text: &str) -> (Server, String, Value, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("satex-lsp-{tag}-{}", std::process::id()));
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

/// A folder opened through a symlink is answered under the symlink, though
/// the server's working directory is the resolved folder (#17), and an input
/// file is analyzed with the document that reads it.
#[cfg(unix)]
#[test]
fn a_folder_opened_through_a_symlink_keeps_its_name() {
    let base = std::env::temp_dir().join(format!("satex-lsp-link-{}", std::process::id()));
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
        which::which("kpsewhich").is_ok()
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
        let dir = std::env::temp_dir().join(format!("satex-lsp-input-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("satex-lsp-unreached-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("satex-lsp-load-{}", std::process::id()));
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

/// The completion items at 0-based `line`:`character`, ordered as a client
/// orders them: by `sortText`, then label.
fn complete(server: &mut Server, uri: &str, line: u32, character: u32) -> Vec<Value> {
    let id = server.request(
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri }, "position": { "line": line, "character": character } }),
    );
    let mut items = server.response(id)["result"].as_array().cloned().unwrap_or_default();
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
    let dir = std::env::temp_dir().join(format!("satex-lsp-{tag}-{}", std::process::id()));
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
    // A document author's commands before the kernel's internals.
    let commands = complete(&mut server, &uri, 10, 1);
    let at = |name: &str| labels(&commands).iter().position(|l| *l == name);
    assert!(at("section").unwrap() < at("@startsection").unwrap(), "section ranks after @startsection");
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
