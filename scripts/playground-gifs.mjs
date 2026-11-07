#!/usr/bin/env node
// Records one short GIF per playground feature into doc/site/gifs/<feature>.gif.
//
//   scripts/playground-gifs.mjs [feature ...]       (all features when none given)
//
// Needs node (22+, for the global WebSocket), a Chromium with a headless mode, ffmpeg and a running
// playground. It drives Chromium over the DevTools protocol, no npm packages. Environment:
//   PLAY_URL  an already running playground (default: starts `cargo playground --dev` on PLAY_PORT)
//   CHROME    the browser binary (default: google-chrome, chromium, or the copy vhs keeps in ~/.cache/rod)
//   GIF_DIR   output folder (default doc/site/gifs), GIF_WIDTH (default 900)
// Waits are on DOM states, never on fixed sleeps. A fresh browser context per feature, a fixed 1280x800 viewport.
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const out = resolve(process.env.GIF_DIR ?? join(root, 'doc/site/gifs'));
const scratch = join(root, 'target/scratch/playground-gifs');
const WIDTH = +(process.env.GIF_WIDTH ?? 900), MAX_BYTES = 1.5 * 2 ** 20, VIEW = { width: 1280, height: 800 };
const DOC = String.raw`\documentclass{article}
\newcommand{\greet}[1]{Hello, #1!}
\begin{document}
\section{Intro}
\greet{world} This is is a test.
\end{document}
`;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const browserPath = () => {
  if (process.env.CHROME) return process.env.CHROME;
  for (const n of ['google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser']) {
    try { return execFileSync('which', [n], { encoding: 'utf8' }).trim(); } catch {}
  }
  const rod = join(homedir(), '.cache/rod/browser');
  const dir = existsSync(rod) && readdirSync(rod).find((d) => existsSync(join(rod, d, 'chrome')));
  if (dir) return join(rod, dir, 'chrome');
  throw new Error('no Chromium found: set CHROME');
};

/** Starts the playground unless PLAY_URL names one. Returns [url, stop]. */
async function server() {
  if (process.env.PLAY_URL) return [process.env.PLAY_URL, () => {}];
  const port = process.env.PLAY_PORT ?? '8137';
  const child = spawn('cargo', ['playground', '--dev', '--port', port], { cwd: root, stdio: 'ignore' });
  const url = `http://127.0.0.1:${port}/play/`;
  for (let i = 0; i < 600; i++) {
    if (await fetch(url).then((r) => r.ok).catch(() => false)) return [url, () => child.kill()];
    await sleep(500);
  }
  child.kill();
  throw new Error('the playground did not come up');
}

/** A DevTools connection: `send(method, params, sessionId)` and `on(method, handler)`. */
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

/** One page in a fresh context: evaluate, wait on conditions, click, type, record. */
async function page(cdp, url) {
  const { browserContextId } = await cdp.send('Target.createBrowserContext');
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank', browserContextId });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  const s = (method, params) => cdp.send(method, params, sessionId);
  await s('Page.enable');
  await s('Runtime.enable');
  await s('Emulation.setDeviceMetricsOverride', { ...VIEW, deviceScaleFactor: 1, mobile: false });
  await s('Page.navigate', { url });
  const frames = [];
  cdp.on('Page.screencastFrame', (p, sid) => {
    if (sid !== sessionId) return;
    frames.push({ t: p.metadata.timestamp, data: p.data });
    s('Page.screencastFrameAck', { sessionId: p.sessionId }).catch(() => {});
  });
  const api = {
    eval: async (expression) => {
      const r = await s('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(`${expression}: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}`);
      return r.result.value;
    },
    /** Waits until `expression` is truthy, polling on animation frames. */
    until: (expression, what = expression, ms = 120000) => api.eval(`new Promise((ok, bad) => { const t0 = performance.now(); const tick = () => { let v; try { v = ${expression}; } catch {} if (v) ok(v); else if (performance.now() - t0 > ${ms}) bad(new Error('timeout: ${what.replace(/['\\\n]/g, ' ')}')); else requestAnimationFrame(tick); }; tick(); })`),
    click: async (selector) => {
      const at = await api.eval(`(() => { const e = document.querySelector(${JSON.stringify(selector)}); if (!e) return null; e.scrollIntoView({ block: 'nearest' }); const r = e.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
      if (!at) throw new Error(`no ${selector}`);
      await api.mouse(...at, true);
    },
    mouse: async (x, y, click = false) => {
      await s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      if (click) for (const type of ['mousePressed', 'mouseReleased']) await s('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 });
    },
    /** The middle of the first `text` in the editor's lines. */
    textAt: (text) => api.eval(`(() => { for (const l of document.querySelectorAll('.cm-line')) { const i = l.textContent.indexOf(${JSON.stringify(text)}); if (i < 0) continue;
      const w = document.createTreeWalker(l, NodeFilter.SHOW_TEXT); let n, k = i; while ((n = w.nextNode())) { if (k < n.length) { const r = document.createRange(); r.setStart(n, k); r.setEnd(n, Math.min(k + ${text.length}, n.length)); const b = r.getBoundingClientRect(); return [b.x + b.width / 2, b.y + b.height / 2]; } k -= n.length; } } return null; })()`),
    key: async (key, code, modifiers = 0) => { for (const type of ['keyDown', 'keyUp']) await s('Input.dispatchKeyEvent', { type, key, code, modifiers }); },
    /** Typed like a person, one key at a time. */
    type: async (text) => { for (const ch of text) { await s('Input.dispatchKeyEvent', { type: 'keyDown', text: ch, key: ch }); await s('Input.dispatchKeyEvent', { type: 'keyUp', key: ch }); await sleep(45); } },
    /** Replaces the editor's text. */
    setSource: async (text) => {
      await api.click('.cm-content');
      await api.key('a', 'KeyA', 2);
      await s('Input.insertText', { text });
    },
    ready: () => api.until(`document.querySelector('#play-status')?.dataset.done === 'ready' && !document.querySelector('#play-status')?.hasAttribute('data-busy')`, 'the playground is ready'),
    analyzed: () => api.until(`document.querySelector('#lsp-diagnostics li')`, 'findings'),
    record: () => s('Page.startScreencast', { format: 'jpeg', quality: 70, maxWidth: VIEW.width, maxHeight: VIEW.height }),
    stop: async () => { await s('Page.stopScreencast'); await sleep(100); },
    frames,
    close: () => cdp.send('Target.closeTarget', { targetId }).then(() => cdp.send('Target.disposeBrowserContext', { browserContextId })).catch(() => {}),
  };
  await api.until(`document.readyState === 'complete'`);
  return api;
}

/** The recorded frames as a GIF under `out`, shrunk until it fits. The last frame holds a moment. */
function gif(name, frames) {
  if (frames.length < 2) throw new Error(`${name}: no frames`);
  const dir = join(scratch, name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const list = frames.map((f, i) => {
    writeFileSync(join(dir, `${i}.jpg`), Buffer.from(f.data, 'base64'));
    const next = frames[i + 1]?.t ?? f.t + 1.2;
    return `file '${i}.jpg'\nduration ${Math.max(next - f.t, 0.04).toFixed(3)}`;
  });
  list.push(`file '${frames.length - 1}.jpg'`);
  writeFileSync(join(dir, 'list.txt'), list.join('\n'));
  mkdirSync(out, { recursive: true });
  const target = join(out, `${name}.gif`);
  for (const [w, fps, colors] of [[WIDTH, 12, 64], [WIDTH * 0.8, 10, 48], [WIDTH * 0.65, 8, 32]]) {
    execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', join(dir, 'list.txt'), '-vf',
      `fps=${fps},scale=${Math.round(w)}:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=${colors}:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle`, '-loop', '0', target]);
    if (statSync(target).size <= MAX_BYTES) break;
  }
  return statSync(target).size;
}

const FEATURES = {
  /** Compile, then the PDF view with its pages. */
  async compile(p) {
    await p.setSource(DOC);
    await p.click('[data-panel="play-pdf"]');
    await p.until(`!document.querySelector('#pdf-compile').disabled`, 'the compile server');
    await p.record();
    await p.click('#pdf-compile');
    await p.until(`document.querySelector('.pdf-page img')?.complete && document.querySelector('.pdf-page img').naturalWidth`, 'a page');
    await p.until(`document.querySelector('#pdf-status a')`, 'the title line');
  },
  /** Slice at the cursor: the badge and the separator mark it. */
  async slice(p) {
    await p.setSource(DOC);
    await p.click('[data-panel="play-pdf"]');
    await p.until(`!document.querySelector('#pdf-slice').disabled`, 'the compile server');
    const at = await p.textAt('Hello');
    await p.click('#pdf-more summary');
    await p.record();
    await p.click('#pdf-slice');
    await p.until(`document.querySelector('.pdf-sep') && document.querySelector('.pdf-page img')?.naturalWidth`, 'the slice');
    void at;
  },
  /** The hover card of a command. */
  async hover(p) {
    await p.setSource(DOC);
    await p.analyzed();
    const [x, y] = await p.textAt('\\greet{world}');
    await p.record();
    await p.mouse(x - 20, y - 40);
    await p.mouse(x, y);
    await p.until(`document.querySelector('.tip-card:not([hidden])')?.textContent.trim()`, 'the hover card');
  },
  /** Completion of a command name while typing. */
  async completion(p) {
    await p.setSource(DOC);
    await p.analyzed();
    const [x, y] = await p.textAt('This');
    await p.mouse(x, y, true);
    await p.key('End', 'End');
    await p.record();
    await p.type(' \\textb');
    await p.until(`document.querySelector('.cm-tooltip-autocomplete li')`, 'the completion list');
    await p.key('Tab', 'Tab');
    await p.until(`document.querySelector('.cm-content').textContent.includes('\\\\textbf')`, 'the completed name');
  },
  /** Fix all: the findings go, the source changes. */
  async fixall(p) {
    await p.setSource(DOC);
    await p.analyzed();
    await p.record();
    await p.click('#fix-all');
    await p.until(`!document.querySelector('.cm-content').textContent.includes('is is')`, 'the fix applied');
    await p.until(`!document.querySelector('#lsp-diagnostics li')`, 'the findings gone');
  },
  /** The typeset view beside the source. */
  async typeset(p) {
    await p.setSource(DOC);
    await p.analyzed();
    await p.record();
    await p.click('[data-panel="play-typeset"]');
    await p.until(`document.querySelector('#typeset-body p.ts')`, 'the typeset paragraphs');
    await p.click('#typeset-envs');
    await p.click('#typeset-keys');
  },
};

const wanted = process.argv.slice(2);
for (const w of wanted) if (!(w in FEATURES)) { console.error(`unknown feature ${w}. Known: ${Object.keys(FEATURES).join(', ')}`); process.exit(2); }
mkdirSync(scratch, { recursive: true });
const [url, stopServer] = await server();
const profile = join(scratch, 'profile');
rmSync(profile, { recursive: true, force: true });
const chrome = spawn(browserPath(), ['--headless=new', '--no-sandbox', '--disable-gpu', '--hide-scrollbars', '--remote-debugging-port=0',
  `--user-data-dir=${profile}`, `--window-size=${VIEW.width},${VIEW.height}`, 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
const wsUrl = await new Promise((ok, bad) => {
  let log = '';
  chrome.stderr.on('data', (d) => { log += d; const m = /DevTools listening on (ws:\/\/\S+)/.exec(log); if (m) ok(m[1]); });
  chrome.on('exit', () => bad(new Error(`the browser stopped:\n${log}`)));
});
const cdp = await connect(wsUrl);
let failed = 0;
try {
  for (const name of wanted.length ? wanted : Object.keys(FEATURES)) {
    const p = await page(cdp, url);
    try {
      await p.ready();
      await FEATURES[name](p);
      await p.stop();
      console.log(`${name}: ${(gif(name, p.frames) / 1024).toFixed(0)} KiB, ${p.frames.length} frames`);
    } catch (e) { failed++; console.error(`${name}: ${e.message}`); }
    await p.close();
  }
} finally {
  cdp.close();
  chrome.kill();
  stopServer();
}
process.exit(failed ? 1 : 0);
