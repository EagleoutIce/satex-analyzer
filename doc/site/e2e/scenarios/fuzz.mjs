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
    steps: 'PDF; type in the footnote, End, wait for the build, type a word: the caret stands at the word\'s last glyph (the oracle had missed the echo\'s horizontal stretch) (fuzz seed 6)' },
  { kind: 'fuzz', name: 'fz-ts-land-jump', doc: 'twocol', view: 'ts', seq: ['c:word:0.789', 'w:xqtotoa', 'k:End', 'wait', 'w:xqrakoc', 'c:word:0.122', 'w:xqnibae', 'wait'],
    steps: 'Typeset view of a two-column doc; type in two places, wait: the caret stays where it was as the typeset lands (it jumped 400 px) (fuzz seed 6)' },
  { kind: 'fuzz', name: 'fz-ts-enter-in-argument', doc: 'hello', view: 'ts', seq: ['c:word:0.083', 'w:xqjoqad', 'k:Enter'],
    steps: 'Typeset view; click the end of "world!" (in `\\greet{world}`), type a word, Enter: a caret stays on screen (faded where it was) though no row sets the break typed into the argument (fuzz seed 3)' },
  { kind: 'fuzz', name: 'fz-land-no-flicker', doc: 'hello', view: 'pdf', seq: ['c:word:0.606', 'w:xqpezua', 'wait'],
    steps: 'type a word after "Another", wait for the build: no frame shows the old page fading over the typed text with the echo gone (the 120 ms crossfade put the old page on top after the echo was removed: "base |" for frames in the landing demo)' },
  { kind: 'fuzz', name: 'fz-demo-narrow-rows', doc: 'demo', view: 'pdf', seq: ['wait', 'c:word:0.42', 'w:Itdrops', 'p', 'k:Enter', 'p', 'w:Itconverges', 'p', 'w:more', 'p', 'k: ', 'w:words'],
    steps: 'the landing demo; type after "fast.", Enter, a paragraph with pauses (builds land between): the echo sets the new paragraph at the text width, never one word (or hyphen piece) a row, and no word shows twice (fuzz)' },
];
