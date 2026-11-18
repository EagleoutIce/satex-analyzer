// The forest dialog (views.ts `forest`): fixtures. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// forest trees: bracket → state → bracket, untouched text as it was
const F = self.satexViews.forest;
test("forest trees: bracket, state, bracket, the untouched text as it was", async () => {
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
// edits: rename, options, add, delete, move; the rest keeps its text, a changed child list fills its places in order
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
[VP [V$'$ [V, circle]  [{D, P}, edge={red, thick}]  [{new, one}]  [NP, name=dp]]
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
});
// The dialog's edits on `[A [B][B]]`, each written back as brackets: the untouched nodes keep their text.
// Pins the dead "Add child" (the button sat under the for-tree row) and the edits the buttons make.
test("The dialog's edits on `[A [B][B]]`, each written back as brackets", async () => {
  const at = (f) => { const t = F.parse('\n[A [B][B]]\n'); const [b1, b2] = t.root.children; f(t, t.root, b1, b2); F.settle(t); return F.emit(t); };
  assert.equal(at((t, a) => F.addChild(a)), '\n[A [B][B][new]]\n', 'add child');
  assert.equal(at((t, a, b1) => F.addChild(b1)), '\n[A [B [new]][B]]\n', 'add a child to a leaf');
  assert.equal(at((t, a, b1) => F.addSibling(t, b1)), '\n[A [B][new][B]]\n', 'add sibling after the node');
  assert.equal(at((t, a) => assert.equal(F.addSibling(t, a), null)), '\n[A [B][B]]\n', 'the root has no sibling');
  assert.equal(at((t, a, b1) => F.remove(t, b1)), '\n[A [B]]\n', 'delete');
  assert.equal(at((t, a, b1, b2) => { b2.content = 'C'; b2.dirty = true; F.shift(t, b2, -1); }), '\n[A [C][B]]\n', 'move up');
  assert.equal(at((t, a, b1, b2) => { b1.content = 'C'; b1.dirty = true; F.shift(t, b1, 1); }), '\n[A [B][C]]\n', 'move down');
  assert.equal(at((t, a, b1) => assert.equal(F.shift(t, b1, -1), null)), '\n[A [B][B]]\n', 'the first stays first');
  assert.equal(at((t, a, b1) => Object.assign(b1, { content: 'x, y', dirty: true })), '\n[A [{x, y}][B]]\n', 'content with a comma is braced');
  assert.equal(at((t, a, b1) => Object.assign(b1, { opts: F.splitTop('draw, fill={red}'), dirty: true })), '\n[A [B, draw, fill={red}][B]]\n', 'options');
  assert.equal(at((t, a, b1) => b1.children.push(...F.outlineIn('- C\n  - D'))), '\n[A [B [C [D]]][B]]\n', 'an outline under the selection');
  assert.equal(at((t) => { const [r] = F.outlineIn('- R\n  - S'); r.place = t.root.place; t.root = r; }), '\n[R [S]]\n', 'an outline replaces the tree');
});
// Typing into a forest node in the PDF: SyncTeX names the tree's `\end` line for every node, so a clicked word maps
// to the node contents in reading order (a level per row). Pins "forest node text cannot be typed" (TikZ could).
test("Typing into a forest node in the PDF", async () => {
  const PE = self.satexPdfEdit, doc = 'x\n\\begin{forest}\n[A [B, draw][{C, D} [B]] % [no]\n]\n\\end{forest}\n';
  const at = doc.indexOf('\\end{forest}'), got = PE.treeNodes(doc, at).map(([a, b]) => doc.slice(a, b));
  assert.deepEqual(got, ['A', 'B', 'C, D', 'B'], 'level by level, options and braces off, comments skipped');
  assert.deepEqual(PE.treeNodes(doc, doc.indexOf('[A')).map(([a, b]) => doc.slice(a, b)), got, 'from a line inside the tree too');
  assert.deepEqual(PE.treeNodes(doc + 'after\n', doc.length), [], 'after the tree: none');
  // the tree's `[` is no option of `\begin`: its node text may be typed
  for (const [a, b] of PE.treeNodes(doc, at)) if (!/,/.test(doc.slice(a, b))) assert.equal(PE.editable(doc, a, b, []), null, doc.slice(a, b));
});
// The feature-model preset: per node mandatory or optional and abstract, per group alternative or or, the
// constraints a node after the tree lists; it is on for a tree that uses its keys, and its styles compile.
test("The feature-model preset", async () => {
  const P = F.PRESETS['feature model'], t = F.parse('\n[A [B][B]]\n');
  assert.equal(F.presetOf(t), null, 'a plain tree');
  const [b1, b2] = t.root.children;
  F.setForTree(t, P.every);
  assert.equal(F.presetOf(t), 'feature model');
  F.setOpt('feature model', b1, 'kind', 'fm mandatory');
  F.setOpt('feature model', b2, 'kind', 'fm mandatory');
  F.setOpt('feature model', b2, 'kind', 'fm optional');
  F.setOpt('feature model', t.root, 'abstract', 'fm abstract');
  F.setOpt('feature model', t.root, 'group', 'fm alternative');
  assert.equal(F.optOf('feature model', b2, 'kind'), 'fm optional', 'one kind at a time');
  F.setConstraints(t, ['B requires C', ' ', 'C excludes D']);
  assert.deepEqual(F.constraintsOf(t), ['B requires C', 'C excludes D']);
  const out = F.emit(t);
  assert.equal(out, '\nfor tree={fm feature}\n[A, fm abstract, fm alternative [B, fm mandatory][B, fm optional]]\n  \\node[fm constraints] at ([yshift=-4mm]current bounding box.south) {B requires C \\\\ C excludes D};\n');
  const back = F.parse(out);
  assert.deepEqual(F.constraintsOf(back), ['B requires C', 'C excludes D'], 'the constraints read back');
  F.setConstraints(back, []);
  assert.equal(F.emit(back), '\nfor tree={fm feature}\n[A, fm abstract, fm alternative [B, fm mandatory][B, fm optional]]\n', 'no constraints, no node');
  // the template is a tree of the preset, and every key it writes has a style
  const tpl = F.parse(P.template), keys = new Set(P.styles.map(([k]) => k.replace(/^\/tikz\//, '').replace('/.style', '')));
  assert.equal(F.presetOf(tpl), 'feature model');
  for (const k of P.template.match(/fm [a-z]+/g)) assert.ok(keys.has(k), `${k} has a style`);
  // the example (examples/feature-model.tex, compiled with pdflatex) holds the styles as the preset writes them
  const ex = readFileSync(new URL('./examples/feature-model.tex', SITE), 'utf8');
  for (const [k, v] of P.styles) assert.ok(ex.includes(`${k}=${v}`), `the example has ${k}`);
  assert.equal(F.presetOf(F.parse(ex.slice(ex.indexOf('\\begin{forest}') + 14, ex.indexOf('\\end{forest}')))), 'feature model');
});
