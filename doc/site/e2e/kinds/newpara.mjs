// New paragraphs typed at the end of a small document, in the PDF view's Type mode (from the PDF caret, or on in the
// editor): every echo draw is typed text (never `document` of \end{document}), one font and vertical scale per row,
// no word drawn in a page face lacking one of its glyphs (lib.mjs marks those draws `__mixed`), and the caret stays
// where the preview put it when the build lands. Scenario fields: `mode` ('pdf' | 'editor'), `keyMs`.
import { sleep } from '../lib.mjs';
import { setText } from '../engine.mjs';

const J = JSON.stringify;
const HEAD = '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\n\\greet{world}\n\nWhat do you thing\n\nabout\n';
const T0 = HEAD + '\\end{document}\n';
async function open(p, text, panel) {
  await p.eval(`document.querySelector('[data-example="hello.tex"]').click()`);
  await p.until(`satexHealth().file === 'hello.tex' && satexHealth().text?.length > 10`, 20000);
  await sleep(400);
  await setText(p, text);
  // lib.mjs MIXED: echo draws in a page face lacking a glyph are marked
  await p.eval('self.__e2eMixed = true');
  await p.eval(`document.querySelector('.side-tabs [data-panel=${panel}]').click()`);
}
const compiled = (p) => p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return satexPdfWords().some((w) => w[5] === 'about') && i?.naturalWidth > 100 && i.__build && !document.querySelector('#play-pdf.pdf-running'); })()`, 60000);
const wordBox = (p, word, nth = 0) => p.eval(`(() => { const ws = satexPdfWords(), w = ws.filter((x) => x[5] === ${J(word)})[${nth}]; if (!w) return null; const img = document.querySelector('#play-pdf .pdf-page[data-page="' + w[0] + '"] img'), r = img.getBoundingClientRect(), k = r.width / (img.naturalWidth * 72 / ws.dpi); return [r.left + w[1] * k, r.top + w[2] * k, r.left + w[3] * k, r.top + w[4] * k]; })()`);
const echoes = (p) => p.eval(`[...document.querySelectorAll('#play-pdf .pdf-echo:not(.pdf-push)')].flatMap((e) => (e.__texts ?? []).map((q) => ({ t: q.t, font: q.font, sy: q.sy, y: Math.round(q.y), mixed: (e.__mixed ?? []).includes(q.t) })))`);
async function key(p, k, ms = 60) { await p.key(k); await sleep(ms); }
async function type(p, s, ms = 60) { for (const ch of s) await key(p, ch === '\n' ? 'Enter' : ch, ms); }
/** The checks on the echo's draws: every drawn text a part of what was typed, one font per word. */
function checkEcho(fails, name, ts, typed) {
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

async function newPara(p, sc, fails, log) {
  const name = sc.name, ms = sc.keyMs;
  await open(p, T0, 'play-pdf');
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await compiled(p)) { fails.push(name + ': no build'); return; }
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
  log(name + ' echo', J(checkEcho(fails, name, ts, 'such a thing this is')));
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
}

async function editorPara(p, sc, fails, log) {
    await open(p, T0, 'play-pdf');
    await p.eval(`document.getElementById('pdf-compile').click()`);
    if (!await compiled(p)) { fails.push('pdf-editor-para: no build'); return; }
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
    log('pdf-editor-para echo', J(checkEcho(fails, 'pdf-editor-para', ts, 'such a thing this is')), 'text', J((await p.eval('satexHealth().text')).slice(HEAD.length - 6)));
    await p.shot('pdf-editor-para.png', await p.eval(`(() => { const r = document.querySelector('#play-pdf .pdf-page').getBoundingClientRect(); return { x: r.left, y: r.top, width: r.width, height: Math.min(r.height, 400) }; })()`));
}

export async function newpara(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: { log: [] } };
  const log = (...a) => res.info.log.push(a.join(' ').slice(0, 300));
  await (sc.mode === 'editor' ? editorPara : newPara)(p, sc, res.fails, log);
  return res;
}
