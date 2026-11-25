// The PDF table controls' edits (pdfedit.ts tableOp on the dialog model): insert, delete and move rows and columns,
// spans grown and shrunk, rules and row ends where they were, a refused edit leaving the table as it was.
import assert from 'node:assert/strict';
import { test, rng } from './harness.mjs';

const T = self.satexDialogs.table, op = self.satexPdfEdit.tableOp;
const cell = (text, span = 1, spec = '') => ({ text, span, spec });
const row = (cells, before = [], end = '\\\\') => ({ before, inline: false, cells, end, comment: '', raw: '' });
/** A 3x3 table: `\toprule` over the header, `\midrule` under it, the last row ending in `\\[2pt]`. */
const grid = () => ({
  env: 'tabular', cols: [{ align: 'l', arg: '', pre: '', post: '' }, { align: 'p', arg: '3cm', pre: '', post: '' }, { align: 'X', arg: '', pre: '', post: '' }], seps: ['|', '', '@{}', ''],
  rows: [row([cell('a'), cell('b'), cell('c')], ['\\toprule']), row([cell('d'), cell('e'), cell('f')], ['\\midrule']), row([cell('g'), cell('h'), cell('i')], [], '\\\\[2pt]')],
  after: ['\\bottomrule'], style: { indent: '  ', spaced: true, aligned: false, inline_rules: false },
});
const texts = (s) => s.rows.map((r) => r.cells.map((c) => c.text).join(' '));

// Intuition: a column inserted right of a p{3cm} column is a p{3cm} too; each row gets one empty cell there; the spec's
// rules stay with the columns they stood by (seps grows by one). Pins a new column without a cell in some row.
test('insert a column: a cell per row, the neighbour\'s kind', () => {
  const s = grid();
  assert.ok(op(T, s, { what: 'col', act: 'insert', at: 2 }));
  assert.deepEqual(s.cols.map((c) => c.align + c.arg), ['l', 'p3cm', 'p3cm', 'X']);
  assert.equal(s.seps.length, 5);
  assert.deepEqual(texts(s), ['a b  c', 'd e  f', 'g h  i']);
  assert.ok(op(T, s, { what: 'col', act: 'insert', at: 0 }));
  assert.equal(s.cols[0].align, 'l');
  assert.ok(s.rows.every((r) => T.width(r) === 5));
});

// Intuition: deleting a column a \multicolumn spans shrinks the span by one, never splits or drops its text.
test('delete a column under a span shrinks it', () => {
  const s = grid();
  s.rows[0].cells = [cell('a'), cell('wide', 2, 'c')];
  assert.ok(op(T, s, { what: 'col', act: 'delete', at: 2 }));
  assert.deepEqual(s.rows[0].cells, [cell('a'), cell('wide', 1, 'c')]);
  assert.deepEqual(texts(s).slice(1), ['d e', 'g h']);
  // inserting inside a span grows it
  assert.ok(op(T, s, { what: 'col', act: 'insert', at: 2 }));
  s.rows[0].cells[1].span = 2;
  assert.ok(op(T, s, { what: 'col', act: 'insert', at: 2 }));
  assert.equal(s.rows[0].cells[1].span, 3);
});

// Intuition: a row moved by drag takes its cells, the rule under it and its own row end (`\\[2pt]`); the rules at the
// table's edges (over the first row, under the last) stay there. Pins a move that left the header's \midrule behind.
test('move a row: cells, the rule under it and its row end travel', () => {
  const s = grid();
  assert.ok(op(T, s, { what: 'row', act: 'move', at: 0, to: 1 }));
  assert.deepEqual(texts(s), ['d e f', 'a b c', 'g h i']);
  assert.deepEqual(s.rows.map((r) => r.before), [['\\toprule'], [], ['\\midrule']]);
  assert.ok(op(T, s, { what: 'row', act: 'move', at: 2, to: 0 }));
  assert.deepEqual(texts(s), ['g h i', 'd e f', 'a b c']);
  assert.equal(s.rows[0].end, '\\\\[2pt]');
  assert.deepEqual(s.rows[0].before, ['\\toprule']);
  const back = grid();
  assert.ok(op(T, back, { what: 'row', act: 'move', at: 0, to: 2 }) && op(T, back, { what: 'row', act: 'move', at: 2, to: 0 }));
  assert.deepEqual(back, grid(), 'there and back is no change');
});

// Intuition: a column moved by drag takes the inner rule right of it (`|`, `@{}`) and a one-column \cline under it;
// the table's outer rules stay. Pins `{|l|cr|}` dragged to `{|cr|l|}`-like specs that lost or doubled a rule.
test('move a column: its rule and its partial rules travel', () => {
  const s = grid();
  s.rows[1].before = ['\\cline{1-1}', '\\cmidrule{2-3}'];
  assert.ok(op(T, s, { what: 'col', act: 'move', at: 0, to: 1 }));
  assert.deepEqual(s.cols.map((c) => c.align), ['p', 'l', 'X']);
  assert.deepEqual(s.seps, ['|', '@{}', '', '']);
  assert.deepEqual(s.rows[1].before, ['\\cline{2-2}', '\\cmidrule{2-3}']);
  assert.ok(op(T, s, { what: 'col', act: 'move', at: 1, to: 0 }));
  assert.deepEqual(s, (() => { const g = grid(); g.rows[1].before = ['\\cline{1-1}', '\\cmidrule{2-3}']; return g; })(), 'there and back is no change');
});

// Intuition: refused edits (the last column, past the ends, a column move across a span, a row move through a
// \multirow) leave the table byte for byte as it was, even when the refusal comes after some steps.
test('refused edits change nothing', () => {
  const s = grid();
  s.rows[1].cells = [cell('d'), cell('e'), cell('\\multirow{2}{*}{f}')];
  s.rows[2].cells[2] = cell('');
  s.rows[0].cells = [cell('a'), cell('wide', 2, 'c')];
  const was = JSON.stringify(s);
  for (const o of [{ what: 'col', act: 'move', at: 0, to: 2 }, { what: 'row', act: 'move', at: 0, to: 2 }, { what: 'col', act: 'delete', at: 3 },
    { what: 'row', act: 'move', at: 1, to: 1 }, { what: 'row', act: 'insert', at: 9 }, { what: 'col', act: 'move', at: 1, to: -1 }]) {
    assert.equal(op(T, s, o), false, JSON.stringify(o));
    assert.equal(JSON.stringify(s), was, JSON.stringify(o));
  }
  const one = { ...grid(), cols: [grid().cols[0]], seps: ['', ''], rows: [row([cell('x')])] };
  assert.equal(op(T, one, { what: 'col', act: 'delete', at: 0 }), false);
  assert.equal(op(T, one, { what: 'row', act: 'delete', at: 0 }), false);
});

// Intuition: random runs of the controls keep a proper grid (every row as wide as the spec) and no word is lost but
// the ones a delete took. Pins drift between the spec's columns and the rows' cells.
test('random control runs keep the grid', () => {
  const R = rng(71);
  for (let k = 0; k < 200; k++) {
    const s = grid();
    if (R.chance(0.5)) s.rows[R.int(3)].cells = [cell('m', 2, 'c'), cell('z')];
    for (let j = 0; j < 8; j++) {
      const what = R.pick(['row', 'col']), n = what === 'row' ? s.rows.length : s.cols.length, act = R.pick(['insert', 'delete', 'move']);
      op(T, s, { what, act, at: R.int(n + (act === 'insert' ? 1 : 0)), to: R.int(n) });
      assert.equal(s.seps.length, s.cols.length + 1);
      assert.ok(s.rows.every((r) => T.width(r) === s.cols.length), JSON.stringify(s.rows.map((r) => r.cells.map((c) => c.span))));
    }
  }
});

// Intuition: a partial rule names columns by number: an insert before it moves it, one inside it grows it, a delete
// shrinks it, and a rule over the deleted column alone goes. Pins `\cline{2-3}` left on the old numbers.
test('partial rules follow inserted and deleted columns', () => {
  const s = grid();
  s.rows[1].before = ['\\cline{2-3}'];
  s.after = ['\\cmidrule(lr){3-3}', '\\bottomrule'];
  assert.ok(op(T, s, { what: 'col', act: 'insert', at: 0 }));
  assert.deepEqual([s.rows[1].before, s.after], [['\\cline{3-4}'], ['\\cmidrule(lr){4-4}', '\\bottomrule']]);
  assert.ok(op(T, s, { what: 'col', act: 'insert', at: 3 }));
  assert.deepEqual([s.rows[1].before, s.after], [['\\cline{3-5}'], ['\\cmidrule(lr){5-5}', '\\bottomrule']]);
  assert.ok(op(T, s, { what: 'col', act: 'delete', at: 4 }));
  assert.deepEqual([s.rows[1].before, s.after], [['\\cline{3-4}'], ['\\bottomrule']]);
  assert.ok(op(T, s, { what: 'col', act: 'delete', at: 0 }));
  assert.deepEqual(s.rows[1].before, ['\\cline{2-3}']);
});
