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
