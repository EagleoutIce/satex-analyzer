// `satex_status` for src/lsp/wasm.rs: the engine's status as JSON, to the
// page's `onStatus` (doc/site/worker.js). A JS exception must not unwind
// through Rust's frames: that aborts the engine.
addToLibrary({
  satex_status: (json) => { try { Module.onStatus?.(UTF8ToString(json)); } catch (e) { console.error(e); } },
});
