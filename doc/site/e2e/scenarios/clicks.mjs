import { is, after, KINDS, at, DEMO } from './sources.mjs';

/** The call `re` matches (its first group, the nth after the preamble), as [start, end]: a click at the edges of its
 * output puts the caret before or after it. */
const call = (re, nth = 0, tail = 0) => (t) => { const b = t.indexOf('\\begin{document}'), m = [...t.slice(b).matchAll(new RegExp(re.source, "g"))][nth]; return m && [b + m.index, b + m.index + m[0].length + tail]; };

export default [
  { kind: 'click', name: 'click-macro-start', source: DEMO, example: 'hello.tex', word: is('TinyNet,'), src: at(/\\newcommand\{\\model\}\{(TinyNet)/), edge: call(/\\model/, 0, 1), len: 7, clickMs: 60000, ready: `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`, 
    steps: 'the landing demo; click the start, middle and end of "TinyNet," (\\model at a paragraph\'s start): the start before the call, the middle in \\model\'s definition, the end after the comma (ui-35: the start was refused; caret2: the edges of an output are the call\'s)' },
  { kind: 'click', name: 'click-macro-typed', source: DEMO.replace('falls fast.', 'falls fast. It drops by \\(x_{42}\\) per step.\n\n\\model{} converges.'), example: 'hello.tex', word: is('TinyNet'), nth: 1, src: at(/\\newcommand\{\\model\}\{(TinyNet)/), edge: call(/(?<=\{)\\model(?=\})/), len: 7, clickMs: 60000, ready: `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`, 
    steps: 'the landing demo; click the start, middle and end of "TinyNet," (the demo typed, then \\model{} first in a new paragraph; the second TinyNet is the picture node\'s): the edges before and after the node\'s \\model, the middle in \\model\'s definition (ui-35: the start was refused)' },
  { kind: 'click', name: 'click-macro-long', source: DEMO.replace('With \\model, the loss falls fast.', '\\model{} makes the loss fall\nfast and then it keeps\non falling for a while\nuntil the run settles\nin the end of the long\nparagraph here.'), example: 'hello.tex', word: is('TinyNet'), src: at(/\\newcommand\{\\model\}\{(TinyNet)/), edge: call(/\\model\{\}/), len: 7, clickMs: 60000, ready: `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`, 
    steps: 'the landing demo; click the start, middle and end of "TinyNet," (\\model{} first in a paragraph of six source lines): the caret in \\model\'s definition (ui-35: the start was refused)' },
  // a click at the start, middle and end of a word in each kind of box: the caret at that offset of its source
  ...[['caption', 'Captioned', /\\caption\{(Captioned)/], ['footnote', 'Footnoted', /\\footnote\{(Footnoted)/], ['heading', 'Heading', /\{Plain (Heading)/],
    ['item', 'Second', /\\item (Second)/], ['cell', 'cellword', /\n(cellword)/], ['node', 'Nodetext', /\{(Nodetext)/], ['math', 'n', /\$(n)\$/], ['above-float', 'here.', /float ends (here)/]]
    .map(([kind, word, re]) => ({ kind: 'click', name: `click-${kind}`, source: KINDS, example: 'hello.tex', word: { word }, src: at(re),
      steps: `a doc with each kind of box; click the start, middle and end of "${word}" (${kind}): the caret at that offset of its source` })),
  // caret2 (user 2026-11-21 18:16, paper.tex): a math glyph no source character spells is placed by the run's typeset
  // row, never refused: cmsy's minus of `-` read in \norm's argument, `\leq`, `\cdot`
  ...[['minus', '−', /x_i (-) \\hat/, 1], ['leq', '≤', /(\\leq)/, 4], ['cdot', '·', /(\\cdot)/, 5]].map(([n, word, re, len]) => ({ kind: 'click', name: `click-math-${n}`, example: 'hello.tex', word: { word }, src: at(re), len,
    source: '\\documentclass{article}\n\\newcommand{\\norm}[1]{\\left\\lVert#1\\right\\rVert}\n\\begin{document}\nWe define the loss.\n\\begin{equation}\n\\mathcal{L} = \\sum_{i=1}^{N} \\norm{x_i - \\hat{x}_i - y}^2 \\cdot a \\leq b\n\\end{equation}\n\\end{document}\n',
    clickMs: 30000, ready: `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`,
    steps: `a display formula; click before and after "${word}": the caret before and after its source token (was refused: "comes from code satex cannot place")` })),
  // the user's 18:18 report: the inline math n of "A run of $n$ lines" (thesis chapter.tex, before a display formula) was refused
  { kind: 'click', name: 'click-math-thesis', word: { word: 'n', after: 'of' }, src: at(/A run of \$(n)\$/),
    steps: 'thesis.tex, PDF, Type on; click the start and end of the math n in "A run of n lines with k paths costs": the caret in $n$ of chapter.tex' },
  // the user's 2026-11-24 report: "2026" of \date{November 2026}, set by \maketitle after the author in the same call, was
  // refused ("comes from code satex cannot place"): its typeset row had no sites, so no character of it was placed
  { kind: 'click', name: 'click-date-thesis', word: is('2026'), src: at(/\\date\{November (2026)\}/),
    steps: 'thesis.tex, PDF, Type on; click the start, middle and end of "2026" of the title page\'s date: the caret in \\date\'s argument (was refused)' },
  // display math in thesis chapter.tex: a click in the equation and in an align row puts the caret into the source math
  { kind: 'click', name: 'click-eq-thesis', word: { word: 'T', beside: '(n,' }, src: at(/\n  (T)\(n, k\)/),
    steps: 'thesis.tex, PDF, Type on; click the start and end of T in the displayed T(n, k) = …: the caret before and after the T of chapter.tex\'s equation' },
  { kind: 'click', name: 'click-align-thesis', word: { word: 'm(p', beside: '⊔' }, src: at(/\n  (m\(p) \\sqcup q\) &=/),
    steps: 'thesis.tex, PDF, Type on; click the start, middle and end of m(p in the first align row: the caret in m(p of chapter.tex\'s align' },
];
