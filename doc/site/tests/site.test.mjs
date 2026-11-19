// Images, dev mode, the typeset view, colors, lengths, undo, tokenizer, console. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// crop to content: the box of the pixels unlike the corners' background, then trim in bp with padding
test("crop to content", async () => {
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
});

// tint: the channels decodearray maps (PNG colour type, JPEG components) and its presets
test("tint", async () => {
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
});

// dev mode: a trace as CSV (nested state one JSON cell) and a step's domains as HTML
test("dev mode", async () => {
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
});

// The typeset view's notation (site.js, loaded on a page with nothing on it: every lookup finds nothing)
test("The typeset view's notation (site.js, loaded on a page with nothing on it", async () => {
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
});

// the color band: groups by who defined the color (the document first), alike colors side by side, greys last
test("the color band", async () => {
  const { colorBand } = self.satexViews;
  const c = (name, rgb, group) => ({ name, rgb, group });
  const band = colorBand([c('gray', [0.5, 0.5, 0.5], 'xcolor'), c('blue', [0, 0, 1], 'xcolor'), c('red', [1, 0, 0], 'xcolor'), c('darkred', [0.5, 0, 0], 'xcolor'),
    c('Sky', [0.4, 0.7, 1], 'dvipsnam'), c('brand', [0, 0.5, 0.5], 'document'), c('white', [1, 1, 1], 'xcolor')]);
  assert.deepEqual(band.map((g) => g.name), ['document', 'xcolor', 'dvipsnam']);
  assert.deepEqual(band[1].colors.map((d) => d.name), ['darkred', 'red', 'blue', 'gray', 'white'], 'a hue family dark to light, greys last');
});

// a length field's text, coloured as the editor colours it: names (each with its hover card) and numbers
test("a length field's text, coloured as the editor colours it", async () => {
  const { texTokens } = self.satexDialogs;
  assert.equal(texTokens('0.8\\linewidth'), '<span class="tok-num">0.8</span><span class="tok-cmd cs-tip" data-cs="\\linewidth">\\linewidth</span>');
  assert.equal(texTokens('5cm'), '<span class="tok-num">5</span>cm');
  assert.equal(texTokens('<\\a'), '&lt;<span class="tok-cmd cs-tip" data-cs="\\a">\\a</span>', 'text escaped');
});

// the widgets' shared undo: one step per settled change, a held gesture as one, reset undoable
test("the widgets' shared undo", async () => {
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
});

// The editor's TeX tokenizer (site.js `siteTex`): a number with its unit is one token, sign and fraction included,
// and an expl3 name with its `_` and `:` is one command. Pins `- 0.05pt` whose `0` stayed plain beside a colored
// `.05pt`, and `\dim_eval:n` split into `\dim`, `_eval`, `:n`.
test("The editor's TeX tokenizer (site.js `siteTex`)", async () => {
  document.documentElement.style ??= {};
  const cm = await import('../codemirror.js');
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
});

// A number is one token wherever TeX or pgf reads one: after `(`, `,` and a sign, with a fraction or exponent, and
// never a word's start. Pins `(2.75,0.0)` split into `(2` plain, `.75` number, `,0` plain, `.0` number, and the
// `0` of the HTML spec `0B3D91` colored alone.
test("numbers in coordinates and specs are whole tokens", async () => {
  const cm = await import('../codemirror.js');
  self.siteCM = () => Promise.resolve(cm);
  const p = (await self.siteTex()).language.streamParser;
  const nums = (line) => {
    const st = p.startState(2), s = new cm.language.StringStream(line, 2, 2), out = [];
    while (!s.eol()) { const t = p.token(s, st); if (t === 'number') out.push(s.current()); s.start = s.pos; }
    return out;
  };
  assert.deepEqual(nums('\\draw (2.75,0.0) -- (5.2,-1.5) -- (.5,1e3);'), ['2.75', '0.0', '5.2', '-1.5', '.5', '1e3']);
  assert.deepEqual(nums('\\node at (2.6,-0.6) {x};'), ['2.6', '-0.6']);
  assert.deepEqual(nums('\\definecolor{accent}{HTML}{0B3D91}'), [], 'a hex spec is a word');
  assert.deepEqual(nums('\\definecolor{soft}{rgb}{0.1,0.2,0.3}'), ['0.1', '0.2', '0.3']);
  assert.deepEqual(nums('version v2.0 and 2.6.3'), [], 'inside a word or a version');
  assert.deepEqual(nums('\\count3 by 400, $x^2$ and 1em'), ['3', '400', '2', '1em']);
});

// array's column injections: the text inside `>{…}` and `<{…}`, several joined
test("array's column injections", async () => {
  const T = self.satexDialogs.table;
  assert.equal(T.injected('>{\\bfseries}'), '\\bfseries');
  assert.equal(T.injected('>{$}>{\\mathbf{x}}'), '$\\mathbf{x}');
  assert.equal(T.injected(''), '');
});

// the console's hover (cli.js): each word of a command line as the command tree names it
test("the console's hover (cli.js)", async () => {
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
});
