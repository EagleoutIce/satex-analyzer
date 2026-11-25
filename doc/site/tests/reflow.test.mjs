// Line breaking while typing (pdfedit.ts `reflow`, `centered`): the echo of a typed word breaks rows as TeX would
// without hyphenation. Written first as a spec; each test skips only while the function is missing (it is new).
import assert from 'node:assert/strict';
import { test, rng } from './harness.mjs';

const E = self.satexPdfEdit;
/** Runs `f` when `reflow` exists; until then the spec is said skipped, never passed silently. */
const spec = (name, f) => test(name, () => {
  if (typeof E.reflow !== 'function') { console.log(`  skipped (no pdfedit reflow yet): ${name}`); return; }
  f();
});
const p = (n, gap = 3, extra = {}) => ({ n, gap, ...extra });
const LINE = { right: 100, left: 0, indent: 15, skip: 12, parskip: 0, rows: [] };
/** Each row as `x:[piece x…]`, compact to compare. */
const shape = (flow) => flow.rows.map((r) => `${r.from}@${r.x}:${r.out.map((o) => o.x).join(',')}${r.par ? ' par' : ''}${r.dy ? ` dy${r.dy}` : ''}`);

// Intuition: a piece ending exactly at the right edge stays on its row (TeX fills a line to its width).
spec('exact fit stays on the row', () => {
  assert.deepEqual(shape(E.reflow([p(47, 0), p(50)], 0, LINE)), ['-2@0:0,50']);
});
// Intuition: one point more and it starts the next row at the line's left; past the page's rows a new row a line lower.
spec('overflow starts a new row', () => {
  assert.deepEqual(shape(E.reflow([p(47, 0), p(51)], 0, LINE)), ['-2@0:0', '-1@0:0 dy12']);
});
// Intuition: overflow runs on into the paragraph's next page row: the piece goes first there, that row's words after it.
spec('overflow runs into the next page row', () => {
  const line = { ...LINE, rows: [{ x: 5, pieces: [p(20), p(20)] }] };
  const f = E.reflow([p(60, 0), p(39)], 0, line);
  assert.deepEqual(shape(f), ['-2@0:0', '0@5:5,47,70']);
  assert.deepEqual(f.freed, []);
});
// Intuition: Enter starts a new paragraph: indented, a line plus the paragraph skip lower, whatever room was left.
// Intuition: a display after a row that ends left of it (an empty row after Enter) gets TeX's short skip above it, after
// a full row the display skip. Pins display-math: the echoed display 9 bp below where the build set it (a 10 px caret jump).
spec('display: the short skip after a short row', () => {
  const d = (x) => ({ n: 20, gap: 0, disp: 8, dshort: 0, ...x });
  assert.deepEqual(shape(E.reflow([p(10, 0), d()], 0, LINE)), ['-2@0:0', '-1@40:40 dy12']);
  assert.deepEqual(shape(E.reflow([p(95, 0), d()], 0, LINE)), ['-2@0:0', '-1@40:40 dy20']);
});
spec('Enter: a new indented paragraph', () => {
  const f = E.reflow([p(10, 0), p(10, 3, { par: true }), p(10)], 0, { ...LINE, parskip: 4 });
  assert.deepEqual(shape(f), ['-2@0:0', '-1@15:15,28 par dy16']);
});
// Intuition: a piece wider than the line (a long formula, a URL) stands alone on its row, never loops or vanishes.
spec('a piece wider than the line stands alone', () => {
  assert.deepEqual(shape(E.reflow([p(10, 0), p(150), p(10)], 0, LINE)), ['-2@0:0', '-1@0:0 dy12', '-1@0:0 dy12']);
});
// Intuition: a zero-width piece (a math shift, an empty group) never breaks a row on its own.
spec('a zero-width piece never breaks', () => {
  assert.deepEqual(shape(E.reflow([p(100, 0), p(0, 0)], 0, LINE)), ['-2@0:0,100']);
});
// Intuition: deleting makes room; with `pull` the row takes up the next row's words while they fit, and a row whose
// words all went up is freed (the echo hides it).
spec('pull: words move up after a deletion, emptied rows freed', () => {
  const line = { ...LINE, pull: true, rows: [{ x: 0, pieces: [p(30), p(30)] }, { x: 0, pieces: [p(90)] }] };
  const f = E.reflow([p(20, 0)], 0, line);
  assert.deepEqual(shape(f), ['-2@0:0,23,56']);
  assert.deepEqual(f.freed, [0], 'the first page row emptied, the second untouched (90 does not fit)');
});
// Intuition: a centered row (a caption) grows to both sides: it moves by half its growth; a left-aligned one stays.
spec('centered: half the growth for a centered row only', () => {
  assert.equal(E.centered([0, 100], [20, 80], 10), 5);
  assert.equal(E.centered([0, 100], [0, 80], 10), 0, 'flush left');
  assert.equal(E.centered([0, 100], [3, 97], 10), 0, 'a full row: no room to tell');
  assert.equal(E.centered([0, 100], [20, 70], 10), 0, 'off centre');
});

// Intuition (properties over seeded paragraphs): the pieces come out in order, all of them, each row's pieces spaced by
// their gaps, none past the right edge but a lone one; a laid-out first row reflowed again stays as it is (idempotence).
spec('properties: order, spacing, edges, idempotence', () => {
  const R = rng(31);
  for (let k = 0; k < 300; k++) {
    const pieces = Array.from({ length: 1 + R.int(20) }, (_, i) => ({ id: i, n: R.chance(0.1) ? 0 : 1 + R.int(40), gap: i ? 2 + R.int(4) : 0, par: i > 0 && R.chance(0.05) }));
    const line = { right: 60 + R.int(100), left: R.int(10), indent: R.int(15), skip: 12, parskip: R.int(5), rows: [] };
    const f = E.reflow(pieces, line.left, line), out = f.rows.flatMap((r) => r.out);
    assert.deepEqual(out.map((o) => o.id), pieces.map((q) => q.id), 'all, in order');
    for (const r of f.rows) {
      r.out.forEach((o, j) => { if (j) assert.equal(o.x, r.out[j - 1].x + r.out[j - 1].n + o.gap, 'spaced by its gap'); });
      assert.equal(r.out[0].x, r.x, 'a row starts at its left edge');
      const ink = r.out.filter((o) => o.n > 0);
      if (ink.length > 1) assert.ok(ink.at(-1).x + ink.at(-1).n <= line.right + 0.5, `within the edge: ${JSON.stringify([line, r])}`);
    }
    // the layout's rows as page rows: the first row's pieces typed, the rest as the page set them
    const [first, ...rest] = f.rows;
    if (rest.some((r) => r.par)) continue;
    const again = E.reflow(first.out.map(({ x, ...q }) => q), first.x, { ...line, rows: rest.map((r) => ({ x: r.x, pieces: r.out.map(({ x, ...q }) => q) })) });
    assert.deepEqual(again.rows.map((r) => r.out.map((o) => [o.id, o.x])), [first.out.map((o) => [o.id, o.x])], 'idempotent: a laid-out row spills nothing');
  }
});

// Intuition: TeX shrinks a full row's spaces (a third of each for Computer Modern) before it breaks: a piece that
// fits only with shrunk spaces stays, and the row is set to the edge, its spaces sharing the shortfall. Pins greedy
// breaking at natural widths, a word earlier than TeX (a 19 px jump when the build landed).
spec('shrink keeps a piece a full row holds with shrunk spaces', () => {
  const f = E.reflow([p(40, 0), p(28), p(28)], 0, { ...LINE, shrink: 1 / 3 });
  assert.equal(f.rows.length, 1, 'one row: 102 natural, 100 with the spaces shrunk by 2');
  const last = f.rows[0].out.at(-1);
  assert.ok(Math.abs(last.x + last.n - 100) < 1e-9, 'set to the edge');
  assert.equal(E.reflow([p(40, 0), p(28), p(28)], 0, LINE).rows.length, 2, 'without shrink it breaks');
});
// Intuition: a row TeX broke for the next is justified: it ends at the right edge, the slack shared by its spaces.
// The first row is left as the page set its words before the typed text; the paragraph's last row stays ragged.
spec('justify sets a broken row to the edge, never the first or the last', () => {
  const f = E.reflow([p(60, 0), p(50), p(30), p(50)], 0, { ...LINE, justify: true });
  assert.deepEqual(f.rows.map((r) => r.out.length), [1, 2, 1]);
  const end = (r) => r.out.at(-1).x + r.out.at(-1).n;
  assert.ok(Math.abs(end(f.rows[1]) - 100) < 1e-9, 'the middle row reaches the edge');
  assert.equal(end(f.rows[2]), 50, 'the last row ragged');
  assert.equal(end(f.rows[0]), 60, 'the first row as laid');
});
/** Each piece of `prose` as `text`, `_` before it for a gap, `¶` a blank line, `:ctx` its context. */
const prose = (raw) => E.prose(raw).map((q) => `${q.par ? '¶' : ''}${q.gap ? '_' : ''}${q.t}${q.ctx ? `:${q.ctx}` : ''}`).join(' ');
// Intuition: typed prose reads as the page sets it: commands dropped, a space typed is the caret's empty word, the first
// piece joins the word before. Pins the echo's words equal to the old plain-text split.
test('prose: plain words, the empty first and last piece', () => {
  assert.equal(prose(' Quokka'), ' _Quokka');
  assert.equal(prose('a \\foo  b '), 'a _b:foo _:foo');
  assert.equal(prose('x\n\ny'), 'x ¶_y');
  assert.equal(prose('50\\% off % gone'), '50% _off _');
});
// Intuition: a word in a command's argument or after a group's declaration is set in the font the run chose there, which
// the playground reads off the page: the piece names that context, no table of font commands. Pins tc-type-textbf:
// \textbf{Quokka} measured upright, the caret 4 px short of where the build set it.
test('prose: the context of a command argument or a group declaration', () => {
  assert.equal(prose(' \\textbf{Quokka} x'), ' _Quokka:textbf _x');
  // (no gap before Big: TeX skips the space after `\\bfseries`)
  assert.equal(prose('{\\bfseries Big} small'), 'Big:bfseries _small');
  assert.equal(prose('Q\\textbf{uo}kka'), 'Q uo:textbf kka');
  assert.equal(prose('{a \\bfseries b} c'), 'a _b:bfseries _c');
  assert.equal(prose('a \\foo b'), 'a _b:foo');
});
// Intuition: an environment's arguments are read by its `\begin`, never set. Pins the thesis's table (user 2026-11-25):
// the echo of `runs.` ran on into the float and showed `runs. [ht] l>p3.9cmr`.
test('prose: an environment\'s arguments set nothing', () => {
  assert.equal(prose('runs.\n\\begin{table}[ht]\n\\centering\n\\begin{tabular}{l>{\\raggedleft\\arraybackslash}p{3.9cm}r}\nDoc'), 'runs. _Doc:centering');
  assert.equal(prose('\\begin{center} x \\end{center}'), ' _x _');
});
// Intuition: TeX skips the spaces after a control word (one line end too, never a blank line), and a symbol command
// sets its glyph. Pins title-markup: ` a\S $x_1` echoed as `a x` (a space, no §), the caret 6 px short of the build's.
test('prose: a control word eats its spaces, a symbol sets its glyph', () => {
  const sym = (n) => ({ S: '§' })[n];
  const p = (raw) => E.prose(raw, undefined, [], sym).map((q) => `${q.par ? '¶' : ''}${q.gap ? '_' : ''}${q.t}`).join(' ');
  assert.equal(p(' a\\S x'), ' _a§x');
  assert.equal(p('a\\foo\n b'), 'a b');
  assert.equal(p('a\\foo\n\nb'), 'a ¶_b');
});
// Intuition: a command the run sets as a box of its own (`\TeX`: T, a kern, a lowered E, X) is one piece holding that
// box, its spaces eaten, the words around it apart. Pins the echo dropping text macros until the build.
test('prose: a command set as a box is a piece of its own', () => {
  const box = { w: 1.6 }, sym = (n) => (n === 'TeX' ? { t: 'TEX', box } : undefined);
  const q = E.prose('a \\TeX{} b', undefined, [], sym);
  assert.deepEqual(q.map((x) => `${x.gap ? '_' : ''}${x.t}${x.box ? '#' : ''}`), ['a', '_TEX#', '_b']);
  assert.equal(q[1].box, box);
});
// Intuition: an environment's name is a csname the kernel reads, never text on the row (`center Quokka center`).
test('prose: \\begin and \\end set no word', () => {
  assert.equal(prose(' \\begin{center}\nQuokka\n\\end{center} more'), ' _Quokka _more');
});
// Intuition: two places typed since the shown run are two edits of whole words, the words between them untouched, so
// the Typeset view shows both at once. Pins fz-ts-end-keeps-typed: one change over both lost the second word.
test('hunks: each place typed is its own edit of whole words', () => {
  const was = 'Another line of plain prose follows it.';
  assert.deepEqual(E.hunks(was, 'Another line of plainxq prose follows it.'), [[16, 21, 'plainxq']]);
  assert.deepEqual(E.hunks(was, 'Another line of plainxq prose fyy follows it.'), [[16, 21, 'plainxq'], [22, 27, 'prose fyy']]);
  assert.deepEqual(E.hunks(was, 'Another line of plainxq xqprose follows it.'), [[16, 27, 'plainxq xqprose']]);
  assert.deepEqual(E.hunks(was, 'Xx Another line of plain prose follows it.'), [[0, 7, 'Xx Another']]);
  assert.deepEqual(E.hunks(was, was), []);
  for (const [w, n] of [[was, 'Another plain prose it.'], ['a b c', 'x a y b z c w'], ['one two', 'one three two four']]) {
    let out = w;
    for (const [f, t, x] of [...E.hunks(w, n)].reverse()) out = out.slice(0, f) + x + out.slice(t);
    assert.equal(out, n, `${w} -> ${n}`);
  }
});

// Intuition: `\\` ends the row there, whatever room is left, and the next words start the next row of the same paragraph
// at its margin: no indent, no \parskip (ltspace.dtx `\@normalcr`). `\\*` and `\\[2pt]` too. Pins Ctrl+Enter's `\\`
// typed in Type mode echoing the next word on the same row, the caret jumping to the next row once the build landed.
test('prose and reflow: a forced line break', () => {
  const brk = (raw) => E.prose(raw).map((q) => `${q.brk ? '↵' : ''}${q.gap ? '_' : ''}${q.t}`).join(' ');
  assert.equal(brk('second\\\\\nQuokka'), 'second ↵_Quokka');
  assert.equal(brk('a\\\\*[2pt] b'), 'a ↵_b');
  assert.equal(brk('a\\\\\\\\ b'), 'a ↵ ↵_b', 'two breaks: an empty row between');
  assert.ok(!E.prose('a \\, b').some((q) => q.brk), 'a thin space is no break');
  assert.deepEqual(shape(E.reflow([p(10, 0), p(10, 3, { brk: true })], 0, { ...LINE, parskip: 4 })), ['-2@0:0', '-1@0:0 dy12']);
});
// Intuition: a paragraph typed after Enter is broken by TeX's total fit too (not greedily): a formula in it breaks
// after its relation at the lower penalty, its operators' spaces shrunk, as pdfTeX did in the probe
// (target/scratch/preview/probe/math3: greedy broke after `h+`, pdfTeX after `=`). Pins preview wish 3.
spec('a typed last paragraph takes the total fit', () => {
  // the new paragraph: a word of 44, `a+` `b+` `c=` (10, 10, 16 wide, operator spaces 2 that shrink to 0), `d` after
  // the relation's space (3, no shrink), a word of 20; 85 bp after the indent: up to `=` is 87, 2 to shrink
  const pieces = [p(30, 0), p(44, 3, { par: true }), { gap: 3, n: 10, pen: 700 }, { gap: 2, n: 10, pen: 700, mg: true, st: 1, sh: 2 }, { gap: 2, n: 16, pen: 500, mg: true, st: 1, sh: 2 }, { gap: 3, n: 10, mg: true, st: 3, sh: 0 }, p(20)];
  const f = E.reflow(pieces, 30, { ...LINE, rows: [], fit: true });
  assert.equal(f.rows.length, 3, 'the old row, then the paragraph in two rows');
  assert.equal(f.rows[1].out.at(-1).pen, 500, 'its first row ends after the relation');
  const end = (r) => r.out.at(-1).x + r.out.at(-1).n;
  assert.ok(Math.abs(end(f.rows[1]) - 100) < 1e-9, 'justified to the edge');
});
// Intuition: text typed inside a command's argument keeps that command's font, text after its `}` the paragraph's:
// prose starts in the groups the caret stands in (`outer`). Pins "Next words" echoed bold after Enter at `\textbf{world|}`.
test('prose: the groups the typed text starts in', () => {
  const ctx = (raw, outer) => E.prose(raw, undefined, outer).map((q) => `${q.t}:${q.ctx}`).join(' ');
  assert.equal(ctx('ld}\n\nNext', ['textbf']), 'ld:textbf Next:');
  assert.equal(ctx('more', ['emph']), 'more:emph');
});
