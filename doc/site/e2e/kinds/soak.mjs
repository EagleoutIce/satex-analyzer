// A long editing session against a fresh one: `n` edits in the editor (a letter typed, then taken back, a pause
// between, a run waited for every `every`), then one more edit: the time from its key to its findings shown (the
// version sent shown, nothing due) stays within `ratio` of the same edit's in the fresh session (the user's "a reload
// makes it fast again"). Fields: `text`, `line` (where the keys go), `n`, `every`, `ratio`.
import { sleep } from '../lib.mjs';
import { setText } from '../engine.mjs';

const settled = (p, ms = 120000) => p.until(`(() => { const h = satexHealth(); if (h.busy || h.due || h.running || h.queued || h.pending || h.sent !== h.shown) { self.__q = null; return false; } self.__q ??= performance.now(); return performance.now() - self.__q > 300; })()`, ms);
const nav = async (p, k, mods = 0) => { const code = { Home: 36, ArrowDown: 40, End: 35, Backspace: 8 }[k]; for (const type of ['rawKeyDown', 'keyUp']) await p.s('Input.dispatchKeyEvent', { type, key: k, code: k, windowsVirtualKeyCode: code, modifiers: mods }); };
/** One edit (a letter at the line's end) to its findings: ms. */
async function edit(p, ch) {
  const t0 = Date.now();
  await p.s('Input.insertText', { text: ch });
  await sleep(50);
  await settled(p);
  return Date.now() - t0 - 300;
}
export async function soak(p, sc) {
  const res = { name: sc.name, kind: 'soak', fails: [], perf: [], info: {} };
  await p.eval(`document.querySelector('[data-example="hello.tex"]').click()`);
  await p.until(`satexHealth().file === 'hello.tex' && satexHealth().text?.length > 10`, 20000);
  await sleep(400);
  await setText(p, sc.text);
  await settled(p, 240000);
  await p.eval(`document.querySelector('.cm-content').focus()`);
  await nav(p, 'Home', 2);
  for (let i = 1; i < sc.line; i++) await nav(p, 'ArrowDown');
  await nav(p, 'End');
  const fresh = [];
  for (let i = 0; i < 3; i++) { fresh.push(await edit(p, 'q')); await nav(p, 'Backspace'); await settled(p); }
  for (let i = 0; i < sc.n; i++) {
    await p.s('Input.insertText', { text: 'abcdefghij'[i % 10] });
    await sleep(120);
    await nav(p, 'Backspace');
    await sleep(120);
    if (i % sc.every === sc.every - 1) await settled(p);
  }
  await settled(p);
  const late = [];
  for (let i = 0; i < 3; i++) { late.push(await edit(p, 'q')); await nav(p, 'Backspace'); await settled(p); }
  const med = (xs) => [...xs].sort((a, b) => a - b)[1];
  res.info = { freshMs: fresh, lateMs: late, heapMB: Math.round((await p.eval('performance.memory?.usedJSHeapSize ?? 0')) / 1e6) };
  if (med(late) > med(fresh) * sc.ratio + 200) res.fails.push(`after ${sc.n} edits an edit takes ${med(late)} ms to its findings, ${med(fresh)} ms in the fresh session (bound ${sc.ratio}x + 200 ms)`);
  return res;
}
