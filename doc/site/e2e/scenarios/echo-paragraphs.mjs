// Paragraphs typed after the last one, keys around a macro's output, arrows: in the PDF and the Typeset view
// (kinds/echo.mjs, one case each). The user reports of 10:02-10:16 (2026-11-19): stray `document` echoed after the
// caret, two fonts in a word, Typeset rows duplicated or changed by an arrow, keys after `\greet{world}` landing in
// its argument.
export default [
  { kind: 'echo', name: 'newpara-pdf', case: 'pdf-new-para' },
  { kind: 'echo', name: 'newpara-pdf-slow', case: 'pdf-new-para-slow' },
  { kind: 'echo', name: 'newpara-editor', case: 'pdf-editor-para' },
  { kind: 'echo', name: 'arrows-pdf', case: 'pdf-arrows' },
  // the PDF caret and the editor's agree while `\(\sum \)` is typed into a new paragraph, before and after builds land
  { kind: 'echo', name: 'math-caret-pdf', case: 'pdf-math-caret' },
  { kind: 'echo', name: 'math-caret-pdf-slow', case: 'pdf-math-caret-slow' },
  // typing right after "Hello," of a macro's output (its definition's text): the comma once (user 2026-11-21 13:21)
  { kind: 'echo', name: 'macro-comma', case: 'pdf-macro-comma' },
  { kind: 'echo', name: 'macro-comma-space', case: 'pdf-macro-comma-space' },
  { kind: 'echo', name: 'macro-edges', case: 'pdf-macro-edges' },
  { kind: 'echo', name: 'updown-columns', case: 'pdf-updown-columns' },
  { kind: 'echo', name: 'tt-echo', case: 'pdf-tt-echo' },
  { kind: 'echo', name: 'math-caret-below', case: 'pdf-math-caret-below' },
  // typing in a table cell lays out the whole table: the other rows' cells where the build then sets them
  // a command name half typed shows no error while it is unfinished
  { kind: 'echo', name: 'naming-quiet', case: 'pdf-naming-quiet' },
  { kind: 'echo', name: 'naming-pick', case: 'pdf-naming-pick' },
  { kind: 'echo', name: 'land-blink', case: 'pdf-land-blink' },
  { kind: 'echo', name: 'sum-kept', case: 'pdf-sum-kept' },
  { kind: 'echo', name: 'first-frame', case: 'pdf-first-frame' },
  { kind: 'echo', name: 'list-clear', case: 'pdf-list-clear' },
  { kind: 'echo', name: 'table-rows-l', case: 'pdf-table-rows-l' },
  { kind: 'echo', name: 'title-centered', case: 'pdf-title-centered' },
  { kind: 'echo', name: 'table-rows-c', case: 'pdf-table-rows-c' },
  { kind: 'echo', name: 'table-span-under', case: 'pdf-table-span-under' },
  { kind: 'echo', name: 'table-span-typed', case: 'pdf-table-span-typed' },
  { kind: 'echo', name: 'table-par-p', case: 'pdf-table-par-p' },
  { kind: 'echo', name: 'table-par-m', case: 'pdf-table-par-m' },
  { kind: 'echo', name: 'table-par-x', case: 'pdf-table-par-x' },
  { kind: 'echo', name: 'math-caret-below-slow', case: 'pdf-math-caret-below-slow' },
  { kind: 'echo', name: 'after-macro-pdf', case: 'pdf-after-macro' },
  { kind: 'echo', name: 'newpara-ts', case: 'ts-new-para' },
  { kind: 'echo', name: 'mid-word-ts', case: 'ts-mid' },
  { kind: 'echo', name: 'arrows-ts', case: 'ts-arrows' },
  { kind: 'echo', name: 'after-macro-ts', case: 'ts-after-macro' },
  // a new paragraph right above a TikZ picture pushes it down on every key until the build lands, never over it
  { kind: 'echo', name: 'para-over-picture', case: 'pdf-para-picture' },
  { kind: 'echo', name: 'para-over-picture-slow', case: 'pdf-para-picture-slow' },
  // the Typeset view's Show menu: a switch toggles its class and is kept after a reload
  { kind: 'echo', name: 'ts-show-menu', case: 'ts-show' },
];
