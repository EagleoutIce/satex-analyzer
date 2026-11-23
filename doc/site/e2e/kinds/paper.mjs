// Page colours and the echo under overlays, per View > Pages mode and site theme: type into a tinted word (origins
// on) and check from screenshots that the echo's row shows the page's paper with its tint (no white box), the typed
// ink reads on it, the caret and the tint line read on the paper. Pages: one plain, one with \\pagecolor.
// Scenario fields: `theme` ('light' | 'dark'), `modes` (the Pages modes, default all five).
import { inflateSync } from 'node:zlib';
import { readFileSync } from 'node:fs';
import { sleep } from '../lib.mjs';

const J = JSON.stringify;
const SRC = `\\documentclass{article}
\\usepackage{xcolor}
\\newcommand{\\greet}[1]{Hello, #1!}
\\begin{document}
\\greet{loool Das ist doch witzig oder} and then some plain words follow on this row of text.
Some \\textcolor{red}{red words here} and \\colorbox{yellow}{boxed words} close the paragraph.
\\newpage
\\pagecolor{blue!15}
On this coloured page \\greet{Walter wanders} along with a few more words.
\\end{document}
`;
/** A PNG (8-bit RGB or RGBA, not interlaced) as {w, h, at(x, y) -> [r, g, b]}. */
function png(file) {
  const b = readFileSync(file), w = b.readUInt32BE(16), h = b.readUInt32BE(20), ch = b[25] === 6 ? 4 : 3, idat = [];
  for (let o = 8; o < b.length;) { const n = b.readUInt32BE(o), t = b.toString('latin1', o + 4, o + 8); if (t === 'IDAT') idat.push(b.subarray(o + 8, o + 8 + n)); o += 12 + n; }
  const raw = inflateSync(Buffer.concat(idat)), st = w * ch, px = Buffer.alloc(h * st);
  for (let y = 0; y < h; y++) {
    const f = raw[y * (st + 1)], src = raw.subarray(y * (st + 1) + 1, (y + 1) * (st + 1));
    for (let x = 0; x < st; x++) {
      const a = x >= ch ? px[y * st + x - ch] : 0, up = y ? px[(y - 1) * st + x] : 0, ul = y && x >= ch ? px[(y - 1) * st + x - ch] : 0;
      const p = a + up - ul, pa = Math.abs(p - a), pb = Math.abs(p - up), pc = Math.abs(p - ul);
      px[y * st + x] = (src[x] + [0, a, up, (a + up) >> 1, pa <= pb && pa <= pc ? a : pb <= pc ? up : ul][f]) & 255;
    }
  }
  return { w, h, at: (x, y) => { const o = Math.round(y) * st + Math.round(x) * ch; return [px[o], px[o + 1], px[o + 2]]; } };
}
/** Colours in a client rect: dominant (binned by 8, mean) and the one farthest from it with real presence, the
 * overlays' own colours (`skip`: a tint's line, the caret) aside. */
function colours(img, [l, t, w, h], skip = []) {
  const bins = new Map();
  for (let y = Math.ceil(t); y < t + h; y++) for (let x = Math.ceil(l); x < l + w; x++) {
    const c = img.at(x, y), k = c.map((v) => v >> 3).join(), b = bins.get(k) ?? [0, 0, 0, 0];
    b[0]++; b[1] += c[0]; b[2] += c[1]; b[3] += c[2]; bins.set(k, b);
  }
  const all = [...bins.values()].map(([n, r, g, b]) => [n, [r / n, g / n, b / n]]).sort((a, b) => b[0] - a[0]), dom = all[0][1], total = all.reduce((s, [n]) => s + n, 0);
  const far = all.filter(([n, c]) => n >= total * 0.004 && !skip.some((s) => dist(s, c) < 24)).map(([, c]) => c).sort((a, b) => dist(b, dom) - dist(a, dom))[0];
  return { dom, far, share: all[0][0] / total };
}
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
const lum = (c) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); };
const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
const rgbOf = (s) => (/rgba?\(([^)]+)\)/.exec(s)?.[1].split(/[ ,/]+/).filter(Boolean).slice(0, 3).map(Number)) ?? null;
const fmt = (c) => c ? `#${c.map((v) => Math.round(v).toString(16).padStart(2, '0')).join('')}` : '-';


export async function paper(p, sc) {
  const info = {}, rows = [];
  await p.eval(`document.querySelector('[data-example="hello.tex"]').click()`);
  await p.until(`satexHealth().file === 'hello.tex'`, 20000);
  await sleep(400);
  const setText = async () => {
    await p.eval(`document.querySelector('.cm-content').focus()`);
    await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
    await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
    await p.s('Input.insertText', { text: SRC });
    await p.until(`satexHealth().text === ${J(SRC)}`, 5000);
  };
  const compile = async () => {
    await p.eval(`(self.__b0 = document.querySelector('#play-pdf .pdf-page img')?.__build ?? null, document.getElementById('pdf-compile').click())`);
    const ok = await p.until(`(() => { const i = [...document.querySelectorAll('#play-pdf .pdf-page img')]; return i.length >= 2 && i.every((x) => x.naturalWidth > 100 && x.__build && x.__build !== self.__b0) && satexPdfWords().some((w) => w[5] === 'loool') && !document.querySelector('#play-pdf.pdf-running'); })()`, 60000);
    if (!ok) throw new Error('no build');
    await sleep(1200);
  };
  await setText();
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await sleep(200);
  await compile();
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await p.eval(`(() => { const i = document.querySelector('#pdf-layer-list input[data-layer=origins]'); if (i && !i.checked) i.click(); })()`);
  await p.until(`!satexHealth().busy && document.querySelectorAll('#play-pdf .pdf-orig').length > 0`, 60000);
  await sleep(800);
  const word = (t, pg) => p.eval(`(async () => {
    const ws = satexPdfWords(), w = ws.find((w) => w[5] === ${J(t)} && w[0] === ${pg}); if (!w) return null;
    const img = () => document.querySelector('#play-pdf .pdf-page[data-page="${pg}"] img'), sc = document.querySelector('#play-pdf .pdf-pages');
    let r = img().getBoundingClientRect(), k = r.width / (img().naturalWidth * 72 / ws.dpi); const scr = sc.getBoundingClientRect(), y = r.top + w[2] * k;
    if (y < scr.top + 80 || y > scr.bottom - 200) { sc.scrollTop += y - (scr.top + scr.height * 0.4); await new Promise((ok) => setTimeout(ok, 500)); }
    r = img().getBoundingClientRect(); k = r.width / (img().naturalWidth * 72 / ws.dpi);
    return [r.left + w[1] * k, r.top + w[2] * k, r.left + w[3] * k, r.top + w[4] * k, k, r.left, r.width];
  })()`);
  const theme = sc.theme ?? 'light';
  await p.eval(`document.documentElement.dataset.theme = ${J(theme)}`);
  await sleep(200);
  info.defaultMode = await p.eval(`document.getElementById('play-pdf').dataset.pdfMode`);
  let first = true;
  for (const mode of sc.modes ?? ['original', 'invert', 'dark', 'dim', 'sepia']) {
    await p.eval(`(() => { const s = document.getElementById('pdf-mode'); s.value = ${J(mode)}; s.dispatchEvent(new Event('change')); })()`);
    await sleep(300);
    for (const [t, pg] of [['loool', 1], ['Walter', 2]]) {
      if (!first) { await p.key('Escape'); await setText(); await compile(); await p.until(`document.querySelectorAll('#play-pdf .pdf-orig').length > 0`, 30000); await sleep(600); }
      first = false;
      const res = { mode, theme, word: t, fails: [] };
      rows.push(res);
      const w = await word(t, pg);
      if (!w) { res.fails.push('no word'); continue; }
      await p.mouse(w[2] - 0.6 * w[4], (w[1] + w[3]) / 2);
      if (!await p.until(`satexPdfCaret()`, 4000)) { res.fails.push('no caret'); continue; }
      await sleep(400);
      const rowClip = { x: w[5], y: w[1] - 14 * w[4], width: w[6], height: (w[3] - w[1]) + 28 * w[4] };
      const before = await p.shot(`${theme}-${mode}-${t}-before.png`, rowClip);
      for (const k of 'xyz qq') await p.key(k);
      await sleep(250);
      const st = await p.eval(`(() => {
        const pg = document.querySelector('#play-pdf .pdf-page[data-page="${pg}"]'), cv = [...pg.querySelectorAll('.pdf-echo:not(.pdf-push, .pdf-erased)')].at(-1), caret = pg.querySelector('.pdf-caret');
        const rect = (e) => { const r = e.getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; };
        const tint = [...pg.querySelectorAll('.pdf-orig.pdf-relaid')].map(rect), orig = pg.querySelector('.pdf-orig:not(.pdf-relaid)');
        const relaid = [...pg.querySelectorAll('.pdf-relaid')].length, hid = pg.querySelectorAll('.pdf-relay-hid').length;
        // the echo sits under the overlays: every overlay later in the page than it
        const kids = [...pg.children], under = cv ? kids.filter((e) => e.matches('.pdf-orig, .pdf-mark, .pdf-ov')).every((e) => kids.indexOf(e) > kids.indexOf(cv) || getComputedStyle(e).zIndex !== 'auto') : null;
        const lines = [...pg.querySelectorAll('.pdf-orig')].map((e) => getComputedStyle(e).boxShadow);
        return { lines, echo: cv && rect(cv), caret: caret && rect(caret), caretBg: caret && getComputedStyle(caret).backgroundColor, tint, relaid, hid, under, paper: pg.dataset.paper,
          tintLine: orig && getComputedStyle(orig).boxShadow, filter: getComputedStyle(pg.querySelector('img')).filter, texts: cv ? cv.tagName + ' ' + (cv.__texts?.map((q) => q.t).join(' ') ?? cv.textContent) : null };
      })()`);
      res.st = st;
      const after = await p.shot(`${theme}-${mode}-${t}-typed.png`, rowClip);
      if (!st.echo) { res.fails.push('no echo canvas'); continue; }
      const A = png(after), B = png(before), toClip = ([l, t2, w2, h2]) => [l - rowClip.x, t2 - rowClip.y, w2, h2];
      const ce = st.echo, inner = toClip([ce[0] + 1, ce[1] + 1, ce[2] - 2, ce[3] - 2]);
      // the typed ink: the tints' lines and the caret over the echo are no ink (their contrast is checked below)
      const overlays = [...st.lines, st.caretBg].map(rgbOf).filter(Boolean), ca = colours(A, inner, overlays), cb = colours(B, inner);
      res.echoPaper = fmt(ca.dom); res.pagePaper = fmt(cb.dom);
      // no white box: the echo's background is the paper (with its tint) the row showed before
      if (dist(ca.dom, cb.dom) > 18) res.fails.push(`echo paper ${fmt(ca.dom)} differs from the row's before ${fmt(cb.dom)}`);
      // the paper a step outside the row, above it
      const above = colours(A, toClip([ce[0], ce[1] - 8 * w[4], ce[2], 4 * w[4]]));
      res.paper = fmt(above.dom);
      // typed ink reads on its paper
      res.ink = fmt(ca.far); res.inkContrast = +contrast(ca.far, ca.dom).toFixed(2);
      if (res.inkContrast < 3) res.fails.push(`echo ink contrast ${res.inkContrast}`);
      const caretC = rgbOf(st.caretBg);
      res.caretContrast = caretC ? +contrast(caretC, above.dom).toFixed(2) : null;
      if (!(res.caretContrast >= 3)) res.fails.push(`caret contrast ${res.caretContrast} (${st.caretBg} on ${res.paper})`);
      const line = rgbOf(st.tintLine);
      res.tintLineContrast = line ? +contrast(line, above.dom).toFixed(2) : null;
      if (line && res.tintLineContrast < 1.8) res.fails.push(`tint line contrast ${res.tintLineContrast}`);
      // the tint went with the typed text: a relaid tint reaches the caret
      if (t === 'loool' || t === 'Walter') {
        const reach = st.tint.some(([l, , wd]) => st.caret && l <= st.caret[0] && l + wd >= st.caret[0] - 2);
        if (!reach) res.fails.push(`no relaid tint reaches the caret (tints ${J(st.tint.map((r) => r.map(Math.round)))}, caret ${J(st.caret?.map(Math.round))})`);
      }
      if (st.under === false) res.fails.push('an overlay lies under the echo');
      await p.shot(`${theme}-${mode}-${t}-pane.png`);
    }
  }
  const fails = rows.flatMap((r) => r.fails.map((f) => `${r.mode}/${r.word}: ${f}`));
  info.rows = rows.map((r) => ({ mode: r.mode, word: r.word, echoPaper: r.echoPaper, pagePaper: r.pagePaper, ink: r.inkContrast, caret: r.caretContrast }));
  return { name: sc.name, fails, perf: [], info };
}
