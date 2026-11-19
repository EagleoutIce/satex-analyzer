import { MARKUP, KINDS, is, after, at } from './sources.mjs';

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
  // a command's arguments and math ranking (user 2026-11-19): `\textbf` brings `{}` with the caret in it, in the editor and
  // in Type mode, from the server's list and from the kept one during a run; `\sum` typed in math takes `\sum`
  { kind: 'complete', name: 'tc-src-textbf', keys: ['\\textb', 'Tab', 'Quokka'], want: /\n\\textbf\{Quokka\}\n\\end\{document\}/,
    steps: 'thesis.tex; a run going on; type "\\textb", Tab, "Quokka": \\textbf{Quokka} from the kept list, the caret in its braces (was: \\textbfQuokka, the lean list had no argument shape)' },
  { kind: 'complete', name: 'tc-src-sum-math', keys: ['\\(x - \\sum', 'Tab', '_i \\)'], want: /\n\\\(x - \\sum ?_i \\\)\)?\n/,
    steps: 'thesis.tex; a run going on; type "\\(x - \\sum", Tab, "_i \\)": \\sum taken in math (was: \\NeedsDocumentMetadata first, \\sum missing from the kept list)' },
  { kind: 'complete', name: 'tc-src-env-clean', keys: ['\\begin{itemi', 'Tab', 'x', 'ArrowDown', 'ArrowDown', 'ArrowDown'], want: /\\begin\{itemize\}/, clean: true,
    steps: 'thesis.tex; a run going on; type "\\begin{itemi", Tab, "x", then leave by ArrowDown: no snippet field or linked \\begin/\\end box stays drawn (was: boxes around {tikzpicture}, {document} and whole lines stayed, user 2026-11-19)' },
  { name: 'tc-type-textbf', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' \\textb', 'Tab', 'Quokka'], speed: 'burst', settle: true, expect: () => null, contains: [/a second \\textbf\{Quokka\}/],
    steps: 'MARKUP, Type mode; click the end of "second"; type " \\textb", Tab, "Quokka": \\textbf{Quokka} (was: \\textbfQuokka, its robust definition takes no argument itself)' },
  { name: 'tc-type-textbf-run', example: 'thesis.tex', word: is('Literate'), src: /Literate/, keys: [' ', 'Pause', '\\textb', 'Tab', 'Quokka'], pauseMs: 900, speed: 'burst', settle: true, expect: () => null, contains: [/Literate \\textbf\{Quokka\}/],
    steps: 'thesis.tex, Type mode; click the end of "Literate"; a space starts a run; during it "\\textb", Tab, "Quokka": \\textbf{Quokka} from the kept list' },
  { name: 'tc-type-sum-math', source: KINDS, example: 'hello.tex', word: (`(t) => t === 'n' || t === 'n,'`), src: /of \$n/, keys: [' - \\sum', 'Tab', ' k'], speed: 'burst', settle: true, expect: () => null, contains: [/\$n - \\sum k ?\$/],
    steps: 'KINDS, Type mode; click the math "n" in "A run of $n$ lines"; type " - \\sum", Tab, " k": $n - \\sum k$ (was: Tab took \\NeedsDocumentMetadata)' },
];
