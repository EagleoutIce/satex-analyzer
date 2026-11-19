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
