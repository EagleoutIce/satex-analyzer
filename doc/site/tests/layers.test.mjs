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
  assert.equal(d.indent, false, 'not reported by the run yet: off');
  assert.deepEqual(L.showClasses(d), Object.keys(L.SHOW).filter((k) => L.SHOW[k][2] && L.SHOW[k][3]).map((k) => `ts-show-${k}`));
  assert.deepEqual(L.showClasses({ ...d, parskip: false, justify: true, indent: true }).filter((c) => /parskip|justify|indent/.test(c)), ['ts-show-justify'], 'off drops its class, a switch not ready never adds one');
  const store = new Map(), S = { get: (k) => store.get(k), set: (k, v) => store.set(k, v) };
  store.set('layers-typeset-show', { colors: false });
  assert.equal(L.state('typeset-show', d, S).colors, false, 'kept per viewer');
  const h = L.showHtml(d);
  assert.equal((h.match(/role="switch"/g) ?? []).length, Object.keys(L.SHOW).length);
  assert.ok(/id="typeset-show-indent"[^>]*disabled/.test(h), 'a switch not ready is disabled');
});
