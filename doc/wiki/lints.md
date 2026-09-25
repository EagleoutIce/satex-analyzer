<!-- generated from doc/src/wiki/lints.md.in by satex-doc; do not edit -->

# Lint rules

`satex lint` checks the input file with every rule below. `--all` also checks the packages and classes it loads. `--rules` lists the codes, `--explain CODE` prints one rule's explanation.

## Quick-fixes

A finding's `fix` says what to do. Where that is mechanical it carries the edits, e.g. removing a `\usepackage` line or one name in `\usepackage{a,b,c}`, a `\label{…}`, or turning `\newcommand` into `\renewcommand`. The text output marks them `[fix]` or `[unsafe fix]`:

* **safe** fixes keep the document's meaning: a duplicate `\usepackage`, a definition replaced before it is read, `\renewcommand` of an undefined name;
* **unsafe** fixes change it and deserve a look: removing a package that may have side effects, an unused label, `\newcommand` turned into `\renewcommand`.

`--fix` applies the safe fixes, `--unsafe-fixes` adds the unsafe ones, `--diff` prints the edits instead of writing them. Only project files change. Overlapping fixes wait for the next round. The document is re-analyzed after each round, up to eight.

```text
$ satex lint --diff --unsafe-fixes -f samples/paper.tex
--- a/samples/paper.tex
+++ b/samples/paper.tex
@@ -2,7 +2,6 @@
 
 \newif\ifdraft
 \usepackage{amsmath}
-\usepackage{graphicx}
 \usepackage{hyperref}
 \usepackage{mypackage}
 \input{preamble.tex}
@@ -11,18 +10,12 @@
 \newcommand{\highlight}[1]{\textbf{#1}}
 \newcommand{\norm}[1]{\left\lVert#1\right\rVert}
 
-\def\WeirdMore super #1\; { #1 }
 
-\def\Weird key: #1, value: #2, magic: #3\;{%
-  \texttt{key: #1, value: #2, magic: #3}%
-  \typeout{key: #1, value: #2, magic: #3}%
-  \WeirdMore
-}
 
-\csname ifdraft\endcsname\else
+\csuse{ifdraft}\else
 % Directly recursive; found by the `recursion` query.
-\newcommand{\repeat}[1]{#1\repeat{#1}}
 \fi
+\usepackage{microtype}
 \begin{document}
 
 \def\DocName{Document}
@@ -39,7 +32,6 @@
 \fi
 
 \section{Methodology}
-\label{sec:method}
 
 We define the loss function as follows.
 
@@ -72,7 +64,6 @@
 \end{table}
 
 \section{Conclusion}
-\label{sec:conclusion}
 
 \highlight{satex} correctly analyzes this document.
 See \autoref{sec:intro} for motivation and \autoref{sec:results} for
--- a/samples/preamble.tex
+++ b/samples/preamble.tex
@@ -1,1 +0,0 @@
-\def\hello{world}
\ No newline at end of file
```

`--format json` gives each fix as `applicability` and `edits` (`path`, 1-based `start`/`end`, `replacement`), `--format sarif` as `fixes`, `--format lsp` as diagnostics with `quickfix` code actions.

## Suppressing findings

A comment in the document silences findings. Without codes it silences every rule, with codes (comma-separated) only those. `lint_off` in `satex.yaml` turns a rule off everywhere.

* `% satex-disable-next-line CODE,…` — the next line.
* `% satex-disable-line CODE,…` — at the end of a line, that line.
* `% satex-disable CODE,…` … `% satex-enable CODE,…` — a region.
* `% satex-disable-file CODE,…` — the whole file.

They apply to every output format and to `--fix`. An unknown code is reported as `unknown-suppress-code`.

| code | category | severity | summary |
| --- | --- | --- | --- |
| `already-defined` | correctness | error | \newcommand redefines an existing name |
| `build-bibliography-disabled` | correctness | error | the document has a bibliography, but the build never runs BibTeX or biber |
| `build-command-placeholders` | correctness | error | an engine command in the latexmkrc lacks %S or %O |
| `build-engine-mismatch` | correctness | error | the build runs another engine, or another output, than the document needs |
| `build-shell-escape-missing` | correctness | error | the document runs programs through \write18, but the build does not allow it |
| `duplicate-label` | correctness | error | the same label is defined twice |
| `missing-graphic` | correctness | error | an included image is not there for this output target |
| `not-defined` | correctness | error | \renewcommand targets a name that does not exist |
| `option-clash` | correctness | error | a package is loaded twice with different options |
| `raised-error` | correctness | error | the document's code raises a TeX error |
| `undefined-control-sequence` | correctness | error | a control sequence is used that nothing defines |
| `undefined-environment` | correctness | error | \begin names an environment that is not defined |
| `undefined-glossary-entry` | correctness | error | a glossary or acronym entry is used but never declared |
| `unguarded-recursion` | correctness | error | a recursive macro has nothing that can stop it |
| `build-missing-custom-dependency` | correctness | warning | the document writes glossary files, but latexmk has no rule to process them |
| `catcode-escapes-group` | correctness | warning | a category code change is still in force at the end of the run |
| `duplicate-bibliography-entry` | correctness | warning | the same key is declared by two bibliography entries |
| `environment-mismatch` | correctness | warning | \end names an environment other than the one that is open |
| `expl-syntax-left-on` | correctness | warning | \ExplSyntaxOn is never turned off |
| `expl3-signature-mismatch` | correctness | warning | an expl3 function takes other arguments than its name says |
| `missing-dependency` | correctness | warning | a TeX Live package the run reads from is not in the dependency file |
| `unbalanced-pdf-content` | correctness | warning | a PDF literal opens marked content or a graphics state that is never closed |
| `undefined-citation` | correctness | warning | a citation names a key no bibliography entry declares |
| `undefined-optional-content` | correctness | warning | marked content refers to an optional content group no resource names |
| `undefined-reference` | correctness | warning | \ref names a label that is never defined |
| `unresolved-file` | correctness | warning | a package, class or input file is not in the installation |
| `build-shell-escape-unneeded` | security | warning | the build allows \write18, but the document never uses it |
| `shell-escape` | security | warning | the document runs a program through \write18 |
| `group-local-definition` | style | warning | a definition is made inside a group and lost when it closes |
| `unknown-suppress-code` | style | warning | a satex-disable comment names a code no rule has |
| `build-bibliography-unneeded` | style | info | the build configures BibTeX or biber for a document without a bibliography |
| `build-engine-options` | style | info | the engine command could report errors as file:line and write SyncTeX data |
| `build-unused-custom-dependency` | style | info | a latexmk custom dependency that nothing in the document triggers |
| `computer-modern-in-t1` | style | info | T1 text in Computer Modern relies on cm-super for scalable fonts |
| `dead-definition` | style | info | a definition is replaced before anything uses it |
| `hand-set-quantity` | style | info | a number and its unit are set by hand where siunitx would set them |
| `microtype-available` | style | info | the engine can protrude characters and expand fonts, but microtype is not loaded |
| `ot1-font-encoding` | style | info | accented letters are built with \accent because the text is set in OT1 |
| `primitive-tex-command` | style | info | a plain TeX primitive is used where LaTeX has its own interface |
| `stray-space` | style | info | a macro body picked up a space from an unescaped line end |
| `unused-bibitem` | style | info | a \bibitem written in the document is never cited |
| `unused-definition` | style | info | a macro defined in the document is never used |
| `unused-label` | style | info | a label is never referenced |
| `duplicate-package` | performance | warning | a package is loaded more than once |
| `package-after-preamble` | performance | warning | a package is loaded after \begin{document} |
| `preamble-cost` | performance | info | what the preamble costs on every build |
| `recursive-macro` | performance | info | a macro calls itself, directly or through others |
| `unused-dependency` | performance | info | the dependency file lists a TeX Live package no file of the run comes from |
| `unused-package` | performance | info | a loaded package defines nothing the document uses |
| `analysis-imprecision` | precision | info | the analysis had to widen |

## correctness

### `undefined-glossary-entry`

```text
$ satex lint --explain undefined-glossary-entry
undefined-glossary-entry  correctness / error
a glossary or acronym entry is used but never declared

`\gls` and its relatives name an entry that no `\newglossaryentry`,
`\newacronym`, `\acro` or `\DeclareAcronym` declares.  A real run stops
with `Glossary entry ... has not been defined`.

Fix by declaring the entry, or by correcting the key.
```

### `shell-escape`

```text
$ satex lint --explain shell-escape
shell-escape  security / warning
the document runs a program through \write18

`\write18` hands its text to the shell.  A run only does that when the engine
was started with `--shell-escape`, or with `--shell-restricted` and the
program is one TeX Live allows.  `shell_escape` in `satex.yaml` says which of
the three this build is.

Fix by running with the flag the document needs, or by removing the call.
```

### `missing-graphic`

```text
$ satex lint --explain missing-graphic
missing-graphic  correctness / error
an included image is not there for this output target

`\includegraphics` names a file that graphicx cannot find, with any extension
in `\Gin@extensions`, beside the document or on `\graphicspath`.  Which
extensions count is the driver's (pdftex.def, dvips.def, xetex.def), so it
follows the output.

Fix by adding the file, or by producing the format the driver reads.
```

### `expl3-signature-mismatch`

```text
$ satex lint --explain expl3-signature-mismatch
expl3-signature-mismatch  correctness / warning
an expl3 function takes other arguments than its name says

An expl3 name carries its signature after the colon ([interface3](https://ctan.org/pkg/expl3), "Naming
conventions"): one letter per argument, `N` a single token, `n` a braced
group, and so on.  What the function really consumes (the calls the run saw,
or, for one defined in the document, running it on probe input) takes a
different number of arguments, or an optional one or a star, which no expl3
signature has.  `\foo:nn` that takes one argument leaves the second for
whatever follows.

Fix by renaming the function to the signature it has, or its parameter text.
```

### `unguarded-recursion`

```text
$ satex lint --explain unguarded-recursion
unguarded-recursion  correctness / error
a recursive macro has nothing that can stop it

The run saw the macro's expansion reach the macro again, from a call written
in the replacement text of a macro of the cycle (a tail call included), the
run could not see the recursion end, and its replacement text contains no
conditional — no `\if…`, `\else` or
`\fi` — so nothing can end the recursion.  A macro nothing expands is judged
by the names its replacement text holds, and reported as a warning.  A real run expands until TeX runs out of memory:
`TeX capacity exceeded, sorry [main memory size]`.

Fix by adding the test that stops it, or by removing the self-reference.
```

### `undefined-control-sequence`

```text
$ satex lint --explain undefined-control-sequence
undefined-control-sequence  correctness / error
a control sequence is used that nothing defines

The name has no meaning at the point it is used, so a real run would stop with
`Undefined control sequence`.  When the message says "not on every path", the
definition sits in a conditional arm that this use does not depend on.

Fix by loading the package that provides it, defining it before the use, or
correcting the spelling.
```

### `undefined-environment`

```text
$ satex lint --explain undefined-environment
undefined-environment  correctness / error
\begin names an environment that is not defined

`\begin{name}` expands `\name`, and nothing defines it.  A real run reports
`Environment name undefined`.

Fix by loading the package that provides the environment or by defining it with
`\newenvironment{name}{…}{…}`.
```

### `unresolved-file`

```text
$ satex lint --explain unresolved-file
unresolved-file  correctness / warning
a package, class or input file is not in the installation

The file was searched for in the document's directory, the configured search
paths and the TEXMF trees, and not found.  `satex query distribution` shows
which installation was searched.

Fix by installing the package, adding its directory to `search_paths` in
`satex.yaml`, or pointing `texmf_roots` at the right installation.
```

### `option-clash`

```text
$ satex lint --explain option-clash
option-clash  correctness / error
a package is loaded twice with different options

LaTeX loads a package once.  A second `\usepackage` with options the first one
did not have raises `Option clash for package`.

Fix by giving all the options to the first load, or by using
`\PassOptionsToPackage` before it.
```

### `already-defined`

```text
$ satex lint --explain already-defined
already-defined  correctness / error
\newcommand redefines an existing name

`\newcommand` refuses to redefine and raises `Command … already defined`.

Fix with `\renewcommand` if the redefinition is intended, or pick another name.
```

### `not-defined`

```text
$ satex lint --explain not-defined
not-defined  correctness / error
\renewcommand targets a name that does not exist

`\renewcommand` requires an existing definition and raises
`Undefined control sequence`.

Fix with `\newcommand`, or load the package that was supposed to define it.
```

### `undefined-reference`

```text
$ satex lint --explain undefined-reference
undefined-reference  correctness / warning
\ref names a label that is never defined

No `\label` in the document or its inputs declares this key, so the reference
prints `??`.

Fix by adding the `\label`, or by correcting the key.
```

### `duplicate-label`

```text
$ satex lint --explain duplicate-label
duplicate-label  correctness / error
the same label is defined twice

LaTeX warns `Label … multiply defined` and every reference resolves to the
later one.

Fix by renaming one of them.
```

### `catcode-escapes-group`

```text
$ satex lint --explain catcode-escapes-group
catcode-escapes-group  correctness / warning
a category code change is still in force at the end of the run

`\catcode` is local, so a change made inside a group is undone by the closing
brace.  One still in force at the end of the document was made at the outer
level, and everything read after it is tokenized differently.

Fix by wrapping the change in `{…}` or `\begingroup…\endgroup`, or by restoring
the old value explicitly.  `satex query catcodes` shows the current table.
```

### `expl-syntax-left-on`

```text
$ satex lint --explain expl-syntax-left-on
expl-syntax-left-on  correctness / warning
\ExplSyntaxOn is never turned off

With expl3 syntax on, spaces are ignored and `_` and `:` are letters, so
ordinary text stops working.

Fix by adding `\ExplSyntaxOff` after the code that needs it.
```

### `missing-dependency`

```text
$ satex lint --explain missing-dependency
missing-dependency  correctness / warning
a TeX Live package the run reads from is not in the dependency file

depp names the TeX Live package of every file a run reads by where it sits in
the tree (`tex/⟨format⟩/⟨package⟩/`) and writes them to `DEPENDS.txt`.  The
project's dependency file lacks one of them, so an installation made from it
— a minimal Docker image, a `texlive.withPackages` — cannot build the
document.

Fix by adding the package to the dependency file, or by rerunning depp.
```

### `undefined-citation`

```text
$ satex lint --explain undefined-citation
undefined-citation  correctness / warning
a citation names a key no bibliography entry declares

No `\bibitem` and no entry of the `.bib` databases `\bibliography` names
declares this key, so LaTeX prints `?` and warns `Citation … undefined`.
Citations are observed rather than listed: a citation command writes its key
to the `.aux` for the bibliography program (`\citation{⟨key⟩}`) and reads
the name the entry defines when the file is read back (`\b@⟨key⟩`); any line
written so is a citation.  Nothing is reported while a named database cannot
be found.

Fix by adding the entry, or by correcting the key.
```

### `duplicate-bibliography-entry`

```text
$ satex lint --explain duplicate-bibliography-entry
duplicate-bibliography-entry  correctness / warning
the same key is declared by two bibliography entries

Two `\bibitem`s with one key make LaTeX warn `Label … multiply defined` and
every citation resolves to the later one; two `.bib` entries with one key make
BibTeX stop with `Repeated entry`.

Fix by renaming or deleting one of them.
```

### `unbalanced-pdf-content`

```text
$ satex lint --explain unbalanced-pdf-content
unbalanced-pdf-content  correctness / warning
a PDF literal opens marked content or a graphics state that is never closed

Marked content (`BMC`/`BDC` … `EMC`) and saved graphics states (`q` … `Q`)
have to nest within a page's content stream (PDF 32000, §§ 8.4.2, 14.6).  An
`EMC` or `Q` without its opening operator, or one left open, makes viewers
reject the page or show an optional content layer on every page after it.
Optional content packages (ocgx2, ocg-p, pdfbase) write these operators with
`\pdfliteral` or `\special{pdf:…}` at the start and end of an environment.

satex does not break pages, so it checks the order in which the run executes
the literals; a pair split by a page break is not seen.

Fix by closing what is opened in the same group, or by using one environment
for both ends.
```

### `undefined-optional-content`

```text
$ satex lint --explain undefined-optional-content
undefined-optional-content  correctness / warning
marked content refers to an optional content group no resource names

`/OC /name BDC` refers to `/name` in the page's `/Properties` resources, which
maps it to an optional content group (PDF 32000, § 8.11.3.2).  A name that no
`\pdfpageresources`, `\pdfobj` or `pdf:` object mentions is unknown to the
viewer, and the content is shown or hidden at its whim.  The rule is silent
while the run observed no such resources at all.

Fix by declaring the group, or by correcting the name.
```

## style

### `dead-definition`

```text
$ satex lint --explain dead-definition
dead-definition  style / info
a definition is replaced before anything uses it

The name is defined, and defined again, with no use in between: the first
replacement text never reaches the typesetter.  Both definitions are made on
every path, so this is not a conditional fallback.

Fix by deleting the first definition, or by checking which one was meant.
```

### `unused-label`

```text
$ satex lint --explain unused-label
unused-label  style / info
a label is never referenced

Nothing refers to this label.  Harmless, but usually a leftover or a typo in
the reference.

Fix by deleting the `\label`, or by checking the `\ref` that was meant to use it.
```

### `group-local-definition`

```text
$ satex lint --explain group-local-definition
group-local-definition  style / warning
a definition is made inside a group and lost when it closes

`\def` and `\newcommand` are local.  Inside `{…}`, `\begingroup…\endgroup` or
an environment, the definition disappears at the closing brace.

Fix by moving the definition out of the group, or by prefixing it with
`\global`.
```

### `primitive-tex-command`

```text
$ satex lint --explain primitive-tex-command
primitive-tex-command  style / info
a plain TeX primitive is used where LaTeX has its own interface

chktex warning 41 (`You ought to not use primitive TeX in LaTeX code`) flags
the primitives in chktexrc's `Primitives` list, which chktex leaves off by
default.  satex reports one only where the name still means the primitive, and
names the LaTeX command for the same job where there is one.  For `\csname`
the job is what the run saw the name formed for: defined after
`\expandafter\def`, made an alias after `\expandafter\let`, tested by
`\expandafter\ifx…\relax`, or used; the suggestion is etoolbox's command
for it (`\csdef`, `\csgdef`, `\csedef`, `\csxdef`, `\cslet`, `\letcs`,
`\ifcsundef`, `\csuse`) when the document has it, else the kernel's
(`\@namedef`, `\@ifundefined`, `\@nameuse`).
`latex_alternatives` in `satex.yaml` adds entries or overrides them.

Fix by using LaTeX's interface for the same job, if one exists.
```

### `unused-definition`

```text
$ satex lint --explain unused-definition
unused-definition  style / info
a macro defined in the document is never used

Nothing expands this name.  It may be intended for later, or it may be dead.

Fix by deleting it if it is dead.
```

### `stray-space`

```text
$ satex lint --explain stray-space
stray-space  style / info
a macro body picked up a space from an unescaped line end

Inside a macro's replacement text, TeX turns the end of a line into a space
token unless a control word already skipped it, or a `%` swallowed the rest
of the line ([tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) § 347).  This is the classic missing `%`: an extra
space nobody wrote, which shows up wherever the macro is used.

A body wrapped start-to-end in the kernel's own `\@bsphack…\@esphack` pair is
exempt, since that pair keeps whatever is inside it from reaching the page.
The finding follows the modes the run was in each time it read that space
([tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) § 1043: a space is glue only in horizontal mode): it says the
macro *inserts* a space when every reading was in horizontal mode, that it
*may* when some might have been, and nothing when none was or the space
was never read as material (an argument delimiter, a `\write`).

Fix by writing `%` at the end of that line.
```

### `unknown-suppress-code`

```text
$ satex lint --explain unknown-suppress-code
unknown-suppress-code  style / warning
a satex-disable comment names a code no rule has

A `satex-disable…`/`satex-enable` magic comment named a code that
`satex lint --rules` does not list, most often a typo.  It suppresses
nothing, since matching a finding's own code is all these comments do.

Fix by correcting the code, or removing it if the rule was renamed or
never existed.
```

### `unused-bibitem`

```text
$ satex lint --explain unused-bibitem
unused-bibitem  style / info
a \bibitem written in the document is never cited

A `thebibliography` written by hand lists what it lists, cited or not.  A
database only contributes what is cited, so its entries are not reported.
`\nocite{*}` cites everything.

Fix by citing the entry, or by deleting it.
```

### `ot1-font-encoding`

```text
$ satex lint --explain ot1-font-encoding
ot1-font-encoding  style / info
accented letters are built with \accent because the text is set in OT1

OT1, LaTeX's default text encoding, has no accented letters: the encoding
builds each one with the `\accent` primitive (ot1enc.def; fntguide § 5).  TeX
does not hyphenate a word that contains an `\accent` ([The TeXbook](https://ctan.org/pkg/texbook), appendix H),
and the PDF's text layer holds an accent and a letter instead of the character,
which breaks search and copy.  T1 has the accented letters as glyphs, and the
same encoding file then turns them into single characters.

The rule fires when `\encodingdefault` is OT1 and the body really executed
`\accent`; how the letters were typed, `\"a` or `ä`, does not matter.

Fix by loading `\usepackage[T1]{fontenc}`.
```

### `computer-modern-in-t1`

```text
$ satex lint --explain computer-modern-in-t1
computer-modern-in-t1  style / info
T1 text in Computer Modern relies on cm-super for scalable fonts

Computer Modern in T1 is the EC family, which TeX Live provides as scalable
Type 1 fonts only through the cm-super package; without it the PDF gets
bitmap fonts that look blurred on screen.  Latin Modern is the same design
with T1 glyphs of its own (lmodern documentation).  The rule fires when
`\encodingdefault` is T1 and `\rmdefault` is still the kernel's `cmr`.

Fix by loading `\usepackage{lmodern}`, or by making sure cm-super is installed.
```

### `microtype-available`

```text
$ satex lint --explain microtype-available
microtype-available  style / info
the engine can protrude characters and expand fonts, but microtype is not loaded

pdfTeX and LuaTeX writing PDF support character protrusion and font expansion
(pdfTeX manual, `\pdfprotrudechars` and `\pdfadjustspacing`), which make
margins look straighter and let paragraphs break with fewer hyphens and less
uneven spacing.  microtype turns both on (microtype manual § 1).  The rule
fires when both are still off at the end of the run.  XeTeX only
protrudes, and DVI output supports neither, so the rule stays silent there.

Fix by loading `\usepackage{microtype}`.
```

### `hand-set-quantity`

```text
$ satex lint --explain hand-set-quantity
hand-set-quantity  style / info
a number and its unit are set by hand where siunitx would set them

`2ms` sets the number and the unit as one word, so the line may break between
them and the unit is italic in math mode.  `2\,\mathrm{ms}` puts the space in
by hand, which fixes neither the unit's own spacing nor the decimal marker, and
a `\newcommand` that holds the unit only hides the same markup.  The SI
brochure (9th ed., § 5.4.3) asks for a non-breaking space between a value and
its unit, and for the unit in an upright font.  siunitx does all of it:
`\qty{2}{\milli\second}` (`\SI` before version 3).

The rule reads what the run typeset, not the source: a digit run followed by
letters, with no space token between them.  The unit has to be an SI symbol or
one of the units the SI accepts (brochure, tables 1-4 and § 4.1), the number
has to start a word, and the quantity has to be set by the document rather than
handed to a package's own command, so a document that already formats its units
with siunitx, units or physics is left alone.

Fix by loading `\usepackage{siunitx}` and writing `\qty{⟨number⟩}{⟨unit⟩}`.
```

## performance

### `duplicate-package`

```text
$ satex lint --explain duplicate-package
duplicate-package  performance / warning
a package is loaded more than once

The second request is ignored by LaTeX.  It costs nothing at run time but it
hides which load carries the options.

Fix by deleting the later `\usepackage`.
```

### `unused-package`

```text
$ satex lint --explain unused-package
unused-package  performance / info
a loaded package defines nothing the document uses

Every name the package defines is unused.  The package may still be needed for
its side effects — page layout, hooks, fonts, `\AtBeginDocument` code — so this
is a hint, not a verdict.

Fix, if the package really is unused, by deleting the `\usepackage`: it is read
on every build.
```

### `recursive-macro`

```text
$ satex lint --explain recursive-macro
recursive-macro  performance / info
a macro calls itself, directly or through others

Recursion is normal in TeX, but an unguarded one loops until TeX runs out of
memory, and the analysis has to widen at it — everything the recursion would
have defined becomes unknown.

Fix, if the recursion is unintended, by breaking the cycle; otherwise make sure
a conditional terminates it.
```

### `package-after-preamble`

```text
$ satex lint --explain package-after-preamble
package-after-preamble  performance / warning
a package is loaded after \begin{document}

`\usepackage` belongs in the preamble; after `\begin{document}` LaTeX raises
`\usepackage before \begin{document}` and the package cannot set anything up.

Fix by moving the load into the preamble.
```

### `unused-dependency`

```text
$ satex lint --explain unused-dependency
unused-dependency  performance / info
the dependency file lists a TeX Live package no file of the run comes from

Every installation made from the dependency file carries the package, but the
run reads nothing from it.  It may still be needed for what satex does not
read — fonts, hyphenation patterns, Lua libraries, programs — so this is a
hint, not a verdict.

Fix, if the package really is unused, by deleting its line.
```

### `preamble-cost`

```text
$ satex lint --explain preamble-cost
preamble-cost  performance / info
what the preamble costs on every build

The packages the document asks for and the files read before
`\begin{document}`.  This work repeats on every compilation.

Fix by precompiling the preamble into a format with `mylatexformat` or
`precompiled preamble` support in your editor, which skips it entirely.
```

## precision

### `analysis-imprecision`

```text
$ satex lint --explain analysis-imprecision
analysis-imprecision  precision / info
the analysis had to widen

Undecided conditionals are analyzed arm by arm, so they are not reported here.
This rule reports the two places where the analysis is incomplete instead:
a recursion that was widened, and a budget that was reached.  Findings that
depend on such a point may be incomplete.

There is nothing to fix in the document; raising the limits in `satex.yaml` or
narrowing the analysis with `load_packages: false` can reduce it.
```

## build

These rules compare the build configuration (`latexmkrc`, `% arara:` lines) with what the document needs: shell escape for a `\write18`, the engine its primitives require, PDF output, a bibliography, a glossary. SaTeX reads latexmkrc's plain `$name = value;` assignments, `set_tex_cmds(…)` and `add_cus_dep(…)` without running Perl. Option meanings follow latexmk(1).

### `build-shell-escape-missing`

```text
$ satex lint --explain build-shell-escape-missing
build-shell-escape-missing  correctness / error
the document runs programs through \write18, but the build does not allow it

The run reached `\write18`, which only hands its text to the shell when the
engine is started with `-shell-escape` (or `-shell-restricted` and a program
TeX Live allows).  latexmk starts the engine with the command in `$pdflatex`,
`$lualatex`, `$xelatex` or `$latex`, whichever `$pdf_mode` selects (latexmk(1),
`$pdf_mode`); arara's `pdflatex`, `lualatex` and `xelatex` rules take
`shell: yes` (arara manual, "The official rules").

Fix by adding `-shell-escape` to the command, e.g.
`$pdflatex = 'pdflatex -shell-escape %O %S';`, or
`set_tex_cmds('-shell-escape %O %S');` for every engine at once.
```

### `build-shell-escape-unneeded`

```text
$ satex lint --explain build-shell-escape-unneeded
build-shell-escape-unneeded  security / warning
the build allows \write18, but the document never uses it

`-shell-escape` lets every package and every `.aux` or `.bbl` line the
document reads run any program as you.  The run reached no `\write18`, so the
permission buys nothing and only widens what a hostile input file can do.

Fix by removing `-shell-escape` from the engine command (or `shell: yes` from
the arara directive).
```

### `build-engine-mismatch`

```text
$ satex lint --explain build-engine-mismatch
build-engine-mismatch  correctness / error
the build runs another engine, or another output, than the document needs

latexmk's `$pdf_mode` picks the engine: 1 runs `$pdflatex`, 4 `$lualatex`, 5
`$xelatex`, and 0, 2 and 3 run `$latex` for DVI (latexmk(1), `$pdf_mode`).  The
document contradicts it when a `% !TeX program` line names another engine,
when it uses a primitive only another engine has (`\directlua` is LuaTeX's,
`\XeTeXrevision` XeTeX's), or when it sets `\pdfoutput` to the other output
than latexmk waits for.

Fix by setting `$pdf_mode` to the engine the document is written for.
```

### `build-bibliography-disabled`

```text
$ satex lint --explain build-bibliography-disabled
build-bibliography-disabled  correctness / error
the document has a bibliography, but the build never runs BibTeX or biber

`$bibtex_use = 0` tells latexmk never to run BibTeX or biber (latexmk(1),
`$bibtex_use`), so the `.bbl` file the bibliography is read from is never made
and every citation stays undefined.

Fix by removing the setting; latexmk's default, 1, runs them whenever the
`.bib` files exist.
```

### `build-bibliography-unneeded`

```text
$ satex lint --explain build-bibliography-unneeded
build-bibliography-unneeded  style / info
the build configures BibTeX or biber for a document without a bibliography

The latexmkrc sets `$bibtex_use`, `$bibtex` or `$biber`, but the document names
no bibliography database (`\bibliography`, `\addbibresource`), so the setting
never takes effect (latexmk(1), `$bibtex_use`).

Fix by removing the setting.
```

### `build-missing-custom-dependency`

```text
$ satex lint --explain build-missing-custom-dependency
build-missing-custom-dependency  correctness / warning
the document writes glossary files, but latexmk has no rule to process them

latexmk runs BibTeX, biber and makeindex by itself, but a glossary is sorted by
a program it does not know about (`makeglossaries`, `makeindex` with a glossary
style, `bib2gls`).  Without a custom dependency (latexmk(1), "CUSTOM
DEPENDENCIES") the glossary files are never made and the glossary stays empty.
The glossaries manual gives the rule, e.g.

    add_cus_dep('glo', 'gls', 0, 'makeglossaries');
    sub makeglossaries { system("makeglossaries '$_[0]'"); }
```

### `build-unused-custom-dependency`

```text
$ satex lint --explain build-unused-custom-dependency
build-unused-custom-dependency  style / info
a latexmk custom dependency that nothing in the document triggers

`add_cus_dep(from, to, …)` makes latexmk run a program when a file with the
`from` extension is newer than the `to` file it produces (latexmk(1), "CUSTOM
DEPENDENCIES").  The document writes no glossary or index, and no file with the
`from` extension is in the project, so the rule never runs.

Fix by removing the rule, or keep it if another document of the project needs it.
```

### `build-command-placeholders`

```text
$ satex lint --explain build-command-placeholders
build-command-placeholders  correctness / error
an engine command in the latexmkrc lacks %S or %O

latexmk substitutes `%S` with the source file and `%O` with the options it
passes itself (latexmk(1), "FORMAT OF COMMAND SPECIFICATIONS").  A command
without `%S` compiles nothing, and one without `%O` loses `-jobname`,
`-output-directory` and the options given on latexmk's own command line.

Fix by ending the command with `%O %S`.
```

### `build-engine-options`

```text
$ satex lint --explain build-engine-options
build-engine-options  style / info
the engine command could report errors as file:line and write SyncTeX data

`-file-line-error` makes the engine report `file:line: message` instead of
`! message`, which editors and satex's log reading can jump to; `-synctex=1`
writes the data that links the PDF back to the source (pdftex(1), luatex(1)).
Both cost nothing measurable.

Fix by adding them to the command, e.g.
`$pdflatex = 'pdflatex -file-line-error -synctex=1 %O %S';`.
```
