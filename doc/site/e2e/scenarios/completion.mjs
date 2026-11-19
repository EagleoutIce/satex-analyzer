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
  { kind: 'complete', name: 'tc-src-env-clean', keys: ['\\begin{itemize', '}', 'x', 'ArrowDown', 'ArrowDown', 'ArrowDown'], want: /\\begin\{itemize\}x\n\\end\{itemize\}/, clean: true, idle: true,
    steps: 'thesis.tex; a run going on; type "\\begin{itemize", "}" (its \\end follows), "x", then leave by ArrowDown: no snippet field or linked \\begin/\\end box stays drawn (was: boxes around {tikzpicture}, {document} and whole lines stayed, user 2026-11-19)' },
  { kind: 'complete', name: 'tc-src-end-brace', keys: ['\\end{tabu', 'Tab', '}'], want: /\n\\end\{tabular\}\n\\end\{document\}/,
    steps: 'thesis.tex; a run going on; type "\\end{tabu", Tab, "}": one } after tabular (was: \\end{tabular}}, user 2026-11-19)' },
  { kind: 'complete', name: 'tc-src-begin-brace', keys: ['\\begin{center', '}'], want: /\n\\begin\{center\}\n\\end\{center\}\n/,
    steps: 'thesis.tex; a run going on; type "\\begin{center", "}" (the editor closed the brace): \\begin{center} and its \\end, one } each (was: \\begin{center}} or \\end{center}})' },
  { name: 'tc-type-begin-arg', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' \\beg', 'Tab', 'cent', 'Tab', 'Quokka'], speed: 'human', settle: true, expect: () => null, contains: [/\\begin\{center\}\s*Quokka\s*\\end\{center\}/],
    steps: 'MARKUP, Type mode; click the end of "second"; type " \\beg", Tab (\\begin{|}: the environments open at once), "cent", Tab, "Quokka": a center environment around Quokka (was: no list in the braces, user 2026-11-19)' },
  { name: 'tc-type-textbf', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' \\textb', 'Tab', 'Quokka'], speed: 'burst', settle: true, expect: () => null, contains: [/a second \\textbf\{Quokka\}/],
    steps: 'MARKUP, Type mode; click the end of "second"; type " \\textb", Tab, "Quokka": \\textbf{Quokka} (was: \\textbfQuokka, its robust definition takes no argument itself)' },
  { name: 'tc-type-textbf-run', example: 'thesis.tex', word: is('Literate'), src: /Literate/, keys: [' ', 'Pause', '\\textb', 'Tab', 'Quokka'], pauseMs: 900, speed: 'burst', settle: true, expect: () => null, contains: [/Literate \\textbf\{Quokka\}/],
    steps: 'thesis.tex, Type mode; click the end of "Literate"; a space starts a run; during it "\\textb", Tab, "Quokka": \\textbf{Quokka} from the kept list' },
  { name: 'tc-type-sum-math', source: KINDS, example: 'hello.tex', word: (`(t) => t === 'n' || t === 'n,'`), src: /of \$n/, keys: [' - \\sum', 'Tab', ' k'], speed: 'burst', settle: true, expect: () => null, contains: [/\$n - \\sum k ?\$/],
    steps: 'KINDS, Type mode; click the math "n" in "A run of $n$ lines"; type " - \\sum", Tab, " k": $n - \\sum k$ (was: Tab took \\NeedsDocumentMetadata)' },
  // user 2026-11-19: `\\` at a row's end opened the list with `\greet` first and Enter wrote `\greet`; a `\` and one
  // non-letter are a whole name (tex.web §354), so Enter there is a new line
  { kind: 'complete', name: 'tc-src-forced-break', keys: ['a & b \\\\', 'Enter', 'c'], want: /\na & b \\\\\n\s*c\n/, keyMs: 400, idle: true,
    steps: 'thesis.tex; a run going on; type "a & b \\\\\\\\", Enter, "c": a new line after \\\\\\\\ (was: Enter took the list\'s first item, \\\\\\\\greet)' },
  // Type mode, user 2026-11-19: `align` taken from the list brings the amsmath it needs, as in the editor
  { name: 'tc-type-align-loads', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' \\beg', 'Tab', 'alig', 'Tab', 'x'], speed: 'human', settle: true, expect: () => null, contains: [/\\usepackage\{amsmath\}/, /\\begin\{align\}\s*x\s*\\end\{align\}/],
    steps: 'MARKUP, Type mode; click the end of "second"; " \\beg", Tab, "alig", Tab, "x": an align around x and \\usepackage{amsmath} (was: no load, "Environment align undefined")' },
  // the load written above the caret moved the text: the caret moves with it (was: "The text changed: click the word again", x lost)
  { name: 'tc-type-load-keeps-caret', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' \\beg', 'Tab', 'alig', 'Tab', 'x'], speed: 'human', keyMs: [100, 140], settle: true, expect: () => null, contains: [/\\begin\{align\}\s*x\s*\\end\{align\}/],
    steps: 'MARKUP, Type mode at 100-140 ms a key; " \\beg", Tab, "alig", Tab, "x": x in the align after its package load is written (was: the caret dropped, x lost)' },
  // Type mode: Enter right after a typed `\begin` takes it as Tab would: its braces and the environments' list
  { name: 'tc-type-begin-enter', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' \\begin', 'Enter', 'cent', 'Tab', 'Quokka'], speed: 'human', settle: true, expect: () => null, contains: [/\\begin\{center\}\s*Quokka\s*\\end\{center\}/],
    steps: 'MARKUP, Type mode; click the end of "second"; " \\begin", Enter, "cent", Tab, "Quokka": a center environment (was: Enter ended the paragraph after \\begin)' },
  // Type mode: Ctrl+Enter is one forced line break, never a build
  { name: 'tc-type-ctrl-enter', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: ['Ctrl+Enter', 'Quokka'], speed: 'human', settle: true, expect: () => null, contains: [/a second\\\\\nQuokka/],
    steps: 'MARKUP, Type mode; click the end of "second"; Ctrl+Enter, "Quokka": `\\\\\\\\` and a new line before Quokka (was: Ctrl+Enter built the PDF, wrote nothing)' },
  // Type mode: Enter right after a whole name that writes nothing more (`\noindent`) is Enter's own; a prefix takes the item
  { name: 'tc-type-enter-whole', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' \\noindent', 'Enter', 'Quokka'], speed: 'human', settle: true, expect: () => null, contains: [/a second \\noindent\n+Quokka/],
    steps: 'MARKUP, Type mode; click the end of "second"; " \\noindent", Enter, "Quokka": a new paragraph after \\noindent (was: Enter took the item, no line)' },
];
