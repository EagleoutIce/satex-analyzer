// The Draw dialog's formula bar (mathlayout.ts `satexMath.formula`): templates with slots, the caret walking them,
// symbols put in, and the clean LaTeX it writes.
import assert from 'node:assert/strict';
import { test } from './harness.mjs';

const F = self.satexMath.formula;
const empty = { src: '', at: 0, to: 0 };
/** The steps `[op, arg]` done from an empty formula: `wrap` a template, `put` text, `next` slot. */
const run = (steps, f = empty) => steps.reduce((g, [op, arg]) => (op === 'next' ? F.next(g) : F[op](g, arg)), f);

// Intuition: a template's first slot takes the caret, Tab (`next`) the one after it, and the result reads as typed by
// hand: `\frac{a}{b}`. Pins the bar's core loop.
test('a fraction filled slot by slot', () => {
  const f = run([['wrap', 'frac'], ['put', 'a'], ['next'], ['put', 'b']]);
  assert.equal(f.src, '\\frac{a}{b}');
  assert.equal(F.clean(f.src), '\\frac{a}{b}');
  assert.deepEqual(run([['wrap', 'frac']]), { src: '\\frac{}{}', at: 6, to: 6 }, 'the caret in the numerator');
});

// Intuition: a script attaches to what stands before the caret; a one-token script loses its braces, an empty one
// goes, and a command's name never runs into a letter (`\alpha b`, never `\alphab`). Pins clean output of scripts.
test('scripts: braces only where needed, empty ones dropped, names kept apart', () => {
  assert.equal(F.clean(run([['put', 'x'], ['wrap', 'sup'], ['put', '2']]).src), 'x^2');
  assert.equal(F.clean(run([['put', 'x'], ['wrap', 'sub'], ['put', '\\alpha'], ['next'], ['put', 'b']]).src), 'x_\\alpha b');
  assert.equal(F.clean('x^{i+1}'), 'x^{i+1}', 'more than one token keeps its braces');
  assert.equal(F.clean('\\sum_{}^{}x'), '\\sum x', 'empty limits dropped, the blank before x kept');
  assert.equal(F.clean('x^{_{}}'), 'x', 'a script of an empty script');
  assert.equal(F.put({ src: '\\alpha', at: 6, to: 6 }, 'x').src, '\\alpha x', 'a letter after a name');
  assert.equal(F.put({ src: 'x', at: 0, to: 0 }, '\\beta').src, '\\beta x', 'a name before a letter');
});

// Intuition: a selection goes into the template's first slot and the caret on to its next (or past it); a sum's
// limits are its slots. Pins wrapping what is already there.
test('a selection wrapped, the caret on to the next slot', () => {
  assert.deepEqual(F.wrap({ src: 'a+b', at: 0, to: 3 }, 'sqrt'), { src: '\\sqrt{a+b}', at: 10, to: 10 }, 'no slot left: past the root');
  const f = F.wrap({ src: 'a+b', at: 0, to: 3 }, 'frac');
  assert.equal(f.src, '\\frac{a+b}{}');
  assert.equal(f.at, f.src.length - 1, 'into the denominator');
  const s = run([['wrap', 'sum'], ['put', 'i=1'], ['next'], ['put', 'n'], ['next']]);
  assert.equal(F.clean(F.put({ ...s, at: s.src.length, to: s.src.length }, 'x_i').src), '\\sum_{i=1}^n x_i');
});

// Intuition: a matrix's empty cells are slots in reading order, and the LaTeX keeps one blank around `&` and `\\`.
// Pins the 2×2 template.
test('a matrix filled cell by cell', () => {
  const f = run([['wrap', 'matrix'], ['put', 'a'], ['next'], ['put', 'b'], ['next'], ['put', 'c'], ['next'], ['put', 'd']]);
  assert.equal(F.clean(F.wrap(empty, 'matrix').src), '\\begin{pmatrix} & \\\\ & \\end{pmatrix}', 'empty cells: one blank apart');
  assert.equal(F.clean(f.src), '\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}');
  assert.equal(F.clean('a \\& b'), 'a \\& b', 'an escaped & is no column');
});

// Intuition: the bar draws the formula with its empty slots as boxes, labelled with the clean LaTeX. Pins the preview.
test('the drawn formula shows empty slots', () => {
  const svg = F.svg('\\frac{}{}');
  assert.equal((svg.match(/□/g) ?? []).length, 2, 'two boxes');
  assert.match(svg, /aria-label="\\frac\{\}\{\}"/);
  assert.equal((F.svg('\\begin{pmatrix}a&\\\\&\\end{pmatrix}').match(/□/g) ?? []).length, 3, 'empty cells too');
  assert.match(F.svg(''), /□/, 'an empty formula is one box');
});

// Intuition: a symbol typed as Unicode (a phone keyboard's α, ≤) is written as its command, which pdfTeX's math reads;
// a character the table does not know stays. Pins `\(α ≤ x\)` failing with pdflatex ("Unicode character α").
test('Unicode symbols typed into the formula become commands', () => {
  assert.equal(F.clean('α≤x'), '\\alpha\\le x');
  assert.equal(F.clean('x^{α}y'), 'x^\\alpha y');
  assert.equal(F.clean('a→b ∑'), 'a\\to b \\sum', 'the first name the table gives a character');
  assert.equal(F.clean('ж'), 'ж', 'unknown: kept');
});
