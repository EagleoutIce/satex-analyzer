// A book in the playground (scenarios/big.mjs): a generated document of several hundred pages is opened, typed in,
// built, scrolled, and a build is dropped. What must hold whatever the machine's load (`fails`): the editor takes keys
// while the analysis runs, the build lands, layers and page images exist only near the view, the page in view is
// drawn after a scroll, a dropped build frees the server for the next one. What is timed (`perf`, failures in the perf
// tier only): the longest frame while typing, landing and scrolling, a key's time to its frame.
// Scenario fields: `pages` (about that many), `near` (pages that may hold layers or images at once), the bounds in ms:
// `key`, `frame` (perf), `stall` (a frame this long fails anywhere), `drop` (the small build after a dropped big one).
import { sleep } from '../lib.mjs';

/** A document of about `pages` pages: sections of generated paragraphs, each with a label and a reference to the last. */
export function book(pages) {
  const words = 'the page line word text block field rule mark note part form list item unit step case line code name type'.split(' ');
  let seed = 7;
  const next = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
  const para = () => { const n = 70 + Math.floor(next() * 40), w = []; for (let i = 0; i < n; i++) w.push(words[Math.floor(next() * words.length)]); const s = w.join(' '); return `${s[0].toUpperCase()}${s.slice(1)}.`; };
  // about six paragraphs to a page of the article class
  const sections = Math.ceil(pages / 2), out = ['\\documentclass{article}', '\\begin{document}', '\\tableofcontents'];
  for (let s = 1; s <= sections; s++) {
    out.push(`\\section{Part ${s}}\\label{sec:p${s}}`);
    for (let k = 0; k < 12; k++) out.push(`${k === 0 && s > 1 ? `After Section~\\ref{sec:p${s - 1}}. ` : ''}${para()}`, '');
  }
  out.push('\\end{document}', '');
  return out.join('\n');
}

const WATCH = `(() => { if (self.__big) return; const B = self.__big = { gaps: [], long: [] };
  new PerformanceObserver((l) => { for (const e of l.getEntries()) B.long.push([e.startTime, e.duration]); }).observe({ type: 'longtask', buffered: true });
  let last = performance.now(); const tick = (t) => { if (t - last > 34) B.gaps.push([last, t - last]); last = t; requestAnimationFrame(tick); }; requestAnimationFrame(tick); })()`;

export async function big(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const d = p.drv, now = () => p.eval('performance.now()');
  /** The longest frame between `t0` and now. */
  const frame = async (t0) => Math.round(Math.max(0, ...(await p.eval('self.__big.gaps')).filter(([t]) => t >= t0).map(([, ms]) => ms)));
  const bound = (what, ms, { perf = sc.frame, hard = sc.stall } = {}) => {
    res.info[what] = ms;
    if (ms > hard) res.fails.push(`${what}: ${ms} ms > ${hard} ms`); else if (ms > perf) res.perf.push(`${what}: ${ms} ms > ${perf} ms`);
  };
  await p.until('self.satexHealth && satexHealth().text?.length > 10', 60000);
  await d.until('quiet', 240000, 500);
  await p.eval(WATCH);
  const text = book(sc.pages);
  res.info.chars = text.length;
  // the text set, keys typed while its analysis runs: each shows within a frame or two, none waits for the run
  let t0 = await now();
  await d.act({ text });
  await sleep(300);
  await d.act({ caret: { src: '\\tableofcontents', k: 0 } });
  const keys = [];
  for (const ch of 'A typed line. ') { const k0 = Date.now(); await p.key(ch); await p.eval('new Promise((ok) => requestAnimationFrame(() => ok(1)))'); keys.push(Date.now() - k0); await sleep(80); }
  bound('key while analyzing', Math.max(...keys), { perf: sc.key });
  if (!(await d.text()).includes('A typed line. ')) res.fails.push('the keys typed while the analysis ran are not in the editor');
  bound('frame while analyzing', await frame(t0));
  const server = await p.eval(`fetch('/__satex/compile').then((r) => r.json()).then((j) => !!j.compile).catch(() => false)`);
  res.info.server = server;
  if (server) {
    // the build lands: the pages, layers and images only near the view
    await d.act({ press: { sel: '.side-tabs [data-panel=play-pdf]' } });
    await d.act({ mark: 'builds' });
    t0 = await now();
    await d.act({ press: { sel: '#pdf-compile' } });
    await sleep(500);
    if (!(await d.until(['built', 'newBuild'], sc.build ?? 300000))) res.fails.push('the build did not land');
    await sleep(2500);
    bound('frame while the build lands', await frame(t0));
    const count = () => p.eval(`(() => { const pages = [...document.querySelectorAll('.pdf-page')]; return { pages: pages.length,
      layered: pages.filter((e) => e.querySelector('.pdf-ov')).length, drawn: pages.filter((e) => e.querySelector('img.pdf-drawn')).length, nodes: document.querySelectorAll('.pdf-pages *').length }; })()`);
    let n = await count();
    Object.assign(res.info, n);
    if (n.pages < sc.pages / 2) res.fails.push(`the build has ${n.pages} pages, the document about ${sc.pages}`);
    if (n.layered > sc.near || n.drawn > sc.near) res.fails.push(`${n.layered} pages hold layers and ${n.drawn} an image, at most ${sc.near} are near the view`);
    // a scroll through the pages: no long frame, then the page in view is drawn
    t0 = await now();
    for (let i = 0; i < 40; i++) { await p.eval(`document.querySelector('.pdf-pages').scrollTop += 700`); await sleep(50); }
    bound('frame while scrolling', await frame(t0));
    const seen = await p.until(`(() => { const v = document.querySelector('.pdf-pages').getBoundingClientRect(), mid = (v.top + v.bottom) / 2;
      const pg = [...document.querySelectorAll('.pdf-page')].find((e) => { const r = e.getBoundingClientRect(); return r.top <= mid && r.bottom >= mid; });
      return pg?.querySelector('img.pdf-drawn') ? +pg.dataset.page : 0; })()`, 15000);
    if (!seen) res.fails.push('the page in view after the scroll was not drawn within 15 s');
    res.info.seen = seen;
    n = await count();
    if (n.layered > sc.near) res.fails.push(`after the scroll ${n.layered} pages hold layers, at most ${sc.near} are near the view`);
    // a build dropped (its request ended, as a reload or a closed tab ends it) stops on the server: the next build,
    // of a small document, does not wait for it. The dropped one would run for minutes (a long loop).
    const drop = await p.eval(`(async () => {
      const ask = (main, text, signal) => fetch('/__satex/compile', { method: 'POST', signal, body: JSON.stringify({ main, files: { [main]: text }, sets: ['engine=pdftex'], pages: false }) });
      const stop = new AbortController(), id = Math.random();
      ask('long.tex', '\\\\documentclass{article}\\\\begin{document}\\\\count255=0 \\\\loop\\\\ifnum\\\\count255<2000000000 \\\\advance\\\\count255 1 \\\\repeat x' + id + '\\\\end{document}', stop.signal).catch(() => {});
      await new Promise((ok) => setTimeout(ok, 1500));
      stop.abort();
      const t = performance.now();
      const a = await (await ask('small.tex', '\\\\documentclass{article}\\\\begin{document}small ' + id + '\\\\end{document}')).json();
      return { ms: Math.round(performance.now() - t), pdf: !!a.pdf, error: a.error ?? null };
    })()`);
    res.info.drop = drop;
    if (!drop.pdf) res.fails.push(`the build after a dropped one gave no PDF (${drop.error})`);
    if (drop.ms > sc.drop) res.fails.push(`the build after a dropped one took ${drop.ms} ms > ${sc.drop} ms: the dropped build was not stopped`);
  }
  // the analysis of the whole document ends, and the page stayed alive through it
  const quiet = await d.until('quiet', sc.limit ?? 900000, 1000);
  if (!quiet) res.fails.push(`the analysis did not end within ${(sc.limit ?? 900000) / 1000} s`);
  res.info.ms = Date.now() - p.t0;
  res.info.long = await p.eval('({ n: self.__big.long.length, max: Math.round(Math.max(0, ...self.__big.long.map((x) => x[1]))) })');
  if (p.errors.length) res.fails.push(`page errors: ${p.errors.slice(0, 3).join(' | ')}`);
  return res;
}
