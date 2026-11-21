// References in the echo (pdfedit.ts `prose`, `refFacts`, `refSet`): a `\ref`, `\eqref`, `\cite` typed shows what the run
// sets for it, learned from the run's typeset rows and its own aux, never the key as words. Fixture: rows of
// `satex query typeset` over two MWEs (tests/fixtures/refs-rows.json, their sources beside them).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from './harness.mjs';

const E = self.satexPdfEdit;
const FX = JSON.parse(readFileSync(new URL('./fixtures/refs-rows.json', import.meta.url), 'utf8'));
// the aux lines the run wrote for the two MWEs (the kernel's \newlabel and \bibcite format)
const AUX = ['\\newlabel{sec:a}{{1}{1}}\n\\newlabel{eq:a}{{1}{1}}\n\\newlabel{sec:b}{{2}{1}}\n\\newlabel{sec:c}{{3}{4}}\n\\newlabel{eq:b}{{2}{4}}',
  '\\bibcite{kn}{1}\n\\bibcite{la}{2}\n\\bibcite{ab}{3}'];
const facts = (name) => E.refFacts(FX[name].rows, () => FX[name].tex, AUX);
const words = (raw, f, labelled = () => false) => E.prose(raw, (c, a) => E.refSet(f, c, a, labelled)).map((q) => `${q.gap ? '_' : ''}${q.t}${q.dim ? '?' : ''}`).join(' ');

// Intuition: a call the run already set shows the run's text. Pins the echo drawing `sec:a` for `\ref{sec:a}`.
test('refs: a call the run set shows its text', () => {
  const f = facts('refs');
  assert.equal(E.refSet(f, 'ref', 'sec:b', () => false).t, '2');
  assert.equal(E.refSet(f, 'eqref', 'eq:a', () => false).t, '(1)');
  assert.equal(words('see \\ref{sec:b} now', f), 'see _2 _now');
});
// Intuition: a key the run never referenced, labelled before, shows its number at once, in the command's own shape the
// run's other calls show (`\eqref` wraps it in parentheses), and `\pageref` its page.
test('refs: a new key takes the shape the command set elsewhere', () => {
  const f = facts('refs');
  // `\eqref{eq:a}` set `(1)`: number and page are both 1, so the number is the first guess, not sure
  assert.deepEqual(E.refSet(f, 'eqref', 'eq:b', () => false), { t: '(2)', sure: false });
  // `\ref{sec:b}` set `2` (page 1): the number, sure
  assert.deepEqual(E.refSet(f, 'ref', 'sec:c', () => false), { t: '3', sure: true });
  // `\pageref{sec:a}` set `1`, the number and the page alike: which group it sets is not known, a guess
  assert.equal(E.refSet(f, 'pageref', 'sec:c', () => false).sure, false);
});
// Intuition: a `\label` sets no text, nor does a key still typed (no closing brace). Pins the echo writing the key.
test('refs: a label and an open key set nothing', () => {
  const f = facts('refs');
  assert.equal(words('x\\label{sec:a} y', f), 'x _y');
  assert.equal(words('x \\ref{sec', f), 'x _');
});
// Intuition: a key labelled in the text but not run yet is pending; an undefined one shows the run's own mark (`??`).
test('refs: pending and undefined keys', () => {
  const f = facts('cite');
  assert.equal(E.refSet(f, 'ref', 'nope', () => false).t, '??');
  assert.deepEqual(E.refSet(f, 'ref', 'fresh', (k) => k === 'fresh'), { t: '…', sure: false });
});
// Intuition: citations take the bibliography's labels in the citation command's shape (`[1]`); options stay out of the key.
test('refs: citations', () => {
  const f = facts('cite');
  assert.equal(E.refSet(f, 'cite', 'kn', () => false).t, '[1]');
  assert.equal(E.refSet(f, 'cite', 'ab', () => false).t, '[3]');
  assert.deepEqual(E.refSet(f, 'cite', 'ab,la', () => false), { t: '[3, 2]', sure: false });
  assert.equal(words('as \\cite[p.~2]{kn,la} show', f), 'as _[1, 2, p. 2] _show');
});
