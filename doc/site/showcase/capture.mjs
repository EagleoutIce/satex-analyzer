// The landing page's showcase images, taken from a built playground: per entry of shots.json, its
// example opened, its steps run, the widget cropped, in light and dark, encoded as AVIF, WebP and PNG.
//   cargo playground --dev --port 8123 & node doc/site/showcase/capture.mjs http://127.0.0.1:8123/
// ONLY=a,b takes the entries of those names, SCHEMES=dark one theme. RAW=dir keeps a full screenshot per entry.
// Needs Chromium ($CHROME or Playwright's), ImageMagick (`magick`), `avifenc` and `pngquant`.
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { execFileSync, spawn } from 'node:child_process';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const base = process.argv[2] ?? 'http://127.0.0.1:8000/';
const out = process.argv[3] ?? join(here, 'img');
const { width: WIDTH, shots } = JSON.parse(readFileSync(join(here, 'shots.json'), 'utf8'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = false;

function chromium() {
  if (process.env.CHROME) return process.env.CHROME;
  const root = join(homedir(), '.cache/ms-playwright');
  const dirs = existsSync(root) ? readdirSync(root).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse() : [];
  return dirs.map((d) => join(root, d, 'chrome-linux64/chrome')).find(existsSync);
}
const scratch = process.env.TMPDIR ?? join(here, '../../../target/scratch');
mkdirSync(scratch, { recursive: true });
const profile = mkdtempSync(join(scratch, 'showcase-chrome-'));
const proc = spawn(chromium(), ['--headless=new', '--no-sandbox', '--disable-gpu', '--hide-scrollbars', '--remote-debugging-pipe', `--user-data-dir=${profile}`, 'about:blank'],
  { stdio: ['ignore', 'ignore', 'ignore', 'pipe', 'pipe'] });
let id = 0, session, rest = '';
const replies = new Map();
const raw = (method, params = {}, sessionId = session) => new Promise((r) => { replies.set(++id, r); proc.stdio[3].write(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }) + '\0'); });
proc.stdio[4].on('data', (chunk) => {
  const parts = (rest + chunk.toString()).split('\0');
  rest = parts.pop();
  for (const part of parts) if (part) { const m = JSON.parse(part); if (m.id) replies.get(m.id)?.(m); }
});
const js = async (expr) => {
  const r = await raw('Runtime.evaluate', { expression: `(async () => { ${expr} })()`, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(`${expr.slice(0, 80)}: ${r.result.exceptionDetails.exception?.description}`);
  return r.result?.result?.value;
};
const until = async (expr, ms = 60000) => { for (const end = Date.now() + ms; Date.now() < end; await sleep(200)) if (await js(expr).catch(() => false)) return true; return false; };
const QUIET = `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`;
/** In the page: `__at(needle, k)` is the editor position k characters into needle, `__xy` its screen point. */
const HELPERS = `const v = () => document.querySelector('.cm-editor').satexView;
  self.__at = (n, k = 0) => { const t = v().state.doc.toString(), i = t.indexOf(n) + k, before = t.slice(0, i).split('\\n'); return [before.length - 1, before.at(-1).length, i]; };
  self.__xy = (n, k = 0) => { const i = __at(n, k)[2]; v().dispatch({ selection: { anchor: i }, scrollIntoView: true }); const c = v().coordsAtPos(i); return [c.left + 2, (c.top + c.bottom) / 2]; };`;

async function open(file) {
  // each entry starts from the example as shipped: no draft, no kept view
  await raw('Storage.clearDataForOrigin', { origin: new URL(base).origin, storageTypes: 'local_storage,indexeddb' });
  await raw('Page.navigate', { url: `${base}play/index.html#${file}` });
  await until(`return document.getElementById('play-status').dataset.done === 'ready'`, 240000);
  const replace = `[...document.querySelectorAll('dialog[open] button')].find((b) => b.textContent === 'Replace')`;
  const shown = `document.querySelector('#play-tabs [aria-selected=true]')?.textContent.includes(${JSON.stringify(file)})`;
  await until(`return ${shown} || !!${replace}`, 60000);
  await js(`${replace}?.click()`);
  await until(`return ${shown} && ${QUIET}`, 120000);
  await js(HELPERS);
  // a view the last entry left open comes back with the page
  await js(`for (const d of document.querySelectorAll('dialog[open]')) d.close();`);
}
/** Replaces the open file's text with `text` and waits for its run. */
async function write(text) {
  await js(`const v = document.querySelector('.cm-editor').satexView; v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: ${JSON.stringify(text)} } });`);
  await sleep(1500);
  await until(`return ${QUIET}`, 120000);
  await sleep(1000);
}
const mouse = async (type, [x, y], extra = {}) => raw('Input.dispatchMouseEvent', { type, x, y, button: 'none', ...extra });

/** One step: `eval` runs, `until` waits for, `hover`/`click` point at an expression giving [x, y] or an element. */
async function step(s) {
  // an eval with an until repeats until it holds: a widget asked for before the server answers opens nothing
  let ok = !s.until;
  for (const end = Date.now() + (s.ms ?? 120000); !ok && Date.now() < end || !s.until && s.eval && !ok;) {
    if (s.eval) await js(s.eval).catch(() => {});
    ok = !s.until || await until(`return !!(${s.until})`, s.eval ? 8000 : s.ms ?? 120000);
  }
  if (!s.until && s.eval) await js(s.eval);
  if (s.wait) await sleep(s.wait);
  if (!ok) throw new Error(`no ${s.until}`);
  const target = s.hover ?? s.click;
  if (target) {
    const p = await js(`const t = ${target}; if (Array.isArray(t)) return t; t.scrollIntoView({ block: 'center' }); const r = t.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2];`);
    await mouse('mouseMoved', [p[0] - 4, p[1]]);
    await mouse('mouseMoved', p);
    if (s.click) for (const type of ['mousePressed', 'mouseReleased']) await mouse(type, p, { button: 'left', clickCount: 1 });
  }
}

/** The union box of `crop` (selectors, in the page) and `pad` px around, in CSS pixels. */
const box = (crop, pad) => js(`const rs = ${JSON.stringify([crop].flat())}.flatMap((s) => [...document.querySelectorAll(s)]).filter((e) => e.checkVisibility()).map((e) => e.getBoundingClientRect()).filter((r) => r.width);
  if (!rs.length) return null;
  const x = Math.max(0, Math.min(...rs.map((r) => r.left)) - ${pad}), y = Math.max(0, Math.min(...rs.map((r) => r.top)) - ${pad});
  return { x, y, width: Math.min(innerWidth, Math.max(...rs.map((r) => r.right)) + ${pad}) - x, height: Math.min(innerHeight, Math.max(...rs.map((r) => r.bottom)) + ${pad}) - y, scale: 1 };`);

/** AVIF, WebP and a quantized PNG of `png`, `width` px wide at most. */
function encode(png, stem, width) {
  const sized = `${stem}.tmp.png`;
  execFileSync('magick', [png, '-resize', `${width}x>`, '-strip', sized]);
  execFileSync('magick', [sized, '-quality', '80', '-define', 'webp:method=6', `${stem}.webp`]);
  execFileSync('avifenc', ['-q', '60', '-s', '4', '-j', '1', sized, `${stem}.avif`], { stdio: 'ignore' });
  execFileSync('pngquant', ['--force', '--skip-if-larger', '--strip', '-Q', '60-85', '-o', `${stem}.png`, sized]);
  if (!existsSync(`${stem}.png`)) execFileSync('magick', [sized, `${stem}.png`]);
  rmSync(sized);
}

try {
  let page;
  for (let i = 0; i < 100 && !page; i++) { await sleep(100); page = (await raw('Target.getTargets', {}, null)).result?.targetInfos?.find((t) => t.type === 'page'); }
  session = (await raw('Target.attachToTarget', { targetId: page.targetId, flatten: true }, null)).result.sessionId;
  await raw('Runtime.enable');
  mkdirSync(out, { recursive: true });
  for (const scheme of (process.env.SCHEMES ?? 'light,dark').split(',')) {
    await raw('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scheme }, { name: 'prefers-reduced-motion', value: 'reduce' }] });
    for (const s of shots.filter((s) => !process.env.ONLY || process.env.ONLY.split(',').includes(s.name))) {
      const [w, h] = s.viewport ?? [1080, 700];
      await raw('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 2, mobile: false });
      try {
        // a first visit may miss the widget while the page still fetches what it shows: one more try
        for (let attempt = 0; ; attempt++) {
          try { await open(s.file); if (s.text) await write(s.text); for (const st of s.steps) await step({ ms: 45000, ...st }); break; } catch (e) { if (attempt) throw e; }
        }
        // no stray title tooltip or focus ring in the picture
        if (!s.keepMouse) await mouse('mouseMoved', [1, 1]);
        await js('document.activeElement?.blur()');
        await sleep(s.settle ?? 600);
        const clip = await box(s.crop, s.pad ?? 0);
        if (process.env.RAW) {
          mkdirSync(process.env.RAW, { recursive: true });
          writeFileSync(join(process.env.RAW, `${s.name}-${scheme}.png`), Buffer.from((await raw('Page.captureScreenshot', { format: 'png' })).result.data, 'base64'));
        }
        if (!clip) throw new Error(`nothing matches ${s.crop}`);
        const shot = await raw('Page.captureScreenshot', { format: 'png', clip, captureBeyondViewport: false });
        const png = join(out, `${s.name}-${scheme}.raw.png`);
        writeFileSync(png, Buffer.from(shot.result.data, 'base64'));
        encode(png, join(out, `${s.name}-${scheme}`), WIDTH);
        rmSync(png);
        console.log(`ok   ${s.name} ${scheme} ${Math.round(clip.width)}x${Math.round(clip.height)}`);
      } catch (e) {
        failed = true;
        if (process.env.RAW) writeFileSync(join(process.env.RAW, `${s.name}-${scheme}-fail.png`), Buffer.from((await raw('Page.captureScreenshot', { format: 'png' })).result.data, 'base64'));
        console.log(`FAIL ${s.name} ${scheme}: ${e.message.slice(0, 300)}`); }
    }
  }
} finally {
  proc.stdio[3].end();
  proc.kill();
  await sleep(500);
  rmSync(profile, { recursive: true, force: true, maxRetries: 5 });
}
process.exit(failed ? 1 : 0);
