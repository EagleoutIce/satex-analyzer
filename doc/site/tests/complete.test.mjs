// The completion lists the playground answers from while the worker is busy (views.ts `complete`): ranking, the kept
// list of a kind of place, and the selection of a list refreshed under the person's eyes.
import assert from 'node:assert/strict';
import { test } from './harness.mjs';

const { rank, kept, reselect } = self.satexViews.complete;
const labels = (items) => items.map((i) => i.label);

// Intuition: the exact name comes first, then prefixes (the project's own before the kernel's, internals last), then
// parts of the name, then the characters in order; what matches none is dropped. Pins `\emp` + Tab taking `emph`.
test('rank: exact, prefix by tier, part, characters in order', () => {
  const items = [
    { label: 'empty' }, { label: '@empty' }, { label: 'emph' }, { label: 'myemph', detail: 'macro (document)' },
    { label: 'emphasis', detail: 'macro (document)' }, { label: 'textbf' }, { label: 'eqmph' }, { label: 'emp' },
  ];
  assert.deepEqual(labels(rank(items, 'emp')), ['emp', 'emphasis', 'emph', 'empty', 'myemph', '@empty', 'eqmph']);
  assert.deepEqual(labels(rank(items, 'emh')), ['myemph', 'emphasis', 'emph', 'eqmph'], 'characters in order: the own commands first');
  // case does not matter; the server's own order (sortText) breaks ties before length
  assert.deepEqual(labels(rank([{ label: 'labelb', sortText: '2' }, { label: 'LabelA', sortText: '1' }], 'LAB')), ['LabelA', 'labelb']);
  // a label's part, then what it is found by (a citation's title)
  assert.deepEqual(labels(rank([{ label: 'knuth84', filterText: 'The TeXbook' }, { label: 'texbook-notes' }], 'texbook')), ['texbook-notes', 'knuth84']);
});

// Intuition: during a run a `\` completes from the server's last list there plus every name in force it lacks, each
// once, the server's item (its snippet) winning. Pins names from the scope list hiding a snippet, or listed twice.
test('kept: served items first, names in force added once, ranges dropped', () => {
  const served = [{ label: 'emph', insertText: 'emph{$1}', insertTextFormat: 2 }, { label: 'ref', textEdit: { newText: 'ref{$1}' } }];
  const names = [{ label: 'emph', kind: 3 }, { label: 'mycmd', kind: 3 }];
  const k = kept(served, names);
  assert.deepEqual(labels(k), ['emph', 'ref', 'mycmd']);
  assert.equal(k[0].insertText, 'emph{$1}');
  // an item asked elsewhere replaces no range here: its text goes in at the word
  assert.equal(k[1].textEdit, undefined);
  assert.equal(k[1].insertText, 'ref{$1}');
  // the server's order for another word ranks nothing here: shorter names first
  assert.deepEqual(labels(rank(kept([{ label: 'empty', sortText: '0' }, { label: 'emph', sortText: '1' }]), 'emp')), ['emph', 'empty']);
  assert.equal(kept(undefined, undefined), null, 'a place never asked keeps nothing');
  assert.deepEqual(labels(kept([{ label: 'fig:a' }], undefined)), ['fig:a'], 'labels after \\ref{ need no names in force');
  assert.deepEqual(labels(kept(undefined, names)), ['emph', 'mycmd'], 'the names in force alone before the first list');
});

// Intuition: when the worker is free again the open list takes the server's items, and the selection stays on the
// item the person moved to (it never jumps back to the first). Pins a refresh resetting the selection to 0.
test('reselect: the same item after a refresh, else the first', () => {
  const now = rank([{ label: 'emph' }, { label: 'empty' }, { label: 'emptyset' }], 'emp');
  assert.equal(reselect(now, 'empty'), 1);
  assert.equal(reselect(now, 'gone'), 0);
  assert.equal(reselect(now, undefined), 0);
  assert.equal(reselect([], 'empty'), 0);
});

// Intuition: a name from the lean scope list carries its argument letters (`args`), so the kept list writes its snippet:
// `\textbf` with `m` takes `{}` with the caret in it, an optional argument a removable `[options]` field, a star nothing.
// Pins `\emph` taken from the kept list during a run without its braces (the lean records had no argument shape).
test('snippet: from argument letters, the caret in the first field', () => {
  const { snippet } = self.satexViews.complete;
  assert.equal(snippet('textbf', 'm'), 'textbf{$1}$0');
  assert.equal(snippet('frac', 'mm'), 'frac{$1}{$2}$0');
  assert.equal(snippet('section', 'som'), 'section${1:[options]}{$2}$0');
  assert.equal(snippet('relax', ''), null);
  assert.equal(snippet('relax', undefined), null);
  const k = kept(undefined, [{ label: 'emph', args: 'm' }, { label: 'today' }]);
  assert.deepEqual([k[0].insertText, k[0].insertTextFormat], ['emph{$1}$0', 2]);
  assert.equal(k[1].insertText, undefined, 'no arguments: the name alone');
});

// Intuition: in math (`\( x - \su`) the math commands come first among equally close names, text ones after, by the
// server's flag (`math`, `data.math`) or the preview's own symbol table; outside math nothing changes. Pins `\sum` typed
// in `\(…\)` offering `\NeedsDocumentMetadata` first (user 2026-11-19).
test('rank: math commands first in math', () => {
  const items = [{ label: 'NeedsDocumentMetadata' }, { label: 'sum', math: true }, { label: 'subsection' }, { label: 'subset' }, { label: 'sumlimits', data: { math: true } }, { label: 'sub' }];
  assert.deepEqual(labels(rank(items, 'su', true)), ['sum', 'subset', 'sumlimits', 'sub', 'subsection', 'NeedsDocumentMetadata']);
  assert.deepEqual(labels(rank(items, 'sum', true)), ['sum', 'sumlimits', 'NeedsDocumentMetadata']);
  assert.deepEqual(labels(rank(items, 'su')), ['sub', 'sum', 'subset', 'sumlimits', 'subsection', 'NeedsDocumentMetadata'], 'in text: shorter first, as before');
  const { mathItem, docOf } = self.satexViews.complete;
  assert.equal(mathItem({ label: 'alpha' }), true, 'the symbol table knows \\alpha without a flag');
  assert.equal(mathItem({ label: 'textbf' }), false);
  assert.equal(mathItem({ label: 'alpha', math: false }), false, 'the server\'s flag wins');
  assert.equal(docOf({ label: 'x', data: { doc: 'Sets bold.' } }), 'Sets bold.');
});

// Intuition: an item that brings a load (`\usetikzlibrary{arrows.meta}` for `-Kite`, `\usepackage{tikz}` for `tikzpicture`)
// is written with it in one transaction, so its snippet goes in as plain text with the caret where its first field was.
// Pins `-Kite` accepted without its library line (the client dropped `additionalTextEdits`, user 2026-11-19).
test('plainSnippet: fields as their text, the caret at the first', () => {
  const { plainSnippet } = self.satexViews.complete;
  assert.deepEqual(plainSnippet('textbf{$1}$0'), { text: 'textbf{}', caret: 7 });
  assert.deepEqual(plainSnippet('tikzpicture}\n\t$0\n\\end{tikzpicture}'), { text: 'tikzpicture}\n\t\n\\end{tikzpicture}', caret: 14 });
  assert.deepEqual(plainSnippet('section${1:[options]}{${2:title}}$0'), { text: 'section[options]{title}', caret: 7 });
  assert.deepEqual(plainSnippet('-Kite'), { text: '-Kite', caret: 5 });
});

// Intuition: after taking a command whose caret lands in its braced first argument, that argument's completion opens at once
// (packages after `\usepackage{`, environments after `\begin{`), one rule for every command; an optional first field or
// none opens nothing. Pins `\usepackage` accepted with no package list (user 2026-11-19).
test('opensArgument: a braced first field opens its completion', () => {
  const { opensArgument } = self.satexViews.complete;
  assert.equal(opensArgument({ label: 'usepackage', insertText: 'usepackage{$1}', insertTextFormat: 2 }), true);
  assert.equal(opensArgument({ label: 'textbf', insertText: 'textbf{${1:text}}$0', insertTextFormat: 2 }), true);
  assert.equal(opensArgument({ label: 'section', insertText: 'section${1:[options]}{$2}$0', insertTextFormat: 2 }), false);
  assert.equal(opensArgument({ label: 'relax', insertText: 'relax' }), false);
});

// Intuition: a `}` typed right before a `}` that already closes the line's braces steps over it, whoever wrote that one (the
// editor's auto-close, a completion's `tabular}`): the name ends in exactly one `}`. Where the line still lacks a `}` the key
// adds one. Pins `\end{tabular}}` and `\end{tikzpicture}}` after completing and typing `}` (user 2026-11-19, twice).
test('stepsOver: a closing brace typed before one that closes the line steps over', () => {
  const { stepsOver } = self.satexViews.complete;
  assert.equal(stepsOver('\\end{tabular}', 12), true);
  assert.equal(stepsOver('\\begin{tikzpicture}', 18), true);
  assert.equal(stepsOver('\\textbf{\\emph{x}', 15), false, 'the outer group is still open: a second } is wanted');
  assert.equal(stepsOver('\\end{tabular', 12), false, 'nothing to step over');
  assert.equal(stepsOver('\\{x}', 3), true, 'an escaped brace counts for nothing');
  assert.equal(stepsOver('a} % {', 1), true, 'a comment counts for nothing');
});

// Intuition: a tip of a library not loaded yet (`-Kite`) has no picture from the run, so the list takes TeX's own from
// previews.json, cropped to the tip's end as the TikZ menu shows it; a shape or pattern comes whole, an unknown name has
// none. Pins "-Kite arrow tip of \usetikzlibrary{arrows.meta}" listed without a preview (user 2026-11-19).
test('previewOf: TeX pictures from previews.json, a tip cropped to its end', () => {
  const { previewOf } = self.satexViews.complete;
  const sheet = { 'arrow:-Kite': "<svg viewBox='0 -3 30 6'><path d='M0 0H30'/></svg>", 'shape:circle': '<svg>c</svg>' };
  assert.equal(previewOf(sheet, '-Kite'), "<svg viewBox='18 -3 12 6'><path d='M0 0H30'/></svg>");
  assert.equal(previewOf(sheet, 'circle'), '<svg>c</svg>');
  assert.equal(previewOf(sheet, 'nothing'), undefined);
});

// Intuition: in a TikZ path the next target's list opens by itself after `--`, `-|`, `|-` and a positioning `of `, as after
// `(`; outside a picture `--` is a dash and opens nothing. Pins having to press Ctrl+Space for node names (user 2026-11-19).
test('tikzTarget: a path operator in a picture opens the node names', () => {
  const { tikzTarget } = self.satexViews.complete;
  assert.equal(tikzTarget('\\begin{tikzpicture}\n\\draw (a) -- '), true);
  assert.equal(tikzTarget('\\begin{tikzpicture}\n\\draw (a) -|'), true);
  assert.equal(tikzTarget('\\begin{tikzpicture}\n\\node[below=of '), true);
  assert.equal(tikzTarget('\\tikz \\draw (0,0) |- '), true);
  assert.equal(tikzTarget('pages 3--'), false, 'a dash in prose');
  assert.equal(tikzTarget('\\begin{tikzpicture}\\end{tikzpicture}\nsome --'), false, 'after the picture');
});

// Intuition: `\` and one non-letter are a whole name (tex.web §354), so after `\\`, `\,` or `\;` no longer name is typed and
// Enter is a new line, not the list's first item. Pins `\\` Enter at a row's end writing `\greet` (user 2026-11-19).
test('controlSymbol: a whole control symbol before the caret', () => {
  const { controlSymbol } = self.satexViews.complete;
  assert.equal(controlSymbol('a & b \\\\'), '\\');
  assert.equal(controlSymbol('x\\,'), ',');
  assert.equal(controlSymbol('x\\\\,'), null, '`\\\\` then a comma: the comma is text');
  assert.equal(controlSymbol('x\\\\\\'), null, '`\\\\` then `\\`: a name starts');
  assert.equal(controlSymbol('x\\'), null);
  assert.equal(controlSymbol('\\gre'), null);
  assert.equal(controlSymbol('a, b'), null);
});

// Intuition: the server's `sortText` orders by code point, so its `~` (a package not loaded yet) sorts after the loaded
// environments' digits. Pins `\begin{cent` Tab taking `centernss` (struktex, `~0000…`) over `center` (`00100…`):
// `localeCompare` put `~` first.
test('rank: sortText by code point, `~` last', () => {
  const items = [{ label: 'centernss', sortText: '~0000centernss' }, { label: 'center', sortText: '00100center' }];
  assert.deepEqual(labels(rank(items, 'cent')), ['center', 'centernss']);
});
