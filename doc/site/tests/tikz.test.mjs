// TikZ nodes from the PDF (pdfedit.ts `satexTikzMove`): snapping a drag, moving a node and back, its text, shape,
// draw/fill and an edge's tip and label rewritten. Seeded properties beside the fixtures of views.test.mjs.
import assert from 'node:assert/strict';
import { test, rng } from './harness.mjs';

const T = self.satexTikzMove;
const box = (l, t, w = 20, h = 10) => ({ l, t, r: l + w, d: t + h });

// Intuition: a snap pulls a drag at most the tolerance on each axis, and with nothing in reach leaves it as dragged.
// Pins a snap that pulled from beyond the tolerance.
test('snap: never more than the tolerance, nothing in reach no pull', () => {
  const R = rng(71);
  for (let k = 0; k < 500; k++) {
    const others = Array.from({ length: R.int(5) }, () => box(R.int(200), R.int(200), 5 + R.int(30), 5 + R.int(20)));
    const me = box(R.int(200), R.int(200)), d = [R.int(80) - 40, R.int(80) - 40], tol = 1 + R.int(5);
    const s = T.snap(me, d, others, tol);
    assert.ok(Math.abs(s.d[0] - d[0]) <= tol + 1e-9 && Math.abs(s.d[1] - d[1]) <= tol + 1e-9, `pulled ${d} to ${s.d}`);
    if (!others.length) assert.deepEqual(s.d, d);
    for (const g of s.guides) assert.ok(g.ax === 0 || g.ax === 1);
  }
});

// Intuition: a node at a point moved by a distance on the grid and back is where it was, its units kept.
// Pins drags that wrote `1.7999999` or turned `2cm` into `2`.
test('move: there and back gives the text again', () => {
  const R = rng(72), pic = { scale: 1, x: 1, y: 1 }, nums = ['0', '1', '1.5', '-2', '2cm', '10mm', '0.25', '3.05'];
  for (let k = 0; k < 300; k++) {
    const s = `\\node[draw] (a) at (${R.pick(nums)},${R.pick(nums)}) {x};`, v = [(R.int(41) - 20) * 0.05, (R.int(41) - 20) * 0.05];
    const there = T.move(s, 0, pic, { by: v });
    assert.ok(there, s);
    const back = T.move(there.text, 0, pic, { by: [-v[0], -v[1]] });
    // the digits a move needed stay (`1.5` → `2.35` → `1.50`): equal as numbers, units as written
    const num = (t) => t.replace(/-?\d*\.?\d+/g, (x) => String(+x));
    assert.equal(num(back.text), num(s), `${s} by ${v}: ${there.text}`);
  }
});

// Intuition: a node placed relative to another gets shifts that add up; there and back the shifts go again.
test('move: a placed node\'s shifts add up and go at zero', () => {
  const R = rng(73), pic = { scale: 1, x: 1, y: 1 };
  for (let k = 0; k < 100; k++) {
    const s = '\\node[box, right=of a] (b) {x};', v = [(R.int(21) - 10) * 0.1, (R.int(21) - 10) * 0.1];
    const there = T.move(s, 0, pic, { by: v }).text, back = T.move(there, 0, pic, { by: [-v[0], -v[1]] }).text;
    assert.equal(back, s, there);
    assert.ok(there.includes('right=of a'), 'its placement kept');
  }
});

// Intuition: the context menu's rewrites read back as what they set: a shape picked is the node's shape, draw and fill
// flipped twice are as they were, a tip set is the path's tip, a text set is the node's text.
test('rewrites read back: shape, draw/fill, tip, text', () => {
  const pic = { scale: 1, x: 1, y: 1, node: [] }, wrap = (s) => `\\begin{tikzpicture}\n${s}\n\\end{tikzpicture}`;
  for (const start of ['\\node (a) {A};', '\\node[circle, red] (a) {A};', '\\node[rounded corners=2pt, draw] (a) {A};', '\\node[shape=diamond] (a) {A};']) {
    for (const to of ['rectangle', 'rounded', 'circle', 'ellipse', 'diamond']) {
      const r = T.shape(start, 0, pic, to);
      assert.equal(T.shapeOf(T.nodes(wrap(r.text))[0]), to, `${start} -> ${to}: ${r.text}`);
    }
    for (const key of ['draw', 'fill']) {
      const once = T.flip(start, 0, pic, key), n0 = T.nodes(wrap(start))[0], n1 = T.nodes(wrap(once))[0];
      assert.notEqual(T.looks(n1)[key], T.looks(n0)[key], `${key} flipped: ${once}`);
      assert.equal(T.looks(T.nodes(wrap(T.flip(once, 0, pic, key)))[0])[key], T.looks(n0)[key], 'twice: as it was');
    }
    for (const text of ['B', '$x_1$', '\\TeX{} file', '']) assert.equal(T.nodes(wrap(T.retext(start, 0, text)))[0].text, text);
  }
  for (const st of ['\\draw (a) -- (b);', '\\draw[->, thick] (a) -- (b);', '\\draw[thick] (a) to[bend left] (b);']) {
    for (const tip of ['', '->', '<-', '<->', '-latex']) {
      const once = T.tip(st, tip);
      assert.equal(T.tipOf(T.edges(wrap(once))[0]), tip, once);
      assert.equal(T.tip(once, tip), once, 'idempotent');
    }
    const lab = T.label(st, 0, 'l');
    assert.equal(lab.text.slice(lab.at, lab.at + 1), 'l');
    assert.ok(T.nodes(wrap(lab.text)).some((n) => n.kind === 'label' && n.text === 'l'), lab.text);
  }
});
