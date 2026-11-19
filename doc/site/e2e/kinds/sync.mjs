// The findings and the Typeset view converge on the editor's text: keys typed into the editor (a pause mid-word lets
// a run read the half-typed name), then quiet: the last text sent is the editor's, the findings shown are of its
// version, and they are those of the final text alone (a native `satex lint` of it), never a run of a half-typed one.
// Scenario fields: `text` (the start), `at` (where the keys go: the line), `keys`, `pauseAfter` (the prefix of
// `keys` after which a run is waited for), `never` (findings or row text that must not show), `want` (that must).
import { sleep } from '../lib.mjs';
import { setText } from '../engine.mjs';

const J = JSON.stringify;
const quiet = (p, ms = 60000) => p.until(`(() => { const h = satexHealth(); if (h.busy || h.due || h.running || h.queued || h.pending) { self.__q = null; return false; } self.__q ??= performance.now(); return performance.now() - self.__q > 1500; })()`, ms);
export async function sync(p, sc) {
  const res = { name: sc.name, kind: 'sync', fails: [], perf: [], info: {} };
  await p.eval(`document.querySelector('[data-example="hello.tex"]').click()`);
  await p.until(`satexHealth().file === 'hello.tex' && satexHealth().text?.length > 10`, 20000);
  await sleep(400);
  await setText(p, sc.text);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-typeset]').click()`);
  await quiet(p, 240000);
  // the caret at the end of line `at` (1-based)
  const nav = async (k, mods = 0) => { const code = { Home: 36, ArrowDown: 40, End: 35 }[k]; for (const type of ['rawKeyDown', 'keyUp']) await p.s('Input.dispatchKeyEvent', { type, key: k, code: k, windowsVirtualKeyCode: code, modifiers: mods }); };
  await p.eval(`document.querySelector('.cm-content').focus()`);
  await nav('Home', 2);
  for (let i = 1; i < sc.at; i++) await nav('ArrowDown');
  await nav('End');
  let n = 0;
  for (const ch of sc.keys) {
    await p.s('Input.insertText', { text: ch });
    await sleep(40);
    if (++n === sc.pauseAfter) await quiet(p);
  }
  await quiet(p);
  await sleep(800);
  return converged(p, sc, res);
}

/** After quiet, what every run shown must hold: the editor's text is the last one sent and the findings are of its
 * version, the run had its kernel (a run cut while it built the kernel once read the text on part of it and found
 * `\\documentclass` undefined), and the scenario's `never`/`want` findings. */
async function converged(p, sc, res) {
  const st = await p.eval(`(() => { const h = satexHealth(), sent = self.__e2e.lsp.filter((m) => m.text != null && m.uri?.endsWith('/' + h.file)).at(-1);
    return { synced: sent?.text === h.text, sentV: h.sent, shownV: h.shown, text: h.text,
      rows: satexTypesetOrigins().map((r) => (r.chars ?? '').replace(/:\\w+/g, '').replace(/ /g, '')).join('\\n'),
      findings: document.getElementById('lsp-diagnostics')?.textContent.replace(/\\s+/g, ' ') ?? '' }; })()`);
  res.info.findings = st.findings.slice(0, 300);
  if (!st.synced) res.fails.push('the server was last sent another text than the editor shows');
  if (st.sentV !== st.shownV) res.fails.push(`findings of version ${st.shownV} shown, the editor is at ${st.sentV}`);
  if (/\\documentclass\S* is never defined/.test(st.findings)) res.fails.push(`a run without its kernel was shown: ${J(st.findings.slice(0, 200))}`);
  for (const re of sc.never ?? []) if (re.test(st.findings) || re.test(st.rows)) res.fails.push(`shown after quiet, from a run of a half-typed text: ${re} (findings: ${J(st.findings.slice(0, 200))}, rows: ${J(st.rows.slice(0, 200))})`);
  for (const re of sc.want ?? []) if (!re.test(st.findings)) res.fails.push(`not in the findings after quiet: ${re} (findings: ${J(st.findings.slice(0, 200))})`);
  if (res.fails.length) res.shot = await p.shot(`${sc.name}.png`);
  return res;
}

/** The first run after the caches are dropped builds the kernel; while it does, `during` happens: `type` types
 * `keys` at the end of line `at`, `example` opens the example `example`. After quiet the shown run is the
 * final text's, with its kernel (`converged`). Scenario fields: `during`, `keys`, `at`, `example`, `never`, `want`. */
export async function kernelCut(p, sc) {
  const res = { name: sc.name, kind: 'kernelCut', fails: [], perf: [], info: {} };
  await p.eval(`document.querySelector('[data-example="hello.tex"]').click()`);
  await p.until(`satexHealth().file === 'hello.tex' && satexHealth().text?.length > 10`, 20000);
  await quiet(p, 600000);
  // every cache dropped: the server starts again and its first run builds the kernel
  await p.eval(`document.getElementById('cache-clear').click()`);
  const t0 = Date.now();
  await p.until(`/kernel/i.test(satexHealth().status)`, 120000);
  res.info.kernelAfter = Date.now() - t0;
  await sleep(1500);
  if (sc.during === 'example') {
    await p.eval(`document.querySelector('#play-exsect').open = true; document.querySelector('[data-example=${J(sc.example)}]').click()`);
    await p.until(`satexHealth().file === ${J(sc.example)}`, 20000);
  } else {
    await p.eval(`document.querySelector('.cm-content').focus()`);
    const key = async (k, mods = 0) => { const code = { Home: 36, ArrowDown: 40, End: 35 }[k]; for (const type of ['rawKeyDown', 'keyUp']) await p.s('Input.dispatchKeyEvent', { type, key: k, code: k, windowsVirtualKeyCode: code, modifiers: mods }); };
    await key('Home', 2);
    for (let i = 1; i < sc.at; i++) await key('ArrowDown');
    await key('End');
    for (const ch of sc.keys) { await p.s('Input.insertText', { text: ch }); await sleep(40); }
  }
  await quiet(p, 900000);
  await sleep(800);
  return converged(p, sc, res);
}
