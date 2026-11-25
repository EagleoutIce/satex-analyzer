// Typing on the thesis example's generated text (user 2026-11-25): the glossary, the index, the bibliography, the
// footer's page number, a table after a paragraph.
import { readFileSync } from 'node:fs';
import { is } from './sources.mjs';

const THESIS = readFileSync(new URL('../../examples/thesis.tex', import.meta.url), 'utf8');
// the user's thesis: fancyhdr's footer, the page number its only field
export const FANCY = THESIS.replace('\\begin{document}', '\\usepackage{fancyhdr}\n\\pagestyle{fancy}\n\\fancyhf{}\n\\fancyfoot[C]{\\thepage}\n\\renewcommand{\\headrulewidth}{0pt}\n\\begin{document}');
const PAGEY = `(() => { const s = document.querySelector('#play-pdf .pdf-pages'), c = document.querySelector('#play-pdf .pdf-caret'); return JSON.stringify({ top: Math.round(s.scrollTop), caret: c && Math.round(c.getBoundingClientRect().top), page: c?.closest('.pdf-page')?.dataset.page }); })()`;

const WORDS = (re) => `JSON.stringify(satexPdfWords().filter((w) => ${re}.test(w[5])).map((w) => [w[0], Math.round(w[1]), Math.round(w[4]), w[5]]).slice(0, 40))`;
const ECHO = `JSON.stringify([...document.querySelectorAll('#play-pdf .pdf-echo')].map((x) => x.textContent + '|' + (x.__texts ?? []).map((q) => q.t).join(' ')))`;
const TOAST = `document.querySelector('#play-toast')?.textContent`;
const TABLE = String.raw`\documentclass{article}
\usepackage{array}
\begin{document}
Table~\ref{tab:runs} lists measured runs.
\begin{table}[ht]
  \centering
  \begin{tabular}{lrr}
    Document & Lines & Seconds \\
    article & 800 & 1.6 \\
  \end{tabular}
  \caption{Measured runs}\label{tab:runs}
\end{table}
\end{document}
`;
export default [
  // the user's 06:33 report: typing at the end of a glossary description (page 4) jumped the pages back a page when the
  // build landed
  { name: 'gloss-type-builds', word: is('typesetting'), src: (t) => t.indexOf('typesetting system') + 'typesetting'.length, keys: ['n', 'n', 'Pause', 'g', 'g', 'Pause', 'x'], pauseMs: 7000, minBuilds: 1,
    expect: () => null, contains: [/description=\{the typesettingnnggx system/],
    steps: 'thesis.tex, PDF, Type on; click the end of "typesetting" in the glossary (page 4); type n, n, a pause for the build, g, g, a pause, x: the pages never scroll, the caret never jumps' },
  // the user's 06:31 report: a click on the footer's page number drew a second number beside it (`23` on page 2)
  { kind: 'user', name: 'gen-foot-number', pdf: true, example: 'thesis.tex', source: FANCY, word: is('Contents'),
    do: [{ pdf: '2', page: 2, nth: 1 }, { wait: 800 }, { js: ECHO }],
    expect: [{ js: /^\[\]$/ }],
    steps: 'the thesis with fancyhdr\'s \\fancyfoot[C]{\\thepage}; PDF, Type on; click the page number 2 at the foot of page 2: no echo over it (it drew "2" beside the page\'s own: "23")' },
  { kind: 'user', name: 'gen-foot-type', pdf: true, example: 'thesis.tex', source: FANCY, word: is('Contents'),
    do: [{ pdf: '2', page: 2, nth: 1 }, { wait: 800 }, { type: 'x' }, { wait: 800 }, { js: ECHO }],
    expect: [{ has: /\\fancyfoot\[C\]\{\\thepage\{\}x\}/ }, { js: /2x/ }],
    steps: 'the thesis with fancyhdr; click the end of the page number on page 2 and type x: the footer\'s call reads \\thepage{}x, the echo 2x' },
  // the user's 06:31 report: page 1 (\thispagestyle{empty}) prints no number, yet a click there drew "1"
  { kind: 'user', name: 'gen-foot-empty-page', pdf: true, example: 'thesis.tex', source: FANCY, word: is('Contents'),
    do: [{ js: `document.querySelector('#play-pdf .pdf-page[data-page="1"]').scrollIntoView({ block: 'end' })` }, { wait: 300 }, { click: { page: 1, fx: 0.5, fy: 0.837 } }, { wait: 600 }, { js: `${ECHO} + ' ' + ${TOAST}` }],
    expect: [{ js: /^\[\] Not typed here: page 1 prints no footer/ }],
    steps: 'the thesis with fancyhdr; click where the footer stands on the title page (empty page style): "Not typed here", no 1 drawn' },
  // typing on a contents line's number went into `\tableofcontents` (`\tableofcontentsx`)
  { kind: 'user', name: 'gen-toc-number', pdf: true, example: 'thesis.tex', word: is('Contents'),
    do: [{ pdf: '2', page: 2, nth: 0 }, { wait: 500 }, { js: TOAST }, { type: 'x' }, { wait: 500 }],
    expect: [{ js: /number and page are generated/ }, { not: /\\tableofcontents\S/ }],
    steps: 'thesis.tex; PDF, Type on; click the number 2 of "2 Terms" in the contents and type x: "Not typed here", the source unchanged' },
  // the user's 06:34 report: typing on the printed bibliography did nothing
  { kind: 'user', name: 'gen-bib-field', pdf: true, example: 'thesis.tex', word: is('Journal,'),
    do: [{ pdf: 'Journal,' }, { wait: 1500 }, { type: 'X' }, { wait: 800 }, { js: 'satexHealth().file' }],
    expect: [{ js: /^refs\.bib$/ }],
    steps: 'thesis.tex; PDF, Type on; click the end of "Journal," in the references and type X: refs.bib opens, journal = {The Computer JournalX}' },
  // the user's 06:33 report: `runs. [ht] l>p3.9cmr` echoed after a table's columns changed
  { kind: 'user', name: 'gen-table-spec-echo', pdf: true, example: 'hello.tex', source: TABLE, word: is('runs.'),
    do: [{ text: TABLE.replace('{lrr}', '{l>{\\raggedleft\\arraybackslash}p{3.9cm}r}') }, { pdf: 'runs.' }, { wait: 300 }],
    expect: [{ echoNot: /\[ht\]|3\.9cm|l>p/ }],
    steps: 'a paragraph and a table, built; the column spec changed in the source, then a click at the end of "runs." before the build lands: the echo shows no [ht] or column spec' },
];
