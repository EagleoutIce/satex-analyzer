// Page actions (actions.ts `satexPageActions`, pdfedit.ts `satexTikzMove`): the right-click menus of the PDF and the
// Typeset view. Each option group's rewrite reads back as what it set and parses again; every menu row of every plugin
// run on fixtures writes text that still parses with its nodes and paths. Seeded properties beside fixed cases.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test, rng, apply } from './harness.mjs';

const T = self.satexTikzMove, PA = self.satexPageActions;
const pic = { scale: 1, x: 1, y: 1, node: [] };
const wrap = (s) => `\\begin{tikzpicture}\n${s}\n\\end{tikzpicture}`;
/** Runs `fn` with a store in memory for the viewer's switches (node has no localStorage). */
const withStore = (fn) => () => { const was = self.satexStore, kept = new Map(); self.satexStore = { get: (k) => kept.get(k) ?? null, set: (k, v) => (v == null ? kept.delete(k) : kept.set(k, v)) }; try { return fn(); } finally { self.satexStore = was; } };
/** `text` with edits `{from, to, now}` applied (as the playground's one span). */
const edit = (text, edits) => edits.filter(Boolean).sort((a, b) => b.from - a.from).reduce((t, e) => apply(t, [e.from, e.to, e.now]), text);

// Intuition: every shape the menu offers is the node's shape once picked, its keys first, the shape before's keys gone.
// Pins a switch from a polygon that left `regular polygon sides=6` on a circle, and a shape TikZ reads as another.
test('shapes: each one reads back, its library named, its own keys go with it', () => {
  for (const start of ['\\node (a) {A};', '\\node[circle, red] (a) {A};', '\\node[regular polygon, regular polygon sides=6, draw] (a) {A};', '\\node[shape=star, star points=7] (a) {A};']) {
    for (const [to] of T.SHAPES) {
      const extra = to === 'regular polygon' ? ['regular polygon sides=3'] : [];
      const r = T.shape(start, 0, pic, to, extra), n = T.nodes(wrap(r.text))[0];
      assert.equal(T.shapeOf(n), to, `${start} -> ${to}: ${r.text}`);
      if (to !== 'regular polygon') assert.ok(!/sides|points/.test(r.text), `the old shape's keys gone: ${r.text}`);
      assert.equal(r.library, ['rectangle', 'rounded', 'circle'].includes(to) ? null : r.library, to);
      if (!['rectangle', 'rounded', 'circle'].includes(to)) assert.match(r.library, /^shapes\.(geometric|symbols|arrows|multipart)$/, to);
      assert.equal(n.text, 'A');
    }
  }
  assert.equal(T.shape('\\node (a) {A};', 0, pic, 'regular polygon', ['regular polygon sides=5']).text, '\\node[regular polygon, regular polygon sides=5] (a) {A};');
  assert.equal(T.library('cloud'), 'shapes.symbols');
  assert.equal(T.library('single arrow'), 'shapes.arrows');
  assert.equal(T.library('rectangle split'), 'shapes.multipart');
});

// Intuition: setting a group writes exactly its keys; set again it is the same text; set to nothing they go and the rest
// stays as written. Pins a width change that kept `thick` beside `line width=2pt`, a side that kept `auto`.
test('option groups: set, read back, idempotent, removed', () => {
  const R = rng(401), cases = [['draw', ['draw=red', 'draw=blue!30']], ['fill', ['fill=red!10']], ['text', ['text=blue']], ['font', ['font=\\small']], ['rotate', ['rotate=45']],
    ['opacity', ['opacity=0.5']], ['width', ['thick', 'ultra thin', 'line width=1.5pt']], ['corners', ['rounded corners=2pt', 'sharp corners']], ['dash', ['dashed', 'densely dotted']],
    ['inner sep', ['inner sep=4pt']], ['minimum width', ['minimum width=2cm']], ['minimum height', ['minimum height=1cm']]];
  const starts = ['\\node (a) {A};', '\\node[draw, thick, fill=green] (a) at (1,2) {A};', '\\node[box, right=of b, dashed, rotate=10] (a) {A};'];
  for (let k = 0; k < 200; k++) {
    const st = R.pick(starts), [g, vals] = R.pick(cases), v = R.pick(vals);
    const once = T.set(st, 0, g, [v]);
    assert.deepEqual(T.current(once, 0, g), [v], `${g}=${v} on ${st}: ${once}`);
    assert.equal(T.set(once, 0, g, [v]), once, 'idempotent');
    const gone = T.set(once, 0, g, []), rest = (s) => (T.keysAt(s, 0) ?? []).filter((x) => !T.current(s, 0, g).includes(x));
    assert.deepEqual(T.current(gone, 0, g), []);
    assert.deepEqual(rest(gone), rest(st), 'the other keys as written');
    const n = T.nodes(wrap(once))[0];
    assert.equal(n.text, 'A'); assert.equal(n.name, 'a');
    if (st.includes('right=of b')) assert.ok(n.placed, 'a placement is no side');
  }
  // a label's side and auto exclude each other; a position is one key
  const lab = '\\draw (a) -- node[midway, above] {x} (b);';
  assert.equal(T.set(lab, 0, 'auto', ['auto']), '\\draw (a) -- node[midway, auto] {x} (b);');
  assert.equal(T.set(T.set(lab, 0, 'auto', ['auto']), 0, 'side', ['below']), '\\draw (a) -- node[midway, below] {x} (b);');
  assert.equal(T.set(lab, 0, 'pos', ['pos=0.3']), '\\draw (a) -- node[above, pos=0.3] {x} (b);');
  assert.equal(T.set(lab, 0, 'sloped', ['sloped']), '\\draw (a) -- node[midway, above, sloped] {x} (b);');
  assert.equal(T.unlabel(lab, 0), '\\draw (a) -- (b);');
  assert.equal(T.unlabel('\\draw (a) -- node {x} (b) node[pos=.2] {y};', 1), '\\draw (a) -- node {x} (b);');
  // a path's own options (-1): a bare color among the names given goes with the color group
  assert.equal(T.set('\\draw[->, red] (a) -- (b);', -1, 'draw', ['blue'], ['red', 'blue']), '\\draw[->, blue] (a) -- (b);');
  assert.equal(T.set('\\draw[->] (a) -- (b);', -1, 'dash', ['dotted']), '\\draw[->, dotted] (a) -- (b);');
  assert.equal(T.set('\\draw (a) -- (b);', -1, 'width', ['very thick']), '\\draw[very thick] (a) -- (b);');
  assert.equal(PA.fontSet('\\bfseries\\small', '\\Large'), '\\Large\\bfseries');
  assert.equal(PA.fontSet('\\small', null), null);
});

// Intuition: a segment's routing is its operation alone: straight, either right angle, a bend, angles; the points and
// labels stay. Pins a bend written over `--` that dropped the label after it, a straight path left as `to[]`.
test('segments: route, bend, angles, looseness, and back', () => {
  const st = '\\draw[->] (a) -- node[above] {l} (b) -- (c);';
  const op = st.indexOf('--');
  assert.deepEqual(T.segOf(st, 0), { a: 'a', b: 'b', at: st.indexOf('{l}') + 3, op: [op, op + 2], kind: '--', keys: [] }, 'a label goes after the one there');
  assert.equal(T.route(st, 1, '-|'), '\\draw[->] (a) -- node[above] {l} (b) -| (c);');
  assert.equal(T.route(T.route(st, 0, '|-'), 0, '--'), st);
  const b = T.bend(st, 0, 'left');
  assert.equal(b, '\\draw[->] (a) to[bend left] node[above] {l} (b) -- (c);');
  assert.equal(T.bend(st, 0, 'right', 45), '\\draw[->] (a) to[bend right=45] node[above] {l} (b) -- (c);');
  assert.equal(T.bend(b, 0, null), st, 'straight again: --');
  assert.equal(T.looseness(T.inout(st, 0, 0, 180), 0, 1.5), '\\draw[->] (a) to[out=0, in=180, looseness=1.5] node[above] {l} (b) -- (c);');
  assert.equal(T.bend(T.inout(st, 0, 0, 180), 0, 'left'), b, 'a bend drops the angles');
  assert.equal(T.route('\\draw (a) to[bend left] (b);', 0, '|-'), '\\draw (a) |- (b);');
  for (const s of [b, T.route(st, 1, '-|'), T.inout(st, 0, 10, 200)]) {
    const e = T.edges(wrap(s))[0];
    assert.deepEqual(e.segs.map((g) => [g.a, g.b]), [['a', 'b'], ['b', 'c']], s);
    assert.ok(T.nodes(wrap(s)).some((n) => n.kind === 'label' && n.text === 'l'), 'the label kept');
  }
  // the hit path: a corner for a right angle, a bend's curve off the straight line to its left (TikZ's y up)
  const g = (s) => T.segOf(s, 0), A = [0, 0], B = [100, 0];
  assert.deepEqual(T.segPath(A, [100, 50], g('\\draw (a) -| (b);')), [[0, 0], [100, 0], [100, 50]]);
  assert.deepEqual(T.segPath(A, [100, 50], g('\\draw (a) |- (b);')), [[0, 0], [0, 50], [100, 50]]);
  const left = T.segPath(A, B, g('\\draw (a) to[bend left] (b);')), mid = left[8];
  assert.ok(mid[1] < -5 && Math.abs(mid[0] - 50) < 1, `bend left goes up on the page: ${mid}`);
  assert.ok(T.segPath(A, B, g('\\draw (a) to[bend right] (b);'))[8][1] > 5);
  assert.deepEqual(left[0], A); assert.deepEqual(left.at(-1), B);
});

// Intuition: a new edge goes on its own line before `\end{tikzpicture}`, indented as the statements; an unnamed end
// gets a fresh name. Pins an edge written after `\end{tikzpicture}` and a name taken twice.
test('new edges, names, picture options, angles', () => {
  const text = '\\begin{tikzpicture}[x=2cm]\n  \\node (a) {A};\n  \\node (src) at (2,0) {Source};\n\\end{tikzpicture}\n';
  const e = T.addEdge(text, 0, 'a', 'src');
  assert.equal(edit(text, [{ from: e.at, to: e.at, now: e.text }]), '\\begin{tikzpicture}[x=2cm]\n  \\node (a) {A};\n  \\node (src) at (2,0) {Source};\n  \\draw[->] (a) -- (src);\n\\end{tikzpicture}\n');
  const one = '\\begin{tikzpicture}\n\\end{tikzpicture}', e1 = T.addEdge(one, 0, 'a', 'b');
  assert.equal(edit(one, [{ from: e1.at, to: e1.at, now: e1.text }]), '\\begin{tikzpicture}\n  \\draw[->] (a) -- (b);\n\\end{tikzpicture}');
  assert.equal(T.fresh(text, 0, 'Source'), 'source');
  assert.equal(T.fresh(text, 0, '\\TeX{} src'), 'src2', 'taken: a number after it');
  assert.equal(T.named('\\node[draw] {A};', 0, 'n'), '\\node[draw] (n) {A};');
  assert.equal(T.named('\\node {A};', 0, 'n'), '\\node (n) {A};');
  assert.equal(T.named('\\node (a) {A};', 0, 'n'), '\\node (a) {A};', 'a name kept');
  const r = T.picSet(text, 0, /^rotate\s*=/, ['rotate=90']);
  assert.equal(edit(text, [{ from: r.from, to: r.to, now: r.now }]).split('\n')[0], '\\begin{tikzpicture}[x=2cm, rotate=90]');
  const bare = '\\begin{tikzpicture}\n\\node {A};\n\\end{tikzpicture}', r2 = T.picSet(bare, 0, /^rotate around/, ['rotate around={45:(a)}']);
  assert.equal(edit(bare, [{ from: r2.from, to: r2.to, now: r2.now }]).split('\n')[0], '\\begin{tikzpicture}[rotate around={45:(a)}]');
  assert.equal(T.picSet(edit(bare, [{ from: r2.from, to: r2.to, now: r2.now }]), 0, /^rotate around/, []).now, '', 'no option left: no brackets');
  for (const [d, want] of [[7, 0], [8, 15], [-97, -90], [181, 180], [359, 0], [-180, 180], [44, 45]]) assert.equal(T.snapAngle(d, 15), want, `${d}`);
  assert.equal(T.angle('\\node[rotate=30] (a) {A};', 0), 30);
});

// Intuition: a library already loaded adds nothing; else its load goes after the file's last `\usetikzlibrary` or its tikz
// `\usepackage`; a file without either gets the line to add said. Pins a load added twice, inside a comment, after \begin{document}.
test('libraries, colors, mixes, cuts', () => {
  const doc = '\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{arrows.meta}\n\\begin{document}\n\\usetikzlibrary{x}\n\\end{document}\n';
  assert.deepEqual(PA.libEdit(doc, doc, [], 'arrows.meta'), { have: true });
  assert.deepEqual(PA.libEdit(doc, doc, ['shapes.geometric'], 'shapes.geometric'), { have: true }, 'the run loaded it');
  const at = doc.indexOf('\\begin{document}');
  assert.deepEqual(PA.libEdit(doc, doc, [], 'shapes.symbols'), { edit: { from: at, to: at, now: '\\usetikzlibrary{shapes.symbols}\n' } });
  const commented = '\\usepackage{tikz}\n% \\usetikzlibrary{shapes.arrows}\n';
  assert.ok('edit' in PA.libEdit(commented, commented, [], 'shapes.arrows'), 'a comment loads nothing');
  assert.deepEqual(PA.libEdit('\\section{A}\n', doc + '\\section{A}', [], 'shapes.symbols'), { line: '\\usetikzlibrary{shapes.symbols}' }, 'chapter.tex: the line to add');
  assert.ok(!('have' in PA.libEdit(doc, doc, [], 'arrows')), 'arrows is not arrows.meta');
  const c = PA.colors('\\definecolor{accent}{HTML}{0B3D91}\n\\colorlet{soft}{accent!20}\n\\definecolor{g}{rgb}{0,0.5,0}', null);
  assert.deepEqual(c.doc, [['accent', '#0b3d91'], ['soft', '#ced8e9'], ['g', '#008000']]);
  assert.ok(c.base.some(([n]) => n === 'red') && !c.more.length, 'xcolor\'s base colors without the run');
  const f = PA.colors('', { colors: [{ name: 'red', hex: '#ff0000' }, { name: 'Maroon', hex: '#af3235' }] });
  assert.deepEqual([f.base, f.more], [[['red', '#ff0000']], [['Maroon', '#af3235']]], 'the run\'s colors');
  const known = new Map([['red', [1, 0, 0]], ['blue', [0, 0, 1]]]);
  assert.deepEqual(PA.mix('red!50!blue', known), [0.5, 0, 0.5]);
  assert.deepEqual(PA.mix('red!25', known), [1, 0.75, 0.75]);
  assert.equal(PA.mix('nope!5', known), null);
  const t = 'a\n  \\node (x) {X};\nb \\node (y) {Y}; c\n';
  assert.deepEqual(PA.cut(t, t.indexOf('\\node (x)'), t.indexOf('};') + 2), { from: 2, to: t.indexOf('b'), now: '' }, 'a statement alone: its line');
  const y = t.indexOf('\\node (y)');
  assert.deepEqual(PA.cut(t, y, y + 15), { from: y, to: y + 15, now: '' });
});

/** Every row of `items` that writes at once (no prompt, no picker), with its path of labels. */
function* leaves(items, path = []) {
  for (const it of items) {
    if (it.items) yield* leaves(it.items, [...path, it.label]);
    else if (it.run && !/…$/.test(it.label)) yield [[...path, it.label].join(' / '), it];
  }
}
/** A context over `text` that records each write and each toast; `pick` picks `other`. */
const ctxOf = (text, other = null, facts = null) => {
  const c = { text, all: text, x: 0, y: 0, facts, writes: [], toasts: [], write: (e) => c.writes.push(e), toast: (m) => c.toasts.push(m), pick: (_, done) => other && done(other), colAlign: (t, k, a) => c.writes.push([{ col: [t, k, a] }]) };
  return c;
};

// Intuition: every menu row of the TikZ plugins, run on the drawing example and the thesis chapter's figure, writes text
// whose pictures still parse: the same number of statements' nodes (or one less when deleted, one more when duplicated),
// and every other picture untouched. Pins rows whose rewrite broke the statement (an option list left open, a lost `;`).
test('every TikZ menu row writes text that parses', withStore(() => {
  const SITE = new URL('../', import.meta.url);
  for (const file of ['examples/drawing.tex', 'examples/chapter.tex']) {
    const text = readFileSync(new URL(file, SITE), 'utf8'), nodes = T.nodes(text), edges = T.edges(text);
    const targets = [...nodes.map((n) => ({ kind: n.kind === 'label' ? 'tikz-label' : 'tikz-node', n })), ...edges.flatMap((e) => e.segs.map((_, k) => ({ kind: 'tikz-edge', e, k })))];
    let rows = 0;
    for (const t of targets) {
      const other = nodes.find((m) => m.kind === 'node' && m !== t.n && m.pic.at === (t.n ?? t.e).pic.at);
      const gs = PA.groups(t, ctxOf(text, other));
      assert.ok(gs.length >= 3, `${file}: ${t.kind} has its groups`);
      for (const [label, it] of leaves(gs.flatMap((g) => g.inline ? g.items : [{ label: g.title, items: g.items }]))) {
        const c = ctxOf(text, other);
        // a row's own context: the plugin's rows close over theirs
        const g2 = PA.groups(t, c), again = [...leaves(g2.flatMap((g) => g.inline ? g.items : [{ label: g.title, items: g.items }]))].find(([l]) => l === label)?.[1];
        again.run();
        rows++;
        for (const w of c.writes) {
          const now = edit(text, w), ns = T.nodes(now).filter((n) => n.kind === 'node'), was = nodes.filter((n) => n.kind === 'node');
          const d = /Duplicate/.test(label) ? 1 : /Delete/.test(label) && t.kind === 'tikz-node' ? -1 : 0;
          assert.equal(ns.length, was.length + d, `${file}: ${t.kind} ${label}: nodes\n${now}`);
          assert.equal((now.match(/\\begin\{tikzpicture\}/g) ?? []).length, (now.match(/\\end\{tikzpicture\}/g) ?? []).length, label);
          for (const s of T.edges(now)) assert.ok(s.segs.length, `${label}: a path lost its points`);
          if (!/Delete|Bring|Send|Duplicate|Add edge|picture/.test(label)) {
            // a row rewrites its own statement only
            const own = t.n ?? t.e, lib = /\\usetikzlibrary\{[^}]*\}\n/.exec(now.slice(0, now.indexOf('\\begin{document}')))?.[0];
            const before = text.slice(0, own.from), after = text.slice(own.to);
            assert.ok(now.endsWith(after) && (now.startsWith(before) || lib), `${file}: ${label} wrote outside its statement`);
          }
        }
      }
    }
    assert.ok(rows > 200, `${file}: ${rows} rows`);
  }
}));

// Intuition: the forest plugin's rows rewrite the tree as the forest widget emits it; the table plugin's rows name
// the column's letters. Pins the menus moved from the playground losing a row.
test('forest and table rows', withStore(() => {
  const F = self.satexViews.forest, body = '\n[Car [Engine, circle] [Body]]\n', t = F.parse(body), n = t.root.children[0];
  const text = `\\begin{forest}${body}\\end{forest}`, f = { from: 14, to: 14 + body.length, body, t, n };
  const c = ctxOf(text), gs = PA.groups({ kind: 'forest-node', f }, c);
  assert.deepEqual(gs.flatMap((g) => g.items.map((i) => i.label + (i.on ? '*' : ''))), ['Rectangle', 'Rounded rectangle', 'Circle*', 'Frame', 'Add child', 'Add sibling', 'Delete', 'Edit text…']);
  gs[2].items[0].run();
  assert.match(edit(text, c.writes[0]), /\[Engine, circle\s*\[new\]\]/);
  const col = PA.groups({ kind: 'table-column', t: 0, k: 1, col: { spec: [1, 2], align: 'r' } }, c);
  assert.deepEqual(col[0].items.map((i) => i.label[0] + (i.on ? '*' : '')), ['l', 'c', 'r*']);
  col[0].items[1].run();
  assert.deepEqual(c.writes.at(-1), [{ col: [0, 1, 'c'] }]);
  assert.deepEqual(PA.groups({ kind: 'table-column', t: 0, k: 0, col: { align: 'X' } }, c), [], 'no letter to rewrite: no menu');
}));

// Intuition: TikZ knows a name only after its node: a node moves in the drawing order only where nothing then refers to a
// name before it is set, and goes only with the paths that end at it, never under a node placed by it. Pins a Bring to
// front that put `\node (src)` after `right=of src`, and a Delete that left `below=of run` without its node.
test('reordering and deleting keep every name defined before its use', withStore(() => {
  const rows = (text, name) => { const n = T.nodes(text).find((m) => m.name === name); return PA.groups({ kind: 'tikz-node', n }, ctxOf(text)).find((g) => g.title === 'Arrange').items.map((i) => i.label); };
  const drawing = readFileSync(new URL('../examples/drawing.tex', import.meta.url), 'utf8');
  assert.deepEqual(rows(drawing, 'src'), ['Add edge from this node…', 'Duplicate'], 'placed by run, used by paths after it');
  assert.deepEqual(rows(drawing, 'note'), ['Add edge from this node…', 'Duplicate', 'Delete with its paths']);
  const a = '\\node (a) {A};', b = '\\node (b) at (1,0) {B};', free = wrap(`${a}\n${b}`), swapped = wrap(`${b}\n${a}`);
  const run = (k, label) => { const c = ctxOf(free); PA.groups({ kind: 'tikz-node', n: T.nodes(free)[k] }, c).find((g) => g.title === 'Arrange').items.find((i) => i.label === label).run(); return edit(free, c.writes[0]); };
  assert.equal(run(0, 'Bring to front'), swapped);
  assert.equal(run(1, 'Send to back'), swapped);
}));

// Intuition: a plugin switched off gives no rows, one whose type plugin is off neither. Pins a switch that did nothing.
test('plugins switched per viewer and with their type plugin', withStore(() => {
  const text = wrap('\\node (a) {A};'), t = { kind: 'tikz-node', n: T.nodes(text)[0] };
  assert.ok(PA.groups(t, ctxOf(text)).length);
  assert.equal(PA.groups(t, ctxOf(text), { 'type_plugins.tikz.enabled': false }).length, 0);
  PA.setOn('tikz-node', false);
  assert.equal(PA.groups(t, ctxOf(text)).length, 0);
  PA.setOn('tikz-node', true);
  assert.ok(PA.groups(t, ctxOf(text)).length);
  assert.deepEqual(PA.plugins.map((p) => p.name), ['tikz-node', 'tikz-label', 'tikz-edge', 'forest-node', 'table-column']);
}));

// Intuition: a dragged node is written beside its reference as positioning does: a side when the centres line up, a
// corner with its vertical then horizontal gap, a gap equal to the node distance unwritten; the placement, shifts and
// `at` it had go. Pins a drag that wrote `at (x,y)` where `right=of a` was meant, and `above right` with its gaps swapped.
test('relative placement: positioning keys from frames', () => {
  const cm = 72 / 2.54, box = (l, t, w = cm, h = cm / 2) => ({ l, t, r: l + w, d: t + h }), a = box(0, 0);
  assert.equal(T.relation(box(2 * cm, 0), a, 0.5, [1, 1], 'a'), 'right=of a', 'a 1 cm gap: the node distance');
  assert.equal(T.relation(box(2.5 * cm, 0), a, 0.5, [1, 1], 'a'), 'right=1.5cm of a');
  assert.equal(T.relation(box(-2 * cm, 0), a, 0.5, [1, 1.2], 'a'), 'left=1cm of a', 'the horizontal distance of `1cm and 1.2cm`');
  assert.equal(T.relation(box(0, 2 * cm), a, 0.5, [1, 1], 'a'), 'below=1.5cm of a');
  assert.equal(T.relation(box(3 * cm, -2 * cm), a, 0.5, [1, 1], 'a'), 'above right=1.5cm and 2cm of a', 'vertical, then horizontal');
  assert.equal(T.relation(a, a, 0.5, [1, 1], 'a'), null);
  assert.equal(T.relation(box(0.5 * cm, 2 * cm), a, 0.5, [1, 1], 'a'), 'below=1.5cm of a, xshift=0.5cm', 'overlapping across: a side and a shift');
  assert.equal(T.place('\\node (b) {B};', 0, 'below=of a, xshift=5mm'), '\\node[below=of a, xshift=5mm] (b) {B};');
  assert.equal(T.place('\\node[box, right=of a, xshift=2mm] (b) {B};', 0, 'below=of a'), '\\node[box, below=of a] (b) {B};');
  assert.equal(T.place('\\node[draw] (b) at (1,2) {B};', 0, 'right=of a'), '\\node[draw, right=of a] (b) {B};');
  assert.equal(T.place('\\node (b) [right of=a] {B};', 0, 'right=of a'), '\\node (b) [right=of a] {B};', 'tikz\'s old `right of=` goes too');
  assert.deepEqual(T.nodes(wrap('\\node (a) {A};'))[0].pic.dist, [1, 1]);
  assert.deepEqual(T.nodes('\\begin{tikzpicture}[node distance=1cm and 2cm]\n\\node (a) {A};\n\\end{tikzpicture}')[0].pic.dist, [1, 2]);
});

// Intuition: Edit text is the menu's first row, the Position rows offer what the playground worked out (a fixed place, a
// place beside the nearest node), and a duplicate stands below its original by positioning, loaded with it.
test('node menu: edit first, fixed and relative places, a relative duplicate', withStore(() => {
  const text = '\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\begin{tikzpicture}\n  \\node (a) at (0,0) {A};\n\\end{tikzpicture}\n\\end{document}\n';
  const n = T.nodes(text)[0], c = ctxOf(text), gs = PA.groups({ kind: 'tikz-node', n, fixed: '\\node (a) at (1,0) {A};', rel: null }, c);
  assert.equal(gs[0].items[0].label, 'Edit text…');
  const pos = gs.find((g) => g.title === 'Position').items;
  assert.deepEqual(pos.map((i) => i.label).slice(0, 2), ['Fixed position', 'Rotate']);
  pos[0].run();
  assert.match(edit(text, c.writes[0]), /\\node \(a\) at \(1,0\) \{A\};/);
  gs.find((g) => g.title === 'Arrange').items.find((i) => i.label === 'Duplicate').run();
  const dup = edit(text, c.writes[1]);
  assert.match(dup, /\\usepackage\{tikz\}\n\\usetikzlibrary\{positioning\}\n/);
  assert.match(dup, /\\node \(a\) at \(0,0\) \{A\};\n  \\node\[below=of a\] \(a2\) \{A\};/);
}));

// Intuition: a dragged node's dependents are those placed from it and the chains placed from them; unlocked, each one
// placed straight from it is shifted back by the drag, so after the node moves every dependent stands where it stood.
// Pins a dependent that followed an unlocked drag, and a chain member shifted twice (it follows its own parent).
test('dependents of a dragged node and their stay rewrite', () => {
  const text = wrap('\\node (a) {A};\n\\node[right=of a] (b) {B};\n\\node[below=1cm of b] (c) {C};\n\\node (d) at (a.east) {D};\n\\node (e) at (3,0) {E};\n\\draw (a) -- (e);\n\\node[draw] at (1,1) {(a)};');
  const [a, , , , e] = T.nodes(text), r = T.dependents(text, a);
  assert.deepEqual([r.direct.map((n) => n.name), r.all.map((n) => n.name)], [['b', 'd'], ['b', 'd', 'c']], 'a path is no dependent, nor a text that spells (a)');
  assert.deepEqual(T.dependents(text, e).all, []);
  // the playground's stay rewrite: each direct one moved back by the drag (here 1 cm right, 5 mm down)
  const back = r.direct.map((n) => T.move(text.slice(n.from, n.to), n.i, n.pic, { by: [-1, 0.5], grid: [0.01, 0.01] }).text);
  assert.deepEqual(back, ['\\node[right=of a, xshift=-10mm, yshift=5mm] (b) {B};', '\\node[xshift=-10mm, yshift=5mm] (d) at (a.east) {D};']);
  for (const s of back) assert.ok(T.nodes(wrap(s))[0].placed || /at \(a\.east\)/.test(s), 'the relative form kept');
});

// Intuition: `placing`'s fix moves the frame to where TikZ sets it from the keys written: its gap to the reference is the
// written gap exactly (on the grid, or the node distance), its centre on the reference's line. The dependents left behind
// are shifted by the drag plus this fix, so they stay to the point. Pins a drift of the grid's rounding (~1 bp).
test('relative placement: the landed place is the written one', () => {
  const cm = 72 / 2.54, box = (l, t, w = cm, h = cm / 2) => ({ l, t, r: l + w, d: t + h }), a = box(0, 0), R = rng(733);
  const move = (b, f) => ({ l: b.l + f[0], r: b.r + f[0], t: b.t + f[1], d: b.d + f[1] });
  for (let k = 0; k < 300; k++) {
    const b = box((R.float() - 0.5) * 8 * cm, (R.float() - 0.5) * 8 * cm), dist = [R.pick([1, 1.2]), R.pick([1, 2.4])];
    const at = T.placing(b, a, 0.5, dist, 'a');
    if (!at) continue;
    const m = move(b, at.fix), keys = at.text.split(', '), [, side, val] = /^([a-z ]+)=(.*?)\s*of a$/.exec(keys[0]);
    const gaps = val ? val.split(' and ').map((x) => parseFloat(x)) : side.includes(' ') ? dist : [/above|below/.test(side) ? dist[0] : dist[1]];
    const [gv, gh] = side.includes(' ') ? gaps : /above|below/.test(side) ? [gaps[0], null] : [null, gaps[0]];
    const near = (x, y) => Math.abs(x - y) < 1e-6;
    if (gh != null) assert.ok(near(side.includes('right') ? m.l - a.r : a.l - m.r, gh * cm), `${at.text}: horizontal gap`);
    if (gv != null) assert.ok(near(side.includes('below') ? m.t - a.d : a.t - m.d, gv * cm), `${at.text}: vertical gap`);
    const shift = keys[1] && /([xy])shift=(-?[\d.]+)cm/.exec(keys[1]);
    if (gh == null) assert.ok(near((m.l + m.r) / 2, (a.l + a.r) / 2 + (shift?.[1] === 'x' ? +shift[2] * cm : 0)), `${at.text}: centred across`);
    if (gv == null) assert.ok(near((m.t + m.d) / 2, (a.t + a.d) / 2 - (shift?.[1] === 'y' ? +shift[2] * cm : 0)), `${at.text}: centred across`);
  }
});

// Intuition: beside the same node on the same side, a node's move is the difference of the written gaps and shifts,
// with no frame size guessed; the dependents left behind get exactly that back. Pins the ~1 bp drift of an estimated frame.
test('relative placement: the move from the keys alone', () => {
  const bp = 72 / 2.54, near = (u, v) => u && v && Math.abs(u[0] - v[0]) < 1e-9 && Math.abs(u[1] - v[1]) < 1e-9;
  assert.ok(near(T.placeDelta(['box', 'right=of src'], 'right=2.15cm of src', [1.2, 1.2]), [0.95 * bp, 0]));
  assert.ok(near(T.placeDelta(['below=of run'], 'below=1.5cm of run, xshift=-3mm', [1, 1]), [-0.3 * bp, 0.5 * bp]));
  assert.ok(near(T.placeDelta(['above right=1cm and 2cm of a'], 'above right=1.5cm and 1cm of a'), [-1 * bp, -0.5 * bp]));
  assert.equal(T.placeDelta(['right=of a'], 'right=of b'), null, 'another node');
  assert.equal(T.placeDelta(['right=of a'], 'below=of a'), null, 'another side');
  assert.equal(T.placeDelta(['draw'], 'right=of a'), null, 'not placed before');
});
