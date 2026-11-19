// Scratch slices as a person uses them: thesis.tex, chapter.tex's figure sliced from the editor's context menu into a
// scratch document (the root while it is open), its caption changed there, merged back through the preview, built.
// `scratch`: `at` the line to right-click, `from` and `to` the text the scratch edit replaces, `lines` the region.
import { sleep } from '../lib.mjs';
import { compileState } from '../engine.mjs';

const J = JSON.stringify;
const started = (p) => p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000);
const view = `document.querySelector('#play-editor .cm-editor').satexView`;

export async function scratch(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  if (!await started(p)) { fail('the playground did not start'); return res; }
  await p.eval(`document.querySelector('[data-example="thesis.tex"]').click()`);
  if (!await p.until(`satexHealth().example === 'thesis.tex' && satexHealth().text?.length > 10`, 20000)) { fail('thesis.tex did not open'); return res; }
  await p.eval(`document.querySelector('#play-tabs [data-name="chapter.tex"]').click()`);
  if (!await p.until(`satexHealth().file === 'chapter.tex'`, 10000)) { fail('chapter.tex did not open'); return res; }
  const before = await p.eval(`satexHealth().text`);
  // the root analyzed first, as when a person works on it: the slice reads that run
  await p.until(`(() => { const h = satexHealth(); return !h.running && !h.busy && !h.due && !h.queued && h.shown != null; })()`, 240000);
  res.info.rootRun = await p.eval(`document.getElementById('t-time')?.textContent ?? ''`);
  // the caret on the caption line, a right click there: the menu's top-level row
  const at = await p.eval(`(() => { const v = ${view}, l = v.state.doc.line(v.state.doc.toString().split('\\n').findIndex((x) => x.includes(${J(sc.at)})) + 1);
    v.dispatch({ selection: { anchor: l.from + 4 }, scrollIntoView: true }); v.focus(); return true; })()`);
  await sleep(300);
  const c = await p.eval(`(() => { const v = ${view}, r = v.coordsAtPos(v.state.selection.main.head); return r && [r.left + 2, (r.top + r.bottom) / 2]; })()`);
  if (!at || !c) { fail('no caption line'); return res; }
  for (const type of ['mousePressed', 'mouseReleased']) await p.s('Input.dispatchMouseEvent', { type, x: c[0], y: c[1], button: 'right', clickCount: 1 });
  const row = `[...document.querySelectorAll('#play-ctx li')].find((li) => /Work on this in a scratch document/.test(li.textContent))`;
  if (!await p.until(`!!${row}`, 10000)) { fail(`the editor menu has no scratch row: ${J(await p.eval(`document.getElementById('play-ctx').textContent.slice(0, 200)`))}`); return res; }
  await p.eval(`${row}.click()`);
  const t0 = Date.now();
  if (!await p.until(`satexHealth().root?.startsWith('scratch-chapter-') && satexHealth().file === satexHealth().root`, 240000)) {
    fail(`no scratch document: ${J(await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`))}`); return res;
  }
  res.info.sliceMs = Date.now() - t0;
  res.info.banner = await p.eval(`document.getElementById('play-scratch').hidden ? null : document.getElementById('scratch-what').textContent`);
  if (res.info.banner !== `Scratch slice of chapter.tex:${sc.lines}`) fail(`the banner says ${J(res.info.banner)}`);
  // the scratch document's own check, then the edit there
  const s0 = Date.now();
  await p.until(`(() => { const h = satexHealth(); return !h.running && !h.busy && !h.due && !h.queued && h.shown != null && h.shown === h.sent; })()`, 240000);
  res.info.checkMs = Date.now() - s0;
  // the point of a scratch document: a fast cycle (timing bounds, failures in the perf tier only)
  if (res.info.sliceMs > (sc.sliceMs ?? 3000)) res.perf.push(`slice ${res.info.sliceMs} ms, want <= ${sc.sliceMs ?? 3000}`);
  if (res.info.checkMs > (sc.checkMs ?? 5000)) res.perf.push(`first check of the scratch document ${res.info.checkMs} ms, want <= ${sc.checkMs ?? 5000}`);
  res.info.run = await p.eval(`document.getElementById('t-time')?.textContent ?? ''`);
  res.info.lines = await p.eval(`satexHealth().text.split('\\n').length`);
  const edited = await p.eval(`(() => { const v = ${view}, t = v.state.doc.toString(), i = t.indexOf(${J(sc.from)}); if (i < 0) return false; v.dispatch({ changes: { from: i, to: i + ${sc.from.length}, insert: ${J(sc.to)} } }); return true; })()`);
  if (!edited) { fail('the scratch document lacks the caption'); return res; }
  await sleep(400);
  await p.eval(`document.getElementById('scratch-merge').click()`);
  if (!await p.until(`document.getElementById('play-scratch-merge').open`, 5000)) { fail('Merge back shows no preview'); return res; }
  res.info.preview = await p.eval(`[...document.querySelectorAll('#scratch-diff .wf-add')].map((x) => x.textContent).join(' | ')`);
  if (!res.info.preview.includes(sc.to)) fail(`the preview adds ${J(res.info.preview)}`);
  await p.shot(`${sc.name}-preview.png`);
  await p.eval(`document.getElementById('scratch-mine').click()`);
  if (!await p.until(`satexHealth().root === 'thesis.tex' && satexHealth().file === 'chapter.tex'`, 20000)) fail(`after the merge the root is ${J(await p.eval('satexHealth().root'))}`);
  await sleep(600);
  const after = await p.eval(`satexHealth().text`), want = before.replace(sc.from, sc.to);
  if (after !== want) fail(`the source after the merge is not the original with the caption changed (${after.length} chars, want ${want.length})`);
  if (await p.eval(`!!document.querySelector('#play-tabs [data-name^="scratch-"]') || !document.getElementById('play-scratch').hidden`)) fail('the scratch tab or banner stayed');
  // the merged project builds
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await p.eval(`(self.__e2eB = new Set([...document.querySelectorAll('#play-pdf .pdf-page img')].map((x) => x.__build).filter(Boolean)), document.getElementById('pdf-compile').click())`);
  await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build && !self.__e2eB.has(i.__build) && !document.querySelector('#play-pdf.pdf-running'); })()`, 120000);
  res.info.build = await compileState(p);
  if (!res.info.build.ok) fail(`the merged project does not build: ${J(res.info.build)}`);
  else if (!await p.until(`satexPdfWords().some((w) => w[5].includes(${J(sc.to.split(' ').pop())}))`, 5000)) fail('the build lacks the new caption text');
  await p.shot(`${sc.name}-built.png`);
  return res;
}
