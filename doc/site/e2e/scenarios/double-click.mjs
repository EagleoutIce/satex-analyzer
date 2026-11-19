import { FM, at } from './sources.mjs';

export default [
  // a double click on an image, a table or a drawing opens its widget as the editor's lens does (ui-34): the PDF view
  // by the box SyncTeX records on the site's last line, the Typeset view by the row of the site's lines
  ...[['pdf', 'PDF'], ['ts', 'Typeset']].flatMap(([view, V]) => [
    { kind: 'dbl', view, name: `dbl-${view}-tikz`, file: 'chapter.tex', at: /\\end\{tikzpicture\}/, want: /view .*drawing/i,
      steps: `thesis.tex, ${V} view; double-click the pipeline drawing of chapter.tex: chapter.tex opens with its drawing widget` },
    { kind: 'dbl', view, name: `dbl-${view}-table`, file: 'chapter.tex', at: /\\end\{tabular\}/, want: /dialog/i,
      steps: `thesis.tex, ${V} view; double-click the measured-runs table of chapter.tex: chapter.tex opens with its table widget` },
    { kind: 'dbl', view, name: `dbl-${view}-image`, example: 'figure.tex', at: /\\includegraphics/, note: true, want: /dialog/i,
      steps: `figure.tex, ${V} view; double-click the image: its image dialog opens` },
    { kind: 'dbl', view, name: `dbl-${view}-forest`, source: FM, example: 'hello.tex', file: 'hello.tex', at: /\\end\{forest\}/, want: /view .*tree/i,
      steps: `the forest example as hello.tex, ${V} view; double-click the tree: its tree widget opens` },
    { kind: 'dbl', view, name: `dbl-${view}-text`, text: 'Literate',
      steps: `thesis.tex, ${V} view; double-click the word "Literate": no widget opens (text keeps its clicks)` },
  ]),
];
