<!-- generated from doc/src/README.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

An abstract interpreter for (La)TeX and related engines. It expands macros, executes primitives and tracks catcode changes and recursion. It is context and path-sensitive and does not typeset the document.
It lints documents (with quick-fixes, and finds unused or missing macros), slices them, analyzes their performance, jumps to definitions and lists package options with what they govern.

<p align="center"><img src="doc/demo.gif" alt="SaTeX demo: version, summary, lint, explain, slice, query, kernel timings and the Vim language server" width="900"></p>

The documentation at
<https://eagleoutice.github.io/satex-analyzer/> covers the assumed [concrete TeX/eTeX semantics][concrete semantics], the [linting rules](https://eagleoutice.github.io/satex-analyzer/wiki/lints), all supported [queries](https://eagleoutice.github.io/satex-analyzer/wiki/queries) and the
[configuration][config].

The first runs of `satex` are slow because it reads, interprets and caches the (La)TeX kernel and the packages it loads. Later runs use the cache.

----

_SaTeX_ is actively developed by [Florian Sihler](https://eagleoutice.github.io/portfolio/) (contact me at: <florian.sihler@uni-ulm.de>) under the [MIT License](LICENSE). Contributions are welcome (see [CONTRIBUTING.md](CONTRIBUTING.md)).

The development and initial Rust codebase is strongly assisted by Claude Code (from architecture descriptions, TeX semantics and [flowR](https://flowr-analysis.github.io/flowr/)). It is still *work in progress*.
Please avoid AI-only contributions. The humans who submit code are responsible for it.

----

## Install

Prebuilt binaries for Linux and macOS (x86-64 and ARM64) and Windows (x86-64) are attached to every [release](../../releases). The [changelog](CHANGELOG.md) lists what changed. With Rust installed, `cargo install satex-analyzer` installs the `satex` command. `cargo install --git https://github.com/EagleoutIce/satex-analyzer` installs the latest version from the repository. To build from source:

```sh
cargo build --release
./target/release/satex --help
```

Shell completion: after installing, run `satex completions install` once. It adds the hook to the startup file of your shell (bash, zsh or fish). `satex completions <shell>` prints the script for bash, zsh, fish, elvish or powershell. Query names and `--set` keys complete from the program itself. See the [command line page](doc/wiki/cli.md).

```sh
source <(satex completions bash)                          # bash: add to ~/.bashrc
source <(satex completions zsh)                           # zsh: add to ~/.zshrc
satex completions fish > ~/.config/fish/completions/satex.fish
```

Current version: 0.2.0. `satex --version` also lists the plugins included in this build and the configuration in effect:

```text
$ satex --version
satex 0.2.0
https://github.com/EagleoutIce/satex-analyzer

plugins: 48 in 11 kinds
  provider            3  texlive, miktex, tectonic
  platform            3  linux, macos, windows
  engine              4  tex, pdftex, luatex, xetex
  kernel              4  latex2e, plain, context, none
  output              4  pdf, dvi, ps, html
  build system        5  latexmk, make, tectonic, arara, l3build
  tools               7  biber, bibtex, makeindex, xindy, makeglossaries, dvips, ps2pdf
  magic comments      3  % !TeX, % !BIB, % arara:
… 17 more lines omitted
```

<details>
<summary>Full satex <code>--help</code> output</summary>

```text
$ satex --help
Static analyzer for TeX and LaTeX

Usage: satex [OPTIONS] [COMMAND]

Commands:
  query         Run a query.  `satex query --list` names them all
  lint          Context-sensitive lints for the input file. `--all` includes libraries
  summary       What the document and every file it loads contribute
  scope         Control sequences visible at a position, for completion
  explain       What a control sequence means and where it comes from
  slice         Slice the document around a control sequence, environment or position: a compilable reconstruction of the main file by default
  controls      Everything a switch, option or conditional governs.  Without a name, the switches and options in effect.  Several names answer in one table, each row's `for` column saying which one it answers
  dependencies  The program dependence graph (data dependencies plus control dependencies) as DOT or JSON
  trace         The execution trace, step by step
  cache         The interpreted LaTeX2e kernel and the package caches.  Without a further command, prints their status
  compile       Build the document with latexmk, or the engine where latexmk is missing, and explain each error and warning the log reports by what the analysis found there.  Never opens a viewer.  The build runs the project's own code, its latexmkrc and what it lets TeX run, so build only projects you trust
  install       Install the packages the document loads that this machine does not have.  Asks first, never elevates, and is never run by `lint --fix`
  tokens        The token stream the mouth produces
  completions   Print the shell completion script: bash, zsh, fish, elvish or powershell.  Values complete from the running binary.  `install [bash|zsh|fish]` registers it in that shell's startup file. Without a shell, it does so in every shell installed or in use
  lsp           A Language Server Protocol server: a thin facade over the queries and lints above, driven by `initialize`/`didOpen`/`didChange` instead of one-shot arguments.  Stdio by default, `--port` switches to TCP
  gui           The browser playground on this machine: opens it on the project in DIR, with the TeX packages installed here, the native language server and the local TeX for the PDF. Edits are saved to the files
  help          Print this message or the help of the given subcommand(s)

Options:
  -V, --version
          The version, the plugins this build offers, and the configuration in effect here

  -h, --help
          Print help (see a summary with '-h')

Input:
  -f, --file <FILE>
          The document to read. Without it, satex reads each root document in the current directory, or stdin if that is not a terminal

      --config <FILE>
          An extra config file, merged on top of every discovered satex.yaml (built-in defaults, the global config, the ones from the filesystem root down to the document's directory)

      --no-config
          Ignore every satex.yaml, discovered or named with --config. Only built-in defaults and `--set` apply

      --profile <NAME>
          Which defaults to apply (usually auto-detected): document, package, class, literate or plain

      --set <PATH=VALUE>
          Overrides an option of `satex.yaml` by its dotted path, the value written as there: `--set limits.memory=8GiB --set load.inputs=false`. `PATH+` appends to a list: `--set lint_off+=[unused-label]`. Repeatable. The flags below win over it

      --no-packages
          Do not load packages. Analyze the document and the kernel only

      --no-classes
          Do not load the document class

      --inject <TEXT>
          Text read at `--at` as if it stood there in the file, such as `\mymacro{x}`: every command then sees what it does in the state the document has there. The file on disk is not changed

      --at <LINE[:COL]>
          Where `--inject` puts its text, `[FILE:]LINE[:COL]`. The main file without FILE, the start of the line without COL

Limits:
      --max-steps <N>
          Stop the interpreter after this many expansion steps

      --max-memory <SIZE>
          What this run may hold at once, as written by `ulimit`: 512M, 4GiB, 0 for no limit

      --threads <N>
          Threads used for program discovery

Output:
  -v, --verbose...
          Report progress: files read (-v), every 100k tokens (-vv), every expansion (-vvv)

      --format <FORMAT>
          Output format: text, json, csv, markdown, dot, github, sarif or lsp

          Possible values:
          - text:     Aligned table for a terminal, with color and links
          - compact:  A short text view. For `satex trace`: what each call of the main file expands to and each conditional's outcome, at most 8 lines
          - json:     One JSON array of objects, one object per record
          - csv:      Comma-separated, one record per line
          - markdown: A Markdown table, for a report or a pull request
          - dot:      Graphviz, for `satex dependencies`
          - github:   GitHub Actions annotations
          - sarif:    SARIF 2.1.0, for code scanning
          - lsp:      Language Server Protocol diagnostics and quick-fix code actions
          - texlog:   A LaTeX log, for editors that read only that (TeXstudio)
          
          [default: text]

      --color <COLOR>
          When to color the output

          Possible values:
          - auto:   Color when stdout is a terminal, off otherwise
          - always: Color even when stdout is not a terminal, for example when piped
          - never:  Never color the output
          
          [default: auto]

      --links
          Turn on terminal hyperlinks from names to their source position, even when auto-detection would leave them off

      --no-links
          Turn off terminal hyperlinks, even when auto-detection would turn them on

      --stats
          Print one line of resource accounting (files, tokens, memory, time) to stderr after the run

      --timings
          Print a per-phase timing breakdown to stderr after the run

      --stats-json
          Print the accounting of `--stats` and `--timings` as one JSON object to stderr after the run
```

</details>

## Linting

Context-sensitive diagnostics: undefined names, duplicates, dead code and misused conditionals.

```text
$ satex lint -f samples/paper.tex
correctness (1)

  ◆ C001 already-defined (1)
    paper.tex:24:1  \repeat is already defined
      24 │ \newcommand{\repeat}[1]{#1\repeat{#1}}
         │ ^^^^^^^^^^^
         = unsafe fix: use `\renewcommand{\repeat}` instead

style (9)

  ● S033 inconsistent-precision (1)
    paper.tex:70:16  numeric column 2 mixes 1 to 2 decimal places: give every number one precision
      70 │     Ours     & 0.92  \\
         │                ^
         = hint: round them by hand, or let siunitx `S` round with `round-mode = places`

  ● S037 numbers-not-aligned (1)
    paper.tex:70:16  numeric column 2 is set `c`: align its decimal separators with siunitx `S`, or at least use `r`
      70 │     Ours     & 0.92  \\
         │                ^
         = hint: add `\usepackage{siunitx}`, set column 2 as `S` and brace its header: `{Score}`

  ● S040 primitive-tex-command (1)
    paper.tex:22:1  `\csname` is plain TeX. Here it uses a control sequence by name
… 49 more lines omitted
```

<details>
<summary> List all rules alongside a description with <code>--rules</code> </summary>

```text
$ satex lint --rules
severity  category     number  code                              message                                                
error     correctness  C001    already-defined                   `\newcommand` redefines an existing name               
error     correctness  C043    assertion-refuted                 an assertion fails on a run of the document            
error     correctness  C002    bibtex-command-under-biblatex     a BibTeX command biblatex replaces stops the run       
error     correctness  C003    build-bibliography-disabled       the document has a bibliography, but the build never…  
error     correctness  C004    build-command-placeholders        an engine command in the latexmkrc lacks %S or %O      
error     correctness  C005    build-engine-mismatch             the build runs another engine, or another output, th…  
error     correctness  C006    build-shell-escape-missing        the document runs programs through `\write18`, but t…  
error     correctness  C007    caused-library-error              the document makes package code raise a TeX error      
error     correctness  C046    invalid-directive                 a `%\satex@` directive cannot be read                  
error     correctness  C008    math-still-open                   the run is still in math mode where the text ends      
error     correctness  C047    missing-end                       the document ends without `\bye` or `\end{document}`   
error     correctness  C009    missing-graphic                   an included image is not there for this output target  
error     correctness  C010    new-redefines                     a `\new…` declaration silently redefines an existing…  
error     correctness  C011    not-defined                       `\renewcommand` targets a name that does not exist     
error     correctness  C012    option-clash                      a package is loaded twice with different options       
error     correctness  C013    raised-error                      the document's code raises a TeX error                 
error     correctness  C014    undefined-color                   a color is used, or aliased from, before xcolor ever…  
error     correctness  C015    undefined-control-sequence        a control sequence is used that nothing defines        
error     correctness  C016    undefined-environment             `\begin` names an environment that is not defined      
error     correctness  C017    undefined-glossary-entry          a glossary or acronym entry is used but never declar…  
error     correctness  C018    unguarded-recursion               a recursive macro has nothing that can stop it         
warning   correctness  C052    bibliography-entry-missing-field  a cited entry lacks a field its type requires          
warning   correctness  C019    build-missing-custom-dependency   the document writes glossary files, but latexmk has …  
warning   correctness  C020    catcode-escapes-group             a category code change is still in force at the end …  
warning   correctness  C021    conflicting-color-definition      the same color name is defined twice with different …  
warning   correctness  C022    delimiter-not-followed            `\left`, `\right` or `\middle` is followed by a brac…  
warning   correctness  C023    duplicate-bibliography-entry      the same key is declared by two bibliography entries   
warning   correctness  C050    duplicate-bibliography-load       a database is loaded twice                             
warning   correctness  C051    duplicate-bibliography-work       two bibliography entries name one work under differe…  
warning   correctness  C024    duplicate-label                   the same label is defined twice                        
warning   correctness  C025    environment-mismatch              `\end` names an environment other than the one that …  
warning   correctness  C026    expl-syntax-left-on               `\ExplSyntaxOn` is never turned off                    
warning   correctness  C027    expl3-constant-set                an expl3 constant is set after it was declared         
warning   correctness  C028    expl3-signature-mismatch          an expl3 function takes other arguments than its nam…  
warning   correctness  C029    ignored-verbatim-tail             text stands on the line that closes a verbatim body …  
warning   correctness  C030    math-script-braces                `^` and `_` take one token, so what follows needs br…  
warning   correctness  C031    missing-dependency                a TeX Live package the run reads from is not in the …  
warning   correctness  C032    unbalanced-brackets               a file sets more of one bracket than of its partner    
warning   correctness  C033    unbalanced-color-stack            a `\pdfcolorstack` push is never popped, or a pop ha…  
warning   correctness  C034    unbalanced-pdf-content            a PDF literal opens marked content or a graphics sta…  
warning   correctness  C053    undefined-bibliography-string     a `.bib` value uses a constant no `@string` defines    
warning   correctness  C035    undefined-citation                a citation names a key no bibliography entry declares  
warning   correctness  C048    undefined-cref-format             cleveref has no name for a label's type, so `\cref` …  
warning   correctness  C036    undefined-optional-content        marked content refers to an optional content group n…  
warning   correctness  C037    undefined-reference               `\ref` names a label that is never defined             
warning   correctness  C038    uninstalled-package               the package is not installed, and the distribution c…  
warning   correctness  C049    unknown-code-language             a listings key names a language nothing defines        
warning   correctness  C039    unresolved-file                   a package, class or input file is not in the install…  
info      correctness  C045    assumption-made                   the document assumes a condition                       
info      correctness  C040    expected-load                     the file was read after a package it expects loaded    
info      correctness  C041    math-mode-mismatch                \(, \), \[ or \] stands where the mode does not allo…  
info      correctness  C042    unintended-escape                 a command is followed by an escaped punctuation mark…  
hint      correctness  C044    assertion-proved                  an assertion holds on every path                       
warning   security     X001    build-shell-escape-unneeded       the build allows `\write18`, but the document never …  
warning   security     X002    shell-escape                      the document runs a program through `\write18`         
warning   style        S001    accent-on-dotted-letter           an accent sits on top of the dot of an `i` or a `j`    
warning   style        S002    command-terminated-space          a control word ate the space after it and the next w…  
warning   style        S003    dimension-times                   the letter x stands between two numbers where a mult…  
warning   style        S004    double-space                      two interword spaces reached the page, one of them f…  
warning   style        S005    ellipsis-dots                     three periods set three periods, not an ellipsis       
warning   style        S054    et-al                             `et al.` spelled another way, or a citation number t…  
warning   style        S006    expl3-scope                       an expl3 variable's scope prefix disagrees with how …  
warning   style        S007    expl3-type                        an expl3 variable's type suffix disagrees with the f…  
warning   style        S008    group-local-definition            a definition is made inside a group and lost when it…  
warning   style        S009    math-operator-name                an operator name is set in math italic instead of by…  
warning   style        S010    missing-italic-correction         a slanted letter runs into upright text with no `\/`…  
warning   style        S011    missing-nonbreaking-space         an ordinary space in front of a reference lets the l…  
warning   style        S012    parenthesis-spacing               a parenthesis carries the space of the text inside i…  
warning   style        S013    quote-direction                   a quotation mark points the wrong way, or a run of t…  
warning   style        S014    sentence-spacing                  a period set the wrong width of space for the senten…  
warning   style        S015    space-before-command              a space lets the line break between a word and the m…  
warning   style        S016    space-before-punctuation          a space stands between a word and the punctuation af…  
warning   style        S017    stray-italic-correction           a repeated italic correction, or one in front of a p…  
warning   style        S018    unknown-suppress-code             a satex-disable comment names a code no rule has       
warning   style        S019    user-pattern                      the source matches a pattern the project asked to be…  
warning   style        S020    wrong-dash                        a run of hyphens sets a dash of the wrong length for…  
warning   style        S021    wrong-quote-character             a character that is not a quotation mark stands in f…  
warning   style        S022    zero-italic-correction            the italic correction is zero here, so `\/` changes …  
info      style        S023    build-bibliography-unneeded       the build configures BibTeX or biber for a document …  
info      style        S024    build-engine-options              the engine command could report errors as file:line …  
info      style        S025    build-unused-custom-dependency    a latexmk custom dependency that nothing in the docu…  
info      style        S053    clones                            a block of the source repeats with only its values c…  
info      style        S026    comment-marker                    a comment still carries a TODO-style marker            
info      style        S027    computer-modern-in-t1             T1 text in Computer Modern relies on cm-super for sc…  
info      style        S028    csquotes-available                the document sets quotation marks by hand instead of…  
info      style        S029    dead-definition                   a definition is replaced before anything uses it       
info      style        S030    duplicate-color-value             two color names store the exact same color             
info      style        S031    duplicate-package                 a package is loaded more than once                     
info      style        S032    hand-set-quantity                 a number and its unit are set by hand where siunitx …  
info      style        S052    hand-written-author               an author name typed before a citation that prints i…  
info      style        S051    hand-written-reference            a number typed after a reference name, where `\ref` …  
info      style        S033    inconsistent-precision            the numbers of a table column give different numbers…  
info      style        S034    math-punctuation                  punctuation sits on the wrong side of a formula        
info      style        S035    math-shift-dollars                display math opened with `$$` where LaTeX spells it …  
info      style        S036    microtype-available               the engine can protrude characters and expand fonts,…  
info      style        S037    numbers-not-aligned               a table column of numbers does not line them up at t…  
info      style        S038    ot1-font-encoding                 accented letters are built with `\accent` because th…  
info      style        S039    prefer-cleveref                   a name written before `\ref` that cleveref prints by…  
info      style        S040    primitive-tex-command             a plain TeX primitive is used where LaTeX has its ow…  
info      style        S041    quote-ligature                    three quotation marks in a row set a double and a si…  
info      style        S042    quote-punctuation                 punctuation stands inside the closing quotation mark   
info      style        S043    stray-space                       a macro body picked up a space from an unescaped lin…  
info      style        S044    table-rules                       a table is ruled with vertical rules or `\hline`       
info      style        S055    uncited-bibliography-entry        a database entry is never cited                        
info      style        S045    unit-in-cells                     every number of a table column carries the same unit   
info      style        S046    unreferenced-numbered-display     a display that stepped a counter is never referenced…  
info      style        S047    unused-bibitem                    a `\bibitem` written in the document is never cited    
info      style        S048    unused-color                      a color the document defines is never used             
info      style        S049    unused-definition                 a macro defined in the document is never used          
info      style        S050    unused-label                      a label is never referenced                            
warning   performance  P001    package-after-preamble            a package is loaded after `\begin{document}`           
info      performance  P002    includeonly-available             a document of several `\include`d parts is built who…  
info      performance  P003    preamble-cost                     the preamble costs time on every build                 
info      performance  P004    recursive-macro                   a macro calls itself, directly or through others       
info      performance  P005    unused-dependency                 the dependency file lists a TeX Live package no file…  
info      performance  P006    unused-package                    a loaded package defines nothing the document uses     
info      precision    A001    analysis-imprecision              the analysis had to widen                              
info      precision    A002    assertion-unknown                 an assertion could not be decided                      
```

</details>
<details>
<summary>Explain one rule with <code>--explain</code></summary>

```text
$ satex lint --explain unguarded-recursion
C018 unguarded-recursion  correctness / error
a recursive macro has nothing that can stop it

The run saw the macro's expansion reach the macro again, could not see the
recursion end, and found no conditional (no `\if…`, `\else` or `\fi`) to
end it.  A macro nothing expands is judged by the names its replacement text
holds.  A real run expands until TeX runs out of memory:
`TeX capacity exceeded, sorry [main memory size]`.

Fix by adding the test that stops it, or by removing the self-reference.
```

</details>

Findings marked `[fix]` are repaired by `satex lint --fix`, those marked `[unsafe fix]` by `--fix --unsafe-fixes` (these could theoretically change the resulting document).
You can use `--diff` to show the specific edits. See [quick-fixes](doc/wiki/lints.md#quick-fixes).

For CI, output formats include `--format github` (GitHub Actions annotations) and `--format sarif` (code scanning).

<details>
<summary>Show the GitHub Actions with <code>--format github</code></summary>

```text
$ satex lint -f samples/paper.tex --format github
::error file=samples/paper.tex,line=24,col=1,title=C001 already-defined::\repeat is already defined
::notice file=samples/paper.tex,line=76,col=1,title=S050 unused-label::label `sec:conclusion` is never referenced
::notice file=samples/paper.tex,line=42,col=1,title=S050 unused-label::label `sec:method` is never referenced
::notice file=samples/paper.tex,line=22,col=1,title=S040 primitive-tex-command::`\csname` is plain TeX. Here it uses a control sequence by name
::notice file=samples/preamble.tex,line=1,col=1,title=S049 unused-definition::\hello is never used
::notice file=samples/paper.tex,line=14,col=1,title=S049 unused-definition::\WeirdMore is never used
… 5 more lines omitted
```

</details>

## Comprehension

These commands explain existing documents and their macros.

### Explaining Macros

With `explain` you can ask SaTeX what a specific macro does.
`explain` may list multiple meanings of a macro if they differ (e.g. in the LaTeX document).
Use `--all` to obtain lists every definition and `--at` to ask at a specific place in the document: `preamble`, `document`, `LINE`, `FILE:LINE` or `after:PACKAGE`.

<details>
<summary>Explain the <code>\usepackage</code> macro</summary>

```text
$ satex explain -f samples/paper.tex '\usepackage'
\usepackage @ latex.ltx:18619:25
  tag            alias
  when           preamble
  context        outside environments
  effective      \usepackage[1]{2}[3]
  signature      omo
  roles          #2 package
  types          #1 options, #2 package (.sty), #3 date, at least
  takes          1-3
  uses           4 times, in this run
  by             \let
  does           Loads packages.
  documentation  The LaTeX2e sources, [source2e](https://ctan.org/pkg/source2e)
  expands        \@fileswithoptions
  body           \@fileswithoptions \@pkgextension 
  reference      https://texdoc.org/serve/source2e/0
  file           /usr/local/texlive/YYYY/texmf-dist/tex/latex/base/latex.ltx:18619

\usepackage @ latex.ltx:9521:22
  tag            alias
  when           document
  context        outside environments
  effective      \usepackage
  error          LaTeX Error: Can be used only in preamble.
  roles          #1 package
  types          #1 package (.sty)
  takes          0
  uses           0 times, in this run
  by             \let
  does           Loads packages.
  documentation  The LaTeX2e sources, [source2e](https://ctan.org/pkg/source2e)
  may_expand     \@fileswithoptions
  body           \@latex@error {Can be used only in preamble}\@eha 
  reference      https://texdoc.org/serve/source2e/0
  file           /usr/local/texlive/YYYY/texmf-dist/tex/latex/base/latex.ltx:9521
```
</details>

<details>
<summary>Explain a user-defined macro <code>--at</code> document</summary>

```text
$ satex explain -f samples/paper.tex '\norm'
\norm @ paper.tex:12:13
  tag         macro
  context     outside environments
  effective   \norm{1}
  signature   m
  takes       1
  uses        1 time, in this run
  by          \newcommand
  expands     \left, \lVert, \right
  may_expand  \hat
  body        \left \lVert #1\right \rVert 
  file        samples/paper.tex:12
```

</details>

See the [explain section](doc/wiki/queries.md#explain) of the queries page, and [signatures](doc/wiki/queries.md#signatures) for what the signature keys mean.

### Document Summaries

A document summary shows the commands, dependencies, and metadata of a document. 

<details>
<summary>What the document contains with <code>summary</code></summary>

```text
$ satex summary -f samples/paper.tex
paper.tex
  identified LaTeX2e document, class article [2025/01/22 v1.4n Standard LaTeX document class][11pt]
  title A Sample Document for saTeX
  author satex Test Suite
  engine pdflatex (build configuration)
  provider texlive (TeX 3.141592653 (TeX Live 2026)) (detected)
  platform linux (this machine)
  kernel latex2e (default)
… 28 more lines omitted
```

### Explore Package Options and Feature Flags

The `controls` command shows the switches and options a package or class defines, and what each governs. You can also use `query options` to see what keys an environment's optional argument takes.

</details>
<details>
<summary>Switches and options, and what each one governs, with <code>controls</code></summary>

```text
$ satex controls -f samples/paper.tex
tag     name       count  value                                  governs  file           
option  article           11pt                                            paper.tex      @ paper.tex:1:1
switch  \ifdraft          false                                  236      paper.tex      @ paper.tex:3:7
offers  amsmath    15     alignedleftspaceno, alignedleftspace…           amsmath.sty    @ amsmath.sty:45:16
… 9 more lines omitted
```

</details>
<details>
<summary>The keys an environment's optional argument takes with <code>query options</code></summary>

```text
$ satex query options -f samples/paper.tex --for itemize
```

</details>
<details>
<summary>Slice the document around a name, position or query with <code>slice</code></summary>

A compilable document by default. `--where EXPR` slices to every occurrence or definition a query accepts (`--where 'kind=begin-environment and key=figure'` for every figure), `--list` prints the matching records instead, and `--out DIR` writes the slice out as its own small project.

```text
$ satex slice -f samples/paper.tex '\ifdraft'
\begin{filecontents*}[overwrite]{mypackage.sty}
\NeedsTeXFormat{LaTeX2e}
\ProvidesPackage{mypackage}[2024/01/01 v1.0 Sample package for satex]

\RequirePackage{xcolor}

… 58 more lines omitted
```

</details>
<details>
<summary>What is visible at a position, for completion, with <code>scope</code></summary>

```text
$ satex scope -f samples/paper.tex --at 20:1
tag          name                           effecti…  does      file                
macro        \Weird                         \Weird …            paper.tex           @ paper.tex:16:1
macro        \WeirdMore                     \WeirdM…            paper.tex           @ paper.tex:14:1
… 10265 more lines omitted
```

</details>

## Performance

Shows what a build costs: which packages and files dominate, and what to do about it.

```text
$ satex lint -f samples/paper.tex --filter 'category=performance'
performance (1)

  ● P003 preamble-cost (1)
    paper.tex:26:1  5 packages requested directly, 275 files read, 82% of the tokens the build takes
      26 │ \begin{document}
         │ ^^^^^^
… 5 more lines omitted
```

`--timings` breaks the time down per phase and file, `--stats` sums up counts and wall time. Both measure the run itself and change between runs.
The kernel and the packages are interpreted once and cached. `satex cache build` prepares the packages in `cache.index` ahead of time.

<details>
<summary>The cached kernel with <code>cache</code></summary>

```text
$ satex cache
cache
  source /usr/local/texlive/YYYY/texmf-dist/tex/latex/base/latex.ltx
  holds 197802 definitions
  state cached
… 2 more lines omitted
```

</details>

## Queries

Structured facts for other tools. See the [queries wiki page](doc/wiki/queries.md) for more details.

The commands a document defines itself, without packages and without internal `@` names:

```text
$ satex query definitions -f samples/paper.tex --filter 'origin=document and not package~. and not name~@'
tag     name         parameters    comment                           body      file          
switch  \ifdraft                                                               paper.tex     @ paper.tex:3:7
switch  \drafttrue                                                   \let \i…  paper.tex     @ paper.tex:3:7
switch  \draftfalse                                                  \let \i…  paper.tex     @ paper.tex:3:7
macro   \hello                                                       world     preamble.tex  @ preamble.tex:1:1
macro   \highlight   #1            Standard single-argument macros.  \textbf…  paper.tex     @ paper.tex:11:13
macro   \norm        #1                                              \left \…  paper.tex     @ paper.tex:12:13
macro   \WeirdMore   super #1\;                                       #1       paper.tex     @ paper.tex:14:1
… 2 more lines omitted
```

<details>
<summary>List all queries with <code>--list</code></summary>

```text
$ satex query --list
name              
calls             
catcodes          
definitions       
dependencies      
dependency-graph  
diagnostics       
distribution      
expansions        
files             
gaps              
occurrences       
options           
pgfkeys           
plugins           
previews          
produces          
project           
recursion         
side-effects      
trace             
type-plugins      
typeset           
```

</details>

<details>
<summary>Several queries over one analysis with <code>--request</code></summary>

```json
{
  "file": "samples/mypackage.sty",
  "queries": [
    {"type": "explain", "names": ["\\mybox"]},
    {"type": "controls", "names": ["my@draft"]},
    {"type": "options", "name": "mybanner"}
  ]
}
```

`satex query --request FILE` (or `-` for stdin) answers each in order, as one JSON array. A `config` object overrides settings as `--set` does.

</details>
<details>
<summary>Every step of the run with <code>trace</code></summary>

```text
$ satex trace -f samples/paper.tex
index   step         name                        detail                    file           
678     expand       \documentclass                                        paper.tex      @ paper.tex:1:1
8412    open-file    \article                    /usr/local/texlive/YYYY…  paper.tex      @ paper.tex:1:1
10261   expand       \par                                                  paper.tex      @ paper.tex:2:1
10268   expand       \newif                                                paper.tex      @ paper.tex:3:1
… 185 more lines omitted
```

</details>
<details>
<summary>The token stream with <code>tokens</code></summary>

```text
$ satex tokens -f samples/paper.tex
kind  name            detail  
cs    \documentclass          @ 1:1
char  [               cat 12  @ 1:15
char  1               cat 12  @ 1:16
char  1               cat 12  @ 1:17
… 1210 more lines omitted
```

</details>

Every command prints text by default. You can use `--format json`, `csv` or `markdown` whenever it prints rows. Some formats like `github`, `sarif` and `lsp` may be specific to commands like `lint`.

## Editor integration

`satex lsp` runs a Language Server: diagnostics, hover, definitions, references, completion, document symbols and quick fixes, all backed by the commands above. Stdio by default, `--port`/`--host` for TCP. See the [LSP wiki page](doc/wiki/lsp.md) for the protocol details and Neovim/VS Code setup.

`satex gui [DIR]` opens the browser playground on a project of your computer: its files, the TeX packages installed there, the native language server and your TeX for the PDF. Edits are saved to the files. See [the playground on your machine](doc/wiki/cli.md#the-playground-on-your-machine).

## Configuration

SaTeX merges its built-in defaults, your global `~/.config/satex/satex.yaml`, every `satex.yaml` from the filesystem root down to the document, `--config FILE`, and `--set PATH=VALUE`, each overriding the one before. `satex --version` lists what was used. Plugins detect the installation, engine, output and build system (latexmk, make, l3build, …). TeX, e-TeX, pdfTeX and LaTeX2e are modeled in full. XeTeX, LuaTeX and ConTeXt are experimental. See the [configuration wiki page](doc/wiki/configuration.md).

## How it works

SaTeX has no abstract syntax tree in the classical sense and it not syntax-guided. When analyzing TeX a token's meaning depends on category codes and on what earlier macros performed, so the token stream only exists while the document runs. 
SaTeX follows an expansion-guided approach instead using TeX's  machine (mouth, gullet, stomach, save stack).

Concretely:

- **State.** Meanings of control sequences, category codes, register values
  and the save stack, are tracked during the analysis. A value SaTeX cannot decide during resolution is `unknown`, (which propagates).
- **Transfer.** Every primitive has the effect associated with the [concrete TeX/eTeX semantics][concrete semantics]. 
- **Branching.** We analyze every branch of a conditional if the test is not deterministic. This also applies transitively to macro-expansions.
- **Termination.** We widen (potentially unbounded) recursive expansion based on the budgets in [`limits`][config]. 

## Gaps

Use `satex query gaps -f <document>` to get details on noted issues SaTeX has with analyzing the document (and its dependencies).

## Related Work

There are various preexisting analysis approaches for TeX, albeit none offer full support for macro expansion, catcode manipulations, build-systems, and the like. However, based on your need, you may find the following useful:

- [Featherweight TeX](https://doi.org/10.1007/978-3-642-19440-5_26) is a formal model of TeX's macro system, aimed at parser correctness (rather than analyzing real documents).
- [Tylax](https://github.com/scipenai/tylax) converts between LaTeX and Typst with support for basic macro expansion.
- [unravel](https://ctan.org/pkg/unravel) steps through TeX's expansion as a debugger, but runs one concrete execution for this.
- [TexLab](https://github.com/latex-lsp/texlab) is a language server that gives editor features from a syntax tree, without expanding macros.
- [unified-latex](https://github.com/siefkenj/unified-latex) parses LaTeX into a syntax tree for transformation, and does not expand macros in general.
- [ChkTeX](https://www.nongnu.org/chktex/) is a token-level linter for typographic and style mistakes (it does not expand macros). The [chktex coverage table](https://eagleoutice.github.io/satex-analyzer/chktex-coverage) lists its 49 warnings and the satex rule for each.


[concrete semantics]: https://eagleoutice.github.io/satex-analyzer/SEMANTICS
[config]: https://eagleoutice.github.io/satex-analyzer/wiki/configuration