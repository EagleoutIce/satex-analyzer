// The page actions' menus (doc/site/actions.ts) on the PDF and the Typeset view, built by the browser's pdfTeX (the
// compile server blocked): each action opens the menu at a node, a path or a label, follows its rows, checks the
// text, builds it and undoes it with one Ctrl+Z. The rewrites themselves are unit-tested (tests/actions.test.mjs:
// every row writes text that parses); here one action per menu kind and code path: a shape with and without a
// library, a submenu parameter, the mixer, an input box, picture options, delete/duplicate/add edge, tips, routes,
// labels, Type mode in a label, the rotate handle, drags, double click. Scenario field: `session` ('drawing' | 'thesis').
import { sleep } from '../lib.mjs';

const J = JSON.stringify;
async function open(p, example) {
  // the browser's pdfTeX: the machine's compile server blocked, the page loaded again
  await p.block(['*/__satex/compile*']);
  await p.s('Page.reload', { ignoreCache: false });
  await sleep(500);
  await p.until(`document.readyState === 'complete'`, 30000);
  if (!await p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000)) throw new Error('the playground did not start');
  await p.eval(`document.querySelector('[data-example=${J(example)}]').click()`);
  await p.until(`satexHealth().example === ${J(example)} && satexHealth().text?.length > 10`, 30000);
}
async function file(p, name) {
  await p.eval(`document.querySelector('#play-files-tree .file[data-name=${J(name)}]')?.click()`);
  if (!await p.until(`satexHealth().file === ${J(name)}`, 20000)) throw new Error(`${name} not opened`);
}
/** A build of the text now: Compile pressed, its pages drawn; the status line after it. */
async function build(p, ms = 180000) {
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await sleep(200);
  await p.until(`!document.querySelector('#play-pdf.pdf-running')`, ms);
  await p.eval(`(self.__b0 = document.querySelector('#play-pdf .pdf-page img')?.__build ?? null, document.getElementById('pdf-compile').click())`);
  const ok = await p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.naturalWidth > 100 && i.__build && (i.__build !== self.__b0 || /in cache/.test(document.getElementById('pdf-status').textContent)) && satexPdfWords().length && !document.querySelector('#play-pdf.pdf-running'); })()`, ms);
  await sleep(600);
  return { ok: !!ok, status: await p.eval(`document.getElementById('pdf-status')?.textContent ?? ''`) };
}
/** The screen box of the `nth` page word `word` (scrolled into view). */
async function word(p, w, nth = 0) {
  return p.eval(`(async () => {
    // a build landing replaces the pages: their words come a moment later, so the word is waited for
    for (let k = 0; k < 40 && !satexPdfWords().filter((x) => x[5] === ${J(w)})[${nth}]; k++) await new Promise((ok) => setTimeout(ok, 250));
    const ws = satexPdfWords().filter((x) => x[5] === ${J(w)}), x = ws[${nth}]; if (!x) { self.__missing = { want: ${J(w)}, words: satexPdfWords().map((x) => x[5]).join(' ').slice(0, 300), build: document.querySelector('#play-pdf .pdf-page img')?.__build?.text === satexHealth().text, status: document.getElementById('pdf-status')?.textContent }; return null; }
    const img = () => document.querySelector('#play-pdf .pdf-page[data-page="' + x[0] + '"] img'), sc = document.querySelector('#play-pdf .pdf-pages'), dpi = satexPdfWords().dpi;
    let r = img().getBoundingClientRect(), k = r.width / (img().naturalWidth * 72 / dpi);
    const s = sc.getBoundingClientRect(), y = r.top + x[2] * k;
    if (y < s.top + 120 || y > s.bottom - 200) { sc.scrollTop += y - (s.top + s.height * 0.4); await new Promise((ok) => setTimeout(ok, 500)); }
    r = img().getBoundingClientRect(); k = r.width / (img().naturalWidth * 72 / dpi);
    return { l: r.left + x[1] * k, t: r.top + x[2] * k, r: r.left + x[3] * k, b: r.top + x[4] * k, k };
  })()`);
}
const mid = (w) => w && [(w.l + w.r) / 2, (w.t + w.b) / 2];
async function rclick(p, [x, y]) {
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await sleep(120);
  for (const type of ['mousePressed', 'mouseReleased']) await p.s('Input.dispatchMouseEvent', { type, x, y, button: 'right', clickCount: 1 });
  return p.until(`document.querySelector('.pa-root') && [...document.querySelectorAll('.pa-root > button')].map((b) => b.querySelector('span').textContent)`, 4000);
}
/** Through the menu by its labels: each row hovered (a submenu opens), the last one clicked. */
async function choose(p, path) {
  for (let i = 0; i < path.length; i++) {
    const at = await p.eval(`(() => { const ms = [...document.querySelectorAll('.pa-menu')], m = ms.at(-1); const b = [...m.querySelectorAll(':scope > button')].find((b) => b.querySelector('span').textContent === ${J(path[i])}); if (!b) return { no: [...m.querySelectorAll(':scope > button')].map((b) => b.querySelector('span').textContent) }; const r = b.getBoundingClientRect(); return [r.left + Math.min(40, r.width / 2), r.top + r.height / 2]; })()`);
    if (!Array.isArray(at)) throw new Error(`no ${J(path[i])} in ${J(at?.no)}`);
    await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    await sleep(140);
    if (i === path.length - 1) for (const type of ['mousePressed', 'mouseReleased']) await p.s('Input.dispatchMouseEvent', { type, x: at[0], y: at[1], button: 'left', clickCount: 1 });
  }
}
async function typeIn(p, value) {
  if (!await p.until(`document.querySelector('.tikz-input input')`, 3000)) throw new Error('no input box');
  await p.eval(`(() => { const i = document.querySelector('.tikz-input input'); i.value = ${J(value)}; i.focus(); })()`);
  await p.key('Enter');
}
async function undo(p) {
  await p.eval(`document.querySelector('.cm-content').focus()`);
  await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'z', code: 'KeyZ', modifiers: 2, windowsVirtualKeyCode: 90 });
  await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'z', code: 'KeyZ', modifiers: 2, windowsVirtualKeyCode: 90 });
  await sleep(200);
}
const text = (p) => p.eval(`satexHealth().text`);
const toastNow = (p) => p.eval(`document.getElementById('play-toast')?.textContent ?? ''`);

/** One action: open the menu at `at()`, follow `path` (`then`: the rest, an input), check the text, build it, undo. */
async function act0(results, p, name, at, path, want, o = {}) {
  const r = { name, fails: [], info: {} };
  try {
    const t0 = await text(p), xy = await at();
    if (!xy) throw new Error(`no target on the page: ${J(await p.eval('self.__missing ?? null'))}`);
    const rows = await rclick(p, xy);
    if (!rows) throw new Error('no menu');
    r.info.rows = rows.join(' | ');
    if (o.shot) await p.shot(`${name}-menu.png`);
    await choose(p, path);
    if (o.shot) { await sleep(150); }
    if (o.then) await o.then();
    const t1 = await p.until(`satexHealth().text !== ${J(t0)} && satexHealth().text`, 8000);
    if (!t1) throw new Error(`text unchanged (toast: ${await toastNow(p)})`);
    if (!want.test(t1)) r.fails.push(`text: want ${want}, got ${J(t1.slice(Math.max(0, t1.search(/\\begin\{tikzpicture\}/)), t1.indexOf('\\end{tikzpicture}') + 17))}`);
    if (o.toast && !o.toast.test(await toastNow(p))) r.fails.push(`toast: want ${o.toast}, got ${J(await toastNow(p))}`);
    if (o.compile !== false) {
      const b = await build(p);
      r.info.status = b.status.slice(0, 100);
      if (!b.ok) r.fails.push(`no build of the new text (${b.status.slice(0, 120)})`);
      else if (/error|failed|stopped/i.test(b.status)) r.fails.push(`build: ${b.status.slice(0, 160)}`);
      if (o.shot) await p.shot(`${name}-after.png`);
    }
    await undo(p);
    if (await text(p) !== t0) r.fails.push('one Ctrl+Z did not give the text back');
    if (o.compile !== false && !o.keep) await build(p);
  } catch (e) { r.fails.push(String(e.message ?? e).slice(0, 300)); await p.shot(`${name}-fail.png`).catch(() => {}); }
  await p.eval(`document.querySelectorAll('.pa-menu, .tikz-input, .pa-mix').forEach((e) => e.remove())`);
  results.push(r);
  return r;
}


export async function tikz(p, sc) {
  const results = [], log = () => {};
  const act = (...a) => act0(results, ...a);
  if (sc.session === 'drawing') {
    await open(p, 'drawing.tex');
    const first = await build(p, 300000);
    if (!first.ok) throw new Error(`no first build (${first.status.slice(0, 120)})`);
    log('first build', first);
    const node = (w) => async () => mid(await word(p, w));
    // the path src -> run: between the two words; run -> out (bend left=30): above the chord's middle
    const between = (a, c, lift = 0) => async () => { const x = await word(p, a), y = await word(p, c); return [(x.r + y.l) / 2, (mid(x)[1] + mid(y)[1]) / 2 - lift * (y.l - x.r)]; };
    await act(p, 'node-shape-diamond', node('run'), ['Shape', 'Diamond'], /\\node\[diamond, box, right=of src\] \(run\)/, { shot: true });
    await act(p, 'node-shape-cloud-lib', node('run'), ['Shape', 'Cloud'], /\\usetikzlibrary\{arrows\.meta,positioning,backgrounds\}\n\\usetikzlibrary\{shapes\.symbols\}\n[^]*\\node\[cloud, box, right=of src\] \(run\)/);
    await act(p, 'node-shape-polygon', node('run'), ['Shape', 'Regular polygon', '6 sides'], /\\node\[regular polygon, regular polygon sides=6, box, right=of src\] \(run\)/);
    await act(p, 'node-style-drawcolor', node('run'), ['Style', 'Draw color', 'red'], /\\node\[box, right=of src, draw=red\] \(run\)/, { shot: true });
    await act(p, 'node-style-fillmix', node('run'), ['Style', 'Fill color', 'Mix…'], /\\node\[box, right=of src, fill=red!30\] \(run\)/, { then: async () => {
      if (!await p.until(`document.querySelector('.pa-mix')`, 3000)) throw new Error('no mixer');
      await p.eval(`(() => { const m = document.querySelector('.pa-mix'); m.querySelector('[name=a]').value = 'red'; const r = m.querySelector('[name=p]'); r.value = 30; m.dispatchEvent(new Event('input')); })()`);
      await p.shot('node-style-fillmix-mixer.png');
      await p.eval(`document.querySelector('.pa-mix button').click()`);
    } });
    await act(p, 'node-text-edit', node('run'), ['Edit text…'], /\(run\) \{check\}/, { then: () => typeIn(p, 'check') });
    await act(p, 'pic-rotate', node('run'), ['Position', 'Rotate', 'Rotate the picture', '90°'], /\\begin\{tikzpicture\}\[node distance=1\.2cm, hot\/\.style=\{fill=red!20\}, rotate=90\]/);
    await act(p, 'node-delete', node('one'), ['Arrange', 'Delete with its paths'], /\\node\[box, hot, right=of run\] \(out\) \{findings\};\n  \\draw\[flow\] \(src\) -- \(run\);\n  \\draw\[flow\] \(run\) to\[bend left=30\] \(out\);\n\\end\{tikzpicture\}/);
    await act(p, 'node-duplicate', node('findings'), ['Arrange', 'Duplicate'], /\\node\[box, hot, right=of run\] \(out\) \{findings\};\n  \\node\[box, hot, below=of out\] \(out2\) \{findings\};/);
    await act(p, 'node-add-edge', node('one'), ['Arrange', 'Add edge from this node…'], /\\draw\[->\] \(note\) -- \(out\);\n\\end\{tikzpicture\}/, { then: async () => {
      await sleep(200);
      const [x, y] = mid(await word(p, 'findings'));
      await p.mouse(x, y);
    } });
    // paths
    await act(p, 'edge-tip-both', between('source', 'run'), ['Arrow tips', '↔'], /\\draw\[flow, <->\] \(src\) -- \(run\);/, { shot: true });
    // each end on its own, the tips drawn by TeX beside their names; the end `flow` brings stays
    await act(p, 'edge-tip-start', between('source', 'run'), ['Arrow tips', 'Start', 'Kite'], /\\draw\[flow, Kite-Stealth\] \(src\) -- \(run\);/, { shot: true });
    {
      const r = { name: 'edge-tip-previews', fails: [], info: {} };
      try {
        if (!await rclick(p, await between('source', 'run')())) throw new Error('no menu');
        for (const l of ['Arrow tips', 'End']) {
          const at = await p.eval(`(() => { const b = [...document.querySelectorAll('.pa-menu')].at(-1).querySelectorAll(':scope > button'); const x = [...b].find((e) => e.querySelector('span').textContent === ${J(l)}); if (!x) return null; const q = x.getBoundingClientRect(); return [q.left + 30, q.top + q.height / 2]; })()`);
          if (!at) throw new Error(`no ${l}`);
          await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
          await sleep(160);
        }
        const got = await p.eval(`(() => { const m = [...document.querySelectorAll('.pa-menu')].at(-1); return { rows: m.querySelectorAll(':scope > button').length, imgs: [...m.querySelectorAll('img.pa-img')].filter((i) => i.naturalWidth > 0).length, heads: [...m.querySelectorAll(':scope > b')].map((b) => b.textContent), on: [...m.querySelectorAll('[aria-checked=true] span')].map((s) => s.textContent) }; })()`);
        r.info = got;
        await p.shot('edge-tip-previews.png');
        if (got.rows < 25) r.fails.push(`only ${got.rows} tips`);
        if (got.imgs < 15) r.fails.push(`only ${got.imgs} pictures`);
        if (!got.heads.includes('arrows.meta')) r.fails.push(`heads ${J(got.heads)}`);
        // `flow` brings -Stealth: its end is ticked
        if (!got.on.includes('Stealth')) r.fails.push(`ticked ${J(got.on)}, want Stealth from the style flow`);
      } catch (e) { r.fails.push(String(e.message)); }
      await p.eval(`document.querySelectorAll('.pa-menu').forEach((e) => e.remove())`);
      results.push(r);
    }
    // bend points: a drag on the path adds one (its glow and grip on hover), a right click on it removes it or makes a curve
    {
      const r = { name: 'bend-drag', fails: [], info: {} };
      try {
        const t0 = await text(p), [x, y] = await between('source', 'run')();
        await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
        if (!await p.until(`document.querySelector('#play-pdf svg.pdf-tikz-path polyline')`, 3000)) r.fails.push('no glow on hover');
        await p.s('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
        for (let k = 1; k <= 6; k++) { await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y: y - 5 * k, button: 'left', buttons: 1 }); await sleep(30); }
        await p.shot('bend-drag-ghost.png');
        await p.s('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y: y - 30, button: 'left', clickCount: 1 });
        const t1 = await p.until(`satexHealth().text !== ${J(t0)} && satexHealth().text`, 5000);
        r.info.wrote = (t1 || '').match(/\\draw\[flow\] \(src\)[^;]*;/)?.[0];
        if (!/\\draw\[flow\] \(src\) -- \(-?[\d.]+cm,[\d.]+cm\) -- \(run\);/.test(t1 || '')) r.fails.push(`text: ${J(r.info.wrote)}`);
        const b = await build(p);
        if (!b.ok || /error|failed|stopped/i.test(b.status)) r.fails.push(`build: ${b.status.slice(0, 120)}`);
        // the run after the build places the boxes again: wait for its words; the page may have moved, so the bend
        // point is found again from the words (30 px above the middle between them, where it was dropped)
        await p.until(`satexPdfWords().length > 3`, 20000);
        await sleep(1500);
        const bendAt = async () => { await p.until(`satexPdfWords().length > 3`, 20000); const [bx, by] = await between('source', 'run')(); return [bx, by - 30]; }, [bx, by] = await bendAt();
        await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: bx + 200, y: by + 200 });
        await sleep(100);
        await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: bx, y: by });
        if (!await p.until(`document.querySelector('#play-pdf .pdf-tikz-bend')`, 3000)) r.fails.push('no grip on the bend point');
        await p.shot('bend-drag-after.png');
        if (t1) {
          await act(p, 'bend-remove', bendAt, ['Remove bend point'], /\\draw\[flow\] \(src\) -- \(run\);/, { compile: false });
          await act(p, 'bend-curve', bendAt, ['Convert to curve'], /\\draw\[flow\] \(src\) to\[out=\d+, in=\d+\] \(run\);/, { shot: true });
          await undo(p);
          if (await text(p) !== t0) r.fails.push('one Ctrl+Z did not take the bend point back');
          await build(p);
        }
      } catch (e) { r.fails.push(String(e.message)); await p.shot('bend-drag-fail.png').catch(() => {}); }
      results.push(r);
    }
    // the picture's grid from a node's menu: lines in its units, axes labelled at the origin; again it goes
    {
      const r = { name: 'picture-grid', fails: [], info: {} };
      try {
        await p.until(`satexPdfWords().length > 3`, 20000);
        const t0 = await text(p), node = mid(await word(p, 'run'));
        if (!await rclick(p, node)) throw new Error('no menu');
        await choose(p, ['Show grid']);
        const got = await p.until(`(() => { const g = document.querySelector('#play-pdf svg.pdf-tikz-grid'); return g && { lines: g.querySelectorAll('line').length, axes: g.querySelectorAll('.g-axis').length, labels: [...g.querySelectorAll('text')].map((t) => t.textContent) }; })()`, 3000);
        await p.shot('picture-grid.png');
        if (!got) throw new Error('no grid drawn');
        r.info = got;
        if (got.lines < 8 || got.axes !== 2 || !got.labels.includes('0')) r.fails.push(`grid ${JSON.stringify(got)}`);
        if (await text(p) !== t0) r.fails.push('the grid wrote text');
        // a line typed above the picture moves its offset: the grid stays with the picture
        await p.eval(`document.querySelector('.cm-content').focus()`);
        await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Home', code: 'Home', modifiers: 2, windowsVirtualKeyCode: 36 });
        await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Home', code: 'Home', modifiers: 2, windowsVirtualKeyCode: 36 });
        await p.s('Input.insertText', { text: '% moved\n' });
        await build(p);
        if (!await p.until(`document.querySelector('#play-pdf svg.pdf-tikz-grid')`, 5000)) r.fails.push('the grid went when a line was typed above the picture');
        await undo(p);
        await build(p);
        const node2 = mid(await word(p, 'run')) ?? node;
        await rclick(p, node2);
        await choose(p, ['Show grid']);
        if (!await p.until(`!document.querySelector('#play-pdf svg.pdf-tikz-grid')`, 3000)) r.fails.push('the grid stayed after switching it off');
      } catch (e) { r.fails.push(String(e.message)); await p.shot('picture-grid-fail.png').catch(() => {}); }
      await p.eval(`document.querySelectorAll('.pa-menu').forEach((e) => e.remove())`);
      results.push(r);
    }
    await act(p, 'edge-bend', between('source', 'run'), ['Route', 'Bend right', '45°'], /\\draw\[flow\] \(src\) to\[bend right=45\] \(run\);/);
    await act(p, 'edge-right-angle', between('run', 'findings', 0.147), ['Route', 'Right angle, vertical first (|-)'], /\\draw\[flow\] \(run\) \|- \(out\);/);
    await act(p, 'edge-inout', between('source', 'run'), ['Route', 'Out and in angles…'], /\\draw\[flow\] \(src\) to\[out=45, in=135\] \(run\);/, { then: () => typeIn(p, '45, 135') });
    // a label added, then its own menu, then typed into
    await act(p, 'edge-label-add', between('source', 'run'), ['Label', 'Add label, above…'], /\\draw\[flow\] \(src\) -- node\[midway, above\] \{feeds\} \(run\);/, { then: () => typeIn(p, 'feeds') });
    // put the label back for the label menu
    const noLabel = await text(p), withLabel = noLabel.replace('\\draw[flow] (src) -- (run);', '\\draw[flow] (src) -- node[midway, above] {feeds} (run);');
    await p.eval(`document.querySelector('.cm-content').focus()`);
    await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
    await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
    await p.s('Input.insertText', { text: withLabel });
    await build(p);
    await act(p, 'label-pos', node('feeds'), ['Position', 'near end'], /node\[above, near end\] \{feeds\}/, { shot: true });
    await act(p, 'label-delete', node('feeds'), ['Delete label'], /\\draw\[flow\] \(src\) -- \(run\);/);
    // typing into the label in Type mode: the caret at its end, a letter typed
    {
      const r = { name: 'label-type', fails: [], info: {} };
      try {
        const w = await word(p, 'feeds'), t0 = await text(p);
        await p.mouse(w.r - 1, (w.t + w.b) / 2);
        await sleep(300);
        await p.key('X');
        const t1 = await p.until(`satexHealth().text !== ${J(t0)} && satexHealth().text`, 5000);
        if (!t1 || !/\{feedsX\}/.test(t1)) r.fails.push(`typed: ${J((t1 || '').match(/node\[[^\]]*\] \{[^}]*\}/)?.[0])} (toast ${await toastNow(p)})`);
        await p.key('Escape');
        await undo(p);
      } catch (e) { r.fails.push(String(e.message)); }
      log(`${r.fails.length ? 'FAIL' : 'pass'} ${r.name} ${J(r.info)}${r.fails.map((f) => `\n    ${f}`).join('')}`);
      results.push(r);
    }
    // a write that leaves a path with the keys of a document style names the style: `-Stealth` there, `thick` picked, `flow`
    {
      await p.eval(`document.querySelector('.cm-content').focus()`);
      await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
      await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
      await p.s('Input.insertText', { text: noLabel.replace('\\draw[dashed] (note) -- (run);', '\\draw[dashed, -Stealth] (note) -- (run);') });
      await build(p);
      const down = async () => { const a = await word(p, 'run'), b = await word(p, 'pass'); return a && b && [(a.l + a.r) / 2, (a.b + b.t) / 2]; };
      await act(p, 'style-prefer', down, ['Style', 'Line width', 'thick (0.8pt)'], /\\draw\[dashed, flow\] \(note\) -- \(run\);/, { compile: false });
    }
    // the label taken out again: the paths below are clicked where it stood
    {
      await p.eval(`document.querySelector('.cm-content').focus()`);
      await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
      await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65 });
      await p.s('Input.insertText', { text: noLabel });
      await build(p);
    }
    // the rotate handle: pressed above the node, dragged to its left (90°)
    {
      const r = { name: 'node-rotate-handle', fails: [], info: {} };
      try {
        await build(p);
        const t0 = await text(p), [x, y] = mid(await word(p, 'run'));
        await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
        const h = await p.until(`(() => { const e = document.querySelector('#play-pdf .pdf-tikz-rot'); if (!e) return null; const q = e.getBoundingClientRect(); return [q.left + q.width / 2, q.top + q.height / 2]; })()`, 3000);
        if (!h) throw new Error('no rotate handle');
        await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: h[0], y: h[1] });
        await p.s('Input.dispatchMouseEvent', { type: 'mousePressed', x: h[0], y: h[1], button: 'left', clickCount: 1 });
        for (let k = 1; k <= 8; k++) { const a = Math.PI / 2 * (1 + k / 8); await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: x + 40 * Math.cos(a) + 0.1, y: y - 40 * Math.sin(a), button: 'left', buttons: 1 }); await sleep(30); }
        await p.shot('node-rotate-handle-drag.png');
        await p.s('Input.dispatchMouseEvent', { type: 'mouseReleased', x: x - 40, y, button: 'left', clickCount: 1 });
        const t1 = await p.until(`satexHealth().text !== ${J(t0)} && satexHealth().text`, 5000);
        if (!t1 || !/\\node\[box, right=of src, rotate=180\] \(run\)|\\node\[box, right=of src, rotate=90\] \(run\)/.test(t1)) r.fails.push(`text: ${J((t1 || '').match(/\\node[^;]*\(run\)/)?.[0])}`);
        r.info.wrote = (t1 || '').match(/\\node[^;]*\(run\)/)?.[0];
        await undo(p);
        await build(p);
      } catch (e) { r.fails.push(String(e.message)); }
      log(`${r.fails.length ? 'FAIL' : 'pass'} ${r.name} ${r.info.wrote ?? ''}${r.fails.map((f) => `\n    ${f}`).join('')}`);
      results.push(r);
    }

    // drags: relative by default (beside the nearest node set before), Ctrl a fixed place; a double click edits the text
    const drag = async (name, w, dx, dy, want, mods = 0) => {
      const r = { name, fails: [], info: {} };
      try {
        await build(p);
        const t0 = await text(p), b0 = await word(p, w), x = b0.l - 6, y = (b0.t + b0.b) / 2;
        await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, modifiers: mods });
        await sleep(150);
        await p.s('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1, modifiers: mods });
        for (let k = 1; k <= 10; k++) { await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: x + dx * k / 10, y: y + dy * k / 10, button: 'left', buttons: 1, modifiers: mods }); await sleep(30); }
        await p.shot(`${name}-ghost.png`);
        await p.s('Input.dispatchMouseEvent', { type: 'mouseReleased', x: x + dx, y: y + dy, button: 'left', clickCount: 1, modifiers: mods });
        const t1 = await p.until(`satexHealth().text !== ${J(t0)} && satexHealth().text`, 6000);
        r.info.wrote = (t1 || '').match(/\\node[^;]*\(note\)[^;]*;/)?.[0];
        if (!t1 || !want.test(t1)) r.fails.push(`text: ${J(r.info.wrote)} (toast ${await toastNow(p)})`);
        else { const b = await build(p); if (!b.ok || /error/i.test(b.status)) r.fails.push(`build: ${b.status}`); await p.shot(`${name}-after.png`); }
        await undo(p);
        if (await text(p) !== t0) r.fails.push('one Ctrl+Z did not give the text back');
      } catch (e) { r.fails.push(String(e.message)); }
      log(`${r.fails.length ? 'FAIL' : 'pass'} ${name} ${r.info.wrote ?? ''}${r.fails.map((f) => `\n    ${f}`).join('')}`);
      results.push(r);
    };
    await drag('drag-relative', 'one', 60, 0, /\\node\[(below|below right|below left)=([-\d.]+cm( and [\d.]+cm)? )?of (run|out)(, xshift=[-\d.]+cm)?\] \(note\)/);
    await drag('drag-fixed-alt', 'one', 60, 0, /\\node\[\] \(note\) at \(|\\node \(note\) at \([-\d.]+,[-\d.]+\) \{/, 1);
    // Ctrl: the centre on the picture's grid, half units
    await drag('drag-grid-ctrl', 'one', 60, 0, /\\node(\[\])? \(note\) at \(-?\d+(\.[05])?(cm)?,-?\d+(\.[05])?(cm)?\) \{/, 2);
    {
      const r = { name: 'dblclick-edit', fails: [], info: {} };
      try {
        await build(p);
        const t0 = await text(p), w = await word(p, 'findings');
        await p.eval(`document.addEventListener('dblclick', (e) => { self.__dbl = (e.target.tagName + '.' + e.target.className).slice(0, 80); }, true)`);
        await p.mouse(w.r - 1, (w.t + w.b) / 2, 1); await sleep(60); await p.mouse(w.r - 1, (w.t + w.b) / 2, 2);
        r.info.dbl = await p.eval('self.__dbl');
        await sleep(400);
        const dialog = await p.eval(`!!document.querySelector('dialog.sd-dialog[open]') || !!document.getElementById('play-view')?.open`);
        if (dialog) r.fails.push('a widget opened');
        await p.key('Q');
        const t1 = await p.until(`satexHealth().text !== ${J(t0)} && satexHealth().text`, 5000);
        if (!t1 || !/\{findingsQ\}/.test(t1)) r.fails.push(`typed: ${J((t1 || '').match(/\(out\) \{[^}]*\}/)?.[0])} (toast ${await toastNow(p)})`);
        await p.shot('dblclick-edit.png');
        await p.key('Escape');
        await undo(p);
      } catch (e) { r.fails.push(String(e.message)); }
      log(`${r.fails.length ? 'FAIL' : 'pass'} ${r.name} ${J(r.info)}${r.fails.map((f) => `\n    ${f}`).join('')}`);
      results.push(r);
    }
    // dark: the menu and a submenu
    await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
    await sleep(400);
    // a word list a build has not given yet: wait for it (the shots need the node)
    await p.until(`satexPdfWords().length > 3`, 20000);
    await rclick(p, mid(await word(p, 'run')) ?? [0, 0]);
    await choose(p, ['Style', 'Fill color']).catch(() => {});
    await sleep(200);
    await p.shot('dark-node-menu.png');
    await p.eval(`document.querySelectorAll('.pa-menu').forEach((e) => e.remove())`);
    await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
    await rclick(p, mid(await word(p, 'run')) ?? [0, 0]);
    await choose(p, ['Shape']).catch(() => {});
    await sleep(200);
    await p.shot('light-node-shapes.png');
    await p.eval(`document.querySelectorAll('.pa-menu').forEach((e) => e.remove())`);
    // the Typeset view: a node's text in the sketch, a path near its ends
    {
      await p.eval(`document.querySelector('.side-tabs [data-panel=play-typeset]').click()`);
      await p.until(`!satexHealth().busy && document.querySelector('#typeset-body .ts-sketch text')`, 120000);
      await sleep(600);
      const tsAt = (w) => async () => p.eval(`(() => { const t = [...document.querySelectorAll('#typeset-body .ts-sketch text')].find((t) => t.textContent.trim() === ${J(w)}); if (!t) return null; t.scrollIntoView({ block: 'center' }); const r = t.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
      const tsBetween = (a, c) => async () => { const x = await tsAt(a)(), y = await tsAt(c)(); return x && y && [(x[0] + y[0]) / 2, (x[1] + y[1]) / 2]; };
      const tsAct = async (name, at, path, want, o = {}) => {
        await p.eval(`document.querySelector('.side-tabs [data-panel=play-typeset]').click()`);
        await p.until(`!satexHealth().busy && document.querySelector('#typeset-body .ts-sketch text')`, 120000);
        await sleep(500);
        return act(p, name, at, path, want, { ...o, compile: o.compile ?? true });
      };
      await tsAct('ts-node-ellipse', tsAt('source'), ['Shape', 'Ellipse'], /\\node\[ellipse, box\] \(src\)/, { shot: true });
      await tsAct('ts-edge-tip', tsBetween('source', 'run'), ['Arrow tips', '←'], /\\draw\[flow, <-\] \(src\) -- \(run\);/, { shot: true });
    }
  }
  if (sc.session === 'thesis') {
    await open(p, 'thesis.tex');
    await file(p, 'chapter.tex');
    log('thesis build', await build(p, 300000));
    const node = (w) => async () => mid(await word(p, w));
    {
      const t0 = await text(p), [x, y] = mid(await word(p, 'SaTeX'));
      const rows = await rclick(p, [x, y]);
      const first = await p.eval(`document.querySelector('.pa-root > button span')?.textContent`);
      log(`${first === 'Edit text…' ? 'pass' : 'FAIL'} thesis-edit-first ${first} | ${rows?.join(' | ')}`);
      results.push({ name: 'thesis-edit-first', fails: first === 'Edit text…' ? [] : [`first row ${first}`] });
      await p.eval(`document.querySelectorAll('.pa-menu').forEach((e) => e.remove())`);
      void t0;
    }
    await act(p, 'thesis-node-diamond', node('SaTeX'), ['Shape', 'Diamond'], /\\node \(satex\) \[diamond, right of=tex\] \{SaTeX\};/, { toast: /\\usetikzlibrary\{shapes\.geometric\}/, compile: false, shot: true });
    await act(p, 'thesis-add-edge', node('findings'), ['Arrange', 'Add edge from this node…'], /\\draw\[->\] \(out\) -- \(tex\);\n  \\end\{tikzpicture\}/, { then: async () => { await sleep(200); const [x, y] = mid(await word(p, 'file')); await p.mouse(x, y); } });
  }
  if (sc.session === 'figure') {
    // a box's width dragged on the page: the graphic's `width=` share, Ctrl to half the line
    await open(p, 'figure.tex');
    const first = await build(p, 300000);
    if (!first.ok) throw new Error(`no first build (${first.status.slice(0, 120)})`);
    const r = { name: 'box-width-drag', fails: [], info: {} };
    try {
      const t0 = await text(p);
      await p.until(`satexPdfWords().length > 3`, 20000);
      await sleep(1000);
      // the picture: above the caption's words, in the middle of the line
      const cap = await word(p, 'example');
      const [x, y] = [cap.l + 10, cap.t - 40];
      await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
      const g = await p.until(`(() => { const e = document.querySelector('#play-pdf .pdf-size-grip'); if (!e) return null; const q = e.getBoundingClientRect(), b = e.parentElement.getBoundingClientRect(); return [q.left + q.width / 2, q.top + q.height / 2, b.left, b.right]; })()`, 4000);
      if (!g) throw new Error('no width grip on the graphic');
      await p.shot('box-width-hover.png');
      await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: g[0], y: g[1] });
      await p.s('Input.dispatchMouseEvent', { type: 'mousePressed', x: g[0], y: g[1], button: 'left', clickCount: 1 });
      // half the line: the box is 0.6 of it
      const to = g[0] - (g[3] - g[2]) / 6;
      for (let k = 1; k <= 6; k++) { await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: g[0] + (to - g[0]) * k / 6, y: g[1], button: 'left', buttons: 1, modifiers: 2 }); await sleep(30); }
      await p.shot('box-width-ghost.png');
      await p.s('Input.dispatchMouseEvent', { type: 'mouseReleased', x: to, y: g[1], button: 'left', clickCount: 1, modifiers: 2 });
      const t1 = await p.until(`satexHealth().text !== ${J(t0)} && satexHealth().text`, 5000);
      r.info.wrote = (t1 || '').match(/\\includegraphics\[[^\]]*\]/)?.[0];
      if (!/\\includegraphics\[width=\.5\\linewidth\]/.test(t1 || '')) r.fails.push(`text: ${J(r.info.wrote)}`);
      const b = await build(p);
      if (!b.ok || /error|failed|stopped/i.test(b.status)) r.fails.push(`build: ${b.status.slice(0, 120)}`);
      await undo(p);
      if (await text(p) !== t0) r.fails.push('one Ctrl+Z did not give the text back');
    } catch (e) { r.fails.push(String(e.message)); await p.shot('box-width-fail.png').catch(() => {}); }
    results.push(r);
  }
  const fails = results.flatMap((r) => r.fails.map((f) => `${r.name}: ${f}`));
  return { name: sc.name, fails, perf: [], info: { actions: results.length, failed: results.filter((r) => r.fails.length).map((r) => r.name) } };
}
export { open, file, build, word, mid, rclick, choose, undo, text, act0 };
