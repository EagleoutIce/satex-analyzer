// Widget models: heading styles, properties, Apply, bibliography, fields, managers, settings. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// heading styles: a format read as fields over its tokens, a change rewriting only its field's tokens;
// the edits land in the level's last call, else in new calls; hand-written text is kept or refused
test("heading styles", async () => {
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
});

// the property editor's key lists (dialogs.js `keyval`): split at top-level commas, a value's braces
test("the property editor's key lists (dialogs.js `keyval`)", async () => {
  const K = self.satexDialogs.keyval;
  assert.deepEqual(K.split('draw, fill={rgb,255:red,1;green,2;blue,3}, label={[red]above:x}, thick'), ['draw', 'fill={rgb,255:red,1;green,2;blue,3}', 'label={[red]above:x}', 'thick']);
  assert.deepEqual(['thick', 'fill=red!20', 'label={above:x}', 'right=of a'].map(K.of), [['thick', null], ['fill', 'red!20'], ['label', 'above:x'], ['right', 'of a']]);
  assert.deepEqual([['thick', null], ['fill', 'red'], ['label', 'above:x, red'], ['x', 'a=b'], ['f', '{rgb:red,1}']].map(([k, v]) => K.text(k, v)), ['thick', 'fill=red', 'label={above:x, red}', 'x={a=b}', 'f={rgb:red,1}']);
  // a key written back reads the same
  for (const o of ['fill=red!20', 'label={above:x, red}', 'thick']) assert.equal(K.text(...K.of(o)), o);
});

// the Apply lifecycle (dialogs.js `applier`): Apply before the first answer waits for it, a pending change is
// flushed and its answer written, never an older state's; an answer to a closed widget writes nothing
test("the Apply lifecycle (dialogs.js `applier`)", async () => {
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
});

// the bibliography manager's model (bib.js): bare values, filters, what to fetch, an import's choices
test("the bibliography manager's model (bib.js)", async () => {
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
});

// widget fields (dialogs.js): one escaper for plain text, one finder of TeX slips with its fix, one set of checks
test("widget fields (dialogs.js)", async () => {
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
});

// the managers' edit templates (views.js): an optional part goes with its empty value, a choice picks its edit
test("the managers' edit templates (views.js)", async () => {
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
});

// TeX lengths (lengths.js): any unit as typed, shown as written, one validator
test("TeX lengths (lengths.js)", async () => {
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
});

// The filters every manager shares (views.js `passes`): chips of one group are alternatives, groups narrow, words narrow.
// Intuition: the glossary's "acronym" and "entry" chips are two answers to one question, so picking both shows both; "used" is another
// question and narrows. Before, the bibliography alone had this rule inline and the glossary and index had no chips at all.
test("The filters every manager shares (views.js `passes`)", async () => {
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
});

// Bug pinned: a later edit dropped the history renderer, so the definition history dialog came up empty.

// a definition history's event as text: the augment's added text, the copy, the call holding an older text
test("a definition history's event as text", async () => {
  const { historyEvent } = self.satexViews.history;
  const e = historyEvent({ by: '\\appto', file: 'main.tex', path: 'main.tex', line: 4, state: 'replaced', owner: 'document', change: { kind: 'appends', added: ' world', removed: '' } });
  assert.deepEqual([e.by, e.at, e.change, e.when], ['\\appto', 'main.tex:4', 'appends ` world`', '']);
  assert.equal(historyEvent({ change: { kind: 'replaces', added: '\\oldgreet !', removed: 'Hello world' } }).change, 'replaces `Hello world` by `\\oldgreet !`');
  const copy = historyEvent({ by: '\\let', copies: { name: '\\greet', file: 'main.tex', line: 4, by: '\\appto' }, calls: [{ name: '\\oldgreet', holds: '\\greet', at: 'main.tex:4' }], when: ['\\ifnum true at main.tex:10'] });
  assert.deepEqual([copy.copies, copy.calls, copy.when], [{ name: '\\greet', at: 'main.tex:4', by: '\\appto' }, [{ name: '\\oldgreet', note: 'holds \\greet of main.tex:4' }], '\\ifnum true at main.tex:10']);
});
assert.equal(typeof self.satexViews.RENDER.history, 'function', 'a history section has a renderer');

// Settings YAML (playground Settings, YAML tab and export): one serializer writes block lists, nested maps, plain text
// where YAML keeps it a string and folds long text; the reader takes it back unchanged. Pins the YAML tab that printed
// lists and objects as JSON (`join_paths_at: [{"command":"\\document","hook":null}]`).
test("Settings YAML (playground Settings, YAML tab and export)", async () => {
  const { dump, read, flat, paths } = self.satexViews.yaml;
  const long = Array.from({ length: 30 }, (_, i) => `word${i}`).join(' ');
  const set = {
    'analysis.join_paths_at': [{ command: '\\document', hook: null }], 'cache.index.common': ['microtype', 'cleveref'], 'cache.index.engines': [],
    'bib.backend': 'biber', 'at_letter': false, 'limits.steps': 200, 'odd.texts': ['true', '1.5', 'a: b', '#x', '- y', '', 'yes'], 'notes.about': long,
    'conventions.math_symbols': { 'α': 'alpha', '%': 'percent' }, 'profiles.package.engine+': ['luatex'],
  };
  const text = dump(set);
  assert.match(text, /^analysis:\n {2}join_paths_at:\n {4}- command: \\document\n {6}hook: null$/m, 'a list of maps in blocks, plain backslash');
  assert.match(text, /^ {4}common:\n {6}- microtype\n {6}- cleveref$/m, 'a block list');
  assert.match(text, /^ {4}engines: \[\]$/m, 'an empty list stays flow');
  assert.match(text, /^ {2}backend: biber$/m, 'plain text unquoted');
  assert.match(text, /- "true"\n.*- "1\.5"\n.*- "a: b"\n.*- "#x"\n.*- "- y"\n.*- ""\n.*- "yes"/s, 'text YAML would read as something else is quoted');
  assert.match(text, /about: >-\n {4}word0 /, 'long text folded');
  assert.ok(text.split('\n').every((l) => l.length <= 80), 'no line past the width');
  assert.match(text, /"%": percent/, 'a key YAML would misread is quoted');
  const objects = new Set(['conventions.math_symbols']);
  assert.deepEqual(flat(read(text), (k) => objects.has(k)), set, 'read gives the settings back');
  assert.equal(dump({ 'a.b': ['x', 'y'] }, true), 'a:\n  b: ["x","y"]', 'flow keeps a list on its line');
  // what people type: flow lists, comments, a list at its key's indentation, `|` blocks, an empty key
  assert.deepEqual(read('a: [x, "y z", 3]  # c\nb:\n- 1\n- two\nc: |\n  l1\n  l2\nd:\n'), { a: ['x', 'y z', 3], b: [1, 'two'], c: 'l1\nl2\n', d: {} });
  assert.equal(read('a: 1\n  b: 2'), null, 'stray indentation does not read');
  assert.equal(read('just text'), null);
  // `@changed` in the YAML tab: a changed key's lines, its list items and a map setting's entries; never the group above
  const changed = { 'cache.index.common': ['a'], 'conventions.math_symbols': { 'α': 'alpha' }, 'load.packages+': ['x'] };
  const lines = dump({ ...changed, 'cache.enabled': true }).split('\n'), hit = paths(lines.join('\n')).map((p, i) => p && self.satexViews.yaml.changed(p.path, changed) && lines[i].trim());
  assert.deepEqual(hit.filter(Boolean), ['common:', '- a', 'math_symbols:', 'α: alpha', 'packages+:', '- x']);
  // each line's path: a list's items and a folded text's lines belong to their key
  assert.deepEqual(paths(text).slice(0, 4).map((p) => p && [p.path.join('.'), p.inner]), [['analysis', false], ['analysis.join_paths_at', false], ['analysis.join_paths_at', true], ['analysis.join_paths_at', true]]);
});
