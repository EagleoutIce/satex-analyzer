<!-- generated from doc/src/wiki/lsp.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# Language Server

`satex lsp` serves the Language Server Protocol on top of the existing lints and queries: diagnostics from [lint](lints.md), hover and definitions from [`explain`](queries.md#explain), completion from [`scope`](../../README.md#comprehension), and any [query request](queries.md#json-requests) through `workspace/executeCommand`.

```sh
satex lsp                    # stdio, the default every editor client speaks
satex lsp --port 9257        # TCP instead, bound to 127.0.0.1
satex lsp --port 9257 --host 0.0.0.0
```

**How it runs.** Edits arrive as changed ranges (incremental sync). A document is re-analyzed `lsp.debounce_ms` after its last edit. The run resumes from the last checkpoint whose input is unchanged, so an edit late in the document reruns only what follows it, with the same result as a full run. Each analysis is the whole build (latexmk-style), and its `.aux`/`.bbl` stay for the next one. Queries and the playground read that one analysis. While a run or its lint rules go on, hover, completion and signature help answer from the last one. A hover over a finding adds its rule's card: number, severity, category, summary, explanation and sources. `\input` sees the unsaved text of other open documents. A file without `\documentclass` is analyzed as part of the document that reads it.

**Style prompt.** When a project's own files break one style rule at least `style_prompt.min_findings` times, the server asks once per session through `window/showMessageRequest`, so any client can answer. *Yes* adds the rule to `lint_off+` in the project's `satex.yaml` (made if missing) and takes its findings off at once. *No* asks again in the next session. *Never ask* adds it to `style_prompt.keep`. `style_prompt.enabled: false` turns the prompt off, and `style_prompt.categories` names the categories a project may call its style. The playground asks the same in a banner, through the command `satex/style` with `{code, answer}`, `answer` being `off` or `keep`.

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

**Roots.** A file is analyzed through the root document that reads it. A file no root reads gets an information diagnostic `unreachable-file`. It is then read after a root's preamble, apart from that root's results.

**Definition fallback.** When the run stopped before a later file defines a name, the server scans the sources: breadth-first through `\input`, `\include`, `\usepackage`, … (at most 200 files), looking for `\def`, `\newcommand`, `\NewDocumentCommand`, `\cs_new:Npn` and relatives. Hover marks such a site as not reached.

**Rename** moves every name one declaration made: `\newcounter{step}` renames `\c@step`, `\thestep`, `\p@step`, `\cl@step`, `\theHstep` and every `{step}` use. `\newenvironment{note}` renames `\begin{note}`/`\end{note}` too. It is refused, with the site named, when a new name is taken, a file may not be edited, or a built name has no spelled site.

### Completion

| Context | Offers |
| --- | --- |
| after `\` | commands in scope, with argument snippets |
| `\begin{` | environment names |
| a reference or citation argument | the document's labels or keys, by the argument's key kind (`\ref{` never offers citations) |
| a file argument | matching files of its kind, the project's and the installation's |
| a quantity argument (`\setlength{\`) | names holding a fitting quantity first |
| a key list (`\pgfkeys{`, `\tikzset{`, `\keys_set:nn`) | stored keys of the path or family, `key=` then its choices or `true`/`false` |
| package options | what the package's `\DeclareOption*` hands on, packages needing another engine come last |

Completion and hover ask one query, the **type at the cursor** (`src/lsp/type_at.rs`). It reads the run's facts: what TeX read the text as, the kind of name a `\csname` formed from it (`r@` a label, `pgf@sh@ns@` a node, `\color@` a color). For text the run has not read yet, it takes the role of the argument the cursor is in. The names offered are those of the most precise type found: labels, citation keys, colors, nodes and anchors, files and libraries, keys, units after a number. Nothing is offered in a comment, in verbatim text, or where the text defines a name (`\label{`). A command the run never called is probed once with a marker in each argument, so `\usetikzlibrary{` completes libraries before any library is loaded.

An argument's purpose, its **role**, decides the offer. Roles come from, in order: what the run saw calls do, the command's own body (through any number of wrappers, with literal prefixes), and a seed table for kernel and common package commands the project does not redefine. So `\newcommand\secref[1]{\ref{sec:#1}}` completes `intro` for the label `sec:intro` before it is ever used.

Each argument also has a **type** ([`Ty`](domains.md#argument-types-the-ty-lattice)), shown in hover, signature help and the completion's usage line: `\setlength{1: skip register}{2: glue}`. A hover adds a short explanation from `src/data/knowledge.md`: of the command itself (`\\` as a line break, `\hfill`, `\vspace`, `\rule`, …) and of the quantities it reads (glue, dimen, number, box).

### Hover extras

- A command with several definitions shows the one in force at the cursor, the rest under `More definitions`.
- `\bibliographystyle` and biblatex's `style=` show the style file and what it does.
- The typeset pass sets the bibliography from a `.bbl` satex writes itself from the cited entries, never LaTeX's.

## Configuration

```yaml
lsp:
  port: ~            # unset: stdio. A number switches to TCP.
  host: 127.0.0.1    # interface `port` binds to
  debounce_ms: 300    # quiet time after an edit before re-analyzing
```

`--port` and `--host` override these for one run.

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

