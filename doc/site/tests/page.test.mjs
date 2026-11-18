// The Page layout widget's model (views.ts `page`): typearea's margins, the numbering in force, header and footer fields
// written as calls and read back. Seeded properties beside the fixtures of views.test.mjs.
import assert from 'node:assert/strict';
import { test, rng } from './harness.mjs';

const P = self.satexViews.page;
const at = (line) => ({ line, character: 0 });

// Intuition: typearea splits the page into DIV stripes: the text takes DIV-3 of the width past the binding, the margins
// the rest (two-sided: outer twice the inner; one-sided: equal), top and bottom one and two stripes of the height.
test('pageBox: typearea\'s stripes for any DIV and BCOR', () => {
  const R = rng(61);
  for (let k = 0; k < 300; k++) {
    const pw = 30e6 + R.int(20e6), ph = 40e6 + R.int(20e6), div = 6 + R.int(10), bcor = R.int(1e6), two = R.chance(0.5);
    const st = { mode: 'koma', pw, ph, div, bcor, ran: null, left: 0, right: 0, top: 0, bottom: 0 };
    const b = P.pageBox(st, two), aw = pw - bcor, tw = aw * (div - 3) / div;
    assert.ok(Math.abs(b.left + tw + b.right - pw) < 1e-6 * pw, 'the margins and text fill the page');
    if (two) assert.ok(Math.abs(b.right - 2 * (b.left - bcor)) < 1e-6 * pw, 'outer twice the inner');
    else assert.ok(Math.abs(b.right - (b.left - bcor)) < 1e-6 * pw, 'one-sided: alike');
    assert.ok(Math.abs(b.top + b.bottom - 3 * ph / div) < 1e-6 * ph && Math.abs(b.bottom - 2 * b.top) < 1e-6 * ph);
    assert.equal(P.pageBox({ ...st, ran: { div, bcor } }, two).left, 0, 'as the run computed it until DIV or BCOR change');
  }
});

// Intuition: the text area starts 1in + \topmargin + \headheight + \headsep below the page's top and is \textheight
// high, in big points (72 per inch, TeX's points 72.27). Pins the Type mode guessing the area's foot from the page number.
test('textArea: the run\'s lengths as the page\'s text area in bp', () => {
  const PT = 65536, INCH = 72.27, bp = (pt) => pt * 72 / INCH, same = (pt) => [pt * PT, pt * PT];
  const L = { topmargin: 16, headheight: 12, headsep: 25, textheight: 550, footskip: 30, textwidth: 345, oddsidemargin: 62, evensidemargin: -10 };
  const run = Object.fromEntries(Object.entries(L).map(([k, v]) => [k, same(v)]));
  const a = P.textArea({ run, twoside: true }), top = INCH + L.topmargin + L.headheight + L.headsep;
  const near = (x, want, what) => assert.ok(Math.abs(x - want) < 1e-3, `${what}: ${x}, want ${want}`);
  near(a.top, bp(top), 'top'); near(a.bottom, bp(top + L.textheight), 'bottom'); near(a.foot, bp(top + L.textheight + L.footskip), 'footer baseline');
  near(a.left, bp(INCH + L.oddsidemargin), 'odd left'); near(a.right, bp(INCH + L.oddsidemargin + L.textwidth), 'odd right'); near(a.even, bp(INCH + L.evensidemargin), 'even left');
  near(P.textArea({ run, twoside: false }).even, a.left, 'one-sided: even pages like odd');
  near(P.textArea({ run: { ...run, voffset: same(5), hoffset: same(-3) }, twoside: true }).top, bp(top + 5), '\\voffset moves it down');
  assert.equal(P.textArea({ run: { ...run, textheight: [500 * PT, 600 * PT] } }), null, 'an open \\textheight: no area');
  assert.equal(P.textArea({ run: { textwidth: run.textwidth } }), null, 'unknown lengths: no area');
  assert.equal(P.textArea({}), null, 'no run');
});

// Intuition: the numbering in force at a line is the last restart at or before it; later lines never go back to an
// earlier restart. Pins a page number click that opened the numbering of a later \pagenumbering.
test('numberInForce: the last point at or before the line', () => {
  const R = rng(62);
  for (let k = 0; k < 200; k++) {
    const marks = Array.from({ length: R.int(3) }, (_, i) => ({ name: ['frontmatter', 'mainmatter', 'appendix'][i], row: 5 + R.int(50), at: at(1) }));
    const calls = Array.from({ length: R.int(3) }, () => ({ row: 5 + R.int(50), node: { kind: 'call', name: 'pagenumbering', args: [{ open: '{', text: 'roman' }] } }));
    const s = { numbering: { start: at(1), insert: at(1), marks, calls, per_chapter: false } };
    const rows = [['start', 0], ...marks.map((m, i) => [`m${i}`, m.row]), ...calls.map((c, i) => [`c${i}`, c.row])];
    let last = 0;
    for (let line = 1; line < 60; line++) {
      const id = P.numberInForce(s, line), row = Object.fromEntries(rows)[id];
      assert.ok(row <= line, `${id} at ${row} for line ${line}`);
      assert.ok(!rows.some(([, r]) => r > row && r <= line), 'none later at or before the line');
      assert.ok(row >= last, 'monotone');
      last = row;
    }
  }
});

// Intuition: header and footer fields written as calls and read back are the same fields, one-sided and two-sided,
// for fancyhdr (selectors grouped by text) and scrlayer-scrpage (a call per field). Pins a two-sided field written
// as `[LE,RO]` and read back on one side only.
test('header fields: written as calls, read back alike', () => {
  const R = rng(63), texts = ['', '', '\\thepage', 'Draft', '\\leftmark', 'A, B'];
  const call = (name, opt, args) => ({ kind: 'call', name, range: { start: at(2), end: at(3) }, text: '', args: [...(opt == null ? [] : [{ open: '[', text: opt }]), ...args.map((t) => ({ open: '{', text: t }))] });
  for (let k = 0; k < 200; k++) {
    const pkg = R.pick(['fancyhdr', 'scrlayer-scrpage']), two = pkg === 'fancyhdr' && R.chance(0.5);
    const m = P.headerOf({ package: pkg, calls: [] }, two);
    const sides = two ? ['E', 'O'] : ['O'], spots = pkg === 'fancyhdr' ? ['L', 'C', 'R'] : ['i', 'c', 'o'];
    for (const part of ['head', 'foot']) for (const side of sides) for (const spot of spots) { const t = R.pick(texts); if (t) m[part][side + spot] = t; }
    const calls = P.headerCalls(m, two, 1).map(([name, opt, args]) => call(name, opt, args)), back = P.headerOf({ package: pkg, calls }, two);
    for (const part of ['head', 'foot']) {
      const want = Object.fromEntries(Object.entries(m[part]).filter(([, v]) => v)), got = Object.fromEntries(Object.entries(back[part]).filter(([, v]) => v));
      assert.deepEqual(got, want, `${pkg} two=${two}: ${JSON.stringify(calls.map((c) => [c.name, c.args.map((a) => a.text)]))}`);
    }
  }
});

// Intuition: numbering edits write one node per point changed and nothing when nothing changed; a call set back to
// none is dropped, a new style after a mark is a call there.
test('numberNodes: one node per change', () => {
  const R = rng(64), STY = ['', 'arabic', 'roman', 'Roman', 'alph'];
  for (let k = 0; k < 200; k++) {
    const call = { kind: 'call', name: 'pagenumbering', star: false, args: [{ open: '{', inner: null, text: 'alph' }], range: null, text: '' };
    const s = { numbering: { start: at(2), insert: at(1), per_chapter: false, reset: null, marks: [{ name: 'mainmatter', row: 5, at: at(6) }], calls: [{ row: 3, node: call }] } };
    const ids = ['start', 'm0', 'c0'], st0 = { numbers: { start: '', m0: '', c0: 'alph' }, perChapter: false };
    const st = { numbers: Object.fromEntries(ids.map((id) => [id, R.chance(0.5) ? st0.numbers[id] : R.pick(STY)])), perChapter: R.chance(0.3) };
    const nodes = P.numberNodes(s, st0, st), changed = ids.filter((id) => st.numbers[id] !== st0.numbers[id]).length + +st.perChapter;
    assert.equal(nodes.length, changed);
    if (st.numbers.c0 === '') assert.ok(nodes.some((n) => n.drop), 'the call dropped');
    for (const n of nodes.filter((x) => !x.drop && x.name === 'pagenumbering')) assert.ok(STY.includes(n.args.find((a) => a.open === '{').text));
  }
});
