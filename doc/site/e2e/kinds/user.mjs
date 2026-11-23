// User journeys (userrun, 2026-11-21): what a person does in minutes, as a list of actions, with the invariants a
// person relies on checked after each: no page error, no step slower than `stepMs`, and at the end the asserts.
// Fields: `example` (default hello.tex), `source` (the root's text first), `pdf` (built, Type mode on; `word` a word
// target the build must show), `viewport` {width, height, mobile}, `dark`, `steps` (prose), `do`: actions, each an
// object with one verb:
//   { text } the root's text set at once · { editor: needle, before } a click into the editor after (with `before`, at)
//   the first `needle` · { type } keys typed one by one · { key } a named key · { chord: 'b', mods = 2 } Ctrl+key
//   { paste } text inserted as one paste · { pdf: word, at: 'end'|'start'|'mid', clicks = 1 } a click on a page word
//   (Type mode, after a build) · { ts: text } a click at the end of `text` in the Typeset view, Type mode on
//   { select: word } a mouse drag over a page word · { js: expr } the page's answer kept (`expect: [{ js: regex }]`)
//   · { watch: target, ms } the first text of `target` as it changes · { drag: word, dx, dy } a mouse drag from a page word
//   { build: true } Compile, wait · { settle: true } the checks done · { findingsWithin: ms } the edit's findings shown
//   in time · { wait: ms } · { shot: tag } · any other step of the drivers' vocabulary (page.mjs: { do: action },
//   { click: target }, { until: cond } …)
// Asserts (`expect`, at the end): { changed: true } the steps wrote, { has: regex } / { not: regex } on the root's text, { once: str } typed text once,
// { noFinding: regex }, { finding: regex }, { buildErrorsCovered: true } each build error's line has a finding there,
// { built: true } the last build has no error, { first: target, want } the first element's text, { selected: text } the editor's selection, { caretSync: true } the
// editor's caret where the PDF caret is.
import { sleep } from '../lib.mjs';
import { load, setText } from '../engine.mjs';

const J = JSON.stringify;

/** Compile pressed, the build ended: its status and its error records (Messages view) as [line, message]. */
async function build(p) {
  const d = p.drv;
  await d.act({ press: { sel: '.side-tabs [data-panel=play-pdf]' } });
  await sleep(200);
  await d.until('built', 180000);
  await d.act({ mark: 'builds' });
  await d.act({ press: { sel: '#pdf-compile' } });
  await sleep(400);
  await d.until(['built', 'newBuild'], 180000);
  await sleep(600);
  const { status } = await d.run({ buildState: true });
  await d.act({ press: { sel: '#pdf-views button, #pdf-views [data-view]', text: 'Messages' } });
  await sleep(400);
  const msgs = await d.run({ read: '#play-pdf' });
  await d.act({ press: { sel: '#pdf-views button, #pdf-views [data-view]', text: 'Pages' } });
  await sleep(300);
  const errors = [...msgs.matchAll(/^error\s+\S+\s+(.*?)\s+[\w.-]+\.tex:(\d+)\s*$/gm)].map((m) => [+m[2], m[1]]);
  return { status, errors };
}
/** The findings listed, as [line, text]. */
const findings = async (p) => (await p.drv.run({ texts: { sel: '#play-findings li:not(:has(li))' } })).map((t) => t.replace(/\s+/g, ' ').trim())
  .filter((t) => /\d+:\d+/.test(t)).map((t) => [+/(\d+):\d+(?!.*\d+:\d+)/.exec(t)[1], t.slice(0, 140)]);
/** A mouse drag from `a` to `b` in `n` moves. */
async function drag(p, [x, y], [x1, y1], n = 10) {
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y }); await sleep(200);
  await p.s('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
  for (let s = 1; s <= n; s++) { await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: x + (x1 - x) * s / n, y: y + (y1 - y) * s / n, buttons: 1 }); await sleep(30); }
  await p.s('Input.dispatchMouseEvent', { type: 'mouseReleased', x: x1, y: y1, button: 'left', clickCount: 1 });
}

export async function user(p, sc) {
  const res = { name: sc.name, kind: 'user', fails: [], perf: [], info: { steps: [] } };
  const fail = (m) => res.fails.push(m), d = p.drv;
  if (sc.viewport || sc.dark) {
    if (sc.viewport) await p.s('Emulation.setDeviceMetricsOverride', { deviceScaleFactor: 1, mobile: !!sc.viewport.mobile, ...sc.viewport });
    if (sc.dark) await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
    await p.s('Page.reload');
    await sleep(500);
    await d.until({ sel: '#play-status[data-done=ready]' }, 120000);
  }
  const ex = sc.example ?? 'hello.tex';
  if (sc.pdf) await load(p, { example: ex, source: sc.source, settle: true, word: sc.word });
  else {
    await d.act({ press: { sel: `[data-example="${ex}"]` } });
    await d.until([{ health: { example: ex } }, { srcMatch: '^[^]{6}' }], 30000);
    await sleep(300);
    if (sc.source != null) await setText(p, sc.source);
    await d.until('quiet', 300000, 1200);
  }
  let last = null;
  const before = await d.text();
  for (const [i, a] of (sc.do ?? []).entries()) {
    const t0 = Date.now(), errs0 = p.errors.length;
    try {
      if (a.text != null) await setText(p, a.text);
      else if (a.editor != null) {
        const t = await d.text(), k = t.indexOf(a.editor);
        if (k < 0) throw new Error(`no ${J(a.editor)} in the text`);
        const c = await d.target({ src: a.editor, k: a.before ? 0 : a.editor.length, fx: 1 });
        await p.mouse(c[0], c[1]); await sleep(150);
      } else if (a.type != null) await d.type(a.type, () => a.delay ?? 50);
      else if (a.key) await p.key(a.key);
      else if (a.chord) { await d.key(a.chord, { ctrl: (a.mods ?? 2) & 2, shift: (a.mods ?? 2) & 8 }); }
      else if (a.paste != null) await p.s('Input.insertText', { text: a.paste });
      else if (a.pdf) {
        const w = await d.word({ word: a.pdf, nth: a.nth ?? 0, scroll: [120, 200] });
        if (!w) throw new Error(`no page word ${J(a.pdf)}`);
        const x = a.at === 'start' ? w[0] + 0.6 : a.at === 'mid' ? (w[0] + w[2]) / 2 : w[2] - 0.6 * w[6], y = (w[1] + w[3]) / 2;
        for (let c = 1; c <= (a.clicks ?? 1); c++) await p.mouse(x, y, c);
        await sleep(400);
      } else if (a.ts) {
        // the Typeset view's Type mode on, a click at the end of the first text `a.ts` drawn there
        await d.act({ press: { sel: '.side-tabs [data-panel=play-typeset]' } }); await sleep(400);
        await d.act({ toggle: { sel: '#ts-type', want: true } }); await sleep(300);
        const c = await d.target({ range: '#play-typeset', text: a.ts, dx: -1 });
        await p.mouse(c[0], c[1]); await sleep(400);
      } else if (a.select) {
        const w = await d.word({ word: a.select, scroll: [120, 200] });
        if (!w) throw new Error(`no page word ${J(a.select)}`);
        const y = (w[1] + w[3]) / 2;
        await drag(p, [w[0] + 0.5, y], [w[2] - 0.5, y], 6);
        await sleep(300);
      } else if (a.drag) {
        const w = await d.word({ word: a.drag, scroll: [120, 200] });
        if (!w) throw new Error(`no page word ${J(a.drag)}`);
        const x = (w[0] + w[2]) / 2, y = (w[1] + w[3]) / 2;
        await drag(p, [x, y], [x + a.dx, y + a.dy]);
        await sleep(800);
      } else if (a.build) last = await build(p);
      else if (a.wait) await sleep(a.wait);
      else if (a.js) (res.info.js ??= []).push(await p.eval(a.js));
      else if (a.watch) {
        // the first item of a list as it changes for `ms`, sampled every frame: [ms since the step, text]
        const seen = res.info.watched = [];
        for (const t1 = Date.now(); Date.now() - t1 < (a.ms ?? 2000);) {
          const v = (await d.run({ texts: a.watch }))[0] ?? null;
          if (v !== (seen.at(-1)?.[1] ?? null)) seen.push([Date.now() - t1, v]);
          await sleep(16);
        }
      }
      else if (a.findingsWithin) {
        // the findings of the text as it is now shown within `findingsWithin` ms of the last key
        const ok = await d.until({ findingsCurrent: true }, 120000), ms = Date.now() - t0;
        res.info.findingsMs = ms;
        if (!ok || ms > a.findingsWithin) {
          const h = await d.health();
          fail(`step ${i + 1}: the findings of the edit came after ${ok ? ms : '>120000'} ms, want <= ${a.findingsWithin} (${J({ list: h.list, version: h.version, listStale: h.listStale, running: h.running, due: h.due, status: h.status })})`);
        }
      } else if (a.settle) { if (!await d.until('quiet', 120000, 1200)) fail(`step ${i + 1}: the checks never settled`); }
      else if (a.shot) await p.shot(`${sc.name}-${a.shot}.png`);
      else await d.step(a);
    } catch (e) { fail(`step ${i + 1} ${J(a).slice(0, 80)}: ${String(e?.message ?? e).slice(0, 200)}`); await p.shot(`${sc.name}-step${i + 1}.png`).catch(() => {}); return res; }
    await sleep(a.pause ?? 150);
    const ms = Date.now() - t0;
    if (!a.build && !a.settle && !a.wait && !a.watch && !a.findingsWithin && a.type == null && !a.until && ms > (sc.stepMs ?? 2000)) fail(`step ${i + 1} ${J(a).slice(0, 60)} took ${ms} ms`);
    if (p.errors.length > errs0) fail(`step ${i + 1} ${J(a).slice(0, 60)}: page error ${J(p.errors.slice(errs0)[0])}`);
  }
  const t = await d.text();
  res.info.text = t;
  for (const e of sc.expect ?? []) {
    if (e.has && !e.has.test(t)) fail(`the text lacks ${e.has}: ${J(t.slice(0, 300))}`);
    if (e.not && e.not.test(t)) fail(`the text has ${e.not}: ${J(t.match(e.not)[0])}`);
    if (e.changed && t === before) fail(`the text is unchanged after the steps (${t.length} chars, ${(t.match(/tikzpicture/g) ?? []).length} tikzpicture)`);
    // `echoNot`: no echo draw on the pages (the typing preview) spells this
    if (e.echoNot) { const ts = await p.eval(`[...document.querySelectorAll('#play-pdf .pdf-echo')].flatMap((x) => [x.textContent, ...(x.__texts ?? []).map((q) => q.t)]).join(' | ')`); if (e.echoNot.test(ts)) fail(`the echo draws ${J(ts.match(e.echoNot)[0])}: ${J(ts.slice(0, 200))}`); }
    if (e.once != null && t.split(e.once).length - 1 !== 1) fail(`${J(e.once)} is in the text ${t.split(e.once).length - 1} times`);
    if (e.finding || e.noFinding || e.buildErrorsCovered) {
      await d.until('quiet', 120000, 1200);
      await d.act({ press: { sel: '.side-tabs [data-panel=play-findings]' } });
      await sleep(400);
      const fs = await findings(p);
      res.info.findings = fs.map((f) => f[1]);
      if (e.finding && !fs.some((f) => e.finding.test(f[1]))) fail(`no finding ${e.finding}: ${J(fs.map((f) => f[1]))}`);
      if (e.noFinding) for (const f of fs) if (e.noFinding.test(f[1])) fail(`a finding ${J(f[1])}`);
      if (e.buildErrorsCovered) {
        last ??= await build(p);
        res.info.build = last;
        for (const [line, msg] of last.errors) if (!fs.some((f) => f[0] === line)) fail(`the build error ${J(msg)} on line ${line} has no finding there (findings ${J(fs.map((f) => f[1].slice(0, 60)))})`);
      }
    }
    if (e.built) { last ??= await build(p); if (last.errors.length || /error/.test(last.status)) fail(`the build fails: ${J(last.status)} ${J(last.errors)}`); }
    // `firstSeen` (a text or a regex): the list a `watch` step saw opened with it first and kept it there
    if (e.firstSeen) { const w = res.info.watched ?? [], shown = w.filter((x) => x[1] != null); const ok = (v) => e.firstSeen instanceof RegExp ? e.firstSeen.test(v) : v === e.firstSeen; if (!shown.length || shown.some((x) => !ok(x[1]))) fail(`the first item went ${J(w)}, want ${J(e.firstSeen)} from the start`); }
    // `js`: the last `js` step's answer matches
    if (e.js && !e.js.test(String(res.info.js?.at(-1)))) fail(`the page answered ${J(res.info.js?.at(-1))}, want ${e.js}`);
    if (e.first) { const v = (await d.run({ texts: e.first }))[0] ?? null; if (v !== e.want) fail(`the first ${J(e.first.sel)} is ${J(v)}, want ${J(e.want)}`); }
    if (e.selected != null) { const v = (await d.run({ carets: true })).selected; if (v !== e.selected) fail(`the selection is ${J(v)}, want ${J(e.selected)}`); }
    if (e.caretSync) {
      const c = await d.run({ carets: true });
      if (c.pdf != null && c.pdf !== c.editor) fail(`the editor caret at ${c.editor}, the PDF caret at ${c.pdf}`);
    }
  }
  if (res.fails.length) await p.shot(`${sc.name}-end.png`).catch(() => {});
  return res;
}
