// The scenario kinds of the playground's browser e2e suite (doc/site/e2e/README.md): each `kind` takes a page
// (lib.mjs `page`) and a scenario and resolves to `{ name, fails, perf, info, shot? }`. `fails` are correctness
// failures; `perf` are timing bounds, failures only in the perf tier (machine load moves them).
import { writeFileSync, mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { sleep } from './lib.mjs';

/** Where screenshots, crash repros and dumps go: the runner sets it per run. */
export const ctx = { out: 'target/e2e/out' };
const EXAMPLES = new URL('../examples/', import.meta.url);

/** Deterministic delays: a seeded LCG. */
let seed = 12345;
/** The delays start over per scenario: a scenario types the same whatever ran before it. */
export const reseed = (s = 12345) => { seed = s; };
const rand = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);

/** The source after `keys` typed at `at` of `text`, as plain prose takes them; null where a key has no fixed meaning. */
function expectEdit(text, at, keys) {
  for (const k of keys) {
    if (k === 'Pause') continue;
    if (k === 'Backspace') { text = text.slice(0, at - 1) + text.slice(at); at--; }
    else if (k === 'Ctrl+Backspace') { const n = /[\p{L}\p{N}]*[ \t]*$/u.exec(text.slice(0, at))[0].length || 1; text = text.slice(0, at - n) + text.slice(at); at -= n; }
    else if (k === 'Enter') { text = text.slice(0, at) + '\n\n' + text.slice(at); at += 2; }
    else if (k.length === 1 && !/[\\$_^{}%&#~]/.test(k)) { text = text.slice(0, at) + k + text.slice(at); at++; }
    else return null;
  }
  return { text, at };
}
/** The keys of a scenario: strings split into characters, named keys kept. */
// `Pause`: no key, a rest of `sc.pauseMs` (a build starts while the person thinks, then lands while they type on)
// `MathDisplay`: the Type bar's display-math control (typing `\\[` is refused: it would change the source's structure)
const NAMED = /^(Backspace|Enter|Escape|ArrowLeft|ArrowRight|ArrowUp|ArrowDown|Tab|Ctrl\+Backspace|Pause|MathDisplay)$/;
const keysOf = (seq) => seq.flatMap((x) => (NAMED.test(x) ? [x] : [...x]));

const J = JSON.stringify;
/** The served roots, read once at the start before any edit. */
const ROOTS = {};
async function load(p, sc) {
  await p.eval(`document.querySelector('[data-example=${J(sc.example ?? 'thesis.tex')}]').click()`);
  await p.until(`satexHealth().example === ${J(sc.example ?? 'thesis.tex')} && satexHealth().file === ${J(sc.example ?? 'thesis.tex')} && satexHealth().text?.length > 10`, 20000);
  await sleep(400);
  // the example's root file as given (an earlier scenario may have left it edited), else the scenario's own text
  if (sc.source) await setText(p, sc.source);
  // `settle`: the first run (a fresh profile builds the kernel cache first, ~90 s) ends before the typing: what the
  // page keeps from it (the completion lists) is there, and the typing's own run is the one going on
  if ((sc.settle || process.env.E2E_SETTLE) && !await settled(p)) throw new Error('the first run never ended');
  await p.eval(`document.getElementById('play-toast') && (document.getElementById('play-toast').textContent = '')`);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await sleep(200);
  const before = await p.eval(`(self.__e2eL = document.querySelector('#play-pdf .pdf-page img')?.__build ?? null, 0)`);
  await p.eval(`document.getElementById('pdf-compile').click()`);
  const ok = await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return satexPdfWords().length && i?.naturalWidth > 100 && i.__build && (i.__build !== self.__e2eL || /in cache/.test(document.getElementById('pdf-status').textContent)) && !document.querySelector('#play-pdf.pdf-running') && (() => { const ws = satexPdfWords(), test = ${sc.word ?? '() => true'}; return ws.some((w) => test(w[5], w, ws)) && [...document.querySelectorAll('#play-pdf .pdf-page img')].every((x) => x.__build === i.__build || !x.__build); })(); })()`, 40000);
  await sleep(800);
  if (!ok) throw new Error(`no build of the scenario's text (status: ${await p.eval(`document.getElementById('pdf-status').textContent`)})`);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(600);
  return before;
}

/** The page's checks done (a fresh context fetches the TeX bundle first, slow on a loaded machine): what a widget or
 * menu asks of the language server is answered at once, not queued behind the fetch. */
async function idle(p, ms = 240000) { if (!await p.until(`!satexHealth().busy`, ms)) throw new Error('setup: the page stayed busy (its first check never ended)'); }
/** No run going on, due or queued for 1.5 s (a fresh profile's first run builds the kernel cache: minutes at most). */
const settled = (p, ms = 240000) => p.until(`(() => { const h = satexHealth(); if (h.running || h.busy || h.due || h.queued) { self.__e2eQ = null; return false; } self.__e2eQ ??= performance.now(); return performance.now() - self.__e2eQ > 1500; })()`, ms);
/** A word on the pages in client px: [x0, y0, x1, y1, page, text] with the page scrolled to show it. */
async function wordAt(p, test, nth = 0, pageNo = null) {
  return p.eval(`(async () => {
    const ws = satexPdfWords(), dpi = ws.dpi, test = ${test};
    const hits = ws.filter((w) => (${pageNo ?? 'null'} == null || w[0] === ${pageNo ?? 'null'}) && test(w[5], w, ws));
    const w = hits[${nth}]; if (!w) return null;
    const img = document.querySelector('#play-pdf .pdf-page[data-page="' + w[0] + '"] img'), sc = document.querySelector('#play-pdf .pdf-pages');
    let r = img.getBoundingClientRect(), k = r.width / (img.naturalWidth * 72 / dpi);
    const scr = sc.getBoundingClientRect(), y = r.top + w[2] * k;
    if (y < scr.top + 80 || y > scr.bottom - 200) { sc.scrollTop += y - (scr.top + scr.height * 0.4); await new Promise((ok) => setTimeout(ok, 400)); }
    // the image again: a page drawn meanwhile replaced it (a detached one measures 0)
    const im = document.querySelector('#play-pdf .pdf-page[data-page="' + w[0] + '"] img'); r = im.getBoundingClientRect(); k = r.width / (im.naturalWidth * 72 / dpi);
    return [r.left + w[1] * k, r.top + w[2] * k, r.left + w[3] * k, r.top + w[4] * k, w[0], w[5], k];
  })()`);
}

/** Settled: the build shows the editor's text, nothing compiles, no echo is drawn. */
// Settled: a build newer than the click's is shown, then nothing compiles, no echo, no edit due, for 1.2 s.
const settle = (p, ms = 25000) => p.eval(`new Promise((ok) => { const t0 = performance.now(); let idle = null;
  const tick = () => { const now = performance.now(), i = [...document.querySelectorAll('#play-pdf .pdf-page img')];
    const fresh = i.some((x) => x.__build && !self.__e2eB.has(x.__build)) || now - t0 > 10000;
    const busy = !fresh || !!document.querySelector('#play-pdf.pdf-running') || !!document.querySelector('.pdf-echo') || satexHealth().due;
    if (busy) idle = null; else idle ??= now;
    if (idle != null && now - idle > 1200) ok(true); else if (now - t0 > ${ms}) ok(false); else setTimeout(tick, 50); };
  tick(); })`);

async function compileState(p) {
  return p.eval(`(() => {
    const st = document.getElementById('pdf-status').textContent, err = !!document.querySelector('#play-pdf .pdf-err, #play-pdf [data-view=messages].pdf-err');
    const cmds = [...document.querySelectorAll('.pdf-cmds li')].map((l) => l.textContent).filter((t) => /exit [1-9]/.test(t));
    const built = [...document.querySelectorAll('#play-pdf .pdf-page img')].some((x) => x.__build && !self.__e2eB?.has(x.__build));
    return { ok: !err && !cmds.length && built && !/error|fail/i.test(st), status: st.slice(0, 80), err, exit: cmds.length, built };
  })()`);
}

/** Analysis of the frames of one typing run. */
function analyse(frames, keyTimes, sc, ins, marker) {
  const r = { overlap: 0, past: 0, rows: 0, fits: new Map(), frames: frames.length, jumps: 0, maxJump: 0, scrollJumps: 0, maxScroll: 0, dup: 0, dupText: '', maxLag: 0, flicker: 0, fontBad: 0, fonts: new Set(), blank: 0, swaps: 0, back: 0 };
  let prev = null;
  for (const f of frames) {
    // the last measure of each font counts: an early frame may measure before the page's embedded face is loaded
    for (const e of f.echoes) e.fonts.forEach((x, i) => { r.fonts.add(x); if (e.fits?.[i]) r.fits.set(x, e.fits[i]); });
    // duplicates: the marker seen more than once in what the row shows (echo + uncovered words)
    if (marker && f.visible != null) {
      const n = f.visible.split(marker).length - 1;
      if (n > 1 && n - 1 > r.dup) { r.dup = n - 1; r.dupText = f.visible.slice(0, 160); r.dupFrame = f; }
    }
    // lag: keys typed minus the longest prefix of the typed text the row shows
    if (f.visible != null && ins) {
      const vis = f.visible.replace(/\s+/g, '');
      let k = f.keys;
      while (k > 0 && !vis.includes(ins[k].replace(/\s+/g, ''))) k--;
      const lag = f.keys - k;
      if (lag > r.maxLag) { r.maxLag = lag; r.lagFrame = f; }
    }
    if (prev) {
      if (prev.caret && !f.caret && f.keys > 0) r.flicker++;
      if (prev.caret && f.caret) {
        const dx = f.caret[0] - prev.caret[0], dy = f.caret[1] - prev.caret[1], dScroll = (f.scroll ?? 0) - (prev.scroll ?? 0);
        // the caret moved on screen with no key in between (a rebuild, a late draw): a jump. Scrolling moves it too: subtract.
        // (a caret whose source offset moved went with its text: keys held for a completion list and applied with it)
        const mdy = dy + dScroll;
        if (f.keys === prev.keys && f.at === prev.at && (Math.abs(dx) > 4 || Math.abs(mdy) > 3)) { r.jumps++; const m = Math.max(Math.abs(dx), Math.abs(mdy)); if (m > r.maxJump) { r.maxJump = Math.round(m); r.jumpFrame = { from: prev.caret, to: f.caret, t: Math.round(f.t), keys: f.keys, echoes: prev.echoes.map((e) => [e.kind, e.text.slice(0, 50), e.r, e.at, e.fonts.map((f) => f.slice(0, 40))]) }; } }
        // typing forward on one row: the caret never steps back
        if (f.keys > prev.keys && sc.forward && Math.abs(mdy) < 3 && dx < -4) r.back++;
      }
      // a scroll with no key (a rebuild, a late draw); one with a key follows the caret's line the key moved
      if (f.keys === prev.keys && f.scroll != null && prev.scroll != null && f.scroll !== prev.scroll) { r.scrollJumps++; r.maxScroll = Math.max(r.maxScroll, Math.abs(f.scroll - prev.scroll)); }
      for (let i = 0; i < f.imgs.length; i++) { if (prev.imgs[i] && prev.imgs[i][1] !== f.imgs[i][1]) r.swaps++; }
      if (f.caretPage && f.imgs.some((m) => String(m[0]) === f.caretPage && m[2] === 0)) r.blank++;
      // the echo's rows: never over each other, never past the text's right edge
      const rs = f.echoes.map((e) => e.r).filter(Boolean).sort((p, q) => p[1] - q[1]);
      for (let i = 1; i < rs.length; i++) if (rs[i][1] > rs[i - 1][1] + 1) r.overlap = Math.max(r.overlap, rs[i - 1][1] + rs[i - 1][3] - rs[i][1]);
      if (sc.right != null) for (const e of rs) r.past = Math.max(r.past, e[0] + e[2] - sc.right);
      r.rows = Math.max(r.rows, new Set(rs.map((e) => Math.round(e[1]))).size);
    }
    prev = f;
  }
  r.fonts = [...r.fonts];
  return r;
}

async function typeScenario(p, sc) {
  const res = { name: sc.name, kind: 'type', fails: [], perf: [], info: {} };
  await load(p, sc);
  // the check of the loaded text done first: the printer waits behind it (measured: up to ~25 s for thesis.tex)
  const idle0 = Date.now();
  await idle(p);
  res.info.analysisMs = Date.now() - idle0;
  await sleep(300);
  // the word placed after the layout settled
  const w = await wordAt(p, sc.word, sc.nth ?? 0, sc.page);
  if (!w) { if (sc.optional) { res.skip = true; res.info.note = 'no such word in this build'; return res; } res.fails.push(`setup: no page word for ${sc.word}`); return res; }
  const builds0 = await p.eval(`(self.__e2eB = new Set([...document.querySelectorAll('#play-pdf .pdf-page img')].map((i) => i.__build))).size`);
  // `gap`: in the space after the word, past its 1 bp of slack (the user's click at a word's right edge)
  const x = sc.at === 'start' ? w[0] + 0.6 : sc.at === 'gap' ? w[2] + 1.4 * w[6] : w[2] - 0.6 * w[6];
  await p.eval(`document.getElementById('play-toast') && (document.getElementById('play-toast').textContent = '')`);
  await p.mouse(x, (w[1] + w[3]) / 2);
  const c0 = await p.until(`satexPdfCaret()`, sc.clickMs ?? 3000);
  if (!c0) { res.fails.push(`click: no caret (toast: ${await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`)})`); await shot(p, res, 'nocaret'); return res; }
  await sleep(300);
  // the source as the click opened it (a word of an \input file opens that file)
  const base = await p.eval(`satexHealth().text`);
  res.info.file = await p.eval(`satexHealth().file`);
  let want0;
  if (typeof sc.src === 'function') want0 = sc.src(base, w[5]);
  else { const m = sc.src.exec(base); want0 = m ? m.index + m[0].length : -1; }
  if (want0 < 0) { res.fails.push(`setup: source pattern ${sc.src} not in the served text`); return res; }
  res.info.word = `${w[5]} p${w[4]}`;
  if (c0.at !== want0) res.fails.push(`click: caret at source ${c0.at} ${J(base.slice(c0.at - 12, c0.at))}|, want ${want0} ${J(base.slice(want0 - 12, want0))}|`);
  const at0 = c0.at;
  const keys = keysOf(sc.keys), plan = sc.expect ? null : expectEdit(base, at0, keys);
  // the inserted text after each key, for the lag measure (plain prose only)
  const ins = [''];
  { let t = base, a = at0; for (const k of keys.filter((x) => x !== 'Pause')) { const e = expectEdit(t, a, [k]); if (!e) { ins.push(null); continue; } t = e.text; a = e.at; ins.push(t.slice(at0, a).split('\n').at(-1)); } }
  const insOk = ins.every((s) => s != null) ? ins : null;
  const caretFont = await p.eval(`(() => { const e = document.querySelector('#play-pdf .pdf-caret'); return e && e.getBoundingClientRect().height; })()`);
  if (!await p.eval(`!!self.__e2e`)) throw new Error(`the page lost its e2e hooks (a reload?): ${J(await p.eval(`[location.href, document.readyState, performance.getEntriesByType('navigation')[0]?.type]`))} errors ${J(p.errors.slice(-3))}`);
  await p.eval(`(() => { const E = self.__e2e; E.frames = []; E.keys = 0; E.keyLog = []; E.marker = ${J(sc.marker ?? null)}; E.word = ${J(w[5])}; E.on = true; })()`);
  // the text block's right edge: the most common left edge and width of the page's SyncTeX line boxes (a page of
  // short lines has no word there), else its words' right end
  sc.right = await p.eval(`(() => { const ws = satexPdfWords(), img = document.querySelector('#play-pdf .pdf-page[data-page="${w[4]}"] img'), r = img.getBoundingClientRect(), k = r.width / (img.naturalWidth * 72 / ws.dpi);
    const mode = (xs) => { const n = new Map(); for (const x of xs) n.set(x, (n.get(x) ?? 0) + 1); return [...n].sort((a, b) => b[1] - a[1])[0]?.[0]; };
    const lines = satexPdfBoxes().filter((b) => b.page === ${w[4]} && b.kind === '(' && b.w > 0), left = mode(lines.map((b) => Math.round(b.x))), width = mode(lines.filter((b) => Math.round(b.x) === left).map((b) => Math.round(b.w)));
    return r.left + Math.max(left + width || 0, ...ws.filter((x) => x[0] === ${w[4]}).map((x) => x[3])) * k; })()`);
  // the echo's glyphs against the page's: font size and baseline of the typed text vs the run under the clicked word,
  // the last frame that shows typed text in an echo
  if (sc.fit) await p.eval(`(() => { self.__geo = null; const t0 = performance.now(); const tick = () => { try {
    const img = document.querySelector('#play-pdf .pdf-page[data-page="${w[4]}"] img'), ir = img.getBoundingClientRect(), k = ${w[6]};
    for (const e of document.querySelectorAll('#play-pdf canvas.pdf-echo')) {
      const t = (e.__texts ?? []).findLast((q) => /[a-z]{2}/.test(q.t) && ${J(keys.join(''))}.includes(q.t)); if (!t) continue;
      const r = e.getBoundingClientRect(), sc = r.height / e.height, ink = r.top + t.y * sc;
      // the page's run on the echo's row, ending at or before the echo's left edge: the text the echo continues
      const run = (img.__runs ?? []).filter((q) => Math.abs(ir.top + q.y * k - ink) < 12 && ir.left + q.x * k < r.left + 2).sort((p, q) => (q.x + q.w) - (p.x + p.w))[0];
      if (!run) continue;
      const px = parseFloat(/([\\d.]+)px/.exec(t.font)[1]) * sc;
      self.__geo = { size: px, base: r.top + t.y * sc, pageSize: run.size * k, pageBase: ir.top + run.y * k, font: t.font, text: t.t };
    }
    } catch (e) { self.__geo ??= { err: String(e) }; } if (performance.now() - t0 < 30000) requestAnimationFrame(tick); }; tick(); })()`);
  await p.eval(`(performance.clearMeasures('pdf-echo'), self.__e2e.keyFrame = [], self.__e2eG = satexPdfCaret()?.builds ?? 0, 0)`);
  const keyTimes = [];
  for (const k of keys) {
    if (k === 'Pause') { await sleep(sc.pauseMs ?? 1500); continue; }
    keyTimes.push(Date.now());
    if (k === 'MathDisplay') await p.eval(`self.__e2e.keys++, document.querySelector('#play-pdf .type-math[data-math=display]').click()`); else await p.key(k);
    // `keyMs` [lo, hi]: a slower hand (the showcase types at 70-130 ms)
    if (sc.speed !== 'burst') await sleep(sc.keyMs ? sc.keyMs[0] + Math.round(rand() * (sc.keyMs[1] - sc.keyMs[0])) : 20 + Math.round(rand() * 40));
  }
  const typedAt = Date.now();
  // the live math preview, before the build lands: each glyph drawn into an echo canvas on screen, then the pairs
  // `sc.math` asks for ([a, b, how]: b a subscript or superscript of a, a stacked over b, b on a row below a) and
  // `sc.italic` (letters in a face other than the prose's: math italic)
  if (sc.math) {
    await sleep(120);
    const gs = await p.eval(`[...document.querySelectorAll('#play-pdf canvas.pdf-echo')].flatMap((c) => { const r = c.getBoundingClientRect(), k = r.width / Math.max(1, c.width);
      const ir = c.closest('.pdf-page').querySelector('img').getBoundingClientRect(), page = +c.closest('.pdf-page').dataset.page;
      return (c.__texts ?? []).map((q) => ({ t: q.t, font: q.font, px: parseFloat(/([\\d.]+)px/.exec(q.font)?.[1] ?? '0') * k * (q.sy ?? 1), x: r.left + q.x * k, y: r.top + q.y * k, page, px0: r.left + q.x * k - ir.left, py0: r.top + q.y * k - ir.top })); })`);
    res.mathGs = gs;
    res.info.mathGlyphs = gs.filter((g) => g.t.length <= 2).map((g) => `${g.t}@${Math.round(g.x)},${Math.round(g.y)}/${g.px.toFixed(1)}`).join(' ');
    const prose = gs.find((g) => /^[a-z]{3,}$/.test(g.t))?.font.replace(/^[\d.]+px /, '').replace(/^\S+px /, '');
    const at = (t) => gs.filter((g) => g.t === t).at(-1);
    for (const [a, b, how] of sc.math) {
      const A = at(a), B = at(b);
      if (!A || !B) { res.fails.push(`math preview: no ${!A ? a : b} drawn (${res.info.mathGlyphs.slice(0, 200)})`); continue; }
      const ok = how === 'sub' ? B.y > A.y + 0.1 * A.px && B.px < 0.8 * A.px : how === 'sup' ? B.y < A.y - 0.2 * A.px && B.px < 0.8 * A.px
        : how === 'over' ? A.y < B.y - 0.4 * A.px && Math.abs(A.x - B.x) < A.px : B.y > A.y + A.px;
      if (!ok) res.fails.push(`math preview: ${b} not ${how} ${a}: ${J([A, B].map((g) => [g.t, Math.round(g.x), Math.round(g.y), +g.px.toFixed(1), g.font.slice(0, 40)]))}`);
    }
    for (const t of sc.italic ?? []) { const g = at(t); if (g && !/italic/.test(g.font) && g.font.includes(prose ?? '\0')) res.fails.push(`math preview: ${t} upright in the prose face (${g.font.slice(0, 50)})`); }
    const c = await p.eval(`(() => { const r = document.querySelector('#play-pdf .pdf-caret')?.getBoundingClientRect(); return r && [r.left, r.top]; })()`);
    const col = await p.eval(`(() => { const r = document.querySelector('#play-pdf .pdf-caret')?.closest('.pdf-page')?.getBoundingClientRect(); return r && [r.left, r.width]; })()`);
    if (c && col) res.mathClip = { x: Math.max(0, col[0] + col[1] * 0.12), y: Math.max(0, c[1] - 80), width: col[1] * 0.76, height: 170 };
    if (res.mathClip) res.info.mathShot = await p.shot(`${sc.name}-math.png`, res.mathClip, 3);
  }
  // the cost of each echo draw (playground's `pdf-echo` measures) and each key's wait for its frame, while typing
  const perf = await p.eval(`(() => { const q = (xs) => { xs = [...xs].sort((a, b) => a - b); return xs.length ? { n: xs.length, p50: +xs[xs.length >> 1].toFixed(1), p95: +xs[Math.floor(xs.length * 0.95)].toFixed(1), max: +xs.at(-1).toFixed(1) } : null; };
    const d = performance.getEntriesByName('pdf-echo').map((e) => e.duration);
    return { draw: q(d.slice(1)), first: d.length ? +d[0].toFixed(1) : null, frame: q(self.__e2e.keyFrame) }; })()`);
  // the first draw after the click fills the caches (the paragraph's words, the font's glyphs): counted apart
  // `foot`: no echo row below its page's text area (the lowest SyncTeX line box), and rows on the next page
  if (sc.foot) {
    const ft = await p.eval(`(() => { const ws = satexPdfWords(), bs = satexPdfBoxes(), out = { past: [], pages: {} };
      for (const cv of document.querySelectorAll('#play-pdf .pdf-echo:not(.pdf-push)')) {
        const pg = +cv.closest('.pdf-page').dataset.page, img = cv.closest('.pdf-page').querySelector('img'), ir = img.getBoundingClientRect(), k = ir.width / (img.naturalWidth * 72 / ws.dpi);
        // the text area's foot: the lowest line, else 30pt (article's \\footskip) over a page number below the text
        const lines = bs.filter((b) => b.page === pg && b.kind === '(').map((b) => b.y + b.d), num = ws.filter((w) => w[0] === pg && w[2] > Math.max(...lines) + 18 && /^\\d+$/.test(w[5])).at(-1);
        const foot = Math.max(...lines, num ? num[4] - 30 : 0), r = cv.getBoundingClientRect();
        const text = (cv.__texts ?? []).map((q) => q.t).join(' ').trim();
        if (text) out.pages[pg] = (out.pages[pg] ?? '') + ' ' + text;
        if (text && r.bottom > ir.top + foot * k + 2) out.past.push([pg, Math.round(r.bottom - (ir.top + foot * k)), text.slice(0, 40)]);
      }
      return out; })()`);
    res.info.echoPages = Object.fromEntries(Object.entries(ft.pages).map(([k, v]) => [k, v.trim().slice(0, 80)]));
    res.info.echoCanvases = await p.eval(`[...document.querySelectorAll('#play-pdf .pdf-echo')].map((c) => c.closest('.pdf-page').dataset.page + ':' + c.className + ':' + (c.__texts ?? []).map((q) => q.t).join(' ').slice(0, 30) + ':' + c.style.top)`);
    if (ft.past.length) res.fails.push(`echo below the text area: ${J(ft.past)}`);
    if (sc.foot.next && !Object.keys(ft.pages).some((k) => +k > sc.foot.from)) res.fails.push(`no echo row on the page after ${sc.foot.from}: ${J(res.info.echoPages)}`);
  }
  res.info.drawMs = perf.draw; res.info.firstDrawMs = perf.first; res.info.keyFrameMs = perf.frame;
  // builds started while the keys went on: a scenario typing through builds needs them to mean anything
  res.info.buildsWhileTyping = await p.eval(`(satexPdfCaret()?.builds ?? 0) - self.__e2eG`);
  if (sc.minBuilds && res.info.buildsWhileTyping < sc.minBuilds) res.fails.push(`setup: ${res.info.buildsWhileTyping} builds while typing, want ${sc.minBuilds}`);
  if (perf.draw && perf.draw.p95 > (sc.drawMs ?? 8)) res.perf.push(`echo draw p95 ${perf.draw.p95} ms (max ${perf.draw.max}), want <= ${sc.drawMs ?? 8}`);
  const geo = sc.fit ? await p.eval(`self.__geo`) : null;
  if (sc.fit) { const c = await p.eval(`(() => { const r = document.querySelector('#play-pdf .pdf-caret')?.getBoundingClientRect(); return r && [r.left, r.top]; })()`); if (c) await p.shot(`${sc.name}-typed.png`, { x: Math.max(0, c[0] - 380), y: c[1] - 40, width: 480, height: 90 }); }
  // the printer writes each key into the source: how long until the last one landed
  if (plan) { const got = await p.until(`satexHealth().text === ${J(plan.text)}`, 45000); res.info.writeMs = got ? Date.now() - typedAt : -1; }
  else { await p.until(`satexHealth().text !== ${J(base)} && !satexHealth().due`, 20000); }
  const settled = await settle(p);
  await sleep(500);
  await p.eval(`self.__e2e.on = false`);
  const settleMs = Date.now() - typedAt;
  // the frames, each with what its caret row shows: echo texts and the build's words not under an echo
  const frames = await p.eval(`(() => {
    const E = self.__e2e;
    return E.frames;
  })()`);
  if (process.env.E2E_DUMPFRAMES) writeFileSync(join(ctx.out, `${sc.name}-boxes.json`), JSON.stringify(await p.eval(`({ text: satexHealth().text, built: document.querySelector('#play-pdf .pdf-page img')?.__build?.text, caret: satexPdfCaret(), boxes: satexPdfBoxes(), words: satexPdfWords() })`)));
  if (process.env.E2E_DUMPFRAMES) writeFileSync(join(ctx.out, `${sc.name}-frames.json`), JSON.stringify(frames));
  res.info.texBreakStats = await p.eval(`satexHealth().texBreaks ?? null`);
  // `sc.noEcho`: text no echo may show (a macro's argument taken for typed text)
  for (const f of sc.noEcho ? frames : []) { const e = f.echoes?.find((e) => sc.noEcho.test(e.text)); if (e) { res.fails.push(`echo shows ${sc.noEcho}: ${J(e.text.slice(0, 80))}`); break; } }
  // visible text needs the words: computed per distinct (page, build) once
  const words = await p.eval(`(() => { const out = {}; for (const i of document.querySelectorAll('#play-pdf .pdf-page img')) { const b = i.__build; out[i.closest('.pdf-page').dataset.page] = { k: i.getBoundingClientRect().width / (i.naturalWidth * 72 / satexPdfWords().dpi), l: i.getBoundingClientRect().left, t: i.getBoundingClientRect().top }; } return out; })()`);
  // without per-frame word lists the row text comes from the echo alone while one is drawn, else from the final build
  const finalRow = await p.eval(`(() => { const c = document.querySelector('#play-pdf .pdf-caret'); if (!c) return null; const r = c.getBoundingClientRect(), pg = c.closest('.pdf-page').dataset.page, img = c.closest('.pdf-page').querySelector('img'), ir = img.getBoundingClientRect(), k = ir.width / (img.naturalWidth * 72 / satexPdfWords().dpi);
    return satexPdfWords().filter((w) => String(w[0]) === pg && Math.abs((ir.top + (w[2] + w[4]) / 2 * k) - (r.top + r.height / 2)) < r.height * 1.6).map((w) => w[5]).join(' '); })()`);
  for (const f of frames) f.visible = f.echoes.map((e) => e.text).join(' ') + ' ' + (f.rowWords ?? '');
  const a = analyse(frames, keyTimes, sc, insOk, sc.marker);
  a.geo = geo;
  // the final state
  const text = await p.eval(`satexHealth().text`), c1 = await p.eval(`satexPdfCaret()`), comp = await compileState(p);
  if (!settled) res.fails.push(`settle: not settled ${settleMs} ms after the last key (running/echo/stale build)`);
  const toast = await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`), logs = p.consoleLog.splice(0);
  // the language server stopped: the texts it was sent and its last words, kept for main to replay
  if (/language server stopped|unreachable/i.test(toast + frames.map((f) => f.toast).join(' ') + logs.join(' ') + p.workerLog.join(' '))) await saveCrash(p, sc.name, logs);
  res.info.toast = toast.slice(0, 160); res.info.console = logs.slice(-3);
  if (res.info.writeMs > (sc.writeMs ?? 200)) res.perf.push(`source write lag: the last key reached the source ${res.info.writeMs} ms after it was typed`);
  if (plan) {
    if (text !== plan.text) res.fails.push(`source: ${diff(plan.text, text)}${toast ? ` (toast: ${J(toast.slice(0, 120))})` : ''}${logs.length ? ` (console: ${J(logs.slice(-2))})` : ''}`);
    if (c1 && c1.at !== plan.at) res.fails.push(`caret offset ${c1.at}, want ${plan.at}`);
  } else if (sc.expect) {
    const want = sc.expect(base, at0);
    if (want != null && text !== want) res.fails.push(`source: ${diff(want, text)}`);
  }
  // the build landed: the words `sc.land` stand where the preview drew them (left edge, baseline within a glyph's descent)
  for (const t of sc.land ?? []) {
    const g = res.mathGs?.filter((q) => q.t === t).at(-1);
    if (!g) continue;
    const at = await p.eval(`(() => { const ws = satexPdfWords(), img = document.querySelector('#play-pdf .pdf-page[data-page="${g.page}"] img'), r = img.getBoundingClientRect(), k = r.width / (img.naturalWidth * 72 / ws.dpi);
      return ws.filter((w) => w[0] === ${g.page} && w[5].startsWith(${J(t)})).map((w) => [w[1] * k, w[4] * k, w[5]]).sort((p, q) => Math.hypot(p[0] - ${g.px0}, p[1] - ${g.py0}) - Math.hypot(q[0] - ${g.px0}, q[1] - ${g.py0}))[0] ?? null; })()`);
    if (!at) { res.fails.push(`landed: no page word ${J(t)}`); continue; }
    const dx = at[0] - g.px0, dy = at[1] - g.py0;
    (res.info.land ??= []).push(`${t} dx ${dx.toFixed(1)} dy ${dy.toFixed(1)}`);
    if (Math.abs(dx) > (sc.landPx ?? 4) || dy < -1 || dy > 0.35 * g.px + 1) res.fails.push(`landed: ${J(t)} moved by ${dx.toFixed(1)},${dy.toFixed(1)} px from the preview`);
  }
  // the landed build where the preview was shot: the two side by side show a landing jump
  if (res.mathClip) res.info.landShot = await p.shot(`${sc.name}-landed.png`, res.mathClip, 3);
  for (const s of sc.contains ?? []) if (!(s instanceof RegExp ? s.test(text) : text.includes(s))) res.fails.push(`source lacks ${J(s)}: ${J(text.slice(at0 - 20, at0 + 40))}`);
  if (!c1) res.fails.push('caret gone after typing');
  if (!comp.ok) res.fails.push(`compile: ${J(comp)}`);
  if (a.dup) res.fails.push(`echo duplicated ${J(sc.marker)} ${a.dup}x: ${J(a.dupText)}`);
  if (a.jumps) res.fails.push(`caret jumped ${a.jumps}x without a key (max ${a.maxJump}px, ${J(a.jumpFrame)})`);
  if (a.back) res.fails.push(`caret stepped back ${a.back}x while typing forward`);
  if (a.scrollJumps) res.fails.push(`pages scrolled ${a.scrollJumps}x (max ${a.maxScroll}px) while typing`);
  if (a.overlap > 3) res.fails.push(`echo rows overlap by ${a.overlap.toFixed(1)}px`);
  if (a.past > 4) res.fails.push(`echo ${a.past.toFixed(1)}px past the text's right edge`);
  if (sc.rows && a.rows < sc.rows) res.fails.push(`echo wrapped into ${a.rows} rows, want ${sc.rows}`);
  if (a.flicker) res.fails.push(`caret vanished ${a.flicker}x mid-typing`);
  if (a.blank) res.fails.push(`caret page blank in ${a.blank} frames`);
  // `ever`: echo texts some frame must show (`x 42`: x and its subscript drawn apart, pdfMathPaint)
  for (const re of sc.ever ?? []) if (!frames.some((f) => f.echoes.some((e) => re.test(e.text)))) res.fails.push(`no frame shows ${re}`);
  // `never`: echo texts no frame may show (a garbled row)
  for (const re of sc.never ?? []) { const bad = frames.filter((f) => f.echoes.some((e) => re.test(e.text))); if (bad.length) res.fails.push(`${bad.length} frames show ${re}: ${J(bad[0].echoes.map((e) => [e.kind, e.text.slice(0, 60)]))}`); }
  if (sc.maxLag != null && a.maxLag > sc.maxLag) res.fails.push(`echo lag ${a.maxLag} chars (allowed ${sc.maxLag})`);
  // the echo's font against the page's: the clicked word drawn in the echo font must be as wide as the page set it
  // the echo's font against the page's: the clicked word in each echo font, measured while it was drawn (the page's
  // embedded faces go with their PDF), as wide as the page set it; the style asked for, unless it is the page's own
  // embedded face (pdf.js's `g_…`), which is the very font the page used
  // math scenarios check their fonts by the glyphs' faces (sc.italic): a formula's fonts are not the word's
  if (a.fonts.length && !sc.math) {
    const pageW = w[2] - w[0], ratio = (f) => (a.fits.get(f) ?? 0) / pageW;
    const bad = a.fonts.filter((f) => Math.abs(ratio(f) - 1) > (sc.fit ?? 0.12));
    const style = sc.style ? a.fonts.filter((f) => !sc.style.test(f) && !/px '?"?g_/.test(f)) : [];
    if (bad.length || style.length) { a.fontBad = bad.length + style.length; res.fails.push(`echo font: ${J(a.fonts.map((f) => f.slice(0, 60)))} width ratio ${a.fonts.map((f) => ratio(f).toFixed(2)).join('/')}${style.length && sc.style ? ` not ${sc.style}` : ''}`); }
  }
  if (a.geo?.err) res.fails.push(`geo: ${a.geo.err}`);
  if (sc.fit && a.geo?.size) {
    const g = a.geo;
    if (Math.abs(g.size / g.pageSize - 1) > sc.fit) res.fails.push(`echo glyph size ${g.size.toFixed(1)}px, page ${g.pageSize.toFixed(1)}px (${g.font.slice(0, 50)})`);
    if (Math.abs(g.base - g.pageBase) > sc.fit * g.pageSize) res.fails.push(`echo baseline ${g.base.toFixed(1)}, page ${g.pageBase.toFixed(1)} (off ${(g.base - g.pageBase).toFixed(1)}px)`);
  } else if (sc.fit) res.fails.push(`no echo seen to measure (echoes: ${J(frames.flatMap((f) => f.echoes.map((e) => e.kind + ':' + e.text + ':' + e.fonts.join('|'))).filter((x, i, l) => l.indexOf(x) === i).slice(-4))})`);
  // the final caret at the expected glyph boundary of the final build
  if (sc.endWord && c1) {
    const e = await wordAt(p, sc.endWord, 0, null);
    const cr = await p.eval(`(() => { const e = document.querySelector('#play-pdf .pdf-caret'); if (!e) return null; const r = e.getBoundingClientRect(); return [r.left, r.top + r.height / 2]; })()`);
    if (!e) res.fails.push(`final: no page word ${sc.endWord}`);
    else res.info.endRect = e.slice(0, 4).map((v) => Math.round(v * 10) / 10);
    if (e && cr) {
      // `endBack`: bp before the word's end (a `.` after the caret: cmr10's is 2.78pt)
      const ex = sc.endAt === 'start' ? e[0] : e[2] - (sc.endBack ?? 0) * e[6];
      if (Math.abs(cr[0] - ex) > 2.5 || cr[1] < e[1] - 2 || cr[1] > e[3] + 2) res.fails.push(`final caret at ${cr.map(Math.round)}, want ${Math.round(ex)},${Math.round((e[1] + e[3]) / 2)} (${sc.endAt ?? 'end'} of ${e[5]})`);
    }
  }
  res.info.texBreaks = await p.eval('JSON.stringify((self.__texBreaks ?? []).splice(0))').catch(() => null);
  res.info = { ...res.info, echoRows: a.rows, keys: keys.length, frames: a.frames, maxLag: a.maxLag, jumps: a.jumps, maxJump: a.maxJump, dup: a.dup, scroll: a.scrollJumps, swaps: a.swaps, fontBad: a.fontBad, settleMs, compile: comp.ok, finalRow: finalRow?.slice(0, 100) };
  if (res.fails.length) await shot(p, res, 'final', a.dupFrame || a.jumpFrame);
  // a fresh context per scenario: nothing to restore
  return res;
}

/** The crash repro: every text the server was sent (one file each, numbered in order), the timeline, the workers' log. */
async function saveCrash(p, name, logs = []) {
  const dir = join(ctx.out, 'crash', name);
  mkdirSync(dir, { recursive: true });
  const lsp = await p.eval(`self.__e2e.lsp`), line = [];
  lsp.forEach((m, i) => {
    const f = m.text != null ? `${String(i).padStart(4, '0')}-${(m.uri ?? '').split('/').pop()}-v${m.v}.txt` : '';
    if (f) writeFileSync(join(dir, f), m.text);
    line.push(`${Math.round(m.t)} ms ${m.method}${m.uri ? ` ${m.uri.split('/').pop()} v${m.v}` : ''}${f ? ` -> ${f}` : ''}`);
  });
  writeFileSync(join(dir, 'timeline.txt'), line.join('\n') + '\n');
  writeFileSync(join(dir, 'worker.log'), p.workerLog.join('\n') + '\n');
  writeFileSync(join(dir, 'console.log'), [...logs, ...p.consoleLog, ...p.errors].join('\n') + '\n');
}
/** The open file's text set back to `text` (the example button keeps edited files): Escape ends the caret, select all, insert. */
async function restore(p, text) {
  await p.key('Escape');
  await setText(p, text);
}
async function setText(p, text) {
  await p.eval(`document.querySelector('.cm-content').focus()`);
  await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
  await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
  await p.s('Input.insertText', { text });
  await p.until(`satexHealth().text === ${J(text)}`, 5000);
}
function diff(want, got) {
  let i = 0; while (i < want.length && want[i] === got[i]) i++;
  let j = 0; while (j < want.length - i && j < got.length - i && want.at(-1 - j) === got.at(-1 - j)) j++;
  return `at ${i}: want ${J(want.slice(Math.max(0, i - 15), want.length - j + 5))}, got ${J(got.slice(Math.max(0, i - 15), got.length - j + 5))}`;
}
async function shot(p, res, tag) {
  const f = `${res.name}-${tag}.png`;
  // the caret's surroundings if there is one, else the viewport
  const clip = await p.eval(`(() => { const e = document.querySelector('#play-pdf .pdf-caret') ?? document.querySelector('#play-pdf .pdf-page'); if (!e) return null; const r = e.getBoundingClientRect(); return { x: Math.max(0, r.left - 450), y: Math.max(0, r.top - 120), width: 900, height: 260 }; })()`);
  res.shot = await p.shot(f, clip ?? undefined);
}

// tint scenarios: the origin tints mapped to the page words they cover
async function tints(p) {
  return p.eval(`(() => { const ws = satexPdfWords(), dpi = ws.dpi, out = [];
    for (const t of document.querySelectorAll('#play-pdf .pdf-orig')) {
      const pg = t.closest('.pdf-page'), img = pg.querySelector('img'), ir = img.getBoundingClientRect(), k = ir.width / (img.naturalWidth * 72 / dpi), r = t.getBoundingClientRect();
      const l = (r.left - ir.left) / k, rt = (r.right - ir.left) / k, tp = (r.top - ir.top) / k, bt = (r.bottom - ir.top) / k;
      const over = ws.filter((w) => w[0] === +pg.dataset.page && w[1] < rt - 0.2 && w[3] > l + 0.2 && w[2] < bt && w[4] > tp);
      out.push({ kind: t.className.replace(/.*pdf-orig-/, ''), page: +pg.dataset.page, box: [l, tp, rt, bt].map((v) => Math.round(v * 10) / 10), words: over.map((w) => w[5]), cover: over.map((w) => Math.round(Math.min(1, (Math.min(rt, w[3]) - Math.max(l, w[1])) / (w[3] - w[1])) * 100)), title: (t.title || '').slice(0, 60) });
    }
    return out; })()`);
}
/** A page pdf.js cannot draw (its draw rejects, `sc.fail` the pages): the page stays without an image, its overlays
 * (the origin tints) are hidden over it, the pages it can draw show theirs. The frame invariant (no overlay over a page
 * with no drawn image) is checked by the main loop. */
async function bareScenario(p, sc) {
  const res = { name: sc.name, kind: 'bare', fails: [], perf: [], info: {} };
  await p.eval(`(() => { const T = self.satexTex, open = T.open; T.open = async (...a) => { const v = await open(...a); if (!v) return v; const draw = v.draw; v.draw = (n) => ${J(sc.fail)}.includes(n) ? Promise.reject(new Error('e2e: draw fails')) : draw(n); return v; }; })()`);
  await p.eval(`document.querySelector('[data-example=${J(sc.example ?? 'thesis.tex')}]').click()`);
  await p.until(`satexHealth().file === ${J(sc.example ?? 'thesis.tex')} && satexHealth().text?.length > 10`, 20000);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await sleep(200);
  await p.eval(`document.getElementById('pdf-compile').click()`);
  const ok = await p.until(`satexPdfWords().length > 20 && document.querySelector('#play-pdf .pdf-page[data-page="${sc.ok}"] img')?.__build && document.querySelectorAll('#play-pdf .pdf-orig').length`, 60000);
  if (!ok) { res.fails.push('no build with tints shown'); return res; }
  await sleep(1500);
  const st = await p.eval(`(() => { const q = (n) => document.querySelector('#play-pdf .pdf-page[data-page="' + n + '"]'), vis = (pg) => [...pg.querySelectorAll('.pdf-orig')].filter((e) => e.checkVisibility({ visibilityProperty: true })).length;
    return { failed: ${J(sc.fail)}.map((n) => ({ n, drawn: !!q(n).querySelector('img.pdf-drawn'), tints: q(n).querySelectorAll('.pdf-orig').length, shown: vis(q(n)) })), ok: { drawn: !!q(${sc.ok}).querySelector('img.pdf-drawn'), shown: vis(q(${sc.ok})) } }; })()`);
  res.info.state = st;
  const warns = p.consoleLog.filter((l) => /e2e: draw fails/.test(l)).length;
  res.info.retries = warns;
  for (const f of st.failed) {
    if (f.drawn) res.fails.push(`page ${f.n}: marked drawn though its draw failed`);
    if (f.shown) res.fails.push(`page ${f.n}: ${f.shown} tints shown over the page with no image`);
    if (!f.tints) res.info.note = `page ${f.n} has no tints to hide (the check is weaker)`;
  }
  if (!st.ok.drawn || !st.ok.shown) res.fails.push(`page ${sc.ok}: drawn ${st.ok.drawn}, ${st.ok.shown} tints shown (want both)`);
  if (warns > 3 * sc.fail.length) res.fails.push(`a failing page was drawn ${warns}x (want at most 3 tries a page)`);
  res.shot = await p.shot(`${sc.name}.png`);
  // the frame check itself: with the page's rule undone the tints show over the blank page and it must see them
  const seen = await p.eval(`new Promise((ok) => { const E = self.__e2e, n0 = E.bareFrames, st = document.createElement('style'); st.textContent = '.pdf .pdf-page > :not(img) { visibility: visible !important; }'; document.head.append(st);
    setTimeout(() => { const n = E.bareFrames - n0; st.remove(); requestAnimationFrame(() => requestAnimationFrame(() => { E.bareFrames = n0; E.bare.length = 0; ok(n); })); }, 300); })`);
  res.info.checkSees = seen;
  if (!seen) res.fails.push('the frame check missed tints shown over a blank page');
  return res;
}
async function tintScenario(p, sc) {
  const res = { name: sc.name, kind: 'tint', fails: [], perf: [], info: {} };
  await load(p, sc);
  // the tints come from the check's run: wait for it
  await idle(p);
  await sleep(1500);
  const t = await tints(p);
  res.info.tints = t.length;
  if (!t.length) res.fails.push('no origin tints drawn');
  // a tint over no word is misplaced
  for (const x of t) if (!x.words.length) res.fails.push(`tint ${x.kind} over no word at p${x.page} ${J(x.box)} (${x.title})`);
  // one word in pieces: two tints of different kinds, or two tints at all, on one word
  const by = new Map();
  for (const x of t) for (const w of x.words) by.set(`${x.page}|${w}`, [...(by.get(`${x.page}|${w}`) ?? []), x]);
  for (const [w, xs] of by) if (sc.whole?.some((s) => w.endsWith(`|${s}`)) && (new Set(xs.map((x) => x.kind)).size > 1 || xs.some((x) => x.cover[x.words.indexOf(w.split('|')[1])] < 95))) res.fails.push(`${w.split('|')[1]} tinted in pieces: ${J(xs.map((x) => [x.kind, x.box]))}`);
  for (const [word, kind] of sc.want ?? []) {
    const hit = t.filter((x) => x.words.includes(word.replace(/#\d+$/, '')));
    const kinds = [...new Set(hit.map((x) => x.kind))];
    if (kind === null ? kinds.length : kind.startsWith('!') ? kinds.includes(kind.slice(1)) : !kinds.includes(kind)) res.fails.push(`${word}: tinted ${J(kinds)}, want ${J(kind)}`);
  }
  // toggling a layer off and on leaves the tints where they were
  if (sc.toggle) {
    const boxes = (u) => u.map((x) => `${x.kind}@${x.page}:${x.box.join(',')}`).sort().join(' ');
    const before = boxes(t);
    await p.eval(`(() => { document.getElementById('pdf-layers').open = true; })()`);
    const inputs = await p.eval(`[...document.querySelectorAll('#pdf-layer-list input[type=checkbox]')].length`);
    for (const i of [0, 0]) { await p.eval(`document.querySelectorAll('#pdf-layer-list input[type=checkbox]')[${i}].click()`); await sleep(500); }
    const after = boxes(await tints(p));
    res.info.layers = inputs;
    if (before !== after) res.fails.push(`tints moved after a layer toggled off and on: ${before.slice(0, 120)} → ${after.slice(0, 120)}`);
    // the Typeset view's tints after the same toggle
  }
  res.info.sample = t.slice(0, 6).map((x) => `${x.kind}:${x.words.join('_')}`).join(' ');
  if (res.fails.length) await shot(p, res, 'tints');
  return res;
}

// click scenarios: a word clicked at its start, middle and end places the caret at that offset of its source
async function clickScenario(p, sc) {
  const res = { name: sc.name, kind: 'click', fails: [], perf: [], info: {} };
  await load(p, sc);
  await idle(p);
  // `ready`: the analysis of the scenario's text is in (the origins a macro's output is placed by)
  if (sc.ready) await p.until(sc.ready, 120000);
  await sleep(300);
  const got = [];
  for (const pos of ['start', 'mid', 'end']) {
    // apart from the click before (two clicks a few px apart within the double-click time select a word), the
    // word measured after Escape settled the pages: a scroll between measuring and clicking hits another word
    await p.key('Escape');
    await sleep(600);
    const w = await wordAt(p, sc.word, sc.nth ?? 0, sc.page);
    if (!w) { res.fails.push(`setup: no page word for ${sc.word}`); return res; }
    const len = [...w[5]].length;
    if (pos === 'mid' && len < 3) continue;
    await p.key('Escape');
    await p.eval(`document.getElementById('play-toast') && (document.getElementById('play-toast').textContent = '')`);
    const x = pos === 'start' ? w[0] + 0.25 * w[6] : pos === 'end' ? w[2] - 0.25 * w[6] : (w[0] + w[2]) / 2;
    await p.mouse(x, (w[1] + w[3]) / 2);
    const c = await p.until(`satexPdfCaret()`, sc.clickMs ?? 3000);
    const text = await p.eval(`satexHealth().text`), s0 = sc.src(text);
    if (!c) { res.fails.push(`${pos}: no caret (toast: ${await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`)})`); got.push(`${pos} -`); continue; }
    await sleep(150);
    const at = (await p.eval(`satexPdfCaret()`))?.at ?? c.at, n = sc.len ?? len;
    const ok = pos === 'start' ? at === s0 : pos === 'end' ? at === s0 + n : at > s0 && at < s0 + n;
    got.push(`${pos} ${at - s0}/${n}`);
    if (!ok) res.fails.push(`${pos} (click ${Math.round(x)},${Math.round((w[1] + w[3]) / 2)} on ${J(w[5])} over ${await p.eval(`(() => { const e = document.elementFromPoint(${x}, ${(w[1] + w[3]) / 2}); return e ? e.tagName + '.' + [...e.classList].join('.') : '-'; })()`)}${await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`).then((t) => t ? `, toast ${J(t.slice(0, 80))}` : '')}): caret at ${at} ${J(text.slice(at - 15, at))}|, want ${pos === 'mid' ? `inside ${s0}..${s0 + n}` : pos === 'start' ? s0 : s0 + n} ${J(text.slice(s0, s0 + n))}`);
  }
  res.info.offsets = got.join(' · ');
  if (res.fails.length) await shot(p, res, 'click');
  return res;
}

// a drag over words selects them; Backspace/Delete (or a character) takes the selection's source at once
// Tab completion in the source editor while a long run holds the worker (ui-37): the list answers from what the
// last run left (the commands, the labels after `\ref{`), Tab takes its item at once, the keys after it land.
// `keys` typed on a new line before `\end{document}` once the edit `Enter` started a run; `want`: the text then.
async function completeScenario(p, sc) {
  const res = { name: sc.name, kind: 'complete', fails: [], perf: [], info: {} };
  await p.eval(`document.querySelector('[data-example=${J(sc.example ?? 'thesis.tex')}]').click()`);
  await p.until(`satexHealth().example === ${J(sc.example ?? 'thesis.tex')} && satexHealth().text?.length > 10`, 20000);
  if (!await settled(p)) throw new Error('the first run never ended');
  // the lists asked once the run ended
  await sleep(3000);
  const raw = (key, code, mod = 0) => ['rawKeyDown', 'keyUp'].reduce((q, type) => q.then(() => p.s('Input.dispatchKeyEvent', { type, key, code: key, windowsVirtualKeyCode: code, modifiers: mod })), Promise.resolve());
  await p.eval(`document.querySelector('.cm-content').focus()`);
  await raw('End', 35, 2); await raw('ArrowUp', 38); await raw('Home', 36);
  await p.key('Enter');
  await raw('ArrowUp', 38);
  if (!await p.until(`satexHealth().running > 0`, 8000)) { res.fails.push('setup: the edit started no run'); return res; }
  await sleep(sc.waitMs ?? 100);
  const t0 = Date.now(), before = await p.eval(`satexHealth().text`);
  for (const k of keysOf(sc.keys)) { await p.key(k); await sleep(sc.keyMs ?? 15); }
  res.info.typedMs = Date.now() - t0;
  // what the keys made within a moment, the run still going on
  const got = await p.until(`(() => { const t = satexHealth().text; return ${sc.want}.test(t) && t; })()`, 1500);
  res.info.running = await p.eval(`satexHealth().running`);
  res.info.ms = Date.now() - t0;
  const text = got || await p.eval(`satexHealth().text`), at = [...before].findIndex((c, i) => c !== text[i]);
  res.info.made = J(text.slice(Math.max(0, at - 10), at + 40));
  if (!got) res.fails.push(`text ${res.info.made}, want ${sc.want}`);
  // `idle`: what is checked holds with or without a run (the boxes a completion leaves)
  if (!res.info.running && !sc.idle) res.fails.push(`the run ended before the check (${res.info.ms} ms): no run was going on`);
  // `clean`: once the caret has left what the completion wrote, no snippet field or linked \begin/\end box stays drawn
  if (sc.clean) {
    await sleep(600);
    const left = await p.eval(`[...document.querySelectorAll('.cm-content .cm-linked, .cm-content .cm-snippetField')].map((e) => e.className + ' ' + JSON.stringify(e.textContent.slice(0, 30)))`);
    if (left.length) res.fails.push(`boxes left after the caret moved on: ${J(left)}`);
  }
  if (res.fails.length) res.shot = await p.shot(`${sc.name}.png`);
  return res;
}

async function selectScenario(p, sc) {
  const res = { name: sc.name, kind: 'select', fails: [], perf: [], info: {} };
  await load(p, sc);
  await idle(p);
  await sleep(300);
  const a = await wordAt(p, sc.from), b = await wordAt(p, sc.to);
  if (!a || !b) { res.fails.push('setup: no page words'); return res; }
  const text0 = await p.eval(`satexHealth().text`);
  await p.eval(`(self.__e2eB = new Set([...document.querySelectorAll('#play-pdf .pdf-page img')].map((i) => i.__build))).size`);
  const y = (w) => (w[1] + w[3]) / 2;
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: a[0] + 1, y: y(a) });
  await p.s('Input.dispatchMouseEvent', { type: 'mousePressed', x: a[0] + 1, y: y(a), button: 'left', clickCount: 1 });
  for (let i = 1; i <= 6; i++) await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: a[0] + 1 + (b[2] - 1 - a[0] - 1) * i / 6, y: y(a) + (y(b) - y(a)) * i / 6, button: 'left', buttons: 1 });
  await p.s('Input.dispatchMouseEvent', { type: 'mouseReleased', x: b[2] - 1, y: y(b), button: 'left', clickCount: 1 });
  await sleep(150);
  const bands = await p.eval(`document.querySelectorAll('#play-pdf .pdf-sel').length`);
  if (!bands) { res.fails.push('drag: no selection drawn'); await shot(p, res, 'nosel'); return res; }
  // the text the selection should take, as the source spells it
  const [s0, s1] = sc.span(text0), want = sc.refused ? text0 : text0.slice(0, s0) + (sc.keys ?? []).filter((k) => k.length === 1).join('') + text0.slice(s1);
  for (const k of sc.keys ?? ['Backspace']) await p.key(k);
  // painted over at once: the selected words' middles show the patch (white) before any build
  await sleep(120);
  const painted = await p.eval(`document.querySelectorAll('#play-pdf .pdf-erased').length`);
  res.info.painted = painted;
  if (!sc.refused && !painted) res.fails.push('the taken words were not painted over at once');
  const text1 = await p.eval(`satexHealth().text`);
  if (text1 !== want) res.fails.push(`source: want ${J(want.slice(s0 - 20, s0 + 20))}, got ${J(text1.slice(s0 - 20, s0 + 20))} (toast: ${await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`)})`);
  if (sc.refused) { const t = await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`); if (!/Not typed here/.test(t)) res.fails.push(`no refusal note (toast: ${t})`); return res; }
  const ok = await settle(p);
  if (!ok) res.fails.push('settle: not settled');
  const words = await p.eval(`satexPdfWords().map((w) => w[5]).join(' ')`);
  for (const g of sc.gone ?? []) if (words.includes(g)) res.fails.push(`"${g}" still set after the rebuild`);
  if (res.fails.length) await shot(p, res, 'final');
  return res;
}

// a column's letter changed from its chip menu: applied like typing, the old pages shown until the new build swaps once
async function colScenario(p, sc) {
  const res = { name: sc.name, kind: 'col', fails: [], perf: [], info: {} };
  await load(p, sc);
  await idle(p);
  // the chips belong to the open file: the table's file opened from the tree, the PDF tab kept
  if (sc.file) {
    await p.eval(`document.querySelector('#play-files-tree .file[data-name=${J(sc.file)}]')?.click()`);
    if (!await p.until(`satexHealth().file === ${J(sc.file)}`, 20000)) { res.fails.push(`setup: ${sc.file} not opened`); return res; }
    await idle(p);
    await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
    await sleep(800);
  }
  await p.eval(`(() => { const i = document.querySelector('#pdf-layer-list input[data-layer=widths]'); if (i && !i.checked) i.click(); })()`);
  await sleep(500);
  const w = await wordAt(p, sc.word);
  if (!w) { res.fails.push('setup: no table word'); return res; }
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: w[0] + 2, y: (w[1] + w[3]) / 2 });
  const more = `#play-pdf .pdf-col-more[data-menu$=":${sc.col ?? 0}"]`;
  const chip = await p.until(`document.querySelector(${J(more)})`, 8000);
  if (!chip) { res.fails.push('no column chip'); await shot(p, res, 'nochip'); return res; }
  await p.click(more);
  if (!await p.until(`[...document.querySelectorAll('.pa-root > button')].find((b) => b.textContent.startsWith(${J(sc.to + ':')}))`, 3000)) { res.fails.push('no column menu'); return res; }
  const text0 = await p.eval(`satexHealth().text`);
  // frames from the click on: chips shown, the pages' opacity, image swaps, overlay elements removed and added (origin
  // tints not: a new build moves them by right)
  await p.eval(`(() => { self.__colF = []; self.__colMut = 0; self.__colB = new Set([...document.querySelectorAll('#play-pdf .pdf-page img')].map((i) => i.__build));
    const mo = new MutationObserver((ms) => { for (const m of ms) for (const n of [...m.addedNodes, ...m.removedNodes]) if (n.classList?.contains('pdf-ov') && !n.classList.contains('pdf-orig')) { self.__colMut++; const c = [...n.classList].filter((x) => x !== 'pdf-ov').join('.'); (self.__colKinds ??= {})[c] = (self.__colKinds[c] ?? 0) + 1; } });
    mo.observe(document.getElementById('play-pdf'), { childList: true, subtree: true });
    // the pages in view: one scrolled away may wait for its image (lazy) or show the last one dimmed
    const t0 = performance.now(), view = document.querySelector('#play-pdf .pdf-pages').getBoundingClientRect(), tick = () => { const imgs = [...document.querySelectorAll('#play-pdf .pdf-page img')].filter((i) => { const r = i.closest('.pdf-page').getBoundingClientRect(); return r.bottom > view.top && r.top < view.bottom; });
      self.__colF.push({ t: performance.now() - t0, chips: document.querySelectorAll('#play-pdf .pdf-col').length, op: Math.min(...imgs.map((i) => +getComputedStyle(i).opacity)), ovOp: Math.min(1, ...[...document.querySelectorAll('#play-pdf .pdf-col')].map((e) => +getComputedStyle(e).opacity)), blank: imgs.filter((i) => !i.__build).length, stray: document.querySelectorAll('#play-pdf .pdf-col-drag, #play-pdf .pdf-echo').length, b: imgs.map((i) => i.__build && (self.__colB.has(i.__build) ? 0 : 1)).join('') });
      if (!self.__colStop && performance.now() - t0 < 40000) requestAnimationFrame(tick); else mo.disconnect(); };
    requestAnimationFrame(tick); })()`);
  { const c = await p.eval(`(() => { const r = [...document.querySelectorAll('.pa-root > button')].find((b) => b.textContent.startsWith(${J(sc.to + ':')})).getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`); await p.mouse(c[0], c[1]); }
  // the new build in, then a moment of frames after it (its swap, the chips again)
  res.info.landWait = await p.until(`[...document.querySelectorAll('#play-pdf .pdf-page img')].some((i) => i.__build && !self.__colB.has(i.__build))`, 35000) ? 'landed' : 'timeout';
  await sleep(1500);
  await p.eval(`self.__colStop = true`);
  const f = await p.eval(`self.__colF`), mut = await p.eval(`self.__colMut`);
  res.info.mutKinds = await p.eval(`self.__colKinds ?? null`);
  const text1 = await p.eval(`satexHealth().text`);
  if (!sc.spec.test(text1)) res.fails.push(`source: ${J(text1.match(/\\begin\{tabular\}\{[^}]*\}/)?.[0])}, want ${sc.spec}`);
  const minOp = Math.min(...f.map((x) => x.op)), chips0 = f.filter((x) => x.chips === 0 && x.t > 50).length, swaps = f.filter((x, i) => i && x.b !== f[i - 1].b).length, blank = f.filter((x) => x.blank).length;
  const minOv = Math.min(...f.filter((x) => x.t > 50).map((x) => x.ovOp)), stray = Math.max(...f.map((x) => x.stray)), landed = f.find((x) => /1/.test(x.b));
  const dim = f.filter((x) => x.t > 50 && x.ovOp < 0.95); res.info = { ...res.info, dimMs: dim.length ? [Math.round(dim[0].t), Math.round(dim.at(-1).t), dim.length] : null, frames: f.length, minOpacity: minOp, minChipOpacity: minOv, framesNoChips: chips0, swaps, blank, ovMutations: mut, landedMs: landed && Math.round(landed.t) };
  if (!landed) res.fails.push('no new build shown within 35 s');
  if (minOp < 0.95) res.fails.push(`pages dimmed to ${minOp} while the build ran`);
  if (minOv < 0.95) res.fails.push(`column chips dimmed to ${minOv.toFixed(2)}`);
  if (chips0) res.fails.push(`chips gone in ${chips0} frames`);
  if (stray) res.fails.push(`${stray} stray drawings (a drag label, an echo) over the page`);
  if (blank) res.fails.push(`blank page in ${blank} frames`);
  if (swaps > sc.pages) res.fails.push(`pages swapped ${swaps}x, want ${sc.pages}`);
  if (mut > sc.maxMut) res.fails.push(`overlay redrawn: ${mut} elements added/removed, want <= ${sc.maxMut}`);
  if (res.fails.length) await shot(p, res, 'final');
  return res;
}

// forest nodes: a right click on a node's word opens its menu; a choice rewrites the tree through the snapshot printer
async function forestScenario(p, sc) {
  const res = { name: sc.name, kind: 'forest', fails: [], perf: [], info: {} };
  await load(p, sc);
  await idle(p);
  // `ts`: the node's text in the Typeset view's sketch instead of the page
  if (sc.ts) await p.eval(`(self.__errs = [], addEventListener('error', (e) => self.__errs.push(String(e.error?.stack ?? e.message).slice(0, 600))), document.querySelector('.side-tabs [data-panel=play-typeset]').click())`);
  const w = sc.ts
    ? await p.until(`(() => { const t = [...document.querySelectorAll('#typeset-body .ts-sketch text')].find((t) => t.textContent.trim() === ${J(sc.ts)}); if (!t) return null; t.scrollIntoView({ block: 'center' }); const r = t.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom]; })()`, 150000)
    : await wordAt(p, sc.word);
  if (!w) { res.fails.push('setup: no node word'); return res; }
  const x = (w[0] + w[2]) / 2, y = (w[1] + w[3]) / 2;
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  // `plus`: the hover's + button of that kind (child, sibling) instead of the menu
  if (sc.plus) {
    const b = await p.until(`(() => { const b = document.querySelector('#play-pdf .pdf-forest-add [data-add=${sc.plus}], .ts-forest-add [data-add=${sc.plus}]'); if (!b) return null; const r = b.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`, 4000);
    if (!b) { res.fails.push(`no + ${sc.plus} button on hover${sc.ts ? ` (sketch texts ${await p.eval(`JSON.stringify([...document.querySelectorAll('#typeset-body .ts-sketch text')].map((t) => t.textContent))`)}, under the pointer ${await p.eval(`(() => { const e = document.elementFromPoint(${x}, ${y}); return e ? e.tagName + '.' + e.getAttribute('class') + ' in ' + e.parentElement?.tagName + '.' + e.parentElement?.getAttribute('class') : null; })()`)}, errors ${await p.eval('JSON.stringify(self.__errs ?? [])')})` : ''}`); await shot(p, res, 'noplus'); return res; }
    const text0 = await p.eval(`satexHealth().text`);
    await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: b[0], y: b[1] });
    await sleep(80);
    if (sc.plus === 'drag') {
      // pressed on the handle, released on the right quarter of `sc.to`: after that node
      const t = await wordAt(p, sc.to);
      if (!t) { res.fails.push('setup: no drop word'); return res; }
      const tx = t[0] + (t[2] - t[0]) * 0.9, ty = (t[1] + t[3]) / 2;
      await p.s('Input.dispatchMouseEvent', { type: 'mousePressed', x: b[0], y: b[1], button: 'left', clickCount: 1 });
      for (let k = 1; k <= 8; k++) { await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: b[0] + (tx - b[0]) * k / 8, y: b[1] + (ty - b[1]) * k / 8, button: 'left', buttons: 1 }); await sleep(30); }
      if (!await p.until(`document.querySelector('#play-pdf .pdf-forest-tag')?.textContent`, 2000)) res.fails.push('no drop tag while dragging');
      await shot(p, res, 'drag');
      await p.s('Input.dispatchMouseEvent', { type: 'mouseReleased', x: tx, y: ty, button: 'left', clickCount: 1 });
    } else await p.mouse(b[0], b[1]);
    const text1 = await p.until(`satexHealth().text !== ${J(text0)} && satexHealth().text`, 8000);
    if (!text1) { res.fails.push('source unchanged'); return res; }
    if (!sc.want.test(text1)) res.fails.push(`source: ${J(text1.match(/\\begin\{forest\}[^]*?\\end\{forest\}/)?.[0]?.slice(0, 300))}, want ${sc.want}`);
    return res;
  }
  for (const type of ['mousePressed', 'mouseReleased']) await p.s('Input.dispatchMouseEvent', { type, x, y, button: 'right', clickCount: 1 });
  // the page actions' menu (actions.ts): a forest node's groups are inline, their rows at the menu's root
  const label = `(b) => b.querySelector('span')?.textContent ?? b.textContent`;
  const items = await p.until(`(() => { const m = document.querySelector('.pa-root'); return m && [...m.querySelectorAll(':scope > button')].map(${label}); })()`, 4000);
  if (!items) { res.fails.push('no node menu'); await shot(p, res, 'nomenu'); return res; }
  res.info.items = items.join(' | ');
  // `choose`: a row, or the rows down submenus to one
  const path = [sc.choose].flat();
  if (!items.includes(path[0])) { res.fails.push(`no ${J(path[0])} in ${J(items)}`); return res; }
  const text0 = await p.eval(`satexHealth().text`);
  for (const [i, l] of path.entries()) {
    const row = await p.eval(`(() => { const b = [...[...document.querySelectorAll('.pa-menu')].at(-1).querySelectorAll(':scope > button')].find((b) => (${label})(b) === ${J(l)}); if (!b) return null; const r = b.getBoundingClientRect(); return [r.x + Math.min(40, r.width / 2), r.y + r.height / 2]; })()`);
    if (!row) { res.fails.push(`no ${J(l)} in the submenu`); return res; }
    await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: row[0], y: row[1] });
    await sleep(150);
    if (i === path.length - 1) await p.mouse(row[0], row[1]);
  }
  if (sc.input != null) {
    if (!await p.until(`document.querySelector('.tikz-input input')`, 2000)) { res.fails.push('no text input'); return res; }
    await p.eval(`(() => { const i = document.querySelector('.tikz-input input'); i.value = ${J(sc.input)}; })()`);
    await p.eval(`document.querySelector('.tikz-input input').focus()`);
    await p.key('Enter');
  }
  const text1 = await p.until(`satexHealth().text !== ${J(text0)} && satexHealth().text`, 8000);
  if (!text1) { res.fails.push(`source unchanged (toast: ${await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`)})`); return res; }
  if (!sc.want.test(text1)) res.fails.push(`source: ${J(text1.match(/\\begin\{forest\}[^]*?\\end\{forest\}/)?.[0]?.slice(0, 300))}, want ${sc.want}`);
  // the rest of the file as it was
  const strip = (t) => t.replace(/\\begin\{forest\}[^]*?\\end\{forest\}/, '');
  if (!sc.outside && strip(text1) !== strip(text0)) res.fails.push('text outside the tree changed');
  return res;
}

// layout shift: the pages stay put while a build runs (its timer and status grow and shrink)
async function shiftScenario(p, sc) {
  const res = { name: sc.name, kind: 'shift', fails: [], perf: [], info: {} };
  await load(p, sc);
  // a new text: the build runs, none comes from the cache
  await setText(p, (await p.eval(`satexHealth().text`)) + `% ${Date.now()}\n`);
  await idle(p);
  const r = await p.eval(`new Promise((ok) => {
    const rect = () => { const e = document.querySelector('#play-pdf .pdf-pages'); if (!e) return null; const r = e.getBoundingClientRect(); return [r.left, r.top, r.width]; };
    const r0 = rect(); let max = 0, frames = 0, saw = false, worst = null; const t0 = performance.now();
    document.getElementById('pdf-compile').click();
    const tick = () => {
      const run = !!document.querySelector('#play-pdf.pdf-running'), r = rect();
      saw ||= run; frames++;
      if (r && r0) { const d = Math.max(...r.map((v, i) => Math.abs(v - r0[i]))); if (d > max) { max = d; worst = { r0, r, status: document.getElementById('pdf-status').textContent.slice(0, 60) }; } }
      if ((saw && !run) || performance.now() - t0 > 60000) ok({ max, frames, saw, worst }); else requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  })`);
  res.info = { ...res.info, maxShift: Math.round(r.max * 10) / 10, frames: r.frames };
  if (!r.saw) res.fails.push('no build ran');
  if (r.max > 0.5) res.fails.push(`the pages moved ${r.max.toFixed(1)}px during a build: ${J(r.worst)}`);
  return res;
}

// Typeset-view scenarios: the open file's paragraphs as the check sets them
async function loadTs(p, sc) {
  await p.eval(`document.querySelector('[data-example=${J(sc.example ?? 'thesis.tex')}]').click()`);
  await p.until(`satexHealth().example === ${J(sc.example ?? 'thesis.tex')} && satexHealth().text?.length > 10`, 20000);
  await sleep(400);
  if (sc.source) await setText(p, sc.source);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-typeset]').click()`);
  await sleep(300);
  await p.until(`!satexHealth().busy && !satexHealth().due`, 60000);
  if (!await p.until(`document.querySelector('#typeset-body p.ts .ts-text')`, 20000)) throw new Error('the Typeset view set no paragraph');
  await sleep(800);
}
async function tsScenario(p, sc) {
  const res = { name: sc.name, kind: 'ts', fails: [], perf: [], info: {} };
  await loadTs(p, sc);
  if (sc.check === 'maybe') {
    await p.eval(`(() => { const b = document.querySelector('#typeset-layers [data-layer=maybe]'); if (b && !b.checked) b.click(); })()`);
    await sleep(500);
    // an undecided conditional: both cases shown, each switch shows one, `both` both again
    const arms = () => p.eval(`[...document.querySelectorAll('#typeset-body .ts-choice')].map((c) => [...c.querySelectorAll(':scope > .ts-arm')].map((a) => [a.className.includes('true') ? 'true' : 'false', a.textContent.trim(), getComputedStyle(a).display !== 'none']))`);
    const all = await arms();
    res.info.choices = all.length;
    if (!all.length) { res.fails.push(`no maybe choice drawn (the row: ${J((await p.eval(`document.querySelector('#typeset-body p.ts')?.innerHTML ?? ''`)).slice(0, 300))})`); return res; }
    const text = (pick) => all.flat().filter(([a]) => pick == null || a === pick).map(([, t]) => t).join(' ');
    for (const [arm, want] of sc.want) if (!text(arm).includes(want)) res.fails.push(`arm ${arm} lacks ${J(want)}: ${J(text(arm))}`);
    res.info.switches = await p.eval(`[...document.querySelectorAll('#typeset-body .ts-switch')].map((s) => s.className + ':' + [...s.querySelectorAll('button')].map((b) => b.dataset.arm).join('/')).join(' ')`);
    // a switch only for a reference or citation the next run decides (site.ts `switches`): any other choice reads
    // inline, both cases shown
    if (!res.info.switches) { if (all.flat().some(([, , on]) => !on)) res.fails.push(`a case hidden: ${J(all)}`); }
    else for (const arm of ['true', 'false', 'both']) {
      const b = await p.eval(`(() => { const b = document.querySelector('#typeset-body .ts-switch button[data-arm=${J(arm)}]'); if (!b) return false; b.click(); return true; })()`);
      if (!b) { res.fails.push(`no ${arm} switch`); continue; }
      await sleep(200);
      const shown = (await arms()).flat().filter(([, , on]) => on).map(([a]) => a);
      const want = arm === 'both' ? ['true', 'false'] : [arm];
      if ([...new Set(shown)].sort().join() !== want.sort().join()) res.fails.push(`switch ${arm}: arms shown ${J([...new Set(shown)])}`);
    }
  } else if (sc.check === 'tints') {
    // tints at their words, kept in place by a layer switched off and on
    const boxes = () => p.eval(`(() => { const b = document.getElementById('typeset-body').getBoundingClientRect(); return [...document.querySelectorAll('#typeset-body .ts-orig')].map((t) => { const r = t.getBoundingClientRect(); return t.className.replace(/.*ts-orig-/, '') + '@' + [r.left - b.left, r.top - b.top, r.width, r.height].map((v) => Math.round(v)).join(','); }).sort().join(' '); })()`);
    const before = await boxes();
    res.info.tints = before ? before.split(' ').length : 0;
    if (!before) res.fails.push('no origin tints in the Typeset view');
    // tints over no text: each tint's middle point lies on a text node's line box
    // (in view only: elementFromPoint sees nothing outside it; a tint of no area draws nothing)
    res.info.flat = await p.eval(`[...document.querySelectorAll('#typeset-body .ts-orig')].filter((t) => { const r = t.getBoundingClientRect(); return !r.width || !r.height; }).length`);
    const off = await p.eval(`[...document.querySelectorAll('#typeset-body .ts-orig')].filter((t) => { const r = t.getBoundingClientRect(); if (!r.width || !r.height || r.bottom < 0 || r.top > innerHeight || r.right < 0 || r.left > innerWidth) return false; t.style.pointerEvents = 'none'; const e = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); t.style.pointerEvents = ''; return !e?.closest('.ts-text'); }).map((t) => { const r = t.getBoundingClientRect(); t.style.pointerEvents = 'none'; const e = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); t.style.pointerEvents = ''; return t.className.replace(/.*ts-orig-/, '') + ':' + JSON.stringify(t.textContent.slice(0, 20)) + '/' + Math.round(r.width) + 'x' + Math.round(r.height) + ' over ' + (e ? e.tagName + '.' + [...e.classList].join('.') : '-') + ' in ' + (t.parentElement?.className ?? ''); })`);
    if (off.length) res.fails.push(`${off.length} tints over no text: ${off.slice(0, 4).join(' | ')}`);
    const n = await p.eval(`document.querySelectorAll('#typeset-layers input[type=checkbox]').length`);
    res.info.layers = n;
    for (let i = 0; i < n; i++) {
      for (const _ of [0, 1]) { await p.eval(`document.querySelectorAll('#typeset-layers input[type=checkbox]')[${i}].click()`); await sleep(400); }
      const after = await boxes();
      if (after !== before) { const id = await p.eval(`document.querySelectorAll('#typeset-layers input[type=checkbox]')[${i}].dataset.layer`); res.fails.push(`tints moved after layer ${id} toggled off and on: ${before.slice(0, 100)} → ${after.slice(0, 100)}`); break; }
    }
  }
  if (res.fails.length) res.shot = await p.shot(`${sc.name}-ts.png`);
  return res;
}

// a double click on an image, a table or a drawing (PDF or Typeset view) opens its widget, as the editor's lens does;
// on a text word it opens none. `at`: the site's last line (a regex on `file`'s text), `want`: what the widget reads
async function dblScenario(p, sc) {
  const res = { name: sc.name, kind: 'dbl', fails: [], perf: [], info: {} };
  if (sc.view === 'ts') {
    // as loadTs, waiting out a fresh profile's package downloads
    await p.eval(`document.querySelector('[data-example=${J(sc.example ?? 'thesis.tex')}]').click()`);
    await p.until(`satexHealth().example === ${J(sc.example ?? 'thesis.tex')} && satexHealth().text?.length > 10`, 20000);
    await sleep(400);
    if (sc.source) await setText(p, sc.source);
    await p.eval(`document.querySelector('.side-tabs [data-panel=play-typeset]').click()`);
  } else await load(p, sc);
  if (!await p.until(`!satexHealth().busy && !satexHealth().due && satexOccurrences().length${sc.view === 'ts' ? ` && document.querySelector('#typeset-body p.ts .ts-text')` : ''}`, 150000)) throw new Error('the run never settled');
  await sleep(500);
  const file = sc.file ?? sc.example ?? 'thesis.tex';
  const text = file === (sc.example ?? 'thesis.tex') && sc.source ? sc.source : readFileSync(new URL(file, EXAMPLES), 'utf8');
  const line = sc.at ? text.slice(0, sc.at.exec(text)?.index ?? -1).split('\n').length : null;
  if (sc.at && !sc.at.test(text)) { res.fails.push(`setup: no ${sc.at} in ${file}`); return res; }
  let at;
  if (sc.view === 'ts') {
    // the Typeset row of the site's lines (an image: its note), else the row of the word
    at = await p.eval(`(() => {
      const rows = [...document.querySelectorAll('#typeset-body p.ts[data-line]')].filter((q) => (q.dataset.path ?? '').endsWith('/' + ${J(file)}));
      const q = ${sc.text ? `[...document.querySelectorAll('#typeset-body p.ts .ts-text')].find((t) => t.textContent.includes(${J(sc.text)}))` : `(${sc.note ? `rows.map((q) => q.querySelector('.ts-note')).find(Boolean)` : `rows.filter((q) => +q.dataset.line <= ${line} && +q.dataset.line >= ${line} - ${sc.span ?? 6}).sort((a, b) => b.dataset.line - a.dataset.line).map((q) => q.querySelector('.ts-text')).find(Boolean)`})`};
      if (!q) return null;
      q.scrollIntoView({ block: 'center' });
      const r = q.getBoundingClientRect(), t = document.createRange();
      // a word's own box: the first text node's first characters
      const n = [...q.childNodes].find((c) => c.nodeType === 3 && c.textContent.trim()) ?? q.firstChild;
      if (n?.nodeType === 3) { const i = n.textContent.search(/\\S/); t.setStart(n, i); t.setEnd(n, i + 1); const b = t.getBoundingClientRect(); return [b.left + b.width / 2, b.top + b.height / 2]; }
      return [r.left + Math.min(20, r.width / 2), r.top + r.height / 2];
    })()`);
  } else if (sc.page) {
    // a place of page 1 by its share of the page (`page`: [x, y]): a margin, the header band
    at = await p.eval(`(() => { const i = document.querySelector('#play-pdf .pdf-page[data-page="1"] img'); if (!i) return null; i.scrollIntoView({ block: 'start' }); const r = i.getBoundingClientRect(); return [r.left + r.width * ${sc.page[0]}, r.top + r.height * ${sc.page[1]}]; })()`);
  } else if (sc.text) {
    const w = await wordAt(p, `(t) => t === ${J(sc.text)}`);
    at = w && [(w[0] + w[2]) / 2, (w[1] + w[3]) / 2];
  } else {
    // the largest box SyncTeX records on the site's last line, at its middle
    at = await p.eval(`(async () => {
      const b = satexPdfBoxes().filter((b) => String(b.file).endsWith(${J(file)}) && b.line === ${line} && '[('.includes(b.kind) && b.w > 0 && b.h + b.d > 0).sort((a, c) => c.w * (c.h + c.d) - a.w * (a.h + a.d))[0];
      if (!b) return null;
      const img = () => document.querySelector('#play-pdf .pdf-page[data-page="' + b.page + '"] img'), sc = document.querySelector('#play-pdf .pdf-pages'), dpi = satexPdfWords().dpi;
      let r = img().getBoundingClientRect(), k = r.width / (img().naturalWidth * 72 / dpi);
      const s = sc.getBoundingClientRect(), y = r.top + (b.y - b.h / 2) * k;
      if (y < s.top + 80 || y > s.bottom - 120) { sc.scrollTop += y - (s.top + s.height * 0.4); await new Promise((ok) => setTimeout(ok, 400)); }
      r = img().getBoundingClientRect(); k = r.width / (img().naturalWidth * 72 / dpi);
      return [r.left + (b.x + b.w * ${sc.fx ?? 0.5}) * k, r.top + (b.y - b.h + (b.h + b.d) * ${sc.fy ?? 0.5}) * k];
    })()`);
  }
  if (!at) { res.fails.push('setup: no place to double-click'); await shot(p, res, 'noplace'); return res; }
  res.info.at = at.map(Math.round).join(',');
  const shown = `!!document.querySelector('dialog.sd-dialog[open], #play-view[open]')`;
  // `single`: a click alone opens nothing there (the Page layout widget opens on a double click only)
  if (sc.single) { await p.mouse(at[0], at[1], 1); await sleep(1500); if (await p.eval(shown)) { res.fails.push('a single click opened a widget'); await shot(p, res, 'single'); return res; } await sleep(600); }
  await p.mouse(at[0], at[1], 1);
  await sleep(60);
  await p.mouse(at[0], at[1], 2);
  const seen = `(() => { const d = document.querySelector('dialog.sd-dialog[open]'), v = document.getElementById('play-view'); return d ? 'dialog ' + (d.dataset.pop ?? '') + ' ' + d.textContent.slice(0, 120) : v?.open ? 'view ' + document.getElementById('view-title').textContent : ''; })()`;
  // a widget of a file just shown opens once its run is in
  const got = await p.until(seen, sc.want ? 60000 : 2500) ?? '';
  res.info.got = got.slice(0, 80);
  res.info.toast = await p.eval(`document.getElementById('play-toast')?.textContent?.slice(0, 120) ?? ''`);
  res.info.file = await p.eval(`satexHealth().file`);
  if (sc.want && !sc.want.test(got)) res.fails.push(`widget: ${J(got.slice(0, 100))}, want ${sc.want}`);
  if (!sc.want && got) res.fails.push(`a text word opened ${J(got.slice(0, 80))}`);
  if (sc.want && res.info.file !== file) res.fails.push(`the editor shows ${res.info.file}, want ${file}`);
  res.shot = await p.shot(`${sc.name}.png`);
  return res;
}

export const KINDS = { type: typeScenario, tint: tintScenario, bare: bareScenario, ts: tsScenario, click: clickScenario, shift: shiftScenario,
  select: selectScenario, complete: completeScenario, col: colScenario, forest: forestScenario, dbl: dblScenario };
export { load, settled, settle, setText, wordAt, shot, compileState, keysOf };
