import { MARKUP, is, after, at } from './sources.mjs';

export default [
  { name: 'fz-complete-fast', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' \\emp', 'Tab', 'Quokka'], speed: 'burst', settle: true, expect: () => null, contains: [/\\emph\{?Quokka/],
    steps: 'MARKUP; click the end of "second"; burst " \\emp", Tab, "Quokka": Tab before the list came still takes \\emph (was: the list never opened, `\\empQuokka`)' },
  // Tab completion while a long run holds the worker (ui-37): the lists the last run left answer at once
  { kind: 'complete', name: 'tc-src-command', keys: ['\\bibliographys', 'Tab', 'Quokka'], want: /\n\\bibliographystyle\{Quokka\}\n\\end\{document\}/,
    steps: 'thesis.tex; Enter on a new line before \\end{document} starts a run; type "\\bibliographys", Tab, "Quokka" during it: \\bibliographystyle{Quokka} (a command the text uses, with its braces) at once (was: Tab waited for the run, the keys after it lost)' },
  { kind: 'complete', name: 'tc-src-ref', keys: ['\\ref{eq:jo', 'Tab', ' Quokka'], want: /\n\\ref\{eq:join\} ?Quokka|\n\\ref\{eq:join Quokka\}/,
    steps: 'thesis.tex; a run going on; type "\\ref{eq:jo", Tab, " Quokka": the label eq:join from the last run, then the keys (was: no list until the run ended)' },
  { name: 'tc-type-command', example: 'thesis.tex', word: is('Literate'), src: /Literate/, keys: [' ', 'Pause', '\\NoCaseC', 'Tab', 'Quokka'], pauseMs: 900, speed: 'burst', settle: true, expect: () => null, contains: [/Literate \\NoCaseChange\{Quokka\}/],
    steps: 'thesis.tex, Type mode; click the end of "Literate"; a space starts a run; during it "\\NoCaseC", Tab, "Quokka": \\NoCaseChange{Quokka} (its argument typeset as is) from the kept list (was: Tab waited up to 9 s, the keys behind it)' },
];
