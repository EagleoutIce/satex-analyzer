// The forest dialog's model (views.ts `forest`): random trees and random edit runs against a plain reference model,
// written as brackets and read back. Pins edits whose brackets read back as another tree.
import assert from 'node:assert/strict';
import { test, rng } from './harness.mjs';

const F = self.satexViews.forest;
const W = ['A', 'B', 'NP', 'VP', 'x, y', '$x$', 'the cat', '{a}', 'V$\'$'];
/** A node's shape as text: content and children, `{…}` braces of a written comma off. */
const shape = (n) => `${n.content.replace(/^\{(.*)\}$/, '$1')}${n.children.length ? `(${n.children.map(shape).join(' ')})` : ''}`;
const nodes = (n) => [n, ...n.children.flatMap(nodes)];
/** A random tree text, as the dialog writes a fresh one. */
function randomTree(R, size = 1 + R.int(10)) {
  const t = F.parse(`\n${R.pick(['', 'for tree={draw},\n'])}[R]\n${R.pick(['', '\\draw (a) -- (b);\n'])}`), all = [t.root];
  for (let k = 0; k < size; k++) { const c = F.addChild(R.pick(all)); c.content = R.pick(W); all.push(c); }
  F.settle(t);
  return F.emit(t);
}

// Intuition: an untouched tree is written back byte for byte, comments, spacing and options as they were.
test('round trip: untouched trees come back as written', () => {
  const R = rng(41);
  for (let k = 0; k < 200; k++) {
    const text = randomTree(R).replace(/\[([^[\]]*)\]/, (m, c) => R.chance(0.5) ? `[${c}, draw  , edge={red,thick}]` : m).replace('\n[', R.chance(0.5) ? '\n  % a [comment]\n[' : '\n[');
    const t = F.parse(text);
    assert.equal(F.emit(t), text);
    F.settle(t);
    assert.equal(F.emit(t), text, 'settle leaves an untouched tree alone');
  }
});

// Intuition: every edit the buttons make (add child, add sibling, delete, move up and down, drag before, after or under)
// does to the written tree what it does to a plain reference tree; the brackets then read back as that tree and as
// themselves. A node never moves into its own subtree; the root has no sibling and cannot go.
test('edit runs: the written tree is the reference tree', () => {
  const R = rng(42);
  let ops = 0;
  for (let k = 0; k < 220; k++) {
    const t = F.parse(randomTree(R));
    const ref = (n) => ({ c: n.content.replace(/^\{(.*)\}$/, '$1'), kids: n.children.map(ref) });
    let rt = ref(t.root);
    const pathOf = (n, at = t.root, p = []) => at === n ? p : at.children.map((c, i) => pathOf(n, c, [...p, i])).find(Boolean) ?? null;
    const get = (p, r = rt) => p.reduce((x, i) => x.kids[i], r);
    for (let j = 0; j < 1 + R.int(8); j++) {
      const all = nodes(t.root), n = R.pick(all), p = pathOf(n), op = R.int(6);
      if (op === 0) { F.addChild(n); get(p).kids.push({ c: 'new', kids: [] }); }
      else if (op === 1) { const got = F.addSibling(t, n); if (p.length) get(p.slice(0, -1)).kids.splice(p.at(-1) + 1, 0, { c: 'new', kids: [] }); else assert.equal(got, null); }
      else if (op === 2) { const got = F.remove(t, n); if (p.length) get(p.slice(0, -1)).kids.splice(p.at(-1), 1); else assert.equal(got, null); }
      else if (op === 3) {
        const by = R.pick([-1, 1]), got = F.shift(t, n, by), sib = p.length ? get(p.slice(0, -1)).kids : [], i = p.at(-1), o = i + by;
        if (p.length && o >= 0 && o < sib.length) { const [x] = sib.splice(i, 1); sib.splice(o, 0, x); assert.equal(got, n); } else assert.equal(got, null);
      } else {
        const target = R.pick(all), where = R.pick(['before', 'after', 'child']), tp = pathOf(target);
        const inside = tp.length >= p.length && p.every((v, i) => tp[i] === v);
        const ok = F.move(t, n, target, where);
        if (inside || !p.length || (where !== 'child' && !tp.length)) { assert.ok(!ok || (!inside && p.length), 'refused'); if (!ok) continue; }
        // the reference: take the node out, then put it beside or under the target where that stands then
        const x = get(p), tr = get(tp), parentOf = (r, at = rt) => at.kids.includes(r) ? at : at.kids.map((c) => parentOf(r, c)).find(Boolean);
        const from = parentOf(x).kids;
        from.splice(from.indexOf(x), 1);
        if (where === 'child') tr.kids.push(x); else { const to = parentOf(tr).kids; to.splice(to.indexOf(tr) + (where === 'after' ? 1 : 0), 0, x); }
      }
      ops++;
      F.settle(t);
      const text = F.emit(t), back = F.parse(text);
      const sh = (r) => `${r.c}${r.kids.length ? `(${r.kids.map(sh).join(' ')})` : ''}`;
      assert.equal(shape(back.root), sh(rt), text);
      assert.equal(F.emit(back), text, 'the written tree reads back as itself');
      rt = ref(back.root);
      // the model goes on from what it wrote, as the dialog does after Apply
      Object.assign(t, back);
    }
  }
  assert.ok(ops > 500, `${ops} edits`);
});

// Intuition: an outline exported and imported again is the same tree.
test('outline: export and import round-trip', () => {
  const R = rng(43);
  for (let k = 0; k < 100; k++) {
    const t = F.parse(randomTree(R)), [r] = F.outlineIn(F.outlineOut(t.root));
    assert.equal(shape(r), shape(t.root).replace(/\{|\}/g, (c) => c), F.outlineOut(t.root));
  }
});

// Intuition: the feature-model preset's choices are options of the nodes: set at random, written, read back, each
// node keeps exactly its choice per option set; constraints round-trip as a list. Pins a kind set twice
// (`fm mandatory, fm optional` both written).
test('feature model: choices and constraints round-trip', () => {
  const R = rng(44), P = F.PRESETS['feature model'], sets = { ...P.node, ...P.group };
  for (let k = 0; k < 100; k++) {
    const t = F.parse(randomTree(R));
    F.setForTree(t, P.every);
    const want = new Map();
    for (let j = 0; j < 10; j++) {
      const all = nodes(t.root), i = R.int(all.length), set = R.pick(Object.keys(sets)), key = R.pick(sets[set]);
      F.setOpt('feature model', all[i], set, key);
      want.set(`${i}:${set}`, key);
    }
    const cs = Array.from({ length: R.int(4) }, () => `${R.pick(['A', 'B'])} ${R.pick(['requires', 'excludes'])} ${R.pick(['C', 'D'])}`);
    F.setConstraints(t, cs);
    F.settle(t);
    const back = F.parse(F.emit(t)), all = nodes(back.root);
    assert.equal(F.presetOf(back), 'feature model');
    for (const [k, key] of want) { const [i, set] = k.split(':'); assert.equal(F.optOf('feature model', all[+i], set), key, `${k} in ${F.emit(t)}`); }
    for (const n of all) for (const set of Object.keys(sets)) assert.ok(n.opts.filter((o) => sets[set].includes(o)).length <= 1, 'one choice per set');
    assert.deepEqual(F.constraintsOf(back), cs);
  }
});

// Intuition: the drawn layout puts parents over the middle of their children and leaves apart.
test('layout: parents centred, leaves apart', () => {
  const R = rng(45);
  for (let k = 0; k < 100; k++) {
    const t = F.parse(randomTree(R)), out = F.layout(t.root), at = new Map(out.map((o) => [o.n, o]));
    for (const { n, x, y } of out) {
      if (n.children.length) assert.equal(x, (at.get(n.children[0]).x + at.get(n.children.at(-1)).x) / 2);
      for (const c of n.children) assert.equal(at.get(c).y, y - 1);
    }
    const leaves = out.filter((o) => !o.n.children.length).map((o) => o.x);
    assert.equal(new Set(leaves).size, leaves.length);
  }
});

// Intuition: a node dragged before or after the root is refused and stays where it was. Pins the node lost by that drag
// (it was taken out before the root's missing parent refused the move).
test('move: before or after the root refused, nothing lost', () => {
  const t = F.parse('\n[A [B][C]]\n'), [b] = t.root.children;
  assert.equal(F.move(t, b, t.root, 'before'), false);
  assert.equal(F.move(t, b, t.root, 'after'), false);
  F.settle(t);
  assert.equal(F.emit(t), '\n[A [B][C]]\n');
});

// Intuition: the edge editor's keys sit on the right node (`parent anchor` on the parent, `child anchor` and the label
// on the child) and read back as set; a cleared label leaves no key. Pins a label written where forest ignores it.
test('edges: anchors and labels on the right node, read back', () => {
  const t = F.parse('\n  [S [NP] [VP]]\n'), [np] = t.root.children;
  F.setKey(t.root, 'parent anchor', 'east');
  F.setKey(np, 'child anchor', 'north west');
  F.setEdgeLabel(np, 'subj', 'left');
  F.settle(t);
  const back = F.parse(F.emit(t)), [np2] = back.root.children;
  assert.equal(F.valOf(back.root, 'parent anchor'), 'east');
  assert.equal(F.valOf(np2, 'child anchor'), 'north west');
  assert.deepEqual(F.edgeLabel(np2), { text: 'subj', side: 'left' });
  F.setEdgeLabel(np2, '', 'left');
  assert.equal(F.valOf(np2, 'edge label'), null);
});

// Intuition: a line between two nodes names both (from their text, never a name taken) and is drawn after the tree,
// where forest runs TikZ code; removing it leaves the text after the tree as it was.
test('lines: named ends, drawn after the tree, removed cleanly', () => {
  const text = '\n  [S [NP, name=np] [S]]\n';
  const t = F.parse(text), [np, s2] = t.root.children;
  F.link(t, s2, np, ['s']);
  assert.equal(F.valOf(s2, 'name'), 's2');
  assert.deepEqual(F.linksOf(t).map((l) => [l.a, l.b, l.opts]), [['s2', 'np', '->']]);
  F.settle(t);
  assert.match(F.emit(t), /\]\n  \\draw\[->\] \(s2\) -- \(np\);\n$/);
  F.unlink(t, 0);
  assert.equal(t.post, '\n');
});

// Intuition: level styles and the strut key edit the preamble's key list in place: one `where level=N` per level,
// emptied keys drop it, the other keys stay. Pins a second `where level=1` added instead of the first changed.
test('preamble: level styles and for tree keys', () => {
  const t = F.parse('\n  for tree={draw}\n  [S [NP]]\n');
  F.setWhereLevel(t, 1, 'fill=red');
  F.setWhereLevel(t, 1, 'fill=blue');
  assert.equal(t.pre, '\n  for tree={draw}, where level=1{fill=blue}{}');
  F.forTreeToggle(t, 'execute at begin node=\\strut', true);
  assert.deepEqual(F.forTreeKeys(t), ['draw', 'execute at begin node=\\strut']);
  F.setWhereLevel(t, 1, '');
  assert.equal(t.pre, '\n  for tree={draw, execute at begin node=\\strut}');
  const bare = F.parse('\n  [S [NP]]\n');
  F.setWhereLevel(bare, 0, 'draw');
  assert.equal(F.emit(bare), '\n  where level=0{draw}{}\n  [S [NP]]\n');
});

// Intuition: the feature toggles flip a pair's style on the node (abstract ↔ concrete, mandatory ↔ optional, and → or →
// alternative) with the styles the run named, never two of one pair, and a role without a style is the bare default.
// Pins a toggle that adds `optional` beside `mandatory`, or offers a group toggle on a leaf.
test('feature toggles: one style per pair, the run\'s names', () => {
  const feats = { abstract: 'fm abstract', mandatory: 'fm mandatory', optional: 'optional', or: 'or', alternative: 'fm alternative' };
  const t = F.parse('\n  [Car, fm abstract [Body, fm mandatory] [Extras, draw [Radio] [Nav]]]\n'), [body, extras] = t.root.children;
  assert.deepEqual(F.featureToggles(t.root, undefined, feats).map((x) => x.pair), ['kind', 'group']);
  assert.deepEqual(F.featureToggles(body, t.root, feats).map((x) => x.pair), ['kind', 'edge']);
  // no concrete style: abstract toggles off and on
  assert.equal(F.nextFeature(t.root, feats, 'kind'), '');
  F.setFeature(t.root, feats, 'kind', '');
  assert.deepEqual(t.root.opts, []);
  F.setFeature(body, feats, 'edge', F.nextFeature(body, feats, 'edge'));
  assert.deepEqual(body.opts, ['optional']);
  const cycle = [];
  for (let k = 0; k < 3; k++) { F.setFeature(extras, feats, 'group', F.nextFeature(extras, feats, 'group')); cycle.push(F.featureOf(extras, feats, 'group')); }
  assert.deepEqual(cycle, ['or', 'alternative', '']);
  assert.deepEqual(extras.opts, ['draw']);
  assert.equal(F.featureMark('group', ''), '∧');
  F.settle(t);
  assert.match(F.emit(t), /\[Car \[Body, optional\] \[Extras, draw/);
  // a tree using none of the styles (a syntax tree in the same document) gets no toggles
  assert.equal(F.featuresIn(F.parse('\n  [S [NP] [VP]]\n'), feats), null);
  assert.equal(F.featuresIn(F.parse('\n  [Car [Body, fm mandatory]]\n'), feats), feats);
});
