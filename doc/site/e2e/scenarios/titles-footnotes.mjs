import { MARKUP, is, big, after, at, TITLES } from './sources.mjs';

export default [
  // the user's 15:43 report: a section title typed into echoes twice, in a regular font
  { name: 'title-human', word: big('Background'), src: /\\section(\[[^\]]*\])?\{Background/, keys: [' Is an important issue!'], marker: 'important', style: /bold|700|Bold|bx/, endWord: is('issue!'), forward: true,
    steps: 'thesis.tex, PDF, Compile, Type on; click the end of the heading word "Background"; type " Is an important issue!" at 20-60 ms a key' },
  { name: 'title-burst', word: big('Background'), src: /\\section(\[[^\]]*\])?\{Background/, keys: [' Quokka wombat'], speed: 'burst', marker: 'Quokka', style: /bold|700|Bold|bx/, endWord: is('wombat'), forward: true,
    steps: 'as title-human, the keys " Quokka wombat" sent with no delay' },
  { name: 'title-backspace', word: big('Background'), src: /\\section(\[[^\]]*\])?\{Background/, keys: ['xyz', 'Backspace', 'Backspace', 'Backspace', 'Backspace', 'd'],
    steps: 'click the end of the heading "Background"; type xyz, Backspace x4, d (expects "Backgrounddd"→"Backgrounc"+"d": the source stays a valid title)' },
  { name: 'title-enter', word: big('Background'), src: /\\section(\[[^\]]*\])?\{Background/, keys: ['Enter', 'Zed'], expect: () => null,
    steps: 'click the end of the heading "Background"; press Enter, type Zed: the title must still compile (a blank line in \\section{} is a runaway argument)' },
  { name: 'title-markup', word: big('Background'), src: /\\section(\[[^\]]*\])?\{Background/, keys: [' a\\S $x_1', 'Escape'], expect: () => null,
    // `\S` (§) takes no argument: `\b` is the bar-under accent, which takes the `\(` after it and breaks the TeX by right
    steps: 'click the end of the heading "Background"; type " a\\S $x_1" then Escape: the source must compile after (user report 15:31)' },
  { name: 'footnote', source: MARKUP, example: 'hello.tex', word: is('footnote'), src: /A footnote/, keys: [' Quokka'], marker: 'Quokka', endWord: is('Quokka'), forward: true,
    steps: 'a doc with a \\footnote; click the end of "footnote" in the footnote; type " Quokka"' },
  { name: 'heading-plain', source: MARKUP, example: 'hello.tex', word: big('Heading'), src: /Plain Heading/, keys: [' Quokka'], marker: 'Quokka', style: /bold|700|Bold|bx/, endWord: is('Quokka'), forward: true,
    steps: 'a doc with \\section{Plain Heading}; click the end of "Heading"; type " Quokka"' },
  { name: 'fz-title-author-start', source: TITLES.article, example: 'hello.tex', word: is('Author'), at: 'start', src: (t) => t.indexOf('Author\\thanks'), keys: ['Quokka '], marker: 'Quokka', expect: (t, a) => t.slice(0, a) + 'Quokka ' + t.slice(a),
    steps: 'article with \\author{Some Author\\thanks{..}}; click the start of "Author"; type "Quokka ": all keys land in \\author (was: the rest lost after 3 keys)' },
  { name: 'fz-thanks-mark', source: TITLES.article, example: 'hello.tex', word: is('Author'), src: /Some Author/, keys: [' Quokka'], marker: 'Quokka', endWord: is('Quokka'),
    steps: 'article; click the end of "Author" (∗ of \\thanks after it); type " Quokka": the echo moves the raised ∗ along, never paints over half of it (fuzz class overlap)' },
  // amsart sets its title in capitals (`\MakeUppercase`): the page's words, the typed one too, are TITLE and QUOKKA
  ...['report', 'scrartcl', 'amsart', 'beamer', 'custom', 'breaks', 'titlepage'].map((c) => ({ name: `fz-title-${c}`, source: TITLES[c], example: 'hello.tex', word: is(c === 'amsart' ? 'TITLE' : 'Title'), src: c === 'titlepage' ? /Plain Title/ : /\\title\{Plain Title/, keys: [' Quokka'], marker: c === 'amsart' ? 'QUOKKA' : 'Quokka', endWord: is(c === 'amsart' ? 'QUOKKA' : 'Quokka'),
    steps: `titles/${c}.tex; click the end of the title word "Title"; type " Quokka": it lands in the title's argument, never after \\maketitle or in the class` })),
];
