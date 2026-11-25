// `offline`: every example opened and built by the browser's own pdfTeX (the compile server blocked, as on the static
// site) asks no host but the site: the site ships what each example's analysis reads (its bundles and package caches)
// and what its build reads (build.py `compile_reads`: the files in play/texfiles, TeX Live's gaps in `tex.absent`).
// Fields: `examples` (default every example the playground lists), `ms` (the bound for one example's analysis and build).
import { sleep } from '../lib.mjs';
import { ctx } from '../engine.mjs';

const J = JSON.stringify;

export async function offline(p, sc) {
  const res = { name: sc.name, kind: 'offline', fails: [], perf: [], info: {} };
  const origin = await p.eval('location.origin');
  // every request of the page and its workers, from here on: the browser's TeX asks from its worker
  const outside = [];
  let on = true, at = '';
  ctx.cdp.on('Network.requestWillBeSent', ({ request }) => {
    if (on && !request.url.startsWith(origin) && !/^(data|blob|about|chrome|devtools):/.test(request.url)) outside.push(`${at}: ${request.url}`);
  });
  await p.block(['*/__satex/compile*']);
  await p.s('Page.reload', {});
  await sleep(500);
  if (!await p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000)) { res.fails.push('the playground did not start again'); return res; }
  const names = sc.examples ?? await p.eval(`[...document.querySelectorAll('[data-example]')].map((e) => e.dataset.example)`);
  const d = p.drv, built = [];
  for (const name of names) {
    at = name;
    const t0 = Date.now();
    await p.eval(`document.querySelector('#play-exsect') && (document.querySelector('#play-exsect').open = true); document.querySelector('[data-example=${J(name)}]').click()`);
    const opened = await p.until(`(() => { const h = satexHealth(); return h.example === ${J(name)} && h.shown !== null && !h.busy && !h.running && !h.due && !h.loading.length; })()`, sc.ms ?? 300000);
    if (!opened) { res.fails.push(`${name}: its findings did not show (${(await d.run({ health: true }))?.status})`); continue; }
    await d.act({ press: { sel: '.side-tabs [data-panel=play-pdf]' } });
    await sleep(200);
    await d.until('built', sc.ms ?? 300000);
    await d.act({ mark: 'builds' });
    await d.act({ press: { sel: '#pdf-compile' } });
    await sleep(400);
    await d.until(['built', 'newBuild'], sc.ms ?? 300000);
    const { status } = await d.run({ buildState: true });
    // the Commands view names each host a file came from
    const said = await p.eval(`document.querySelector('#play-pdf [data-view=commands]')?.textContent ?? ''`);
    const hosts = [...said.matchAll(/newly fetched from ([^.]+)\./g)].map((m) => m[1]).filter((h) => h !== 'this site');
    if (hosts.length) res.fails.push(`${name}: its build fetched files from ${hosts.join(', ')}`);
    built.push(`${name} ${Math.round((Date.now() - t0) / 1000)} s: ${status}`);
  }
  on = false;
  const hosts = [...new Set(outside.map((u) => new URL(u.slice(u.indexOf(': ') + 2)).host))];
  if (outside.length) res.fails.push(`${outside.length} requests left the site for ${hosts.join(', ')}: ${[...new Set(outside.map((u) => u.replace(/ https?:\/\/[^/]*/, ' ')))].join(' · ')}`);
  res.info.built = built;
  res.shot = await p.shot(`${sc.name}.png`);
  return res;
}
