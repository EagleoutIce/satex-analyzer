// The viewers' layers (layers.ts). Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
test("the layers of both viewers: one catalogue, grouped alike, kept per viewer", async () => {
// the layers of both viewers (layers.js): one catalogue, grouped alike, kept per viewer
load('./layers.js');
const layers = self.satexLayers;
const TYPESET = ['envs', 'keys', 'boxes', 'glue', 'ligs', 'findings', 'maybe'], PDF = ['envs', 'keys', 'glue', 'geometry', 'figures', 'findings', 'bad'];
assert.deepEqual(layers.groups(TYPESET), [['structure', 'Structure', ['envs', 'keys']], ['layout', 'Layout', ['boxes', 'glue', 'ligs']], ['analysis', 'Analysis', ['findings', 'maybe']]]);
// a layer both draw sits in the same group under the same name
for (const k of TYPESET.filter((k) => PDF.includes(k))) {
  const where = (ids) => layers.groups(ids).find((g) => g[2].includes(k))[0];
  assert.equal(where(TYPESET), where(PDF), k);
}
assert.deepEqual(layers.groups(['maybe', 'envs']).map((g) => g[0]), ['structure', 'analysis'], 'an empty group is left out');
// the markup: a labelled group each, one switch per layer named by its viewer, a fold only where asked
const about = Object.fromEntries(PDF.map((k) => [k, `about ${k}`]));
const markup = layers.html(about, { glue: true }, { prefix: 'pdf-layer', facets: new Set(['glue']) });
assert.deepEqual([...markup.matchAll(/class="layer-label">(\w+)</g)].map((m) => m[1]), ['Structure', 'Layout', 'Analysis']);
assert.deepEqual([...markup.matchAll(/id="pdf-layer-(\w+)"/g)].map((m) => m[1]), ['envs', 'keys', 'glue', 'geometry', 'figures', 'findings', 'bad']);
assert.match(markup, /id="pdf-layer-glue" data-layer="glue" checked/);
assert.doesNotMatch(markup, /id="pdf-layer-envs" data-layer="envs" checked/);
assert.deepEqual([...markup.matchAll(/data-fold="(\w+)"/g)].map((m) => m[1]), ['glue']);
// the state: defaults, under what an older version kept, under the store's; each viewer its own key
const kept = new Map(), store = { get: (k) => kept.get(k) ?? null, set: (k, v) => kept.set(k, v) };
const on = layers.state('typeset', { glue: false, maybe: true, keys: false }, store, { keys: true });
assert.deepEqual(on, { glue: false, maybe: true, keys: true });
let applied = null;
const root = { addEventListener: (_, f) => { root.fire = f; } };
layers.wire(root, 'typeset', on, store, (k) => { applied = k; });
root.fire({ target: { dataset: { layer: 'glue' }, checked: true } });
root.fire({ target: { dataset: { layer: 'nothing' }, checked: true } });
assert.equal(applied, 'glue');
assert.deepEqual(kept.get('layers-typeset'), { glue: true, maybe: true, keys: true });
assert.equal(kept.get('layers-pdf'), undefined);
assert.deepEqual(layers.state('typeset', { glue: false, maybe: true, keys: false }, store, { keys: false, glue: false }), { glue: true, maybe: true, keys: true }, 'the store wins over older keys');
// the grid's step: any TeX unit, em and ex only with the run's font, nothing at or below 0
assert.deepEqual(layers.length(' 1in'), { pt: 72.27, num: 1, unit: 'in' });
assert.equal(layers.length('2pc').pt, 24);
assert.equal(layers.length('1em'), null);
assert.equal(layers.length('1.5em', { em: 10 }).pt, 15);
assert.deepEqual([layers.length('0cm'), layers.length('cm'), layers.length('-1pt')], [null, null, null]);
assert.deepEqual(layers.groups(['grid', 'glue', 'geometry']), [['layout', 'Layout', ['glue', 'geometry', 'grid']]]);
});
// The Typeset view's Show menu (user 2026-11-19): each ready switch on is one class on the view, kept per viewer; one
// the run does not report yet is shown disabled and puts no class (pins the option-to-class mapping).
test('the Typeset Show menu: switches to classes', async () => {
  const L = self.satexLayers, d = L.showDefaults();
  assert.deepEqual(Object.keys(d), Object.keys(L.SHOW));
  assert.equal(d.parskip, true, 'on by default');
  assert.equal(d.indent, true, 'the run reports each paragraph\'s indent: on');
  assert.equal(d.index, false, 'not reported by the run yet: off');
  assert.deepEqual(L.showClasses(d), Object.keys(L.SHOW).filter((k) => L.SHOW[k][2] && L.SHOW[k][3]).map((k) => `ts-show-${k}`));
  assert.deepEqual(L.showClasses({ ...d, parskip: false, justify: true, numbers: true }).filter((c) => /parskip|justify|numbers/.test(c)), ['ts-show-justify'], 'off drops its class, a switch not ready never adds one');
  const store = new Map(), S = { get: (k) => store.get(k), set: (k, v) => store.set(k, v) };
  store.set('layers-typeset-show', { colors: false });
  assert.equal(L.state('typeset-show', d, S).colors, false, 'kept per viewer');
  const h = L.showHtml(d);
  assert.equal((h.match(/role="switch"/g) ?? []).length, Object.keys(L.SHOW).length);
  assert.ok(/id="typeset-show-numbers"[^>]*disabled/.test(h), 'a switch not ready is disabled');
});
// The paragraph indent switch (user 2026-11-19) draws the run's facts: the width set before the first character,
// none after \noindent or a heading, nothing where the run did not know (pins indentOf's reading of the rows).
test('the Typeset indent comes from the row facts', async () => {
  const L = self.satexLayers;
  assert.equal(L.indentOf({ indented: true, indent: 12, parindent: 15 }), 12, 'what was set wins');
  assert.equal(L.indentOf({ indented: true, parindent: 15 }), 15, 'else \\parindent');
  assert.equal(L.indentOf({ indented: false, parindent: 15 }), 0, '\\noindent and after a heading: none');
  assert.equal(L.indentOf({ parindent: 15 }), null, 'not known: no indent drawn');
});
// The glue labels (user 2026-11-19, a screenshot of `3.33333pt plus 1.66667pt minus 1.11111pt` drawn over `end · 253.6pt`):
// the spec shortened, two decimals at most, the unit once; other labels keep their words (pins glueShort).
test('glue labels: short form', async () => {
  const L = self.satexLayers;
  assert.equal(L.glueShort('3.33333pt plus 1.66667pt minus 1.11111pt'), '3.33+1.67−1.11pt');
  assert.equal(L.glueShort('0pt plus 1fil'), '0pt+1fil', 'units differ: each said');
  assert.equal(L.glueShort('12.0pt'), '12pt');
  assert.equal(L.glueShort('2pt minus 1pt'), '2−1pt');
  assert.equal(L.glueShort('end · 253.6pt'), 'end · 253.6pt');
  assert.equal(L.glueShort('centred · 12.345pt each side'), 'centred · 12.35pt each side');
});
// Overlapping labels (same bug): a label moves off one placed before or a fixed chip, prefers a spot over no ink, and
// is elided (null) when no spot is free (pins fitLabels' order: in place, below, above, two below).
test('glue labels: collisions moved or elided', async () => {
  const L = self.satexLayers, R = (l, t, r, b) => ({ l, t, r, b });
  assert.deepEqual(L.fitLabels([R(0, 10, 50, 20), R(40, 10, 90, 20)], [], []), [0, 10], 'the second moves a height down');
  assert.deepEqual(L.fitLabels([R(0, 10, 50, 20)], [], [R(0, 10, 50, 20), R(0, 20, 50, 30)]), [-10], 'above: the spot over no ink');
  assert.deepEqual(L.fitLabels([R(0, 10, 50, 20)], [R(0, 0, 50, 40)], []), [null], 'no free spot: elided');
  assert.deepEqual(L.fitLabels([R(0, 10, 50, 20), R(0, 10, 50, 20), R(0, 10, 50, 20), R(0, 10, 50, 20), R(0, 10, 50, 20)], [], []), [0, 10, -10, 20, null]);
});
// The math layer (user 2026-11-19) tints inline math where SyncTeX's `$` records put its start and end: one span per
// pair, a formula broken over two lines in two parts (pins mathSpans' pairing and the line box heights).
test('math layer: spans from the SyncTeX math records', async () => {
  const L = self.satexLayers, B = (kind, x, y, w = 0, h = 0, d = 0) => ({ page: 1, file: 'a.tex', line: 3, kind, x, y, w, h, d });
  const boxes = [B('(', 70, 100, 300, 7, 2), B('(', 70, 112, 300, 7, 2), B('$', 100, 100), B('$', 130, 100), B('$', 340, 100), B('$', 90, 112)];
  assert.deepEqual(L.mathSpans(boxes).map((s) => [s.l, s.r, s.t, s.d]), [[100, 130, 93, 102], [340, 370, 93, 102], [70, 90, 105, 114]]);
  assert.deepEqual(L.mathSpans([B('$', 10, 50)]), [], 'an unpaired record: no span');
});
// The legend (user 2026-11-19): every layer's switch shows a swatch in its colour, read from a theme token so it follows
// light and dark as its overlay does (pins: no layer without a `.sw-<id>` rule, no swatch with a bare hex colour).
test('every layer has a themed swatch', async () => {
  const css = readFileSync(new URL('style.css', SITE), 'utf8');
  for (const k of Object.keys(self.satexLayers.LAYERS)) {
    const rule = new RegExp(`\\.sw-${k} \\{([^}]*)\\}`).exec(css);
    assert.ok(rule, `.sw-${k} missing`);
    assert.doesNotMatch(rule[1], /#[0-9a-f]{3,8}\b/i, `.sw-${k} names a bare colour, not a token`);
  }
});
