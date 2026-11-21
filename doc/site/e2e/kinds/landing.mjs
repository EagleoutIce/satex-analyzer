// The landing page's live window (site.ts `.shot-live`): the recording gives way to the editor ("Click to type
// yourself"), a click puts the caret on a line, the engine analyzes the text, and the typeset pane beside it shows the
// paragraphs the run set. Pins the pane left at "It sets no paragraph." (user report 2026-11-21 13:13): the page took the
// engine's mid-run notes as answers, so the typeset question got the analysis's reply.
// Scenario fields: `line` (1-based line to click), `want` (texts the pane must show).
import { sleep } from '../lib.mjs';
import { ctx } from '../engine.mjs';

const J = JSON.stringify;
export async function landing(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  await p.s('Page.navigate', { url: new URL('/', ctx.url).href });
  if (!await p.until(`document.querySelector('.shot-live')`, 30000)) { fail('no live window on the landing page'); return res; }
  await p.eval(`document.querySelector('.shot-live').scrollIntoView({ block: 'center' })`);
  await sleep(1500);
  await p.eval(`[...document.querySelectorAll('.shot-live button')].find((b) => /type yourself/.test(b.textContent))?.click()`);
  if (!await p.until(`document.querySelectorAll('.shot-live .cm-line').length >= ${sc.line}`, 240000)) { fail('the editor did not stand'); return res; }
  await sleep(500);
  const xy = await p.eval(`(() => { const r = [...document.querySelectorAll('.shot-live .cm-line')][${sc.line - 1}].getBoundingClientRect(); return [r.left + 60, r.top + r.height / 2]; })()`);
  await p.mouse(xy[0], xy[1]);
  if (!await p.until(`/Analyzed in/.test(document.querySelector('.shot-live .live-status')?.textContent ?? '')`, 240000)) { fail(`no analysis: ${await p.eval(`document.querySelector('.shot-live .live-status')?.textContent`)}`); return res; }
  await sleep(500);
  const pane = await p.eval(`document.querySelector('.shot-live .live-ts')?.textContent ?? ''`);
  res.info.pane = pane.slice(0, 120);
  for (const w of sc.want) if (!pane.includes(w)) fail(`the typeset pane lacks ${J(w)}: ${J(pane.slice(0, 120))}`);
  res.shot = await p.shot(`${sc.name}.png`);
  return res;
}
