<!-- generated from doc/src/wiki/queries.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Queries

`satex query NAME -f FILE` runs a query and prints its records. `satex query --list` names them all. Every query takes `--filter EXPR` (e.g. `'tag=macro and arity>0'`) and `--format json`/`csv`. The examples analyze `samples/paper.tex`.

- [calls](#calls): which control sequences each macro's body invokes
- [catcodes](#catcodes): explicit \catcode assignments the document makes
- [definitions](#definitions): every macro, environment and register the document or its packages define
- [dependencies](#dependencies): files, packages and classes read, and what loaded each
- [dependency-graph](#dependency-graph): the dependency graph between files and packages as records
- [diagnostics](#diagnostics): what satex noticed while interpreting, apart from lint rules
- [distribution](#distribution): the detected TeX installation, version and searched trees
- [expansions](#expansions): how often each macro was expanded during the run
- [files](#files): every file read, in the order it was opened
- [gaps](#gaps): constructs the run could not interpret, grouped by cause
- [occurrences](#occurrences): package banners, passed options and other one-off events
- [options](#options): the keys an environment or command's optional argument accepts
- [pgfkeys](#pgfkeys): pgfkeys defined at the end of the run and what each does
- [plugins](#plugins): what each plugin chose for this run, and why
- [previews](#previews): arrow tips and patterns the run declared, drawn by TeX
- [produces](#produces): where a given text can come from, expanded or not
- [project](#project): document class, engine and build settings detected or assumed
- [recursion](#recursion): macros that call themselves, directly or through others
- [side-effects](#side-effects): definitions and assignments a macro makes when called
- [trace](#trace): every step: macro expansions, primitives run, changes made
- [type-plugins](#type-plugins): the editor's type plugins: the types each triggers on and its features
- [typeset](#typeset): the text the document typesets, one row per paragraph

## calls

The call graph: which control sequences a macro's replacement text invokes.

```text
$ satex query calls -f samples/paper.tex --filter 'name=\maketitle'
name        callee                  
\maketitle  \@ifnextchar            
\maketitle  \let                    
\maketitle  \@footnotetext          
\maketitle  \@footnotemark          
\maketitle  \H@@footnotetext        
… 6 more lines omitted
```

## catcodes

Explicit category code assignments (`\catcode`) the document makes. `doc/src/wiki/queries/demo.tex` makes `!` active:

```text
$ satex query catcodes -f doc/src/wiki/queries/demo.tex
code  kind    key  detail     
13    Active  !    was Other  
```

## definitions

Every name the document or its packages define. `origin` is `document` for files in the main file's directory tree, `library` otherwise. `package` names the package or class.

```text
$ satex query definitions -f samples/paper.tex --filter 'tag=macro and arity>0'
tag    name      paramet…  comment   arguments    body      file                  
macro  \__sys_…  #1                               \str_if…  expl3-code.tex        @ expl3-code.tex:1539:17
macro  \sys_gs…  #1                               \tex_se…  expl3-code.tex        @ expl3-code.tex:1531:34
macro  \reserv…  #1.sty\…                         \def \r…  latex.ltx             @ latex.ltx:18742:20
macro  \reserv…  #10#2#{                          \expand…  latex.ltx             @ latex.ltx:1277:3
macro  \reserv…  #1\@nil                          \@for \…  latex.ltx             @ latex.ltx:18611:3
… 2591 more lines omitted
```

## dependencies

The files, packages and classes the document reads, and what loaded each. Lua modules loaded by `\directlua` chunks appear with kind `lua`. `texlive` names each file's TeX Live package (see [configuration](configuration.md#plugins)).

```text
$ satex query dependencies -f samples/paper.tex --filter 'kind=package'
kind     status          name            by              file                
package  read            amsmath                         paper.tex           @ paper.tex:4:1
package  read            amstext         amsmath         amsmath.sty         @ amsmath.sty:126:1
package  read            amsgen          amstext         amstext.sty         @ amstext.sty:27:1
package  read            amsbsy          amsmath         amsmath.sty         @ amsmath.sty:127:1
package  already-loaded  amsgen          amsbsy          amsbsy.sty          @ amsbsy.sty:27:1
… 49 more lines omitted
```

## dependency-graph

The dependency graph as records. `satex dependencies` renders it as DOT or JSON.

```text
$ satex query dependency-graph -f samples/paper.tex --filter 'file=paper.tex'
tag                  name                                                          file       
macro-call           \documentclass                                                paper.tex  @ paper.tex:1:1
variable-definition  \pkgcls@targetdate                                            paper.tex  @ paper.tex:1:1
use                  @                                                             paper.tex  @ paper.tex:1:1
use                  11pt                                                          paper.tex  @ paper.tex:1:1
macro-call           \value                                                        paper.tex  @ paper.tex:0:0
… 539 more lines omitted
```

## diagnostics

Everything SaTeX noticed while interpreting the document: widened conditionals, extra `\else`, and the rest of the internal diagnostics distinct from the lint rules.

```text
$ satex query diagnostics -f samples/paper.tex
severity     code                          count  message              file            
assumption   forced-page-breaks            1      the page may fill …  paper.tex       @ paper.tex:56:13
imprecision  argument-ends-in-unknown      1      a delimited argume…  latex.ltx       @ latex.ltx:20534:3
imprecision  unknown-name                  7      a name built from …  latex.ltx       @ latex.ltx:20322:31
imprecision  unknown-name                  7      a name built from …  latex.ltx       @ latex.ltx:20322:41
imprecision  mismatched-group              2      extra }, or forgot…  latex.ltx       @ latex.ltx:20534:3
… 16 more lines omitted
```

## distribution

The detected TeX installation: provider, version, and the texmf trees searched.

```text
$ satex query distribution -f samples/paper.tex
kind        name                                                 count  message   root                                  
                                                                        TeX Liv…                                        
texmf-tree                                                                        ~/texmf                   
texmf-tree                                                                        ~/.texliveYYYY/texmf-var  
texmf-tree                                                                        /usr/local/texlive/texmf-local        
texmf-tree                                                                        /usr/local/texlive/YYYY/texmf-config  
… 5384 more lines omitted
```

## expansions

Every macro call, with how many times it was expanded.

```text
$ satex query expansions -f samples/paper.tex --filter 'name=\newcommand'
name         count  package         file                
\newcommand  1      article         article.cls         @ article.cls:48:1
\newcommand  1      article         article.cls         @ article.cls:203:3
\newcommand  1      article         article.cls         @ article.cls:268:1
\newcommand  1      article         article.cls         @ article.cls:302:1
\newcommand  1      article         article.cls         @ article.cls:306:1
… 163 more lines omitted
```

## files

Every file read while analyzing the document, in the order it was opened.

```text
$ satex query files -f samples/paper.tex
id   kind     file                                      
0    input    paper.tex                                 
1    input    latex.ltx                                 
2    input    texsys.cfg                                
3    input    texsys.aux                                
4    input    expl3.ltx                                 
… 307 more lines omitted
```

## gaps

Lists the places the run could not interpret, grouped by cause. Widened recursion and undecided conditionals are not gaps: both stay sound. `satex --stats` and `gaps.log` use the same list.

```text
$ satex query gaps -f doc/src/wiki/challenges/06-unknown.tex
severity     code                    count  message                        file            
unsupported  unbalanced-conditional  1      end of 06-unknown.tex occurr…  06-unknown.tex  @ 06-unknown.tex:0:0
```

## occurrences

Package and class identification banners, passed options, and other one-off events.

```text
$ satex query occurrences -f samples/paper.tex --filter 'kind=identification'
kind            key                   detail                  file                  
identification  article               2025/01/22 v1.4n Stan…  article.cls           @ article.cls:45:1
identification  size11.clo            2025/01/22 v1.4n Stan…  size11.clo            @ size11.clo:44:1
identification  amsmath               2025/07/09 v2.17z AMS…  amsmath.sty           @ amsmath.sty:29:1
identification  amstext               2024/11/17 v2.01 AMS …  amstext.sty           @ amstext.sty:26:1
identification  amsgen                1999/11/30 v2.0 gener…  amsgen.sty            @ amsgen.sty:26:1
… 38 more lines omitted
```

## options

`--for NAME` lists the keys an environment's or command's optional argument accepts. SaTeX follows what the command's code calls until it reaches a key-setting call (keyval, xkeyval, pgfkeys, l3keys, or a package's own copy such as enumitem's) and lists the keys of the family it used. An optional argument that takes no keys (enumerate's label template) is reported as `positional`.

This needs call arguments recorded (`record_arguments`). `query options` and `--for` in a [request](#json-requests) turn that on.

`mybanner` in `samples/mypackage.sty` takes an optional argument but reads it directly (`[plain]`, no keys), so it comes back `positional`:

```text
$ satex query options --for mybanner -f samples/mypackage.sty
kind        name       detail                                                                                  
positional  \mybanner  takes an optional argument (\mybanner[plain]) not read through a recognized key family  
```

## pgfkeys

The pgfkeys keys defined at the end of the run and what each does (`initial`, `code`, `code args`, `is family`, `is choice`, `is if`). `--prefix PATH` narrows to one path, e.g. `/tikz/pingu`.

```text
$ satex query pgfkeys -f samples/keys.tex
kind       key                                                  value         default  file      
is family  /demo                                                \edef \pgfk…           keys.tex  @ keys.tex:4:1
style      /demo/color                                          \pgfkeysals…  red      keys.tex  @ keys.tex:4:1
is if      /demo/draft                                          \pgfkeys@ha…  true     keys.tex  @ keys.tex:4:1
initial    /demo/family                                         /demo                  keys.tex  @ keys.tex:4:1
code args  /demo/label                                          \def \demol…           keys.tex  @ keys.tex:4:1
… 90 more lines omitted
```

## plugins

The provider, platform, engine, output target, and build system plugins chose for this run, and why.

```text
$ satex query plugins -f samples/paper.tex
kind              name                                                                               detail             
provider          texlive (TeX 3.141592653 (TeX Live 2026))                                          detected           
platform          linux                                                                              this machine       
engine            pdflatex (pdftex primitives)                                                       build configurat…  
kernel            latex2e                                                                            default            
output            pdf                                                                                build configurat…  
… 4 more lines omitted
```

## previews

The arrow tips and fill patterns the run declared, each drawn by TeX (`latex` and `dvisvgm`) as SVG. The names are `arrow:SPEC` and `pattern:NAME`. The pictures are kept in `previews.json` in the cache directory, where completion reads them. `doc/src/wiki/queries/demo.tex` loads the `arrows.meta` and `patterns` libraries:

```text
$ satex query previews -f doc/src/wiki/queries/demo.tex
name                                      
arrow:-.                                  
arrow:-<                                  
arrow:->                                  
arrow:->|                                 
arrow:-Bar                                
… 100 more lines omitted
```

## produces

Where a piece of text can come from, with or without expansion. `--text` is a literal, case-sensitive substring. `kind` says how: `source` (literal in a file. `tag` `comment` if commented out), `macro body` (at the definition), `value` (a register, counter or length. Dimensions in points), `occurrence` (a label, section title, …), `metadata` (`\title`, `\author`, `\date`), or `expansion` (a call the run expanded).

```text
$ satex query produces -f samples/paper.tex --text 1.5em
kind        tag          name           detail                             file           
source      class                       \vskip 1.5em%                      article.cls    @ article.cls:184:14
source      class                       \vskip 1.5em%                      article.cls    @ article.cls:243:12
source      class                       \vskip 1.5em}                      article.cls    @ article.cls:253:10
source      class                       \itemindent -1.5em%                article.cls    @ article.cls:392:40
source      class                       \advance\leftmargin 1.5em}%        article.cls    @ article.cls:395:45
source      class                       {\list{}{\listparindent 1.5em%     article.cls    @ article.cls:399:40
source      class                       \setlength\@tempdima{1.5em}%       article.cls    @ article.cls:532:26
… 11 more lines omitted
```

Only the places that stand behind an expansion, for a text the document never writes out:

```text
$ satex query produces -f samples/paper.tex --text Introduction --filter 'kind!=source'
kind        tag      name                                              detail    file       
macro body  macro    \r@sec:intro                                      {1}{1}{…  paper.aux  @ paper.aux:6:11
occurrence  section  Introduction                                      Introdu…  paper.tex  @ paper.tex:33:1
occurrence  write    \@writefiletoccontentslinesectionnumberline1Int…  \@write…  latex.ltx  @ latex.ltx:9552:34
occurrence  write    \newlabelsec:intro11Introductionsection.1         \newlab…  latex.ltx  @ latex.ltx:9552:34
```

# Explain

`satex explain NAME…` shows what a control sequence means: where it is defined, what it takes, what it does. Without `-f` it explains against the smallest LaTeX document.

```text
$ satex explain '\itshape'
\itshape @ latex.ltx:12499:13
  tag            macro
  context        outside environments
  effective      \itshape
  takes          0
  uses           0 times, in this run
  by             \def
  does           Switches to italic.
  documentation  The LaTeX2e sources, [source2e](https://ctan.org/pkg/source2e)
  body           \not@math@alphabet \itshape \mathit \fontshape \itdefault \selectfont 
… 2 more lines omitted
```

## project

The document class, engine, and build configuration SaTeX detected or was told to assume.

The rows below list what the project walk found, without what `.gitignore` excludes.
`unreachable` lists the inputs, packages, classes and bibliographies no root reads.
Without `-f`, each root document is analyzed on its own.

```text
$ satex query project -f samples/paper.tex
kind       name                                                      count  detail    message                           
document   article [2025/01/22 v1.4n Standard LaTeX document class]         LaTeX2e…  engine pdflatex (build configur…  
latexmk    samples/latexmkrc                                                pdf_mod…                                    
documents                                                            3      samples…                                    
inputs                                                               1      samples…                                    
packages                                                             1      samples…                                    
… 1 more line omitted
```

## recursion

Macros that call themselves, directly or through another macro.

`doc/src/wiki/queries/demo.tex` defines `\countdown`, which calls itself:

```text
$ satex query recursion -f doc/src/wiki/queries/demo.tex --filter 'name=\countdown'
kind    name        file      
direct  \countdown  demo.tex  @ demo.tex:8:12
```

## side-effects

Definitions and assignments a macro makes as a side effect of being called, rather than through `\def` or `\newcommand` directly at that point.

```text
$ satex query side-effects -f samples/paper.tex --filter 'file=paper.tex'
tag                  name                       detail                       file       
macro-call           \documentclass             while expanding at 731:7     paper.tex  @ paper.tex:1:1
variable-definition  \ifdraft                   while expanding at 3:1       paper.tex  @ paper.tex:3:7
variable-definition  \drafttrue                 while expanding at 3:1       paper.tex  @ paper.tex:3:7
variable-definition  \draftfalse                while expanding at 3:1       paper.tex  @ paper.tex:3:7
macro-call           \usepackage                while expanding at 1:1       paper.tex  @ paper.tex:4:1
… 116 more lines omitted
```

## trace

Every step: which macro expanded, which primitive ran, what changed. Use `satex trace`. `satex query trace` is empty unless `trace: true` is set.

```text
$ satex trace -f samples/paper.tex
index   step         name                        detail                    file           
678     expand       \documentclass                                        paper.tex      @ paper.tex:1:1
8412    open-file    \article                    /usr/local/texlive/YYYY…  paper.tex      @ paper.tex:1:1
10261   expand       \par                                                  paper.tex      @ paper.tex:2:1
10268   expand       \newif                                                paper.tex      @ paper.tex:3:1
10272   define       \ifdraft                    iffalse                   paper.tex      @ paper.tex:3:7
… 209 more lines omitted
```

`--lines FROM:TO` records only those lines of the main file and what their calls do. `--steps FROM:TO` only those steps. Either end may be left out (`40:`, `:60`). Nothing outside the range is recorded. `--from [FILE:]LINE[:COL]` starts at a position (`FILE` is a trailing part of a path, for a subfile, and `--to` and `--lines` take it too) and runs to the end. `--to [FILE:]LINE[:COL]` ends there. `--interactive` steps through the events one at a time, showing each macro's definition, arguments and inserted tokens: Enter or `e` steps into a call, `o` steps over it, `l` goes to the next line of the main file, `s N` to line N, `c` runs on, `q` quits. Only the document's own files are shown. `--include-internal` adds packages, classes and the kernel. `--format compact` prints at most 8 text lines instead: what each top-level call of the main file expands to, and each conditional's outcome. `--interactive` marks each run of hidden steps in gray, and with `--lines`, `--from` or `--to` ends with the text those lines typeset.

```text
$ satex trace -f samples/paper.tex --lines 10:11
index  step    name         detail                        file       
0      expand  \newcommand                                paper.tex  @ paper.tex:11:1
105    define  \highlight   \long macro:#1->\textbf {#1}  paper.tex  @ paper.tex:11:13
```

### Dev mode: the state at each step

`--state` (or `trace_state: true`) gives each step the abstract state it ran in, to track down where a result lost precision. It costs nothing while off. With `--format json` each record has a `state` object. With `--format csv` it is one JSON cell in the `state` column. A text table leaves it out.

| Key | What it holds |
| --- | --- |
| `paths` | `live` paths, `forks` made so far, the `peak`, and `apart`: why the waiting paths do not join (progress, stacks, tags), as `SATEX_WHY_APART` prints it |
| `choices` | the undecided choices this path took: the command, its place, `arm` of `arms`, and `joined` when the choice splits a joined meaning set |
| `mode` | the modes the path may be in |
| `conditionals`, `groups` | the conditional and group stacks, innermost first. `many` marks a widened entry |
| `input` | the input stack, top first: a file and position, a token list and the macro it expands, a `\scantokens` text |
| `bindings` | the step's own name and the names ahead, then the paired ones: `meaning`, the meaning set `may` a join left (`maybe_undefined` when one member is undefined), `certain`, `global`, and a register's `value` |
| `paired` | the names a join kept paired across its arms, and how many arms |
| `catcodes` | the catcode of each non-alphanumeric character in the file text ahead |

A `value` names its type in the [`Ty` lattice](Domains#argument-types-the-ty-lattice) (`number`, `dimen`, `glue`, `muglue`, `quantity`, `text`, `any`) and what the domain knows: `exact`, an `interval` `[lo, hi]`, a `set`, or `unknown`. Typed unknown text appears as its own names (`unknown digits:0..1439`). `limits.trace_state` caps each list. In the playground, *dev mode* in the settings turns this on: the trace shows the domains of the selected step, the editor menu offers *Inspect state here*, and the trace exports as JSON or CSV.

## typeset

The text satex computes the document typesets, one row per paragraph, then the named assumptions it rests on. `{a|b}` is a choice between the arms of an undecided conditional. `⟦x⟧` is text set in a box: it reaches the page only if the box is used. `⟨?⟩` is a character the run could not know. `│` is a table's `|`, `―` an `\hline` or other `\hrule`, `▬` a `\rule` bar. Rules of no width (struts) are left out. The [challenges](Challenges) compare it with what pdflatex sets.

```text
$ satex query typeset -f doc/src/wiki/challenges/04-typesetting.tex
3:25  paragraph  ⟦f⟧wide, {empty page|page started}
6:15  page  page 1
assumption  clock-assumed
```

## Several definitions

If the preamble meaning differs from the one at the end of the run, both are shown. `--all` lists every definition with its place.

## Contexts

A name can mean different things in different places: `\item` outside a list is the kernel's error, inside `enumerate` it starts an item. Each definition records the environments open where it was made, and `explain` shows one record per distinct meaning. `context` says where it holds (`"outside environments"`, `"in enumerate, itemize"`, …).

A name without its own redefinitions can still behave differently by context. `error` is the error a real use raised there, or, without a real use, what running the command in that context raises.

## Signatures

`signature` spells what a command reads, one letter per argument, in the notation of LaTeX's `\NewDocumentCommand` ([usrguide](https://ctan.org/pkg/usrguide)):

| key | reads |
|---|---|
| `m` | a mandatory argument: a braced group or one token |
| `o` | an optional `[…]` argument, absent when no `[` follows |
| `O{default}` | an optional `[…]` argument with the default it takes when left out |
| `s` | an optional star `*` |
| `t⟨c⟩` | an optional token `⟨c⟩` |
| `r⟨a⟩⟨b⟩`, `R⟨a⟩⟨b⟩{default}` | a mandatory argument delimited by `⟨a⟩…⟨b⟩` |
| `d⟨a⟩⟨b⟩`, `D⟨a⟩⟨b⟩{default}` | an optional argument delimited by `⟨a⟩…⟨b⟩` |
| `u⟨tokens⟩` | everything up to `⟨tokens⟩` |
| `v` | verbatim text |
| `e{…}`, `E{…}{…}` | embellishments such as `^` and `_` |
| `+`, `!`, `>{…}` | before a letter: may contain `\par`, no spaces before it, a processor applied to it |
| `…` | the probe could not decide the rest |

`effective` writes a call: `\section*?[1]{2}` is `som` (optional star, optional argument, mandatory argument). `[default]` shows a default. `takes` is the least and most arguments a call reads.

SaTeX finds a signature by running the command on probe input and watching what it reads (SEMANTICS [§ 13](https://ctan.org/pkg/usrguide), "What a command takes").

## Locations

`--at WHERE` asks for the meaning at one place instead of the default preamble/document view (a suffix on the name would be ambiguous, since `@` is a letter in LaTeX's internal names):

* `--at preamble`: the meaning right before `\begin{document}`.
* `--at document`: the meaning at the end of the run.
* `--at 302`: the meaning at line 302 of the main file.
* `--at somefile.tex:302`: the meaning at line 302 of `somefile.tex`, a file the run read.
* `--at after:enumitem`: the meaning right after the `enumitem` package finished contributing what it defines.

In a [request](#json-requests), an `explain` entry takes the same place as `"at"`.

```text
$ satex explain '\itshape' --at preamble
\itshape @ latex.ltx:1416:0
  tag            macro
  when           preamble
  context        outside environments
  effective      \itshape
  takes          0
  package        kernel
  does           Switches to italic.
… 4 more lines omitted
```

# Slice

`satex slice NAME…` keeps what a name, environment or label depends on (`--forward`: what depends on it) and prints it as a compilable LaTeX document. `--list` or `--format json`/`csv`/`markdown` prints the records instead.

The criterion is a name, `--at [FILE:]LINE:COL[-LINE[:COL]]` (a range, to the end of the last line without a column), or `--where EXPR` over the `occurrences`/`definitions` records, e.g. `--where 'kind=begin-environment and key=figure'` for every figure.

```text
$ satex slice -f samples/paper.tex '\ifdraft'
\begin{filecontents*}[overwrite]{mypackage.sty}
\NeedsTeXFormat{LaTeX2e}
\ProvidesPackage{mypackage}[2024/01/01 v1.0 Sample package for satex]

\RequirePackage{xcolor}

… 58 more lines omitted
```

```text
$ satex slice -f samples/paper.tex --where 'kind=begin-environment and key=figure'
\begin{filecontents*}[overwrite]{mypackage.sty}
\NeedsTeXFormat{LaTeX2e}
\ProvidesPackage{mypackage}[2024/01/01 v1.0 Sample package for satex]

\RequirePackage{xcolor}

… 89 more lines omitted
```

`--out DIR` writes the slice as a standalone project: the main file plus the local packages, classes, graphics and bibliographies it needs, paths preserved.

# Controls

`satex controls NAME…` shows what a switch, package option or conditional governs. With several names, the `for` column says which. Without a name it lists the switches and options in effect (`--all` includes the kernel's and packages'). That analyzes the document's own switches as undecided and is slower. The kernel's and the packages' switches keep their values.

```text
$ satex controls my@draft my@wide -f samples/mypackage.sty
tag    name         for       means                              detail    file           
use    \newcommand  my@draft  \ifmy@draft, the switch `\newif`…  \ifmy@d…  mypackage.sty  @ mypackage.sty:29:3
macro  \mymode      my@draft  \ifmy@draft, the switch `\newif`…  \ifmy@d…  mypackage.sty  @ mypackage.sty:29:15
use    \newcommand  my@draft  \ifmy@draft, the switch `\newif`…  \ifmy@d…  mypackage.sty  @ mypackage.sty:31:3
macro  \mymode      my@draft  \ifmy@draft, the switch `\newif`…  \ifmy@d…  mypackage.sty  @ mypackage.sty:31:15
```

# JSON requests

`satex query --request FILE` (`-` for stdin) runs several queries on one analysis and prints one JSON answer per query. The request is a JSON object:

* `file`: the document to analyze. Falls back to `-f` when omitted.
* `config`: optional overrides, the same dotted paths `--set` takes: `{"limits.memory": "1GiB", "load.packages": false}`.
* `queries`: an array of query objects, each with a `type`: any name from the query list above (`definitions`, `pgfkeys`, `gaps`, …, with `text`/`prefix`/`name`/`filter` as that query takes them), or `explain`, `controls`, `slice`, `options`, `providers`, or `docs`, with their own arguments:
  * `explain`: `names` (array), `all` (bool).
  * `controls`: `names` (array), `all` (bool).
  * `slice`: `names` (array), `at` (`"[FILE:]LINE:COL"`), `forward` (bool).
  * `options`: `name`.
  * `providers`: `names` (array). For each name, the installed package or class that defines it while loading the fewest files.
  * `docs`: no arguments. What the manuals of the packages and classes the run read say about their commands, one record per command with the directory it was mined from.

Each answer is `{"type": …, "records": […]}`, or `{"type": …, "error": …}` for a query that named something wrong. One bad query in the batch does not stop the rest of it.

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

```text
$ satex query --request doc/src/wiki/request-example.json
[
  {
    "hints": [],
    "records": [
      {
        "arguments": [
          "⟨arg1⟩ the text to box"
        ],
        "arity": 1,
        "body": "\\fbox {#1}",
        "by": "\\newcommand",
        "certain": true,
        "col": 13,
        "comment": "Draws a frame around its argument.",
        "context": "outside environments",
        "effective": "\\mybox{1}",
        "file": "mypackage.sty",
        "line": 21,
        "name": "\\mybox",
        "package": null,
… 28 more lines omitted
```
