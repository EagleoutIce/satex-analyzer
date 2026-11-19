// Step lists the glitch fuzzer (fuzz/fuzz.mjs) found and shrank, replayed with all its invariants. `seq`: the steps
// (fuzz.mjs's notation), `doc`: a key of its DOCS, `view`: the first view.
export default [
  { kind: 'fuzz', name: 'fz-macro-output-kept', doc: 'hello', view: 'pdf', seq: ['c:word:0.429', 'w:xqvepeb'],
    steps: 'hello with `about \\model{} here,`; click the end of "here,", type a word: the echo keeps TinyNet (the macro\'s output) on the row, never re-lays the row from "about" without it (fuzz seed 3)' },
  { kind: 'fuzz', name: 'fz-display-closer-row', doc: 'markup', view: 'pdf', seq: ['c:word:0.558', 'k:,'],
    steps: 'a word right after a display (`\\]⏎and the prose`); click the end of "and", type a comma: "and," is drawn on its own row, never on the row below over "A second paragraph" (fuzz seed 4)' },
  { kind: 'fuzz', name: 'fz-two-edits', doc: 'hello', view: 'pdf', seq: ['c:word:0.558', 'k:,', 'c:word:0.606', 'w:xqpezua'],
    steps: 'type a comma after "asked.", click the end of "Another" in the next paragraph before the build lands, type a word: the echo keeps "line of" on the row (it lays out one change from the first edit to the caret, over the paragraph break) (fuzz seed 4)' },
  { kind: 'fuzz', name: 'fz-ts-end-keeps-typed', doc: 'hello', view: 'ts', seq: ['c:word:0.083', 'c:word:0.789', 'w:xqtotoa', 'k:ArrowRight', 'w:xqpepeb', 'k:End'],
    steps: 'Typeset view; click "plain", type a word, ArrowRight, type another, End: the row still shows both typed words (End dropped the second from the shown row) (fuzz seed 6)' },
  { kind: 'fuzz', name: 'fz-ts-footnote-row', doc: 'markup', view: 'ts', seq: ['c:word:0.083', 'c:word:0.789', 'w:xqtotoa'],
    steps: 'Typeset view; click the end of "A" in the footnote text, type a word: the footnote row shows it once, the caret on that row (it showed a second footnote row, the caret on another) (fuzz seed 6)' },
  { kind: 'fuzz', name: 'fz-pdf-caret-after-end', doc: 'markup', view: 'pdf', seq: ['c:word:0.083', 'c:word:0.789', 'w:xqtotoa', 'k:ArrowRight', 'w:xqpepeb', 'k:End', 'wait', 'w:xqrakoc'],
    steps: 'PDF; type in the footnote, End, wait for the build, type a word: the caret stands at the word\'s last glyph (7 px past it) (fuzz seed 6)' },
  { kind: 'fuzz', name: 'fz-ts-land-jump', doc: 'twocol', view: 'ts', seq: ['c:word:0.083', 'c:word:0.789', 'w:xqtotoa', 'k:ArrowRight', 'w:xqpepeb', 'k:End', 'wait', 'w:xqrakoc', 'k:ArrowUp', 'w:xqqazud', 'c:word:0.122', 'w:xqnibae', 'w:xqtojof', 'wait'],
    steps: 'Typeset view of a two-column doc; type in two places, wait: the caret stays where it was as the typeset lands (it jumped 400 px) (fuzz seed 6)' },
];
