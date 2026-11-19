// Crash recovery (playground.js `recover`): the analyzer killed, panicking, out of memory or hung restarts by itself
// with the open text, keys typed during the restart land, and three crashes in a minute stop it with Restart and
// Report. doc/site/chaos.mjs covers a kill at rest and a hang against the static site; these need `?debug` hooks
// (`satexChaos`) and typing around the crash. Scenario field: `how` ('midrun' | 'panic' | 'oom' | 'backoff').
import { sleep } from '../lib.mjs';

const J = JSON.stringify;
/** No run going, due, queued or recovering for 400 ms; null after `ms`. */
const rest = (p, ms = 120000) => p.until(`(() => { const h = satexHealth(); if (h.busy || h.boot || h.running || h.queued || h.recovering || h.due) { self.__e2eR = null; return false; } self.__e2eR ??= performance.now(); return performance.now() - self.__e2eR > 400 && h; })()`, ms);
const type = async (p, t, gap = 0) => { for (const ch of t) { await p.s('Input.insertText', { text: ch }); if (gap) await sleep(gap); } };
const focusEnd = async (p) => {
  await p.eval(`document.querySelector('.cm-content').focus()`);
  for (const type of ['rawKeyDown', 'keyUp']) await p.s('Input.dispatchKeyEvent', { type, key: 'End', code: 'End', windowsVirtualKeyCode: 35, modifiers: 2 });
};

export async function crash(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  await p.s('Page.navigate', { url: (await p.eval('location.href')).replace(/[?#].*$/, '') + '?debug#thesis.tex' });
  await sleep(300);
  if (!await p.until(`!!self.satexHealth && !satexHealth().boot && satexHealth().file === 'thesis.tex'`, 120000)) throw new Error('the playground did not start');
  if (!await rest(p, 240000)) throw new Error('the first run never ended');
  const recoveries0 = (await p.eval('satexHealth().recoveries')) ?? 0;
  const t0 = Date.now();
  let want = null;
  if (sc.how === 'midrun') {
    await focusEnd(p); await type(p, ' midrun'); await sleep(300);
    await p.eval(`self.satexKill('crash')`);
    await type(p, ' during', 60);
    want = ' midrun during';
  } else if (sc.how === 'panic' || sc.how === 'oom') {
    if (!await p.eval('!!self.satexChaos')) throw new Error('no satexChaos hook (?debug)');
    await focusEnd(p); await type(p, ` ${sc.how}`);
    await p.eval(`self.satexChaos(${J(sc.how)})`);
    await sleep(50); await type(p, ' x');
    want = ` ${sc.how} x`;
  } else if (sc.how === 'backoff') {
    // three crashes a little apart: the third stops the restarts and offers Restart and Report
    for (let i = 0; i < 3; i++) {
      await p.eval(`self.satexKill('crash')`);
      if (i < 2) { await p.until(`!satexHealth().recovering`, 30000); await sleep(1500); }
    }
    const buttons = await p.until(`(() => { const b = [...document.querySelectorAll('#play-status button')].map((b) => b.textContent); return b.length && b; })()`, 20000);
    res.info.buttons = buttons;
    if (!buttons?.some((b) => /restart/i.test(b))) res.fails.push(`no Restart after three crashes: ${J(buttons)} (status ${J(await p.eval(`document.getElementById('play-status')?.textContent?.slice(0, 120)`))})`);
    if (!await p.eval(`!!document.getElementById('play-report')`)) res.fails.push('no Report button after three crashes');
    await p.eval(`(document.getElementById('play-restart') ?? document.getElementById('play-retry'))?.click()`);
  }
  const h = await rest(p, 180000);
  res.info.recoverMs = Date.now() - t0;
  if (!h) { res.fails.push(`never back at rest (status ${J(await p.eval(`document.getElementById('play-status')?.textContent?.slice(0, 120)`))})`); return res; }
  res.info.recoveries = h.recoveries - recoveries0;
  if (!(h.recoveries > recoveries0)) res.fails.push('no recovery counted');
  if (want && !(h.text ?? '').includes(want)) res.fails.push(`text lost: want ${J(want)} in the editor, got ${J((h.text ?? '').slice(-60))}`);
  if (!(h.text ?? '').includes('\\documentclass')) res.fails.push('the open file is gone after the restart');
  return res;
}
