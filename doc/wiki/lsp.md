<!-- generated from doc/src/wiki/lsp.md.in by satex-doc; do not edit -->

# Language Server

`satex lsp` serves the Language Server Protocol on top of the existing lints and queries: diagnostics from [lint](lints.md), hover and definitions from [`explain`](queries.md#explain), completion from [`scope`](../../README.md#comprehension), and any [query request](queries.md#json-requests) through `workspace/executeCommand`.

```sh
satex lsp                    # stdio, the default every editor client speaks
satex lsp --port 9257        # TCP instead, bound to 127.0.0.1
satex lsp --port 9257 --host 0.0.0.0
```

A document is re-analyzed `lsp.debounce_ms` after its last edit. Requests wait for the first analysis. `\input` and `\include` see the unsaved text of other open documents.

## Capabilities

| Feature | Request | Backed by |
| --- | --- | --- |
| Diagnostics | `textDocument/publishDiagnostics`, on open/change/save | `satex lint --format lsp` |
| Quick fixes | `textDocument/codeAction` | the `fix`/`unsafe fix` edits [lint](lints.md#quick-fixes) attaches to a finding |
| Hover | `textDocument/hover` | `satex explain` |
| Definition | `textDocument/definition` | `satex explain` |
| References | `textDocument/references` | `satex query expansions`/`occurrences` |
| Completion | `textDocument/completion` | `satex scope`, plus label/citation/environment keys from `occurrences` |
| Document symbols | `textDocument/documentSymbol` | sections, labels and document-defined names |
| Run any query | `workspace/executeCommand` (`satex/query`) | `satex query --request` |

Completion looks at the text before the cursor: after `\` it offers the commands in scope there, with a snippet for their arguments. Inside `\begin{` environment names. Inside a reference or citation argument the document's labels or keys.

`workspace/executeCommand` runs `satex/query` with one argument, a [query request](queries.md#json-requests). Without `file` it applies to the open document.

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
