// Stress checks of the playground in a headless Chromium, against a built site:
// example switches during runs, typing and undo storms, Compile while analyzing,
// slow and failing downloads, a killed worker, evicted caches, a reload mid-run,
// two tabs. After each, the page must answer within a bound, end idle, and show
// findings, PDF and text of the project open now.
//   node doc/site/chaos.mjs http://127.0.0.1:8013/        (ONLY=switch,typing to pick)
// Env: CHROME, SHOTS=dir for screenshots, SETTLE_MS (default 300000), TRACE=1 for the steps, TMPDIR for the browser profile.
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';

const base = process.argv[2] ?? 'http://127.0.0.1:8013/';
const play = `${base}play/`;
const SETTLE = +(process.env.SETTLE_MS ?? 300000), ANSWER = 3000, SILENT = 45000;
const only = process.env.ONLY?.split(',');
const examples = new URL('examples/', import.meta.url);
const firstLine = (f) => readFileSync(new URL(f, examples), 'utf8').split('\n')[0];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail = '') => { results.push({ name, ok }); console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? `: ${detail}` : ''}`); };

function chromium() {
  if (process.env.CHROME) return process.env.CHROME;
  const root = join(homedir(), '.cache/ms-playwright');
  const dirs = existsSync(root) ? readdirSync(root).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse() : [];
  return dirs.map((d) => ['chrome-linux64/chrome', 'chrome-linux/chrome'].map((c) => join(root, d, c)).find(existsSync)).find(Boolean);
}
const chrome = chromium();
if (!chrome) { console.log('skip: no Chromium (set CHROME)'); process.exit(0); }
const profile = mkdtempSync(join(tmpdir(), 'satex-chaos-'));
const proc = spawn(chrome, ['--headless=new', '--no-sandbox', '--disable-gpu', '--remote-debugging-pipe', `--user-data-dir=${profile}`, '--window-size=1440,1000', 'about:blank'],
  { stdio: ['ignore', 'ignore', process.env.CHROME_LOG ? 'inherit' : 'ignore', 'pipe', 'pipe'] });
let id = 0, rest = '';
const replies = new Map(), events = [];
const raw = (method, params = {}, sessionId) => new Promise((r) => { replies.set(++id, r); proc.stdio[3].write(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }) + '\0'); });
proc.stdio[4].on('data', (chunk) => {
  const parts = (rest + chunk.toString()).split('\0');
  rest = parts.pop();
  for (const part of parts) {
    if (!part) continue;
    const m = JSON.parse(part);
    if (m.id) replies.get(m.id)?.(m); else for (const f of events) f(m);
  }
});

/** One tab: its session and helpers. */
async function tab(first) {
  let target;
  if (first) for (let i = 0; i < 100 && !target; i++) { await sleep(100); target = (await raw('Target.getTargets')).result?.targetInfos?.find((t) => t.type === 'page')?.targetId; }
  else target = (await raw('Target.createTarget', { url: 'about:blank' })).result.targetId;
  const session = (await raw('Target.attachToTarget', { targetId: target, flatten: true })).result.sessionId;
  const send = (method, params = {}) => raw(method, params, session);
  const errors = [];
  events.push((m) => { if (m.sessionId === session && m.method === 'Runtime.exceptionThrown') errors.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text); });
  await send('Runtime.enable');
  const js = async (expr) => {
    const r = await send('Runtime.evaluate', { expression: `(async () => { ${expr} })()`, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(`${expr.slice(0, 80)}: ${r.result.exceptionDetails.exception?.description}`);
    return r.result?.result?.value;
  };
  /** The page's health; a main thread that does not answer for 10 s is stuck: where it runs is printed. */
  const health = async () => {
    const r = await Promise.race([js('return self.satexHealth?.() ?? null').catch(() => null), sleep(10000).then(() => 'stuck')]);
    if (r !== 'stuck') return r;
    await send('Debugger.enable');
    const paused = new Promise((done) => events.push((m) => { if (m.sessionId === session && m.method === 'Debugger.paused') done(m.params.callFrames); }));
    await send('Debugger.pause');
    const frames = await Promise.race([paused, sleep(5000).then(() => [])]);
    check('main thread answers', false, frames.slice(0, 8).map((f) => `${f.functionName || '?'}:${f.location.lineNumber + 1}`).join(' < '));
    await send('Debugger.resume'); await send('Debugger.disable');
    throw new Error('the page is stuck');
  };
  const key = async (key, modifiers = 0, code = `Key${key.toUpperCase()}`) => {
    for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key, code, modifiers, windowsVirtualKeyCode: { End: 35, Home: 36 }[key] ?? key.toUpperCase().charCodeAt(0) });
  };
  const shot = async (name) => {
    if (!process.env.SHOTS) return;
    mkdirSync(process.env.SHOTS, { recursive: true });
    const r = await send('Page.captureScreenshot', { format: 'png' });
    if (r.result) writeFileSync(join(process.env.SHOTS, `${name}.png`), Buffer.from(r.result.data, 'base64'));
  };
  return { send, js, health, key, shot, errors, session, target };
}

/** Waits until the page is idle; a busy status that stops changing for SILENT ms is a hang. */
async function settle(t, what, ms = SETTLE) {
  const end = Date.now() + ms;
  let h;
  let traced = 0;
  while (Date.now() < end) {
    h = await t.health();
    // TRACE=1: what the page does meanwhile, every 2 s
    if (process.env.TRACE && Date.now() - traced > 2000 && h) { traced = Date.now(); console.log(`   ${what} ${JSON.stringify({ status: h.status, runs: h.runs, step: h.step, running: h.running, pending: h.pending, loading: h.loading })}`); }
    if (h && !h.busy && !h.boot && !h.running && !h.queued && !h.recovering && !h.due) {
      await sleep(600);
      const again = await t.health();
      if (again && !again.busy && !again.running && !again.queued && !again.due) return again;
    }
    // a hang: the line says nothing new and the worker says nothing at all
    if (h?.busy && h.quiet > SILENT && h.heard > SILENT) { check(`${what}: no hang`, false, `'${h.status}' unchanged for ${Math.round(h.quiet / 1000)} s: ${JSON.stringify({ ...h, text: undefined })}`); await t.shot(`hang-${what}`); return null; }
    await sleep(250);
  }
  check(`${what}: settles within ${ms / 1000} s`, false, JSON.stringify(h && { ...h, text: undefined })?.slice(0, 600));
  await t.shot(`slow-${what}`);
  return null;
}
/** What the page shows is of the project open now and of its newest text. */
function consistent(what, h, { example, file = example } = {}) {
  if (!h) return;
  const problems = [];
  if (example && h.example !== example) problems.push(`example ${h.example}, wanted ${example}`);
  if (file && h.file !== file) problems.push(`file ${h.file}, wanted ${file}`);
  if (file && /\.tex$/.test(file) && h.text !== null && !h.text.startsWith(firstLine(file))) problems.push(`editor text is not ${file}'s`);
  const u = `file://${h.dir}/${h.file}`;
  if (h.list && h.list[0] !== u) problems.push(`findings of ${h.list[0]}`);
  if (/\.tex$/.test(h.file) && h.shown !== null && h.sent !== null && h.shown !== h.sent) problems.push(`findings of version ${h.shown}, text is ${h.sent}`);
  if (h.listStale) problems.push('findings still dimmed');
  if (h.undimmed?.length) problems.push(`views of another file shown as current: ${h.undimmed.join(', ')}`);
  if (h.pdf && h.pdf.dir !== h.dir && !h.pdfOther) problems.push(`PDF of ${h.pdf.dir} shown live`);
  if (/^the playground failed|did not start|Stopped/.test(h.status)) problems.push(`status: ${h.status}`);
  check(`${what}: consistent`, !problems.length, problems.join('; '));
}
/** An action is answered: within ANSWER ms the status changes or says work goes on. */
async function answered(t, what, act) {
  const before = (await t.health())?.status;
  const t0 = Date.now();
  await act();
  for (; Date.now() - t0 < ANSWER; await sleep(50)) { const h = await t.health(); if (h && (h.busy || h.status !== before)) return check(`${what}: answered`, true, `${Date.now() - t0} ms`); }
  check(`${what}: answered within ${ANSWER} ms`, false, `status stays '${before}'`);
}
const exampleClick = (t, ex) => t.js(`document.querySelector('#play-examples [data-example="${ex}"]').click(); await new Promise((r) => setTimeout(r, 200)); document.querySelector('#play-replace[open] button[value=replace]')?.click();`);
const focusEnd = (t) => t.js(`const c = document.querySelector('.cm-content'); c.focus(); const v = c.cmView?.view; return 1;`).then(() => t.key('End', 2, 'End'));
const type = async (t, text) => { for (const ch of text) await t.send('Input.insertText', { text: ch }); };
const run = async (name, f) => {
  if (only && name !== 'boot' && !only.includes(name)) return;
  const t0 = Date.now();
  console.log(`-- ${name}`);
  try { await f(); } catch (e) { check(`${name}: ran`, false, e.message.slice(0, 300)); }
  console.log(`   ${name}: ${Math.round((Date.now() - t0) / 1000)} s`);
};

const A = await tab(true);
await A.send('Page.enable');
let t0 = Date.now();
await A.send('Page.navigate', { url: play });
await run('boot', async () => {
  const h = await settle(A, 'boot');
  check('boot: ready', !!h, `${Math.round((Date.now() - t0) / 1000)} s`);
  consistent('boot', h, {});
  // the typeset view's 'all | at cursor': one state drives the buttons and the view
  const scope = (b) => A.js(`document.querySelector('.ts-scope [data-scope=${b}]').click();
    return [[...document.querySelectorAll('.ts-scope button')].map((x) => x.dataset.scope + '=' + x.getAttribute('aria-pressed')).join(), !!document.querySelector('.ts-only-here')]`);
  const here = await scope('here'), all = await scope('all');
  check('typeset scope: one state', here[0] === 'all=false,here=true' && here[1] && all[0] === 'all=true,here=false' && !all[1], JSON.stringify([here, all]));
});

await run('idle', async () => {
  // a page left alone stays as it is: no worker restart, no work
  const before = await A.health();
  await sleep(+(process.env.IDLE_MS ?? 40000));
  const h = await A.health();
  check('idle: nothing restarts', h?.recoveries === before?.recoveries && !h?.busy, JSON.stringify({ recoveries: h?.recoveries, status: h?.status }));
});

await run('heavy', async () => {
  // the big examples one by one, each to its end: how long each takes (TRACE=1 shows the steps)
  for (const ex of ['slides.tex', 'thesis.tex']) {
    const t1 = Date.now();
    await exampleClick(A, ex);
    // the status card while it runs: each kind of step once
    await sleep(6000);
    const at = await A.js(`const b = document.getElementById('play-status').getBoundingClientRect(); return [b.left + 10, b.top + b.height / 2]`);
    await A.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    await sleep(800);
    await A.shot(`status-${ex}`);
    await A.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
    const h = await settle(A, `heavy ${ex}`);
    consistent(`heavy ${ex}`, h, { example: ex });
    console.log(`   ${ex}: ${Math.round((Date.now() - t1) / 1000)} s, ${h ? h.runs : '?'} runs so far`);
  }
});

await run('switch', async () => {
  // switches while the last one still analyzes: the page never shows the loading card again, ends on the last one
  for (const ex of ['thesis.tex', 'slides.tex', 'hello.tex', 'thesis.tex', 'macros.tex', 'slides.tex']) {
    await exampleClick(A, ex);
    await sleep(ex === 'hello.tex' ? 50 : 400);
    const h = await A.health();
    if (h?.boot) { check('switch: no loading card', false, `card open after ${ex}`); await A.shot('switch-card'); break; }
  }
  consistent('switch', await settle(A, 'switch'), { example: 'slides.tex' });
  await A.shot('switch');
  // into thesis and straight back, the second click inside the first's start
  await exampleClick(A, 'thesis.tex'); await sleep(30); await exampleClick(A, 'hello.tex');
  consistent('switch-back', await settle(A, 'switch-back'), { example: 'hello.tex' });
});

await run('typing', async () => {
  await exampleClick(A, 'thesis.tex');
  await sleep(300);
  await focusEnd(A);
  // a burst while the run goes on, then undo all of it and redo half
  const burst = '\n\\section{Chaos}\nSome text \\ref{sec:x} and \\cite{knuth} more.';
  await type(A, burst);
  let h = await settle(A, 'typing');
  check('typing: text kept', !!h?.text?.includes(burst), JSON.stringify(h?.text?.slice(-60)));
  consistent('typing', h, { example: 'thesis.tex' });
  for (let i = 0; i < 60; i++) await A.key('z', 2);
  for (let i = 0; i < 25; i++) await A.key('z', 2 | 8);
  h = await settle(A, 'undo-storm');
  consistent('undo-storm', h, { example: 'thesis.tex' });
  // typing then a switch at once: the new example's findings, never the old one's
  await type(A, ' more');
  await exampleClick(A, 'slides.tex');
  consistent('type-then-switch', await settle(A, 'type-then-switch'), { example: 'slides.tex' });
  await exampleClick(A, 'thesis.tex');
  h = await settle(A, 'edits-kept');
  check('edits kept across switches', !!h && h.text.includes('\\section{Chaos}'), JSON.stringify(h?.text?.slice(-60)));
});

await run('navigation', async () => {
  await exampleClick(A, 'thesis.tex');
  await settle(A, 'navigation-open');
  // during a run: Ctrl+click on \input{chapter} opens chapter.tex at once
  await focusEnd(A); await type(A, ' %');
  await sleep(450);
  const at = await A.js(`for (const l of document.querySelectorAll('.cm-line')) { const i = l.textContent.indexOf('\\\\input{chapter}'); if (i < 0) continue;
    const w = document.createTreeWalker(l, NodeFilter.SHOW_TEXT); let n, off = i + 9;
    while ((n = w.nextNode())) { if (off <= n.length) { const r = document.createRange(); r.setStart(n, off); r.setEnd(n, off); const b = r.getBoundingClientRect(); return [b.left + 1, b.top + b.height / 2]; } off -= n.length; } } return null;`);
  if (!at) return check('navigation: \\input{chapter} on screen', false);
  const t1 = Date.now();
  for (const type of ['mousePressed', 'mouseReleased']) await A.send('Input.dispatchMouseEvent', { type, x: at[0], y: at[1], button: 'left', clickCount: 1, modifiers: 2 });
  let h;
  for (; Date.now() - t1 < 1500; await sleep(50)) { h = await A.health(); if (h?.file === 'chapter.tex') break; }
  check('navigation: Ctrl+click answered during a run', h?.file === 'chapter.tex', `${Date.now() - t1} ms, file ${h?.file}`);
  await settle(A, 'navigation');
  await A.js(`document.querySelector('#play-tabs [data-name="thesis.tex"]')?.click()`);
});

await run('compile', async () => {
  await exampleClick(A, 'hello.tex');
  await sleep(100);
  // Compile during the analysis: the page says at once what happens (compiling, or why it cannot)
  const said = async () => A.js(`const s = document.getElementById('pdf-status'), b = document.getElementById('pdf-compile'), p = document.getElementById('play-pdf');
    return { text: s.textContent, running: p.classList.contains('pdf-running'), visible: !p.hidden, busy: b.getAttribute('aria-busy'), main: document.getElementById('play-status').textContent }`);
  const t1 = Date.now();
  await A.js(`document.getElementById('pdf-compile').disabled ? document.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true })) : document.getElementById('pdf-compile').click()`);
  let s;
  for (; Date.now() - t1 < 1000; await sleep(50)) { s = await said(); if (s.visible && (s.running || /compil|server|cannot|offline/i.test(s.text + s.main))) break; }
  check('compile: feedback within 1 s', s?.visible && (s.running || /compil|server|cannot|offline/i.test(s.text + s.main)), JSON.stringify(s));
  // it ends: a PDF, an error, or a clear reason, never 'compiling' forever
  let done = false;
  for (const end = Date.now() + SETTLE; Date.now() < end && !done; await sleep(500)) { s = await said(); done = !s.running; }
  check('compile: ends', done, JSON.stringify(s));
  await A.shot('compile');
  // a switch after a build: that PDF is not shown live for the other project
  await exampleClick(A, 'slides.tex');
  const h = await settle(A, 'compile-switch');
  consistent('compile-switch', h, { example: 'slides.tex' });
  if (h?.pdf) check('compile-switch: the PDF says it is of another project, takes no clicks', h.pdfOther && await A.js(`return /Built for hello.tex.*Compile slides.tex/.test(document.querySelector('.pdf-owner')?.textContent) && getComputedStyle(document.querySelector('.pdf-pages')).pointerEvents === 'none'`));
  await A.shot('compile-switch');
});

await run('network', async () => {
  // downloads that fail: the page says so and stays usable; once back, the next switch loads
  // the page's requests and its workers' (where the downloads run)
  const patterns = [{ urlPattern: '*.bin*' }, { urlPattern: '*/bundles/*' }, { urlPattern: '*.gz' }];
  const sessions = new Set([A.session]);
  let mode = 'fail';
  const handler = (m) => {
    if (m.method === 'Target.attachedToTarget' && m.sessionId === A.session) {
      const s = m.params.sessionId;
      sessions.add(s);
      raw('Fetch.enable', { patterns }, s).then(() => raw('Runtime.runIfWaitingForDebugger', {}, s));
    }
    if (m.method !== 'Fetch.requestPaused' || !sessions.has(m.sessionId)) return;
    const req = { requestId: m.params.requestId };
    if (mode === 'fail') raw('Fetch.failRequest', { ...req, errorReason: 'InternetDisconnected' }, m.sessionId);
    else setTimeout(() => raw('Fetch.continueRequest', req, m.sessionId), 2500);
  };
  events.push(handler);
  await A.send('Fetch.enable', { patterns });
  await A.send('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
  await A.js(`indexedDB.deleteDatabase('satex-playground'); for (const k of await caches.keys()) await caches.delete(k); return 1`).catch(() => {});
  for (const ex of ['context.tex', 'option-clash.tex', 'spacing.tex']) await answered(A, `network-fail ${ex}`, () => exampleClick(A, ex));
  let h = await settle(A, 'network-fail');
  consistent('network-fail', h, { example: 'spacing.tex' });
  // a package not fetched yet, while downloads fail: said so, the page goes on
  const needs = async (pkg) => { await A.js(`document.querySelector('.cm-content').focus()`); await A.key('Home', 2, 'Home'); await A.key('End', 0, 'End'); await A.send('Input.insertText', { text: `\n\\usepackage{${pkg}}` }); };
  await needs('todonotes');
  h = await settle(A, 'network-fail-package');
  check('network-fail-package: said', !!h && /todonotes|did not load|offline|connection|fetch/i.test(`${h.status} ${await A.js(`return document.getElementById('play-toast').textContent`)}`), h?.status);
  mode = 'slow';
  await answered(A, 'network-slow', () => exampleClick(A, 'macros.tex'));
  consistent('network-slow', await settle(A, 'network-slow'), { example: 'macros.tex' });
  await needs('tcolorbox');
  await settle(A, 'network-slow-package');
  check('network-slow: the package arrives', /tcolorbox/.test(await A.js(`return document.getElementById('play-status').dataset.packages ?? ''`)),
    await A.js(`return [document.getElementById('play-status').textContent, document.getElementById('play-toast').textContent, document.getElementById('lsp-diagnostics').textContent.slice(0, 200)].join(' | ')`));
  mode = 'pass';
  for (const s of sessions) await raw('Fetch.disable', {}, s);
  events.splice(events.indexOf(handler), 1);
  await A.send('Target.setAutoAttach', { autoAttach: false, waitForDebuggerOnStart: false, flatten: true });
});

await run('kill', async () => {
  // the worker dies mid-run (out of memory, a crash): the page restarts it, keeps the text, says so
  await exampleClick(A, 'thesis.tex');
  await sleep(500);
  await focusEnd(A); await type(A, ' kept');
  await sleep(500);
  await A.js(`self.satexKill?.('crash'); return 1`);
  let h = await settle(A, 'kill-crash');
  check('kill: text kept', !!h?.text?.includes(' kept'));
  consistent('kill-crash', h, { example: 'thesis.tex' });
  // a worker that goes silent (stuck in a loop): the watchdog notices and restarts it
  await focusEnd(A); await type(A, ' again');
  await sleep(500);
  await A.js(`self.satexKill?.('hang'); return 1`);
  h = await settle(A, 'kill-hang');
  check('kill-hang: text kept', !!h?.text?.includes(' again'));
  consistent('kill-hang', h, { example: 'thesis.tex' });
  // a worker that keeps dying (out of memory on this document): no restart loop, the line says so, Try again works
  for (let i = 0; i < 3; i++) {
    await A.js(`self.satexKill?.('crash'); return 1`);
    for (let k = 0; k < 120 && (await A.health())?.recovering !== false; k++) await sleep(500);
    await sleep(500);
  }
  h = await settle(A, 'kill-loop');
  check('kill-loop: stops and says so', /keeps stopping/.test(h?.status ?? ''), h?.status);
  await A.js(`document.getElementById('play-retry')?.click(); return 1`);
  await sleep(1000);
  consistent('kill-loop retry', await settle(A, 'kill-loop retry'), { example: 'thesis.tex' });
});

await run('reload', async () => {
  // a reload mid-run keeps what was typed (drafts in localStorage) and starts clean
  await exampleClick(A, 'macros.tex');
  await settle(A, 'reload-open');
  await focusEnd(A); await type(A, '\n% typed before the reload');
  await sleep(200);
  await A.send('Page.reload', { ignoreCache: false });
  const h = await settle(A, 'reload');
  check('reload: draft restored', !!h?.text?.includes('% typed before the reload'), JSON.stringify(h?.text?.slice(-60)));
  consistent('reload', h, { example: 'macros.tex' });
});

await run('evict', async () => {
  // the browser drops the kept caches while the page runs: the next start fetches again
  await exampleClick(A, 'thesis.tex');
  await A.js(`indexedDB.deleteDatabase('satex-playground'); for (const k of await caches.keys()) await caches.delete(k); return 1`).catch(() => {});
  await A.send('Page.reload', { ignoreCache: true });
  const h = await settle(A, 'evict');
  consistent('evict', h, {});
});

await run('tabs', async () => {
  // two tabs share the browser's caches: both start, edits in one never reach the other
  const B = await tab(false);
  await B.send('Page.enable');
  await B.send('Page.navigate', { url: `${play}#slides.tex` });
  await A.send('Page.reload');
  const [ha, hb] = await Promise.all([settle(A, 'tab-a'), settle(B, 'tab-b')]);
  consistent('tab-a', ha, {});
  consistent('tab-b', hb, { example: 'slides.tex' });
  await exampleClick(B, 'thesis.tex');
  await exampleClick(A, 'hello.tex');
  consistent('tab-b switch', await settle(B, 'tab-b2'), { example: 'thesis.tex' });
  consistent('tab-a switch', await settle(A, 'tab-a2'), { example: 'hello.tex' });
  check('tab-b: no page errors', !B.errors.length, B.errors.join(' | ').slice(0, 300));
  await raw('Target.closeTarget', { targetId: B.target });
});

console.log('max worker silence while owing an answer (ms):', (await A.health())?.gap);
check('no page errors', !A.errors.length, A.errors.join(' | ').slice(0, 400));
const failed = results.filter((r) => !r.ok);
console.log(`chaos: ${results.length - failed.length} of ${results.length} checks passed`);
proc.stdio[3].end();
proc.kill();
await sleep(500);
rmSync(profile, { recursive: true, force: true });
process.exit(failed.length ? 1 : 0);
