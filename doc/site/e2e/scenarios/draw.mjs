// The Draw dialog's pad (kinds/draw.mjs): taps, strokes, undo and redo, at phone width and in the dark theme.
export default [
  { kind: 'draw', name: 'draw-dot', check: 'dot',
    steps: 'hello.tex; open Draw a symbol, tap the pad once: a dot is inked (was: nothing drawn, so `\\cdot` and the dots of `÷` could not be drawn)' },
  { kind: 'draw', name: 'draw-undo', check: 'undo',
    steps: 'hello.tex; Draw a symbol, two strokes; Ctrl+Z leaves the first, Ctrl+Shift+Z brings the second back; Clear, then the undo button brings both back; the editor text never changes' },
  { kind: 'draw', name: 'draw-phone-dark', check: 'fit', size: [360, 740], theme: 'dark',
    steps: 'phone width 360 px, dark theme; Draw a symbol, one stroke: no sideways scroll, the tools and the pad inside the dialog, the ink light on the dark pad' },
  { kind: 'draw', name: 'draw-formula', check: 'formula',
    steps: 'hello.tex; Draw a symbol, the ∑ formula bar; Fraction, type a, Tab, type b, Superscript, type 2: the bar reads \\frac{a}{b^{2}} and draws it without empty boxes; Insert: \\(\\frac{a}{b^2}\\) at the cursor, nothing else changed, the dialog closes' },
  { kind: 'draw', name: 'draw-formula-phone', check: 'formula', size: [360, 740], theme: 'dark',
    steps: 'as draw-formula at phone width in the dark theme: the bar fits the dialog, no sideways scroll' },
];
