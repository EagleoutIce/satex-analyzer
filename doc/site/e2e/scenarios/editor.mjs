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
];
