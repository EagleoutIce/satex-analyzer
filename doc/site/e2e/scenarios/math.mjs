import { MARKUP, is } from './sources.mjs';

export default [
  { name: 'math-burst', word: is('Literate'), src: /Literate/, keys: [' cost $x_1$ and $a+b$ or x'], speed: 'burst', marker: 'cost', expect: () => null, math: [['x', '1', 'sub']], contains: [/x_\{?1\}?/, /a\+b/], drawMs: 8,
    steps: 'thesis.tex; click the end of "Literate"; burst " cost $x_1$ and $a+b$ or x": both formulas written, echoed in math italic with the subscript lowered' },
  // the live math preview (user 2026-11-18): letters in math italic, scripts smaller and moved, a fraction stacked,
  // a display formula on its own row; the build then lands with no jump
  { name: 'math-preview', word: is('Literate'), src: /Literate/, keys: [' cost $x_1+\\alpha^2$ and $\\frac{a}{b}$ ok'], expect: () => null, contains: [/x_\{?1\}?\+\\alpha\^\{?2\}?/, /\\frac\{a\}\{b\}/],
    land: ['ok'], math: [['x', '1', 'sub'], ['α', '2', 'sup'], ['a', 'b', 'over']], italic: ['x', 'a'],
    steps: 'thesis.tex; click the end of "Literate"; type " cost $x_1+\\alpha^2$ and $\\frac{a}{b}$ ok": the echo shows x with 1 lowered, α with 2 raised, a over b' },
  // a command's symbol in the echo (user 2026-11-22: typing `\cdot` broke the preview, `a \cdot b` echoed as `a b`):
  // ⋅ spaced as a binary operator, α as a letter, and the build lands where the preview drew
  { name: 'math-cdot-alpha', settle: true, source: MARKUP, example: 'hello.tex', word: is('paragraph.'), src: /second paragraph\./, keys: [' so $a\\cdot b+\\alpha$ ok'], expect: () => null, contains: [/a\\cdot b\+\\alpha *(\\\)|\$) ok/],
    land: ['ok'], ever: [/⋅/, /α/], never: [/a\s*b\+/],
    steps: 'hello.tex as MARKUP; click the end of "paragraph."; type " so $a\\cdot b+\\alpha$ ok": the echo shows a ⋅ b+α (was: "a b"), ok lands where the preview drew it' },
  { name: 'math-cdot-into', source: '\\documentclass{article}\n\\begin{document}\nSome text with $x$ in it and more words.\n\\end{document}\n', example: 'hello.tex', word: { word: ['x'] }, src: /\$x/, keys: ['\\cdot y+\\alpha'], expect: () => null, contains: [/\$x\\cdot y\+\\alpha *\$/],
    land: ['in'], ever: [/⋅/, /α/], never: [/x\s*y\+/],
    steps: 'a line with $x$; click the end of the math x; type "\\cdot y+\\alpha" into the formula: the echo shows x ⋅ y+α, "in" lands where the preview drew it' },
  { name: 'math-display-preview', source: MARKUP, example: 'hello.tex', word: is('paragraph.'), src: /second paragraph\./, keys: [' so', 'MathDisplay', 'y=\\frac{a}{b}', 'Escape', ' ok'], expect: () => null, contains: [/\\\[\s*y=\\frac\{a\}\{b\}\s*\\\]\s*ok/],
    land: ['ok'], math: [['so', 'y', 'below'], ['y', 'ok', 'below'], ['a', 'b', 'over']],
    steps: 'hello.tex as MARKUP; click the end of "paragraph."; type " so", the display-math control, "y=\\frac{a}{b}", Escape, " ok": the formula on a row of its own below, "ok" on the next' },
  // the shared math layout (mathlayout.ts, user 2026-11-19): a sum's scripts, an integral's limits, \\lim's limit below, a fraction
  { name: 'math-sum', word: is('Literate'), src: /Literate/, keys: [' cost $\\sum_{i=1}^n x_i^2$ ok'], expect: () => null, contains: [/\\sum_\{\{?i=1\}\}?\^\{?n\}? ?x_\{?i\}?\^\{?2\}?/],
    land: ['ok'], math: [['x', 'i', 'sub'], ['x', '2', 'sup'], ['∑', 'n', 'sup']], italic: ['x', 'i'],
    steps: 'thesis.tex; click the end of "Literate"; type " cost $\\sum_{i=1}^n x_i^2$ ok": n raised and i lowered beside the sum (inline), x with i lowered and 2 raised' },
  { name: 'math-integral', source: MARKUP, example: 'hello.tex', word: is('paragraph.'), src: /second paragraph\./, keys: [' so', 'MathDisplay', '\\int_0^1 f(x)\\,dx = \\lim_{n\\to\\infty} \\frac{a}{b}', 'Escape', ' ok'], expect: () => null,
    contains: [/\\\[\s*\\int_\{?0\}?\^\{?1\}? ?f\(x\)\\,dx = \\lim_\{\{?n\\to\s?\\infty\s?\}\}? ?\\frac\{a\}\{b\}\s*\\\]\s*ok/],
    land: ['ok'], math: [['so', '∫', 'below'], ['∫', '1', 'sup'], ['∫', '0', 'sub'], ['lim', 'n', 'sub'], ['a', 'b', 'over'], ['∫', 'ok', 'below']],
    steps: 'hello.tex as MARKUP; click the end of "paragraph."; type " so", the display-math control, the integral formula, Escape, " ok": a display integral with its limits beside, \\lim with n→∞ below, a over b' },
  { name: 'display-math', source: MARKUP, example: 'hello.tex', word: is('paragraph.'), src: /second paragraph\./, keys: [' so', 'Enter', '\\[', 'y=2', 'Escape'], expect: () => null, contains: [/\\\[\s*y=2/], math: [['so', 'y', 'below']],
    steps: 'a small doc; click the end of "paragraph."; type " so", Enter, \\[ y=2: a display formula, echoed on its own centered row' },
  { name: 'math', word: { word: ['n', 'n,'] }, src: /of \$n/, keys: ['2'], expect: (t, a) => t.slice(0, a) + '2' + t.slice(a),
    steps: 'click the end of the math "n" in "A run of $n$ lines"; type 2' },
];
