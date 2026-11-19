import { readFileSync } from 'node:fs';
// Documents and word tests the scenarios share, and the scenario fields of the typing kind. A typing one: `word` (a page-word test `(text, w, words) => bool`, `nth`), `src` (where the caret
// must land in the source: the end of the regex match, or a function of the text and the clicked word), `keys`
// (strings split into characters, named keys like 'Backspace' kept), `speed` ('human' 20-60 ms or 'burst'),
// `marker` (a word typed once: seen twice = a duplicated echo), `style` (the echo font must match), `endWord`
// (the caret ends at this word's end on the final build), `contains` (the final source must contain these).
// A tint one: `want` [[word, kind | '!kind' | null]], `whole` (words that must be tinted in one piece), `toggle`.

export const filler = Array.from({ length: 46 }, (_, i) => `Filler line ${i} says w${i}a w${i}b w${i}c w${i}d w${i}e w${i}f.${i % 9 === 8 ? '\n' : ''}`).join('\n');
export const PAGEBREAK = `\\documentclass{article}\n\\begin{document}\n${filler}\n\\end{document}\n`;
export const MARKUP = `\\documentclass{article}\n\\begin{document}\n\\section{Plain Heading}\nPlain words with \\textbf{bold words} and \\emph{slanted words} here.\\footnote{A footnote text goes here.}\n\nMore plain text follows in a second paragraph.\n\\end{document}\n`;
export const BANG = `\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\n\\greet{loool Das ist doch witzig oder?}\n$x_{42}+ 2 - 41$Generally I'd say this is nice wouldn't you!\nFoo bar baz ban chicken asmana \\greet{Walter}\n\\end{document}\n`;

// the forest example as a hello.tex text: the examples list does not offer it
export const FM = readFileSync(new URL('../../examples/feature-model.tex', import.meta.url), 'utf8');

export const is = (s) => `(t) => t === ${JSON.stringify(s)}`;
export const big = (s) => `(t, w) => t === ${JSON.stringify(s)} && w[4] - w[2] > 12`;
export const lastOn1 = `(t, w, ws) => w === ws.filter((x) => x[0] === 1 && /^w\\d+[a-f]\\.?$/.test(x[5])).at(-1)`;
export const after = (s) => (text, w) => { const i = text.indexOf(s ?? w.replace(/[.,]$/, '')); return i < 0 ? -1 : i + (s ?? w.replace(/[.,]$/, '')).length; };

// an undecided test: the run reads \\time as the time it ran (decided), a random number stays unknown
export const MAYBE = `\\documentclass{article}\n\\begin{document}\nToday \\ifnum\\pdfuniformdeviate 1440<720 good morning\\else good afternoon\\fi{} to you.\n\\end{document}\n`;
export const KINDS = `\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\section{Plain Heading}\nText above the float ends here.\n\\begin{figure}[h]\n\\centering\n\\tikz \\node[draw] {Nodetext};\n\\caption{Captioned words here}\n\\end{figure}\nWords with a note.\\footnote{Footnoted words here.}\n\\begin{itemize}\n\\item First item words\n\\item Second item words\n\\end{itemize}\n\\begin{tabular}{ll}\ncellword & other \\\\\n\\end{tabular}\n\nA run of $n$ lines.\n\\end{document}\n`;
// the start of the match's group (a group before the match's end too: `\$(n)\$`)
export const at = (re) => (t) => { const m = re.exec(t); return m ? m.index + m[0].lastIndexOf(m[1]) : -1; };
// the title documents (fixtures/titles): \\maketitle of each class, a custom one, a titlepage, line breaks and logos
export const TITLES = Object.fromEntries(['article', 'report', 'book', 'scrartcl', 'scrreprt', 'amsart', 'beamer', 'custom', 'breaks', 'titlepage'].map((c) => [c, readFileSync(new URL(`../fixtures/titles/${c}.tex`, import.meta.url), 'utf8')]));
// the landing demo (shots.json typing, spacing.tex): math typed after "fast.", Enter, a new paragraph (ui-35 garble)
export const DEMO = "\\documentclass{article}\n\\usepackage[paperwidth=9cm,paperheight=6cm,margin=6mm]{geometry}\n\\usepackage{tikz}\n\\usetikzlibrary{arrows.meta}\n\\definecolor{accent}{HTML}{0B3D91}\n\\tikzset{>=Stealth, thick,\n  box/.style={draw=accent, fill=accent!8,\n  rounded corners=3pt, minimum width=10mm}}\n\\newcommand{\\model}{TinyNet}\n\\begin{document}\n\\section{Results}\nWith \\model, the loss falls fast.\n\n\\begin{tikzpicture}[draw=accent]\n  \\node[box] (d) at (0,0) {data};\n  \\node[box] (m) at (2.6,-0.6) {\\model};\n  \\node[box] (l) at (5.2,0) {loss};\n  \\draw[->] (d) -- node[above] {train} (m);\n  \\draw[->] (m) -- (l);\n\\end{tikzpicture}\n\\end{document}\n";
