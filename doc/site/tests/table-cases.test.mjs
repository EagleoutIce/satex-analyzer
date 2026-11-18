// The table dialog (dialogs.ts `table`): fixtures. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// the table dialog's model (dialogs.js): merges, CSV/JSON round trips, paste
load('./dialogs.js');
const T = self.satexDialogs.table;
const cell = (text) => ({ text, span: 1, spec: '' });
const grid = () => ({
  env: 'tabular', cols: [{ align: 'l' }, { align: 'c' }, { align: 'r' }].map((c) => ({ arg: '', pre: '', post: '', ...c })), seps: ['', '', '', ''],
  rows: [['Method', 'Score', 'Time'], ['Ours', '0.92', '1.5'], ['Base', '0.81', '2']].map((r, k) => ({ before: k === 1 ? ['\\hline'] : [], inline: false, cells: r.map(cell), end: '\\\\', comment: '', raw: '' })),
  after: [], style: { indent: '  ', spaced: true, aligned: false, inline_rules: false },
});
const texts = (s) => s.rows.map((r) => r.cells.map((c) => c.span > 1 ? `${c.span}:${c.text}` : c.text));
test("table-cases 1", async () => {
  // a row span is a \multicolumn, a column span a \multirow, a block both
  let s = grid();
  assert.ok(T.merge(s, [0, 0, 0, 1]));
  assert.deepEqual(texts(s)[0], ['2:Method Score', 'Time']);
  T.split(s, [0, 0, 0, 0]);
  assert.deepEqual(texts(s)[0], ['Method Score', '', 'Time']);
  s = grid();
  T.merge(s, [1, 0, 2, 0]);
  assert.deepEqual(texts(s).map((r) => r[0]), ['Method', '\\multirow{2}{*}{Ours Base}', '']);
  // a selection inside a merged cell grows to all of it
  assert.deepEqual(T.normalize(s, [2, 0, 2, 0]), [1, 0, 2, 0]);
  T.split(s, [2, 0, 2, 0]);
  assert.equal(s.rows[1].cells[0].text, 'Ours Base');
  s = grid();
  T.merge(s, [1, 1, 2, 2]);
  assert.deepEqual(texts(s).slice(1), [['Ours', '2:\\multirow{2}{*}{0.92 1.5 0.81 2}'], ['Base', '2:']]);
  // deleting a row a \multirow reaches into shrinks it
  T.deleteRows(s, 2, 2);
  assert.equal(s.rows[1].cells[1].text, '0.92 1.5 0.81 2');
});
test("table-cases 2", async () => {
  // CSV and JSON round trips: the same cells come back, specials escaped once
  const s = grid();
  s.rows[1].cells[0].text = 'A\\&B 50\\%';
  s.rows[2].cells[0].text = '\\textbf{x\\_1}';
  const csv = T.toCSV(s);
  assert.equal(csv.split('\n')[1], 'A&B 50%,0.92,1.5');
  const back = grid();
  T.importText(back, csv);
  assert.equal(back.rows[1].cells[0].text, 'A\\&B 50\\%');
  assert.equal(back.rows[2].cells[0].text, 'x\\_1');
  const json = T.toJSON(s);
  assert.equal(json.header, 1);
  const again = grid();
  again.rows.pop();
  T.importText(again, JSON.stringify(json));
  assert.deepEqual(texts(again), texts(s));
  assert.deepEqual(again.rows.map((r) => r.before), s.rows.map((r) => r.before));
  assert.deepEqual(T.parseCSV('a\t"b ""q"""\n1\t2\n'), [['a', 'b "q"'], ['1', '2']]);
  assert.equal(T.plain(T.tex('50% of $x_1 & {y}~^\\')), '50% of $x_1 & {y}~^\\');
});
test("table-cases 3", async () => {
  // paste grows the table; columns insert and delete with their rules
  const s = grid();
  T.paste(s, 2, 2, [['a', 'b'], ['c', 'd']]);
  assert.deepEqual([s.rows.length, s.cols.length, s.seps.length], [4, 4, 5]);
  assert.deepEqual(texts(s)[3], ['', '', 'c', 'd']);
  s.seps = ['|', '', '', '', '|'];
  T.deleteCols(s, 3, 3);
  assert.deepEqual(s.seps, ['|', '', '', '|']);
  T.insertCol(s, 0);
  assert.deepEqual([s.cols.length, s.seps[0], s.seps.at(-1), s.rows[0].cells[0].text], [4, '|', '|', '']);
  T.setStyle(s, 'compact');
  assert.equal(T.styleOf(s), 'compact');
});
test("table-cases 4", async () => {
  // array's injects: the alignment is the column's letter, not one inside `>{\bf}`; the preview sets what the inject switches
  assert.deepEqual(['>{\\bf}r', '>{\\bfseries}l<{\\%}', 'p{3cm}', '@{}>{\\centering\\arraybackslash}m{2cm}'].map((x) => T.alignOf(x)), ['r', 'l', 'p', 'm']);
  assert.equal(T.alignOf('>{\\bf}b{2cm}', 'lcrS'), 'l');
  assert.equal(T.injectStyle('\\bf'), 'font-weight:bold');
  assert.equal(T.injectStyle('\\itshape\\color{blue}'), 'font-style:italic;color:blue');
  assert.equal(T.injectStyle('\\arraybackslash'), '');
});
