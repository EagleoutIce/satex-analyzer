import { BANG, is, after, MAYBE } from './sources.mjs';

export default [
  // overlays never over a blank page (user 2026-11-18): pdf.js fails to draw page 1, its tints stay hidden, page 2's show
  { kind: 'bare', name: 'bare-page', fail: [1], ok: 2,
    steps: 'thesis.tex, PDF, Compile with pdf.js drawing page 1 failing: page 1 stays blank with no tints over it, page 2 shows its tints' },
  { kind: 'tint', name: 'tint-bang', source: BANG, example: 'hello.tex', want: [['Hello,', 'macro'], ['Walter!', 'arg'], ['you!', '!macro'], ['Generally', null]], toggle: true,
    steps: 'the \\greet doc of the 13:04 report; PDF, Compile, Type on: "Hello," is macro, Walter argument, the "!" after "you" is plain (no macro tint); toggle the first layer off and on' },
  { kind: 'tint', name: 'tint-thesis', whole: ['SaTeX'], toggle: true,
    steps: 'thesis.tex, PDF, Compile, Type on: each "SaTeX" is tinted in one piece (user report 15:30); toggle the first layer off and on: tints stay put (15:29)' },
  // the Typeset view
  { kind: 'ts', name: 'ts-maybe', source: MAYBE, example: 'hello.tex', check: 'maybe', want: [['true', 'good morning'], ['false', 'good afternoon']],
    steps: 'a doc with \\ifnum\\pdfuniformdeviate 1440<720 … \\else … \\fi (a random number the run cannot decide); Typeset view: both cases drawn inline as a choice (a switch only for a reference the next run decides)' },
  { kind: 'ts', name: 'ts-tints', check: 'tints',
    steps: 'thesis.tex, Typeset view: origin tints lie on text; each layer switched off and on leaves every tint where it was' },
];
