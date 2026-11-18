// Seeded random LaTeX sources for the property tests: valid documents built from the constructs Type mode meets
// (groups, arguments, math in every form, escapes, keys, verbatim, comments, lists, tables, paragraphs).

const WORDS = ['alpha', 'beta', 'word', 'text', 'Lorem', 'ipsum', 'cat', 'sat', 'on', 'the', 'mat', 'x'];
const MATH = ['x', 'y_1', 'a^{2}', '\\alpha', '\\frac{a}{b}', '+', '=', '{x}', 'z_{i}', '2'];

/** A formula's body: a few math tokens. */
const math = (R) => Array.from({ length: 1 + R.int(3) }, () => R.pick(MATH)).join(' ');
/** Inline text, `d` levels of groups deep at most: no blank line, no environment. */
export function inline(R, d = 0) {
  const n = 1 + R.int(5), out = [];
  for (let k = 0; k < n; k++) {
    const p = R.int(d >= 2 ? 6 : 16);
    out.push([
      () => R.pick(WORDS), () => R.pick(WORDS), () => R.pick(WORDS), () => R.pick(['\\%', '\\_', '\\&', '\\$', '\\textbackslash{}', '~', ',', '.']),
      () => `$${math(R)}$`, () => `\\(${math(R)}\\)`,
      () => `\\emph{${inline(R, d + 1)}}`, () => `\\textbf{${inline(R, d + 1)}}`, () => `{\\bfseries ${inline(R, d + 1)}}`,
      () => `\\footnote{${inline(R, d + 1)}}`, () => `\\label{sec:${R.pick(WORDS)}}`, () => `\\ref{sec:${R.pick(WORDS)}}`,
      () => `\\cite[p.~${R.int(9)}]{key${R.int(3)}}`, () => '\\verb|a{b$|', () => '\\TeX{}', () => '\\LaTeX\\ ',
    ][p]());
  }
  return out.join(' ');
}
/** A block of a document body. */
const block = (R) => [
  () => inline(R), () => inline(R), () => inline(R),
  () => `\\section{${inline(R, 1)}}`, () => '% a comment {$ here', () => `\\[ ${math(R)} \\]`,
  () => `\\begin{itemize}\n  \\item ${inline(R, 1)}\n  \\item[x] ${inline(R, 1)}\n\\end{itemize}`,
  () => `\\begin{equation}\n  ${math(R)}\n\\end{equation}`, () => '\\begin{verbatim}\nraw {$ % _\n\\end{verbatim}',
  () => `\\begin{tabular}{ll}\n${R.pick(WORDS)} & ${R.pick(WORDS)} \\\\\n${R.pick(WORDS)} & $x$\n\\end{tabular}`,
][R.int(10)]();
/** A document body: blocks parted by line breaks and blank lines. */
export function source(R, blocks = 3 + R.int(6)) {
  let out = '';
  for (let k = 0; k < blocks; k++) out += block(R) + (R.chance(0.4) ? '\n\n' : '\n');
  return out;
}
