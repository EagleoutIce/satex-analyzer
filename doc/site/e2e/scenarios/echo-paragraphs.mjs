// New paragraphs typed after the last one (kinds/newpara.mjs): the echo draws only typed text, one font a row, and
// the caret stays put when the build lands. Fast keys (no build lands between them), a person's pace (builds land
// between keys), and on in the editor after the PDF caret.
export default [
  { kind: 'newpara', name: 'newpara-pdf', mode: 'pdf', keyMs: 25 },
  { kind: 'newpara', name: 'newpara-pdf-slow', mode: 'pdf', keyMs: 160 },
  { kind: 'newpara', name: 'newpara-editor', mode: 'editor' },
];
