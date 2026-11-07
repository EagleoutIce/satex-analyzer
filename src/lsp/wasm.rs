//! The server inside the browser playground (`doc/site/worker.js`): no
//! threads, no stdio. `satex lsp` sets up the engine and returns with the
//! runtime alive. The page then hands each JSON-RPC message to `satex_lsp`,
//! which answers with the JSON array of messages to send back ([`Batch`]). While a
//! message runs, `satex_status` (src/lsp/wasm-status.js) hands the page what
//! the engine is doing, as `satex/status` does for an editor.

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char};

use super::batch::Batch;

struct Local {
    batch: Batch,
    reply: CString,
}

thread_local! { static LOCAL: RefCell<Option<Local>> = const { RefCell::new(None) }; }

unsafe extern "C" {
    fn emscripten_exit_with_live_runtime() -> !;
    fn satex_status(json: *const c_char);
    fn satex_cancelled() -> i32;
}

/// Whether the page asked to stop the run under way (src/lsp/wasm-status.js).
pub(super) fn cancelled() -> bool {
    unsafe { satex_cancelled() != 0 }
}

pub fn serve(cfg: crate::project::config::Config, root: std::path::PathBuf, main: Option<std::path::PathBuf>) -> ! {
    let batch = Batch::new(cfg, root, main);
    crate::util::progress::listen(Some(Box::new(|now: &crate::util::progress::Status| {
        if let Some(json) = serde_json::to_string(now).ok().and_then(|j| CString::new(j).ok()) {
            unsafe { satex_status(json.as_ptr()) }
        }
    })));
    LOCAL.set(Some(Local { batch, reply: CString::default() }));
    unsafe { emscripten_exit_with_live_runtime() }
}

/// # Safety
/// `text` is a NUL-terminated UTF-8 string; the result lives until the next call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn satex_lsp(text: *const c_char) -> *const c_char {
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    LOCAL.with_borrow_mut(|local| {
        let Some(local) = local else { return c"[]".as_ptr() };
        let out = serde_json::to_string(&local.batch.message(&text)).unwrap_or_else(|_| "[]".into());
        local.reply = CString::new(out).unwrap_or_default();
        local.reply.as_ptr()
    })
}
