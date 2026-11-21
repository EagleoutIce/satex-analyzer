import { is, after, KINDS, at, DEMO } from './sources.mjs';

export default [
  { kind: 'click', name: 'click-macro-start', source: DEMO, example: 'hello.tex', word: is('TinyNet,'), src: at(/\\newcommand\{\\model\}\{(TinyNet)/), len: 7, clickMs: 60000, ready: `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`, 
    steps: 'the landing demo; click the start, middle and end of "TinyNet," (\\model at a paragraph\'s start): the caret in \\model\'s definition (ui-35: the start was refused)' },
  { kind: 'click', name: 'click-macro-typed', source: DEMO.replace('falls fast.', 'falls fast. It drops by \\(x_{42}\\) per step.\n\n\\model{} converges.'), example: 'hello.tex', word: is('TinyNet'), nth: 1, src: at(/\\newcommand\{\\model\}\{(TinyNet)/), len: 7, clickMs: 60000, ready: `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`, 
    steps: 'the landing demo; click the start, middle and end of "TinyNet," (the demo typed, then \\model{} first in a new paragraph): the caret in \\model\'s definition (ui-35: the start was refused)' },
  { kind: 'click', name: 'click-macro-long', source: DEMO.replace('With \\model, the loss falls fast.', '\\model{} makes the loss fall\nfast and then it keeps\non falling for a while\nuntil the run settles\nin the end of the long\nparagraph here.'), example: 'hello.tex', word: is('TinyNet'), src: at(/\\newcommand\{\\model\}\{(TinyNet)/), len: 7, clickMs: 60000, ready: `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`, 
    steps: 'the landing demo; click the start, middle and end of "TinyNet," (\\model{} first in a paragraph of six source lines): the caret in \\model\'s definition (ui-35: the start was refused)' },
  // a click at the start, middle and end of a word in each kind of box: the caret at that offset of its source
  ...[['caption', 'Captioned', /\\caption\{(Captioned)/], ['footnote', 'Footnoted', /\\footnote\{(Footnoted)/], ['heading', 'Heading', /\{Plain (Heading)/],
    ['item', 'Second', /\\item (Second)/], ['cell', 'cellword', /\n(cellword)/], ['node', 'Nodetext', /\{(Nodetext)/], ['math', 'n', /\$(n)\$/], ['above-float', 'here.', /float ends (here)/]]
    .map(([kind, word, re]) => ({ kind: 'click', name: `click-${kind}`, source: KINDS, example: 'hello.tex', word: { word }, src: at(re),
      steps: `a doc with each kind of box; click the start, middle and end of "${word}" (${kind}): the caret at that offset of its source` })),
  // the user's 18:18 report: the inline math n of "A run of $n$ lines" (thesis chapter.tex, before a display formula) was refused
  { kind: 'click', name: 'click-math-thesis', word: { word: 'n', after: 'of' }, src: at(/A run of \$(n)\$/),
    steps: 'thesis.tex, PDF, Type on; click the start and end of the math n in "A run of n lines with k paths costs": the caret in $n$ of chapter.tex' },
  // display math in thesis chapter.tex: a click in the equation and in an align row puts the caret into the source math
  { kind: 'click', name: 'click-eq-thesis', word: { word: 'T', beside: '(n,' }, src: at(/\n  (T)\(n, k\)/),
    steps: 'thesis.tex, PDF, Type on; click the start and end of T in the displayed T(n, k) = …: the caret before and after the T of chapter.tex\'s equation' },
  { kind: 'click', name: 'click-align-thesis', word: { word: 'm(p', beside: '⊔' }, src: at(/\n  (m\(p) \\sqcup q\) &=/),
    steps: 'thesis.tex, PDF, Type on; click the start, middle and end of m(p in the first align row: the caret in m(p of chapter.tex\'s align' },
];
