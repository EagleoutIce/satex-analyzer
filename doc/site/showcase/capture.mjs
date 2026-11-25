// The landing page's showcase images, taken from a built playground: per entry of shots.json, its
// example opened, its steps run, the widget cropped, in light and dark, encoded as AVIF, WebP and PNG.
//   cargo playground --dev --port 8123 & node doc/site/showcase/capture.mjs http://127.0.0.1:8123/
// Entries with `"server": true` need the compile server of `cargo playground`: build.py runs `satex-playground --dev` for them.
// ONLY=a,b takes the entries of those names, SCHEMES=dark one theme. RAW=dir keeps a full screenshot per entry.
// E2E_SITE_SRC=dir serves the page's scripts and style from that doc/site (a work copy's).
// An entry's `open` is the widget its try-it link opens (playground.js `satexOpen`); a step `{ "open": true }` opens it
// the same way. Steps are data (../e2e/page.mjs): the page is only asked `satexTest.run(<JSON>)` (../e2e/probe.js). A step with `snap` keeps the crop at that point as `<name>-<snap>` (a refactoring's before), `video`
// steps are recorded after the other steps into a looping WebM and MP4 whose first frame is the still.
// Needs Chromium ($CHROME or Playwright's), ImageMagick (`magick`), `avifenc`, `pngquant` and `ffmpeg`.
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { execFileSync, spawn } from 'node:child_process';
import { cpus, homedir, loadavg } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { driver, PROBE } from '../e2e/page.mjs';
import { siteBody } from '../e2e/lib.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const base = process.argv[2] ?? 'http://127.0.0.1:8000/';
const out = process.argv[3] ?? join(here, 'img');
const { width: WIDTH, video: VIDEO = 1000, budget: BUDGET = 90000, shots } = JSON.parse(readFileSync(join(here, 'shots.json'), 'utf8'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = false;
// waits and budgets stretch with the machine's load per core (at least 1) and with the pass: the last retry gets twice as long
const load = () => Math.max(1, loadavg()[0] / cpus().length);
let scale = 1;

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
let deadline = Infinity;
const send = (method, params) => raw(method, params).then((r) => { if (r.error) throw new Error(`${method}: ${r.error.message}`); return r.result; });
const drive = driver(send, { scale: () => scale, deadline: () => deadline });

/** `config`: satex.yaml settings the entry's page starts with (as the Settings dialog keeps them), e.g. `playground.wrap`. */
async function open(file, config = null) {
  // a fresh page per entry: an example seen before comes back with its edits (drafts stay in memory), and a view left open or a run not yet in skews the next
  await raw('Page.navigate', { url: 'about:blank' }); await sleep(500); // the old page's last draft write lands before the clear
  // each entry starts from the example as shipped: no draft, no kept view
  await raw('Storage.clearDataForOrigin', { origin: new URL(base).origin, storageTypes: 'local_storage,indexeddb' });
  await raw('Page.navigate', { url: `${base}play/index.html#${file}` });
  const ready = () => drive.until({ sel: '#play-status[data-done=ready]' }, 240000);
  if (!await ready()) throw Object.assign(new Error(`the playground never started; the page: ${await drive.state()}`), { wait: true });
  // the settings go in as the Settings dialog keeps them, read when the page starts: once more from the top
  if (config) {
    await send('DOMStorage.enable', {});
    await send('DOMStorage.setDOMStorageItem', { storageId: { securityOrigin: new URL(base).origin, isLocalStorage: true }, key: 'satex-config', value: JSON.stringify(config) });
    await send('Page.reload', {}); await sleep(1500); await ready();
  }
  const replace = { sel: 'dialog[open] button', exact: 'Replace' }, shown = { sel: '#play-tabs [aria-selected=true]', text: file };
  await drive.until({ any: [shown, replace] }, 60000);
  await drive.act({ press: replace });
  if (!await drive.until([shown, 'quiet'], 120000)) throw Object.assign(new Error(`${file} never came to rest after it opened; the page: ${await drive.state()}`), { wait: true });
  // a view the last entry left open comes back with the page
  await drive.act({ close: 'dialog[open]' });
}
/** Replaces the open file's text with `text` and waits for its run. */
async function write(text) {
  await drive.act({ text });
  await sleep(1500);
  await drive.until('quiet', 120000);
  await sleep(1000);
}
/** The union box of `crop` (selectors, in the page) and `pad` px around, in CSS pixels; an error where the viewport cuts it. */
/** `{ lines: [a, b] }` in a crop: the editor's lines from the one holding `a` to the one holding `b`. */
const box = async (crop, pad) => {
  const b = await drive.run({ crop: { parts: [crop].flat(), pad } });
  if (b?.cut) throw new Error(`the viewport cuts ${JSON.stringify(crop)}: make the entry's viewport larger`);
  if (b) delete b.cut;
  return b;
};
const union = (a, b) => { const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y); return { x, y, width: Math.max(a.x + a.width, b.x + b.width) - x, height: Math.max(a.y + a.height, b.y + b.height) - y, scale: 1 }; };
const capture = async (clip) => Buffer.from((await raw('Page.captureScreenshot', { format: 'png', ...(clip ? { clip } : {}), captureBeyondViewport: false })).result.data, 'base64');

/** The `video` steps recorded as screencast frames, then cut to the union of the crop before and after,
 * `<stem>.webm` beside the still (its first frame); build.py derives the MP4. */
/** The longest a frame stands in a clip, in seconds (`still` of an entry overrides it). */
const STILL = 1.2;
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
    if (s.cursor) await drive.act({ cursor: true });
    await raw('Page.startScreencast', { format: 'png', maxWidth: w * 2, maxHeight: h * 2, everyNthFrame: 1 });
    await sleep(300);
    // a modal that opens on the way (a widget's dialog) is part of the take: the crop grows to hold it whole
    let modal = null;
    for (const st of s.video) {
      await drive.step({ ms: 45000, ...st }, { open: s.open, dir: here });
      const m = await box('dialog[open]', 0);
      if (m) modal = modal ? union(modal, m) : m;
    }
    await sleep(s.hold ?? 900);
    await raw('Page.stopScreencast');
    events.delete('Page.screencastFrame');
    const clip = [await box(s.crop, s.pad ?? 0), modal].filter(Boolean).reduce(union, before);
    // the timeline: each frame as long as it stood (shorter by the entry's `speed`), the last one a beat longer; a frame
    // that stood longer than `still` seconds (a wait for a build: the screencast sends no frame while nothing changes)
    // stands `still` only, so the clip never seems to hang (frozen stretches of 3-4.5 s before, freezedetect)
    const most = s.still ?? STILL;
    const list = frames.map(([f, t], i) => `file '${f}'\nduration ${Math.max(0.02 / (s.speed ?? 1), Math.min(most, ((frames[i + 1]?.[1] ?? t + .5) - t) / (s.speed ?? 1))).toFixed(3)}`).join('\n') + `\nfile '${frames.at(-1)[0]}'\n`;
    writeFileSync(join(dir, 'list.txt'), list);
    // frames of either source to one size first, then cut
    const crop = `scale=${w * 2}:${h * 2},crop=${Math.round(clip.width * 2)}:${Math.round(clip.height * 2)}:${Math.round(clip.x * 2)}:${Math.round(clip.y * 2)}`, wide = Math.min(VIDEO, Math.round(clip.width * 2)) & ~1;
    const vf = `${crop},scale=${wide}:-2:flags=lanczos,fps=25,format=yuv420p`;
    const input = ['-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', join(dir, 'list.txt')];
    // a keyframe every 2 s: a loop's restart and a resumed clip decode from near where they are
    execFileSync('ffmpeg', [...input, '-vf', vf, '-c:v', 'libvpx-vp9', '-crf', '40', '-b:v', '0', '-g', '50', '-row-mt', '1', '-deadline', 'good', '-cpu-used', '2', '-an', `${stem}.webm`]);
    // the still: the first frame, cut alike
    const still = join(dir, 'still.png');
    execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-i', frames[0][0], '-vf', crop, still]);
    return [readFileSync(still), clip];
  } finally {
    // a failed take must not leave the screencast on: the retake would start on a running one
    await raw('Page.stopScreencast').catch(() => {});
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
  if (!s.keepMouse) await drive.mouse('mouseMoved', [1, 1]);
  if (!s.keepFocus) await drive.act({ blur: true });
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
  // the script below runs in each new document only with the Page domain on
  await raw('Page.enable');
  await raw('Page.addScriptToEvaluateOnNewDocument', { source: PROBE });
  // E2E_SITE_SRC=<dir>: the page's scripts and style from a work copy's doc/site, the rest from the server (as the e2e suite)
  if (process.env.E2E_SITE_SRC) {
    const local = siteBody(process.env.E2E_SITE_SRC);
    events.set('Fetch.requestPaused', ({ requestId, request }) => {
      const body = local(request.url);
      if (!body) return raw('Fetch.continueRequest', { requestId });
      raw('Fetch.fulfillRequest', { requestId, responseCode: 200, responseHeaders: [{ name: 'Content-Type', value: request.url.includes('.css') ? 'text/css' : 'application/javascript' }, { name: 'Cache-Control', value: 'no-cache' }], body: Buffer.from(body).toString('base64') });
    });
    await raw('Fetch.enable', { patterns: [{ urlPattern: '*.js*' }, { urlPattern: '*.css*' }] });
    await raw('Network.enable');
    await raw('Network.setBypassServiceWorker', { bypass: true });
  }
  // `cargo playground --dev` reloads the page when a site file changes: a take keeps its page
  await raw('Page.addScriptToEvaluateOnNewDocument', { source: `const E = self.EventSource; if (E) self.EventSource = class extends E { constructor(u, o) { super(String(u).includes('/__dev/') ? 'data:,' : u, o); } };` });
  mkdirSync(out, { recursive: true });
  const schemes = (process.env.SCHEMES ?? 'light,dark').split(',');
  const failures = [];
  const only = shots.filter((s) => !s.live).filter((s) => !process.env.ONLY || process.env.ONLY.split(',').includes(s.name));
  /** One entry in one theme; a failure is kept for the retry pass. */
  async function take(s, scheme, pass) {
    scale = load() * pass;
    await raw('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scheme }, { name: 'prefers-reduced-motion', value: 'reduce' }] });
    const [w, h] = s.viewport ?? [1080, 700];
    await raw('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 2, mobile: false });
    try {
      // a first visit may miss the widget while the page still fetches what it shows: one more try, from a fresh page with a fresh budget
      for (let attempt = 0; ; attempt++) {
        try {
          deadline = Infinity;
          await open(s.file, s.config);
          // the steps share one budget: a failing wait fails fast, never one timeout per step
          deadline = Date.now() + (s.budget ?? BUDGET) * scale;
          if (s.text) await write(s.text);
          for (const st of s.steps) {
            await drive.step({ ms: 45000, ...st }, { open: s.open, dir: here });
            if (st.snap) await still({ ...s, crop: s.beforeCrop ?? s.crop, video: undefined, keepMouse: true, keepFocus: true }, `${s.name}-${st.snap}-${scheme}`);
          }
          break;
        } catch (e) { if (attempt) throw e; console.log('retry', s.name, e.message.slice(0, 200)); }
      }
      deadline = Infinity;
      const clip = await still(s, `${s.name}-${scheme}`, [w, h]);
      console.log(`ok   ${s.name} ${scheme} ${Math.round(clip.width)}x${Math.round(clip.height)}${s.video ? ' + video' : ''}${pass > 1 ? ' (second pass: the first was too slow under load)' : ''}`);
    } catch (e) {
      deadline = Infinity;
      if (process.env.RAW) writeFileSync(join(process.env.RAW, `${s.name}-${scheme}-fail.png`), Buffer.from((await raw('Page.captureScreenshot', { format: 'png' })).result.data, 'base64'));
      failures.push([s, scheme, e]);
    }
  }
  for (const scheme of schemes) for (const s of only) await take(s, scheme, 1);
  // what failed gets a last go with twice the time: a wait that holds now was load, one that fails again never held
  for (const [s, scheme] of failures.splice(0)) await take(s, scheme, 2);
  for (const [s, scheme, e] of failures) {
    failed = true;
    const t = Math.round((s.budget ?? BUDGET) * scale / 1000);
    console.log(`FAIL ${s.name} ${scheme}: ${e.wait ? `${e.message.slice(0, 300)}; steps budget ${t} s at load ${load().toFixed(1)}/core` : e.message.slice(0, 300)}`);
  }
} finally {
  proc.stdio[3].end();
  proc.kill();
  await sleep(500);
  rmSync(profile, { recursive: true, force: true, maxRetries: 5 });
}
process.exit(failed ? 1 : 0);
