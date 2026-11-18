// The table dialog's model (dialogs.ts `table`): random grids under random runs of merges, splits, inserts, deletes
// and moves; after each the grid stays rectangular, every slot has one owner, and no cell's text is lost or doubled.
import assert from 'node:assert/strict';
import { test, rng } from './harness.mjs';

const T = self.satexDialogs.table;
const cell = (text) => ({ text, span: 1, spec: '' });
/** A grid of `r` rows and `c` columns, every cell a distinct word. */
const grid = (r, c) => ({
  env: 'tabular', cols: Array.from({ length: c }, () => ({ align: 'l', arg: '', pre: '', post: '' })), seps: Array(c + 1).fill(''),
  rows: Array.from({ length: r }, (_, i) => ({ before: i === 1 ? ['\\hline'] : [], inline: false, cells: Array.from({ length: c }, (_, j) => cell(`w${i}x${j}`)), end: '\\\\', comment: '', raw: '' })),
  after: [], style: { indent: '  ', spaced: true, aligned: false, inline_rules: false },
});
/** The words of every cell, sorted: what no merge, split or move may lose. */
const words = (s) => s.rows.flatMap((r) => r.cells.flatMap((c) => T.inner(c).split(/\s+/).filter(Boolean))).sort();

/** Why `s` is no proper grid, '' when it is one. */
function bad(s) {
  if (s.seps.length !== s.cols.length + 1) return `seps ${s.seps.length} for ${s.cols.length} columns`;
  const w = s.rows.map((r) => T.width(r));
  if (w.some((x) => x !== s.cols.length)) return `row widths ${w} for ${s.cols.length} columns`;
  const map = T.layout(s);
  for (let r = 0; r < s.rows.length; r++) for (let c = 0; c < s.cols.length; c++) {
    const o = map[r][c];
    if (!o) return `no cell at ${r},${c}`;
    if (o.r > r || o.c > c || o.r + o.rows <= r || o.c + o.span <= c) return `the owner of ${r},${c} does not cover it`;
  }
  return '';
}

// Intuition: merges and splits only regroup cells: the grid stays rectangular, each slot owned once, every word kept.
// Pins a merge that dropped the text of a cell it covered and a split that left a row a cell short.
test('merge and split runs keep a proper grid and every word', () => {
  const R = rng(51);
  for (let k = 0; k < 150; k++) {
    const s = grid(2 + R.int(4), 2 + R.int(4)), all = words(s);
    for (let j = 0; j < 8; j++) {
      const rect = [R.int(s.rows.length), R.int(s.cols.length), R.int(s.rows.length), R.int(s.cols.length)];
      const n = T.normalize(s, rect);
      assert.deepEqual(T.normalize(s, n), n, 'normalize is idempotent');
      assert.ok(n[0] <= Math.min(rect[0], rect[2]) && n[2] >= Math.max(rect[0], rect[2]) && n[1] <= Math.min(rect[1], rect[3]) && n[3] >= Math.max(rect[1], rect[3]), 'it only grows');
      if (R.chance(0.6)) T.merge(s, rect); else T.split(s, rect);
      assert.equal(bad(s), '', JSON.stringify(s.rows.map((r) => r.cells.map((c) => `${c.span}:${c.text}`))));
      assert.deepEqual(words(s), all);
    }
    // all split again: one cell per slot
    T.split(s, [0, 0, s.rows.length - 1, s.cols.length - 1]);
    assert.ok(s.rows.every((r) => r.cells.length === s.cols.length && r.cells.every((c) => c.span === 1 && T.rowsOf(c) === 1)));
  }
});

// Intuition: inserting and deleting rows and columns keeps the grid proper around merged cells (a `\multicolumn` grows
// or shrinks, a `\multirow` reaching in shrinks); the table never loses its last row or column.
test('insert and delete runs around merged cells', () => {
  const R = rng(52);
  for (let k = 0; k < 150; k++) {
    const s = grid(2 + R.int(4), 2 + R.int(4));
    for (let j = 0; j < 10; j++) {
      const op = R.int(6), rows = s.rows.length, cols = s.cols.length;
      if (op === 0) T.merge(s, [R.int(rows), R.int(cols), R.int(rows), R.int(cols)]);
      else if (op === 1) { T.insertRow(s, R.int(rows + 1)); assert.equal(s.rows.length, rows + 1); }
      else if (op === 2) { T.insertCol(s, R.int(cols + 1)); assert.equal(s.cols.length, cols + 1); }
      else if (op === 3) { const a = R.int(rows), b = Math.min(rows - 1, a + R.int(2)); assert.equal(T.deleteRows(s, a, b), b - a + 1 < rows); }
      else if (op === 4) { const a = R.int(cols), b = Math.min(cols - 1, a + R.int(2)); assert.equal(T.deleteCols(s, a, b), b - a + 1 < cols); }
      else { const r = R.int(rows), d = R.pick([-1, 1]), before = words(s); T.moveRow(s, r, d); assert.deepEqual(words(s), before); }
      assert.ok(s.rows.length >= 1 && s.cols.length >= 1);
      assert.equal(bad(s), '', `op ${op}: ${JSON.stringify(s.rows.map((r) => r.cells.map((c) => `${c.span}:${c.text}`)))}`);
    }
  }
});

// Intuition: moving a row or a column there and back is no change; a column move across a merged cell is refused.
test('moves there and back are no change', () => {
  const R = rng(53);
  for (let k = 0; k < 100; k++) {
    const s = grid(2 + R.int(3), 2 + R.int(3));
    if (R.chance(0.5)) T.merge(s, [0, 0, 0, 1]);
    const was = JSON.stringify(s), r = R.int(s.rows.length), c = R.int(s.cols.length), d = R.pick([-1, 1]);
    if (T.moveRow(s, r, d)) { assert.notEqual(JSON.stringify(s), was); T.moveRow(s, r + d, -d); }
    assert.equal(JSON.stringify(s), was, 'row back');
    if (T.moveCol(s, c, d)) T.moveCol(s, c + d, -d);
    assert.equal(JSON.stringify(s), was, 'column back');
    assert.equal(bad(s), '');
  }
  const s = grid(2, 3);
  T.merge(s, [0, 0, 0, 1]);
  assert.equal(T.moveCol(s, 1, 1), false, 'a column under a \\multicolumn stays');
});

// Intuition: the CSV export read back gives the same cells for any text, specials included. Pins `A\&B` exported as
// `A\&B` and read back as `A\textbackslash{}\&B`.
test('CSV round trip over random cell texts', () => {
  const R = rng(54), parts = ['a', 'b c', '50\\%', 'A\\&B', '\\_x', '"q"', ',', '\\textbf{y}', '$x_1$'];
  for (let k = 0; k < 100; k++) {
    const s = grid(1 + R.int(3), 1 + R.int(3));
    for (const r of s.rows) for (const c of r.cells) c.text = Array.from({ length: 1 + R.int(2) }, () => R.pick(parts)).join(' ');
    const back = grid(s.rows.length, s.cols.length);
    T.importText(back, T.toCSV(s));
    const plain = (t) => T.plain(t);
    assert.deepEqual(back.rows.map((r) => r.cells.map((c) => plain(c.text))), s.rows.map((r) => r.cells.map((c) => plain(c.text))));
  }
});

// Intuition: a column spec's alignment is its letter, never one inside an inject or an argument.
test('alignOf: the letter past injects and arguments', () => {
  const cases = { l: 'l', '>{\\bfseries}c': 'c', '>{\\raggedleft\\arraybackslash}p{3cm}': 'p', '@{}r@{}': 'r', 'm{2cm}': 'm', '!{\\vrule}b{1cm}': 'b', 'X': 'X', '>{\\bf}S': 'S', '': 'l' };
  for (const [spec, want] of Object.entries(cases)) assert.equal(T.alignOf(spec), want, spec);
  assert.equal(T.injected('>{\\bf}>{\\it}l<{x}'), '\\bf\\itx');
});
