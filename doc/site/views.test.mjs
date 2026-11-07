// The view models of views.js that need no page: node doc/site/views.test.mjs
import { createRequire } from 'node:module';
import assert from 'node:assert/strict';

globalThis.self = globalThis;
createRequire(import.meta.url)('./lengths.js');
createRequire(import.meta.url)('./snapshot.js');
createRequire(import.meta.url)('./views.js');
const { parse, emit, mermaidIn, mermaidOut, importMermaid } = self.satexViews.drawing;

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

// the canvas's model beyond nodes and arrows (views.js `drawing`)
{
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
}
{
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
}
console.log('drawing model: ok');

// forest trees: bracket → state → bracket, untouched text as it was
const F = self.satexViews.forest;
const tree = `
for tree={draw, s sep=5mm},
  % a comment [not a node]
[VP [DP, name=dp]
    [V$'$ [V]  [{D, P}, edge={red, thick}]]
]
\\draw (dp) -- (!r1);
`;
const tr = F.parse(tree);
assert.equal(F.emit(tr), tree, 'untouched, the tree comes back as it was');
assert.equal(tr.root.content, 'VP');
assert.deepEqual(tr.root.children.map((c) => c.content), ['DP', "V$'$"]);
assert.deepEqual(tr.root.children[0].opts, ['name=dp']);
assert.deepEqual(tr.root.children[1].children[1].opts, ['edge={red, thick}']);
assert.equal(tr.root.children[1].children[1].content, '{D, P}');
assert.ok(tr.post.includes('\\draw'));
assert.deepEqual(F.splitTop('a, b={c,d}, e'), ['a', 'b={c,d}', 'e']);
assert.equal(tr.pre.slice(...F.forTree(tr.pre)), 'draw, s sep=5mm');
// edits: rename, options, add, delete, move; the rest keeps its text
const [dp, vbar] = tr.root.children;
Object.assign(dp, { content: 'NP', dirty: true });
Object.assign(vbar.children[0], { opts: ['circle'], dirty: true });
vbar.children.push(F.fresh('new, one'));
F.move(tr, dp, vbar, 'child');
F.setForTree(tr, 'draw');
F.settle(tr);
const edited = F.emit(tr);
assert.equal(edited, `
for tree={draw},
  % a comment [not a node]
[VP
    [V$'$ [V, circle]  [{D, P}, edge={red, thick}]  [{new, one}]  [NP, name=dp]]
]
\\draw (dp) -- (!r1);
`);
assert.equal(F.emit(F.parse(edited)), edited, 'the edited tree reads back as itself');
// a node cannot move into its own subtree
assert.equal(F.move(tr, vbar, vbar.children[0], 'child'), false);
// an indented outline
const outline = '- S\n  - NP\n    - the cat\n  * VP\n\t- sat\n';
const [s0] = F.outlineIn(outline);
const fresh = { pre: '\n', root: s0, post: '\n' };
s0.place = '';
F.settle(fresh);
assert.equal(F.emit(fresh), '\n[S [NP [the cat]] [VP [sat]]]\n');
assert.equal(F.outlineOut(s0), '- S\n  - NP\n    - the cat\n  - VP\n    - sat');
// a preamble without `for tree` gets one
const bare = F.parse('\n[a [b]]\n');
F.setForTree(bare, 'circle');
assert.equal(F.emit(bare), '\nfor tree={circle}\n[a [b]]\n');
assert.equal(F.parse('no tree here'), null);
console.log('ok views.test.mjs');

// the table dialog's model (dialogs.js): merges, CSV/JSON round trips, paste
createRequire(import.meta.url)('./dialogs.js');
const T = self.satexDialogs.table;
const cell = (text) => ({ text, span: 1, spec: '' });
const grid = () => ({
  env: 'tabular', cols: [{ align: 'l' }, { align: 'c' }, { align: 'r' }].map((c) => ({ arg: '', pre: '', post: '', ...c })), seps: ['', '', '', ''],
  rows: [['Method', 'Score', 'Time'], ['Ours', '0.92', '1.5'], ['Base', '0.81', '2']].map((r, k) => ({ before: k === 1 ? ['\\hline'] : [], inline: false, cells: r.map(cell), end: '\\\\', comment: '', raw: '' })),
  after: [], style: { indent: '  ', spaced: true, aligned: false, inline_rules: false },
});
const texts = (s) => s.rows.map((r) => r.cells.map((c) => c.span > 1 ? `${c.span}:${c.text}` : c.text));
{
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
}
{
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
}
{
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
}
{
  // array's injects: the alignment is the column's letter, not one inside `>{\bf}`; the preview sets what the inject switches
  assert.deepEqual(['>{\\bf}r', '>{\\bfseries}l<{\\%}', 'p{3cm}', '@{}>{\\centering\\arraybackslash}m{2cm}'].map((x) => T.alignOf(x)), ['r', 'l', 'p', 'm']);
  assert.equal(T.alignOf('>{\\bf}b{2cm}', 'lcrS'), 'l');
  assert.equal(T.injectStyle('\\bf'), 'font-weight:bold');
  assert.equal(T.injectStyle('\\itshape\\color{blue}'), 'font-style:italic;color:blue');
  assert.equal(T.injectStyle('\\arraybackslash'), '');
}
console.log('table model: ok');

// crop to content: the box of the pixels unlike the corners' background, then trim in bp with padding
{
  const { contentBox, trimTo } = self.satexDialogs.image;
  const raster = (w, hh, bg, ink) => { const d = new Uint8ClampedArray(w * hh * 4); for (let i = 0; i < w * hh; i++) d.set(bg, i * 4); for (const [x, y] of ink) d.set([0, 0, 0, 255], (y * w + x) * 4); return d; };
  const white = [255, 255, 255, 255], clear = [0, 0, 0, 0];
  assert.deepEqual(contentBox(raster(10, 8, white, [[2, 3], [6, 5]]), 10, 8), [2, 3, 7, 6], 'white margins');
  assert.deepEqual(contentBox(raster(10, 8, clear, [[0, 0], [5, 5]]), 10, 8), [0, 0, 6, 6], 'transparent background, ink in a corner');
  assert.equal(contentBox(raster(4, 4, white, []), 4, 4), null, 'blank: nothing to crop to');
  assert.deepEqual(contentBox(raster(10, 8, [250, 250, 250, 255], [[4, 4]]), 10, 8), [4, 4, 5, 5], 'near-white background within tolerance');
  // 10 × 8 px of 100 × 80 bp: trim l, b, r, t; padding grows the box and never goes below 0
  assert.deepEqual(trimTo([2, 3, 7, 6], 10, 8, [100, 80]), [20, 20, 30, 30]);
  assert.deepEqual(trimTo([2, 3, 7, 6], 10, 8, [100, 80], 25), [0, 0, 5, 5]);
  assert.deepEqual(trimTo([2, 3, 7, 6], 10, 8, [100, 80], -5), [25, 25, 35, 35], 'negative padding cuts into the content');
}

// tint: the channels decodearray maps (PNG colour type, JPEG components) and its presets
{
  const { colorModel, DECODE } = self.satexDialogs.image;
  const png = (type) => { const b = new Uint8Array(33); b.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]); b[25] = type; return b; };
  assert.deepEqual([0, 2, 3, 4, 6, 5].map((t) => colorModel(png(t))), ['gray', 'rgb', 'indexed', 'gray', 'rgb', null]);
  // SOI, an APP0 of 4 bytes, then SOF0 whose 10th byte counts the components
  const jpeg = (n) => Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8, 0, 1, 0, 1, n, 1, 0x11, 0]);
  assert.deepEqual([1, 3, 4].map((n) => colorModel(jpeg(n))), ['gray', 'rgb', 'cmyk']);
  assert.equal(colorModel(Uint8Array.from([0x25, 0x50, 0x44, 0x46])), null, 'a PDF has no raster channels');
  assert.equal(colorModel(new Uint8Array(0)), null);
  assert.deepEqual(DECODE.invert(6), [1, 0, 1, 0, 1, 0]);
  assert.deepEqual(DECODE.tint(6, [1, 0, 0.5], 1), [0, 1, 0, 0, 0, 0.5], 'full strength: white becomes the colour');
  assert.deepEqual(DECODE.tint(6, [1, 0, 0.5], 0), [0, 1, 0, 1, 0, 1], 'no strength: as in the file');
  assert.deepEqual(DECODE.tint(2, [1, 1, 1], 1), [0, 1], 'gray: the luminance of white');
  assert.deepEqual(DECODE.of(null), ['none']);
  assert.deepEqual(DECODE.of([1, 0]), ['invert']);
  assert.deepEqual(DECODE.of([0, 1, 0, 0, 0, 0.5]), ['tint', [1, 0, 0.5]]);
  assert.deepEqual(DECODE.of([0.2, 1, 0, 1, 0, 1]), ['custom']);
}

// dev mode: a trace as CSV (nested state one JSON cell) and a step's domains as HTML
{
  const { traceCsv, stateHtml } = self.satexViews.dev;
  const state = { paths: { live: 2, forks: 1, peak: 2, apart: 'why-apart main.tex:3' }, choices: [{ by: '\\ifx', at: 'main.tex:3:1', arm: 0, arms: 2, joined: false }],
    mode: ['vertical'], conditionals: [{ at: 'main.tex:3:1', limit: 'Else', many: false }], groups: [], input: [{ file: 'main.tex:3:9' }, { tokens: 4, expanding: '\\x' }],
    bindings: [{ name: '\\x', meaning: 'macro:->a', certain: false, global: false, may: ['macro:->a', 'undefined'], maybe_undefined: true, value: null },
      { name: '\\count@', meaning: '\\count255', certain: true, global: false, value: { type: 'number', interval: [0, 9] } }],
    paired: { names: [], arms: 0 }, catcodes: { '{': 1, '@': 12 } };
  const records = [{ index: 0, name: '\\x', detail: 'a, "b"' }, { index: 1, name: '\\y', state }];
  const csv = traceCsv(records).trim().split('\n');
  assert.equal(csv[0], 'index,name,detail,state');
  assert.equal(csv[1], '0,\\x,"a, ""b""",');
  assert.deepEqual(JSON.parse(csv[2].split(',\\y,,')[1].slice(1, -1).replace(/""/g, '"')), state, 'the state cell reads back');
  const html = stateHtml(state);
  for (const part of ['2 live', 'arm 1 of 2', 'maybe undefined', '[0, 9]', 'number', 'why-apart', 'tokens of \\x', '<sub>12</sub>']) assert.ok(html.includes(part), part);
  // each place opens: `file:line[:col]` a link to its line
  assert.ok(html.includes('file <a class="place" href="#" data-path="main.tex" data-line="3"'), html);
  assert.ok(stateHtml({ ...state, groups: [{ kind: 'Simple', at: 'latex.ltx:15355:3' }] }).includes('data-path="latex.ltx" data-line="15355"'));
  assert.ok(stateHtml(null).includes('without dev mode'));
}

// The typeset view's notation (site.js, loaded on a page with nothing on it: every lookup finds nothing)
{
  const inert = new Proxy(function () {}, { get: (t, k) => (k === Symbol.iterator ? [][Symbol.iterator] : k === 'length' ? 0 : inert), apply: () => inert });
  globalThis.document ??= { documentElement: { dataset: {} }, getElementById: () => null, querySelector: () => ({ dataset: {} }), querySelectorAll: () => [], addEventListener() {}, createElement: () => inert, head: inert, body: inert };
  globalThis.matchMedia ??= () => ({ matches: false, addEventListener() {} });
  globalThis.location ??= { href: 'http://localhost/' };
  // the page's own wiring fails without a page; the cards are set before it
  try { createRequire(import.meta.url)('./site.js'); } catch {}
  const { notation, typesetView, withKeys, keyTitle } = self.satexCards;
  const N = (s) => `${s}`, M = (m) => N(`\u0003${m}`);
  // booktabs: the rule after the last row borders that row across every column; a cell of leaders alone is a rule over that column
  const booktabs = notation(`${N('―')}A${M('cell')}1${M('row')}${M('cell')}${M('leaders')}${M('fill 0.0pt plus 1.0fill')}${M('row')}B${M('cell')}2${M('row')}${N('―')}${M('end')}`.replace(/^/, M('begin')));
  assert.match(booktabs, /<span class="ts-tr ts-ruled ts-rule-after"><\/span><\/span>$/);
  assert.match(booktabs, /<span class="ts-td"><\/span><span class="ts-td ts-rule" title="a rule over this column"><i class="ts-fill/);
  assert.equal(booktabs.match(/class="ts-td"/g).length, 5);
  // display limits: one column of over, operator, under; a sub after them is a script again
  const sum = notation(`${N('\u0006L')}∑${N('\u0006')}${M('under')}i=1${M('scriptend')}${M('over')}N${M('scriptend')}x${M('sub')}i${M('scriptend')}`);
  assert.equal(sum, '<span class="ts-limits"><span class="ts-op">∑</span><span class="ts-under">i=1</span><span class="ts-over">N</span></span>x<span class="ts-sub">i</span>');
  // text style keeps them at the side
  assert.equal(notation(`∑${M('sub')}i${M('scriptend')}${M('sup')}n${M('scriptend')}`), '∑<span class="ts-sub">i</span><span class="ts-sup">n</span>');
  // an equation's number at the margin, closed at the end
  assert.match(notation(`a${M('eqno')}(1)`), /^a<span class="ts-eqno"[^>]*>\(1\)<\/span>$/);
  assert.match(notation(`b${M('leqno')}(2)`), /<span class="ts-leqno"/);
  // an undecided conditional's arms, its switch named; marks the view does not know draw nothing, in a table too
  assert.equal(notation(`${N('\u0001choice\u00017')}${N('{')}Heads${N('|')}${N('}')}.`), '<span class="ts-choice" data-on="7"><span class="ts-arm ts-arm-true">Heads</span><span class="ts-arm ts-arm-false"></span></span>.');
  assert.ok(!notation(`${M('begin')}a${M('limits')}${M('cell')}b${M('end')}`).includes('ts-limits'));
  // keys: the run's role, not a table of kinds, says what defines and what refers; one title for the Typeset view and the pages
  const keyed = notation(withKeys('ab', [{ kind: 'key', key: 'tex', role: 'def', at: 0 }, { kind: 'key-use', key: 'tex', role: 'use', at: 1 }, { kind: 'label', key: 'fig:a', role: 'def', at: 2 }]));
  assert.deepEqual([...keyed.matchAll(/class="ts-key ([^"]*)" title="([^"]*)"/g)].map((m) => [m[1], m[2]]),
    [['ts-key-key ts-key-def', 'defines tex (key)'], ['ts-key-key-use ts-key-use', 'refers to tex (key-use)'], ['ts-key-label ts-key-def', 'defines fig:a (label)']]);
  assert.equal(keyTitle({ kind: 'entry', key: 'source', role: 'use' }), 'refers to source (entry)');
  assert.equal(keyTitle({ kind: 'choice', key: '7' }), 'choice 7', 'no role: no claim about the key');
  // glue with its spec: infinite glue in a line still sets where the text sits, glue between lines stands apart
  const filled = notation(`${M('fill 0.0pt plus 1.0fill')}Right`);
  assert.match(filled, /^<i class="ts-fill ts-fill-l" title="infinite glue in the line: 0.0pt plus 1.0fill" data-glue="0.0pt plus 1.0fill"><\/i>Right$/);
  assert.match(notation(`A${M('hskip 2.0pt plus 3.0pt')}B`), /^A<i class="ts-glue ts-glue-hskip" title="glue in the line: 2.0pt plus 3.0pt"[^>]*><\/i>B$/);
  // glue between lines: its natural size sets the mark's, glue that stretches without bound is marked so
  assert.match(notation(`A${M('vskip 6.0pt plus 2.0pt')}B`), /<i class="ts-glue ts-glue-vskip" [^>]*style="--glue: 6pt"/);
  assert.match(notation(`A${M('vskip 0.0pt plus 1.0fil')}B`), /<i class="ts-glue ts-glue-vskip ts-inf" [^>]*style="--glue: 0pt"/);
  // a \parskip before a centred line leaves it centred
  assert.match(notation(`${M('fill 0.0pt plus 1.0fil')}${M('parskip 0.0pt plus 1.0pt')}Mid${M('fill 0.0pt plus 1.0fil')}`), /^<span class="ts-fillrow">/);
  // glue in a row keeps the cell's fill on its side
  assert.match(notation(`${M('begin')}a${M('hskip 0.0pt plus 1.0pt')}${M('fill 0.0pt plus 1.0fil')}${M('end')}`), /ts-fill-r/);
  assert.equal(notation(`${M('begin')}a${M('mystery')}${M('cell')}b${M('end')}`).match(/<span/g).length, notation(`${M('begin')}a${M('cell')}b${M('end')}`).match(/<span/g).length);
  // environments: a begin mark before the first row in one, an end mark after its last, nested outermost first
  const quote = { env: 'quote', line: 2, end: 9 }, center = { env: 'center', line: 4, end: 6 };
  const rows = [
    { kind: 'paragraph', line: 1, detail: 'Before.' },
    { kind: 'paragraph', line: 3, detail: 'Outer.', envs: [quote], depth: 1 },
    { kind: 'paragraph', line: 5, detail: 'Inner.', envs: [quote, center], depth: 2 },
    { kind: 'paragraph', line: 10, detail: 'After.' },
  ];
  const marks = [...typesetView(rows).matchAll(/ts-env-(begin|end)[^]*?\\(?:begin|end)\{(\w+)\}|<p [^]*?<span class="ts-text">(\w+)/g)].map((m) => m[3] ?? `${m[1]} ${m[2]}`);
  assert.deepEqual(marks, ['Before', 'begin quote', 'Outer', 'begin center', 'Inner', 'end center', 'end quote', 'After']);
  // a paragraph that may be set carries a switch per conditional
  const may = typesetView([{ kind: 'paragraph', line: 3, detail: 'x', marked: 'x', causes: [{ on: '7', test: '\\ifdraft', line: 3, mark: '{x|}' }] }]);
  assert.match(may, /<span class="ts-switch" data-on="7" title="may be typeset: depends on \\ifdraft \(line 3\)[^>]*><span class="ts-switch-q">maybe<\/span><code>\\ifdraft<\/code><button type="button" data-arm="both" aria-pressed="true"/);
  // a macro of several meanings is no test: its options are the texts its paths set
  const meant = typesetView([{ kind: 'paragraph', line: 3, detail: 'x', marked: 'x', causes: [{ on: '8', test: '', parted: '`\\tleapy`', line: 3, mark: '{|not }', arms: ['', 'not '] }] }]);
  assert.deepEqual([...meant.matchAll(/data-arm="(\w+)"[^>]*>(.*?)<\/button>/g)].map((m) => [m[1], m[2]]), [['both', 'both'], ['true', '<code>empty</code>'], ['false', '<code>not␣</code>']]);
  assert.match(may, /data-arm="true"[^>]*>true<\/button>/);
  // the document's marks from the open file's lines; a block's header joins its begin mark
  const doc = ['\\documentclass{article}', '\\begin{document}', 'x', '\\begin{figure}[ht]', 'y\\label{f}', '\\end{figure}', '\\end{document}'];
  const fig = { env: 'figure', line: 4, end: 6 };
  const view = typesetView([{ kind: 'paragraph', line: 3, detail: 'x' }, { kind: 'paragraph', line: 5, detail: 'y', envs: [fig], depth: 1, outer: { env: 'figure', line: 4, labels: ['f'] } }], { lineOf: (n) => doc[n - 1] });
  assert.deepEqual([...view.matchAll(/\\(begin|end)\{(\w+)\}/g)].map((m) => m[0]), ['\\begin{document}', '\\begin{figure}', '\\end{figure}', '\\end{document}']);
  assert.match(view, /\\begin\{figure\}<\/span><code class="ts-block-place"[^>]*>\[ht\]<\/code><code class="ts-block-label"[^>]*>f<\/code>/);
}

// the color band: groups by who defined the color (the document first), alike colors side by side, greys last
{
  const { colorBand } = self.satexViews;
  const c = (name, rgb, group) => ({ name, rgb, group });
  const band = colorBand([c('gray', [0.5, 0.5, 0.5], 'xcolor'), c('blue', [0, 0, 1], 'xcolor'), c('red', [1, 0, 0], 'xcolor'), c('darkred', [0.5, 0, 0], 'xcolor'),
    c('Sky', [0.4, 0.7, 1], 'dvipsnam'), c('brand', [0, 0.5, 0.5], 'document'), c('white', [1, 1, 1], 'xcolor')]);
  assert.deepEqual(band.map((g) => g.name), ['document', 'xcolor', 'dvipsnam']);
  assert.deepEqual(band[1].colors.map((d) => d.name), ['darkred', 'red', 'blue', 'gray', 'white'], 'a hue family dark to light, greys last');
}

// a length field's text, coloured as the editor colours it: names (each with its hover card) and numbers
{
  const { texTokens } = self.satexDialogs;
  assert.equal(texTokens('0.8\\linewidth'), '<span class="tok-num">0.8</span><span class="tok-cmd cs-tip" data-cs="\\linewidth">\\linewidth</span>');
  assert.equal(texTokens('5cm'), '<span class="tok-num">5</span>cm');
  assert.equal(texTokens('<\\a'), '&lt;<span class="tok-cmd cs-tip" data-cs="\\a">\\a</span>', 'text escaped');
}

// the widgets' shared undo: one step per settled change, a held gesture as one, reset undoable
{
  let state = { a: 1 };
  const hist = self.satexDialogs.history(() => state, (s) => { state = s; });
  state = { a: 2 }; hist.settle();
  hist.hold(true); state = { a: 3 }; hist.settle(); state = { a: 4 }; hist.hold(false);
  assert.ok(hist.undo()); assert.deepEqual(state, { a: 2 }, 'the held gesture is one step');
  assert.ok(hist.redo()); assert.deepEqual(state, { a: 4 });
  hist.reset(); assert.deepEqual(state, { a: 1 });
  assert.ok(hist.undo()); assert.deepEqual(state, { a: 4 }, 'reset undoes');
  state = { a: 5 }; hist.settle();
  assert.equal(hist.redo(), false, 'a new change drops the redo steps');
}

// array's column injections: the text inside `>{…}` and `<{…}`, several joined
{
  const T = self.satexDialogs.table;
  assert.equal(T.injected('>{\\bfseries}'), '\\bfseries');
  assert.equal(T.injected('>{$}>{\\mathbf{x}}'), '$\\mathbf{x}');
  assert.equal(T.injected(''), '');
}

// the console's hover (cli.js): each word of a command line as the command tree names it
{
  createRequire(import.meta.url)('./cli.js');
  const { at, card } = self.satexCli;
  const fix = { id: 'fix', long: 'fix', takes_value: false, help: 'Apply the safe fixes' };
  const format = { id: 'format', long: 'format', takes_value: true, possible_values: ['text', 'json'], defaults: ['text'], help: 'Output format' };
  const tree = { queries: ['labels'], command: { name: 'satex', args: [format], subcommands: [
    { name: 'lint', about: 'Lints the document', args: [fix, format], subcommands: [] },
    { name: 'query', about: 'Asks the run', args: [format], subcommands: [] }] } };
  const line = '$ satex lint --fix --format json';
  const on = (word, off = 1) => at(tree, line, line.indexOf(word) + off);
  assert.equal(on('lint').kind, 'command');
  assert.match(card(on('lint')), /satex lint.*Lints the document/);
  assert.equal(on('--fix').arg, fix);
  assert.match(card(on('--fix')), /--fix.*flag.*Apply the safe fixes/);
  assert.deepEqual([on('json').kind, on('json').arg], ['value', format]);
  assert.match(card(on('--format')), /one of <code>text<\/code> <code>json<\/code> · default <code>text<\/code>/);
  assert.equal(at(tree, 'satex query labels', 13).kind, 'query');
  assert.equal(at(tree, 'satex lint --nope', 12), null);
}
// the layers of both viewers (layers.js): one catalogue, grouped alike, kept per viewer
createRequire(import.meta.url)('./layers.js');
const layers = self.satexLayers;
const TYPESET = ['envs', 'keys', 'boxes', 'glue', 'ligs', 'findings', 'maybe'], PDF = ['envs', 'keys', 'glue', 'geometry', 'figures', 'findings', 'bad'];
assert.deepEqual(layers.groups(TYPESET), [['structure', 'Structure', ['envs', 'keys']], ['layout', 'Layout', ['boxes', 'glue', 'ligs']], ['analysis', 'Analysis', ['findings', 'maybe']]]);
// a layer both draw sits in the same group under the same name
for (const k of TYPESET.filter((k) => PDF.includes(k))) {
  const where = (ids) => layers.groups(ids).find((g) => g[2].includes(k))[0];
  assert.equal(where(TYPESET), where(PDF), k);
}
assert.deepEqual(layers.groups(['maybe', 'envs']).map((g) => g[0]), ['structure', 'analysis'], 'an empty group is left out');
// the markup: a labelled group each, one switch per layer named by its viewer, a fold only where asked
const about = Object.fromEntries(PDF.map((k) => [k, `about ${k}`]));
const markup = layers.html(about, { glue: true }, { prefix: 'pdf-layer', facets: new Set(['glue']) });
assert.deepEqual([...markup.matchAll(/class="layer-label">(\w+)</g)].map((m) => m[1]), ['Structure', 'Layout', 'Analysis']);
assert.deepEqual([...markup.matchAll(/id="pdf-layer-(\w+)"/g)].map((m) => m[1]), ['envs', 'keys', 'glue', 'geometry', 'figures', 'findings', 'bad']);
assert.match(markup, /id="pdf-layer-glue" data-layer="glue" checked/);
assert.doesNotMatch(markup, /id="pdf-layer-envs" data-layer="envs" checked/);
assert.deepEqual([...markup.matchAll(/data-fold="(\w+)"/g)].map((m) => m[1]), ['glue']);
// the state: defaults, under what an older version kept, under the store's; each viewer its own key
const kept = new Map(), store = { get: (k) => kept.get(k) ?? null, set: (k, v) => kept.set(k, v) };
const on = layers.state('typeset', { glue: false, maybe: true, keys: false }, store, { keys: true });
assert.deepEqual(on, { glue: false, maybe: true, keys: true });
let applied = null;
const root = { addEventListener: (_, f) => { root.fire = f; } };
layers.wire(root, 'typeset', on, store, (k) => { applied = k; });
root.fire({ target: { dataset: { layer: 'glue' }, checked: true } });
root.fire({ target: { dataset: { layer: 'nothing' }, checked: true } });
assert.equal(applied, 'glue');
assert.deepEqual(kept.get('layers-typeset'), { glue: true, maybe: true, keys: true });
assert.equal(kept.get('layers-pdf'), undefined);
assert.deepEqual(layers.state('typeset', { glue: false, maybe: true, keys: false }, store, { keys: false, glue: false }), { glue: true, maybe: true, keys: true }, 'the store wins over older keys');
// the grid's step: any TeX unit, em and ex only with the run's font, nothing at or below 0
assert.deepEqual(layers.length(' 1in'), { pt: 72.27, num: 1, unit: 'in' });
assert.equal(layers.length('2pc').pt, 24);
assert.equal(layers.length('1em'), null);
assert.equal(layers.length('1.5em', { em: 10 }).pt, 15);
assert.deepEqual([layers.length('0cm'), layers.length('cm'), layers.length('-1pt')], [null, null, null]);
assert.deepEqual(layers.groups(['grid', 'glue', 'geometry']), [['layout', 'Layout', ['glue', 'geometry', 'grid']]]);

// page layout: the run's lengths as body margins; Apply sends snapshots the server prints:
// geometry's keys of what changed, typearea's DIV and BCOR, \newgeometry around a part, fancyhdr's fields grouped by text
{
  const P = self.satexViews.page, S = self.satexSnapshot;
  const cm = 72.27 / 2.54 * 65536, two = [2 * cm, 2 * cm].map(Math.round);
  const at = (line) => ({ line, character: 0 }), here = { start: at(1), end: at(1) };
  const same = (v) => [v, v];
  // a call as the server reads it (src/lsp/snapshot.rs `read`)
  const read = (name, opts, req) => ({ kind: 'call', name, star: false, range: here, text: `\\${name}[${opts}]{${req}}`, line: false, drop: false,
    args: [{ open: '[', inner: [0, 0], text: opts, keys: opts.split(',').map((o) => o.split('=')).map(([key, value]) => ({ key, value: value ?? null })) }, { open: '{', inner: [0, 0], text: req }] });
  const pairs = (n) => S.list(n).map((k) => k.value == null ? k.key : `${k.key}=${k.value}`);
  const s = {
    run: { paperwidth: same(40258437), paperheight: same(52099153), textwidth: same(32799719), textheight: same(44640435), oddsidemargin: same(-1006927), evensidemargin: same(-1006927),
      topmargin: same(-3431759), headheight: same(786432), headsep: same(1638400), footskip: same(1966080), marginparwidth: same(4259840), marginparsep: same(720896) },
    twoside: false, family: 'basic', papers: [{ name: 'a4paper', w: 39158276, h: 55380990 }, { name: 'legalpaper', w: 40258437, h: 66331225 }, { name: 'letterpaper', w: 40258437, h: 52099153 }], units: [{ name: 'cm', sp: cm }, { name: 'pt', sp: 65536 }], unit: 'cm', decimals: 2,
    geometry: { loaded: true, global: read('usepackage', 'margin=2cm', 'geometry'), package: null }, koma: null,
    part: { here: at(3), marks: [{ title: 'One', level: 'section', at: at(5) }], end: at(7) },
    header: { package: null, insert: at(2), load: { name: 'fancyhdr' } },
  };
  const len = (sp) => `${+(sp / cm).toFixed(2)}cm`;
  const st0 = P.pageOf(s);
  assert.deepEqual([st0.left, st0.right, st0.top, st0.bottom].map((v) => Math.abs(v - two[0]) < 2), [true, true, true, true], 'margin=2cm on all sides');
  assert.equal(st0.paper, 'letterpaper', '21.59 x 27.94 cm is letter, not custom');
  assert.deepEqual(P.pageNodes(s, st0, structuredClone(st0), len), [], 'nothing changed, nothing sent');
  // a wider left margin: the margin group set anew, the text narrower
  const wide = { ...structuredClone(st0), left: Math.round(3 * cm) };
  const [w] = P.pageNodes(s, st0, wide, len);
  assert.deepEqual([w.range, pairs(w)], [here, ['left=3cm', 'right=2cm', 'top=2cm', 'bottom=2cm']]);
  assert.deepEqual(pairs(s.geometry.global), ['margin=2cm'], 'the view keeps its snapshot as opened');
  // another paper: its name, the margins kept as written
  const a4 = { ...structuredClone(st0), paper: 'a4paper', pw: 39158276, ph: 55380990 };
  assert.deepEqual(pairs(P.pageNodes(s, st0, a4, len)[0]), ['margin=2cm', 'a4paper']);
  // these pages only: \newgeometry at the cursor, \restoregeometry before the section
  const part = { ...structuredClone(wide), scope: 'part', from: 'here', to: '0' };
  const [ng, rg] = P.pageNodes(s, st0, part, len);
  assert.deepEqual([ng.name, ng.at, ng.range, pairs(ng), rg.name, rg.at, rg.args], ['newgeometry', at(3), null,
    ['left=3cm', 'right=2cm', 'top=2cm', 'bottom=2cm', 'headheight=0.42cm', 'headsep=0.88cm', 'footskip=1.05cm', 'marginparwidth=2.28cm', 'marginparsep=0.39cm'], 'restoregeometry', at(5), []]);
  // without geometry: the new calls load it with `pass`, the layout elsewhere kept
  const pass = { name: 'geometry', options: 'pass' };
  const bare = { ...s, geometry: { ...s.geometry, loaded: false, package: pass } };
  assert.deepEqual(P.pageNodes(bare, st0, { ...part, to: 'end' }, len).map((n) => [n.name, n.package]), [['newgeometry', pass]], 'no \\restoregeometry');
  // typearea: DIV and BCOR as class options, only what changed set
  const koma = { ...s, family: 'koma', koma: { ...read('documentclass', 'DIV=10,BCOR=5mm', 'scrartcl'), div: 10, bcor: Math.round(cm / 2) } };
  const k0 = P.pageOf(koma);
  assert.equal(k0.mode, 'koma');
  assert.deepEqual([k0.paper, k0.div], ['letterpaper', 10], 'typearea keeps its DIV beside the detected paper');
  assert.deepEqual(P.pageNodes(koma, k0, { ...structuredClone(k0), div: 12 }, len).map(pairs), [['DIV=12', 'BCOR=5mm']]);
  const box = P.pageBox({ ...k0, div: 12, ran: null }, true);
  assert.ok(Math.abs(2 * (box.left - k0.bcor) - box.right) < 2, 'two-sided: the outer margin twice the inner');
  // fancyhdr: the fields its calls set, written back grouped by text
  const call = (name, opt, args, line) => ({ kind: 'call', name, range: { start: at(line), end: at(line + 1) }, text: '', args: [...(opt == null ? [] : [{ open: '[', text: opt }]), ...args.map((t) => ({ open: '{', text: t }))] });
  const h = { package: 'fancyhdr', insert: at(2), rules: { head: '0.4pt', foot: '0pt' },
    calls: [call('fancyhf', null, [''], 2), call('fancyhead', 'LE,RO', ['\\thepage'], 3), call('fancyfoot', 'C', ['draft'], 4)] };
  const m = P.headerOf(h, true);
  assert.deepEqual([m.head.EL, m.head.OR, m.head.OL, m.foot.EC, m.foot.OC], ['\\thepage', '\\thepage', '', 'draft', 'draft']);
  assert.deepEqual(P.headerCalls(m, true, true), [['fancyhf', null, ['']], ['fancyhead', 'LE,RO', ['\\thepage']], ['fancyfoot', 'CE,CO', ['draft']]]);
  assert.deepEqual(P.headerNodes(h, P.headerOf(h, true), true), [], 'untouched, nothing sent');
  const m2 = P.headerOf(h, true);
  m2.head.OL = 'Title'; m2.rules.head = '0pt';
  const shown = (n) => n.drop ? `-${n.name}` : `${n.name}${n.args.map((a) => a.open + a.text).join('')}@${n.at.line}`;
  assert.deepEqual(P.headerNodes(h, m2, true).map(shown), ['-fancyhf', '-fancyhead', '-fancyfoot', 'fancyhf{@2', 'fancyhead[LE,RO{\\thepage@2', 'fancyhead[LO{Title@2', 'fancyfoot[CE,CO{draft@2', 'renewcommand{\\headrulewidth{0pt@2']);
  // scrlayer-scrpage: one call per field, the plain style's text kept
  const ks = { package: 'scrlayer-scrpage', insert: at(2), calls: [call('ohead', 'x', ['\\pagemark'], 2)] };
  const km = P.headerOf(ks, true);
  km.foot.Oc = 'draft';
  assert.deepEqual(P.headerNodes(ks, km, true).map(shown), ['-ohead', 'ohead[x{\\pagemark@2', 'cfoot{draft@2']);
  // none loaded: \pagestyle{fancy} before the fields, fancyhdr loaded by the server for them
  const none = { package: null, insert: at(2), load: { name: 'fancyhdr' } };
  const nm = P.headerOf(none, false);
  Object.assign(nm, { package: 'fancyhdr' });
  nm.head.OR = '\\thepage';
  const nn = P.headerNodes(none, nm, false);
  assert.deepEqual(nn.map(shown), ['pagestyle{fancy@2', 'fancyhf{@2', 'fancyhead[R{\\thepage@2']);
  assert.deepEqual(nn[1].package, { name: 'fancyhdr' });
  // the snapshot helper: kept keys stay in place, a dropped group's keys go, bare keys stay bare
  const g = read('geometry', 'left=1cm,top=2cm,a4paper', '');
  S.setKeys(g, ['left', 'top', 'right'], [['top', '3cm'], ['right', '1cm']]);
  assert.deepEqual(pairs(g), ['top=3cm', 'a4paper', 'right=1cm']);
  console.log('page model: ok');
}

// the property editor's key lists (dialogs.js `keyval`): split at top-level commas, a value's braces
{
  const K = self.satexDialogs.keyval;
  assert.deepEqual(K.split('draw, fill={rgb,255:red,1;green,2;blue,3}, label={[red]above:x}, thick'), ['draw', 'fill={rgb,255:red,1;green,2;blue,3}', 'label={[red]above:x}', 'thick']);
  assert.deepEqual(['thick', 'fill=red!20', 'label={above:x}', 'right=of a'].map(K.of), [['thick', null], ['fill', 'red!20'], ['label', 'above:x'], ['right', 'of a']]);
  assert.deepEqual([['thick', null], ['fill', 'red'], ['label', 'above:x, red'], ['x', 'a=b'], ['f', '{rgb:red,1}']].map(([k, v]) => K.text(k, v)), ['thick', 'fill=red', 'label={above:x, red}', 'x={a=b}', 'f={rgb:red,1}']);
  // a key written back reads the same
  for (const o of ['fill=red!20', 'label={above:x, red}', 'thick']) assert.equal(K.text(...K.of(o)), o);
  console.log('property editor model: ok');
}

// the Apply lifecycle (dialogs.js `applier`): Apply before the first answer waits for it, a pending change is
// flushed and its answer written, never an older state's; an answer to a closed widget writes nothing
{
  const { applier } = self.satexDialogs;
  const tick = (ms = 5) => new Promise((r) => setTimeout(r, ms));
  let state = { w: 1 }, slow = 20;
  const written = [], asked = [];
  const life = applier({
    get: () => state, set: (s) => { state = s; }, delay: 50, commit: (edits) => written.push(edits[0].newText),
    ask: async (op) => { const at = JSON.stringify(state); asked.push(at); await tick(slow); return { edits: [{ newText: at + (op ?? '') }] }; },
  });
  life.change();
  life.apply();
  assert.ok(life.wanted && life.waiting, 'Apply before the first answer waits');
  await tick(40);
  assert.deepEqual(written, ['{"w":1}'], 'and writes once it lands');
  state = { w: 2 };
  life.change();
  life.apply();
  await tick(40);
  assert.deepEqual(written.at(-1), '{"w":2}', 'an edit just before Apply is flushed, not dropped');
  assert.equal(asked.length, 2, 'the flush asks once, not after the debounce again');
  state = { w: 3 }; life.change(); await tick(60);
  state = { w: 4 };
  life.apply();
  await tick(40);
  assert.deepEqual(written.at(-1), '{"w":4}', 'a state changed after the last answer is asked for before writing');
  state = { w: 5 }; life.change(); life.apply(); life.cancel(); await tick(80);
  assert.equal(written.length, 3, 'closed: the late answer writes nothing');
  const failing = applier({ get: () => state, set() {}, commit: () => written.push('x'), ask: async () => ({ error: 'no table here' }) });
  failing.apply(); await tick(10);
  assert.equal(failing.error, 'no table here'); assert.equal(failing.wanted, false, 'an error drops the wanted Apply');
  // an op's answer brings a new state: written as that state
  const op = applier({ get: () => state, set: (s) => { state = s; }, commit: (e) => written.push(e[0].newText), ask: async (o) => ({ edits: [{ newText: o }], state: { w: 'S' } }) });
  op.change('siunitx'); op.apply(); await tick(10);
  assert.deepEqual([written.at(-1), state], ['siunitx', { w: 'S' }]);
  console.log('apply lifecycle: ok');
}

// the bibliography manager's model (bib.js): bare values, filters, what to fetch, an import's choices
{
  createRequire(import.meta.url)('./bib.js');
  const { hue, isBare, setField, matches, identify, chosen, joined } = self.satexBib;
  assert.equal(hue('article'), hue('article'), 'a type keeps its colour');
  assert.notEqual(hue('article'), hue('inproceedings'));
  assert.ok(isBare('1984', []) && isBare('jan', ['jan']) && isBare('cacm # " 1"', ['cacm']) && isBare('JAN', ['jan']));
  assert.ok(!isBare('Spring', ['jan']) && !isBare('"x"', []), 'a word no constant names is text');
  const node = { args: [{ text: 'k' }, { keys: [{ key: 'year', value: '1984' }, { key: 'month', value: 'jan' }], bare: ['year', 'month'] }] };
  setField(node, 'month', 'Spring', ['jan']);
  assert.deepEqual(node.args[1].bare, ['year'], 'a text value is braced');
  setField(node, 'month', 'feb', ['jan', 'feb']);
  assert.deepEqual(node.args[1].bare, ['year', 'month']);
  setField(node, 'year', '');
  assert.deepEqual(node.args[1].keys.map((k) => k.key), ['month'], 'an emptied field goes');
  setField(node, 'title', 'T');
  assert.deepEqual(node.args[1].keys.at(-1), { key: 'title', value: 'T' });
  const r = { key: 'knuth84', type: 'article', author: 'Knuth', year: '1984', title: 'Literate Programming', cited: 0, missing: [], duplicates: [], constants: [], undefined: [],
    node: { args: [{}, { keys: [{ key: 'doi', value: '10.1093/x' }] }] } };
  assert.ok(matches(r, 'knuth literate', []) && matches(r, '', ['type:article', 'uncited', 'doi']));
  assert.ok(!matches(r, '', ['type:book']) && !matches(r, '', ['cited']) && !matches(r, 'lamport', []));
  assert.ok(matches(r, '', ['type:book', 'type:article']), 'types: any of them');
  assert.deepEqual(identify('https://arxiv.org/abs/1706.03762v7'), { doi: '10.48550/arXiv.1706.03762', what: 'arXiv 1706.03762' });
  assert.equal(identify('doi:10.1145/3360588.').doi, '10.1145/3360588');
  assert.equal(identify('https://doi.org/10.1145/3360588').doi, '10.1145/3360588');
  assert.equal(identify('https://example.org/paper').url, 'https://example.org/paper');
  assert.equal(identify('not a reference'), null);
  // an update takes only the fields chosen; a new entry drops the unchosen and takes its key
  const rows = [
    { match: 'knuth', file: 'a.bib', node: { args: [{ text: 'knuth' }, { keys: [{ key: 'publisher', value: 'AW' }], bare: [] }] },
      fields: [{ name: 'isbn', value: '0-201', old: null }, { name: 'publisher', value: 'Addison', old: 'AW' }] },
    { match: null, file: 'b.bib', node: { args: [{ text: 'new' }, { keys: [{ key: 'title', value: 'T' }, { key: 'note', value: 'n' }], bare: [] }] },
      fields: [{ name: 'title', value: 'T' }, { name: 'note', value: 'n' }] },
  ];
  const out = chosen(rows, [{ isbn: true, publisher: false }, { title: true, note: false }], { 1: 'mine' });
  assert.deepEqual(out['a.bib'][0].args[1].keys, [{ key: 'publisher', value: 'AW' }, { key: 'isbn', value: '0-201' }], 'the local publisher stays');
  assert.deepEqual([out['b.bib'][0].args[0].text, out['b.bib'][0].args[1].keys], ['mine', [{ key: 'title', value: 'T' }]]);
  assert.deepEqual(joined([{ changes: { u: [1] } }, { changes: { u: [2], v: [3] } }]), { changes: { u: [1, 2], v: [3] } });
  // the field checks bibliography fields name (dialogs.js)
  const { check } = self.satexDialogs;
  for (const [v, rule] of [['Doe, J. and Roe, R.', 'names'], ['2020-05', 'bibdate'], ['2020/2021', 'bibdate'], ['97--111', 'pages'], ['https://x.org', 'url'], ['10.1145/3360588', 'doi'], ['0-201-13447-0', 'isbn'], ['2475-1421', 'issn'], ['jan', 'datepart']])
    assert.equal(check(v, [rule], []), '', `${rule}: ${v}`);
  for (const [v, rule] of [['Doe and', 'names'], ['2020-02-30', 'bibdate'], ['May 2020', 'bibdate'], ['a b', 'pages'], ['www.x.org', 'url'], ['https://doi.org/10.1/x', 'doi'], ['0-201-13447-1', 'isbn'], ['2475-1422', 'issn']])
    assert.notEqual(check(v, [rule], []), '', `${rule}: ${v}`);
  assert.match(check('nokey', ['bibkeyref'], ['knuth']), /no entry nokey/);
}

// widget fields (dialogs.js): one escaper for plain text, one finder of TeX slips with its fix, one set of checks
{
  const { texText, texIssue, texSafe, check } = self.satexDialogs;
  assert.equal(texText('50% & co #1 a_b ^ ~ {x} \\'), '50\\% \\& co \\#1 a\\_b \\textasciicircum{} \\textasciitilde{} \\{x\\} \\textbackslash{}');
  assert.equal(texIssue('$\\alpha_1$ and \\% \\& fine'), '', 'math and escaped specials are fine');
  for (const bad of ['50% off', 'a # b', 'R&D', 'costs $5', 'a_b', 'x^2', 'a } b']) assert.ok(texIssue(bad), bad);
  for (const [raw, safe] of [['50% off', '50\\% off'], ['R&D #1', 'R\\&D \\#1'], ['a_b $x_1$ x^2', 'a\\_b $x_1$ x\\^{}2'], ['costs $5', 'costs \\$5'], ['\\% kept', '\\% kept']]) {
    assert.equal(texSafe(raw), safe, raw);
    assert.equal(texIssue(texSafe(raw)), '', `${raw}: escaped, nothing left`);
  }
  assert.equal(check('', ['required', 'bibkey']), 'needed');
  assert.match(check('a b,c', ['required', 'bibkey']), /BibTeX key/);
  assert.match(check('knuth84', ['required', 'bibkey', 'unique'], ['knuth84']), /exists already/);
  assert.equal(check('knuth85', ['required', 'bibkey', 'unique'], ['knuth84']), '');
  assert.match(check('foo bar', ['key']), /no spaces/);
  for (const ok of ['2cm', '-.5em', '0.8\\linewidth', '\\textwidth', ' 3 pt ']) assert.equal(check(ok, ['length']), '', ok);
  for (const bad of ['banana', '-3', 'cm']) assert.ok(check(bad, ['length']), bad);
  assert.equal(check('-5', ['integer', 'min:0']), 'at least 0');
  assert.equal(check('3.7', ['integer']), 'a whole number');
  assert.equal(check('1e99', ['integer']), 'a whole number');
  assert.match(check('2020-02-30', ['date']), /no day/); assert.equal(check('2020-02-29T10:00', ['date']), ''); assert.equal(check('now', ['date']), '');
  console.log('field checks: ok');
}

// the managers' edit templates (views.js): an optional part goes with its empty value, a choice picks its edit
{
  const { fill, filled, chosen } = self.satexViews.templates;
  const index = '\\index{${?sort}${sort}@${/sort}${entry}${?format}|${format}${/format}}';
  assert.equal(fill(index, { entry: 'alpha', sort: '', format: '' }), '\\index{alpha}');
  assert.equal(fill(index, { entry: '$x$', sort: 'x', format: 'textbf' }), '\\index{x@$x$|textbf}');
  assert.equal(fill('${?a},\n  a = {${a}}${/a}${?b},\n  b = {${b}}${/b}', { a: ' ', b: '1' }), ',\n  b = {1}', 'blank is empty');
  assert.equal(fill('${?x}[${x}]${/x}{${k}}', { k: 'key' }), '{key}');
  const r = { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } };
  const form = { by: 'numbering', edits: { 'own counter': { changes: { u: [{ range: r, newText: '' }] } }, 'within section': { changes: { u: [{ range: r, newText: '[section]' }] } } } };
  assert.equal(chosen(form, { numbering: 'within section' }).changes.u[0].newText, '[section]');
  assert.deepEqual(filled(chosen({ edit: { changes: { u: [] } } }, {}), {}), { changes: { u: [] } });
  // a form of snapshots: each field's value where its fill puts it, an empty key dropped
  const node = { kind: 'call', name: 'newglossaryentry', args: [{ open: '{', text: '' }, { open: '{', keys: [], text: '' }] };
  const fills = [['key', 0, 0, null], ['name', 0, 1, 'name'], ['plural', 0, 1, 'plural'], ['value', 0, null, null]];
  const [n] = chosen({ by: 'k', prints: { a: { nodes: [node], fills } } }, { k: 'a', key: 'dag', name: 'DAG', plural: ' ', value: 'b' }).nodes;
  assert.deepEqual([n.args[0].text, n.args[1].keys, n.body, node.args[1].keys.length], ['dag', [{ key: 'name', value: 'DAG' }], 'b', 0]);
  const { check } = self.satexDialogs;
  assert.equal(check('alpha', ['index']), '');
  assert.match(check('a!b', ['index']), /makeindex/);
  assert.equal(check('a"!b', ['index']), '', 'a quoted ! is text');
  assert.equal(check('\\textbf{a|b}', ['index']), '', 'braces hide |');
  console.log('manager templates: ok');
}

// TeX lengths (lengths.js): any unit as typed, shown as written, one validator
{
  const L = self.satexLengths;
  assert.deepEqual([L.parts('1.5in'), L.parts(' -.5 em '), L.parts('0.5\\linewidth'), L.parts('-\\textwidth'), L.parts('3')], [
    { n: 1.5, unit: 'in' }, { n: -0.5, unit: 'em' }, { n: 0.5, unit: '\\linewidth' }, { n: -1, unit: '\\textwidth' }, { n: 3, unit: '' }]);
  for (const bad of ['banana', 'cm', '1.5.2cm', '2 cm cm']) assert.equal(L.parts(bad), null, bad);
  assert.equal(L.check('2cm') + L.check('12pt') + L.check('0.5\\linewidth') + L.check(''), '');
  assert.equal(L.check('-3'), L.MESSAGE, 'a bare number is no TeX length');
  const cm = 72.27 / 2.54 * 65536;
  assert.equal(L.sp('2cm'), Math.round(2 * cm));
  assert.equal(L.sp('1in'), Math.round(72.27 * 65536));
  assert.equal(L.sp('12pt'), 12 * 65536);
  assert.equal(L.sp('3', { dflt: 'cm' }), Math.round(3 * cm), 'a bare number takes the default unit');
  assert.equal(L.sp('0.5\\linewidth', { names: { linewidth: 1000 } }), 500);
  assert.equal(L.sp('0.5\\linewidth'), null, 'a command nothing resolves');
  assert.equal(L.sp('2cm', { per: { cm: 100 } }), 200, 'the run\'s table wins');
  assert.equal(L.show(Math.round(1.5 * 72.27 * 65536), 'in'), '1.5in');
  assert.equal(L.show(Math.round(2 * cm), 'mm'), '20mm');
  assert.ok(Math.abs(L.to('1in', 'cm') - 2.54) < 1e-9);
}
