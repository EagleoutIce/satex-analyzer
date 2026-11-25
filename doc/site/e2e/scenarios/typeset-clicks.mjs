// Clicks and keys in the Typeset view of leapyear.tex (kinds/user.mjs): its last rows set an undecided arm (`[not]`,
// the clock unknown) and an unknown value (⟨?⟩). Every click places the caret in the source or explains, never throws
// (a page error fails each step), and typed text deletes again, before and after the run lands.

// a failure's toast (if shown) and the source line up to the Type caret (`|` after it)
const WHERE = `(() => { const c = self.satexPdfCaret?.(), t = satexHealth().text, at = c?.own, s = document.getElementById('play-toast');
  const line = (k) => t.slice(t.lastIndexOf('\\n', k - 1) + 1, k) + '|';
  return (s?.classList.contains('on') && /fail|Not typed|Waiting|no source place/.test(s.textContent) ? 'toast:' + s.textContent + ' ' : '') + (at != null ? 'caret:' + line(at) : 'no caret');
})()`;
// the editor's line number at its cursor
const EDITOR = `satexTest.run({ carets: true }).then((c) => 'editor line ' + satexHealth().text.slice(0, c.editor).split('\\n').length)`;
// the maybe layer on: undecided arms drawn in their row
const MAYBE = { js: `(() => { const b = document.querySelector('#typeset-layers [data-layer=maybe]'); if (b && !b.checked) b.click(); return 1; })()` };
const TS = { do: { press: { sel: '.side-tabs [data-panel=play-typeset]' } } }, TYPE = { do: { toggle: { sel: '#ts-type', want: true } } };
// the row of the undecided `\tleapy`, by its choice
const ROW36 = '#typeset-body p.ts:has(.ts-choice) .ts-text';
const base = { kind: 'user', example: 'leapyear.tex' };

export default [
  { ...base, name: 'ts-arm-click', do: [TS, TYPE, MAYBE, { wait: 400 }, { click: { range: '#typeset-body .ts-choice', text: 'not', dx: -4 } }, { wait: 400 }, { js: WHERE }],
    expect: [{ js: /^caret:\s*\\def\\tleapy\{no?\|$/ }],
    steps: 'leapyear.tex; Typeset view, Type on, maybe layer on; click in the undecided arm `not` of line 36: the caret goes into `\\def\\tleapy{not }` (was: "can\'t access property undefined", the row read without its characters)' },
  { ...base, name: 'ts-arm-start', do: [TS, TYPE, MAYBE, { wait: 400 }, { click: { range: '#typeset-body .ts-choice', text: 'n', dx: -6 } }, { wait: 400 }, { js: WHERE }],
    expect: [{ js: /^caret:\s*\\def\\tleapy\{\|$/ }],
    steps: 'click before the arm `not`: the caret at the start of the definition\'s `not` (was: before the `\\ckleapyear` call, the space before the arm counted outside the output)' },
  { ...base, name: 'ts-arm-double', do: [TS, TYPE, MAYBE, { wait: 400 }, { click: { range: '#typeset-body .ts-choice', text: 'not', dx: -4 }, double: true }, { wait: 400 }, { js: WHERE }],
    expect: [{ js: /^caret:\s*\\def\\tleapy\{no?\|$/ }],
    steps: 'a double click on the arm: no page error, the caret in the definition' },
  { ...base, name: 'ts-macro-word', do: [TS, TYPE, MAYBE, { wait: 400 }, { click: { range: ROW36, text: 'year', dx: -4 } }, { wait: 400 }, { js: WHERE }],
    expect: [{ js: /^caret:\s*The ye?a?\|$/ }],
    steps: 'click in `year` of line 36, text of the `\\ckleapyear` body: the caret in the body (was: the same throw)' },
  { ...base, name: 'ts-unknown-click', do: [TS, TYPE, MAYBE, { wait: 400 }, { click: { sel: '#typeset-body .ts-note[title]' } }, { wait: 400 }, { js: EDITOR }],
    expect: [{ js: /^editor line 36$/ }],
    steps: 'click on the unknown value ⟨?⟩: its note explains it, the editor goes to its row\'s source line (36)' },
  { ...base, name: 'ts-type-delete', do: [{ ts: '1996' }, { type: '99' }, { key: 'Backspace' }, { key: 'Backspace' }, { wait: 300 }, { js: WHERE }],
    expect: [{ has: /\\ckleapyear\{1996\}/ }, { not: /19969/ }, { js: /^caret:\\ckleapyear\{1996\|$/ }],
    steps: 'click after 1996 of line 30, type 99, Backspace twice: the source is as before' },
  { ...base, name: 'ts-type-click-delete', do: [{ ts: '1996' }, { type: '99' }, { click: { range: '#play-typeset', text: '199699', dx: -1 } }, { key: 'Backspace' }, { key: 'Backspace' }, { wait: 300 }, { js: WHERE }],
    expect: [{ has: /\\ckleapyear\{1996\}/ }, { not: /19969/ }, { js: /^caret:\\ckleapyear\{1996\|$/ }],
    steps: 'type 99 into 1996, click at once after the typed digits (shown pending, the run not landed), Backspace twice: the digits go (was: "has no source place in the run", the caret gone)' },
  { ...base, name: 'ts-click-shifted', stepMs: 2000, do: [TS, TYPE, { wait: 400 }, { editor: '\\def', before: true }, { do: { insert: { at: '\\def', k: 0, text: '%\n%\n%\n' } }, pause: 0 }, { click: { range: '#play-typeset', text: '1996', dx: -1 } }, { type: '7' }, { wait: 300 }, { js: WHERE }],
    expect: [{ has: /\\ckleapyear\{19967\}/ }, { js: /^caret:\\ckleapyear\{19967\|$/ }],
    steps: 'three lines added at the top, then at once a click after 1996 and a key: the rows\' lines are moved by the edit, the caret is placed without waiting for the run (was: "Waiting for the analysis to read this text…")' },
  { ...base, name: 'pdf-click-shifted', pdf: true, stepMs: 2000, do: [{ editor: '\\def', before: true }, { do: { insert: { at: '\\def', k: 0, text: '%\n%\n%\n' } }, pause: 0 }, { click: { word: '1996', fx: 0.95 } }, { type: '7' }, { wait: 300 }, { js: WHERE }],
    expect: [{ has: /\\ckleapyear\{19967\}/ }, { js: /^caret:\\ckleapyear\{19967\|$/ }],
    steps: 'PDF view built, three lines added at the top, then at once a click after 1996 on the page and a key: SyncTeX\'s line is moved by the edit, the caret placed at once (was: "Waiting for the analysis to read this text…" until a run and build that never came)' },
  { ...base, name: 'ts-after-pdf-select', pdf: true, do: [{ select: '1990' }, { ts: '1996' }, { type: '9' }, { key: 'Backspace' }, { wait: 300 }, { js: WHERE }],
    expect: [{ has: /\\ckleapyear\{1990\}/ }, { has: /\\ckleapyear\{1996\}/ }, { js: /^caret:\\ckleapyear\{1996\|$/ }],
    steps: 'a word selected in the PDF, then a click and keys in the Typeset view: the keys go to the caret (was: they erased the PDF selection\'s 1990, or "the selection is not found")' },
];
