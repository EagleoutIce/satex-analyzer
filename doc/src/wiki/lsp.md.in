<!-- generated from doc/src/wiki/lsp.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Language Server

This page is for editor authors and users of `satex lsp`: what it serves, how it decides what to offer and how to start it. For ready-made clients see [VS Code](vscode.md) and [IntelliJ](intellij.md). It serves the Language Server Protocol on top of the existing lints and queries: diagnostics from [lint](lints.md), hover and definitions from [`explain`](queries.md#explain), completion from [`scope`](../../README.md#comprehension), and any [query request](queries.md#json-requests) through `workspace/executeCommand`.

```sh
satex lsp                    # stdio, the default every editor client speaks
satex lsp --port 9257        # TCP instead, bound to 127.0.0.1
satex lsp --port 9257 --host 0.0.0.0
```

**How it runs.** Edits arrive as changed ranges (incremental sync). A document is re-analyzed `lsp.debounce_ms` after its last edit. The run resumes from the last checkpoint whose input is unchanged, so an edit late in the document reruns only what follows it, with the same result as a full run. Each analysis is the whole build (latexmk-style), and its `.aux`/`.bbl` stay for the next one. Queries and the playground read that one analysis. While a run or its lint rules go on, hover, completion, signature help, widgets, colors, lenses and inlay hints answer from the last one, at most `lsp.pause_ms` later. Each request is answered on its own: one that waits for the run (a code action, a rename) holds no other. A hover over a finding adds its rule's card: number, severity, category, summary, explanation and sources. `\input` sees the unsaved text of other open documents. A file without `\documentclass` is analyzed as part of the document that reads it.

**Style prompt.** When a project's own files break one style rule at least `lints.style-prompt.min_findings` times, the server asks once per session through `window/showMessageRequest`, so any client can answer. *Yes* adds the rule to `lint_off+` in the project's `satex.yaml` (made if missing) and takes its findings off at once. *No* asks again in the next session. *Never ask* adds it to `lints.style-prompt.keep`. `lints.style-prompt.enabled: false` turns the prompt off, and `lints.style-prompt.categories` names the categories a project may call its style. `satex lint --style` lists the same rules (see [lints](lints.md#suppressing-findings)). The playground asks the same in a banner, through the command `satex/style` with `{code, answer}`, `answer` being `off` or `keep`.

## Capabilities

| Feature | Request | Backed by |
| --- | --- | --- |
| Diagnostics | `textDocument/publishDiagnostics` | `satex lint --format lsp` |
| Quick fixes | `textDocument/codeAction` | the [fix edits](lints.md#quick-fixes) of a finding |
| Hover | `textDocument/hover` | `satex explain`, on a citation key its `.bib` entry |
| Definition | `textDocument/definition` | `satex explain`, else a source scan |
| References | `textDocument/references` | `satex query expansions`/`occurrences` |
| Completion | `textDocument/completion` | `satex scope`, labels, citation keys, environments, files, stored pgfkeys/l3keys |
| Document symbols | `textDocument/documentSymbol` | sections, labels, document-defined names |
| Rename | `textDocument/prepareRename`, `rename` | `satex query definitions`/`expansions`/`occurrences` |
| Linked editing | `textDocument/linkedEditingRange` | a `\begin` name and its `\end`, paired by nesting in the open text |
| Any query | `workspace/executeCommand` (`satex/query`) | a [query request](queries.md#json-requests). Without `file` it reads the open document |
| New document | `satex/templates`, then `satex/newDocument` (a request, or `workspace/executeCommand`) | the templates of `src/data/templates.yaml`. `{template, options}`, `{from}` (a GitHub URL, a zip URL, a local zip or folder) or `{files}`: the project's files, its main file, its header line and where its files came from. With `dir` it writes them there |
| Colors | `textDocument/documentColor`, `colorPresentation` | the [color plugin](type-plugins.md#color) |
| Dialogs | `satex/view` (`{textDocument, position}`) | the [type plugins](type-plugins.md)' views: a color picker, a length converter |
| Hyphenation | `satex/hyphenate` (`{textDocument, words}`) | per word the characters a hyphen may precede, from the last run's tables ([typesetting plugins](configuration.md#plugins)) |
| Math symbols | `satex/mathwidths` (`{textDocument, symbols, size}`) | per symbol `[width, height, depth, italic]` in em of its font, from the run's math codes and family fonts (`size` 0 text, 1 script, 2 scriptscript), `null` where not known |
| Font at a place | `satex/fontAt` (`{textDocument, path?, position, char?}`) | the font the run set the character before `position` in (`char` when given): `{style, family, series, shape}`, `style` its family, series and shape among the defaults (`bf it`), `null` where the run set no such character |
| Line breaking | `satex/break` (`{textDocument, path, line, words, indent}`) | the lines the words break into: per line the word and character it ends before, whether at a hyphen, its glue ratio and order. `indent` is a box in sp, or `true` for `\parindent` |

**Roots.** A file is analyzed through the root document that reads it. A file no root reads gets an information diagnostic `unreachable-file`. It is then read after a root's preamble, apart from that root's results.

**Definition fallback.** When the run stopped before a later file defines a name, the server scans the sources: breadth-first through `\input`, `\include`, `\usepackage`, … (at most 200 files), looking for `\def`, `\newcommand`, `\NewDocumentCommand`, `\cs_new:Npn` and relatives. Hover marks such a site as not reached.

**Rename** moves every name one declaration made: `\newcounter{step}` renames `\c@step`, `\thestep`, `\p@step`, `\cl@step`, `\theHstep` and every `{step}` use. `\newenvironment{note}` renames `\begin{note}`/`\end{note}` too. It is refused, with the site named, when a new name is taken, a file may not be edited, or a built name has no spelled site.

### Completion

| Context | Offers |
| --- | --- |
| after `\` | commands in scope: the signature beside the name (`[⟨options⟩]{⟨label⟩}`), a tab stop per argument named by its type (an optional one with its brackets), the signature help once accepted, the command's card as documentation. `data` carries `args` (`m`, `som`), `math` and a one-line `doc` |
| after `\` or `\begin{` with 3 letters | below the names in scope, those of packages not loaded yet (the installation's index), each with the `\usepackage` it adds as `additionalTextEdits` (`completion.unloaded`) |
| after `\` in math | math commands first (`$…$`, `\(…\)`, `\[…\]`, `completion.math_environments`): a command is math when its body reaches material only math mode takes within `completion.math_depth` expansions |
| after `\` in text | a math command's edit sets it in math, the caret inside: `\(\sum\)`, or `$\sum$` without a LaTeX kernel or in a file that uses `$` (`conventions.math.inline`). Text is where the run set the characters around the cursor in text. `data.wrap` holds the delimiters. Off with `completion.wrap_math: false` |
| `\begin{` | environment names |
| a reference or citation argument | the document's labels or keys, by the argument's key kind (`\ref{` never offers citations) |
| a file argument | matching files of its kind, the project's and the installation's |
| a quantity argument (`\setlength{\`) | names holding a fitting quantity first |
| a key list (`\pgfkeys{`, `\tikzset{`, `\keys_set:nn`) | stored keys of the path or family, `key=` then its choices or `true`/`false` |
| package options | what the package's `\DeclareOption*` hands on, packages needing another engine come last |

Completion and hover ask one query, the **type at the cursor** (`src/lsp/type_at.rs`). It uses what the run found out: how TeX read the text, and what kind of name a `\csname` built from it (`r@` a label, `pgf@sh@ns@` a node, `\color@` a color). For text the run has not read yet, it takes the role of the argument the cursor is in. The names offered are those of the most precise type found: labels, citation keys, colors, nodes and anchors, files and libraries, keys, units after a number. Nothing is offered in a comment, in verbatim text, or where the text defines a name (`\label{`). A command the run never called is probed once with a marker in each argument, so `\usetikzlibrary{` completes libraries before any library is loaded.

An argument's purpose, its **role**, decides the offer. Roles come from, in order: what the run saw calls do, the command's own body (through any number of wrappers, with literal prefixes), and a seed table for kernel and common package commands the project does not redefine. So `\newcommand\secref[1]{\ref{sec:#1}}` completes `intro` for the label `sec:intro` before it is ever used.

Each argument also has a **type** ([`Ty`](domains.md#argument-types-the-ty-lattice)), shown in hover, signature help and the completion's usage line: `\setlength{⟨skip register⟩}{⟨glue⟩}`. A hover adds a short explanation from `src/data/knowledge.md`: of the command itself (`\\` as a line break, `\hfill`, `\vspace`, `\rule`, …) and of the quantities it reads (glue, dimen, number, box).

### Where a card's text comes from

A card shows only what the documentation says about the name. One list of sources in `src/queries/said.rs` serves the editor's hover and the site's cards. The first source that has an answer wins:

1. The description table (`src/data/describe.tsv`) for primitives and the kernel's user commands.
2. The manual of each package that defines the name, the first definer first: a `\DescribeMacro` paragraph, a `\begin{macro}` section, an l3doc `function` block, a pgf `command` block. Only prose counts: a `\changes` note, a usage line or a fragment never stands in for a description.
3. For an alias (`\let\a\b`), the documentation of the name it copies. The card says `as \b`.
4. The manual of any loaded package that documents the name, unless the format defines it.
5. The kernel's manual for a name the format defines.
6. The comment block right above the definition, or the comment that ends its line. `% #1: the text to box` names the argument.

When none of them has an answer, the card says `No documentation found`, the sources it tried and `Defined at file:line`. The usage line names each argument by the documentation's syntax line, else by the type the run gives it, else by its place (`⟨arg2⟩`). A default is a hint (`[⟨number⟩ = 0]`), never the label. The `from` chip names the package that owns the name: the format, else the first package that defines it. A package that redefines it shows as `redefined by`. A name another package only `\let`s (tikz-timing's `\draw`) belongs to the loaded package whose manual documents it and whose source defines it (TikZ).

```tex
% Draws a frame around its argument.
% #1: the text to box
\newcommand{\mybox}[1]{\fbox{#1}}
```

Hover over `\mybox` shows `\mybox{⟨the text to box⟩}`, from `mypackage`.

### Hover extras

- A command with several definitions shows the one in force at the cursor, the rest under `More definitions`.
- `\bibliographystyle` and biblatex's `style=` show the style file and what it does.
- The typeset pass sets the bibliography from a `.bbl` satex writes itself from the cited entries, never LaTeX's.

## Configuration

```yaml
lsp:
  # port: 2087       # unset: stdio. A number switches to TCP.
  host: 127.0.0.1    # interface `port` binds to
  debounce_ms: 100   # quiet time after an edit before re-analyzing
  pause_ms: 10       # how long a run goes on before requests answer from the last one
```

`--port` and `--host` override these for one run. What completion offers is set under `completion.*`, the rest of the file is described on the [configuration page](configuration.md).

## Editor setup

### VS Code (generic client)

A minimal client with `vscode-languageclient`:

```js
const { LanguageClient } = require("vscode-languageclient/node");

const client = new LanguageClient(
  "satex",
  { command: "satex", args: ["lsp"] },
  { documentSelector: [{ scheme: "file", language: "latex" }] },
);
client.start();
```

### Neovim (`lspconfig`)

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")

if not configs.satex then
  configs.satex = {
    default_config = {
      cmd = { "satex", "lsp" },
      filetypes = { "tex", "plaintex" },
      root_dir = lspconfig.util.root_pattern("satex.yaml", ".git"),
    },
  }
end
lspconfig.satex.setup({})
```

With Neovim's built-in client (`vim.lsp.config`, 0.11+) instead of `lspconfig`:

```lua
vim.lsp.config.satex = {
  cmd = { "satex", "lsp" },
  filetypes = { "tex", "plaintex" },
  root_markers = { "satex.yaml", ".git" },
}
vim.lsp.enable("satex")
```

### TeXstudio

TeXstudio has no LSP client. It reads the LaTeX log, so `satex lint --format texlog` writes findings in that form: errors as `! Package satex Error: …` with `l.⟨line⟩`, the rest as `Package satex Warning: … on input line ⟨line⟩.` Append them to the log after compiling, then let TeXstudio show the log. Under *Options → Configure TeXstudio → Build*, set *Build & View* to

```
txs:///compile | satex lint -f %.tex --format texlog >> %.log | txs:///view-log | txs:///view
```

The findings then appear in the log panel and as error and warning marks in the editor.

