// A seeded glitch fuzzer of Type mode in the PDF and Typeset views (doc/site/e2e/README.md): random keys, clicks,
// view switches and waits on a few documents, the invariants checked after every step. A run is its document, its
// first view and its step list: `gen(seed)` makes the list, `run` replays it, `minimize` shrinks a failing one.
// Steps: `w:word` types a word, `k:Key` one key (`Shift+`/`Ctrl+` held), `c:word:f` clicks the end of the
// word at fraction f of the view's words, `c:blank:fx:fy` a point of the first page, `v` switches the view, `wait`
// waits for the build or typeset to land.
import { sleep } from '../lib.mjs';
import { setText } from '../engine.mjs';

const J = JSON.stringify;
export const DOCS = {
  hello: '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\newcommand{\\model}{TinyNet}\n\\begin{document}\n\\greet{world}\n\nWhat do you think about \\model{} here, she asked.\n\nAnother line of plain prose follows it.\n\\end{document}\n',
  thesis: null,
  markup: '\\documentclass{article}\n\\begin{document}\nSome \\textbf{bold words here} and \\emph{gentle emphasis} with $x^2 + y_1$ inline math.\nA display follows\n\\[ a = b + c \\]\nand the prose goes on after it\\footnote{A short note.}.\n\nA second paragraph with \\textbf{more} text.\n\\end{document}\n',
  twocol: '\\documentclass[twocolumn]{article}\n\\begin{document}\n' + Array.from({ length: 6 }, (_, i) => `Paragraph ${'one two three four five six'.split(' ')[i]} talks about columns and rows of text that wrap inside a narrow measure, so every line is short.\n\n`).join('') + '\\end{document}\n',
};
const EXAMPLE = { thesis: 'thesis.tex' };

/** A seeded LCG: the same seed, the same list. */
const lcg = (s) => () => ((s = (s * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);
const SYL = ['ba', 'ko', 'mi', 'zu', 've', 'ra', 'to', 'ni', 'pe', 'lu', 'qa', 'jo'];
/** The step list of `seed`: typed words are unique made-up words, so each one's count on screen is checkable. */
export function gen(seed, n = 30) {
  const r = lcg(seed), pick = (xs) => xs[Math.floor(r() * xs.length)], steps = ['c:word:' + r().toFixed(3)];
  let w = 0;
  for (let i = 1; i < n; i++) {
    const x = r();
    if (x < 0.24) steps.push('w:' + 'xq' + pick(SYL) + pick(SYL) + String.fromCharCode(97 + (w++ % 26)));
    else if (x < 0.34) steps.push('k:' + pick([' ', ' ', ',', '.', ' ']));
    else if (x < 0.39) steps.push('k:Enter');
    else if (x < 0.48) steps.push('k:' + pick(['Backspace', 'Backspace', 'Delete', 'Ctrl+Backspace']));
    else if (x < 0.62) steps.push('k:' + pick(['ArrowLeft', 'ArrowRight', 'ArrowLeft', 'Home', 'End', 'ArrowUp', 'ArrowDown', 'Shift+ArrowLeft', 'Shift+ArrowRight']));
    else if (x < 0.80) steps.push(r() < 0.8 ? 'c:word:' + r().toFixed(3) : `c:blank:${r().toFixed(2)}:${r().toFixed(2)}`);
    else if (x < 0.84) steps.push('v');
    else steps.push('wait');
  }
  return steps;
}

const KEYS = { Backspace: 8, Enter: 13, Escape: 27, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Home: 36, End: 35, Delete: 46, Tab: 9 };
async function key(p, k) {
  let mod = 0;
  for (;;) { const m = /^(Ctrl|Shift)\+(.+)/.exec(k); if (!m) break; mod |= m[1] === 'Ctrl' ? 2 : 8; k = m[2]; }
  if (KEYS[k]) {
    await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: k, code: k, windowsVirtualKeyCode: KEYS[k], modifiers: mod });
    await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code: k, windowsVirtualKeyCode: KEYS[k], modifiers: mod });
  } else {
    await p.s('Input.dispatchKeyEvent', { type: 'keyDown', text: k, key: k, unmodifiedText: k });
    await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: k });
  }
}

// What the view shows now, read in the page: the caret (relative to its page or the Typeset body), the PDF's echo
// draws and uncovered page words, the Typeset rows with their words' font sizes
const SNAP = `self.__fzSnap = (view) => {
  const h = satexHealth(), root = document.getElementById(view === 'pdf' ? 'play-pdf' : 'play-typeset');
  const c = root.querySelector('.pdf-caret'), cr = c && c.checkVisibility() ? c.getBoundingClientRect() : null;
  const host = view === 'pdf' ? c?.closest('.pdf-page') : document.getElementById('typeset-body'), hr = host?.getBoundingClientRect();
  const out = { text: h.text, file: h.file, view, busy: h.busy || h.due || h.running, caret: cr && [cr.left, cr.top, cr.height], rel: cr && hr && [cr.left - hr.left, cr.top - hr.top], at: satexPdfCaret()?.at ?? null };
  const ctx = (self.__fzC ??= document.createElement('canvas').getContext('2d'));
  if (view === 'ts') {
    const rows = [...root.querySelectorAll('p.ts')].filter((r) => r.checkVisibility());
    out.rows = rows.map((r) => { const b = r.getBoundingClientRect(); return { t: (r.querySelector('.ts-text')?.textContent ?? '').replace(/\\u200b/g, ''), y: [b.top, b.bottom] }; });
    // a word's letters in more than one font size (math aside)
    out.sizes = [];
    for (const t of root.querySelectorAll('p.ts .ts-text')) {
      const w = document.createTreeWalker(t, NodeFilter.SHOW_TEXT); let word = [], sz = new Set();
      const end = () => { if (sz.size > 1 && word.join('').length > 1) out.sizes.push(word.join('') + ' ' + [...sz].join('/')); word = []; sz = new Set(); };
      for (let n = w.nextNode(); n; n = w.nextNode()) {
        const el = n.parentElement; if (!el.checkVisibility() || el.closest('[class*=math], sub, sup, .ts-math')) { end(); continue; }
        const fs = getComputedStyle(el).fontSize;
        for (const ch of n.data.replace(/\\u200b/g, '')) { if (/\\s/.test(ch)) end(); else { word.push(ch); if (/\\p{L}/u.test(ch)) sz.add(fs); } }
      }
      end();
    }
  } else {
    const cans = [...root.querySelectorAll('.pdf-echo:not(.pdf-push, .pdf-erased)')];
    out.draws = []; out.boxes = [];
    for (const e of cans) {
      const r = e.getBoundingClientRect(), k = r.width / Math.max(1, e.width);
      if (r.width * r.height > 1 && e.checkVisibility()) out.boxes.push([+e.closest('.pdf-page')?.dataset.page, r.left, r.top, r.right, r.bottom]);
      for (const q of e.__texts ?? []) { ctx.font = q.font; out.draws.push({ t: q.t, font: q.font, sy: q.sy, x: r.left + q.x * k, y: r.top + q.y * k, k, mixed: (e.__mixed ?? []).includes(q.t), px: parseFloat(/([\\d.]+)px/.exec(q.font)?.[1] ?? '0') * k * (q.sy ?? 1) }); }
    }
    const ws = satexPdfWords(); out.words = [];
    out.build = [...root.querySelectorAll('.pdf-page img')].map((i) => i.__build ? (i.__build.__fz ??= ++self.__fzN) : 0).join(',');
    for (const w of ws) {
      const img = root.querySelector('.pdf-page[data-page="' + w[0] + '"] img'); if (!img?.naturalWidth) continue;
      const ir = img.getBoundingClientRect(), k = ir.width / (img.naturalWidth * 72 / ws.dpi), b = [ir.left + w[1] * k, ir.top + w[2] * k, ir.left + w[3] * k, ir.top + w[4] * k];
      const covered = out.boxes.some((c) => c[0] === w[0] && b[0] >= c[1] - 1 && b[2] <= c[3] + 1 && (b[1] + b[3]) / 2 >= c[2] && (b[1] + b[3]) / 2 <= c[4]);
      out.words.push({ t: w[5], b, covered });
    }
  }
  return out;
};
/** The client x of the end of the last occurrence of 'tok' on the caret's row, and whether any is on that row. */
self.__fzEnd = (view, tok, mid, h) => {
  const root = document.getElementById(view === 'pdf' ? 'play-pdf' : 'play-typeset');
  if (view === 'ts') {
    let best = null;
    for (const t of root.querySelectorAll('p.ts .ts-text')) { const w = document.createTreeWalker(t, NodeFilter.SHOW_TEXT); for (let n = w.nextNode(); n; n = w.nextNode()) { let i = -1; while ((i = n.data.indexOf(tok, i + 1)) >= 0) { const g = document.createRange(); g.setStart(n, i + tok.length - 1); g.setEnd(n, i + tok.length); const b = g.getBoundingClientRect(); if (Math.abs(b.top + b.height / 2 - mid) < h) best = b.right; } } }
    return best;
  }
  const ctx = (self.__fzC ??= document.createElement('canvas').getContext('2d'));
  let best = null;
  for (const e of root.querySelectorAll('.pdf-echo:not(.pdf-push, .pdf-erased)')) { const r = e.getBoundingClientRect(), k = r.width / Math.max(1, e.width);
    for (const q of e.__texts ?? []) { const i = q.t.lastIndexOf(tok); if (i < 0) continue; const y = r.top + q.y * k; if (Math.abs(y - h * 0.3 - mid) > h) continue; ctx.font = q.font; best = r.left + q.x * k + ctx.measureText(q.t.slice(0, i + tok.length)).width * k * (q.sy ?? 1); } }
  return best;
};`;

const norm = (s) => s.replace(/[\s\u00ad\-]+/g, '');
const tokens = (s) => s.split(/[\s,.;:!?()[\]{}"'`~]+/).filter(Boolean);
const count = (hay, needle) => hay.split(needle).length - 1;
/** What the view shows as one text: Typeset rows, or the PDF's echo draws and the page words they do not cover. */
const shown = (s) => s.view === 'ts' ? s.rows.map((r) => r.t).join('\n') : [...s.draws.map((d) => d.t), ...s.words.filter((w) => !w.covered).map((w) => w.t)].join(' ');
/** Words only the markup spells: command and environment names the first render does not show. */
const markupWords = (src, base) => new Set([...src.matchAll(/\\([a-zA-Z]{4,})|\\begin\{(\w{4,})\}/g)].map((m) => m[1] ?? m[2]).filter((w) => !base.has(w)));

async function open(p, doc, view) {
  await p.eval('self.__e2eMixed = true');
  const ex = EXAMPLE[doc] ?? 'hello.tex';
  await p.eval(`document.querySelector('[data-example=${J(ex)}]').click()`);
  await p.until(`satexHealth().file === ${J(ex)} && satexHealth().text?.length > 10`, 20000);
  await sleep(400);
  if (DOCS[doc]) await setText(p, DOCS[doc]);
  await p.eval(SNAP);
  await p.eval('self.__fzN = 0');
  await toView(p, 'pdf', true);
  if (view === 'ts') await toView(p, 'ts');
}
async function toView(p, view, compile = false) {
  await p.eval(`document.querySelector('.side-tabs [data-panel=${view === 'pdf' ? 'play-pdf' : 'play-typeset'}]').click()`);
  await sleep(300);
  if (view === 'pdf') {
    if (compile) await p.eval(`document.getElementById('pdf-compile').click()`);
    await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return satexPdfWords().length > 3 && i?.naturalWidth > 100 && i.__build && !document.querySelector('#play-pdf.pdf-running'); })()`, 90000);
  } else await p.until(`document.querySelector('#typeset-body p.ts .ts-text') && !satexHealth().busy`, 60000);
  await sleep(500);
  const btn = view === 'pdf' ? 'pdf-type' : 'ts-type';
  if (await p.eval(`document.getElementById(${J(btn)}).getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById(${J(btn)}).click()`);
  await sleep(250);
}
/** Landed: nothing compiles, analyses or is due, no echo drawn, for 1 s. */
const landed = (p, view) => p.until(`(() => { const h = satexHealth(), busy = h.busy || h.due || h.running || h.queued || ${view === 'pdf'} && (!!document.querySelector('#play-pdf.pdf-running') || !!document.querySelector('#play-pdf .pdf-echo:not(.pdf-push)'));
  if (busy) { self.__fzQ = null; return false; } self.__fzQ ??= performance.now(); return performance.now() - self.__fzQ > 1000; })()`, 40000);

async function click(p, view, step) {
  const [, what, a, b] = step.split(':');
  const at = await p.eval(view === 'pdf'
    ? `(async () => { const ws = satexPdfWords(); if (!ws.length) return null; const sc = document.querySelector('#play-pdf .pdf-pages');
      if (${J(what)} === 'blank') { const img = document.querySelector('#play-pdf .pdf-page img'); img.scrollIntoView({ block: 'start' }); await new Promise((ok) => setTimeout(ok, 300)); const r = img.getBoundingClientRect(); return [r.left + r.width * ${+a || 0}, r.top + Math.min(r.height, innerHeight - r.top - 10) * ${+b || 0}]; }
      const w = ws[Math.min(ws.length - 1, Math.floor(${+a || 0} * ws.length))];
      let img = document.querySelector('#play-pdf .pdf-page[data-page="' + w[0] + '"] img'), r = img.getBoundingClientRect(), k = r.width / (img.naturalWidth * 72 / ws.dpi);
      const y = r.top + w[2] * k, s = sc.getBoundingClientRect(); if (y < s.top + 80 || y > s.bottom - 120) { sc.scrollTop += y - (s.top + s.height * 0.4); await new Promise((ok) => setTimeout(ok, 400)); }
      img = document.querySelector('#play-pdf .pdf-page[data-page="' + w[0] + '"] img'); r = img.getBoundingClientRect(); k = r.width / (img.naturalWidth * 72 / ws.dpi);
      return [r.left + w[3] * k - 0.6 * k, r.top + (w[2] + w[4]) / 2 * k, w[5]]; })()`
    : `(async () => { const body = document.getElementById('typeset-body');
      if (${J(what)} === 'blank') { const r = body.getBoundingClientRect(); return [r.left + r.width * ${+a || 0}, r.top + Math.min(r.height, innerHeight - r.top - 10) * ${+b || 0}]; }
      const ws = []; for (const t of body.querySelectorAll('p.ts .ts-text')) { if (!t.checkVisibility()) continue; const tw = document.createTreeWalker(t, NodeFilter.SHOW_TEXT); for (let n = tw.nextNode(); n; n = tw.nextNode()) for (const m of n.data.matchAll(/\\S+/g)) ws.push([n, m.index + m[0].length, m[0]]); }
      if (!ws.length) return null; const [n, e, t] = ws[Math.min(ws.length - 1, Math.floor(${+a || 0} * ws.length))];
      n.parentElement.scrollIntoView({ block: 'nearest' }); await new Promise((ok) => setTimeout(ok, 200));
      const g = document.createRange(); g.setStart(n, e - 1); g.setEnd(n, e); const b = g.getBoundingClientRect(); return [b.right - 1, b.top + b.height / 2, t]; })()`);
  if (!at) return null;
  await p.mouse(at[0], at[1]);
  return at[2] ?? 'blank';
}

/** Replays `steps` on `doc` from `view`; resolves to { fails: [{ step, cls, msg }], log }. Stops at the first failing step. */
export async function run(p, { doc, view, steps }) {
  const fails = [], log = [];
  await open(p, doc, view);
  const typed = new Set(), seenText = new Set();
  let s = await p.eval(`__fzSnap(${J(view)})`);
  for (const t of tokens(shown(s))) seenText.add(t);
  const mk = markupWords(s.text ?? '', seenText);
  let errs = 0;
  const fail = (i, cls, msg) => fails.push({ step: i, cls, msg: msg.slice(0, 400) });
  for (let i = 0; i < steps.length && !fails.length; i++) {
    const st = steps[i], before = s;
    const kind = st.startsWith('w:') ? 'type' : st.startsWith('k:') ? (/^k:(.|Enter|Backspace|Delete|Ctrl\+Backspace)$/.test(st) ? 'edit' : 'move') : st.startsWith('c:') ? 'click' : st;
    let clicked = null;
    if (kind === 'type') { for (const ch of st.slice(2)) { await key(p, ch); await sleep(35); } typed.add(st.slice(2)); }
    else if (kind === 'edit' || kind === 'move') await key(p, st.slice(2));
    else if (kind === 'click') { clicked = await click(p, view, st); await p.until(`satexPdfCaret()`, 3000); }
    else if (kind === 'v') { view = view === 'pdf' ? 'ts' : 'pdf'; await toView(p, view); }
    else if (kind === 'wait') {
      await landed(p, view);
      await sleep(400);
    }
    await sleep(kind === 'wait' ? 0 : 180);
    s = await p.eval(`__fzSnap(${J(view)})`);
    log.push(`${i} ${st}${clicked ? ' on ' + J(clicked) : ''} -> at ${s.at} ${s.file}`);
    // (7) no page errors
    if (p.errors.length > errs) { fail(i, 'console', p.errors.slice(errs).join(' | ')); errs = p.errors.length; continue; }
    const text = shown(s), toks = tokens(text);
    // (2) a key or click that edits nothing changes neither the source nor what is shown
    if ((kind === 'move' || kind === 'click' || kind === 'v' || kind === 'wait') && before.file === s.file && before.text !== s.text && !/^k:Shift/.test(st))
      fail(i, 'nonedit-source', `the source changed: ${J(diffAt(before.text, s.text))}`);
    if ((kind === 'move') && before.view === s.view && (s.view === 'ts' || before.build === s.build) && norm(shown(before)) !== norm(text))
      fail(i, 'nonedit-shown', `what is shown changed: ${J(diffAt(norm(shown(before)), norm(text)))}`);
    // (1) no typed word shown more often than the source has it; no word the markup alone spells
    const src = s.text ?? '';
    for (const t of typed) { const n = count(src, t), m = count(text, t); if (m > n) fail(i, 'duplicate', `${J(t)} shown ${m}x, ${n}x in the source: ${J(text.slice(Math.max(0, text.indexOf(t) - 60), text.indexOf(t) + 80))}`); }
    for (const t of toks) if (mk.has(t) && !src.includes(t + ' ') && ![...typed].some((y) => y.includes(t))) fail(i, 'markup', `${J(t)} drawn, a word only the markup spells`);
    if (s.view === 'pdf') {
      const page = new Set(s.words.map((w) => w.t));
      for (const d of s.draws) for (const t of tokens(d.t)) {
        const known = (x) => page.has(x) || seenText.has(x) || [...typed, ...page, ...seenText].some((y) => y.includes(x)) || [...Array(x.length - 1).keys()].some((j) => [x.slice(0, j + 1), x.slice(j + 1)].every((h) => [...typed, ...page, ...seenText].some((y) => y.includes(h))));
        if (t.length > 2 && !known(t)) fail(i, 'echo-foreign', `echo draws ${J(t)}, neither typed nor on the page`);
      }
      // (4) one size a row's letters, no word in a face lacking its glyphs
      const rows = Map.groupBy(s.draws.filter((d) => /\p{L}{2}/u.test(d.t)), (d) => Math.round(d.y / 3));
      for (const [, ds] of rows) { const px = [...new Set(ds.map((d) => d.px.toFixed(1)))]; if (px.length > 1 && Math.max(...px) / Math.min(...px) > 1.08) fail(i, 'mixed-size', `one row in sizes ${px.join('/')}: ${J(ds.map((d) => d.t))}`); }
      const mixed = s.draws.filter((d) => d.mixed); if (mixed.length) fail(i, 'mixed-font', `drawn in a face lacking a glyph: ${J(mixed.map((d) => d.t))}`);
      // (5) echo rows never over each other
      for (const a of s.boxes) for (const b of s.boxes) if (a !== b && a[0] === b[0] && a[2] < b[2] && b[2] < a[4] - 1.5 && a[1] < b[3] - 1 && b[1] < a[3] - 1) fail(i, 'overlap', `echo rows overlap: ${J([a, b].map((x) => x.map(Math.round)))}`);
    } else if (s.sizes.length) fail(i, 'mixed-size', `a word in two sizes: ${J(s.sizes.slice(0, 3))}`);
    // (3) after typing a word the caret stands at its last glyph, on its row
    if (kind === 'type' && s.caret) {
      const tok = st.slice(2), mid = s.caret[1] + s.caret[2] / 2;
      if (!src.includes(tok)) fail(i, 'lost-key', `${J(tok)} typed, not in the source at ${s.at}`);
      else {
        const x = await p.eval(`__fzEnd(${J(s.view)}, ${J(tok)}, ${mid}, ${s.caret[2]})`);
        if (x == null && !(s.view === 'pdf' && before.build !== s.build)) fail(i, 'wrong-row', `${J(tok)} not drawn on the caret's row (caret ${J(s.caret.map(Math.round))}): ${J(text.slice(0, 200))}`);
        else if (x != null && Math.abs(x - s.caret[0]) > 3) fail(i, 'caret-glyph', `caret at x ${Math.round(s.caret[0])}, ${J(tok)} ends at ${Math.round(x)}`);
      }
    }
    if ((kind === 'type' || kind === 'edit') && !s.caret) fail(i, 'caret-lost', 'no caret after an editing key');
    // (6) a landing moves the caret by at most 3 px, keeps the Typeset rows' text
    if (kind === 'wait' && before.rel && s.rel && before.text === s.text && before.view === s.view && (Math.abs(before.rel[0] - s.rel[0]) > 3 || Math.abs(before.rel[1] - s.rel[1]) > 3))
      fail(i, 'land-jump', `the caret moved ${J(before.rel.map(Math.round))} -> ${J(s.rel.map(Math.round))} as the build landed`);
    if (kind === 'wait') for (const t of typed) { const n = count(src, t); if (n && count(text, t) < n && !inMarkup(src, t)) fail(i, 'lost-word', `${J(t)} ${n}x in the source, ${count(text, t)}x shown after landing`); }
    for (const t of toks) seenText.add(t);
  }
  return { fails, log, view };
}
/** Whether every occurrence of `t` in `src` sits in math or a command name: places the shown text spells otherwise. */
const inMarkup = (src, t) => [...src.matchAll(new RegExp(t, 'g'))].every((m) => { const pre = src.slice(0, m.index); return /\\[a-zA-Z]*$/.test(pre) || (pre.split('$').length - 1) % 2 === 1 || pre.lastIndexOf('\\[') > pre.lastIndexOf('\\]') || /\\begin\{(equation|align|tikzpicture|lstlisting)\}(?![^]*\\end\{\1\})/.test(pre); });
const diffAt = (a = '', b = '') => { let i = 0; while (i < a.length && a[i] === b[i]) i++; return [a.slice(Math.max(0, i - 20), i + 20), b.slice(Math.max(0, i - 20), i + 20)]; };

/** Delta debugging: the shortest step list (within `budget` replays) still failing in class `cls`. */
export async function minimize(fresh, sc, cls, budget = 10) {
  let steps = sc.steps, n = 2;
  const fails = async (xs) => { if (budget-- <= 0) return false; const p = await fresh(); try { const r = await run(p, { ...sc, steps: xs }); return r.fails.some((f) => f.cls === cls); } catch { return false; } finally { await p.close(); } };
  while (steps.length > 1 && budget > 0) {
    const size = Math.ceil(steps.length / n);
    let cut = false;
    for (let i = 0; i < steps.length && budget > 0; i += size) {
      const rest = [...steps.slice(0, i), ...steps.slice(i + size)];
      // the first click places the caret: a list without one types nowhere
      if (rest.length && rest[0].startsWith('c:') && await fails(rest)) { steps = rest; n = Math.max(n - 1, 2); cut = true; break; }
    }
    if (!cut) { if (n >= steps.length) break; n = Math.min(steps.length, n * 2); }
  }
  return steps;
}

/** The `fuzz` scenario kind: a saved step list replayed, its invariants as failures. */
export async function fuzz(p, sc) {
  const r = await run(p, sc);
  const res = { name: sc.name, kind: 'fuzz', fails: r.fails.map((f) => `step ${f.step} ${sc.steps[f.step]} [${f.cls}] ${f.msg}`), perf: [], info: { log: r.log.slice(-6) } };
  if (res.fails.length) res.shot = await p.shot(`${sc.name}-fuzz.png`);
  return res;
}
