// Type mode's keys at the spots that matter (pdfedit.ts `key`): one table per theme, each row the text, the caret, the
// key and what the source then is (with the caret as `|`), or the reason the key is refused.
import assert from 'node:assert/strict';
import { test, apply } from './harness.mjs';

const E = self.satexPdfEdit;
/** Checks each `[text, at, key, want]`: `want` the text after the key with `|` at the caret, or a RegExp of the refusal. */
const table = (rows, o = {}) => {
  for (const [text, at, k, want] of rows) {
    const e = E.key(text, at, k, o), name = `${k} at ${JSON.stringify(text.slice(0, at) + '|' + text.slice(at))}`;
    if (want instanceof RegExp) { assert.equal(typeof e, 'string', `${name}: refused`); assert.match(e, want, name); continue; }
    assert.notEqual(typeof e, 'string', `${name}: ${e}`);
    const now = apply(text, e), caret = e[0] + e[3];
    assert.equal(now.slice(0, caret) + '|' + now.slice(caret), want, name);
  }
};

// Intuition: Enter in running text ends the paragraph, also where a line of the source ends or starts (the blank line
// joins the line break there). Pins Enter at a source line's end refused as "it would change the markup around it".
test('Enter: a paragraph at a line end or start', () => table([
  ['one\ntwo', 3, 'Enter', 'one\n\n|\ntwo'],
  ['one\ntwo', 4, 'Enter', 'one\n\n\n|two'],
  ['one \ntwo', 4, 'Enter', 'one \n\n|\ntwo'],
  ['one two', 3, 'Enter', 'one\n\n| two'],
  ['one two', 3, 'Shift+Enter', 'one\n| two'],
  ['\\emph{a\nb}', 7, 'Enter', /paragraph cannot end inside \\emph/],
  ['\\section{T}', 9, 'Enter', /paragraph cannot end inside \\section/],
  ['\\section{T}', 9, 'Shift+Enter', '\\section{\n|T}'],
  ['a $x$ b', 3, 'Enter', /one line/],
]));

// Intuition: a command's option is structure like its argument: its brackets stay, its command keeps it, nothing is
// typed between them. Pins `\cite[p.~3]{k}` losing its `[` to Delete, a letter after `\cite` writing `\cite{}z[p.~3]{k}`
// (the option and key turned text), and a blank line typed between the option and the argument.
test('options: brackets and their command stay together', () => table([
  ['\\cite[p.~3]{k} x', 5, 'Delete', /\\cite/],
  ['\\cite[p.~3]{k} x', 6, 'Backspace', /\\cite/],
  ['\\cite[p.~3]{k} x', 5, 'z', /markup around it/],
  ['\\cite[p.~3]{k} x', 5, ' ', /markup around it/],
  ['\\cite[p]{k}', 8, 'Enter', /part \] from its argument/],
  ['\\cite[p]{k}', 8, 'Shift+Enter', '\\cite[p]\n|{k}'],
  ['\\item[x] y', 7, ']', /unbalance/],
  ['\\item[x] y', 7, '[', '\\item[x[|] y'],
  ['\\item y', 6, '[', /unbalance/],
]));
// The gate itself, not only key: a splice that cuts an option's bracket or puts a blank line before an argument.
test('options: the gate refuses splices that part them', () => {
  const t = 'See \\cite[p.~6]{key1} now.';
  for (const [a, b, body] of [[9, 10, ''], [9, 12, ''], [9, 10, '\\%'], [15, 15, '\n\n'], [15, 16, ''], [9, 9, 'x']]) assert.equal(typeof E.checked(t, [a, b, body]), 'string', JSON.stringify(apply(t, [a, b, body])));
  assert.match(E.checked(t, [10, 13, 'pp.'], {}), /option/, "a key command's option is no text");
  assert.equal(E.checked('\\section[Sh]{Long}', [10, 11, 'x'], {}), null, "a heading's short title is text");
  assert.equal(E.checked(t, [15, 15, '\n'], {}), null, 'one line break: TeX skips it');
});

// Intuition: Backspace and Delete at a formula's edge take its character inside, a formula of one character whole, the
// same for `$…$`, `\(…\)` and `\[…\]`; never a lone delimiter. Pins Backspace after a closing `$` (and Delete before an
// opening one) refused, while `\)` already worked.
test('Backspace and Delete at math delimiters', () => table([
  ['a \\(x\\) b', 7, 'Backspace', 'a | b'],
  ['a \\(x\\) b', 2, 'Delete', 'a | b'],
  ['a \\(xy\\) b', 8, 'Backspace', 'a \\(x|\\) b'],
  ['a \\(xy\\) b', 2, 'Delete', 'a \\(|y\\) b'],
  ['a \\(xy\\) b', 4, 'Backspace', /unbalance/],
  ['a \\[x\\] b', 2, 'Delete', 'a | b'],
  ['a \\[xy\\] b', 8, 'Backspace', 'a \\[x|\\] b'],
  ['a $x$ b', 4, 'Backspace', 'a | b'],
  ['a $x$ b', 5, 'Backspace', 'a | b'],
  ['a $xy$ b', 6, 'Backspace', 'a $x|$ b'],
  ['a $xy$ b', 2, 'Delete', 'a $|y$ b'],
  ['a $x$ b', 2, 'Delete', 'a | b'],
  ['a $xy$ b', 5, 'Backspace', 'a $x|$ b'],
  ['a \\$x b', 4, 'Backspace', 'a |x b'],
  ['a $ $ b', 3, 'Backspace', 'a | b'],
]));

// Intuition: braces go only in pairs and only when they are no command's argument; an empty group goes whole.
test('Backspace and Delete at braces', () => table([
  ['a {} b', 3, 'Backspace', 'a | b'],
  ['a {} b', 3, 'Delete', 'a | b'],
  ['a {x} b', 3, 'Backspace', /unbalance/],
  ['a {x} b', 5, 'Backspace', /unbalance/],
  ['\\emph{x} b', 6, 'Backspace', /unbalance/],
  ['\\emph{} b', 6, 'Backspace', /argument of \\emph/],
]));

// Intuition: an escape is one character on the page, so one key takes it whole; a control word that is no escape stays.
test('Backspace and Delete at escapes', () => table([
  ['a \\_ b', 4, 'Backspace', 'a | b'],
  ['a \\_ b', 2, 'Delete', 'a | b'],
  ['a \\textbackslash{} b', 18, 'Backspace', 'a | b'],
  ['a \\% b', 2, 'Delete', 'a | b'],
  ['a \\\\ b', 4, 'Backspace', /command \\\\/],
]));

// Intuition: verbatim takes every character as it is and keeps its closer and, inline, its one line.
test('verbatim: keys as typed, closer kept', () => table([
  ['a \\verb|x{y| b', 10, 'Backspace', 'a \\verb|x|y| b'],
  ['a \\verb|x{y| b', 8, 'Backspace', 'a \\verb||x{y| b'],
  ['a \\verb|x{y| b', 11, 'Delete', 'a \\verb|x{y|| b'],
  ['a \\verb|x{y| b', 9, 'Enter', /one line/],
  ['a \\verb|x{y| b', 9, '|', 'a \\verb|x|{y| b'],
  ['\\begin{verbatim}\nab\n\\end{verbatim}', 18, 'Enter', '\\begin{verbatim}\na\n|b\n\\end{verbatim}'],
  ['\\begin{verbatim}\nab\n\\end{verbatim}', 17, '%', '\\begin{verbatim}\n%|ab\n\\end{verbatim}'],
]));

// Intuition: in text a special character is written so the page shows it; in math it goes in as typed but for the
// ones that would end the formula's structure.
test('characters: escaped in text, raw in math', () => table([
  ['a b', 1, '#', 'a\\#| b'],
  ['a b', 1, '&', 'a\\&| b'],
  ['a b', 1, '%', 'a\\%| b'],
  // a tie is what a LaTeX author types `~` for (`See~\\ref`): kept (user 2026-11-21, u-tilde-pdf)
  ['a b', 1, '~', 'a~| b'],
  ['a b', 1, '^', 'a\\textasciicircum{}| b'],
  ['a b', 1, '_', 'a\\_| b'],
  ['a b', 1, '{', 'a\\{| b'],
  ['a $x$ b', 3, '{', 'a ${|}x$ b'],
  ['a $x$ b', 3, '}', 'a $\\}|x$ b'],
  ['a $x$ b', 3, '&', 'a $\\&|x$ b'],
  ['a $x$ b', 3, '_', 'a $_|x$ b'],
]));

// Intuition: what is no text refuses typing: a comment, an environment's name, a key.
test('refused: comments, environment names, keys', () => table([
  ['% c\na', 2, 'x', /comment/],
  ['% c\na', 3, 'Delete', /comment/],
  ['\\begin{itemize}', 9, 'x', /./],
  ['\\label{a}', 7, 'x', /key/],
  ['\\ref{a} b', 6, 'Backspace', /key/],
]));

// Intuition: the fresh span (text typed since the caret was put) grows with edits in or at it and moves with edits
// before it; an edit elsewhere starts a new one. It decides which commands may still change.
test('grow: the fresh span follows the edits', () => {
  assert.deepEqual(E.grow(null, 3, 3, 2), [3, 5], 'a first edit');
  assert.deepEqual(E.grow([3, 5], 5, 5, 1), [3, 6], 'typed at its end');
  assert.deepEqual(E.grow([3, 6], 5, 6, 0), [3, 5], 'erased inside it');
  assert.deepEqual(E.grow([3, 5], 9, 9, 1), [9, 10], 'typed elsewhere: a new span');
  assert.deepEqual(E.grow([3, 5], 0, 1, 0), [2, 4], 'erased before it: it moves');
  assert.deepEqual(E.grow([3, 5], 7, 8, 0), [3, 5], 'erased after it: it stays');
  assert.equal(E.grow(null, 3, 4, 0), null, 'an erase alone types nothing');
  // a name typed since the caret was put may still change; an older one not
  assert.equal(E.naming('a \\em', 5, [2, 5]), true);
  assert.equal(E.naming('a \\em', 5, null), false);
  assert.equal(E.naming('a \\', 3, null), true, 'a lone backslash is a name begun');
  assert.equal(E.naming('a \\\\', 4, null), false, 'a line break is no name');
});

// Intuition: Tab steps cells, Enter adds a row only in a table; elsewhere Tab has nowhere to go.
test('tables: Enter adds a row, cells step', () => {
  const t = '\\begin{tabular}{ll}\na & b \\\\\nc & d\n\\end{tabular}';
  table([[t, t.indexOf('b \\\\') + 1, 'Enter', t.replace('b \\\\\n', 'b \\\\\n| & \\\\\n')]]);
  assert.equal(typeof E.key(t, t.indexOf('a'), '&'), 'string', 'a typed & is refused: Tab steps');
  assert.deepEqual(E.broken('a b', 1, false), [1, 1, '\n\n', 2]);
  assert.deepEqual(E.broken('a b', 1, true), [1, 1, '\n', 1]);
  assert.match(E.broken('\\verb|ab| c', 7, false), /one line/);
});

// Intuition: Type mode's Right at a command argument's end leaves the command (caret after `}`), Left at its start leaves
// before it; Ctrl+B outside bold starts `\textbf{|}`, in it splits it in the middle and leaves it at either end. Pins a
// caret stuck at `}` (user 2026-11-21: "Right does not get me out of \textbf{world}").
test('a command argument: arrows leave it, Ctrl+B toggles', () => {
  const E = self.satexPdfEdit, t = 'Say \\textbf{world} now', open = t.indexOf('{') + 1, close = t.indexOf('}');
  assert.equal(E.leave(t, close, 1), close + 1);
  assert.equal(E.leave(t, open, -1), t.indexOf('\\textbf'));
  assert.equal(E.leave(t, open + 2, 1), null, 'inside the word: a plain step');
  assert.deepEqual(E.toggle(t, close, 'textbf'), [close, close, '', 1]);
  assert.deepEqual(E.toggle(t, open, 'textbf'), [t.indexOf('\\textbf'), t.indexOf('\\textbf'), '', 0]);
  assert.deepEqual(E.toggle(t, open + 2, 'textbf'), [open + 2, open + 2, '}\\textbf{', 1]);
  assert.deepEqual(E.toggle(t, 2, 'textbf'), [2, 2, '\\textbf{}', 8]);
  assert.deepEqual(E.toggle(t, open + 2, 'textit'), [open + 2, open + 2, '\\textit{}', 8], 'italic inside bold: nested');
  assert.equal(E.callAround('\\a{\\{x}', 5)?.name, 'a', 'an escaped brace is no group');
});

// Intuition: Ctrl+Right and Ctrl+Left step by words as an editor does: to a word's end (right) or start (left), over
// blanks, line ends and markup, never stopping in a command's name or a key. Pins Type mode's Ctrl+arrows moving one
// character (user 2026-11-22).
test('Ctrl+arrows: by words, never into code', () => {
  const t = 'Some plain words.\nNext \\textbf{bold} see \\ref{sec:a} end', why = (a, b) => E.editable(t, a, b, []);
  const right = (at) => E.wordStep(t, at, 1, why), left = (at) => E.wordStep(t, at, -1, why), at = (w, k = 0) => t.indexOf(w) + k;
  assert.equal(right(at('plain', 5)), at('words', 5), 'from a word end to the next word end');
  assert.equal(right(at('plain', 2)), at('plain', 5), 'in a word: its end');
  assert.equal(right(at('words', 5)), at('Next', 4), 'across the line end and the period');
  assert.equal(right(at('Next', 4)), at('bold', 4), 'over \\textbf{ into its argument');
  assert.equal(right(at('see', 3)), at('end', 3), 'over \\ref{sec:a}: a key is no word');
  assert.equal(left(at('see')), at('bold'), 'left: a word start, inside the argument');
  assert.equal(left(at('bold')), at('Next'), 'left over \\textbf{');
  assert.equal(left(at('Next')), at('words'), 'left across the line');
  assert.equal(left(at('end')), at('see'), 'left over the key');
  assert.equal(right(t.length), t.length, 'no word that way: stays');
  assert.equal(left(0), 0);
});

// Intuition: LaTeX reads a footnote's (and a margin note's) argument `\long`, so Enter there starts a paragraph inside
// it and the caret stays in the note. A short command's argument still refuses one, also around a footnote in it.
// Pins Enter at a footnote's end writing the paragraph after `}` in the body (user 2026-11-22 18:54).
test('Enter in a footnote: a paragraph inside it', () => {
  const t = 'Body.\\footnote{Foobar chicken!!} More.', m = 'A \\marginpar{note} b', n = 'A \\textbf{x\\footnote{y}} b';
  table([
    [t, t.indexOf('}'), 'Enter', 'Body.\\footnote{Foobar chicken!!\n\n|} More.'],
    [t, t.indexOf(' chicken'), 'Enter', 'Body.\\footnote{Foobar\n\n| chicken!!} More.'],
    [m, m.indexOf('}'), 'Enter', 'A \\marginpar{note\n\n|} b'],
    [n, n.indexOf('}'), 'Enter', /inside \\textbf/],
  ]);
});

// Intuition: an inline formula is one source line (TeX sets no line break in it): Enter at its end steps out past the
// closer, inside it does nothing. Left and Right in math stop in each `_{…}`/`^{…}` slot, an empty one too. Pins
// `\(\sum` Enter `_{42}^{}` (a raw line break in the formula) and Left after `^{}` not reaching the slot (user 20:37).
test('inline math: one line, slots as caret stops', () => {
  const t = 'In \\(\\sum_{42}^{}\\) ok', end = t.indexOf('\\)');
  table([[t, end, 'Enter', 'In \\(\\sum_{42}^{}\\)| ok'], [t, t.indexOf('_'), 'Enter', /one line/], ['a $x$ b', 4, 'Enter', 'a $x$| b']]);
  assert.equal(E.slotStep(t, end, -1), end - 1, '^{}| Left: into the empty slot');
  assert.equal(E.slotStep(t, end - 1, -1), t.indexOf('^'), '^{|} Left: before the script');
  assert.equal(E.slotStep(t, t.indexOf('^'), 1), t.indexOf('^') + 2, 'Right over ^{ into its slot');
  assert.equal(E.slotStep(t, t.indexOf('42') + 1, -1), null, 'a digit: the plain step');
  assert.equal(E.slotStep('a_{b}', 4, -1), null, 'outside math: none');
});
