// Insert (kinds/insert.mjs): a table from the PDF's right click, a figure from the PDF bar's Insert menu at the caret.
export default [
  { kind: 'insert', name: 'insert-hello',
    steps: 'hello.tex built; right-click Hello, in the PDF, Insert, Table: a table after the paragraph, the table widget open, builds, one Ctrl+Z undoes it; the caret after \\greet{world}, the PDF bar\'s Insert menu, Figure with an image: graphicx loaded, the figure written, the image dialog open, the caret in the caption, builds, one Ctrl+Z undoes it' },
];
