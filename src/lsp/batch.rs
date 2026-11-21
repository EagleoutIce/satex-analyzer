//! The server one message at a time: each JSON-RPC message in, the messages
//! it causes out, with no thread of its own. The browser playground runs it
//! through `satex_lsp` (`wasm.rs`), `satex gui` through `satex lsp --lines`
//! ([`lines`]). An edit only updates the text, so a hover or a completion
//! asked while typing is answered from the last analysis. The page sends
//! `satex/analyze` once the typing pauses.

use lsp_server::{Message, Notification, Response};

use super::{Engine, notification, respond, server_capabilities};

pub(super) struct Batch {
    pub(super) engine: Engine,
    pub(super) outbox: crossbeam_channel::Receiver<Notification>,
}

impl Batch {
    pub(super) fn new(
        cfg: crate::project::config::Config,
        root: std::path::PathBuf,
        main: Option<std::path::PathBuf>,
    ) -> Self {
        let (tx, outbox) = crossbeam_channel::unbounded();
        Batch { engine: Engine::new(cfg, root, main, tx), outbox }
    }

    pub(super) fn message(&mut self, text: &str) -> Vec<Message> {
        let engine = &mut self.engine;
        // What the last run replaced is freed before a question, not before the next text's run
        // (the page's questions follow a run's results; a text typed meanwhile goes first).
        let message = serde_json::from_str::<Message>(text);
        let edit = |m: &str| matches!(m, "textDocument/didChange" | "textDocument/didOpen" | "satex/analyze");
        if !matches!(&message, Ok(Message::Notification(n)) if edit(&n.method)) || engine.buried.len() > 2 {
            engine.rest_buried();
        }
        let mut out = match message {
            Ok(Message::Request(req)) => vec![Message::Response(match req.method.as_str() {
                "initialize" => Response::new_ok(req.id, serde_json::json!({"capabilities": server_capabilities()})),
                "shutdown" => Response::new_ok(req.id, ()),
                "textDocument/linkedEditingRange" => {
                    let found = super::linked(&req, |uri| engine.docs.get(uri).map(|d| &d.text));
                    Response::new_ok(req.id, found.unwrap_or(serde_json::Value::Null))
                }
                // A question about the text: a newer text abandons it (`probe::abandonable`), so the
                // newer text's run never waits for it; answered `null`, as the page answers a question
                // it dropped before it began.
                _ if read_only(&req) => {
                    let (id, stale) = (req.id.clone(), engine.newer_text());
                    let answer = crate::analysis::probe::abandonable(stale, || {
                        respond(
                            &mut |m| {
                                engine.step(m);
                                true
                            },
                            req,
                        )
                    });
                    answer.unwrap_or_else(|| Response::new_ok(id, serde_json::Value::Null))
                }
                _ => respond(
                    &mut |m| {
                        engine.step(m);
                        true
                    },
                    req,
                ),
            })],
            // `{"files": true}`: files outside the documents changed, so all run again.
            // `{"first": URI}`: the document the page shows, analyzed before the others.
            // `{"typeset": URI}`: its typeset rows go out as `satex/typesetRows` once its run ends.
            Ok(Message::Notification(not)) if not.method == "satex/analyze" => {
                engine.typeset.early = not.params.get("typeset").and_then(|f| f.as_str()).and_then(|f| f.parse().ok());
                if not.params.get("files").is_some_and(|f| f.as_bool() == Some(true)) {
                    engine.outside_changed();
                }
                let first: Option<lsp_types::Uri> = not.params.get("first").and_then(|f| f.as_str()?.parse().ok());
                if let Some(first) = first.as_ref().filter(|f| engine.dirty.contains(f)) {
                    engine.first(first);
                }
                // The page waits for the shown file's findings to end its run: a run that
                // published nothing new there (a file switch, a reused run) sends them again,
                // of the text's version, where they are of the analysis that text has.
                let last = |e: &super::Engine| {
                    let f = first.as_ref().filter(|f| !e.dirty.contains(f))?;
                    let (run, p) = e.published.get(f)?;
                    let doc = e.docs.get(f)?;
                    let current =
                        run.upgrade().zip(doc.analysis.as_ref()).is_some_and(|(r, a)| std::rc::Rc::ptr_eq(&r, a));
                    current.then(|| (p.clone(), doc.version))
                };
                let before = last(engine).map(|(p, _)| p);
                engine.flush();
                if let Some(f) = first.as_ref() {
                    engine.publish_installed(f);
                }
                if let Some((mut again, version)) = last(engine).filter(|(now, _)| Some(now) == before.as_ref()) {
                    again.version = Some(version);
                    engine.send_diagnostics(vec![again]);
                }
                vec![]
            }
            // An opened document waits for `satex/analyze` too: the page
            // opens a project's files together and asks for one run.
            Ok(Message::Notification(not)) => {
                if let Some(m) = notification(not, |uri| engine.docs.get(uri).map(|d| d.text.as_str())) {
                    engine.step(m);
                }
                vec![]
            }
            _ => vec![],
        };
        out.extend(self.outbox.try_iter().map(Message::Notification));
        out
    }
}

/// Whether `req` only asks about the texts: the requests the page's worker drops once a newer
/// text came (doc/site/worker.ts `readOnly`).
fn read_only(req: &lsp_server::Request) -> bool {
    matches!(
        req.method.as_str(),
        "textDocument/completion"
            | "textDocument/hover"
            | "textDocument/codeLens"
            | "textDocument/documentColor"
            | "textDocument/inlayHint"
            | "textDocument/codeAction"
            | "textDocument/documentSymbol"
            | "textDocument/documentHighlight"
            | "textDocument/documentLink"
            | "textDocument/foldingRange"
            | "textDocument/signatureHelp"
            | "satex/typeset"
            | "satex/written"
            | "satex/caches"
    ) || req.method == "workspace/executeCommand"
        && req.params.get("command").and_then(|c| c.as_str()) == Some("satex/query")
}

/// The line that stops the run under way ([`lines`]).
#[cfg(not(target_os = "emscripten"))]
const CANCEL: &str = "{\"cancel\"";

/// `satex lsp --lines`: one message per line on stdin. Per message it
/// writes a `satex/status` line for each step of the work and each
/// notification of a run that ended (JSON objects), then the replies (a JSON array). A line `{"cancel":true}` stops the
/// run under way, as a newer text does. The end of stdin stops it too.
#[cfg(not(target_os = "emscripten"))]
pub fn lines(cfg: crate::project::config::Config, root: std::path::PathBuf, main: Option<std::path::PathBuf>) {
    use std::io::{BufRead, Write};
    use std::sync::atomic::Ordering::Relaxed;
    let mut batch = Batch::new(cfg, root, main);
    let kept = batch.engine.hovers.clone();
    // Counted as they come, taken in turn (`Engine::step`): a run is stale once more came.
    let (tx, rx) = crossbeam_channel::unbounded::<String>();
    let reader = kept.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let method = serde_json::from_str::<serde_json::Value>(&line).ok().map(|m| m["method"].clone());
            let edit = method
                .as_ref()
                .and_then(|m| m.as_str())
                .is_some_and(|m| m == "textDocument/didOpen" || m == "textDocument/didChange");
            if edit || line.starts_with(CANCEL) {
                reader.edits.fetch_add(1, Relaxed);
            }
            if tx.send(line).is_err() {
                break;
            }
        }
        // stdin closed: the run under way stops, the loop ends
        reader.edits.fetch_add(1, Relaxed);
    });
    fn say(line: String) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}").and_then(|()| out.flush());
    }
    // A run's findings go out once it ends, not when the message's other runs end too.
    let outbox = batch.outbox.clone();
    crate::util::progress::listen(Some(Box::new(move |now: &crate::util::progress::Status| {
        for note in outbox.try_iter() {
            say(serde_json::to_string(&note).unwrap_or_default());
        }
        say(serde_json::to_string(&Notification::new("satex/status".into(), now)).unwrap_or_default());
    })));
    for line in rx {
        // a cancel is taken like an edit, so the counts stay even
        if line.starts_with(CANCEL) {
            batch.engine.taken += 1;
            continue;
        }
        say(serde_json::to_string(&batch.message(&line)).unwrap_or_else(|_| "[]".into()));
    }
    crate::util::progress::listen(None);
}
