// Widgets on edge cases (kinds/robust.mjs), and Type mode with letters past ASCII (the typing kind).
import { MARKUP, is } from './sources.mjs';

const DOC = (body, pre = '') => `\\documentclass{article}\n${pre}\\begin{document}\n${body}\n\\end{document}\n`;
const TAB = DOC('Before the table.\n\n\\begin{tabular}{ll}\n  a & b \\\\\n  c & d \\\\\n\\end{tabular}\n\nAfter it.');
const NESTED = DOC('Intro.\n\n\\begin{tabular}{lll}\n  \\toprule\n  \\multicolumn{2}{c}{Head} & x \\\\\n  \\midrule\n  \\multirow{2}{*}{alpha} & beta & \\begin{tabular}{@{}c@{}}in1 \\\\ in2\\end{tabular} \\\\\n  & gamma & \\(a+b\\) \\\\\n  \\cmidrule(lr){2-3}\n  delta & {x y} & eps \\\\*\n  \\bottomrule\n\\end{tabular}\n\nEnd.', '\\usepackage{booktabs}\n\\usepackage{multirow}\n');
const par = (i) => `\\section{Part ${i}}\n` + Array.from({ length: 6 }, (_, j) => `Paragraph ${i}.${j} has words to fill the page so that the document grows long, with $x_{${i}}^{${j}} = ${i + j}$ inline and text after it, w${i}x${j}.`).join('\n\n')
  + (i % 4 ? '' : `\n\n\\begin{tabular}{lr}\n  key${i} & ${i} \\\\\n\\end{tabular}\n\n\\begin{equation}\n  y_{${i}} = ${i}x\n\\end{equation}`);
const BIG = DOC(Array.from({ length: 120 }, (_, i) => par(i + 1)).join('\n\n'));

export default [
  { kind: 'robust', check: 'insert', name: 'ins-caret-tabular', text: TAB, caret: 'a & b', k: 4, item: 'Table', widget: 'table',
    want: /\\end\{tabular\}\n\n\\begin\{table\}[^]*\\end\{table\}\n\nAfter it\./,
    steps: 'a tabular; the caret in cell b; Insert, Table: the float after \\end{tabular}, builds (was: written into the cell, "Not in outer par mode")' },
  { kind: 'robust', check: 'insert', name: 'ins-caret-math', text: DOC('So $a+b$ and more.\n\nNext.'), caret: 'a+b', k: 1, item: 'Equation', type: 'x',
    want: /So \$a\+b\$ and more\.\n\n?\\begin\{equation\}\n\s*x\n\\end\{equation\}\n/,
    steps: 'inline math; the caret after a; Insert, Equation, x: after the line, never inside $…$ (was: an equation inside math)' },
  { kind: 'robust', check: 'table', name: 'table-nested-edit', text: NESTED, at: '\\begin{tabular}{lll}', cell: 'beta', to: 'BETA', want: (t) => t.replace('& beta &', '& BETA &'),
    steps: 'booktabs, \\multicolumn, \\multirow, a nested tabular, math, a braced cell, \\\\*; the table dialog: Apply untouched writes nothing; beta set to BETA: only that cell changes' },
  { kind: 'robust', check: 'big', name: 'big-40-pages', text: BIG, pages: 40, word: 'w120x5.', keys: [' Quokka'], contains: '/w120x5\\. Quokka/', ms: 8000,
    steps: '120 sections, over 40 pages, tables and equations; build; Type mode on the last page\'s last word, " Quokka": lands in the source' },
  { kind: 'robust', check: 'screen', name: 'phone-dark-table', width: 390, height: 844, theme: 'dark', text: NESTED, widget: 'table', at: '\\begin{tabular}{lll}',
    steps: '390 x 844 dark; the nested table dialog: inside the screen, no sideways page scroll' },
  { kind: 'robust', check: 'screen', name: 'phone-font-view', width: 390, height: 844, theme: 'light', text: DOC('Text.', '\\usepackage{lmodern}\n'), widget: 'font', at: '\\documentclass', k: 2,
    steps: '390 x 844; the Font and size view at \\documentclass: inside the screen, no control cut off' },
  { kind: 'robust', check: 'screen', name: 'wide-dark-font-view', width: 1400, height: 900, theme: 'dark', text: DOC('Text.', '\\usepackage{lmodern}\n'), widget: 'font', at: '\\documentclass', k: 2,
    steps: 'desktop dark; the Font and size view: a screenshot, inside the screen' },
  { name: 'uni-type-pdf', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' Grüße – naïve café'], speed: 'burst', settle: true, expect: () => null, contains: [/a second Grüße – naïve café/],
    steps: 'MARKUP, Type mode; click the end of "second"; burst " Grüße – naïve café": the source holds them as typed' },
  { name: 'uni-type-backspace', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' Straße😀', 'Backspace', 'Backspace', 'Backspace', 'ße'], speed: 'human', settle: true, expect: () => null, contains: [/a second Straße paragraph/],
    steps: 'MARKUP, Type mode; " Straße😀", three Backspaces (😀, e, ß), "ße": one character as seen per Backspace (was: half of 😀 left, a lone surrogate)' },
];
