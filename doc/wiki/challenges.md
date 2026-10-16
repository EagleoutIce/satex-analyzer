# Why LaTeX is hard to analyze

satex analyzes LaTeX by running it on abstract values.
This page lists what makes that harder than for other languages. Under each problem: how satex answers it, and the research behind that.
Every example is a complete document. Below each one, a table puts the text one pdflatex run ships (read from the boxes `\tracingoutput` logs, random seed 7) next to the text satex computes (`satex query typeset`), paragraph by paragraph. If a row is not equal, a short list says why: which conditional stayed undecided. Two collapsed blocks show the commands that ran, with the computed text and with a trace of what each call inserted, what each primitive set and how each conditional went.

How to read a table. In the satex column the notation is **bold**, so a literal `{`, `|` or `…` in the text stays plain:

| Notation | Meaning |
|---|---|
| `⟦x⟧` | text in a box satex cannot place: never shipped, or placed only on some paths. A box that is placed (`\box`, `\copy`, `\unhbox`, `\raise` and relatives) reads as plain text in order. A box that is never shipped does not count against equality, its row keeps the mark ⊇ |
| `{a\|b}` | a choice: an undecided conditional, one arm each |
| `⟨?⟩` | an unknown character |
| `c…` | `c` repeated one or more times by a summarized loop |
| `de·pen` | `·` is a hyphenation point where the line breaker may set a hyphen |
| ✓ | equal |
| ⊇ | covers: pdflatex's text is one of satex's choices, so the result is sound but less precise |
| ✗ | differs: unsound, a bug |

## A frame: an abstract machine with reflection

λ-calculus is close in one respect: there are no real calls or return values, only rewriting, and loops are tail recursion through `\expandafter`.
But TeX has no binding and no hygiene, and arguments are token streams.
A better frame is an abstract machine:

| TeX | machine |
|---|---|
| the input stack, the token stream | the continuation |
| `eqtb`, registers, macros | the store |
| the save stack, groups | dynamic extent |
| `\def`, `\let` at run time | reflection, as in 3-Lisp or fexprs |

λ-calculus tools still apply to control flow: k-CFA style contexts and pushdown precision for calls and returns.
The machine frame tells how to make the analysis terminate: abstract the continuation and the store to finitely many addresses.

## Relations to other formalisms

- **[λ-calculus](https://en.wikipedia.org/wiki/Lambda_calculus).** Expansion substitutes arguments into a body, and loops are tail calls. But TeX has no binding, no scope and no values: an argument is whatever tokens the body reads next.
- **[Term rewriting](https://en.wikipedia.org/wiki/Rewriting).** A macro is a rewrite rule on the head of the token stream. The rules change while rewriting, and which rule applies depends on catcodes and the store.
- **Macro processors** such as [m4](https://en.wikipedia.org/wiki/M4_(computer_language)) and the [C preprocessor](https://en.wikipedia.org/wiki/C_preprocessor). They also rescan their output, but they do not typeset, and their syntax is fixed.
- **Reflective Lisps.** [Fexprs](https://en.wikipedia.org/wiki/Fexpr) receive their arguments unevaluated, and [3-Lisp](https://doi.org/10.1145/800017.800513) lets a program inspect and change its own interpreter. `\meaning`, `\catcode` and `\def` at run time play these roles.
- **[Abstract interpretation](https://doi.org/10.1145/512950.512973).** satex executes TeX on abstract values. Soundness comes from over-approximating every concrete run.
- **[Partial evaluation](https://en.wikipedia.org/wiki/Partial_evaluation).** Known parts run, unknown parts stay residual, as in satex's typed unknowns.
- **[Control-flow analysis](https://en.wikipedia.org/wiki/Control-flow_analysis) (k-CFA).** Contexts that keep calls apart. In TeX the call is the macro and its input position.

## The problems

### 1. The syntax is not fixed

`\catcode` changes what a character means while the document runs.
There is no AST before execution.
A graph of tokens fits better than a tree.
Each token is a node. An edge leads to the tokens that can come next: the next character in the file, the body a macro expands to, the arm a conditional takes.
A `\catcode` change rewires the edges for every token read after it, and a loop is a cycle.
satex analyzes this graph rather than a parse tree, and terminates because it keeps finitely many abstract input positions per site, so every cycle is finite (see [the frame](#a-frame-an-abstract-machine-with-reflection)).

```tex
\documentclass{article}
\begin{document}
\catcode`\$=0
$def$foo{bar}
$foo
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>bar</code></td><td><code>bar</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 01-catcode.tex --set limits.seconds=60
5:5  paragraph  bar
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 01-catcode.tex --set limits.seconds=60
l.5 \foo ⇒ bar
```

</details>


### 2. Code is data

Macros take token lists, build names with `\csname`, mutate (`\g@addto@macro`), redefine themselves, and read their own text (`\meaning`).

```tex
\documentclass{article}
\makeatletter
\def\hook{A}
\g@addto@macro\hook{B}
\def\once{\global\let\once\relax first}
\begin{document}
\hook, \once\once, \csname h\@gobble Xook\endcsname, \meaning\hook
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>AB, first, AB, macro:-&gt;AB</code></td><td><code>AB, first, AB, macro:-&gt;AB</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 02-code-is-data.tex --set limits.seconds=60
7:6  paragraph  AB, first, AB, macro:->AB
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 02-code-is-data.tex --set limits.seconds=60
l.4 \g@addto@macro ⇒ \begingroup \toks@ \expandafter {\hook B… (defines \hook)
l.7 \hook ⇒ AB
l.7 \once ⇒ \global \let \once \relax first (defines \once)
l.7 \@gobble ⇒ nothing
l.7 \hook ⇒ AB
l.7 \meaning\hook ⇒ macro:->AB
```

</details>

**How satex answers it.** Code is data in satex too: a definition is a store update, and the program continues from the new store. This follows [Dalla Preda, Giacobazzi and Debray](https://www2.cs.arizona.edu/~debray/Publications/metamorphism.html), who track the versions of self-modifying code as abstract values (phase semantics).


```tex
\documentclass{article}
\makeatletter
\def\list@{}
\def\add#1{\g@addto@macro\list@{[#1]}}
\begin{document}
\add a\add b\add c\list@
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>[a][b][c]</code></td><td><code>[a][b][c]</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f r4-self-modifying.tex --set limits.seconds=60
6:25  paragraph  [a][b][c]
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f r4-self-modifying.tex --set limits.seconds=60
l.6 \add ⇒ \g@addto@macro \list@ {[a]} (defines \list@)
l.6 \add ⇒ \g@addto@macro \list@ {[b]} (defines \list@)
l.6 \add ⇒ \g@addto@macro \list@ {[c]} (defines \list@)
l.6 \list@ ⇒ [a][b][c]
```

</details>

### 3. Arguments are found at run time

How many arguments a command takes depends on how it reads them: `\@ifnextchar`, xparse, delimited parameters.
Many macros also push their own end markers into the stream, or use `\aftergroup`.

```tex
\documentclass{article}
\makeatletter
\def\opt{\@ifnextchar[\opt@with\opt@without}
\def\opt@with[#1]{(#1)}
\def\opt@without{(none)}
\def\upto#1.{[#1]}
\begin{document}
\opt[a] \opt b \upto everything up to the dot. after
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>(a) (none)b [everything up to the dot] after</code></td><td><code>(a) (none)b [everything up to the dot] after</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 03-arguments.tex --set limits.seconds=60
8:8  paragraph  (a) (none)b [everything up to the dot] after
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 03-arguments.tex --set limits.seconds=60
l.8 \opt ⇒ \@ifnextchar [\opt@with \opt@without 
l.3 \@ifnextchar ⇒ \let \reserved@d =[\def \reserved@a {\op… (defines \reserved@d \reserved@a \reserved@b … 1 more)
l.3 \opt@with ⇒ (a)
l.8 \opt ⇒ \@ifnextchar [\opt@with \opt@without 
l.3 \@ifnextchar ⇒ \let \reserved@d =[\def \reserved@a {\op… (defines \reserved@d \reserved@a \reserved@b … 1 more)
l.3 \opt@without ⇒ (none)
l.8 \upto ⇒ [everything up to the dot]
```

</details>

**How satex answers it.** An unknown token list is abstracted as a regular language over token classes, as [Christensen, Møller and Schwartzbach](https://www.brics.dk/JSA/) do for strings and [Unevalizer](https://www.cs.au.dk/~amoeller/papers/unevalizer/) for JavaScript's `eval`. So `\ifx` and argument reading still decide what they can, and a catcode write to an unknown character never touches a structural one.


```tex
\documentclass{article}
\begin{document}
\edef\x{\ifodd\pdfuniformdeviate 2 ab\else\fi}
\ifx\x\empty empty\else nonempty\fi
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>nonempty</code></td><td><code><b>{</b>empty<b>|</b>nonempty<b>}</b></code></td><td>⊇</td></tr>
</table>

Why:

- `{empty|nonempty}`: l.4 `\ifx\x\empty empty\else nonempty\fi` is undecided, it reads `\x`, `\empty`, `\else`, `\fi`.

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f r5-token-domain.tex --set limits.seconds=60
4:15  paragraph  {empty|nonempty}
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f r5-token-domain.tex --set limits.seconds=60
l.3 \ifodd\pdfuniformdeviate ⇒ undecided
l.4 \ifx\x\empty ⇒ undecided
```

</details>

### 4. Typesetting feeds back into control flow

Conditionals branch on box sizes and page state.
Whether a box has zero width can depend on fonts, kerning and hundreds of other things.

```tex
\documentclass{article}
\begin{document}
\setbox0\hbox{\textit{f}}
\ifdim\wd0>3pt wide\else narrow\fi,
\ifdim\pagegoal=\maxdimen empty page\else page started\fi
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>wide, empty page</code></td><td><code><b>⟦</b>f<b>⟧</b>wide, <b>{</b>empty page<b>|</b>page started<b>}</b></code></td><td>⊇</td></tr>
</table>

Why:

- `{empty page|page started}`: l.5 `\ifdim\pagegoal=\maxdimen` is undecided, it reads `\pagegoal`, `\maxdimen`.

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 04-typesetting.tex --set limits.seconds=60
3:25  paragraph  ⟦f⟧wide, {empty page|page started}
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 04-typesetting.tex --set limits.seconds=60
l.3 \textit ⇒ \protect \textit   (defines \reserved@a \check@icl \check@icr … 40 more)
l.4 \ifdim\wd0>3pt ⇒ true
l.5 \ifdim\pagegoal=\maxdimen ⇒ undecided
```

</details>

**How satex answers it.** Typesetting is modelled only as far as a decision reads it. Relations between values matter here: [octagons](https://doi.org/10.1007/s10990-006-8609-1) and zonotopes keep that a dimension minus itself is zero, where intervals forget it, so impossible branches survive and paths multiply, as in pgf's math.


```tex
\documentclass{article}
\begin{document}
\dimen0=\pdfuniformdeviate 1000 sp
\dimen2=-\dimen0
\advance\dimen2\dimen0
\ifdim\dimen2=0pt always\else never\fi
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>always</code></td><td><code><b>{</b>always<b>|</b>never<b>}</b></code></td><td>⊇</td></tr>
</table>

Why:

- `{always|never}`: l.6 `\ifdim\dimen2=0pt` is undecided, it reads `\dimen`.

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f r6-relational.tex --set limits.seconds=60
6:20  paragraph  {always|never}
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f r6-relational.tex --set limits.seconds=60
l.6 \ifdim\dimen2=0pt ⇒ undecided
```

</details>

### 5. Packages are interpreters

pgf computes with macro loops, tcolorbox splits boxes, hyperref and biblatex run hooks per page.
A small document executes hundreds of thousands of lines of library code.

```tex
\documentclass{article}
\usepackage{tikz}
\begin{document}
\pgfmathparse{veclen(3,4)}\pgfmathresult
\tikz\draw[->] (0,0) -- (1,1);
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>4.99994</code></td><td><code>4.99994</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 05-packages.tex --set limits.seconds=60
4:41  paragraph  4.99994
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 05-packages.tex --set limits.seconds=60
l.4 \pgfmathparse ⇒ \begingroup \pgfmath@catcodes \pgfmath@q… (defines " \ifpgfmath@quickparse \pgfmath@parse@next … 202 more)
l.4 \pgfmathresult ⇒ 4.99994
l.5 \tikz ⇒ \begingroup \tikz@startup@env \tikz@coll… (defines ; : | … 341 more)
l.5 \draw ⇒ \path [draw] (defines \tikz@signal@path \pgfutil@reserved@d \pgfutil@reserved@a … 2827 more)
```

</details>

### 6. Unknown inputs everywhere

The `.aux` file is missing on a first run, and the clock, `\pdfelapsedtime` and random numbers change every run.
Without knowing what a macro holds, no token stream after it can be read.
One unknown spreads: forks multiply, and arms with different code cannot be merged.

```tex
\documentclass{article}
\begin{document}
\ref{later}
\pdfuniformdeviate 100

\def\foo{\ifx\bar\undefined\else\bar\fi}
\def\bar#1{baz}
\foo{x}

\def\possibilityA{1}
\def\possibilityB{12}
\def\doubletaker#1#2{#2#1#2}
\expandafter\expandafter\expandafter\doubletaker
  \csname possibility\ifcsname pdftexversion\endcsname A\else B\fi\endcsname{X}%
Hehe
\section{Later}\label{later}
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>1 50</code></td><td><code>1 <b>⟨?⟩</b></code></td><td>⊇</td></tr>
<tr><td><code>bazx</code></td><td><code>bazx</code></td><td>✓</td></tr>
<tr><td><code>X1XHehe</code></td><td><code>X1XHehe</code></td><td>✓</td></tr>
<tr><td><code>1 Later</code></td><td><code>1Later</code></td><td>✓</td></tr>
</table>

Why:

- `⟨?⟩`: l.5 sets a character the run could not compute.

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 06-unknown.tex --set limits.seconds=60
3:12  paragraph  1 ⟨?⟩
8:5  paragraph  bazx
14:80  paragraph  X1XHehe
16:16  paragraph  1Later
assumption  clock-assumed
recognized gaps
  unbalanced-conditional 1× first at 06-unknown.tex:0
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 06-unknown.tex --set limits.seconds=60
l.3 \ref ⇒ \__cmd_start:nNNnnn {s}\ref  \ref code {… (defines \l__cmd_environment_bool \l__cmd_args_tl \l__cmd_fn_tl … 18 more)
l.4 \pdfuniformdeviate ⇒ 􏿿
l.5 \par ⇒ \scan_stop: \mode_if_horizontal:TF {\mod…
l.8 \foo ⇒ \ifx \bar \undefined \else \bar \fi 
l.6 \bar ⇒ baz
l.9 \par ⇒ \scan_stop: \mode_if_horizontal:TF {\mod…
l.14 \ifcsname ⇒ true
… 4 more
```

</details>

**How satex answers it.** Unknown conditions stay as choices, as in [TypeChef](https://doi.org/10.1145/2048066.2048128), [SuperC](https://doi.org/10.1145/2254064.2254103) and variational execution, which keep presence conditions and values as choices. One token stream with a choice node replaces two forked paths, and the rest is shared. After a bounded number of forks at one conditional, the arms are skipped to `\fi` and their effects over-approximated on one path.


```tex
\documentclass{article}
\begin{document}
\ifdefined\unknownmacro 1\else 12\fi X
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>12X</code></td><td><code>12X</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f r3-choice.tex --set limits.seconds=60
3:33  paragraph  12X
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f r3-choice.tex --set limits.seconds=60
l.3 \ifdefined\unknownmacro ⇒ false
```

</details>

### 7. Termination is not structural

Loops are tail recursion through `\expandafter`, error recovery, or expansion cuts.
Their length can depend on an unknown value.

```tex
\documentclass{article}
\newcount\n
\n=\pdfuniformdeviate 8
\def\down{\ifnum\n>0 .\advance\n-1 \expandafter\down\fi}
\begin{document}
\down
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>....</code></td><td><code><b>{</b>.<b>…</b><b>|</b><b>}</b></code></td><td>⊇</td></tr>
</table>

Why:

- `{.…|}`: l.4 `\ifnum\n>0` is undecided, it reads `\n`.

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 07-termination.tex --set limits.seconds=60
6:6  paragraph  {.…|}
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 07-termination.tex --set limits.seconds=60
l.2 \newcount ⇒ \e@alloc \count \countdef {\count 10}\in… (defines \n)
l.6 \down ⇒ \ifnum \n >0 .\advance \n -1 \expandafte…
l.4 \down ⇒ \ifnum \n >0 .\advance \n -1 \expandafte…
```

</details>

**How satex answers it.** [Van Horn and Might](https://doi.org/10.1145/1863543.1863553) derive an analysis from the concrete machine and keep the continuation in the store under finitely many addresses, so the analysis ends by construction. Pushdown variants ([PDCFA](https://arxiv.org/abs/1007.4268)) keep calls and returns matched. For TeX the pending input of a recursion grows with its depth, so satex bounds it. How many passes a loop on an unknown gets before a summary replaces it is a cost question, as in state merging by [Kuznetsov et al.](https://doi.org/10.1145/2254064.2254088), who merge when an estimate says it pays.


```tex
\documentclass{article}
\def\nest#1{\ifnum#1>0 (\nest{\numexpr#1-1\relax})\fi}
\begin{document}
\nest{3}
\nest{\pdfuniformdeviate 4}
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>((())) ()</code></td><td><code>((())) <b>{</b>(<b>…</b>)<b>…</b><b>|</b><b>}</b></code></td><td>⊇</td></tr>
</table>

Why:

- `{(…)…|}`: l.2 `\ifnum#1>0` is undecided.

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f r1-input-stack.tex --set limits.seconds=60
4:9  paragraph  ((())) {(…)…|}
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f r1-input-stack.tex --set limits.seconds=60
l.4 \nest ⇒ \ifnum 3>0 (\nest {\numexpr 3-1\relax })…
l.5 \nest ⇒ \ifnum \pdfuniformdeviate 4>0 (\nest {\n…
```

</details>

```tex
\documentclass{article}
\newcount\n \n=\pdfuniformdeviate 8
\def\cheap{\ifnum\n>0 \advance\n-1 \expandafter\cheap\fi}
\def\costly{\ifnum\n>0 \setbox0\hbox{\tiny long body}\advance\n-1 \expandafter\costly\fi}
\begin{document}
\cheap\costly done
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>done</code></td><td><code>done</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f r7-merge-cost.tex --set limits.seconds=60
6:16  paragraph  done
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f r7-merge-cost.tex --set limits.seconds=60
l.2 \newcount ⇒ \e@alloc \count \countdef {\count 10}\in… (defines \n)
l.6 \cheap ⇒ \ifnum \n >0 \advance \n -1 \expandafter…
l.6 \costly ⇒ \ifnum \n >0 \setbox 0\hbox {\tiny long …
l.3 \cheap ⇒ \ifnum \n >0 \advance \n -1 \expandafter…
l.6 \costly ⇒ \ifnum \n >0 \setbox 0\hbox {\tiny long …
```

</details>

### 8. Global, untyped state

Registers, `\aftergroup`, and groups that one macro opens and another closes.
Paragraph and page state change only when the output routine runs.

```tex
\documentclass{article}
\def\open{\begingroup\itshape}
\def\close{\endgroup}
\begin{document}
{\gdef\x{ after the group}\aftergroup\x inside}
\open italic\close{} upright
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>inside after the group italic upright</code></td><td><code>inside after the group italic upright</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 08-global-state.tex --set limits.seconds=60
5:42  paragraph  inside after the group italic upright
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 08-global-state.tex --set limits.seconds=60
l.5 \x ⇒  after the group
l.6 \open ⇒ \begingroup \itshape 
l.2 \itshape ⇒ \not@math@alphabet \itshape \mathit \fon… (defines \delayed@f@adjustment \f@shape@saved \f@series@saved … 26 more)
l.6 \close ⇒ \endgroup 
```

</details>

### 9. Progressive recursion

Macros call each other in a cycle and define the macros that continue it. Names are built from a counter, so the cycle never meets the same macro twice.

```tex
\documentclass{article}
\def\a#1{\ifnum#1<3 \expandafter\def\csname b#1\endcsname{\c{#1}}\csname b#1\endcsname\else end#1\fi}
\def\c#1{\expandafter\def\csname a\the\numexpr#1+1\endcsname{\a{\the\numexpr#1+1}}\csname a\the\numexpr#1+1\endcsname}
\begin{document}
\a{0}
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>end3</code></td><td><code>end3</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 09-progressive.tex --set limits.seconds=60
5:6  paragraph  end3
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 09-progressive.tex --set limits.seconds=60
l.5 \a ⇒ \ifnum 0<3 \expandafter \def \csname b0\… (defines \b0 \a1 \b1 … 3 more)
```

</details>

**How satex answers it.** Supercompilers ([Sørensen and Glück](https://arxiv.org/abs/1005.5278), [Leuschel](https://doi.org/10.1007/3-540-36377-7_17)) stop when a new configuration embeds an earlier one (homeomorphic embedding) and replace both by their most specific generalization (anti-unification). This catches a loop whose key grows every pass, which an equality test misses. Together with the self-modifying view of problem 2, each new definition is a new version of the program, not a new program.


```tex
\documentclass{article}
\def\grow#1{\ifnum\numexpr#1<5 .\grow{\numexpr#1+1\relax}\fi}
\begin{document}
\grow{0}
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>.....</code></td><td><code>.....</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f r2-growing-key.tex --set limits.seconds=60
4:9  paragraph  .....
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f r2-growing-key.tex --set limits.seconds=60
l.4 \grow ⇒ \ifnum \numexpr 0<5 .\grow {\numexpr 0+1…
```

</details>

### 10. The engine is not fixed

pdfTeX, XeTeX and LuaTeX differ in fonts, line breaking and Lua callbacks.

```tex
\documentclass{article}
\begin{document}
\ifdefined\directlua LuaTeX\else\ifdefined\XeTeXversion XeTeX\else pdfTeX\fi\fi
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>pdfTeX</code></td><td><code>pdfTeX</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 10-engine.tex --set limits.seconds=60
3:69  paragraph  pdfTeX
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 10-engine.tex --set limits.seconds=60
l.3 \ifdefined\directlua ⇒ false
l.3 \ifdefined\XeTeXversion ⇒ false
```

</details>

### 11. A document is a fixpoint computation

One run is not the program. A build runs pdflatex, bibtex or biber, makeindex, and pdflatex again, until the auxiliary files stop changing. latexmk compares the files after each run and stops at the fixpoint or after a fixed number of runs.
Each run reads what the last one wrote: labels and page numbers from the `.aux`, the table of contents, the bibliography, longtable's column widths, tikz's `remember picture` positions.
What a run writes depends on what it read. A `\ref` that turns `??` into `12` changes a line's width, which moves a page break, which changes a `\pageref`.
The cycle need not converge. A `\pageref` whose own text pushes its target onto the next page oscillates between two values, and varioref reports such a loop.
Inside one run the output routine is a cycle of its own: it fires when a page fills, and what it puts back decides the next break.

In this example the label moves away from the page the last run recorded. The kept `.aux` says page 1, so this run breaks the page and writes page 2. The next run reads 2, does not break, and writes 1 again. latexmk gives up after its maximum number of runs.

```tex
\documentclass{article}
\usepackage{refcount}
\begin{document}
The label was on page \pageref{x}.
\ifnum\getpagerefnumber{x}=1 \newpage\fi
\label{x}Here.
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>The label was on page 1.</code></td><td><code>The label was on page 1.</code></td><td>✓</td></tr>
<tr><td><code>Here.</code></td><td><code>Here.</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f 11-fixpoint.tex --set limits.seconds=60
4:2  paragraph  The label was on page 1.
6:11  paragraph  Here.
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f 11-fixpoint.tex --set limits.seconds=60
l.4 \pageref ⇒ \__cmd_start:nNNnnn {s}\pageref  \pagere… (defines \l__cmd_environment_bool \l__cmd_args_tl \l__cmd_fn_tl … 18 more)
l.5 \getpagerefnumber ⇒ \romannumeral \ltx@ifundefined {r@x}{\ex…
l.5 \ifnum\getpagerefnumber{x}=1 ⇒ true
l.5 \newpage ⇒ \if@noskipsec \ifx \@nodocument \relax \…
l.6 \label ⇒ \@bsphack \begingroup \UseHookWithArgume… (defines \thepage \protect \reserved@a)
```

</details>

**How satex answers it.** satex analyzes one run. The auxiliary files on disk are its input, and a missing one is unknown. Each line the run writes is run as the next run reads it, in the kernel state at `\begin{document}`, so the keys the next run declares are known. satex does not iterate runs. In abstract interpretation terms the build is a [Kleene iteration](https://en.wikipedia.org/wiki/Kleene_fixed-point_theorem) of one function from auxiliary files to auxiliary files. A sound analysis of the whole build needs that function's abstract fixpoint, with [widening](https://doi.org/10.1145/512950.512973) where the concrete runs oscillate or need more rounds than an analysis can afford.

## Where satex is still wrong or stuck

These examples are red tests today.
Each runs under a 60 second limit.
If satex hits a limit, the table shows what it computes at that point and the limit's name.

### A reference in math inside positioned nodes

A `\ref` to an unknown label in math in a tikz node, with the nodes placed by `positioning`, is sound but far too imprecise: the text of each node becomes a large choice of unknown characters.

```tex
\documentclass{article}
\usepackage{tikz}
\usetikzlibrary{positioning}
\usepackage{hyperref}
\begin{document}
\begin{tikzpicture}
  \node (a) {$B_{\ref{cell:B-a}}$};
  \node[below=of a] (c) {$B_{\ref{cell:B-c}}$};
  \node[below right=of a] (d) {$B_{\ref{cell:B-d}}$};
  \node[below left=of a] (b) {$B_{\ref{cell:B-b}}$};
  \draw[->] (a) -- (b);
\end{tikzpicture}
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>B?? B?? B?? B??</code></td><td><code>B??B??B??B??</code></td><td>✓</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f s1-tikz-ref.tex --set limits.seconds=60
7:16  paragraph  B??B??B??B??
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f s1-tikz-ref.tex --set limits.seconds=60
l.3 \usetikzlibrary ⇒ \pgfutil@ifnextchar [{\use@tikzlibrary }… (defines \pgfutil@reserved@d \pgfutil@reserved@a \pgfutil@reserved@b … 16 more)
l.6 \tikzpicture ⇒ \begingroup \tikz@startup@env \tikz@coll… (defines ; : | … 327 more)
l.7 \node ⇒ \tikz@path@overlay {node} (defines \tikz@signal@path \pgfutil@reserved@d \pgfutil@reserved@a … 240 more)
l.7 \ref ⇒ \__cmd_start:nNNnnn {s}\ref  \ref code {… (defines \l__cmd_environment_bool \l__cmd_args_tl \l__cmd_fn_tl … 71 more)
l.8 \node ⇒ \tikz@path@overlay {node} (defines \tikz@signal@path \pgfutil@reserved@d \pgfutil@reserved@a … 472 more)
l.8 \ref ⇒ \__cmd_start:nNNnnn {s}\ref  \ref code {… (defines \l__cmd_environment_bool \l__cmd_args_tl \l__cmd_fn_tl … 51 more)
l.9 \node ⇒ \tikz@path@overlay {node} (defines \tikz@signal@path \pgfutil@reserved@d \pgfutil@reserved@a … 472 more)
… 2 more
```

</details>

### A rounded node around a paragraph that breaks

The height of a node with `text width` depends on the line breaker, and pgf's rounded corners read it. satex marks each possible hyphen (`·`) but does not choose the breaks, so the height stays unknown, and pgf's math leaves a long tail of unknown boxes.

```tex
\documentclass{article}
\usepackage{tikz}
\begin{document}
\begin{tikzpicture}
\node[rounded corners=10pt,draw,text width=3cm,align=center] {files, dependencies, more};
\end{tikzpicture}
\end{document}
```

<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>
<tr><td><code>files, de-pen-den-cies, more</code></td><td><code>files, de<b>·</b>pen<b>·</b>den<b>·</b>cies, more</code></td><td>⊇</td></tr>
</table>

<details><summary>Computed text</summary>

```sh
$ satex query typeset -f s2-rounded-paragraph.tex --set limits.seconds=60
5:64  paragraph  files, de·pen·den·cies, more
assumption  clock-assumed
```

</details>

<details><summary>Trace</summary>

```sh
$ satex trace --format compact -f s2-rounded-paragraph.tex --set limits.seconds=60
l.4 \tikzpicture ⇒ \begingroup \tikz@startup@env \tikz@coll… (defines ; : | … 327 more)
l.5 \node ⇒ \tikz@path@overlay {node} (defines \tikz@signal@path \pgfutil@reserved@d \pgfutil@reserved@a … 587 more)
```

</details>
