// How long the playground's actions take in a headless Chromium: the median over several tries of the first
// diagnostics after opening an example, and of a hover, a completion, the explain panel's query, the typeset
// pane's query and a code action, each for thesis.tex and hello.tex. Also how many `run` messages the worker got.
// No build lock; give it at most 300 s: `timeout 300 node doc/site/latency.mjs http://localhost:8000/`.
import { existsSync, mkdtempSync, readdirSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';

const base = process.argv[2] ?? 'http://localhost:8000/';
const TRIES = Number(process.env.TRIES ?? 5);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function chromium() {
  if (process.env.CHROME) return process.env.CHROME;
  const root = join(homedir(), '.cache/ms-playwright');
  const dirs = existsSync(root) ? readdirSync(root).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse() : [];
  return dirs.map((d) => ['chrome-linux64/chrome', 'chrome-linux/chrome'].map((c) => join(root, d, c)).find(existsSync)).find(Boolean);
}

const chrome = chromium();
if (!chrome) { console.log('skip: no Chromium (set CHROME)'); process.exit(0); }
const port = 9300 + Math.floor(Math.random() * 600);
const proc = spawn(chrome, ['--headless=new', '--no-sandbox', '--disable-gpu', `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), 'satex-latency-'))}`,
  '--window-size=1440,1000', 'about:blank'], { stdio: 'ignore' });
try {
  let target;
  for (let i = 0; i < 100 && !target; i++) { await sleep(200); try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((x) => x.type === 'page'); } catch {} }
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => { ws.onopen = r; });
  let id = 0;
  const replies = new Map();
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id) replies.get(m.id)?.(m); };
  const send = (method, params = {}) => new Promise((r) => { replies.set(++id, r); ws.send(JSON.stringify({ id, method, params })); });
  const js = async (expr) => {
    const r = await send('Runtime.evaluate', { expression: `(async () => { ${expr} })()`, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(`${expr.slice(0, 80)}: ${r.result.exceptionDetails.exception?.description}`);
    return r.result?.result?.value;
  };
  const until = async (expr, ms) => { for (const end = Date.now() + ms; Date.now() < end; await sleep(200)) if (await js(expr).catch(() => false)) return true; return false; };

  // The page's worker, seen from outside: every message posted to it counted, its replies timed.
  await send('Page.enable');
  await send('Page.addScriptToEvaluateOnNewDocument', { source: `
    window.__posted = {};
    const post = Worker.prototype.postMessage;
    Worker.prototype.postMessage = function (m, t) { window.__worker = this; if (m?.type) window.__posted[m.type] = (window.__posted[m.type] ?? 0) + 1; return post.call(this, m, t); };
    window.__n = 0;
    /** ms until the worker answers the request. */
    window.__ask = (method, params) => new Promise((res) => {
      const mid = 900000 + ++window.__n, t = performance.now();
      const h = (e) => { if (e.data.type === 'lsp' && e.data.out.some((m) => m.id === mid)) { window.__worker.removeEventListener('message', h); res(performance.now() - t); } };
      window.__worker.addEventListener('message', h);
      window.__worker.postMessage({ type: 'lsp', message: { jsonrpc: '2.0', id: mid, method, params } });
    });
    /** ms from opening the example until the first diagnostics of its file arrive. */
    window.__open = (name) => new Promise((res) => {
      const uri = 'file:///work/' + name, t = performance.now();
      const h = (e) => { if (e.data.type === 'lsp' && e.data.out.some((m) => m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri)) { window.__worker.removeEventListener('message', h); res(performance.now() - t); } };
      window.__worker.addEventListener('message', h);
      document.querySelector('[data-example="' + name + '"]').click();
    });` });
  await send('Page.navigate', { url: base + 'play/index.html' });
  if (!await until(`return document.getElementById('play-status').dataset.done === 'ready'`, 120000)) { console.log('the playground did not get ready'); process.exit(1); }

  console.log('playground ready');
  const median = (xs) => xs.length ? [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] : NaN;
  const names = ['hello.tex', 'thesis.tex'];
  const rows = {};
  const deadline = Date.now() + 270000;
  for (const [i, name] of names.entries()) {
    const other = names[1 - i];
    const uri = `file:///work/${name}`, doc = { uri };
    const times = { 'first diagnostics': [], hover: [], completion: [], 'explain panel': [], 'typeset pane': [], codeAction: [] };
    for (let t = 0; t < TRIES && Date.now() < deadline; t++) {
      await js(`await window.__open(${JSON.stringify(other)}); return 0`);
      times['first diagnostics'].push(await js(`return await window.__open(${JSON.stringify(name)})`));
      const at = { line: 0, character: 3 };
      const ask = (method, params) => js(`return await window.__ask(${JSON.stringify(method)}, ${JSON.stringify(params)})`);
      times.hover.push(await ask('textDocument/hover', { textDocument: doc, position: at }));
      times.completion.push(await ask('textDocument/completion', { textDocument: doc, position: { line: 0, character: 5 } }));
      times['explain panel'].push(await ask('workspace/executeCommand', { command: 'satex/query', arguments: [{ file: `/work/${name}`, queries: [{ type: 'explain', names: ['\\documentclass'] }] }] }));
      times['typeset pane'].push(await ask('satex/typeset', { textDocument: doc }));
      times.codeAction.push(await ask('textDocument/codeAction', { textDocument: doc, range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, context: { diagnostics: [] } }));
    }
    console.log(name, JSON.stringify(times));
    rows[name] = Object.fromEntries(Object.entries(times).map(([k, v]) => [k, v.length ? `${Math.round(median(v))} ms (n=${v.length})` : 'no sample']));
  }
  console.table(rows);
  console.log('messages posted to the worker:', JSON.stringify(await js(`return window.__posted`)));
} finally {
  proc.kill();
}
process.exit(0);
