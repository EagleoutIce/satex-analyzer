// The two context menus, the editor's (#play-ctx) and the pages' (.pa-menu, actions.ts): a right click opens each within
// a moment, also while an analysis runs (the editor's once waited for the worker's widget and rename answers behind the
// run, and showed nothing until it ended), and both look the same: header, row, submenu row and hint computed alike.
// A selection's menu starts with the format bar in both, B pressed inside \textbf. Field: none.
import { sleep } from '../lib.mjs';
import { build } from './tikz.mjs';

/** Example `example` opened, built by the machine's compile server (tikz.mjs `open` blocks it for the browser's pdfTeX). */
async function open(p, example) {
  if (!await p.drv.until({ sel: '#play-status[data-done=ready]' }, 120000)) throw new Error('the playground did not start');
  await p.drv.act({ press: { sel: `[data-example="${example}"]` } });
  await p.drv.until([{ health: { example } }, { srcMatch: '^[^]{11}' }], 30000);
}

const view = `document.querySelector('#play-editor .cm-editor').satexView`;
const KEYS = ['font-size', 'font-weight', 'padding-top', 'padding-left', 'padding-right', 'border-radius', 'color'];
const pick = `const pk = (el) => { if (!el) return null; const s = getComputedStyle(el); return ${JSON.stringify(KEYS)}.map((k) => k + '=' + s[k]).join(';'); };`;
async function right(p, x, y) {
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  for (const type of ['mousePressed', 'mouseReleased']) await p.s('Input.dispatchMouseEvent', { type, x, y, button: 'right', clickCount: 1 });
}
/** The editor's menu on `\section` while a run goes on (an edit just started one): open within `ms`, its parts' styles. */
async function editorMenu(p, fails, ms, word = null) {
  await p.eval(`(() => { const v = ${view}; v.dispatch({ changes: { from: v.state.doc.length, insert: '\\n%' } }); })()`);
  await sleep(300);
  const c = await p.eval(`(() => { const v = ${view}, t = v.state.doc.toString(), w = ${JSON.stringify(word)}, at = w ? t.indexOf(w) : t.indexOf('\\\\section') + 3;
    v.dispatch({ selection: w ? { anchor: at, head: at + w.length } : { anchor: at }, scrollIntoView: true }); const r = v.coordsAtPos(at + 2); return r && [r.left + 1, (r.top + r.bottom) / 2]; })()`);
  if (!c) { fails.push('no \\section on screen'); return null; }
  // a run going on: the right click lands while the worker is busy
  await p.until(`self.satexHealth().running > 0`, 3000);
  const running = await p.eval(`self.satexHealth().running > 0`), t0 = Date.now();
  await right(p, ...c);
  const shown = await p.until(`!document.getElementById('play-ctx').hidden`, ms);
  (p.menuLog ??= []).push({ running, ms: Date.now() - t0, status: await p.eval(`self.satexHealth().status.slice(0, 50)`) });
  if (!shown) { fails.push(`no editor menu within ${ms} ms of the right click (a run going on: ${running})`); return null; }
  await sleep(250);
  return p.eval(`(() => { ${pick} const m = document.getElementById('play-ctx'); return { bar: [...m.querySelectorAll('.pa-bar .pa-tog:not(.pa-col)')].map((b) => b.textContent + (b.getAttribute('aria-pressed') === 'true' ? '+' : '-')).join('') + [...m.querySelectorAll('.pa-bar .pa-dd')].map((s) => ' ' + s.dataset.value).join(''), picked: (() => { const v = ${view}, r = v.state.selection.main; return v.state.sliceDoc(r.from, r.to); })(), head: pk(m.querySelector(':scope > .ctx-head')), row: pk(m.querySelector(':scope > li[role=menuitem]:not(.ctx-sub):not(.ctx-wait)')), sub: pk(m.querySelector(':scope > li.ctx-sub')), hint: pk(m.querySelector(':scope > li > .muted')) }; })()`);
}
/** The pages' menu on page word `w`, selected first by a double click when `select`. */
async function pageMenu(p, fails, ms, w, select = false) {
  if (!await p.drv.until({ word: w, nth: 0 }, 20000)) { fails.push(`no page word ${w}`); return null; }
  const bx = await p.drv.word({ word: w, nth: 0, scroll: [120, 200] }), c = bx && [(bx[0] + bx[2]) / 2, (bx[1] + bx[3]) / 2];
  if (!c) { fails.push(`page word ${w} not on screen`); return null; }
  if (select) { await p.mouse(...c, 1); await sleep(80); await p.mouse(...c, 2); await sleep(400); }
  await right(p, ...c);
  if (!await p.until(`!!document.querySelector('.pa-root')`, ms)) { fails.push(`no page menu within ${ms} ms`); return null; }
  await sleep(250);
  return p.eval(`(() => { ${pick} const m = document.querySelector('.pa-root'); return { bar: [...m.querySelectorAll('.pa-bar .pa-tog:not(.pa-col)')].map((b) => b.textContent + (b.getAttribute('aria-pressed') === 'true' ? '+' : '-')).join('') + [...m.querySelectorAll('.pa-bar .pa-dd')].map((s) => ' ' + s.dataset.value).join(''), picked: (() => { const v = ${view}, r = v.state.selection.main; return v.state.sliceDoc(r.from, r.to); })(), head: pk(m.querySelector(':scope > b')), row: pk(m.querySelector(':scope > button:not([aria-haspopup]):not(.pa-wait)')), sub: pk(m.querySelector(':scope > button[aria-haspopup]')), hint: pk(m.querySelector(':scope small, :scope .pa-more')) }; })()`);
}

const DOC = '\\documentclass{article}\n\\begin{document}\n\\section{Start}\nPlain words and \\textbf{boldword here} end.\n\\end{document}\n';
/** The open menu's bar dropdowns and color button, and its Color ▸ submenu's swatches (the row hovered). */
async function barAndColors(p, rowSel, subSel) {
  const bar = await p.eval(`(() => { const s = [...document.querySelectorAll('.pa-bar .pa-dd')].map((x) => x.dataset.labels.split('\\n')); return { size: s[0], family: s[1], color: !!document.querySelector('.pa-bar .pa-col') }; })()`);
  const at = await p.eval(`(() => { const r = [...document.querySelectorAll(${JSON.stringify(rowSel)})].find((x) => x.textContent.trim().replace('▸', '') === 'Color')?.getBoundingClientRect(); return r && [r.left + 20, r.top + r.height / 2]; })()`);
  if (at) { await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] }); await sleep(300); }
  bar.swatches = await p.eval(`document.querySelectorAll(${JSON.stringify(subSel)}).length`);
  return bar;
}
/** A table whose line has a finding (array's column code without array): its widget is reached from the gutter. */
const TABLE = '\\documentclass{article}\n\\begin{document}\n\\begin{tabular}{l>{\\centering\\arraybackslash}p{.45\\linewidth}}\nA & B \\\\\n\\end{tabular}\n\\end{document}\n';
async function tableFromGutter(p, fails, info) {
  await p.eval(`(() => { const v = ${view}; v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: ${JSON.stringify(TABLE)} } }); })()`);
  await p.drv.until({ findingsCurrent: true }, 60000);
  await sleep(500);
  const g = await p.eval(`(() => { const v = ${view}, top = v.coordsAtPos(v.state.doc.line(3).from)?.top; if (top == null) return null;
    const at = (sel) => [...document.querySelectorAll(sel)].find((m) => Math.abs(m.getBoundingClientRect().top + m.getBoundingClientRect().height / 2 - top - 9) < 9);
    const m = at('.cm-lint-marker') ?? at('.cm-wmark'); if (!m) return { none: true }; const r = m.getBoundingClientRect(); return { kind: m.className, x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
  info.gutter = g;
  if (!g || g.none) { fails.push('no gutter mark on the table line'); return; }
  await p.mouse(g.x, g.y);
  const ok = await p.until(`!!document.querySelector('dialog[open]') || [...document.querySelectorAll('#play-ctx:not([hidden]) li')].some((l) => /Edit table/.test(l.textContent))`, 8000);
  if (process.env.MENU_SHOTS) await p.shot(`menu-gutter-${process.env.MENU_SHOTS}.png`);
  if (!ok) fails.push(`the table line's gutter mark (${g.kind}) offers no Edit table… (menu: ${await p.eval(`document.getElementById('play-ctx').hidden ? 'none' : document.getElementById('play-ctx').textContent.slice(0, 120)`)})`);
  await p.key('Escape').catch(() => {});
}
/** Theorems: a gutter mark on each instance's \\begin opens the theorem manager on that instance; a reference's completion
 * ends with "Browse all labels…", which opens the labels list, and a label's Insert writes it into the reference. */
const THEOREMS = '\\documentclass{article}\n\\usepackage{amsthm}\n\\newtheorem{theorem}{Theorem}[section]\n\\begin{document}\n\\section{A}\n\\begin{theorem}\\label{thm:a}\nX.\n\\end{theorem}\nSee \\ref{}.\n\\end{document}\n';
async function theorems(p, fails, info) {
  await p.eval(`(() => { const v = ${view}; v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: ${JSON.stringify(THEOREMS)} } }); })()`);
  await p.drv.until({ findingsCurrent: true }, 60000);
  await sleep(500);
  const g = await p.eval(`(() => { const v = ${view}, top = v.coordsAtPos(v.state.doc.line(6).from)?.top; if (top == null) return null;
    const m = [...document.querySelectorAll('.cm-wmark')].find((m) => Math.abs(m.getBoundingClientRect().top + m.getBoundingClientRect().height / 2 - top - 9) < 9);
    if (!m) return { none: true }; const r = m.getBoundingClientRect(); return { title: m.title, x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
  info.theoremGutter = g;
  if (!g || g.none) { fails.push('no gutter mark on the \\begin{theorem} line'); return; }
  await p.mouse(g.x, g.y);
  if (!await p.until(`/Theorem-like environments/.test(document.querySelector('dialog[open]')?.textContent ?? '')`, 8000)) fails.push(`the \\begin{theorem} gutter mark (${g.title}) opens no theorem manager`);
  else if (!await p.until(`[...document.querySelectorAll('dialog[open] tr.on')].some((r) => /Theorem 1\\.1/.test(r.textContent))`, 4000)) fails.push('the theorem manager does not select instance Theorem 1.1');
  if (process.env.MENU_SHOTS) await p.shot(`theorem-gutter-${process.env.MENU_SHOTS}.png`);
  await p.key('Escape').catch(() => {});
  // the reference: a letter typed in its braces completes, the last item opens the list
  await p.eval(`(() => { const v = ${view}, at = v.state.doc.toString().indexOf('\\\\ref{}') + 5; v.focus(); v.dispatch({ selection: { anchor: at } }); })()`);
  await p.key('t');
  if (!await p.until(`[...document.querySelectorAll('.cm-tooltip-autocomplete li')].some((l) => /Browse all labels/.test(l.textContent))`, 8000)) { fails.push('no "Browse all labels…" in a reference\'s completion'); return; }
  await p.eval(`[...document.querySelectorAll('.cm-tooltip-autocomplete li')].find((l) => /Browse all labels/.test(l.textContent)).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))`);
  if (!await p.until(`!!document.querySelector('dialog[open] .tv-actions .tv-primary')`, 8000)) { fails.push('"Browse all labels…" opens no labels list with Insert'); return; }
  if (process.env.MENU_SHOTS) await p.shot(`labels-browse-${process.env.MENU_SHOTS}.png`);
  await p.eval(`document.querySelector('dialog[open] .tv-actions .tv-primary').click()`);
  if (!await p.until(`${view}.state.doc.toString().includes('\\\\ref{thm:a}')`, 4000)) fails.push('Insert wrote no thm:a into \\ref{}');
}
export async function menus(p, sc) {
  const fails = [], info = {};
  if (process.env.MENU_SHOTS === 'dark') await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
  await open(p, 'thesis.tex');
  const ed = await editorMenu(p, fails, 400);
  await p.key('Escape').catch(() => {});
  // a small document of its own: a word in \textbf, selected in the editor and on the page
  await p.eval(`(() => { const v = ${view}; v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: ${JSON.stringify(DOC)} } }); })()`);
  const sel = await editorMenu(p, fails, 400, 'boldword');
  const ec = await barAndColors(p, '#play-ctx > li', '#play-ctx .ctx-nested .pa-sw');
  if (process.env.MENU_SHOTS) await p.shot(`menu-editor-color-${process.env.MENU_SHOTS}.png`);
  if (process.env.MENU_SHOTS) await p.shot(`menu-editor-${process.env.MENU_SHOTS}.png`);
  await p.key('Escape').catch(() => {});
  if (sel?.bar !== 'B+I-E-T- normalsize rm') fails.push(`the editor's selection menu: format bar ${JSON.stringify(sel?.bar)}, want B on, normalsize, Roman`);
  const b = await build(p, 300000);
  if (!b.ok) fails.push(`no build: ${b.status.slice(0, 120)}`);
  const pg = await pageMenu(p, fails, 1500, 'Plain');
  await p.key('Escape').catch(() => {});
  const ps = await pageMenu(p, fails, 1500, 'boldword', true);
  const pc = await barAndColors(p, '.pa-root > button', '.pa-sub .pa-sw');
  if (process.env.MENU_SHOTS) await p.shot(`menu-page-color-${process.env.MENU_SHOTS}.png`);
  if (process.env.MENU_SHOTS) await p.shot(`menu-page-${process.env.MENU_SHOTS}.png`);
  await p.key('Escape').catch(() => {});
  if (ps?.bar !== 'B+I-E-T- normalsize rm') fails.push(`the page's selection menu: format bar ${JSON.stringify(ps?.bar)}, want B on, normalsize, Roman`);
  info.colors = { editor: ec, page: pc };
  // one color picker: the editor's Color ▸ draws the same swatches as the page's
  if (!(ec.swatches > 5) || ec.swatches !== pc.swatches) fails.push(`Color ▸ swatches: editor ${ec.swatches}, page ${pc.swatches}`);
  // the bar: sizes in pt with Custom…, families with their face and Custom…, a color button; both menus alike
  for (const [n, c] of [['editor', ec], ['page', pc]]) {
    if (!/^small · 9 pt$/.test(c.size?.[3] ?? '') || c.size?.at(-1) !== 'Custom…') fails.push(`${n} bar sizes: ${JSON.stringify(c.size)}`);
    if (!/^Roman · Computer Modern$/.test(c.family?.[0] ?? '') || c.family?.at(-1) !== 'Custom…') fails.push(`${n} bar families: ${JSON.stringify(c.family)}`);
    if (!c.color) fails.push(`${n} bar: no color button`);
  }
  await tableFromGutter(p, fails, info);
  await theorems(p, fails, info);
  info.log = p.menuLog; info.editor = ed; info.page = pg; info.editorSel = sel; info.pageSel = ps;
  // the same look: each part's computed style equal where both menus show it
  if (ed && pg) for (const k of ['head', 'row', 'sub', 'hint']) if (ed[k] && pg[k] && ed[k] !== pg[k]) fails.push(`${k}: editor ${ed[k]} ≠ page ${pg[k]}`);
  return { name: sc.name, fails, perf: [], info };
}
