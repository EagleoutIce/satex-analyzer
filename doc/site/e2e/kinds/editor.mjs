// The editor's gutter and the widgets' close, as a person uses them.
// `gutter`: hover a finding's marker (its card shows the finding and its fixes), click it (the Findings panel opens on it).
// `close`: open widget `widget` at `at` (+`k` characters) of example `example`, close it untouched (no discard question);
// with `edit` (page fields: a key and a value) Close asks and Close again (`how`: the X or Esc) discards.
import { sleep } from '../lib.mjs';

const J = JSON.stringify;
const started = (p) => p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000);
const example = async (p, name) => {
  await p.eval(`document.querySelector('[data-example=${J(name)}]').click()`);
  return p.until(`satexHealth().example === ${J(name)} && satexHealth().text?.length > 10`, 20000);
};

export async function gutter(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  if (!await started(p) || !await example(p, sc.example)) { fail('the playground did not start'); return res; }
  if (sc.dark) await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
  const mark = `document.querySelector('.cm-gutter-lint .cm-lint-marker')`;
  if (!await p.until(`!!${mark}`, 120000)) { fail('no finding marker'); return res; }
  await sleep(600);
  await p.until(`!!${mark}`, 30000);
  const m = await p.eval(`(() => { const r = ${mark}.getBoundingClientRect(), x = r.x + r.width / 2, y = r.y + r.height / 2; return { x, y, top: !!document.elementFromPoint(x, y)?.closest('.cm-lint-marker') }; })()`);
  if (!m.top) fail('the marker is covered');
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: m.x - 2, y: m.y });
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: m.x, y: m.y });
  res.info.card = await p.until(`[...document.querySelectorAll('.tip')].find((t) => !t.hidden && t.getClientRects().length && t.querySelector('.tip-lint, [class*=f-]'))?.textContent?.slice(0, 80)`, 4000);
  if (!res.info.card) fail('hovering the marker shows no finding');
  else if (sc.card && !sc.card.test(res.info.card)) fail(`the card says ${J(res.info.card)}`);
  await p.shot(`${sc.name}-hover.png`);
  await p.eval(`document.querySelector('[data-panel=play-typeset]').click()`);
  await sleep(300);
  await p.mouse(m.x, m.y);
  await sleep(700);
  res.info.after = await p.eval(`({ panel: document.querySelector('[data-panel=play-findings]').getAttribute('aria-selected'), open: document.querySelectorAll('#play-findings li.open').length })`);
  if (res.info.after.panel !== 'true' || !res.info.after.open) fail(`clicking the marker opens no finding: ${J(res.info.after)}`);
  await p.shot(`${sc.name}-click.png`);
  return res;
}

export async function close(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  if (!await started(p) || !await example(p, sc.example)) { fail('the playground did not start'); return res; }
  const open = `self.satexOpen(${J({ widget: sc.widget, at: sc.at, k: sc.k ?? 0 })})`;
  if (!await p.until(`(self.__e2eOpen ??= ${open}.then((v) => (self.__e2eOpen = null, v))) && document.querySelector('#play-view[open], dialog.sd-dialog[open]')`, 120000)) { fail('the widget did not open'); return res; }
  await sleep(1500);
  const state = `(() => ({ open: !!document.querySelector('#play-view[open], dialog.sd-dialog[open]'), ask: !!document.querySelector('dialog .sd-discard') }))()`;
  const x = `(() => { const d = [...document.querySelectorAll('dialog[open]')].pop(); (d.querySelector('#view-close') ?? d.querySelector('header [aria-label=close]')).click(); })()`;
  const act = async () => { if (sc.how === 'escape') await p.key('Escape'); else await p.eval(x); await sleep(400); };
  if (sc.edit) {
    const [key, value] = sc.edit;
    await p.eval(`(() => { const i = document.querySelector('#play-view input[data-key=${key}]'); i.focus(); i.value = ${J(value)}; i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new Event('change', { bubbles: true })); })()`);
    await sleep(300);
    await act();
    res.info.asked = await p.eval(state);
    await p.shot(`${sc.name}-ask.png`);
    if (!res.info.asked.open || !res.info.asked.ask) fail(`an edit, then ${sc.how ?? 'close'}: no question ${J(res.info.asked)}`);
  }
  await act();
  res.info.closed = await p.eval(state);
  if (res.info.closed.open || res.info.closed.ask) fail(`${sc.edit ? 'closing again' : 'closing untouched'} left ${J(res.info.closed)}`);
  return res;
}

/** `auto`: a fresh viewer presses Compile once with the PDF shown, then types a word in the editor: a new build of that
 * text lands without another press (the first Compile turns the automatic one on). With the Typeset tab shown instead,
 * no build runs until the PDF tab is back (`hidden`). */
export async function auto(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  await example(p, 'hello.tex');
  await started(p);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  const built = () => p.eval(`document.querySelector('#play-pdf .pdf-page img')?.__build ?? null`).then((b) => b && JSON.stringify([b.text?.length, b.ms ?? 0]));
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await p.until(`document.querySelector('#play-pdf .pdf-page img')?.__build && !document.querySelector('#play-pdf.pdf-running')`, 90000)) { res.fails.push('setup: the first build never landed'); return res; }
  const word = sc.word ?? 'Quokka';
  if (sc.hidden) await p.eval(`document.querySelector('.side-tabs [data-panel=play-typeset]').click()`);
  // the builds started from here on: each turns the pane's pdf-running on
  await p.eval(`(() => { const el = document.getElementById('play-pdf'); let was = el.classList.contains('pdf-running'); self.__e2eRuns = 0; new MutationObserver(() => { const now = el.classList.contains('pdf-running'); if (now && !was) self.__e2eRuns++; was = now; }).observe(el, { attributes: true, attributeFilter: ['class'] }); })()`);
  await p.eval(`document.querySelector('.cm-content').focus()`);
  for (const [type, key, code, mod] of [['rawKeyDown', 'End', 35, 2], ['keyUp', 'End', 35, 2]]) await p.s('Input.dispatchKeyEvent', { type, key, code: key, windowsVirtualKeyCode: code, modifiers: mod });
  await p.s('Input.insertText', { text: `\n${word}\n` });
  const text = await p.eval(`satexHealth().text`);
  const landed = (ms) => p.until(`[...document.querySelectorAll('#play-pdf .pdf-page img')].some((i) => i.__build?.text === ${J(text)})`, ms);
  if (sc.hidden) {
    await sleep(4000);
    if (await landed(100) || await p.eval(`self.__e2eRuns`)) res.fails.push('a build ran while the PDF tab was hidden');
    await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  }
  if (!await landed(45000)) res.fails.push(`no build of the typed text landed without a second press (auto: ${await p.eval(`document.getElementById('pdf-compile').title`)})`);
  res.info.built = await built();
  return res;
}

/** `dated`: automatic compile off; a build of `text` fails; the editor's text then becomes `fixed`: the bar's counts (and
 * a failure banner, shown after Type edits) say they are of an older text; Compile replaces them with the build of the text shown. */
export async function dated(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  await example(p, 'hello.tex');
  await started(p);
  const { setText } = await import('../engine.mjs');
  await setText(p, sc.text);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await p.eval(`(() => { const i = document.querySelector('#pdf-auto-list input[value="0"]'); i.checked = true; i.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  const banner = () => p.eval(`document.querySelector('#play-pdf .pdf-fail')?.textContent ?? ''`);
  const build = async () => { await p.eval(`(self.__e2eG = document.querySelector('#play-pdf .pdf-page img')?.__build ?? null, document.getElementById('pdf-compile').click())`); return p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build && i.__build !== self.__e2eG && !document.querySelector('#play-pdf.pdf-running'); })()`, 90000); };
  if (!await build()) { res.fails.push('setup: the first build never landed'); return res; }
  const bar0 = await p.eval(`document.getElementById('pdf-status').textContent`);
  if (!/error/.test(bar0)) { res.fails.push(`setup: the build did not fail: ${J(bar0)}`); return res; }
  await setText(p, sc.fixed);
  await sleep(1500);
  const old = await banner(), bar = await p.eval(`document.getElementById('pdf-status').textContent`);
  res.info.old = old; res.info.bar = bar;
  if (old && !/older text/.test(old)) res.fails.push(`the old build's banner does not say it is of an older text: ${J(old)}`);
  if (/error/.test(bar) && !/older text/.test(bar)) res.fails.push(`the bar counts the old build's errors as the text's: ${J(bar)}`);
  if (!await build()) { res.fails.push('the second build never landed'); return res; }
  const now = await banner(), bar1 = await p.eval(`document.getElementById('pdf-status').textContent`);
  if (now || /error|older/.test(bar1)) res.fails.push(`the build of the fixed text still shows a failure: ${J(now || bar1)}`);
  return res;
}
