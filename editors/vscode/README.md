# satex for VS Code

Static analysis for LaTeX and TeX documents, powered by [satex](https://github.com/EagleoutIce/satex-analyzer) and its built-in language server (`satex lsp`): diagnostics, lints and navigation for `.tex`, `.sty`, `.cls` and `.bib` files.

## Features

- Starts `satex lsp` over stdio for LaTeX, TeX and BibTeX files.
- Builds satex's LaTeX kernel caches in the background on first use, with a cancellable progress notification. The language server is never blocked by it.

## Requirements

satex uses **your installed TeX distribution** (`kpsewhich`, `pdflatex`, ...). TeX Live is **not** bundled, as it is far too large; install TeX Live, MiKTeX or MacTeX yourself. The extension warns if `kpsewhich` or `pdflatex` is not on the `PATH`.

The platform-specific packages bundle the matching `satex` binary. The universal package does not; it uses `satex` from the `PATH` (`cargo install satex`).

## Settings

| Setting | Description |
| --- | --- |
| `satex.path` | Path to the `satex` binary. Empty: bundled binary, then `satex` on the `PATH`. |
| `satex.trace.server` | `off`, `messages` or `verbose`: trace the language server communication. |
