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
  // the case typed breaks a tie of equally close names: `\\Blind` offers `Blinddocument` before `blinddocument` (user 2026-11-22)
  assert.deepEqual(labels(rank([{ label: 'blinddocument', sortText: '1' }, { label: 'Blinddocument', sortText: '2' }], 'Blind')), ['Blinddocument', 'blinddocument']);
  assert.deepEqual(labels(rank([{ label: 'blinddocument', sortText: '2' }, { label: 'Blinddocument', sortText: '1' }], 'blind')), ['blinddocument', 'Blinddocument']);
  // a label's part, then what it is found by (a citation's title)
  assert.deepEqual(labels(rank([{ label: 'knuth84', filterText: 'The TeXbook' }, { label: 'texbook-notes' }], 'texbook')), ['texbook-notes', 'knuth84']);
});

// Intuition: one letter typed is the start of a name, not the one-letter accent of that name: `\t` lists `\textbf`
// (the server's prior: the seed of common commands) before the tie accent `\t`. Pins the page's list after `\t`
// opening with the accent (user 2026-11-22: useless names first, `\textbf` only later).
test('rank: one letter typed is a prefix, never an exact name', () => {
  const items = [{ label: 't', data: { prior: '1999999~' } }, { label: 'textbf', data: { prior: '19999990000' } }, { label: 'tt', data: { prior: '1999999~' } }];
  assert.deepEqual(labels(rank(items, 't')), ['textbf', 't', 'tt']);
  assert.deepEqual(labels(rank(items, 'tt')), ['tt', 'textbf'], 'two letters: the exact name first');
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
// `\textbf` with `m` takes `{}` with the caret in it; what may be left out (an optional argument, a star) is left to the
// writer, so `\section` takes `{}` with the caret in it and no `[options]` text stays behind.
// Pins `\emph` taken from the kept list during a run without its braces (the lean records had no argument shape).
test('snippet: from argument letters, the caret in the first field', () => {
  const { snippet } = self.satexViews.complete;
  assert.equal(snippet('textbf', 'm'), 'textbf{$1}$0');
  assert.equal(snippet('frac', 'mm'), 'frac{$1}{$2}$0');
  assert.equal(snippet('section', 'som'), 'section{$1}$0');
  assert.equal(snippet('item', 'o'), null, 'only optional arguments: the name alone');
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

// Intuition: Enter right after a typed name takes the list's item only when that item writes more than the name: a
// prefix (`\noind`) or arguments (`\begin` → `begin{$1}`). After `\noindent` in full it is Enter's new line. Pins
// `\noindent` Enter writing nothing (the named-Enter path took the item, user 2026-11-19).
test('whole: a complete name with nothing more to write', () => {
  const { whole } = self.satexViews.complete;
  assert.equal(whole({ label: 'noindent', insertText: 'noindent' }, 'noindent'), true);
  assert.equal(whole({ label: 'noindent', insertText: 'noindent' }, 'noind'), false, 'a prefix takes the item');
  assert.equal(whole({ label: 'begin', insertText: 'begin{$1}', insertTextFormat: 2 }, 'begin'), false, 'arguments to write');
  assert.equal(whole({ label: 'toprule', insertText: 'toprule', additionalTextEdits: [{}] }, 'toprule'), false, 'a load to write');
  assert.equal(whole(null, 'x'), false);
});

// Intuition: the name typed at the caret is the `\` and the letters at that place (tex.web §356), the run's letters
// (query `letters`, replayed in run order). In a document's body `_` is a subscript and `@` other: after `\sum_` no name
// is typed and no list opens; under `\ExplSyntaxOn` (`_`, `:` letters) `\c__fp` goes on as one name, and only names
// typeable there are offered. Pins `\(\sum_` opening `\c__socket_tagsupport/mc_args_int` in the editor and the pages.
test('nameBefore, typeable, lettersAt: the name ends where the letters there end', () => {
  const { nameBefore, typeable, lettersAt } = self.satexViews.complete;
  const body = '', expl = '_:';
  assert.equal(nameBefore('\\(\\sum', body), 'sum');
  assert.equal(nameBefore('\\(\\sum_', body), null, '`_` ends the name: none typed');
  assert.equal(nameBefore('\\(\\sum_', expl), 'sum_', 'under expl3 `_` is a letter: the name goes on');
  assert.equal(nameBefore('x \\c__fp', expl), 'c__fp');
  assert.equal(nameBefore('x \\c__fp', body), null);
  assert.equal(nameBefore('\\make@', body), null, '`@` is other in a body');
  assert.equal(nameBefore('\\make@', '@'), 'make@');
  assert.equal(nameBefore('a \\', body), '', 'a lone `\\` starts a name');
  assert.equal(nameBefore('a \\\\sum', body), null, '`\\\\` is a control symbol, `sum` text');
  assert.equal(nameBefore('\\ä', body), 'ä', 'a non-ASCII letter goes on');
  assert.ok(typeable('sum', body) && typeable(',', body) && typeable('@', body), 'a control symbol is always typeable');
  assert.ok(!typeable('c__socket_tagsupport/mc_args_int', expl), '`/` is a letter nowhere here');
  assert.ok(!typeable('l_tmpa_tl', body) && typeable('l_tmpa_tl', expl));
  assert.ok(!typeable('@empty', body) && typeable('@empty', '@'));
  // a file opened with `@` a letter (line 0), `\ExplSyntaxOn` on line 4, `\ExplSyntaxOff` on line 6 col 3
  const changes = [{ line: 0, col: 0, char: '@', letter: true }, { line: 4, col: 1, char: '_', letter: true }, { line: 4, col: 1, char: ':', letter: true },
    { line: 6, col: 3, char: '_', letter: false }, { line: 6, col: 3, char: ':', letter: false }];
  assert.equal(lettersAt(changes, 1, 1), '@');
  assert.equal(lettersAt(changes, 4, 1), '@', 'the change takes effect after its place');
  assert.equal(lettersAt(changes, 5, 1), '@_:');
  assert.equal(lettersAt(changes, 6, 3), '@_:');
  assert.equal(lettersAt(changes, 6, 4), '@');
  assert.equal(lettersAt(null, 3, 1), '', 'no run read the file: TeX\'s letters only');
});

// Intuition: a control symbol is `\` and one non-letter *there*: `\@` in a body is one (the space factor), in a package
// `@` is a letter and a name goes on. Pins `@` counted a letter everywhere.
test('controlSymbol: by the letters at the caret', () => {
  const { controlSymbol } = self.satexViews.complete;
  assert.equal(controlSymbol('x\\@'), '@');
  assert.equal(controlSymbol('x\\@', '@'), null);
  assert.equal(controlSymbol('x\\_', '_:'), null);
  assert.equal(controlSymbol('x\\_'), '_');
});

// Intuition: what the document loads ranks before what a package it does not load would define, however close the
// name: `\textb` takes `\textbf`, not drm's `\textb` (whose item adds `\usepackage{drm}`). Pins the user's 2026-11-21
// list with `\textb (adds \usepackage{drm})` first.
// Intuition: the server's rank of a command that holds whatever is typed (tier, the document's uses, the seed of common
// commands) travels with the item (`data.prior`), so a list answered from kept items (a run going, sortText dropped as
// another word's) ranks as the server does. Pins `\textb` listing textbackslash, textbaht, textbar … before textbf.
test('rank: the server\'s prior orders equally close names, kept lists too', () => {
  const p = (label, prior) => ({ label, data: { prior } });
  const items = [p('textbackslash', '1999999' + '0040'), p('textbar', '1999999' + '0040'), p('textbf', '1999999' + '0000'), { label: 'textbaht' }];
  assert.deepEqual(labels(rank(items, 'textb')), ['textbf', 'textbar', 'textbackslash', 'textbaht']);
  const used = [p('textbf', '1999999' + '0000'), p('textbar', '1999997' + '0040')];
  assert.deepEqual(labels(rank(used, 'textb')), ['textbar', 'textbf'], 'the document\'s uses before the seed');
});
test('rank: a name of a package not loaded after the loaded ones', () => {
  const items = [{ label: 'textb', additionalTextEdits: [{ newText: '\\usepackage{drm}\n' }] }, { label: 'textbf' }, { label: 'textbullet' }];
  assert.deepEqual(labels(rank(items, 'textb')), ['textbf', 'textbullet', 'textb']);
  assert.deepEqual(labels(rank([{ label: 'textb', sortText: '~0001textb' }, { label: 'textbf', sortText: '1textbf' }], 'textb')), ['textbf', 'textb'], 'by the server\'s `~` too');
});

// Intuition: a list answered from the kept items during a run shows what the server said of each name last: its
// shape beside the name, its description and its snippet, never a bare name. Pins the PDF page's list for `\secti`
// during a run: `\section` without its arguments or description while the server's own answer had both (2026-11-21).
test('kept: a name keeps the shape, description and snippet the server gave it last', () => {
  const { remember, recall, shapeOf, docOf } = self.satexViews.complete;
  const memo = new Map();
  const section = { label: 'section', detail: '\\section*[⟨options⟩]{⟨arg⟩}  (article)', insertText: 'section{${1:arg}}$0', insertTextFormat: 2,
    labelDetails: { detail: '*[⟨options⟩]{⟨arg⟩}', description: 'article' }, data: { doc: 'Starts a section.' }, sortText: '1', textEdit: { newText: 'x' } };
  remember(memo, [section, { label: 'relax' }]);
  assert.deepEqual([...memo.keys()], ['section'], 'a name with nothing to say is not kept');
  // the names in force (the lean scope list) know only the bare name and its letters
  const k = kept(undefined, [{ label: 'section', detail: '\\section', args: 'som' }, { label: 'relax', detail: '\\relax' }], memo);
  assert.equal(shapeOf(k[0]), '*[⟨options⟩]{⟨arg⟩}');
  assert.equal(docOf(k[0]), 'Starts a section.');
  assert.deepEqual([k[0].insertText, k[0].detail], ['section{${1:arg}}$0', section.detail]);
  assert.equal(k[0].textEdit, undefined, 'a range of another place is never kept');
  assert.deepEqual([shapeOf(k[1]), docOf(k[1]), k[1].detail], ['', '', '\\relax']);
  // a newer answer's own fields win over the kept ones
  const own = recall(memo, { label: 'section', detail: '\\section{⟨title⟩}  (mine)', insertText: 'section{$1}$0', labelDetails: { detail: '{⟨title⟩}' } });
  assert.deepEqual([shapeOf(own), own.insertText, own.detail], ['{⟨title⟩}', 'section{$1}$0', '\\section{⟨title⟩}  (mine)']);
});
