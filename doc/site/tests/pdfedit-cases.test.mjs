// Editing in the PDF (pdfedit.ts): fixtures. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// Editing in the PDF (pdfedit.ts): which SyncTeX boxes a column handle sits on, what a drag writes, where typing may go.
test("Editing in the PDF (pdfedit.ts)", async () => {
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
});
// Typing in a typeset view: the caret stays after each typed character. The view finds its word with wordAt in the
// text as now written; pins the bug where the caret jumped back to the word's start after each key ("lol" -> "lool").
test("Typing in a typeset view", async () => {
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
});
// "Type in the argument instead": the first argument of a use of the macro, on its line or a neighbour, never the
// definition's `\greet}`.
test("\"Type in the argument instead\"", async () => {
  const E = self.satexPdfEdit, text = '\\newcommand{\\greet}[1]{Hello, #1!}\n\\greet{world}\n\\greeting{x}\n';
  assert.equal(E.argAt(text, '\\greet', 2), text.indexOf('world'));
  assert.equal(E.argAt(text, '\\greet', 1), text.indexOf('world'), 'the next line');
  assert.equal(E.argAt(text, '\\greet', 3), text.indexOf('world'), 'not \\greeting');
  assert.equal(E.argAt(text, '\\nope', 2), null);
});
// An argument-less macro called as `\model{}` (ui-35): its `{}` is an empty group, no argument to type into, so the
// note offers no "Type in the argument instead"; one with parameters (`[1]`, `#1`, an xparse `{m}`) keeps it.
test("no argument to type into for a macro without parameters", async () => {
  const E = self.satexPdfEdit, use = '\n\\model{} runs.\n';
  for (const def of ['\\newcommand{\\model}{TinyNet}', '\\newcommand\\model[0]{TinyNet}', '\\def\\model{TinyNet}', '\\NewDocumentCommand{\\model}{}{TinyNet}'])
    assert.equal(E.argAt(def + use, '\\model', 2), null, def);
  for (const def of ['\\newcommand{\\model}[1]{Tiny#1}', '\\def\\model#1{Tiny#1}', '\\NewDocumentCommand{\\model}{m}{Tiny#1}', '\\newcommand{\\modelx}{X}', ''])
    assert.equal(E.argAt(def + use, '\\model', 2), (def + use).indexOf('} runs'), def || 'no definition: a package macro');
});
// Typing into an undecided choice (`The year ⟨?⟩ is [not] a leap year.`): the run reads `not` at the call
// `\tleapy`, so typing goes to the definition that spells it, not the empty one, never `\tleapyx`.
test("Typing into an undecided choice (`The year ⟨?⟩ is [not] a leap year.`)", async () => {
  const E = self.satexPdfEdit, text = '\\def\\tleapy{not }\n\\def\\tleapy{}\n\\def\\tleapyx{not}\nis \\tleapy a\n';
  const at = text.indexOf('\\tleapy a') + 3;
  assert.deepEqual(E.defining(text, at, 'not'), [['\\tleapy', text.indexOf('not'), 1]]);
  assert.deepEqual(E.defining(text, text.indexOf(' a\n'), 'not'), [], 'no call there');
});
// The typed word drawn at once over the page: it takes its measured width at the page's scale, the words after it move.
test("The typed word drawn at once over the page", async () => {
  const E = self.satexPdfEdit, width = (s) => s.length;
  // two units per character as set: one more character, two units more
  assert.deepEqual(E.relaid([[0, 12, 'Hello,'], [14, 26, 'world!']], 0, 'Hellmo,', width), [[0, 14, 'Hellmo,'], [16, 28, 'world!']]);
  assert.deepEqual(E.relaid([[0, 10, 'ab'], [12, 20, 'cd']], 1, 'cdd', width), [[0, 10, 'ab'], [12, 24, 'cdd']]);
});
// The caret on a page stands between two characters, never inside a glyph: boundaries spaced by the font's widths
// (a wide `m` takes more than an `i`), a click going to the nearest one, and the page's pixels moving each to the
// gap between the glyphs. Pins the caret drawn at an even share of the word, in the middle of a glyph.
test("The caret on a page stands between two characters, never inside a glyph", async () => {
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
});
// A tint on the PDF marks the glyphs a run of the row set, found by their place in the row: the `!` of
// `\greet{…}` (`Hello, #1!`) is the one after the argument, not the document's own `!` after `you`. Math the page
// writes otherwise (`−` for `-`) stays outside the context. Pins the macro tint on the document's last `!`.
test("A tint on the PDF marks the glyphs a run of the row set, found by their place in the row", async () => {
  const E = self.satexPdfEdit;
  const texts = ['Hello,', 'Walter!', 'x42', '+', '2', '−', '41Generally', 'I’d', 'say', 'you!'];
  const row = 'Hello,Walter!x42+2-41GenerallyI’dsayyou!', bang = row.indexOf('!');
  assert.deepEqual(E.locate(texts, row, bang, bang), [[1, 6, 7]], 'the macro\'s `!`, after Walter');
  assert.deepEqual(E.locate(texts, row, row.lastIndexOf('!'), row.lastIndexOf('!')), [[9, 3, 4]], 'the text\'s own `!`');
  assert.deepEqual(E.locate(texts, row, 0, bang), [[0, 0, 6], [1, 0, 7]], 'a run over two words');
  const g = row.indexOf('Generally');
  assert.deepEqual(E.locate(texts, row, g, g + 8), [[6, 2, 11]], 'after math the page spells otherwise');
  assert.equal(E.locate(['x!', 'x!'], 'x!', 1, 1), null, 'a row the page spells twice: no guess');
});
// The math control: one Insert menu per Type bar (inline, display and align*), inline in the convention's
// delimiters, nothing inside math. Pins the two `$x$`/`\[x\]` buttons inside the Type legend.
test("The math control", async () => {
  const E = self.satexPdfEdit;
  assert.deepEqual(E.mathAt('ab', 1, 'inline'), [1, 1, '\\(\\)', 2], 'LaTeX\'s \\( by default');
  assert.deepEqual(E.mathAt('ab', 1, 'inline', '$'), [1, 1, '$ $', 1], '`$ $`: `$$` is display math');
  assert.deepEqual(E.mathAt('ab', 1, 'display'), [1, 1, '\\[\\]', 2]);
  assert.deepEqual(E.mathAt('ab', 1, 'align'), [1, 1, '\\begin{align*}\n\n\\end{align*}', '\\begin{align*}\n'.length]);
  assert.deepEqual(E.mathAt('a\\(x\\)b', 4, 'display'), [4, 4, '', 0], 'in math: nothing');
  const html = readFileSync(new URL('./play.html', SITE), 'utf8');
  const groups = [...html.matchAll(/<span class="math-group"[^]*?<\/details><\/span>/g)].map((m) => m[0]);
  assert.equal(groups.length, 2, 'one in the Typeset bar, one in the PDF bar');
  for (const g of groups) assert.deepEqual([...g.matchAll(/data-math="(\w+)"/g)].map((m) => m[1]), ['inline', 'display', 'align'], 'each once, in the Insert menu');
  assert.ok(!/data-math="[$\\]/.test(html), 'the old $x$ and \\[x\\] buttons are gone');
});
// One origin tint per spot: an argument's word inside its macro's run keeps its tint, the run is cut around it
// (pins the double box "Helmlo [World]"); rows apart and other pages stay whole.
test("One origin tint per spot", async () => {
  const E = self.satexPdfEdit, box = (l, r, kind, page = 1, t = 0) => ({ page, l, r, t, d: t + 10, kind });
  const got = E.apart([box(0, 100, 'macro'), box(40, 70, 'arg'), box(0, 100, 'moved', 1, 20), box(0, 100, 'moved', 2)]);
  assert.deepEqual(got.map((b) => [b.kind, b.page, b.t, b.l, b.r]).sort(), [['arg', 1, 0, 40, 70], ['macro', 1, 0, 0, 40], ['macro', 1, 0, 70, 100], ['moved', 1, 20, 0, 100], ['moved', 2, 0, 0, 100]]);
});
// Column widths by drag (pdfedit.ts): an `l`/`c`/`r` column becomes a `p` keeping its alignment, a drag snaps to
// 0.5 mm or to a twentieth of `\linewidth` when near one, and an `l` column's width is its cells' ink on the page.
// Pins "≈1.6cm" for every column of an l/c/r table and a drag that could not widen a column without a width.
test("Column widths by drag (pdfedit.ts)", async () => {
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
});
// Typing in verbatim (pdfedit.ts): listings, verbatim, minted, \verb and \lstinline take every character as it is,
// no escape, no math, no group; a `$` inside a listing opens no math for the text after it. Pins `\` typed into a
// listing written as \textbackslash{} and the text after a `$` in a \verb read as math.
test("Typing in verbatim (pdfedit.ts)", async () => {
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
});
// Typing in a table (pdfedit.ts): Tab and Shift+Tab step from cell to cell, row over row, past rules; Enter adds a
// row after the caret's, as many cells as it has. Pins Enter writing a blank line (a paragraph) inside a tabular.
test("Typing in a table (pdfedit.ts)", async () => {
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
});
// Typing in the PDF: the source words of a paragraph by offset, matched in order to the page's words. The caret and
// the echo are placed by these offsets, never by a word's spelling alone (`lorem … lorem`), so a rebuilt page that
// moved the caret's line to the next page still puts the caret after the same character.
test("Typing in the PDF", async () => {
  const E = self.satexPdfEdit, text = 'A \\emph{big} cat~sat, 50\\% % note\n{} $x$ end';
  assert.deepEqual(E.tokens(text, 0, text.length).map((t) => [text.slice(t.s, t.e), t.t]), [['A', 'A'], ['\\emph{big}', 'big'], ['cat', 'cat'], ['sat,', 'sat,'], ['50\\%', '50%'], ['$x$', 'x'], ['end', 'end']]);
  // a heading sets its long title: the short one (the contents') and the label set no word here (the heading typed
  // into echoed its words twice: the short title took the long title's page word)
  const head = '\\section[Background]{Background and Motivation}\\label{sec:bg}\nLiterate';
  assert.deepEqual(E.tokens(head, 0, head.length).map((t) => [head.slice(t.s, t.e), t.t]), [['{Background', 'Background'], ['and', 'and'], ['Motivation}', 'Motivation'], ['Literate', 'Literate']]);
  assert.deepEqual(E.aligned(['a', 'b', 'c', 'd'], ['a', 'c', 'd']), [0, -1, 1, 2], 'a source word the page lacks');
  assert.deepEqual(E.aligned(['a', 'b', 'c'], ['a', 'X', 'b', 'c']), [0, 2, 3], 'a page word the source lacks (a macro\'s text)');
  assert.deepEqual(E.aligned(['lorem', 'consectetur', 'lorem'], ['lorem', 'consec-', 'tetur', 'lorem']), [0, 1, 3], 'hyphenated across rows: its first part');
  // a footnote's call glues its first word to the word before (`here.\footnote{A` is `here.A`): it pairs with the
  // page's `here.`, the footnote's words with the footnote row. Pins its last `here.` taken for the text's `here.`.
  assert.deepEqual(E.aligned(['words', 'here.A', 'footnote', 'text', 'goes', 'here.'], ['words', 'here.', '1', 'A', 'footnote', 'text', 'goes', 'here.']), [0, 1, 4, 5, 6, 7], 'a footnote call');
  // The live math preview (PE.mathRuns, PE.mathLaid): typed text cut into prose and formulas (one still open runs to
  // the end), each formula laid out the way TeX sets it, roughly: letters math italic, scripts at 0.7 and moved,
  // a binary operator's medium space (none for a unary minus), a fraction stacked about the axis. Pins "(x1)" echoed
  // in roman for `\(x_1\)` (user 2026-11-18).
  {
    const t = 'cost $x_1$ and \\(a+b\\) or \\[y=2\\] \\begin{equation}z\\end{equation} and $op';
    assert.deepEqual(E.mathRuns(t, 0, t.length).map((r) => [r.kind, t.slice(r.b, r.be)]), [['', 'cost '], ['inline', 'x_1'], ['', ' and '], ['inline', 'a+b'], ['', ' or '], ['display', 'y=2'], ['', ' '], ['display', 'z'], ['', ' and '], ['inline', 'op']]);
    assert.deepEqual(E.mathRuns('$x$ y', 1, 5).map((r) => r.kind), ['inline', ''], 'from inside a formula');
    const w = (s) => s.length * 0.5, lay = (s, d) => E.mathLaid(s, w, d), at = (b, t) => b.g.find((g) => g[0] === t);
    const sub = lay('x_1');
    assert.deepEqual([at(sub, 'x')[4], at(sub, '1')[4], at(sub, '1')[3]], ['it', 'rm', 0.7], 'x italic, 1 upright and smaller');
    assert.ok(at(sub, '1')[2] < 0 && at(lay('x^2'), '2')[2] > 0.3, 'a subscript lowered, a superscript raised');
    const sum = lay('a+b'), neg = lay('-x');
    assert.ok(Math.abs(at(sum, '+')[1] - (0.5 + 4 / 18)) < 1e-9 && Math.abs(at(sum, 'b')[1] - (1 + 8 / 18)) < 1e-9, 'a medium space about +');
    assert.deepEqual([at(neg, '−')[1], at(neg, 'x')[1]], [0, 0.5], 'a unary minus: no space');
    assert.equal(at(lay('x_{a+b}'), 'b')[1], at(lay('x_{a+b}'), '+')[1] + 0.35, 'no medium space in a script');
    const fr = lay('\\frac{a}{b}');
    assert.ok(at(fr, 'a')[2] > 0.25 && at(fr, 'b')[2] < 0 && fr.r.length === 1 && at(fr, 'a')[3] === 0.7, 'a fraction stacked over its rule, one size down inline');
    assert.equal(at(lay('\\frac{a}{b}', true), 'a')[3], 1, 'full size in display');
    assert.deepEqual(at(lay('\\alpha'), 'α').slice(3), [1, 'it'], 'Greek lowercase in math italic');
    assert.deepEqual(lay('x_').g.length, 1, 'a script still being typed: no fault');
    assert.ok(at(lay('\\sum_{i=1}^n', true), 'n')[2] > 0.75, 'display limits above');
  }
  // The echo's line breaking (PE.reflow), pure: pieces of width `n` after a space `gap`, rows of the page after.
  {
    const W = (ns, gap = 3) => ns.map((n, i) => ({ n, gap: i ? gap : 0, id: i }));
    const L = (o = {}) => ({ right: 100, left: 0, indent: 15, skip: 12, parskip: 0, rows: [], ...o });
    const shape = (f) => f.rows.map((r) => r.out.map((q) => q.id ?? q.t));
    // exact fit: a row filled to its right edge keeps the piece that ends there. Pins `>` against `>=`.
    assert.deepEqual(shape(E.reflow(W([47, 50]), 0, L())), [[0, 1]], 'exact fit stays on the row');
    assert.deepEqual(shape(E.reflow(W([47, 51]), 0, L())), [[0], [1]], 'one bp more breaks');
    // a piece wider than a line stands alone: never an endless break, never two on its row
    const wide = E.reflow(W([10, 150, 10]), 0, L());
    assert.deepEqual(shape(wide), [[0], [1], [2]], 'an overwide piece on a row of its own');
    assert.equal(wide.rows[1].out[0].x, 0, 'at the left margin');
    // spaces: an empty piece (a second space) advances only by its gap, never breaks
    const sp = E.reflow([{ n: 10, gap: 0, id: 'a' }, { n: 0, gap: 3, id: '' }, { n: 10, gap: 3, id: 'b' }], 0, L());
    assert.deepEqual(sp.rows.map((r) => r.out.map((q) => q.x)), [[0, 13, 16]], 'empty pieces take their gap');
    // a break swallows the space: the row's first piece stands at its left edge
    const br = E.reflow(W([60, 60]), 20, L());
    assert.equal(br.rows[1].out[0].x, 0, 'a new row starts at the margin, its space dropped');
    assert.equal(br.rows[1].dy, 12, 'a line below');
    // Enter: a new paragraph, indented, the parskip lower; a second Enter on an empty row adds a row of its own
    const par = E.reflow([{ n: 20, gap: 0, id: 0 }, { n: 10, gap: 0, par: true, id: 1 }], 0, L({ parskip: 4 }));
    assert.deepEqual([par.rows[1].x, par.rows[1].dy, par.rows[1].par, par.rows[1].out[0].x], [15, 16, true, 15], 'a paragraph indented and parskip lower');
    // overflow runs into the page's next rows, row by row, their words after it in order
    const rows = [{ x: 0, pieces: W([45, 45]).map((p) => ({ ...p, id: `r1-${p.id}` })) }, { x: 0, pieces: W([45, 20]).map((p) => ({ ...p, id: `r2-${p.id}` })) }];
    const on = E.reflow(W([40, 40, 30]), 0, L({ rows }));
    assert.deepEqual(shape(on), [[0, 1], [2, 'r1-0'], ['r1-1', 'r2-0'], ['r2-1']], 'overflow cascades through the rows');
    assert.deepEqual(on.rows.map((r) => r.from), [-2, 0, 1, -1], 'each page row reused, then a new one');
    // a deletion across a row boundary: with `pull` the row takes up the next row's words, a row emptied is freed
    const short = [{ x: 0, pieces: W([30, 30]).map((p) => ({ ...p, id: `s1-${p.id}` })) }, { x: 0, pieces: W([20]).map((p) => ({ ...p, id: `s2-${p.id}` })) }];
    const pulled = E.reflow(W([20]), 0, L({ rows: short, pull: true }));
    assert.deepEqual(shape(pulled), [[0, 's1-0', 's1-1']], 'a row taken up whole while it fits, the next (20 more passes 100) not');
    assert.deepEqual(pulled.freed, [0], 'its row freed');
    const part = E.reflow(W([50]), 0, L({ rows, pull: true }));
    assert.deepEqual(shape(part), [[0, 'r1-0'], ['r1-1', 'r2-0'], ['r2-1']], 'a row taken up in part, the rest on its own row, which then takes up the next');
    assert.deepEqual([part.rows.map((r) => r.from), part.freed], [[-2, 0, 1], []], 'the page rows reused, none freed');
    assert.deepEqual(shape(E.reflow(W([20]), 0, L({ rows }))), [[0]], 'without pull the rows below stay as the page set them');
    // math: a formula is one piece (its sub- and superscripts inside), never broken
    assert.deepEqual(shape(E.reflow([{ n: 50, gap: 0, id: 'text' }, { n: 48, gap: 3, id: '$x_1+y^2$' }], 0, L())), [['text'], ['$x_1+y^2$']], 'a formula moves whole');
    // centered rows: half the growth, only a row as far from both edges
    assert.equal(E.centered([0, 100], [30, 70], 10), 5, 'centered: half');
    assert.equal(E.centered([0, 100], [0, 70], 10), 0, 'flush left: none');
    assert.equal(E.centered([0, 100], [30, 60], 10), 0, 'off-centre: none');
    // display math: centered on a row of its own, `disp` more space above, `below` before the text after it, which
    // starts at the margin; past it no page row is taken (the page below is pushed down instead)
    const dm = E.reflow([{ n: 20, gap: 0, id: 'so' }, { n: 30, gap: 0, disp: 8, below: 6, id: 'F' }, { n: 10, gap: 0, id: 'ok' }], 0, L({ rows }));
    assert.deepEqual(shape(dm), [['so'], ['F'], ['ok']], 'the formula alone, the text after it below, the page rows left as set');
    assert.deepEqual(dm.rows.slice(1).map((r) => [r.from, r.x, r.dy]), [[-1, 35, 20], [-1, 0, 18]], 'centered, lower by skip + disp, then skip + below');
    // properties over random paragraphs: no row passes the edge (but a lone overwide piece), order kept, re-layout
    // of the result as one row's pieces again gives the same rows (idempotent)
    let seed = 7;
    const rnd = () => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff;
    for (let run = 0; run < 300; run++) {
      const ps = Array.from({ length: 1 + Math.floor(rnd() * 30) }, (_, i) => ({ n: Math.floor(rnd() * 40), gap: i ? 1 + Math.floor(rnd() * 5) : 0, par: i > 0 && rnd() < 0.05, id: i }));
      const line = L({ right: 60 + Math.floor(rnd() * 100), indent: Math.floor(rnd() * 20) });
      const f = E.reflow(ps, Math.floor(rnd() * 30), line);
      for (const r of f.rows) if (r.out.length > 1) assert.ok(r.out.at(-1).x + r.out.at(-1).n <= line.right + 0.5 || r.out.at(-1).n === 0, `run ${run}: a row past the edge`);
      assert.deepEqual(f.rows.flatMap((r) => r.out.map((q) => q.id)), ps.map((p) => p.id), `run ${run}: order kept`);
      const again = E.reflow(f.rows.flatMap((r) => r.out.map(({ x, ...q }) => q)), f.rows[0].x, line);
      assert.deepEqual(again.rows.map((r) => r.out.map((q) => [q.id, q.x])), f.rows.map((r) => r.out.map((q) => [q.id, q.x])), `run ${run}: idempotent`);
    }
  }
  assert.deepEqual(E.aligned(['ﬁne'], ['fine']), [0], 'a ligature as the page spells it');
  assert.deepEqual(E.changed('abcXdef', 'abcYYdef'), [3, 3]);
  assert.deepEqual(E.changed('aaa', 'aaaa'), [3, 0], 'the common end never overlaps the common start');
  const doc = 'one\n\ntwo three\nfour\n\nfive';
  assert.deepEqual(E.paragraph(doc, doc.indexOf('three')), [5, 19]);
  assert.deepEqual(E.paragraph(doc, 1), [0, 3]);
  // a caption is its own paragraph, never the text above its float (the caret was drawn on the line above the figure)
  const fig = 'Text above.\n\\begin{figure}\n\\caption{From here}\n\\end{figure}\nAfter.';
  const [c0, c1] = E.paragraph(fig, fig.indexOf('here'));
  assert.equal(fig.slice(c0, c1), '\\caption{From here}');
  assert.equal(fig.slice(...E.paragraph(fig, 3)), 'Text above.');
});
// References switched from the pages (pdfedit.ts): the key written over is the one in the command's list, past its
// options; a page word is a reference's output when its line does not spell it; the list ranks by fuzzy match.
// Pins a switch that rewrote `\cite[p.~3]{a,b}`'s options or the other key.
test("References switched from the pages (pdfedit.ts)", async () => {
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
});
