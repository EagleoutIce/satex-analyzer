// The Page layout widget (views.ts `page`): fixtures. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// page layout: the run's lengths as body margins; Apply sends snapshots the server prints:
// geometry's keys of what changed, typearea's DIV and BCOR, \newgeometry around a part, fancyhdr's fields grouped by text
test("page layout", async () => {
  const P = self.satexViews.page, S = self.satexSnapshot;
  const cm = 72.27 / 2.54 * 65536, two = [2 * cm, 2 * cm].map(Math.round);
  const at = (line) => ({ line, character: 0 }), here = { start: at(1), end: at(1) };
  const same = (v) => [v, v];
  // a call as the server reads it (src/lsp/snapshot.rs `read`)
  const read = (name, opts, req) => ({ kind: 'call', name, star: false, range: here, text: `\\${name}[${opts}]{${req}}`, line: false, drop: false,
    args: [{ open: '[', inner: [0, 0], text: opts, keys: opts.split(',').map((o) => o.split('=')).map(([key, value]) => ({ key, value: value ?? null })) }, { open: '{', inner: [0, 0], text: req }] });
  const pairs = (n) => S.list(n).map((k) => k.value == null ? k.key : `${k.key}=${k.value}`);
  const s = {
    run: { paperwidth: same(40258437), paperheight: same(52099153), textwidth: same(32799719), textheight: same(44640435), oddsidemargin: same(-1006927), evensidemargin: same(-1006927),
      topmargin: same(-3431759), headheight: same(786432), headsep: same(1638400), footskip: same(1966080), marginparwidth: same(4259840), marginparsep: same(720896) },
    twoside: false, family: 'basic', papers: [{ name: 'a4paper', w: 39158276, h: 55380990 }, { name: 'legalpaper', w: 40258437, h: 66331225 }, { name: 'letterpaper', w: 40258437, h: 52099153 }], units: [{ name: 'cm', sp: cm }, { name: 'pt', sp: 65536 }], unit: 'cm', decimals: 2,
    geometry: { loaded: true, global: read('usepackage', 'margin=2cm', 'geometry'), package: null }, koma: null,
    part: { here: at(3), marks: [{ title: 'One', level: 'section', at: at(5) }], end: at(7) },
    header: { package: null, insert: at(2), load: { name: 'fancyhdr' } },
  };
  const len = (sp) => `${+(sp / cm).toFixed(2)}cm`;
  const st0 = P.pageOf(s);
  assert.deepEqual([st0.left, st0.right, st0.top, st0.bottom].map((v) => Math.abs(v - two[0]) < 2), [true, true, true, true], 'margin=2cm on all sides');
  assert.equal(st0.paper, 'letterpaper', '21.59 x 27.94 cm is letter, not custom');
  assert.deepEqual(P.pageNodes(s, st0, structuredClone(st0), len), [], 'nothing changed, nothing sent');
  // a wider left margin: the margin group set anew, the text narrower
  const wide = { ...structuredClone(st0), left: Math.round(3 * cm) };
  const [w] = P.pageNodes(s, st0, wide, len);
  assert.deepEqual([w.range, pairs(w)], [here, ['left=3cm', 'right=2cm', 'top=2cm', 'bottom=2cm']]);
  assert.deepEqual(pairs(s.geometry.global), ['margin=2cm'], 'the view keeps its snapshot as opened');
  // another paper: its name, the margins kept as written
  const a4 = { ...structuredClone(st0), paper: 'a4paper', pw: 39158276, ph: 55380990 };
  assert.deepEqual(pairs(P.pageNodes(s, st0, a4, len)[0]), ['margin=2cm', 'a4paper']);
  // a part of it: \newgeometry at the cursor, \restoregeometry before the section
  const part = { ...structuredClone(wide), scope: 'part', from: 'here', to: '0' };
  const [ng, rg] = P.pageNodes(s, st0, part, len);
  assert.deepEqual([ng.name, ng.at, ng.range, pairs(ng), rg.name, rg.at, rg.args], ['newgeometry', at(3), null,
    ['left=3cm', 'right=2cm', 'top=2cm', 'bottom=2cm', 'headheight=0.42cm', 'headsep=0.88cm', 'footskip=1.05cm', 'marginparwidth=2.28cm', 'marginparsep=0.39cm'], 'restoregeometry', at(5), []]);
  // without geometry: the new calls load it with `pass`, the layout elsewhere kept
  const pass = { name: 'geometry', options: 'pass' };
  const bare = { ...s, geometry: { ...s.geometry, loaded: false, package: pass } };
  assert.deepEqual(P.pageNodes(bare, st0, { ...part, to: 'end' }, len).map((n) => [n.name, n.package]), [['newgeometry', pass]], 'no \\restoregeometry');
  // typearea: DIV and BCOR as class options, only what changed set
  const koma = { ...s, family: 'koma', koma: { ...read('documentclass', 'DIV=10,BCOR=5mm', 'scrartcl'), div: 10, bcor: Math.round(cm / 2) } };
  const k0 = P.pageOf(koma);
  assert.equal(k0.mode, 'koma');
  assert.deepEqual([k0.paper, k0.div], ['letterpaper', 10], 'typearea keeps its DIV beside the detected paper');
  assert.deepEqual(P.pageNodes(koma, k0, { ...structuredClone(k0), div: 12 }, len).map(pairs), [['DIV=12', 'BCOR=5mm']]);
  const box = P.pageBox({ ...k0, div: 12, ran: null }, true);
  assert.ok(Math.abs(2 * (box.left - k0.bcor) - box.right) < 2, 'two-sided: the outer margin twice the inner');
  // fancyhdr: the fields its calls set, written back grouped by text
  const call = (name, opt, args, line) => ({ kind: 'call', name, range: { start: at(line), end: at(line + 1) }, text: '', args: [...(opt == null ? [] : [{ open: '[', text: opt }]), ...args.map((t) => ({ open: '{', text: t }))] });
  const h = { package: 'fancyhdr', insert: at(2), rules: { head: '0.4pt', foot: '0pt' },
    calls: [call('fancyhf', null, [''], 2), call('fancyhead', 'LE,RO', ['\\thepage'], 3), call('fancyfoot', 'C', ['draft'], 4)] };
  const m = P.headerOf(h, true);
  assert.deepEqual([m.head.EL, m.head.OR, m.head.OL, m.foot.EC, m.foot.OC], ['\\thepage', '\\thepage', '', 'draft', 'draft']);
  assert.deepEqual(P.headerCalls(m, true, true), [['fancyhf', null, ['']], ['fancyhead', 'LE,RO', ['\\thepage']], ['fancyfoot', 'CE,CO', ['draft']]]);
  assert.deepEqual(P.headerNodes(h, P.headerOf(h, true), true), [], 'untouched, nothing sent');
  const m2 = P.headerOf(h, true);
  m2.head.OL = 'Title'; m2.rules.head = '0pt';
  const shown = (n) => n.drop ? `-${n.name}` : `${n.name}${n.args.map((a) => a.open + a.text).join('')}@${n.at.line}`;
  assert.deepEqual(P.headerNodes(h, m2, true).map(shown), ['-fancyhf', '-fancyhead', '-fancyfoot', 'fancyhf{@2', 'fancyhead[LE,RO{\\thepage@2', 'fancyhead[LO{Title@2', 'fancyfoot[CE,CO{draft@2', 'renewcommand{\\headrulewidth{0pt@2']);
  // scrlayer-scrpage: one call per field, the plain style's text kept
  const ks = { package: 'scrlayer-scrpage', insert: at(2), calls: [call('ohead', 'x', ['\\pagemark'], 2)] };
  const km = P.headerOf(ks, true);
  km.foot.Oc = 'draft';
  assert.deepEqual(P.headerNodes(ks, km, true).map(shown), ['-ohead', 'ohead[x{\\pagemark@2', 'cfoot{draft@2']);
  // none loaded: \pagestyle{fancy} before the fields, fancyhdr loaded by the server for them
  const none = { package: null, insert: at(2), load: { name: 'fancyhdr' } };
  const nm = P.headerOf(none, false);
  Object.assign(nm, { package: 'fancyhdr' });
  nm.head.OR = '\\thepage';
  const nn = P.headerNodes(none, nm, false);
  assert.deepEqual(nn.map(shown), ['pagestyle{fancy@2', 'fancyhf{@2', 'fancyhead[R{\\thepage@2']);
  assert.deepEqual(nn[1].package, { name: 'fancyhdr' });
  // the snapshot helper: kept keys stay in place, a dropped group's keys go, bare keys stay bare
  const g = read('geometry', 'left=1cm,top=2cm,a4paper', '');
  S.setKeys(g, ['left', 'top', 'right'], [['top', '3cm'], ['right', '1cm']]);
  assert.deepEqual(pairs(g), ['top=3cm', 'a4paper', 'right=1cm']);
});

// page numbering: a style picked after a matter mark is a new \pagenumbering there, a call as written
// changes its argument or goes, the chapter reset is a starred \counterwithin in the preamble
test("page numbering", async () => {
  const P = self.satexViews.page, at = (line) => ({ line, character: 0 });
  const call = { kind: 'call', name: 'pagenumbering', star: false, args: [{ open: '{', inner: null, text: 'alph' }], range: null, text: '\\pagenumbering{alph}' };
  const s = { numbering: { start: at(2), insert: at(1), chapters: true, per_chapter: false, reset: null, marks: [{ name: 'mainmatter', row: 5, at: at(6) }], calls: [{ row: 3, node: call }] } };
  const st0 = { numbers: { start: '', m0: '', c0: 'alph' }, perChapter: false };
  const nodes = P.numberNodes(s, st0, { numbers: { start: '', m0: 'Roman', c0: '' }, perChapter: true });
  assert.deepEqual(nodes.map((n) => [n.name, n.drop ? 'drop' : n.args.map((a) => a.text).join(','), n.star]), [['pagenumbering', 'drop', false], ['pagenumbering', 'Roman', false], ['counterwithin', 'page,chapter', true]]);
  assert.deepEqual(nodes[1].at, at(6), 'after the mark: \\mainmatter sets its own style first');
  assert.deepEqual(P.numberNodes(s, st0, st0), [], 'nothing changed, nothing written');
  // a page number clicked: the numbering in force at its line (1-based rows) is the last point at or before it
  assert.deepEqual([1, 3, 4, 5, 9].map((l) => P.numberInForce(s, l)), ['start', 'c0', 'c0', 'm0', 'm0']);
});

// Intuition: typing into a footer cell of the PDF writes the footer once, in one write: no header package means
// fancyhdr (scrlayer-scrpage for a KOMA class) with the plain style's page number kept, and the clicked cell gets a
// call of its own when none sets it. Pins the first key on hello.tex's page number wiping the number, and KOMA getting
// \pagestyle{fancy}.
test('footSetup: the footer written for typing into a cell', () => {
  const P = self.satexViews.page, at = (line) => ({ line, character: 0 });
  const shown = (n) => n.drop ? `-${n.name}` : `${n.name}${n.args.map((a) => a.open + a.text).join('')}`;
  const none = { header: { package: null, insert: at(2), load: { name: 'fancyhdr' } } };
  const c = P.footSetup(none, 1, false);
  assert.deepEqual([c.package, c.letter, c.nodes.map(shown)], ['fancyhdr', 'C', ['pagestyle{fancy', 'fancyhf{', 'fancyfoot[C{\\thepage', 'renewcommand{\\headrulewidth{0pt']]);
  assert.deepEqual(c.nodes.map((n) => n.package?.name), ['fancyhdr', 'fancyhdr', 'fancyhdr', 'fancyhdr'], 'fancyhdr loaded for them');
  assert.deepEqual(P.footSetup(none, 0, false).nodes.map(shown).at(-1), 'fancyfoot[L{', 'a side cell: an empty call beside the number');
  assert.deepEqual(P.footSetup({ ...none, twoside: true }, 1, false).nodes.map(shown)[2], 'fancyfoot[CE,CO{\\thepage', 'two-sided: both sides numbered');
  const k = P.footSetup({ ...none, family: 'koma' }, 2, false);
  assert.deepEqual([k.package, k.letter, k.nodes.map(shown)], ['scrlayer-scrpage', 'o', ['pagestyle{scrheadings', 'clearpairofpagestyles', 'cfoot[\\pagemark{\\pagemark', 'ofoot{']]);
  assert.ok(k.nodes.every((n) => n.package?.name === 'scrlayer-scrpage'), 'KOMA: scrlayer-scrpage, never fancyhdr');
  assert.equal(P.footSetup({ ...none, class: 'scrreprt' }, 1, false).package, 'fancyhdr', 'the run\'s family decides, never the class\'s name');
  // a package loaded: a cell a call sets is typed in, nothing written; another cell gets its call after the last one
  const call = (name, opt, args, line) => ({ kind: 'call', name, range: { start: at(line), end: { line, character: 20 } }, text: '', args: [...(opt == null ? [] : [{ open: '[', text: opt }]), ...args.map((t) => ({ open: '{', text: t }))] });
  const fancy = { header: { package: 'fancyhdr', insert: at(3), calls: [call('fancyhf', null, [''], 3), call('fancyfoot', 'C', ['\\thepage'], 4)] } };
  assert.deepEqual(P.footSetup(fancy, 1, true).nodes, []);
  const r = P.footSetup(fancy, 2, false).nodes;
  assert.deepEqual([r.map(shown), r[0].at], [['fancyfoot[R{'], at(5)]);
  // fancyhdr without \fancyhf fills the center with the page number: a call for it keeps the number
  assert.deepEqual(P.footSetup({ header: { package: 'fancyhdr', insert: at(3), calls: [] } }, 1, false).nodes.map(shown), ['fancyfoot[C{\\thepage']);
});
