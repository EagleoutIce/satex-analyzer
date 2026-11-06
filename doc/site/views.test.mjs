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
