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
