// `satex_status` for src/lsp/wasm.rs: the engine's status as JSON, to the
// page's `onStatus` (doc/site/worker.js).
addToLibrary({
  satex_status: (json) => Module.onStatus?.(UTF8ToString(json)),
});
