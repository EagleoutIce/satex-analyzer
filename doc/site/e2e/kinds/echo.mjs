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
// a centered table of `l`, `c` and `r` columns with rules: typing widens a column for all its rows
// the column chips shown, left and right edges in px
const CHIPS = `[...document.querySelectorAll('#play-pdf .pdf-col')].filter((e) => getComputedStyle(e).visibility !== 'hidden').map((e) => { const r = e.getBoundingClientRect(); return [r.left, r.right]; }).sort((a, b) => a[0] - b[0])`;
const TABLE_DOC = '\\documentclass{article}\n\\begin{document}\nBefore the table.\n\\begin{center}\n\\begin{tabular}{|l|c|r|}\n\\hline\napple & mid & zed \\\\\napricot & m & zz \\\\\n\\hline\nap & middle & zzz \\\\\n\\hline\n\\end{tabular}\n\\end{center}\nAfter it.\n\\end{document}\n';
// a centered table whose first row is a `\\multicolumn{2}{c}` span over the `l` and `c` columns
const SPAN_DOC = TABLE_DOC.replace('\\hline\napple', '\\hline\n\\multicolumn{2}{|c|}{Heading} & top \\\\\n\\hline\napple');
// a table with a paragraph column (`p`, `m` with array, `X` in a tabularx of 6cm) of 2cm between an `l` and an `r` one
const PAR_DOC = (c) => `\\documentclass{article}\n\\usepackage{array,tabularx}\n\\begin{document}\nBefore the table.\n\n${c === 'X' ? '\\begin{tabularx}{6cm}{|l|X|r|}' : `\\begin{tabular}{|l|${c}{2cm}|r|}`}\n\\hline\nleft & one two & right \\\\\n\\hline\nbelow & cell & here \\\\\n\\hline\n\\end{${c === 'X' ? 'tabularx' : 'tabular'}}\n\nAfter it.\n\\end{document}\n`;
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

/** A table's other rows follow a typed cell: the end of `word` clicked, `keys` typed fast. The echo
 * moves every other word of the table (satexPdfBlock's `moved`) where Compile's build then sets it, within 2.5 bp, and
 * the caret stays put when that build lands. Red before 2026-11-22: only the typed row moved. */
async function tableRows(name, word, keys, doc = TABLE_DOC, skip = []) {
  const p = await open(doc, 'play-pdf');
  const build = async () => { await p.eval(`(self.__e2eG = document.querySelector('#play-pdf .pdf-page img')?.__build ?? null, document.getElementById('pdf-compile').click())`); return p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build && i.__build !== self.__e2eG && i.naturalWidth > 100 && !document.querySelector('#play-pdf.pdf-running'); })()`, 90000); };
  if (!await build()) { fails.push(name + ': no build'); return p.close(); }
  await sleep(800);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  // the table's words: between "table." and "After"
  const table = (ws) => { const a = ws.find((w) => w[5] === 'table.'), b = ws.find((w) => w[5] === 'After'); return ws.filter((w) => a && b && w[0] === a[0] && w[4] > a[4] + 2 && w[4] < b[4] - 2); };
  const before = table(await p.eval(`satexPdfWords()`));
  const w = await wordBox(p, word);
  if (!w || before.length < 6) { fails.push(`${name}: table words ${J(before.map((v) => v[5]))}`); return p.close(); }
  await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  // fast keys: the echo is read before a build of them lands (Type mode builds whatever the automatic compile says)
  await type(p, keys, 20);
  const moved = await p.eval(`satexPdfBlock(1).moved ?? []`), pushed = await p.eval(`satexPdfBlock(1).pushed`), chips0 = await p.eval(CHIPS), x0 = await p.eval(`document.querySelector('#play-pdf .pdf-caret')?.getBoundingClientRect().left ?? null`);
  const early = await p.eval(`!document.querySelector('#play-pdf .pdf-page img').__build?.text?.includes(${J(keys)})`);
  await p.shot(name + '-echo.png', { x: w[0] - 160, y: w[1] - 40, width: 420, height: 110 }, 3);
  if (!early) fails.push(`${name}: a build of the keys landed before the echo was read`);
  if (!await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build?.text === satexHealth().text && !document.querySelector('#play-pdf.pdf-running'); })()`, 30000) && !await build()) { fails.push(name + ': the typed text was never built'); return p.close(); }
  await sleep(1500);
  await p.shot(name + '-landed.png', { x: w[0] - 160, y: w[1] - 40, width: 420, height: 110 }, 3);
  const after = table(await p.eval(`satexPdfWords()`)), chips1 = await p.eval(CHIPS), x1 = await p.eval(`document.querySelector('#play-pdf .pdf-caret')?.getBoundingClientRect().left ?? null`);
  const same = (a, b) => a.every((v, i) => v === b[i]), off = [];
  for (const v of before) {
    if (v[5] === word || skip.includes(v[5])) continue;
    // where the echo drew it: moved (dx, dy), else pushed down with what is below the push's top
    const m = moved.find((m) => same(m.from, v)), want = v[1] + (m?.dx ?? 0), wantY = v[4] + (m ? m.dy : pushed && v[4] > pushed.top ? pushed.dy : 0);
    const got = after.filter((u) => u[5] === v[5]).sort((a, b) => Math.hypot(a[1] - want, a[4] - wantY) - Math.hypot(b[1] - want, b[4] - wantY))[0];
    if (!got) { off.push(`${v[5]} gone`); continue; }
    if (Math.abs(got[1] - want) > 2.5 || Math.abs(got[4] - wantY) > 2.5) off.push(`${v[5]} echoed at ${want.toFixed(1)},${wantY.toFixed(1)}, built at ${got[1].toFixed(1)},${got[4].toFixed(1)} bp`);
  }
  log(name, J({ words: before.length, moved: moved.length, off, caret: [x0, x1] }));
  if (off.length) fails.push(`${name}: the table's other words jump when the build lands: ${off.join('; ')}`);
  // the column chips (the pointer rests on the table) stand where the build then puts them
  const chipOff = chips0.length !== chips1.length ? [`${chips0.length} chips, then ${chips1.length}`] : chips0.flatMap((c, i) => c.some((v, j) => Math.abs(v - chips1[i][j]) > 4) ? [`${c.map(Math.round)} -> ${chips1[i].map(Math.round)}`] : []);
  if (chipOff.length) fails.push(`${name}: the column chips jump when the build lands: ${chipOff.join('; ')}`);
  if (x0 == null || x1 == null || Math.abs(x1 - x0) > 4) fails.push(`${name}: the caret moved ${x0?.toFixed(1)} -> ${x1?.toFixed(1)} px when the build landed`);
  await p.close();
}

/** A command name typed in the PDF (`\su` on the way to `\sum`): no "1 error" while the name is unfinished. Samples the
 * status lines every 50 ms through a 1 s pause after `\su`. Red before 2026-11-22: the run of `\su` showed its
 * undefined-name finding until the name was done. */
async function namingQuiet(name) {
  const p = await open(T0, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p)) { fails.push(name + ': no build'); return p.close(); }
  await sleep(1500);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  const w = await wordBox(p, 'about');
  await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  await type(p, ' $\\su', 80);
  const seen = [];
  for (let t = 0; t < 20; t++) { seen.push(await p.eval(`(document.getElementById('play-status')?.textContent ?? '') + ' | ' + (document.getElementById('pdf-status')?.textContent ?? '')`)); await sleep(50); }
  log('built', J(await p.eval(`(() => { const t = document.querySelector('#play-pdf .pdf-page img')?.__build?.text ?? ''; const i = t.indexOf('about'); return t.slice(i, i + 20); })()`)));
  await type(p, 'm x$', 80);
  const bad = seen.filter((t) => /\b\d+ errors?\b/.test(t));
  log(name, J({ samples: seen.length, bad: bad.slice(0, 2) }));
  if (bad.length) fails.push(`${name}: an error shown while \\su was typed (${bad.length} of ${seen.length} samples): ${J(bad[0])}`);
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
  // the screenshot of 2026-11-21: `\(\sum \)` typed into a new paragraph, the caret on the sum's row, the editor alike
  'pdf-math-caret': () => mathCaret('pdf-math-caret', 60),
  'pdf-math-caret-slow': () => mathCaret('pdf-math-caret-slow', 400),
  'pdf-macro-comma': () => macroComma('pdf-macro-comma', ['x', 'y']),
  'pdf-macro-comma-space': () => macroComma('pdf-macro-comma-space', [' ', 'x']),
  // caret2 (screenshots 2026-11-21 18:14, 18:15): the edges of a macro's output are the call's edges
  'pdf-macro-edges': () => macroEdges('pdf-macro-edges'),
  // caret2 (user 2026-11-21): Up and Down by visual line, in two columns
  'pdf-updown-columns': () => upDownColumns('pdf-updown-columns'),
  // caret2 (user 2026-11-21): typing into teletype text echoes in its fixed-width face
  'pdf-tt-echo': () => ttEcho('pdf-tt-echo'),
  'pdf-math-caret-below': () => mathCaret('pdf-math-caret-below', 60, true),
  // a cell typed wider than its column's widest: the column grows in every row, the columns after it move (`l`, `c`)
  'pdf-naming-quiet': () => namingQuiet('pdf-naming-quiet'),
  'pdf-table-rows-l': () => tableRows('pdf-table-rows-l', 'apple', 'sauce'),
  'pdf-table-rows-c': () => tableRows('pdf-table-rows-c', 'm', 'agnificent'),
  // a `\multicolumn` span over the grown column moves by its share; a span typed wider widens its last column
  // a paragraph cell typed past its width: its lines wrap, the rows below and the rules move down; `m`: the row's other
  // cells by half; an `X` column of tabularx alike
  'pdf-table-par-p': () => tableRows('pdf-table-par-p', 'two', ' more words to wrap', PAR_DOC('p'), ['one', 'more', 'words', 'to', 'wrap']),
  'pdf-table-par-m': () => tableRows('pdf-table-par-m', 'two', ' more words to wrap', PAR_DOC('m'), ['one', 'more', 'words', 'to', 'wrap']),
  'pdf-table-par-x': () => tableRows('pdf-table-par-x', 'two', ' more words to wrap', PAR_DOC('X'), ['one', 'more', 'words', 'to', 'wrap']),
  'pdf-table-span-under': () => tableRows('pdf-table-span-under', 'apple', 'sauce', SPAN_DOC),
  'pdf-table-span-typed': () => tableRows('pdf-table-span-typed', 'Heading', 'sverylong', SPAN_DOC),
  'pdf-math-caret-below-slow': () => mathCaret('pdf-math-caret-below-slow', 400, true),
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

// The PDF caret against the source caret: the editor's cursor at the caret's offset, and the caret drawn on a row
// SyncTeX sets from the caret's source line (where it sets any: a macro's definition is set at its calls) once a build of the text as typed shows, else marked pending (never on
// another paragraph's row). `{line, built, pending, onRow, editor, at}`; null where no caret is drawn.
const caretState = (p) => p.eval(`(() => {
  const c = document.querySelector('#play-pdf .pdf-caret'), pc = satexPdfCaret(), h = satexHealth();
  if (!c || !pc) return null;
  const pg = c.closest('.pdf-page'), img = pg?.querySelector('img');
  if (!img) return null;
  const r = img.getBoundingClientRect(), cr = c.getBoundingClientRect(), ws = satexPdfWords(), k = r.height / (img.naturalHeight * 72 / ws.dpi);
  const line = h.text.slice(0, pc.at).split('\\n').length, built = img.__build?.text === h.text, y = (cr.top + cr.height / 2 - r.top) / k;
  const rows = satexPdfBoxes().filter((b) => b.line === line && b.page === +pg.dataset.page).map((b) => b.y);
  return { line, built, pending: c.classList.contains('pdf-caret-stale'), onRow: rows.some((b) => y > b - 9 && y < b + 2), y: Math.round(y), rows: [...new Set(rows.map(Math.round))], editor: h.cursor?.[1] ?? null, at: pc.at, ghost: pc.ghost, own: pc.own };
})()`);
/** The checks of one caret sample: the editor at the caret's offset; a landed build's caret on its line's row or pending. */
function caretCheck(name, st, what) {
  if (!st) return;
  if (st.editor != null && st.editor !== st.at) fails.push(`${name}: ${what}: editor cursor ${st.editor}, PDF caret ${st.at}`);
  if (st.built && !st.pending && st.rows.length && !st.onRow) fails.push(`${name}: ${what}: caret on y ${st.y} bp, line ${st.line} is set on ${J(st.rows)}`);
}
/** The screenshot of 2026-11-21 13:16: a paragraph after `\textbf{world}`, `\(\sum \)` typed into a new paragraph from
 * the pages. The caret stayed on "I think…" (the word it was put in) while the build set the sum a row below. */
async function mathCaret(name, ms, below = false) {
  const T = '\\documentclass{article}\n\\begin{document}\nHello, world! This is an amazing concept\n\nWhat do you say?\n\nI think it is nice to consider a \\textbf{world}\n\\end{document}\n';
  const p = await open(T, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p, 'world')) { fails.push(name + ': no build'); return p.close(); }
  await sleep(800);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  const w = await wordBox(p, 'world', 1) ?? await wordBox(p, 'world');
  // `below`: a click on the empty page under the last paragraph, a new paragraph the first key writes; else the end of
  // "world" and Enter
  if (below) await p.mouse(w[0] + 2, w[3] + 3 * (w[3] - w[1])); else await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  // the editor's cursor follows once the keys rest (150 ms, pdfCaretDraw): sampled after that
  const sample = async (what) => { await sleep(250); const st = await caretState(p); log(what, J(st)); caretCheck(name, st, what); };
  if (!below) await key(p, 'Enter', ms);
  for (const k of ['\\', '(', '\\', 's', 'u', 'm']) { await key(p, k, ms); await sample(`key ${k}`); }
  await key(p, 'Escape', ms);
  await key(p, ' ', ms);
  await sample('space');
  // the build of the text as typed lands: the caret on the sum's row, the source's cursor at the same offset
  await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build?.text === satexHealth().text && !document.querySelector('#play-pdf.pdf-running'); })()`, 20000);
  await sleep(800);
  await sample('landed');
  log('text', J((await p.eval('satexHealth().text')).slice(-60)));
  await p.shot(name + '.png', await p.eval(`(() => { const r = document.querySelector('#play-pdf .pdf-page').getBoundingClientRect(); return { x: r.left, y: r.top, width: r.width, height: Math.min(r.height, 300) }; })()`));
  // typing on in text after the formula, then a new paragraph: each landed build's caret on its row
  for (const k of ['a', 'n', 'd']) { await key(p, k, ms); await sample(`after ${k}`); }
  await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build?.text === satexHealth().text && !document.querySelector('#play-pdf.pdf-running'); })()`, 20000);
  await sleep(800);
  await sample('landed 2');
  await p.close();
}

/** The screenshot of 2026-11-21 13:21: a click right after "Hello," of `\greet{world}` (its definition's text
 * `{Hello, #1!}`), then keys: the row as drawn must hold the comma once ("Hello,x, world!" was drawn). */
async function macroComma(name, keys) {
  const T = '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\n\\greet{world} This is an amazing concept\n\\end{document}\n';
  const p = await open(T, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p, 'amazing')) { fails.push(name + ': no build'); return p.close(); }
  await sleep(800);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  const w = await wordBox(p, 'Hello,');
  if (!w) { fails.push(`${name}: no "Hello," on the page: ${J(await p.eval('satexPdfWords().map((x) => x[5]).join(" ")'))}`); return p.close(); }
  await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(400);
  const row = async (what) => {
    // the echo's row as drawn: its words' texts (the page's own pixels moved carry their text hidden)
    const txt = await p.eval(`[...document.querySelectorAll('#play-pdf .pdf-echo:not(.pdf-push)')].map((e) => e.textContent).join('|')`);
    const st = await caretState(p);
    log(what, J(txt), J(st));
    if ((txt.match(/,/g) ?? []).length > 1) fails.push(`${name}: ${what}: the comma drawn twice: ${J(txt)}`);
    caretCheck(name, st, what);
  };
  await row('clicked');
  for (const k of keys) { await key(p, k, 60); await row(`key ${k}`); await p.shot(`${name}-${keys.indexOf(k)}.png`, { x: w[0] - 10, y: w[1] - 15, width: 260, height: 50 }, 4); }
  log('text', J((await p.eval('satexHealth().text')).split('\n')[1]));
  await p.shot(name + '.png', { x: w[0] - 10, y: w[1] - 15, width: 260, height: 50 }, 4);
  await p.close();
}

/** The screenshots of 2026-11-21 18:14 and 18:15 (`\greet{world}` setting "Hello, world!"): an editor position after
 * the call is drawn after the output's last glyph `!` (was: before it, the argument's end), also after Left/Right and
 * once a build lands; a click before "Hello" puts the caret before the call (typing goes into the document, was: into
 * the definition's `{|Hello`), one inside the output into the definition. */
async function macroEdges(name) {
  const T = '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\n\\greet{world}\n\nThis is an example text.\n\\end{document}\n';
  const p = await open(T, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p, 'example')) { fails.push(name + ': no build'); return p.close(); }
  await sleep(800);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  const call = T.indexOf('\\greet{world}'), after = call + '\\greet{world}'.length;
  const caretX = () => p.eval(`(() => { const c = document.querySelector('#play-pdf .pdf-caret'); if (!c) return null; const r = c.getBoundingClientRect(); return r.left + r.width / 2; })()`);
  const at = () => p.eval(`satexPdfCaret()?.at ?? null`);
  const hello = await wordBox(p, 'Hello,'), world = await wordBox(p, 'world!');
  if (!hello || !world) { fails.push(`${name}: no "Hello, world!" on the page: ${J(await p.eval('satexPdfWords().map((x) => x[5]).join(" ")'))}`); return p.close(); }
  const mid = (w) => (w[1] + w[3]) / 2;
  // the end of the output: after the call, drawn after `!`, after the arrows too
  await p.mouse(world[2] - 0.5, mid(world));
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  const end = async (what) => {
    const a = await at(), x = await caretX();
    log(what, a, x, J(world));
    if (a !== after) fails.push(`${name}: ${what}: caret at ${a}, want ${after} (after the call)`);
    if (x == null || Math.abs(x - world[2]) > 2) fails.push(`${name}: ${what}: caret drawn at x ${x?.toFixed(1)}, want after "!" at ${world[2].toFixed(1)}`);
  };
  await end('clicked end');
  await key(p, 'ArrowLeft', 200); await key(p, 'ArrowRight', 300);
  await end('left, right');
  // a build of the text lands (a key typed and taken back): the caret placed by its offset, no click's x
  await key(p, 'Z', 300); await key(p, 'Backspace', 300);
  await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build?.text === satexHealth().text && !document.querySelector('#play-pdf.pdf-running'); })()`, 20000);
  await sleep(800);
  await end('landed');
  await p.shot(name + '-end.png', { x: hello[0] - 20, y: hello[1] - 15, width: 240, height: 50 }, 4);
  // the start of the output: before the call
  await key(p, 'Escape', 200);
  await p.mouse(hello[0] + 0.3, mid(hello));
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  const a0 = await at(), x0 = await caretX();
  log('clicked start', a0, x0);
  if (a0 !== call) fails.push(`${name}: start: caret at ${a0} ${J(T.slice(a0 - 12, a0))}|, want ${call} (before the call)`);
  if (x0 == null || Math.abs(x0 - hello[0]) > 2) fails.push(`${name}: start: caret drawn at x ${x0?.toFixed(1)}, want before "H" at ${hello[0].toFixed(1)}`);
  // placed by its offset once a build lands: before "H" still (was: before "world!", the first word the source spells)
  await key(p, 'Z', 300); await key(p, 'Backspace', 300);
  await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build?.text === satexHealth().text && !document.querySelector('#play-pdf.pdf-running'); })()`, 20000);
  await sleep(800);
  const x1 = await caretX();
  log('start landed', await at(), x1);
  if (x1 == null || Math.abs(x1 - hello[0]) > 2) fails.push(`${name}: start landed: caret drawn at x ${x1?.toFixed(1)}, want before "H" at ${hello[0].toFixed(1)}`);
  // inside the output (after "Hel"): the definition
  await key(p, 'Escape', 200);
  await p.mouse(hello[0] + (hello[2] - hello[0]) * 0.45, mid(hello));
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  const a1 = await at(), d0 = T.indexOf('{Hello') + 1;
  log('clicked inside', a1);
  if (!(a1 > d0 && a1 < d0 + 6)) fails.push(`${name}: inside: caret at ${a1}, want in the definition's "Hello" ${d0}..${d0 + 6}`);
  // typing before the output goes into the document
  await key(p, 'Escape', 200);
  await p.mouse(hello[0] + 0.3, mid(hello));
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  await key(p, 'X', 300);
  const t = await p.eval('satexHealth().text');
  log('typed', J(t.split('\n').slice(1, 4)));
  if (!t.includes('{Hello, #1!}') || !t.includes('X\\greet{world}')) fails.push(`${name}: X typed before the output: ${J(t.split('\n').slice(1, 4))}, want X\\greet{world} and the definition kept`);
  await p.close();
}

/** Up and Down in the PDF move by visual line as Home and End do, keeping the x they started from; in two columns they
 * stay in the column and go on from a column's bottom to the next column's top (and back up to its bottom). Was: the
 * nearest baseline with a word under the caret's x, the x drifting to word edges, stuck at a column's end. */
async function upDownColumns(name) {
  const body = Array.from({ length: 40 }, (_, i) => `Line ${i} has words q${i}a q${i}b q${i}c q${i}d.`).join('\n');
  const T = `\\documentclass[twocolumn]{article}\n\\usepackage[paperwidth=12cm,paperheight=9cm,margin=8mm]{geometry}\n\\pagestyle{empty}\n\\begin{document}\n${body}\n\\end{document}\n`;
  const p = await open(T, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p, 'q2a')) { fails.push(name + ': no build'); return p.close(); }
  await sleep(800);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  const caret = () => p.eval(`(() => { const c = document.querySelector('#play-pdf .pdf-caret'), i = document.querySelector('#play-pdf .pdf-page img'); if (!c || !i) return null; const r = c.getBoundingClientRect(), q = i.getBoundingClientRect(); return { x: r.left - q.left, y: r.top - q.top, w: q.width, at: satexPdfCaret()?.at }; })()`);
  const start = await wordBox(p, 'q2b');
  if (!start) { fails.push(`${name}: no q2b on the page`); return p.close(); }
  const img = await p.eval(`(() => { const q = document.querySelector('#play-pdf .pdf-page img').getBoundingClientRect(); return [q.left, q.top]; })()`);
  await p.mouse(start[0] + (start[2] - start[0]) * 0.5, (start[1] + start[3]) / 2);
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  const c0 = await caret(), seen = [c0];
  log('start', J(c0));
  // down the left column: one line a key, the x kept, until the caret goes over to the right column
  let k = 0, c = c0;
  for (; k < 40; k++) {
    await key(p, 'ArrowDown', 250);
    const n = await caret();
    if (!n) { fails.push(`${name}: down ${k}: no caret`); break; }
    seen.push(n);
    if (n.x > c0.w / 2) { c = n; break; }
    if (n.y <= c.y + 2) { fails.push(`${name}: down ${k}: caret did not move down (y ${c.y.toFixed(1)} -> ${n.y.toFixed(1)})`); break; }
    if (n.y - c.y > 3 * (start[3] - start[1])) fails.push(`${name}: down ${k}: skipped lines (y ${c.y.toFixed(1)} -> ${n.y.toFixed(1)})`);
    if (Math.abs(n.x - c0.x) > 12) fails.push(`${name}: down ${k}: x drifted ${c0.x.toFixed(1)} -> ${n.x.toFixed(1)}`);
    c = n;
  }
  log('down', k, J(seen.map((q) => [Math.round(q.x), Math.round(q.y)])));
  const bottom = seen.at(-2);
  if (!(c.x > c0.w / 2)) fails.push(`${name}: Down never reached the right column (last ${J(c)})`);
  else {
    // the right column's first line, at the same x within the column
    if (c.y > c0.y) fails.push(`${name}: the right column's caret at y ${c.y.toFixed(1)}, want its top line (start was ${c0.y.toFixed(1)})`);
    // the columns' left edges (the page's words, in image px): the same x within the column
    const [l0, l1] = await p.eval(`(() => { const ws = satexPdfWords().filter((w) => w[0] === 1), i = document.querySelector('#play-pdf .pdf-page img'), k = i.getBoundingClientRect().width / (i.naturalWidth * 72 / satexPdfWords().dpi), mid = Math.max(...ws.map((w) => w[3])) / 2 + Math.min(...ws.map((w) => w[1])) / 2; return [Math.min(...ws.map((w) => w[1])) * k, Math.min(...ws.filter((w) => w[1] > mid).map((w) => w[1])) * k]; })()`);
    if (Math.abs(c.x - l1 - (c0.x - l0)) > 12) fails.push(`${name}: the right column's caret at x ${c.x.toFixed(1)}, want about ${(c0.x - l0 + l1).toFixed(1)} (the same x in the column)`);
    await key(p, 'ArrowUp', 300);
    const u = await caret();
    log('up', J(u));
    if (!u || u.x > c0.w / 2 || Math.abs(u.y - bottom.y) > 2) fails.push(`${name}: Up from the right column's top: ${J(u)}, want the left column's last line ${J(bottom)}`);
  }
  await p.shot(name + '.png', { x: img[0], y: img[1], width: c0.w, height: 340 }, 2);
  await p.close();
}

/** Typing into `\\texttt{…}` from the page: the echo sets the typed letters in the teletype face or a monospace
 * fallback at its advance (was: Latin Modern Roman's proportional widths), so the caret keeps its place when the build
 * lands. Letters the page's subset lacks (`q`, `z`) test the fallback. */
async function ttEcho(name) {
  const T = '\\documentclass{article}\n\\begin{document}\nPlain words and \\texttt{code words here} end.\n\\end{document}\n';
  const p = await open(T, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p, 'end.')) { fails.push(name + ': no build'); return p.close(); }
  await sleep(800);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(400);
  const w = await wordBox(p, 'here');
  await p.mouse(w[2] - 0.5, (w[1] + w[3]) / 2);
  await p.until(`satexPdfCaret()`, 3000);
  await sleep(300);
  for (const k of 'qzqz') await key(p, k, 80);
  await sleep(200);
  const x0 = await p.eval(`document.querySelector('#play-pdf .pdf-caret')?.getBoundingClientRect().left ?? null`), es = await echoes(p);
  log('echo', J(es.map((q) => [q.t, q.font])));
  const typed = es.filter((q) => /[qz]/.test(q.t));
  if (!typed.length) fails.push(`${name}: no echo of the typed letters: ${J(es)}`);
  for (const q of typed) if (!/g_|Mono|monospace/.test(q.font)) fails.push(`${name}: ${J(q.t)} drawn in ${J(q.font)}, want the teletype face or a monospace`);
  await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build?.text === satexHealth().text && !document.querySelector('#play-pdf.pdf-running'); })()`, 20000);
  await sleep(800);
  const x1 = await p.eval(`document.querySelector('#play-pdf .pdf-caret')?.getBoundingClientRect().left ?? null`);
  log('caret', x0, x1);
  if (x0 == null || x1 == null || Math.abs(x1 - x0) > 3) fails.push(`${name}: the caret moved ${x0?.toFixed(1)} -> ${x1?.toFixed(1)} when the build landed`);
  await p.shot(name + '.png', { x: w[0] - 200, y: w[1] - 15, width: 420, height: 50 }, 3);
  await p.close();
}

export async function echo(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: { log: [] } };
  fails = res.fails; first = p; log = (...a) => res.info.log.push(a.join(' ').slice(0, 300));
  if (!CASES[sc.case]) throw new Error(`no echo case ${sc.case}`);
  await CASES[sc.case]();
  return res;
}
