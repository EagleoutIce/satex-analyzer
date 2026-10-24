## number, count register, count, counter
An integer, as TeX reads it: decimal `42`, octal `'17`, hex `"2A`, a character `` `\a``, or a `\count` register.
Signs may repeat: `--3` is 3. A space after the digits ends the number.

```tex
\count255=-12
\setcounter{section}{3}
\ifnum\count1>5 ...\fi
```

## dimen, dimen register
A length: a number with a unit (`pt`, `bp`, `mm`, `cm`, `in`, `em`, `ex`, `sp`), or a `\dimen` register. `true` before the unit undoes magnification.
The largest value is about 16383pt.

```tex
\dimen0=2.5pt
\setlength{\parindent}{1em}
\hsize=0.5\textwidth
```

## glue, skip, skip register
A length that may stretch and shrink: `⟨dimen⟩ plus ⟨dimen⟩ minus ⟨dimen⟩`. The stretch and shrink may use the infinite orders `fil`, `fill`, `filll`.
A dimen is glue without stretch and shrink. Glue sets the space between boxes, lines and paragraphs.

```tex
\skip0=3pt plus 1pt minus 1pt
\setlength{\parskip}{0pt plus 1fil}
\hskip 1em plus 1fill
```

## muglue, muskip, muskip register, mudimen
Glue in math units (`mu`, 1/18 em of the math font), for the space between math atoms. `\mskip` and `\muskip` registers take it.

```tex
\muskip0=3mu plus 2mu minus 1mu
\mskip 18mu
```

## quantity
A number, a dimen, glue or muglue: the arguments of `\advance`, `\multiply` and `\divide` read one of them, by the register they change.

## box, box register
A box register holds typeset material, numbered 0 to 32767 (e-TeX), or named by `\newsavebox` in LaTeX.
`\setbox` fills it, `\box` uses and empties it, `\copy` uses it and keeps it, `\wd`, `\ht`, `\dp` measure it.

```tex
\setbox0=\hbox{text}
\wd0
\copy0
```

## register
A named storage place: `\count`, `\dimen`, `\skip`, `\muskip`, `\toks` or `\box`. A register of one kind holds that kind of value.
`\newcount` and its relatives give a name to the next free number.

## toks, toks register
A register holding a token list, set with braces: `\toks0={text}`. `\the\toks0` gives the tokens back.

## control sequence
A command name: `\name` of letters, or `\` and one other character. It is an argument of `\def`, `\let`, `\newcommand`, `\newif` and others that define it.

```tex
\newif\ifdraft
\def\greet#1{Hello, #1}
```

## conditional
`\newif\iffoo` defines `\iffoo`, `\footrue` and `\foofalse`. `\iffoo ...\else ...\fi` tests it.

## key
A name a label, a citation or a key list is looked up by. It is case sensitive.

## file, input file
A file name without a trailing space. TeX adds the extension (`.tex`, `.sty`, `.cls`, `.bib`) where the command expects one, and searches the project and the TeX tree.

## options
The key list in the optional argument of `\documentclass`, `\usepackage` or `\RequirePackage`: `[a4paper, 11pt]` or `[style=numeric, backend=biber]`.
A key may take a value after `=`. The class passes the options on to the packages that declare them.

## name
Characters that form a name, as `\csname` forms a command name or LaTeX forms a counter name.

## bst plain
BibTeX style: entries sorted by author, then year and title. Labels are numbers. Works with `\cite` and `\bibliographystyle{plain}`.

## bst unsrt
Like `plain`, but entries stay in the order they are first cited.

## bst alpha
Like `plain`, with labels from author and year, as in `[Knu86]`.

## bst abbrv
Like `plain` with first names, months and journals abbreviated.

## bst ieeetr
IEEE Transactions style: numeric labels in citation order.

## bst siam
SIAM style: numeric labels, entries sorted by author.

## bst acm
ACM style: numeric labels, entries sorted by author.

## biblatex style numeric
biblatex style: numeric labels such as `[1]`, in citation order. Pair it with `\printbibliography`.

## biblatex style alphabetic
biblatex style: labels from author and year, as in `[Knu86]`, entries sorted by label.

## biblatex style authoryear
biblatex style: author-year citations such as `(Knuth 1986)`, entries sorted by author and year.

## biblatex style authortitle
biblatex style: author-title citations, entries sorted by author and title.

## biblatex style verbose
biblatex style: the first citation of a work prints its full entry in a footnote.
