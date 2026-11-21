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
  assert.match(b, /^x\n\\begin\{figure\}\[htbp\]\n  \\centering\n  \\includegraphics\[alt=\{A\}\]\{a\.png\}\n/);
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
  // out of a line of text: the spaces at its line breaks go
  assert.equal(edit(s, [R.mathEdit(s, m, 'equation')]), 'Cost\n\\begin{equation}\n  a + b\n\\end{equation}\nhere and \\$5.');
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
  const eq = '\\begin{equation}\\label{eq:c}\n  T = c\n\\end{equation}';
  assert.equal(R.mathTo(eq, R.mathAt(eq, 30), 'align'), '\\begin{align}\\label{eq:c}\n  T &= c\n\\end{align}');
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
    assert.deepEqual(mg.map((g) => g.title), ['Format', 'Color', 'Refactor'], 'the shared order: Format ▸ before Refactor ▸');
    assert.deepEqual(mg.find((g) => g.title === 'Refactor').items.map((i) => i.label), ['Math', 'Extract to \\newcommand…']);
    assert.ok(PA.plugins.filter((p) => p.category === 'refactor').length === 4, 'wrap, math, extract, format');
  } finally { self.satexStore = was; }
});

/** `text` with the smart wrap of the line around `at` at `max` applied, unchanged when none. */
const wrapped = (text, max, at = text.length) => { const e = R.wrapLine(text, at, max); return e ? edit(text, [e]) : text; };

// Intuition: a line typed past the length breaks at a sentence's end before any other space, else after an argument's
// closing brace, else after a comma, else at the last space that fits. Pins a break at the last space when a sentence
// ended earlier on the line, and an `A. Turing` initial read as a sentence end.
test('smart wrap: sentence end, then brace, then comma, then the last space', () => {
  assert.equal(wrapped('One idea. Two ideas more than that fit here', 30), 'One idea.\nTwo ideas more than that fit here');
  assert.equal(wrapped('See \\cite{knuth} for long words and more', 30), 'See \\cite{knuth}\nfor long words and more');
  assert.equal(wrapped('first, second and third and fourth', 30), 'first,\nsecond and third and fourth');
  assert.equal(wrapped('a bb cc dd ee ff gg hh ii jj kk ll', 20), 'a bb cc dd ee ff gg\nhh ii jj kk ll');
  assert.equal(wrapped('By A. Turing who wrote it all down once', 30), 'By A. Turing who wrote it all\ndown once');
  // fits: nothing; the indentation goes with the break
  assert.equal(wrapped('short line', 30), 'short line');
  assert.equal(wrapped('  \\item one two three four five six', 21), '  \\item one two three\n  four five six');
});

// Intuition: a break inside `\textbf{…}` is only a space TeX reads the same; inside math, an option list, a `\verb`, a
// comment or after `\` (a control space) it is not, and a verbatim environment is never touched. Pins a key list
// `[width=3cm, alt=x]` broken, a comment's tail turned into text.
test('smart wrap: inside braces fine; math, options, verb, comments, verbatim untouched', () => {
  assert.equal(wrapped('xx \\textbf{bold words here and there}', 24), 'xx \\textbf{bold words\nhere and there}');
  assert.equal(wrapped('aa $x + y = z + w + v$ bb', 12), 'aa\n$x + y = z + w + v$ bb');
  assert.equal(wrapped('aa \\(x + y = z\\) bb', 10), 'aa\n\\(x + y = z\\) bb');
  assert.equal(wrapped('\\includegraphics[width=3cm, alt={a b}]{f} z', 20), '\\includegraphics[width=3cm, alt={a b}]{f}\nz');
  assert.equal(wrapped('aa \\verb|x y z w| bb', 12), 'aa\n\\verb|x y z w| bb');
  // the comment moves whole: its own spaces never break
  assert.equal(wrapped('aa % a comment that is long', 10), 'aa\n% a comment that is long');
  assert.equal(wrapped('aaaa\\ bbbb', 6), 'aaaa\\ bbbb');
  const v = '\\begin{verbatim}\nthis line is long and has many spaces\n\\end{verbatim}';
  assert.equal(wrapped(v, 10, 20), v);
});

// Intuition: the length a document wraps at is read from its own wrapped lines, 80 when it keeps a paragraph per line.
// Pins a habit read from one long unwrapped paragraph.
test('smart wrap: the document\'s own line length', () => {
  const lines = Array.from({ length: 20 }, (_, i) => 'x'.repeat(60 + (i % 5)));
  assert.equal(R.lineHabit(`${lines.join('\n')}\n`), 64);
  assert.equal(R.lineHabit('one paragraph per line '.repeat(30)), 80);
});

// ---------------------------------------------------------------- insert
const DOC = '\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\section{Intro}\nFirst paragraph\nruns on.\n\nSecond one.\n\\end{document}\n';
/** Insert `kind` into `text` at target `t`: the text after, the plan. */
const insert = (text, kind, t) => {
  const it = R.inserts.find((k) => k.kind === kind), s = R.insertSpot(text, t, !!it.flow);
  const p = s && R.insertPlan(text, text, kind, s.at, s.mode);
  return p && { out: edit(text, p.edits), p };
};

// Intuition: a right click on a paragraph puts a new block after it, parted by blank lines, never inside it or glued
// to the next paragraph. Pins a table written into the middle of the clicked paragraph or run into the next one.
test('insert: a table after the clicked paragraph, in a float, caret in its caption, widget at the tabular', () => {
  const { out, p } = insert(DOC, 'table', { block: { to: 6, what: 'paragraph' } });
  assert.match(out, /runs on\.\n\n\\begin\{table\}\[htbp\]\n  \\centering\n  \\caption\{\}\n  \\label\{tab:name\}\n  \\begin\{tabular\}\{ll\}\n    Name & Value \\\\\n    \\hline\n    a & 1 \\\\\n  \\end\{tabular\}\n\\end\{table\}\n\nSecond one\./);
  assert.equal(out.slice(p.caret - 9, p.caret + 1), '\\caption{}');
  assert.ok(out.startsWith('\\begin{tabular}', p.open));
  assert.equal(p.widget, 'table');
  assert.equal(p.edits.length, 1);
  assert.ok(paired(out));
});

// Intuition: a figure is its content in a float, the caret in the caption and the widget offset at the content; the
// planner writes no \usepackage itself (the snapshot printer loads what the run lacks, else both load and overlap).
// Pins a client-side load that the server's own load overlapped ("two edits overlap").
test('insert: figures in a float, one edit, no load of their own', () => {
  const img = insert(DOC, 'figure-image', { block: { to: 8, what: 'paragraph' } });
  assert.equal(img.p.edits.length, 1);
  assert.match(img.out, /Second one\.\n\n\\begin\{figure\}\[htbp\]\n  \\centering\n  \\includegraphics\[width=0\.8\\linewidth\]\{example-image\}\n  \\caption\{\}\n  \\label\{fig:example-image\}\n\\end\{figure\}\n\\end\{document\}/);
  assert.equal(img.p.widget, 'image');
  assert.ok(img.out.startsWith('\\includegraphics', img.p.open));
  const pic = insert(DOC, 'figure-drawing', { block: { to: 8, what: 'paragraph' } });
  assert.equal(pic.p.edits.length, 1);
  assert.ok(!/usepackage\{tikz\}/.test(pic.out));
  assert.ok(pic.out.startsWith('\\begin{tikzpicture}', pic.p.open));
  assert.equal(pic.out.slice(pic.p.caret - 9, pic.p.caret + 1), '\\caption{}');
  assert.ok(paired(pic.out));
});

// Intuition: a blank place between paragraphs (PE.blankPlace's offset) takes blocks only; a mark of the running text
// (footnote, citation) needs a paragraph. Pins a footnote written into empty space between paragraphs.
test('insert: blank place takes blocks, marks need a paragraph', () => {
  const at = DOC.indexOf('runs on.') + 8;
  assert.deepEqual(R.insertSpot(DOC, { place: at }, false), { at, mode: 'after' });
  assert.equal(R.insertSpot(DOC, { place: at }, true), null);
  assert.equal(R.insertSpot(DOC, { block: { to: 4, what: 'heading' } }, true), null);
  const { out } = insert(DOC, 'itemize', { place: at });
  assert.match(out, /runs on\.\n\n\\begin\{itemize\}\n  \\item \n\\end\{itemize\}\n\nSecond one\./);
});

// Intuition: a footnote goes after the paragraph's last character, a citation or reference before the sentence's
// end with a tie; at the caret after a space no tie. Pins `.~\cite{}` and a footnote before the period.
test('insert: footnote after the sentence, citation before it with a tie', () => {
  const t = { block: { to: 6, what: 'paragraph' } };
  const f = insert(DOC, 'footnote', t);
  assert.match(f.out, /runs on\.\\footnote\{\}\n\nSecond/);
  assert.equal(f.out.slice(f.p.caret - 10, f.p.caret + 1), '\\footnote{}');
  assert.match(insert(DOC, 'cite', t).out, /runs on~\\cite\{\}\.\n/);
  const c = DOC.indexOf('Second ') + 7;
  assert.match(insert(DOC, 'ref', { caret: c }).out, /Second \\ref\{\}one\./);
});

// Intuition: at the caret a display (list, equation) splits the line onto its own lines without blank lines, so the
// paragraph goes on; a heading or float stands apart by blank lines; a heading takes the level of the one before.
// Pins a display glued into the middle of a line.
test('insert: at the caret, displays on own lines, headings at the level before', () => {
  const c = DOC.indexOf('paragraph') + 9;
  const eq = insert(DOC, 'equation', { caret: c });
  assert.match(eq.out, /First paragraph\n\\begin\{equation\}\n  \\end\{equation\}\nruns on\./);
  const h = insert(DOC, 'heading', { caret: c });
  assert.match(h.out, /First paragraph\n\n\\section\{\}\n\nruns on\./);
  const sub = DOC.replace('\\section{Intro}', '\\subsection{Intro}');
  assert.match(insert(sub, 'heading', { caret: sub.indexOf('Second') }).out, /\n\\subsection\{\}\n\nSecond one\./);
  assert.ok(paired(insert(DOC, 'align', { caret: c }).out) && paired(h.out));
});

// Intuition: every insert, at every kind of place in a document, keeps the environments paired and writes one edit,
// its caret and widget offsets inside the text. Pins a planner that breaks the
// document's structure for some kind or place.
test('insert: every kind at every place keeps the document paired', () => {
  const lines = DOC.split('\n').length;
  for (const it of R.inserts) for (let n = 4; n < lines - 1; n++) for (const t of [{ block: { to: n, what: 'paragraph' } }, { caret: DOC.split('\n').slice(0, n).join('\n').length }]) {
    const r = insert(DOC, it.kind, t);
    if (!r) continue;
    assert.ok(paired(r.out), `${it.kind} at ${JSON.stringify(t)}`);
    assert.ok(r.p.edits.length === 1 && r.p.caret >= 0 && r.p.caret <= r.out.length && r.p.open >= 0, it.kind);
  }
});

// Intuition: the menu of a paragraph offers every insert, a blank place or a figure only blocks; off in Settings,
// none. Pins marks offered between paragraphs and a switched-off plugin still offering rows.
test('insert: the menu rows by target, switched by the viewer', () => {
  const ctx = { text: DOC, all: DOC, x: 0, y: 0, facts: null, write() {}, toast() {}, pick() {}, colAlign() {}, insert() {} };
  const rows = (t, cfg) => PA.groups(t, ctx, cfg).filter((g) => g.title === 'Insert').flatMap((g) => g.items.flatMap((i) => i.items.map((r) => r.label)));
  assert.equal(rows({ kind: 'block', block: { file: 'a.tex', from: 5, to: 6, what: 'paragraph' } }).length, R.inserts.length);
  assert.ok(!rows({ kind: 'block', block: { file: 'a.tex', from: 5, to: 6, what: 'figure' } }).includes('Footnote'));
  assert.ok(!rows({ kind: 'block', place: 10, block: { file: 'a.tex', from: 5, to: 6, what: 'paragraph' } }).includes('Citation'));
  assert.ok(rows({ kind: 'caret', caret: 3 }).includes('Footnote'));
  assert.equal(rows({ kind: 'caret', caret: 3 }, { 'type_plugins.insert.enabled': false }).length, 0);
});

// Intuition: Insert is nested by kind (Math, Floats, Lists, Structure, References, Pictures, in that order), each row
// hinted with the LaTeX it writes, the heading's at the level of the last one; at the Type caret Math starts with
// inline, display and align* math in the document's inline convention. Pins a flat menu and hints that said "same level".
test('insert: rows nested by kind, hinted with their LaTeX', () => {
  const text = '\\begin{document}\n\\subsection{A}\nText.\n\\end{document}\n', ran = [];
  const ctx = { text, all: text, x: 0, y: 0, facts: null, write() {}, toast() {}, pick() {}, colAlign() {}, insert() {}, math: (k) => ran.push(k), mathInline: '$…$' };
  const menu = PA.groups({ kind: 'caret', caret: text.indexOf('Text') }, ctx).find((g) => g.title === 'Insert').items;
  assert.deepEqual(menu.map((g) => g.label), ['Math', 'Floats', 'Lists', 'Structure', 'References', 'Pictures']);
  const math = menu[0].items;
  assert.deepEqual(math.map((r) => [r.label, r.hint]).slice(0, 3), [['Inline math', '$…$'], ['Display math', '\\[…\\]'], ['Aligned', 'align*']]);
  math[1].run();
  assert.deepEqual(ran, ['display']);
  assert.equal(menu[3].items.find((r) => r.label === 'Section heading').hint, '\\subsection{…}');
  assert.equal(menu[1].items.find((r) => r.label === 'Table').hint, '\\begin{table} tabular');
  // no caret math outside the Type caret (a right click between paragraphs)
  assert.ok(!PA.groups({ kind: 'block', place: 3, block: { file: 'a.tex', from: 1, to: 2, what: 'paragraph' } }, ctx).find((g) => g.title === 'Insert').items[0].items.some((r) => r.label === 'Inline math'));
});

// Intuition: Insert at the caret puts a block where the caret is, unless that splits a group: a caret inside a macro's
// argument (`\greet{wo|rld}`) takes the block after the line the argument closes on; a footnote stays at the caret.
// Pins a table written into `\greet{wo…rld}`, a paragraph inside a short argument (sweep 2026-11-19).
test('insert: a block at the caret never splits a group', () => {
  const text = '\\begin{document}\n\\greet{world} and more\n\nNext.\n\\end{document}\n', at = text.indexOf('rld');
  assert.deepEqual(R.insertSpot(text, { caret: at }, false), { at: text.indexOf(' and more') + 9, mode: 'after' });
  assert.deepEqual(R.insertSpot(text, { caret: at }, true), { at, mode: 'caret' });
  const free = text.indexOf(' and');
  assert.deepEqual(R.insertSpot(text, { caret: free }, false), { at: free, mode: 'caret' });
  const { out } = insert(text, 'equation', { caret: at });
  assert.match(out, /\\greet\{world\} and more\n\n\\begin\{equation\}\n {2}\\end\{equation\}\n\nNext\./);
  assert.deepEqual(R.insertSpot('\\greet{wo', { caret: 8 }, false), { at: 8, mode: 'caret' }, 'a group never closed: at the caret');
});

// Intuition: a block needs paragraph mode and a float outer paragraph mode. With the caret in math, a tabular, a
// picture or any alignment (a body with a top-level `&`, which TeX takes only in an alignment, tex.web § 1128), the
// block goes after the line that box ends on; a float also leaves the float around it. A list item or a float's
// own body takes a display at the caret. Pins a figure written into a tabular cell or `$…$` ("Not in outer par mode").
test('insert: a block at the caret never lands in math, a table or a picture', () => {
  const after = (text, end) => ({ at: text.indexOf('\n', text.indexOf(end)), mode: 'after' });
  const spot = (text, mark, kind) => { const it = R.inserts.find((k) => k.kind === kind); return R.insertSpot(text, { caret: text.indexOf(mark) + 1 }, !!it.flow, !!it.wrap); };
  const tab = 'x\n\\begin{tabular}{ll}\na & b \\\\\n\\end{tabular}\ny\n';
  assert.deepEqual(spot(tab, 'b ', 'figure-image'), after(tab, '\\end{tabular}'), 'float: after the tabular');
  assert.deepEqual(spot(tab, 'b ', 'equation'), after(tab, '\\end{tabular}'), 'display: after the tabular');
  const flt = 'x\n\\begin{table}\n\\caption{c}\n\\begin{tabular}{ll}\na & b \\\\\n\\end{tabular}\n\\end{table}\ny\n';
  assert.deepEqual(spot(flt, 'b ', 'table'), after(flt, '\\end{table}'), 'a float never inside a float');
  assert.deepEqual(spot(flt, 'b ', 'itemize'), after(flt, '\\end{tabular}'), 'a list may stand in the float');
  const inl = 'so $a+b$ and more\n\nNext.\n';
  assert.deepEqual(spot(inl, 'a+', 'equation'), { at: inl.indexOf('\n'), mode: 'after' }, 'inline math');
  assert.deepEqual(spot(inl, 'a+', 'footnote'), { at: inl.indexOf('a+') + 1, mode: 'caret' }, 'a mark stays at the caret');
  const al = 'x\n\\begin{align}\n  a &= b\n\\end{align}\ny\n';
  assert.deepEqual(spot(al, 'a ', 'itemize'), after(al, '\\end{align}'));
  const own = 'x\n\\begin{mygrid}\n  a & b\n\\end{mygrid}\ny\n';
  assert.deepEqual(spot(own, 'a ', 'table'), after(own, '\\end{mygrid}'), 'an alignment of any name');
  const pic = 'x\n\\begin{tikzpicture}\n  \\node {A};\n\\end{tikzpicture}\ny\n';
  assert.deepEqual(spot(pic, 'node', 'figure-image'), after(pic, '\\end{tikzpicture}'));
  const box = 'x \\mbox{\\begin{tabular}{l} a \\end{tabular}} more\ny\n';
  assert.deepEqual(spot(box, ' a ', 'equation'), { at: box.indexOf(' more') + 5, mode: 'after' }, 'a box around the table, left too');
    assert.deepEqual(spot('x\n\\begin{itemize}\n  \\item one\n\\end{itemize}\n', 'one', 'equation').mode, 'caret', 'a list item takes it');
});

// Intuition: a selection's Format, Color, Font and Size rows wrap it in the command they name; in math the math
// commands (bold by the loaded package: \boldsymbol with amsmath), no text sizes; a color loads xcolor; Remove formatting
// unwraps the command around the selection. Pins the user's Format ▸ table (2026-11-21) and text commands offered in math.
test('format: a selection wrapped, math-aware, unwrapped', () => {
  const text = '\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\nSay \\textbf{hello} world and $x+y$.\n\\end{document}\n';
  const written = [];
  const ctx = { text, all: text, x: 0, y: 0, facts: null, write: (e) => written.push(edit(text, e)), toast() {}, pick() {}, colAlign() {} };
  const menu = (from, to) => Object.fromEntries(PA.groups({ kind: 'source', at: from, end: to, sel: [from, to] }, ctx).filter((g) => ['Format', 'Color', 'Font', 'Size'].includes(g.title)).map((g) => [g.title, g.items]));
  const w = text.indexOf('world'), m = menu(w, w + 5);
  assert.deepEqual(Object.keys(m), ['Format', 'Color', 'Font', 'Size']);
  assert.deepEqual(m.Format.map((r) => r.hint), ['\\textbf', '\\textit', '\\emph', '\\texttt', '\\textsc'], 'no underline without ulem or soul');
  m.Format[0].run();
  assert.match(written.pop(), /\\textbf\{world\} and/);
  m.Size.find((r) => r.label === 'large').run();
  assert.match(written.pop(), /\{\\large world\} and/);
  m.Color.find((r) => r.label === 'red').run();
  const colored = written.pop();
  assert.match(colored, /\\textcolor\{red\}\{world\}/); assert.match(colored, /\\usepackage\{xcolor\}\n/);
  const x = text.indexOf('x+y'), mm = menu(x, x + 3);
  assert.deepEqual(mm.Format.map((r) => r.hint), ['\\boldsymbol', '\\mathrm', '\\mathsf', '\\mathtt']);
  assert.ok(!mm.Size && !mm.Font, 'no text sizes or families in math');
  mm.Color.find((r) => r.label === 'red').run();
  assert.match(written.pop(), /\$\{\\color\{red\}x\+y\}\$/, 'a group where the kernel date is unknown');
  const h = text.indexOf('hello'), um = menu(h + 1, h + 3);
  um.Format.find((r) => r.label === 'Remove formatting').run();
  assert.match(written.pop(), /Say hello world/);
});

// Intuition: every Insert row writes a document that builds before anything is typed. A body line of blanks is an
// empty line, a \par: Insert ▸ Aligned equations wrote `\begin{align}\n  \n\end{align}` and the build failed
// ("Paragraph ended before \align was complete"). No environment an insert opens holds a blank line.
test('insert: no blank line inside an environment, every kind, caret and after a paragraph', () => {
  for (const it of R.inserts) for (const t of [{ caret: DOC.indexOf('runs on') - 1 }, { block: { to: 6, what: 'paragraph' } }]) {
    const r = insert(DOC, it.kind, t);
    if (!r) continue;
    for (const m of r.out.matchAll(/\\begin\{(\w+\*?)\}[^]*?\\end\{\1\}/g)) if (m[1] !== 'document') assert.ok(!/\n[ \t]*\n/.test(m[0]), `${it.kind}: ${JSON.stringify(m[0])}`);
  }
});

// Intuition: Ctrl+B over a selection on the page bolds its source, and again unbolds it; the caret stays at the
// argument's end inside, so Right leaves the call (`\textbf{bold}X`). Pins Ctrl+B over a page selection doing nothing.
test('format: Ctrl+B wraps a selection and unwraps its own call', () => {
  const t = 'make bold later', a = t.indexOf('bold'), b = a + 4;
  const on = R.formatToggle(t, a, b, 'textbf'), w = edit(t, [on.edit]);
  assert.equal(w, 'make \\textbf{bold} later');
  assert.equal(w.slice(on.caret), '} later');
  const off = R.formatToggle(w, w.indexOf('bold'), w.indexOf('bold') + 4, 'textbf');
  assert.equal(edit(w, [off.edit]), t);
  // another command around it, or only part of its argument: wrapped again
  assert.equal(edit(w, [R.formatToggle(w, w.indexOf('bold'), w.indexOf('bold') + 4, 'textit').edit]), 'make \\textbf{\\textit{bold}} later');
  assert.equal(edit(w, [R.formatToggle(w, w.indexOf('bold'), w.indexOf('bold') + 2, 'textbf').edit]), 'make \\textbf{\\textbf{bo}ld} later');
});
