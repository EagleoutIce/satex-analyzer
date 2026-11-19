import { MARKUP, is, after, at } from './sources.mjs';

export default [
  // a drag over words, then Backspace / Delete / a character: one edit over the selection's source, painted over at once
  { kind: 'select', name: 'select-backspace', source: MARKUP, example: 'hello.tex', from: is('plain'), to: is('follows'), span: (t) => { const a = t.indexOf('plain text follows'); return [a, a + 'plain text follows'.length]; }, keys: ['Backspace'], gone: ['follows'],
    steps: 'a small doc; drag from "plain" to "follows" in the second paragraph; Backspace: all three words go in one edit, painted over before the rebuild' },
  { kind: 'select', name: 'select-type', source: MARKUP, example: 'hello.tex', from: is('second'), to: is('second'), span: (t) => { const a = t.indexOf('second'); return [a, a + 6]; }, keys: ['Z'], gone: ['second'],
    steps: 'drag over "second"; type Z: the word is replaced by Z' },
  { kind: 'select', name: 'select-refused', source: MARKUP, example: 'hello.tex', from: is('Plain'), to: is('words'), span: () => [0, 0], keys: ['Delete'], refused: true,
    steps: 'drag from the heading word "Plain" into the paragraph after it; Delete: refused with a note (it crosses the \\section), the source unchanged' },
  { kind: 'select', name: 'fz-select-repeated', source: `\\documentclass{article}\n\\begin{document}\n${Array.from({ length: 14 }, (_, i) => `Sentence ${i} carries several plain words, like river${i} and stone${i}.`).join(' ')}\n\\end{document}\n`, example: 'hello.tex',
    from: `(t, w, ws) => t === 'like' && w === ws.filter((v) => v[5] === 'like')[5]`, to: `(t, w, ws) => t === 'like' && w === ws.filter((v) => v[5] === 'like')[5]`, span: (t) => { const a = t.indexOf('like river5'); return [a, a + 4]; }, keys: ['Z'], gone: [],
    steps: 'one source line of 14 alike sentences; drag over the 6th "like"; type Z: the 6th "like" becomes Z (was: the first occurrence on the line was taken)' },
];
