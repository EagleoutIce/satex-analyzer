// Findings and Typeset rows after keys in the editor converge on the editor's text (kinds/sync.mjs). The user report
// of 11:47 (2026-11-19): `\usetikzlibrary{arrows.meta}` typed, `\use` reported undefined and `tikzlibraryarrows.meta`
// set as text; of 11:48: a stray `}` typed after `\end{tikzpicture}`, no "Too many }'s" shown.
const TIKZ = '\\documentclass{article}\n\\usepackage{tikz}\n\n\\begin{document}\nSome text.\n\\begin{tikzpicture}\n  \\draw[->] (0,0) -- (1,0);\n\\end{tikzpicture}\n\\end{document}\n';
export default [
  { kind: 'sync', name: 'sync-usetikzlibrary', text: TIKZ, at: 3, keys: '\\usetikzlibrary{arrows.meta}', pauseAfter: 4, never: [/never defined|tikzlibrary|ryarrows/],
    steps: 'line 3 of a tikz document; type `\\use`, wait for its run, type `tikzlibrary{arrows.meta}`: after quiet no `\\use` finding and no `tikzlibraryarrows.meta` row (native `satex lint` of the final text reports neither)' },
  { kind: 'sync', name: 'sync-stray-brace', text: TIKZ, at: 8, keys: '}', want: [/Too many \}'s/],
    steps: 'type a `}` after `\\end{tikzpicture}`: after quiet the findings say "Too many }\'s" (native `satex lint` reports C013 there)' },
];
