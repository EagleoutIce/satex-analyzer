<!-- generated from doc/src/wiki/lints.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Lint rules

`satex lint` checks the input file with every rule below. `--all` also checks the packages and classes it loads. `--rules` lists the codes, `--explain CODE` prints one rule's explanation. See [Chktex-Coverage](Chktex-Coverage) for the chktex warning each rule follows, and [Primitive-coverage](Primitive-coverage) and [Domains](Domains) for how satex gets the facts a rule reads.

## Errors caused in package code

An engine error can arise inside a package while the document's own call caused it. `satex lint` then shows it at that call, as `caused-library-error`. The message names the package line, e.g. `error in expl3-code.tex:24703: Argument has an extra }, caused here`. The call is the innermost place in the user's own files on the input stack. These are the main file, the files it reads, and other files below its directory. One row stands for each call line and error code, with a count when it repeats. An error on a path satex approximated may not be TeX's. It is a precision note, which the text output folds into a count and an editor does not show. An exact error after such a one may be its echo, so it is a warning that names the earlier one. A finding that something is absent from the whole document, such as an undefined reference or an unused label, is a precision note too when the run left a project file unread. `query diagnostics` gives the call as `origin`. A runaway over an argument the document itself leaves open is the document's error. It is a `raised-error` at the call, e.g. `File ended while scanning use of \pgfkeys@@set: the argument of \pgfkeys opened here never closes, the file ends inside it`.

## Suppressing findings

A comment in the document silences findings. Without codes it silences every rule, with codes (comma-separated) only those. `lint_off` in `satex.yaml` turns a rule off everywhere.

* `% satex-disable-next-line CODE,…` — the next line.
* `% satex-disable-line CODE,…` — at the end of a line, that line.
* `% satex-disable CODE,…` … `% satex-enable CODE,…` — a region.
* `% satex-disable-file CODE,…` — the whole file.

They apply to every output format and to `--fix`. An unknown code is reported as `unknown-suppress-code`.

## Rules

Every rule, with a link to its explanation below. The shape gives the severity: ◆ error, ▲ warning, ● info. The badge gives the category. The origin names the [chktex](Chktex-Coverage) warnings a rule follows.

| code | kind | quick fix | origin | summary |
| --- | --- | --- | --- | --- |
| [`already-defined`](#already-defined) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | <span class="fix fix-unsafe">unsafe</span> | — | `\newcommand` redefines an existing name |
| [`bibtex-command-under-biblatex`](#bibtex-command-under-biblatex) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | <span class="fix fix-unsafe">unsafe</span> | — | a BibTeX command biblatex replaces stops the run |
| [`build-bibliography-disabled`](#build-bibliography-disabled) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | the document has a bibliography, but the build never runs BibTeX or biber |
| [`build-command-placeholders`](#build-command-placeholders) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | an engine command in the latexmkrc lacks %S or %O |
| [`build-engine-mismatch`](#build-engine-mismatch) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | the build runs another engine, or another output, than the document needs |
| [`build-shell-escape-missing`](#build-shell-escape-missing) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | the document runs programs through `\write18`, but the build does not allow it |
| [`caused-library-error`](#caused-library-error) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | the document makes package code raise a TeX error |
| [`math-still-open`](#math-still-open) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | <a class="from" href="Chktex-Coverage" title="follows chktex warning 16">chktex 16</a> | the run is still in math mode where the text ends |
| [`missing-graphic`](#missing-graphic) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | an included image is not there for this output target |
| [`new-redefines`](#new-redefines) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | a `\new…` declaration silently redefines an existing command |
| [`not-defined`](#not-defined) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | <span class="fix fix-safe">safe</span> | — | `\renewcommand` targets a name that does not exist |
| [`option-clash`](#option-clash) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | <span class="fix fix-unsafe">unsafe</span> | — | a package is loaded twice with different options |
| [`raised-error`](#raised-error) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | the document's code raises a TeX error |
| [`undefined-color`](#undefined-color) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | a color is used, or aliased from, before xcolor ever defines it |
| [`undefined-control-sequence`](#undefined-control-sequence) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | a control sequence is used that nothing defines |
| [`undefined-environment`](#undefined-environment) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | `\begin` names an environment that is not defined |
| [`undefined-glossary-entry`](#undefined-glossary-entry) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | a glossary or acronym entry is used but never declared |
| [`unguarded-recursion`](#unguarded-recursion) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat">correctness</span> | — | — | a recursive macro has nothing that can stop it |
| [`build-missing-custom-dependency`](#build-missing-custom-dependency) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | the document writes glossary files, but latexmk has no rule to process them |
| [`catcode-escapes-group`](#catcode-escapes-group) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | a category code change is still in force at the end of the run |
| [`conflicting-color-definition`](#conflicting-color-definition) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | the same color name is defined twice with different colors |
| [`delimiter-not-followed`](#delimiter-not-followed) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 43">chktex 43</a> | `\left`, `\right` or `\middle` is followed by a brace or a math shift |
| [`duplicate-bibliography-entry`](#duplicate-bibliography-entry) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | the same key is declared by two bibliography entries |
| [`duplicate-label`](#duplicate-label) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | the same label is defined twice |
| [`environment-mismatch`](#environment-mismatch) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 9, 10, 15">chktex 9, 10, 15</a> | `\end` names an environment other than the one that is open |
| [`expl-syntax-left-on`](#expl-syntax-left-on) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | `\ExplSyntaxOn` is never turned off |
| [`expl3-constant-set`](#expl3-constant-set) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | an expl3 constant is set after it was declared |
| [`expl3-signature-mismatch`](#expl3-signature-mismatch) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | an expl3 function takes other arguments than its name says |
| [`ignored-verbatim-tail`](#ignored-verbatim-tail) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 31">chktex 31</a> | text stands on the line that closes a verbatim body and never reaches the page |
| [`math-script-braces`](#math-script-braces) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | <span class="fix fix-safe">safe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 3, 25">chktex 3, 25</a> | `^` and `_` take one token, so what follows needs braces |
| [`missing-dependency`](#missing-dependency) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | <span class="fix fix-safe">safe</span> | — | a TeX Live package the run reads from is not in the dependency file |
| [`unbalanced-brackets`](#unbalanced-brackets) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | <a class="from" href="Chktex-Coverage" title="follows chktex warning 17">chktex 17</a> | a file sets more of one bracket than of its partner |
| [`unbalanced-color-stack`](#unbalanced-color-stack) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | a `\pdfcolorstack` push is never popped, or a pop has nothing to pop |
| [`unbalanced-pdf-content`](#unbalanced-pdf-content) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | a PDF literal opens marked content or a graphics state that is never closed |
| [`undefined-citation`](#undefined-citation) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | a citation names a key no bibliography entry declares |
| [`undefined-optional-content`](#undefined-optional-content) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | marked content refers to an optional content group no resource names |
| [`undefined-reference`](#undefined-reference) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | `\ref` names a label that is never defined |
| [`uninstalled-package`](#uninstalled-package) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | — | the package is not installed, and the distribution could install it |
| [`unresolved-file`](#unresolved-file) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">correctness</span> | — | <a class="from" href="Chktex-Coverage" title="follows chktex warning 27">chktex 27</a> | a package, class or input file is not in the installation |
| [`expected-load`](#expected-load) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">correctness</span> | — | — | the file was read after a package it expects loaded |
| [`math-mode-mismatch`](#math-mode-mismatch) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">correctness</span> | — | <a class="from" href="Chktex-Coverage" title="follows chktex warning 49">chktex 49</a> | \(, \), \[ or \] stands where the mode does not allow it |
| [`unintended-escape`](#unintended-escape) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">correctness</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 21">chktex 21</a> | a command is followed by an escaped punctuation mark, which is another command |
| [`build-shell-escape-unneeded`](#build-shell-escape-unneeded) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">security</span> | — | — | the build allows `\write18`, but the document never uses it |
| [`shell-escape`](#shell-escape) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">security</span> | — | — | the document runs a program through `\write18` |
| [`accent-on-dotted-letter`](#accent-on-dotted-letter) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 7">chktex 7</a> | an accent sits on top of the dot of an `i` or a `j` |
| [`command-terminated-space`](#command-terminated-space) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 1">chktex 1</a> | a control word ate the space after it and the next word is set against what it produced |
| [`dimension-times`](#dimension-times) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 29">chktex 29</a> | the letter x stands between two numbers where a multiplication sign belongs |
| [`double-space`](#double-space) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 30, 39">chktex 30, 39</a> | two interword spaces reached the page, one of them from a tie |
| [`ellipsis-dots`](#ellipsis-dots) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 11">chktex 11</a> | three periods set three periods, not an ellipsis |
| [`expl3-scope`](#expl3-scope) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | — | — | an expl3 variable's scope prefix disagrees with how the run assigned it |
| [`expl3-type`](#expl3-type) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | — | — | an expl3 variable's type suffix disagrees with the functions that set it |
| [`group-local-definition`](#group-local-definition) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | — | — | a definition is made inside a group and lost when it closes |
| [`math-operator-name`](#math-operator-name) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 35">chktex 35</a> | an operator name is set in math italic instead of by its command |
| [`missing-italic-correction`](#missing-italic-correction) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 6">chktex 6</a> | a slanted letter runs into upright text with no `\/` between them |
| [`missing-nonbreaking-space`](#missing-nonbreaking-space) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 2">chktex 2</a> | an ordinary space in front of a reference lets the line break before the number |
| [`parenthesis-spacing`](#parenthesis-spacing) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 36, 37">chktex 36, 37</a> | a parenthesis carries the space of the text inside it instead of the text around it |
| [`quote-direction`](#quote-direction) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 32, 33, 34">chktex 32, 33, 34</a> | a quotation mark points the wrong way, or a run of them mixes both directions |
| [`sentence-spacing`](#sentence-spacing) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 12, 13">chktex 12, 13</a> | a period set the wrong width of space for the sentence around it |
| [`space-before-command`](#space-before-command) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 24, 42">chktex 24, 42</a> | a space lets the line break between a word and the mark that belongs to it |
| [`space-before-punctuation`](#space-before-punctuation) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 26">chktex 26</a> | a space stands between a word and the punctuation after it |
| [`stray-italic-correction`](#stray-italic-correction) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 5, 28">chktex 5, 28</a> | a repeated italic correction, or one in front of a period or comma |
| [`unknown-suppress-code`](#unknown-suppress-code) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | — | — | a satex-disable comment names a code no rule has |
| [`user-pattern`](#user-pattern) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | — | <a class="from" href="Chktex-Coverage" title="follows chktex warning 20, 44">chktex 20, 44</a> | the source matches a pattern the project asked to be warned about |
| [`wrong-dash`](#wrong-dash) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 8">chktex 8</a> | a run of hyphens sets a dash of the wrong length for what stands around it |
| [`wrong-quote-character`](#wrong-quote-character) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 18, 19">chktex 18, 19</a> | a character that is not a quotation mark stands in for one |
| [`zero-italic-correction`](#zero-italic-correction) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 4">chktex 4</a> | the italic correction is zero here, so `\/` changes nothing |
| [`build-bibliography-unneeded`](#build-bibliography-unneeded) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | the build configures BibTeX or biber for a document without a bibliography |
| [`build-engine-options`](#build-engine-options) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | the engine command could report errors as file:line and write SyncTeX data |
| [`build-unused-custom-dependency`](#build-unused-custom-dependency) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | a latexmk custom dependency that nothing in the document triggers |
| [`comment-marker`](#comment-marker) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | <a class="from" href="Chktex-Coverage" title="follows chktex warning 22">chktex 22</a> | a comment still carries a TODO-style marker |
| [`computer-modern-in-t1`](#computer-modern-in-t1) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | — | T1 text in Computer Modern relies on cm-super for scalable fonts |
| [`csquotes-available`](#csquotes-available) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 18">chktex 18</a> | the document sets quotation marks by hand instead of loading csquotes |
| [`dead-definition`](#dead-definition) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span> | — | a definition is replaced before anything uses it |
| [`duplicate-color-value`](#duplicate-color-value) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | two color names store the exact same color |
| [`duplicate-package`](#duplicate-package) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> | — | a package is loaded more than once |
| [`hand-set-quantity`](#hand-set-quantity) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | — | a number and its unit are set by hand where siunitx would set them |
| [`inconsistent-precision`](#inconsistent-precision) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | the numbers of a table column give different numbers of decimals |
| [`math-punctuation`](#math-punctuation) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | <a class="from" href="Chktex-Coverage" title="follows chktex warning 40">chktex 40</a> | punctuation sits on the wrong side of a formula |
| [`math-shift-dollars`](#math-shift-dollars) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 45, 46">chktex 45, 46</a> | display math opened with `$$` where LaTeX spells it \[ |
| [`microtype-available`](#microtype-available) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | — | the engine can protrude characters and expand fonts, but microtype is not loaded |
| [`numbers-not-aligned`](#numbers-not-aligned) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | a table column of numbers does not line them up at their decimal mark |
| [`ot1-font-encoding`](#ot1-font-encoding) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | — | accented letters are built with `\accent` because the text is set in OT1 |
| [`prefer-cleveref`](#prefer-cleveref) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | — | a hand-written name in front of `\ref` is what cleveref sets from the label itself |
| [`primitive-tex-command`](#primitive-tex-command) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 41">chktex 41</a> | a plain TeX primitive is used where LaTeX has its own interface |
| [`quote-ligature`](#quote-ligature) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 23">chktex 23</a> | three quotation marks in a row set a double and a single mark with nothing between them |
| [`quote-punctuation`](#quote-punctuation) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | <a class="from" href="Chktex-Coverage" title="follows chktex warning 38">chktex 38</a> | punctuation stands inside the closing quotation mark |
| [`stray-space`](#stray-space) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-safe">safe</span> | — | a macro body picked up a space from an unescaped line end |
| [`table-rules`](#table-rules) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | a table is ruled with vertical rules or `\hline` |
| [`unit-in-cells`](#unit-in-cells) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | every number of a table column carries the same unit |
| [`unreferenced-numbered-display`](#unreferenced-numbered-display) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | — | a display that stepped a counter is never referenced, so its number is spent for nothing |
| [`unused-bibitem`](#unused-bibitem) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | a `\bibitem` written in the document is never cited |
| [`unused-color`](#unused-color) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | — | — | a color the document defines is never used |
| [`unused-definition`](#unused-definition) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | — | a macro defined in the document is never used |
| [`unused-label`](#unused-label) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">style</span> | <span class="fix fix-unsafe">unsafe</span> | — | a label is never referenced |
| [`package-after-preamble`](#package-after-preamble) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat">performance</span> | <span class="fix fix-unsafe">unsafe</span> | — | a package is loaded after `\begin{document}` |
| [`includeonly-available`](#includeonly-available) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">performance</span> | <span class="fix fix-unsafe">unsafe</span> | — | a document of several `\include`d parts is built whole |
| [`preamble-cost`](#preamble-cost) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">performance</span> | <span class="fix fix-unsafe">unsafe</span> | — | what the preamble costs on every build |
| [`recursive-macro`](#recursive-macro) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">performance</span> | — | — | a macro calls itself, directly or through others |
| [`unused-dependency`](#unused-dependency) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">performance</span> | <span class="fix fix-unsafe">unsafe</span> | — | the dependency file lists a TeX Live package no file of the run comes from |
| [`unused-package`](#unused-package) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">performance</span> | <span class="fix fix-unsafe">unsafe</span> | — | a loaded package defines nothing the document uses |
| [`analysis-imprecision`](#analysis-imprecision) | <span class="sev sev-info" title="default: info">●</span> <span class="cat">precision</span> | — | — | the analysis had to widen |

## Quick-fixes

A finding's `fix` says what to do. Where that is mechanical it carries the edits, e.g. removing a `\usepackage` line or one name in `\usepackage{a,b,c}`, a `\label{…}`, or turning `\newcommand` into `\renewcommand`. The text output marks them `[fix]` or `[unsafe fix]`:

* **safe** fixes keep the document's meaning: a duplicate `\usepackage`, a definition replaced before it is read, `\renewcommand` of an undefined name;
* **unsafe** fixes change it and deserve a look: removing a package that may have side effects, an unused label, `\newcommand` turned into `\renewcommand`.

`--fix` applies the safe fixes, `--unsafe-fixes` adds the unsafe ones, `--diff` prints the edits instead of writing them. Only project files change. Overlapping fixes wait for the next round. The document is re-analyzed after each round, up to eight.

`--format json` gives each fix as `applicability` and `edits` (`path`, 1-based `start`/`end`, `replacement`), `--format sarif` as `fixes`, `--format lsp` as diagnostics with `quickfix` code actions.

These rules have quick fixes:

* [`already-defined`](#already-defined) <span class="fix fix-unsafe">unsafe</span> — `\newcommand` redefines an existing name
* [`bibtex-command-under-biblatex`](#bibtex-command-under-biblatex) <span class="fix fix-unsafe">unsafe</span> — a BibTeX command biblatex replaces stops the run
* [`not-defined`](#not-defined) <span class="fix fix-safe">safe</span> — `\renewcommand` targets a name that does not exist
* [`option-clash`](#option-clash) <span class="fix fix-unsafe">unsafe</span> — a package is loaded twice with different options
* [`delimiter-not-followed`](#delimiter-not-followed) <span class="fix fix-unsafe">unsafe</span> — `\left`, `\right` or `\middle` is followed by a brace or a math shift
* [`environment-mismatch`](#environment-mismatch) <span class="fix fix-unsafe">unsafe</span> — `\end` names an environment other than the one that is open
* [`ignored-verbatim-tail`](#ignored-verbatim-tail) <span class="fix fix-unsafe">unsafe</span> — text stands on the line that closes a verbatim body and never reaches the page
* [`math-script-braces`](#math-script-braces) <span class="fix fix-safe">safe</span> — `^` and `_` take one token, so what follows needs braces
* [`missing-dependency`](#missing-dependency) <span class="fix fix-safe">safe</span> — a TeX Live package the run reads from is not in the dependency file
* [`unintended-escape`](#unintended-escape) <span class="fix fix-unsafe">unsafe</span> — a command is followed by an escaped punctuation mark, which is another command
* [`accent-on-dotted-letter`](#accent-on-dotted-letter) <span class="fix fix-unsafe">unsafe</span> — an accent sits on top of the dot of an `i` or a `j`
* [`command-terminated-space`](#command-terminated-space) <span class="fix fix-safe">safe</span> — a control word ate the space after it and the next word is set against what it produced
* [`dimension-times`](#dimension-times) <span class="fix fix-unsafe">unsafe</span> — the letter x stands between two numbers where a multiplication sign belongs
* [`double-space`](#double-space) <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span> — two interword spaces reached the page, one of them from a tie
* [`ellipsis-dots`](#ellipsis-dots) <span class="fix fix-unsafe">unsafe</span> — three periods set three periods, not an ellipsis
* [`math-operator-name`](#math-operator-name) <span class="fix fix-unsafe">unsafe</span> — an operator name is set in math italic instead of by its command
* [`missing-italic-correction`](#missing-italic-correction) <span class="fix fix-unsafe">unsafe</span> — a slanted letter runs into upright text with no `\/` between them
* [`missing-nonbreaking-space`](#missing-nonbreaking-space) <span class="fix fix-unsafe">unsafe</span> — an ordinary space in front of a reference lets the line break before the number
* [`parenthesis-spacing`](#parenthesis-spacing) <span class="fix fix-unsafe">unsafe</span> — a parenthesis carries the space of the text inside it instead of the text around it
* [`quote-direction`](#quote-direction) <span class="fix fix-unsafe">unsafe</span> — a quotation mark points the wrong way, or a run of them mixes both directions
* [`sentence-spacing`](#sentence-spacing) <span class="fix fix-unsafe">unsafe</span> — a period set the wrong width of space for the sentence around it
* [`space-before-command`](#space-before-command) <span class="fix fix-unsafe">unsafe</span> — a space lets the line break between a word and the mark that belongs to it
* [`space-before-punctuation`](#space-before-punctuation) <span class="fix fix-safe">safe</span> — a space stands between a word and the punctuation after it
* [`stray-italic-correction`](#stray-italic-correction) <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span> — a repeated italic correction, or one in front of a period or comma
* [`wrong-dash`](#wrong-dash) <span class="fix fix-unsafe">unsafe</span> — a run of hyphens sets a dash of the wrong length for what stands around it
* [`wrong-quote-character`](#wrong-quote-character) <span class="fix fix-unsafe">unsafe</span> — a character that is not a quotation mark stands in for one
* [`zero-italic-correction`](#zero-italic-correction) <span class="fix fix-safe">safe</span> — the italic correction is zero here, so `\/` changes nothing
* [`computer-modern-in-t1`](#computer-modern-in-t1) <span class="fix fix-unsafe">unsafe</span> — T1 text in Computer Modern relies on cm-super for scalable fonts
* [`csquotes-available`](#csquotes-available) <span class="fix fix-unsafe">unsafe</span> — the document sets quotation marks by hand instead of loading csquotes
* [`dead-definition`](#dead-definition) <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span> — a definition is replaced before anything uses it
* [`duplicate-package`](#duplicate-package) <span class="fix fix-safe">safe</span> — a package is loaded more than once
* [`hand-set-quantity`](#hand-set-quantity) <span class="fix fix-unsafe">unsafe</span> — a number and its unit are set by hand where siunitx would set them
* [`math-shift-dollars`](#math-shift-dollars) <span class="fix fix-safe">safe</span> — display math opened with `$$` where LaTeX spells it \[
* [`microtype-available`](#microtype-available) <span class="fix fix-unsafe">unsafe</span> — the engine can protrude characters and expand fonts, but microtype is not loaded
* [`ot1-font-encoding`](#ot1-font-encoding) <span class="fix fix-unsafe">unsafe</span> — accented letters are built with `\accent` because the text is set in OT1
* [`prefer-cleveref`](#prefer-cleveref) <span class="fix fix-unsafe">unsafe</span> — a hand-written name in front of `\ref` is what cleveref sets from the label itself
* [`primitive-tex-command`](#primitive-tex-command) <span class="fix fix-unsafe">unsafe</span> — a plain TeX primitive is used where LaTeX has its own interface
* [`quote-ligature`](#quote-ligature) <span class="fix fix-unsafe">unsafe</span> — three quotation marks in a row set a double and a single mark with nothing between them
* [`quote-punctuation`](#quote-punctuation) <span class="fix fix-unsafe">unsafe</span> — punctuation stands inside the closing quotation mark
* [`stray-space`](#stray-space) <span class="fix fix-safe">safe</span> — a macro body picked up a space from an unescaped line end
* [`unreferenced-numbered-display`](#unreferenced-numbered-display) <span class="fix fix-unsafe">unsafe</span> — a display that stepped a counter is never referenced, so its number is spent for nothing
* [`unused-definition`](#unused-definition) <span class="fix fix-unsafe">unsafe</span> — a macro defined in the document is never used
* [`unused-label`](#unused-label) <span class="fix fix-unsafe">unsafe</span> — a label is never referenced
* [`package-after-preamble`](#package-after-preamble) <span class="fix fix-unsafe">unsafe</span> — a package is loaded after `\begin{document}`
* [`includeonly-available`](#includeonly-available) <span class="fix fix-unsafe">unsafe</span> — a document of several `\include`d parts is built whole
* [`preamble-cost`](#preamble-cost) <span class="fix fix-unsafe">unsafe</span> — what the preamble costs on every build
* [`unused-dependency`](#unused-dependency) <span class="fix fix-unsafe">unsafe</span> — the dependency file lists a TeX Live package no file of the run comes from
* [`unused-package`](#unused-package) <span class="fix fix-unsafe">unsafe</span> — a loaded package defines nothing the document uses

## Explanations

One group per category. Open a group to read its rules.

<details>
<summary><b>correctness</b> — 42 rules</summary>

### already-defined

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span> <span class="fix fix-unsafe">unsafe</span></p>

**`\newcommand` redefines an existing name**

`\newcommand` refuses to redefine and raises `Command … already defined`.

Fix with `\renewcommand` if the redefinition is intended, or pick another name.

See [source2e § ltdefns.dtx](https://ctan.org/pkg/source2e).

### bibtex-command-under-biblatex

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span> <span class="fix fix-unsafe">unsafe</span></p>

**a BibTeX command biblatex replaces stops the run**

biblatex makes the commands of a BibTeX bibliography raise errors where it has its own: the style is its `style` option, not `\bibliographystyle`, and the databases are named in the preamble with `\addbibresource` and printed with `\printbibliography`, not with `\bibliography` ([biblatex manual, §3.7.1](https://ctan.org/pkg/biblatex) and [§3.7.2](https://ctan.org/pkg/biblatex)). Only a call the run reached and that raised the error is reported.

Fix by deleting `\bibliographystyle` (choose a style with `\usepackage[style=…]{biblatex}`), and by writing `\printbibliography` for `\bibliography` with an `\addbibresource` per database in the preamble.

See [biblatex § §3.7](https://ctan.org/pkg/biblatex).

### build-bibliography-disabled

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**the document has a bibliography, but the build never runs BibTeX or biber**

`$bibtex_use = 0` tells latexmk never to run BibTeX or biber (latexmk(1), `$bibtex_use`), so the `.bbl` file the bibliography is read from is never made and every citation stays undefined.

Fix by removing the setting. latexmk's default, 1, runs them whenever the `.bib` files exist.

See [latexmk § $bibtex_use](https://ctan.org/pkg/latexmk).

### build-command-placeholders

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**an engine command in the latexmkrc lacks %S or %O**

latexmk substitutes `%S` with the source file and `%O` with the options it passes itself (latexmk(1), "FORMAT OF COMMAND SPECIFICATIONS"). A command without `%S` compiles nothing, and one without `%O` loses `-jobname`, `-output-directory` and the options given on latexmk's own command line.

Fix by ending the command with `%O %S`.

See [latexmk § FORMAT OF COMMAND SPECIFICATIONS](https://ctan.org/pkg/latexmk).

### build-engine-mismatch

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**the build runs another engine, or another output, than the document needs**

latexmk's `$pdf_mode` picks the engine: 1 runs `$pdflatex`, 4 `$lualatex`, 5 `$xelatex`, and 0, 2 and 3 run `$latex` for DVI (latexmk(1), `$pdf_mode`). The document contradicts it when a `% !TeX program` line names another engine, when it uses a primitive only another engine has (`\directlua` is LuaTeX's, `\XeTeXrevision` XeTeX's), or when it sets `\pdfoutput` to the other output than latexmk waits for.

Fix by setting `$pdf_mode` to the engine the document is written for.

See [latexmk § $pdf_mode](https://ctan.org/pkg/latexmk).

### build-shell-escape-missing

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**the document runs programs through `\write18`, but the build does not allow it**

The run reached `\write18`, which only hands its text to the shell when the engine is started with `-shell-escape` (or `-shell-restricted` and a program TeX Live allows). latexmk starts the engine with the command in `$pdflatex`, `$lualatex`, `$xelatex` or `$latex`, whichever `$pdf_mode` selects (latexmk(1), `$pdf_mode`). arara's `pdflatex`, `lualatex` and `xelatex` rules take `shell: yes` ([arara manual, "The official rules"](https://ctan.org/pkg/arara)).

Fix by adding `-shell-escape` to the command, e.g. `$pdflatex = 'pdflatex -shell-escape %O %S';`, or `set_tex_cmds('-shell-escape %O %S');` for every engine at once.

See [latexmk § $pdf_mode](https://ctan.org/pkg/latexmk), [arara § The official rules](https://ctan.org/pkg/arara).

### caused-library-error

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**the document makes package code raise a TeX error**

An engine error such as `Argument has an extra }` arises inside package code. The cause is the document's own call into that code. The finding sits at that call and names the package line where the error arose. A runaway over the document's own unclosed argument is not one: `raised-error` reports it. Each call and error code is one finding, with a count when it repeats.

When satex had to approximate on the way, the error may not be TeX's: it is a precision note, not a finding. An exact error after such a one may be its echo, as TeX's recovery left a state that may not be TeX's. The finding is then a warning that names the earlier error.

See [The TeXbook, chapter 27](https://ctan.org/pkg/texbook#chapter-27).

### math-still-open

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 16">chktex 16</a></p>

**the run is still in math mode where the text ends**

A `$` that is never closed leaves TeX in math mode. The next `\par` makes TeX report `Missing $ inserted` and close the formula itself ([tex.web § 1047](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20410) `insert_dollar_sign`), and everything up to there is set as mathematics.

The rule reports what the interpreter already saw: the `missing-dollar` diagnostic it raised, or, where no `\par` followed, a last character still carrying a math mode (unless a limit stopped the run before the text's end). It reports one finding, never both.

Fix by closing the formula.

See [chktex 16](https://www.nongnu.org/chktex/), [tex.web § 1047](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20410).

### missing-graphic

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**an included image is not there for this output target**

`\includegraphics` names a file that graphicx cannot find, with any extension in `\Gin@extensions`, beside the document or on `\graphicspath`. Which extensions count is the driver's (pdftex.def, dvips.def, xetex.def), so it follows the output.

Fix by adding the file, or by producing the format the driver reads.

See [graphics § pdftex.def](https://ctan.org/pkg/graphics).

### new-redefines

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**a `\new…` declaration silently redefines an existing command**

A LaTeX `\new…` command declares a fresh name, and some do not check that it is: `\newif\begin` makes `\begin` a conditional (and defines `\gintrue` and `\ginfalse` from what follows its first two letters), `\newcount` and `\newtoks` overwrite whatever the name meant. Every later use of the name then does something else, and the error a run reports is somewhere after it, often far away. Reported where the document's own `\new…` call made the definition of a name its own files spell.

Fix by giving the declaration the name it was meant to have (a conditional's name begins with `if`), or by declaring the argument it lacks.

See [source2e § ltdefns.dtx](https://ctan.org/pkg/source2e).

### not-defined

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span> <span class="fix fix-safe">safe</span></p>

**`\renewcommand` targets a name that does not exist**

`\renewcommand` requires an existing definition and raises `Undefined control sequence`.

Fix with `\newcommand`, or load the package that was supposed to define it.

See [source2e § ltdefns.dtx](https://ctan.org/pkg/source2e).

### option-clash

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span> <span class="fix fix-unsafe">unsafe</span></p>

**a package is loaded twice with different options**

LaTeX loads a package once. A second `\usepackage` with options the first one did not have raises `Option clash for package`.

Fix by giving all the options to the first load, or by using `\PassOptionsToPackage` before it.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### raised-error

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**the document's code raises a TeX error**

Running the document reaches `\errmessage`, which every LaTeX error (`\PackageError`, `\msg_error:nn`, the kernel's own) ends in: a real run stops there. A more specific rule that explains the same error reports it instead. An error reached only on a path satex could not decide, or after it lost track of the mode, is not one a real run need reach, and is not reported. A runaway in package code over an argument the document leaves open is the document's error too: it is shown at the call that opens it.

### undefined-color

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**a color is used, or aliased from, before xcolor ever defines it**

xcolor reaches every color through `\color@⟨name⟩` ([xcolor § 2.5](https://ctan.org/pkg/xcolor)). A name the run never defined there raises `Undefined color` when xcolor itself looks it up. The rule reads the same lookup the run made: a name a `\color`, `\textcolor` or `\colorlet` reached while nothing had defined it.

Fix by defining the color first, or by correcting its name.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### undefined-control-sequence

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**a control sequence is used that nothing defines**

The name has no meaning at the point it is used, so a real run would stop with `Undefined control sequence`. When the message says "not on every path", the definition sits in a conditional arm that this use does not depend on.

Fix by loading the package that provides it, defining it before the use, or correcting the spelling.

See [The TeXbook, chapter 27](https://ctan.org/pkg/texbook#chapter-27).

### undefined-environment

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**`\begin` names an environment that is not defined**

`\begin{name}` expands `\name`, and nothing defines it. A real run reports `Environment name undefined`.

Fix by loading the package that provides the environment or by defining it with `\newenvironment{name}{…}{…}`.

See [source2e § ltmiscen.dtx](https://ctan.org/pkg/source2e).

### undefined-glossary-entry

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**a glossary or acronym entry is used but never declared**

`\gls` and its relatives name an entry that no `\newglossaryentry`, `\newacronym`, `\acro` or `\DeclareAcronym` declares. A real run stops with `Glossary entry ... has not been defined`.

Fix by declaring the entry, or by correcting the key.

See [glossaries § Defining glossary entries](https://ctan.org/pkg/glossaries).

### unguarded-recursion

<p class="badges"><span class="sev sev-error" title="default: error">◆ default: error</span> <span class="cat">correctness</span></p>

**a recursive macro has nothing that can stop it**

The run saw the macro's expansion reach the macro again, could not see the recursion end, and found no conditional — no `\if…`, `\else` or `\fi` — to end it. A macro nothing expands is judged by the names its replacement text holds. A real run expands until TeX runs out of memory: `TeX capacity exceeded, sorry [main memory size]`.

Fix by adding the test that stops it, or by removing the self-reference.

### build-missing-custom-dependency

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**the document writes glossary files, but latexmk has no rule to process them**

latexmk runs BibTeX, biber and makeindex by itself, but a glossary is sorted by a program it does not know about (`makeglossaries`, `makeindex` with a glossary style, `bib2gls`). Without a custom dependency (latexmk(1), "CUSTOM DEPENDENCIES") the glossary files are never made and the glossary stays empty. The glossaries manual gives the rule, e.g.

```text
add_cus_dep('glo', 'gls', 0, 'makeglossaries');
sub makeglossaries { system("makeglossaries '$_[0]'"); }
```

See [latexmk § CUSTOM DEPENDENCIES](https://ctan.org/pkg/latexmk), [glossaries § bib2gls](https://ctan.org/pkg/glossaries).

### catcode-escapes-group

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**a category code change is still in force at the end of the run**

`\catcode` is local, so a change made inside a group is undone by the closing brace. One still in force at the end of the document was made at the outer level, and everything read after it is tokenized differently.

Fix by wrapping the change in `{…}` or `\begingroup…\endgroup`, or by restoring the old value explicitly. `satex query catcodes` shows the current table.

See [The TeXbook, chapter 5](https://ctan.org/pkg/texbook#chapter-5).

### conflicting-color-definition

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**the same color name is defined twice with different colors**

Two `\definecolor`-family calls store different colors under the same name. Only the last one is ever read back.

Fix by giving the second color its own name, or by removing the earlier definition.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### delimiter-not-followed

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 43">chktex 43</a></p>

**`\left`, `\right` or `\middle` is followed by a brace or a math shift**

`\left`, `\right` and `\middle` scan a delimiter: one character with a `\delcode`, or one control sequence made by `\delimiter` ([tex.web § 1161](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L22057)). A `{`, a `}` or a `$` is none of those, so TeX reports `Missing delimiter (. inserted)` and sets nothing.

The rule asks what each command means rather than matching its name, so it covers `\middle` and any `\let` alias, which chktex's name list does not.

Fix by writing `\{` and `\}` for brace delimiters, or `\left.` where no delimiter is wanted.

See [chktex 43](https://www.nongnu.org/chktex/), [tex.web § 1161](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L22057).

### duplicate-bibliography-entry

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**the same key is declared by two bibliography entries**

Two `\bibitem`s with one key make LaTeX warn `Label … multiply defined` and every citation resolves to the later one. Two `.bib` entries with one key make BibTeX stop with `Repeated entry`.

Fix by renaming or deleting one of them.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### duplicate-label

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**the same label is defined twice**

LaTeX warns `Label … multiply defined` and every reference resolves to the later one. The finding stands at the innermost command of the document's text that writes the label: the `\begin` inside a `\sbox` argument, not the `\sbox`. Only the kernel's `\newlabel` (and zref's) warn: a name another package writes to the `.aux` file twice, such as a tikzmark, is not checked.

Fix by renaming one of them.

See [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### environment-mismatch

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 9, 10, 15">chktex 9, 10, 15</a></p>

**`\end` names an environment other than the one that is open**

Environments nest like brackets: the name `\end` gives has to match the one `\begin` most recently opened. chktex tracks this as warnings 9 (`'%s' expected, found '%s'`), 10 (`Solo '%s' found`) and 15 (`No match found for '%s'`) for brackets and environments alike. satex only records `\begin`/`\end` occurrences, so this rule covers environments and not generic `{}`, `[]` or `()` nesting.

An error raised inside the environment before the `\end` may have ended it early, through TeX's recovery or the analysis's approximation of its input. The finding is then a note that names that error, as is one the analysis reached on only some of its paths.

Fix by naming the environment that is actually open, or by closing the inner environments first.

See [chktex 9](https://www.nongnu.org/chktex/), [chktex 10](https://www.nongnu.org/chktex/), [chktex 15](https://www.nongnu.org/chktex/), [source2e § ltmiscen.dtx](https://ctan.org/pkg/source2e).

### expl-syntax-left-on

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**`\ExplSyntaxOn` is never turned off**

With expl3 syntax on, spaces are ignored and `_` and `:` are letters, so ordinary text stops working.

Fix by adding `\ExplSyntaxOff` after the code that needs it.

See [source2e § ltfiles.dtx](https://ctan.org/pkg/source2e).

### expl3-constant-set

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**an expl3 constant is set after it was declared**

A `\c_…` variable is a constant ([interface3](https://ctan.org/pkg/l3kernel)): it is made once, by `\…_const:N…` or `\…_new:N`, and never set. The run set this one again.

Fix by making it a variable (`\l_…` or `\g_…`) or by not changing it.

See [interface3 § Naming functions and variables](https://ctan.org/pkg/l3kernel).

### expl3-signature-mismatch

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**an expl3 function takes other arguments than its name says**

An expl3 name carries its signature after the colon ([interface3](https://ctan.org/pkg/expl3), "Naming conventions"): one letter per argument, `N` a single token, `n` a braced group.

What the function really consumes — the calls the run saw, or probe input for one the document defines — takes a different number of arguments, or an optional one or a star, which no expl3 signature has. `\foo:nn` that takes one argument leaves the second for whatever follows.

Fix by renaming the function to the signature it has, or its parameter text.

See [interface3 § Naming conventions](https://ctan.org/pkg/l3kernel).

### ignored-verbatim-tail

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 31">chktex 31</a></p>

**text stands on the line that closes a verbatim body and never reaches the page**

A verbatim environment that reads its body line by line throws away whatever follows `\end{…}` on the same line. `fancyvrb` does this and the kernel's `verbatim` does not, so chktex reports warning 31 for both.

satex reports only where the text was really lost. It knows which environments the run read verbatim, because opening one changed the category code of `\`, and reports the tail only where the run set none of it.

Fix by moving the text to the next line.

See [chktex 31](https://www.nongnu.org/chktex/).

### math-script-braces

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span> <span class="fix fix-safe">safe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 3, 25">chktex 3, 25</a></p>

**`^` and `_` take one token, so what follows needs braces**

`^` and `_` attach the single next token to the atom before them ([The TeXbook, chapter 16](https://ctan.org/pkg/texbook#chapter-16)). `x^12` sets the exponent 1 and then a 2 of its own size, and `(a+b)^2` puts the exponent on the closing parenthesis alone.

The rule lexes the source with TeX's own mouth where the run really was in math mode, so a `^` under another catcode regime is not one, and `\left( … \right)^2` is one atom the script attaches to.

Fix by putting the script between `{` and `}`.

See [chktex 3](https://www.nongnu.org/chktex/), [chktex 25](https://www.nongnu.org/chktex/), [The TeXbook, chapter 16](https://ctan.org/pkg/texbook#chapter-16).

### missing-dependency

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span> <span class="fix fix-safe">safe</span></p>

**a TeX Live package the run reads from is not in the dependency file**

depp names the TeX Live package of every file a run reads by where it sits in the tree (`tex/⟨format⟩/⟨package⟩/`) and writes them to `DEPENDS.txt`. The project's dependency file lacks one of them, so an installation made from it — a minimal Docker image, a `texlive.withPackages` — cannot build the document.

Fix by adding the package to the dependency file, or by rerunning depp.

### unbalanced-brackets

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 17">chktex 17</a></p>

**a file sets more of one bracket than of its partner**

TeX gives `(`, `)`, `[` and `]` no meaning of their own, but a document uses them for structure, so a count that does not match lost a bracket (chktex 17).

This rule counts the brackets the run really set, so one a macro produced counts and one a macro read as an argument does not. A bracket in math is a delimiter, and one read verbatim is program text. The finding points at the first bracket left over.

Fix by adding the bracket that is missing.

See [chktex 17](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2).

### unbalanced-color-stack

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**a `\pdfcolorstack` push is never popped, or a pop has nothing to pop**

`\pdfcolorstack⟨stack⟩ push` and `pop` nest like `q`/`Q` in the page's own content stream ([pdfTeX manual, § 8.9.8](https://ctan.org/pkg/pdftex)): a viewer that reads the stack back after the page still expects it empty. A stack number the run could not decide is left out, since satex would otherwise be guessing which stack a push or pop belongs to.

Fix by balancing every push with a pop in the same group.

See [pdftex § 8.9.8](https://ctan.org/pkg/pdftex).

### unbalanced-pdf-content

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**a PDF literal opens marked content or a graphics state that is never closed**

`\pdfliteral` and `\special{pdf:…}` (what ocgx2, ocg-p and pdfbase use to write raw PDF) drop `q`/`Q` and `BMC`/`BDC`/`EMC` straight into the page's content stream. `q`/`Q` save and restore the graphics state — color, line width, the current transform; `BMC`/`BDC`/`EMC` bracket marked content, an optional content group among them. Both have to nest within the page ([PDF 32000, §§ 8.4.2, 14.6](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf)). One left open leaks its state into everything typeset after it; a viewer may also just reject the page.

satex does not break pages into content streams itself, so it checks the order the run executes the literals in.

Fix by closing what is opened in the same group, or by using one environment for both ends.

See [PDF 32000 § 8.4.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf), [PDF 32000 § 14.6](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).

### undefined-citation

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**a citation names a key no bibliography entry declares**

No `\bibitem` and no entry of the `.bib` databases `\bibliography` names declares this key, so LaTeX prints `?` and warns `Citation … undefined`. Citations are observed rather than listed: a citation command writes its key to the `.aux` for the bibliography program (`\citation{⟨key⟩}`) and reads the name the entry defines when the file is read back (`\b@⟨key⟩`). Any line written so is a citation. Without any bibliography every citation is undefined. Nothing is reported while a named database cannot be found, or while a file of the document was not read.

Fix by adding the entry, or by correcting the key.

See [source2e § ltbibl.dtx](https://ctan.org/pkg/source2e).

### undefined-optional-content

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**marked content refers to an optional content group no resource names**

`/OC /name BDC` refers to `/name` in the page's `/Properties` resources, which maps it to an optional content group ([PDF 32000, § 8.11.3.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf)). A name that no `\pdfpageresources`, `\pdfobj` or `pdf:` object mentions is unknown to the viewer, and the content is shown or hidden at its whim. The rule is silent while the run observed no such resources at all.

Fix by declaring the group, or by correcting the name.

See [PDF 32000 § 8.11.3.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).

### undefined-reference

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**`\ref` names a label that is never defined**

No `\label` in the document or its inputs declares this key, so the reference prints `??`. When the run left a project file unread, the label may stand there: the finding is then a precision note.

Fix by adding the `\label`, or by correcting the key.

See [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### uninstalled-package

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span></p>

**the package is not installed, and the distribution could install it**

The file was not found in any tree the run searched, and the distribution's package manager names a package that ships it. The load is therefore no typo: the package is only missing from this machine. A LaTeX package and the package that carries it often differ in name, so the finding names both.

Fix by running the command the finding gives, or `satex install`, which never runs as part of `lint --fix`. The question is off by default because it reads the package repository: turn on `plugins.packages` in `satex.yaml`.

See [TeX Live manual § tlmgr](https://tug.org/texlive/doc.html), [MiKTeX manual § miktex packages](https://docs.miktex.org/manual/).

### unresolved-file

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">correctness</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 27">chktex 27</a></p>

**a package, class or input file is not in the installation**

The file was searched for in the document's directory, the configured search paths and the TEXMF trees, and not found. Case mismatches on case-insensitive filesystems are also reported: they work on Windows or macOS but break on Linux. `satex query distribution` shows which installation was searched.

Fix by installing the package, adding its directory to `search_paths` in `satex.yaml`, pointing `texmf.roots` at the right installation, or (for case mismatches) renaming the file to match the requested name.

See [chktex 27](https://www.nongnu.org/chktex/).

### expected-load

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">correctness</span></p>

**the file was read after a package it expects loaded**

A package or class read on its own may use names that another one defines, as a beamer theme uses beamer's. When `load.expect_detect` is on, SaTeX looks up which classes and packages on disk define those names, and reads the file again after the one that defines most of them. It keeps that run when it leaves fewer names undefined.

Fix by naming what the file expects in `load.expect`, or by loading it with `\RequirePackage` in the file itself.

### math-mode-mismatch

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">correctness</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 49">chktex 49</a></p>

**\(, \), \[ or \] stands where the mode does not allow it**

LaTeX's four math delimiters each test the mode before they act. `\(` and `\[` require that math is off, `\)` inner math and `\]` display math. Where the test fails they call `\@badmath`, which reports `Bad math environment delimiter` ([source2e](https://ctan.org/pkg/source2e), ltmath.dtx).

The rule reads the mode each one really ran in, so it sees a `\(` nested in a `$ … $` and a delimiter reached through a macro, where chktex follows the source and misses both.

Fix by pairing each delimiter with its own partner.

See [chktex 49](https://www.nongnu.org/chktex/), [source2e § ltmath.dtx](https://ctan.org/pkg/source2e).

### unintended-escape

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">correctness</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 21">chktex 21</a></p>

**a command is followed by an escaped punctuation mark, which is another command**

`\TeX\. Right?` does not set `TeX. Right?`. `\.` is the dot accent, so it takes the `R` and sets `TeXṘight?` (chktex 21).

The rule looks for a control word followed with nothing between by `\.`, `\,`, `\;`, `\:`, `\!` or `\?`, where a space and a word follow. It stays quiet in math mode, where `\,` is a thin space, inside a body the run read verbatim, and where the run really did set that mark there.

Fix by removing the backslash.

See [chktex 21](https://www.nongnu.org/chktex/), [The TeXbook, chapter 9](https://ctan.org/pkg/texbook#chapter-9).

</details>

<details>
<summary><b>security</b> — 2 rules</summary>

### build-shell-escape-unneeded

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">security</span></p>

**the build allows `\write18`, but the document never uses it**

`-shell-escape` lets every package and every `.aux` or `.bbl` line the document reads run any program as you. The run reached no `\write18`, so the permission buys nothing and only widens what a hostile input file can do.

Fix by removing `-shell-escape` from the engine command (or `shell: yes` from the arara directive).

See [pdftex § -shell-escape](https://ctan.org/pkg/pdftex).

### shell-escape

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">security</span></p>

**the document runs a program through `\write18`**

`\write18` hands its text to the shell. A run only does that when the engine was started with `--shell-escape`, or with `--shell-restricted` and the program is one TeX Live allows. `shell_escape` in `satex.yaml` says which of the three this build is.

Fix by running with the flag the document needs, or by removing the call.

See [pdftex § -shell-escape](https://ctan.org/pkg/pdftex).

</details>

<details>
<summary><b>style</b> — 50 rules</summary>

### accent-on-dotted-letter

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 7">chktex 7</a></p>

**an accent sits on top of the dot of an `i` or a `j`**

An accent is placed over the whole character, so an `i` keeps its dot under it ([The TeXbook, chapter 9](https://ctan.org/pkg/texbook#chapter-9)). `\i` and `\j` are the dotless letters, and math has `\imath` and `\jmath` ([fntguide § 5](https://ctan.org/pkg/fntguide)).

The rule reads the character the `\accent` or `\mathaccent` primitive really took, and how far the argument reaches from the source, so `\^{}i` is left alone. In OT1 `\'i` is a composite for `\'\i` and in T1 the glyph `í`, so neither is reported.

Fix by writing the dotless letter under the accent.

See [chktex 7](https://www.nongnu.org/chktex/), [The TeXbook, chapter 9](https://ctan.org/pkg/texbook#chapter-9), [fntguide § 5](https://ctan.org/pkg/fntguide).

### command-terminated-space

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 1">chktex 1</a></p>

**a control word ate the space after it and the next word is set against what it produced**

TeX reads a control word and then discards the spaces after it ([The TeXbook, chapter 3](https://ctan.org/pkg/texbook#chapter-3)), so `\TeX ok` sets `TEXok`.

A space the source has after a control word and the run never turned into glue is one TeX swallowed. The rule reports it only where the command put a character on the page and the next word starts against it, so `\rm text` and `\noindent text` stay silent.

Fix by writing `\TeX{} ok`, or `\TeX\ ok` where a space belongs.

See [chktex 1](https://www.nongnu.org/chktex/), [The TeXbook, chapter 3](https://ctan.org/pkg/texbook#chapter-3).

### dimension-times

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 29">chktex 29</a></p>

**the letter x stands between two numbers where a multiplication sign belongs**

`640x480` sets the letter x between the numbers. The sign for a product is `\times`, a binary operator with the spacing of one ([The TeXbook, chapter 16](https://ctan.org/pkg/texbook#chapter-16)).

The rule reads the characters the run set, so it finds a dimension a macro assembled as well as one typed out. It names `\times` where the letter was already in math mode and `$\times$` where it was in text.

Fix by writing the operator.

See [chktex 29](https://www.nongnu.org/chktex/), [The TeXbook, chapter 16](https://ctan.org/pkg/texbook#chapter-16).

### double-space

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 30, 39">chktex 30, 39</a></p>

**two interword spaces reached the page, one of them from a tie**

TeX reads a run of spaces as one space token ([The TeXbook, chapter 8](https://ctan.org/pkg/texbook#chapter-8)), so two spaces in the source set one on the page. satex reports only the spaces that really reached it.

Two do when a tie stands beside an ordinary space, as in `Dr.~ Smith`, and when the run appended two glues in a row, as in `a {} b`. That second case is chktex's warning 30, off by default, so it comes as a note.

Fix by deleting the ordinary space.

See [chktex 39](https://www.nongnu.org/chktex/), [chktex 30](https://www.nongnu.org/chktex/), [The TeXbook, chapter 8](https://ctan.org/pkg/texbook#chapter-8).

### ellipsis-dots

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 11">chktex 11</a></p>

**three periods set three periods, not an ellipsis**

Three `.` characters are set with the spacing of three periods, which is tighter than an ellipsis ([The TeXbook, chapter 18](https://ctan.org/pkg/texbook#chapter-18)). `\ldots` sets them on the baseline and `\cdots` at the height of a binary operator.

The rule reads the periods that reached the page, so an ellipsis a command produced is already excluded. In math it names `\cdots` between the operators chktex lists.

Fix by writing the command.

See [chktex 11](https://www.nongnu.org/chktex/), [The TeXbook, chapter 18](https://ctan.org/pkg/texbook#chapter-18).

### expl3-scope

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span></p>

**an expl3 variable's scope prefix disagrees with how the run assigned it**

A variable named `\l_…` is local and `\g_…` global ([interface3](https://ctan.org/pkg/l3kernel)). The run set this one the other way: a `\l_` variable by a global function (`\tl_gset:Nn`), or a `\g_` variable by a local one. Declarations (`\…_new:N`) are global for every variable and do not count.

Fix by renaming the variable or using the function of the other scope.

See [interface3 § Naming functions and variables](https://ctan.org/pkg/l3kernel).

### expl3-type

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span></p>

**an expl3 variable's type suffix disagrees with the functions that set it**

A variable's name ends in its type (`_tl`, `_seq`, `_int`, …; [interface3](https://ctan.org/pkg/l3kernel)). The run made or set this one with another module's function, as `\seq_set_split:Nnn` on a `_tl` variable. A string and a comma list are token lists, so token list functions on them are fine.

Fix by renaming the variable or using the functions of its type.

See [interface3 § Naming functions and variables](https://ctan.org/pkg/l3kernel).

### group-local-definition

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span></p>

**a definition is made inside a group and lost when it closes**

`\def` and `\newcommand` are local. Inside `{…}`, `\begingroup…\endgroup` or an environment, the definition disappears at the closing brace.

Fix by moving the definition out of the group, or by prefixing it with `\global`.

See [The TeXbook, chapter 5](https://ctan.org/pkg/texbook#chapter-5).

### math-operator-name

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 35">chktex 35</a></p>

**an operator name is set in math italic instead of by its command**

Letters in math mode are set in math italic, one variable per letter, with the spacing of a product ([The TeXbook, chapter 18](https://ctan.org/pkg/texbook#chapter-18)), so `$sin(x)$` sets a product of three variables.

satex decides this from the run, not from a list of names: the name the letters spell has to be a command this document has whose body sets exactly those letters. So `\argmax` from `\DeclareMathOperator` is suggested too, and `\mathrm{sin}` is left alone.

Fix by writing the command.

See [chktex 35](https://www.nongnu.org/chktex/), [The TeXbook, chapter 18](https://ctan.org/pkg/texbook#chapter-18).

### missing-italic-correction

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 6">chktex 6</a></p>

**a slanted letter runs into upright text with no `\/` between them**

A slanted letter leans over the space after it, and `\/` adds the kern the font asks for ([The TeXbook, chapter 4](https://ctan.org/pkg/texbook#chapter-4)).

satex compares each character's `\f@shape` against the kernel's `\itdefault` and `\sldefault` ([fntguide § 2](https://ctan.org/pkg/fntguide)), and the character's italic correction in its own font says whether the kern is worth having. So `{\it …}`, `\emph{…}` and a shape a package selects are all seen alike, where chktex knows only a list of names.

Fix by writing `\/` after the last slanted character.

See [chktex 6](https://www.nongnu.org/chktex/), [The TeXbook, chapter 4](https://ctan.org/pkg/texbook#chapter-4), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L21365).

### missing-nonbreaking-space

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 2">chktex 2</a></p>

**an ordinary space in front of a reference lets the line break before the number**

A reference should not begin a line on its own, so a tie belongs in front of it. `~` is an interword space TeX may not break at ([The TeXbook, chapter 14](https://ctan.org/pkg/texbook#chapter-14)).

The rule reads the reference and citation sites the run recorded, so it follows a `\ref` inside a macro of yours without a list of names, and it reports only where the space really became glue ([tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20358)).

Fix by writing `Figure~\ref{fig:one}`.

See [chktex 2](https://www.nongnu.org/chktex/), [The TeXbook, chapter 14](https://ctan.org/pkg/texbook#chapter-14), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20358).

### parenthesis-spacing

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 36, 37">chktex 36, 37</a></p>

**a parenthesis carries the space of the text inside it instead of the text around it**

A parenthesis takes the space of the text around it, never of the text inside it: `text (inside) text`, not `text( inside )text`.

The rule reads the characters the run set, skips math, where `f(x)` is right, and reports a space inside the parentheses only where a real space token became glue. A missing space is the noisier half, because `word(s)` is ordinary English, so those findings come as notes.

Code shown as text is not prose. The rule skips text in the typewriter family (`\ttdefault`) and a call: a `(` right after a name, around a list of single words or nothing, as in `plot(x, y)` or `c(1, 2, 3)`.

Fix by moving the space to the other side of the parenthesis.

See [chktex 36](https://www.nongnu.org/chktex/), [chktex 37](https://www.nongnu.org/chktex/).

### quote-direction

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 32, 33, 34">chktex 32, 33, 34</a></p>

**a quotation mark points the wrong way, or a run of them mixes both directions**

A backtick opens a quotation and an apostrophe closes it ([The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2)). A mark the other way round sets the wrong glyph, and `` `' `` sets an empty quotation.

satex reads the direction from the characters around the mark in the typeset text. An apostrophe inside a word, an elision such as `'90s`, and a prime in math are left alone, and `` `{}` `` counts as two marks because the group stops the ligature.

Fix by turning the mark the right way round.

See [chktex 32](https://www.nongnu.org/chktex/), [chktex 33](https://www.nongnu.org/chktex/), [chktex 34](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2).

### sentence-spacing

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 12, 13">chktex 12, 13</a></p>

**a period set the wrong width of space for the sentence around it**

How wide a space TeX sets follows the space factor of the character before it ([tex.web § 1034](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20114)). A period raises it to 3000, which asks for the wider space between sentences, but a period after an uppercase letter leaves it at 1000.

So `etc. and` gets a sentence space and `the ABC. Next` a word space, both the wrong way round. The rule reads the characters the run set and reports only where a real space token became glue.

Fix by writing `etc.\ and`, or `the ABC\@. Next`.

See [chktex 12](https://www.nongnu.org/chktex/), [chktex 13](https://www.nongnu.org/chktex/), [tex.web § 1034](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20114).

### space-before-command

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 24, 42">chktex 24, 42</a></p>

**a space lets the line break between a word and the mark that belongs to it**

`\label` records the page its own position falls on, and `\index` and `\footnote` mark the word they stand after ([source2e](https://ctan.org/pkg/source2e), ltxref.dtx). A space in front of one is glue TeX may break the line at, so the mark can land on the next line.

The rule finds the label sites the run recorded, so a `\label` your own macro calls counts too, and reports the space only where the run turned it into glue.

Fix by deleting the space, so the command touches the word.

See [chktex 24](https://www.nongnu.org/chktex/), [chktex 42](https://www.nongnu.org/chktex/), [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### space-before-punctuation

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 26">chktex 26</a></p>

**a space stands between a word and the punctuation after it**

Punctuation belongs against the word it follows. A space in front of a comma or a period is glue, and the reader sees it on the page.

The rule reports only a real space token that became glue in horizontal mode ([tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20358)). A thin space a language package inserts before `:` comes from that package's own code, which the run keeps out of the text, so French typography is not reported.

Fix by deleting the space.

See [chktex 26](https://www.nongnu.org/chktex/), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20358).

### stray-italic-correction

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 5, 28">chktex 5, 28</a></p>

**a repeated italic correction, or one in front of a period or comma**

`\/` appends a kern as wide as the italic correction of the last character on the current list ([The TeXbook, chapter 4](https://ctan.org/pkg/texbook#chapter-4)). The rule reports two places where that kern does not belong: a second `\/` with nothing typeset in between, which finds a kern rather than a character ([tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L21365)), and one in front of a period or a comma (chktex 28).

satex reads the characters the document set, so it sees through a closing brace: in `\textit{word\/}.` the next character set is the period.

Fix by deleting the `\/`.

See [chktex 5](https://www.nongnu.org/chktex/), [chktex 28](https://www.nongnu.org/chktex/), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L21365).

### unknown-suppress-code

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span></p>

**a satex-disable comment names a code no rule has**

A `satex-disable…`/`satex-enable` magic comment named a code that `satex lint --rules` does not list, most often a typo. It suppresses nothing, since matching a finding's own code is all these comments do.

Fix by correcting the code, or removing it if the rule was renamed or never existed.

### user-pattern

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 20, 44">chktex 20, 44</a></p>

**the source matches a pattern the project asked to be warned about**

chktex keeps two lists of its own in `chktexrc`: `UserWarn`, literal strings to report (warning 20), and `UserWarnRegex`, regular expressions (warning 44).

satex reads both from `lints.user-pattern` in `satex.yaml`, and both are empty by default. A regex is written as [the `regex` crate](https://docs.rs/regex) reads it, and one that does not compile is skipped. A match inside a comment or a verbatim body is not reported.

Fix by writing what the project's own rule asks for, or by removing the pattern from `satex.yaml`.

See [chktex 20](https://www.nongnu.org/chktex/), [chktex 44](https://www.nongnu.org/chktex/).

### wrong-dash

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 8">chktex 8</a></p>

**a run of hyphens sets a dash of the wrong length for what stands around it**

TeX's text fonts turn `-` into a hyphen, `--` into an en-dash and `---` into an em-dash ([The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2)).

The rule reads the characters the document set, following chktex's counts: one or three hyphens between letters, two between digits, three between spaces. `lints.wrong-dash` in `satex.yaml` changes them and its `except` list names spellings to leave alone. A source scan misses that `-{}-` sets two hyphens, the group having broken the ligature, and that every `-` in math is a minus sign.

Fix by writing the run of hyphens the surrounding text calls for.

See [chktex 8](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2).

### wrong-quote-character

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 18, 19">chktex 18, 19</a></p>

**a character that is not a quotation mark stands in for one**

TeX sets a quotation from two backticks and two apostrophes, which the font's ligature program pairs into the marks ([The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2)).

What a straight `"` sets depends on the encoding, so the rule reads `\encodingdefault`: OT1 has a right double quote there, T1 and TU the typewriter one. `´` is an acute accent, not the apostrophe `'`. A prime in math is never reported, and the rule is silent outside an English `\languagename`.

Fix by opening with two backticks and closing with two apostrophes, or with `\enquote{…}` from csquotes.

See [chktex 18](https://www.nongnu.org/chktex/), [chktex 19](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2).

### zero-italic-correction

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 4">chktex 4</a></p>

**the italic correction is zero here, so `\/` changes nothing**

`\/` appends a kern as wide as the italic correction the font gives the last character on the current list ([tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L21365)). Where that correction is zero the `\/` does nothing.

Every character carries the font it was set in ([fntguide § 2](https://ctan.org/pkg/fntguide)), and satex reads the correction out of that font's TFM. That is a measurement, where chktex decides from a list of command names. In math, and after a space, the kern is zero whatever the font.

Fix by deleting the `\/`.

See [chktex 4](https://www.nongnu.org/chktex/), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L21365), [The TeXbook, chapter 4](https://ctan.org/pkg/texbook#chapter-4).

### build-bibliography-unneeded

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**the build configures BibTeX or biber for a document without a bibliography**

The latexmkrc sets `$bibtex_use`, `$bibtex` or `$biber`, but the document names no bibliography database (`\bibliography`, `\addbibresource`), so the setting never takes effect (latexmk(1), `$bibtex_use`).

Fix by removing the setting.

See [latexmk § $bibtex_use](https://ctan.org/pkg/latexmk).

### build-engine-options

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**the engine command could report errors as file:line and write SyncTeX data**

`-file-line-error` makes the engine report `file:line: message` instead of `! message`, which editors and satex's log reading can jump to. `-synctex=1` writes the data that links the PDF back to the source.

Fix by adding them to the command, e.g. `$pdflatex = 'pdflatex -file-line-error -synctex=1 %O %S';`.

See [pdftex § -file-line-error](https://ctan.org/pkg/pdftex), [luatex § -synctex](https://ctan.org/pkg/luatex).

### build-unused-custom-dependency

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**a latexmk custom dependency that nothing in the document triggers**

`add_cus_dep(from, to, …)` makes latexmk run a program when a file with the `from` extension is newer than the `to` file it produces (latexmk(1), "CUSTOM DEPENDENCIES"). The document writes no glossary or index, and no file with the `from` extension is in the project, so the rule never runs.

Fix by removing the rule, or keep it if another document of the project needs it.

See [latexmk § CUSTOM DEPENDENCIES](https://ctan.org/pkg/latexmk).

### comment-marker

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 22">chktex 22</a></p>

**a comment still carries a TODO-style marker**

chktex's warning 22 prints every comment it finds, which is noise in a document of any size.

This rule reports only the comments carrying a marker that says the note is unfinished work. The defaults are `TODO`, `FIXME`, `XXX` and `HACK`, matched as whole words and case-sensitively, and `lints.comment-marker` in `satex.yaml` sets its own list. A `%` inside a body the run read verbatim is a character, not a comment.

Fix by doing what the note asks and deleting it.

See [chktex 22](https://www.nongnu.org/chktex/).

### computer-modern-in-t1

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span></p>

**T1 text in Computer Modern relies on cm-super for scalable fonts**

Computer Modern in T1 is the EC family, which TeX Live ships as scalable Type 1 fonts only through cm-super. Without it the PDF gets bitmap fonts that look blurred on screen. [Latin Modern](https://ctan.org/pkg/lm) is the same design with T1 glyphs of its own.

The rule reads the encoding and the family of every character the document set, so it fires on the characters really set in T1 `cmr`.

Fix by loading `\usepackage{lmodern}`, or by making sure cm-super is installed.

See [lmodern § Latin Modern](https://ctan.org/pkg/lm), [fntguide § 5](https://ctan.org/pkg/fntguide).

### csquotes-available

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 18">chktex 18</a></p>

**the document sets quotation marks by hand instead of loading csquotes**

`\enquote{…}` sets the marks the current language wants, nests an inner quotation inside an outer one, and keeps one spelling for the whole document ([csquotes](https://ctan.org/pkg/csquotes) [§ 3.1](https://ctan.org/pkg/csquotes)).

The rule fires where the run set quotation marks itself and csquotes is not loaded. Apostrophes inside a word and primes in math do not count.

Fix by loading `\usepackage{csquotes}` and writing `\enquote{…}`.

See [chktex 18](https://www.nongnu.org/chktex/), [csquotes § 3.1](https://ctan.org/pkg/csquotes).

### dead-definition

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span> <span class="fix fix-unsafe">unsafe</span></p>

**a definition is replaced before anything uses it**

The name is defined, and defined again, with no use in between: the first replacement text never reaches the typesetter. Both definitions are made on every path, so this is not a conditional fallback.

Fix by deleting the first definition, or by checking which one was meant.

### duplicate-color-value

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**two color names store the exact same color**

Two `\definecolor`-family calls, at least one of them the document's own, store the same model and components under different names. Not wrong, but one name likely means to be the other.

Silent unless the run could read both colors back exactly.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### duplicate-package

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span></p>

**a package is loaded more than once**

The second request is ignored by LaTeX. It costs nothing at run time but it hides which load carries the options. A request with options the first did not have is an option clash instead (`option-clash`).

Fix by deleting the later `\usepackage`.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### hand-set-quantity

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span></p>

**a number and its unit are set by hand where siunitx would set them**

`2ms` sets the number and the unit as one word, so the line may break between them and the unit is italic in math. The [SI brochure](https://www.bipm.org/en/publications/si-brochure) (9th ed., [§ 5.4.3](https://www.bipm.org/en/publications/si-brochure)) asks for a non-breaking space and an upright unit, and [siunitx](https://ctan.org/pkg/siunitx) does both.

The rule reads what the run typeset: a digit run followed by letters with no space token between them, where the unit is an SI symbol and the document set it itself.

Fix by loading `\usepackage{siunitx}` and writing `\qty{⟨number⟩}{⟨unit⟩}`.

See [SI brochure § 5.4.3](https://www.bipm.org/en/publications/si-brochure), [siunitx § \qty](https://ctan.org/pkg/siunitx).

### inconsistent-precision

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**the numbers of a table column give different numbers of decimals**

`1.5`, `2.25` and `3` in one column claim three different precisions, and do not line up even at their decimal mark. Give them one precision. An `S` column of [siunitx](https://ctan.org/pkg/siunitx) rounds them with `round-mode = places, round-precision = …`.

See [siunitx § 4.6](https://ctan.org/pkg/siunitx).

### math-punctuation

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 40">chktex 40</a></p>

**punctuation sits on the wrong side of a formula**

A formula is part of the sentence around it. Punctuation that ends the sentence goes after the closing delimiter of inline math, and inside a display, where it is set on the display's own line ([The TeXbook, chapter 18](https://ctan.org/pkg/texbook#chapter-18)).

The rule reads the mode, so it treats every display alike, whether it came from `\[`, from `$$` or from an `equation` environment, where chktex follows only `\]` and `$$`.

Fix by moving the punctuation to the other side of the delimiter.

See [chktex 40](https://www.nongnu.org/chktex/), [The TeXbook, chapter 18](https://ctan.org/pkg/texbook#chapter-18).

### math-shift-dollars

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 45, 46">chktex 45, 46</a></p>

**display math opened with `$$` where LaTeX spells it \[**

`$$ … $$` is plain TeX's display. LaTeX spells it `\[ … \]`, which obeys the `fleqn` and `leqno` class options and the class's `\everydisplay` ([source2e](https://ctan.org/pkg/source2e), ltmath.dtx). Inline `$ … $` sets the same as `\( … \)` and is left alone.

Both spellings put the run in the same mode. satex takes the first character set in math mode, lexes the source before it with TeX's own mouth, and reports the math shift characters standing there, so math an environment opened is left alone.

Fix by writing `\[ … \]`.

See [chktex 45](https://www.nongnu.org/chktex/), [chktex 46](https://www.nongnu.org/chktex/), [source2e § ltmath.dtx](https://ctan.org/pkg/source2e).

### microtype-available

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span></p>

**the engine can protrude characters and expand fonts, but microtype is not loaded**

pdfTeX and LuaTeX writing PDF support character protrusion and font expansion ([pdfTeX manual](https://ctan.org/pkg/pdftex), `\pdfprotrudechars` and `\pdfadjustspacing`), which make margins look straighter and let paragraphs break with fewer hyphens and less uneven spacing. microtype turns both on ([microtype manual § 1](https://ctan.org/pkg/microtype)). The rule fires when both are still off at the end of the run and the body set about a page of text in justified paragraphs, so a figure, a standalone picture or ragged slides are left alone. XeTeX only protrudes, and DVI output supports neither, so the rule stays silent there.

Fix by loading `\usepackage{microtype}`.

See [microtype § 1](https://ctan.org/pkg/microtype), [pdftex § \pdfprotrudechars](https://ctan.org/pkg/pdftex).

### numbers-not-aligned

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**a table column of numbers does not line them up at their decimal mark**

A column of numbers is read by comparing them, which works when each digit stands under the digit of the same place: the units under the units, the decimal marks under each other. An `l` or `c` column sets numbers of different lengths ragged, and an `r` column lines up only numbers with as many decimals. [siunitx](https://ctan.org/pkg/siunitx)'s `S` column aligns them at the decimal mark ([§ 7.12](https://ctan.org/pkg/siunitx) of its manual); its header, not a number, goes in braces: `{Time}`.

The rule reads the table's source: a column whose cells below the header are all numbers, three or more, set in a column that does not align them.

See [siunitx § 7.12](https://ctan.org/pkg/siunitx).

### ot1-font-encoding

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span></p>

**accented letters are built with `\accent` because the text is set in OT1**

OT1, LaTeX's default text encoding, has no accented letters: it builds each one with the `\accent` primitive (ot1enc.def, [fntguide § 5](https://ctan.org/pkg/fntguide)). TeX does not hyphenate a word holding an `\accent` ([The TeXbook, appendix H](https://ctan.org/pkg/texbook#appendix-h)), and the PDF's text layer holds an accent and a letter instead of the character.

Every character carries the encoding it was set in, so a document whose default is T1 but which sets one stretch in OT1 is reported for that stretch alone.

Fix by loading `\usepackage[T1]{fontenc}`.

See [source2e § ot1enc.def](https://ctan.org/pkg/source2e), [fntguide § 5](https://ctan.org/pkg/fntguide), [The TeXbook, appendix H](https://ctan.org/pkg/texbook#appendix-h).

### prefer-cleveref

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span></p>

**a hand-written name in front of `\ref` is what cleveref sets from the label itself**

`Section~\ref{sec:x}` writes the name of the thing by hand, so renaming a section into an appendix leaves the prose behind. cleveref's `\cref` and `\Cref` read the name from the label's counter.

The rule reads the `\ref` sites the run recorded and takes a word, or abbreviation with a period, tied or spaced directly in front of one. The word must name the label's kind, read from the counter the label was set after: the counter's name, its `\<counter>name` macro or a `\crefname` text, in singular, plural or as a prefix. `see~\ref{x}` stays silent, and so does a label the run never found. `\eqref`, `\pageref` and `(\ref{x})` have no word in front and stay silent. `Sections~\ref{a} and~\ref{b}` is one finding.

Whether the document loaded cleveref is read from the run. If it did, the fix writes `\Cref{sec:x}`, or `\cref` after a lowercase word, and the rule speaks only where cleveref would set the very word written (`Sec.` stays where it sets `Section`). If it did not, the fix also loads it, after the last `\usepackage` of the preamble.

Fix by writing `\Cref{sec:x}` and loading cleveref.

See [cleveref § 2](https://ctan.org/pkg/cleveref).

### primitive-tex-command

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 41">chktex 41</a></p>

**a plain TeX primitive is used where LaTeX has its own interface**

chktex warning 41 (`You ought to not use primitive TeX in LaTeX code`) flags the primitives in chktexrc's `Primitives` list. satex reports one only where the name still means the primitive, and names the LaTeX command for the same job.

For `\csname` the job is the one the run saw the name formed for — defined, aliased, tested or used — and the suggestion is [etoolbox](https://ctan.org/pkg/etoolbox)'s command for it when the document has it.

Fix by using LaTeX's interface for the same job.

See [chktex 41](https://www.nongnu.org/chktex/), [l2tabu § 1](https://ctan.org/pkg/l2tabu).

### quote-ligature

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 23">chktex 23</a></p>

**three quotation marks in a row set a double and a single mark with nothing between them**

Three backticks in a row set a left double quote followed by a left single quote, glued together ([The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2)). The reader cannot tell which quotation is the outer one.

satex counts only marks TeX really joined, so a group between two of them already keeps them apart and the rule says nothing.

Fix by putting a thin space `\,` between the double mark and the single one, with the double mark on the outside.

See [chktex 23](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook#chapter-2).

### quote-punctuation

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 38">chktex 38</a></p>

**punctuation stands inside the closing quotation mark**

Under the logical convention the closing mark comes directly after the quoted words, and punctuation that belongs to the sentence stands outside it. A full stop written before the closing mark puts it inside the quotation, which claims the stop was part of what was quoted. American practice is the other way round, which is why chktex has this check off by default and satex reports it as a suggestion.

Fix by moving the punctuation after the closing mark.

See [chktex 38](https://www.nongnu.org/chktex/).

### stray-space

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-safe">safe</span></p>

**a macro body picked up a space from an unescaped line end**

Inside a macro's replacement text, TeX turns the end of a line into a space token unless a control word already skipped it or a `%` swallowed the rest of the line ([tex.web § 347](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L7326)).

The finding follows the modes the run read that space in ([§ 1043](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20358): a space is glue only in horizontal mode). It says the macro *inserts* a space where every reading was horizontal, and that it *may* where some might have been.

Fix by writing `%` at the end of that line.

See [tex.web § 347](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L7326), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/fd728dfcdeda36e6180c75121b35aa1b128771f0/texk/web2c/tex.web#L20358).

### table-rules

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**a table is ruled with vertical rules or `\hline`**

[booktabs](https://ctan.org/pkg/booktabs) gives a table three rules of different weights, `\toprule`, `\midrule` and `\bottomrule`, with space around them, and asks for no vertical rules at all ([§ 2](https://ctan.org/pkg/booktabs) of its manual): the columns are separated by their space, and `\hline` sets every rule alike and too close to the text. A table of fewer rows than `lints.table-rules.rows` (3) is left alone.

See [booktabs § 2](https://ctan.org/pkg/booktabs).

### unit-in-cells

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**every number of a table column carries the same unit**

A unit repeated in every cell of a column says the same thing on each line and pushes the numbers out of alignment. The [SI brochure](https://www.bipm.org/en/publications/si-brochure) (9th ed., [§ 5.4.1](https://www.bipm.org/en/publications/si-brochure)) heads such a column with the quantity and its unit, as `Time / ms`; with [siunitx](https://ctan.org/pkg/siunitx), `{Time / \unit{ms}}` over an `S` column of bare numbers.

See [SI brochure § 5.4.1](https://www.bipm.org/en/publications/si-brochure), [siunitx § \unit](https://ctan.org/pkg/siunitx).

### unreferenced-numbered-display

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span></p>

**a display that stepped a counter is never referenced, so its number is spent for nothing**

`\begin{equation}` and its relatives step a counter and print a number beside the display. The starred forms set the same mathematics and spend no number.

satex decides *numbered* by running the document: `\refstepcounter` redefines `\@currentlabel` ([clsguide § counters](https://ctan.org/pkg/clsguide)), so an environment whose body redefined it stepped a counter. `\@captype` and `\@listctr` say the number went to something inside, and amsmath's `\notag`, `\nonumber` and `\tag` say the display governs itself.

Fix by writing the starred environment and dropping the label that nothing reads.

See [amsmath § 3.1](https://ctan.org/pkg/amsmath), [clsguide § counters](https://ctan.org/pkg/clsguide).

### unused-bibitem

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**a `\bibitem` written in the document is never cited**

A `thebibliography` written by hand lists what it lists, cited or not. A database only contributes what is cited, so its entries are not reported. `\nocite{*}` cites everything.

Fix by citing the entry, or by deleting it.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### unused-color

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span></p>

**a color the document defines is never used**

The document names a color with `\definecolor`, `\providecolor` or `\colorlet` and never reads it back through `\color`, `\textcolor` or a `\colorlet` source.

Silent for a color a package defines: a library may define more colors than any one document uses.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### unused-definition

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span></p>

**a macro defined in the document is never used**

Nothing expands this name.

Fix by deleting it, or by checking why nothing calls it yet.

### unused-label

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">style</span> <span class="fix fix-unsafe">unsafe</span></p>

**a label is never referenced**

Fix by deleting the `\label`, or by checking the `\ref` that was meant to use it.

</details>

<details>
<summary><b>performance</b> — 6 rules</summary>

### package-after-preamble

<p class="badges"><span class="sev sev-warning" title="default: warning">▲ default: warning</span> <span class="cat">performance</span> <span class="fix fix-unsafe">unsafe</span></p>

**a package is loaded after `\begin{document}`**

`\usepackage` belongs in the preamble. After `\begin{document}` LaTeX raises `\usepackage before \begin{document}` and the package cannot set anything up.

Fix by moving the load into the preamble.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### includeonly-available

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">performance</span> <span class="fix fix-unsafe">unsafe</span></p>

**a document of several `\include`d parts is built whole**

`\includeonly` names the parts a build reads. The others keep their page numbers and labels from their `.aux` files ([source2e](https://ctan.org/pkg/source2e), ltfiles.dtx). A draft build of one chapter then skips the rest. The rule fires when the document `\include`s at least two parts, no `\includeonly` is in force, and the body is most of what the build interprets. It also has to set a book's worth of paragraph text, `lints.build-cost.characters` in `satex.yaml`: a shorter document builds whole in seconds.

Fix by listing the parts being written in `\includeonly` while drafting.

See [source2e § ltfiles.dtx](https://ctan.org/pkg/source2e).

### preamble-cost

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">performance</span> <span class="fix fix-unsafe">unsafe</span></p>

**what the preamble costs on every build**

The packages the document asks for and the files read before `\begin{document}`, reported when the preamble takes most of the tokens the build interprets and is no small one.

A format holds the preamble already interpreted, so a build that starts from one skips all of it. [mylatexformat](https://ctan.org/pkg/mylatexformat) dumps one from the document's own preamble, and TeX reads a `%&`(format) line before anything else (web2c manual, "Format files").

Fix by dumping the preamble into a format and starting every build from it.

See [The LaTeX Companion § formats](https://ctan.org/pkg/tlc3-examples).

### recursive-macro

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">performance</span></p>

**a macro calls itself, directly or through others**

Recursion is normal in TeX, but an unguarded one loops until TeX runs out of memory, and the analysis has to widen at it — everything the recursion would have defined becomes unknown.

Fix, if the recursion is unintended, by breaking the cycle. Otherwise make sure a conditional terminates it.

### unused-dependency

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">performance</span> <span class="fix fix-unsafe">unsafe</span></p>

**the dependency file lists a TeX Live package no file of the run comes from**

Every installation made from the dependency file carries the package, but the run reads nothing from it. It may still be needed for what satex does not read — fonts, hyphenation patterns, Lua libraries, programs — so this is a hint, not a verdict.

Fix, if the package really is unused, by deleting its line.

### unused-package

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">performance</span> <span class="fix fix-unsafe">unsafe</span></p>

**a loaded package defines nothing the document uses**

Every name the package defines is unused, and no code outside it reads a setting it made (a class's `\textheight` is read for every page). It may still act through hooks or fonts, so this is a hint, not a verdict.

Fix, if the package really is unused, by deleting the `\usepackage`: it is read on every build.

</details>

<details>
<summary><b>precision</b> — 1 rule</summary>

### analysis-imprecision

<p class="badges"><span class="sev sev-info" title="default: info">● default: info</span> <span class="cat">precision</span></p>

**the analysis had to widen**

Undecided conditionals are analyzed arm by arm, so they are not reported here. This rule reports the two places where the analysis is incomplete instead: a recursion that was widened, and a budget that was reached. Findings that depend on such a point may be incomplete.

There is nothing to fix in the document. Raising the limits in `satex.yaml` or narrowing the analysis with `load.packages: false` can reduce it.

</details>
