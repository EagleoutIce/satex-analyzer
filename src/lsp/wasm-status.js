// `satex_status` for src/lsp/wasm.rs: the engine's status as JSON, to the
// page's `onStatus` (doc/site/worker.js). A JS exception must not unwind
// through Rust's frames: that aborts the engine.
addToLibrary({
  // Whether the page asked to stop the run under way: a newer text came (doc/site/worker.js).
  satex_cancelled__sig: 'i',
  satex_cancelled: () => { try { return Module.cancelled?.() ? 1 : 0; } catch (e) { return 0; } },
  // A notification the engine sends while the message still runs (`satex/typesetRows`).
  satex_note: (json) => { try { Module.onNote?.(UTF8ToString(json)); } catch (e) { console.error(e); } },
  satex_status: (json) => { try { Module.onStatus?.(UTF8ToString(json)); } catch (e) { console.error(e); } },
});
