// The editor widgets end to end in a built playground: per widget, its entry on an example
// document (a code lens or the context menu), its dialog, one edit applied, the source changed.
//   python3 -m http.server -d site-out 8000 & node doc/site/showcase.mjs http://localhost:8000/
// SHOTS=dir keeps a screenshot of each step; ONLY=name runs the widgets whose name holds it.
import { existsSync, mkdirSync, mkdtempSync, readdirSync, writeFileSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';

const base = process.argv[2] ?? 'http://localhost:8000/';
let failed = false;
const check = (name, ok, detail = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? `: ${detail}` : ''}`); failed ||= !ok; };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** $CHROME, else Playwright's newest Chromium (as test.mjs finds it). */
function chromium() {
  if (process.env.CHROME) return process.env.CHROME;
  const root = join(homedir(), '.cache/ms-playwright');
  const dirs = existsSync(root) ? readdirSync(root).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse() : [];
  return dirs.map((d) => ['chrome-linux64/chrome', 'chrome-linux/chrome'].map((c) => join(root, d, c)).find(existsSync)).find(Boolean);
}
const chrome = chromium();
if (!chrome) { console.log('skip: no Chromium for the showcase (set CHROME)'); process.exit(0); }
const profile = mkdtempSync(join(tmpdir(), 'satex-showcase-'));
const proc = spawn(chrome, ['--headless=new', '--no-sandbox', '--disable-gpu', '--remote-debugging-pipe', `--user-data-dir=${profile}`, '--window-size=1440,1000', 'about:blank'],
  { stdio: ['ignore', 'ignore', 'ignore', 'pipe', 'pipe'] });
let id = 0, session, rest = '';
const replies = new Map(), errors = [];
const raw = (method, params = {}, sessionId = session) => new Promise((r) => { replies.set(++id, r); proc.stdio[3].write(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }) + '\0'); });
proc.stdio[4].on('data', (chunk) => {
  const parts = (rest + chunk.toString()).split('\0');
  rest = parts.pop();
  for (const part of parts) {
    if (!part) continue;
    const m = JSON.parse(part);
    if (m.id) replies.get(m.id)?.(m);
    else if (m.method === 'Runtime.exceptionThrown') errors.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text);
  }
});
const js = async (expr) => {
  const r = await raw('Runtime.evaluate', { expression: `(async () => { ${expr} })()`, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(`${expr.slice(0, 80)}: ${r.result.exceptionDetails.exception?.description}`);
  return r.result?.result?.value;
};
const until = async (expr, ms = 60000) => { for (const end = Date.now() + ms; Date.now() < end; await sleep(200)) if (await js(expr).catch(() => false)) return true; return false; };
const shot = async (name) => {
  if (!process.env.SHOTS) return;
  mkdirSync(process.env.SHOTS, { recursive: true });
  const r = await raw('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(process.env.SHOTS, `${name}.png`), Buffer.from(r.result.data, 'base64'));
};

/** The whole text of the open editor (playground.js keeps its view on the editor element). */
const TEXT = `document.querySelector('.cm-editor').satexView.state.doc.toString()`;
const QUIET = `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`;
/** The example `file` open, analysed. */
async function open(file) {
  await raw('Page.navigate', { url: `${base}play/index.html#${file}` });
  await until(`return document.getElementById('play-status').dataset.done === 'ready'`, 240000);
  // the last widget's edit is kept: the page asks before it replaces that project
  const replace = `[...document.querySelectorAll('dialog[open] button')].find((b) => b.textContent === 'Replace')`;
  const shown = `document.querySelector('#play-tabs [aria-selected=true]')?.textContent.includes(${JSON.stringify(file)})`;
  await until(`return ${shown} || !!${replace}`, 60000);
  await js(`${replace}?.click()`);
  await until(`return ${shown} && ${QUIET}`, 120000);
}
/** The screen point of the `at`-th character of `needle` in the editor, scrolled into view. */
const spot = (needle, at = 1) => js(`const v = document.querySelector('.cm-editor').satexView, i = v.state.doc.toString().indexOf(${JSON.stringify(needle)});
  if (i < 0) return null; v.dispatch({ selection: { anchor: i + ${at} }, scrollIntoView: true }); await new Promise((r) => setTimeout(r, 300));
  const c = v.coordsAtPos(i + ${at}); return c && [c.left + 1, (c.top + c.bottom) / 2];`);
/** Clicks the code lens titled `title`, once the server sent it. */
async function lens(title) {
  const sel = `[...document.querySelectorAll('.cm-lens')].find((b) => b.textContent === ${JSON.stringify(title)})`;
  if (!await until(`return !!${sel}`, 120000)) return `no lens; lenses: ${await js(`return [...document.querySelectorAll('.cm-lens')].map((b) => b.textContent).join()`)}`;
  await js(`${sel}.scrollIntoView({ block: 'center' })`);
  const [x, y] = await js(`const r = ${sel}.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]`);
  for (const type of ['mousePressed', 'mouseReleased']) await raw('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 });
  return '';
}
/** Right-clicks `needle` in the editor and picks the menu row whose label matches `entry`. */
async function menu(needle, at, entry, name) {
  const p = await spot(needle, at);
  if (!p) return `no ${needle} in the editor`;
  await raw('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p[0], y: p[1] });
  for (const type of ['mousePressed', 'mouseReleased']) await raw('Input.dispatchMouseEvent', { type, x: p[0], y: p[1], button: 'right', clickCount: 1 });
  const rows = `[...document.querySelectorAll('#play-ctx > li')]`;
  const row = `${rows}.find((li) => ${entry}.test(li.firstChild.textContent))`;
  if (!await until(`return !!${row}`, 30000)) return `no ${entry} in the menu: ${await js(`return ${rows}.map((li) => li.firstChild.textContent).join(' | ')`)}`;
  await shot(`${name}-1-menu`);
  await js(`${row}.click()`);
  return '';
}
const VIEW = `document.getElementById('play-view')`, DIALOG = `document.querySelector('dialog.sd-dialog[open]')`;
/** Sets an input's value as typing would. */
const type = (sel, value) => js(`const e = ${sel}; e.value = ${JSON.stringify(value)}; e.dispatchEvent(new Event('input', { bubbles: true })); e.dispatchEvent(new Event('change', { bubbles: true }));`);
/** The dialog's Apply, once the server previewed the edit (its TeX shows `want`). */
async function applyDialog(want) {
  await until(`return ${DIALOG}?.querySelector('.sd-tex')?.textContent.includes(${JSON.stringify(want)})`, 30000);
  await js(`${DIALOG}.querySelector('.sd-primary').click()`);
}

// Each widget: the example, how its dialog opens, the edit, and what the source holds after.
const WIDGETS = [
  { name: 'table', file: 'paper.tex', entry: () => lens('Edit table…'), shown: DIALOG, want: '0.97',
    edit: async () => { await type(`${DIALOG}.querySelector('input[aria-label="row 2 column 2"]')`, '0.97'); await applyDialog('0.97'); } },
  { name: 'image', file: 'figure.tex', entry: () => lens('Edit image…'), shown: DIALOG, want: 'angle=90',
    edit: async () => { await type(`${DIALOG}.querySelector('input[aria-label="angle in degrees"]')`, '90'); await applyDialog('angle=90'); } },
  { name: 'color', file: 'colors.tex', entry: () => menu('\\textcolor{brand}', 11, /color/i, 'color'), shown: `${VIEW}.open && ${VIEW}`, want: '\\textcolor{soft}',
    edit: () => js(`[...${VIEW}.querySelectorAll('.tv-chip')].find((b) => b.textContent === 'soft').click()`) },
  { name: 'units', file: 'units.tex', entry: () => menu('\\vspace{12pt}', 9, /length/i, 'units'), shown: `${VIEW}.open && ${VIEW}`, want: '\\vspace{0.42cm}',
    edit: () => js(`[...${VIEW}.querySelectorAll('.tv-unit')].find((b) => b.querySelector('span').textContent === 'cm').click()`) },
  { name: 'keys', file: 'units.tex', entry: () => menu('[margin', 1, /option|key/i, 'keys'), shown: `${VIEW}.open && ${VIEW}`, want: '\\usepackage[top=2.5cm]',
    edit: async () => { await type(`${VIEW}.querySelector('.tv-choices input')`, 'top'); await js(`[...${VIEW}.querySelectorAll('.tv-choice b')].find((b) => b.textContent === 'top').click()`); } },
  { name: 'environment', file: 'units.tex', entry: () => menu('\\begin{itemize}', 8, /environment/i, 'environment'), shown: `${VIEW}.open && ${VIEW}`, want: '\\begin{enumerate}',
    edit: async () => { await type(`${VIEW}.querySelector('.tv-choices input')`, 'enumerate'); await js(`[...${VIEW}.querySelectorAll('.tv-choice b')].find((b) => b.textContent === 'enumerate').click()`); } },
  { name: 'tikz', file: 'spacing.tex', entry: () => lens('Edit drawing…'), shown: `${VIEW}.open && ${VIEW}`, want: '(c)',
    edit: async () => {
      await js(`${VIEW}.querySelector('.tv-mermaid').open = true`);
      await type(`${VIEW}.querySelector('.tv-mermaid textarea')`, 'graph LR\n  a --> c[checked]');
      await js(`[...${VIEW}.querySelectorAll('.tv-mermaid button')].find((b) => b.textContent === 'Import').click()`);
      await sleep(300);
      await shot('tikz-2b-imported');
      await js(`${VIEW}.querySelector('.tv-foot .tv-primary').click()`);
    } },
  { name: 'page', file: 'units.tex', entry: () => lens('Page layout…'), shown: `${VIEW}.open && ${VIEW}.querySelector('.tv-page')`, want: 'left=3cm',
    edit: async () => { await type(`${VIEW}.querySelector('input[aria-label=Left]')`, '3'); await js(`${VIEW}.querySelector('.tv-foot .tv-primary').click()`); } },
];

try {
  let page;
  for (let i = 0; i < 100 && !page; i++) { await sleep(100); page = (await raw('Target.getTargets', {}, null)).result?.targetInfos?.find((t) => t.type === 'page'); }
  session = (await raw('Target.attachToTarget', { targetId: page.targetId, flatten: true }, null)).result.sessionId;
  await raw('Runtime.enable');
  await raw('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false });
  for (const w of WIDGETS.filter((w) => !process.env.ONLY || w.name.includes(process.env.ONLY))) {
    try {
      await open(w.file);
      const before = await js(`return ${TEXT}`);
      await shot(`${w.name}-0-open`);
      const miss = await w.entry();
      if (miss) { await shot(`${w.name}-miss`); check(`${w.name}: entry on ${w.file}`, false, miss); continue; }
      const shown = await until(`return !!(${w.shown})`, 30000);
      await sleep(400);
      await shot(`${w.name}-2-dialog`);
      if (!shown) { check(`${w.name}: dialog opens`, false); continue; }
      await w.edit();
      const changed = await until(`const t = ${TEXT}; return t !== ${JSON.stringify(before)} && t.includes(${JSON.stringify(w.want)})`, 30000);
      await sleep(300);
      await shot(`${w.name}-3-applied`);
      const after = await js(`return ${TEXT}`);
      check(`${w.name}: ${w.file} edited through its dialog`, changed, changed ? '' : `wanted ${w.want}; ${after === before ? 'unchanged' : after.slice(0, 300)}`);
    } catch (e) { await shot(`${w.name}-error`); check(w.name, false, e.message.slice(0, 300)); }
  }
  if (errors.length) check('no page exceptions', false, errors.slice(0, 3).join(' | ').slice(0, 400));
} finally {
  proc.stdio[3].end();
  proc.kill();
}
process.exit(failed ? 1 : 0);
