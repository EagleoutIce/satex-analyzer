// Refactor plugins (refactor.ts `satexRefactor`): floats wrapped and unwrapped, math from one form to another, a
// selection extracted to a \newcommand. Each rewrite is checked as text, and its environments still pair up.
import assert from 'node:assert/strict';
import { test } from './harness.mjs';

const R = self.satexRefactor, PA = self.satexPageActions;
/** `text` with edits `{from, to, now}` applied, the last first. */
const edit = (text, edits) => edits.filter(Boolean).sort((a, b) => b.from - a.from || b.to - a.to).reduce((t, e) => t.slice(0, e.from) + e.now + t.slice(e.to), text);
/** Whether every \begin has its \end in order. */
const paired = (t) => { const st = []; for (const m of t.matchAll(/\\(begin|end)\{([^}]*)\}/g)) { if (m[1] === 'begin') st.push(m[2]); else if (st.pop() !== m[2]) return false; } return !st.length; };
const PIC = 'Text before.\n\n\\begin{tikzpicture}\n  \\node (a) {Data Flow Graph};\n  \\draw (a) -- (0,1);\n\\end{tikzpicture}\n\nAfter.\n';

// Intuition: a picture on its own lines goes into a figure on those lines, centred, its caption empty with the caret in
// it and a label from its first node's words. Pins a float that took the blank lines around it or put the caret elsewhere.
test('wrap: a picture into a figure, caret in the caption, label from its words', () => {
  const at = PIC.indexOf('\\node'), t = R.wrapTarget(PIC, at);
  assert.equal(t.env, 'tikzpicture'); assert.equal(t.kind, 'figure'); assert.equal(t.float, null);
  const r = R.wrap(PIC, t, PIC), out = edit(PIC, [r.edit]);
  assert.equal(out, 'Text before.\n\n\\begin{figure}[htbp]\n  \\centering\n  \\begin{tikzpicture}\n    \\node (a) {Data Flow Graph};\n    \\draw (a) -- (0,1);\n  \\end{tikzpicture}\n  \\caption{}\n  \\label{fig:data-flow-graph}\n\\end{figure}\n\nAfter.\n');
  assert.equal(out.slice(r.caret - 9, r.caret + 1), '\\caption{}');
  // the label taken: the next one numbered
  assert.match(edit(PIC, [R.wrap(PIC, t, '\\label{fig:data-flow-graph}').edit]), /\\label\{fig:data-flow-graph-2\}/);
});

// Intuition: a tabular goes into a table, its caption above it (tables are captioned at the top); an image gets
// `alt=` beside its options (tagging) and a label from its file name. Pins an image whose options lost a key.
test('wrap: tabular into a table, image with alt text', () => {
  const tab = '  \\begin{tabular}{lr}\n    Name & Size \\\\\n  \\end{tabular}\n', t = R.wrapTarget(tab, 5);
  assert.equal(edit(tab, [R.wrap(tab, t, '').edit]), '  \\begin{table}[htbp]\n    \\centering\n    \\caption{}\n    \\label{tab:name}\n    \\begin{tabular}{lr}\n      Name & Size \\\\\n    \\end{tabular}\n  \\end{table}\n');
  const img = 'See\n\\includegraphics[width=3cm]{img/run_times.pdf}\nnow', g = R.wrapTarget(img, 6);
  assert.equal(g.env, 'includegraphics');
  assert.equal(R.firstWords(img, g), 'run times');
  const out = edit(img, [R.wrap(img, g, '', 'Run times per document').edit]);
  assert.match(out, /\\includegraphics\[width=3cm, alt=\{Run times per document\}\]\{img\/run_times\.pdf\}/);
  assert.match(out, /\\label\{fig:run-times\}/);
  const bare = 'x \\includegraphics{a.png} y', b = edit(bare, [R.wrap(bare, R.wrapTarget(bare, 4), '', 'A').edit]);
  assert.match(b, /^x \n\\begin\{figure\}\[htbp\]\n  \\centering\n  \\includegraphics\[alt=\{A\}\]\{a\.png\}\n/);
  assert.ok(paired(b));
});

// Intuition: unwrapping takes the float, its \centering, \caption (with a short form) and \label; the content stays,
// one level out, and wrapping again is the same shape. Pins a caption's nested braces cut in half, a picture's own
// \label inside it removed.
test('unwrap: the float goes, the content stays', () => {
  const fig = 'A\n\\begin{figure}[ht]\n  \\centering\n  \\begin{tikzpicture}\n    \\node (x) {X}; \\label{inner}\n  \\end{tikzpicture}\n  \\caption[S]{Long {nested} one}\\label{fig:x}\n\\end{figure}\nB';
  const t = R.wrapTarget(fig, fig.indexOf('\\node'));
  assert.equal(t.float.env, 'figure');
  const r = R.unwrap(fig, t.float), out = edit(fig, [r.edit]);
  assert.equal(out, 'A\n\\begin{tikzpicture}\n  \\node (x) {X}; \\label{inner}\n\\end{tikzpicture}\nB');
  assert.deepEqual(r.dropped, ['\\caption[S]{Long {nested} one}', '\\label{fig:x}']);
  // wrap, unwrap: the text again
  const w = edit(PIC, [R.wrap(PIC, R.wrapTarget(PIC, PIC.indexOf('\\node')), '').edit]);
  assert.equal(edit(w, [R.unwrap(w, R.wrapTarget(w, w.indexOf('\\node')).float).edit]), PIC);
});

// Intuition: inline and display math swap their delimiters, an environment wraps the body on its own lines, and the
// way back gives the text again. Pins `\\[2pt]` read as a display's start and `\$` as math.
test('math: inline, display and equation into each other', () => {
  const s = 'Cost \\(a + b\\) here and \\$5.', m = R.mathAt(s, 8);
  assert.equal(m.kind, 'inline');
  assert.deepEqual(R.targets(m), ['display', 'equation', 'equation*', 'align', 'align*']);
  assert.equal(R.mathTo(s, m, 'display'), '\\[a + b\\]');
  assert.equal(R.mathTo(s, m, 'equation'), '\n\\begin{equation}\n  a + b\n\\end{equation}\n');
  const d = 'x\n\\[\n  e = mc^2\n\\]\ny', dm = R.mathAt(d, 6);
  assert.equal(dm.kind, 'display');
  const eq = edit(d, [{ from: dm.from, to: dm.to, now: R.mathTo(d, dm, 'equation*') }]);
  assert.equal(eq, 'x\n\\begin{equation*}\n  e = mc^2\n\\end{equation*}\ny');
  const back = R.mathAt(eq, 20);
  assert.equal(edit(eq, [{ from: back.from, to: back.to, now: R.mathTo(eq, back, 'display') }]), d);
  assert.equal(R.mathTo(d, dm, 'inline'), '\\(e = mc^2\\)');
  assert.equal(R.mathAt('a \\\\[2pt] b $x$', 12).body, 'x');
  assert.equal(R.mathAt('price \\$5 and $y$', 15).body, 'y');
  // a label keeps the numbered forms only
  const l = '\\begin{equation}\\label{e} x = 1\\end{equation}';
  assert.deepEqual(R.targets(R.mathAt(l, 20)), ['align']);
});

// Intuition: into align each row gets `&` before its first top-level relation (not one inside braces, not `\le` read
// as `\l`), a row with `&` keeps its own; out of align the `&` go, `\&` stays. Pins `&` put inside a subscript.
test('math: rows into align at the relation, and back', () => {
  const g = '\\begin{gather}\n  f_{i=1} + x \\le y \\\\\n  a = b \\& c\\\\[2pt]\n  p &= q\n\\end{gather}', m = R.mathAt(g, 20);
  assert.deepEqual(R.rows(m.body).length, 3);
  assert.deepEqual(R.targets(m), ['align', 'align*', 'gather*', 'multline', 'multline*']);
  const a = R.mathTo(g, m, 'align');
  assert.equal(a, '\\begin{align}\n  f_{i=1} + x &\\le y \\\\\n  a &= b \\& c\\\\[2pt]\n  p &= q\n\\end{align}');
  const am = R.mathAt(a, 20);
  assert.equal(R.mathTo(a, am, 'gather'), '\\begin{gather}\n  f_{i=1} + x \\le y \\\\\n  a = b \\& c\\\\[2pt]\n  p = q\n\\end{gather}');
  const one = '\\[ T(n) = \\sum_i c_i \\le C \\]';
  assert.equal(R.mathTo(one, R.mathAt(one, 4), 'align*'), '\\begin{align*}\n  T(n) &= \\sum_i c_i \\le C\n\\end{align*}');
  assert.ok(R.needsAms('align*') && R.needsAms('pmatrix') && !R.needsAms('equation') && !R.needsAms('display'));
});

// Intuition: amsmath is added once after the last package of this file's preamble, never when amsmath or mathtools
// is loaded; a file without a preamble names the line. Pins a second \usepackage{amsmath}.
test('math: amsmath loaded with the change', () => {
  const doc = '\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\nx\n\\end{document}';
  assert.deepEqual(R.pkgEdit(doc, doc, 'amsmath'), { edit: { from: 42, to: 42, now: '\\usepackage{amsmath}\n' } });
  assert.equal(doc.slice(0, 42), '\\documentclass{article}\n\\usepackage{tikz}\n');
  assert.deepEqual(R.pkgEdit(doc, `${doc}\n\\usepackage[fleqn]{mathtools}`, 'amsmath'), { have: true });
  assert.deepEqual(R.pkgEdit(doc, `${doc}\n% \\usepackage{amsmath}`, 'amsmath').edit.from, 42);
  assert.deepEqual(R.pkgEdit('x', 'x', 'amsmath'), { line: '\\usepackage{amsmath}' });
});

// Intuition: a display of rows and columns gets a matrix around its body; a matrix switches kind, `array` with one
// column spec letter per column and its spec gone on the way out. Pins `array` read with its spec as a cell.
test('math: grids and matrices', () => {
  const d = '\\[\n  a & b \\\\\n  c & d\n\\]', g = R.matrixAt(d, 6);
  assert.equal(g.grid, true);
  const out = edit(d, [{ from: g.from, to: g.to, now: R.matrixTo(d, g, 'pmatrix') }]);
  assert.equal(out, '\\[\n  \\begin{pmatrix}\n  a & b \\\\\n  c & d\n  \\end{pmatrix}\n\\]');
  const ar = '\\( \\begin{array}{cc} 1 & 0 \\\\ 0 & 1 \\end{array} \\)', x = R.matrixAt(ar, 20);
  assert.equal(x.env, 'array'); assert.equal(x.body, ' 1 & 0 \\\\ 0 & 1 ');
  assert.equal(R.matrixTo(ar, x, 'bmatrix'), '\\begin{bmatrix} 1 & 0 \\\\ 0 & 1 \\end{bmatrix}');
  const p = '$\\begin{pmatrix} 1 & 2 & 3 \\end{pmatrix}$', y = R.matrixAt(p, 18);
  assert.equal(R.matrixTo(p, y, 'array'), '\\begin{array}{ccc} 1 & 2 & 3 \\end{array}');
  // an align is rows of relations, no grid
  assert.equal(R.matrixAt('\\begin{align} a &= b \\\\ c &= d \\end{align}', 16), null);
});

// Intuition: the selection becomes `\name` and its definition goes after the preamble's last package or definition;
// `{}` follows where a letter or space would join the name; every place on request, never inside the preamble. Pins
// a name already in use redefined, a selection of half a group.
test('extract: a selection to a \\newcommand', () => {
  const doc = '\\documentclass{article}\n\\usepackage{xcolor}\n\\begin{document}\nThe SaTeX tool. SaTeX runs.\n\\end{document}';
  const a = doc.indexOf('SaTeX'), b = a + 5, one = R.extract(doc, doc, a, b, 'satex', false);
  assert.equal(edit(doc, one.edits), '\\documentclass{article}\n\\usepackage{xcolor}\n\\newcommand{\\satex}{SaTeX}\n\\begin{document}\nThe \\satex{} tool. SaTeX runs.\n\\end{document}');
  const all = R.extract(doc, doc, a, b, 'satex', true);
  assert.match(edit(doc, all.edits), /The \\satex\{\} tool\. \\satex\{\} runs\./);
  assert.equal(R.occurrences(doc, 'SaTeX').length, 2);
  assert.match(R.extract(doc, doc, a, b, 'usepackage', false).error, /used already/);
  assert.match(R.extract(doc, doc, a, b, 'sa1', false).error, /letters only/);
  assert.match(R.extract('{a} b', '{a} b', 1, 4, 'x', false).error, /balanced/);
  // a chapter file without a preamble: at its top
  const ch = '% chapter\nSee x.\n', c = ch.indexOf('x');
  assert.equal(edit(ch, R.extract(ch, ch, c, c + 1, 'xx', false).edits), '% chapter\n\\newcommand{\\xx}{x}\nSee \\xx.\n');
});

// Intuition: the refactor plugins' groups merge into one Refactor submenu with the rows that fit the target. Pins two
// Refactor rows side by side and a math row offered outside math.
test('plugins: one Refactor group per target', () => {
  const was = self.satexStore;
  self.satexStore = { get: () => null, set: () => {} };
  try {
    const ctx = { text: PIC, all: PIC, x: 0, y: 0, facts: null, write() {}, toast() {}, pick() {}, colAlign() {} };
    const gs = PA.groups({ kind: 'source', at: PIC.indexOf('\\node'), sel: null }, ctx);
    assert.deepEqual(gs.map((g) => g.title), ['Refactor']);
    assert.deepEqual(gs[0].items.map((i) => i.label), ['Wrap in a figure']);
    const ms = 'a \\[ x = 1 \\] b', mg = PA.groups({ kind: 'source', at: 5, sel: [5, 10] }, { ...ctx, text: ms, all: ms });
    assert.deepEqual(mg[0].items.map((i) => i.label), ['Math', 'Extract to \\newcommand…']);
    assert.ok(PA.plugins.filter((p) => p.category === 'refactor').length === 3);
  } finally { self.satexStore = was; }
});
