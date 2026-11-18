// Package loads, typeset rows, tips and the compile message. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// Package loads: findings of a run that lacked a bundle are never shown. It lacked one never fetched, one still
// downloading (in the background too), or one landed only after it began; once a run saw them all, it is shown.
// Pins the cold-load bug: the first analysis showed `example-image-a` missing while mwe's bundle downloaded.
test("Package loads", async () => {
  const { unlanded, lacked } = self.satexViews.loads;
  const asked = new Map([['pdftex/mwe', 1], ['pdftex/tikz', 1], ['pdftex/amsmath', 1]]), inFlight = new Set(['pdftex/tikz']), background = new Set(), landedAt = new Map([['pdftex/amsmath', 1], ['pdftex/mwe', 2]]);
  const ids = ['pdftex/mwe', 'pdftex/tikz', 'pdftex/amsmath', 'pdftex/listings'];
  assert.deepEqual(unlanded(ids, asked, [inFlight, background]), ['pdftex/tikz', 'pdftex/listings'], 'downloading or never asked for');
  assert.deepEqual(lacked(ids, asked, [inFlight, background], landedAt, 1), ['pdftex/mwe', 'pdftex/tikz', 'pdftex/listings'], 'mwe landed after the run began');
  inFlight.delete('pdftex/tikz'); background.add('pdftex/tikz');
  assert.deepEqual(lacked(['pdftex/tikz'], asked, [inFlight, background], landedAt, 9), ['pdftex/tikz'], 'a background download counts too');
  background.delete('pdftex/tikz'); landedAt.set('pdftex/tikz', 3);
  // the check that runs once all landed sees them: it is shown, no endless rerun
  assert.deepEqual(lacked(ids.slice(0, 3), asked, [inFlight, background], landedAt, 3), []);
});
// Typeset rows: final rows win. A preview (a lax run's rows, page breaks marked maybe) of a text whose final rows
// are shown, or any older rows, never replaces them. Pins "page 2 → 3 maybe" and "page 1–4" on a one-page text.
test("Typeset rows", async () => {
  const { rowsWin } = self.satexViews;
  assert.equal(rowsWin(undefined, 3, true), true, 'nothing final yet: a preview shows');
  assert.equal(rowsWin(5, 5, true), false, 'a preview of the text already final');
  assert.equal(rowsWin(5, 4, true), false);
  assert.equal(rowsWin(5, 4, false), false, 'older final rows');
  assert.equal(rowsWin(5, 5, false), true);
  assert.equal(rowsWin(5, 6, true), true, 'a newer text\'s preview');
});
// Tips (tips.json): each stays true, its evidence still stands where it says.
test("Tips (tips.json)", async () => {
  const tips = JSON.parse(readFileSync(new URL('./tips.json', SITE), 'utf8'));
  assert.ok(tips.length >= 50, 'a collection of at least 50');
  assert.equal(new Set(tips.map((t) => t[0])).size, tips.length, 'no tip twice');
  assert.ok(tips.filter((t) => t[0].includes('satex gui')).length >= 3, 'a few name the local playground');
  for (const [text, file, evidence] of tips) {
    assert.ok(readFileSync(new URL(`../../${file}`, SITE), 'utf8').includes(evidence), `${text}: no "${evidence}" in ${file}`);
    assert.ok(text.length <= 105 && !text.includes(';'), `${text}: short, no semicolon`);
    assert.ok(!/\bsatex\b(?![^`]*`)/.test(text.replace(/`[^`]*`/g, '')), `${text}: the text says SaTeX, not satex`);
  }
  // no repeat until the list is done, also across a reshuffle: a tip never follows itself
  let seed = 7;
  const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const bag = self.satexViews.tipBag(tips.map((t) => t[0]), random), shown = Array.from({ length: tips.length * 4 }, bag);
  for (let i = 0; i < 4; i++) assert.equal(new Set(shown.slice(i * tips.length, (i + 1) * tips.length)).size, tips.length, 'one round shows each once');
  assert.ok(shown.every((t, i) => t !== shown[i - 1]));
});

// Compile in the browser: a build's message names where it looked, and where else it can run.
test("Compile in the browser", async () => {
  globalThis.location = { host: 'example.org' };
  load('./tex.js');
  const { advice, parseLog } = self.satexTex;
  const run = (over) => ({ asked: ['tex.example', 'example.org'], dead: [], failed: [], biblatex: false, ...over });
  const missing = "! LaTeX Error: File `x.sty' not found.";
  const asked = advice(missing, run(), 'pdftex');
  assert.ok(asked.includes('x.sty') && asked.includes('tex.example, this site') && asked.includes('satex-gui'), asked);
  const down = advice(missing, run({ dead: ['tex.example'], failed: ['x.sty'] }), 'pdftex');
  assert.ok(/No answer from tex\.example/.test(down) && down.includes('satex-gui'), down);
  const off = advice(missing, run({ asked: [] }), 'pdftex');
  assert.ok(/Downloads are off/.test(off) && off.includes('satex-gui'), off);
  assert.equal(advice('all fine', run(), 'pdftex'), '', 'a clean build says nothing');
  assert.ok(/pdfTeX only/.test(advice('', run(), 'luatex')));
  // a log's error lands on its project file, one from a TeX Live file on the main file
  const records = parseLog("(./sub.tex\n./sub.tex:3: Undefined control sequence.\n)\n/tmp/texlive_remote/26_x.sty:9: Oops\n", new Set(['main.tex', 'sub.tex']), 'main.tex');
  assert.deepEqual(records.map((r) => [r.path, r.line, r.severity]), [['sub.tex', 3, 'error'], ['main.tex', 9, 'error']]);
  delete globalThis.location;
});
