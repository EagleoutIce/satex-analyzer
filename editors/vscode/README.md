# satex for VS Code

Static analysis for LaTeX and TeX documents, powered by [satex](https://github.com/EagleoutIce/satex-analyzer) and its built-in language server (`satex lsp`): diagnostics, lints and navigation for `.tex`, `.sty`, `.cls` and `.bib` files.

## Features

- Starts `satex lsp` over stdio for LaTeX, TeX and BibTeX files.
- Builds satex's LaTeX kernel caches in the background on first use, with a cancellable progress notification. The language server is never blocked by it.
- `satex: Trace Expansion Here` (`satex.traceAtCursor`): traces what the selection, or the control sequence at the cursor, expands to at that position and shows the steps beside the editor.
- `satex: Compile` (`satex.compile`): builds the root document, streams the build output to the "satex build" channel and shows the findings as diagnostics.

## Requirements

satex uses **your installed TeX distribution** (`kpsewhich`, `pdflatex`, ...). TeX Live is **not** bundled, as it is far too large; install TeX Live, MiKTeX or MacTeX yourself. The extension warns if `kpsewhich` or `pdflatex` is not on the `PATH`.

The platform-specific packages bundle the matching `satex` binary. The universal package does not; it uses `satex` from the `PATH` (`cargo install satex`).

## Settings

| Setting | Description |
| --- | --- |
| `satex.path` | Path to the `satex` binary. Empty: bundled binary, then `satex` on the `PATH`. |
| `satex.rootFile` | Root document, relative to the workspace folder. Empty: the active file. Used by the trace and compile commands. |
| `satex.trace.server` | `off`, `messages` or `verbose`: trace the language server communication. |

## Try it locally

From `editors/vscode` (needs Rust and Node):

- `npm run package:local` builds the release binary (`cargo build --release`), bundles it under `bin/<platform>-<arch>/`, compiles and writes `satex-local.vsix`.
- `npm run try` does the same, then installs the VSIX with `code --install-extension --force`.
