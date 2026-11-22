import { filler, PAGEBREAK, MARKUP, is, lastOn1, after, at, DEMO } from './sources.mjs';

export default [
  { name: 'demo-macro-row', source: DEMO.replace('falls fast.', 'falls fast. It drops by \\(x_{42}\\) per step.'), example: 'hello.tex', word: is('TinyNet,'), src: /With \\model,/, keys: ['2'], expect: () => null, contains: [/With \\model,2 the/, /\{TinyNet\}/], math: [],  clickMs: 60000,
    steps: 'the landing demo after its typing; click the end of "TinyNet,", type 2 (after the comma, outside \\model: caret2, the end of an output is the call\'s): the row\'s other words, x₄₂ among them, are the page\'s own pixels moved, never x42 in the text font' },
  { name: 'demo-garble', source: DEMO, example: 'hello.tex', word: is('fast.'), src: /falls fast\./, keys: [' It drops by $x_42$ per', 'Pause', ' step.', 'Pause', 'Enter', 'Pause', 'It converges.'], keyMs: [70, 130], pauseMs: 600, never: [/\(x42\)|fast\. It|^fast\. /], expect: () => null, contains: [/x_\{42\}/], ever: [/ x 42 /],
    steps: 'the landing demo; click the end of "fast."; type " It drops by $x_42$ per step.", Enter, "It converges.": the echo keeps the sentence in order, x with 42 lowered (ui-35: a garbled first row)' },
  { name: 'prose-human', word: is('Literate'), src: /Literate/, keys: [' Quokka zebu'], marker: 'Quokka', style: /^(?!.*(bold|700|italic)).*$/i, endWord: is('zebu'), forward: true,
    steps: 'thesis.tex, PDF, Type on; click the end of "Literate"; type " Quokka zebu" at 20-60 ms a key' },
  { name: 'prose-burst', word: is('Literate'), src: /Literate/, keys: [' Quokka zebu wombat'], speed: 'burst', marker: 'Quokka', endWord: is('wombat'), forward: true,
    steps: 'click the end of "Literate"; send " Quokka zebu wombat" with no delay' },
  { kind: 'click', name: 'click-repeated', source: `\\documentclass{article}\n\\newcommand{\\yr}[1]{The year #1 is a year.\\par}\n\\begin{document}\n\\yr{2000}\n\\yr{2004}\n\\yr{2000}\n\\end{document}\n`, example: 'hello.tex', word: `(t) => t === '2004'`, src: (t) => t.indexOf('2004'),
    steps: 'three rows set by one macro line, 2000 / 2004 / 2000 (the leapyear report 16:53): a click on 2004 puts the caret in 2004, drawn on its own row' },
  // the visual helper's strip (ui-26/tyP-strip.png): a click just past "Inside", before `\\texttt{lstlisting}`, typed into the
  // \\texttt's argument; the caret at a word's end before a command inserts before the command
  { name: 'gap-before-command', word: is('Inside'), at: 'gap', src: /Inside/, keys: [' quickly'], marker: 'quickly', endWord: is('quickly'), forward: true,
    steps: 'thesis.tex; click in the space after "Inside" (nearer `lstlisting`); type " quickly": the source says `Inside quickly \\texttt{lstlisting}`' },
  // the live preview re-flows the paragraph (user 2026-11-18 evening): a burst over three rows, Enter twice, math
  { name: 'burst-3rows', word: is('Literate'), src: /Literate/, keys: [' quokka zebu wombat emu koala dingo numbat bilby possum echidna platypus wallaby kangaroo bandicoot cassowary dugong galah kookaburra lyrebird quoll wombat taipan yabby bunyip'], speed: 'burst', marker: 'cassowary', rows: 3, endWord: is('bunyip'),
    steps: 'thesis.tex; click the end of "Literate"; burst 26 words: the echo re-flows them over three rows, no overflow, the rest pushed down; the build lands with no jump' },
  { name: 'type-through-builds', word: is('Literate'), src: /Literate/, keys: [' quokka zebu wombat emu', 'Pause', ' koala dingo numbat bilby possum echidna platypus', 'Pause', ' wallaby kangaroo bandicoot cassowary dugong galah', 'Pause', ' kookaburra lyrebird quoll wombat taipan yabby bunyip'], marker: 'cassowary', minBuilds: 2, endWord: is('bunyip'),
    steps: 'thesis.tex; click the end of "Literate"; type 26 words at a person\'s pace (builds land meanwhile): the typed paragraph never jumps or flickers, a build that lacks a key never swaps in under the echo' },
  // the user's 22:45 report: typing in a page's last paragraph wraps on at the next page's top, the rest pushed down
  { name: 'page-foot', source: `\\documentclass{article}\n\\begin{document}\n\\vspace*{19.6cm}\nThe last paragraph of the page ends here.\n\nNext page text stays below the rows that come over.\n\\end{document}\n`, example: 'hello.tex',
    word: is('here.'), src: /ends here\./, keys: [' quokka zebu wombat emu koala dingo numbat bilby possum echidna platypus wallaby kangaroo bandicoot cassowary dugong galah kookaburra lyrebird quoll wombat taipan yabby bunyip'], speed: 'burst', marker: 'cassowary', endWord: is('bunyip'), foot: { from: 1, next: true },
    steps: 'a page whose last paragraph ends at its foot; type three rows there: no echo below the text area, the rows go on at the next page\'s top, the next page\'s text pushed down; no jump when the build lands' },
  // satex/break's own ground (ui-39): paragraphs where the echo's fit may miss pdfTeX: a narrow measure that hyphenates
  // long words at the edge, one tight enough for TeX's second pass and \emergencystretch, ligature-heavy words
  { name: 'tex-hyphen-edge', source: `\\documentclass{article}\n\\setlength\\textwidth{200pt}\n\\begin{document}\nTypesetting a narrow column is where hyphenation decides the rows, so the end of this sentence is here.\n\nAfter it another paragraph follows.\n\\end{document}\n`, example: 'hello.tex',
    word: is('here.'), src: /is here\./, keys: [' Characteristically, internationalization incomprehensibility uncharacteristically counterrevolutionaries misunderstandings photosynthesis'], speed: 'burst', marker: 'internationalization', endWord: is('photosynthesis'),
    steps: 'a 200pt column; burst long words that pdfTeX hyphenates at the edge: the echo rows land where the build sets them' },
  { name: 'tex-second-pass', source: `\\documentclass{article}\n\\setlength\\textwidth{150pt}\n\\emergencystretch=1.5em\n\\begin{document}\nA tight measure needs the second pass of the breaker and its emergency stretch, as here.\n\nThe next paragraph stays.\n\\end{document}\n`, example: 'hello.tex',
    word: is('here.'), src: /as here\./, keys: [' Unquestionably, extraordinary circumstances necessitated the reconsideration of administrative responsibilities everywhere'], speed: 'burst', marker: 'circumstances', endWord: is('everywhere'),
    steps: 'a 150pt column with \\emergencystretch; burst words no first-pass break fits: the echo takes TeX\'s second or third pass' },
  { name: 'tex-ligatures', source: `\\documentclass{article}\n\\setlength\\textwidth{220pt}\n\\begin{document}\nThe staff office found the first waffles affordable, and the coffee was fine here.\n\nAnother paragraph follows.\n\\end{document}\n`, example: 'hello.tex',
    word: is('here.'), src: /fine here\./, keys: [' affluent officials fluffily shuffled difficult offices, affirming efficient effortless sufficiency of baffling snuffles'], speed: 'burst', marker: 'officials', endWord: is('snuffles'),
    steps: 'ligatures ff, fi, fl, ffi, ffl in every word: widths with the font\'s ligatures and kerns, as pdfTeX sets them' },
  { name: 'enter-twice', word: is('Literate'), src: /Literate/, keys: ['Enter', 'Enter', 'Wombat'], marker: 'Wombat', endWord: is('Wombat'), expect: () => null,
    steps: 'thesis.tex; click the end of "Literate"; Enter twice, type Wombat: one new paragraph (a second blank line adds nothing), Wombat at its start' },
  // the coordinator's report 2026-11-19: typing into the first word after \pagecolor{blue!15} echoed "blue!15" as typed
  { name: 'pagecolor-first', source: `\\documentclass{article}\n\\usepackage{xcolor}\n\\begin{document}\n\\pagecolor{blue!15}\nFirst words of the text stand here.\n\\end{document}\n`, example: 'hello.tex',
    word: is('First'), src: /First/, keys: ['ly'], endWord: is('Firstly'), noEcho: /blue/,
    steps: 'a page coloured by \\pagecolor{blue!15}; click the end of "First"; type ly: the echo shows Firstly, never blue!15' },
  { name: 'pagecolor-inline', source: `\\documentclass{article}\n\\usepackage{xcolor}\n\\begin{document}\n\\pagecolor{blue!15}First words of the text stand here.\n\\end{document}\n`, example: 'hello.tex',
    word: is('words'), src: /First words/, keys: ['ly'], endWord: is('wordsly'), noEcho: /blue/,
    steps: 'the text on \\pagecolor\'s line; type into the second word: never blue!15 echoed' },
  { name: 'pagecolor-preamble-start', source: `\\documentclass{article}\n\\usepackage{xcolor}\n\\pagecolor{blue!15}\n\\begin{document}\nFirst words of the text stand here.\n\\end{document}\n`, example: 'hello.tex',
    word: is('First'), at: 'start', src: (t) => t.indexOf('First'), keys: ['The '], noEcho: /blue/, expect: () => null, contains: [/The First words/],
    steps: 'as pagecolor-preamble, typing at the start of "First"' },
  { name: 'prose-backspace', word: is('programming'), src: /programming/, keys: ['xyz', 'Backspace', 'Backspace', 'Backspace', '!'], endWord: is('programming!'),
    steps: 'click the end of "programming"; type xyz, Backspace x3, "!"' },
  { name: 'prose-dollar', word: is('Literate'), src: /Literate/, keys: [' cost $x_1', 'Escape'], expect: () => null, contains: [/x_\{?1\}?/],
    steps: 'click the end of "Literate"; type " cost $x_1" then Escape: math starts (`\\(x_{1}\\)` by playground.math_inline, or `$x_1$`), the source compiles' },
  { name: 'caption', word: is('From'), src: /\\caption(\[[^\]]*\])?\{From/, keys: ['Quokka'], marker: 'Quokka', endWord: is('FromQuokka'), forward: true,
    steps: 'click the end of the caption word "From" (Figure 1); type Quokka' },
  { name: 'table-cell', word: is('article'), src: /\barticle(?= &)/, keys: ['s'], endWord: is('articles'),
    steps: 'click the end of the table cell "article"; type s' },
  { name: 'listing', word: is('\\twice{ab}'), src: /\\twice\{ab\}(?=\n\\end\{lstlisting)/, keys: ['Z'], endWord: is('\\twice{ab}Z'),
    steps: 'click the end of the listing line "\\twice{ab}"; type Z' },
  { name: 'tikz-node', word: is('findings'), src: /\{findings(?=\})/, keys: ['X'], endWord: is('findingsX'),
    steps: 'click the end of the tikz node text "findings" (Figure 1); type X' },
  { name: 'toc-entry', word: { word: 'Background', maxH: 11 }, src: /\\section\[Background/, keys: ['Q'], expect: () => null,
    steps: 'click the end of "Background" in the table of contents; type Q: goes to the section title or is refused, never breaks the build' },
  { name: 'bold', source: MARKUP, example: 'hello.tex', word: is('bold'), src: /\{bold/, keys: [' Quokka'], marker: 'Quokka', style: /bold|700|Bold|bx/, endWord: is('Quokka'), forward: true,
    steps: 'a doc with \\textbf{bold words}; click the end of "bold"; type " Quokka"' },
  { name: 'emph', source: MARKUP, example: 'hello.tex', word: is('slanted'), src: /\{slanted/, keys: [' Quokka'], marker: 'Quokka', style: /italic|oblique|Italic|ti/, endWord: is('Quokka'), forward: true,
    steps: 'a doc with \\emph{slanted words}; click the end of "slanted"; type " Quokka"' },
  { name: 'pagebreak', source: PAGEBREAK, example: 'hello.tex', word: lastOn1, src: after(), keys: [' Quokka zebu wombat emu koala dingo'], marker: 'Quokka', endWord: is('dingo.'), endBack: 2.78,
    steps: 'a 46-line filler doc over two pages; click the end of the last word on page 1; type " Quokka zebu wombat emu koala dingo" at 20-60 ms (the text crosses the page break)' },
  // the user's 18:04 report: text typed at a paragraph's end echoed in a bigger font on another baseline
  { name: 'para-end', source: MARKUP, example: 'hello.tex', word: is('paragraph.'), src: /second paragraph\./, keys: [' in this setup'], marker: 'setup', fit: 0.05, style: /^(?!.*(bold|700|italic)).*$/i, endWord: is('setup'), forward: true,
    steps: 'a small doc; click the end of the paragraph "...in a second paragraph."; type " in this setup": the echo has the page glyphs\' size and baseline (5%)' },
  { name: 'para-end-dooo', source: `\\documentclass{article}\n\\begin{document}\nWe would understand this much better what doooooooo\n\nNext paragraph here.\n\\end{document}\n`, example: 'hello.tex', word: is('doooooooo'), src: /doooooooo/, keys: [' in this setup'], marker: 'setup', fit: 0.05, endWord: is('setup'), forward: true,
    steps: 'the 18:04 screenshot: a paragraph ending "better what doooooooo"; click its end; type " in this setup": the echo in the page font and size' },
  // ui-29 typing fuzz (target/scratch/ui-29/fuzz.mjs), each failure class minimized
  { name: 'fz-ctrl-backspace', source: MARKUP, example: 'hello.tex', word: is('second'), src: /a second/, keys: [' oops', 'Ctrl+Backspace', 'Zebu'], marker: 'Zebu', endWord: is('Zebu'),
    steps: 'MARKUP; click the end of "second"; type " oops", Ctrl+Backspace, "Zebu": the word goes whole (was: one letter, "oopZebu")' },
  { name: 'fz-enter-mid', source: MARKUP, example: 'hello.tex', word: is('plain'), src: /More plain/, keys: ['Enter', 'qdaqum'], marker: 'qdaqum', endWord: is('qdaqum'),
    steps: 'MARKUP; click the end of "plain" in "More plain text"; Enter, then "qdaqum" at 30-120 ms: all six letters land (fuzz: only "qda" did, the caret gone)' },
  { name: 'fz-hyphen', source: `\\documentclass{article}\n\\usepackage[textwidth=5cm]{geometry}\n\\begin{document}\n${Array.from({ length: 14 }, (_, i) => `Sentence ${i} carries several plain words, like river${i} and stone${i}.`).join(' ')}\n\\end{document}\n`, example: 'hello.tex',
    word: { match: '^\\p{L}{2,}-$' }, src: (t, w) => { const m = new RegExp(w.slice(0, -1) + '\\p{L}+', 'u').exec(t); return m ? m.index + w.length - 1 : -1; }, keys: ['Q'], expect: () => null, contains: [/\p{L}Q\p{L}/u],
    steps: 'a paragraph TeX hyphenates; click the end of a hyphenated word\'s first part ("car-"); type Q: it lands at the break, inside the source word (was: refused, "comes from code satex cannot place")' },
  { kind: 'shift', name: 'compile-shift', source: MARKUP, example: 'hello.tex',
    steps: 'a small doc built once; Compile again: the pages never move while the status and timer change (user report 16:40)' },
  // references typed after their labels (preview wish 4, 2026-11-21): the echo shows the number the run set, never the
  // key's letters, a \label sets nothing
  { name: 'ref-from-label', source: `\\documentclass{article}\n\\begin{document}\n\\section{Intro}\\label{sec:a}\nThe intro ends here.\n\\section{Method}\\label{sec:b}\nSee \\ref{sec:a} above.\n\\end{document}\n`, example: 'hello.tex',
    word: is('here.'), src: /ends here\./, keys: [' As in \\ref{sec:b}\\label{z} done'], marker: 'done', ever: [/As in 2/], never: [/sec:b|sec|\bz\b/], expect: () => null, contains: [/\\ref\{sec:b\}\\label\{z\} done/],
    steps: 'two sections, each labelled; click the end of "here."; type " As in \\ref{sec:b}\\label{z} done": the echo reads "As in 2 done" before the build, never "sec:b" or "z"' },
  // a formula in text at a row's end (preview wish 3, 2026-11-21): TeX breaks it after a relation or binary operator,
  // the echo too, so the build lands where the echo set it (was: the whole formula on the next row)
  { name: 'math-break-inline', source: `\\documentclass{article}\n\\setlength\\textwidth{200pt}\n\\begin{document}\nA narrow column of text where a formula typed at the end must break. It ends here\n\nAfter it another paragraph follows.\n\\end{document}\n`, example: 'hello.tex',
    word: is('here'), src: /ends here/, keys: [' then $a+b+c+d+e+f+g+h=k+m+n$ holds.'], marker: 'holds', endWord: is('holds.'), expect: () => null, contains: [/a\+b\+c\+d\+e\+f\+g\+h=k\+m\+n\S* holds\./],
    steps: 'a 200pt column; click the end of "here"; type " then $a+b+…=k+m+n$ holds.": the echo breaks the formula after an operator as pdfTeX does, no jump when the build lands' },
  // Enter in the page is a new paragraph as TeX sets one (user report 2026-11-21 13:15): at the end of a bold word the
  // paragraph starts after `\\textbf{…}` (was: refused, the next keys went into the bold); on an item's line the next item
  { name: 'enter-after-bold', source: `\\documentclass{article}\n\\begin{document}\nHello there, this is a first paragraph of words.\n\nI think it is nice to consider a \\textbf{world}\n\\end{document}\n`, example: 'hello.tex',
    word: is('world'), src: (t) => t.indexOf('world}') + 5, keys: ['Enter', 'then words'], marker: 'then', expect: () => null, contains: [/\\textbf\{world\}\n\nthen words/],
    steps: 'click the end of the bold "world"; Enter, type "then words" (letters the page face holds: the fallback font width is another check): a new paragraph after the bold command, the words not bold' },
  { name: 'enter-in-list', source: `\\documentclass{article}\n\\begin{document}\n\\begin{itemize}\n\\item First thing here\n\\end{itemize}\n\\end{document}\n`, example: 'hello.tex',
    word: is('here'), src: /First thing here/, keys: ['Enter', 'Second'], marker: 'Second', expect: () => null, contains: [/\\item First thing here\n\\item Second/],
    steps: 'click the end of an item; Enter, type "Second": the next item' },
  // user screenshot 2026-11-22 (Screenshot_20261122_095719): Enter after the last word, `\\textb`, Tab: the caret and the
  // `\\textbf{…}` hint stood at the right end of the line above, not at the new paragraph's start (indented)
  { name: 'par-textbf', source: '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, #1!}\n\\begin{document}\n\\greet{world}\n\nThis is an example that i have thought about long and intense you know?\n\\end{document}\n', example: 'hello.tex',
    word: is('know?'), src: /you know\?/, keys: ['Enter', '\\textbf{', 'Pause', 'Bold'], pauseMs: 2500, speed: 'human', keyMs: [90, 140], settle: true, expect: () => null, contains: [/know\?\n\n\\textbf\{Bold\}/],
    steps: 'hello.tex with a sentence after the greeting; click after "know?", Enter, type \\textbf{ (its } follows), wait for the build of the empty \\textbf{}, type Bold: the caret stays where the new paragraph\'s first line starts (after its indent; was: the build of the empty paragraph put it under the end of the line above)' },
  // the typing showcase (target/scratch/main-vid, 2026-11-22): `\\sum` echoed as `⌣` or `▁` (a glyph of the page's
  // CMEX subset at another slot) and `x_42` upright as `x42` until the build lands
  { name: 'demo-sum', source: DEMO.replace('the loss falls fast.', '$\\sum_k y_k$ falls.'), example: 'hello.tex', word: is('falls.'), src: /\$ falls\./, keys: [' ', 'Pause', '$\\sum_i x_42$'], pauseMs: 3000, keyMs: [70, 130],
    expect: () => null, contains: [/\\sum_\{?i\}? ?x_\{?42\}?/], math: [['∑', 'i', 'sub'], ['x', '42', 'sub']], italic: ['x', 'i'], inked: ['∑'],
    steps: 'the landing demo with a sum already built (CMEX on the page); click the end of "falls."; type " $\\sum_i x_42$": the echo draws ∑ (not another CMEX slot) with i lowered, x italic with 42 lowered' },
];
