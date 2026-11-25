// The refactor plugins (doc/site/refactor.ts) as a person uses them on thesis.tex's chapter.tex, built by the browser's
// pdfTeX: the picture wrapped in a figure from the PDF (and the figure unwrapped), the cost equation turned into an
// align from the editor's menu, a word extracted to a \newcommand. Each one checks the text, builds it and is undone
// by one Ctrl+Z. The rewrites themselves are unit-tested (tests/refactor.test.mjs). Field: none.
import { sleep } from '../lib.mjs';
import { open, file, build, word, mid, act0, undo, text, written } from './tikz.mjs';

const J = JSON.stringify;
const view = `document.querySelector('#play-editor .cm-editor').satexView`;
const toastNow = (p) => p.drv.run({ toast: true });
const excerpt = (t) => t.slice(t.indexOf('Pictures'), t.indexOf('Cost}'));
/** The whole text replaced by `t` (one edit), built. */
async function setText(p, t) {
  await p.eval(`(() => { const v = ${view}; v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: ${J(t)} } }); })()`);
  await sleep(300);
  return build(p);
}
/** The editor's menu at selection `[a, b]`, then through `path` (a `?` label only when the menu has it). */
async function editorMenu(p, a, b, path) {
  await p.eval(`(() => { const v = ${view}; v.dispatch({ selection: { anchor: ${a}, head: ${b} }, scrollIntoView: true }); v.focus(); })()`);
  await sleep(300);
  const c = await p.eval(`(() => { const v = ${view}, r = v.coordsAtPos(${b > a ? `${a} + 1` : a}); return r && [r.left + 1, (r.top + r.bottom) / 2]; })()`);
  if (!c) throw new Error('the selection is not on screen');
  for (const type of ['mousePressed', 'mouseReleased']) await p.s('Input.dispatchMouseEvent', { type, x: c[0], y: c[1], button: 'right', clickCount: 1 });
  if (!await p.until(`!document.getElementById('play-ctx').hidden`, 8000)) throw new Error('no editor menu');
  for (const [i, label] of path.entries()) {
    const opt = label.endsWith('?'), want = label.replace(/\?$/, '');
    const at = await p.eval(`(() => { const boxes = [...document.querySelectorAll('#play-ctx, #play-ctx .ctx-nested')], box = boxes.at(-1);
      const li = [...box.querySelectorAll(':scope > li')].find((l) => l.firstChild?.textContent?.trim() === ${J(want)});
      if (!li) return { no: [...box.querySelectorAll(':scope > li')].map((l) => l.firstChild?.textContent) }; const r = li.getBoundingClientRect(); return [r.left + 20, r.top + r.height / 2]; })()`);
    if (!Array.isArray(at)) { if (opt) continue; throw new Error(`no ${J(want)} in ${J(at?.no)}`); }
    await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    await sleep(160);
    if (i === path.length - 1) for (const type of ['mousePressed', 'mouseReleased']) await p.s('Input.dispatchMouseEvent', { type, x: at[0], y: at[1], button: 'left', clickCount: 1 });
  }
}
/** One editor action: the menu at `[a, b]`, `path`, `then`; the text checked, built and undone. */
async function editorAct(results, p, name, sel, path, want, then = null) {
  const r = { name, fails: [], info: {} };
  try {
    const t0 = await text(p), [a, b] = sel(t0);
    await editorMenu(p, a, b, path);
    if (then) await then();
    const t1 = await written(p, t0);
    if (!t1) throw new Error(`text unchanged (toast: ${await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`)})`);
    if (!want.test(t1)) r.fails.push(`text: want ${want}, got ${J(t1.slice(0, 1500))}`);
    const bd = await build(p);
    r.info.status = bd.status.slice(0, 100);
    if (!bd.ok || /error|failed|stopped/i.test(bd.status)) r.fails.push(`build: ${bd.status.slice(0, 160)}`);
    await undo(p);
    if (await text(p) !== t0) r.fails.push('one Ctrl+Z did not give the text back');
  } catch (e) { r.fails.push(String(e.message ?? e).slice(0, 300)); await p.shot(`${name}-fail.png`).catch(() => {}); }
  await p.key('Escape').catch(() => {});
  results.push(r);
}

export async function refactor(p, sc) {
  const results = [], act = (...a) => act0(results, ...a);
  await open(p, 'thesis.tex');
  await file(p, 'chapter.tex');
  const first = await build(p, 300000);
  console.log('first build', first.status);
  if (!first.ok) throw new Error(`no first build (${first.status.slice(0, 120)})`);
  const original = await text(p), node = (w) => async () => mid(await word(p, w));
  // the figure unwrapped from the PDF: the picture stays where the float stood
  await act(p, 'rf-unwrap', node('SaTeX'), ['Refactor', 'Unwrap the figure'], /draws the pipeline\.\n\\begin\{tikzpicture\}\[node distance=2\.4cm[^]*\\end\{tikzpicture\}\n\n\\subsection\{Cost\}/, { excerpt });
  // the bare picture wrapped in a figure from the PDF, the caret in its caption
  const bare = original.replace(/\\begin\{figure\}\[ht\]\n  \\centering\n([^]*?)  \\caption[^\n]*\n\\end\{figure\}\n/, (_, pic) => pic.replace(/^  /gm, ''));
  const b = await setText(p, bare);
  if (!b.ok) results.push({ name: 'rf-wrap-setup', fails: [`the bare picture does not build: ${b.status.slice(0, 120)}`] });
  await act(p, 'rf-wrap', node('SaTeX'), ['Refactor', 'Wrap in a figure'], /draws the pipeline\.\n\\begin\{figure\}\[htbp\]\n  \\centering\n  \\begin\{tikzpicture\}[^]*  \\end\{tikzpicture\}\n  \\caption\{\}\n  \\label\{fig:file\}\n\\end\{figure\}\n/, {
    excerpt, after: async () => {
      const caret = await p.eval(`(() => { const v = ${view}, t = v.state.doc.toString(); return [v.state.selection.main.head, t.indexOf('\\\\caption{}') + 9]; })()`);
      if (caret[0] !== caret[1]) results.push({ name: 'rf-wrap-caret', fails: [`the caret at ${caret[0]}, the caption at ${caret[1]}`] });
    } });
  await setText(p, original);
  // the cost equation into an align from the editor: & before its first relation
  await editorAct(results, p, 'rf-display-align', (t) => { const i = t.indexOf('T(n, k) ='); return [i, i]; }, ['Refactor?', 'Math', 'To align'],
    /\\begin\{align\}\\label\{eq:cost\}\n  T\(n, k\) &= \\sum_\{i=1\}\^\{n\} c_i \\cdot k_i \\le c_\{\\max\}\\, n k ,\n\\end\{align\}/);
  // a word to a \newcommand at the chapter's top, here only
  await editorAct(results, p, 'rf-extract', (t) => { const i = t.indexOf('SaTeX checks'); return [i, i + 5]; }, ['Refactor?', 'Extract to \\newcommand…'],
    /^\\newcommand\{\\satex\}\{SaTeX\}\n\\section[^]*\{From source to findings: \\TeX\{\} reads, \\satex\{\} checks\}/, async () => {
      if (!await p.until(`document.querySelector('.tikz-input input')`, 3000)) throw new Error('no name box');
      await p.eval(`(() => { const i = document.querySelector('.tikz-input input'); i.value = 'satex'; i.focus(); })()`);
      await p.key('Enter');
    });
  for (const r of results) console.log(`${r.fails.length ? 'FAIL' : 'pass'} ${r.name} ${r.fails.join(' / ').slice(0, 300)}`);
  const fails = results.flatMap((r) => r.fails.map((f) => `${r.name}: ${f}`));
  return { name: sc.name, fails, perf: [], info: { actions: results.length, failed: results.filter((r) => r.fails.length).map((r) => r.name) } };
}
