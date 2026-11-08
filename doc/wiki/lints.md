<!-- generated from doc/src/wiki/lints.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Lint rules

`satex lint` checks the input file with every rule below. `--all` also checks the packages and classes it loads. `--rules` lists the codes, `--explain CODE` prints one rule's explanation. See [Chktex-Coverage](Chktex-Coverage) for the chktex warning each rule follows, and [Primitive-coverage](Primitive-coverage) and [Domains](Domains) for how satex gets the facts a rule reads.

Every rule has a stable number: its category's letter and a count, such as `C012`. The letters are `C` correctness, `X` security, `S` style, `P` performance and `A` precision (of the analysis). Output shows it before the code: the text and GitHub formats, SARIF's rule id and the editor's diagnostic code. `--explain`, `lint_off` and the suppression comments take the number in place of the code. A new rule gets the next free number of its category in [`src/data/lint-numbers.txt`](https://github.com/EagleoutIce/satex-analyzer/blob/main/src/data/lint-numbers.txt). A removed rule keeps its line there, so a number never changes meaning. A rule that moves to another category keeps its number, so suppressions and links that name it stay valid.

## Rules

Every rule, with a link to its explanation below. The shape gives the severity: ◆ error, ▲ warning, ● info. The badge gives the category. The engine column names the engines a rule is about, when not all of them. The origin names the [chktex](Chktex-Coverage) warnings a rule follows. On the site the table comes in pages, and the columns icon in its header hides columns or wraps long summaries. Hovering a rule's name anywhere on the site shows its card.

| no. | code | kind | engine | quick fix | summary | origin |
| --- | --- | --- | --- | --- | --- | --- |
| C001 | [`already-defined`](#already-defined) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | `\newcommand` redefines an existing name | — |
| C043 | [`assertion-refuted`](#assertion-refuted) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | an assertion fails on a run of the document | — |
| C002 | [`bibtex-command-under-biblatex`](#bibtex-command-under-biblatex) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a BibTeX command biblatex replaces stops the run | — |
| C003 | [`build-bibliography-disabled`](#build-bibliography-disabled) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | the document has a bibliography, but the build never runs BibTeX or biber | — |
| C004 | [`build-command-placeholders`](#build-command-placeholders) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | an engine command in the latexmkrc lacks %S or %O | — |
| C005 | [`build-engine-mismatch`](#build-engine-mismatch) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | the build runs another engine, or another output, than the document needs | — |
| C006 | [`build-shell-escape-missing`](#build-shell-escape-missing) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | the document runs programs through `\write18`, but the build does not allow it | — |
| C007 | [`caused-library-error`](#caused-library-error) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | the document makes package code raise a TeX error | — |
| C046 | [`invalid-directive`](#invalid-directive) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | a `%\satex@` directive cannot be read | — |
| C008 | [`math-still-open`](#math-still-open) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | the run is still in math mode where the text ends | <a class="from" href="Chktex-Coverage" title="follows chktex warning 16">chktex 16</a> |
| C047 | [`missing-end`](#missing-end) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | the document ends without `\bye` or `\end{document}` | — |
| C009 | [`missing-graphic`](#missing-graphic) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | an included image is not there for this output target | — |
| C010 | [`new-redefines`](#new-redefines) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | a `\new…` declaration silently redefines an existing command | — |
| C011 | [`not-defined`](#not-defined) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | `\renewcommand` targets a name that does not exist | — |
| C012 | [`option-clash`](#option-clash) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a package is loaded twice with different options | — |
| C013 | [`raised-error`](#raised-error) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | the document's code raises a TeX error | — |
| C014 | [`undefined-color`](#undefined-color) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | a color is used, or aliased from, before xcolor ever defines it | — |
| C015 | [`undefined-control-sequence`](#undefined-control-sequence) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a control sequence is used that nothing defines | — |
| C016 | [`undefined-environment`](#undefined-environment) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | `\begin` names an environment that is not defined | — |
| C017 | [`undefined-glossary-entry`](#undefined-glossary-entry) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | a glossary or acronym entry is used but never declared | — |
| C018 | [`unguarded-recursion`](#unguarded-recursion) | <span class="sev sev-error" title="default: error">◆</span> <span class="cat cat-correctness">correctness</span> | — | — | a recursive macro has nothing that can stop it | — |
| C052 | [`bibliography-entry-missing-field`](#bibliography-entry-missing-field) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | a cited entry lacks a field its type requires | — |
| C019 | [`build-missing-custom-dependency`](#build-missing-custom-dependency) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | the document writes glossary files, but latexmk has no rule to process them | — |
| C020 | [`catcode-escapes-group`](#catcode-escapes-group) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | a category code change is still in force at the end of the run | — |
| C021 | [`conflicting-color-definition`](#conflicting-color-definition) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | the same color name is defined twice with different colors | — |
| C022 | [`delimiter-not-followed`](#delimiter-not-followed) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | `\left`, `\right` or `\middle` is followed by a brace or a math shift | <a class="from" href="Chktex-Coverage" title="follows chktex warning 43">chktex 43</a> |
| C023 | [`duplicate-bibliography-entry`](#duplicate-bibliography-entry) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | the same key is declared by two bibliography entries | — |
| C050 | [`duplicate-bibliography-load`](#duplicate-bibliography-load) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | a database is loaded twice | — |
| C051 | [`duplicate-bibliography-work`](#duplicate-bibliography-work) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | two bibliography entries name one work under different keys | — |
| C024 | [`duplicate-label`](#duplicate-label) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | the same label is defined twice | — |
| C025 | [`environment-mismatch`](#environment-mismatch) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | `\end` names an environment other than the one that is open | <a class="from" href="Chktex-Coverage" title="follows chktex warning 9, 10, 15">chktex 9, 10, 15</a> |
| C026 | [`expl-syntax-left-on`](#expl-syntax-left-on) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | `\ExplSyntaxOn` is never turned off | — |
| C027 | [`expl3-constant-set`](#expl3-constant-set) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | an expl3 constant is set after it was declared | — |
| C028 | [`expl3-signature-mismatch`](#expl3-signature-mismatch) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | an expl3 function takes other arguments than its name says | — |
| C029 | [`ignored-verbatim-tail`](#ignored-verbatim-tail) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | text stands on the line that closes a verbatim body and never reaches the page | <a class="from" href="Chktex-Coverage" title="follows chktex warning 31">chktex 31</a> |
| C030 | [`math-script-braces`](#math-script-braces) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | `^` and `_` take one token, so what follows needs braces | <a class="from" href="Chktex-Coverage" title="follows chktex warning 3, 25">chktex 3, 25</a> |
| C031 | [`missing-dependency`](#missing-dependency) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | a TeX Live package the run reads from is not in the dependency file | — |
| C032 | [`unbalanced-brackets`](#unbalanced-brackets) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | a file sets more of one bracket than of its partner | <a class="from" href="Chktex-Coverage" title="follows chktex warning 17">chktex 17</a> |
| C033 | [`unbalanced-color-stack`](#unbalanced-color-stack) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | <span class="eng" title="only under pdftex, luatex">pdftex, luatex</span> | — | a `\pdfcolorstack` push is never popped, or a pop has nothing to pop | — |
| C034 | [`unbalanced-pdf-content`](#unbalanced-pdf-content) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | <span class="eng" title="only under pdftex, luatex, xetex">pdftex, luatex, xetex</span> | — | a PDF literal opens marked content or a graphics state that is never closed | — |
| C053 | [`undefined-bibliography-string`](#undefined-bibliography-string) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | a `.bib` value uses a constant no `@string` defines | — |
| C035 | [`undefined-citation`](#undefined-citation) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a citation names a key no bibliography entry declares | — |
| C048 | [`undefined-cref-format`](#undefined-cref-format) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | cleveref has no name for a label's type, so `\cref` prints `??` | — |
| C036 | [`undefined-optional-content`](#undefined-optional-content) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | marked content refers to an optional content group no resource names | — |
| C037 | [`undefined-reference`](#undefined-reference) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | `\ref` names a label that is never defined | — |
| C038 | [`uninstalled-package`](#uninstalled-package) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | the package is not installed, and the distribution could install it | — |
| C049 | [`unknown-code-language`](#unknown-code-language) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | a listings key names a language nothing defines | — |
| C039 | [`unresolved-file`](#unresolved-file) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-correctness">correctness</span> | — | — | a package, class or input file is not in the installation | <a class="from" href="Chktex-Coverage" title="follows chktex warning 27">chktex 27</a> |
| C045 | [`assumption-made`](#assumption-made) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-correctness">correctness</span> | — | — | the document assumes a condition | — |
| C040 | [`expected-load`](#expected-load) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-correctness">correctness</span> | — | — | the file was read after a package it expects loaded | — |
| C041 | [`math-mode-mismatch`](#math-mode-mismatch) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-correctness">correctness</span> | — | — | \(, \), \[ or \] stands where the mode does not allow it | <a class="from" href="Chktex-Coverage" title="follows chktex warning 49">chktex 49</a> |
| C042 | [`unintended-escape`](#unintended-escape) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-correctness">correctness</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a command is followed by an escaped punctuation mark, which is another command | <a class="from" href="Chktex-Coverage" title="follows chktex warning 21">chktex 21</a> |
| C044 | [`assertion-proved`](#assertion-proved) | <span class="sev sev-hint" title="default: hint">●</span> <span class="cat cat-correctness">correctness</span> | — | — | an assertion holds on every path | — |
| X001 | [`build-shell-escape-unneeded`](#build-shell-escape-unneeded) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-security">security</span> | — | — | the build allows `\write18`, but the document never uses it | — |
| X002 | [`shell-escape`](#shell-escape) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-security">security</span> | — | — | the document runs a program through `\write18` | — |
| S001 | [`accent-on-dotted-letter`](#accent-on-dotted-letter) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | an accent sits on top of the dot of an `i` or a `j` | <a class="from" href="Chktex-Coverage" title="follows chktex warning 7">chktex 7</a> |
| S002 | [`command-terminated-space`](#command-terminated-space) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | a control word ate the space after it and the next word is set against what it produced | <a class="from" href="Chktex-Coverage" title="follows chktex warning 1">chktex 1</a> |
| S003 | [`dimension-times`](#dimension-times) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | the letter x stands between two numbers where a multiplication sign belongs | <a class="from" href="Chktex-Coverage" title="follows chktex warning 29">chktex 29</a> |
| S004 | [`double-space`](#double-space) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | two interword spaces reached the page, one of them from a tie | <a class="from" href="Chktex-Coverage" title="follows chktex warning 30, 39">chktex 30, 39</a> |
| S005 | [`ellipsis-dots`](#ellipsis-dots) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | three periods set three periods, not an ellipsis | <a class="from" href="Chktex-Coverage" title="follows chktex warning 11">chktex 11</a> |
| S054 | [`et-al`](#et-al) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | `et al.` spelled another way, or a citation number typed after it | — |
| S006 | [`expl3-scope`](#expl3-scope) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | — | an expl3 variable's scope prefix disagrees with how the run assigned it | — |
| S007 | [`expl3-type`](#expl3-type) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | — | an expl3 variable's type suffix disagrees with the functions that set it | — |
| S008 | [`group-local-definition`](#group-local-definition) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | — | a definition is made inside a group and lost when it closes | — |
| S009 | [`math-operator-name`](#math-operator-name) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | an operator name is set in math italic instead of by its command | <a class="from" href="Chktex-Coverage" title="follows chktex warning 35">chktex 35</a> |
| S010 | [`missing-italic-correction`](#missing-italic-correction) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a slanted letter runs into upright text with no `\/` between them | <a class="from" href="Chktex-Coverage" title="follows chktex warning 6">chktex 6</a> |
| S011 | [`missing-nonbreaking-space`](#missing-nonbreaking-space) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | an ordinary space in front of a reference lets the line break before the number | <a class="from" href="Chktex-Coverage" title="follows chktex warning 2">chktex 2</a> |
| S012 | [`parenthesis-spacing`](#parenthesis-spacing) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a parenthesis carries the space of the text inside it instead of the text around it | <a class="from" href="Chktex-Coverage" title="follows chktex warning 36, 37">chktex 36, 37</a> |
| S013 | [`quote-direction`](#quote-direction) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a quotation mark points the wrong way, or a run of them mixes both directions | <a class="from" href="Chktex-Coverage" title="follows chktex warning 32, 33, 34">chktex 32, 33, 34</a> |
| S014 | [`sentence-spacing`](#sentence-spacing) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a period set the wrong width of space for the sentence around it | <a class="from" href="Chktex-Coverage" title="follows chktex warning 12, 13">chktex 12, 13</a> |
| S015 | [`space-before-command`](#space-before-command) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a space lets the line break between a word and the mark that belongs to it | <a class="from" href="Chktex-Coverage" title="follows chktex warning 24, 42">chktex 24, 42</a> |
| S016 | [`space-before-punctuation`](#space-before-punctuation) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | a space stands between a word and the punctuation after it | <a class="from" href="Chktex-Coverage" title="follows chktex warning 26">chktex 26</a> |
| S017 | [`stray-italic-correction`](#stray-italic-correction) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a repeated italic correction, or one in front of a period or comma | <a class="from" href="Chktex-Coverage" title="follows chktex warning 5, 28">chktex 5, 28</a> |
| S018 | [`unknown-suppress-code`](#unknown-suppress-code) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | — | a satex-disable comment names a code no rule has | — |
| S019 | [`user-pattern`](#user-pattern) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | — | the source matches a pattern the project asked to be warned about | <a class="from" href="Chktex-Coverage" title="follows chktex warning 20, 44">chktex 20, 44</a> |
| S020 | [`wrong-dash`](#wrong-dash) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a run of hyphens sets a dash of the wrong length for what stands around it | <a class="from" href="Chktex-Coverage" title="follows chktex warning 8">chktex 8</a> |
| S021 | [`wrong-quote-character`](#wrong-quote-character) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a character that is not a quotation mark stands in for one | <a class="from" href="Chktex-Coverage" title="follows chktex warning 18, 19">chktex 18, 19</a> |
| S022 | [`zero-italic-correction`](#zero-italic-correction) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | the italic correction is zero here, so `\/` changes nothing | <a class="from" href="Chktex-Coverage" title="follows chktex warning 4">chktex 4</a> |
| S023 | [`build-bibliography-unneeded`](#build-bibliography-unneeded) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | the build configures BibTeX or biber for a document without a bibliography | — |
| S024 | [`build-engine-options`](#build-engine-options) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | the engine command could report errors as file:line and write SyncTeX data | — |
| S025 | [`build-unused-custom-dependency`](#build-unused-custom-dependency) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | a latexmk custom dependency that nothing in the document triggers | — |
| S053 | [`clones`](#clones) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a block of the source repeats with only its values changed | — |
| S026 | [`comment-marker`](#comment-marker) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | a comment still carries a TODO-style marker | <a class="from" href="Chktex-Coverage" title="follows chktex warning 22">chktex 22</a> |
| S027 | [`computer-modern-in-t1`](#computer-modern-in-t1) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | <span class="eng" title="only under pdftex">pdftex</span> | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | T1 text in Computer Modern relies on cm-super for scalable fonts | — |
| S028 | [`csquotes-available`](#csquotes-available) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | the document sets quotation marks by hand instead of loading csquotes | <a class="from" href="Chktex-Coverage" title="follows chktex warning 18">chktex 18</a> |
| S029 | [`dead-definition`](#dead-definition) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a definition is replaced before anything uses it | — |
| S030 | [`duplicate-color-value`](#duplicate-color-value) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | two color names store the exact same color | — |
| S031 | [`duplicate-package`](#duplicate-package) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | a package is loaded more than once | — |
| S032 | [`hand-set-quantity`](#hand-set-quantity) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a number and its unit are set by hand where siunitx would set them | — |
| S052 | [`hand-written-author`](#hand-written-author) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | an author name typed before a citation that prints it from the bibliography | — |
| S051 | [`hand-written-reference`](#hand-written-reference) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a number typed after a reference name, where `\ref` would keep it right | — |
| S033 | [`inconsistent-precision`](#inconsistent-precision) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | the numbers of a table column give different numbers of decimals | — |
| S034 | [`math-punctuation`](#math-punctuation) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | punctuation sits on the wrong side of a formula | <a class="from" href="Chktex-Coverage" title="follows chktex warning 40">chktex 40</a> |
| S035 | [`math-shift-dollars`](#math-shift-dollars) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | display math opened with `$$` where LaTeX spells it \[ | <a class="from" href="Chktex-Coverage" title="follows chktex warning 45, 46">chktex 45, 46</a> |
| S036 | [`microtype-available`](#microtype-available) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | <span class="eng" title="only under pdftex, luatex">pdftex, luatex</span> | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | the engine can protrude characters and expand fonts, but microtype is not loaded | — |
| S037 | [`numbers-not-aligned`](#numbers-not-aligned) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | a table column of numbers does not line them up at their decimal mark | — |
| S038 | [`ot1-font-encoding`](#ot1-font-encoding) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | <span class="eng" title="only under pdftex">pdftex</span> | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | accented letters are built with `\accent` because the text is set in OT1 | — |
| S039 | [`prefer-cleveref`](#prefer-cleveref) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a name written before `\ref` that cleveref prints by itself | — |
| S040 | [`primitive-tex-command`](#primitive-tex-command) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a plain TeX primitive is used where LaTeX has its own interface | <a class="from" href="Chktex-Coverage" title="follows chktex warning 41">chktex 41</a> |
| S041 | [`quote-ligature`](#quote-ligature) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | three quotation marks in a row set a double and a single mark with nothing between them | <a class="from" href="Chktex-Coverage" title="follows chktex warning 23">chktex 23</a> |
| S042 | [`quote-punctuation`](#quote-punctuation) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | punctuation stands inside the closing quotation mark | <a class="from" href="Chktex-Coverage" title="follows chktex warning 38">chktex 38</a> |
| S043 | [`stray-space`](#stray-space) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> | a macro body picked up a space from an unescaped line end | — |
| S044 | [`table-rules`](#table-rules) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | a table is ruled with vertical rules or `\hline` | — |
| S055 | [`uncited-bibliography-entry`](#uncited-bibliography-entry) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a database entry is never cited | — |
| S045 | [`unit-in-cells`](#unit-in-cells) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | every number of a table column carries the same unit | — |
| S046 | [`unreferenced-numbered-display`](#unreferenced-numbered-display) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a display that stepped a counter is never referenced, so its number is spent for nothing | — |
| S047 | [`unused-bibitem`](#unused-bibitem) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | a `\bibitem` written in the document is never cited | — |
| S048 | [`unused-color`](#unused-color) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | — | a color the document defines is never used | — |
| S049 | [`unused-definition`](#unused-definition) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a macro defined in the document is never used | — |
| S050 | [`unused-label`](#unused-label) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-style">style</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a label is never referenced | — |
| P001 | [`package-after-preamble`](#package-after-preamble) | <span class="sev sev-warning" title="default: warning">▲</span> <span class="cat cat-performance">performance</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a package is loaded after `\begin{document}` | — |
| P002 | [`includeonly-available`](#includeonly-available) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-performance">performance</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a document of several `\include`d parts is built whole | — |
| P003 | [`preamble-cost`](#preamble-cost) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-performance">performance</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | the preamble costs time on every build | — |
| P004 | [`recursive-macro`](#recursive-macro) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-performance">performance</span> | — | — | a macro calls itself, directly or through others | — |
| P005 | [`unused-dependency`](#unused-dependency) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-performance">performance</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | the dependency file lists a TeX Live package no file of the run comes from | — |
| P006 | [`unused-package`](#unused-package) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-performance">performance</span> | — | <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> | a loaded package defines nothing the document uses | — |
| A001 | [`analysis-imprecision`](#analysis-imprecision) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-precision">precision</span> | — | — | the analysis had to widen | — |
| A002 | [`assertion-unknown`](#assertion-unknown) | <span class="sev sev-info" title="default: info">●</span> <span class="cat cat-precision">precision</span> | — | — | an assertion could not be decided | — |

## Errors caused in package code

An engine error can arise inside a package while the document's own call caused it. `satex lint` then shows it at that call, as `caused-library-error`. The message names the package line, e.g. `error in expl3-code.tex:24703: Argument has an extra }, caused here`. The call is the innermost place in the user's own files on the input stack. These are the main file, the files it reads, and other files below its directory. One row stands for each call line and error code, with a count when it repeats. An error on a path satex approximated may not be TeX's. It is a precision note, which the text output folds into a count and an editor does not show. An exact error after such a one may be its echo, so it is a warning that names the earlier one. A finding that something is absent from the whole document, such as an undefined reference or an unused label, is a precision note too when the run left a project file unread. `query diagnostics` gives the call as `origin`. A runaway over an argument the document itself leaves open is the document's error. It is a `raised-error` at the call, e.g. `File ended while scanning use of \pgfkeys@@set: the argument of \pgfkeys opened here never closes, the file ends inside it`.

## Suppressing findings

A comment in the document silences findings. Without codes it silences every rule, with codes (comma-separated) only those. `lint_off` in `satex.yaml` turns a rule off everywhere.

* `% satex-disable-next-line CODE,…` silences the next line.
* `% satex-disable-line CODE,…` at the end of a line silences that line.
* `% satex-disable CODE,…` … `% satex-enable CODE,…` silences a region.
* `% satex-disable-file CODE,…` silences the whole file.

They apply to every output format and to `--fix`. An unknown code is reported as `unknown-suppress-code`.

`satex lint --style` lists the rules the project breaks so often that they may be its style, as an editor asks about them (`lints.style-prompt` in `satex.yaml`).

## Assertions

A comment can state a property for satex to check. TeX skips the comment. satex runs it where it reads it, on every path.

```latex
%\satex@assert{\thepage < 5}
%\satex@assume{\value{section} <= 3 and \ifdefined\mymacro}
%\satex@configure{limits.site_forks=8, opaque_conditionals=[ifdraft]}
```

* `\satex@assert{⟨condition⟩}`: Checks a condition on every path that reaches it.
* `\satex@assume{⟨condition⟩}`: Drops the paths where a condition fails.
* `\satex@configure{⟨key⟩=⟨value⟩, …}`: Changes settings of `satex.yaml` until the group ends.

* `⟨condition⟩` is ⟨test⟩s joined by `or` and `and`, `and` binding tighter, each maybe after `not`.
* `⟨test⟩` is ⟨operand⟩ ⟨relation⟩ ⟨operand⟩, or a TeX conditional such as `\ifdefined\x`.
* `⟨operand⟩` is a TeX number (`\value{page}`, `\c@page`, `\numexpr…`), a dimension when one side has a unit (`\wd0 < 3cm`), `\the⟨counter⟩` the counter's value (`\thepage`).
* `⟨relation⟩` is one of `<` `=` `>` `<=` `>=` `!=` `==`.
* `⟨key⟩` is a setting of `satex.yaml`, dotted or written with `/`.
* `⟨value⟩` is what the setting takes, as in `satex.yaml`.

* `\satex@assert{cond}` checks `cond`. It is [`assertion-proved`](#assertion-proved) when it holds on every path that reads it. It is [`assertion-refuted`](#assertion-refuted), an error, when it fails on a path that approximated nothing. The message names what the operands are there. Otherwise it is [`assertion-unknown`](#assertion-unknown), with the reason: a value the run does not know, a path that approximated, or no path that reads it.
* `\satex@assume{cond}` drops the paths where `cond` fails. On the others it narrows the registers a conjunction of comparisons reads, as an `\ifnum` arm does. It is listed as [`assumption-made`](#assumption-made).
* `\satex@configure{key=value, …}` changes settings of `satex.yaml` until the current TeX group ends. Keys are dotted, or written with `/`. A setting the run reads only at its start keeps its value.

`\thepage < 5` compares the page number whether `\pagenumbering` prints it in arabic or roman numerals. Only the document's own files are read for directives. A package's comments and text a conditional skips are not. A directive that cannot be read is [`invalid-directive`](#invalid-directive).

## Quick-fixes

A finding's `fix` says what to do. Where that is mechanical it carries the edits, e.g. removing a `\usepackage` line or one name in `\usepackage{a,b,c}`, a `\label{…}`, or turning `\newcommand` into `\renewcommand`. The text output marks them `[fix]` or `[unsafe fix]`:

* **safe** fixes keep the document's meaning: a duplicate `\usepackage`, a definition replaced before it is read, `\renewcommand` of an undefined name.
* **unsafe** fixes change it and deserve a look: removing a package that may have side effects, an unused label, `\newcommand` turned into `\renewcommand`.

`--fix` applies the safe fixes, `--unsafe-fixes` adds the unsafe ones, `--diff` prints the edits instead of writing them. Only project files change. Overlapping fixes wait for the next round. The document is re-analyzed after each round, up to eight.

`--format json` gives each fix as `applicability` and `edits` (`path`, 1-based `start`/`end`, `replacement`), `--format sarif` as `fixes`, `--format lsp` as diagnostics with `quickfix` code actions.

`S` marks a safe fix, `U` an unsafe one. A rule with both has findings of each kind: the fix is safe where it cannot change what the document means and unsafe where it might. Each finding names its own kind.

<div class="fold">
<h3 class="fold-head">60 rules with quick fixes</h3>

* C001 [`already-defined`](#already-defined) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> `\newcommand` redefines an existing name
* C002 [`bibtex-command-under-biblatex`](#bibtex-command-under-biblatex) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a BibTeX command biblatex replaces stops the run
* C047 [`missing-end`](#missing-end) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> the document ends without `\bye` or `\end{document}`
* C011 [`not-defined`](#not-defined) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> `\renewcommand` targets a name that does not exist
* C012 [`option-clash`](#option-clash) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a package is loaded twice with different options
* C015 [`undefined-control-sequence`](#undefined-control-sequence) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a control sequence is used that nothing defines
* C016 [`undefined-environment`](#undefined-environment) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> `\begin` names an environment that is not defined
* C022 [`delimiter-not-followed`](#delimiter-not-followed) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> `\left`, `\right` or `\middle` is followed by a brace or a math shift
* C050 [`duplicate-bibliography-load`](#duplicate-bibliography-load) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> a database is loaded twice
* C051 [`duplicate-bibliography-work`](#duplicate-bibliography-work) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> two bibliography entries name one work under different keys
* C025 [`environment-mismatch`](#environment-mismatch) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> `\end` names an environment other than the one that is open
* C029 [`ignored-verbatim-tail`](#ignored-verbatim-tail) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> text stands on the line that closes a verbatim body and never reaches the page
* C030 [`math-script-braces`](#math-script-braces) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> `^` and `_` take one token, so what follows needs braces
* C031 [`missing-dependency`](#missing-dependency) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> a TeX Live package the run reads from is not in the dependency file
* C035 [`undefined-citation`](#undefined-citation) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a citation names a key no bibliography entry declares
* C048 [`undefined-cref-format`](#undefined-cref-format) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> cleveref has no name for a label's type, so `\cref` prints `??`
* C042 [`unintended-escape`](#unintended-escape) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a command is followed by an escaped punctuation mark, which is another command
* S001 [`accent-on-dotted-letter`](#accent-on-dotted-letter) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> an accent sits on top of the dot of an `i` or a `j`
* S002 [`command-terminated-space`](#command-terminated-space) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> a control word ate the space after it and the next word is set against what it produced
* S003 [`dimension-times`](#dimension-times) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> the letter x stands between two numbers where a multiplication sign belongs
* S004 [`double-space`](#double-space) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> two interword spaces reached the page, one of them from a tie
* S005 [`ellipsis-dots`](#ellipsis-dots) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> three periods set three periods, not an ellipsis
* S054 [`et-al`](#et-al) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> `et al.` spelled another way, or a citation number typed after it
* S009 [`math-operator-name`](#math-operator-name) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> an operator name is set in math italic instead of by its command
* S010 [`missing-italic-correction`](#missing-italic-correction) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a slanted letter runs into upright text with no `\/` between them
* S011 [`missing-nonbreaking-space`](#missing-nonbreaking-space) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> an ordinary space in front of a reference lets the line break before the number
* S012 [`parenthesis-spacing`](#parenthesis-spacing) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a parenthesis carries the space of the text inside it instead of the text around it
* S013 [`quote-direction`](#quote-direction) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a quotation mark points the wrong way, or a run of them mixes both directions
* S014 [`sentence-spacing`](#sentence-spacing) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a period set the wrong width of space for the sentence around it
* S015 [`space-before-command`](#space-before-command) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a space lets the line break between a word and the mark that belongs to it
* S016 [`space-before-punctuation`](#space-before-punctuation) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> a space stands between a word and the punctuation after it
* S017 [`stray-italic-correction`](#stray-italic-correction) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a repeated italic correction, or one in front of a period or comma
* S020 [`wrong-dash`](#wrong-dash) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a run of hyphens sets a dash of the wrong length for what stands around it
* S021 [`wrong-quote-character`](#wrong-quote-character) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a character that is not a quotation mark stands in for one
* S022 [`zero-italic-correction`](#zero-italic-correction) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> the italic correction is zero here, so `\/` changes nothing
* S053 [`clones`](#clones) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a block of the source repeats with only its values changed
* S027 [`computer-modern-in-t1`](#computer-modern-in-t1) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> T1 text in Computer Modern relies on cm-super for scalable fonts
* S028 [`csquotes-available`](#csquotes-available) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> the document sets quotation marks by hand instead of loading csquotes
* S029 [`dead-definition`](#dead-definition) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a definition is replaced before anything uses it
* S031 [`duplicate-package`](#duplicate-package) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> a package is loaded more than once
* S032 [`hand-set-quantity`](#hand-set-quantity) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a number and its unit are set by hand where siunitx would set them
* S052 [`hand-written-author`](#hand-written-author) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> an author name typed before a citation that prints it from the bibliography
* S051 [`hand-written-reference`](#hand-written-reference) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a number typed after a reference name, where `\ref` would keep it right
* S035 [`math-shift-dollars`](#math-shift-dollars) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> display math opened with `$$` where LaTeX spells it \[
* S036 [`microtype-available`](#microtype-available) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> the engine can protrude characters and expand fonts, but microtype is not loaded
* S038 [`ot1-font-encoding`](#ot1-font-encoding) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> accented letters are built with `\accent` because the text is set in OT1
* S039 [`prefer-cleveref`](#prefer-cleveref) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a name written before `\ref` that cleveref prints by itself
* S040 [`primitive-tex-command`](#primitive-tex-command) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a plain TeX primitive is used where LaTeX has its own interface
* S041 [`quote-ligature`](#quote-ligature) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> three quotation marks in a row set a double and a single mark with nothing between them
* S042 [`quote-punctuation`](#quote-punctuation) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> punctuation stands inside the closing quotation mark
* S043 [`stray-space`](#stray-space) <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> a macro body picked up a space from an unescaped line end
* S055 [`uncited-bibliography-entry`](#uncited-bibliography-entry) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a database entry is never cited
* S046 [`unreferenced-numbered-display`](#unreferenced-numbered-display) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a display that stepped a counter is never referenced, so its number is spent for nothing
* S049 [`unused-definition`](#unused-definition) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a macro defined in the document is never used
* S050 [`unused-label`](#unused-label) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a label is never referenced
* P001 [`package-after-preamble`](#package-after-preamble) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a package is loaded after `\begin{document}`
* P002 [`includeonly-available`](#includeonly-available) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a document of several `\include`d parts is built whole
* P003 [`preamble-cost`](#preamble-cost) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> the preamble costs time on every build
* P005 [`unused-dependency`](#unused-dependency) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> the dependency file lists a TeX Live package no file of the run comes from
* P006 [`unused-package`](#unused-package) <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> a loaded package defines nothing the document uses

</div>

## Explanations

One group per category, each with its first rules. A group's button shows the rest.

<div class="fold" id="category-correctness">
<h3 class="fold-head"><b>correctness</b> (53 rules)</h3>

### already-defined

<p class="badges"><code class="num" title="rule number">C001</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**`\newcommand` redefines an existing name**

`\newcommand` refuses to redefine and raises `Command … already defined`.

Fix with `\renewcommand` if the redefinition is intended, or pick another name.

See [source2e § ltdefns.dtx](https://ctan.org/pkg/source2e).

### assertion-refuted

<p class="badges"><code class="num" title="rule number">C043</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**an assertion fails on a run of the document**

The condition of `%\satex@assert` is false on a path that approximated nothing, so a run of the document reaches it false. The finding names the values the condition read there.

See [Assertions](#assertions).

### bibtex-command-under-biblatex

<p class="badges"><code class="num" title="rule number">C002</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a BibTeX command biblatex replaces stops the run**

biblatex makes the commands of a BibTeX bibliography raise errors where it has its own: the style is its `style` option, not `\bibliographystyle`, and the databases are named in the preamble with `\addbibresource` and printed with `\printbibliography`, not with `\bibliography` ([biblatex manual, §3.7.1](https://ctan.org/pkg/biblatex) and [§3.7.2](https://ctan.org/pkg/biblatex)). Only a call the run reached and that raised the error is reported.

Fix by deleting `\bibliographystyle` (choose a style with `\usepackage[style=…]{biblatex}`), and by writing `\printbibliography` for `\bibliography` with an `\addbibresource` per database in the preamble.

See [biblatex §3.7](https://ctan.org/pkg/biblatex).

### build-bibliography-disabled

<p class="badges"><code class="num" title="rule number">C003</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**the document has a bibliography, but the build never runs BibTeX or biber**

`$bibtex_use = 0` tells latexmk never to run BibTeX or biber (latexmk(1), `$bibtex_use`), so the `.bbl` file the bibliography is read from is never made and every citation stays undefined.

Fix by removing the setting. latexmk's default, 1, runs them whenever the `.bib` files exist.

See [latexmk § $bibtex_use](https://ctan.org/pkg/latexmk).

### build-command-placeholders

<p class="badges"><code class="num" title="rule number">C004</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**an engine command in the latexmkrc lacks %S or %O**

latexmk substitutes `%S` with the source file and `%O` with the options it passes itself (latexmk(1), "FORMAT OF COMMAND SPECIFICATIONS"). A command without `%S` compiles nothing, and one without `%O` loses `-jobname`, `-output-directory` and the options given on latexmk's own command line.

Fix by ending the command with `%O %S`.

See [latexmk § FORMAT OF COMMAND SPECIFICATIONS](https://ctan.org/pkg/latexmk).

### build-engine-mismatch

<p class="badges"><code class="num" title="rule number">C005</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**the build runs another engine, or another output, than the document needs**

latexmk's `$pdf_mode` picks the engine: 1 runs `$pdflatex`, 4 `$lualatex`, 5 `$xelatex`, and 0, 2 and 3 run `$latex` for DVI (latexmk(1), `$pdf_mode`). The document contradicts it when a `% !TeX program` line names another engine, when it uses a primitive only another engine has (`\directlua` is LuaTeX's, `\XeTeXrevision` XeTeX's), or when it sets `\pdfoutput` to the other output than latexmk waits for.

Fix by setting `$pdf_mode` to the engine the document is written for.

See [latexmk § $pdf_mode](https://ctan.org/pkg/latexmk).

### build-shell-escape-missing

<p class="badges"><code class="num" title="rule number">C006</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**the document runs programs through `\write18`, but the build does not allow it**

The run reached `\write18`, which only hands its text to the shell when the engine is started with `-shell-escape` (or `-shell-restricted` and a program TeX Live allows). latexmk starts the engine with the command in `$pdflatex`, `$lualatex`, `$xelatex` or `$latex`, whichever `$pdf_mode` selects (latexmk(1), `$pdf_mode`). arara's `pdflatex`, `lualatex` and `xelatex` rules take `shell: yes` ([arara manual, "The official rules"](https://ctan.org/pkg/arara)).

Fix by adding `-shell-escape` to the command, e.g. `$pdflatex = 'pdflatex -shell-escape %O %S';`, or `set_tex_cmds('-shell-escape %O %S');` for every engine at once.

See [latexmk § $pdf_mode](https://ctan.org/pkg/latexmk), [arara § The official rules](https://ctan.org/pkg/arara).

### caused-library-error

<p class="badges"><code class="num" title="rule number">C007</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**the document makes package code raise a TeX error**

An engine error such as `Argument has an extra }` arises inside package code. The cause is the document's own call into that code. The finding sits at that call and names the package line where the error arose. A runaway over the document's own unclosed argument is not one: `raised-error` reports it. Each call and error code is one finding, with a count when it repeats.

When satex had to approximate on the way, the error may not be TeX's: it is a precision note, not a finding. An exact error after such a one may be its echo, as TeX's recovery left a state that may not be TeX's. The finding is then a warning that names the earlier error.

See [The TeXbook, chapter 27](https://ctan.org/pkg/texbook).

### invalid-directive

<p class="badges"><code class="num" title="rule number">C046</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**a `%\satex@` directive cannot be read**

A directive is a comment, so TeX skips it: `%\satex@assert{cond}`, `%\satex@assume{cond}` or `%\satex@configure{key=value, ...}`. A condition compares two TeX numbers (`\thepage < 5`, `\value{section} >= 2`) or two dimensions when one has a unit (`\wd0 <= 3cm`), with `<`, `<=`, `=`, `!=`, `>=` or `>`. It may also be a TeX conditional such as `\ifdefined\foo`. `and`, `or` and `not` combine tests. `\the<counter>` is the counter's value, however `\pagenumbering` prints it.

See [Assertions](#assertions).

### math-still-open

<p class="badges"><code class="num" title="rule number">C008</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 16">chktex 16</a></p>

**the run is still in math mode where the text ends**

A `$` that is never closed leaves TeX in math mode. The next `\par` makes TeX report `Missing $ inserted` and close the formula itself ([tex.web § 1047](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20410) `insert_dollar_sign`), and everything up to there is set as mathematics.

The rule reports what the interpreter already saw: the `missing-dollar` diagnostic it raised, or, where no `\par` followed, a last character still carrying a math mode (unless a limit stopped the run before the text's end). It reports one finding, never both.

Fix by closing the formula.

See [chktex 16](https://www.nongnu.org/chktex/), [tex.web § 1047](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20410).

### missing-end

<p class="badges"><code class="num" title="rule number">C047</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span></p>

**the document ends without `\bye` or `\end{document}`**

Every path of the run reads the document to its end without meeting TeX's `\end`: plain TeX's `\bye`, or LaTeX's `\end{document}`. TeX then stops with `*** (job aborted, no legal \end found)` and writes no last page. A file that is no document of its own (a chapter `\input` by another) ends this way by design and is not reported.

Fix by ending the document with `\bye` (plain TeX) or `\end{document}` (LaTeX).

See [The TeXbook, chapter 23](https://ctan.org/pkg/texbook).

### missing-graphic

<p class="badges"><code class="num" title="rule number">C009</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**an included image is not there for this output target**

`\includegraphics` names a file that graphicx cannot find, with any extension in `\Gin@extensions`, beside the document or on `\graphicspath`. Which extensions count is the driver's (pdftex.def, dvips.def, xetex.def), so it follows the output.

Fix by adding the file, or by producing the format the driver reads.

See [graphics § pdftex.def](https://ctan.org/pkg/graphics).

### new-redefines

<p class="badges"><code class="num" title="rule number">C010</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**a `\new…` declaration silently redefines an existing command**

A LaTeX `\new…` command declares a fresh name, and some do not check that it is: `\newif\begin` makes `\begin` a conditional (and defines `\gintrue` and `\ginfalse` from what follows its first two letters), `\newcount` and `\newtoks` overwrite whatever the name meant. Every later use of the name then does something else, and the error a run reports is somewhere after it, often far away. Reported where the document's own `\new…` call made the definition of a name its own files spell.

Fix by giving the declaration the name it was meant to have (a conditional's name begins with `if`), or by declaring the argument it lacks.

See [source2e § ltdefns.dtx](https://ctan.org/pkg/source2e).

### not-defined

<p class="badges"><code class="num" title="rule number">C011</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span></p>

**`\renewcommand` targets a name that does not exist**

`\renewcommand` requires an existing definition and raises `Undefined control sequence`.

Fix with `\newcommand`, or load the package that was supposed to define it.

See [source2e § ltdefns.dtx](https://ctan.org/pkg/source2e).

### option-clash

<p class="badges"><code class="num" title="rule number">C012</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a package is loaded twice with different options**

LaTeX loads a package once. A second `\usepackage` with options the first one did not have raises `Option clash for package`.

Fix by giving all the options to the first load, or by using `\PassOptionsToPackage` before it.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### raised-error

<p class="badges"><code class="num" title="rule number">C013</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**the document's code raises a TeX error**

Running the document reaches `\errmessage`, which every LaTeX error (`\PackageError`, `\msg_error:nn`, the kernel's own) ends in: a real run stops there. A more specific rule that explains the same error reports it instead. An error reached only on a path satex could not decide, or after it lost track of the mode, is not one a real run need reach, and is not reported. A runaway in package code over an argument the document leaves open is the document's error too: it is shown at the call that opens it.

### undefined-color

<p class="badges"><code class="num" title="rule number">C014</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**a color is used, or aliased from, before xcolor ever defines it**

xcolor reaches every color through `\color@⟨name⟩` ([xcolor § 2.5](https://ctan.org/pkg/xcolor)). A name the run never defined there raises `Undefined color` when xcolor itself looks it up. The rule reads the same lookup the run made: a name a `\color`, `\textcolor` or `\colorlet` reached while nothing had defined it.

Fix by defining the color first, or by correcting its name.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### undefined-control-sequence

<p class="badges"><code class="num" title="rule number">C015</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a control sequence is used that nothing defines**

The name has no meaning at the point it is used, so a real run would stop with `Undefined control sequence`. When the message says "not on every path", the definition sits in a conditional arm that this use does not depend on.

Fix by loading the package that provides it, defining it before the use, or correcting the spelling.

See [The TeXbook, chapter 27](https://ctan.org/pkg/texbook).

### undefined-environment

<p class="badges"><code class="num" title="rule number">C016</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**`\begin` names an environment that is not defined**

`\begin{name}` expands `\name`, and nothing defines it. A real run reports `Environment name undefined`.

Fix by loading the package that provides the environment or by defining it with `\newenvironment{name}{…}{…}`.

See [source2e § ltmiscen.dtx](https://ctan.org/pkg/source2e).

### undefined-glossary-entry

<p class="badges"><code class="num" title="rule number">C017</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**a glossary or acronym entry is used but never declared**

`\gls` and its relatives name an entry that no `\newglossaryentry`, `\newacronym`, `\acro` or `\DeclareAcronym` declares. A real run stops with `Glossary entry ... has not been defined`.

Fix by declaring the entry, or by correcting the key.

See [glossaries § Defining glossary entries](https://ctan.org/pkg/glossaries).

### unguarded-recursion

<p class="badges"><code class="num" title="rule number">C018</code> <span class="sev sev-error" title="default: error">default: error</span> <span class="cat cat-correctness">correctness</span></p>

**a recursive macro has nothing that can stop it**

The run saw the macro's expansion reach the macro again, could not see the recursion end, and found no conditional (no `\if…`, `\else` or `\fi`) to end it. A macro nothing expands is judged by the names its replacement text holds. A real run expands until TeX runs out of memory: `TeX capacity exceeded, sorry [main memory size]`.

Fix by adding the test that stops it, or by removing the self-reference.

### bibliography-entry-missing-field

<p class="badges"><code class="num" title="rule number">C052</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**a cited entry lacks a field its type requires**

The style's data model requires fields per entry type: the `.bst`'s `output.check`s (BibTeX warns `empty journal in knuth84`), or biblatex's mandatory constraints (biber warns under `--validate-datamodel`). An entry with `crossref` or `xref` inherits fields and is not reported. Only cited entries are checked.

Fix by adding the field.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex), [biblatex §2.1](https://ctan.org/pkg/biblatex).

### build-missing-custom-dependency

<p class="badges"><code class="num" title="rule number">C019</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**the document writes glossary files, but latexmk has no rule to process them**

latexmk runs BibTeX, biber and makeindex by itself, but a glossary is sorted by a program it does not know about (`makeglossaries`, `makeindex` with a glossary style, `bib2gls`). Without a custom dependency (latexmk(1), "CUSTOM DEPENDENCIES") the glossary files are never made and the glossary stays empty. The glossaries manual gives the rule, e.g.

```text
add_cus_dep('glo', 'gls', 0, 'makeglossaries');
sub makeglossaries { system("makeglossaries '$_[0]'"); }
```

See [latexmk § CUSTOM DEPENDENCIES](https://ctan.org/pkg/latexmk), [glossaries § bib2gls](https://ctan.org/pkg/glossaries).

### catcode-escapes-group

<p class="badges"><code class="num" title="rule number">C020</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**a category code change is still in force at the end of the run**

`\catcode` is local, so a change made inside a group is undone by the closing brace. One still in force at the end of the document was made at the outer level, and everything read after it is tokenized differently.

Fix by wrapping the change in `{…}` or `\begingroup…\endgroup`, or by restoring the old value explicitly. `satex query catcodes` shows the current table.

See [The TeXbook, chapter 5](https://ctan.org/pkg/texbook).

### conflicting-color-definition

<p class="badges"><code class="num" title="rule number">C021</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**the same color name is defined twice with different colors**

Two `\definecolor`-family calls store different colors under the same name. Only the last one is ever read back. Silent where the first is a package's: a default the document recolors on purpose.

Fix by giving the second color its own name, or by removing the earlier definition.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### delimiter-not-followed

<p class="badges"><code class="num" title="rule number">C022</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 43">chktex 43</a></p>

**`\left`, `\right` or `\middle` is followed by a brace or a math shift**

`\left`, `\right` and `\middle` scan a delimiter: one character with a `\delcode`, or one control sequence made by `\delimiter` ([tex.web § 1161](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L22057)). A `{`, a `}` or a `$` is none of those, so TeX reports `Missing delimiter (. inserted)` and sets nothing.

The rule asks what each command means rather than matching its name, so it covers `\middle` and any `\let` alias, which chktex's name list does not.

Fix by writing `\{` and `\}` for brace delimiters, or `\left.` where no delimiter is wanted.

See [chktex 43](https://www.nongnu.org/chktex/), [tex.web § 1161](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L22057).

### duplicate-bibliography-entry

<p class="badges"><code class="num" title="rule number">C023</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**the same key is declared by two bibliography entries**

Two `\bibitem`s with one key make LaTeX warn `Label … multiply defined` and every citation resolves to the later one. Two `.bib` entries with one key make BibTeX stop with `Repeated entry`.

Fix by renaming or deleting one of them.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### duplicate-bibliography-load

<p class="badges"><code class="num" title="rule number">C050</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span></p>

**a database is loaded twice**

A `.bib` file named twice (`\bibliography{a,a}`, `\addbibresource` twice, or `\bibliography` and `\addbibresource` both) is read twice. BibTeX reports every entry as a `Repeated entry`. biber warns about each duplicate key.

Fix by dropping the second load.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### duplicate-bibliography-work

<p class="badges"><code class="num" title="rule number">C051</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**two bibliography entries name one work under different keys**

Two entries with the same DOI, or the same title and year, are one work under two keys, in one database or in two. Citing both prints it twice in the bibliography.

Fix by merging them: the entry with more fields stays, gains the fields only the other has, and the citations of the other key cite the kept one.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### duplicate-label

<p class="badges"><code class="num" title="rule number">C024</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**the same label is defined twice**

LaTeX warns `Label … multiply defined` and every reference resolves to the later one. The finding stands at the innermost command of the document's text that writes the label: the `\begin` inside a `\sbox` argument, not the `\sbox`. Only the kernel's `\newlabel` (and zref's) warn: a name another package writes to the `.aux` file twice, such as a tikzmark, is not checked.

Fix by renaming one of them.

See [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### environment-mismatch

<p class="badges"><code class="num" title="rule number">C025</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 9, 10, 15">chktex 9, 10, 15</a></p>

**`\end` names an environment other than the one that is open**

Environments nest like brackets: the name `\end` gives has to match the one `\begin` most recently opened. chktex tracks this as warnings 9 (`'%s' expected, found '%s'`), 10 (`Solo '%s' found`) and 15 (`No match found for '%s'`) for brackets and environments alike. satex only records `\begin`/`\end` occurrences, so this rule covers environments and not generic `{}`, `[]` or `()` nesting.

An error raised inside the environment before the `\end` may have ended it early, through TeX's recovery or the analysis's approximation of its input. The finding is then a note that names that error, as is one the analysis reached on only some of its paths.

Fix by naming the environment that is actually open, or by closing the inner environments first.

See [chktex 9](https://www.nongnu.org/chktex/), [chktex 10](https://www.nongnu.org/chktex/), [chktex 15](https://www.nongnu.org/chktex/), [source2e § ltmiscen.dtx](https://ctan.org/pkg/source2e).

### expl-syntax-left-on

<p class="badges"><code class="num" title="rule number">C026</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**`\ExplSyntaxOn` is never turned off**

With expl3 syntax on, spaces are ignored and `_` and `:` are letters, so ordinary text stops working.

Fix by adding `\ExplSyntaxOff` after the code that needs it.

See [source2e § ltfiles.dtx](https://ctan.org/pkg/source2e).

### expl3-constant-set

<p class="badges"><code class="num" title="rule number">C027</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**an expl3 constant is set after it was declared**

A `\c_…` variable is a constant ([interface3](https://ctan.org/pkg/l3kernel)): it is made once, by `\…_const:N…` or `\…_new:N`, and never set. The run set this one again.

Fix by making it a variable (`\l_…` or `\g_…`) or by not changing it.

See [interface3 § Naming functions and variables](https://ctan.org/pkg/l3kernel).

### expl3-signature-mismatch

<p class="badges"><code class="num" title="rule number">C028</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**an expl3 function takes other arguments than its name says**

An expl3 name carries its signature after the colon ([interface3](https://ctan.org/pkg/expl3), "Naming conventions"): one letter per argument, `N` a single token, `n` a braced group.

What the function really consumes (the calls the run saw, or probe input for one the document defines) takes a different number of arguments, or an optional one or a star, which no expl3 signature has. `\foo:nn⟨a⟩` that takes one argument leaves the second for whatever follows.

Fix by renaming the function to the signature it has, or its parameter text.

See [interface3 § Naming conventions](https://ctan.org/pkg/l3kernel).

### ignored-verbatim-tail

<p class="badges"><code class="num" title="rule number">C029</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 31">chktex 31</a></p>

**text stands on the line that closes a verbatim body and never reaches the page**

A verbatim environment that reads its body line by line throws away whatever follows `\end{…}` on the same line. `fancyvrb` does this and the kernel's `verbatim` does not, so chktex reports warning 31 for both.

satex reports only where the text was really lost. It knows which environments the run read verbatim, because opening one changed the category code of `\`, and reports the tail only where the run set none of it.

Fix by moving the text to the next line.

See [chktex 31](https://www.nongnu.org/chktex/).

### math-script-braces

<p class="badges"><code class="num" title="rule number">C030</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 3, 25">chktex 3, 25</a></p>

**`^` and `_` take one token, so what follows needs braces**

`^` and `_` attach the single next token to the atom before them ([The TeXbook, chapter 16](https://ctan.org/pkg/texbook)). `x^12` sets the exponent 1 and then a 2 of its own size, and `(a+b)^2` puts the exponent on the closing parenthesis alone.

The rule lexes the source with TeX's own mouth where the run really was in math mode, so a `^` under another catcode regime is not one, and `\left( … \right)^2` is one atom the script attaches to.

Fix by putting the script between `{` and `}`.

See [chktex 3](https://www.nongnu.org/chktex/), [chktex 25](https://www.nongnu.org/chktex/), [The TeXbook, chapter 16](https://ctan.org/pkg/texbook).

### missing-dependency

<p class="badges"><code class="num" title="rule number">C031</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span></p>

**a TeX Live package the run reads from is not in the dependency file**

depp names the TeX Live package of every file a run reads by where it sits in the tree (`tex/⟨format⟩/⟨package⟩/`) and writes them to `DEPENDS.txt`. The project's dependency file lacks one of them, so an installation made from it (a minimal Docker image, a `texlive.withPackages`) cannot build the document.

Fix by adding the package to the dependency file, or by rerunning depp.

### unbalanced-brackets

<p class="badges"><code class="num" title="rule number">C032</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 17">chktex 17</a></p>

**a file sets more of one bracket than of its partner**

TeX gives `(`, `)`, `[` and `]` no meaning of their own, but a document uses them for structure, so a count that does not match lost a bracket (chktex 17).

This rule counts the brackets the run really set, so one a macro produced counts and one a macro read as an argument does not. A bracket in math is a delimiter, and one read verbatim is program text. The finding points at the first bracket left over.

Fix by adding the bracket that is missing.

See [chktex 17](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook).

### unbalanced-color-stack

<p class="badges"><code class="num" title="rule number">C033</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="eng" title="only under pdftex, luatex">pdftex, luatex</span></p>

**a `\pdfcolorstack` push is never popped, or a pop has nothing to pop**

`\pdfcolorstack⟨stack⟩ push` and `pop` nest like `q`/`Q` in the page's own content stream ([pdfTeX manual, § 8.9.8](https://ctan.org/pkg/pdftex)): a viewer that reads the stack back after the page still expects it empty. A stack number the run could not decide is left out, since satex would otherwise be guessing which stack a push or pop belongs to.

Fix by balancing every push with a pop in the same group.

See [pdftex § 8.9.8](https://ctan.org/pkg/pdftex).

### unbalanced-pdf-content

<p class="badges"><code class="num" title="rule number">C034</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="eng" title="only under pdftex, luatex, xetex">pdftex, luatex, xetex</span></p>

**a PDF literal opens marked content or a graphics state that is never closed**

`\pdfliteral` and `\special{pdf:…}` (what ocgx2, ocg-p and pdfbase use to write raw PDF) drop `q`/`Q` and `BMC`/`BDC`/`EMC` straight into the page's content stream. `q`/`Q` save and restore the graphics state: color, line width, the current transform. `BMC`/`BDC`/`EMC` bracket marked content, an optional content group among them. Both have to nest within the page ([PDF 32000](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf), [§§ 8](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L316), [4](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L231), [2](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L131), [14](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L463), [6](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L279)). One left open leaks its state into everything typeset after it. A viewer may also just reject the page.

satex does not break pages into content streams itself, so it checks the order the run executes the literals in.

Fix by closing what is opened in the same group, or by using one environment for both ends.

See [PDF 32000 § 8.4.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf), [PDF 32000 § 14.6](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).

### undefined-bibliography-string

<p class="badges"><code class="num" title="rule number">C053</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**a `.bib` value uses a constant no `@string` defines**

A bare word in a field value is a constant: an `@string` of the databases, or one the style predefines (`jan`). Any other is undefined: BibTeX warns `I don't have a string` and writes nothing for it.

Fix by defining the constant, or by writing the value in braces.

See [btxdoc § The database files](https://ctan.org/pkg/bibtex).

### undefined-citation

<p class="badges"><code class="num" title="rule number">C035</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a citation names a key no bibliography entry declares**

No `\bibitem` and no entry of the `.bib` databases `\bibliography` names declares this key, so LaTeX prints `?` and warns `Citation … undefined`. Citations are observed rather than listed: a citation command writes its key to the `.aux` for the bibliography program (`\citation{⟨key⟩}`) and reads the name the entry defines when the file is read back (`\b@⟨key⟩`). Any line written so is a citation. Without any bibliography every citation is undefined. Nothing is reported while a named database cannot be found, or while a file of the document was not read.

Fix by adding the entry, or by correcting the key. The quick fix adds a stub entry to the first database, to fill in.

See [source2e § ltbibl.dtx](https://ctan.org/pkg/source2e).

### undefined-cref-format

<p class="badges"><code class="num" title="rule number">C048</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**cleveref has no name for a label's type, so `\cref` prints `??`**

cleveref prints a reference with the format of the label's type: `\cref` the `cref` one, `\Cref` the `Cref` one, `\crefrange` the range ones. `\crefname` makes them all. cleveref names the common types itself, and a theorem's from its heading, but a counter of the document's own (a `\newcounter` stepped with `\refstepcounter`) has none. The reference then prints `?? 1`, and cleveref warns that the `cref` reference format for label type `task` is undefined.

The finding is that warning, raised by cleveref's own code in the run, at the first reference of each type. Where it was raised on only some paths of the analysis, or after code the run could not follow, it is a hint.

Fix by naming the type after `\usepackage{cleveref}`: `\crefname{task}{task}{tasks}`.

See [cleveref § 8.1.2](https://ctan.org/pkg/cleveref).

### undefined-optional-content

<p class="badges"><code class="num" title="rule number">C036</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**marked content refers to an optional content group no resource names**

`/OC /name BDC` refers to `/name` in the page's `/Properties` resources, which maps it to an optional content group ([PDF 32000, § 8.11.3.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf)). A name that no `\pdfpageresources`, `\pdfobj` or `pdf:` object mentions is unknown to the viewer, and the content is shown or hidden at its whim. The rule is silent while the run observed no such resources at all.

Fix by declaring the group, or by correcting the name.

See [PDF 32000 § 8.11.3.2](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).

### undefined-reference

<p class="badges"><code class="num" title="rule number">C037</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**`\ref` names a label that is never defined**

No `\label` in the document or its inputs declares this key, so the reference prints `??`. When the run left a project file unread, the label may stand there: the finding is then a precision note.

Fix by adding the `\label`, or by correcting the key.

See [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### uninstalled-package

<p class="badges"><code class="num" title="rule number">C038</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**the package is not installed, and the distribution could install it**

The file was not found in any tree the run searched, and the distribution's package manager names a package that ships it. The load is therefore no typo: the package is only missing from this machine. A LaTeX package and the package that carries it often differ in name, so the finding names both.

Fix by running the command the finding gives, or `satex install`, which never runs as part of `lint --fix`. The question is off by default because it reads the package repository: turn on `plugins.packages` in `satex.yaml`.

See [TeX Live manual § tlmgr](https://tug.org/texlive/doc.html), [MiKTeX manual § miktex packages](https://docs.miktex.org/manual/).

### unknown-code-language

<p class="badges"><code class="num" title="rule number">C049</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span></p>

**a listings key names a language nothing defines**

listings finds a language through the driver `\lstlang@⟨language⟩$⟨dialect⟩`, which `\lstdefinelanguage` defines and the language files (`\lstlanguagefiles`, `lstlang1.sty` and its siblings) declare. A name none of them defines raises `Couldn't load requested language`, and the listing is typeset without highlighting.

The rule reads the keys whose values the run formed such a driver from (`language=`), and checks every value of those keys against the run's definitions and the declarations of the language files. Where the run did not know the list of language files, the finding is a note.

Fix by correcting the name, or by defining the language with `\lstdefinelanguage`.

### unresolved-file

<p class="badges"><code class="num" title="rule number">C039</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-correctness">correctness</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 27">chktex 27</a></p>

**a package, class or input file is not in the installation**

The file was searched for in the document's directory, the configured search paths and the TEXMF trees, and not found. Case mismatches on case-insensitive filesystems are also reported: they work on Windows or macOS but break on Linux. `satex query distribution` shows which installation was searched.

Fix by installing the package, adding its directory to `search_paths` in `satex.yaml`, pointing `texmf.roots` at the right installation, or (for case mismatches) renaming the file to match the requested name.

See [chktex 27](https://www.nongnu.org/chktex/).

### assumption-made

<p class="badges"><code class="num" title="rule number">C045</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-correctness">correctness</span></p>

**the document assumes a condition**

`%\satex@assume` drops the paths where its condition is false and narrows the registers it compares on the others. What the run proves after it holds for the runs where the assumption does.

See [Assertions](#assertions).

### expected-load

<p class="badges"><code class="num" title="rule number">C040</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-correctness">correctness</span></p>

**the file was read after a package it expects loaded**

A package or class read on its own may use names that another one defines, as a beamer theme uses beamer's. When `load.expect_detect` is on, SaTeX looks up which classes and packages on disk define those names, and reads the file again after the one that defines most of them. It keeps that run when it leaves fewer names undefined.

Fix by naming what the file expects in `load.expect`, or by loading it with `\RequirePackage` in the file itself.

### math-mode-mismatch

<p class="badges"><code class="num" title="rule number">C041</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-correctness">correctness</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 49">chktex 49</a></p>

**\(, \), \[ or \] stands where the mode does not allow it**

LaTeX's four math delimiters each test the mode before they act. `\(` and `\[` require that math is off, `\)` inner math and `\]` display math. Where the test fails they call `\@badmath`, which reports `Bad math environment delimiter` ([source2e](https://ctan.org/pkg/source2e), ltmath.dtx).

The rule reads the mode each one really ran in, so it sees a `\(` nested in a `$ … $` and a delimiter reached through a macro, where chktex follows the source and misses both.

Fix by pairing each delimiter with its own partner.

See [chktex 49](https://www.nongnu.org/chktex/), [source2e § ltmath.dtx](https://ctan.org/pkg/source2e).

### unintended-escape

<p class="badges"><code class="num" title="rule number">C042</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-correctness">correctness</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 21">chktex 21</a></p>

**a command is followed by an escaped punctuation mark, which is another command**

`\TeX\. Right?` does not set `TeX. Right?`. `\.` is the dot accent, so it takes the `R` and sets `TeXṘight?` (chktex 21).

The rule looks for a control word followed with nothing between by `\.`, `\,`, `\;`, `\:`, `\!` or `\?`, where a space and a word follow. It stays quiet in math mode, where `\,` is a thin space, inside a body the run read verbatim, and where the run really did set that mark there.

Fix by removing the backslash.

See [chktex 21](https://www.nongnu.org/chktex/), [The TeXbook, chapter 9](https://ctan.org/pkg/texbook).

### assertion-proved

<p class="badges"><code class="num" title="rule number">C044</code> <span class="sev sev-hint" title="default: hint">default: hint</span> <span class="cat cat-correctness">correctness</span></p>

**an assertion holds on every path**

The condition of `%\satex@assert` is true on every path that reads it. The paths cover every run of the document, so the property holds, under the assumptions the run reports.

See [Assertions](#assertions).

</div>

<div class="fold" id="category-security">
<h3 class="fold-head"><b>security</b> (2 rules)</h3>

### build-shell-escape-unneeded

<p class="badges"><code class="num" title="rule number">X001</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-security">security</span></p>

**the build allows `\write18`, but the document never uses it**

`-shell-escape` lets every package and every `.aux` or `.bbl` line the document reads run any program as you. The run reached no `\write18`, so the permission buys nothing and only widens what a hostile input file can do.

Fix by removing `-shell-escape` from the engine command (or `shell: yes` from the arara directive).

See [pdftex § -shell-escape](https://ctan.org/pkg/pdftex).

### shell-escape

<p class="badges"><code class="num" title="rule number">X002</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-security">security</span></p>

**the document runs a program through `\write18`**

`\write18` hands its text to the shell. A run only does that when the engine was started with `--shell-escape`, or with `--shell-restricted` and the program is one TeX Live allows. `shell_escape` in `satex.yaml` says which of the three this build is.

Fix by running with the flag the document needs, or by removing the call.

See [pdftex § -shell-escape](https://ctan.org/pkg/pdftex).

</div>

<div class="fold" id="category-style">
<h3 class="fold-head"><b>style</b> (55 rules)</h3>

### accent-on-dotted-letter

<p class="badges"><code class="num" title="rule number">S001</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 7">chktex 7</a></p>

**an accent sits on top of the dot of an `i` or a `j`**

An accent is placed over the whole character, so an `i` keeps its dot under it ([The TeXbook, chapter 9](https://ctan.org/pkg/texbook)). `\i` and `\j` are the dotless letters, and math has `\imath` and `\jmath` ([fntguide § 5](https://ctan.org/pkg/fntguide)).

The rule reads the character the `\accent` or `\mathaccent` primitive really took, and how far the argument reaches from the source, so `\^{}i` is left alone. In OT1 `\'i` is a composite for `\'\i` and in T1 the glyph `í`, so neither is reported.

Fix by writing the dotless letter under the accent.

See [chktex 7](https://www.nongnu.org/chktex/), [The TeXbook, chapter 9](https://ctan.org/pkg/texbook), [fntguide § 5](https://ctan.org/pkg/fntguide).

### command-terminated-space

<p class="badges"><code class="num" title="rule number">S002</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 1">chktex 1</a></p>

**a control word ate the space after it and the next word is set against what it produced**

TeX reads a control word and then discards the spaces after it ([The TeXbook, chapter 3](https://ctan.org/pkg/texbook)), so `\TeX ok` sets `TEXok`.

A space the source has after a control word and the run never turned into glue is one TeX swallowed. The rule reports it only where the command put a character on the page and the next word starts against it, so `\rm text` and `\noindent text` stay silent.

Fix by writing `\TeX{} ok`, or `\TeX\ ok` where a space belongs. Where the joined word is meant (`\TeX book` for TeXbook), write `\TeX{}book`: no space is swallowed there, so the rule stays silent and the intent shows.

See [chktex 1](https://www.nongnu.org/chktex/), [The TeXbook, chapter 3](https://ctan.org/pkg/texbook).

### dimension-times

<p class="badges"><code class="num" title="rule number">S003</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 29">chktex 29</a></p>

**the letter x stands between two numbers where a multiplication sign belongs**

`640x480` sets the letter x between the numbers. The sign for a product is `\times`, a binary operator with the spacing of one ([The TeXbook, chapter 16](https://ctan.org/pkg/texbook)).

The rule reads the characters the run set, so it finds a dimension a macro assembled as well as one typed out. It names `\times` where the letter was already in math mode and `$\times$` where it was in text.

Fix by writing the operator.

See [chktex 29](https://www.nongnu.org/chktex/), [The TeXbook, chapter 16](https://ctan.org/pkg/texbook).

### double-space

<p class="badges"><code class="num" title="rule number">S004</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 30, 39">chktex 30, 39</a></p>

**two interword spaces reached the page, one of them from a tie**

TeX reads a run of spaces as one space token ([The TeXbook, chapter 8](https://ctan.org/pkg/texbook)), so two spaces in the source set one on the page. satex reports only the spaces that really reached it.

Two do when a tie stands beside an ordinary space, as in `Dr.~ Smith`, and when the run appended two glues in a row, as in `a {} b`. That second case is chktex's warning 30, off by default, so it comes as a note.

Fix by deleting the ordinary space.

See [chktex 39](https://www.nongnu.org/chktex/), [chktex 30](https://www.nongnu.org/chktex/), [The TeXbook, chapter 8](https://ctan.org/pkg/texbook).

### ellipsis-dots

<p class="badges"><code class="num" title="rule number">S005</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 11">chktex 11</a></p>

**three periods set three periods, not an ellipsis**

Three `.` characters are set with the spacing of three periods, which is tighter than an ellipsis ([The TeXbook, chapter 18](https://ctan.org/pkg/texbook)). `\ldots` sets them on the baseline and `\cdots` at the height of a binary operator.

The rule reads the periods that reached the page, so an ellipsis a command produced is already excluded. In math it names `\cdots` between the operators chktex lists.

Fix by writing the command.

See [chktex 11](https://www.nongnu.org/chktex/), [The TeXbook, chapter 18](https://ctan.org/pkg/texbook).

### et-al

<p class="badges"><code class="num" title="rule number">S054</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**`et al.` spelled another way, or a citation number typed after it**

`et al.` shortens *et alii*. `et` is a whole word, so only `al.` takes a period. `et. al.`, `et.al.`, `et al` and the spelled-out `et alii` or `et alia` read as slips. The fix writes `et al.` and keeps a tie the source had.

A number in brackets typed after it, as in `Smith et al. [3]`, is a citation typed by hand, so it does not follow the bibliography. Write `\cite`, or `\textcite` (biblatex) and `\citet` (natbib), which print the authors too.

Text in math, verbatim or a comment is never read. The space after `al.` is the business of `sentence-spacing`.

See [The LaTeX Companion § 16.3](https://ctan.org/pkg/tlc3-examples).

### expl3-scope

<p class="badges"><code class="num" title="rule number">S006</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span></p>

**an expl3 variable's scope prefix disagrees with how the run assigned it**

A variable named `\l_…` is local and `\g_…` global ([interface3](https://ctan.org/pkg/l3kernel)). The run set this one the other way: a `\l_` variable by a global function (`\tl_gset:Nn`), or a `\g_` variable by a local one. Declarations (`\…_new:N`) are global for every variable and do not count.

Fix by renaming the variable or using the function of the other scope.

See [interface3 § Naming functions and variables](https://ctan.org/pkg/l3kernel).

### expl3-type

<p class="badges"><code class="num" title="rule number">S007</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span></p>

**an expl3 variable's type suffix disagrees with the functions that set it**

A variable's name ends in its type (`_tl`, `_seq`, `_int`, … in [interface3](https://ctan.org/pkg/l3kernel)). The run made or set this one with another module's function, as `\seq_set_split:Nnn` on a `_tl` variable. A string and a comma list are token lists, so token list functions on them are fine.

Fix by renaming the variable or using the functions of its type.

See [interface3 § Naming functions and variables](https://ctan.org/pkg/l3kernel).

### group-local-definition

<p class="badges"><code class="num" title="rule number">S008</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span></p>

**a definition is made inside a group and lost when it closes**

`\def` and `\newcommand` are local. Inside `{…}`, `\begingroup…\endgroup` or an environment, the definition disappears at the closing brace.

Fix by moving the definition out of the group, or by prefixing it with `\global`.

See [The TeXbook, chapter 5](https://ctan.org/pkg/texbook).

### math-operator-name

<p class="badges"><code class="num" title="rule number">S009</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 35">chktex 35</a></p>

**an operator name is set in math italic instead of by its command**

Letters in math mode are set in math italic, one variable per letter, with the spacing of a product ([The TeXbook, chapter 18](https://ctan.org/pkg/texbook)), so `$sin(x)$` sets a product of three variables.

satex decides this from the run, not from a list of names: the name the letters spell has to be a command this document has whose body sets exactly those letters. So `\argmax` from `\DeclareMathOperator` is suggested too, and `\mathrm{sin}` is left alone.

Fix by writing the command.

See [chktex 35](https://www.nongnu.org/chktex/), [The TeXbook, chapter 18](https://ctan.org/pkg/texbook).

### missing-italic-correction

<p class="badges"><code class="num" title="rule number">S010</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 6">chktex 6</a></p>

**a slanted letter runs into upright text with no `\/` between them**

A slanted letter leans over the space after it, and `\/` adds the kern the font asks for ([The TeXbook, chapter 4](https://ctan.org/pkg/texbook)).

satex compares each character's `\f@shape` against the kernel's `\itdefault` and `\sldefault` ([fntguide § 2](https://ctan.org/pkg/fntguide)), and the character's italic correction in its own font says whether the kern is worth having. So `{\it …}`, `\emph{…}` and a shape a package selects are all seen alike, where chktex knows only a list of names.

Fix by writing `\/` after the last slanted character.

See [chktex 6](https://www.nongnu.org/chktex/), [The TeXbook, chapter 4](https://ctan.org/pkg/texbook), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L21365).

### missing-nonbreaking-space

<p class="badges"><code class="num" title="rule number">S011</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 2">chktex 2</a></p>

**an ordinary space in front of a reference lets the line break before the number**

A reference should not begin a line on its own, so a tie belongs in front of it. `~` is an interword space TeX may not break at ([The TeXbook, chapter 14](https://ctan.org/pkg/texbook)).

The rule reads the reference and citation sites the run recorded, so it follows a `\ref` inside a macro of yours without a list of names, and it reports only where the space really became glue ([tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20358)).

Fix by writing `Figure~\ref{fig:one}`.

See [chktex 2](https://www.nongnu.org/chktex/), [The TeXbook, chapter 14](https://ctan.org/pkg/texbook), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20358).

### parenthesis-spacing

<p class="badges"><code class="num" title="rule number">S012</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 36, 37">chktex 36, 37</a></p>

**a parenthesis carries the space of the text inside it instead of the text around it**

A parenthesis takes the space of the text around it, never of the text inside it: `text (inside) text`, not `text( inside )text`.

The rule reads the characters the run set, skips math, where `f(x)` is right, and reports a space inside the parentheses only where a real space token became glue. A missing space is the noisier half, because `word(s)` is ordinary English, so those findings come as notes.

Code shown as text is not prose. The rule skips text in the typewriter family (`\ttdefault`) and a call: a `(` right after a name, around a list of single words or nothing, as in `plot(x, y)` or `c(1, 2, 3)`. What an undefined command or environment leaves behind is unknown code as well, so every prose rule skips it: the command's arguments and the rest of its statement or line, an environment's body up to its `\end`.

Fix by moving the space to the other side of the parenthesis.

See [chktex 36](https://www.nongnu.org/chktex/), [chktex 37](https://www.nongnu.org/chktex/).

### quote-direction

<p class="badges"><code class="num" title="rule number">S013</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 32, 33, 34">chktex 32, 33, 34</a></p>

**a quotation mark points the wrong way, or a run of them mixes both directions**

A backtick opens a quotation and an apostrophe closes it ([The TeXbook, chapter 2](https://ctan.org/pkg/texbook)). A mark the other way round sets the wrong glyph, and `` `' `` sets an empty quotation.

satex reads the direction from the characters around the mark in the typeset text. An apostrophe inside a word, an elision such as `'90s`, and a prime in math are left alone, and `` `{}` `` counts as two marks because the group stops the ligature.

Fix by turning the mark the right way round.

See [chktex 32](https://www.nongnu.org/chktex/), [chktex 33](https://www.nongnu.org/chktex/), [chktex 34](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook).

### sentence-spacing

<p class="badges"><code class="num" title="rule number">S014</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 12, 13">chktex 12, 13</a></p>

**a period set the wrong width of space for the sentence around it**

How wide a space TeX sets follows the space factor of the character before it ([tex.web § 1034](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20114)). A period raises it to 3000, which asks for the wider space between sentences, but a period after an uppercase letter leaves it at 1000. Under `\frenchspacing` a period sets 1000 too, so nothing is reported.

So `etc. and` gets a sentence space and `the ABC. Next` a word space, both the wrong way round. The rule reads the characters the run set and reports only where a real space token became glue. A lowercase word counts as the sentence running on only when the source writes it right after the space, in the same line and group, unless the word before the period is a listed abbreviation.

Fix by writing `etc.\ and`, or `the ABC\@. Next`.

See [chktex 12](https://www.nongnu.org/chktex/), [chktex 13](https://www.nongnu.org/chktex/), [tex.web § 1034](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20114).

### space-before-command

<p class="badges"><code class="num" title="rule number">S015</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 24, 42">chktex 24, 42</a></p>

**a space lets the line break between a word and the mark that belongs to it**

`\label` records the page its own position falls on, and `\index` and `\footnote` mark the word they stand after ([source2e](https://ctan.org/pkg/source2e), ltxref.dtx). A space in front of one is glue TeX may break the line at, so the mark can land on the next line.

The rule finds the label sites the run recorded, so a `\label` your own macro calls counts too, and reports the space only where the run turned it into glue.

Fix by deleting the space, so the command touches the word.

See [chktex 24](https://www.nongnu.org/chktex/), [chktex 42](https://www.nongnu.org/chktex/), [source2e § ltxref.dtx](https://ctan.org/pkg/source2e).

### space-before-punctuation

<p class="badges"><code class="num" title="rule number">S016</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 26">chktex 26</a></p>

**a space stands between a word and the punctuation after it**

Punctuation belongs against the word it follows. A space in front of a comma or a period is glue, and the reader sees it on the page.

The rule reports only a real space token that became glue in horizontal mode ([tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20358)). A thin space a language package inserts before `:` comes from that package's own code, which the run keeps out of the text, so French typography is not reported.

Fix by deleting the space.

See [chktex 26](https://www.nongnu.org/chktex/), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20358).

### stray-italic-correction

<p class="badges"><code class="num" title="rule number">S017</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 5, 28">chktex 5, 28</a></p>

**a repeated italic correction, or one in front of a period or comma**

`\/` appends a kern as wide as the italic correction of the last character on the current list ([The TeXbook, chapter 4](https://ctan.org/pkg/texbook)). The rule reports two places where that kern does not belong: a second `\/` with nothing typeset in between, which finds a kern rather than a character ([tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L21365)), and one in front of a period or a comma (chktex 28).

satex reads the characters the document set, so it sees through a closing brace: in `\textit{word\/}.` the next character set is the period.

Fix by deleting the `\/`.

See [chktex 5](https://www.nongnu.org/chktex/), [chktex 28](https://www.nongnu.org/chktex/), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L21365).

### unknown-suppress-code

<p class="badges"><code class="num" title="rule number">S018</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span></p>

**a satex-disable comment names a code no rule has**

A `satex-disable…`/`satex-enable` magic comment named a code that `satex lint --rules` does not list, most often a typo. It suppresses nothing, since matching a finding's own code is all these comments do.

Fix by correcting the code, or removing it if the rule was renamed or never existed.

### user-pattern

<p class="badges"><code class="num" title="rule number">S019</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 20, 44">chktex 20, 44</a></p>

**the source matches a pattern the project asked to be warned about**

chktex keeps two lists of its own in `chktexrc`: `UserWarn`, literal strings to report (warning 20), and `UserWarnRegex`, regular expressions (warning 44).

satex reads both from `lints.user-pattern` in `satex.yaml`, and both are empty by default. A regex is written as [the `regex` crate](https://docs.rs/regex) reads it, and one that does not compile is skipped. A match inside a comment or a verbatim body is not reported.

Fix by writing what the project's own rule asks for, or by removing the pattern from `satex.yaml`.

See [chktex 20](https://www.nongnu.org/chktex/), [chktex 44](https://www.nongnu.org/chktex/).

### wrong-dash

<p class="badges"><code class="num" title="rule number">S020</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 8">chktex 8</a></p>

**a run of hyphens sets a dash of the wrong length for what stands around it**

TeX's text fonts turn `-` into a hyphen, `--` into an en-dash and `---` into an em-dash ([The TeXbook, chapter 2](https://ctan.org/pkg/texbook)).

The rule reads the characters the document set, following chktex's counts: one or three hyphens between letters, two between digits, three between spaces. `lints.wrong-dash` in `satex.yaml` changes them and its `except` list names spellings to leave alone. A source scan misses that `-{}-` sets two hyphens, the group having broken the ligature, and that every `-` in math is a minus sign.

Fix by writing the run of hyphens the surrounding text calls for.

See [chktex 8](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook).

### wrong-quote-character

<p class="badges"><code class="num" title="rule number">S021</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 18, 19">chktex 18, 19</a></p>

**a character that is not a quotation mark stands in for one**

TeX sets a quotation from two backticks and two apostrophes, which the font's ligature program pairs into the marks ([The TeXbook, chapter 2](https://ctan.org/pkg/texbook)).

What a straight `"` sets depends on the encoding, so the rule reads `\encodingdefault`: OT1 has a right double quote there, T1 and TU the typewriter one. `´` is an acute accent, not the apostrophe `'`. A prime in math is never reported, and the rule is silent outside an English `\languagename`.

Fix by opening with two backticks and closing with two apostrophes, or with `\enquote{…}` from csquotes.

See [chktex 18](https://www.nongnu.org/chktex/), [chktex 19](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook).

### zero-italic-correction

<p class="badges"><code class="num" title="rule number">S022</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 4">chktex 4</a></p>

**the italic correction is zero here, so `\/` changes nothing**

`\/` appends a kern as wide as the italic correction the font gives the last character on the current list ([tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L21365)). Where that correction is zero the `\/` does nothing.

Every character carries the font it was set in ([fntguide § 2](https://ctan.org/pkg/fntguide)), and satex reads the correction out of that font's TFM. That is a measurement, where chktex decides from a list of command names. In math, and after a space, the kern is zero whatever the font.

Fix by deleting the `\/`.

See [chktex 4](https://www.nongnu.org/chktex/), [tex.web § 1113](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L21365), [The TeXbook, chapter 4](https://ctan.org/pkg/texbook).

### build-bibliography-unneeded

<p class="badges"><code class="num" title="rule number">S023</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**the build configures BibTeX or biber for a document without a bibliography**

The latexmkrc sets `$bibtex_use`, `$bibtex` or `$biber`, but the document names no bibliography database (`\bibliography`, `\addbibresource`), so the setting never takes effect (latexmk(1), `$bibtex_use`).

Fix by removing the setting.

See [latexmk § $bibtex_use](https://ctan.org/pkg/latexmk).

### build-engine-options

<p class="badges"><code class="num" title="rule number">S024</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**the engine command could report errors as file:line and write SyncTeX data**

`-file-line-error` makes the engine report `file:line: message` instead of `! message`, which editors and satex's log reading can jump to. `-synctex=1` writes the data that links the PDF back to the source.

Fix by adding them to the command, e.g. `$pdflatex = 'pdflatex -file-line-error -synctex=1 %O %S';`.

See [pdftex § -file-line-error](https://ctan.org/pkg/pdftex), [luatex § -synctex](https://ctan.org/pkg/luatex).

### build-unused-custom-dependency

<p class="badges"><code class="num" title="rule number">S025</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**a latexmk custom dependency that nothing in the document triggers**

`add_cus_dep(from, to, …)` makes latexmk run a program when a file with the `from` extension is newer than the `to` file it produces (latexmk(1), "CUSTOM DEPENDENCIES"). The document writes no glossary or index, and no file with the `from` extension is in the project, so the rule never runs.

Fix by removing the rule, or keep it if another document of the project needs it.

See [latexmk § CUSTOM DEPENDENCIES](https://ctan.org/pkg/latexmk).

### clones

<p class="badges"><code class="num" title="rule number">S053</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a block of the source repeats with only its values changed**

Two figures that differ only in the width, the file, the caption and the label are one macro used twice. Copies drift apart when one is edited and the other is not.

The rule reads the document's own files token by token, as TeX reads them, with comments and spaces dropped. A run of letters and other characters is a value. Blocks that match up to their values are clones, and so are blocks that match after a few inserted or deleted tokens (`lints.clones.similarity`). The finding stands at the first block and names the other places. Its message lists each hole, the places where the blocks differ, with the values there.

A clone shares at least `min_tokens` tokens, and its values take at most `max_value_share` of it, so paragraphs and lists of items are no clones. Library code is never read. Two of the document's macros with the same body are reported too.

Fix by defining one macro with a parameter per hole and using it at every place. The fix writes it when there are at most `max_parameters` holes (TeX allows 9). It is unsafe: the macro's name is a guess.

### comment-marker

<p class="badges"><code class="num" title="rule number">S026</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 22">chktex 22</a></p>

**a comment still carries a TODO-style marker**

chktex's warning 22 prints every comment it finds, which is noise in a document of any size.

This rule reports only the comments carrying a marker that says the note is unfinished work. The defaults are `TODO`, `FIXME`, `XXX` and `HACK`, matched as whole words and case-sensitively, and `lints.comment-marker` in `satex.yaml` sets its own list. A `%` inside a body the run read verbatim is a character, not a comment.

Fix by doing what the note asks and deleting it.

See [chktex 22](https://www.nongnu.org/chktex/).

### computer-modern-in-t1

<p class="badges"><code class="num" title="rule number">S027</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="eng" title="only under pdftex">pdftex</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**T1 text in Computer Modern relies on cm-super for scalable fonts**

Computer Modern in T1 is the EC family, which TeX Live ships as scalable Type 1 fonts only through cm-super. Without it the PDF gets bitmap fonts that look blurred on screen. [Latin Modern](https://ctan.org/pkg/lm) is the same design with T1 glyphs of its own.

The rule reads the encoding and the family of every character the document set, so it fires on the characters really set in T1 `cmr`.

Fix by loading `\usepackage{lmodern}`, or by making sure cm-super is installed.

See [lmodern § Latin Modern](https://ctan.org/pkg/lm), [fntguide § 5](https://ctan.org/pkg/fntguide).

### csquotes-available

<p class="badges"><code class="num" title="rule number">S028</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 18">chktex 18</a></p>

**the document sets quotation marks by hand instead of loading csquotes**

`\enquote{…}` sets the marks the current language wants, nests an inner quotation inside an outer one, and keeps one spelling for the whole document ([csquotes](https://ctan.org/pkg/csquotes) [§§ 3](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L195), [1](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L95)).

The rule fires where the run set quotation marks itself and csquotes is not loaded. Apostrophes inside a word and primes in math do not count.

Fix by loading `\usepackage{csquotes}` and writing `\enquote{…}`.

See [chktex 18](https://www.nongnu.org/chktex/), [csquotes § 3.1](https://ctan.org/pkg/csquotes).

### dead-definition

<p class="badges"><code class="num" title="rule number">S029</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a definition is replaced before anything uses it**

The name is defined, and defined again, with no use in between: the first replacement text never reaches the typesetter. Both definitions are made on every path, so this is not a conditional fallback. The first is no `\providecommand`, which yields to a later definition on purpose, and both are statements the file reads in order: a definition inside a macro body runs whenever the macro is called, so a default that a key handler or a later call overrides is left alone.

Fix by deleting the first definition, or by checking which one was meant.

### duplicate-color-value

<p class="badges"><code class="num" title="rule number">S030</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**two color names store the exact same color**

Two `\definecolor`-family calls, the later one the document's own, store the same model and components under different names. Not wrong, but one name likely means to be the other.

Silent unless the run could read both colors back exactly, and for a `\colorlet` given exactly another color's name: an alias, such as a role named after the color it takes.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### duplicate-package

<p class="badges"><code class="num" title="rule number">S031</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span></p>

**a package is loaded more than once**

The second request is ignored by LaTeX. It costs nothing at run time but it hides which load carries the options. A request with options the first did not have is an option clash instead (`option-clash`).

Fix by deleting the later `\usepackage`.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### hand-set-quantity

<p class="badges"><code class="num" title="rule number">S032</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a number and its unit are set by hand where siunitx would set them**

`2ms` sets the number and the unit as one word, so the line may break between them and the unit is italic in math. The [SI brochure](https://www.bipm.org/en/publications/si-brochure) (9th ed., [§§ 5](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L268), [4](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L231), [3](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L195)) asks for a non-breaking space and an upright unit, and [siunitx](https://ctan.org/pkg/siunitx) does both.

The rule reads what the run typeset: a digit run followed by letters with no space token between them, where the unit is an SI symbol and the document set it itself.

Fix by loading `\usepackage{siunitx}` and writing `\qty{⟨number⟩}{⟨unit⟩}`.

See [SI brochure § 5.4.3](https://www.bipm.org/en/publications/si-brochure), [siunitx § \qty](https://ctan.org/pkg/siunitx).

### hand-written-author

<p class="badges"><code class="num" title="rule number">S052</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**an author name typed before a citation that prints it from the bibliography**

In `Knuth \cite{knuth84}` the name is typed by hand. natbib's `\citet` and biblatex's `\textcite` print the author from the entry and the citation after it, so the two always agree.

A finding needs a citation of one key whose entry, in a database the run read, has that author's last name right before it. `Knuth and Plass` and `Knuth et al.` count, as does a citation in brackets, `Knuth~[\cite{k}]`. It needs natbib or biblatex too: without `\textcite`, `\citet` or `\citeauthor`, there is nothing better to write.

Fix by writing `\textcite{knuth84}`, `\citet{knuth84}` or `\citeauthor{knuth84}~\cite{knuth84}`. These shorten a long author list as the style says (biblatex's `maxcitenames`, natbib's `.bst`).

`Plass et al.` before a citation whose entry starts with another name, or has fewer than three authors, gets a hint without an edit: the name or the key may be the slip.

See [biblatex § 3.9.1](https://ctan.org/pkg/biblatex), [The LaTeX Companion § 16.3](https://ctan.org/pkg/tlc3-examples).

### hand-written-reference

<p class="badges"><code class="num" title="rule number">S051</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a number typed after a reference name, where `\ref` would keep it right**

`see Section 2` stays `2` when a section is added in front of it. `\ref` prints the number the label has in this run, so it never goes stale.

A finding needs a word that names a kind of thing the document labels (a counter's name, its `\<counter>name` text such as `Figure`, a cleveref name, or an abbreviation with a period such as `Eq.`), then a number or a letter typed in the source, as in `Figure 2`, `Eq. (5)` or `Appendix A`. The words come from the run, so babel's names count too. Text in math, verbatim or a comment is never read, and neither is a number `\ref` set.

Where one label of that kind prints that very number, the fix writes `\cref` (`\Cref` after a capital word), and loads cleveref when the document does not. cleveref then prints the word itself. Otherwise the finding asks for a label.

An `\item` of a numbered list counts too, labelled or not, in enumitem's inline lists as well: `see item (b)` or `in step 2`, where one item prints `(b)` or `Step~2`. Its words are the list counter's (`enumi`, its cleveref name `item`) and the word its label format writes (`Step`). The fix puts a `\label` right after that `\item` and refers to it. A number with no word before it, `in 3.)`, is never read.

See [The LaTeX Companion § 2.4](https://ctan.org/pkg/tlc3-examples), [cleveref § 2](https://ctan.org/pkg/cleveref).

### inconsistent-precision

<p class="badges"><code class="num" title="rule number">S033</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**the numbers of a table column give different numbers of decimals**

`1.5`, `2.25` and `3` in one column claim three different precisions, and do not line up even at their decimal mark. Give them one precision. An `S` column of [siunitx](https://ctan.org/pkg/siunitx) rounds them with `round-mode = places, round-precision = …`.

See [siunitx § 4.6](https://ctan.org/pkg/siunitx).

### math-punctuation

<p class="badges"><code class="num" title="rule number">S034</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 40">chktex 40</a></p>

**punctuation sits on the wrong side of a formula**

A formula is part of the sentence around it. Punctuation that ends the sentence goes after the closing delimiter of inline math, and inside a display, where it is set on the display's own line ([The TeXbook, chapter 18](https://ctan.org/pkg/texbook)).

The rule reads the mode, so it treats every display alike, whether it came from `\[`, from `$$` or from an `equation` environment, where chktex follows only `\]` and `$$`.

Fix by moving the punctuation to the other side of the delimiter.

See [chktex 40](https://www.nongnu.org/chktex/), [The TeXbook, chapter 18](https://ctan.org/pkg/texbook).

### math-shift-dollars

<p class="badges"><code class="num" title="rule number">S035</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 45, 46">chktex 45, 46</a></p>

**display math opened with `$$` where LaTeX spells it \[**

`$$ … $$` is plain TeX's display. LaTeX spells it `\[ … \]`, which obeys the `fleqn` and `leqno` class options and the class's `\everydisplay` ([source2e](https://ctan.org/pkg/source2e), ltmath.dtx). Inline `$ … $` sets the same as `\( … \)` and is left alone.

Both spellings put the run in the same mode. satex takes the first character set in math mode, lexes the source before it with TeX's own mouth, and reports the math shift characters standing there, so math an environment opened is left alone.

Fix by writing `\[ … \]`.

See [chktex 45](https://www.nongnu.org/chktex/), [chktex 46](https://www.nongnu.org/chktex/), [source2e § ltmath.dtx](https://ctan.org/pkg/source2e).

### microtype-available

<p class="badges"><code class="num" title="rule number">S036</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="eng" title="only under pdftex, luatex">pdftex, luatex</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**the engine can protrude characters and expand fonts, but microtype is not loaded**

pdfTeX and LuaTeX writing PDF support character protrusion and font expansion ([pdfTeX manual](https://ctan.org/pkg/pdftex), `\pdfprotrudechars` and `\pdfadjustspacing`), which make margins look straighter and let paragraphs break with fewer hyphens and less uneven spacing. microtype turns both on ([microtype manual § 1](https://ctan.org/pkg/microtype)). The rule fires when both are still off at the end of the run and the body set about a page of text in justified paragraphs, so a figure, a standalone picture or ragged slides are left alone. XeTeX only protrudes, and DVI output supports neither, so the rule stays silent there.

Fix by loading `\usepackage{microtype}`.

See [microtype § 1](https://ctan.org/pkg/microtype), [pdftex § \pdfprotrudechars](https://ctan.org/pkg/pdftex).

### numbers-not-aligned

<p class="badges"><code class="num" title="rule number">S037</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**a table column of numbers does not line them up at their decimal mark**

A column of numbers is read by comparing them, which works when each digit stands under the digit of the same place: the units under the units, the decimal marks under each other. An `l` or `c` column sets numbers of different lengths ragged, and an `r` column lines up only numbers with as many decimals. [siunitx](https://ctan.org/pkg/siunitx)'s `S` column aligns them at the decimal mark ([§§ 7](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L290), [12](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L430) of its manual). Its header, not a number, goes in braces: `{Time}`.

The rule reads the table's source: a column whose cells below the header are all numbers, three or more, set in a column that does not align them.

See [siunitx § 7.12](https://ctan.org/pkg/siunitx).

### ot1-font-encoding

<p class="badges"><code class="num" title="rule number">S038</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="eng" title="only under pdftex">pdftex</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**accented letters are built with `\accent` because the text is set in OT1**

OT1, LaTeX's default text encoding, has no accented letters: it builds each one with the `\accent` primitive (ot1enc.def, [fntguide § 5](https://ctan.org/pkg/fntguide)). TeX does not hyphenate a word holding an `\accent` ([The TeXbook, appendix H](https://ctan.org/pkg/texbook)), and the PDF's text layer holds an accent and a letter instead of the character.

Every character carries the encoding it was set in, so a document whose default is T1 but which sets one stretch in OT1 is reported for that stretch alone.

Fix by loading `\usepackage[T1]{fontenc}`.

See [source2e § ot1enc.def](https://ctan.org/pkg/source2e), [fntguide § 5](https://ctan.org/pkg/fntguide), [The TeXbook, appendix H](https://ctan.org/pkg/texbook).

### prefer-cleveref

<p class="badges"><code class="num" title="rule number">S039</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a name written before `\ref` that cleveref prints by itself**

In `Section~\ref{sec:x}` the word `Section` is typed by hand. If the section becomes an appendix, the word is wrong. cleveref's `\cref` and `\Cref` print the word from the label's counter.

A finding needs a word, or an abbreviation with a period, right before a `\ref`. The word must name the kind of the label: its counter, the counter's `\<counter>name` or a `\crefname` text, singular, plural or a prefix. So `see~\ref{x}` gives no finding, and neither do a label the run never found, `\eqref`, `\pageref` and `(\ref{x})`. `Sections~\ref{a} and~\ref{b}` is one finding.

With cleveref loaded, a finding needs cleveref to print the same word (`Sec.` gives none where it prints `Section`), and the fix writes `\Cref{sec:x}`, or `\cref` after a lowercase word. Without cleveref, the fix also loads it after the last `\usepackage` of the preamble.

Fix by writing `\Cref{sec:x}` and loading cleveref.

See [cleveref § 2](https://ctan.org/pkg/cleveref).

### primitive-tex-command

<p class="badges"><code class="num" title="rule number">S040</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 41">chktex 41</a></p>

**a plain TeX primitive is used where LaTeX has its own interface**

chktex warning 41 (`You ought to not use primitive TeX in LaTeX code`) flags the primitives in chktexrc's `Primitives` list. satex reports one only where the name still means the primitive, and names the LaTeX command for the same job.

For `\csname` the job is the one the run saw the name formed for (defined, aliased, tested or used), and the suggestion is [etoolbox](https://ctan.org/pkg/etoolbox)'s command for it when the document has it.

Fix by using LaTeX's interface for the same job.

See [chktex 41](https://www.nongnu.org/chktex/), [l2tabu § 1](https://ctan.org/pkg/l2tabu).

### quote-ligature

<p class="badges"><code class="num" title="rule number">S041</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 23">chktex 23</a></p>

**three quotation marks in a row set a double and a single mark with nothing between them**

Three backticks in a row set a left double quote followed by a left single quote, glued together ([The TeXbook, chapter 2](https://ctan.org/pkg/texbook)). The reader cannot tell which quotation is the outer one.

satex counts only marks TeX really joined, so a group between two of them already keeps them apart and the rule says nothing.

Fix by putting a thin space `\,` between the double mark and the single one, with the double mark on the outside.

See [chktex 23](https://www.nongnu.org/chktex/), [The TeXbook, chapter 2](https://ctan.org/pkg/texbook).

### quote-punctuation

<p class="badges"><code class="num" title="rule number">S042</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span> <a class="from" href="Chktex-Coverage" title="follows chktex warning 38">chktex 38</a></p>

**punctuation stands inside the closing quotation mark**

Under the logical convention the closing mark comes directly after the quoted words, and punctuation that belongs to the sentence stands outside it. A full stop written before the closing mark puts it inside the quotation, which claims the stop was part of what was quoted. American practice is the other way round, which is why chktex has this check off by default and satex reports it as a suggestion.

Fix by moving the punctuation after the closing mark.

See [chktex 38](https://www.nongnu.org/chktex/).

### stray-space

<p class="badges"><code class="num" title="rule number">S043</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-safe" title="safe: the fix keeps the document's meaning, --fix applies it">S</span></p>

**a macro body picked up a space from an unescaped line end**

Inside a macro's replacement text, TeX turns the end of a line into a space token unless a control word already skipped it or a `%` swallowed the rest of the line ([tex.web § 347](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L7326)).

The finding follows the modes the run read that space in ([§ 1043](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20358): a space is glue only in horizontal mode). It says the macro *inserts* a space where every reading was horizontal, and that it *may* where some might have been. Only a space that a character, box or rule follows in its list is reported. The glue that ends a paragraph goes ([§ 816](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L16066)), and glue before it only widens the end of the last line. One an `\unskip` takes is gone too ([§ 1105](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L21264)), as at `\end{center}`.

Fix by writing `%` at the end of that line.

See [tex.web § 347](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L7326), [tex.web § 816](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L16066), [tex.web § 1043](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L20358).

### table-rules

<p class="badges"><code class="num" title="rule number">S044</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**a table is ruled with vertical rules or `\hline`**

[booktabs](https://ctan.org/pkg/booktabs) gives a table three rules of different weights, `\toprule`, `\midrule` and `\bottomrule`, with space around them, and asks for no vertical rules at all ([§ 2](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L131) of its manual): the columns are separated by their space, and `\hline` sets every rule alike and too close to the text. A table of fewer rows than `lints.table-rules.rows` (3) is left alone.

See [booktabs § 2](https://ctan.org/pkg/booktabs).

### uncited-bibliography-entry

<p class="badges"><code class="num" title="rule number">S055</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a database entry is never cited**

An entry no `\cite` names is left out of the bibliography. That is what a shared database is for, so this is only a note. `\nocite{*}` cites everything.

Fix by citing the entry, or by deleting it.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### unit-in-cells

<p class="badges"><code class="num" title="rule number">S045</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**every number of a table column carries the same unit**

A unit repeated in every cell of a column says the same thing on each line and pushes the numbers out of alignment. The [SI brochure](https://www.bipm.org/en/publications/si-brochure) (9th ed., [§§ 5](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L268), [4](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L231), [1](https://github.com/TeX-Live/texlive-source/blob/4387e30b6c5e9f45b192476533bfaf12cd0764d9/texk/web2c/tex.web#L95)) heads such a column with the quantity and its unit, as `Time / ms`. With [siunitx](https://ctan.org/pkg/siunitx), `{Time / \unit{ms}}` over an `S` column of bare numbers.

See [SI brochure § 5.4.1](https://www.bipm.org/en/publications/si-brochure), [siunitx § \unit](https://ctan.org/pkg/siunitx).

### unreferenced-numbered-display

<p class="badges"><code class="num" title="rule number">S046</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a display that stepped a counter is never referenced, so its number is spent for nothing**

`\begin{equation}` and its relatives step a counter and print a number beside the display. The starred forms set the same mathematics and spend no number.

satex decides *numbered* by running the document: `\refstepcounter` redefines `\@currentlabel` ([clsguide § counters](https://ctan.org/pkg/clsguide)), so an environment whose body redefined it stepped a counter. `\@captype` and `\@listctr` say the number went to something inside, and amsmath's `\notag`, `\nonumber` and `\tag` say the display governs itself.

Fix by writing the starred environment and dropping the label that nothing reads.

See [amsmath § 3.1](https://ctan.org/pkg/amsmath), [clsguide § counters](https://ctan.org/pkg/clsguide).

### unused-bibitem

<p class="badges"><code class="num" title="rule number">S047</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**a `\bibitem` written in the document is never cited**

A `thebibliography` written by hand lists what it lists, cited or not. A database only contributes what is cited, so its entries are not reported. `\nocite{*}` cites everything.

Fix by citing the entry, or by deleting it.

See [btxdoc § Using BibTeX](https://ctan.org/pkg/bibtex).

### unused-color

<p class="badges"><code class="num" title="rule number">S048</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span></p>

**a color the document defines is never used**

The document names a color with `\definecolor`, `\providecolor` or `\colorlet` and never reads it back through `\color`, `\textcolor` or a `\colorlet` source.

Silent for a color a package defines: a library may define more colors than any one document uses. So is a name with `@`, which code built and no document text wrote, and xcolor's `.` for the current color.

See [xcolor § 2.5](https://ctan.org/pkg/xcolor).

### unused-definition

<p class="badges"><code class="num" title="rule number">S049</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a macro defined in the document is never used**

Nothing expands this name. A name some other definition of it gives a meaning that is read is used: a default a later call overrides is no unused name, and `dead-definition` covers a definition replaced before any use.

Fix by deleting it, or by checking why nothing calls it yet.

### unused-label

<p class="badges"><code class="num" title="rule number">S050</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-style">style</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a label is never referenced**

Fix by deleting the `\label`, or by checking the `\ref` that was meant to use it.

</div>

<div class="fold" id="category-performance">
<h3 class="fold-head"><b>performance</b> (6 rules)</h3>

### package-after-preamble

<p class="badges"><code class="num" title="rule number">P001</code> <span class="sev sev-warning" title="default: warning">default: warning</span> <span class="cat cat-performance">performance</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a package is loaded after `\begin{document}`**

`\usepackage` belongs in the preamble. After `\begin{document}` LaTeX raises `\usepackage before \begin{document}` and the package cannot set anything up.

Fix by moving the load into the preamble.

See [source2e § ltclass.dtx](https://ctan.org/pkg/source2e).

### includeonly-available

<p class="badges"><code class="num" title="rule number">P002</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-performance">performance</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a document of several `\include`d parts is built whole**

`\includeonly` names the parts a build reads. The others keep their page numbers and labels from their `.aux` files ([source2e](https://ctan.org/pkg/source2e), ltfiles.dtx). A draft build of one chapter then skips the rest. The rule fires when the document `\include`s at least two parts, no `\includeonly` is in force, and the body is most of what the build interprets. It also has to set a book's worth of paragraph text, `lints.build-cost.characters` in `satex.yaml`: a shorter document builds whole in seconds.

Fix by listing the parts being written in `\includeonly` while drafting.

See [source2e § ltfiles.dtx](https://ctan.org/pkg/source2e).

### preamble-cost

<p class="badges"><code class="num" title="rule number">P003</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-performance">performance</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**the preamble costs time on every build**

The packages the document asks for and the files read before `\begin{document}`, reported when the preamble takes most of the tokens the build interprets and is no small one.

A format holds the preamble already interpreted, so a build that starts from one skips all of it. [mylatexformat](https://ctan.org/pkg/mylatexformat) dumps one from the document's own preamble, and TeX reads a `%&`(format) line before anything else (web2c manual, "Format files").

Fix by dumping the preamble into a format and starting every build from it.

See [The LaTeX Companion § formats](https://ctan.org/pkg/tlc3-examples).

### recursive-macro

<p class="badges"><code class="num" title="rule number">P004</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-performance">performance</span></p>

**a macro calls itself, directly or through others**

Recursion is normal in TeX, but an unguarded one loops until TeX runs out of memory, and the analysis has to widen at it. Everything the recursion would have defined becomes unknown.

Fix, if the recursion is unintended, by breaking the cycle. Otherwise make sure a conditional terminates it.

### unused-dependency

<p class="badges"><code class="num" title="rule number">P005</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-performance">performance</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**the dependency file lists a TeX Live package no file of the run comes from**

Every installation made from the dependency file carries the package, but the run reads nothing from it. It may still be needed for what satex does not read (fonts, hyphenation patterns, Lua libraries, programs), so this is a hint, not a verdict.

Fix, if the package really is unused, by deleting its line.

### unused-package

<p class="badges"><code class="num" title="rule number">P006</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-performance">performance</span> <span class="fix fix-unsafe" title="unsafe: the fix changes the meaning, only --unsafe-fixes applies it">U</span></p>

**a loaded package defines nothing the document uses**

Every name the package defines is unused, and no code outside it reads a setting it made (a class's `\textheight` is read for every page). It may still act through hooks or fonts, so this is a hint, not a verdict.

Fix, if the package really is unused, by deleting the `\usepackage`: it is read on every build.

</div>

<div class="fold" id="category-precision">
<h3 class="fold-head"><b>precision</b> (2 rules)</h3>

### analysis-imprecision

<p class="badges"><code class="num" title="rule number">A001</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-precision">precision</span></p>

**the analysis had to widen**

Undecided conditionals are analyzed arm by arm, so they are not reported here. This rule reports the two places where the analysis is incomplete instead: a recursion that was widened, and a budget that was reached. Findings that depend on such a point may be incomplete.

There is nothing to fix in the document. Raising the limits in `satex.yaml` or narrowing the analysis with `load.packages: false` can reduce it.

### assertion-unknown

<p class="badges"><code class="num" title="rule number">A002</code> <span class="sev sev-info" title="default: info">default: info</span> <span class="cat cat-precision">precision</span></p>

**an assertion could not be decided**

The condition of `%\satex@assert` is neither true on every path nor false on an exact one: a value it reads is not known, a path approximated, or no path reads the assertion. `%\satex@assume` can narrow what it reads.

See [Assertions](#assertions).

</div>
