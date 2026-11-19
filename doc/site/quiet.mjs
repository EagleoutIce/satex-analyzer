// The playground at rest, as page code for the DevTools drivers (test.mjs, showcase.mjs, showcase/capture.mjs):
// no run going, due or queued, no work in the status line (playground.js `satexHealth`).
export const QUIET = `(() => { const h = self.satexHealth?.(); return h ? !h.busy && !h.running && !h.due && !h.queued : !document.getElementById('play-status').hasAttribute('data-busy'); })()`;
