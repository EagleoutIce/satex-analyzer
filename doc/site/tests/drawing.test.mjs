// The drawing canvas model (views.ts `drawing`) and circuits. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
const { parse, emit, mermaidIn, mermaidOut, importMermaid } = self.satexViews.drawing;
test("a picture: nodes and arrows edited, relative nodes, Mermaid in and out", async () => {

// a picture: nodes and an arrow edited, a loop and a plain path kept as they are
const body = `
  \\node[draw] (a) at (0,0) {Start};
  \\node[draw, circle] (b) at (3,1) {End};
  \\draw[->] (a) -- (b);
  \\foreach \\i in {1,2} { \\node (c\\i) at (\\i,-2) {\\i}; }
  \\draw (0,-3) -- (2,-3); % a path
`;
const state = parse(body);
assert.equal(emit(state), body, 'untouched, the text comes back as it was');
assert.deepEqual(state.items.map((i) => i.type), ['node', 'node', 'edge', 'code', 'code']);
const [a, b, edge, loop] = state.items;
assert.deepEqual([a.name, a.x, a.y, a.label, b.opts], ['a', 0, 0, 'Start', ['draw', 'circle']]);
assert.deepEqual([edge.from, edge.to, edge.arrow], ['a', 'b', '->']);
assert.ok(loop.code.startsWith('\\foreach') && loop.code.endsWith('}'));
// move a, relabel b, add an arrow back, delete nothing else
Object.assign(a, { x: 1.5, y: -0.5, dirty: true });
Object.assign(b, { label: 'Done', dirty: true });
state.items.push({ type: 'edge', dirty: true, lead: '', from: 'b', to: 'a', arrow: '<->', opts: [], label: 'again', at: '' });
const out = emit(state);
assert.equal(out, `
  \\node[draw] (a) at (1.5,-0.5) {Start};
  \\node[draw, circle] (b) at (3,1) {Done};
  \\draw[->] (a) -- (b);
  \\foreach \\i in {1,2} { \\node (c\\i) at (\\i,-2) {\\i}; }
  \\draw (0,-3) -- (2,-3); % a path
  \\draw[<->] (b) -- node[above] {again} (a);
`);
// the edited state reads back as itself
const again = parse(out);
assert.equal(emit(again), out);
assert.deepEqual(again.items.map((i) => i.type), ['node', 'node', 'edge', 'code', 'code', 'edge']);
// a deleted node goes with its arrows
const del = parse(body);
del.items[0].deleted = true;
del.items[2].deleted = true;
assert.ok(!emit(del).includes('(a)') && emit(del).includes('\\foreach'));

// nodes placed relative to others: the text's order of name and options, no `at`, a label in braces
const rel = `
  \\node (tex) {\\TeX{} file};
  \\node (satex) [right of=tex] {SaTeX};
  \\node (out) [above right=5mm and 1cm of satex, fill=red] {findings};
  \\node[right=of a, left=of b] (two) {x};
  \\node (dup) at (1,2) [below of=tex] {y};
`;
const r = parse(rel);
assert.equal(emit(r), rel);
assert.deepEqual(r.items.map((i) => i.type), ['node', 'node', 'node', 'code', 'code'], 'two placements, or one with `at`: code');
const [tex, sat, outn] = r.items;
assert.deepEqual([tex.label, tex.at, tex.place, tex.x, tex.y], ['\\TeX{} file', false, null, 0, 0]);
assert.deepEqual([sat.place.dir, sat.place.of, sat.place.centers], [[1, 0], 'tex', true]);
assert.deepEqual([outn.place.dir, outn.place.of, outn.place.dist, outn.opts], [[1, 1], 'satex', '5mm and 1cm', ['above right=5mm and 1cm of satex', 'fill=red']]);
// a relabeled relative node stays relative; a moved one is placed where it was dropped
Object.assign(sat, { label: 'Tool', dirty: true });
Object.assign(outn, { x: 3, y: 1, at: true, place: null, opts: ['fill=red'], dirty: true });
assert.equal(emit(r), `
  \\node (tex) {\\TeX{} file};
  \\node[right of=tex] (satex) {Tool};
  \\node[fill=red] (out) at (3,1) {findings};
  \\node[right=of a, left=of b] (two) {x};
  \\node (dup) at (1,2) [below of=tex] {y};
`);
assert.deepEqual(parse(emit(r)).items.map((i) => i.type), ['node', 'node', 'node', 'code', 'code']);

// Mermaid: a 4-node flowchart, top-down, by rank
const flow = `graph TD
  A[Start] --> B{Check}
  B -->|yes| C((Done))
  B -- no --> D(Retry)
  D --- A`;
const into = { items: [], tail: '\n' };
importMermaid(into, flow, 2);
assert.equal(emit(into), `
  \\node[draw] (A) at (0,0) {Start};
  \\node[draw] (B) at (0,-2) {Check};
  \\node[draw, circle] (C) at (-1,-4) {Done};
  \\node[draw, rounded corners] (D) at (1,-4) {Retry};
  \\draw[->] (A) -- (B);
  \\draw[->] (B) -- node[above] {yes} (C);
  \\draw[->] (B) -- node[above] {no} (D);
  \\draw (D) -- (A);
`);
// export gives back the same graph
const graph = (g) => [g.nodes.map((n) => `${n.name}:${n.label}:${n.shape}`).sort(), g.edges.map((e) => `${e.from}${e.arrow}${e.to}:${e.label}`).sort()];
const read = parse(emit(into));
const text = mermaidOut(read.items.filter((i) => i.type === 'node'), read.items.filter((i) => i.type === 'edge'));
const back = mermaidIn(text);
assert.deepEqual(graph(back), graph(mermaidIn(flow)));
assert.equal(back.dir, 'TD');
});

// the canvas's model beyond nodes and arrows (views.js `drawing`)
test("the canvas's model beyond nodes and arrows (views.js `drawing`)", async () => {
  const D = self.satexViews.drawing;
  const ty = (st) => st.items.map((i) => i.type);
  // 1. arrow tips as the text names them, per end; any tip the run declares
  assert.deepEqual(['->', '-Stealth', 'Latex-{Straight Barb[open]}', '-', '<->', 'thick', 'draw=red', 'right hook-latex'].map(D.tipsOf),
    [['', '>'], ['', 'Stealth'], ['Latex', '{Straight Barb[open]}'], ['', ''], ['<', '>'], null, null, ['right hook', 'latex']]);
  const tipped = '\n  \\draw[Latex-{Stealth[open]}, thick] (a.north) -- (b.30);\n';
  const te = D.parse(tipped);
  const [e] = te.items;
  assert.deepEqual([e.type, e.arrow, e.opts, e.from, e.fromAt, e.to, e.toAt], ['edge', 'Latex-{Stealth[open]}', ['thick'], 'a', 'north', 'b', '30']);
  assert.equal(D.emit(te), tipped, 'untouched: as it was');
  // 3. anchors written back, and read back the same
  Object.assign(e, { arrow: '-Triangle', fromAt: 'south east', toAt: '', dirty: true });
  assert.equal(D.emit(te), '\n  \\draw[-Triangle, thick] (a.south east) -- (b);\n');
  assert.deepEqual((({ arrow, fromAt, toAt }) => [arrow, fromAt, toAt])(D.parse(D.emit(te)).items[0]), ['-Triangle', 'south east', '']);
  const box = { x: 0, y: 0 };
  assert.deepEqual(D.anchorAt(box, [2, 1], false, 'north east'), [1, 0.5]);
  assert.deepEqual(D.anchorAt(box, [2, 2], true, '0').map((v) => +v.toFixed(6)), [1, 0]);
  assert.equal(D.nearestAnchor(box, [2, 1], false, [0.05, 0.55]), 'north');
  assert.equal(D.nearestAnchor(box, [2, 1], false, [1, 1]), '45', 'away from the compass points: the angle, in steps of 15°');
  assert.equal(D.nearestAnchor(box, [2, 1], false, [-0.9, -0.48]), 'south west');
  // a tip TeX drew becomes a marker: its last points, the line's height its reference
  const m = D.tipMarker("<svg version='1.1' width='52' height='10' viewBox='76 57 26 5'><g id='page1'><g transform='translate(77,59.5)scale(1,-1)'><g fill='#888' stroke='#888'><path d='M0 0H21'/></g></g></g></svg>");
  assert.deepEqual([m.box, m.ref], [[94, 57, 8, 5], [102, 59.5]]);
  assert.ok(m.inner.includes("fill='currentColor'") && !m.inner.includes('id='), m.inner);
  // the sample's shaft is the edge's line, not the marker's: else a solid stub sits on a dashed or missing line
  const shaft = 'M0 0H24.8';
  const shafted = D.tipMarker(`<svg viewBox='76 55 26 7'><g transform='translate(77,59)'><path d='${shaft}' fill='none'/><g transform='translate(25,0)'><path d='M-2.5 3C-1 1 0 0 0 0' fill='none'/></g></g></svg>`);
  assert.ok(!shafted.inner.includes(shaft) && shafted.inner.includes('M-2.5 3'), shafted.inner);
  // a shape TeX drew: its ink and widths dropped for the node's own, a path only stroked stays unfilled
  const sample = D.shapeSample("<svg width='60' height='30' viewBox='10 20 30 15'><g id='page3'><g fill='#888' stroke='#888' fill-opacity='0.15'><g stroke-width='0.4'><path d='M0 0L5 5Z'/><path d='M1 1H4' fill='none'/></g></g></g></svg>");
  assert.deepEqual(sample.box, [10, 20, 30, 15]);
  assert.ok(!/#888|stroke-width|fill-opacity|id=/.test(sample.inner) && sample.inner.includes("fill='none'"), sample.inner);
  // the shapes offered are the ones the run defined, by name, then the libraries' with their load; none named: rectangle and circle
  const offered = D.shapeList({ shapes: [{ name: 'rectangle' }, { name: 'star', keys: [{ name: 'star points', default: '5' }] }, { name: 'cloud', load: '\\usetikzlibrary{shapes.symbols}' }] });
  assert.deepEqual([...offered.keys()], ['rectangle', 'star', 'cloud']);
  assert.deepEqual([offered.get('star').keys[0].name, offered.get('cloud').load, offered.get('rectangle').keys], ['star points', '\\usetikzlibrary{shapes.symbols}', []]);
  assert.deepEqual([...D.shapeList({ shapes: ['rectangle', 'diamond'] }).keys()], ['rectangle', 'diamond'], 'names alone, as older runs send them');
  assert.deepEqual([...D.shapeList({}).keys()], ['rectangle', 'circle']);
  // Tab walks nodes and arrows in order and leaves the canvas past either end
  const [n1, e1, n2] = [{}, {}, {}];
  assert.deepEqual([D.cycle([n1, e1, n2], null, false), D.cycle([n1, e1, n2], n1, false), D.cycle([n1, e1, n2], e1, true), D.cycle([n1, e1, n2], n2, false), D.cycle([n1, e1, n2], null, true)], [n1, e1, n1, null, n2]);
  // an arrow's hit band: its width set per drawing in screen pixels, never fixed in cm by the style; its label takes clicks too
  const hitRule = D.STYLE.match(/\.tv-hit \{[^}]*\}/)[0];
  assert.ok(!/stroke-width/.test(hitRule) && /stroke: transparent/.test(hitRule), hitRule);
  assert.match(D.STYLE, /\.tv-edge text \{[^}]*pointer-events: auto/);

  // 8. layers: blocks of `pgfonlayer` and of a scope whose key opens a layer
  const layered = `
  \\node (a) at (0,0) {A};
  \\begin{pgfonlayer}{background}
    \\node[fill=red] (b) at (1,0) {B};
    \\draw (a) -- (b);
  \\end{pgfonlayer}
  \\begin{scope}[on background layer, blue]
    \\node (c) at (2,0) {C};
  \\end{scope}
  \\begin{scope}[xshift=1cm]
    \\node (d) at (3,0) {D};
  \\end{scope}
`;
  const L = D.parse(layered, { 'on background layer': 'background' });
  assert.equal(D.emit(L), layered);
  const by = (n) => L.items.find((i) => i.name === n);
  assert.deepEqual(['a', 'b', 'c', 'd'].map((n) => by(n).layer), ['main', 'background', 'background', 'main']);
  assert.equal(L.items.find((i) => i.type === 'edge').layer, 'background');
  assert.deepEqual(L.items.filter((i) => i.type === 'layer').map((i) => i.layer ?? (i.close ? '/' : 'scope')), ['background', '/', 'background', '/', 'scope', '/'], 'a scope without a layer key: a block on no layer');
  // moved: written where it stood, out of its block and into one of its own
  D.toLayer(by('b'), 'main');
  D.toLayer(by('a'), 'background');
  const moved = D.emit(L);
  assert.equal(moved, `
  \\begin{pgfonlayer}{background}
    \\node (a) at (0,0) {A};
  \\end{pgfonlayer}
  \\begin{pgfonlayer}{background}
  \\end{pgfonlayer}
    \\node[fill=red] (b) at (1,0) {B};
  \\begin{pgfonlayer}{background}
    \\draw (a) -- (b);
  \\end{pgfonlayer}
  \\begin{scope}[on background layer, blue]
    \\node (c) at (2,0) {C};
  \\end{scope}
  \\begin{scope}[xshift=1cm]
    \\node (d) at (3,0) {D};
  \\end{scope}
`);
  const L2 = D.parse(moved, { 'on background layer': 'background' });
  assert.deepEqual(['a', 'b', 'c'].map((n) => L2.items.find((i) => i.name === n).layer), ['background', 'main', 'background'], 'read back: the same layers');
  assert.equal(L2.items.find((i) => i.type === 'edge').layer, 'background');

  // 4. nodes in a grid as one matrix of nodes, centers kept, arrows by name
  const grid = '\n  \\node[draw] (a) at (0,0) {A};\n  \\node[draw, circle] (b) at (2,0) {B};\n  \\node (c) at (2,-1.5) {C};\n  \\draw[->] (a) -- (b);\n';
  const G = D.parse(grid);
  const where = (ns) => ns.map((n) => `${n.name}:${n.x},${n.y}`).sort();
  const before = where(G.items.filter((i) => i.type === 'node'));
  const { matrix } = D.toMatrix(G, G.items.filter((i) => i.type === 'node'), 'm');
  const text = D.emit(G);
  assert.equal(text, `
  \\matrix (m) [matrix of nodes, row sep={1.5cm,between origins}, column sep={2cm,between origins}, matrix anchor=a.center] at (0,0) {
    |[draw] (a)| A & |[draw, circle] (b)| B \\\\
     & |(c)| C \\\\
  };
  \\draw[->] (a) -- (b);
`);
  const G2 = D.parse(text);
  assert.deepEqual(ty(G2), ['matrix', 'edge']);
  const [mx] = G2.items;
  D.cellPlaces(mx).forEach(([x, y], i) => Object.assign(mx.cells[i], { x, y }));
  assert.deepEqual(where(mx.cells), before, 'read back: every node where it was');
  assert.deepEqual(mx.cells.map((c) => [c.name, c.opts, c.label]), [['a', ['draw'], 'A'], ['b', ['draw', 'circle'], 'B'], ['c', [], 'C']]);
  assert.equal(D.emit(G2), text);
  assert.ok(matrix && D.toMatrix(D.parse('\n  \\node (a) at (0,0) {A};\n  \\node (b) at (1,0.3) {B};\n  \\node (c) at (0.5,0) {C};\n  \\node (e) at (0.25,0.3) {E};\n'), [], 'x').why);
  const skew = D.parse('\n  \\node (a) at (0,0) {A};\n  \\node (b) at (1,0) {B};\n  \\node (c) at (2.5,0) {C};\n');
  assert.equal(D.toMatrix(skew, skew.items, 'm').why, 'the nodes are not in a grid');
  // a matrix whose cells have no names: `m-r-c`, as TikZ names them
  assert.deepEqual(D.parse('\\matrix (q) [matrix of nodes] { x & y \\\\ };').items[0].cells.map((c) => c.name), ['q-1-1', 'q-1-2']);

  // 5. Mermaid out and back: the same graph; what it cannot say, named
  const flow = D.parse('\n  \\node[draw] (s) at (0,0) {Start};\n  \\node[draw, circle] (e) at (0,-2) {End};\n  \\node[draw, rounded corners] (w) at (2,-2) {Wait};\n  \\draw[->] (s) -- node[above] {go} (e);\n  \\draw[<-] (w) -- (e);\n  \\draw (s) -- (w);\n');
  const ns = flow.items.filter((i) => i.type === 'node'), es = flow.items.filter((i) => i.type === 'edge');
  const out = D.mermaidOut(ns, es);
  assert.equal(out, 'graph TD\n  s[Start]\n  e((End))\n  w(Wait)\n  s -->|go| e\n  e --> w\n  s --- w');
  const into = { items: [], tail: '\n' };
  D.importMermaid(into, out, 2);
  const g = (items) => [items.filter((i) => i.type === 'node').map((n) => `${n.name}:${n.label}:${n.opts.join('+')}`).sort(), items.filter((i) => i.type === 'edge').map((e) => `${e.from}${e.arrow}${e.to}:${e.label}`).sort()];
  assert.deepEqual(g(D.parse(D.emit(into)).items), [['e:End:draw+circle', 's:Start:draw', 'w:Wait:draw+rounded corners'], ['e->w:', 's-->e:go', 's-w:'].map((x) => x.replace('-->', '->'))]);
  assert.deepEqual(D.mermaidLoss(ns, es), ['positions']);
  es[0].toAt = 'north'; ns[0].opts.push('fill=red'); es[1].arrow = '-Stealth';
  assert.deepEqual(D.mermaidLoss(ns, es, 2), ['positions', 'node styles', 'anchors', 'arrow tips', '2 statements kept as code']);
  // import replaces, or merges by name
  const rep = D.parse('\n  \\node (s) at (0,0) {Old};\n  \\draw (0,0) -- (1,1);\n');
  D.importMermaid(rep, 'graph TD\n  s[New] --> t[T]', 2, true);
  assert.deepEqual(D.parse(D.emit(rep)).items.map((i) => i.type), ['code', 'node', 'node', 'edge'], 'the path stays, the old node goes');

  // 6. a relative node dragged: the smallest edit of its placement
  const size = () => [1, 0.6];
  const env = (by, earlier = []) => ({ by: new Map(by.map((n) => [n.name, n])), earlier, size, distance: [1, 1], onGrid: false, grid: 0.5 });
  const rel = D.parse('\n  \\node (a) at (0,0) {A};\n  \\node (b) at (0,-3) {B};\n  \\node (c) [right=of a] {C};\n  \\node (d) [right of=a] {D};\n');
  const [na, nb, nc, nd] = rel.items;
  // the same relation, a new distance; the default distance needs none
  assert.deepEqual((({ opts, hint }) => [opts, hint])(D.relocate(nc, 4.1, 0.1, env([na]))), [['right=3cm of a'], 'right of a, 3 cm']);
  assert.deepEqual(D.relocate(nc, 2, 0, env([na])).opts, ['right=of a']);
  assert.deepEqual(D.relocate(nd, 2.5, 0, env([na])).opts, ['right of=a', 'node distance=2.5cm'], 'centers apart: its own node distance');
  // off the line, a little: a shift
  assert.deepEqual(D.relocate(nc, 2, 0.5, env([na])).opts, ['right=of a', 'yshift=0.5cm']);
  // far off: a relation to the nearest node before it, else `at`
  assert.deepEqual(D.relocate(nc, 0, -4.6, env([na, nb], [na, nb])).opts, ['below=of b']);
  assert.deepEqual(D.relocate(nc, 3.2, -2.1, env([na, nb], [na, nb])).opts, ['above right=0.5cm and 2cm of b'], 'borders apart: a diagonal reads any place');
  const free = D.relocate(nd, 3.2, -2.1, env([na, nb], [na, nb]));
  assert.deepEqual([free.opts, free.at, free.x, free.y, free.place], [[], true, 3, -2, null]);
  // kept relative (a locked node, or the global toggle): where no relation reads the place, a shift from the nearest node does
  const kept = D.relocate(nd, 3.2, -2.1, { ...env([na, nb], [na, nb]), keep: true });
  assert.deepEqual([kept.at, kept.place.of, kept.place.shift], [false, 'b', true]);
  assert.deepEqual(D.relPos({ opts: kept.opts, place: kept.place }, nb, env([])), [3, -2], 'TikZ puts it where it was dropped, on the grid');
  // written back and read back: the node as placed
  Object.assign(nc, D.relocate(nc, 4.1, 0.1, env([na])), { dirty: true });
  const back2 = D.parse(D.emit(rel)).items[2];
  assert.deepEqual([back2.place.dir, back2.place.of, back2.place.dist], [[1, 0], 'a', '3cm']);
  assert.deepEqual(D.relPos(back2, { name: 'a', x: 0, y: 0 }, env([])), [4, 0]);

  // 7. the picture's options: one key changed, the rest as typed
  const po = D.picOptions('node distance=2cm, every node/.style={draw, circle}, >=Stealth');
  assert.deepEqual([po.get('node distance'), po.get('every node/.style'), po.get('>')], ['2cm', 'draw, circle', 'Stealth']);
  po.set('every node/.style', 'draw');
  po.set('scale', '2');
  po.set('>', '');
  assert.deepEqual(po.keys(), [{ key: 'node distance', value: '2cm' }, { key: 'every node/.style', value: 'draw' }, { key: 'scale', value: '2' }]);
  // scaled: the text's coordinates written back as they were
  const sc = D.parse('\n  \\node (a) at (1,2) {A};\n');
  Object.assign(sc.items[0], { x: 3, y: 4, dirty: true });
  assert.equal(D.emit(sc, 2), '\n  \\node (a) at (1.5,2) {A};\n');
});
test("drawing 2", async () => {
  const D = self.satexViews.drawing;
  // 12. a bend makes a curve on a `to` path: read with its options, written with `to` once an arrow has one
  const bent = D.parse('\n  \\draw[->] (a) to[bend left=20] (b);\n  \\draw (a) -- (b);\n');
  const [c1, c2] = bent.items;
  assert.deepEqual([c1.type, c1.via, c2.via], ['edge', 'bend left=20', undefined]);
  c2.opts = ['bend right']; c2.dirty = true;
  assert.equal(D.emit(bent), '\n  \\draw[->] (a) to[bend left=20] (b);\n  \\draw[bend right] (a) to (b);\n');
  // 13. a node at (x,y) moved in relative mode: a relation to the nearest node before it, distances on the grid
  {
    const sz = () => [1, 0.6], A = { name: 'a', x: 0, y: 0, opts: [] };
    const env = { size: sz, distance: [1, 1], onGrid: false, grid: 0.5, by: new Map([['a', A]]), earlier: [A] };
    const r = D.relocate({ name: 'b', x: 3.1, y: 0, opts: ['draw'], place: null }, 3.1, 0.05, env);
    assert.deepEqual([r.opts, r.at], [['draw', 'right=2cm of a'], false], 'border to border: 3 - 1 = 2cm');
    const placed = D.relPos({ name: 'b', opts: r.opts, place: r.place }, A, env);
    assert.deepEqual(placed, [3, 0], 'TikZ puts it on the grid point next to where it was dropped');
    const lone = D.relocate({ name: 'b', x: 1, y: 1, opts: [], place: null }, 1, 1, { ...env, earlier: [] });
    assert.deepEqual([lone.at, lone.x, lone.y], [true, 1, 1], 'no node before it: at (x,y)');
    // where a size is a guess, only a shift from the neighbour's center is sure: `at (a)` shifted
    const sure = D.relocate({ name: 'b', x: 3.1, y: -1, opts: ['draw'], place: null }, 3.1, -1, { ...env, exact: (n) => n.name === 'a' });
    assert.deepEqual([sure.opts, sure.place.of, sure.place.shift], [['draw', 'xshift=3cm', 'yshift=-1cm'], 'a', true]);
    assert.deepEqual(D.relPos({ opts: sure.opts, place: sure.place }, A, env), [3, -1]);
    const sh = D.parse('\n  \\node[draw, xshift=3cm, yshift=-1cm] (b) at (a) {B};\n');
    assert.deepEqual([sh.items[0].type, sh.items[0].place.of, sh.items[0].place.shift, sh.items[0].at], ['node', 'a', true, false]);
    sh.items[0].dirty = true;
    assert.equal(D.emit(sh), '\n  \\node[draw, xshift=3cm, yshift=-1cm] (b) at (a) {B};\n', 'written back the same');
  }
  // 11. styles: a name in the options is followed by its keys, nested ones too; a style with a parameter is not
  const bodies = new Map([['box', 'draw, hot'], ['hot', 'fill=red!20'], ['thick', 'line width=0.8pt'], ['arg', 'fill=#1'], ['loop', 'loop']]);
  assert.deepEqual(D.expandStyles(['box', 'thick', 'text=blue'], bodies), ['box', 'draw', 'hot', 'fill=red!20', 'thick', 'line width=0.8pt', 'text=blue']);
  assert.deepEqual(D.expandStyles(['arg'], bodies), ['arg']);
  assert.ok(D.expandStyles(['loop'], bodies).length < 10, 'a style naming itself stops');
  assert.deepEqual(['box/.style', 'every node/.append style', 'draw'].map((x) => x.match(D.STYLE_DEF)?.[1] ?? null), ['box', 'every node', null]);
  // the picture's options as the snapshot's keys (unbraced values, bare keys null); a style dropped leaves the rest
  const po = D.picOptions('node distance=2cm, box/.style={draw, thick}, transform shape');
  po.set('hot/.style', 'fill=red!20');
  assert.deepEqual(po.keys().map((k) => [k.key, k.value]), [['node distance', '2cm'], ['box/.style', 'draw, thick'], ['transform shape', null], ['hot/.style', 'fill=red!20']]);
  po.set('box/.style', null);
  assert.deepEqual(po.keys().map((k) => k.key), ['node distance', 'transform shape', 'hot/.style']);
});

// Circuits (circuitikz): a chain splits into parts, ground and gates are read, other statements stay code, and an
// untouched statement is written back as it was. Pins the circuit view's round trip.
test("Circuits (circuitikz)", async () => {
  const C = self.satexViews.circuit, body = '\n  \\draw (0,0) to[R, l=$R_1$] (2,0) -- (2,2);\n  \\draw (0,0) node[ground] {};\n  \\node[and port] (G1) at (4,1) {};\n  \\fill (1,1) circle (2pt);\n';
  const st = C.parse(body), parts = st.stmts.map((s) => s.parts?.map((p) => [p.kind, p.label, p.name ?? null, JSON.stringify(p.a), JSON.stringify(p.b ?? null)]) ?? null);
  assert.deepEqual(parts, [[['R', '$R_1$', null, '[0,0]', '[2,0]'], ['short', '', null, '[2,0]', '[2,2]']], [['ground', '', null, '[0,0]', 'null']], [['and port', '', 'G1', '[4,1]', 'null']], null]);
  assert.equal(C.body(st), body, 'untouched: as written');
  st.stmts[0].parts[1] = { ...st.stmts[0].parts[1], b: 'G1.in 1' };
  st.stmts[0].dirty = true;
  assert.ok(C.body(st).includes('\\draw (0,0) to[R, l=$R_1$] (2,0);\n  \\draw (2,0) -- (G1.in 1);'), 'an edited chain: one statement per part');
  assert.equal(C.emit({ kind: 'C', a: [0, 0], b: [0, 1.5], label: 'a, b', opts: ['v=$U$'] }), '\\draw (0,0) to[C, l={a, b}, v=$U$] (0,1.5);');
});
