<!-- generated from doc/src/README.md.in by satex-doc; do not edit -->

# SaTeX - Static Analysis for (La)TeX and Friends

An abstract interpreter for (La)TeX and related engines with the ability to expands macros, execute primitives, track dynamic catcode changes, recursion, and more. While SaTeX is context and path-sensitive, it does not typeset the document.
Instead, SaTeX can lint your documents to find various issues (including quick-fixes and the identification of unused or missing macros), slice your document, analyze its performance, jump to definitions,  explore package options and their consequences.

<p align="center"><img src="doc/demo.gif" alt="SaTeX demo: version, summary, lint, explain, slice, query, kernel timings and the Vim language server" width="900"></p>

Please consult the documentation published at
<https://eagleoutice.github.io/satex-analyzer/> with details on the assumed [concrete TeX/eTeX semantics][concrete semantics], the [linting rules](https://eagleoutice.github.io/satex-analyzer/wiki/lints), all supported [queries](https://eagleoutice.github.io/satex-analyzer/wiki/queries) and the
[configuration][config].

Please note that the first run(s) of `satex` may be slow because it reads, interprets, and caches the (La)TeX kernel and the packages it requires for the first time. Subsequent runs are faster because they use the cache.

----

_SaTeX_ is actively developed by *Florian Sihler* (contact me at: <florian.sihler@uni-ulm.de>) under the [MIT License](LICENSE). I am very happy about every contribution (see [CONTRIBUTING.md](CONTRIBUTING.md)).

The development and initial Rust codebase is strongly assisted by Claude Code (based on detailed architecture descriptions, TeX semantics, and [flowR](https://flowr-analysis.github.io/flowr/)). Please consider it to be still *work in progress*.
Yet, please avoid AI-only contributions. Humans are still considered responsible for the code submitted.

----

## Install

Prebuilt binaries for Linux, macOS and Windows (x86-64 and ARM64) are attached to every [release](../../releases). With Rust installed, `cargo install satex-analyzer` installs the `satex` command. To build from source:

```sh
cargo build --release
./target/release/satex --help
```

Current version: 0.1.0. `satex --version` also lists the plugins included in this build and the configuration in effect:

```text
$ satex --version
satex 0.1.0
https://github.com/EagleoutIce/satex-analyzer

plugins: 46 in 11 kinds
  provider            3  texlive, miktex, tectonic
  platform            3  linux, macos, windows
  engine              4  tex, pdftex, luatex, xetex
  kernel              4  latex2e, plain, context, none
  output              4  pdf, dvi, ps, html
  build system        5  latexmk, make, tectonic, arara, l3build
  tools               7  biber, bibtex, makeindex, xindy, makeglossaries, dvips, ps2pdf
  magic comments      3  % !TeX, % !BIB, % arara:
… 7 more lines omitted
```

<details>
<summary>Full satex <code>--help</code> output</summary>

```text
$ satex --help
Static analyzer for TeX and LaTeX

Usage: satex [OPTIONS] [COMMAND]

Commands:
  query         Run a query.  `satex query --list` names them all
  lint          Context-sensitive lints for the input file; `--all` includes libraries
  summary       What the document and every file it loads contribute
  scope         Control sequences visible at a position, for completion
  explain       What a control sequence means and where it comes from
  slice         Slice the document around a control sequence, environment or position: a compilable reconstruction of the main file by default
  controls      Everything a switch, option or conditional governs.  Without a name, the switches and options in effect.  Several names answer in one table, each row's `for` column saying which one it answers
  dependencies  The program dependence graph — data dependencies plus control dependencies — as DOT or JSON
  trace         The execution trace, step by step
  cache         The interpreted LaTeX2e kernel and the package caches.  Without a further command, prints their status
  tokens        The token stream the mouth produces
  lsp           A Language Server Protocol server: a thin facade over the queries and lints above, driven by `initialize`/`didOpen`/`didChange` instead of one-shot arguments.  Stdio by default; `--port` switches to TCP
  help          Print this message or the help of the given subcommand(s)

Options:
  -V, --version
          The version, the plugins this build offers, and the configuration in effect here

  -h, --help
          Print help (see a summary with '-h')

Input:
  -f, --file <FILE>
          The document to read. Without it, satex looks for one root document in the current directory, or reads stdin if that is not a terminal

      --config <FILE>
          An extra config file, merged on top of every discovered satex.yaml (built-in defaults, the global config, the ones from the filesystem root down to the document's directory)

      --no-config
          Ignore every satex.yaml, discovered or named with --config — built-in defaults and `--set` only

      --profile <NAME>
          Which defaults to apply (usually auto-detected): document, package, class, literate or plain

      --set <PATH=VALUE>
          Overrides an option of `satex.yaml` by its dotted path, the value written as there: `--set limits.memory=8GiB --set load_inputs=false`. Repeatable; the flags below win over it

      --no-packages
          Do not load packages; analyze the document and the kernel only

      --no-classes
          Do not load the document class

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
          - json:     One JSON array of objects, one object per record
          - csv:      Comma-separated, one record per line
          - markdown: A Markdown table, for a report or a pull request
          - dot:      Graphviz, for `satex dependencies`
          - github:   GitHub Actions annotations
          - sarif:    SARIF 2.1.0, for code scanning
          - lsp:      Language Server Protocol diagnostics and quick-fix code actions
          
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
```

</details>

## Linting

Provides context-sensitive diagnostics like undefined names, duplicates, dead code, and misused conditionals.

```text
$ satex lint -f samples/paper.tex
errors (1)
  paper.tex:24:1  error    already-defined  \repeat is already defined
    [unsafe fix] use \renewcommand{\repeat} instead

suggestions (7)
  paper.tex:14:1  info     unused-definition  \WeirdMore is never used
    [unsafe fix] delete \WeirdMore
  paper.tex:16:1  info     unused-definition  \Weird is never used
… 21 more lines omitted
```

<details>
<summary> List all rules alongside a description with <code>--rules</code> </summary>

```text
$ satex lint --rules
severity  category     code                             message                                                         
error     correctness  already-defined                  \newcommand redefines an existing name                          
error     correctness  build-bibliography-disabled      the document has a bibliography, but the build never runs Bib…  
error     correctness  build-command-placeholders       an engine command in the latexmkrc lacks %S or %O               
error     correctness  build-engine-mismatch            the build runs another engine, or another output, than the do…  
error     correctness  build-shell-escape-missing       the document runs programs through \write18, but the build do…  
error     correctness  duplicate-label                  the same label is defined twice                                 
error     correctness  missing-graphic                  an included image is not there for this output target           
error     correctness  not-defined                      \renewcommand targets a name that does not exist                
error     correctness  option-clash                     a package is loaded twice with different options                
error     correctness  raised-error                     the document's code raises a TeX error                          
error     correctness  undefined-control-sequence       a control sequence is used that nothing defines                 
error     correctness  undefined-environment            \begin names an environment that is not defined                 
error     correctness  undefined-glossary-entry         a glossary or acronym entry is used but never declared          
error     correctness  unguarded-recursion              a recursive macro has nothing that can stop it                  
warning   correctness  build-missing-custom-dependency  the document writes glossary files, but latexmk has no rule t…  
warning   correctness  catcode-escapes-group            a category code change is still in force at the end of the run  
warning   correctness  duplicate-bibliography-entry     the same key is declared by two bibliography entries            
warning   correctness  environment-mismatch             \end names an environment other than the one that is open       
warning   correctness  expl-syntax-left-on              \ExplSyntaxOn is never turned off                               
warning   correctness  expl3-signature-mismatch         an expl3 function takes other arguments than its name says      
warning   correctness  missing-dependency               a TeX Live package the run reads from is not in the dependenc…  
warning   correctness  unbalanced-pdf-content           a PDF literal opens marked content or a graphics state that i…  
warning   correctness  undefined-citation               a citation names a key no bibliography entry declares           
warning   correctness  undefined-optional-content       marked content refers to an optional content group no resourc…  
warning   correctness  undefined-reference              \ref names a label that is never defined                        
warning   correctness  unresolved-file                  a package, class or input file is not in the installation       
warning   security     build-shell-escape-unneeded      the build allows \write18, but the document never uses it       
warning   security     shell-escape                     the document runs a program through \write18                    
warning   style        group-local-definition           a definition is made inside a group and lost when it closes     
warning   style        unknown-suppress-code            a satex-disable comment names a code no rule has                
info      style        build-bibliography-unneeded      the build configures BibTeX or biber for a document without a…  
info      style        build-engine-options             the engine command could report errors as file:line and write…  
info      style        build-unused-custom-dependency   a latexmk custom dependency that nothing in the document trig…  
info      style        computer-modern-in-t1            T1 text in Computer Modern relies on cm-super for scalable fo…  
info      style        dead-definition                  a definition is replaced before anything uses it                
info      style        hand-set-quantity                a number and its unit are set by hand where siunitx would set…  
info      style        microtype-available              the engine can protrude characters and expand fonts, but micr…  
info      style        ot1-font-encoding                accented letters are built with \accent because the text is s…  
info      style        primitive-tex-command            a plain TeX primitive is used where LaTeX has its own interfa…  
info      style        stray-space                      a macro body picked up a space from an unescaped line end       
info      style        unused-bibitem                   a \bibitem written in the document is never cited               
info      style        unused-definition                a macro defined in the document is never used                   
info      style        unused-label                     a label is never referenced                                     
warning   performance  duplicate-package                a package is loaded more than once                              
warning   performance  package-after-preamble           a package is loaded after \begin{document}                      
info      performance  preamble-cost                    what the preamble costs on every build                          
info      performance  recursive-macro                  a macro calls itself, directly or through others                
info      performance  unused-dependency                the dependency file lists a TeX Live package no file of the r…  
info      performance  unused-package                   a loaded package defines nothing the document uses              
info      precision    analysis-imprecision             the analysis had to widen                                       
```

</details>
<details>
<summary>Explain one rule with <code>--explain</code></summary>

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

</details>

Findings marked `[fix]` are repaired by `satex lint --fix`, those marked `[unsafe fix]` by `--fix --unsafe-fixes` (these could theoretically change the resulting document).
You can use `--diff` to show the specific edits. See [quick-fixes](doc/wiki/lints.md#quick-fixes).

For CI, output formats include `--format github` (GitHub Actions annotations) and `--format sarif` (code scanning).

<details>
<summary>Show the GitHub Actions with <code>--format github</code></summary>

```text
$ satex lint -f samples/paper.tex --format github
::error file=samples/paper.tex,line=24,col=1,title=already-defined::\repeat is already defined
::notice file=samples/paper.tex,line=75,col=1,title=unused-label::label `sec:conclusion` is never referenced
::notice file=samples/paper.tex,line=42,col=1,title=unused-label::label `sec:method` is never referenced
::notice file=samples/paper.tex,line=22,col=1,title=primitive-tex-command::\csname is plain TeX; here it uses a control sequence by name
::notice file=samples/preamble.tex,line=1,col=1,title=unused-definition::\hello is never used
::notice file=samples/paper.tex,line=14,col=1,title=unused-definition::\WeirdMore is never used
… 4 more lines omitted
```

</details>

## Comprehension

SaTeX can support you in understanding existing documents and their macros.

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
  takes          1-3
  uses           4 times, in this run
  by             \let
  documentation  The LaTeX2e sources, source2e
  body           \@fileswithoptions \@pkgextension 
  reference      https://texdoc.org/serve/source2e/0
  file           /usr/local/texlive/2026/texmf-dist/tex/latex/base/latex.ltx:18619

\usepackage @ latex.ltx:9521:22
  tag            alias
  when           document
  context        outside environments
  effective      \usepackage
  error          LaTeX Error: Can be used only in preamble.
  takes          0
  uses           0 times, in this run
  by             \let
  documentation  The LaTeX2e sources, source2e
  body           \@latex@error {Can be used only in preamble}\@eha 
  reference      https://texdoc.org/serve/source2e/0
  file           /usr/local/texlive/2026/texmf-dist/tex/latex/base/latex.ltx:9521
```
</details>

<details>
<summary>Explain a user-defined macro <code>--at</code> document</summary>

```text
$ satex explain -f samples/paper.tex '\norm'
\norm @ paper.tex:12:13
  tag        macro
  context    outside environments
  effective  \norm{1}
  signature  m
  takes      1
  uses       1 time, in this run
  by         \newcommand
  expands    \left, \hat, \hat , \mathaccentV, \right
  body       \left \lVert #1\right \rVert 
  file       /home/ostwind/git/phd/satex/samples/paper.tex:12
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
tag     name       count  value                                   governs  file           
option  article           11pt                                             paper.tex      @ paper.tex:1:1
switch  \ifdraft          false                                   270      paper.tex      @ paper.tex:3:7
offers  amsmath    15     alignedleftspaceno, alignedleftspacey…           amsmath.sty    @ amsmath.sty:45:16
… 11 more lines omitted
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
\documentclass[11pt]{article}
\newif\ifdraft
\usepackage{amsmath}
\usepackage{graphicx}
\usepackage{hyperref}
\usepackage{mypackage}
… 16 more lines omitted
```

</details>
<details>
<summary>What is visible at a position, for completion, with <code>scope</code></summary>

```text
$ satex scope -f samples/paper.tex --at 20:1
tag          name                                      effecti…  file                
macro        \Weird                                    \Weird …  paper.tex           @ paper.tex:16:1
macro        \WeirdMore                                \WeirdM…  paper.tex           @ paper.tex:14:1
… 7766 more lines omitted
```

</details>

## Performance

Shows what a build costs: which packages and files dominate, and what to do about it.

```text
$ satex lint -f samples/paper.tex --filter 'category=performance'
performance (2)
  paper.tex:5:1  info     unused-package  graphicx defines 72 names, none of which are used
    [unsafe fix] delete \usepackage{graphicx} if its side effects are not needed
  paper.tex:26:1  info     preamble-cost  5 packages requested directly, 263 files read
    fix: precompile the preamble into a format to skip this on every build

… 2 more lines omitted
```

`--timings` breaks the time down per phase and file, `--stats` sums up counts and wall time. Both measure the run itself and change between runs.
The kernel and the packages are interpreted once and cached. `satex cache build` prepares the packages in `cache_index` ahead of time.

<details>
<summary>The cached kernel with <code>cache</code></summary>

```text
$ satex cache
cache
  source /usr/local/texlive/2026/texmf-dist/tex/latex/base/latex.ltx
  holds 182295 definitions
  state cached
… 2 more lines omitted
```

</details>

## Queries

Obtain structured facts for further use. See the [queries wiki page](doc/wiki/queries.md) for more details.

The commands a document defines itself, without packages and without internal `@` names:

```text
$ satex query definitions -f samples/paper.tex --filter 'origin=document and not package~. and not name~@'
tag     name         parameters                       body                      file          
switch  \ifdraft                                                                paper.tex     @ paper.tex:3:7
switch  \drafttrue                                    \let \ifdraft \iftrue     paper.tex     @ paper.tex:3:7
switch  \draftfalse                                   \let \ifdraft \iffalse    paper.tex     @ paper.tex:3:7
macro   \hello                                        world                     preamble.tex  @ preamble.tex:1:1
macro   \highlight   #1                               \textbf {#1}              paper.tex     @ paper.tex:11:13
macro   \norm        #1                               \left \lVert #1\right \…  paper.tex     @ paper.tex:12:13
macro   \WeirdMore   super #1\;                        #1                       paper.tex     @ paper.tex:14:1
… 2 more lines omitted
```

<details>
<summary>List all queries with <code>--list</code></summary>

```text
$ satex query --list
name              
definitions       
expansions        
dependencies      
occurrences       
recursion         
calls             
dependency-graph  
trace             
files             
diagnostics       
distribution      
catcodes          
side-effects      
project           
plugins           
produces          
gaps              
pgfkeys           
options           
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
index   step       name            detail                                             file       
0       expand     \documentclass                                                     paper.tex  @ paper.tex:1:1
7734    open-file  \article        /usr/local/texlive/2026/texmf-dist/tex/latex/bas…  paper.tex  @ paper.tex:1:1
16002   expand     \value                                                             paper.tex  @ paper.tex:0:0
29937   expand     \par                                                               paper.tex  @ paper.tex:2:1
… 12 more lines omitted
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
… 1193 more lines omitted
```

</details>

Every command prints text by default. You can use `--format json`, `csv` or `markdown` whenever it prints rows. Some formats like `github`, `sarif` and `lsp` may be specific to commands like `lint`.

## Editor integration

`satex lsp` runs a Language Server: diagnostics, hover, definitions, references, completion, document symbols and quick fixes, all backed by the commands above. Stdio by default, `--port`/`--host` for TCP. See the [LSP wiki page](doc/wiki/lsp.md) for the protocol details and Neovim/VS Code setup.

## Configuration

SaTeX merges its built-in defaults, your global `~/.config/satex/satex.yaml`, every `satex.yaml` from the filesystem root down to the document, `--config FILE`, and `--set PATH=VALUE`, each overriding the one before. `satex --version` lists what was used. Plugins detect the installation, engine, output and build system (latexmk, make, l3build, …). TeX, e-TeX, pdfTeX and LaTeX2e are modelled in full. XeTeX, LuaTeX and ConTeXt are experimental. See the [configuration wiki page](doc/wiki/configuration.md).

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
- [ChkTeX](https://www.nongnu.org/chktex/) is a token-level linter for typographic and style mistakes (it does not expand macros).


[concrete semantics]: https://eagleoutice.github.io/satex-analyzer/SEMANTICS
[config]: https://eagleoutice.github.io/satex-analyzer/wiki/configuration