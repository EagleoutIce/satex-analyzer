// How a build's pages replace the shown ones while a person types (scenarios/swap.mjs). Every animation frame of the
// typing is sampled: the pages in view (image, opacity, the build it shows), their layers, the scroll place.
// What must hold in every frame, whatever the machine's load:
//   - no page in view without a drawn image, none dimmed (`blank`, `dim`)
//   - a page never shows an older build after a newer one (`back`)
//   - the pages do not scroll without a wheel or a key that moves them (`scroll`)
//   - a layer that showed before the typing and shows after it shows in every frame between (`layers`): its marks may
//     move with the text, they are never gone, hidden or dimmed
//   - the pages' elements are the same ones before and after (`rebuilt`): a build swaps images, it does not make the
//     pane anew
// Fields: `example`, `source` (the root's text), `layers` (switched on, the others off), `at` (the editor's caret:
// the first `at` in the text, `k` characters in), `keys` typed one by one with `ms` [min, max] between them,
// `via`: 'editor' (default) or 'pdf' (Type mode: a click at the end of page word `word`), `scrollTo` (a page brought
// into view before the typing), `follow: false` (Follow switched off), `keys`: also 'Pause' (the edits rest, a build starts) and 'Build' (waits until a build
// runs: the keys after it are typed while it does, so it lands on a newer text), `minBuilds` (builds that must land while the keys go, default 1).
// `E2E_DUMPFRAMES=1` writes the frames and a screenshot every 100 ms.
import { writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { sleep } from '../lib.mjs';
import { load, settled, ctx } from '../engine.mjs';

const J = JSON.stringify;

/** The in-page sampler: `self.__swap.frames`, one entry per animation frame while `on`. */
const SAMPLER = `(() => { if (self.__swap) return; const S = self.__swap = { on: false, frames: [], ids: new WeakMap(), n: 0 };
  const id = (o) => { if (!o) return 0; if (!S.ids.has(o)) S.ids.set(o, ++S.n); return S.ids.get(o); };
  const seen = (e) => e.checkVisibility({ visibilityProperty: true, opacityProperty: true });
  const tick = () => {
    if (S.on) {
      const sc = document.querySelector('#play-pdf .pdf-pages'), view = sc?.getBoundingClientRect();
      const pages = !sc ? [] : [...sc.querySelectorAll('.pdf-page')].filter((e) => { const r = e.getBoundingClientRect(); return !e.hidden && r.bottom > view.top + 8 && r.top < view.bottom - 8; }).map((e) => {
        const img = e.querySelector(':scope > img:not(.pdf-fade)'), layers = {};
        for (const o of e.querySelectorAll('.pdf-ov')) { const c = o.classList[1]; if (!c) continue; const l = layers[c] ??= [0, 0, 1]; l[0]++; if (seen(o)) l[1]++; l[2] = Math.min(l[2], +getComputedStyle(o).opacity); }
        const drawn = !!img && img.complete && img.naturalWidth > 0 && !/^data:image\\/svg/.test(img.currentSrc || img.src);
        return { n: +e.dataset.page, el: id(e), drawn, op: img ? +getComputedStyle(img).opacity : 0, gen: img?.__build?.gen ?? 0, layers };
      });
      S.frames.push({ t: performance.now(), box: id(sc), scroll: sc ? Math.round(sc.scrollTop) : null, count: sc ? sc.querySelectorAll('.pdf-page').length : 0, pages,
        running: !!document.querySelector('#play-pdf.pdf-running'), echo: document.querySelectorAll('.pdf-echo').length });
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
})()`;

/** What the frames break: a list of failures, each with its count and first frame. */
export function judge(frames, sc) {
  const fails = [], hit = (what, list, show) => { if (list.length) fails.push(`${what} in ${list.length} of ${frames.length} frames (first at ${Math.round(list[0].t - frames[0].t)} ms: ${J(show(list[0])).slice(0, 220)})`); };
  hit('no pages', frames.filter((f) => !f.pages.length), (f) => ({ count: f.count }));
  hit('a page in view not drawn', frames.filter((f) => f.pages.some((p) => !p.drawn)), (f) => f.pages.filter((p) => !p.drawn).map((p) => p.n));
  hit('a page in view dimmed', frames.filter((f) => f.pages.some((p) => p.drawn && p.op < 0.99)), (f) => f.pages.filter((p) => p.op < 0.99).map((p) => [p.n, p.op]));
  // per page the build shown never goes back
  const last = new Map(), back = [];
  for (const f of frames) for (const p of f.pages) { if (p.gen < (last.get(p.n) ?? 0)) back.push({ t: f.t, n: p.n, from: last.get(p.n), to: p.gen }); else last.set(p.n, p.gen); }
  hit('an older build shown after a newer one', back, (b) => b);
  if (!sc.scrolls) hit('the pages scrolled', frames.filter((f, i) => i && f.scroll !== frames[i - 1].scroll), (f) => ({ to: f.scroll, from: frames[frames.indexOf(f) - 1].scroll }));
  // a layer shown at both ends shows in every frame between, on the pages in view
  const shown = (f, c) => f.pages.reduce((s, p) => s + (p.layers[c]?.[1] ?? 0), 0), dimmest = (f, c) => Math.min(1, ...f.pages.filter((p) => p.layers[c]).map((p) => p.layers[c][2]));
  const first = frames[0], end = frames.at(-1), classes = Object.keys(Object.assign({}, ...first.pages.map((p) => p.layers))).filter((c) => shown(first, c) && shown(end, c));
  for (const c of classes) {
    hit(`layer ${c} gone`, frames.filter((f) => !shown(f, c)), () => ({ before: shown(first, c), after: shown(end, c) }));
    hit(`layer ${c} dimmed`, frames.filter((f) => shown(f, c) && dimmest(f, c) < 0.99), (f) => ({ opacity: dimmest(f, c) }));
  }
  for (const c of sc.layerClasses ?? []) if (!classes.includes(c)) fails.push(`layer ${c} not shown ${shown(first, c) ? 'after' : 'before'} the typing (before ${shown(first, c)}, after ${shown(end, c)})`);
  const boxes = new Set(frames.map((f) => f.box)), els = new Map();
  for (const f of frames) for (const p of f.pages) (els.get(p.n) ?? els.set(p.n, new Set()).get(p.n)).add(p.el);
  const remade = [...els].filter(([, s]) => s.size > 1).map(([n, s]) => `page ${n}: ${s.size} elements`);
  if (boxes.size > 1 || remade.length) fails.push(`the pages were made anew (${boxes.size} page lists; ${remade.join(', ') || 'no page twice'})`);
  return fails;
}

export async function swap(p, sc) {
  const res = { name: sc.name, kind: 'swap', fails: [], perf: [], info: {} }, d = p.drv;
  await load(p, { example: sc.example, source: sc.source, settle: true, word: sc.word });
  if (sc.via !== 'pdf') await d.act({ toggle: { sel: '#pdf-type', want: false } });
  // `follow: false`: the pages do not go to the editor's cursor when it changes its line (a typed line wrapped)
  if (sc.follow === false) await d.act({ toggle: { sel: '#pdf-follow', want: false } });
  // the layers: those named on, every other off
  for (const k of await p.eval(`[...document.querySelectorAll('#pdf-layer-list input[data-layer]')].map((i) => i.dataset.layer)`)) await d.act({ check: { sel: `#pdf-layer-${k}` }, want: (sc.layers ?? []).includes(k) });
  if (!await settled(p)) throw new Error('the page never settled before the typing');
  await d.until('oneBuild', 30000);
  await sleep(1500);
  if (sc.via === 'pdf') {
    const b = await d.word({ ...sc.word, scroll: [80, 200] });
    if (!b) throw new Error(`no word ${J(sc.word)}`);
    await p.mouse(b[2] - 0.5, (b[1] + b[3]) / 2);
    if (!await d.until('caret', 20000)) throw new Error('click: no caret');
    await sleep(600);
  } else {
    const at = await d.point({ src: sc.at, k: sc.k ?? sc.at.length });
    if (!at) throw new Error(`no ${J(sc.at)} in the editor`);
    await p.mouse(...at);
    await d.act({ caret: { src: sc.at, k: sc.k ?? sc.at.length } });
    await sleep(300);
  }
  // a page scrolled to after the caret is placed (the pages follow the editor's cursor when it moves)
  if (sc.scrollTo) { await p.eval(`document.querySelector('#play-pdf .pdf-page[data-page="${sc.scrollTo}"]').scrollIntoView({ block: 'start' })`); await sleep(800); }
  await p.eval(SAMPLER);
  const gen0 = await p.eval(`Math.max(0, ...[...document.querySelectorAll('#play-pdf .pdf-page img')].map((i) => i.__build?.gen ?? 0))`);
  const dump = process.env.E2E_DUMPFRAMES ? join(ctx.out, `${sc.name}-frames`) : null;
  let shots = 0, shooting = !!dump;
  if (dump) mkdirSync(dump, { recursive: true });
  const t0 = Date.now();
  const film = dump ? (async () => { while (shooting) { const t = Date.now(); await p.shot(join(`${sc.name}-frames`, `${String(shots++).padStart(4, '0')}-${String(t - t0).padStart(6, '0')}.png`)).catch(() => {}); await sleep(Math.max(0, 100 - (Date.now() - t))); } })() : null;
  await p.eval('self.__swap.on = true');
  await sleep(300);
  let seed = 4711;
  const rand = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648, [lo, hi] = sc.ms ?? [60, 160];
  for (const k of sc.keys) {
    if (k === 'Pause') { await sleep(sc.pauseMs ?? 1500); continue; }
    // the next keys go while a build of the text so far runs: it lands under them
    if (k === 'Build') { await p.until(`!!document.querySelector('#play-pdf.pdf-running')`, 20000); continue; }
    for (const ch of /^(Enter|Backspace)$/.test(k) ? [k] : [...k]) { await p.key(ch); await sleep(lo + rand() * (hi - lo)); }
  }
  // the last build lands and draws
  await sleep(500);
  const done = await settled(p, 120000) && await d.until(['built', 'oneBuild'], 60000);
  await sleep(1200);
  await p.eval('self.__swap.on = false');
  shooting = false;
  await film;
  const frames = await p.eval('self.__swap.frames');
  if (dump) writeFileSync(join(dump, 'frames.json'), J(frames));
  if (!done) res.fails.push('the last build never landed');
  if (!frames.length) { res.fails.push('no frame sampled'); return res; }
  const gens = [...new Set(frames.flatMap((f) => f.pages.map((q) => q.gen)))].sort((a, b) => a - b);
  res.info = { frames: frames.length, ms: Math.round(frames.at(-1).t - frames[0].t), builds: gens, shots };
  const landed = gens.filter((g) => g > gen0).length;
  if (landed < (sc.minBuilds ?? 1)) res.fails.push(`${landed} builds landed while typing, want ${sc.minBuilds ?? 1}`);
  res.fails.push(...judge(frames, sc));
  if (res.fails.length) res.shot = await p.shot(`${sc.name}.png`).catch(() => undefined);
  return res;
}
