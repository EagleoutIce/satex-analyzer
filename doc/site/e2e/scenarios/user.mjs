const REFS = String.raw`\documentclass{article}
\usepackage{hyperref}
\begin{document}
\section{Introduction}\label{sec:intro}
Some text.
\section{Methodology}\label{sec:method}
\begin{equation}\label{eq:loss} x = 1 \end{equation}
\section{Conclusion}
See \autoref{sec:intro} for motivation and \autoref{sec:method} for
the method.
\end{document}
`;
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
  // user 2026-11-22: a `\\` typed on the page first listed useless names, `\\textbf` only ~1200 ms later (the server's
  // answer): the first list the page shows is the one it keeps, its first item where the server puts it
  { kind: 'user', name: 'u-pdf-backslash-first', pdf: true, source: PAPER, word: { word: 'numbers.' },
    do: [{ pdf: 'numbers.' }, { type: ' \\' }, { watch: { sel: '.type-complete li code' }, ms: 2500 }],
    expect: [{ firstSeen: '\\section*[⟨options⟩]{⟨key⟩}' }],
    steps: 'a paper, built, PDF Type mode; click after "numbers.", type ` \\`: the list opens with what the paper uses most (\\section) and keeps it' },
  { kind: 'user', name: 'u-pdf-par-backslash-first', pdf: true, word: { word: 'world!' },
    do: [{ pdf: 'world!' }, { key: 'Enter' }, { type: '\\' }, { watch: { sel: '.type-complete li code' }, ms: 6000 }],
    expect: [{ firstSeen: '\\greet{⟨arg⟩}' }],
    steps: 'hello.tex, built, PDF Type mode; click after "world!", Enter (a new paragraph, a run starts), `\\`: the list opens with \\textbf first and keeps it (user screenshot 2026-11-22)' },
  { kind: 'user', name: 'u-pdf-par-textb-first', pdf: true, word: { word: 'world!' },
    do: [{ pdf: 'world!' }, { key: 'Enter' }, { type: '\\t', delay: 120 }, { watch: { sel: '.type-complete li code' }, ms: 4000 }],
    expect: [{ firstSeen: /^\\textbf\{/ }],
    steps: 'hello.tex, built, PDF Type mode; click after "world!", Enter, `\\t` at a hand\'s pace: \\textbf first from the list\'s first draw' },
  // user 2026-11-22: the page's hover is flaky while builds land. A guard (not reproduced red here; the pages' own swap
  // on landing is typeswap's): under the resting pointer the same tint after a landing, its note open throughout
  { kind: 'user', name: 'u-pdf-hover-stays', pdf: true, source: '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\nSo \\greet{world} and more words here.\n\\end{document}\n', word: { word: 'world!' },
    do: [{ hover: { sel: '#play-pdf .pdf-orig-macro' } }, { wait: 900 },
      { js: `(() => { const el = document.querySelector('#play-pdf .pdf-orig-macro'), r = el.getBoundingClientRect(), n = self.satexTip.note.el;
        self.__hov = { x: r.left + r.width * 0.6, y: r.top + r.height / 2, flips: [], title: el.dataset.title ?? el.title };
        new MutationObserver(() => self.__hov.flips.push(n.hidden)).observe(n, { attributes: true, attributeFilter: ['hidden'] }); return !n.hidden; })()` },
      { do: { mark: 'builds' } }, { text: '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\nSo, \\greet{world} and more words here.\n\\end{document}\n' }, { until: 'newBuild', ms: 60000 }, { wait: 1500 },
      { js: `(() => { const h = self.__hov, under = document.elementFromPoint(h.x, h.y)?.closest('.pdf-orig-macro'), n = self.satexTip.note.el;
        return JSON.stringify({ under: (under?.dataset.title ?? under?.title) === h.title, open: !n.hidden, closed: h.flips.filter(Boolean).length }); })()` }],
    expect: [{ js: /^\{"under":true,"open":true,"closed":0\}$/ }],
    steps: 'a sentence with \\greet{world}, built; rest the pointer on the tint of "Hello, world!" until its note shows; the text gains a comma before the call, the build lands: under the resting pointer the same tint, its note open throughout' },
  // every row of the PDF bar's Insert menu at a caret in prose: what it writes must build (packages loaded with it)
  ...['Inline math', 'Display math', 'Aligned', 'Figure with a drawing', 'Bulleted list', 'Numbered list', 'Equation', 'Aligned equations', 'Section heading', 'Footnote', 'Citation', 'Reference', 'TikZ picture', 'Forest tree'].map((label) => ({ entry: row(label), label })).map(({ entry, label: row }) => ({
    kind: 'user', name: `u-insert-${row.toLowerCase().replace(/\W+/g, '-')}`, pdf: true, source: PAPER, word: { word: 'numbers.' },
    do: [{ pdf: 'numbers.' }, INSERT, { wait: 300 }, entry, { wait: 2500 }, { do: { close: 'dialog[open]' } }, { wait: 500 }],
    expect: [{ changed: true }, { built: true }],
    steps: `a paper, built, PDF Type mode; click after "numbers.", the PDF bar's Insert, ${GROUP[row]} ▸ ${row}, close what opens: it wrote, and the document builds with pdfTeX` })),
  // ui2's finding (shots/u-insert-aligned-written.png): right after Insert ▸ Aligned the echo drew "numbers. align*"
  // over the row: an environment's name is a key, never set; a block begun in the typed text is no text of the row
  { kind: 'user', name: 'u-insert-aligned-echo', pdf: true, source: PAPER, word: { word: 'numbers.' },
    do: [{ pdf: 'numbers.' }, INSERT, { wait: 300 }, row('Aligned'), { wait: 700 }, { shot: 'echo' }],
    expect: [{ changed: true }, { echoNot: /align/ }],
    steps: 'a paper, built, PDF Type mode; click after "numbers.", Insert, Math ▸ Aligned: no echo spells "align*" before the build lands' },
  // a color over a word inside \textit from the page's menu: xcolor loaded, the word wrapped, the build clean (user 2026-11-22)
  { kind: 'user', name: 'u-pdf-color-in-textit', pdf: true, source: '\\documentclass{article}\n\\begin{document}\n\\textit{In the end everything we} do has its benefits.\n\\end{document}\n', word: { word: 'everything' },
    do: [{ select: 'end' }, { click: { word: 'end' }, right: true }, { wait: 300 }, { menu: ['Color', 'olive'] }, { wait: 1500 }, { build: true }],
    expect: [{ has: /\\usepackage\{xcolor\}/ }, { has: /\\textit\{In the \\textcolor\{olive\}\{end\} everything we\}/ }, { built: true }],
    steps: 'a line in \\textit, built, PDF Type mode; drag over "end", right-click, Color ▸ olive: xcolor loaded, \\textcolor{olive}{end} inside \\textit, builds' },
  // Ctrl+M on the page: an inline formula begun at the caret, x typed in it, Right leaves it, y typed after (user 2026-11-22)
  { kind: 'user', name: 'u-pdf-ctrl-m-then-right', pdf: true, source: '\\documentclass{article}\n\\begin{document}\nSome plain words here.\n\\end{document}\n', word: { word: 'plain' },
    do: [{ pdf: 'plain', at: 'end' }, { chord: 'm' }, { wait: 600 }, { type: 'x' }, { wait: 600 }, { key: 'ArrowRight' }, { type: 'y' }, { wait: 800 }],
    expect: [{ has: /plain\\\(x\\\)y words/ }],
    steps: 'click after "plain", Ctrl+M, type x, Right, type y: plain\\(x\\)y' },
  // Ctrl+Right and Ctrl+Left step by words as an editor does: word ends and starts, across the line, over \textbf{ and a key (user 2026-11-22)
  ...[['right', [{ pdf: 'plain', at: 'end' }, ...Array(3).fill({ key: 'Ctrl+ArrowRight' })], /Next \\textbf\{boldX\} see/],
    ['left', [{ pdf: 'once', at: 'start' }, ...Array(2).fill({ key: 'Ctrl+ArrowLeft' })], /\\textbf\{Xbold\} see/]].map(([dir, keys, want]) => ({
    kind: 'user', name: `u-pdf-ctrl-${dir}-words`, pdf: true, source: '\\documentclass{article}\n\\begin{document}\nSome plain words.\nNext \\textbf{bold} see \\ref{sec:a} once more.\n\\end{document}\n', word: { word: 'plain' },
    do: [...keys, { wait: 300 }, { type: 'X' }, { wait: 800 }], expect: [{ has: want }, { caretSync: true }],
    steps: dir === 'right' ? 'click after "plain", Ctrl+Right three times (words, Next, bold), type X: \\textbf{boldX} (was: three characters on)'
      : 'click before "once", Ctrl+Left twice (over the key to see, then bold), type X: \\textbf{Xbold}' })),
  // a finding marked on the page offers its fix: on hover in its card, on right-click in the menu; one click applies it, the
  // finding goes (user 2026-11-22, Screenshot_20261122_173544: the menu had only Work on / Insert / Show in source)
  ...[['hover', [{ hover: { sel: '.pdf-finding[data-f]' } }, { until: { sel: '.tip-card [data-fix]', visible: true }, ms: 5000 }, { shot: 'card' }, { click: { sel: '.tip-card [data-fix]' } }]],
    ['menu', [{ click: { sel: '.pdf-finding[data-f]' }, right: true }, { until: { sel: '.pa-menu [role=menuitem]', starts: 'Fix:' }, ms: 5000 }, { shot: 'menu' }, { click: { sel: '.pa-menu [role=menuitem]', starts: 'Fix:' } }]]].map(([how, steps]) => ({
    kind: 'user', name: `u-pdf-finding-fix-${how}`, pdf: true, settle: true, source: '\\documentclass{article}\n\\begin{document}\nSome people find... it weird.\n\\end{document}\n', word: { word: 'weird.' },
    do: [{ js: `(() => { const i = document.querySelector('#pdf-layer-list input[data-layer=findings]'); if (i && !i.checked) i.click(); return !!i; })()` }, { settle: true },
      { until: { sel: '.pdf-finding[data-f]' }, ms: 30000 }, ...steps, { wait: 500 }, { settle: true }, { until: { none: { sel: '.pdf-finding[data-f]' } }, ms: 30000 }],
    expect: [{ has: /find\\dots/ }, { noFinding: /ellipsis/ }],
    steps: `"find..." with the findings layer on; ${how === 'hover' ? 'hover its mark: the card offers the fix' : 'right-click its mark: the menu lists "Fix: …"'}; one click: \\dots written, the mark and the finding gone (was: no fix on the page)` })),
  // a footnote in Type mode (user 2026-11-22 18:54): Enter at its end is a paragraph inside the note (LaTeX reads its
  // argument \long), and a click below the note puts the caret at the note's end; both kept "Generally" out of the body
  ...[['enter', [{ pdf: 'chicken!!', at: 'end' }, { key: 'Enter' }], /\\footnote\{Foobar chicken!!\n\nGenerally\}/],
    ['below', [{ click: { word: 'chicken!!', dy: 14, scroll: true } }, { wait: 400 }, { type: ' ' }], /\\footnote\{Foobar chicken!! Generally\}/]].map(([how, steps, want]) => ({
    kind: 'user', name: `u-pdf-footnote-${how}`, pdf: true, settle: true, source: '\\documentclass{article}\n\\begin{document}\nThere is merit to this, you know?\\footnote{Foobar chicken!!}\n\n\\end{document}\n', word: { word: 'chicken!!' },
    do: [...steps, { type: 'Generally' }, { wait: 800 }, { build: true }], expect: [{ has: want }, { built: true }],
    steps: how === 'enter' ? 'click the end of "chicken!!" in the footnote, Enter, type Generally: a paragraph inside \\footnote{…}, the build clean (was: a body paragraph after the note)'
      : 'click the empty page below the footnote, type " Generally": the note\'s end (was: a body paragraph after \\footnote{…})' })),
  // inline math in Type mode (user 2026-11-22 20:37): Left after `^{}` goes into the empty slot; Enter at the formula's
  // end steps out past its closer, never a line break inside the formula
  { kind: 'user', name: 'u-pdf-math-slot-left', pdf: true, settle: true, source: '\\documentclass{article}\n\\begin{document}\nIn the magical mist of the future.\n\\end{document}\n', word: { word: 'future.' },
    do: [{ pdf: 'future.', at: 'end' }, { type: ' ' }, { chord: 'm' }, { wait: 600 }, { type: '\\sum_{42}^{}' }, { wait: 600 }, { key: 'ArrowLeft' }, { type: 'n' }, { wait: 800 }],
    expect: [{ has: /\\sum_\{42\}\^\{n\}/ }],
    steps: 'click after "future.", Ctrl+M, type \\sum_{42}^{}, Left once, type n: ^{n} (was: Left skipped the slot)' },
  { kind: 'user', name: 'u-pdf-math-enter-one-line', pdf: true, settle: true, source: '\\documentclass{article}\n\\begin{document}\nIn the magical mist of the future.\n\\end{document}\n', word: { word: 'future.' },
    do: [{ pdf: 'future.', at: 'end' }, { type: ' ' }, { chord: 'm' }, { wait: 600 }, { type: 'a+b' }, { key: 'ArrowLeft' }, { key: 'Enter' }, { wait: 400 }, { key: 'ArrowRight' }, { key: 'Enter' }, { type: ' y' }, { wait: 800 }],
    expect: [{ has: /future\. \\\(a\+b\\\) y/ }, { not: /\\\([^)]*\n/ }],
    steps: 'click after "future.", Ctrl+M, type a+b, Left, Enter (inside: nothing), Right, Enter (at the end: out past \\)), " y": the formula on one line (was: a line break inside it)' },
  // the PDF's reference picker (user 2026-11-22 20:40): a click on the plain word after a reference is a caret, never the
  // picker; the picker's rows say what each label is and where (the run's aux: kind, number, section, page)
  ...[['plain-word', [{ pdf: 'motivation', at: 'end' }, { wait: 500 }, { until: { none: { sel: '.ref-pick' } }, ms: 1000 }, { type: 'X' }, { wait: 600 }], [{ has: /for motivationX and/ }]],
    ['where', [{ hover: { word: 'section', scroll: true } }, { wait: 400 }, { click: { sel: '.pdf-refpick' } }, { until: { sel: '.ref-pick li', text: 'Section 1 · Introduction · p. 1' }, ms: 8000 }, { shot: 'picker' }], [{ not: /X/ }]]].map(([how, steps, expect]) => ({
    kind: 'user', name: `u-pdf-ref-${how}`, pdf: true, settle: true, source: REFS, word: { word: 'motivation' }, do: [{ build: true }, ...steps], expect,
    steps: how === 'plain-word' ? 'a paper with \\autoref; click the end of "motivation" after "See section 1 for": a caret, X typed there (was: the reference picker)'
      : 'click the printed "section" of \\autoref{sec:intro}: the picker lists sec:intro as "Section 1 · Introduction · p. 1" (was: bare keys)' })),
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
