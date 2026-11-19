// The PDF's "you are here" mark (follow): it hugs the cursor line's glyphs (never the footer), moves with the cursor,
// fades when the cursor rests, and goes on a view change, follow off, a Type caret, a build swap and a file switch.
// The compile server is blocked: the browser's own pdfTeX builds. Scenario fields: `dark` (prefers-color-scheme).
import { sleep } from '../lib.mjs';

const J = JSON.stringify;

export async function follow(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  // a boolean is a check, anything else a measurement kept for the report
  const ok = (k, v) => { if (v === false) res.fails.push(k); else if (v !== true) res.info[k] = v; };
  await p.block(['*/__satex/compile*']);
  // the page again, its workers started under the block
  await p.s('Page.reload', { ignoreCache: false });
  await sleep(500);
  await p.until(`document.readyState === 'complete'`, 30000);
  if (sc.dark) await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
  if (!await p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000)) throw new Error('the playground did not start');
  await p.eval(`document.querySelector('[data-example="spacing.tex"]').click()`);
  await p.until(`satexHealth().example === "spacing.tex"`, 30000);
  const files = await p.eval(`[...document.querySelectorAll('#play-files-tree .file')].map((f) => f.dataset.name)`);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await sleep(300);
  const fresh = `(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.naturalWidth > 100 && satexPdfWords().some((w) => w[5] === 'office') && !document.querySelector('#play-pdf.pdf-running'); })()`;
  if (!await p.eval(fresh)) { await p.eval(`document.getElementById('pdf-compile').click()`); }
  if (!await p.until(fresh, 240000)) { await p.shot('follow-fail.png'); throw new Error(JSON.stringify(await p.eval(`[satexHealth().text?.slice(0, 80), document.getElementById('pdf-body')?.textContent.slice(0, 600)]`)) + 'no build: ' + await p.eval(`document.getElementById('pdf-status')?.textContent`)); }
  await sleep(800);
  // marks as page-relative bp boxes, with the words of the page
  const marks = `(() => { const out = []; for (const m of document.querySelectorAll('#pdf-body .pdf-mark')) { const pg = m.closest('.pdf-page'), img = pg.querySelector('img'), r = img.getBoundingClientRect(), q = m.getBoundingClientRect(), k = (img.naturalWidth * 72 / satexPdfWords().dpi) / r.width; out.push({ cls: m.className, l: (q.left - r.left) * k, t: (q.top - r.top) * k, r: (q.right - r.left) * k, d: (q.bottom - r.top) * k, fade: m.classList.contains('fade') }); } return out; })()`;
  const cursor = (line, ch) => p.eval(`(() => { const v = document.querySelector('#play-editor .cm-editor').satexView; v.focus(); v.dispatch({ selection: { anchor: v.state.doc.line(${line}).from + ${ch} } }); })()`);
  const words = await p.eval(`satexPdfWords().map((w) => [w[1], w[2], w[3], w[4], w[5]])`);
  const see = words.find((w) => w[4] === 'See'), hello = words.filter((w) => Math.abs(w[1] - see[1]) < 2);
  const right = Math.max(...hello.map((w) => w[2]));
  await cursor(7, 9);
  await sleep(300);
  const m1 = await p.eval(marks);
  ok('line 7: marks', m1.map((m) => `${m.cls} ${m.l.toFixed(0)}-${m.r.toFixed(0)} @${m.t.toFixed(0)}`));
  ok('line 7: hugs (right edge within 3bp of the row last word)', m1.filter((m) => !/word/.test(m.cls)).every((m) => Math.abs(m.r - right) < 3));
  ok('line 7: words of the row', hello.map((w) => `${w[4]} ${w[0].toFixed(0)}-${w[2].toFixed(0)}`).join(' '));
  ok('line 7: no footer mark', m1.every((m) => m.t < 600));
  const box = await p.eval(`(() => { const pg = document.querySelector('#pdf-body .pdf-page'), r = pg.getBoundingClientRect(); return { x: r.x, y: r.y, width: Math.min(r.width, 520), height: 200 }; })()`);
  await p.shot(`${sc.name}-follow.png`, box);
  await cursor(10, 6);
  await sleep(300);
  const m2 = await p.eval(marks);
  ok('cursor move: moved to line 10', m2.length > 0 && JSON.stringify(m2) !== JSON.stringify(m1));
  ok('line 10: marks', m2.map((m) => `${m.cls} ${m.l.toFixed(0)}-${m.r.toFixed(0)} @${m.t.toFixed(0)}`));
  await p.shot(`${sc.name}-moved.png`, box);
  // the fade starts ~1.9 s after the cursor rests: polled, not slept, so a loaded machine only waits longer
  ok('rest: faded and gone after the cursor rests', !!await p.until(`${marks}.length === 0`, 8000));
  await cursor(7, 2); await sleep(200);
  ok('again on line 7', (await p.eval(marks)).length > 0);
  // a view change
  const views = await p.eval(`[...document.querySelectorAll('#pdf-views [data-view]')].map((v) => v.dataset.view)`);
  if (views.length > 1) { await p.eval(`document.querySelector('#pdf-views [data-view="${views[1]}"]').click()`); await sleep(100); ok(`view change (${views[1]}): gone`, (await p.eval(marks)).length === 0); await p.eval(`document.querySelector('#pdf-views [data-view="${views[0]}"]').click()`); }
  // follow off: gone; on again: back
  await cursor(7, 4); await sleep(200);
  await p.eval(`document.getElementById('pdf-follow').click()`); await sleep(100);
  ok('follow off: gone', (await p.eval(marks)).length === 0);
  await p.eval(`document.getElementById('pdf-follow').click()`); await sleep(200);
  // a Type caret on the page
  await cursor(7, 4); await sleep(200);
  const w = await p.eval(`(() => { const x = satexPdfWords().find((w) => w[5] === 'findings'); const img = document.querySelector('#play-pdf .pdf-page[data-page="' + x[0] + '"] img'), r = img.getBoundingClientRect(), k = r.width / (img.naturalWidth * 72 / satexPdfWords().dpi); return [r.left + (x[1] + x[3]) / 2 * k, r.top + (x[2] + x[4]) / 2 * k]; })()`);
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await p.mouse(...w); await sleep(400);
  ok('Type caret: no stale mark on line 7', (await p.eval(marks)).every((m) => m.t > m1[0].d));
  await p.key('Escape'); await sleep(200);
  // a build swap: a new text compiled
  await cursor(7, 4); await sleep(200);
  await p.eval(`(() => { const v = document.querySelector('#play-editor .cm-editor').satexView; v.dispatch({ changes: { from: v.state.doc.line(7).from, insert: 'New ' } }); })()`);
  await sleep(200);
  await p.eval(`document.getElementById('pdf-compile').click()`);
  await p.until(`satexPdfWords().some((w) => w[5] === 'New')`, 120000);
  await p.until(`${marks}.length === 0`, 8000);
  ok('build swap: none left over', (await p.eval(marks)).length === 0);
  // a file switch
  const other = files.find((f) => f !== 'spacing.tex' && /\.tex$/.test(f));
  if (other) {
    await cursor(7, 4); await sleep(200);
    await p.eval(`document.querySelector('#play-files-tree .file[data-name=${J(other)}]')?.click()`);
    await p.until(`satexHealth().file === ${J(other)}`, 20000);
    await sleep(100);
    const left = await p.eval(marks);
    ok(`file switch (${other}): line 7 gone`, left.every((m) => Math.abs(m.t - m1[0].t) > 2));
  }
  if (p.errors.length) res.fails.push(`page errors: ${J(p.errors.slice(0, 5))}`);
  return res;
}
