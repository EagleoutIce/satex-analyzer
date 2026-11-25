// The landing page's live window (site.ts `.shot-live`): a click anywhere in it opens the playground on that
// window's example (a shared `#s=` link, user wish 2026-11-22: "just navigate to the playground"), never an engine
// inside the landing page that may fail to load ("Typing here did not load").
// Scenario fields: `want` (texts the playground's editor must hold once it opened the example).
import { sleep } from '../lib.mjs';
import { ctx } from '../engine.mjs';

const J = JSON.stringify;
export async function landing(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  await p.s('Page.navigate', { url: new URL('/', ctx.url).href });
  if (!await p.until(`document.querySelector('.shot-live')`, 30000)) { fail('no live window on the landing page'); return res; }
  await p.eval(`document.querySelector('.shot-live').scrollIntoView({ block: 'center' })`);
  await sleep(1000);
  const xy = await p.eval(`(() => { const r = document.querySelector('.shot-live').getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
  await p.mouse(xy[0], xy[1]);
  // the click navigates: no question to the page until the playground's page stands
  await sleep(3000);
  // (the playground restores the shared link and names the opened file in the hash: `#results.tex`)
  if (!await p.until(`/\\/play\\//.test(location.pathname)`, 30000)) {
    fail(`the click did not open the playground: ${await p.eval('location.href')}`);
    return res;
  }
  const text = `document.querySelector('.cm-content')?.textContent ?? ''`;
  if (!await p.until(`(${text}).includes(${J(sc.want[0])})`, 240000)) {
    fail(`the playground's editor lacks ${J(sc.want[0])}: ${J((await p.eval(text)).slice(0, 120))}`);
    return res;
  }
  const shown = await p.eval(text);
  for (const w of sc.want) if (!shown.includes(w)) fail(`the playground's editor lacks ${J(w)}`);
  res.shot = await p.shot(`${sc.name}.png`);
  return res;
}
