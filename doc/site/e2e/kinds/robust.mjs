// Widgets on edge cases, as a person uses them, on the browser's pdfTeX (the compile server blocked). `check`:
// `insert`: `text`, the caret after `caret` (+`k`), the PDF bar's Insert row `item`: the text then matches `want`,
//   builds with no more errors than before, one Ctrl+Z gives it back.
// `table`: `text`, the table dialog at `at` (+`k`): Apply untouched writes nothing (or the same text); the cell
//   holding `cell` set to `to` and applied changes only that cell (`want` the text after).
// `big`: `text` of many pages (`pages` at least): built, the last page's word `word` clicked in Type mode, `keys`
//   typed: they land in the source (`contains`) within `ms`.
// `screen`: `width` x `height`, `theme`, widget `widget` at `at` opened: no page-wide horizontal scroll, the dialog
//   or view inside the screen, a screenshot each.
import { sleep } from '../lib.mjs';
import { setText, keysOf } from '../engine.mjs';
import { open, build, word, undo, text, written } from './tikz.mjs';

const J = JSON.stringify;
const errs = (st) => +(/(\d+) errors?/.exec(st)?.[1] ?? 0);
const view = `document.querySelector('#play-editor .cm-editor').satexView`;
const quiet = (p, ms = 240000) => p.until(`(() => { const h = satexHealth(); if (h.busy || h.due || h.running || h.queued || h.pending || h.sent !== h.shown) { self.__q = null; return false; } self.__q ??= performance.now(); return performance.now() - self.__q > 500; })()`, ms);
const caretAt = (p, at) => p.eval(`(() => { const v = ${view}; v.dispatch({ selection: { anchor: ${at} } }); v.focus(); })()`);
const shown = `document.querySelector('dialog.sd-dialog[open]') ?? (document.getElementById('play-view')?.open ? document.getElementById('play-view') : null)`;

async function start(p, sc) {
  await open(p, 'hello.tex');
  await setText(p, sc.text);
  if (!await quiet(p)) throw new Error('the first run never ended');
}

async function insert(p, sc, res) {
  await start(p, sc);
  const first = await build(p, 300000);
  if (!first.ok) throw new Error(`no first build (${first.status.slice(0, 120)})`);
  const t0 = await text(p), at = t0.indexOf(sc.caret) + (sc.k ?? 0);
  await caretAt(p, at);
  await sleep(200);
  await p.eval(`document.querySelector('#play-pdf .math-menu').open = true`);
  const row = `#play-pdf .math-menu .ins-row[data-insert=${J(sc.item)}]`;
  if (!await p.until(`!!document.querySelector(${J(row)})`, 3000)) throw new Error(`the Insert menu has no ${sc.item}`);
  await p.eval(`document.querySelector(${J(row)}).click()`);
  let t1 = await written(p, t0);
  // `type`: keys at the caret the insert left (an empty equation's body), as the person goes on
  if (t1 && sc.type) { await p.eval(`document.querySelectorAll('dialog.sd-dialog[open]').forEach((d) => d.close()); ${view}.focus()`); for (const k of sc.type) await p.key(k); t1 = await written(p, t1); }
  if (!t1) { res.fails.push(`text unchanged (toast: ${await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`)})`); return; }
  res.info.text = t1.slice(t1.indexOf('\\begin{document}'));
  if (!sc.want.test(t1)) res.fails.push(`text: ${J(res.info.text)}`);
  // `widget`: the dialog the insert opens once its run is in, waited for and closed (else it opens later over the keys)
  if (sc.widget && !await p.until(`document.querySelector('dialog.sd-dialog[open]')?.dataset.pop === ${J(sc.widget)}`, 60000)) res.fails.push(`no ${sc.widget} widget opened`);
  await p.eval(`document.querySelectorAll('dialog.sd-dialog[open]').forEach((d) => d.close())`);
  const b = await build(p);
  res.info.status = b.status.slice(0, 120);
  if (!b.ok || errs(b.status) > errs(first.status) || /failed|stopped/i.test(b.status)) res.fails.push(`build: ${b.status.slice(0, 160)}`);
  // the insert is one undo step, the keys typed after it (`type`) one more
  for (let k = sc.type ? 2 : 1; k--;) await undo(p);
  const back = await text(p);
  if (back !== t0) { let i = 0; while (back[i] === t0[i]) i++; res.fails.push(`Ctrl+Z did not give the text back: at ${i} ${J(back.slice(i - 20, i + 60))}`); }
}

async function table(p, sc, res) {
  await start(p, sc);
  const t0 = await text(p);
  const opened = () => p.until(`(self.__e2eOpen ??= self.satexOpen(${J({ widget: 'table', at: sc.at, k: sc.k ?? 1 })}).then((v) => (self.__e2eOpen = null, v))) && document.querySelector('dialog.sd-dialog[open]')`, 120000);
  const apply = async () => { await p.eval(`document.querySelector('dialog.sd-dialog[open] .sd-primary')?.click()`); await sleep(300); return written(p, t0, 6000); };
  if (!await opened()) throw new Error('the table dialog did not open');
  await sleep(800);
  await p.shot(`${sc.name}-dialog.png`);
  res.info.grid = await p.eval(`[...document.querySelectorAll('dialog.sd-dialog[open] tr')].map((r) => [...r.querySelectorAll('td.sd-cell')].map((c) => c.textContent.trim())).filter((r) => r.length)`);
  const same = await apply();
  if (same && same !== t0) res.fails.push(`Apply untouched rewrote the text: ${J(same.slice(same.indexOf('\\begin{tabular}')))}`);
  await p.eval(`document.querySelectorAll('dialog.sd-dialog[open]').forEach((d) => d.close())`);
  await setText(p, t0);
  await quiet(p);
  if (!await opened()) throw new Error('the table dialog did not open again');
  await sleep(800);
  const cell = await p.eval(`(() => { const c = [...document.querySelectorAll('dialog.sd-dialog[open] td.sd-cell')].find((c) => c.textContent.trim() === ${J(sc.cell)}); if (!c) return null; const r = c.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()`);
  if (!cell) { res.fails.push(`no cell ${J(sc.cell)} in ${J(res.info.grid)}`); return; }
  // a double click as the browser sees one: two presses, the second with clickCount 2
  await p.mouse(...cell, 1);
  await sleep(80);
  await p.mouse(...cell, 2);
  if (!await p.until(`document.activeElement?.closest?.('td.sd-cell') && document.activeElement.tagName === 'INPUT'`, 3000)) { res.fails.push('a double click opened no cell input'); return; }
  await p.eval(`(() => { const i = document.activeElement; i.value = ${J(sc.to)}; i.dispatchEvent(new Event('input', { bubbles: true })); })()`);
  await p.key('Enter');
  await sleep(300);
  const t1 = await apply();
  if (!t1) { res.fails.push(`Apply after a cell edit wrote nothing (dialog: ${J(await p.eval(`document.querySelector('dialog.sd-dialog[open] .sd-err')?.textContent ?? 'closed'`))}, toast: ${J(await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`))})`); await p.shot(`${sc.name}-apply.png`); return; }
  if (t1 !== sc.want(t0)) res.fails.push(`after the edit: ${J(t1.slice(t1.indexOf('\\begin{tabular}'), t1.indexOf('\\end{document}')))}`);
}

async function big(p, sc, res) {
  await start(p, sc);
  const t0 = Date.now(), b = await build(p, 600000);
  res.info.buildMs = Date.now() - t0;
  if (!b.ok) throw new Error(`no build (${b.status.slice(0, 120)})`);
  res.info.pages = await p.eval(`document.querySelectorAll('#play-pdf .pdf-page').length`);
  if (res.info.pages < sc.pages) res.fails.push(`${res.info.pages} pages, want >= ${sc.pages}`);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  const w = await word(p, sc.word);
  if (!w) { res.fails.push(`no page word ${sc.word}: ${J(await p.eval('self.__missing'))}`); return; }
  await p.mouse(w.r - 1, (w.t + w.b) / 2);
  if (!await p.until(`satexPdfCaret()`, 5000)) { res.fails.push('the click placed no Type caret'); return; }
  const k0 = Date.now();
  for (const k of keysOf(sc.keys)) { await p.key(k); await sleep(30); }
  const got = await p.until(`(() => { const t = satexHealth().text; return ${sc.contains}.test(t); })()`, sc.ms ?? 5000);
  res.info.keyMs = Date.now() - k0;
  if (!got) res.fails.push(`the keys did not land: caret ${J(await p.eval('satexHealth().cursor'))}`);
  await p.shot(`${sc.name}-typed.png`);
}

async function screen(p, sc, res) {
  await p.s('Emulation.setDeviceMetricsOverride', { width: sc.width, height: sc.height, deviceScaleFactor: 2, mobile: sc.width < 600 });
  await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: sc.theme ?? 'light' }] });
  await start(p, sc);
  const over = () => p.eval(`document.documentElement.scrollWidth - innerWidth`);
  if (await over() > 1) res.fails.push(`the page scrolls sideways by ${await over()} px`);
  await p.shot(`${sc.name}-page.png`);
  if (!await p.until(`(self.__e2eOpen ??= self.satexOpen(${J({ widget: sc.widget, at: sc.at, k: sc.k ?? 1 })}).then((v) => (self.__e2eOpen = null, v))) && (${shown})`, 120000)) throw new Error(`the ${sc.widget} widget did not open`);
  await sleep(1200);
  await p.shot(`${sc.name}-${sc.widget}.png`);
  const box = await p.eval(`(() => { const d = ${shown}, r = d.getBoundingClientRect(); return { l: r.left, r: r.right, t: r.top, b: r.bottom, w: innerWidth, h: innerHeight, inner: d.scrollWidth - d.clientWidth }; })()`);
  res.info.box = box;
  if (box.l < -1 || box.r > box.w + 1) res.fails.push(`the ${sc.widget} widget runs past the screen: ${J(box)}`);
  if (await over() > 1) res.fails.push(`with the widget open the page scrolls sideways by ${await over()} px`);
  // nothing in the widget is wider than the widget itself (cut off on the right with no scroll)
  const cut = await p.eval(`(() => { const d = ${shown}, r = d.getBoundingClientRect(); return [...d.querySelectorAll('button, input, select, label, h2, h3')].filter((e) => { const b = e.getBoundingClientRect(); return b.width && b.right > r.right + 1 && !e.closest('[style*=overflow], .sd-wrap, .sd-tex, .sd-body'); }).map((e) => e.tagName + ':' + e.textContent.trim().slice(0, 20)).slice(0, 5); })()`);
  if (cut.length) res.fails.push(`controls cut off at the right: ${J(cut)}`);
  // the title bar on one row: its close button beside the title, not wrapped below it (390 px phone)
  const rows = await p.eval(`(() => { const hd = (${shown}).querySelector('header'); if (!hd) return 0; const ys = [...hd.children].filter((e) => e.getClientRects().length).map((e) => Math.round(e.getBoundingClientRect().top + e.getBoundingClientRect().height / 2)); return Math.max(...ys) - Math.min(...ys); })()`);
  if (rows > 12) res.fails.push(`the title bar wraps: its items ${rows} px apart`);
}

const CHECKS = { insert, table, big, screen };
export async function robust(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  try { await CHECKS[sc.check](p, sc, res); }
  catch (e) { res.fails.push(String(e.message ?? e).slice(0, 300)); await p.shot(`${sc.name}-fail.png`).catch(() => {}); }
  return res;
}
