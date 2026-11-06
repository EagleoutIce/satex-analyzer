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
        let mut out = match serde_json::from_str::<Message>(text) {
            Ok(Message::Request(req)) => vec![Message::Response(match req.method.as_str() {
                "initialize" => Response::new_ok(req.id, serde_json::json!({"capabilities": server_capabilities()})),
                "shutdown" => Response::new_ok(req.id, ()),
                "textDocument/linkedEditingRange" => {
                    let found = super::linked(&req, |uri| engine.docs.get(uri).map(|d| &d.text));
                    Response::new_ok(req.id, found.unwrap_or(serde_json::Value::Null))
                }
                _ => respond(
                    &mut |m| {
                        engine.step(m);
                        true
                    },
                    req,
                ),
            })],
            // `{"files": true}`: files outside the documents changed, so all run again
            Ok(Message::Notification(not)) if not.method == "satex/analyze" => {
                if not.params.get("files").is_some_and(|f| f.as_bool() == Some(true)) {
                    engine.outside_changed();
                }
                engine.flush();
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

/// The line that stops the run under way ([`lines`]).
#[cfg(not(target_os = "emscripten"))]
const CANCEL: &str = "{\"cancel\"";

/// `satex lsp --lines`: one message per line on stdin. Per message it
/// writes a `satex/status` line for each step of the work (a JSON object)
/// and then the replies (a JSON array). A line `{"cancel":true}` stops the
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
    crate::util::progress::listen(Some(Box::new(move |now: &crate::util::progress::Status| {
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
