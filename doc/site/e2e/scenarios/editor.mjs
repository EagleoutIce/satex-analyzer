// The editor's gutter and the widgets' shared close (kinds/editor.mjs).
export default [
  // the user's 10:21 report: after the gutter merge the ▲ marker showed nothing on hover and its click seemed dead
  { kind: 'gutter', name: 'gutter-marker', example: 'hello.tex', card: /already-defined/,
    steps: 'hello.tex; hover the finding marker of line 3: a card with the finding and its fix; click it: the Findings panel opens on it' },
  { kind: 'gutter', name: 'gutter-marker-dark', example: 'hello.tex', dark: true, card: /already-defined/,
    steps: 'as gutter-marker, dark' },
  // the user's 09:59 report: Page layout asked "Your changes are not written yet" on close though the footer said no changes
  ...[['page', 'hello.tex', '\\documentclass', 1], ['image', 'figure.tex', '\\includegraphics', 2], ['float', 'figure.tex', '\\begin{figure}', 2],
    ['color', 'colors.tex', '\\definecolor', 14], ['tikz', 'drawing.tex', '\\begin{tikzpicture}', 3], ['forest', 'proofs.tex', '\\begin{forest}', 3]].map(([widget, example, at, k]) => (
    { kind: 'close', name: `close-untouched-${widget}`, example, widget, at, k, steps: `${example}; open the ${widget} widget, close it untouched: no discard question` })),
  { kind: 'close', name: 'close-edited-x', example: 'hello.tex', widget: 'page', at: '\\documentclass', k: 1, edit: ['top', '3cm'], how: 'close',
    steps: 'hello.tex; Page layout, top margin 3cm, X: the question; X again: closed, nothing written' },
  { kind: 'close', name: 'close-edited-esc', example: 'hello.tex', widget: 'page', at: '\\documentclass', k: 1, edit: ['top', '3cm'], how: 'escape',
    steps: 'hello.tex; Page layout, top margin 3cm, Esc: the question; Esc again: closed, nothing written' },
  // user 2026-11-19: auto-compile on by default once Compile was pressed in the PDF tab; paused while the PDF is hidden
  // completion and command colors by the catcodes at the caret (user 2026-11-21: `\(\sum_` listed `\c__socket_…`)
  { kind: 'catcomplete', name: 'catcode-complete', steps: 'a body line and an \\ExplSyntaxOn block; type `\\(\\sum` then `_` in the body, `\\c__` in expl3: no list after `_`, only typeable names' },
  { kind: 'auto', name: 'auto-after-compile', steps: 'hello.tex, PDF tab; Compile once; type a line in the editor: its build lands without a second press' },
  { kind: 'auto', name: 'auto-paused-hidden', hidden: true, steps: 'hello.tex; Compile once; Typeset tab; type a line: no build for 4 s; PDF tab: the build lands' },
  // user 2026-11-19: "Environment align undefined (hello.tex:16)" stayed on the PDF while line 16 was a tabular and Findings said 0
  { kind: 'dated', name: 'pdf-banner-dated', text: '\\documentclass{article}\n\\begin{document}\nIn the\n\\begin{align}\nx\n\\end{align}\n\\end{document}\n',
    fixed: '\\documentclass{article}\n\\begin{document}\nIn the\n\\begin{tabular}{ll}\na & b \\\\\n\\end{tabular}\n\\end{document}\n',
    steps: 'hello.tex, auto compile off; a build of an undefined align fails; the text becomes a tabular: the banner and bar say "of an older text"; Compile: no banner' },
  // sweep 2026-11-19: Ctrl+Z at the Type caret did nothing, a key after an undo in the editor was refused
  { kind: 'typeundo', name: 'type-undo-redo', word: 'world', steps: 'hello.tex, Typeset; click "world", K; Ctrl+Z: gone; Ctrl+Shift+Z: back; Ctrl+Z, J: J types' },
  // sweep 2026-11-19: a click on a line number left the focus on the scroller, the next keys went nowhere
  { kind: 'gutterkey', name: 'gutter-click-keys', line: 3, steps: 'hello.tex; click line number 3, press W: W starts line 3' },
  // sweep 2026-11-19: a fix of chapter.tex from thesis.tex left the caret at the top, the note did not say where
  { kind: 'fixfile', name: 'fix-other-file', example: 'thesis.tex', finding: 'sec:cost', file: 'chapter.tex', line: '\\subsection{Cost}',
    steps: 'thesis.tex; Findings, Fix of "label sec:cost is never referenced": chapter.tex shown at the edit, the note names it' },
  // sweep 2026-11-19: `%` typed before \newcommand{\section}, then erased: the finding stayed gone until the next edit
  { kind: 'heldback', name: 'finding-after-pause', line: 3, key: '%',
    steps: 'hello.tex; type % at the start of line 3 (its finding goes), Backspace; once typing pauses the finding is back' },
  // user 2026-11-19: "\toprule: not reached by the analysis" stayed on the card while booktabs was loaded and the status said Ready
  { kind: 'hoverlate', name: 'hover-after-load', text: '\\documentclass{article}\n\\begin{document}\nIn the\n\n\\begin{tabular}{llr}\n  \n  a & b & c \\\\\n\\end{tabular}\n\\end{document}\n',
    line: 6, keys: ['\\', 't', 'o', 'p', 'r', 'u', 'l'], word: '\\toprule', want: /booktabs\.sty:\d+/,
    steps: 'hello.tex; "\\toprul" Tab in a tabular (writes \\usepackage{booktabs}); the pointer on \\toprule during the run; once quiet the card is booktabs\' macro card (was: the busy note or "not reached" stayed)' },
];
