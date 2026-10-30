# satex for VS Code

Static analysis for LaTeX and TeX documents, powered by [satex](https://github.com/EagleoutIce/satex-analyzer) and its built-in language server (`satex lsp`): diagnostics, lints and navigation for `.tex`, `.sty`, `.cls` and `.bib` files.

## Features

- Starts `satex lsp` over stdio for LaTeX, TeX and BibTeX files, at a lower priority and with a memory limit (`satex.server.maxMemory`).
- A satex view in the activity bar: the findings of the open documents by rule, most severe first, and the actions below.
- A status bar entry: what satex caches or analyzes (`caching pdftex kernel`), then its errors and warnings; a click opens the view. The kernel cache is built once on first use.
- Hovers are kept and answered at once, also while an edit is analyzed; the names a document uses most are prepared while satex is idle.
- With LaTeX Workshop installed, satex offers once to turn its chktex linting off, so a finding is not reported twice.
- `satex: Trace Expansion Here` (`satex.traceAtCursor`): traces what the selection, or the control sequence at the cursor, expands to at that position and shows the steps beside the editor.
- `satex: Compile` (`satex.compile`): builds the root document, streams the build output to the "satex build" channel and shows the findings as diagnostics.

## Requirements

satex uses **your installed TeX distribution** (`kpsewhich`, `pdflatex`, ...). TeX Live is **not** bundled, as it is far too large; install TeX Live, MiKTeX or MacTeX yourself. The extension warns if `kpsewhich` or `pdflatex` is not on the `PATH`.

The platform-specific packages bundle the matching `satex` binary. The universal package does not; it uses `satex` from the `PATH` (`cargo install satex-analyzer`).

## Settings

| Setting | Description |
| --- | --- |
| `satex.path` | Path to the `satex` binary. Empty: bundled binary, then `satex` on the `PATH`. User settings only. |
| `satex.statusBar.enabled` | Show satex in the status bar. |
| `satex.server.maxMemory` | Most memory one analysis may hold (`2GiB`). |
| `satex.rootFile` | Root document, relative to the workspace folder. Empty: the active file. Used by the trace and compile commands. |
| `satex.trace.server` | `off`, `messages` or `verbose`: trace the language server communication. |

## Try it locally

`cargo vscode` from the repository builds satex, bundles it and installs the extension (needs Rust, Node and VS Code's `code`). Then reload the VS Code window.

Or from `editors/vscode`:

- `npm run package:local` builds the release binary (`cargo build --release`), bundles it under `bin/<platform>-<arch>/`, compiles and writes `satex-local.vsix`.
- `npm run try` does the same, then installs the VSIX with `code --install-extension --force`.
