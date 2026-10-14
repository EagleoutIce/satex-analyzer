<!-- generated from doc/src/wiki/queries.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Queries

`satex query NAME -f FILE` runs a query and prints its records. `satex query --list` names them all. Every query takes `--filter EXPR` (e.g. `'tag=macro and arity>0'`) and `--format json`/`csv`. The examples analyze `samples/paper.tex`.

- [definitions](#definitions)
- [expansions](#expansions)
- [dependencies](#dependencies)
- [occurrences](#occurrences)
- [recursion](#recursion)
- [calls](#calls)
- [dependency-graph](#dependency-graph)
- [trace](#trace)
- [files](#files)
- [diagnostics](#diagnostics)
- [distribution](#distribution)
- [catcodes](#catcodes)
- [side-effects](#side-effects)
- [project](#project)
- [plugins](#plugins)
- [produces](#produces)
- [gaps](#gaps)
- [pgfkeys](#pgfkeys)
- [options](#options)
- [typeset](#typeset)

## definitions

Every name the document or its packages define. `origin` is `document` for files in the main file's directory tree, `library` otherwise. `package` names the package or class.

```text
$ satex query definitions -f samples/paper.tex --filter 'tag=macro and arity>0'
tag    name                              paramet…  body      file                  
macro  \__sys_tmp:w                      #1        \str_if…  expl3-code.tex        @ expl3-code.tex:1538:17
macro  \sys_gset_rand_seed:n             #1        \tex_se…  expl3-code.tex        @ expl3-code.tex:1530:34
macro  \reserved@c                       #1.sty\…  \def \r…  latex.ltx             @ latex.ltx:18742:20
macro  \reserved@a                       #10#2#{   \expand…  latex.ltx             @ latex.ltx:1277:3
macro  \reserved@a                       #1\@nil   \@for \…  latex.ltx             @ latex.ltx:18611:3
… 2473 more lines omitted
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

## dependencies

The files, packages and classes the document reads, and what loaded each. Lua modules loaded by `\directlua` chunks appear with kind `lua`. `texlive` names each file's TeX Live package (see [configuration](configuration.md#tex-live-packages-depp)).

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

## occurrences

Package and class identification banners, passed options, and other one-off events.

```text
$ satex query occurrences -f samples/paper.tex --filter 'kind=identification'
kind            key                   detail                   file                  
identification  article               2025/01/22 v1.4n Stand…  article.cls           @ article.cls:45:1
identification  size11.clo            2025/01/22 v1.4n Stand…  size11.clo            @ size11.clo:44:1
identification  amsmath               2025/07/09 v2.17z AMS …  amsmath.sty           @ amsmath.sty:29:1
identification  amstext               2024/11/17 v2.01 AMS t…  amstext.sty           @ amstext.sty:26:1
identification  amsgen                1999/11/30 v2.0 generi…  amsgen.sty            @ amsgen.sty:26:1
… 38 more lines omitted
```

## recursion

Macros that call themselves, directly or through another macro.

```text
$ satex query recursion -f samples/paper.tex
kind    name                          file            
direct  \KV@do                        keyval.sty      @ keyval.sty:31:6
direct  \do                           amsgen.sty      @ amsgen.sty:115:5
direct  \XC@if@                       xcolor.sty      @ xcolor.sty:128:8
direct  \XC@cl@@n                     xcolor.sty      @ xcolor.sty:359:3
direct  \@@mptopdf@@stripnewabove     supp-pdf.mkii   @ supp-pdf.mkii:127:5
… 2 more lines omitted
```

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

## dependency-graph

The dependency graph as records. `satex dependencies` renders it as DOT or JSON.

```text
$ satex query dependency-graph -f samples/paper.tex
tag                  name      file                            
macro-call           \show@r…  latex.ltx                       @ latex.ltx:731:7
macro-call           \fmtname  latex.ltx                       @ latex.ltx:731:26
macro-call           \space    latex.ltx                       @ latex.ltx:731:34
macro-call           \fmtver…  latex.ltx                       @ latex.ltx:731:42
use                  LaTeX2e…  latex.ltx                       @ latex.ltx:129:24
… 38849 more lines omitted
```

## trace

Every step: which macro expanded, which primitive ran, what changed. Use `satex trace`. `satex query trace` is empty unless `trace: true` is set.

```text
$ satex trace -f samples/paper.tex
index   step         name                        detail                     file           
678     expand       \documentclass                                         paper.tex      @ paper.tex:1:1
8412    open-file    \article                    /usr/local/texlive/YYYY/…  paper.tex      @ paper.tex:1:1
10261   expand       \par                                                   paper.tex      @ paper.tex:2:1
10268   expand       \newif                                                 paper.tex      @ paper.tex:3:1
10272   define       \ifdraft                                               paper.tex      @ paper.tex:3:7
… 208 more lines omitted
```

`--lines FROM:TO` records only those lines of the main file and what their calls do. `--steps FROM:TO` only those steps. Either end may be left out (`40:`, `:60`). Nothing outside the range is recorded. `--from [FILE:]LINE[:COL]` starts at a position (`FILE` is a trailing part of a path, for a subfile; `--to` and `--lines` take it too) and runs to the end. `--to [FILE:]LINE[:COL]` ends there. `--interactive` steps through the events one at a time, showing each macro's definition, arguments and inserted tokens: Enter or `e` steps into a call, `o` steps over it, `l` goes to the next line of the main file, `s N` to line N, `c` runs on, `q` quits. Only the document's own files are shown; `--include-internal` adds packages, classes and the kernel. `--format compact` prints at most 8 text lines instead: what each top-level call of the main file expands to, and each conditional's outcome. `--interactive` marks each run of hidden steps in gray, and with `--lines`, `--from` or `--to` ends with the text those lines typeset.

```text
$ satex trace -f samples/paper.tex --lines 10:11
index  step    name         file       
0      expand  \newcommand  paper.tex  @ paper.tex:11:1
105    define  \highlight   paper.tex  @ paper.tex:11:13
```

## files

Every file read while analyzing the document, in the order it was opened.

```text
$ satex query files -f samples/paper.tex
id   kind     file                                      
0    input    paper.tex                                 
1    input    latex.ltx                                 
2    input    texsys.cfg                                
3    input    expl3.ltx                                 
4    input    expl3-code.tex                            
… 257 more lines omitted
```

## diagnostics

Everything SaTeX noticed while interpreting the document: widened conditionals, extra `\else`, and the rest of the internal diagnostics distinct from the lint rules.

```text
$ satex query diagnostics -f samples/paper.tex
severity    code           count  message                               file            
assumption  clock-assumed  1      the document is assumed to be compi…  expl3-code.tex  @ expl3-code.tex:8169:38
```

## distribution

The detected TeX installation: provider, version, and the texmf trees searched.

```text
$ satex query distribution -f samples/paper.tex
kind        message                                                            root                                     
            TeX Live 2025, LaTeX2e 2025-11-01, 6 trees via kpsewhich, 52950 …                                           
texmf-tree                                                                     ~/texmf                   
texmf-tree                                                                     ~/.texliveYYYY/texmf-var  
texmf-tree                                                                     /usr/local/texlive/texmf-local           
texmf-tree                                                                     /usr/local/texlive/YYYY/texmf-config     
… 2 more lines omitted
```

## catcodes

Explicit category code assignments (`\catcode`) the document makes. `samples/paper.tex` does not change any, so this query returns nothing for it:

```text
$ satex query catcodes -f samples/paper.tex
```

## side-effects

Definitions and assignments a macro makes as a side effect of being called, rather than through `\def` or `\newcommand` directly at that point.

```text
$ satex query side-effects -f samples/paper.tex
tag                  name      detail    file                  
variable-definition  \c_sys_…  while e…  expl3-code.tex        @ expl3-code.tex:2099:42
variable-definition  \c_sys_…  while e…  expl3-code.tex        @ expl3-code.tex:6658:27
variable-definition  \c_sys_…  while e…  expl3-code.tex        @ expl3-code.tex:6658:27
variable-definition  \c_sys_…  while e…  expl3-code.tex        @ expl3-code.tex:6658:27
variable-definition  \c_sys_…  while e…  expl3-code.tex        @ expl3-code.tex:6658:27
… 1698 more lines omitted
```

## project

The document class, engine, and build configuration SaTeX detected or was told to assume.

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

## plugins

The provider, platform, engine, output target, and build system plugins chose for this run, and why.

```text
$ satex query plugins -f samples/paper.tex
kind              name                                                                               detail             
provider          texlive (TeX 3.141592653 (TeX Live 2025))                                          detected           
platform          linux                                                                              this machine       
engine            pdflatex (pdftex primitives)                                                       build configurat…  
kernel            latex2e                                                                            default            
output            pdf                                                                                build configurat…  
… 4 more lines omitted
```

## pgfkeys

The pgfkeys keys defined at the end of the run and what each does (`initial`, `code`, `code args`, `is family`, `is choice`, `is if`). `--prefix PATH` narrows to one path, e.g. `/tikz/pingu`.

```text
$ satex query pgfkeys -f samples/keys.tex
kind       key                                                  value          default  file      
is family  /demo                                                \edef \pgfke…           keys.tex  @ keys.tex:4:1
style      /demo/color                                          \pgfkeysalso…  red      keys.tex  @ keys.tex:4:1
is if      /demo/draft                                          \pgfkeys@han…  true     keys.tex  @ keys.tex:4:1
initial    /demo/family                                         /demo                   keys.tex  @ keys.tex:4:1
code args  /demo/label                                          \def \demola…           keys.tex  @ keys.tex:4:1
… 90 more lines omitted
```

## gaps

What the run could not interpret, grouped by cause. Widened recursion and undecided conditionals are not gaps: both stay sound. `satex --stats` and `gaps.log` use the same list.

```text
$ satex query gaps -f samples/mypackage.sty
```

## options

`--for NAME` lists the keys an environment's or command's optional argument accepts. SaTeX follows what the command's code calls until it reaches a key-setting call (keyval, xkeyval, pgfkeys, l3keys, or a package's own copy such as enumitem's) and lists the keys of the family it used. An optional argument that takes no keys (enumerate's label template) is reported as `positional`.

This needs call arguments recorded (`record_arguments`). `query options` and `--for` in a [request](#json-requests) turn that on.

`mybanner` in `samples/mypackage.sty` takes an optional argument but reads it directly (`[plain]`, no keys), so it comes back `positional`:

```text
$ satex query options --for mybanner -f samples/mypackage.sty
```

## typeset

The text satex computes the document typesets, one row per paragraph, then the named assumptions it rests on. `{a|b}` is a choice between the arms of an undecided conditional. `⟦x⟧` is text set in a box: it reaches the page only if the box is used. `⟨?⟩` is a character the run could not know. The [challenges](Challenges) compare it with what pdflatex sets.

```text
$ satex query typeset -f doc/src/wiki/challenges/04-typesetting.tex
3:25  paragraph  ⟦f⟧wide, {empty page|page started}
assumption  clock-assumed
```

## produces

Where a piece of text can come from, with or without expansion. `--text` is a literal, case-sensitive substring. `kind` says how: `source` (literal in a file. `tag` `comment` if commented out), `macro body` (at the definition), `value` (a register, counter or length. Dimensions in points), `occurrence` (a label, section title, …), `metadata` (`\title`, `\author`, `\date`), or `expansion` (a call the run expanded).

```text
$ satex query produces -f samples/paper.tex --text 1.5em
kind        tag          name           detail                              file           
source      class                       \vskip 1.5em%                       article.cls    @ article.cls:184:14
source      class                       \vskip 1.5em%                       article.cls    @ article.cls:243:12
source      class                       \vskip 1.5em}                       article.cls    @ article.cls:253:10
source      class                       \itemindent -1.5em%                 article.cls    @ article.cls:392:40
source      class                       \advance\leftmargin 1.5em}%         article.cls    @ article.cls:395:45
source      class                       {\list{}{\listparindent 1.5em%      article.cls    @ article.cls:399:40
source      class                       \setlength\@tempdima{1.5em}%        article.cls    @ article.cls:532:26
… 11 more lines omitted
```

Only the places that stand behind an expansion, for a text the document never writes out:

```text
$ satex query produces -f samples/paper.tex --text Introduction --filter 'kind!=source'
kind        tag      name                                               detail    file       
occurrence  section  Introduction                                       Introdu…  paper.tex  @ paper.tex:33:1
occurrence  write    \@writefiletocprotectcontentslinesectionprotectn…  \@write…  latex.ltx  @ latex.ltx:9552:34
occurrence  write    \newlabelsec:intro1thepageIntroductionsection.1    \newlab…  latex.ltx  @ latex.ltx:9552:34
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
  documentation  The LaTeX2e sources, [source2e](https://ctan.org/pkg/source2e)
  body           \not@math@alphabet \itshape \mathit \fontshape \itdefault \selectfont 
  reference      https://texdoc.org/serve/source2e/0
… 1 more line omitted
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

* `--at preamble` — the meaning right before `\begin{document}`.
* `--at document` — the meaning at the end of the run.
* `--at 302` — the meaning at line 302 of the main file.
* `--at somefile.tex:302` — the meaning at line 302 of `somefile.tex`, a file the run read.
* `--at after:enumitem` — the meaning right after the `enumitem` package finished contributing what it defines.

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
  documentation  The LaTeX2e sources, [source2e](https://ctan.org/pkg/source2e)
… 3 more lines omitted
```

# Slice

`satex slice NAME…` keeps what a name, environment or label depends on (`--forward`: what depends on it) and prints it as a compilable LaTeX document. `--list` or `--format json`/`csv`/`markdown` prints the records instead.

The criterion is a name, `--at [FILE:]LINE:COL[-LINE[:COL]]` (a range, to the end of the last line without a column), or `--where EXPR` over the `occurrences`/`definitions` records, e.g. `--where 'kind=begin-environment and key=figure'` for every figure.

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

```text
$ satex slice -f samples/paper.tex --where 'kind=begin-environment and key=figure'
\documentclass[11pt]{article}
\newif\ifdraft
\usepackage{amsmath}
\usepackage{graphicx}
\usepackage{hyperref}
\usepackage{mypackage}
… 16 more lines omitted
```

`--out DIR` writes the slice as a standalone project: the main file plus the local packages, classes, graphics and bibliographies it needs, paths preserved.

# Controls

`satex controls NAME…` shows what a switch, package option or conditional governs. With several names, the `for` column says which. Without a name it lists the switches and options in effect (`--all` includes the kernel's and packages'). That analyzes the document's own switches as undecided and is slower. The kernel's and the packages' switches keep their values.

```text
$ satex controls my@draft my@wide -f samples/mypackage.sty
```

# JSON requests

`satex query --request FILE` (`-` for stdin) runs several queries on one analysis and prints one JSON answer per query. The request is a JSON object:

* `file` — the document to analyze. Falls back to `-f` when omitted.
* `config` — optional overrides, the same dotted paths `--set` takes: `{"limits.memory": "1GiB", "load.packages": false}`.
* `queries` — an array of query objects, each with a `type`: any name from the query list above (`definitions`, `pgfkeys`, `gaps`, …, with `text`/`prefix`/`name`/`filter` as that query takes them), or `explain`, `controls`, `slice`, or `options`, with their own arguments:
  * `explain`: `names` (array), `all` (bool).
  * `controls`: `names` (array), `all` (bool).
  * `slice`: `names` (array), `at` (`"[FILE:]LINE:COL"`), `forward` (bool).
  * `options`: `name`.

Each answer is `{"type": …, "records": […]}`, or `{"type": …, "error": …}` for a query that named something wrong — one bad query in the batch does not stop the rest of it.

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
        "arity": 1,
        "body": "\\fbox {#1}",
        "by": "\\newcommand",
        "certain": true,
        "col": 13,
        "context": "outside environments",
        "effective": "\\mybox{1}",
        "file": "mypackage.sty",
        "line": 19,
        "name": "\\mybox",
        "package": null,
        "path": "samples/mypackage.sty",
        "signature": "m",
        "tag": "macro",
        "takes": {
… 23 more lines omitted
```
