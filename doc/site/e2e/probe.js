// The test probe: the page side of the drivers' step vocabulary (page.mjs), injected by the e2e suite and the showcase
// before the page's scripts, never shipped. Every call is `satexTest.run(<JSON>)`: data in, data out, no code sent.
// Targets (`point`): { at, plus, minus } at + (plus - minus) · { word, nth, fx, fy } a word of the shown build (playground.js `satexPdfWordIndex`, one index per
// build), { sel, text | starts (its text) | exact (its label: its first span's text) | match | title | data: { key: regex } | has | child: [sel, text] | leaf, nth, or, fx, fy } an element, { src, k } the editor at a source offset (the caret put there),
// { range: sel, text } the end of a text drawn inside sel, { page, fx, fy } a point of a page image, { x: target, y: target } one coordinate of each; all take dx, dy, and
// `scroll` (a word or editor place brought into view first).
// Conditions (`until`): 'quiet' 'built' 'ready' 'caret' 'dialog' 'pdf' 'words' 'echo' 'drawn' 'newBuild' 'oneBuild', { sel, …, visible, count }, { none: element },
// { src: text } / { srcMatch: regex } the editor's text, { word }, { diag: regex } a finding, { open: sel } a dialog
// open, { hidden: sel }, { pressed: [sel, bool] }, { health: { key: value } }, { not }, { all: […] }, { any: […] }.
// Actions (`act`): { press } an element's click(), { toggle: { sel, attr, want } }, { check: sel }, { inputs: { sel,
// values: { label: value } } }, { text } / { replace: [from, to] } / { insert: { at, k, text } } the editor's text,
// { tab: file }, { caret: { src, k } }, { select: { from, to, k } }, { scroll: { src, top } | 0 }, { css: text },
// { size: { sel, width, height } }, { highlight: { match, color } | null }, { blur }, { focus: sel }, { keydown: { key, keyCode, shift, on } }, { fill: { sel, value } }, { remove: sel }, { clear: sel } (its text emptied), { close: sel }, { mark: 'builds' }, { cursor }, { open: { widget, at, k } }.
(() => {
  if (self.satexTest) return;
  const $$ = (s) => [...document.querySelectorAll(s)];
  const view = () => document.querySelector('.cm-editor')?.satexView;
  const text = () => view()?.state.doc.toString() ?? '';
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const quiet = () => { const h = self.satexHealth?.(); return h ? !h.busy && !h.running && !h.due && !h.queued : !document.getElementById('play-status').hasAttribute('data-busy'); };
  const label = (e) => (e.querySelector(':scope > span')?.textContent ?? e.textContent).trim();
  /** The elements of `t`: a selector, kept by their text, title or a descendant. */
  // `within`: inside the last element of that selector (the newest submenu)
  const elements = (t) => (t.within ? [...($$(t.within).at(-1)?.querySelectorAll(t.sel) ?? [])] : $$(t.sel)).filter((e) => (t.text == null || e.textContent.includes(t.text)) && (t.starts == null || e.textContent.trim().startsWith(t.starts))
    && (t.exact == null || label(e) === t.exact) && (t.match == null || new RegExp(t.match).test(e.textContent))
    && (t.title == null || new RegExp(t.title).test(e.title ?? '')) && (t.has == null || !!e.querySelector(t.has)) && (!t.visible || e.checkVisibility())
    && (t.data == null || Object.entries(t.data).every(([k, v]) => new RegExp(v).test(e.dataset[k] ?? ''))) && (!t.leaf || !e.children.length) && (t.child == null || e.querySelector(t.child[0])?.textContent === t.child[1]));
  const element = (t) => elements(t)[t.nth ?? 0] ?? (t.or ? element(t.or) : null);
  /** A page word's box in client px: [left, top, right, bottom], its page scrolled to show it first with `scroll`. */
  // a word target: `word` one text or any of several, else `match` (a regex over every word); kept by its height
  // (`minH`, `maxH` in bp), a word `after` it on its row within 8 bp before it, a word `beside` it on its row
  const rowOf = (ix, w, text) => (ix.words.get(text) ?? []).filter((v) => v[0] === w[0] && Math.abs(v[4] - w[4]) < 1);
  function wordOf(t) {
    const ix = self.satexPdfWordIndex?.();
    if (!ix) return null;
    // one text: its index entry; several or a regex: every word in page order (a few such scenarios)
    const all = typeof t.word === 'string' ? ix.words.get(t.word) ?? [] : (self.satexPdfWords?.() ?? []).filter((w) => Array.isArray(t.word) ? t.word.includes(w[5]) : new RegExp(t.match, 'u').test(w[5]));
    const keep = all.filter((w) => (t.page == null || w[0] === t.page) && (t.minH == null || w[4] - w[2] > t.minH) && (t.maxH == null || w[4] - w[2] < t.maxH)
      && (t.after == null || rowOf(ix, w, t.after).some((v) => v[3] < w[1] && w[1] - v[3] < 8)) && (t.beside == null || rowOf(ix, w, t.beside).length));
    const w = keep.at(t.nth ?? 0);
    return w && { w, dpi: ix.dpi };
  }
  /** A page word's box in client px: [left, top, right, bottom, scale]; with `scroll` ([top, bottom] margins, default
   * [120, 200]) its page scrolled to bring it into view first. */
  async function wordBox(t) {
    const hit = wordOf(t);
    if (!hit) return null;
    const { w, dpi } = hit, img = () => document.querySelector(`#play-pdf .pdf-page[data-page="${w[0]}"] img`);
    const box = () => { const i = img(), r = i.getBoundingClientRect(), k = r.width / (i.naturalWidth * 72 / dpi); return [r.left + w[1] * k, r.top + w[2] * k, r.left + w[3] * k, r.top + w[4] * k, k, w[0], w[5]]; };
    if (!img()) return null;
    if (t.scroll) {
      const [top, bottom] = Array.isArray(t.scroll) ? t.scroll : [120, 200], sc = document.querySelector('#play-pdf .pdf-pages').getBoundingClientRect(), b = box();
      if (b[1] < sc.top + top || b[1] > sc.bottom - bottom) { document.querySelector('#play-pdf .pdf-pages').scrollTop += b[1] - (sc.top + sc.height * 0.4); await sleep(500); }
    }
    return box();
  }
  async function point(t) {
    if (t == null) return null;
    let p = null;
    if (t.at) { const a = await point(t.at), b = await point(t.plus), c = await point(t.minus); p = a && b && c && [a[0] + b[0] - c[0], a[1] + b[1] - c[1]]; }
    else if (t.x || t.y) { const a = await point(t.x), b = await point(t.y); p = a && b && [a[0], b[1]]; }
    else if (!t.sel && (t.word != null || t.match != null)) { const b = await wordBox(t); p = b && [b[0] + (b[2] - b[0]) * (t.fx ?? .5), b[1] + (b[3] - b[1]) * (t.fy ?? .5)]; }
    else if (t.src != null) {
      const v = view(), i = text().indexOf(t.src);
      if (!v || i < 0) return null;
      const at = i + (t.k ?? 0);
      // the caret goes there and the editor scrolls to it, as a person's click would leave it
      v.dispatch({ selection: { anchor: at }, scrollIntoView: true });
      const c = v.coordsAtPos(at);
      p = c && [c.left + (t.fx ?? 2), (c.top + c.bottom) / 2];
    } else if (t.range) {
      // the end of the first `text` drawn inside `range` (a view's text, no element of its own)
      const w = document.createTreeWalker(document.querySelector(t.range), NodeFilter.SHOW_TEXT);
      for (let n; !p && (n = w.nextNode());) {
        const i = n.data.indexOf(t.text); if (i < 0) continue;
        const r = document.createRange(); r.setStart(n, i + t.text.length - 1); r.setEnd(n, i + t.text.length);
        const b = r.getBoundingClientRect(); if (b.width) p = [b.right, b.top + b.height / 2];
      }
    } else if (t.page != null) {
      const i = $$('#play-pdf .pdf-page img')[t.page - 1], r = i?.getBoundingClientRect();
      p = r && [r.x + r.width * (t.fx ?? .5), r.y + r.height * (t.fy ?? .5)];
    } else if (t.sel) {
      const e = element(t);
      if (!e) return null;
      if (t.scroll !== false) e.scrollIntoView({ block: 'nearest' });
      const r = e.getBoundingClientRect();
      // `left`: that many px in from the left edge, at most half the width (a menu row's label)
      p = [t.left != null ? r.x + Math.min(t.left, r.width / 2) : r.x + r.width * (t.fx ?? .5), r.y + r.height * (t.fy ?? .5)];
    }
    return p && [p[0] + (t.dx ?? 0), p[1] + (t.dy ?? 0)];
  }
  const NAMED = {
    quiet,
    built: () => !document.getElementById('play-pdf')?.classList.contains('pdf-running'),
    ready: () => /^Ready/.test(document.getElementById('play-status')?.textContent ?? ''),
    caret: () => !!document.querySelector('.pdf-caret'),
    dialog: () => !!document.querySelector('dialog.sd-dialog[open]') || !!document.getElementById('play-view')?.open,
    pdf: () => !!document.querySelector('#pdf-body .pdf-page img')?.complete && !!self.satexPdfWordIndex?.()?.words.size,
    // builds: `newBuild` a page drawn from a build other than those `{ mark: 'builds' }` saw, or the same text again
    // from the cache; `oneBuild` every drawn page of one build; `drawn` the first page's image decoded
    newBuild: () => imgs().some((i) => i.__build && (!seen.has(i.__build) || i.__build.text === text() && /in cache/.test(document.getElementById('pdf-status')?.textContent ?? ''))),
    oneBuild: () => { const b = imgs()[0]?.__build; return !!b && imgs().every((i) => !i.__build || i.__build === b); },
    drawn: () => imgs()[0]?.naturalWidth > 100,
    echo: () => !!document.querySelector('.pdf-echo'),
    words: () => !!self.satexPdfWordIndex?.()?.words.size,
  };
  const imgs = () => $$('#play-pdf .pdf-page img');
  let seen = new Set();
  function cond(c) {
    if (typeof c === 'string') return NAMED[c]();
    if (Array.isArray(c)) return c.every(cond);
    if (c.all) return c.all.every(cond);
    if (c.any) return c.any.some(cond);
    if (c.not) return !cond(c.not);
    if (c.sel) return c.count != null ? elements(c).length >= c.count : !!element(c);
    if (c.none) return !elements(c.none).length;
    if (c.src != null) return text().includes(c.src);
    if (c.srcMatch != null) return new RegExp(c.srcMatch).test(text());
    if (c.word != null || c.match != null) return !!wordOf(c);
    if (c.diag) return $$('#lsp-diagnostics li').some((l) => new RegExp(c.diag, 'i').test(l.textContent + (l.dataset.code ?? '') + (l.title ?? '')));
    if (c.open) return !!document.querySelector(c.open)?.open;
    if (c.hidden) return !!document.querySelector(c.hidden)?.hidden;
    if (c.pressed) return document.querySelector(c.pressed[0])?.getAttribute('aria-pressed') === String(c.pressed[1]);
    // the findings shown are of the text as it is now: what a write waits for before it reads them
    // the findings list is of the editor's version, nothing runs or is due
    if (c.findingsCurrent) { const h = self.satexHealth(); return !!h.list && h.list[1] === h.version && !h.listStale && !h.running && !h.due; }
    if (c.shownCurrent) { const h = self.satexHealth(); return h.shown != null && h.shown >= h.version && !h.running && !h.due; }
    if (c.health) { const h = self.satexHealth(); return Object.entries(c.health).every(([k, v]) => h[k] === v); }
    throw new Error(`no condition ${JSON.stringify(c)}`);
  }
  let marks = null;
  async function act(a) {
    const v = view();
    if (a.press) { const e = element(a.press); if (!e) return false; e.click(); return true; }
    if (a.toggle) { const e = element(a.toggle); if (e && (e.getAttribute(a.toggle.attr ?? 'aria-pressed') === 'true') !== a.toggle.want) e.click(); return !!e; }
    if (a.check) { for (const e of elements(a.check)) if (!e.checked) { e.checked = true; e.dispatchEvent(new Event('change', { bubbles: true })); } return true; }
    if (a.inputs) {
      const d = element(a.inputs), find = (n) => [...d.querySelectorAll('input')].find((i) => (i.closest('label')?.textContent ?? i.previousElementSibling?.textContent ?? '').trim() === n);
      for (const [n, x] of Object.entries(a.inputs.values)) { const i = find(n); if (!i) continue; i.value = x; i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new Event('change', { bubbles: true })); }
      return true;
    }
    if (a.tab) { $$('#play-tabs [role=tab]').find((b) => b.textContent.includes(a.tab))?.click(); await sleep(500); return true; }
    if (a.text != null) { v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: a.text } }); return true; }
    if (a.replace) { const i = text().indexOf(a.replace[0]); if (i < 0) return false; v.dispatch({ changes: { from: i, to: i + a.replace[0].length, insert: a.replace[1] } }); return true; }
    if (a.insert) {
      // `pace`: a character at a time, that many ms apart, the caret after each
      const i = text().indexOf(a.insert.at); if (i < 0) return false; let at = i + (a.insert.k ?? 0);
      if (!a.insert.pace) { v.dispatch({ changes: { from: at, insert: a.insert.text }, selection: { anchor: at } }); return true; }
      for (const c of a.insert.text) { v.dispatch({ changes: { from: at, insert: c }, selection: { anchor: at + 1 } }); at++; await sleep(a.insert.pace); }
      return true;
    }
    if (a.caret) { const i = text().indexOf(a.caret.src); if (i < 0) return false; v.focus(); v.dispatch({ selection: { anchor: i + (a.caret.k ?? 0) }, scrollIntoView: true }); return true; }
    if (a.select) { const t = text(), i = t.indexOf(a.select.from), j = t.indexOf(a.select.to) + a.select.to.length; if (i < 0 || j < i) return false; v.focus(); v.dispatch({ selection: { anchor: i, head: j } }); return true; }
    if (a.scroll != null) {
      if (!a.scroll) { v.scrollDOM.scrollTop = 0; return true; }
      const i = text().indexOf(a.scroll.src); v.dispatch({ selection: { anchor: i } }); await sleep(200);
      v.scrollDOM.scrollTop += v.coordsAtPos(i).top - (a.scroll.top ?? 140); await sleep(300); return true;
    }
    if (a.css) { document.head.append(Object.assign(document.createElement('style'), { textContent: a.css })); return true; }
    if (a.size) { const e = document.querySelector(a.size.sel); for (const k of ['width', 'maxWidth', 'height']) if (a.size[k]) e.style[k] = a.size[k]; return true; }
    if ('highlight' in a) {
      clearInterval(marks);
      for (const l of $$('.cm-line')) l.style.background = '';
      if (a.highlight) marks = setInterval(() => { for (const l of $$('.cm-line')) l.style.background = new RegExp(a.highlight.match).test(l.textContent) ? a.highlight.color : ''; }, 100);
      return true;
    }
    if (a.blur) { document.activeElement?.blur(); return true; }
    if (a.focus) { document.querySelector(a.focus)?.focus(); return true; }
    // a key event sent to an element (the editor's Shift+F10 menu: a real key also opens the browser's own)
    if (a.keydown) { const k = a.keydown, e = document.querySelector(k.on ?? '.cm-content'); e?.focus(); e?.dispatchEvent(new KeyboardEvent('keydown', { key: k.key, code: k.key, keyCode: k.keyCode, shiftKey: !!k.shift, bubbles: true, cancelable: true })); return !!e; }
    if (a.fill) { const i = element(a.fill); if (!i) return false; i.value = a.fill.value; i.focus(); return true; }
    if (a.remove) { for (const e of $$(a.remove)) e.remove(); return true; }
    if (a.clear) { const e = document.querySelector(a.clear); if (e) e.textContent = ''; return true; }
    if (a.mark === 'builds') { seen = new Set(imgs().map((i) => i.__build).filter(Boolean)); return seen.size; }
    if (a.close) { for (const e of $$(a.close)) e.close(); return true; }
    if (a.cursor) {
      // a drawn arrow that follows the mouse: a headless screencast shows no pointer
      if (document.getElementById('showcase-cursor')) return true;
      const c = Object.assign(document.createElement('div'), { id: 'showcase-cursor' });
      c.style.cssText = 'position:fixed;left:0;top:0;width:18px;height:24px;z-index:2147483647;pointer-events:none;transform:translate(-40px,-40px)';
      c.innerHTML = '<svg width="18" height="24" viewBox="0 0 18 24"><path d="M1 1v19l5-5 3.5 7.5 3-1.4L9 13.6h7z" fill="#111" stroke="#fff" stroke-width="1.4" stroke-linejoin="round"/></svg>';
      document.documentElement.append(c);
      addEventListener('mousemove', (e) => { c.style.transform = `translate(${e.clientX}px,${e.clientY}px)`; }, true);
      return true;
    }
    if (a.open) return self.satexOpen(a.open);
    throw new Error(`no action ${JSON.stringify(a)}`);
  }
  /** The box of the editor's lines from the one holding `a` to the one holding `b` (a showcase crop). */
  function lines([a, b]) {
    const v = view(), t = text(), x = v.lineBlockAt(t.indexOf(a)), y = v.lineBlockAt(t.indexOf(b)), c = v.contentDOM.getBoundingClientRect();
    const right = Math.min(c.right, v.scrollDOM.getBoundingClientRect().right), top = c.top + x.top - v.documentPadding.top, bottom = c.top + y.bottom - v.documentPadding.top;
    return { left: c.left, right, top, bottom, width: right - c.left };
  }
  /** The union box of crop parts (selectors, { lines }) and `pad`, or null; `cut` where the viewport cuts it. */
  function crop({ parts, pad = 0 }) {
    const rs = parts.flatMap((p) => typeof p === 'string' ? $$(p).filter((e) => e.checkVisibility()).map((e) => e.getBoundingClientRect()) : [lines(p.lines)]).filter((r) => r?.width);
    if (!rs.length) return null;
    const l = Math.min(...rs.map((r) => r.left)) - pad, t = Math.min(...rs.map((r) => r.top)) - pad, r = Math.max(...rs.map((r) => r.right)) + pad, b = Math.max(...rs.map((r) => r.bottom)) + pad;
    const x = Math.max(0, l), y = Math.max(0, t);
    return { x, y, width: Math.min(innerWidth, r) - x, height: Math.min(innerHeight, b) - y, scale: 1, cut: l + pad < -.5 || t + pad < -.5 || r - pad > innerWidth + .5 || b - pad > innerHeight + .5 };
  }
  /** What the page is doing, for a wait that failed. */
  const state = () => { const h = self.satexHealth?.(); return h ? `status "${h.status.slice(0, 60)}", ${h.running} running, ${h.runs} runs${h.queued ? ', one queued' : ''}` : document.getElementById('play-status')?.textContent.slice(0, 60); };
  /** Waits for `c` on animation frames, at most `ms`: true when it held. */
  /** Waits for `c` (held `hold` ms without a break), polled every 50 ms, at most `ms`: true when it held. */
  const until = (c, ms, hold = 0) => new Promise((ok) => {
    const t0 = performance.now();
    let since = null;
    const tick = () => {
      let v = false; try { v = cond(c); } catch {}
      const now = performance.now();
      since = v ? since ?? now : null;
      if (v && now - since >= hold) ok(true); else if (now - t0 > ms) ok(false); else setTimeout(tick, 50);
    };
    tick();
  });
  self.satexTest = {
    run: async (cmd) => {
      if ('point' in cmd) return point(cmd.point);
      if ('cond' in cmd) return cond(cmd.cond);
      if ('until' in cmd) return until(cmd.until, cmd.ms ?? 60000, cmd.hold);
      if ('act' in cmd) return act(cmd.act);
      if ('crop' in cmd) return crop(cmd.crop);
      if ('words' in cmd) { const b = await wordBox(cmd.words); return b && { box: b.slice(0, 4), k: b[4], page: b[5], text: b[6] }; }
      if ('state' in cmd) return state();
      if ('text' in cmd) return text();
      // the shown build: its status line, an error mark, commands that exited non-zero, a page drawn from a new build
      if ('buildState' in cmd) {
        const st = document.getElementById('pdf-status').textContent, err = !!document.querySelector('#play-pdf .pdf-err, #play-pdf [data-view=messages].pdf-err');
        const exit = $$('.pdf-cmds li').map((l) => l.textContent).filter((t) => /exit [1-9]/.test(t)).length, built = imgs().some((x) => x.__build && !seen.has(x.__build));
        return { ok: !err && !exit && built && !/error|fail/i.test(st), status: st.slice(0, 80), err, exit, built };
      }
      if ('health' in cmd) return self.satexHealth();
      // the labels of a target's elements (a menu's rows), for a failure message or a menu's check
      if ('labels' in cmd) return elements(cmd.labels).map(label);
      if ('read' in cmd) return document.querySelector(cmd.read)?.innerText ?? '';
      if ('texts' in cmd) return elements(cmd.texts).map((e) => e.textContent);
      // the editor's caret and selection, the PDF caret's source offset
      if ('carets' in cmd) { const v = view(), s = v.state.selection.main; return { editor: s.head, selected: v.state.sliceDoc(s.from, s.to), pdf: self.satexPdfCaret?.()?.at ?? null }; }
      if ('toast' in cmd) return document.getElementById('play-toast')?.textContent ?? '';
      throw new Error(`no command ${JSON.stringify(cmd)}`);
    },
  };
})();
