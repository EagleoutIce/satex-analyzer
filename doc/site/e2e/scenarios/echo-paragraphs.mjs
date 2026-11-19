// Paragraphs typed after the last one, keys around a macro's output, arrows: in the PDF and the Typeset view
// (kinds/echo.mjs, one case each). The user reports of 10:02-10:16 (2026-11-19): stray `document` echoed after the
// caret, two fonts in a word, Typeset rows duplicated or changed by an arrow, keys after `\greet{world}` landing in
// its argument.
export default [
  { kind: 'echo', name: 'newpara-pdf', case: 'pdf-new-para' },
  { kind: 'echo', name: 'newpara-pdf-slow', case: 'pdf-new-para-slow' },
  { kind: 'echo', name: 'newpara-editor', case: 'pdf-editor-para' },
  { kind: 'echo', name: 'arrows-pdf', case: 'pdf-arrows' },
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
