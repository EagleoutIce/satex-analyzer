// Insert (plugin `insert`, doc/site/refactor.ts) as a person uses it on hello.tex, built by the browser's pdfTeX: a
// right click on the paragraph in the PDF, Insert, Table: the table goes after the paragraph and its widget opens; the
// PDF bar's Insert menu writes a figure at the editor's caret, graphicx loaded with it by the snapshot printer (the run lacks it), the image dialog open. Each is
// built and undone by one Ctrl+Z. The planners are unit-tested (tests/refactor.test.mjs). Field: none.
import { sleep } from '../lib.mjs';
import { open, build, word, mid, act0, undo, text, written } from './tikz.mjs';

const J = JSON.stringify;
const view = `document.querySelector('#play-editor .cm-editor').satexView`;
/** The open widget dialog's kind (`table`, `image`), waited for; null when none opens. */
const dialog = (p) => p.until(`document.querySelector('dialog.sd-dialog[open]')?.dataset.pop`, 60000);
async function shut(p) {
  await p.eval(`document.querySelectorAll('dialog.sd-dialog[open]').forEach((d) => d.close())`);
  await sleep(200);
}

export async function insert(p, sc) {
  const results = [];
  await open(p, 'hello.tex');
  const first = await build(p, 300000);
  if (!first.ok) throw new Error(`no first build (${first.status.slice(0, 120)})`);
  // hello.tex's own error (its \newcommand{\section}) stays: a build counts as clean with no more errors than that
  const errs = (st) => +(/(\d+) errors?/.exec(st)?.[1] ?? 0), base = errs(first.status);
  // the PDF's right click on the paragraph: Insert ▸ Table after it, the table widget open on it
  await act0(results, p, 'ins-table-pdf', async () => mid(await word(p, 'Hello,')), ['Insert', 'Table'],
    /\\greet\{world\}\n\n\\begin\{table\}\[htbp\]\n  \\centering\n  \\caption\{\}\n  \\label\{tab:name\}\n  \\begin\{tabular\}\{ll\}[^]*\\end\{tabular\}\n\\end\{table\}\n\\end\{document\}/, {
      excerpt: (t) => t.slice(t.indexOf('\\begin{document}')),
      after: async () => {
        const kind = await dialog(p);
        if (kind !== 'table') results.push({ name: 'ins-table-widget', fails: [`the widget open: ${J(kind)}`] });
        await shut(p);
        // act0's own build fails on any error: hello.tex has one of its own
        const b = await build(p);
        if (!b.ok || errs(b.status) > base) results.push({ name: 'ins-table-built', fails: [`build: ${b.status.slice(0, 160)}`] });
      }, compile: false });
  // the PDF bar's Insert menu: a figure at the editor's caret (the end of \greet{world}), graphicx loaded, one undo step
  const r = { name: 'ins-figure-toolbar', fails: [], info: {} };
  try {
    const t0 = await text(p), at = t0.indexOf('\\greet{world}') + 13;
    await p.eval(`(() => { const v = ${view}; v.dispatch({ selection: { anchor: ${at} } }); v.focus(); })()`);
    await sleep(200);
    await p.eval(`(() => { const d = document.querySelector('#play-pdf .math-menu'); d.open = true; })()`);
    if (!await p.until(`!!document.querySelector('#play-pdf .math-menu .ins-row[data-insert="Figure with an image"]')`, 3000)) throw new Error('the Insert menu has no figure row');
    await p.eval(`document.querySelector('#play-pdf .math-menu .ins-row[data-insert="Figure with an image"]').click()`);
    const t1 = await written(p, t0);
    if (!t1) throw new Error(`text unchanged (toast: ${await p.eval(`document.getElementById('play-toast')?.textContent ?? ''`)})`);
    if (!/\\documentclass\{article\}\n\\usepackage\{graphicx\}\n/.test(t1)) r.fails.push(`no graphicx load: ${J(t1.slice(0, 200))}`);
    if (!/\\greet\{world\}\n\n\\begin\{figure\}\[htbp\]\n  \\centering\n  \\includegraphics\[width=0\.8\\linewidth\]\{example-image\}\n  \\caption\{\}\n  \\label\{fig:example-image\}\n\\end\{figure\}\n\\end\{document\}/.test(t1)) r.fails.push(`text: ${J(t1.slice(t1.indexOf('\\begin{document}')))}`);
    const kind = await dialog(p);
    if (kind !== 'image') r.fails.push(`the widget open: ${J(kind)}`);
    r.info.alt = await p.eval(`!!document.querySelector('dialog.sd-dialog[open] .sd-alt')`);
    await shut(p);
    const caret = await p.eval(`(() => { const v = ${view}, t = v.state.doc.toString(); return [v.state.selection.main.head, t.indexOf('\\\\caption{}') + 9]; })()`);
    if (caret[0] !== caret[1]) r.fails.push(`the caret at ${caret[0]}, the caption at ${caret[1]}`);
    const b = await build(p);
    r.info.status = b.status.slice(0, 100);
    if (!b.ok || errs(b.status) > base || /failed|stopped/i.test(b.status)) r.fails.push(`build: ${b.status.slice(0, 160)}`);
    await undo(p);
    if (await text(p) !== t0) r.fails.push('one Ctrl+Z did not give the text back');
  } catch (e) { r.fails.push(String(e.message ?? e).slice(0, 300)); await p.shot('ins-figure-toolbar-fail.png').catch(() => {}); }
  results.push(r);
  for (const x of results) console.log(`${x.fails.length ? 'FAIL' : 'pass'} ${x.name} ${x.fails.join(' / ').slice(0, 300)}`);
  const fails = results.flatMap((x) => x.fails.map((f) => `${x.name}: ${f}`));
  return { name: sc.name, fails, perf: [], info: { actions: results.length, failed: results.filter((x) => x.fails.length).map((x) => x.name), ...r.info } };
}
