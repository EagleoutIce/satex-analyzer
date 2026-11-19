// Math laid out from its source (mathlayout.ts `layout`): the PDF's live preview of typed math draws these boxes,
// the Typeset view takes the same parameters (`css`). Relations, not pixels: what is above, beside, smaller, apart.
import assert from 'node:assert/strict';
import { test, rng } from './harness.mjs';

const M = self.satexMath, P = M.PARAMS;
/** Half an em a character, as a width only: heights and depths are the layout's own guesses. */
const w = (s) => [...s].length * 0.5;
/** A metric with ink: capitals and digits 0.7 high, small letters 0.43, descenders 0.2 deep, delimiters 0.75 / 0.25. */
const ink = (t) => [[...t].length * 0.5, /[()[\]{}|∑∫√]/.test(t) ? 0.75 : /[A-Z0-9dfhkl]/.test(t) ? 0.7 : 0.43, /[()[\]{}|∑∫]/.test(t) ? 0.25 : /[gjpqy]/.test(t) ? 0.2 : 0];
const lay = (s, d = false, m = w) => M.layout(s, m, d);
const at = (b, t, k = 0) => b.g.filter((g) => g[0] === t)[k];
/** A glyph's size: its height's scale (a script's is set wider, its height not). */
const sz = (g) => +(g[3] * (g[5] ?? 1)).toFixed(9);
/** A glyph's ink box `[left, right, bottom, top]` under `ink`, its stretch applied. */
const inkBox = (g) => { const [gw, h, d] = ink(g[0]), sy = g[5] ?? 1; return [g[1], g[1] + gw * g[3], g[2] - d * g[3] * sy, g[2] + h * g[3] * sy]; };
const apart = (p, q) => p[1] <= q[0] + 1e-9 || q[1] <= p[0] + 1e-9 || p[3] <= q[2] + 1e-9 || q[3] <= p[2] + 1e-9;

// Intuition: letters are math italic, digits upright, a script one size down (0.7, then 0.5). Pins "(x1)" in roman
// for `\(x_1\)` (user 2026-11-18).
test('faces and script sizes', () => {
  const b = lay('x_1^{a_2}');
  assert.deepEqual([at(b, 'x')[4], at(b, '1')[4], sz(at(b, '1')), sz(at(b, 'a')), sz(at(b, '2'))], ['it', 'rm', 0.7, 0.7, 0.5]);
  assert.equal(at(b, '1')[3], 0.7 * P.script, 'a script set wider, as cmr7 is');
  assert.deepEqual(at(lay('\\alpha'), 'α').slice(3, 5), [1, 'it'], 'Greek lowercase in math italic');
  assert.deepEqual(at(lay('\\Gamma'), 'Γ').slice(3, 5), [1, 'rm'], 'Greek capitals upright');
});

// Intuition: a superscript is raised by the font's sup shift (σ13 in display, σ14 in text, σ15 cramped), a subscript
// lowered by σ16 alone and by σ17 under a superscript, the two four rule widths apart at least (Appendix G, rule 18).
test('scripts: raised and lowered by the font parameters', () => {
  assert.equal(at(lay('x^2'), '2')[2], P.sup2, 'text style: sup2');
  assert.equal(at(lay('x^2', true), '2')[2], P.sup1, 'display: sup1');
  assert.equal(at(lay('\\sqrt{x^2}'), '2')[2], P.sup3, 'cramped under a radical: sup3');
  assert.equal(at(lay('x_1'), '1')[2], -P.sub1, 'a subscript alone: sub1');
  assert.equal(at(lay('x_i^2'), 'i')[2], -P.sub2, 'under a superscript: sub2');
  const b = lay('x_{\\frac{a}{b}}^{\\frac{c}{d}}', false, ink), sup = Math.min(...b.g.filter((g) => /[cd]/.test(g[0])).map((g) => inkBox(g)[2])), sub = Math.max(...b.g.filter((g) => /[ab]/.test(g[0])).map((g) => inkBox(g)[3]));
  assert.ok(sup - sub >= 4 * P.rule * 0.7 - 1e-9, `tall scripts kept apart (${sup} over ${sub})`);
  assert.ok(at(lay('x_i^2'), '2')[1] > at(lay('x_i^2'), 'i')[1], 'the italic correction shifts the superscript right');
  assert.deepEqual(lay('x_').g.length, 1, 'a script still being typed: no fault');
});

// Intuition: the spaces between atoms are TeX's table (tex.web §764): medium about a binary operator, thick about a
// relation, thin after punctuation and between an operator and an ordinary, none of the conditional ones in a script;
// a binary operator with no left operand is ordinary (a unary minus).
test('spacing classes', () => {
  const sum = lay('a+b'), rel = lay('a=b'), neg = lay('-x');
  assert.ok(Math.abs(at(sum, '+')[1] - (0.5 + 4 / 18 + (P.sign - 0.5) / 2)) < 1e-9 && Math.abs(at(sum, 'b')[1] - (0.5 + P.sign + 8 / 18)) < 1e-9, 'medium about +, the sign as wide as cmr10 sets it');
  assert.ok(Math.abs(at(rel, 'b')[1] - (0.5 + P.sign + 10 / 18)) < 1e-9, 'thick about =');
  assert.ok(Math.abs(at(neg, 'x')[1] - P.sign) < 1e-9, 'a unary minus: no space');
  assert.ok(Math.abs(at(lay('x_{a+b}'), 'b')[1] - at(lay('x_{a+b}'), '+')[1] - (P.sign * 0.7 + 0.35 * P.script) / 2) < 1e-9, 'no medium space in a script');
  assert.ok(Math.abs(at(lay('\\sin x'), 'x')[1] - (1.5 + 3 / 18)) < 1e-9, 'thin between an operator and an ordinary');
  assert.ok(Math.abs(at(lay('a,b'), 'b')[1] - (1 + 3 / 18)) < 1e-9, 'thin after punctuation');
  for (const [cmd, mu] of [['\\,', 3], ['\\:', 4], ['\\;', 5], ['\\!', -3], ['\\quad', 18], ['\\qquad', 36]]) assert.ok(Math.abs(at(lay(`a${cmd} b`), 'b')[1] - (0.5 + mu / 18)) < 1e-9, cmd);
  assert.equal(M.spaceMu('rel', 'rel', false), 0, 'two relations: none');
});

// Intuition: a fraction's numerator stands above the axis, its denominator below, its rule on the axis; inline its
// parts are one size down, in display full size; \dfrac and \tfrac force the style, \binom has parentheses, no rule.
test('fractions', () => {
  const fr = lay('\\frac{a}{b}');
  assert.ok(at(fr, 'a')[2] > P.axis && at(fr, 'b')[2] < 0 && fr.r.length === 1 && sz(at(fr, 'a')) === 0.7, 'stacked over its rule, one size down');
  assert.ok(Math.abs(fr.r[0][1] + fr.r[0][3] / 2 - P.axis) < 1e-9, 'the rule on the axis');
  assert.equal(sz(at(lay('\\frac{a}{b}', true), 'a')), 1, 'full size in display');
  assert.equal(sz(at(lay('\\dfrac{a}{b}'), 'a')), 1, '\\dfrac inline: full size');
  assert.equal(sz(at(lay('\\tfrac{a}{b}', true), 'a')), 0.7, '\\tfrac in display: one size down');
  const bi = lay('\\binom{n}{k}');
  assert.ok(bi.r.length === 0 && at(bi, '(') && at(bi, ')') && at(bi, 'n')[2] > at(bi, 'k')[2], '\\binom: parentheses, no rule');
  assert.equal(sz(at(lay('\\frac12'), '1')), 0.7, '\\frac12 reads one digit an argument');
  assert.ok(at(lay('a \\over b'), 'a')[2] > at(lay('a \\over b'), 'b')[2], '\\over');
});

// Intuition: in display a large operator's scripts are limits, centered above and below it (rule 13a); inline they
// stand beside; \limits and \nolimits overrule; an integral keeps them beside (plain's \int is \nolimits);
// \lim, \max take limits in display, \sin never.
test('limits', () => {
  const ink3 = (b) => b.g.map((g) => [g[0], inkBox(g)]);
  const sum = lay('\\sum_{i=1}^n', true, ink), S = inkBox(at(sum, '∑')), N = inkBox(at(sum, 'n')), I = inkBox(at(sum, 'i'));
  assert.ok(N[2] > S[3] && I[3] < S[2], `display: n above, i below (${JSON.stringify(ink3(sum))})`);
  assert.ok(Math.abs((N[0] + N[1]) / 2 - (S[0] + S[1]) / 2) < 1e-9, 'centered over the operator');
  assert.ok((S[3] + S[2]) / 2 - P.axis < 1e-9 && (S[3] + S[2]) / 2 - P.axis > -1e-9, 'the operator centered on the axis');
  const txt = lay('\\sum_{i=1}^n', false, ink);
  assert.ok(at(txt, 'n')[1] >= inkBox(at(txt, '∑'))[1] - 1e-9, 'inline: beside');
  assert.ok(at(lay('\\sum\\limits_{i}', false, ink), 'i')[2] < -0.5, '\\limits inline: below');
  assert.ok(at(lay('\\sum\\nolimits_{i}', true, ink), 'i')[1] > 0.5, '\\nolimits in display: beside');
  const int = lay('\\int_0^1', true, ink), F = inkBox(at(int, '∫'));
  assert.ok(at(int, '1')[1] > F[0] && at(int, '0')[1] < at(int, '1')[1] && F[3] - F[2] > 2, 'a display integral: tall, its scripts beside, the lower tucked in');
  assert.ok(at(lay('\\lim_{n\\to\\infty}', true), 'n')[2] < -0.3 && at(lay('\\lim_{n}'), 'n')[1] >= 1.5, '\\lim: limits in display only');
  assert.ok(at(lay('\\sin^2', true), '2')[1] >= 1.5, '\\sin: never limits');
  assert.ok(at(lay('\\operatorname*{arg\\,max}_x', true), 'x')[2] < 0 && at(lay('\\operatorname{Var}'), 'Var')[4] === 'rm', 'operator names');
});

// Intuition: a radical's rule runs over its body a clearance above it, the sign stretched as tall; an index is set
// small, before the sign; \left…\right delimiters grow to cover what they enclose.
test('radicals and delimiters', () => {
  const r = lay('\\sqrt{\\frac{a}{b}}', false, ink), rule = r.r.at(-1), A = inkBox(at(r, 'a'));
  assert.ok(rule[1] > A[3] && at(r, '√')[5] > 1, 'the rule above the body, the sign stretched');
  const ix = lay('\\sqrt[3]{x}');
  assert.ok(sz(at(ix, '3')) === 0.5 && at(ix, '3')[1] < at(ix, 'x')[1] && at(ix, '3')[2] > 0.2, 'an index: scriptscript, raised, before');
  const f = lay('\\left( \\frac{\\frac{a}{b}}{c} \\right)', true, ink), L = inkBox(at(f, '(')), T = Math.max(...f.g.filter((g) => /[abc]/.test(g[0])).map((g) => inkBox(g)[3])), B = Math.min(...f.g.filter((g) => /[abc]/.test(g[0])).map((g) => inkBox(g)[2]));
  assert.ok(L[3] >= 0.9 * T && L[2] <= 0.9 * B, `the parenthesis covers the body (${L} vs ${B}..${T})`);
  assert.ok(Math.abs(inkBox(at(lay('\\left(x\\right)', false, ink), '('))[3] - 0.75) < 1e-9, 'a small body: the natural size');
  assert.ok(at(lay('\\Bigl( x \\Bigr)', false, ink), '(')[5] > 1, '\\Big: a fixed larger size');
  assert.ok(lay('\\left\\{ x \\middle| y \\right.', false, ink).g.length === 4, '\\middle, an empty \\right.');
  assert.doesNotThrow(() => lay('\\left( x'), 'a \\right not typed yet');
});

// Intuition: accents stand over their nucleus, \overline's rule above it, \underline's below; the alphabets map to
// their Unicode letters in the math font (`ms`), \text and \mathrm upright, \mathbf bold.
test('accents and alphabets', () => {
  const h = lay('\\hat{x}', false, ink), x = at(h, 'x'), hat = at(h, 'ˆ');
  assert.ok(hat && hat[1] >= x[1] && hat[1] < x[1] + 0.5, 'the hat over the x');
  assert.ok(at(lay('\\hat{A}', false, ink), 'ˆ')[2] > 0.2, 'over a capital: raised past the x-height');
  assert.ok(at(lay('\\vec{v}', false, ink), '→')[2] > 0.3, '\\vec: an arrow above');
  const o = lay('\\overline{AB}'), u = lay('\\underline{x}');
  assert.ok(o.r[0][1] > 0.6 && o.r[0][2] === 1 && u.r[0][1] < 0, 'over- and underline');
  assert.deepEqual([at(lay('\\mathbb{R}'), 'ℝ')?.[4], at(lay('\\mathcal{A}'), '𝒜')?.[4], at(lay('\\mathbb{Z}'), 'ℤ')?.[4]], ['ms', 'ms', 'ms']);
  assert.equal(at(lay('\\mathbf{x}'), 'x')[4], 'bf');
  assert.equal(at(lay('\\mathrm{d}x'), 'd')[4], 'rm');
  assert.deepEqual(at(lay('x \\text{if } y'), 'if ').slice(3, 5), [1, 'rm']);
  assert.equal(at(lay("f'(x)"), '′')[2] > 0.3, true, 'a prime is a superscript');
});

// Intuition: matrices and cases are grids: cells of a row on one baseline, rows a baseline skip apart, columns as wide
// as their widest cell, the grid centered on the axis inside delimiters that cover it; a display's `&` and `\\`
// (an `align` body) align its rows at the `&`.
test('grids', () => {
  const m = lay('\\begin{pmatrix} a & b+c \\\\ e & d \\end{pmatrix}', false, ink);
  const [a, b, c, d] = ['a', 'b', 'e', 'd'].map((t) => at(m, t));
  assert.ok(a[2] === b[2] && c[2] === d[2] && a[2] - c[2] >= 1.2 - 1e-9 && a[1] === c[1] && b[1] > a[1] + 0.5, 'two rows, two columns');
  assert.ok(Math.abs((a[2] + c[2]) / 2 + (0.43 - 0) / 2 - P.axis) < 0.3, 'about the axis');
  const L = inkBox(at(m, '('));
  assert.ok(L[3] >= a[2] + 0.43 && L[2] <= c[2], 'the parentheses cover both rows');
  const cs = lay('f(x) = \\begin{cases} 1 & x > 0 \\\\ 0 & \\text{otherwise} \\end{cases}');
  assert.ok(at(cs, '{') && at(cs, '1')[1] === at(cs, '0', 1)[1] && at(cs, 'x', 1)[1] === at(cs, 'otherwise')[1], 'cases: left aligned columns');
  const al = lay('a &= b + c \\\\ \\text{dd} &= e', true);
  assert.ok(Math.abs(at(al, '=')[1] - at(al, '=', 1)[1]) < 1e-9 && at(al, 'a')[2] > at(al, 'dd')[2], 'aligned at the &');
  assert.ok(at(lay('a \\\\ b+c', true), 'a')[1] > at(lay('a \\\\ b+c', true), 'b')[1], 'gathered: rows centered');
});

// Intuition: the two formulas the user types (2026-11-19) set with no glyph over another, inline and in display.
test('no glyph overlaps another', () => {
  for (const [src, d] of [['\\sum_{i=1}^n x_i^2', false], ['\\sum_{i=1}^n x_i^2', true], ['\\int_0^1 f(x)\\,dx = \\lim_{n\\to\\infty} \\frac{a}{b}', true], ['\\int_0^1 f(x)\\,dx = \\lim_{n\\to\\infty} \\frac{a}{b}', false]]) {
    const gs = lay(src, d, ink).g.map((g) => [g[0], inkBox(g)]);
    // a slanted integral's lower limit tucks under its box (its italic correction), never under its ink's middle
    for (let i = 0; i < gs.length; i++) for (let j = i + 1; j < gs.length; j++) if (!(gs[i][0] === '∫' && gs[j][1][3] < (gs[i][1][2] + gs[i][1][3]) / 2 && gs[j][1][0] > (gs[i][1][0] + gs[i][1][1]) / 2)) assert.ok(apart(gs[i][1], gs[j][1]), `${src}${d ? ' (display)' : ''}: ${gs[i][0]} over ${gs[j][0]}: ${JSON.stringify([gs[i], gs[j]])}`);
  }
});

// Intuition: typing goes through every prefix of a formula, and people type anything: every prefix of random math
// lays out with finite numbers and no fault, never loops (each call returns).
test('any prefix of random math lays out', () => {
  const r = rng(41), parts = ['x', '^', '_', '{', '}', '\\frac', '\\sqrt', '[', ']', '\\left(', '\\right)', '\\middle|', '\\sum', '\\int', '\\limits', '&', '\\\\', '\\begin{pmatrix}', '\\end{pmatrix}', '\\hat', '\\text{a b}', '\\mathbb', "'", '\\over', '+', '=', '\\alpha', '\\foo', '2', '\\,', '\\left.', '\\right.', '\\not', '\\big('];
  for (let n = 0; n < 300; n++) {
    const src = Array.from({ length: 1 + r.int(12) }, () => r.pick(parts)).join(' ');
    for (let k = 0; k <= src.length; k++) for (const d of [false, true]) {
      const b = lay(src.slice(0, k), d, ink);
      assert.ok([b.w, b.h, b.d, ...b.g.flatMap((g) => [g[1], g[2], g[3]]), ...b.r.flat()].every(Number.isFinite), src.slice(0, k));
    }
  }
});

// Intuition: the Typeset view's scripts and spacing come from the same parameters: a superscript raised by TeX's text
// sup shift in em of its own size, the binary operator's medium space.
test('the Typeset view takes the same parameters', () => {
  const c = M.css();
  assert.ok(Math.abs(parseFloat(c['--m-sup']) * parseFloat(c['--m-script']) - P.sup2) < 1e-3, 'sup2 in em of the script');
  assert.equal(c['--m-bin'], `${+(4 / 18).toFixed(3)}em`);
});
