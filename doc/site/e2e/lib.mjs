// Browser plumbing for the playground's e2e suite (doc/site/e2e/README.md): one headless Chromium over the DevTools
// protocol, one page per scenario in a fresh browser context, an in-page frame sampler and draw recorders.
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

export const VIEW = { width: 1400, height: 900 };
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const browserPath = () => {
  if (process.env.CHROME) return process.env.CHROME;
  for (const n of ['chromium', 'chromium-browser', 'google-chrome', 'google-chrome-stable']) {
    try { return execFileSync('which', [n], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim(); } catch {}
  }
  const rod = join(homedir(), '.cache/rod/browser');
  const dir = existsSync(rod) && readdirSync(rod).find((d) => existsSync(join(rod, d, 'chrome')));
  if (dir) return join(rod, dir, 'chrome');
  throw new Error('no Chromium found: set CHROME');
};

async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl), pending = new Map(), handlers = new Map();
  let id = 0;
  await new Promise((ok, bad) => { ws.onopen = ok; ws.onerror = bad; });
  ws.onmessage = ({ data }) => {
    const m = JSON.parse(data);
    if (m.id) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.bad(new Error(`${m.error.message}`)) : p.ok(m.result); }
    else for (const h of handlers.get(m.method) ?? []) h(m.params, m.sessionId);
  };
  return {
    send: (method, params = {}, sessionId) => new Promise((ok, bad) => { pending.set(++id, { ok, bad }); ws.send(JSON.stringify({ id, method, params, sessionId })); }),
    on: (method, h) => handlers.set(method, [...(handlers.get(method) ?? []), h]),
    close: () => ws.close(),
  };
}

// Runs before the page's scripts: echo canvases remember what was drawn into them (text, font), and a
// frame sampler records the Type-mode overlay on every animation frame while `__e2e.on`.
const PRELUDE = `(() => {
  // dev reloads from other helpers' edits would restart a scenario: /__dev/events never fires here
  const ES = self.EventSource; self.EventSource = function (u, ...r) { return /__dev\\/events/.test(String(u)) ? { close() {}, set onmessage(_) {}, addEventListener() {} } : new ES(u, ...r); };
  const fill = CanvasRenderingContext2D.prototype.fillText;
  CanvasRenderingContext2D.prototype.fillText = function (t, x, y, ...r) {
    // where the glyph lands on the canvas under the context's transform (a stretched delimiter, a scaled word), its height's scale
    const c = this.canvas, m = this.getTransform(); (c.__texts ??= []).push({ t: String(t), font: this.font, x: m.a * x + m.c * y + m.e, y: m.b * x + m.d * y + m.f, sy: m.d }); return fill.call(this, t, x, y, ...r);
  };
  const E = self.__e2e = { on: false, frames: [], keys: 0, keyLog: [] };
  // the language server's inputs, for a crash repro: each text sent (didOpen, didChange) and each request, timed
  E.lsp = [];
  const post = Worker.prototype.postMessage;
  Worker.prototype.postMessage = function (m, ...r) {
    try {
      const msg = m?.message;
      if (msg?.method) {
        const d = msg.params?.textDocument, text = d?.text ?? msg.params?.contentChanges?.[0]?.text;
        E.lsp.push({ t: performance.now(), method: msg.method, uri: d?.uri, v: d?.version, ...(text != null ? { text } : {}) });
        if (E.lsp.length > 400) E.lsp.splice(0, 100);
      }
    } catch {}
    return post.call(this, m, ...r);
  };
  // each key's latency to the next frame after it: what the person waits to see the key
  E.keyFrame = [];
  addEventListener('keydown', (e) => { if (E.on) { E.keys++; const t = performance.now(); E.keyLog.push([t, e.key]); requestAnimationFrame(() => requestAnimationFrame(() => E.keyFrame.push(performance.now() - t))); } }, true);
  const rect = (r) => r && [Math.round(r.left * 10) / 10, Math.round(r.top * 10) / 10, Math.round(r.width * 10) / 10, Math.round(r.height * 10) / 10];
  const tick = () => {
    if (E.on && !E.quiet) {
      const caret = document.querySelector('#play-pdf .pdf-caret, #play-typeset .pdf-caret'), sc = document.querySelector('#play-pdf .pdf-pages');
      const echoes = [...document.querySelectorAll('.pdf-echo:not(.pdf-push, .pdf-erased)')].map((e) => {
        const cs = getComputedStyle(e), r = e.getBoundingClientRect();
        // the clicked word measured in each echo font now, while the page's embedded faces are loaded: its width on screen
        // a page's embedded face (pdf.js's g_…) counts only while loaded: a closed PDF's face falls back to another font
        const up = (f) => { const g = /px\\s+'?"?(g_[\\w]+)/.exec(f)?.[1]; return !g || [...document.fonts].some((x) => x.status === 'loaded' && x.family.replace(/^["']|["']$/g, '') === g); };
        const fit = (fonts, scale) => fonts.map((f) => { (E.ctx ??= document.createElement('canvas').getContext('2d')).font = f; return E.word && up(f) ? E.ctx.measureText(E.word).width * scale : 0; });
        if (e.tagName === 'CANVAS') { const fonts = [...new Set((e.__texts ?? []).map((q) => q.font))], scale = r.width / Math.max(1, e.width); return { kind: 'canvas', r: rect(r), scale, text: (e.__texts ?? []).map((q) => q.t).join(' '), at: (e.__texts ?? []).map((q) => [q.t, Math.round((r.left + q.x * scale) * 10) / 10]), fonts, fits: fit(fonts, scale) }; }
        const inner = e.querySelector('span') ?? e, ics = getComputedStyle(inner), fonts = [(ics.fontStyle === 'normal' ? '' : ics.fontStyle + ' ') + ics.fontWeight + ' ' + ics.fontSize + ' ' + ics.fontFamily];
        return { kind: 'span', r: rect(r), text: [...e.querySelectorAll('i')].map((i) => i.textContent).join(' ') || e.textContent, fonts, fits: fit(fonts, 1) };
      });
      const imgs = [...document.querySelectorAll('#play-pdf .pdf-page img')].map((i) => [+i.closest('.pdf-page').dataset.page, (i.currentSrc || '').slice(-24), i.naturalWidth, i.__build?.text?.length ?? -1]);
      const pc = self.satexPdfCaret?.();
      // the build's words around the caret's row the echo does not cover: what the row shows besides the echo
      let rowWords = '';
      const pg = caret?.closest('.pdf-page'), img = pg?.querySelector('img'), b = img?.__build;
      if (b?.wordList && img.naturalWidth) {
        const ir = img.getBoundingClientRect(), k = ir.width / (img.naturalWidth * 72 / b.dpi), cr = caret.getBoundingClientRect(), mid = cr.top + cr.height / 2;
        const covers = [...pg.querySelectorAll('.pdf-echo:not(.pdf-push)')].map((e) => e.getBoundingClientRect());
        rowWords = b.wordList.filter((w) => w[0] === +pg.dataset.page && Math.abs(ir.top + (w[2] + w[4]) / 2 * k - mid) < cr.height * 1.6).filter((w) => {
          const x0 = ir.left + w[1] * k, x1 = ir.left + w[3] * k, y = ir.top + (w[2] + w[4]) / 2 * k;
          return !covers.some((c) => x0 >= c.left - 1 && x1 <= c.right + 1 && y >= c.top && y <= c.bottom);
        }).map((w) => w[5]).join(' ');
      }
      E.frames.push({ t: performance.now(), keys: E.keys, caret: caret && rect(caret.getBoundingClientRect()), caretPage: caret?.closest('.pdf-page')?.dataset.page ?? null,
        at: pc?.at ?? null, echoes, rowWords, scroll: sc ? Math.round(sc.scrollTop) : null, imgs, running: !!document.querySelector('#play-pdf.pdf-running'), toast: document.getElementById('play-toast')?.textContent?.slice(0, 120) ?? '' });
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  // the invariant, every frame of every scenario: no overlay (tint, chip, layer, caret, echo) seen over a page whose
  // image is not drawn (missing, not decoded, the blank placeholder of a page not drawn yet)
  E.bare = []; E.bareFrames = 0;
  const bare = () => {
    let hit = false;
    for (const pg of document.querySelectorAll('#play-pdf .pdf-page')) {
      const img = pg.querySelector(':scope > img');
      if (img && img.complete && img.naturalWidth > 0 && !/^data:image\\/svg/.test(img.currentSrc || img.src)) continue;
      for (const el of pg.querySelectorAll(':scope > :not(img), :scope > :not(img) *')) {
        const r = el.getBoundingClientRect();
        if (r.width * r.height < 1 || !el.checkVisibility({ visibilityProperty: true, opacityProperty: true })) continue;
        hit = true;
        if (E.bare.length < 20) E.bare.push({ t: Math.round(performance.now()), page: pg.dataset.page, el: (el.tagName + '.' + el.className).slice(0, 60), img: img ? (img.currentSrc || img.src).slice(0, 30) + ' ' + img.naturalWidth : 'none' });
        break;
      }
    }
    if (hit) E.bareFrames++;
    requestAnimationFrame(bare);
  };
  requestAnimationFrame(bare);
})();`;

// at draw time (a build landing later closes the face): a page face (pdf.js `g_…`) drawing a text that lacks one of
// its glyphs, so the browser takes those letters from the next family: two fonts in one word (`canvas.__mixed`)
const MIXED = `(() => { const fill = CanvasRenderingContext2D.prototype.fillText, c = document.createElement('canvas').getContext('2d');
  const has = (fam, ch) => { const at = (fb) => { c.font = '20px ' + fam + ', ' + fb; return c.measureText(ch).width; }; return at('monospace') === at('serif'); };
  CanvasRenderingContext2D.prototype.fillText = function (t, ...r) { const g = /(g_\\w+)/.exec(this.font)?.[1]; if (self.__e2eMixed && g && [...String(t)].some((ch) => /\\S/.test(ch) && !has(g, ch))) (this.canvas.__mixed ??= []).push(String(t)); return fill.call(this, t, ...r); };
})();`;

export async function browser(outDir) {
  const profile = join(outDir, 'profile');
  rmSync(profile, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });
  const chrome = spawn(browserPath(), ['--headless=new', '--no-sandbox', '--hide-scrollbars', '--remote-debugging-port=0', '--disable-gpu',
    `--user-data-dir=${profile}`, `--window-size=${VIEW.width},${VIEW.height}`, 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
  const wsUrl = await new Promise((ok, bad) => {
    let log = '';
    chrome.stderr.on('data', (d) => { log += d; const m = /DevTools listening on (ws:\/\/\S+)/.exec(log); if (m) ok(m[1]); });
    chrome.on('exit', () => bad(new Error(`the browser stopped:\n${log}`)));
  });
  const cdp = await connect(wsUrl);
  return { cdp, close: () => { try { cdp.close(); } catch {} chrome.kill(); }, pid: chrome.pid };
}

/** One page in a fresh context. */
export async function page(cdp, url, outDir) {
  const { browserContextId } = await cdp.send('Target.createBrowserContext');
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank', browserContextId });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  const s = (method, params) => cdp.send(method, params, sessionId);
  const errors = [];
  cdp.on('Runtime.exceptionThrown', (p, sid) => { if (sid === sessionId) errors.push(p.exceptionDetails?.exception?.description?.split('\n')[0] ?? p.exceptionDetails?.text); });
  const consoleLog = [];
  cdp.on('Runtime.consoleAPICalled', (p, sid) => { if (sid === sessionId && /warn|error/.test(p.type)) consoleLog.push(p.args.map((x) => x.value ?? x.description ?? '').join(' ').slice(0, 200)); });
  // the page's workers (the language server's among them): their console and uncaught errors, timed
  // `blocked`: URL patterns no request of the page or its workers reaches (the workers fetch on their own)
  const workerLog = [], kids = new Set(), t0 = Date.now(), blocked = ['*/__dev/events*'];
  cdp.on('Target.attachedToTarget', (p, sid) => {
    if (sid !== sessionId) return;
    const k = p.sessionId;
    kids.add(k);
    cdp.send('Network.enable', {}, k).then(() => cdp.send('Network.setBlockedURLs', { urls: blocked }, k)).catch(() => {});
    cdp.send('Runtime.enable', {}, k).catch(() => {});
    cdp.send('Runtime.runIfWaitingForDebugger', {}, k).catch(() => {});
  });
  cdp.on('Runtime.consoleAPICalled', (p, sid) => { if (kids.has(sid)) workerLog.push(`${Date.now() - t0} ${p.type} ${p.args.map((x) => x.value ?? x.description ?? '').join(' ').slice(0, 400)}`); });
  cdp.on('Runtime.exceptionThrown', (p, sid) => { if (kids.has(sid)) workerLog.push(`${Date.now() - t0} exception ${p.exceptionDetails?.exception?.description ?? p.exceptionDetails?.text}`.slice(0, 2000)); });
  await s('Page.enable');
  await s('Runtime.enable');
  await s('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
  await s('Emulation.setDeviceMetricsOverride', { ...VIEW, deviceScaleFactor: 1, mobile: false });
  await s('Network.enable');
  await s('Network.setBlockedURLs', { urls: blocked });
  await s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
  await s('Page.addScriptToEvaluateOnNewDocument', { source: PRELUDE + MIXED });
  await s('Page.navigate', { url });
  const api = {
    s, errors, consoleLog, workerLog, t0,
    /** Blocks `urls` too, for the page and every worker, those running and those to come. */
    block: async (urls) => {
      blocked.push(...urls);
      await s('Network.setBlockedURLs', { urls: blocked });
      for (const k of kids) await cdp.send('Network.setBlockedURLs', { urls: blocked }, k).catch(() => {});
    },
    eval: async (expression) => {
      const r = await s('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(`${expression.slice(0, 120)}: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}`);
      return r.result.value;
    },
    /** Waits until `expression` is truthy, polled on animation frames; resolves to null on timeout. */
    until: (expression, ms = 60000) => api.eval(`new Promise((ok) => { const t0 = performance.now(); const tick = () => { let v; try { v = ${expression}; } catch {} if (v) ok(v); else if (performance.now() - t0 > ${ms}) ok(null); else setTimeout(tick, 30); }; tick(); })`),
    mouse: async (x, y, clicks = 1) => {
      await s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      for (const type of ['mousePressed', 'mouseReleased']) await s('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: clicks });
    },
    click: async (selector) => {
      const at = await api.eval(`(() => { const e = document.querySelector(${JSON.stringify(selector)}); if (!e) return null; e.scrollIntoView({ block: 'nearest' }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
      if (!at) throw new Error(`no ${selector}`);
      await api.mouse(...at);
    },
    /** One key as a person presses it: a character, or a named key. */
    key: async (k) => {
      // `Ctrl+Backspace`: the named key with Ctrl held
      const ctrl = /^Ctrl\+/.test(k) ? 2 : 0; k = k.replace(/^Ctrl\+/, '');
      const named = { Backspace: [8, 'Backspace'], Enter: [13, 'Enter'], Escape: [27, 'Escape'], ArrowLeft: [37, 'ArrowLeft'], ArrowRight: [39, 'ArrowRight'], Tab: [9, 'Tab'], Delete: [46, 'Delete'] }[k];
      if (named) {
        await s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: k, code: named[1], windowsVirtualKeyCode: named[0], modifiers: ctrl });
        await s('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code: named[1], windowsVirtualKeyCode: named[0], modifiers: ctrl });
      } else {
        await s('Input.dispatchKeyEvent', { type: 'keyDown', text: k, key: k, unmodifiedText: k });
        await s('Input.dispatchKeyEvent', { type: 'keyUp', key: k });
      }
    },
    shot: async (file, clip, scale = 1) => {
      const r = await s('Page.captureScreenshot', { format: 'png', ...(clip ? { clip: { ...clip, scale } } : {}) });
      writeFileSync(join(outDir, file), Buffer.from(r.data, 'base64'));
      return join(outDir, file);
    },
    close: () => cdp.send('Target.closeTarget', { targetId }).then(() => cdp.send('Target.disposeBrowserContext', { browserContextId })).catch(() => {}),
  };
  await api.until(`document.readyState === 'complete'`, 30000);
  return api;
}
