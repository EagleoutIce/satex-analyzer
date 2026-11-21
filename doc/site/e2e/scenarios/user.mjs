// User journeys (kinds/user.mjs): bugs a person hits in the first minutes, each reduced to its few actions.
const PAPER = String.raw`\documentclass{article}
\usepackage{amsmath}
\title{A Short Note}
\author{A. Person}
\begin{document}
\maketitle
\section{Introduction}\label{sec:intro}
We study sums of integers here, which matters.
Hello, world of numbers.
\section{Method}
As shown in Section~\ref{sec:intro}, see~\cite{knuth}.
Some plain words to make bold later.
\begin{thebibliography}{9}
\bibitem{knuth} D. Knuth. The TeXbook.
\end{thebibliography}
\end{document}
`;
// the PDF bar's Insert menu, opened by a click, and one of its rows by its group and label
const INSERT = { click: { sel: '#play-pdf .ins-open' } };
const GROUP = { 'Inline math': 'Math', 'Display math': 'Math', Aligned: 'Math', Equation: 'Math', 'Aligned equations': 'Math', 'Figure with a drawing': 'Floats',
  Table: 'Floats', 'Bulleted list': 'Lists', 'Numbered list': 'Lists', 'Section heading': 'Structure', Footnote: 'Structure', Citation: 'References',
  Reference: 'References', 'TikZ picture': 'Pictures', 'Forest tree': 'Pictures' };
const row = (label) => ({ menu: [GROUP[label], label] });
const TINY = '\\documentclass{article}\n\\begin{document}\nHi there.\n\\end{document}\n';

export default [
  { kind: 'user', name: 'u-root-from-scratch', do: [{ editor: '' , before: true }, { chord: 'a' }, { key: 'Backspace' }, { settle: true }, { paste: TINY }, { settle: true }],
    expect: [{ noFinding: /unreachable-file/ }],
    steps: 'hello.tex; Ctrl+A, Backspace, the checks run on the empty root; paste a whole document: the root is flagged "No root document reads this file" (unreachable-file) and stays so' },
  { kind: 'user', name: 'u-sum-sub-errors', do: [{ editor: '\\greet{world}' }, { type: '\nSum: \\(\\sum_ \\)' }, { settle: true }, { build: true }],
    expect: [{ buildErrorsCovered: true }],
    steps: 'hello.tex; type a line `Sum: \\(\\sum_ \\)` after \\greet{world}; Compile: pdfTeX says Missing { inserted / Missing } inserted on that line, Findings has nothing there' },
  { kind: 'user', name: 'u-table-add-col-while-editing', pdf: true, source: PAPER, word: { word: 'numbers.' },
    do: [{ pdf: 'numbers.' }, INSERT, { wait: 300 }, row('Table'), { wait: 2500 },
      { type: 'Item' }, { key: 'Tab' }, { type: 'Cost' },
      { do: { press: { sel: 'dialog.sd-dialog[open] .sd-ins', title: 'column at the end' } } }, { do: { press: { sel: 'dialog.sd-dialog[open] .tv-primary' } } }, { wait: 1000 }],
    expect: [{ has: /\\begin\{tabular\}\{lll\}\n\s*Item & Cost/ }],
    steps: 'a paper, built; click after "numbers." in the PDF, Insert ▸ Table; type Item, Tab, Cost in the grid; click + (a column at the end): NotFoundError replaceChildren … moved in a blur event handler' },
  { kind: 'user', name: 'u-paren-autoclose', do: [{ editor: '\\greet{world}' }, { type: ' \\(x\\)' }, { wait: 500 }],
    expect: [{ has: /\\greet\{world\} \\\(x\\\)\n/ }],
    steps: 'hello.tex; type ` \\(x\\)` after \\greet{world} in the editor: the editor closes the `(` of `\\(` with `)`, the text becomes `\\(x\\))` (completion Enter on `\\(\\alp` gives `\\(\\alpha)`)' },
  { kind: 'user', name: 'u-begin-end-typed', do: [{ editor: '\\greet{world}' }, { type: '\n\\begin{center}\nmid\n\\end{center}' }, { wait: 600 }],
    expect: [{ once: '\\end{center}' }],
    steps: 'hello.tex; after \\greet{world} type \\begin{center}, Enter, mid, Enter, \\end{center} as one writes it: the editor\'s own \\end{center} stays besides the typed one, the file ends with a stray \\end{center} after \\end{document}' },
  { kind: 'user', name: 'u-pdf-dblclick-word', pdf: true, source: PAPER, word: { word: 'bold' },
    do: [{ pdf: 'bold', at: 'mid', clicks: 2 }, { wait: 400 }],
    expect: [{ selected: 'bold' }],
    steps: 'a paper, built, Type mode; double-click the word "bold" on the page: no word is selected (the caret sits inside it), so Ctrl+B or typing has nothing to act on' },
  { kind: 'user', name: 'u-tikz-drag-node-text', pdf: true, example: 'drawing.tex', word: { word: 'findings' },
    do: [{ drag: 'findings', dx: 48, dy: 36 }, { wait: 1500 }],
    expect: [{ not: /\\node\[box, hot, right=of run\] \(out\) \{findings\};/ }],
    steps: 'drawing.tex, built, Type mode on (the default); press on the node text "findings" and drag it 48 px right, 36 down: nothing moves, no ghost, no selection, the source is unchanged' },
  { kind: 'user', name: 'u-tilde-typeset', do: [{ ts: 'world!' }, { type: ' see~\\ref{x}' }, { wait: 800 }],
    expect: [{ has: /see~\\ref\{x\}/ }],
    steps: 'hello.tex; Typeset view, Type on; click after "world!" and type ` see~\\ref{x}` as a LaTeX user writes a tie: the source gets `see\\textasciitilde{}\\ref{x}` (the command passes through, the tie is escaped)' },
  { kind: 'user', name: 'u-tilde-pdf', pdf: true, source: PAPER, word: { word: 'numbers.' },
    do: [{ pdf: 'numbers.' }, { type: ' See~\\ref{sec:intro}.' }, { wait: 1000 }],
    expect: [{ has: /numbers\. See~\\ref\{sec:intro\}\./ }],
    steps: 'a paper, built, PDF Type mode; click after "numbers." and type ` See~\\ref{sec:intro}.`: the tie must stay a tie' },
  { kind: 'user', name: 'u-pdf-undo-burst', pdf: true, source: PAPER, word: { word: 'numbers.' },
    do: [{ pdf: 'numbers.' }, { type: ' More text' }, { wait: 800 }, { chord: 'z' }, { wait: 800 }],
    expect: [{ has: /numbers\.\n\\section\{Method\}/ }, { caretSync: true }],
    steps: 'a paper, built, PDF Type mode; click after "numbers.", type " More text", Ctrl+Z once: the typed run is gone, the carets agree' },
  { kind: 'user', name: 'u-pdf-paste-specials', pdf: true, source: PAPER, word: { word: 'numbers.' },
    do: [{ pdf: 'numbers.' }, { paste: ' Costs 50% & more_than #1.' }, { wait: 1000 }],
    expect: [{ has: /numbers\. Costs 50\\% \\& more\\_than \\#1\./ }, { built: true }],
    steps: 'a paper, built, PDF Type mode; click after "numbers.", paste ` Costs 50% & more_than #1.`: each special escaped once, the document builds' },
  { kind: 'user', name: 'u-pdf-backspace-after-ref', pdf: true, source: PAPER, word: { word: 'see' },
    do: [{ pdf: 'see', at: 'start' }, { key: 'Backspace' }, { key: 'Backspace' }, { key: 'Backspace' }, { wait: 1200 }],
    expect: [{ built: true }, { caretSync: true }],
    steps: 'a paper, built, PDF Type mode; click before "see" (after `Section~\\ref{sec:intro}, `) and press Backspace three times (the space, the comma, then into the \\ref output): the source must still build, the carets agree' },
  // completion as a person types it in the editor (user 2026-11-21): `\\usepa` Enter writes the braces with the caret
  // in them, as `\\textbf` gets its own; `\\textb` lists what the document loads first (`\\textbf`), never a name only an
  // unloaded package (drm) defines
  { kind: 'user', name: 'u-usepackage-braces', source: PAPER, do: [{ text: PAPER }, { wait: 400 }, { editor: 'numbers.' }, { type: ' \\usepa' }, { wait: 1200 }, { key: 'Enter' }, { type: 'xcolor' }, { wait: 600 }],
    expect: [{ has: /numbers\. \\usepackage\{xcolor\}/ }],
    steps: 'a paper; after "numbers." type \\usepa, Enter, xcolor (as userrun j5: the text set anew, a run going): \\usepackage{xcolor} (it wrote \\usepackage without braces)' },
  { kind: 'user', name: 'u-textb-first', source: PAPER, do: [{ text: PAPER }, { wait: 400 }, { editor: 'numbers.' }, { type: ' \\textb' }, { wait: 1200 }],
    expect: [{ first: { sel: '.cm-tooltip-autocomplete li .cm-completionLabel' }, want: 'textbf' }],
    steps: 'a paper; after "numbers." type \\textb: the list starts with textbf (it started with drm\'s textb)' },
  // every row of the PDF bar's Insert menu at a caret in prose: what it writes must build (packages loaded with it)
  ...['Inline math', 'Display math', 'Aligned', 'Figure with a drawing', 'Bulleted list', 'Numbered list', 'Equation', 'Aligned equations', 'Section heading', 'Footnote', 'Citation', 'Reference', 'TikZ picture', 'Forest tree'].map((label) => ({ entry: row(label), label })).map(({ entry, label: row }) => ({
    kind: 'user', name: `u-insert-${row.toLowerCase().replace(/\W+/g, '-')}`, pdf: true, source: PAPER, word: { word: 'numbers.' },
    do: [{ pdf: 'numbers.' }, INSERT, { wait: 300 }, entry, { wait: 2500 }, { do: { close: 'dialog[open]' } }, { wait: 500 }],
    expect: [{ changed: true }, { built: true }],
    steps: `a paper, built, PDF Type mode; click after "numbers.", the PDF bar's Insert, ${GROUP[row]} ▸ ${row}, close what opens: it wrote, and the document builds with pdfTeX` })),
  { kind: 'user', name: 'u-pdf-ctrl-b-then-right', pdf: true, source: PAPER, word: { word: 'bold' },
    do: [{ select: 'bold' }, { chord: 'b' }, { wait: 800 }, { key: 'ArrowRight' }, { type: 'X' }, { wait: 800 }],
    expect: [{ has: /make \\textbf\{bold\}X later/ }],
    steps: 'a paper, built, PDF Type mode; drag over "bold", Ctrl+B, Right arrow, type X: \\textbf{bold}X (the reported Ctrl+B and Right-arrow-out-of-\\textbf bugs)' },
  ...[['thesis.tex', '\\printindex'], ['slides.tex', '\\end{document}']].map(([ex, at]) => ({
    kind: 'user', name: `u-edit-latency-${ex.replace('.tex', '')}`, example: ex,
    do: [{ editor: at, before: true }, { type: 'x' }, { findingsWithin: 2000 }],
    steps: `${ex}, its first check done; type one letter before ${at}: the findings for the edit take longer than 2 s (j10 sweep: slides 4.0 s, j13: 3-5 s, status says slides.tex:1695; thesis 1.2-12.7 s; every other example ≤ 0.4 s)` })),
  { kind: 'user', name: 'u-par-in-math-covered', source: '\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\nText.\n\\begin{equation}\n  \n\\end{equation}\n\\begin{align*}\n\n\\end{align*}\n\\end{document}\n',
    do: [{ settle: true }, { build: true }],
    expect: [{ buildErrorsCovered: true }],
    steps: 'hello.tex as an empty equation and align* with a blank line inside (what Insert ▸ Equation / Aligned write); Compile: pdfTeX stops with Missing $ inserted / Paragraph ended before \\align* was complete, Findings must have them' },
];
