// The view models of views.js that need no page: node doc/site/views.test.mjs
import { createRequire } from 'node:module';
import assert from 'node:assert/strict';

globalThis.self = globalThis;
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
  assert.equal(po.text(), 'node distance=2cm, every node/.style={draw}, scale=2');
  // scaled: the text's coordinates written back as they were
  const sc = D.parse('\n  \\node (a) at (1,2) {A};\n');
  Object.assign(sc.items[0], { x: 3, y: 4, dirty: true });
  assert.equal(D.emit(sc, 2), '\n  \\node (a) at (1.5,2) {A};\n');
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
  const { notation, typesetView } = self.satexCards;
  const N = (s) => `${s}`, M = (m) => N(`\u0003${m}`);
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
  // glue with its spec: infinite glue in a line still sets where the text sits, glue between lines stands apart
  const filled = notation(`${M('fill 0.0pt plus 1.0fill')}Right`);
  assert.match(filled, /^<i class="ts-fill ts-fill-l" title="infinite glue in the line: 0.0pt plus 1.0fill" data-glue="0.0pt plus 1.0fill"><\/i>Right$/);
  assert.match(notation(`A${M('hskip 2.0pt plus 3.0pt')}B`), /^A<i class="ts-glue ts-glue-hskip" title="glue in the line: 2.0pt plus 3.0pt"[^>]*><\/i>B$/);
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
