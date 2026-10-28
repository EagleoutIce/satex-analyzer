// Dev-mode reload client, served by `cargo playground --dev`: CSS swaps in place, anything else reloads.
new EventSource('/__dev/events').onmessage = (e) => {
  if (e.data === 'reload') return location.reload();
  if (e.data !== 'css') return;
  for (const old of document.querySelectorAll('link[rel=stylesheet]')) {
    const next = old.cloneNode();
    next.href = old.href.replace(/[?#].*/, '') + '?t=' + Date.now();
    next.onload = () => old.remove();
    old.after(next);
  }
};
