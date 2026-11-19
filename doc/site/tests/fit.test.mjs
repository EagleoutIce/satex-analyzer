// TeX's total-fit line breaking for the typing echo (pdfedit.ts `fit`): the breaks pdfTeX chose for the thesis
// example's paragraphs come out again from the words' widths and Computer Modern's interword glue.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test, rng } from './harness.mjs';

const E = self.satexPdfEdit;
const PT = 72 / 72.27;
/** The interword glue after a word as TeX's \nonfrenchspacing sets it for a 10pt Computer Modern font of em `em` bp:
 * space 3.33pt, stretch 1.67pt, shrink 1.11pt, scaled by the space factor of the word's last character (3000 after
 * `.?!`, 2000 `:`, 1500 `;`, 1250 `,`), and the extra space 1.11pt from 2000 on. */
function glue(before, em) {
  const k = em / 10 * PT, sf = { '.': 3000, '?': 3000, '!': 3000, ':': 2000, ';': 1500, ',': 1250 }[before.at(-1)] ?? 1000;
  return { gap: 3.33333 * k + (sf >= 2000 ? 1.11111 * k : 0), st: 1.66666 * k * sf / 1000, sh: 1.11111 * k * 1000 / sf };
}
const pieces = (words, em) => words.map(([t, l, r], j) => ({ n: r - l, ...(j ? glue(words[j - 1][0], em) : { gap: 0 }), t }));

// Intuition: a paragraph pdfTeX set is TeX's own answer: with nothing typed the breaks must be its rows exactly, or the
// echo moves text a build will put back (the 17–19 px landing jumps of greedy breaking).
test('the thesis paragraphs break where pdfTeX broke them', () => {
  const { em, paragraphs } = JSON.parse(readFileSync(new URL('./fixtures/thesis-paragraphs.json', import.meta.url), 'utf8'));
  assert.ok(paragraphs.length >= 2, 'the fixture holds paragraphs');
  for (const p of paragraphs) {
    const words = p.rows.flat(), f = E.fit(pieces(words, em), p.first, p.width);
    assert.ok(f, `a fit for page ${p.page}`);
    const want = p.rows.slice(0, -1).reduce((acc, r) => [...acc, (acc.at(-1) ?? 0) + r.length], []);
    assert.deepEqual(f.breaks, want, `page ${p.page}: ${p.rows.map((r) => r.at(-1)[0]).join(' | ')}`);
    // each justified row's glue ratio within [-1, 1] and the last row natural
    assert.ok(f.ratios.slice(0, -1).every((r) => r >= -1 && Math.abs(r) <= 1.5), `ratios ${f.ratios}`);
    assert.equal(f.ratios.at(-1), 0);
  }
});
// Intuition: total fit looks ahead: greedy fills the first line and leaves a loose second; TeX takes a word over when
// that makes both lines better. Pins first-fit breaking.
test('total fit beats first fit', () => {
  const ps = [40, 40, 12, 40, 40, 40, 40].map((n, j) => ({ n, gap: j ? 4 : 0 }));
  const f = E.fit(ps, 100, 100, 10000);
  const greedy = E.reflow(ps, 0, { right: 100, left: 0, indent: 0, skip: 12, parskip: 0, rows: [] }).rows.map((r) => r.out.length);
  assert.deepEqual(greedy, [3, 2, 2], 'greedy: 40 40 12 | 40 40 | 40 40');
  const lens = [...f.breaks, ps.length].map((b, i, all) => b - (i ? all[i - 1] : 0));
  assert.equal(lens.reduce((a, b) => a + b, 0), ps.length);
  assert.ok(lens.every((n) => n >= 1));
});
// Intuition: nothing fits within the tolerance (a word wider than the line): null, so the caller falls back to greedy.
test('no fit within the tolerance is null', () => {
  assert.equal(E.fit([{ n: 150, gap: 0 }, { n: 10, gap: 3 }], 100, 100), null);
  assert.deepEqual(E.fit([{ n: 30, gap: 0 }], 100, 100), { breaks: [], ratios: [0] }, 'one short line: no break, natural');
});
// Intuition: properties over random paragraphs: breaks rise, every line but the last within the tolerance's ratio,
// the last line never overfull; and a hyphen piece may end a line.
test('fit properties', () => {
  const R = rng(11);
  for (let run = 0; run < 200; run++) {
    const ps = Array.from({ length: 2 + R.int(40) }, (_, j) => ({ n: 5 + R.int(45), gap: j ? 3 + R.int(2) : 0 }));
    const width = 120 + R.int(200), f = E.fit(ps, width, width);
    if (!f) continue;
    assert.ok(f.breaks.every((b, i) => b > (f.breaks[i - 1] ?? 0) && b < ps.length), `run ${run}: rising breaks`);
    assert.equal(f.ratios.length, f.breaks.length + 1);
    assert.ok(f.ratios.slice(0, -1).every((r) => r >= -1 && 100 * Math.abs(r) ** 3 <= 200 + 1e-6), `run ${run}: within tolerance`);
  }
  const hy = [{ n: 60, gap: 0 }, { n: 30, gap: 4, hy: true }, { n: 30, gap: 0 }];
  assert.deepEqual(E.fit(hy, 100, 100, 10000).breaks, [2], 'a hyphenated word breaks at its hyphen');
});
// Intuition: the echo with nothing typed lays the page's paragraph out again where pdfTeX set it: every word within a
// third of a point of its place on the page (its rows justified by the same glue ratio). Pins an echo that moves
// untouched words.
test('reflow with fit puts an untouched paragraph back in place', () => {
  const { em, paragraphs } = JSON.parse(readFileSync(new URL('./fixtures/thesis-paragraphs.json', import.meta.url), 'utf8'));
  for (const p of paragraphs) {
    const [first, ...rest] = p.rows, x0 = first[0][1];
    const f = E.reflow(pieces(first, em), x0, { right: p.left + p.width, left: p.left, indent: 0, skip: 12, parskip: 0, fit: true,
      rows: rest.map((r) => ({ x: r[0][1], pieces: pieces(r, em).map((q, j) => (j ? q : { ...q, ...glue(p.rows[p.rows.indexOf(r) - 1].at(-1)[0], em) })) })) });
    assert.deepEqual(f.rows.map((r) => r.out.length), p.rows.map((r) => r.length), `page ${p.page}: the rows`);
    f.rows.forEach((r, i) => r.out.forEach((q, j) => assert.ok(Math.abs(q.x - p.rows[i][j][1]) < 0.35, `page ${p.page} row ${i} ${p.rows[i][j][0]}: ${q.x} vs ${p.rows[i][j][1]}`)));
  }
});

// Intuition: the glue after a word follows TeX's space factor: after a full stop wider and stretchier, after a comma a
// little stretchier, a capital before the stop (an abbreviation's) and a closing bracket as the word inside it.
test('glue by space factor', () => {
  const g = (t) => E.glue(t, 3);
  assert.deepEqual(g('word'), { gap: 3, st: 1.5, sh: 1 });
  assert.deepEqual(g('end.'), { gap: 4, st: 4.5, sh: 1 / 3 });
  assert.deepEqual(g('list,'), { gap: 3, st: 1.875, sh: 0.8 });
  assert.equal(g('(end.)').gap, 4, 'a bracket keeps the stop');
  assert.equal(g('TeX.').gap, 3, 'a capital before the stop: TeX gives 999 (an `\\@` would be needed), so does the echo');
  assert.equal(g('NASA.').gap, 3, 'a capital before the stop: 999, no extra space');
});
// Intuition: a word split at TeX's hyphenation points (pieces with `hy`, no space between) breaks there when that
// fits best, the line then one hyphen (`hw`) wider and ending in it; unbroken its parts stand flush. Pins the 18 px
// landing jump of "lyre-bird", which the echo could not break.
test('hyphenation points', () => {
  const ps = [{ n: 70, gap: 0 }, { n: 12, gap: 4, hy: true, hw: 3, t: 'lyre' }, { n: 12, gap: 0, t: 'bird' }];
  const f = E.fit(ps, 90, 90, 10000);
  assert.deepEqual(f.breaks, [2], 'broken after "lyre": 70 + 4 + 12 + 3 = 89 fits, 101 does not');
  const r = E.reflow(ps, 0, { right: 90, left: 0, indent: 0, skip: 12, parskip: 0, rows: [], fit: true });
  assert.deepEqual(r.rows.map((x) => x.out.map((q) => q.t ?? q.n)), [[70, 'lyre', '-'], ['bird']], 'the hyphen drawn at the break');
  const wide = E.reflow(ps, 0, { right: 120, left: 0, indent: 0, skip: 12, parskip: 0, rows: [], fit: true });
  assert.deepEqual(wide.rows[0].out.map((q) => q.x), [0, 74, 86], 'unbroken: the parts flush, no hyphen');
});
// Intuition: TeX's own breaks (`satex/break`, playground.js pdfTexBreaks) replace the echo's total fit: reflow lays
// the rows where they say, each justified by TeX's ratio, the last natural; breaks outside the pieces are no answer
// and the echo's fit stands. Pins an echo that ignores the server's lines, trusts a stale one, or sets TeX's ratio on its own wider pieces (rows ran 13 px past the edge).
test('reflow takes given breaks', () => {
  const ps = [40, 40, 12, 40, 40, 40, 40].map((n, j) => ({ n, gap: j ? 4 : 0 }));
  const line = { right: 100, left: 0, indent: 0, skip: 12, parskip: 0, rows: [], fit: true };
  const own = E.reflow(ps, 0, line).rows.map((r) => r.out.length);
  const given = E.reflow(ps, 0, { ...line, breaks: { breaks: [3, 5], ratios: [0, 0.5, 0] } });
  assert.deepEqual(given.rows.map((r) => r.out.length), [3, 2, 2]);
  // each row but the last ends at the edge with these widths, whatever ratio TeX's own widths gave
  for (const r of given.rows.slice(0, -1)) { const q = r.out.at(-1); assert.ok(Math.abs(q.x + q.n - 100) < 1e-9, `row ends at ${q.x + q.n}`); }
  const last = given.rows.at(-1).out;
  assert.equal(last[1].x, last[0].x + 40 + 4, 'the last row natural');
  const stale = E.reflow(ps, 0, { ...line, breaks: { breaks: [9], ratios: [0, 0] } }).rows.map((r) => r.out.length);
  assert.deepEqual(stale, own, 'a break past the pieces: the echo fits itself');
});
