// The landing page's showcase images, taken from a built playground: per entry of shots.json, its
// example opened, its steps run, the widget cropped, in light and dark, encoded as AVIF, WebP and PNG.
//   cargo playground --dev --port 8123 & node doc/site/showcase/capture.mjs http://127.0.0.1:8123/
// Entries with `"server": true` need the compile server of `cargo playground`: build.py leaves them out.
// ONLY=a,b takes the entries of those names, SCHEMES=dark one theme. RAW=dir keeps a full screenshot per entry.
// An entry's `open` is the widget its try-it link opens (playground.js `satexOpen`); a step `{ "open": true }` opens it
// the same way. A step with `snap` keeps the crop at that point as `<name>-<snap>` (a refactoring's before), `video`
// steps are recorded after the other steps into a looping WebM and MP4 whose first frame is the still.
// Needs Chromium ($CHROME or Playwright's), ImageMagick (`magick`), `avifenc`, `pngquant` and `ffmpeg`.
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { execFileSync, spawn } from 'node:child_process';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const base = process.argv[2] ?? 'http://127.0.0.1:8000/';
const out = process.argv[3] ?? join(here, 'img');
const { width: WIDTH, video: VIDEO = 1000, budget: BUDGET = 90000, shots } = JSON.parse(readFileSync(join(here, 'shots.json'), 'utf8'));
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
const replies = new Map(), events = new Map();
const raw = (method, params = {}, sessionId = session) => new Promise((r) => { replies.set(++id, r); proc.stdio[3].write(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }) + '\0'); });
proc.stdio[4].on('data', (chunk) => {
  const parts = (rest + chunk.toString()).split('\0');
  rest = parts.pop();
  for (const part of parts) if (part) { const m = JSON.parse(part); if (m.id) replies.get(m.id)?.(m); else events.get(m.method)?.(m.params); }
});
const js = async (expr) => {
  const r = await raw('Runtime.evaluate', { expression: `(async () => { ${expr} })()`, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(`${expr.slice(0, 80)}: ${r.result.exceptionDetails.exception?.description}`);
  return r.result?.result?.value;
};
let deadline = Infinity;
const until = async (expr, ms = 60000) => { for (const end = Math.min(Date.now() + ms, deadline); Date.now() < end; await sleep(200)) if (await js(expr).catch(() => false)) return true; return false; };
const QUIET = `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`;
/** In the page: `__at(needle, k)` is the editor position k characters into needle, `__xy` its screen point. */
const HELPERS = `const v = () => document.querySelector('.cm-editor').satexView;
  self.__at = (n, k = 0) => { const t = v().state.doc.toString(), i = t.indexOf(n) + k, before = t.slice(0, i).split('\\n'); return [before.length - 1, before.at(-1).length, i]; };
  self.__xy = (n, k = 0) => { const i = __at(n, k)[2]; v().dispatch({ selection: { anchor: i }, scrollIntoView: true }); const c = v().coordsAtPos(i); return [c.left + 2, (c.top + c.bottom) / 2]; };`;

async function open(file) {
  // a fresh page per entry: an example seen before comes back with its edits (drafts stay in memory), and a view left open or a run not yet in skews the next
  await raw('Page.navigate', { url: 'about:blank' }); await sleep(500); // the old page's last draft write lands before the clear
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
  await js(LINES);
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

/** The point an expression gives: [x, y], or an element's centre. */
const point = (e) => js(`const t = ${e}; if (Array.isArray(t)) return t; t.scrollIntoView({ block: 'nearest' }); const r = t.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2];`);
/** A mouse drag from one point to another over `ms`, eased, as a hand moves. */
async function drag(from, to, ms = 900) {
  const [a, b] = [await point(from), await point(to)], n = Math.max(8, Math.round(ms / 30));
  await mouse('mouseMoved', a);
  await mouse('mousePressed', a, { button: 'left', buttons: 1, clickCount: 1 });
  for (let i = 1; i <= n; i++) {
    const t = i / n, e = t < .5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
    await mouse('mouseMoved', [a[0] + (b[0] - a[0]) * e, a[1] + (b[1] - a[1]) * e], { button: 'left', buttons: 1 });
    await sleep(ms / n);
  }
  await mouse('mouseReleased', b, { button: 'left', buttons: 0, clickCount: 1 });
}
/** Keys typed as a hand types them: `\t` is Tab, `\n` Enter. */
async function type(text) {
  const KEYS = { '\t': ['Tab', 9], '\n': ['Enter', 13] };
  for (const c of text) {
    const [key, code] = KEYS[c] ?? (c === '.' ? ['.', 190] : [c, c.toUpperCase().charCodeAt(0)]);
    const name = KEYS[c] ? key : /\d/.test(c) ? `Digit${c}` : c === '.' ? 'Period' : `Key${c.toUpperCase()}`;
    const base = { key, code: name, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code };
    await raw('Input.dispatchKeyEvent', { type: 'keyDown', ...base, ...(KEYS[c] ? (c === '\n' ? { text: '\r' } : {}) : { text: c }) });
    await raw('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
    await sleep(70 + Math.random() * 60);
  }
}
const OPENED = `document.querySelector('dialog.sd-dialog[open]') || document.getElementById('play-view').open`;

/** One step: `eval` runs, `until` waits for, `hover`/`click` point at an expression giving [x, y] or an element,
 * `drag` is [from, to], `open` opens the entry's widget. */
async function step(s, shot) {
  if (s.open) s = { eval: `return await satexOpen(${JSON.stringify(Object.fromEntries(['widget', 'at', 'k'].map((k, i) => [k, shot.open[i]])))})`, until: OPENED, ...s, open: undefined };
  if (s.drag) { await drag(...s.drag, s.over); if (s.wait) await sleep(s.wait); return; }
  if (s.type != null) { await type(s.type); if (s.wait) await sleep(s.wait); return; }
  // an eval with an until repeats until it holds: a widget asked for before the server answers opens nothing
  let ok = !s.until;
  for (const end = Math.min(Date.now() + (s.ms ?? 120000), deadline); !ok && Date.now() < end || !s.until && s.eval && !ok;) {
    if (s.eval) await js(s.eval).catch((e) => process.env.DEBUG && console.log('eval failed', e.message.slice(0, 300)));
    ok = !s.until || await until(`return !!(${s.until})`, s.eval ? 8000 : s.ms ?? 120000);
  }
  if (!s.until && s.eval) await js(s.eval);
  if (s.wait) await sleep(s.wait);
  if (!ok) throw new Error(`no ${s.until}`);
  const target = s.hover ?? s.click;
  if (target) {
    const p = await point(target);
    await mouse('mouseMoved', [p[0] - 4, p[1]]);
    await mouse('mouseMoved', p);
    if (s.click) for (const type of ['mousePressed', 'mouseReleased']) await mouse(type, p, { button: 'left', clickCount: 1 });
  }
}

/** The union box of `crop` (selectors, in the page) and `pad` px around, in CSS pixels; an error where the viewport cuts it. */
/** `=lines(a, b)` in a crop: the editor's lines from the one holding `a` to the one holding `b`. */
const LINES = `self.lines = (a, b) => { const v = document.querySelector('.cm-editor').satexView, t = v.state.doc.toString(), x = v.lineBlockAt(t.indexOf(a)), y = v.lineBlockAt(t.indexOf(b)), c = v.contentDOM.getBoundingClientRect(), right = Math.min(c.right, v.scrollDOM.getBoundingClientRect().right), top = c.top + x.top - v.documentPadding.top, bottom = c.top + y.bottom - v.documentPadding.top; return { left: c.left, right, top, bottom, width: right - c.left }; };`;
const box = async (crop, pad) => {
  // a crop entry `=expr` is a box the page computes, { left, top, right, bottom, width }
  const sels = [crop].flat(), exprs = sels.filter((c) => c.startsWith('=')).map((c) => c.slice(1));
  const b = await js(`const rs = ${JSON.stringify(sels.filter((c) => !c.startsWith('=')))}.flatMap((s) => [...document.querySelectorAll(s)]).filter((e) => e.checkVisibility()).map((e) => e.getBoundingClientRect()).concat([${exprs.join(',')}]).filter((r) => r?.width);
    if (!rs.length) return null;
    const l = Math.min(...rs.map((r) => r.left)) - ${pad}, t = Math.min(...rs.map((r) => r.top)) - ${pad}, r = Math.max(...rs.map((r) => r.right)) + ${pad}, b = Math.max(...rs.map((r) => r.bottom)) + ${pad};
    const x = Math.max(0, l), y = Math.max(0, t);
    return { x, y, width: Math.min(innerWidth, r) - x, height: Math.min(innerHeight, b) - y, scale: 1, cut: l < -.5 || t < -.5 || r > innerWidth + .5 || b > innerHeight + .5 };`);
  if (b?.cut) throw new Error(`the viewport cuts ${crop}: make the entry's viewport larger`);
  if (b) delete b.cut;
  return b;
};
const union = (a, b) => { const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y); return { x, y, width: Math.max(a.x + a.width, b.x + b.width) - x, height: Math.max(a.y + a.height, b.y + b.height) - y, scale: 1 }; };
const capture = async (clip) => Buffer.from((await raw('Page.captureScreenshot', { format: 'png', ...(clip ? { clip } : {}), captureBeyondViewport: false })).result.data, 'base64');

/** The `video` steps recorded as screencast frames, then cut to the union of the crop before and after,
 * `<stem>.webm` beside the still (its first frame); build.py derives the MP4. */
async function record(s, stem, [w, h]) {
  const dir = mkdtempSync(join(scratch, 'showcase-frames-'));
  try {
    const before = await box(s.crop, s.pad ?? 0);
    const frames = [];
    writeFileSync(join(dir, 'f0.png'), await capture());
    frames.push([join(dir, 'f0.png'), performance.now() / 1000]);
    events.set('Page.screencastFrame', (p) => {
      const f = join(dir, `f${frames.length}.png`);
      writeFileSync(f, Buffer.from(p.data, 'base64'));
      frames.push([f, performance.now() / 1000]);
      raw('Page.screencastFrameAck', { sessionId: p.sessionId });
    });
    await raw('Page.startScreencast', { format: 'png', maxWidth: w * 2, maxHeight: h * 2, everyNthFrame: 1 });
    await sleep(300);
    for (const st of s.video) await step({ ms: 45000, ...st }, s);
    await sleep(s.hold ?? 900);
    await raw('Page.stopScreencast');
    events.delete('Page.screencastFrame');
    const clip = union(before, await box(s.crop, s.pad ?? 0));
    // the timeline: each frame as long as it stood, the last one a beat longer
    const list = frames.map(([f, t], i) => `file '${f}'\nduration ${Math.max(0.02, ((frames[i + 1]?.[1] ?? t + .5) - t)).toFixed(3)}`).join('\n') + `\nfile '${frames.at(-1)[0]}'\n`;
    writeFileSync(join(dir, 'list.txt'), list);
    // frames of either source to one size first, then cut
    const crop = `scale=${w * 2}:${h * 2},crop=${Math.round(clip.width * 2)}:${Math.round(clip.height * 2)}:${Math.round(clip.x * 2)}:${Math.round(clip.y * 2)}`, wide = Math.min(VIDEO, Math.round(clip.width * 2)) & ~1;
    const vf = `${crop},scale=${wide}:-2:flags=lanczos,fps=25,format=yuv420p`;
    const input = ['-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', join(dir, 'list.txt')];
    execFileSync('ffmpeg', [...input, '-vf', vf, '-c:v', 'libvpx-vp9', '-crf', '40', '-b:v', '0', '-row-mt', '1', '-deadline', 'good', '-cpu-used', '2', '-an', `${stem}.webm`]);
    // the still: the first frame, cut alike
    const still = join(dir, 'still.png');
    execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-i', frames[0][0], '-vf', crop, still]);
    return [readFileSync(still), clip];
  } finally {
    events.delete('Page.screencastFrame');
    rmSync(dir, { recursive: true, force: true });
  }
}

/** A quantized PNG of `png`, `width` px wide at most: the source git keeps (build.py derives AVIF and WebP). */
function encode(png, stem, width) {
  const sized = `${stem}.tmp.png`;
  execFileSync('magick', [png, '-resize', `${width}x>`, '-strip', sized]);
  execFileSync('pngquant', ['--force', '--skip-if-larger', '--strip', '-Q', '60-85', '-o', `${stem}.png`, sized]);
  if (!existsSync(`${stem}.png`)) execFileSync('magick', [sized, `${stem}.png`]);
  rmSync(sized);
}

/** The entry's crop now as `<stem>` (AVIF, WebP, PNG); with `video` steps, their recording beside it. */
async function still(s, stem, size) {
  // no stray title tooltip or focus ring in the picture
  if (!s.keepMouse) await mouse('mouseMoved', [1, 1]);
  if (!s.keepFocus) await js('document.activeElement?.blur()');
  await sleep(s.settle ?? 600);
  if (process.env.RAW) {
    mkdirSync(process.env.RAW, { recursive: true });
    writeFileSync(join(process.env.RAW, `${stem}.png`), await capture());
  }
  let clip = await box(s.crop, s.pad ?? 0);
  if (!clip) throw new Error(`nothing matches ${s.crop}`);
  const png = join(out, `${stem}.raw.png`);
  if (s.video && size) { const [buf, c] = await record(s, join(out, stem), size); writeFileSync(png, buf); clip = c; }
  else writeFileSync(png, await capture(clip));
  encode(png, join(out, stem), WIDTH);
  rmSync(png);
  return clip;
}

try {
  let page;
  for (let i = 0; i < 100 && !page; i++) { await sleep(100); page = (await raw('Target.getTargets', {}, null)).result?.targetInfos?.find((t) => t.type === 'page'); }
  session = (await raw('Target.attachToTarget', { targetId: page.targetId, flatten: true }, null)).result.sessionId;
  await raw('Runtime.enable');
  mkdirSync(out, { recursive: true });
  for (const scheme of (process.env.SCHEMES ?? 'light,dark').split(',')) {
    await raw('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scheme }, { name: 'prefers-reduced-motion', value: 'reduce' }] });
    for (const s of shots.filter((s) => !s.live).filter((s) => !process.env.ONLY || process.env.ONLY.split(',').includes(s.name))) {
      const [w, h] = s.viewport ?? [1080, 700];
      await raw('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 2, mobile: false });
      try {
        // a first visit may miss the widget while the page still fetches what it shows, or wait past its
        // budget under load: one more try, from a fresh page with a fresh budget
        for (let attempt = 0; ; attempt++) {
          try {
            deadline = Infinity;
            await open(s.file);
            // the steps share one budget: a failing wait fails fast, never one timeout per step
            deadline = Date.now() + (s.budget ?? BUDGET);
            if (process.env.PROBE0) console.log('PROBE0', JSON.stringify(await js(process.env.PROBE0)));
            if (s.text) await write(s.text);
            for (const st of s.steps) {
              await step({ ms: 45000, ...st }, s);
              if (st.snap) await still({ ...s, crop: s.beforeCrop ?? s.crop, video: undefined, keepMouse: true, keepFocus: true }, `${s.name}-${st.snap}-${scheme}`);
            }
            break;
          } catch (e) { if (attempt) throw e; console.log('retry', s.name, e.message.slice(0, 200)); }
        }
        deadline = Infinity;
        const clip = await still(s, `${s.name}-${scheme}`, [w, h]);
        console.log(`ok   ${s.name} ${scheme} ${Math.round(clip.width)}x${Math.round(clip.height)}${s.video ? ' + video' : ''}`);
      } catch (e) {
        failed = true;
        if (process.env.PROBE) console.log('PROBE', JSON.stringify(await js(process.env.PROBE).catch((e) => String(e))));
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
