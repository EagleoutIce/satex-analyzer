<!-- generated from doc/src/wiki/lints.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Lint rules

`satex lint` checks the input file with every rule below. `--all` also checks the packages and classes it loads. `--rules` lists the codes, `--explain CODE` prints one rule's explanation. See [Chktex-Coverage](Chktex-Coverage) for the chktex warning each rule follows, and [Primitive-coverage](Primitive-coverage) and [Domains](Domains) for how satex gets the facts a rule reads.

## Quick-fixes

A finding's `fix` says what to do. Where that is mechanical it carries the edits, e.g. removing a `\usepackage` line or one name in `\usepackage{a,b,c}`, a `\label{…}`, or turning `\newcommand` into `\renewcommand`. The text output marks them `[fix]` or `[unsafe fix]`:

* **safe** fixes keep the document's meaning: a duplicate `\usepackage`, a definition replaced before it is read, `\renewcommand` of an undefined name;
* **unsafe** fixes change it and deserve a look: removing a package that may have side effects, an unused label, `\newcommand` turned into `\renewcommand`.

`--fix` applies the safe fixes, `--unsafe-fixes` adds the unsafe ones, `--diff` prints the edits instead of writing them. Only project files change. Overlapping fixes wait for the next round. The document is re-analyzed after each round, up to eight.

`--format json` gives each fix as `applicability` and `edits` (`path`, 1-based `start`/`end`, `replacement`), `--format sarif` as `fixes`, `--format lsp` as diagnostics with `quickfix` code actions.

## Suppressing findings

A comment in the document silences findings. Without codes it silences every rule, with codes (comma-separated) only those. `lint_off` in `satex.yaml` turns a rule off everywhere.

* `% satex-disable-next-line CODE,…` — the next line.
* `% satex-disable-line CODE,…` — at the end of a line, that line.
* `% satex-disable CODE,…` … `% satex-enable CODE,…` — a region.
* `% satex-disable-file CODE,…` — the whole file.

They apply to every output format and to `--fix`. An unknown code is reported as `unknown-suppress-code`.

## Rules

Every rule, with a link to its explanation below.

| code | category | severity | summary |
| --- | --- | --- | --- |
| [`already-defined`](#already-defined) | correctness | error | \newcommand redefines an existing name |
| [`build-bibliography-disabled`](#build-bibliography-disabled) | correctness | error | the document has a bibliography, but the build never runs BibTeX or biber |
| [`build-command-placeholders`](#build-command-placeholders) | correctness | error | an engine command in the latexmkrc lacks %S or %O |
| [`build-engine-mismatch`](#build-engine-mismatch) | correctness | error | the build runs another engine, or another output, than the document needs |
| [`build-shell-escape-missing`](#build-shell-escape-missing) | correctness | error | the document runs programs through \write18, but the build does not allow it |
| [`duplicate-label`](#duplicate-label) | correctness | error | the same label is defined twice |
| [`math-still-open`](#math-still-open) | correctness | error | the run is still in math mode where the text ends |
| [`missing-graphic`](#missing-graphic) | correctness | error | an included image is not there for this output target |
| [`not-defined`](#not-defined) | correctness | error | \renewcommand targets a name that does not exist |
| [`option-clash`](#option-clash) | correctness | error | a package is loaded twice with different options |
| [`raised-error`](#raised-error) | correctness | error | the document's code raises a TeX error |
| [`undefined-color`](#undefined-color) | correctness | error | a color is used, or aliased from, before xcolor ever defines it |
| [`undefined-control-sequence`](#undefined-control-sequence) | correctness | error | a control sequence is used that nothing defines |
| [`undefined-environment`](#undefined-environment) | correctness | error | \begin names an environment that is not defined |
| [`undefined-glossary-entry`](#undefined-glossary-entry) | correctness | error | a glossary or acronym entry is used but never declared |
| [`unguarded-recursion`](#unguarded-recursion) | correctness | error | a recursive macro has nothing that can stop it |
| [`build-missing-custom-dependency`](#build-missing-custom-dependency) | correctness | warning | the document writes glossary files, but latexmk has no rule to process them |
| [`catcode-escapes-group`](#catcode-escapes-group) | correctness | warning | a category code change is still in force at the end of the run |
| [`conflicting-color-definition`](#conflicting-color-definition) | correctness | warning | the same color name is defined twice with different colors |
| [`delimiter-not-followed`](#delimiter-not-followed) | correctness | warning | \left, \right or \middle is followed by a brace or a math shift |
| [`duplicate-bibliography-entry`](#duplicate-bibliography-entry) | correctness | warning | the same key is declared by two bibliography entries |
| [`environment-mismatch`](#environment-mismatch) | correctness | warning | \end names an environment other than the one that is open |
| [`expl-syntax-left-on`](#expl-syntax-left-on) | correctness | warning | \ExplSyntaxOn is never turned off |
| [`expl3-signature-mismatch`](#expl3-signature-mismatch) | correctness | warning | an expl3 function takes other arguments than its name says |
| [`ignored-verbatim-tail`](#ignored-verbatim-tail) | correctness | warning | text stands on the line that closes a verbatim body and never reaches the page |
| [`math-script-braces`](#math-script-braces) | correctness | warning | `^` and `_` take one token, so what follows needs braces |
| [`missing-dependency`](#missing-dependency) | correctness | warning | a TeX Live package the run reads from is not in the dependency file |
| [`unbalanced-brackets`](#unbalanced-brackets) | correctness | warning | a file sets more of one bracket than of its partner |
| [`unbalanced-color-stack`](#unbalanced-color-stack) | correctness | warning | a `\pdfcolorstack` push is never popped, or a pop has nothing to pop |
| [`unbalanced-pdf-content`](#unbalanced-pdf-content) | correctness | warning | a PDF literal opens marked content or a graphics state that is never closed |
| [`undefined-citation`](#undefined-citation) | correctness | warning | a citation names a key no bibliography entry declares |
| [`undefined-optional-content`](#undefined-optional-content) | correctness | warning | marked content refers to an optional content group no resource names |
| [`undefined-reference`](#undefined-reference) | correctness | warning | \ref names a label that is never defined |
| [`uninstalled-package`](#uninstalled-package) | correctness | warning | the package is not installed, and the distribution could install it |
| [`unresolved-file`](#unresolved-file) | correctness | warning | a package, class or input file is not in the installation |
| [`math-mode-mismatch`](#math-mode-mismatch) | correctness | info | \(, \), \[ or \] stands where the mode does not allow it |
| [`unintended-escape`](#unintended-escape) | correctness | info | a command is followed by an escaped punctuation mark, which is another command |
| [`build-shell-escape-unneeded`](#build-shell-escape-unneeded) | security | warning | the build allows \write18, but the document never uses it |
| [`shell-escape`](#shell-escape) | security | warning | the document runs a program through \write18 |
| [`accent-on-dotted-letter`](#accent-on-dotted-letter) | style | warning | an accent sits on top of the dot of an `i` or a `j` |
| [`command-terminated-space`](#command-terminated-space) | style | warning | a control word ate the space after it and the next word is set against what it produced |
| [`dimension-times`](#dimension-times) | style | warning | the letter x stands between two numbers where a multiplication sign belongs |
| [`double-space`](#double-space) | style | warning | two interword spaces reached the page, one of them from a tie |
| [`ellipsis-dots`](#ellipsis-dots) | style | warning | three periods set three periods, not an ellipsis |
| [`group-local-definition`](#group-local-definition) | style | warning | a definition is made inside a group and lost when it closes |
| [`math-operator-name`](#math-operator-name) | style | warning | an operator name is set in math italic instead of by its command |
| [`math-shift-dollars`](#math-shift-dollars) | style | warning | math opened with `$` or `$$` where LaTeX spells it \( or \[ |
| [`missing-italic-correction`](#missing-italic-correction) | style | warning | a slanted letter runs into upright text with no `\/` between them |
| [`missing-nonbreaking-space`](#missing-nonbreaking-space) | style | warning | an ordinary space in front of a reference lets the line break before the number |
| [`parenthesis-spacing`](#parenthesis-spacing) | style | warning | a parenthesis carries the space of the text inside it instead of the text around it |
| [`quote-direction`](#quote-direction) | style | warning | a quotation mark points the wrong way, or a run of them mixes both directions |
| [`sentence-spacing`](#sentence-spacing) | style | warning | a period set the wrong width of space for the sentence around it |
| [`space-before-command`](#space-before-command) | style | warning | a space lets the line break between a word and the mark that belongs to it |
| [`space-before-punctuation`](#space-before-punctuation) | style | warning | a space stands between a word and the punctuation after it |
| [`stray-italic-correction`](#stray-italic-correction) | style | warning | a repeated italic correction, or one in front of a period or comma |
| [`unknown-suppress-code`](#unknown-suppress-code) | style | warning | a satex-disable comment names a code no rule has |
| [`user-pattern`](#user-pattern) | style | warning | the source matches a pattern the project asked to be warned about |
| [`wrong-dash`](#wrong-dash) | style | warning | a run of hyphens sets a dash of the wrong length for what stands around it |
| [`wrong-quote-character`](#wrong-quote-character) | style | warning | a character that is not a quotation mark stands in for one |
| [`zero-italic-correction`](#zero-italic-correction) | style | warning | the italic correction is zero here, so `\/` changes nothing |
| [`build-bibliography-unneeded`](#build-bibliography-unneeded) | style | info | the build configures BibTeX or biber for a document without a bibliography |
| [`build-engine-options`](#build-engine-options) | style | info | the engine command could report errors as file:line and write SyncTeX data |
| [`build-unused-custom-dependency`](#build-unused-custom-dependency) | style | info | a latexmk custom dependency that nothing in the document triggers |
| [`comment-marker`](#comment-marker) | style | info | a comment still carries a TODO-style marker |
| [`computer-modern-in-t1`](#computer-modern-in-t1) | style | info | T1 text in Computer Modern relies on cm-super for scalable fonts |
| [`csquotes-available`](#csquotes-available) | style | info | the document sets quotation marks by hand instead of loading csquotes |
| [`dead-definition`](#dead-definition) | style | info | a definition is replaced before anything uses it |
| [`duplicate-color-value`](#duplicate-color-value) | style | info | two color names store the exact same color |
| [`hand-set-quantity`](#hand-set-quantity) | style | info | a number and its unit are set by hand where siunitx would set them |
| [`math-punctuation`](#math-punctuation) | style | info | punctuation sits on the wrong side of a formula |
| [`microtype-available`](#microtype-available) | style | info | the engine can protrude characters and expand fonts, but microtype is not loaded |
| [`ot1-font-encoding`](#ot1-font-encoding) | style | info | accented letters are built with \accent because the text is set in OT1 |
| [`primitive-tex-command`](#primitive-tex-command) | style | info | a plain TeX primitive is used where LaTeX has its own interface |
| [`quote-ligature`](#quote-ligature) | style | info | three quotation marks in a row set a double and a single mark with nothing between them |
| [`quote-punctuation`](#quote-punctuation) | style | info | punctuation stands inside the closing quotation mark |
| [`stray-space`](#stray-space) | style | info | a macro body picked up a space from an unescaped line end |
| [`unreferenced-numbered-display`](#unreferenced-numbered-display) | style | info | a display that stepped a counter is never referenced, so its number is spent for nothing |
| [`unused-bibitem`](#unused-bibitem) | style | info | a \bibitem written in the document is never cited |
| [`unused-color`](#unused-color) | style | info | a color the document defines is never used |
| [`unused-definition`](#unused-definition) | style | info | a macro defined in the document is never used |
| [`unused-label`](#unused-label) | style | info | a label is never referenced |
| [`package-after-preamble`](#package-after-preamble) | performance | warning | a package is loaded after \begin{document} |
| [`duplicate-package`](#duplicate-package) | performance | info | a package is loaded more than once |
| [`preamble-cost`](#preamble-cost) | performance | info | what the preamble costs on every build |
| [`recursive-macro`](#recursive-macro) | performance | info | a macro calls itself, directly or through others |
| [`unused-dependency`](#unused-dependency) | performance | info | the dependency file lists a TeX Live package no file of the run comes from |
| [`unused-package`](#unused-package) | performance | info | a loaded package defines nothing the document uses |
| [`analysis-imprecision`](#analysis-imprecision) | precision | info | the analysis had to widen |

## Explanations

One group per category. Open a group to read its rules.

<details>
<summary><b>correctness</b> — 37 rules</summary>

### `already-defined`

**correctness / error** — \newcommand redefines an existing name

`\newcommand` refuses to redefine and raises `Command … already defined`.

Fix with `\renewcommand` if the redefinition is intended, or pick another name.

See [source2e § ltdefns.dtx](https://ctan.org/pkg/source2e).

### `build-bibliography-disabled`

**correctness / error** — the document has a bibliography, but the build never runs BibTeX or biber

`$bibtex_use = 0` tells latexmk never to run BibTeX or biber (latexmk(1), `$bibtex_use`), so the `.bbl` file the bibliography is read from is never made and every citation stays undefined.

Fix by removing the setting. latexmk's default, 1, runs them whenever the `.bib` files exist.

See [latexmk § $bibtex_use](https://ctan.org/pkg/latexmk).

### `build-command-placeholders`

**correctness / error** — an engine command in the latexmkrc lacks %S or %O

latexmk substitutes `%S` with the source file and `%O` with the options it passes itself (latexmk(1), "FORMAT OF COMMAND SPECIFICATIONS"). A command without `%S` compiles nothing, and one without `%O` loses `-jobname`, `-output-directory` and the options given on latexmk's own command line.

Fix by ending the command with `%O %S`.

See [latexmk § FORMAT OF COMMAND SPECIFICATIONS](https://ctan.org/pkg/latexmk).

### `build-engine-mismatch`

**correctness / error** — the build runs another engine, or another output, than the document needs

latexmk's `$pdf_mode` picks the engine: 1 runs `$pdflatex`, 4 `$lualatex`, 5 `$xelatex`, and 0, 2 and 3 run `$latex` for DVI (latexmk(1), `$pdf_mode`). The document contradicts it when a `% !TeX program` line names another engine, when it uses a primitive only another engine has (`\directlua` is LuaTeX's, `\XeTeXrevision` XeTeX's), or when it sets `\pdfoutput` to the other output than latexmk waits for.

Fix by setting `$pdf_mode` to the engine the document is written for.

See [latexmk § $pdf_mode](https://ctan.org/pkg/latexmk).

### `build-shell-escape-missing`

**correctness / error** — the document runs programs through \write18, but the build does not allow it

The run reached `\write18`, which only hands its text to the shell when the engine is started with `-shell-escape` (or `-shell-restricted` and a program TeX Live allows). latexmk starts the engine with the command in `$pdflatex`, `$lualatex`, `$xelatex` or `$latex`, whichever `$pdf_mode` selects (latexmk(1), `$pdf_mode`). arara's `pdflatex`, `lualatex` and `xelatex` rules take `shell: yes` ([arara manual, "The official rules"](https://ctan.org/pkg/arara)).

Fix by adding `-shell-escape` to the command, e.g. `$pdflatex = 'pdflatex -shell-escape %O %S';`, or `set_tex_cmds('-shell-escape %O %S');` for every engine at once.

See [latexmk § $pdf_mode](https://ctan.org/pkg/latexmk), [arara § The official rules](https://ctan.org/pkg/arara).

### `duplicate-label`

**correctness / error** — the same label is defined twice

LaTeX warns `Label … multiply defined` and every reference resolves to the later one.

Fix by renaming one of them.

See [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### `math-still-open`

**correctness / error** — the run is still in math mode where the text ends

A `$` that is never closed leaves TeX in math mode. The next `\par` makes TeX report `Missing $ inserted` and close the formula itself ([tex.web § 1047][tw-1047] `insert_dollar_sign`), and everything up to there is set as mathematics.

The rule reports what the interpreter already saw: the `missing-dollar` diagnostic it raised, or, where no `\par` followed, a last character still carrying a math mode. It reports one finding, never both.

Fix by closing the formula.

See [chktex 16](https://www.nongnu.org/chktex/), [tex.web § 1047](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20410).

### `missing-graphic`

**correctness / error** — an included image is not there for this output target

`\includegraphics` names a file that graphicx cannot find, with any extension in `\Gin@extensions`, beside the document or on `\graphicspath`. Which extensions count is the driver's (pdftex.def, dvips.def, xetex.def), so it follows the output.

Fix by adding the file, or by producing the format the driver reads.

See [graphics § pdftex.def](https://ctan.org/pkg/graphics).

### `not-defined`

**correctness / error** — \renewcommand targets a name that does not exist

`\renewcommand` requires an existing definition and raises `Undefined control sequence`.

Fix with `\newcommand`, or load the package that was supposed to define it.

See [source2e § ltdefns.dtx](https://ctan.org/pkg/source2e).

### `option-clash`

**correctness / error** — a package is loaded twice with different options

LaTeX loads a package once. A second `\usepackage` with options the first one did not have raises `Option clash for package`.

Fix by giving all the options to the first load, or by using `\PassOptionsToPackage` before it.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### `raised-error`

**correctness / error** — the document's code raises a TeX error

Running the document reaches `\errmessage`, which every LaTeX error (`\PackageError`, `\msg_error:nn`, the kernel's own) ends in: a real run stops there. A more specific rule that explains the same error reports it instead. An error reached only on a path satex could not decide, or after it lost track of the mode, is not one a real run need reach, and is not reported.

### `undefined-color`

**correctness / error** — a color is used, or aliased from, before xcolor ever defines it

xcolor reaches every color through `\color@⟨name⟩` ([xcolor § 2.5](https://ctan.org/pkg/xcolor)). A name the run never defined there raises `Undefined color` when xcolor itself looks it up. The rule reads the same lookup the run made: a name a `\color`, `\textcolor` or `\colorlet` reached while nothing had defined it.

Fix by defining the color first, or by correcting its name.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### `undefined-control-sequence`

**correctness / error** — a control sequence is used that nothing defines

The name has no meaning at the point it is used, so a real run would stop with `Undefined control sequence`. When the message says "not on every path", the definition sits in a conditional arm that this use does not depend on.

Fix by loading the package that provides it, defining it before the use, or correcting the spelling.

See [The TeXbook § chapter 27](https://ctan.org/pkg/texbook).

### `undefined-environment`

**correctness / error** — \begin names an environment that is not defined

`\begin{name}` expands `\name`, and nothing defines it. A real run reports `Environment name undefined`.

Fix by loading the package that provides the environment or by defining it with `\newenvironment{name}{…}{…}`.

See [source2e § ltmiscen.dtx](https://ctan.org/pkg/source2e).

### `undefined-glossary-entry`

**correctness / error** — a glossary or acronym entry is used but never declared

`\gls` and its relatives name an entry that no `\newglossaryentry`, `\newacronym`, `\acro` or `\DeclareAcronym` declares. A real run stops with `Glossary entry ... has not been defined`.

Fix by declaring the entry, or by correcting the key.

See [glossaries § Defining glossary entries](https://ctan.org/pkg/glossaries).

### `unguarded-recursion`

**correctness / error** — a recursive macro has nothing that can stop it

The run saw the macro's expansion reach the macro again, could not see the recursion end, and found no conditional — no `\if…`, `\else` or `\fi` — to end it. A macro nothing expands is judged by the names its replacement text holds. A real run expands until TeX runs out of memory: `TeX capacity exceeded, sorry [main memory size]`.

Fix by adding the test that stops it, or by removing the self-reference.

### `build-missing-custom-dependency`

**correctness / warning** — the document writes glossary files, but latexmk has no rule to process them

latexmk runs BibTeX, biber and makeindex by itself, but a glossary is sorted by a program it does not know about (`makeglossaries`, `makeindex` with a glossary style, `bib2gls`). Without a custom dependency (latexmk(1), "CUSTOM DEPENDENCIES") the glossary files are never made and the glossary stays empty. The glossaries manual gives the rule, e.g.

```text
add_cus_dep('glo', 'gls', 0, 'makeglossaries');
sub makeglossaries { system("makeglossaries '$_[0]'"); }
```

See [latexmk § CUSTOM DEPENDENCIES](https://ctan.org/pkg/latexmk), [glossaries § bib2gls](https://ctan.org/pkg/glossaries).

### `catcode-escapes-group`

**correctness / warning** — a category code change is still in force at the end of the run

`\catcode` is local, so a change made inside a group is undone by the closing brace. One still in force at the end of the document was made at the outer level, and everything read after it is tokenized differently.

Fix by wrapping the change in `{…}` or `\begingroup…\endgroup`, or by restoring the old value explicitly. `satex query catcodes` shows the current table.

See [The TeXbook § chapter 5](https://ctan.org/pkg/texbook).

### `conflicting-color-definition`

**correctness / warning** — the same color name is defined twice with different colors

Two `\definecolor`-family calls store different colors under the same name. Only the last one is ever read back.

Fix by giving the second color its own name, or by removing the earlier definition.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### `delimiter-not-followed`

**correctness / warning** — \left, \right or \middle is followed by a brace or a math shift

`\left`, `\right` and `\middle` scan a delimiter: one character with a `\delcode`, or one control sequence made by `\delimiter` ([tex.web § 1161][tw-1161]). A `{`, a `}` or a `$` is none of those, so TeX reports `Missing delimiter (. inserted)` and sets nothing.

The rule asks what each command means rather than matching its name, so it covers `\middle` and any `\let` alias, which chktex's name list does not.

Fix by writing `\{` and `\}` for brace delimiters, or `\left.` where no delimiter is wanted.

See [chktex 43](https://www.nongnu.org/chktex/), [tex.web § 1161](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L22057).

### `duplicate-bibliography-entry`

**correctness / warning** — the same key is declared by two bibliography entries

Two `\bibitem`s with one key make LaTeX warn `Label … multiply defined` and every citation resolves to the later one. Two `.bib` entries with one key make BibTeX stop with `Repeated entry`.

Fix by renaming or deleting one of them.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### `environment-mismatch`

**correctness / warning** — \end names an environment other than the one that is open

Environments nest like brackets: the name `\end` gives has to match the one `\begin` most recently opened. chktex tracks this as warnings 9 (`'%s' expected, found '%s'`), 10 (`Solo '%s' found`) and 15 (`No match found for '%s'`) for brackets and environments alike. satex only records `\begin`/`\end` occurrences, so this rule covers environments and not generic `{}`, `[]` or `()` nesting.

Fix by naming the environment that is actually open, or by closing the inner environments first.

See [chktex 9](https://www.nongnu.org/chktex/), [chktex 10](https://www.nongnu.org/chktex/), [chktex 15](https://www.nongnu.org/chktex/), [source2e § ltmiscen.dtx](https://ctan.org/pkg/source2e).

### `expl-syntax-left-on`

**correctness / warning** — \ExplSyntaxOn is never turned off

With expl3 syntax on, spaces are ignored and `_` and `:` are letters, so ordinary text stops working.

Fix by adding `\ExplSyntaxOff` after the code that needs it.

See [source2e § ltfiles.dtx](https://ctan.org/pkg/source2e).

### `expl3-signature-mismatch`

**correctness / warning** — an expl3 function takes other arguments than its name says

An expl3 name carries its signature after the colon ([interface3](https://ctan.org/pkg/expl3), "Naming conventions"): one letter per argument, `N` a single token, `n` a braced group.

What the function really consumes — the calls the run saw, or probe input for one the document defines — takes a different number of arguments, or an optional one or a star, which no expl3 signature has. `\foo:nn` that takes one argument leaves the second for whatever follows.

Fix by renaming the function to the signature it has, or its parameter text.

See [interface3 § Naming conventions](https://ctan.org/pkg/l3kernel).

### `ignored-verbatim-tail`

**correctness / warning** — text stands on the line that closes a verbatim body and never reaches the page

A verbatim environment that reads its body line by line throws away whatever follows `\end{…}` on the same line. `fancyvrb` does this and the kernel's `verbatim` does not, so chktex reports warning 31 for both.

satex reports only where the text was really lost. It knows which environments the run read verbatim, because opening one changed the category code of `\`, and reports the tail only where the run set none of it.

Fix by moving the text to the next line.

See [chktex 31](https://www.nongnu.org/chktex/).

### `math-script-braces`

**correctness / warning** — `^` and `_` take one token, so what follows needs braces

`^` and `_` attach the single next token to the atom before them ([The TeXbook](https://ctan.org/pkg/texbook), chapter 16). `x^12` sets the exponent 1 and then a 2 of its own size, and `(a+b)^2` puts the exponent on the closing parenthesis alone.

The rule lexes the source with TeX's own mouth where the run really was in math mode, so a `^` under another catcode regime is not one, and `\left( … \right)^2` is one atom the script attaches to.

Fix by putting the script between `{` and `}`.

See [chktex 3](https://www.nongnu.org/chktex/), [chktex 25](https://www.nongnu.org/chktex/), [The TeXbook § chapter 16](https://ctan.org/pkg/texbook).

### `missing-dependency`

**correctness / warning** — a TeX Live package the run reads from is not in the dependency file

depp names the TeX Live package of every file a run reads by where it sits in the tree (`tex/⟨format⟩/⟨package⟩/`) and writes them to `DEPENDS.txt`. The project's dependency file lacks one of them, so an installation made from it — a minimal Docker image, a `texlive.withPackages` — cannot build the document.

Fix by adding the package to the dependency file, or by rerunning depp.

### `unbalanced-brackets`

**correctness / warning** — a file sets more of one bracket than of its partner

TeX gives `(`, `)`, `[` and `]` no meaning of their own, but a document uses them for structure, so a count that does not match lost a bracket (chktex 17).

This rule counts the brackets the run really set, so one a macro produced counts and one a macro read as an argument does not. A bracket in math is a delimiter, and one read verbatim is program text. The finding points at the first bracket left over.

Fix by adding the bracket that is missing.

See [chktex 17](https://www.nongnu.org/chktex/), [The TeXbook § chapter 2](https://ctan.org/pkg/texbook).

### `unbalanced-color-stack`

**correctness / warning** — a `\pdfcolorstack` push is never popped, or a pop has nothing to pop

`\pdfcolorstack⟨stack⟩ push` and `pop` nest like `q`/`Q` in the page's own content stream ([pdfTeX manual, § 8.9.8](https://ctan.org/pkg/pdftex)): a viewer that reads the stack back after the page still expects it empty. A stack number the run could not decide is left out, since satex would otherwise be guessing which stack a push or pop belongs to.

Fix by balancing every push with a pop in the same group.

See [pdftex § 8.9.8](https://ctan.org/pkg/pdftex).

### `unbalanced-pdf-content`

**correctness / warning** — a PDF literal opens marked content or a graphics state that is never closed

`\pdfliteral` and `\special{pdf:…}` (what ocgx2, ocg-p and pdfbase use to write raw PDF) drop `q`/`Q` and `BMC`/`BDC`/`EMC` straight into the page's content stream. `q`/`Q` save and restore the graphics state — color, line width, the current transform; `BMC`/`BDC`/`EMC` bracket marked content, an optional content group among them. Both have to nest within the page ([PDF 32000, §§ 8.4.2, 14.6](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf)). One left open leaks its state into everything typeset after it; a viewer may also just reject the page.

satex does not break pages into content streams itself, so it checks the order the run executes the literals in.

Fix by closing what is opened in the same group, or by using one environment for both ends.

See [PDF 32000 § 8.4.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf), [PDF 32000 § 14.6](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).

### `undefined-citation`

**correctness / warning** — a citation names a key no bibliography entry declares

No `\bibitem` and no entry of the `.bib` databases `\bibliography` names declares this key, so LaTeX prints `?` and warns `Citation … undefined`. Citations are observed rather than listed: a citation command writes its key to the `.aux` for the bibliography program (`\citation{⟨key⟩}`) and reads the name the entry defines when the file is read back (`\b@⟨key⟩`). Any line written so is a citation. Nothing is reported while a named database cannot be found.

Fix by adding the entry, or by correcting the key.

See [source2e § ltbibl.dtx](https://ctan.org/pkg/source2e).

### `undefined-optional-content`

**correctness / warning** — marked content refers to an optional content group no resource names

`/OC /name BDC` refers to `/name` in the page's `/Properties` resources, which maps it to an optional content group ([PDF 32000, § 8.11.3.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf)). A name that no `\pdfpageresources`, `\pdfobj` or `pdf:` object mentions is unknown to the viewer, and the content is shown or hidden at its whim. The rule is silent while the run observed no such resources at all.

Fix by declaring the group, or by correcting the name.

See [PDF 32000 § 8.11.3.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).

### `undefined-reference`

**correctness / warning** — \ref names a label that is never defined

No `\label` in the document or its inputs declares this key, so the reference prints `??`.

Fix by adding the `\label`, or by correcting the key.

See [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### `uninstalled-package`

**correctness / warning** — the package is not installed, and the distribution could install it

The file was not found in any tree the run searched, and the distribution's package manager names a package that ships it. The load is therefore no typo: the package is only missing from this machine. A LaTeX package and the package that carries it often differ in name, so the finding names both.

Fix by running the command the finding gives, or `satex install`, which never runs as part of `lint --fix`. The question is off by default because it reads the package repository: turn on `plugins.packages` in `satex.yaml`.

See [TeX Live manual § tlmgr](https://tug.org/texlive/doc.html), [MiKTeX manual § miktex packages](https://docs.miktex.org/manual/).

### `unresolved-file`

**correctness / warning** — a package, class or input file is not in the installation

The file was searched for in the document's directory, the configured search paths and the TEXMF trees, and not found. Case mismatches on case-insensitive filesystems are also reported: they work on Windows or macOS but break on Linux. `satex query distribution` shows which installation was searched.

Fix by installing the package, adding its directory to `search_paths` in `satex.yaml`, pointing `texmf.roots` at the right installation, or (for case mismatches) renaming the file to match the requested name.

See [chktex 27](https://www.nongnu.org/chktex/).

### `math-mode-mismatch`

**correctness / info** — \(, \), \[ or \] stands where the mode does not allow it

LaTeX's four math delimiters each test the mode before they act. `\(` and `\[` require that math is off, `\)` inner math and `\]` display math. Where the test fails they call `\@badmath`, which reports `Bad math environment delimiter` ([source2e](https://ctan.org/pkg/source2e), ltmath.dtx).

The rule reads the mode each one really ran in, so it sees a `\(` nested in a `$ … $` and a delimiter reached through a macro, where chktex follows the source and misses both.

Fix by pairing each delimiter with its own partner.

See [chktex 49](https://www.nongnu.org/chktex/), [source2e § ltmath.dtx](https://ctan.org/pkg/source2e).

### `unintended-escape`

**correctness / info** — a command is followed by an escaped punctuation mark, which is another command

`\TeX\. Right?` does not set `TeX. Right?`. `\.` is the dot accent, so it takes the `R` and sets `TeXṘight?` (chktex 21).

The rule looks for a control word followed with nothing between by `\.`, `\,`, `\;`, `\:`, `\!` or `\?`, where a space and a word follow. It stays quiet in math mode, where `\,` is a thin space, inside a body the run read verbatim, and where the run really did set that mark there.

Fix by removing the backslash.

See [chktex 21](https://www.nongnu.org/chktex/), [The TeXbook § chapter 9](https://ctan.org/pkg/texbook).

</details>

<details>
<summary><b>security</b> — 2 rules</summary>

### `build-shell-escape-unneeded`

**security / warning** — the build allows \write18, but the document never uses it

`-shell-escape` lets every package and every `.aux` or `.bbl` line the document reads run any program as you. The run reached no `\write18`, so the permission buys nothing and only widens what a hostile input file can do.

Fix by removing `-shell-escape` from the engine command (or `shell: yes` from the arara directive).

See [pdftex § -shell-escape](https://ctan.org/pkg/pdftex).

### `shell-escape`

**security / warning** — the document runs a program through \write18

`\write18` hands its text to the shell. A run only does that when the engine was started with `--shell-escape`, or with `--shell-restricted` and the program is one TeX Live allows. `shell_escape` in `satex.yaml` says which of the three this build is.

Fix by running with the flag the document needs, or by removing the call.

See [pdftex § -shell-escape](https://ctan.org/pkg/pdftex).

</details>

<details>
<summary><b>style</b> — 42 rules</summary>

### `accent-on-dotted-letter`

**style / warning** — an accent sits on top of the dot of an `i` or a `j`

An accent is placed over the whole character, so an `i` keeps its dot under it ([The TeXbook](https://ctan.org/pkg/texbook), chapter 9). `\i` and `\j` are the dotless letters, and math has `\imath` and `\jmath` ([fntguide § 5](https://ctan.org/pkg/fntguide)).

The rule reads the character the `\accent` or `\mathaccent` primitive really took, and how far the argument reaches from the source, so `\^{}i` is left alone. In OT1 `\'i` is a composite for `\'\i` and in T1 the glyph `í`, so neither is reported.

Fix by writing the dotless letter under the accent.

See [chktex 7](https://www.nongnu.org/chktex/), [The TeXbook § chapter 9](https://ctan.org/pkg/texbook), [fntguide § 5](https://ctan.org/pkg/fntguide).

### `command-terminated-space`

**style / warning** — a control word ate the space after it and the next word is set against what it produced

TeX reads a control word and then discards the spaces after it ([The TeXbook](https://ctan.org/pkg/texbook), chapter 3), so `\TeX ok` sets `TEXok`.

A space the source has after a control word and the run never turned into glue is one TeX swallowed. The rule reports it only where the command put a character on the page and the next word starts against it, so `\rm text` and `\noindent text` stay silent.

Fix by writing `\TeX{} ok`, or `\TeX\ ok` where a space belongs.

See [chktex 1](https://www.nongnu.org/chktex/), [The TeXbook § chapter 3](https://ctan.org/pkg/texbook).

### `dimension-times`

**style / warning** — the letter x stands between two numbers where a multiplication sign belongs

`640x480` sets the letter x between the numbers. The sign for a product is `\times`, a binary operator with the spacing of one ([The TeXbook](https://ctan.org/pkg/texbook), chapter 16).

The rule reads the characters the run set, so it finds a dimension a macro assembled as well as one typed out. It names `\times` where the letter was already in math mode and `$\times$` where it was in text.

Fix by writing the operator.

See [chktex 29](https://www.nongnu.org/chktex/), [The TeXbook § chapter 16](https://ctan.org/pkg/texbook).

### `double-space`

**style / warning** — two interword spaces reached the page, one of them from a tie

TeX reads a run of spaces as one space token ([The TeXbook](https://ctan.org/pkg/texbook), chapter 8), so two spaces in the source set one on the page. satex reports only the spaces that really reached it.

Two do when a tie stands beside an ordinary space, as in `Dr.~ Smith`, and when the run appended two glues in a row, as in `a {} b`. That second case is chktex's warning 30, off by default, so it comes as a note.

Fix by deleting the ordinary space.

See [chktex 39](https://www.nongnu.org/chktex/), [chktex 30](https://www.nongnu.org/chktex/), [The TeXbook § chapter 8](https://ctan.org/pkg/texbook).

### `ellipsis-dots`

**style / warning** — three periods set three periods, not an ellipsis

Three `.` characters are set with the spacing of three periods, which is tighter than an ellipsis ([The TeXbook](https://ctan.org/pkg/texbook), chapter 18). `\ldots` sets them on the baseline and `\cdots` at the height of a binary operator.

The rule reads the periods that reached the page, so an ellipsis a command produced is already excluded. In math it names `\cdots` between the operators chktex lists.

Fix by writing the command.

See [chktex 11](https://www.nongnu.org/chktex/), [The TeXbook § chapter 18](https://ctan.org/pkg/texbook).

### `group-local-definition`

**style / warning** — a definition is made inside a group and lost when it closes

`\def` and `\newcommand` are local. Inside `{…}`, `\begingroup…\endgroup` or an environment, the definition disappears at the closing brace.

Fix by moving the definition out of the group, or by prefixing it with `\global`.

See [The TeXbook § chapter 5](https://ctan.org/pkg/texbook).

### `math-operator-name`

**style / warning** — an operator name is set in math italic instead of by its command

Letters in math mode are set in math italic, one variable per letter, with the spacing of a product ([The TeXbook](https://ctan.org/pkg/texbook), chapter 18), so `$sin(x)$` sets a product of three variables.

satex decides this from the run, not from a list of names: the name the letters spell has to be a command this document has whose body sets exactly those letters. So `\argmax` from `\DeclareMathOperator` is suggested too, and `\mathrm{sin}` is left alone.

Fix by writing the command.

See [chktex 35](https://www.nongnu.org/chktex/), [The TeXbook § chapter 18](https://ctan.org/pkg/texbook).

### `math-shift-dollars`

**style / warning** — math opened with `$` or `$$` where LaTeX spells it \( or \[

`$$ … $$` and `$ … $` are plain TeX's displays. LaTeX spells them `\[ … \]` and `\( … \)`, which obey the `fleqn` and `leqno` class options and the class's `\everydisplay` ([source2e](https://ctan.org/pkg/source2e), ltmath.dtx).

Both spellings put the run in the same mode. satex takes the first character set in math mode, lexes the source before it with TeX's own mouth, and reports the math shift characters standing there, so math an environment opened is left alone.

Fix by writing `\[ … \]` and `\( … \)`.

See [chktex 45](https://www.nongnu.org/chktex/), [chktex 46](https://www.nongnu.org/chktex/), [source2e § ltmath.dtx](https://ctan.org/pkg/source2e).

### `missing-italic-correction`

**style / warning** — a slanted letter runs into upright text with no `\/` between them

A slanted letter leans over the space after it, and `\/` adds the kern the font asks for ([The TeXbook](https://ctan.org/pkg/texbook), chapter 4).

satex compares each character's `\f@shape` against the kernel's `\itdefault` and `\sldefault` ([fntguide § 2](https://ctan.org/pkg/fntguide)), and the character's italic correction in its own font says whether the kern is worth having. So `{\it …}`, `\emph{…}` and a shape a package selects are all seen alike, where chktex knows only a list of names.

Fix by writing `\/` after the last slanted character.

See [chktex 6](https://www.nongnu.org/chktex/), [The TeXbook § chapter 4](https://ctan.org/pkg/texbook), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21365).

### `missing-nonbreaking-space`

**style / warning** — an ordinary space in front of a reference lets the line break before the number

A reference should not begin a line on its own, so a tie belongs in front of it. `~` is an interword space TeX may not break at ([The TeXbook](https://ctan.org/pkg/texbook), chapter 14).

The rule reads the reference and citation sites the run recorded, so it follows a `\ref` inside a macro of yours without a list of names, and it reports only where the space really became glue ([tex.web § 1043][tw-1043]).

Fix by writing `Figure~\ref{fig:one}`.

See [chktex 2](https://www.nongnu.org/chktex/), [The TeXbook § chapter 14](https://ctan.org/pkg/texbook), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20358).

### `parenthesis-spacing`

**style / warning** — a parenthesis carries the space of the text inside it instead of the text around it

A parenthesis takes the space of the text around it, never of the text inside it: `text (inside) text`, not `text( inside )text`.

The rule reads the characters the run set, skips math, where `f(x)` is right, and reports a space inside the parentheses only where a real space token became glue. A missing space is the noisier half, because `word(s)` is ordinary English, so those findings come as notes.

Fix by moving the space to the other side of the parenthesis.

See [chktex 36](https://www.nongnu.org/chktex/), [chktex 37](https://www.nongnu.org/chktex/).

### `quote-direction`

**style / warning** — a quotation mark points the wrong way, or a run of them mixes both directions

A backtick opens a quotation and an apostrophe closes it ([The TeXbook](https://ctan.org/pkg/texbook), chapter 2). A mark the other way round sets the wrong glyph, and `` `' `` sets an empty quotation.

satex reads the direction from the characters around the mark in the typeset text. An apostrophe inside a word, an elision such as `'90s`, and a prime in math are left alone, and `` `{}` `` counts as two marks because the group stops the ligature.

Fix by turning the mark the right way round.

See [chktex 32](https://www.nongnu.org/chktex/), [chktex 33](https://www.nongnu.org/chktex/), [chktex 34](https://www.nongnu.org/chktex/), [The TeXbook § chapter 2](https://ctan.org/pkg/texbook).

### `sentence-spacing`

**style / warning** — a period set the wrong width of space for the sentence around it

How wide a space TeX sets follows the space factor of the character before it ([tex.web § 1034][tw-1034]). A period raises it to 3000, which asks for the wider space between sentences, but a period after an uppercase letter leaves it at 1000.

So `etc. and` gets a sentence space and `the ABC. Next` a word space, both the wrong way round. The rule reads the characters the run set and reports only where a real space token became glue.

Fix by writing `etc.\ and`, or `the ABC\@. Next`.

See [chktex 12](https://www.nongnu.org/chktex/), [chktex 13](https://www.nongnu.org/chktex/), [tex.web § 1034](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20114).

### `space-before-command`

**style / warning** — a space lets the line break between a word and the mark that belongs to it

`\label` records the page its own position falls on, and `\index` and `\footnote` mark the word they stand after ([source2e](https://ctan.org/pkg/source2e), ltxref.dtx). A space in front of one is glue TeX may break the line at, so the mark can land on the next line.

The rule finds the label sites the run recorded, so a `\label` your own macro calls counts too, and reports the space only where the run turned it into glue.

Fix by deleting the space, so the command touches the word.

See [chktex 24](https://www.nongnu.org/chktex/), [chktex 42](https://www.nongnu.org/chktex/), [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### `space-before-punctuation`

**style / warning** — a space stands between a word and the punctuation after it

Punctuation belongs against the word it follows. A space in front of a comma or a period is glue, and the reader sees it on the page.

The rule reports only a real space token that became glue in horizontal mode ([tex.web § 1043][tw-1043]). A thin space a language package inserts before `:` comes from that package's own code, which the run keeps out of the text, so French typography is not reported.

Fix by deleting the space.

See [chktex 26](https://www.nongnu.org/chktex/), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20358).

### `stray-italic-correction`

**style / warning** — a repeated italic correction, or one in front of a period or comma

`\/` appends a kern as wide as the italic correction of the last character on the current list ([The TeXbook](https://ctan.org/pkg/texbook), chapter 4). The rule reports two places where that kern does not belong: a second `\/` with nothing typeset in between, which finds a kern rather than a character ([tex.web § 1113][tw-1113]), and one in front of a period or a comma (chktex 28).

satex reads the characters the document set, so it sees through a closing brace: in `\textit{word\/}.` the next character set is the period.

Fix by deleting the `\/`.

See [chktex 5](https://www.nongnu.org/chktex/), [chktex 28](https://www.nongnu.org/chktex/), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21365).

### `unknown-suppress-code`

**style / warning** — a satex-disable comment names a code no rule has

A `satex-disable…`/`satex-enable` magic comment named a code that `satex lint --rules` does not list, most often a typo. It suppresses nothing, since matching a finding's own code is all these comments do.

Fix by correcting the code, or removing it if the rule was renamed or never existed.

### `user-pattern`

**style / warning** — the source matches a pattern the project asked to be warned about

chktex keeps two lists of its own in `chktexrc`: `UserWarn`, literal strings to report (warning 20), and `UserWarnRegex`, regular expressions (warning 44).

satex reads both from `lints.user-pattern` in `satex.yaml`, and both are empty by default. A regex is written as [the `regex` crate](https://docs.rs/regex) reads it, and one that does not compile is skipped. A match inside a comment or a verbatim body is not reported.

Fix by writing what the project's own rule asks for, or by removing the pattern from `satex.yaml`.

See [chktex 20](https://www.nongnu.org/chktex/), [chktex 44](https://www.nongnu.org/chktex/).

### `wrong-dash`

**style / warning** — a run of hyphens sets a dash of the wrong length for what stands around it

TeX's text fonts turn `-` into a hyphen, `--` into an en-dash and `---` into an em-dash ([The TeXbook](https://ctan.org/pkg/texbook), chapter 2).

The rule reads the characters the document set, following chktex's counts: one or three hyphens between letters, two between digits, three between spaces. `lints.wrong-dash` in `satex.yaml` changes them and its `except` list names spellings to leave alone. A source scan misses that `-{}-` sets two hyphens, the group having broken the ligature, and that every `-` in math is a minus sign.

Fix by writing the run of hyphens the surrounding text calls for.

See [chktex 8](https://www.nongnu.org/chktex/), [The TeXbook § chapter 2](https://ctan.org/pkg/texbook).

### `wrong-quote-character`

**style / warning** — a character that is not a quotation mark stands in for one

TeX sets a quotation from two backticks and two apostrophes, which the font's ligature program pairs into the marks ([The TeXbook](https://ctan.org/pkg/texbook), chapter 2).

What a straight `"` sets depends on the encoding, so the rule reads `\encodingdefault`: OT1 has a right double quote there, T1 and TU the typewriter one. `´` is an acute accent, not the apostrophe `'`. A prime in math is never reported, and the rule is silent outside an English `\languagename`.

Fix by opening with two backticks and closing with two apostrophes, or with `\enquote{…}` from csquotes.

See [chktex 18](https://www.nongnu.org/chktex/), [chktex 19](https://www.nongnu.org/chktex/), [The TeXbook § chapter 2](https://ctan.org/pkg/texbook).

### `zero-italic-correction`

**style / warning** — the italic correction is zero here, so `\/` changes nothing

`\/` appends a kern as wide as the italic correction the font gives the last character on the current list ([tex.web § 1113][tw-1113]). Where that correction is zero the `\/` does nothing.

Every character carries the font it was set in ([fntguide § 2](https://ctan.org/pkg/fntguide)), and satex reads the correction out of that font's TFM. That is a measurement, where chktex decides from a list of command names. In math, and after a space, the kern is zero whatever the font.

Fix by deleting the `\/`.

See [chktex 4](https://www.nongnu.org/chktex/), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21365), [The TeXbook § chapter 4](https://ctan.org/pkg/texbook).

### `build-bibliography-unneeded`

**style / info** — the build configures BibTeX or biber for a document without a bibliography

The latexmkrc sets `$bibtex_use`, `$bibtex` or `$biber`, but the document names no bibliography database (`\bibliography`, `\addbibresource`), so the setting never takes effect (latexmk(1), `$bibtex_use`).

Fix by removing the setting.

See [latexmk § $bibtex_use](https://ctan.org/pkg/latexmk).

### `build-engine-options`

**style / info** — the engine command could report errors as file:line and write SyncTeX data

`-file-line-error` makes the engine report `file:line: message` instead of `! message`, which editors and satex's log reading can jump to. `-synctex=1` writes the data that links the PDF back to the source.

Fix by adding them to the command, e.g. `$pdflatex = 'pdflatex -file-line-error -synctex=1 %O %S';`.

See [pdftex § -file-line-error](https://ctan.org/pkg/pdftex), [luatex § -synctex](https://ctan.org/pkg/luatex).

### `build-unused-custom-dependency`

**style / info** — a latexmk custom dependency that nothing in the document triggers

`add_cus_dep(from, to, …)` makes latexmk run a program when a file with the `from` extension is newer than the `to` file it produces (latexmk(1), "CUSTOM DEPENDENCIES"). The document writes no glossary or index, and no file with the `from` extension is in the project, so the rule never runs.

Fix by removing the rule, or keep it if another document of the project needs it.

See [latexmk § CUSTOM DEPENDENCIES](https://ctan.org/pkg/latexmk).

### `comment-marker`

**style / info** — a comment still carries a TODO-style marker

chktex's warning 22 prints every comment it finds, which is noise in a document of any size.

This rule reports only the comments carrying a marker that says the note is unfinished work. The defaults are `TODO`, `FIXME`, `XXX` and `HACK`, matched as whole words and case-sensitively, and `lints.comment-marker` in `satex.yaml` sets its own list. A `%` inside a body the run read verbatim is a character, not a comment.

Fix by doing what the note asks and deleting it.

See [chktex 22](https://www.nongnu.org/chktex/).

### `computer-modern-in-t1`

**style / info** — T1 text in Computer Modern relies on cm-super for scalable fonts

Computer Modern in T1 is the EC family, which TeX Live ships as scalable Type 1 fonts only through cm-super. Without it the PDF gets bitmap fonts that look blurred on screen. [Latin Modern](https://ctan.org/pkg/lm) is the same design with T1 glyphs of its own.

The rule reads the encoding and the family of every character the document set, so it fires on the characters really set in T1 `cmr`.

Fix by loading `\usepackage{lmodern}`, or by making sure cm-super is installed.

See [lmodern § Latin Modern](https://ctan.org/pkg/lm), [fntguide § 5](https://ctan.org/pkg/fntguide).

### `csquotes-available`

**style / info** — the document sets quotation marks by hand instead of loading csquotes

`\enquote{…}` sets the marks the current language wants, nests an inner quotation inside an outer one, and keeps one spelling for the whole document ([csquotes](https://ctan.org/pkg/csquotes) [§ 3.1](https://ctan.org/pkg/csquotes)).

The rule fires where the run set quotation marks itself and csquotes is not loaded. Apostrophes inside a word and primes in math do not count.

Fix by loading `\usepackage{csquotes}` and writing `\enquote{…}`.

See [chktex 18](https://www.nongnu.org/chktex/), [csquotes § 3.1](https://ctan.org/pkg/csquotes).

### `dead-definition`

**style / info** — a definition is replaced before anything uses it

The name is defined, and defined again, with no use in between: the first replacement text never reaches the typesetter. Both definitions are made on every path, so this is not a conditional fallback.

Fix by deleting the first definition, or by checking which one was meant.

### `duplicate-color-value`

**style / info** — two color names store the exact same color

Two `\definecolor`-family calls, at least one of them the document's own, store the same model and components under different names. Not wrong, but one name likely means to be the other.

Silent unless the run could read both colors back exactly.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### `hand-set-quantity`

**style / info** — a number and its unit are set by hand where siunitx would set them

`2ms` sets the number and the unit as one word, so the line may break between them and the unit is italic in math. The [SI brochure](https://www.bipm.org/en/publications/si-brochure) (9th ed., [§ 5.4.3](https://www.bipm.org/en/publications/si-brochure)) asks for a non-breaking space and an upright unit, and [siunitx](https://ctan.org/pkg/siunitx) does both.

The rule reads what the run typeset: a digit run followed by letters with no space token between them, where the unit is an SI symbol and the document set it itself.

Fix by loading `\usepackage{siunitx}` and writing `\qty{⟨number⟩}{⟨unit⟩}`.

See [SI brochure § 5.4.3](https://www.bipm.org/en/publications/si-brochure), [siunitx § \qty](https://ctan.org/pkg/siunitx).

### `math-punctuation`

**style / info** — punctuation sits on the wrong side of a formula

A formula is part of the sentence around it. Punctuation that ends the sentence goes after the closing delimiter of inline math, and inside a display, where it is set on the display's own line ([The TeXbook](https://ctan.org/pkg/texbook), chapter 18).

The rule reads the mode, so it treats every display alike, whether it came from `\[`, from `$$` or from an `equation` environment, where chktex follows only `\]` and `$$`.

Fix by moving the punctuation to the other side of the delimiter.

See [chktex 40](https://www.nongnu.org/chktex/), [The TeXbook § chapter 18](https://ctan.org/pkg/texbook).

### `microtype-available`

**style / info** — the engine can protrude characters and expand fonts, but microtype is not loaded

pdfTeX and LuaTeX writing PDF support character protrusion and font expansion ([pdfTeX manual](https://ctan.org/pkg/pdftex), `\pdfprotrudechars` and `\pdfadjustspacing`), which make margins look straighter and let paragraphs break with fewer hyphens and less uneven spacing. microtype turns both on ([microtype manual § 1](https://ctan.org/pkg/microtype)). The rule fires when both are still off at the end of the run. XeTeX only protrudes, and DVI output supports neither, so the rule stays silent there.

Fix by loading `\usepackage{microtype}`.

See [microtype § 1](https://ctan.org/pkg/microtype), [pdftex § \pdfprotrudechars](https://ctan.org/pkg/pdftex).

### `ot1-font-encoding`

**style / info** — accented letters are built with \accent because the text is set in OT1

OT1, LaTeX's default text encoding, has no accented letters: it builds each one with the `\accent` primitive (ot1enc.def, [fntguide § 5](https://ctan.org/pkg/fntguide)). TeX does not hyphenate a word holding an `\accent` ([The TeXbook](https://ctan.org/pkg/texbook), appendix H), and the PDF's text layer holds an accent and a letter instead of the character.

Every character carries the encoding it was set in, so a document whose default is T1 but which sets one stretch in OT1 is reported for that stretch alone.

Fix by loading `\usepackage[T1]{fontenc}`.

See [source2e § ot1enc.def](https://ctan.org/pkg/source2e), [fntguide § 5](https://ctan.org/pkg/fntguide), [The TeXbook § appendix H](https://ctan.org/pkg/texbook).

### `primitive-tex-command`

**style / info** — a plain TeX primitive is used where LaTeX has its own interface

chktex warning 41 (`You ought to not use primitive TeX in LaTeX code`) flags the primitives in chktexrc's `Primitives` list. satex reports one only where the name still means the primitive, and names the LaTeX command for the same job.

For `\csname` the job is the one the run saw the name formed for — defined, aliased, tested or used — and the suggestion is [etoolbox](https://ctan.org/pkg/etoolbox)'s command for it when the document has it.

Fix by using LaTeX's interface for the same job.

See [chktex 41](https://www.nongnu.org/chktex/), [l2tabu § 1](https://ctan.org/pkg/l2tabu).

### `quote-ligature`

**style / info** — three quotation marks in a row set a double and a single mark with nothing between them

Three backticks in a row set a left double quote followed by a left single quote, glued together ([The TeXbook](https://ctan.org/pkg/texbook), chapter 2). The reader cannot tell which quotation is the outer one.

satex counts only marks TeX really joined, so a group between two of them already keeps them apart and the rule says nothing.

Fix by putting a thin space `\,` between the double mark and the single one, with the double mark on the outside.

See [chktex 23](https://www.nongnu.org/chktex/), [The TeXbook § chapter 2](https://ctan.org/pkg/texbook).

### `quote-punctuation`

**style / info** — punctuation stands inside the closing quotation mark

Under the logical convention the closing mark comes directly after the quoted words, and punctuation that belongs to the sentence stands outside it. A full stop written before the closing mark puts it inside the quotation, which claims the stop was part of what was quoted. American practice is the other way round, which is why chktex has this check off by default and satex reports it as a suggestion.

Fix by moving the punctuation after the closing mark.

See [chktex 38](https://www.nongnu.org/chktex/).

### `stray-space`

**style / info** — a macro body picked up a space from an unescaped line end

Inside a macro's replacement text, TeX turns the end of a line into a space token unless a control word already skipped it or a `%` swallowed the rest of the line ([tex.web § 347][tw-347]).

The finding follows the modes the run read that space in ([§ 1043][tw-1043]: a space is glue only in horizontal mode). It says the macro *inserts* a space where every reading was horizontal, and that it *may* where some might have been.

Fix by writing `%` at the end of that line.

See [tex.web § 347](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L7326), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20358).

### `unreferenced-numbered-display`

**style / info** — a display that stepped a counter is never referenced, so its number is spent for nothing

`\begin{equation}` and its relatives step a counter and print a number beside the display. The starred forms set the same mathematics and spend no number.

satex decides *numbered* by running the document: `\refstepcounter` redefines `\@currentlabel` ([clsguide § counters](https://ctan.org/pkg/clsguide)), so an environment whose body redefined it stepped a counter. `\@captype` and `\@listctr` say the number went to something inside, and amsmath's `\notag`, `\nonumber` and `\tag` say the display governs itself.

Fix by writing the starred environment and dropping the label that nothing reads.

See [amsmath § 3.1](https://ctan.org/pkg/amsmath), [clsguide § counters](https://ctan.org/pkg/clsguide).

### `unused-bibitem`

**style / info** — a \bibitem written in the document is never cited

A `thebibliography` written by hand lists what it lists, cited or not. A database only contributes what is cited, so its entries are not reported. `\nocite{*}` cites everything.

Fix by citing the entry, or by deleting it.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### `unused-color`

**style / info** — a color the document defines is never used

The document names a color with `\definecolor`, `\providecolor` or `\colorlet` and never reads it back through `\color`, `\textcolor` or a `\colorlet` source.

Silent for a color a package defines: a library may define more colors than any one document uses.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### `unused-definition`

**style / info** — a macro defined in the document is never used

Nothing expands this name.

Fix by deleting it, or by checking why nothing calls it yet.

### `unused-label`

**style / info** — a label is never referenced

Fix by deleting the `\label`, or by checking the `\ref` that was meant to use it.

</details>

<details>
<summary><b>performance</b> — 6 rules</summary>

### `package-after-preamble`

**performance / warning** — a package is loaded after \begin{document}

`\usepackage` belongs in the preamble. After `\begin{document}` LaTeX raises `\usepackage before \begin{document}` and the package cannot set anything up.

Fix by moving the load into the preamble.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### `duplicate-package`

**performance / info** — a package is loaded more than once

The second request is ignored by LaTeX. It costs nothing at run time but it hides which load carries the options. A request with options the first did not have is an option clash instead (`option-clash`).

Fix by deleting the later `\usepackage`.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### `preamble-cost`

**performance / info** — what the preamble costs on every build

The packages the document asks for and the files read before `\begin{document}`.

A format holds the preamble already interpreted, so a build that starts from one skips all of it. [mylatexformat](https://ctan.org/pkg/mylatexformat) dumps one from the document's own preamble, and TeX reads a `%&`(format) line before anything else (web2c manual, "Format files").

Fix by dumping the preamble into a format and starting every build from it.

See [The LaTeX Companion § formats](https://ctan.org/pkg/tlc3-examples).

### `recursive-macro`

**performance / info** — a macro calls itself, directly or through others

Recursion is normal in TeX, but an unguarded one loops until TeX runs out of memory, and the analysis has to widen at it — everything the recursion would have defined becomes unknown.

Fix, if the recursion is unintended, by breaking the cycle. Otherwise make sure a conditional terminates it.

### `unused-dependency`

**performance / info** — the dependency file lists a TeX Live package no file of the run comes from

Every installation made from the dependency file carries the package, but the run reads nothing from it. It may still be needed for what satex does not read — fonts, hyphenation patterns, Lua libraries, programs — so this is a hint, not a verdict.

Fix, if the package really is unused, by deleting its line.

### `unused-package`

**performance / info** — a loaded package defines nothing the document uses

Every name the package defines is unused. The package may still be needed for its side effects — page layout, hooks, fonts, `\AtBeginDocument` code — so this is a hint, not a verdict.

Fix, if the package really is unused, by deleting the `\usepackage`: it is read on every build.

</details>

<details>
<summary><b>precision</b> — 1 rule</summary>

### `analysis-imprecision`

**precision / info** — the analysis had to widen

Undecided conditionals are analyzed arm by arm, so they are not reported here. This rule reports the two places where the analysis is incomplete instead: a recursion that was widened, and a budget that was reached. Findings that depend on such a point may be incomplete.

There is nothing to fix in the document. Raising the limits in `satex.yaml` or narrowing the analysis with `load.packages: false` can reduce it.

</details>

[tw-347]: https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L7326
[tw-1034]: https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20114
[tw-1043]: https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20358
[tw-1047]: https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L20410
[tw-1113]: https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L21365
[tw-1161]: https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L22057
