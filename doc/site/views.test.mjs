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
console.log('ok views.test.mjs');
