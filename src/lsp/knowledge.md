## number, count register, count
TeX accepts an integer in several spellings: `42` in decimal, `'52` in octal, `"2A` in hex, or `` `* `` for the code of a character. A `\count` register or a `\numexpr` works too.
Leading signs may repeat, so `--3` is 3. The number ends at the first character that is not a digit, and one space after it is eaten.

```tex
\count255=-12
\setcounter{section}{3}
\ifnum\count1>5 ...\fi
```

## dimen, dimen register
A length is a number with a unit: `pt`, `bp`, `mm`, `cm`, `in`, `em`, `ex` or `sp`. A `\dimen` register works too, and so does a factor before one (`0.5\textwidth`).
Lengths stop at about 16383pt. `true` before the unit ignores `\mag`.

```tex
\dimen0=2.5pt
\setlength{\parindent}{1em}
\hsize=0.5\textwidth
```

## glue, skip, skip register
Glue is a length that can stretch and shrink: `3pt plus 1pt minus 1pt`. The stretch may be infinite (`fil`, `fill`, `filll`), so `0pt plus 1fil` takes whatever room is left.
TeX puts glue between boxes, lines and paragraphs. A plain length is glue that neither stretches nor shrinks.

```tex
\skip0=3pt plus 1pt minus 1pt
\setlength{\parskip}{0pt plus 1fil}
\hskip 1em plus 1fill
```

## muglue, muskip, muskip register, mudimen
Math spacing is measured in `mu`, 1/18 of an em of the math font. Glue written in `mu` sets the space between math atoms; `\mskip` and `\muskip` registers take it.

```tex
\muskip0=3mu plus 2mu minus 1mu
\mskip 18mu
```

## quantity
Any of the four: a number, a length, glue or math glue. `\advance`, `\multiply` and `\divide` read whichever fits the register they change.

## box, box register
A box register keeps typeset material for later. Plain TeX numbers them (0 to 32767 with e-TeX); LaTeX names them with `\newsavebox`.
`\setbox` fills one, `\box` puts it down and empties it, `\copy` puts it down and keeps it. `\wd`, `\ht` and `\dp` give its size.

```tex
\setbox0=\hbox{text}
\wd0
\copy0
```

## register
TeX stores values in registers: `\count`, `\dimen`, `\skip`, `\muskip`, `\toks` and `\box`, each holding its own kind of value. `\newcount` and its relatives give the next free one a name.

## toks, toks register
A token register keeps a list of tokens: `\toks0={text}` sets it, `\the\toks0` gives the tokens back.

## read, read register
An input stream number, 0 to 15, that `\openin` opens and `\read` reads lines from.

## write, write register
An output stream number, 0 to 15, that `\openout` opens and `\write` writes lines to.

## counter
A LaTeX counter, by name: `\newcounter{step}` makes the count register `\c@step` and the printer `\thestep`. `\setcounter`, `\stepcounter` and `\value` take the name.

## dimen name, skip name, muskip name, toks name, box name, read name, write name
The characters of a register's name, put together by `\csname` (`\csname dimen@\endcsname`). Which register it is depends on the text.

## control sequence
A command name: a backslash and letters (`\name`), or a backslash and one other character (`\%`). `\def`, `\let`, `\newcommand` and `\newif` take one to define.

```tex
\newif\ifdraft
\def\greet#1{Hello, #1}
```

## conditional
`\newif\iffoo` makes three commands: `\iffoo` tests, `\footrue` and `\foofalse` set. Use it as `\iffoo ...\else ...\fi`.

## key
A name something is looked up by: a label, a citation or an option. Case matters.

## label
The name `\label` gives a place and `\ref` looks up. Case matters, and citation keys live elsewhere.

## citation key
The key of a `.bib` entry or a `\bibitem`, which `\cite` looks up. Case matters, and labels live elsewhere.

## option key
A key in a key list (`\pgfkeys`, `\tikzset`, a package's options). A value may follow after `=`.

## environment name
What `\begin` opens and `\end` closes: `figure`, `itemize`, or one the document defines.

## file, input file
A file name, which ends at the first space. TeX adds the usual extension (`.tex`, `.sty`, `.cls`, `.bib`) where the command expects one, and looks in the project, then in the TeX tree.

## TeX file (.tex)
A part of the document that `\input` or `\include` reads. The `.tex` may be left out.

## package (.sty)
A package `\usepackage` loads, from the project or the TeX tree.

## class (.cls)
The class `\documentclass` loads, from the project or the TeX tree.

## bibliography database (.bib)
One `.bib` file, or several separated by commas, that BibTeX or biber reads entries from. The `.bib` may be left out.

## bibliography style (.bst)
A BibTeX style by name. It decides how entries are sorted and printed. Leave the `.bst` out.

## biblatex bibliography style (.bbx)
A biblatex style for the list of references, picked with `style=`.

## biblatex citation style (.cbx)
A biblatex style for citations, picked with `citestyle=`.

## graphic file
A figure. The extension may be left out when the driver can try several.

## options
The key list in brackets after `\documentclass`, `\usepackage` or `\RequirePackage`: `[a4paper, 11pt]` or `[style=numeric, backend=biber]`.
A key may take a value after `=`. Options the class does not know reach the packages that declare them.

## name
Characters that make up a name, the way `\csname` builds a command name or LaTeX builds a counter's.

## text
Any balanced text: names and code alike. Nothing narrower is known about it.

## definition body
The replacement text of a definition: code that runs each time the command is used, with `#1`… standing for its arguments.

## any
Nothing is known about the text: it is read as it stands.

## nothing
No text at all: the type of an argument nothing reads.

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

## \\, \newline
Ends the line here, inside a paragraph: the line is not justified, and the paragraph goes on (`\newline` is the same without the options).
`\\*` forbids a page break after the line. `\\[⟨dimen⟩]` adds that much space below it.
In `tabular`, `array` and `align` it ends the row instead. Outside a paragraph it raises "There's no line here to end".

```tex
Name\\ Street 1\\[2pt] City
a & b \\ \hline
```

## \linebreak, \nolinebreak
`\linebreak` asks for a line break here and justifies the line, unlike `\\`. `\linebreak[⟨0–4⟩]` only suggests it, 4 forcing it. `\nolinebreak` forbids one.

```tex
a long word\linebreak[2] continues
```

## \par
Ends the paragraph: the text so far is broken into lines (tex.web § 1096). An empty line in the source is a `\par`.

## \hspace, \hspace*
Horizontal space of a given glue (`\hspace{1em plus 1fil}`). It disappears at a line break unless starred.

```tex
a\hspace{1cm}b
\hspace*{\fill}right
```

## \vspace, \vspace*
Vertical space of a given glue between paragraphs. It disappears at a page break unless starred.

```tex
\vspace{2\baselineskip}
\vspace*{\fill}
```

## \hfill, \hfil, \hss
Infinitely stretchable horizontal glue: `\hfill` (order `fill`) beats `\hfil` (order `fil`). `\hss` stretches and shrinks.

```tex
left\hfill right
```

## \vfill, \vfil, \vss
Infinitely stretchable vertical glue, pushing material to the bottom or centring it.

## \quad, \qquad, \enspace, \thinspace, \;, \:, \!
Fixed spaces: `\quad` is 1em, `\qquad` 2em, `\enspace` 0.5em, `\,` a thin space (3mu in math), `\!` a negative thin space.

## \smallskip, \medskip, \bigskip
Vertical glue between paragraphs: `\smallskipamount` (3pt plus 1pt minus 1pt), `\medskipamount` (twice that), `\bigskipamount` (four times).

## \noindent, \indent
Starts the paragraph without (or with) the `\parindent` indentation.

## \newpage, \clearpage, \cleardoublepage
`\newpage` ends the page. `\clearpage` also places every pending float first. `\cleardoublepage` continues on the next odd page in two-sided documents.

## \pagebreak, \nopagebreak
Asks for (or forbids) a page break here. `\pagebreak[⟨0–4⟩]` only suggests it.

## \hline, \cline
A horizontal rule across a table (`\hline`) or over columns i to j (`\cline{i-j}`). It goes right after a row's `\\`.

## \rule
A filled box: `\rule[⟨raise⟩]{⟨width⟩}{⟨height⟩}`. With width 0 it is a strut that holds the line open.

```tex
\rule{1cm}{0.4pt}
\rule{0pt}{3ex}
```

## \strut
An invisible rule of the line's height and depth (70% and 30% of `\baselineskip`), so a box keeps the line's size.

## \mbox, \makebox
A horizontal box: its text is never broken across lines. `\makebox[⟨width⟩][⟨l|c|r|s⟩]{…}` gives it a width.

## \phantom, \hphantom, \vphantom
Space the size of its argument, which is not shown. `\hphantom` keeps only the width, `\vphantom` the height and depth.

