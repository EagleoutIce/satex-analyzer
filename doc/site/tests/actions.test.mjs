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
  // joined to the file's list (its `}`), comma-separated in the list's style; a file with none gets a line
  const at = doc.indexOf('arrows.meta}') + 'arrows.meta'.length;
  assert.deepEqual(PA.libEdit(doc, doc, [], 'shapes.symbols'), { edit: { from: at, to: at, now: ',shapes.symbols' } });
  const spaced = doc.replace('{arrows.meta}', '{arrows.meta, calc}'), at2 = spaced.indexOf('calc}') + 4;
  assert.deepEqual(PA.libEdit(spaced, spaced, [], 'matrix'), { edit: { from: at2, to: at2, now: ', matrix' } });
  const bare = '\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\end{document}\n', at3 = bare.indexOf('\\begin');
  assert.deepEqual(PA.libEdit(bare, bare, [], 'shapes.symbols'), { edit: { from: at3, to: at3, now: '\\usetikzlibrary{shapes.symbols}\n' } });
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
  const c = { text, all: text, x: 0, y: 0, facts, writes: [], toasts: [], write: (e) => c.writes.push(e), toast: (m) => c.toasts.push(m), pick: (_, done) => other && done(other), colAlign: (t, k, a, w, h) => c.writes.push([{ col: [t, k, a, ...(w == null && h == null ? [] : [w]), ...(h == null ? [] : [h])] }]) };
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

// Intuition: an arrow key splits into its two ends and joins back the same, braces where a name has spaces or options;
// each end is picked on its own, the other kept; a variant toggles in the tip's options; arrows.meta is loaded once a
// capitalized tip is new to the run. Pins `-{Arc Barb}` not read as a tip (a second one written beside it) and a start
// pick that dropped the end.
test('arrow tips: ends, join, variants, each end on its own', withStore(() => {
  for (const [spec, ends] of [['->', ['', '>']], ['<->', ['<', '>']], ['-{Stealth[open]}', ['', 'Stealth[open]']], ['{Arc Barb}-Latex', ['Arc Barb', 'Latex']], ['-', ['', '']], ['', ['', '']], ['|->', ['|', '>']]]) {
    assert.deepEqual(T.tipEnds(spec), ends, spec);
    assert.deepEqual(T.tipEnds(T.tipSpec(...ends)), ends, `${spec} joins back`);
  }
  assert.equal(T.tipEnds('draw=red'), null);
  assert.equal(T.tipSpec('', 'Stealth[open, round]'), '-{Stealth[open, round]}');
  assert.equal(T.tipSpec('', ''), '');
  assert.equal(T.tipVary('Stealth', 'open', true), 'Stealth[open]');
  assert.equal(T.tipVary('Stealth[open, round]', 'open', false), 'Stealth[round]');
  assert.equal(T.tipVary('Stealth[open]', 'open', true), 'Stealth[open]', 'idempotent');
  assert.equal(T.tip('\\draw[-{Arc Barb}, red] (a) -- (b);', '->'), '\\draw[red, ->] (a) -- (b);', 'a braced tip is a tip key');
  const text = '\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\begin{tikzpicture}\n\\node (a) {A}; \\node (b) at (2,0) {B};\n\\draw[->, red] (a) -- (b);\n\\end{tikzpicture}\n\\end{document}\n';
  const e = T.edges(text)[0], tips = (c) => PA.groups({ kind: 'tikz-edge', e, k: 0 }, c).find((g) => g.title === 'Arrow tips').items;
  const row = (items, ...path) => path.reduce((its, l, i) => { const r = its.find((x) => x.label === l); assert.ok(r, `${l} in ${its.map((x) => x.label)}`); return i === path.length - 1 ? r : r.items; }, items);
  const c0 = ctxOf(text, null, { tips: [{ name: '>', svg: null }] });
  c0.previews = { 'arrow:-Stealth': "<svg viewBox='0 0 1 1'><path d='M0 0H5'/><path d='M1 1L2 2Z'/></svg>" };
  assert.equal(row(tips(c0), '→').on, true, 'the current pair ticked');
  assert.equal(row(tips(c0), 'End', 'default (>)').on, true, 'the current end ticked');
  assert.equal(row(tips(c0), 'Start', 'none').on, true);
  assert.match(row(tips(c0), 'End', 'Stealth').img, /M1 1L2 2Z/, 'its picture from previews.json');
  const pick = (facts, ...path) => { const c = ctxOf(text, null, facts); row(tips(c), ...path).run(); return edit(text, c.writes[0]); };
  const s1 = pick({ tips: [] }, 'Start', 'Stealth');
  assert.match(s1, /\\draw\[red, Stealth->\]/, 'the start picked, the end kept');
  assert.match(s1, /\\usetikzlibrary\{arrows\.meta\}/, 'arrows.meta loaded for it');
  assert.doesNotMatch(pick({ tips: [{ name: 'Stealth' }] }, 'End', 'Arc Barb'), /usetikzlibrary/, 'a run that declares capitalized tips has arrows.meta');
  assert.match(pick({ tips: [{ name: 'Stealth' }] }, 'End', 'Arc Barb'), /\\draw\[red, -\{Arc Barb\}\]/);
  assert.match(pick({ tips: [] }, 'Start', 'default (>)'), /\\draw\[red, <->\]/, 'the classic start is written <');
  // a variant of the current tip
  const t2 = text.replace('->', '-Stealth'), e2 = T.edges(t2)[0], c2 = ctxOf(t2, null, { tips: [{ name: 'Stealth' }] });
  const items = PA.groups({ kind: 'tikz-edge', e: e2, k: 0 }, c2).find((g) => g.title === 'Arrow tips').items;
  assert.deepEqual(row(items, 'End').items.filter((r) => r.head || /^(open|round|reversed)$/.test(r.label)).map((r) => r.label).slice(-3), ['open', 'round', 'reversed']);
  row(items, 'End', 'open').run();
  assert.match(edit(t2, c2.writes[0]), /\\draw\[red, -\{Stealth\[open\]\}\]/);
  const c3 = ctxOf(text, null, { tips: [{ name: 'Stealth' }] });
  assert.equal(row(tips(c3), 'End', 'Arc Barb').items, undefined);
  assert.ok(!row(tips(c3), 'End').items.some((r) => r.label === 'open'), 'no variants for the classic >');
}));

// Intuition: a bend point is a coordinate between two segments: added it splits a segment (a `to[…]` one straight to it),
// moved only its text changes, removed the two segments are one again with both labels, made a curve the path leaves and
// enters at the given angles. Pins a removal that dropped a label and a bend point read in a node's name.
test('bend points: add, move, remove, curve', withStore(() => {
  const st = '\\draw[->] (a) -- (b);', a1 = T.bendAdd(st, 0, '1,2');
  assert.equal(a1, '\\draw[->] (a) -- (1,2) -- (b);');
  assert.ok(T.isBend(a1, 0) && !T.isBend(a1, 1) && !T.isBend(st, 0));
  assert.equal(T.bendMove(a1, 0, '1.5,2'), '\\draw[->] (a) -- (1.5,2) -- (b);');
  assert.equal(T.bendRemove(a1, 0), st, 'removed: the text before');
  assert.equal(T.bendCurve(a1, 0, 45, 135), '\\draw[->] (a) to[out=45, in=135] (b);');
  assert.equal(T.bendAdd('\\draw (a) to[bend left] node[above] {x} (b);', 0, '0,1'), '\\draw (a) -- node[above] {x} (0,1) -- (b);');
  assert.equal(T.bendRemove('\\draw (a) -- node {l} (1,1) -- node {m} (b);', 0), '\\draw (a) -- node {l} node {m} (b);', 'both labels kept');
  assert.equal(T.bendRemove('\\draw (a) -- (b) -- (c);', 0), null, 'a node is no bend point');
  for (const p of ['0.5,-1', '2cm,3cm']) { const t = T.bendAdd(st, 0, p); assert.equal(T.edges(wrap(t))[0].segs.length, 2, t); }
  const text = wrap('\\node (a) {A}; \\node (b) at (3,0) {B};\n\\draw (a) -- (1,1) -- (b);'), e = T.edges(text)[0];
  const c = ctxOf(text), gs = PA.groups({ kind: 'tikz-bend', e, k: 0, curve: [45, 135] }, c);
  assert.deepEqual(gs.flatMap((g) => g.items.map((i) => i.label)), ['Remove bend point', 'Convert to curve']);
  gs[0].items[1].run();
  assert.match(edit(text, c.writes[0]), /\\draw \(a\) to\[out=45, in=135\] \(b\);/);
}));

// Intuition: every width argument a box drag rewrites is found with its value's exact range and the lines its box spans;
// a commented call, `\resizebox{!}` and a graphic without `width=` are none. Ctrl snaps to a share of the line or a width
// the document uses when within 3 mm, else to the 5 mm grid. Pins a range off by the key's spaces (`width = 3cm`).
test('box widths: found, ranged, snapped', () => {
  const t = 'a\n\\begin{minipage}[t]{0.45\\linewidth}\nx\n\\end{minipage}\n\\parbox{3cm}{y}\\makebox[2cm]{z} % \\parbox{1cm}{q}\n\\resizebox{!}{1cm}{w}\\resizebox{.5\\textwidth}{!}{w}\n\\includegraphics[height=1cm, width = 3cm]{f}\\includegraphics{g}';
  const got = T.sizers(t);
  assert.deepEqual(got.map((s) => [s.kind, s.line, s.end, s.value]), [['minipage', 2, 4, '0.45\\linewidth'], ['parbox', 5, 5, '3cm'], ['makebox', 5, 5, '2cm'], ['resizebox', 6, 6, '.5\\textwidth'], ['includegraphics', 7, 7, '3cm']]);
  for (const s of got) assert.equal(t.slice(s.from, s.to), s.value, s.kind);
  const MM = 72 / 25.4, line = 100 * MM; // its quarter is 25 mm, apart from the 30 mm width below
  assert.equal(T.snapSize(line / 2 + 2 * MM, line, 'linewidth', []), '.5\\linewidth');
  assert.equal(T.snapSize(line / 3, line, 'textwidth', []), '.333\\textwidth');
  assert.equal(T.snapSize(31 * MM, line, 'linewidth', [['3cm', 30 * MM]]), '3cm', 'a width the document uses');
  assert.equal(T.snapSize(42 * MM, line, 'linewidth', []), '4cm', 'else the 5 mm grid');
  assert.equal(T.snapSize(line, line, 'linewidth', []), '\\linewidth');
});

// Intuition: the document's named styles are offered where options are: each with where it is defined and the comment
// above it, ticked where the part uses it, applied and removed as its one key. `every …` styles and commented definitions
// are none. Pins a style offered twice (defined in the file and in the text of the document) and a toggle that dropped
// another key.
test('named styles: found with their comment, applied, removed', withStore(() => {
  const head = '% a rounded box for steps\n\\tikzset{box/.style={draw, rounded corners}, every node/.style={font=\\small},\n  flow/.style={-Stealth, thick}}\n% \\tikzset{old/.style={red}}\n\\tikzstyle{legacy}=[blue]\n';
  const got = PA.styles(head + '\\begin{tikzpicture}[hot/.style={fill=red!20}]\n\\end{tikzpicture}');
  assert.deepEqual(got.map((x) => [x.name, x.body, x.line, x.note]), [['box', 'draw, rounded corners', 2, 'a rounded box for steps'], ['flow', '-Stealth, thick', 3, ''], ['legacy', 'blue', 5, ''], ['hot', 'fill=red!20', 6, '']]);
  const text = `${head}\\begin{tikzpicture}\n\\node[red] (a) {A};\n\\end{tikzpicture}\n`, n = T.nodes(text)[0], c = ctxOf(text);
  c.file = 'main.tex';
  const rows = PA.groups({ kind: 'tikz-node', n }, c).find((g) => g.title === 'Style').items.find((r) => r.label === 'Document styles').items;
  assert.deepEqual(rows.map((r) => [r.label, r.hint]), [['box', 'defined in main.tex:2'], ['flow', 'defined in main.tex:3'], ['legacy', 'defined in main.tex:5']]);
  assert.match(rows[0].title, /^a rounded box for steps\nbox: draw, rounded corners$/);
  rows[0].run();
  const on = edit(text, c.writes[0]);
  assert.match(on, /\\node\[red, box\] \(a\)/);
  const c2 = ctxOf(on), n2 = T.nodes(on)[0], r2 = PA.groups({ kind: 'tikz-node', n: n2 }, c2).find((g) => g.title === 'Style').items.find((r) => r.label === 'Document styles').items[0];
  assert.equal(r2.on, true);
  r2.run();
  assert.match(edit(on, c2.writes[0]), /\\node\[red\] \(a\)/);
}));

// Intuition: a plain tree turned into a feature model gets the preset's key in `for tree` and its styles once above
// `\begin{document}`; a tree already one is not offered it; a document that defines the styles keeps its own. Pins styles
// written twice and a `for tree` whose other keys were lost.
test('forest: transform to feature model', withStore(() => {
  const F = self.satexViews.forest, body = '\nfor tree={draw}\n[Car [Engine] [Body]]\n';
  const run = (text) => {
    const from = text.indexOf('\\begin{forest}') + 14, to = text.indexOf('\\end{forest}'), t = F.parse(text.slice(from, to)), c = ctxOf(text);
    const row = PA.groups({ kind: 'forest-node', f: { from, to, body: text.slice(from, to), t, n: t.root } }, c).flatMap((g) => g.items).find((i) => i.label === 'Transform to feature model');
    row?.run();
    return row && edit(text, c.writes[0]);
  };
  const doc = `\\documentclass{article}\n\\usepackage{forest}\n\\begin{document}\n\\begin{forest}${body}\\end{forest}\n\\end{document}\n`, now = run(doc);
  assert.match(now, /for tree=\{draw, fm feature\}/);
  assert.match(now, /\\forestset\{\n  fm feature\/\.style=\{draw, rounded corners=1pt[^\n]*\n[^]*\}\n\\begin\{document\}/);
  assert.equal((now.match(/fm feature\/\.style/g) ?? []).length, 1);
  assert.equal(run(now), undefined, 'a feature model is not offered it again');
  const own = doc.replace('\\begin{document}', '\\forestset{fm feature/.style={draw}}\n\\begin{document}');
  assert.equal((run(own).match(/fm feature\/\.style/g) ?? []).length, 1, 'the document\'s own styles kept');
}));

// Intuition: a picture is named by its place among the file's pictures, so a grid switched on stays on its picture when
// text above it changes (its offset moves, its ordinal does not); a commented-out picture is none. Pins the grid keyed by
// offset, lost on the first key typed above the picture.
test('pictures by ordinal', () => {
  const a = 'x\n\\begin{tikzpicture}\\end{tikzpicture}\n% \\begin{tikzpicture}\n\\begin{tikzpicture}\\end{tikzpicture}', b = `more text\n${a}`;
  const second = a.lastIndexOf('\\begin{tikzpicture}');
  assert.equal(T.picOrdinal(a, second), 1);
  assert.equal(T.picAt(b, T.picOrdinal(a, second)), b.lastIndexOf('\\begin{tikzpicture}'));
  assert.equal(T.picOrdinal(a, 5), -1);
  assert.equal(T.picAt(a, 2), null);
  assert.equal(T.nodes(wrap('\\node (a) {A};'))[0].pic.at, T.picAt(wrap('\\node (a) {A};'), 0), 'the offsets the node parser gives');
});

// Intuition: a path without a tip of its own draws the tip its style brings, so the menu ticks that one and says where it
// comes from; a pick writes the tip after the style, so it wins. Pins `\draw[flow]` (flow: -Stealth) ticked "none".
test('arrow tips from a style', withStore(() => {
  const text = '\\tikzset{flow/.style={-Stealth, thick}}\n\\begin{tikzpicture}\n\\node (a) {A}; \\node (b) at (2,0) {B};\n\\draw[flow] (a) -- (b);\n\\end{tikzpicture}\n';
  const e = T.edges(text)[0], c = ctxOf(text, null, { tips: [{ name: 'Stealth' }] }), items = PA.groups({ kind: 'tikz-edge', e, k: 0 }, c).find((g) => g.title === 'Arrow tips').items;
  assert.equal(items[0].head, 'From style flow');
  assert.equal(items.find((r) => r.label === 'none').on, false);
  assert.equal(items.find((r) => r.label === 'End').items.find((r) => r.label === 'Stealth').on, true);
  items.find((r) => r.label === 'Start').items.find((r) => r.label === 'Latex').run();
  assert.match(edit(text, c.writes[0]), /\\draw\[flow, Latex-Stealth\]/);
  const own = text.replace('[flow]', '[flow, ->]'), c2 = ctxOf(own, null, { tips: [] });
  const it2 = PA.groups({ kind: 'tikz-edge', e: T.edges(own)[0], k: 0 }, c2).find((g) => g.title === 'Arrow tips').items;
  assert.equal(it2[0].on, true, 'its own tip ticked');
  assert.equal(it2[0].head, undefined);
}));

// Intuition: when a menu row leaves a part with the very keys a document style holds, the write names the style instead,
// in the place of its first key; a style of one key, or one only partly matched, leaves the keys as they are. Pins a path
// given `thick` beside `-Stealth` that kept both raw where `flow` says the same.
test('writes prefer a matching document style', withStore(() => {
  const head = '\\tikzset{flow/.style={-Stealth, thick}, hot/.style={fill=red!20}, box/.style={draw, rounded corners, fill=blue!10}}\n';
  const text = `${head}\\begin{tikzpicture}\n\\node (a) {A}; \\node (b) at (2,0) {B};\n\\draw[-Stealth, red] (a) -- (b);\n\\end{tikzpicture}\n`;
  const e = T.edges(text)[0], c = ctxOf(text, null, { tips: [{ name: 'Stealth' }] });
  const style = PA.groups({ kind: 'tikz-edge', e, k: 0 }, c).find((g) => g.title === 'Style').items;
  style.find((r) => r.label === 'Line width').items.find((r) => /^thick/.test(r.label)).run();
  assert.match(edit(text, c.writes[0]), /\\draw\[flow, red\] \(a\) -- \(b\);/);
  const t2 = text.replace('\\node (a) {A};', '\\node[draw, fill = blue!10] (a) {A};'), n = T.nodes(t2)[0], c2 = ctxOf(t2);
  PA.groups({ kind: 'tikz-node', n }, c2).find((g) => g.title === 'Style').items.find((r) => r.label === 'Rounded corners').items.find((r) => r.label === '4pt').run();
  assert.match(edit(t2, c2.writes[0]), /\\node\[draw, fill = blue!10, rounded corners=4pt\] \(a\)/, 'rounded corners=4pt is not the style\'s key');
  const c3 = ctxOf(t2);
  PA.groups({ kind: 'tikz-node', n }, c3).find((g) => g.title === 'Style').items.find((r) => r.label === 'Fill color').items.find((r) => r.label === 'None').run();
  assert.match(edit(t2, c3.writes[0]), /\\node\[draw, fill=none\] \(a\)|\\node\[draw\] \(a\)/, 'no style of one key, none partly matched');
}));

// Intuition: a feature model's look is its `\forestset` styles: each row rewrites one key of `fm feature` (or the arc's bend
// in both group styles), keeps every other key, ticks the value in force, and the document still parses as a tree. Pins
// a growth switch that left the old anchors (edges from the south of a tree growing right).
test('forest: feature model look via its styles', withStore(() => {
  const SITE = new URL('../', import.meta.url), text = readFileSync(new URL('examples/feature-model.tex', SITE), 'utf8'), F = self.satexViews.forest;
  const from = text.indexOf('\\begin{forest}') + 14, to = text.indexOf('\\end{forest}'), t = F.parse(text.slice(from, to));
  const rows = (tx) => PA.groups({ kind: 'forest-node', f: { from, to, body: tx.slice(from, to), t, n: t.root } }, ctxOf(tx)).find((g) => g.title === 'Feature model look').items;
  const run = (tx, ...path) => { const c = ctxOf(tx); const its = PA.groups({ kind: 'forest-node', f: { from, to, body: tx.slice(from, to), t, n: t.root } }, c).find((g) => g.title === 'Feature model look').items;
    path.reduce((r, l, i) => { const x = r.find((y) => y.label === l); assert.ok(x, l); if (i === path.length - 1) x.run(); return x.items; }, its); return edit(tx, c.writes[0]); };
  assert.equal(rows(text)[0].items.find((r) => r.on).label, '8mm', 'l sep in force ticked');
  const wide = run(text, 'Level distance (l sep)', '12mm');
  assert.match(wide, /fm feature\/\.style=\{draw, rounded corners=1pt, font=\\sffamily, inner sep=3pt, parent anchor=south, child anchor=north, l sep=12mm, s sep=4mm\}/);
  const right = run(text, 'Grows', 'right (edges east to west)');
  assert.match(right, /fm feature\/\.style=\{[^}]*grow'=0, parent anchor=east, child anchor=west\}/);
  assert.doesNotMatch(right.slice(right.indexOf('fm feature/'), right.indexOf('fm abstract')), /anchor=south|anchor=north/, 'the old anchors go');
  assert.equal(rows(right).find((r) => r.label === 'Grows').items[1].on, true);
  assert.equal(run(right, 'Grows', 'down (edges south to north)').slice(0, 900), text.replace('parent anchor=south, child anchor=north, l sep=8mm, s sep=4mm', 'l sep=8mm, s sep=4mm, parent anchor=south, child anchor=north').slice(0, 900), 'and back');
  assert.match(run(text, 'Arrow ends', '→ to the child'), /s sep=4mm, edge=\{->\}\}/);
  const bent = run(text, 'Group arcs', 'bend 50°');
  assert.equal((bent.match(/bend right=50/g) ?? []).length, 2, 'both group styles');
  assert.equal(F.parse(bent.slice(from, bent.indexOf('\\end{forest}'))).root.children.length, 3);
  // no styles in the document: no rows
  assert.equal(PA.groups({ kind: 'forest-node', f: { from: 14, to: 40, body: '', t, n: t.root } }, ctxOf('\\begin{forest}[a [b]]\\end{forest}')).some((g) => g.title === 'Feature model look'), false);
}));

// Intuition: the forest plugin's rows rewrite the tree as the forest widget emits it; the table plugin's rows name
// the column's letters. Pins the menus moved from the playground losing a row.
test('forest and table rows', withStore(() => {
  const F = self.satexViews.forest, body = '\n[Car [Engine, circle] [Body]]\n', t = F.parse(body), n = t.root.children[0];
  const text = `\\begin{forest}${body}\\end{forest}`, f = { from: 14, to: 14 + body.length, body, t, n };
  const c = ctxOf(text), gs = PA.groups({ kind: 'forest-node', f }, c);
  assert.deepEqual(gs.flatMap((g) => g.items.map((i) => i.label + (i.on ? '*' : ''))), ['Rectangle', 'Rounded rectangle', 'Circle*', 'Frame', 'Add child', 'Add sibling', 'Delete', 'Edit text…',
    'Leaves Car at', 'Arrives at Engine at', 'Add label…', 'Style level 1…', 'Transform to feature model']);
  gs[2].items[0].run();
  assert.match(edit(text, c.writes[0]), /\[Engine, circle\s*\[new\]\]/);
  // the edge's anchors from the run's list, on the parent and the child
  const e = ctxOf(text);
  e.facts = { anchors: ['north', 'east'] };
  const t2 = F.parse(body), edge = PA.groups({ kind: 'forest-node', f: { ...f, t: t2, n: t2.root.children[0] } }, e).find((g) => g.title === 'Edge from parent').items;
  edge[0].items.find((i) => i.label === 'east').run();
  assert.match(edit(text, e.writes[0]), /\[Car, parent anchor=east/);
  const col = PA.groups({ kind: 'table-column', t: 0, k: 1, col: { spec: [1, 2], align: 'r' } }, c);
  assert.deepEqual(col[0].items.map((i) => i.label[0] + (i.on ? '*' : '')), ['l', 'c', 'r*']);
  col[0].items[1].run();
  assert.deepEqual(c.writes.at(-1), [{ col: [0, 1, 'c'] }]);
  // widths: a preset makes the column a paragraph `p{…}`, natural width keeps its letter
  assert.deepEqual(col[1].items.map((i) => i.label + (i.on ? '*' : '')).slice(0, 2), ['natural (as wide as its text)*', '2cm']);
  col[1].items[2].run();
  assert.deepEqual(c.writes.at(-1), [{ col: [0, 1, 'r', '3cm'] }], 'an r column given a width stays right (respec)');
  assert.deepEqual(PA.groups({ kind: 'table-column', t: 0, k: 0, col: { align: 'X' } }, c), [], 'no letter to rewrite: no menu');
}));

// Intuition: a paragraph column (p, m, b, X) is aligned horizontally by the declaration its `>{…}` puts first, one
// table (pdfedit.ts HALIGN) read for the checked row and written by the one spec rewrite (respec) the width path uses,
// other `>{…}` kept, array loaded. Pins the p column menu offering only top/middle/bottom.
test('paragraph column alignment', withStore(() => {
  const E = self.satexPdfEdit, c = ctxOf('x');
  // the whole spec and its package; its parts make it up
  const rs = (...a) => { const r = E.respec(...a); assert.ok(!r || r.body === r.pre + r.letter); return r && { body: r.body, array: r.array }; };
  const menu = (text, align, width) => PA.groups({ kind: 'table-column', t: 0, k: 2, col: { spec: { text }, align, width } }, c);
  const rows = (gs) => gs.map((g) => `${g.title}: ${g.items.filter((i) => i.on).map((i) => i.label).join()}`);
  assert.deepEqual(rows(menu('p{3cm}', 'p', '3cm')), ['Align: p: top (paragraph)', 'Paragraph: justified', 'Width: 3cm']);
  assert.deepEqual(rows(menu('>{\\raggedleft\\arraybackslash}p{3cm}', 'p', '3cm')).slice(1, 2), ['Paragraph: right (\\raggedleft)']);
  assert.deepEqual(rows(menu('>{\\centering}X', 'X')), ['Paragraph: centered (\\centering)'], 'X: only the paragraph alignment');
  menu('m{2cm}', 'm', '2cm')[1].items[1].run();
  assert.deepEqual(c.writes.at(-1), [{ col: [0, 2, 'm', '2cm', 'l'] }]);
  menu('>{\\raggedleft\\arraybackslash}p{3cm}', 'p', '3cm')[2].items[0].run();
  assert.deepEqual(c.writes.at(-1), [{ col: [0, 2, 'r', ''] }], 'natural width: the letter of its alignment');
  // the spec rewrite: each alignment, other `>{…}` kept, back to justified, to natural, array where `>` or m/b
  assert.equal(E.halign('>{\\bfseries\\centering\\arraybackslash}p{1cm}'), 'c');
  assert.equal(E.halign('>{\\centering}>{\\raggedright}p{1cm}'), 'l', 'the last declaration wins');
  assert.deepEqual(rs('m', '2cm', 'm{2cm}', 'l'), { body: '>{\\raggedright\\arraybackslash}m{2cm}', array: true });
  assert.deepEqual(rs('p', '3cm', '>{\\bfseries}>{\\raggedleft\\arraybackslash}p{3cm}', 'c'), { body: '>{\\centering\\arraybackslash}>{\\bfseries}p{3cm}', array: true });
  assert.deepEqual(rs('p', '3cm', '>{\\raggedleft\\arraybackslash}p{3cm}', 'j'), { body: 'p{3cm}', array: false });
  assert.deepEqual(rs('p', '3cm', '>{\\raggedleft\\arraybackslash} p{3cm}'), { body: '>{\\raggedleft\\arraybackslash}p{3cm}', array: true }, 'no h: kept');
  assert.deepEqual(rs('r', null, '>{\\raggedleft\\arraybackslash}p{3cm}'), { body: 'r', array: false });
  assert.deepEqual(rs('X', null, 'X', 'r'), { body: '>{\\raggedleft\\arraybackslash}X', array: true });
  assert.equal(E.respec('p', null, 'p{1cm}'), null, 'a paragraph takes a width');
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
  assert.deepEqual(PA.plugins.map((p) => p.name), ['tikz-node', 'tikz-label', 'tikz-edge', 'tikz-bend', 'tikz-grid', 'forest-node', 'table-column', 'slice', 'refactor-float', 'refactor-math', 'refactor-extract', 'format', 'insert']);
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

// Intuition: a write follows the picture's own conventions: its unit and decimals, bare numbers in its x/y units, its
// scale (and `transform shape` for distances); a text without any falls back to `conventions.lengths`. Pins a new `at`
// written in cm into an mm picture, a 2 cm drag of a `scale=2` picture written as 2 units, a table width in cm in an mm document.
test('length conventions: the picture\'s units, scale and decimals; the document\'s for tables', () => {
  const pic = (opts, body) => T.nodes(`\\begin{tikzpicture}${opts}\n${body}\n\\end{tikzpicture}`);
  assert.deepEqual(T.convention('\\draw (0,0) -- (1.5,2); \\node at (3,1) {x};', true), { unit: '', dec: 1 });
  assert.deepEqual(T.convention('\\node at (10mm,5mm) {a}; \\node[xshift=2mm] {b}; % 3cm', true), { unit: 'mm', dec: 0 });
  assert.deepEqual(T.convention('\\setlength{\\parindent}{1.25cm} p{3.5cm}'), { unit: 'cm', dec: 2 });
  assert.deepEqual(T.convention('no lengths'), { unit: 'cm', dec: 1 }, 'the fallback');
  T.setConvention({ unit: 'mm', dec: 0 });
  try { assert.deepEqual(T.convention('no lengths'), { unit: 'mm', dec: 0 }, 'conventions.lengths'); } finally { T.setConvention(null); }
  // an mm picture: a node placed anew gets its `at` in mm; a new shift too
  const mm = pic('', '\\node (a) at (10mm,5mm) {A};\n\\node[right=of a] (b) {B};');
  assert.equal(T.move('\\node[right=of a] (b) {B};', 0, mm[1].pic, { to: [2, -1] }).text, '\\node (b) at (20mm,-10mm) {B};');
  assert.equal(T.move('\\node[right=of a] (b) {B};', 0, mm[1].pic, { by: [0.5, 0] }).text, '\\node[right=of a, xshift=5mm] (b) {B};');
  // bare coordinates with x=2cm: a node placed anew stays in the picture's units
  const xy = pic('[x=2cm, y=2cm]', '\\node (a) at (1,1) {A};\n\\node[right=of a] (b) {B};');
  assert.equal(T.move('\\node[right=of a] (b) {B};', 0, xy[1].pic, { to: [3, 1] }).text, '\\node (b) at (1.5,0.5) {B};');
  // a scale=2 picture: a drag of 2 cm on the page is 1 unit of the picture
  const sc = pic('[scale=2]', '\\node (a) at (1,1) {A};');
  assert.equal(T.move('\\node (a) at (1,1) {A};', 0, sc[0].pic, { by: [2, 0] }).text, '\\node (a) at (2,1) {A};');
  // positioning gaps: in the convention's unit and decimals, divided by the scale under `transform shape`
  const cm = 72 / 2.54, box = (l, w = cm) => ({ l, t: 0, r: l + w, d: cm / 2 }), a = box(0);
  assert.equal(T.relation(box(2.5 * cm), a, 0.5, [1, 1], 'a', { unit: 'mm', dec: 0 }), 'right=15mm of a');
  assert.equal(T.relation(box(2.5 * cm), a, 0.5, [1, 1], 'a', { unit: 'cm', dec: 1 }, 2), 'right=0.8cm of a', '1.5 cm on the page at scale 2');
  assert.deepEqual(pic('[scale=2, transform shape]', '\\node {A};')[0].pic.shape, true);
  // a table column's first width: the document's unit
  const E = self.satexPdfEdit;
  assert.equal(E.snapWidth(null, 50, 72 / 25.4 * 32, 0, false, { unit: 'mm', dec: 0 }), '32mm');
  assert.match(E.snapWidth(null, 50, 72 / 2.54 * 3.2, 0, false, { unit: 'cm', dec: 1 }), /^3\.2\d*cm$/);
});

// Intuition: the drawing canvas writes its distances and shifts in the picture's convention, the one resolver every
// length-writing widget asks; without one it writes cm as before. Pins a canvas that wrote `3cm` into an mm picture.
test('the drawing canvas writes in the picture\'s convention', () => {
  const D = self.satexViews.drawing, size = () => [1, 0.6];
  const env = (by, conv) => ({ by: new Map(by.map((n) => [n.name, n])), earlier: [], size, distance: [1, 1], onGrid: false, grid: 0.5, conv });
  const [na, , nc] = D.parse('\n  \\node (a) at (0,0) {A};\n  \\node (b) at (0,-3) {B};\n  \\node (c) [right=of a] {C};\n').items;
  const mm = T.convention('\\node (a) at (10mm,0mm) {A}; \\draw (0mm,0mm) -- (5mm,0mm);');
  assert.deepEqual(D.relocate(nc, 4.1, 0.1, env([na], mm)).opts, ['right=30mm of a']);
  assert.deepEqual(D.relocate(nc, 2, 0.5, env([na], mm)).opts, ['right=of a', 'yshift=5mm']);
  assert.deepEqual(D.relocate(nc, 4.1, 0.1, env([na], null)).opts, ['right=3cm of a'], 'no convention: cm');
  assert.equal(T.written(1.25, { unit: 'in', dec: 2 }), '0.49in');
  assert.equal(T.convention('no lengths', false, true), null, 'strict: none');
});

// Scratch slices (actions.ts `satexScratch`): a slice's copy of the chosen lines is replaced by those lines between two
// markers, and what stands between them after the edits maps back to the lines they came from.
const SC = self.satexScratch;
const CHAPTER = ['\\section{A}', 'Text.', '', '\\begin{figure}[ht]', '  \\centering', '  \\includegraphics{x}', '  \\caption{Old}\\label{f}', '\\end{figure}', '', 'More.'].join('\n');
const sliceOf = (body) => ['\\documentclass{article}', '\\usepackage{graphicx}', '\\begin{document}', '%% from chapter.tex', ...body, '\\end{document}', ''].join('\n');

// Intuition: the region is cut from the slice and read back whole, the slice's other lines around it untouched. Pins a
// region matched at a `\centering` of another figure before it, and a comment line the slicer dropped that is lost.
test('scratch: build puts the region between markers, extract reads it back', () => {
  const slice = sliceOf(['\\begin{table}', '  \\centering', '\\end{table}', '\\begin{figure}[ht]', '  \\centering', '  \\includegraphics{x}', '  \\caption{Old}\\label{f}', '\\end{figure}']);
  const text = SC.build(slice, 'chapter.tex', CHAPTER, 4, 8, false);
  const r = SC.extract(text);
  assert.deepEqual([r.file, r.from, r.to], ['chapter.tex', 4, 8]);
  assert.equal(r.body, CHAPTER.split('\n').slice(3, 8).join('\n'));
  assert.match(text, /\\begin\{table\}\n {2}\\centering\n\\end\{table\}\n%\\satex@scratch\{begin/, 'the table before stays whole');
  assert.match(text, /%\\satex@scratch\{end\}\n\\end\{document\}\n$/);
  assert.equal(text.match(/\\begin\{figure\}/g).length, 1, 'the slice\'s copy is gone');
  // a dropped comment line of the region comes back with it
  const commented = CHAPTER.replace('  \\centering', '  % keep\n  \\centering');
  assert.match(SC.extract(SC.build(slice, 'chapter.tex', commented, 4, 9, false)).body, /% keep/);
});

// Intuition: a file the slice reads whole stays an `\input`; its lines are then written out in its place, so the region
// can be marked. A slice with none of the lines and no load maps nowhere: null, never a guess.
test('scratch: a whole input is written out; no lines, no scratch', () => {
  const slice = ['\\documentclass{article}', '\\begin{document}', '\\input{chapter}', '\\end{document}'].join('\n');
  const text = SC.build(slice, 'chapter.tex', CHAPTER, 7, 7, false);
  assert.ok(!/\\input\{chapter\}/.test(text));
  assert.equal(SC.extract(text).body, '  \\caption{Old}\\label{f}');
  assert.ok(text.includes('More.'), 'the rest of the file runs too');
  // a standalone slice carries the file as filecontents*: written out in place, that copy goes
  const standalone = ['\\begin{filecontents*}[overwrite]{chapter.tex}', ...CHAPTER.split('\n'), '\\end{filecontents*}', slice].join('\n');
  const s2 = SC.build(standalone, 'chapter.tex', CHAPTER, 7, 7, false);
  assert.ok(!/filecontents/.test(s2), s2);
  assert.equal(SC.extract(s2).body, '  \\caption{Old}\\label{f}');
  assert.equal(SC.build(sliceOf(['Other.']), 'chapter.tex', CHAPTER, 7, 7, false), null);
  // the root's own lines need no header
  assert.equal(SC.extract(SC.build('\\documentclass{article}\nText.\n\\begin{document}\nText.\n\\end{document}\n', 'main.tex', 'x\nText.', 2, 2, true)).body, 'Text.');
});

// Intuition: three-way: base (the lines when sliced), mine (the scratch region), the file now. Unchanged there: clean;
// moved by lines added above: written where they now stand; changed there: a conflict with the lines that stand there.
test('scratch: merge maps the edit back, follows a move, reports a conflict', () => {
  const base = SC.extract(SC.build(sliceOf(CHAPTER.split('\n').slice(3, 8)), 'chapter.tex', CHAPTER, 4, 8, false)).body, mine = base.replace('Old', 'New');
  const size = CHAPTER.split('\n').length;
  assert.equal(SC.merge(base, base, CHAPTER, 4, 8, size).status, 'same');
  const m = SC.merge(base, mine, CHAPTER, 4, 8, size);
  assert.deepEqual([m.status, m.from, m.to], ['clean', 4, 8]);
  assert.equal(SC.apply(CHAPTER, m.from, m.to, mine), CHAPTER.replace('Old', 'New'));
  const grown = `Intro.\n\n${CHAPTER}`, g = SC.merge(base, mine, grown, 4, 8, size);
  assert.deepEqual([g.status, g.from, g.to], ['moved', 6, 10]);
  assert.equal(SC.apply(grown, g.from, g.to, mine), grown.replace('Old', 'New'));
  const theirs = CHAPTER.replace('Old', 'Theirs').replace('  \\centering', '  \\centering\n  \\small'), c = SC.merge(base, mine, theirs, 4, 8, size);
  assert.deepEqual([c.status, c.from, c.to], ['conflict', 4, 9]);
  assert.match(c.theirs, /Theirs/);
  assert.equal(SC.apply(theirs, c.from, c.to, mine), CHAPTER.replace('Old', 'New'), 'taking the scratch writes over the lines that stand there');
});

// Intuition: a right click on the page names a block: the figure or table around the line, else the display, else the
// paragraph, cut at blank lines, headings and page breaks. Pins a caption line that sliced only itself, an equation inside
// a figure taken for the figure's, and a thesis body without blank lines sliced whole from one click.
test('scratch: block finds the figure, the equation, the paragraph', () => {
  assert.deepEqual(SC.block(CHAPTER, 7), [4, 8, 'figure']);
  assert.deepEqual(SC.block(CHAPTER, 2), [2, 2, 'paragraph'], 'the heading above is no part of it');
  assert.deepEqual(SC.block(CHAPTER, 1), [1, 1, 'heading']);
  assert.deepEqual(SC.block('\\begin{document}\n\\maketitle\nA\nB\n\\clearpage\nC', 3), [3, 4, 'paragraph'], 'a body without blank lines is cut at its commands');
  assert.deepEqual(SC.block(CHAPTER, 10), [10, 10, 'paragraph']);
  // sweep 2026-11-19: a picture on lines of its own was offered as "this paragraph"
  assert.deepEqual(SC.block('A\n\n\\begin{tikzpicture}\n\\node {x};\n\\end{tikzpicture}\n\nB', 4), [3, 5, 'tikzpicture'], 'a paragraph that is one environment is named by it');
  assert.deepEqual(SC.block('A\n\\begin{itemize}\n\\item x\n\\end{itemize}\n\nB', 3), [1, 4, 'paragraph'], 'text around it keeps it a paragraph');
  assert.deepEqual(SC.block('a\n\\begin{equation}\n x % \\end{equation}\n\\end{equation}\nb', 3), [2, 4, 'equation']);
  assert.deepEqual(SC.block('\\begin{table*}\n\\begin{equation}\nx\n\\end{equation}\n\\end{table*}', 3), [1, 5, 'table']);
});

// Intuition: the local scratch keeps the root's preamble as written (the run resumes from its checkpoint), only the body
// definitions the region names (and those they name in turn, whole over their lines), then the region. Pins a scratch
// that carried the whole thesis (60 s first check) and a body `\newcommand` the region needs left out.
test('scratch: local keeps the preamble, the definitions the region needs, the region', () => {
  const root = ['\\documentclass{article}', '\\usepackage{tikz}', '\\begin{document}', '\\newcommand\\inner{x}', '\\newcommand\\outer{%', '  \\inner y}', '\\newcommand\\unused{z}', '\\input{chapter}', '\\end{document}'].join('\n');
  const texts = { 'main.tex': root, 'chapter.tex': CHAPTER.replace('Old', 'Old \\outer') };
  const defs = [{ file: 'main.tex', line: 4, name: '\\inner' }, { file: 'main.tex', line: 5, name: '\\outer' }, { file: 'main.tex', line: 7, name: '\\unused' },
    { file: 'main.tex', line: 2, name: '\\tikz' }, { file: 'chapter.tex', line: 7, name: '\\caption@x' }];
  const text = SC.local(texts, 'main.tex', 'chapter.tex', 4, 8, defs);
  assert.ok(text.startsWith('\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n'), 'the preamble as written');
  assert.match(text, /\\newcommand\\outer\{%\n {2}\\inner y\}/, 'the definition whole over its lines');
  assert.match(text, /\\newcommand\\inner\{x\}/, 'what the kept definition names');
  assert.ok(!/unused|\\input|Text\.|More\./.test(text), text);
  assert.equal(SC.extract(text).body, texts['chapter.tex'].split('\n').slice(3, 8).join('\n'));
  assert.match(text, /%\\satex@scratch\{end\}\n\\end\{document\}\n$/);
  assert.equal(SC.local({ 'a.tex': 'x' }, 'a.tex', 'a.tex', 1, 1, []), null, 'no document: no local scratch');
});

// Intuition: a table's column spacing is \tabcolsep (an array's \arraycolsep) set right before its \begin, kept local:
// inside the environment around the table, else in a group written around it, as a group undoes a declaration at its
// end (tex.web § 282). The current value reads back from that setting, else the run's; default drops it, giving the
// text back as it was. Pins a spacing menu that leaks the setting into the rest of the document or reads a fixed 6pt.
test('table column spacing', withStore(() => {
  const pos = (text, o) => ({ line: text.slice(0, o).split('\n').length - 1, character: o - text.lastIndexOf('\n', o - 1) - 1 });
  const menu = (text, env = 'tabular', seps = { tabcolsep: 5, arraycolsep: 2.5 }) => {
    const c = ctxOf(text), o = text.indexOf('{ll}') + 1;
    const gs = PA.groups({ kind: 'table-column', t: 0, k: 0, env, seps, col: { spec: { text: 'l', range: { start: pos(text, o) } }, align: 'l' } }, c);
    const g = gs.find((g) => g.title === 'Spacing');
    return { g, set: (label) => { g.items.find((i) => i.label.endsWith(label)).run(); return edit(text, c.writes.at(-1)); } };
  };
  const table = '\\begin{table}\n  \\centering\n  \\begin{tabular}{ll}\n  a & b \\\\\n  \\end{tabular}\n\\end{table}\n';
  const set = menu(table).set('4pt');
  assert.equal(set, table.replace('  \\begin{tabular}', '  \\setlength{\\tabcolsep}{4pt}\n  \\begin{tabular}'), 'inside the table environment: a line before');
  assert.deepEqual(menu(set).g.items.filter((i) => i.on).map((i) => i.label), ['\\tabcolsep 4pt'], 'read back');
  assert.equal(menu(set).set('8pt'), set.replace('{4pt}', '{8pt}'), 'rewritten in place');
  assert.equal(menu(set).set(')'), table, 'default: as it was');
  assert.equal(menu(table).g.items[0].label, 'default (5pt)', "the run's value");
  // mutations: in the document body a group around the table, the outer \end past a nested table, an array's own
  const body = '\\begin{document}\nx\n\\begin{tabular}{ll}\na & \\begin{tabular}{l}y\\end{tabular} \\\\\n\\end{tabular}\nz\n\\end{document}\n';
  const wrapped = menu(body).set('0pt');
  assert.equal(wrapped, body.replace('\\begin{tabular}{ll}', '{\\setlength{\\tabcolsep}{0pt}%\n\\begin{tabular}{ll}').replace('\\end{tabular}\nz', '\\end{tabular}}\nz'));
  assert.equal(menu(wrapped).set('2pt'), wrapped.replace('{0pt}', '{2pt}'));
  assert.equal(menu(wrapped).set(')'), body, 'default: the group goes too');
  const arr = '\\[\\begin{array}{ll} a & b \\end{array}\\]';
  assert.equal(menu(arr, 'array').g.items[0].label, 'default (2.5pt)');
  assert.match(menu(arr, 'array').set('4pt'), /\{\\setlength\{\\arraycolsep\}\{4pt\}%\n\\begin\{array\}/);
  // a table without its \end (typed half-way): no place for the group's `}`, so no spacing is offered rather than a
  // \setlength at document level that every later table takes
  const open = '\\begin{document}\nx\n\\begin{tabular}{ll}\na & b \\\\\nz\n\\end{document}\n';
  assert.equal(menu(open).g, undefined, 'no \\end: no spacing menu');
  assert.equal(self.satexPdfEdit.padding(open, open.indexOf('\\begin{tabular}'), 'tabular'), null, 'no \\end: no local padding');
}));
