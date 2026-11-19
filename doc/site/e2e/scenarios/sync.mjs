// Findings and Typeset rows after keys in the editor converge on the editor's text (kinds/sync.mjs). The user report
// of 11:47 (2026-11-19): `\usetikzlibrary{arrows.meta}` typed, `\use` reported undefined and `tikzlibraryarrows.meta`
// set as text; of 11:48: a stray `}` typed after `\end{tikzpicture}`, no "Too many }'s" shown.
const TIKZ = '\\documentclass{article}\n\\usepackage{tikz}\n\n\\begin{document}\nSome text.\n\\begin{tikzpicture}\n  \\draw[->] (0,0) -- (1,0);\n\\end{tikzpicture}\n\\end{document}\n';
export default [
  // A run cut while it builds the kernel: the user's explorer report (2026-11-19) of an example clicked during the
  // first kernel build, ending "Ready" with `\documentclass`, `\newcommand`, `\begin` never defined; ui-61's older text.
  { kind: 'kernelCut', name: 'kernel-cut-typed', during: 'type', at: 3, keys: ' x', never: [/never defined/],
    steps: 'caches dropped, so the next run builds the kernel; while it does, ` x` typed at the end of line 3 of hello.tex: after quiet the findings are of the final text and its run had the kernel (no `\\documentclass` never defined)' },
  { kind: 'kernelCut', name: 'kernel-cut-example', during: 'example', example: 'macros.tex', never: [/\\documentclass\S* is never defined/],
    steps: 'caches dropped; while the next run builds the kernel, the macros.tex example clicked: after quiet its findings are its own run with the kernel' },
  { kind: 'sync', name: 'sync-usetikzlibrary', text: TIKZ, at: 3, keys: '\\usetikzlibrary{arrows.meta}', pauseAfter: 4, never: [/never defined|tikzlibrary|ryarrows/],
    steps: 'line 3 of a tikz document; type `\\use`, wait for its run, type `tikzlibrary{arrows.meta}`: after quiet no `\\use` finding and no `tikzlibraryarrows.meta` row (native `satex lint` of the final text reports neither)' },
  { kind: 'sync', name: 'sync-stray-brace', text: TIKZ, at: 8, keys: '}', want: [/Too many \}'s/],
    steps: 'type a `}` after `\\end{tikzpicture}`: after quiet the findings say "Too many }\'s" (native `satex lint` reports C013 there)' },
  { kind: 'soak', name: 'soak-200-edits', text: '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\n\\greet{world}\n\nWhat do you think about it.\n\\end{document}\n', line: 6, n: 200, every: 20, ratio: 1.5,
    steps: '200 edits in the editor (a letter typed and taken back), a run waited for every 20th: the next edit reaches its findings within 1.5x (+200 ms) of the same edit in a fresh session (the user: "a reload makes it fast again")' },
];
