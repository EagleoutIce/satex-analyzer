// Typing paragraphs and around a macro's output in the PDF and the Typeset view (ported from the ui-48 probe): every
// echo draw is typed text (never `document` of \end{document}), one font and vertical scale a row, no word drawn in
// a page face lacking a glyph (lib.mjs MIXED), the caret where the preview put it when the build lands, each
// Typeset row once (no duplicates), arrows never change what is shown, keys after a macro's output after its call.
// Scenario field: `case`, a key of CASES. A case may open more pages (each a fresh context, closed by the case).
import { sleep, page } from '../lib.mjs';
import { ctx, setText } from '../engine.mjs';

const J = JSON.stringify;
const HEAD = '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\n\\greet{world}\n\nWhat do you thing\n\nabout\n';
const T0 = HEAD + '\\end{document}\n';
let fails = [], log = () => {}, first = null;
/** The runner's page for the case's first `open`, a fresh one for each later one. */
const fresh = async () => {
  if (first) { const p = first; first = null; return p; }
  const p = await page(ctx.cdp, ctx.url, ctx.out);
  if (!await p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000)) throw new Error('the playground did not start');
  return p;
};

async function open(text, panel) {
  const p = await fresh();
  // lib.mjs MIXED: echo draws in a page face lacking a glyph are marked
  await p.eval('self.__e2eMixed = true');
  await p.eval(`document.querySelector('[data-example="hello.tex"]').click()`);
  await p.until(`satexHealth().file === 'hello.tex' && satexHealth().text?.length > 10`, 20000);
  await sleep(400);
  await setText(p, text);
  await p.eval(`document.querySelector('.side-tabs [data-panel=${panel}]').click()`);
  return p;
}
const compiled = (p, word = 'about') => p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return satexPdfWords().some((w) => w[5] === ${J(word)}) && i?.naturalWidth > 100 && i.__build && !document.querySelector('#play-pdf.pdf-running'); })()`, 60000);
/** A page word's client box: [x0, y0, x1, y1]. */
const wordBox = (p, word, nth = 0) => p.eval(`(() => { const ws = satexPdfWords(), w = ws.filter((x) => x[5] === ${J(word)})[${nth}]; if (!w) return null; const img = document.querySelector('#play-pdf .pdf-page[data-page="' + w[0] + '"] img'), r = img.getBoundingClientRect(), k = r.width / (img.naturalWidth * 72 / ws.dpi); return [r.left + w[1] * k, r.top + w[2] * k, r.left + w[3] * k, r.top + w[4] * k]; })()`);
/** The echo canvases' drawn texts: each fillText with its font and the context's scale. */
// `mixed`: a draw whose font is a page's embedded face (pdf.js `g_…`) lacking one of its letters: the browser takes
// those from the next family, another font in one word
const echoes = (p) => p.eval(`[...document.querySelectorAll('#play-pdf .pdf-echo:not(.pdf-push)')].flatMap((e) => (e.__texts ?? []).map((q) => ({ t: q.t, font: q.font, sy: q.sy, y: Math.round(q.y), mixed: (e.__mixed ?? []).includes(q.t) })))`);
async function key(p, k, ms = 60) { await p.key(k); await sleep(ms); }
async function type(p, s, ms = 60) { for (const ch of s) await key(p, ch === '\n' ? 'Enter' : ch, ms); }
/** The checks on the echo's draws: every drawn text a part of what was typed, one font per word. */
function checkEcho(name, ts, typed) {
  const words = new Set(typed.split(/\s+/).filter(Boolean));
  const allowed = (t) => !t.trim() || [...words].some((w) => w.includes(t.trim())) ;
  const bad = ts.filter((q) => !allowed(q.t));
  if (bad.length) fails.push(`${name}: echo text not typed: ${J(bad.map((q) => q.t))}`);
  // the draws per row (y), in order: a run of letters with no space between draws is one word; its draws' fonts one
  const byRow = Map.groupBy(ts, (q) => q.y), mixed = [];
  for (const [, qs] of byRow) {
    // one size a row: the fallback's x-height the face's, its px may differ; the vertical scale one
    const sy = new Set(qs.filter((q) => /\p{L}/u.test(q.t)).map((q) => q.sy));
    if (sy.size > 1) mixed.push([qs.map((q) => q.t).join('·'), [...sy]]);
  }
  const lack = ts.filter((q) => q.mixed).map((q) => q.t);
  if (lack.length) fails.push(`${name}: words drawn in a page face lacking a glyph (two fonts in a word): ${J(lack)}`);
  if (mixed.length) fails.push(`${name}: fonts mixed in a row: ${J(mixed)}`);
  return { draws: ts.length, text: ts.map((q) => q.t).join('·') };
}

async function newPara(name, ms) {
  const p = await open(T0, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p)) { fails.push(name + ': no build'); return p.close(); }
  await sleep(800);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  const w = await wordBox(p, 'about');
  await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  // the pages lag: the build of the first keys must not land meanwhile (its keys fast, the run ~250 ms)
  await key(p, 'Enter', ms);
  // every key's echo checked: a build landing between keys takes another branch
  const ts = [];
  for (const ch of 'such a thing this is ') { await key(p, ch, ms); ts.push(...await echoes(p)); }
  log(name + ' echo', J(checkEcho(name, ts, 'such a thing this is')));
  log('fonts', J([...new Set(ts.map((q) => q.font))]));
  { const a = await wordBox(p, 'about'); await p.shot(name + '-echo.png', { x: a[0] - 10, y: a[1] - 50, width: 260, height: 90 }, 4); }
  const pre = await p.eval(`(() => { const c = document.querySelector('#play-pdf .pdf-caret'); const r = c?.getBoundingClientRect(); return r && [r.left, r.top, r.height]; })()`);
  // the landed build: the caret where the preview put it, the row's height alike
  await p.until(`satexPdfWords().some((w) => w[5] === 'is') && !document.querySelector('#play-pdf .pdf-echo') && !document.querySelector('#play-pdf.pdf-running')`, 20000);
  await sleep(800);
  const post = await p.eval(`(() => { const c = document.querySelector('#play-pdf .pdf-caret'); const r = c?.getBoundingClientRect(); return r && [r.left, r.top, r.height]; })()`);
  log('caret preview', J(pre), 'landed', J(post));
  if (pre && post && (Math.abs(pre[0] - post[0]) > 3 || Math.abs(pre[1] - post[1]) > 3 || Math.abs(pre[2] - post[2]) > 2)) fails.push(`${name}: caret jumps on landing ${J(pre)} -> ${J(post)}`);
  { const a = await wordBox(p, 'about'); await p.shot(name + '-landed.png', { x: a[0] - 10, y: a[1] - 50, width: 260, height: 90 }, 4); }
  // the landed row's words against the preview's draws: the same x within 3 px (indent, spacing)
  log('landed row', J(await p.eval(`(() => { const ws = satexPdfWords(); return ws.filter((w) => ['such', 'about', 'What', 'is'].includes(w[5])).map((w) => [w[5], Math.round(w[1] * 10) / 10, Math.round(w[4] * 10) / 10]); })()`)));
  await p.close();
}

// the Typeset view: rows' texts (pending ones too) and where the caret stands (the row index it is on)
const tsState = (p) => p.eval(`(() => { const rows = [...document.querySelectorAll('#play-typeset p.ts')].filter((r) => r.checkVisibility()), c = document.querySelector('#play-typeset .pdf-caret'), cr = c?.getBoundingClientRect();
  const at = cr ? rows.findIndex((r) => { const b = r.getBoundingClientRect(); return cr.top + cr.height / 2 >= b.top && cr.top + cr.height / 2 <= b.bottom; }) : -1;
  return { rows: rows.map((r) => r.querySelector('.ts-text')?.textContent.replace(/\\u200b/g, '') ?? ''), caretRow: at, caretX: cr ? Math.round(cr.left) : null }; })()`);
/** The client point at the end of `word` (its `nth` occurrence) in the Typeset view's rows. */
const tsWordEnd = (p, word, nth = 0) => p.eval(`(() => { let n = 0; for (const r of document.querySelectorAll('#play-typeset p.ts .ts-text')) { const w = document.createTreeWalker(r, NodeFilter.SHOW_TEXT); for (let t = w.nextNode(); t; t = w.nextNode()) { let i = -1; while ((i = t.data.indexOf(${J(word)}, i + 1)) >= 0) { if (n++ === ${nth}) { const g = document.createRange(); g.setStart(t, i + ${word.length} - 1); g.setEnd(t, i + ${word.length}); const b = g.getBoundingClientRect(); return [b.right - 1, b.top + b.height / 2]; } } } } return null; })()`);
async function tsOpen(text) {
  const p = await open(text, 'play-typeset');
  await p.until(`[...document.querySelectorAll('#play-typeset p.ts .ts-text')].some((t) => t.textContent.includes('about')) && !satexHealth().busy`, 60000);
  await sleep(600);
  if (await p.eval(`document.getElementById('ts-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('ts-type').click()`);
  await sleep(300);
  return p;
}
const tsShot = (p, file) => p.eval(`(() => { const r = document.getElementById('play-typeset').getBoundingClientRect(); return { x: r.left, y: r.top, width: r.width, height: 320 }; })()`).then((c) => p.shot(file, c));
/** The rows' checks: `keep` rows unchanged (each exactly once), `typed` shown in exactly one row, the caret on it. */
function tsCheck(name, st, keep, typed) {
  for (const k of keep) if (st.rows.filter((r) => r.trim() === k).length !== 1) fails.push(`${name}: row ${J(k)} not kept as is: ${J(st.rows)}`);
  if (typed != null) {
    const hit = st.rows.map((r, i) => [r, i]).filter(([r]) => r.includes(typed));
    if (hit.length !== 1) fails.push(`${name}: ${J(typed)} in ${hit.length} rows: ${J(st.rows)}`);
    else if (st.caretRow !== hit[0][1]) fails.push(`${name}: caret on row ${st.caretRow}, typed text on row ${hit[0][1]}: ${J(st.rows)}`);
  }
}
const DEMO = "\\documentclass{article}\n\\usepackage[paperwidth=9cm,paperheight=6cm,margin=6mm]{geometry}\n\\usepackage{tikz}\n\\usetikzlibrary{arrows.meta}\n\\definecolor{accent}{HTML}{0B3D91}\n\\tikzset{>=Stealth, thick,\n  box/.style={draw=accent, fill=accent!8,\n  rounded corners=3pt, minimum width=10mm}}\n\\newcommand{\\model}{TinyNet}\n\\begin{document}\n\\section{Results}\nWith \\model, the loss falls fast.\n\n\\begin{tikzpicture}[draw=accent]\n  \\node[box] (d) at (0,0) {data};\n  \\node[box] (m) at (2.6,-0.6) {\\model};\n  \\node[box] (l) at (5.2,0) {loss};\n  \\draw[->] (d) -- node[above] {train} (m);\n  \\draw[->] (m) -- (l);\n\\end{tikzpicture}\n\\end{document}\n";
const TS1 = HEAD + '\nsuch a thing this is\n\\end{document}\n';
const KEEP = ['Hello, world!', 'What do you thing', 'about', 'such a thing this is'];

async function paraPicture(name, ms) {
  const p = await open(DEMO, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p, 'data')) { fails.push(name + ': no build'); return p.close(); }
  await sleep(800);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  const w = await wordBox(p, 'fast.');
  if (!w) { fails.push(`${name}: no "fast." on the page: ${J(await p.eval('satexPdfWords().map((x) => x[5]).join(" ")'))}`); return p.close(); }
  if (process.env.DBG) log('w', J(w));
  await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  // each sample: the typed rows' canvases below the clicked row, the picture's top (its "data" node, as the page shows
  // it now), whether a push is drawn
  const sample = () => p.eval(`(() => { const ws = satexPdfWords(), img = document.querySelector('#play-pdf .pdf-page img'), r = img.getBoundingClientRect(), k = r.width / (img.naturalWidth * 72 / ws.dpi);
    const d = ws.find((x) => x[5] === 'data'), f = ws.find((x) => x[5] === 'fast.' || x[5] === 'step.');
    const built = img.__build?.text === satexHealth().text;
    const rows = [...document.querySelectorAll('#play-pdf .pdf-echo:not(.pdf-push)')].filter((e) => (e.__texts ?? []).some((q) => /\\S/.test(q.t))).map((e) => e.getBoundingClientRect()).filter((b) => f && b.top > r.top + f[4] * k + 1);
    return { blen: img.__build?.text?.length, built, push: !!document.querySelector('#play-pdf .pdf-push'), dataTop: d ? r.top + (d[2] - 3) * k : null, rows: rows.map((b) => [Math.round(b.top), Math.round(b.bottom)]) }; })()`);
  const bad = [], builds = new Set();
  const check = async (label) => {
    const s = await sample();
    builds.add(s.blen);
    // a typed row reaching the picture's (unpushed) top with nothing pushed: drawn over it
    if (!s.built && s.dataTop != null && s.rows.some(([t, b]) => b > s.dataTop) && !s.push) {
      bad.push([label, s, await p.eval('[ document.querySelectorAll(".pdf-push").length, [...document.querySelectorAll("#play-pdf .pdf-echo")].map((e) => e.className + " " + e.tagName)]'), await p.eval(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'), b = i.__build?.text ?? '', t = satexHealth().text, a = b.indexOf('per step') >= 0 ? b.slice(b.indexOf('fast.'), b.indexOf('\\begin{tikz')) : b.slice(b.indexOf('fast.'), b.indexOf('\\begin{tikz')); return a; })()`)]);
      if (bad.length === 1) { const a = await wordBox(p, 'With'); if (a) await p.shot(name + '-bad.png', { x: a[0] - 20, y: a[1] - 30, width: 360, height: 220 }, 2); }
    }
  };
  if (process.env.DBG) log('clicked');
  for (const ch of ' It drops by $x_42$ per step.') { await key(p, ch, 100); await check(ch); }
  await key(p, 'Enter', ms); await check('Enter');
  for (const ch of 'It converges.') { await key(p, ch, ms); await check(ch); }
  // and on, keys rested: the builds land one after another, every frame over 3 s
  for (let i = 0; i < 30; i++) { await sleep(100); await check('rest'); }
  { const a = await wordBox(p, 'With'); if (a) await p.shot(name + '.png', { x: a[0] - 20, y: a[1] - 30, width: 360, height: 220 }, 2); }
  log(name, 'builds seen', builds.size, bad.length ? J(bad.slice(0, 3)) : 'no overlap', J((await p.eval('satexHealth().text')).split('\n').slice(11, 14)));
  if (bad.length) fails.push(`${name}: a typed row over the unpushed picture ${bad.length}x: ${J(bad[0])}`);
  await p.close();
}

export const CASES = {

  // coordinator 13:00 (newpara-over-picture.txt): Enter after a paragraph right above a TikZ picture, type on: the
  // new paragraph's row pushes the picture down (pdf-push) on every key until the build lands, never drawn over it
  'pdf-para-picture': () => paraPicture('pdf-para-picture', 100),
  // builds land between the keys after Enter: the build of the empty paragraph, then of its first letters
  'pdf-para-picture-slow': () => paraPicture('pdf-para-picture-slow', 450),

  // the Show menu: a pill beside layers; a switch toggles its class on the view and is kept after a reload
  async 'ts-show'() {
    const p = await tsOpen(TS1);
    await p.eval(`document.querySelector('#typeset-show-menu > summary').click()`);
    await sleep(300);
    await p.shot('ts-show-menu.png', await p.eval(`(() => { const r = document.getElementById('play-typeset').getBoundingClientRect(); return { x: r.left, y: r.top, width: r.width, height: 420 }; })()`));
    const cls = () => p.eval(`[...document.getElementById('typeset-body').classList].filter((c) => c.startsWith('ts-show-')).join(' ')`);
    const c0 = await cls();
    await p.eval(`document.getElementById('typeset-show-parskip').click()`);
    const c1 = await cls();
    log('ts-show', J(c0), '->', J(c1));
    if (!c0.includes('ts-show-parskip') || c1.includes('ts-show-parskip')) fails.push(`ts-show: parskip switch ${J(c0)} -> ${J(c1)}`);
    const h = await p.eval(`(() => { const a = document.querySelector('#typeset-show-menu > summary').getBoundingClientRect(), b = document.querySelector('#typeset-layers-menu > summary').getBoundingClientRect(); return [Math.round(a.height), Math.round(b.height), Math.round(a.top - b.top)]; })()`);
    if (h[0] !== h[1] || h[2] !== 0) fails.push(`ts-show: the pill is not like the layers pill ${J(h)}`);
    await p.close();
  },

  // user 10:16: caret keys never change what is shown (an arrow re-spliced the span typed since the click)
  async 'ts-arrows'() {
    const p = await tsOpen(TS1);
    const at = await tsWordEnd(p, 'thing', 0);
    await p.mouse(at[0] + 0.5, at[1]);
    await p.until(`satexPdfCaret()`, 3000);
    await type(p, 's or', 80);
    let st = await tsState(p);
    for (const k of ['ArrowRight', 'ArrowRight', 'ArrowLeft', 'ArrowLeft', 'ArrowLeft', 'ArrowLeft', 'ArrowLeft', 'ArrowLeft', 'ArrowRight']) {
      await key(p, k, 120);
      const now = await tsState(p);
      if (J(now.rows) !== J(st.rows)) { fails.push(`ts-arrows: ${k} changed the rows ${J(st.rows)} -> ${J(now.rows)}`); break; }
    }
    log('ts-arrows', J(st.rows));
    // then a key typed where the caret went: the row has it once
    await type(p, 'X', 80);
    st = await tsState(p);
    const text = await p.eval('satexHealth().text');
    log('ts-arrows typed', J(st.rows), J(text.split('\n')[5]));
    const line = text.split('\n')[5];
    if (st.rows.filter((r) => r.includes('do you')).length !== 1 || !st.rows.some((r) => r.trim() === line.trim())) fails.push(`ts-arrows: after typing X the row is not the source line ${J(line)}: ${J(st.rows)}`);
    // a new paragraph's row, arrows in it
    await p.close();
    const q = await tsOpen(TS1), at2 = await tsWordEnd(q, 'is', 1);
    await q.mouse(at2[0] + 0.5, at2[1]);
    await q.until(`satexPdfCaret()`, 3000);
    await key(q, 'Enter', 80);
    await type(q, 'I am not sure', 70);
    st = await tsState(q);
    for (const k of ['ArrowLeft', 'ArrowLeft', 'ArrowRight', 'ArrowRight', 'ArrowRight']) {
      await key(q, k, 120);
      const now = await tsState(q);
      if (J(now.rows) !== J(st.rows)) { fails.push(`ts-arrows new row: ${k} changed the rows ${J(st.rows)} -> ${J(now.rows)}`); break; }
    }
    await q.close();
  },
  async 'pdf-arrows'() {
    const p = await open(T0, 'play-pdf');
    await p.eval(`document.getElementById('pdf-compile').click()`);
    if (!await compiled(p)) { fails.push('pdf-arrows: no build'); return p.close(); }
    await sleep(800);
    if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
    await sleep(400);
    const w = await wordBox(p, 'What');
    await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
    await p.until(`satexPdfCaret()`, 3000);
    await type(p, ' now', 40);
    // what the row shows: the echo's draws, or none once a build that holds the text landed (then the page shows it)
    const shown = async () => (await p.eval(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build?.text === satexHealth().text; })()`)) ? 'BUILT' : (await echoes(p)).map((q) => q.t).join('·');
    const before = await shown();
    if (!before || before === 'BUILT') fails.push('pdf-arrows: no echo after typing (a build landed: type faster)');
    for (const k of ['ArrowRight', 'ArrowRight', 'ArrowLeft', 'ArrowLeft', 'ArrowLeft', 'ArrowLeft', 'ArrowLeft', 'ArrowLeft', 'ArrowRight']) {
      await key(p, k, 40);
      const now = await shown();
      // a build landing meanwhile draws no echo at all (the page shows it): only a different echo counts
      if (now !== before && now !== 'BUILT') { fails.push(`pdf-arrows: ${k} changed the echo ${J(before)} -> ${J(now)}`); break; }
    }
    log('pdf-arrows', J(before));
    await p.close();
  },

  // user 10:04: a click after the "!" \greet's body sets after its argument, then typing: after the call, never in the argument
  async 'pdf-after-macro'() {
    const p = await open(T0, 'play-pdf');
    await p.eval(`document.getElementById('pdf-compile').click()`);
    if (!await compiled(p)) { fails.push('pdf-after-macro: no build'); return p.close(); }
    await sleep(800);
    if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
    await sleep(400);
    const w = await wordBox(p, 'world!');
    await p.mouse(w[2] + 0.5, (w[1] + w[3]) / 2);
    await p.until(`satexPdfCaret()`, 3000);
    await sleep(300);
    await type(p, ' What', 60);
    log('after-macro echo', J(await echoes(p)), J(await p.eval(`[...document.querySelectorAll('#play-pdf .pdf-echo')].map((e) => [e.tagName, e.className, e.style.cssText.slice(0, 300)])`)));
    log('metrics', J(await p.eval(`(() => { const cv = document.querySelector('#play-pdf canvas.pdf-echo'), q = cv.__texts.at(-1), g = document.createElement('canvas').getContext('2d'); g.font = q.font; const m = g.measureText(q.t), r = cv.getBoundingClientRect(), s = r.height / cv.height; const ws = satexPdfWords(), img = document.querySelector('#play-pdf .pdf-page img'), ir = img.getBoundingClientRect(), k = ir.width / (img.naturalWidth * 72 / ws.dpi); const W = ws.find((w) => w[5] === 'What'), wd = ws.find((w) => w[5] === 'world!'); return { echoAsc: m.actualBoundingBoxAscent * s, echoBase: r.top + q.y * s, echoW: m.width * s * (r.width / cv.width) / s, pageWhat: [(W[4] - W[2]) * k, (W[3] - W[1]) * k], worldBase: ir.top + wd[4] * k, worldTop: ir.top + wd[2] * k, sx: r.width / cv.width, sy: s }; })()`)));
    { const a = await wordBox(p, 'about'); await p.shot('pdf-after-macro.png', { x: a[0] - 4, y: a[1] - 30, width: 130, height: 26 }, 8); }
    const text = await p.eval('satexHealth().text');
    log('pdf-after-macro', J(text.split('\n')[3]));
    if (!text.includes('\\greet{world} What\n')) fails.push(`pdf-after-macro: source line ${J(text.split('\n')[3])}`);
    // the click before the "!" (on "d"'s right): in the argument
    await p.close();
  },
  async 'ts-after-macro'() {
    const p = await tsOpen(TS1);
    const at = await tsWordEnd(p, 'world!', 0);
    await p.mouse(at[0] - 0.5, at[1]);
    await p.until(`satexPdfCaret()`, 3000);
    await type(p, ' What', 80);
    const text = await p.eval('satexHealth().text'), st = await tsState(p);
    if (!/^Hello, world!( What)?$/.test(st.rows[0])) fails.push(`ts-after-macro: the row shows ${J(st.rows[0])}`);
    log('ts-after-macro', J(text.split('\n')[3]), J(st));
    if (!text.includes('\\greet{world} What\n')) fails.push(`ts-after-macro: source line ${J(text.split('\n')[3])}`);
    await tsShot(p, 'ts-after-macro.png');
    await p.close();
    // the click on the argument's last letter (before the body's "!"): in the argument still
    const q = await tsOpen(TS1), at2 = await tsWordEnd(q, 'world', 0);
    await q.mouse(at2[0] - 0.5, at2[1]);
    await q.until(`satexPdfCaret()`, 3000);
    await type(q, 's', 80);
    const t2 = await q.eval('satexHealth().text');
    if (!t2.includes('\\greet{worlds}\n')) fails.push(`ts-after-macro: a click in the argument wrote ${J(t2.split('\n')[3])}`);
    await q.close();
  },

  // the Typeset view (user 10:02): Enter after "is", type "I do not": its own row, the row above untouched; the run lands; Enter again: an empty row with the caret
  async 'ts-new-para'() {
    const p = await tsOpen(TS1);
    const at = await tsWordEnd(p, 'is', 1);
    if (!at) { fails.push('ts-new-para: no "is" in the view'); return p.close(); }
    await p.mouse(at[0] + 0.5, at[1]);
    await p.until(`satexPdfCaret()`, 3000);
    await key(p, 'Enter', 80);
    let st = await tsState(p);
    log('ts after Enter', J(st));
    tsCheck('ts-new-para Enter', st, KEEP, null);
    if (st.caretRow !== st.rows.length - 1 || st.rows.length !== KEEP.length + 1) fails.push(`ts-new-para: Enter shows no new row with the caret: ${J(st)}`);
    await type(p, 'I do not', 70);
    st = await tsState(p);
    log('ts typed', J(st));
    tsCheck('ts-new-para typed', st, KEEP, 'I do not');
    await tsShot(p, 'ts-new-para-typed.png');
    await p.until(`!satexHealth().busy && [...document.querySelectorAll('#play-typeset p.ts:not(.ts-typed) .ts-text')].some((t) => t.textContent.includes('I do not'))`, 30000);
    await sleep(800);
    st = await tsState(p);
    log('ts landed', J(st));
    tsCheck('ts-new-para landed', st, KEEP, 'I do not');
    await key(p, 'Enter', 80);
    st = await tsState(p);
    log('ts Enter again', J(st));
    tsCheck('ts-new-para Enter2', st, [...KEEP, 'I do not'], null);
    if (st.caretRow !== st.rows.length - 1 || st.rows.length !== KEEP.length + 2) fails.push(`ts-new-para: second Enter shows no new row with the caret: ${J(st)}`);
    await tsShot(p, 'ts-new-para-enter2.png');
    const text = await p.eval('satexHealth().text');
    if (!/such a thing this is\n\nI do not\n\n/.test(text)) fails.push(`ts-new-para: source ${J(text.slice(HEAD.length))}`);
    await p.close();
  },
  // typing inside a paragraph's word: only that row changes
  async 'ts-mid'() {
    const p = await tsOpen(TS1);
    const at = await tsWordEnd(p, 'thing', 0);
    await p.mouse(at[0] + 0.5, at[1]);
    await p.until(`satexPdfCaret()`, 3000);
    await type(p, 's', 80);
    const st = await tsState(p);
    log('ts-mid', J(st));
    tsCheck('ts-mid', st, KEEP.filter((k) => !k.startsWith('What')), 'What do you things');
    await p.close();
  },
  // the PDF: click the end of "about", Enter, type the screenshot's text; the build after must match the preview
  'pdf-new-para': () => newPara('pdf-new-para', 25),
  // a person's pace: builds land between the keys (the open-paragraph branch, then the paragraph the build sets)
  'pdf-new-para-slow': () => newPara('pdf-new-para-slow', 160),
  // the editor: type the paragraph in the source while the PDF shows (Type on, caret in the PDF follows the editor?)
  async 'pdf-editor-para'() {
    const p = await open(T0, 'play-pdf');
    await p.eval(`document.getElementById('pdf-compile').click()`);
    if (!await compiled(p)) { fails.push('pdf-editor-para: no build'); return p.close(); }
    await sleep(800);
    if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
    await sleep(400);
    const w = await wordBox(p, 'about');
    await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
    await p.until(`satexPdfCaret()`, 3000);
    await key(p, 'Enter', 30);
    await type(p, 'such a thing', 25);
    await sleep(1500);
    // then on in the editor: its cursor where the PDF's caret is
    await p.eval(`document.querySelector('.cm-content').focus()`);
    await type(p, ' this is ', 25);
    const ts = await echoes(p);
    log('pdf-editor-para echo', J(checkEcho('pdf-editor-para', ts, 'such a thing this is')), 'text', J((await p.eval('satexHealth().text')).slice(HEAD.length - 6)));
    await p.shot('pdf-editor-para.png', await p.eval(`(() => { const r = document.querySelector('#play-pdf .pdf-page').getBoundingClientRect(); return { x: r.left, y: r.top, width: r.width, height: Math.min(r.height, 400) }; })()`));
    await p.close();
  },
};

export async function echo(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: { log: [] } };
  fails = res.fails; first = p; log = (...a) => res.info.log.push(a.join(' ').slice(0, 300));
  if (!CASES[sc.case]) throw new Error(`no echo case ${sc.case}`);
  await CASES[sc.case]();
  return res;
}
