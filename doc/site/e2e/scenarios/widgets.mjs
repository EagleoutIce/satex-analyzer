import { FM, is, after, KINDS, at } from './sources.mjs';

export default [
  // a table column's letter changed from its chip menu
  // the user's 18:18 report: the middle column of thesis chapter.tex's {lrr} set to c must write {lcr}, never a p{…}
  { kind: 'col', name: 'col-align-thesis', file: 'chapter.tex', word: is('Lines'), col: 1, to: 'c', spec: /\\begin\{tabular\}\{lcr\}/, pages: 2, maxMut: 30,
    steps: 'thesis.tex, chapter.tex opened; widths layer on; hover the table of Table 1, open the middle column\'s ⋯ menu, choose c: the source says {lcr}, nothing stray drawn on the page' },
  { kind: 'col', name: 'col-align', source: KINDS, example: 'hello.tex', word: is('cellword'), to: 'c', spec: /\\begin\{tabular\}\{cl\}/, pages: 1, maxMut: 12,
    steps: 'a doc with a tabular{ll}; widths layer on; hover the table, open column 1\'s ⋯ menu, choose c: the source says {cl}, the old page stays shown at full opacity, one swap when the build lands, the chips never vanish or dim' },
  // forest nodes in the PDF: a right click on a node's word, its menu as a TikZ node's (shape, frame, tree edits)
  { kind: 'forest', name: 'forest-child', source: FM, example: 'hello.tex', word: is('Engine'), choose: 'Add child', want: /\[Engine, fm mandatory, fm alternative \[Electric\] \[Gas\]\s*\[new\]\]/,
    steps: 'feature-model.tex, PDF; right click the node Engine, Add child: a [new] child after Gas, nothing else changed' },
  { kind: 'forest', name: 'forest-sibling', source: FM, example: 'hello.tex', word: is('Radio'), choose: 'Add sibling', want: /\[Radio\]\s*\[new\]\s*\[Navigation\]/,
    steps: 'feature-model.tex, PDF; right click the node Radio, Add sibling: a [new] node between Radio and Navigation' },
  { kind: 'forest', name: 'forest-frame', source: FM, example: 'hello.tex', word: is('Gas'), choose: 'Circle', want: /\[Gas, circle\]/,
    steps: 'feature-model.tex, PDF; right click the node Gas, Circle: [Gas, circle]' },
  { kind: 'forest', name: 'forest-text', source: FM, example: 'hello.tex', word: is('Body'), choose: 'Edit text…', input: 'Frame', want: /\[Frame, fm mandatory\]/,
    steps: 'feature-model.tex, PDF; right click the node Body, Edit text…, type Frame, Enter: [Frame, fm mandatory]' },
];
