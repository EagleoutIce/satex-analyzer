// The view models of views.js that need no page: node doc/site/views.test.mjs
import { load } from './load.mjs';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

globalThis.self = globalThis;
load('./lengths.js');
load('./snapshot.js');
load('./views.js');
load('./layers.js');
load('./pdfedit.js');
// Package loads: findings of a run that lacked a bundle are never shown. It lacked one never fetched, one still
// downloading (in the background too), or one landed only after it began; once a run saw them all, it is shown.
// Pins the cold-load bug: the first analysis showed `example-image-a` missing while mwe's bundle downloaded.
{
  const { unlanded, lacked } = self.satexViews.loads;
  const asked = new Map([['pdftex/mwe', 1], ['pdftex/tikz', 1], ['pdftex/amsmath', 1]]), inFlight = new Set(['pdftex/tikz']), background = new Set(), landedAt = new Map([['pdftex/amsmath', 1], ['pdftex/mwe', 2]]);
  const ids = ['pdftex/mwe', 'pdftex/tikz', 'pdftex/amsmath', 'pdftex/listings'];
  assert.deepEqual(unlanded(ids, asked, [inFlight, background]), ['pdftex/tikz', 'pdftex/listings'], 'downloading or never asked for');
  assert.deepEqual(lacked(ids, asked, [inFlight, background], landedAt, 1), ['pdftex/mwe', 'pdftex/tikz', 'pdftex/listings'], 'mwe landed after the run began');
  inFlight.delete('pdftex/tikz'); background.add('pdftex/tikz');
  assert.deepEqual(lacked(['pdftex/tikz'], asked, [inFlight, background], landedAt, 9), ['pdftex/tikz'], 'a background download counts too');
  background.delete('pdftex/tikz'); landedAt.set('pdftex/tikz', 3);
  // the check that runs once all landed sees them: it is shown, no endless rerun
  assert.deepEqual(lacked(ids.slice(0, 3), asked, [inFlight, background], landedAt, 3), []);
}
// Typeset rows: final rows win. A preview (a lax run's rows, page breaks marked maybe) of a text whose final rows
// are shown, or any older rows, never replaces them. Pins "page 2 → 3 maybe" and "page 1–4" on a one-page text.
{
  const { rowsWin } = self.satexViews;
  assert.equal(rowsWin(undefined, 3, true), true, 'nothing final yet: a preview shows');
  assert.equal(rowsWin(5, 5, true), false, 'a preview of the text already final');
  assert.equal(rowsWin(5, 4, true), false);
  assert.equal(rowsWin(5, 4, false), false, 'older final rows');
  assert.equal(rowsWin(5, 5, false), true);
  assert.equal(rowsWin(5, 6, true), true, 'a newer text\'s preview');
}
// Tips (tips.json): each stays true, its evidence still stands where it says.
{
  const tips = JSON.parse(readFileSync(new URL('./tips.json', import.meta.url), 'utf8'));
  assert.ok(tips.length >= 50, 'a collection of at least 50');
  assert.equal(new Set(tips.map((t) => t[0])).size, tips.length, 'no tip twice');
  assert.ok(tips.filter((t) => t[0].includes('satex gui')).length >= 3, 'a few name the local playground');
  for (const [text, file, evidence] of tips) {
    assert.ok(readFileSync(new URL(`../../${file}`, import.meta.url), 'utf8').includes(evidence), `${text}: no "${evidence}" in ${file}`);
    assert.ok(text.length <= 105 && !text.includes(';'), `${text}: short, no semicolon`);
    assert.ok(!/\bsatex\b(?![^`]*`)/.test(text.replace(/`[^`]*`/g, '')), `${text}: the text says SaTeX, not satex`);
  }
  // no repeat until the list is done, also across a reshuffle: a tip never follows itself
  let seed = 7;
  const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const bag = self.satexViews.tipBag(tips.map((t) => t[0]), random), shown = Array.from({ length: tips.length * 4 }, bag);
  for (let i = 0; i < 4; i++) assert.equal(new Set(shown.slice(i * tips.length, (i + 1) * tips.length)).size, tips.length, 'one round shows each once');
  assert.ok(shown.every((t, i) => t !== shown[i - 1]));
}

// Compile in the browser: a build's message names where it looked, and where else it can run.
{
  globalThis.location = { host: 'example.org' };
  load('./tex.js');
  const { advice, parseLog } = self.satexTex;
  const run = (over) => ({ asked: ['tex.example', 'example.org'], dead: [], failed: [], biblatex: false, ...over });
  const missing = "! LaTeX Error: File `x.sty' not found.";
  const asked = advice(missing, run(), 'pdftex');
  assert.ok(asked.includes('x.sty') && asked.includes('tex.example, this site') && asked.includes('satex-gui'), asked);
  const down = advice(missing, run({ dead: ['tex.example'], failed: ['x.sty'] }), 'pdftex');
  assert.ok(/No answer from tex\.example/.test(down) && down.includes('satex-gui'), down);
  const off = advice(missing, run({ asked: [] }), 'pdftex');
  assert.ok(/Downloads are off/.test(off) && off.includes('satex-gui'), off);
  assert.equal(advice('all fine', run(), 'pdftex'), '', 'a clean build says nothing');
  assert.ok(/pdfTeX only/.test(advice('', run(), 'luatex')));
  // a log's error lands on its project file, one from a TeX Live file on the main file
  const records = parseLog("(./sub.tex\n./sub.tex:3: Undefined control sequence.\n)\n/tmp/texlive_remote/26_x.sty:9: Oops\n", new Set(['main.tex', 'sub.tex']), 'main.tex');
  assert.deepEqual(records.map((r) => [r.path, r.line, r.severity]), [['sub.tex', 3, 'error'], ['main.tex', 9, 'error']]);
  delete globalThis.location;
}

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
load('./dialogs.js');
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
  try { load('./site.js'); } catch {}
  const { notation, typesetView, withKeys, keyTitle } = self.satexCards;
  const N = (s) => `${s}`, M = (m) => N(`\u0003${m}`);
  // booktabs: the rule after the last row borders that row across every column; a cell of leaders alone is a rule over that column
  const booktabs = notation(`${N('―')}A${M('cell')}1${M('row')}${M('cell')}${M('leaders')}${M('fill 0.0pt plus 1.0fill')}${M('row')}B${M('cell')}2${M('row')}${N('―')}${M('end')}`.replace(/^/, M('begin')));
  assert.match(booktabs, /<span class="ts-tr ts-ruled ts-rule-after"><\/span><\/span>$/);
  assert.match(booktabs, /<span class="ts-td"><\/span><span class="ts-td ts-rule" title="a rule over this column"><i class="ts-fill/);
  assert.equal(booktabs.match(/class="ts-td"/g).length, 5);
  // `|c|c|`: a `|` at a cell's start or end is that cell's edge, drawn over its full height so a column's rules line
  // up row over row; one inside a cell stays inline. A row of no line and no text (page boxes) is no row "0".
  // Pins the ruled tabular whose rules sat beside each cell's text and a bogus row numbered 0.
  {
    const F = M('fill 0.0pt plus 1.0fil');
    const ruled = notation(`${M('begin')}${N('―')}${N('│')}${F}a${F}${N('│')}${M('cell')}${F}b${N('│')}x${F}${N('│')}${M('row')}${N('│')}c${M('cell')}dd${N('│')}${M('row')}${N('―')}${M('end')}`);
    assert.equal(ruled.match(/ts-edge-l/g).length, 2, 'each row\'s leading |');
    assert.equal(ruled.match(/ts-edge-r/g).length, 3, 'a cell\'s trailing |, glue after it aside');
    assert.equal(ruled.match(/class="ts-vrule ts-cvr"/g).length, 1, 'the | inside `b|x` stays inline');
    assert.equal(notation(`${M('begin')}a${N('│')} ${N('│')}${M('cell')}b${M('end')}`).match(/ts-edge-r/g).length, 2, '`||`: both rules at the edge');
    assert.ok(!/<p [^>]*data-line="0"/.test(typesetView([{ kind: 'paragraph', line: 0, col: 0, detail: '', marked: '' }, { kind: 'paragraph', line: 3, detail: 'x', marked: 'x' }])), 'no row 0');
  }
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
  // an undecided choice reads inline with its tip, no chip: `is [not] a leap year` (an optional word, the space after
  // it outside), a test's choice too; a reference the next run decides keeps its chip. Pins the cryptic
  // `maybe \tleapy all|nothing|"not "` chip and `is ∅ | not a`.
  const choice = (on) => `\uE000\u0001choice\u0001${on}\uE001\uE000{\uE001`, arm = '\uE000|\uE001', shut = '\uE000}\uE001';
  const meant = typesetView([{ kind: 'paragraph', line: 3, detail: 'is a', marked: `is ${choice(8)}${arm}not ${shut}a`, causes: [{ on: '8', test: '', parted: '`\\tleapy`', line: 3, mark: '{|not }', arms: ['', 'not '] }] }]);
  assert.ok(!meant.includes('ts-switch'), 'no chip');
  assert.match(meant, /<span class="ts-choice" data-on="8" title="`\\tleapy` is empty or “not ” here: the run could not decide which \(line 3\), so both are kept"><span class="ts-arm ts-arm-true"><\/span><span class="ts-arm ts-arm-false">not<\/span><\/span> a/);
  const may = typesetView([{ kind: 'paragraph', line: 3, detail: 'x', marked: `${choice(7)}x${arm}${shut}`, causes: [{ on: '7', test: '\\ifdraft', line: 3, mark: '{x|}' }] }]);
  assert.match(may, /title="the text when \\ifdraft is true, or when it is false/);
  const ref = typesetView([{ kind: 'paragraph', line: 3, detail: 'x', marked: 'x', causes: [{ on: '9', test: '', parted: '\\r@fig', line: 3, mark: '{|}' }] }]);
  assert.match(ref, /<span class="ts-switch ts-ref ts-switch-reference" data-on="9"/);
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

// The editor's TeX tokenizer (site.js `siteTex`): a number with its unit is one token, sign and fraction included,
// and an expl3 name with its `_` and `:` is one command. Pins `- 0.05pt` whose `0` stayed plain beside a colored
// `.05pt`, and `\dim_eval:n` split into `\dim`, `_eval`, `:n`.
{
  document.documentElement.style ??= {};
  const cm = await import('./codemirror.js');
  self.siteCM = () => Promise.resolve(cm);
  const p = (await self.siteTex()).language.streamParser;
  const toks = (line) => {
    const st = p.startState(2), s = new cm.language.StringStream(line, 2, 2), out = [];
    while (!s.eol()) { const t = p.token(s, st); if (t) out.push(`${s.current()}:${t}`); s.start = s.pos; }
    return out;
  };
  assert.deepEqual(toks('1pt - 0.05pt + .5em'), ['1pt:number', '0.05pt:number', '.5em:number']);
  assert.deepEqual(toks('\\vskip -3.5pt plus 2fill minus 1truecm'), ['\\vskip:tag', '-3.5pt:number', '2fill:number', '1truecm:number']);
  assert.deepEqual(toks('{ \\dim_eval:n { \\l_tmpa_dim + 0.05pt } }').filter((t) => !t.endsWith(':bracket')), ['\\dim_eval:n:tag', '\\l_tmpa_dim:tag', '0.05pt:number']);
  // under \ExplSyntaxOn any name; elsewhere a math subscript stays apart
  assert.ok(toks('\\ExplSyntaxOn \\foo_x:').includes('\\foo_x::tag'));
  assert.deepEqual(toks('$\\alpha_i: x$').slice(0, 2), ['$:keyword', '\\alpha:tag']);
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
  load('./cli.js');
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
load('./layers.js');
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
  // a part of it: \newgeometry at the cursor, \restoregeometry before the section
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

// page numbering: a style picked after a matter mark is a new \pagenumbering there, a call as written
// changes its argument or goes, the chapter reset is a starred \counterwithin in the preamble
{
  const P = self.satexViews.page, at = (line) => ({ line, character: 0 });
  const call = { kind: 'call', name: 'pagenumbering', star: false, args: [{ open: '{', inner: null, text: 'alph' }], range: null, text: '\\pagenumbering{alph}' };
  const s = { numbering: { start: at(2), insert: at(1), chapters: true, per_chapter: false, reset: null, marks: [{ name: 'mainmatter', row: 5, at: at(6) }], calls: [{ row: 3, node: call }] } };
  const st0 = { numbers: { start: '', m0: '', c0: 'alph' }, perChapter: false };
  const nodes = P.numberNodes(s, st0, { numbers: { start: '', m0: 'Roman', c0: '' }, perChapter: true });
  assert.deepEqual(nodes.map((n) => [n.name, n.drop ? 'drop' : n.args.map((a) => a.text).join(','), n.star]), [['pagenumbering', 'drop', false], ['pagenumbering', 'Roman', false], ['counterwithin', 'page,chapter', true]]);
  assert.deepEqual(nodes[1].at, at(6), 'after the mark: \\mainmatter sets its own style first');
  assert.deepEqual(P.numberNodes(s, st0, st0), [], 'nothing changed, nothing written');
  console.log('page numbering: ok');
}

// heading styles: a format read as fields over its tokens, a change rewriting only its field's tokens;
// the edits land in the level's last call, else in new calls; hand-written text is kept or refused
{
  const P = self.satexViews.page, S = self.satexSnapshot;
  const at = (line) => ({ line, character: 0 });
  const h = {
    switches: { family: [['serif', '\\rmfamily'], ['sans', '\\sffamily'], ['mono', '\\ttfamily']], series: [['normal', '\\mdseries'], ['bold', '\\bfseries']], shape: [['upright', '\\upshape'], ['italic', '\\itshape'], ['small caps', '\\scshape']] },
    sizes: ['\\small', '\\normalsize', '\\large', '\\Large', '\\huge'], align: [['left', '\\filright', '\\raggedright'], ['center', '\\filcenter', '\\centering'], ['right', '\\filleft', '\\raggedleft']],
  };
  // read: the last of each field wins, \normalfont resets three, unknown tokens set nothing
  assert.deepEqual(P.fmtRead('\\normalfont \\Large \\bfseries ', h), { family: 'serif', series: 'bold', shape: 'upright', size: '\\Large' });
  assert.deepEqual(P.fmtRead('\\sffamily\\rmfamily\\color{red}\\centering\\size@x', h), { family: 'serif', color: 'red', align: 'center' });
  assert.deepEqual(P.fmtRead('\\color[HTML]{FF0000}', h), { color: '[HTML]{FF0000}' });
  // write: the field's own tokens go, \normalfont and unknown ones stay in place, the new token at the end
  assert.equal(P.fmtWrite('\\normalfont\\Large\\bfseries', h, { size: '\\huge', family: 'sans' }), '\\normalfont\\bfseries\\sffamily\\huge');
  assert.equal(P.fmtWrite('\\fontsize{20}{24}\\selectfont \\color{red}', h, { color: 'blue' }), '\\fontsize{20}{24}\\selectfont\\color{blue}');
  assert.equal(P.fmtWrite('', h, { color: '' }), '\\normalcolor');
  assert.equal(P.fmtWrite('\\normalfont \\Large \\bfseries ', h, { align: 'center' }), '\\normalfont\\Large\\bfseries\\filcenter', 'the run\'s spaces between commands dropped');
  // text in a format (titlesec's label-less heads, `\\bfseries Part`): the new commands go before it, never after
  assert.equal(P.fmtWrite('\\bfseries text', h, { size: '\\small' }), '\\bfseries\\small text');
  assert.equal(P.fmtWrite('\\bfseries Part \\Large', h, { size: '\\small', color: 'red' }), '\\bfseries\\small\\color{red}Part');
  assert.equal(P.fmtWrite('{\\bfseries}x', h, { family: 'sans' }), '{\\bfseries}\\sffamily x', 'a group is not text');
  // a value without a command in the run is refused, not dropped
  assert.equal(P.fmtWrite('\\bfseries', h, { family: 'fraktur' }), null);
  // refused: a parameter, unbalanced braces
  assert.equal(P.fmtWrite('#1\\bfseries', h, { series: 'normal' }), null);
  assert.equal(P.fmtWrite('{\\bfseries', h, { series: 'normal' }), null);
  // mutation: every token kept or rewritten, never another field's (each field changed alone)
  const base = '\\normalfont\\Large\\bfseries\\color{red}\\filcenter\\hspace{1em}';
  for (const [f, v] of [['family', 'mono'], ['series', 'normal'], ['shape', 'small caps'], ['size', '\\small'], ['color', 'blue'], ['align', 'right']]) {
    const out = P.fmtWrite(base, h, { [f]: v }), was = P.fmtRead(base, h), now = P.fmtRead(out, h);
    assert.deepEqual({ ...now, [f]: was[f] }, was, `${f}: only it changes`);
    assert.equal(now[f], v);
    assert.ok(out.includes('\\hspace{1em}'), 'the unknown token stays');
  }
  const call = (name, args, line) => ({ kind: 'call', name, star: false, range: { start: at(line), end: at(line + 1) }, text: 'x', line: false, drop: false, args: args.map((t) => ({ open: '{', inner: [0, 0], text: t })) });
  const s = {
    headings: { ...h, scheme: 'titlesec', insert: at(3), secnumdepth: { value: 3, call: null },
      levels: [{ name: 'section', level: 1, font: '\\normalfont\\Large\\bfseries', call: null }, { name: 'subsection', level: 2, font: '\\itshape', call: call('titleformat', ['\\subsection', '\\itshape'], 2) },
        { name: 'subsubsection', level: 3, font: 'x', call: call('titleformat', ['\\subsubsection', '#1'], 1) }] },
    pagecolor: { call: null, colors: [{ name: 'yellow', css: '#ffff00' }] },
  };
  const shown = (n) => n.drop ? `-${n.name}` : `${n.name}${n.star ? '*' : ''}${n.args.map((a) => `{${a.text}}`).join('')}@${(n.at ?? n.range.start).line}`;
  assert.deepEqual(P.styleNodes(s, {}), [], 'nothing changed, nothing sent');
  assert.deepEqual(P.styleNodes(s, { levels: { section: { series: 'bold', size: '\\Large' } } }), [], 'the run\'s values: nothing sent');
  // a new \titleformat* keeps the class's format, the existing call is edited in place, the hand-written one refused
  const ch = { levels: { section: { family: 'sans' }, subsection: { shape: 'small caps' }, subsubsection: { family: 'sans' } }, depth: 1, pagecolor: 'yellow' };
  assert.deepEqual(P.styleNodes(s, ch).map(shown), ['titleformat*{\\section}{\\normalfont\\Large\\bfseries\\sffamily}@3', 'titleformat{\\subsection}{\\scshape}@2', 'setcounter{secnumdepth}{1}@3', 'pagecolor{yellow}@3']);
  // a style set: what the levels show, written back as the same edits; switching back to the run's sends nothing
  const set = P.styleSet(s, ch);
  assert.deepEqual([set.levels.section.family, set.levels.subsubsection, set.depth, set.pagecolor], ['sans', undefined, 1, 'yellow']);
  assert.deepEqual(P.styleNodes(s, P.styleEdits(s, set).ch).map(shown), P.styleNodes(s, ch).map(shown));
  assert.deepEqual(P.styleNodes(s, P.styleEdits(s, P.styleSet(s, {})).ch), [], 'the set as written sends nothing');
  // a set from another document: what this one cannot take is listed, the rest applies
  const foreign = { levels: { chapter: { family: 'sans' }, section: { family: 'fraktur', size: '\\small' } }, depth: 7, pagecolor: 'brand' };
  const fe = P.styleEdits(s, foreign);
  assert.deepEqual(fe.skipped, ['chapter', 'section family fraktur', 'numbering 7', 'page color brand']);
  assert.deepEqual(P.styleNodes(s, fe.ch).map(shown), ['titleformat*{\\section}{\\normalfont\\bfseries\\small}@3']);
  // a call-less level whose run font does not read as commands is not offered: no null text in a snapshot
  const odd = { ...s, headings: { ...s.headings, levels: [{ name: 'section', level: 1, font: '#1\\bfseries', call: null }] } };
  assert.deepEqual(P.styleNodes(odd, { levels: { section: { family: 'sans' } } }), []);
  assert.deepEqual(P.styleSet(odd, {}).levels, {});
  // KOMA: new \addtokomafont with only the change, alignment in the shared \raggedsection, page color dropped
  const k = { headings: { ...h, scheme: 'koma', insert: at(3), align: [['left', '\\raggedright', '\\filright'], ['center', '\\centering', '\\filcenter']], secnumdepth: { value: 2, call: call('setcounter', ['secnumdepth', '\\subsectionnumdepth'], 1) },
    ragged: { raggedsection: { text: '\\protect \\raggedright ', call: null } },
    levels: [{ name: 'section', level: 1, font: '\\normalcolor\\sffamily\\bfseries', call: null, ragged: 'raggedsection' }, { name: 'subsection', level: 2, font: '\\sffamily', call: null, ragged: 'raggedsection' }] },
    pagecolor: { call: call('pagecolor', ['yellow'], 2), colors: [] } };
  assert.deepEqual(P.styleNodes(k, { levels: { section: { family: 'serif', align: 'center' } }, ragged: { raggedsection: 'center' }, depth: 1, pagecolor: '' }).map(shown),
    ['addtokomafont{section}{\\rmfamily}@3', 'renewcommand*{\\raggedsection}{\\centering}@3', 'setcounter{secnumdepth}{1}@1', '-pagecolor']);
  const ks = P.styleEdits(k, { levels: { section: { align: 'center' }, subsection: { align: 'left' } } });
  assert.deepEqual([ks.ch.ragged, ks.skipped], [{ raggedsection: 'center' }, ['subsection align (\\raggedsection follows section)']], 'a shared alignment: the first level\'s, the other reported');
  // typearea to geometry: one convention, the left margin from the paper's edge with the binding in it, geometry's inner past it
  {
    const cm = 72.27 / 2.54 * 65536, len = (sp) => `${+(sp / cm).toFixed(2)}cm`, same = (v) => [v, v];
    const g = { run: { paperwidth: same(Math.round(21 * cm)), paperheight: same(Math.round(29.7 * cm)), textwidth: same(Math.round(15 * cm)), textheight: same(Math.round(25 * cm)),
      oddsidemargin: same(Math.round(3 * cm) - 4736286), topmargin: same(0), headheight: same(0), headsep: same(0) }, twoside: false, papers: [], koma: null, binding: Math.round(cm),
      geometry: { loaded: true, global: { kind: 'call', name: 'geometry', args: [{ open: '{', keys: [] }], range: null, text: '' } }, header: {} };
    const g0 = P.pageOf(g), keys = P.pageNodes(g, g0, { ...structuredClone(g0), left: Math.round(4 * cm) }, len)[0].args[0].keys;
    assert.deepEqual(keys.filter((k) => ['left', 'bindingoffset'].includes(k.key)).map((k) => `${k.key}=${k.value}`), ['left=3cm', 'bindingoffset=1cm'], 'left 4cm with 1cm binding');
  }
  console.log('heading styles: ok');
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
  load('./bib.js');
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

// The filters every manager shares (views.js `passes`): chips of one group are alternatives, groups narrow, words narrow.
// Intuition: the glossary's "acronym" and "entry" chips are two answers to one question, so picking both shows both; "used" is another
// question and narrows. Before, the bibliography alone had this rule inline and the glossary and index had no chips at all.
{
  const { passes, hue } = self.satexViews;
  const rows = [{ t: 'acronym', used: true, s: 'cpu central processing unit' }, { t: 'entry', used: false, s: 'tex the typesetting system' }, { t: 'symbol', used: true, s: 'pi 3.14' }];
  const chip = (id, group, test) => ({ id, group, test });
  const chips = [chip('t:acronym', 'type', (r) => r.t === 'acronym'), chip('t:entry', 'type', (r) => r.t === 'entry'), chip('used', undefined, (r) => r.used)];
  const shown = (q, on) => rows.filter((r) => passes(r, q, chips, new Set(on), r.s)).map((r) => r.t);
  assert.deepEqual(shown('', []), ['acronym', 'entry', 'symbol'], 'no chip, no word: every row');
  assert.deepEqual(shown('', ['t:acronym', 't:entry']), ['acronym', 'entry'], 'two chips of a group are alternatives');
  assert.deepEqual(shown('', ['t:acronym', 't:entry', 'used']), ['acronym'], 'another group narrows');
  assert.deepEqual(shown('typesetting system', ['t:entry']), ['entry'], 'every word must occur');
  assert.deepEqual(shown('nothing', []), []);
  assert.equal(hue('Article'), hue('article'), 'a kind keeps its colour in every manager, whatever its case');
}

// Bug pinned: a later edit dropped the history renderer, so the definition history dialog came up empty.

// a definition history's event as text: the augment's added text, the copy, the call holding an older text
{
  const { historyEvent } = self.satexViews.history;
  const e = historyEvent({ by: '\\appto', file: 'main.tex', path: 'main.tex', line: 4, state: 'replaced', owner: 'document', change: { kind: 'appends', added: ' world', removed: '' } });
  assert.deepEqual([e.by, e.at, e.change, e.when], ['\\appto', 'main.tex:4', 'appends ` world`', '']);
  assert.equal(historyEvent({ change: { kind: 'replaces', added: '\\oldgreet !', removed: 'Hello world' } }).change, 'replaces `Hello world` by `\\oldgreet !`');
  const copy = historyEvent({ by: '\\let', copies: { name: '\\greet', file: 'main.tex', line: 4, by: '\\appto' }, calls: [{ name: '\\oldgreet', holds: '\\greet', at: 'main.tex:4' }], when: ['\\ifnum true at main.tex:10'] });
  assert.deepEqual([copy.copies, copy.calls, copy.when], [{ name: '\\greet', at: 'main.tex:4', by: '\\appto' }, [{ name: '\\oldgreet', note: 'holds \\greet of main.tex:4' }], '\\ifnum true at main.tex:10']);
}
assert.equal(typeof self.satexViews.RENDER.history, 'function', 'a history section has a renderer');

// Editing in the PDF (pdfedit.ts): which SyncTeX boxes a column handle sits on, what a drag writes, where typing may go.
{
  const E = self.satexPdfEdit, BP = 72 / 72.27, cm = (n) => n * 72.27 / 2.54 * BP;
  // a `p` cell is a vbox as wide as its column, on the cell's line; the table's own box stands on its `\end` line
  const cell = (line, x, w, y = 100) => ({ page: 1, file: 'a.tex', line, kind: '[', x, y, w, h: 6, d: 2 });
  const outer = { env: 'tabular', lines: [1, 5], width: null, cols: [{ align: 'l', width: '' }, { align: 'p', width: '3.26cm', node: {} }, { align: 'p', width: '2cm', node: {} }, { align: 'p', width: '\\dimexpr 0.2\\linewidth', node: {} }] };
  // the nested table set over its own lines inside the second row's last cell
  const inner = { env: 'tabular', lines: [3, 4], width: null, cols: [{ align: 'p', width: '1cm', node: {} }] };
  const boxes = [
    // two rows, the second cell set over two lines (a multiline cell has one box)
    cell(2, 80, cm(3.26)), cell(2, 80 + cm(3.26) + 12, cm(2)), cell(2, 260, 40),
    cell(3, 80, cm(3.26), 120), cell(3, 80 + cm(3.26) + 12, cm(2), 120), cell(3, 80 + cm(3.26) + 18, cm(1), 120), cell(3, 260, 40, 120),
    { page: 1, file: 'a.tex', line: 4, kind: '(', x: 80 + cm(3.26) + 16, y: 120, w: cm(1) + 4, h: 6, d: 2 },
    { page: 1, file: 'a.tex', line: 5, kind: '(', x: 60, y: 100, w: 300, h: 8, d: 30 },
    // a paragraph's box above the table, recorded at its first line, is no column
    { page: 1, file: 'a.tex', line: 1, kind: '[', x: 60, y: 80, w: 100, h: 6, d: 2 }, { page: 1, file: 'a.tex', line: 2, kind: '[', x: 60, y: 80, w: 100, h: 6, d: 2 },
    // another file's box at the same line is not the table's
    { ...cell(2, 80, cm(3.26)), file: 'b.tex' },
  ];
  const placed = E.place(boxes, 'a.tex', outer, [outer, inner]);
  assert.deepEqual(placed.map((c) => [c.k, Math.round(c.x), c.n]), [[1, 80, 2], [2, Math.round(80 + cm(3.26) + 12), 2], [3, 260, 2]], 'each width column on its cells, the nested 1cm column not taken for the 2cm one');
  assert.equal(E.place(boxes, 'a.tex', inner, [outer, inner])[0]?.k, 0, 'the inner table finds its own column on the shared line');
  assert.equal(E.extent(boxes, 'a.tex', outer).w, 300, "the table's box on its \\end line");
  assert.deepEqual(E.place([], 'a.tex', outer, [outer]), [], 'no boxes, no handles');
  assert.deepEqual(E.place(boxes.filter((r) => r.line !== 5), 'a.tex', outer, [outer, inner]), [], "no table box, no handles (not every box)");
  // a nested table on the outer row's own line: its cells are inside its box, not the outer's columns
  const flat = { env: 'tabular', lines: [3, 3], width: null, cols: [{ align: 'p', width: '2cm', node: {} }] };
  const fb = [...boxes, { page: 1, file: 'a.tex', line: 3, kind: '(', x: 80 + cm(3.26) + 12, y: 120, w: cm(2), h: 7, d: 3 }, cell(3, 80 + cm(3.26) + 14, cm(2) - 6, 120)];
  assert.ok(E.place(fb, 'a.tex', outer, [outer, flat]).every((c) => Math.abs(c.w - (cm(2) - 6)) > 1), 'the nested cell is no outer column');
  // the source rewritten before the next build: the column is still found, by its place among the others
  const edited = { ...outer, cols: outer.cols.map((c, k) => k === 1 ? { ...c, width: '5cm' } : c) };
  assert.deepEqual(E.place(boxes, 'a.tex', edited, [edited, inner]).map((c) => c.k), [1, 2, 3]);
  // an X column: computed, found between its neighbours, never the whole table
  const x = { env: 'tabularx', lines: [1, 3], width: {}, cols: [{ align: 'p', width: '1cm', node: {} }, { align: 'X', width: '' }] };
  const xb = [cell(2, 50, cm(1)), cell(2, 50 + cm(1) + 12, 300), { ...cell(3, 40, 400), kind: '[' }];
  assert.deepEqual(E.place(xb, 'a.tex', x, [x]).map((c) => [c.k, Math.round(c.w)]), [[0, Math.round(cm(1))], [1, 300]]);
  // what a drag writes: the unit and form kept, snapped, Shift finer; an expression has no form to keep
  assert.equal(E.resized('3.26cm', 100, 120, false), '3.9cm');
  assert.equal(E.resized('3.26cm', 100, 120, true), '3.91cm');
  assert.equal(E.resized('30mm', 100, 50, false), '15mm');
  assert.equal(E.resized('2in', 100, 1, false), '0.05in', 'never below one step');
  assert.equal(E.resized('.3\\linewidth', 100, 150, false), '.45\\linewidth', 'a share keeps its leading dot');
  assert.equal(E.resized('\\linewidth', 100, 90, false), '0.9\\linewidth');
  assert.equal(E.resized('0.5\\textwidth', 100, 200, false), '1\\textwidth');
  for (const t of ['\\dimexpr 0.3\\linewidth-2\\tabcolsep', '3cm plus 1fil', '', '\\mylen', '2\\linewidth+1pt']) assert.equal(E.resized(t, 100, 120, false), null, t);
  assert.equal(E.resized('3cm', 0, 120, false), null, 'nothing measured, nothing written');
  // where typing may go: plain text only, not math, keys or environment names; the line's ends too
  const line = 'Text $x+y$ and \\label{sec:a} \\begin{center} more, words';
  assert.equal(E.editable(line, 0, 4, []), null);
  assert.equal(E.editable(line, line.length, line.length, []), null, 'at the line end');
  assert.equal(E.editable(line, 0, 0, []), null, 'at the line start');
  assert.match(E.editable(line, 6, 7, []), /math/);
  assert.match(E.editable(line, 22, 25, [[22, 27]]), /key/);
  assert.match(E.editable(line, line.indexOf('center'), line.indexOf('center') + 6, []), /environment/);
  assert.match(E.editable(line, 14, 20, []), /markup/);
  assert.equal(E.editable('\\textbf{bold} x', 8, 12, []), null, "a style command's argument is text");
  assert.equal(E.editable('a \\$5 and more', 10, 14, []), null, 'an escaped dollar is no math');
  assert.ok(E.plain('Hello, world!') && !['a\\b', '{', '$', '&', '#', '^', '_', '~', '%', 'a\nb'].some(E.plain));
  // math and names are read from the file's start, not the caret's line: earlier lines, \( \), \[ \], math environments
  const doc = 'One $a\nb$ two \\(c\nd\\) three\n\\[ e \\] four\n\\begin{align*}\nf\n\\end{align*}\ngive % $ no math\nh \\begin{\ncenter} i';
  const at = (w) => doc.indexOf(w);
  for (const w of ['b$', 'd\\)', ' e ', 'f\n']) assert.match(E.editable(doc, at(w), at(w), []) ?? '', /math/, w);
  for (const w of ['One', 'two', 'three', 'four', 'give', 'h ', ' i']) assert.equal(E.editable(doc, at(w), at(w), []), null, w);
  assert.match(E.editable(doc, at('center'), at('center') + 2, []), /environment/, 'a name on the next line');
  assert.match(E.editable(doc, at('two'), at('two') + 3, [[at('two'), at('two') + 3]]), /key/);
  assert.match(E.editable(doc, -1, 0, []), /outside/);
  // a write that fails is never retried in a loop; while one is under way the newest value waits and wins
  {
    const sent = [];
    let ok = false;
    const q = E.sender(async (v) => { sent.push(v); await null; return ok; });
    q.put(1); q.put(2); q.put(3);
    await q.flush();
    assert.deepEqual(sent, [1, 3], 'a failed send stops; flush sends the newest once');
    ok = true; sent.length = 0;
    q.put(4); q.put(5); q.put(6);
    await q.flush();
    assert.deepEqual(sent, [4, 6], 'after a success the newest goes');
  }
  // typing in the output writes what the output then shows: specials escaped in text, `$` opens math with its closer
  // (the convention's: LaTeX's `\(` by default, `$`, or `auto`, the document's most used), in math characters as typed,
  // a closer typed where it stands steps over it. Pins plain-only typing, which refused `_` and could not start a
  // formula, and `$` writing `$…$` where the user asked for `\(…\)`.
  {
    const type = (text, at, keys, inline) => { let t = text, f = null; for (const c of keys) { const [a, b, body, step] = E.typed(t, at, c, inline, false, f); f = E.grow(f, a, b, body.length); t = t.slice(0, a) + body + t.slice(b); at = a + step; } return [t, at]; };
    // the caret ends before the text's `b`, or inside the pair just opened
    const ends = (want, at = want.lastIndexOf('b')) => [want, at];
    assert.deepEqual(type('ab', 1, '_%&#{}~^'), ends('a\\_\\%\\&\\#\\{\\}\\textasciitilde{}\\textasciicircum{}b'));
    assert.deepEqual(type('ab', 1, '$x_1$ y'), ends('a\\(x_1\\) yb'), 'default: `$` types LaTeX\'s `\\(`, `$` steps over `\\)`');
    assert.deepEqual(type('ab', 1, '$x_1$ y', '$'), ends('a$x_1$ yb'), 'math typed raw, the closer stepped over');
    const opened = '\\(p\\) a\\(\\)b';
    assert.deepEqual(type('\\(p\\) ab', '\\(p\\) a'.length, '$', 'auto'), ends(opened, opened.lastIndexOf('\\)')), 'auto: the document uses \\(');
    assert.deepEqual(type('$p$ ab', '$p$ a'.length, '$', 'auto'), ends('$p$ a$ $b', '$p$ a$'.length), 'auto: the document uses $, written `$ $` (`$$` is display math)');
    assert.deepEqual(type('ab', 1, '${x', '$'), ends('a${x}$b', 'a${x'.length), 'a brace in math gets its closer');
    // smart scripts: `_`/`^` open a group, a space, an operator or the formula's end leave it; off, typed raw.
    // Pins `x_42` typed in the output writing `x_42` (a subscript 4, then 2).
    const smart = (text, at, keys) => { let t = text; for (const c of keys) { const [a, b, body, step] = E.typed(t, at, c, '$', true); t = t.slice(0, a) + body + t.slice(b); at = a + step; } return [t, at]; };
    assert.deepEqual(smart('ab', 1, '$x_42$'), ends('a$x_{42}$b'), 'the formula\'s end leaves the group, then the formula');
    assert.deepEqual(smart('ab', 1, '$x^2+y_i j'), ends('a$x^{2}+y_{i}j$b', 'a$x^{2}+y_{i}j'.length), 'an operator and a space leave it');
    assert.deepEqual(smart('a$x_{4}$b', 'a$x_{4'.length, '2'), ['a$x_{42}$b', 'a$x_{42'.length], 'moved back in, typing goes on in the group');
    assert.deepEqual(type('ab', 1, '$x_42$', '$'), ends('a$x_42$b'), 'off: as typed');
    // a brace after a command's name is its argument: pins `\emph` from the completion list, whose `{` came out `\{`
    assert.deepEqual(type('ab', 1, '\\emph{x}'), ends('a\\emph{x}b'), 'the argument opened and left');
    assert.deepEqual(type('a\\TeX b', 'a\\TeX'.length, 's{'), ['a\\TeX{}s\\{ b', 'a\\TeX{}s\\{'.length], 'an older command keeps its name: `{}` ends it');
    assert.deepEqual(type('a\\[x\\]b', 'a\\[x'.length, '$'), ['a\\[x\\]b', 'a\\[x\\]'.length], '$ steps over a display closer');
    // Backspace takes an escape or an empty pair whole, Delete the escape ahead
    assert.deepEqual(E.erased('a\\_b', 3, true), [1, 3]);
    assert.deepEqual(E.erased('a\\textbackslash{}b', 17, true), [1, 17]);
    assert.deepEqual(E.erased('a$ $b', 2, true), [1, 4]);
    assert.deepEqual(E.erased('a$x$b', 3, true), [1, 4], "a formula's only character takes the formula: `$$` would open display math");
    assert.deepEqual(E.erased('a\\%b', 1, false), [1, 3]);
    assert.deepEqual(E.erased('abc', 2, true), [1, 2]);
  }
  // every column of a table on the page: an `l` or `r` column has no box, so it shares the room its placed neighbours
  // leave (approximate). Pins the layer that showed only width columns.
  {
    const own = { page: 1, file: 'b.tex', line: 9, kind: '(', x: 100, y: 200, w: 300, h: 10, d: 20 };
    const t = { env: 'tabular', lines: [5, 9], width: null, cols: [{ align: 'l', width: '' }, { align: 'p', width: '2cm', node: {} }, { align: 'r', width: '' }, { align: 'c', width: '' }] };
    const p = { page: 1, file: 'b.tex', line: 6, kind: '[', x: 150, y: 200, w: cm(2), h: 6, d: 2 };
    const got = E.columns([own, p], 'b.tex', t, [t]).map((c) => [c.k, +c.x.toFixed(1), +c.w.toFixed(1), !!c.approx]);
    const after = 150 + cm(2), rest = (400 - after) / 2;
    assert.deepEqual(got, [[0, 100, 50, true], [1, 150, +cm(2).toFixed(1), false], [2, +after.toFixed(1), +rest.toFixed(1), true], [3, +(after + rest).toFixed(1), +rest.toFixed(1), true]]);
  }
  // words selected on a page: the nearest word to a point (its row first), one band per row, the words as read.
  // Pins the drag selection whose copy fell back to the page's words.
  {
    const ws = [[1, 10, 0, 30, 10, 'The'], [1, 34, 0, 60, 10, 'loss'], [1, 10, 14, 40, 24, 'falls.']];
    assert.equal(E.nearest(ws, 45, 5), 1);
    assert.equal(E.nearest(ws, 45, 20), 2, 'a row first: the word below, not the nearer edge across');
    assert.deepEqual(E.bands(ws).map((b) => [b.l, b.r, b.d]), [[10, 60, 10], [10, 40, 24]]);
    assert.equal(E.joined(ws), 'The loss\nfalls.');
  }
  // a handle's key steps one snap step in the column's unit, never below one step
  assert.equal(E.stepped('3.26cm', 1, false), '3.4cm');
  assert.equal(E.stepped('3cm', -1, true), '2.99cm');
  assert.equal(E.stepped('0.1cm', -1, false), '0.1cm');
  assert.equal(E.stepped('.3\\linewidth', 1, false), '.31\\linewidth');
  assert.equal(E.stepped('\\dimexpr 1cm', 1, false), null);
  // the room of a table: the narrowest line box around it (a minipage's line), none without one
  const tb = { page: 1, file: 'a.tex', line: 5, kind: '(', x: 60, y: 100, w: 300, h: 8, d: 30 };
  assert.equal(E.room([tb, { ...tb, line: 6, w: 320, x: 50, h: 10, d: 32 }, { ...tb, line: 7, w: 400, x: 40, h: 12, d: 34 }], 'a.tex', outer), 320);
  assert.equal(E.room([tb], 'a.tex', outer), null);
  // the occurrences of a page's word in its line: whole words, punctuation left out, none after a backslash
  assert.deepEqual(E.spans('the cat, the \\the end', 'the'), [[0, 3], [9, 12]]);
  assert.deepEqual(E.spans('cat, catalog', 'cat,'), [[0, 3]]);
  assert.deepEqual(E.spans('x', '—'), []);
}
// Type mode keeps the source's structure: a property test over the site's examples and synthetic documents. From
// thousands of carets where a page word can put one (plain text or math), random keys (letters, space, Backspace,
// Delete, Enter, `{`, `}`, `$`, `\`, `_`, `^`, `%`, `&`) go through E.key, the one function the playground uses;
// after each edit an independent reader checks: braces balanced, no group closed by typing, every command name,
// environment, comment and math pair as before (commands typed since the caret was put may change), no new paragraph
// inside a group, no new script outside math, no new `&` but by Enter. Pins `\section{… an }` left by typing into a
// section title (a paragraph break inside the argument), a `$` inside `\(…\)`, `\` before a word renaming it.
{
  const E = self.satexPdfEdit;
  const dir = new URL('./examples/', import.meta.url);
  const docs = readdirSync(dir).filter((f) => f.endsWith('.tex')).map((f) => readFileSync(new URL(f, dir), 'utf8'));
  docs.push(String.raw`\documentclass{article}
\newcommand{\greet}[1]{Hello, #1!}
\begin{document}
\section[Short]{Long title with \emph{words}}\label{sec:a}
Text with a footnote\footnote{Some \textbf{\emph{nested} words} here.} and \TeX{} words, \LaTeX\ too.
\caption[Short cap]{A long caption, see Section~\ref{sec:a}.}
\begin{tabular}{l|c}
\hline
one & two \\
\multicolumn{2}{c}{three words} \\
four & $x^2$ \\
\end{tabular}
Math $a+b=c$ and \(x_i\) and \[\int_0^1 f\] and
\begin{align*}
  y &= mx + b \\
  z &= 2
\end{align*}
\begin{itemize}
  \item[label] first item
  \item second {\bfseries item}
\end{itemize}
\verb|a{b| and \lstinline{c_d} and
\begin{verbatim}
raw { text % here
\end{verbatim}
\tikz \node[draw] (a) at (0,0) {node text};
\greet{World} % a comment
\end{document}
`);
  // the independent reader: braces, commands, environments, comments, math, blank lines in groups, scripts, tabs
  const verbs = (t) => E.verbatim(t);
  const read = (t, mark = Infinity, end = t.length) => {
    const r = { depth: 0, low: 0, after: Infinity, cs: [], env: [], com: [], math: '', bad: 0, parIn: 0, script: 0, amp: 0, opens: [] };
    const vs = verbs(t);
    let k = 0;
    for (let i = 0; i < end; i++) {
      while (vs[k] && vs[k].to < i) k++;
      if (vs[k] && vs[k].from <= i) { i = vs[k].to - 1; k++; continue; }
      const c = t[i];
      if (c === '%') { const e = t.indexOf('\n', i); r.com.push(t.slice(i, e < 0 ? t.length : e)); i = e < 0 ? t.length : e - 1; continue; }
      if (c === '\\') {
        const m = /^\\([A-Za-z@]+|[\s\S]?)/.exec(t.slice(i, i + 64)), n = m[1];
        if (/^[A-Za-z@]/.test(n)) {
          r.cs.push([i, n]);
          const env = (n === 'begin' || n === 'end') && /^\{([^{}]*)\}/.exec(t.slice(i + m[0].length));
          if (env) r.env.push(`${n}:${env[1]}`);
          if (env && /^(align|equation)\*?$/.test(env[1])) r.math = n === 'begin' ? (r.math ? (r.bad++, r.math) : env[1]) : (r.math === env[1] ? '' : (r.bad++, r.math));
        } else if (n === '(' || n === '[') { if (r.math) r.bad++; r.math = n; } else if (n === ')' || n === ']') { if (r.math !== (n === ')' ? '(' : '[')) r.bad++; r.math = ''; }
        i += m[0].length - 1;
        continue;
      }
      if (c === '$') { const two = t[i + 1] === '$' && (r.math === '$$' || !r.math), d = two ? '$$' : '$'; if (!r.math) r.math = d; else if (r.math === d) r.math = ''; else r.bad++; i += d.length - 1; continue; }
      if (c === '{') { r.depth++; r.opens.push(i); }
      else if (c === '}') { r.depth--; r.opens.pop(); r.low = Math.min(r.low, r.depth); if (i >= mark) r.after = Math.min(r.after, r.depth); }
      else if (c === '\n' && /^\n[ \t]*\n/.test(t.slice(i, i + 64)) && r.depth > 0) r.parIn++;
      else if ((c === '_' || c === '^') && !r.math) r.script++;
      else if (c === '&') r.amp++;
    }
    if (r.math) r.bad++;
    return r;
  };
  // a page word's caret: in or at the edge of a letter run not after a backslash, where Type mode puts one
  let seed = 0x5a7e;
  const rand = (n) => { seed = (seed * 1103515245 + 12345) >>> 0; return (seed >>> 8) % n; };
  const KEYS = [...'abcdefghijklmnopqrstuvwxyz', ' ', ' ', 'Backspace', 'Backspace', 'Delete', 'Enter', 'Shift+Enter', '{', '}', '$', '\\', '\\', '_', '^', '%', '&'];
  const ESCW = /^(textbackslash|textasciitilde|textasciicircum)$/;
  let edits = 0, refused = 0;
  const fails = [];
  for (const doc of docs) {
    const base = read(doc), carets = [];
    for (const m of doc.matchAll(/(?<![\\A-Za-z@])\p{L}+/gu)) for (let p = m.index; p <= m.index + m[0].length; p++) if (/^$|math/.test(E.editable(doc, p, p, []) ?? '')) carets.push(p);
    for (let run = 0; run < 2400 / docs.length * 4 && carets.length; run++) {
      let t = doc, at = carets[rand(carets.length)], f = null, fresh = new Array(t.length).fill(false);
      for (let n = 1 + rand(8); n--;) {
        const k = KEYS[rand(KEYS.length)], e = E.key(t, at, k, { fresh: f, inline: rand(2) ? '\\(' : '$', smart: !!rand(2) });
        if (typeof e === 'string') { refused++; continue; }
        const [a, b, body, step] = e, was = t;
        t = t.slice(0, a) + body + t.slice(b);
        fresh = [...fresh.slice(0, a), ...Array(body.length).fill(true), ...fresh.slice(b)];
        f = E.grow(f, a, b, body.length);
        at = a + step;
        edits++;
        const r = read(t), why = [];
        if (r.depth !== base.depth || r.low < base.low) why.push('braces');
        const own = (cs) => cs.filter(([i, nm]) => !ESCW.test(nm) && !fresh.slice(i, i + nm.length + 1).every(Boolean)).map(([, nm]) => nm).join(' ');
        const own0 = base.cs.filter(([, nm]) => !ESCW.test(nm)).map(([, nm]) => nm).join(' ');
        if (own(r.cs) !== own0) why.push('commands');
        if (r.env.join() !== base.env.join()) why.push('environments');
        if (r.com.join('\n') !== base.com.join('\n')) why.push('comments');
        if (r.bad > base.bad) why.push('math');
        if (r.parIn > base.parIn) why.push('paragraph in a group');
        if (r.script > base.script) why.push('script outside math');
        if (r.amp > read(was).amp && k !== 'Enter') why.push('alignment tab');
        // what was typed closes no group it did not open
        if (body && read(t, a, a + body.length).after < read(was, Infinity, a).depth) why.push('typed text left its group');
        if (why.length) { fails.push({ why, k, at: a, before: was.slice(Math.max(0, a - 30), b + 30), after: t.slice(Math.max(0, a - 30), a + body.length + 30) }); t = was; at = Math.min(at, t.length); break; }
      }
    }
  }
  assert.ok(edits > 2000, `the fuzz made ${edits} edits`);
  assert.deepEqual(fails.slice(0, 3), [], `${fails.length} Type-mode edits broke the source's structure`);
}
// Type mode's rules at the spots that broke: Enter in a section title (the bug: `\section{… an }` and a paragraph
// break inside the argument, a runaway), `\` before a word (it renamed it `\Background`), `$` inside `\(…\)`, Backspace
// at a group's edge, a comment's line end.
{
  const E = self.satexPdfEdit;
  const t = '\\section{Background}\\label{sec:b}\nText % note\nmore \\(x\\) end.';
  const at = (s, d = 0) => t.indexOf(s) + d;
  assert.match(E.key(t, at('ground'), 'Enter'), /paragraph cannot end inside \\section/, 'Enter in a title: refused');
  assert.deepEqual(E.key(t, at('ground'), 'Shift+Enter'), [at('ground'), at('ground'), '\n', 1], 'Shift+Enter: a line break, a space');
  assert.deepEqual(E.key(t, at('Back'), '\\'), [at('Back'), at('Back'), '\\ ', 1], '`\\` keeps off the word after it');
  assert.equal(typeof E.key(t, at('Back'), 'Backspace'), 'string', "Backspace at a group's start keeps the brace");
  assert.equal(typeof E.key(t, at('note'), 'a'), 'string', 'a comment is no text');
  assert.equal(typeof E.key(t, at('note', 4), 'Delete'), 'string', "a comment's line end stays");
  assert.deepEqual(E.key(t, at('x\\)', 1), '$'), [at('x\\)', 1), at('x\\)', 1), '', 2], '`$` in \\(…\\) steps over its closer');
  assert.deepEqual(E.key(t, at('x\\)'), '$'), [at('x\\)'), at('x\\)'), '', 3], '`$` elsewhere in it leaves the formula');
  assert.deepEqual(E.key(t, at('x\\)'), '%'), [at('x\\)'), at('x\\)'), '\\%', 2], '`%` in math is escaped');
  assert.equal(typeof E.key(t, at('sec:b', 2), 'a'), 'string', 'a label is a key');
  // a contents entry is written where its short title is, else its title; a starred section, a float-less caption none
  const texts = { 'a.tex': '\\section[Short]{Long}\\input{b}\\section*{No}\\caption{Lone}', 'b.tex': '\\begin{figure}\\caption[F]{Fig}\\end{figure}\\subsection{Sub}\n\\verb|\\section{v}|' };
  const got = (k) => E.entries(texts, 'a.tex', k).map(([f, a, b]) => `${f}:${texts[f].slice(a, b)}`);
  assert.deepEqual(got('toc'), ['a.tex:Short', 'b.tex:Sub']);
  assert.deepEqual(got('lof'), ['b.tex:F']);
  assert.deepEqual(got('lot'), []);
  assert.equal(E.key(texts['a.tex'], 'section[Sh'.length + 1, 'x')[2], 'x', 'a short title takes typing');
}
// Typing in a typeset view: the caret stays after each typed character. The view finds its word with wordAt in the
// text as now written; pins the bug where the caret jumped back to the word's start after each key ("lol" -> "lool").
{
  const E = self.satexPdfEdit;
  let text = '\\greet{lol}', at = text.indexOf('lol') + 1, typed = 'l';
  for (const c of 'ooo') {
    const [a, b, body, step] = E.typed(text, at, c);
    text = text.slice(0, a) + body + text.slice(b);
    at = a + step;
    typed += c;
    const [start, word] = E.wordAt(text, at);
    assert.equal(word.slice(0, at - start), typed, 'caret after the typed character');
  }
  assert.equal(text, '\\greet{looool}');
  assert.deepEqual(E.wordAt('a bc', 4), [2, 'bc'], 'the caret at a word end is in it');
  assert.equal(E.wordAt('a  b', 2), null, 'between words none');
}
// "Type in the argument instead": the first argument of a use of the macro, on its line or a neighbour, never the
// definition's `\greet}`.
{
  const E = self.satexPdfEdit, text = '\\newcommand{\\greet}[1]{Hello, #1!}\n\\greet{world}\n\\greeting{x}\n';
  assert.equal(E.argAt(text, '\\greet', 2), text.indexOf('world'));
  assert.equal(E.argAt(text, '\\greet', 1), text.indexOf('world'), 'the next line');
  assert.equal(E.argAt(text, '\\greet', 3), text.indexOf('world'), 'not \\greeting');
  assert.equal(E.argAt(text, '\\nope', 2), null);
}
// The typed word drawn at once over the page: it takes its measured width at the page's scale, the words after it move.
{
  const E = self.satexPdfEdit, width = (s) => s.length;
  // two units per character as set: one more character, two units more
  assert.deepEqual(E.relaid([[0, 12, 'Hello,'], [14, 26, 'world!']], 0, 'Hellmo,', width), [[0, 14, 'Hellmo,'], [16, 28, 'world!']]);
  assert.deepEqual(E.relaid([[0, 10, 'ab'], [12, 20, 'cd']], 1, 'cdd', width), [[0, 10, 'ab'], [12, 24, 'cdd']]);
}
// The caret on a page stands between two characters, never inside a glyph: boundaries spaced by the font's widths
// (a wide `m` takes more than an `i`), a click going to the nearest one, and the page's pixels moving each to the
// gap between the glyphs. Pins the caret drawn at an even share of the word, in the middle of a glyph.
{
  const E = self.satexPdfEdit, width = (s) => [...s].reduce((n, c) => n + (c === 'm' ? 3 : 1), 0);
  assert.deepEqual(E.boundaries('mi', 0, 8, width), [0, 6, 8], 'the m three times as wide as the i');
  assert.deepEqual(E.boundaries('ab', 0, 4, () => 0), [0, 2, 4], 'nothing measured: even shares');
  assert.equal(E.boundaryAt([0, 6, 8], 5), 1, 'inside the m, nearer its end');
  assert.equal(E.boundaryAt([0, 6, 8], 7.5), 2);
  // columns: glyph, glyph, gap, gap, glyph; the estimate 2.2 goes to the gap's middle, 3
  const ink = [9, 9, 0, 0, 9];
  assert.equal(E.inkSnap(2.2, ink, 2), 3);
  assert.equal(E.inkSnap(1.4, ink, 0.5), 1.5, 'no gap in reach: the least ink nearest, a column\'s middle');
  assert.equal(E.inkSnap(9, ink, 1), 9, 'outside the word: as it was');
}
// A tint on the PDF marks the glyphs a run of the row set, found by their place in the row: the `!` of
// `\greet{…}` (`Hello, #1!`) is the one after the argument, not the document's own `!` after `you`. Math the page
// writes otherwise (`−` for `-`) stays outside the context. Pins the macro tint on the document's last `!`.
{
  const E = self.satexPdfEdit;
  const texts = ['Hello,', 'Walter!', 'x42', '+', '2', '−', '41Generally', 'I’d', 'say', 'you!'];
  const row = 'Hello,Walter!x42+2-41GenerallyI’dsayyou!', bang = row.indexOf('!');
  assert.deepEqual(E.locate(texts, row, bang, bang), [[1, 6, 7]], 'the macro\'s `!`, after Walter');
  assert.deepEqual(E.locate(texts, row, row.lastIndexOf('!'), row.lastIndexOf('!')), [[9, 3, 4]], 'the text\'s own `!`');
  assert.deepEqual(E.locate(texts, row, 0, bang), [[0, 0, 6], [1, 0, 7]], 'a run over two words');
  const g = row.indexOf('Generally');
  assert.deepEqual(E.locate(texts, row, g, g + 8), [[6, 2, 11]], 'after math the page spells otherwise');
  assert.equal(E.locate(['x!', 'x!'], 'x!', 1, 1), null, 'a row the page spells twice: no guess');
}
// The math control: one Insert menu per Type bar (inline, display and align*), inline in the convention's
// delimiters, nothing inside math. Pins the two `$x$`/`\[x\]` buttons inside the Type legend.
{
  const E = self.satexPdfEdit;
  assert.deepEqual(E.mathAt('ab', 1, 'inline'), [1, 1, '\\(\\)', 2], 'LaTeX\'s \\( by default');
  assert.deepEqual(E.mathAt('ab', 1, 'inline', '$'), [1, 1, '$ $', 1], '`$ $`: `$$` is display math');
  assert.deepEqual(E.mathAt('ab', 1, 'display'), [1, 1, '\\[\\]', 2]);
  assert.deepEqual(E.mathAt('ab', 1, 'align'), [1, 1, '\\begin{align*}\n\n\\end{align*}', '\\begin{align*}\n'.length]);
  assert.deepEqual(E.mathAt('a\\(x\\)b', 4, 'display'), [4, 4, '', 0], 'in math: nothing');
  const html = readFileSync(new URL('./play.html', import.meta.url), 'utf8');
  const groups = [...html.matchAll(/<span class="math-group"[^]*?<\/details><\/span>/g)].map((m) => m[0]);
  assert.equal(groups.length, 2, 'one in the Typeset bar, one in the PDF bar');
  for (const g of groups) assert.deepEqual([...g.matchAll(/data-math="(\w+)"/g)].map((m) => m[1]), ['inline', 'display', 'align'], 'each once, in the Insert menu');
  assert.ok(!/data-math="[$\\]/.test(html), 'the old $x$ and \\[x\\] buttons are gone');
}
// One origin tint per spot: an argument's word inside its macro's run keeps its tint, the run is cut around it
// (pins the double box "Helmlo [World]"); rows apart and other pages stay whole.
{
  const E = self.satexPdfEdit, box = (l, r, kind, page = 1, t = 0) => ({ page, l, r, t, d: t + 10, kind });
  const got = E.apart([box(0, 100, 'macro'), box(40, 70, 'arg'), box(0, 100, 'moved', 1, 20), box(0, 100, 'moved', 2)]);
  assert.deepEqual(got.map((b) => [b.kind, b.page, b.t, b.l, b.r]).sort(), [['arg', 1, 0, 40, 70], ['macro', 1, 0, 0, 40], ['macro', 1, 0, 70, 100], ['moved', 1, 20, 0, 100], ['moved', 2, 0, 0, 100]]);
}
// Circuits (circuitikz): a chain splits into parts, ground and gates are read, other statements stay code, and an
// untouched statement is written back as it was. Pins the circuit view's round trip.
{
  const C = self.satexViews.circuit, body = '\n  \\draw (0,0) to[R, l=$R_1$] (2,0) -- (2,2);\n  \\draw (0,0) node[ground] {};\n  \\node[and port] (G1) at (4,1) {};\n  \\fill (1,1) circle (2pt);\n';
  const st = C.parse(body), parts = st.stmts.map((s) => s.parts?.map((p) => [p.kind, p.label, p.name ?? null, JSON.stringify(p.a), JSON.stringify(p.b ?? null)]) ?? null);
  assert.deepEqual(parts, [[['R', '$R_1$', null, '[0,0]', '[2,0]'], ['short', '', null, '[2,0]', '[2,2]']], [['ground', '', null, '[0,0]', 'null']], [['and port', '', 'G1', '[4,1]', 'null']], null]);
  assert.equal(C.body(st), body, 'untouched: as written');
  st.stmts[0].parts[1] = { ...st.stmts[0].parts[1], b: 'G1.in 1' };
  st.stmts[0].dirty = true;
  assert.ok(C.body(st).includes('\\draw (0,0) to[R, l=$R_1$] (2,0);\n  \\draw (2,0) -- (G1.in 1);'), 'an edited chain: one statement per part');
  assert.equal(C.emit({ kind: 'C', a: [0, 0], b: [0, 1.5], label: 'a, b', opts: ['v=$U$'] }), '\\draw (0,0) to[C, l={a, b}, v=$U$] (0,1.5);');
}
// Column widths by drag (pdfedit.ts): an `l`/`c`/`r` column becomes a `p` keeping its alignment, a drag snaps to
// 0.5 mm or to a twentieth of `\linewidth` when near one, and an `l` column's width is its cells' ink on the page.
// Pins "≈1.6cm" for every column of an l/c/r table and a drag that could not widen a column without a width.
{
  const E = self.satexPdfEdit, MM = 72 / 25.4, line = 400;
  assert.deepEqual(E.respec('l', '3cm'), { body: 'p{3cm}', array: false });
  assert.deepEqual(E.respec('c', '3cm'), { body: '>{\\centering\\arraybackslash}p{3cm}', array: true }, 'c keeps its centering');
  assert.deepEqual(E.respec('r', '2cm'), { body: '>{\\raggedleft\\arraybackslash}p{2cm}', array: true });
  assert.deepEqual(E.respec('m', '2cm'), { body: 'm{2cm}', array: true }, 'm needs array');
  assert.equal(E.respec('X', '2cm'), null, 'tabularx computes X');
  // 0.5 mm steps in cm, a share near a twentieth of the line, a share kept a share, mm kept mm, Shift finer
  assert.equal(E.snapWidth(null, 40, 30.2 * MM, line, false), '3cm', '30.2 mm: 30 mm');
  assert.equal(E.snapWidth(null, 40, 30.3 * MM, line, false), '3.05cm');
  assert.equal(E.snapWidth(null, 40, 0.25 * line + 1, line, false), '.25\\linewidth', 'near a quarter of the line');
  assert.equal(E.snapWidth(null, 40, line, line, false), '\\linewidth');
  assert.equal(E.snapWidth(null, 40, 0.25 * line + 1, line, true), `${+((0.25 * line + 1) / MM / 10).toFixed(2)}cm`, 'Shift: no share snap');
  assert.equal(E.snapWidth('20mm', 20 * MM, 25.2 * MM, line, false), '25mm');
  assert.equal(E.snapWidth('.3\\linewidth', 0.3 * line, 0.4 * line, line, false), '.4\\linewidth');
  assert.equal(E.snapWidth('\\dimexpr 2cm', 40, 50, line, false), null, 'a form not read is not rewritten');
  assert.equal(Math.round(E.widthBp('.5\\linewidth', line)), Math.round(line / 2));
  // two rows of three left cells: each column's ink; a \multicolumn row (two runs) tells nothing
  const box = { page: 1, file: 'a.tex', line: 9, kind: '(', x: 100, y: 200, w: 200, h: 40, d: 2 };
  const w = (x, r, y, t = 'a') => [1, x, y - 7, r, y, t];
  const words = [w(106, 130, 170), w(150, 170, 170), w(172, 190, 170), w(220, 240, 170), w(106, 120, 185), w(150, 160, 185), w(220, 260, 185), w(106, 180, 198), w(220, 230, 198)];
  assert.deepEqual(E.measure(words, box, 3), [[106, 130], [150, 190], [220, 260]], 'a word space joins, a cell gap parts');
  const table = { env: 'tabular', lines: [1, 9], width: null, cols: [{ align: 'l', width: '' }, { align: 'c', width: '' }, { align: 'r', width: '' }] };
  const cols = E.columns([box], 'a.tex', table, [table], words);
  assert.deepEqual(cols.map((c) => [c.x, c.w, !!c.measured, !!c.approx]), [[106, 24, true, false], [150, 40, true, false], [220, 40, true, false]]);
  assert.ok(E.columns([box], 'a.tex', table, [table]).every((c) => c.approx), 'without words: shared evenly, said approximate');
}
// Typing in verbatim (pdfedit.ts): listings, verbatim, minted, \verb and \lstinline take every character as it is,
// no escape, no math, no group; a `$` inside a listing opens no math for the text after it. Pins `\` typed into a
// listing written as \textbackslash{} and the text after a `$` in a \verb read as math.
{
  const E = self.satexPdfEdit;
  const text = 'Pay \\verb|$x_1| now.\n\\begin{lstlisting}[language=C]\nint a = b_c{};\n\\end{lstlisting}\n\\begin{minted}{python}\nx = "$"\n\\end{minted}\nAfter \\lstinline{f(x)} and 50 more.\n';
  const at = (s, k = 0) => text.indexOf(s) + k;
  assert.deepEqual(E.verbatim(text).map((v) => text.slice(v.from, v.to)), ['$x_1', '\nint a = b_c{};\n', '\nx = "$"\n', 'f(x)']);
  for (const c of ['\\', '{', '_', '$', '%', '^']) assert.deepEqual(E.typed(text, at('b_c'), c, '\\(', true), [at('b_c'), at('b_c'), c, 1], `${c} in a listing: as typed`);
  assert.deepEqual(E.typed(text, at('$x_1', 1), '|'), [at('$x_1', 1), at('$x_1', 1), '', 0], "\\verb's closer is never typed");
  assert.deepEqual(E.typed(text, at('f(x)', 1), '_'), [at('f(x)', 1), at('f(x)', 1), '_', 1]);
  assert.equal(E.editable(text, at('b_c'), at('b_c', 3), []), null, 'markup characters in a listing are text');
  assert.equal(E.editable(text, at('50'), at('50', 2), []), null, 'a `$` in verbatim opens no math after it');
  assert.equal(E.editable(text, at('now'), at('now', 3), []), null);
  assert.deepEqual(E.erased(text, at('{};', 1), true), [at('{};'), at('{};', 1)], 'one character, never an empty pair');
  assert.deepEqual(E.typed(text, at('now'), '_'), [at('now'), at('now'), '\\_', 2], 'outside: escaped as before');
  assert.equal(E.verbatimAt('% \\verb|x|\nplain', 9), null, 'a comment holds no verbatim');
}
// Typing in a table (pdfedit.ts): Tab and Shift+Tab step from cell to cell, row over row, past rules; Enter adds a
// row after the caret's, as many cells as it has. Pins Enter writing a blank line (a paragraph) inside a tabular.
{
  const E = self.satexPdfEdit;
  const text = 'x\n\\begin{tabular}{l|c}\n\\toprule\nqq & cd \\\\\n\\midrule\nef & {g & h} \\\\\n\\bottomrule\n\\end{tabular}\n';
  const at = (s, k = 0) => text.indexOf(s) + k;
  const t = E.cells(text, at('cd'));
  assert.deepEqual(t.rows.map((r) => r.map(([a, b]) => text.slice(a, b))), [['qq', 'cd'], ['ef', '{g & h}']], 'rules and a group\'s & are no cells');
  assert.deepEqual([t.r, t.c], [0, 1]);
  assert.equal(E.cellStep(text, at('qq', 1), 1), at('cd', 2), 'Tab: the end of the next cell');
  assert.equal(E.cellStep(text, at('cd', 1), 1), at('ef', 2), 'row over row, past \\midrule');
  assert.equal(E.cellStep(text, at('ef'), -1), at('cd', 2), 'Shift+Tab');
  assert.equal(E.cellStep(text, at('{g', 1), 1), null, 'past the last cell');
  assert.equal(E.cellStep('no table', 2, 1), null);
  const [a, b, body, step] = E.rowAfter(text, at('ef'));
  assert.equal(text.slice(0, a) + body + text.slice(b), text.replace('{g & h} \\\\\n', '{g & h} \\\\\n & \\\\\n'), 'a new row after the caret\'s');
  assert.equal(step, 1, 'the caret in its first cell');
  const open = '\\begin{tabular}{ll}\na & b\n\\end{tabular}', r = E.rowAfter(open, open.indexOf('& b') + 2);
  assert.equal(open.slice(0, r[0]) + r[2] + open.slice(r[1]), '\\begin{tabular}{ll}\na & b \\\\\n &\n\\end{tabular}', 'a last row without \\\\ gets one');
}
// Typing in the PDF: the source words of a paragraph by offset, matched in order to the page's words. The caret and
// the echo are placed by these offsets, never by a word's spelling alone (`lorem … lorem`), so a rebuilt page that
// moved the caret's line to the next page still puts the caret after the same character.
{
  const E = self.satexPdfEdit, text = 'A \\emph{big} cat~sat, 50\\% % note\n{} $x$ end';
  assert.deepEqual(E.tokens(text, 0, text.length).map((t) => [text.slice(t.s, t.e), t.t]), [['A', 'A'], ['\\emph{big}', 'big'], ['cat', 'cat'], ['sat,', 'sat,'], ['50\\%', '50%'], ['$x$', 'x'], ['end', 'end']]);
  assert.deepEqual(E.aligned(['a', 'b', 'c', 'd'], ['a', 'c', 'd']), [0, -1, 1, 2], 'a source word the page lacks');
  assert.deepEqual(E.aligned(['a', 'b', 'c'], ['a', 'X', 'b', 'c']), [0, 2, 3], 'a page word the source lacks (a macro\'s text)');
  assert.deepEqual(E.aligned(['lorem', 'consectetur', 'lorem'], ['lorem', 'consec-', 'tetur', 'lorem']), [0, 1, 3], 'hyphenated across rows: its first part');
  assert.deepEqual(E.aligned(['ﬁne'], ['fine']), [0], 'a ligature as the page spells it');
  assert.deepEqual(E.changed('abcXdef', 'abcYYdef'), [3, 3]);
  assert.deepEqual(E.changed('aaa', 'aaaa'), [3, 0], 'the common end never overlaps the common start');
  const doc = 'one\n\ntwo three\nfour\n\nfive';
  assert.deepEqual(E.paragraph(doc, doc.indexOf('three')), [5, 19]);
  assert.deepEqual(E.paragraph(doc, 1), [0, 3]);
}
// TikZ nodes moved by drag (pdfedit.ts): `at (x,y)` keeps its units and digits on the 1 mm grid, a placed node gets
// shifts that add up and go at zero, Alt places it at a fixed point, a path label moves by `pos=`.
// Pins drags that wrote `at (1.7999999,2cm)` or dropped the `right=of` a node was placed by.
{
  const T = self.satexTikzMove, pic = { scale: 1, x: 1, y: 1 }, mv = (s, m, p = pic) => T.move(s, 0, p, m)?.text;
  assert.equal(mv('\\node (a) at (1.50,2cm) {x};', { by: [0.33, -0.21] }), '\\node (a) at (1.80,1.8cm) {x};', 'grid, digits as written');
  assert.equal(mv('\\node (a) at (10mm,2in) {x};', { by: [0.33, -0.21], free: true }), '\\node (a) at (13.3mm,1.917in) {x};', 'Shift: off the grid');
  assert.equal(mv('\\node (a) at (1,1) {x};', { by: [0.5, 0] }, { scale: 2, x: 1, y: 1 }), '\\node (a) at (1.3,1) {x};', 'a scaled picture moves by less');
  assert.equal(mv('\\node[box, right=of a] (b) {x};', { by: [0.3, -0.2] }), '\\node[box, right=of a, xshift=3mm, yshift=-2mm] (b) {x};');
  assert.equal(mv('\\node[right=of a, xshift=2mm] (b) {x};', { by: [-0.2, 0] }), '\\node[right=of a] (b) {x};', 'a shift moved to zero goes');
  assert.equal(mv('\\node[right=of a, xshift=1cm] (b) {x};', { by: [0.25, 0] }), '\\node[right=of a, xshift=1.3cm] (b) {x};', 'a shift keeps its unit');
  assert.equal(mv('\\node (b) {x};', { by: [0.1, 0.1] }), '\\node[xshift=1mm, yshift=1mm] (b) {x};', 'options added where there were none');
  assert.equal(mv('\\node[draw, right=of a, yshift=2mm] (b) {x};', { to: [2.04, -1.46] }), '\\node[draw] (b) at (2,-1.5) {x};', 'Alt: placed at a point');
  assert.equal(mv('\\draw (a) -- node[midway, above] {l} (b);', { pos: 0.304 }), '\\draw (a) -- node[above, pos=0.3] {l} (b);', 'a label along its path');
  const ns = T.nodes('\\begin{tikzpicture}[scale=2]\n\\node (a) at (0,0) {A}; % a; b\n\\node[right=of a] (b) {B};\n\\draw (a) -- node {m} (b) node[pos=.2] {n};\n\\foreach \\i in {1,2} \\node at (\\i,0) {\\i};\n\\end{tikzpicture}');
  assert.deepEqual(ns.map((n) => [n.kind, n.line, n.name, n.placed, n.plain, n.seg, n.pos]), [['node', 2, 'a', false, true, null, null], ['node', 3, 'b', true, false, null, null],
    ['label', 4, null, false, true, ['a', 'b'], 0.5], ['label', 4, null, false, true, ['a', 'b'], 0.2]], 'a loop is skipped, a comment is no statement');
  assert.equal(ns[0].pic.scale, 2);
  assert.equal(T.posOn([0, 0], [10, 0], [3, 5]), 0.3);
  assert.deepEqual(T.clipped([0, 0], [2, 1], [10, 0], [3, 1]), [[2, 0], [7, 0]], 'cut where it leaves the boxes');
}
// References switched from the pages (pdfedit.ts): the key written over is the one in the command's list, past its
// options; a page word is a reference's output when its line does not spell it; the list ranks by fuzzy match.
// Pins a switch that rewrote `\cite[p.~3]{a,b}`'s options or the other key.
{
  const R = self.satexRefPick, line = 'See~\\cite[p.~3]{knuth84, lamport94} and Section~\\ref{sec:intro}.';
  const at = (k, col) => R.keyAt(line, col, k), col = (s) => line.indexOf(s) + 1;
  assert.deepEqual(at('lamport94', col('\\cite')), [line.indexOf('lamport94'), line.indexOf('lamport94') + 9]);
  assert.deepEqual(at('knuth84', col('\\cite')), [line.indexOf('knuth84'), line.indexOf('knuth84') + 7]);
  assert.equal(at('sec:intro', col('\\cite')), null, 'a key of another command');
  assert.equal(R.unspelled(line, '[1,'), true);
  assert.equal(R.unspelled(line, '1.2.'), true);
  assert.equal(R.unspelled(line, 'Section'), false, 'written in the line');
  assert.equal(R.unspelled(line, '—.'), false, 'nothing but punctuation: no word');
  assert.equal(R.fuzzy('sint', 'xyz'), -1);
  assert.ok(R.fuzzy('sint', 'sec:intro') > R.fuzzy('sint', 'some:other:intent'), 'word starts and runs rank first');
}
