// TikZ nodes from the PDF (pdfedit.ts `satexTikzMove`): fixtures. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// TikZ nodes moved by drag (pdfedit.ts): `at (x,y)` keeps its units and digits on the 0.5 mm grid, a placed node gets
// shifts that add up and go at zero, Alt places it at a fixed point, a path label moves by `pos=`.
// Pins drags that wrote `at (1.7999999,2cm)` or dropped the `right=of` a node was placed by.
test("TikZ nodes moved by drag (pdfedit.ts)", async () => {
  const T = self.satexTikzMove, pic = { scale: 1, x: 1, y: 1 }, mv = (s, m, p = pic) => T.move(s, 0, p, m)?.text;
  assert.equal(mv('\\node (a) at (1.50,2cm) {x};', { by: [0.33, -0.21] }), '\\node (a) at (1.85,1.8cm) {x};', 'the 0.5 mm grid, digits as written');
  assert.equal(mv('\\node (a) at (10mm,2in) {x};', { by: [0.33, -0.21], free: true }), '\\node (a) at (13.3mm,1.917in) {x};', 'Shift: off the grid');
  assert.equal(mv('\\node (a) at (1,1) {x};', { by: [0.5, 0] }, { scale: 2, x: 1, y: 1 }), '\\node (a) at (1.25,1) {x};', 'a scaled picture moves by less');
  assert.equal(mv('\\node[box, right=of a] (b) {x};', { by: [0.3, -0.2] }), '\\node[box, right=of a, xshift=3mm, yshift=-2mm] (b) {x};');
  assert.equal(mv('\\node[right=of a, xshift=2mm] (b) {x};', { by: [-0.2, 0] }), '\\node[right=of a] (b) {x};', 'a shift moved to zero goes');
  assert.equal(mv('\\node[right=of a, xshift=1cm] (b) {x};', { by: [0.25, 0] }), '\\node[right=of a, xshift=1.25cm] (b) {x};', 'a shift keeps its unit');
  assert.equal(mv('\\node (b) {x};', { by: [0.1, 0.1] }), '\\node[xshift=1mm, yshift=1mm] (b) {x};', 'options added where there were none');
  assert.equal(mv('\\node[draw, right=of a, yshift=2mm] (b) {x};', { to: [2.04, -1.46] }), '\\node[draw] (b) at (2.05,-1.45) {x};', 'Alt: placed at a point');
  assert.equal(mv('\\draw (a) -- node[midway, above] {l} (b);', { pos: 0.304 }), '\\draw (a) -- node[above, pos=0.3] {l} (b);', 'a label along its path');
  const ns = T.nodes('\\begin{tikzpicture}[scale=2]\n\\node (a) at (0,0) {A}; % a; b\n\\node[right=of a] (b) {B};\n\\draw (a) -- node {m} (b) node[pos=.2] {n};\n\\foreach \\i in {1,2} \\node at (\\i,0) {\\i};\n\\end{tikzpicture}');
  assert.deepEqual(ns.map((n) => [n.kind, n.line, n.name, n.placed, n.plain, n.seg, n.pos]), [['node', 2, 'a', false, true, null, null], ['node', 3, 'b', true, false, null, null],
    ['label', 4, null, false, true, ['a', 'b'], 0.5], ['label', 4, null, false, true, ['a', 'b'], 0.2]], 'a loop is skipped, a comment is no statement');
  assert.equal(ns[0].pic.scale, 2);
  assert.equal(T.posOn([0, 0], [10, 0], [3, 5]), 0.3);
  assert.deepEqual(T.clipped([0, 0], [2, 1], [10, 0], [3, 1]), [[2, 0], [7, 0]], 'cut where it leaves the boxes');
});
// TikZ nodes snapped, reshaped and labelled from the PDF (pdfedit.ts): a drag snaps a centre or an edge onto another
// node's within the tolerance, else its gap onto an equal gap; the context menu rewrites only the node's or the path's
// own keys. Pins a snap that pulled from beyond the tolerance, a shape change that left `circle` beside `ellipse`, a
// label put after the target point (`(a) -- (b) node {…}` sits at the end, not midway).
test("TikZ nodes snapped, reshaped and labelled from the PDF (pdfedit.ts)", async () => {
  const T = self.satexTikzMove, box = (l, t, w = 20, h = 10) => ({ l, t, r: l + w, d: t + h });
  const s = T.snap(box(0, 0), [48, 31], [box(50, 30), box(100, 80)], 3);
  assert.deepEqual(s.d, [50, 30], 'left and top edges onto the nearest node\'s');
  assert.deepEqual(s.guides.map((g) => [g.ax, g.at]), [[0, 50], [0, 60], [0, 70], [1, 30], [1, 35], [1, 40]], 'a guide for each line it shares');
  assert.deepEqual(T.snap(box(0, 0), [44, 0], [box(50, 30)], 3).d, [44, 0], 'beyond the tolerance: as dragged');
  // in a row a, b 30 apart: the moved box after b snaps to the same gap
  const g = T.snap(box(0, 0), [99, 0], [box(0, 0), box(50, 0)], 3);
  assert.deepEqual(g.d, [100, 0], 'equal spacing');
  assert.deepEqual(g.gaps[0].segs.map((x) => x.slice(0, 2)), [[20, 50], [70, 100]]);
  const pic = { scale: 1, x: 1, y: 1, node: ['draw', 'rounded corners'] }, n = (src) => T.nodes(`\\begin{tikzpicture}[every node/.style={draw, rounded corners}]\n${src}\n\\end{tikzpicture}`)[0];
  assert.equal(T.shapeOf(n('\\node (a) {A};')), 'rounded', 'from the picture\'s every node');
  assert.equal(T.shapeOf(n('\\node[shape=ellipse] (a) {A};')), 'ellipse');
  assert.deepEqual(T.shape('\\node[circle, red] (a) {A};', 0, pic, 'ellipse'), { text: '\\node[ellipse, red] (a) {A};', library: 'shapes.geometric' });
  assert.equal(T.shape('\\node (a) {A};', 0, pic, 'rectangle').text, '\\node[sharp corners] (a) {A};', 'the style\'s round corners undone');
  assert.equal(T.shape('\\node[rounded corners=2pt] (a) {A};', 0, { ...pic, node: [] }, 'rectangle').text, '\\node (a) {A};', 'an empty option list goes');
  assert.equal(T.flip('\\node (a) {A};', 0, pic, 'draw'), '\\node[draw=none] (a) {A};', 'the style draws: none');
  assert.equal(T.flip('\\node[draw=none] (a) {A};', 0, pic, 'draw'), '\\node (a) {A};');
  assert.equal(T.flip('\\node[red] (a) {A};', 0, pic, 'fill'), '\\node[red, fill=black!10] (a) {A};');
  assert.deepEqual(T.looks(n('\\node[fill=none] (a) {A};')), { draw: true, fill: false });
  assert.equal(T.retext('\\node (a) {\\TeX{} file};', 0, 'input'), '\\node (a) {input};');
  const es = T.edges('\\begin{tikzpicture}\n\\node (a) {A};\n\\draw[->] (a) -- (b) to[bend left] node {x} (c);\n\\end{tikzpicture}');
  assert.deepEqual(es.map((e) => [e.line, e.opts, e.segs.map((x) => [x.a, x.b])]), [[3, ['->'], [['a', 'b'], ['b', 'c']]]], 'a node statement is no path');
  assert.equal(T.tipOf(es[0]), '->');
  const st = '\\draw[->, thick] (a) -- (b);', lab = T.label(st, 0, 'reads');
  assert.equal(lab.text, '\\draw[->, thick] (a) -- node[midway, above] {reads} (b);');
  assert.equal(lab.text.slice(lab.at, lab.at + 5), 'reads', 'the caret goes to the label\'s text');
  assert.equal(T.label('\\draw (a) to[bend left] (b);', 0, '').text, '\\draw (a) to[bend left] node[midway, above] {} (b);');
  assert.equal(T.tip(st, '<->'), '\\draw[<->, thick] (a) -- (b);');
  assert.equal(T.tip(st, ''), '\\draw[thick] (a) -- (b);');
  assert.equal(T.tip('\\draw[->] (a) -- (b);', ''), '\\draw (a) -- (b);');
  assert.equal(T.tip('\\draw[flow] (a) -- (b);', ''), '\\draw[flow, -] (a) -- (b);', 'a style may give a tip: none written after it, the later key wins');
  assert.equal(T.tip('\\draw[flow] (a) -- (b);', '<->'), '\\draw[flow, <->] (a) -- (b);', 'a tip after the style that may give one');
  assert.equal(T.tip('\\draw (a) -- (b);', '->'), '\\draw[->] (a) -- (b);');
});
