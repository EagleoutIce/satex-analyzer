//! The server inside the browser playground (`doc/site/worker.js`): no
//! threads, no stdio. `satex lsp` sets up the engine and returns with the
//! runtime alive. The page then hands each JSON-RPC message to `satex_lsp`,
//! which answers with the JSON array of messages to send back. While a
//! message runs, `satex_status` (src/lsp/wasm-status.js) hands the page what
//! the engine is doing, as `satex/status` does for an editor.

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char};

use lsp_server::{Message, Notification, Response};

use super::{Engine, handle_notification, respond, server_capabilities};
use lsp_types::PublishDiagnosticsParams;

struct Local {
    engine: Engine,
    diagnostics: crossbeam_channel::Receiver<PublishDiagnosticsParams>,
    reply: CString,
}

thread_local! { static LOCAL: RefCell<Option<Local>> = const { RefCell::new(None) }; }

unsafe extern "C" {
    fn emscripten_exit_with_live_runtime() -> !;
    fn satex_status(json: *const c_char);
}

pub fn serve(cfg: crate::config::Config, root: std::path::PathBuf, main: Option<std::path::PathBuf>) -> ! {
    let (tx, diagnostics) = crossbeam_channel::unbounded();
    let engine = Engine::new(cfg, root, main, tx);
    crate::progress::listen(Some(Box::new(|now: &crate::progress::Status| {
        if let Some(json) = serde_json::to_string(now).ok().and_then(|j| CString::new(j).ok()) {
            unsafe { satex_status(json.as_ptr()) }
        }
    })));
    LOCAL.set(Some(Local { engine, diagnostics, reply: CString::default() }));
    unsafe { emscripten_exit_with_live_runtime() }
}

impl Local {
    /// Handles one message. An edit only updates the text, so a hover or a
    /// completion asked while typing is answered from the last analysis;
    /// the page sends `satex/analyze` once the typing pauses.
    fn message(&mut self, text: &str) -> Vec<Message> {
        let engine = &mut self.engine;
        let mut out = match serde_json::from_str::<Message>(text) {
            Ok(Message::Request(req)) => vec![Message::Response(match req.method.as_str() {
                "initialize" => Response::new_ok(req.id, serde_json::json!({"capabilities": server_capabilities()})),
                "shutdown" => Response::new_ok(req.id, ()),
                _ => respond(
                    &mut |m| {
                        engine.step(m);
                        true
                    },
                    req,
                ),
            })],
            Ok(Message::Notification(not)) if not.method == "satex/analyze" => {
                engine.flush();
                vec![]
            }
            Ok(Message::Notification(not)) => {
                let opened = not.method == "textDocument/didOpen";
                handle_notification(
                    &mut |m| {
                        engine.step(m);
                        true
                    },
                    not,
                );
                // A document just opened has no analysis to answer from yet.
                if opened {
                    engine.flush();
                }
                vec![]
            }
            _ => vec![],
        };
        out.extend(
            self.diagnostics
                .try_iter()
                .map(|p| Message::Notification(Notification::new("textDocument/publishDiagnostics".into(), p))),
        );
        out
    }
}

/// # Safety
/// `text` is a NUL-terminated UTF-8 string; the result lives until the next call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn satex_lsp(text: *const c_char) -> *const c_char {
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    LOCAL.with_borrow_mut(|local| {
        let Some(local) = local else { return c"[]".as_ptr() };
        let out = serde_json::to_string(&local.message(&text)).unwrap_or_else(|_| "[]".into());
        local.reply = CString::new(out).unwrap_or_default();
        local.reply.as_ptr()
    })
}
