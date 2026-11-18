// Word and offset mapping of Type mode (pdfedit.ts): source words to page words, paragraphs, carets between glyphs,
// contents entries and their titles, renamed names, leaders, column specs and widths. Properties over seeded inputs.
import assert from 'node:assert/strict';
import { test, rng, apply } from './harness.mjs';
import { source } from './gen.mjs';

const E = self.satexPdfEdit, F = self.satexViews.forest;
const DOCS = Array.from({ length: 30 }, (_, k) => source(rng(500 + k)));

// Intuition: the alignment is a monotone partial matching: each source word maps to one page word or none, in reading
// order, and an identical page maps one to one. Pins a matching that crossed (a caret drawn on an earlier word).
test('aligned: monotone, in range, identity on equal lists', () => {
  const R = rng(11), W = ['a', 'lorem', 'ipsum', 'dolor', 'sit', 'consec-', 'tetur', 'here.', '1', 'x'];
  for (let k = 0; k < 400; k++) {
    const toks = Array.from({ length: R.int(12) }, () => R.pick(W)), words = toks.filter(() => R.chance(0.8));
    for (let j = 0; j < R.int(3); j++) words.splice(R.int(words.length + 1), 0, R.pick(W));
    const out = E.aligned(toks, words), got = out.filter((j) => j >= 0);
    assert.equal(out.length, toks.length);
    assert.ok(got.every((j, i) => j < words.length && (i === 0 || j > got[i - 1])), `${toks} | ${words} -> ${out}`);
    assert.deepEqual(E.aligned(toks, toks), toks.map((_, i) => i));
  }
});

// Intuition: `changed` gives the longest common start and then the longest common end that does not overlap it, so a
// typed run is found exactly where it went. Pins an end that overlapped the start (`aaa` → `aaaa`).
test('changed: the longest common start and end, never overlapping', () => {
  const R = rng(12);
  for (let k = 0; k < 500; k++) {
    const was = Array.from({ length: R.int(8) }, () => R.pick('ab')).join(''), a = R.int(was.length + 1), b = Math.min(was.length, a + R.int(3));
    const now = apply(was, [a, b, Array.from({ length: R.int(3) }, () => R.pick('ab')).join('')]);
    const [p, q] = E.changed(was, now), m = Math.min(was.length, now.length);
    assert.ok(p + q <= m);
    assert.equal(was.slice(0, p), now.slice(0, p));
    assert.equal(was.slice(was.length - q), now.slice(now.length - q));
    assert.ok(p === m || was[p] !== now[p], 'the start is longest');
    assert.ok(p + q === m || was[was.length - 1 - q] !== now[now.length - 1 - q], 'the end is longest');
  }
});

// Intuition: the paragraph around an offset holds it and holds no blank line; a heading or an environment's edge bounds
// it. Pins a caption's paragraph that ran into the text above its float.
test('paragraph: holds the offset, no blank line inside', () => {
  const R = rng(13);
  for (const doc of DOCS) for (let k = 0; k < 40; k++) {
    const at = R.int(doc.length + 1), [p0, p1] = E.paragraph(doc, at);
    if (/^[ \t]*\n/.test(doc.slice(at)) && /\n[ \t]*$/.test(doc.slice(0, at))) continue; // on a blank line itself
    assert.ok(0 <= p0 && p0 <= at && at <= p1 && p1 <= doc.length, `${p0} ${at} ${p1}`);
    assert.ok(!/\n[ \t]*\n/.test(doc.slice(p0, p1)), JSON.stringify(doc.slice(p0, p1)));
  }
  const t = '\\section{Intro}\nText one.\n\\begin{figure}\n\\caption{Cap}\n\\end{figure}\nAfter.';
  assert.equal(t.slice(...E.paragraph(t, t.indexOf('one'))), 'Text one.', 'between a heading and a float');
  assert.equal(t.slice(...E.paragraph(t, t.indexOf('Intro'))), '\\section{Intro}', 'a heading is its own');
});

// Intuition: source words come in order, inside the span asked for, never overlapping, each setting some text and no
// command name. Pins words that spelled `emph` (a command's name taken for a word).
test('tokens: ordered, in range, no command names', () => {
  for (const doc of DOCS) {
    const ts = E.tokens(doc, 0, doc.length);
    ts.forEach((t, i) => {
      assert.ok(0 <= t.s && t.s < t.e && t.e <= doc.length && (i === 0 || ts[i - 1].e <= t.s));
      assert.ok(t.t && !/[\\{}$\s]/.test(t.t), JSON.stringify(t));
    });
    assert.ok(!ts.some((t) => /^(emph|textbf|bfseries|footnote|section|begin|end|item)$/.test(t.t)), 'no command names');
  }
  // a label's key and a short title set no word on the page here
  assert.deepEqual(E.tokens('\\label{k} a \\section[S]{L}', 0, 26).map((t) => t.t), ['a', 'L']);
});

// Intuition: caret boundaries run from the word's left to its right, never back, one more than characters; a click
// goes to the nearest. Pins a caret drawn left of the one before it.
test('boundaries: monotone from left to right, a click to the nearest', () => {
  const R = rng(14), width = (s) => [...s].reduce((n, c) => n + (c === 'm' ? 3 : c === 'i' ? 0.5 : 1), 0);
  for (let k = 0; k < 200; k++) {
    const w = Array.from({ length: 1 + R.int(8) }, () => R.pick('mixa')).join(''), l = R.int(100), r = l + 1 + R.int(80);
    const xs = E.boundaries(w, l, r, width);
    assert.equal(xs.length, w.length + 1);
    assert.ok(Math.abs(xs[0] - l) < 1e-9 && Math.abs(xs.at(-1) - r) < 1e-9);
    assert.ok(xs.every((x, i) => i === 0 || x >= xs[i - 1]));
    const x = l + R.float() * (r - l), b = E.boundaryAt(xs, x);
    assert.ok(xs.every((y) => Math.abs(y - x) >= Math.abs(xs[b] - x)));
    const ink = Array.from({ length: 30 }, () => R.int(3)), reach = 1 + R.int(4), at = R.float() * 30;
    const s = E.inkSnap(at, ink, reach);
    assert.ok(s === at || Math.abs(s - at) <= reach + 0.5, `snapped ${at} to ${s} beyond ${reach}`);
  }
});

// Intuition: what `locate` finds spells the row's characters it was asked for, word by word. Pins a tint on the
// document's own `!` instead of the macro's.
test('locate: the run found spells the row', () => {
  const R = rng(15), W = ['Hello,', 'world!', 'x', 'ab', 'abc', 'the', 'cat'];
  for (let k = 0; k < 300; k++) {
    const texts = Array.from({ length: 1 + R.int(8) }, () => R.pick(W)), row = texts.join(''), a = R.int(row.length), b = a + R.int(Math.min(5, row.length - a));
    const got = E.locate(texts, row, a, b);
    if (got) assert.equal(got.map(([i, p, q]) => texts[i].slice(p, q)).join(''), row.slice(a, b + 1));
  }
});

// Intuition: the word at the caret holds it and is letters only; its spans in a line are whole words.
test('wordAt and spans: whole words around the caret', () => {
  const R = rng(16);
  for (const doc of DOCS.slice(0, 10)) for (let k = 0; k < 50; k++) {
    const at = R.int(doc.length + 1), w = E.wordAt(doc, at);
    if (!w) continue;
    assert.ok(w[0] <= at && at <= w[0] + w[1].length && /^[\p{L}\p{N}]+$/u.test(w[1]) && doc.startsWith(w[1], w[0]));
    for (const [p, q] of E.spans(doc, w[1])) assert.equal(doc.slice(p, q), w[1]);
  }
});

// Intuition: forest sets its nodes level by level at the tree's `\end`, so `treeNodes` lists the node texts in that
// order: checked against the forest model's own parse of random trees. Pins typing into the wrong node of a tree.
test('treeNodes: the forest model\'s nodes, level by level', () => {
  const R = rng(17), W = ['A', 'B', 'NP', 'VP', '{x, y}', '$x$', 'the cat'];
  for (let k = 0; k < 100; k++) {
    const t = F.parse('\n[R]\n'), all = [t.root];
    for (let j = 0; j < 1 + R.int(8); j++) { const c = F.addChild(R.pick(all)); c.content = R.pick(W); all.push(c); }
    F.settle(t);
    const body = F.emit(t), doc = `x\n\\begin{forest}${body}\\end{forest}\n`;
    const bfs = [], q = [[t.root, 0]];
    const levels = [];
    const walk = (n, d) => { (levels[d] ??= []).push(n.content.replace(/^\{(.*)\}$/, '$1')); n.children.forEach((c) => walk(c, d + 1)); };
    walk(t.root, 0);
    bfs.push(...levels.flat());
    assert.deepEqual(E.treeNodes(doc, doc.indexOf('\\end{forest}')).map(([a, b]) => doc.slice(a, b)), bfs, body);
    void q;
  }
});

// Intuition: a short title written for a heading and then made the heading's title gives the heading back as it was,
// for any title (a `]` or braces in it included). Pins a copy that ended the option early at a `]`.
test('shortTitle and bothTitles round-trip', () => {
  const R = rng(18), parts = ['A', ' b', '[x]', '{y}', ']', '\\emph{z}', ' and'];
  for (let k = 0; k < 200; k++) {
    const title = Array.from({ length: 1 + R.int(4) }, () => R.pick(parts)).join(''), cmd = R.pick(['section', 'subsection', 'caption', 'chapter']);
    const text = `x \\${cmd}{${title}} y`, head = 2, st = E.shortTitle(text, head, R.int(title.length + 1));
    assert.ok(st, text);
    const two = apply(text, st);
    assert.equal(E.shortTitle(two, head, 0), null, 'one short title only');
    assert.ok(st[0] + st[3] >= st[0] && st[0] + st[3] <= st[0] + st[2].length, 'the caret in the copy');
    assert.equal(apply(two, E.bothTitles(two, head)), text, two);
    // the contents entry is the short title just written
    const [[, a, b]] = E.entries({ 'm.tex': two }, 'm.tex', cmd === 'caption' ? 'none' : 'toc').concat([[0, 0, 0]]);
    if (cmd !== 'caption') assert.equal(two.slice(a, b), st[2].slice(1, -1));
  }
});

// Intuition: contents entries follow `\input` and `\include` in document order, once per file, starred headings and
// commented or verbatim ones left out. Pins a toc whose entries came per file, not in reading order.
test('entries: reading order across files', () => {
  const texts = {
    'main.tex': '\\section{One}\n\\input{a}\n% \\section{Gone}\n\\section*{Star}\n\\include{b}\n\\section{Four}\n\\input{a}',
    'a.tex': '\\subsection{Two}\n\\begin{table}\\caption{T}\\end{table}', 'b.tex': '\\section[Short]{Three}\n\\verb|\\section{v}|',
  };
  const got = (k) => E.entries(texts, 'main.tex', k).map(([f, a, b]) => `${f}:${texts[f].slice(a, b)}`);
  assert.deepEqual(got('toc'), ['main.tex:One', 'a.tex:Two', 'b.tex:Short', 'main.tex:Four'], 'a file read twice lists once');
  assert.deepEqual(got('lot'), ['a.tex:T']);
  assert.deepEqual(E.entries(texts, 'missing.tex', 'toc'), []);
});

// Intuition: renaming a generated name twice with one value changes nothing more; with another only the value. Pins
// a second `\renewcommand` line added for each rename.
test('renamed and leaders: idempotent, one line a setting', () => {
  const pre = '\\documentclass{article}\n\\begin{document}\nx\n\\end{document}';
  for (const lang of ['', 'ngerman']) for (const name of ['\\contentsname', '\\listfigurename', '\\figurename']) {
    const once = apply(pre, E.renamed(pre, name, 'V1', lang)), twice = apply(once, E.renamed(once, name, 'V1', lang));
    assert.equal(twice, once, `${name} ${lang}`);
    const other = apply(once, E.renamed(once, name, 'V2', lang));
    assert.equal(other.replace('V2', 'V1'), once);
  }
  for (const kind of ['toc', 'lof', 'lot']) for (const tocloft of [false, true]) for (const what of ['none', '3', 'nopage']) {
    const e = E.leaders(pre, kind, what, tocloft, true);
    if (!e) continue;
    const once = apply(pre, e), again = E.leaders(once, kind, what, tocloft, true);
    assert.equal(apply(once, again), once, `${kind} ${what} ${tocloft}`);
    assert.equal(once.split('\n').length, pre.split('\n').length + 1, 'one line');
  }
});

// Intuition: a column given a width keeps its alignment, and needs array exactly when it writes array's syntax.
test('respec: alignment kept, array when used', () => {
  const T = self.satexDialogs.table;
  for (const a of 'lcrpmbXS') for (const w of ['3cm', '.3\\linewidth']) {
    const r = E.respec(a, w);
    if ('XS'.includes(a)) { assert.equal(r, null); continue; }
    assert.ok(r.body.endsWith(`{${w}}`));
    assert.equal(r.array, /[>]|^[mb]/.test(r.body), r.body);
    assert.equal(T.alignOf(r.body), 'lcr'.includes(a) ? 'p' : a);
  }
});

// Intuition: a drag writes a width the form reader takes back, within half a snap step of where it was dropped (0.5 mm,
// or a twentieth of the line when near one); Shift is finer. Pins drags that wrote `1.7999999cm` or jumped a step.
test('snapWidth and resized: snapped, readable, near the drop', () => {
  const R = rng(19), MM = 72 / 25.4, line = 345;
  for (let k = 0; k < 500; k++) {
    const now = 5 + R.float() * 400, fine = R.chance(0.3), was = R.pick([null, '20mm', '3cm', '.3\\linewidth', '2in']);
    const out = E.snapWidth(was, 50, now, line, fine);
    assert.ok(out && E.form(out), `${was} -> ${out}`);
    const bp = E.widthBp(out, line);
    if (was == null || /cm|mm/.test(was)) assert.ok(Math.abs(bp - now) <= (fine ? 0.05 : 0.5) * MM + 1e-6, `${now} -> ${out} (${bp})`);
    if (was && !/linewidth/.test(out)) assert.equal(E.form(out).unit ?? '', E.form(was).unit ?? '', 'the unit kept');
  }
  for (const w of ['3cm', '30mm', '.3\\linewidth', '2in', '12pt']) {
    const up = E.stepped(w, 1, false), down = E.stepped(w, -1, false);
    assert.ok(E.widthBp(up, line) > E.widthBp(w, line) && E.widthBp(down, line) < E.widthBp(w, line), `${down} < ${w} < ${up}`);
    assert.equal(E.stepped(E.stepped(w, 1, true), -1, true), w.replace(/^\./, '.'), 'a fine step there and back');
  }
});

// Intuition: a line break `\\` and a control space `\ ` set no word. Pins source words `\` the page never shows, which
// the aligner then had to skip (`\LaTeX\ ` read as the word `\`).
test('tokens: a line break and a control space set no word', () => {
  assert.deepEqual(E.tokens(String.raw`a \\ b\\ \LaTeX\ c`, 0, 99).map((t) => t.t), ['a', 'b', 'c']);
});
// Intuition: an option's `]` inside braces is text: the short title `[{a]b}]` ends at its own `]`. Pins the contents
// entry and the heading's title cut at the braced `]` (`\section{{}}]{…}` written back).
test('options: a braced `]` does not end the short title', () => {
  const t = '\\section[{a]b}]{Long}';
  assert.deepEqual(E.entries({ 'm.tex': t }, 'm.tex', 'toc').map(([, a, b]) => t.slice(a, b)), ['{a]b}']);
  assert.equal(apply(t, E.bothTitles(t, 0)), '\\section{a]b}');
  assert.equal(apply('\\section[{y}]{L}', E.bothTitles('\\section[{y}]{L}', 0)), '\\section{{y}}', 'braces no wrap needed stay');
});

// Intuition: an LSP position maps to an offset on its own line: a character past the line's end is its end (LSP's rule),
// never the next line's start. Pins a finding or caret past a shortened line drawn on the line below (playground.js
// `offsetIn`, moved here to be tested).
test('offsetAt: LSP positions clamp to their line', () => {
  const t = 'ab\ncde\n\nf';
  assert.deepEqual([[0, 0], [0, 2], [0, 9], [1, 1], [1, 3], [2, 0], [2, 5], [3, 1], [3, 9], [9, 0]].map(([line, character]) => E.offsetAt(t, { line, character })), [0, 2, 2, 4, 6, 7, 7, 9, 9, 9]);
  const R = rng(20);
  for (const doc of DOCS.slice(0, 10)) for (let k = 0; k < 50; k++) {
    const at = R.int(doc.length + 1), before = doc.slice(0, at), line = before.split('\n').length - 1, character = at - before.lastIndexOf('\n') - 1;
    assert.equal(E.offsetAt(doc, { line, character }), at, 'offset → position → offset');
  }
});
// Intuition: a view word is split where math meets text, the pieces' offsets in the text. Pins `x42Generally` taken
// as one word for a typed caret (playground.js `tsUnits`, moved here to be tested).
test('units: words split where math meets text', () => {
  const t = 'Let x42Generally go', math = (i) => i >= 4 && i < 7;
  assert.deepEqual(E.units(t, math).map((m) => [m[0], m.index]), [['Let', 0], ['x42', 4], ['Generally', 7], ['go', 17]]);
  assert.deepEqual(E.units('a b', () => false).map((m) => m[0]), ['a', 'b']);
});
