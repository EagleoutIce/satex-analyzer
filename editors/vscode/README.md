# satex for VS Code

Static analysis for LaTeX and TeX documents, powered by [satex](https://github.com/EagleoutIce/satex-analyzer) and its built-in language server (`satex lsp`): diagnostics, lints and navigation for `.tex`, `.sty`, `.cls` and `.bib` files.

## Features

- Starts `satex lsp` over stdio for LaTeX, TeX and BibTeX files, at a lower priority and with a memory limit (`satex.server.maxMemory`).
- A satex view in the activity bar: the findings of the open documents by rule, most severe first, and the actions below.
- A status bar entry: what satex caches or analyzes (`caching pdftex kernel`), then its errors and warnings. A click opens the view. The kernel cache is built once on first use.
- Hovers are kept and answered at once, also while an edit is analyzed. The names a document uses most are prepared while satex is idle.
- With LaTeX Workshop installed, satex offers once to turn its chktex linting off, so a finding is not reported twice.
- `satex: Trace Expansion Here` (`satex.traceAtCursor`): traces what the selection, or the control sequence at the cursor, expands to at that position and shows the steps beside the editor.
- The playground's widgets open beside the editor: a code lens above a table, figure, image, drawing or tree, the light bulb at the cursor, or `satex: Open Widget…` (`satex.openWidget`). It lists the widgets at the cursor (table, image, placement, color, length, drawing, tree), then the managers (glossary, bibliography, index, theorems), the Library and the Settings. Edits go through the editor's undo. The widgets follow the color theme.
- `satex: Library` (`satex.library`): the names in force at the end of the document and what the loaded packages' manuals document. Search them, and a click inserts one at the cursor.
- `satex: Project Settings` (`satex.projectSettings`): every key of `satex.yaml`, with its description and default. An edit writes the key in place. With the YAML extension, `satex.yaml` also completes and checks against the schema.
- With another copy of SaTeX installed, it warns once and offers to uninstall the other copy (`code --uninstall-extension <id>`). Two copies start two servers and two status items.
- `satex: Compile` (`satex.compile`): builds the root document, streams the build output to the "satex build" channel and shows the findings as diagnostics.

## Requirements

satex uses **your installed TeX distribution** (`kpsewhich`, `pdflatex`, ...). TeX Live is **not** bundled, as it is far too large. Install TeX Live, MiKTeX or MacTeX yourself. The extension warns if `kpsewhich` or `pdflatex` is not on the `PATH`.

The platform-specific packages bundle the matching `satex` binary. The universal package does not. It uses `satex` from the `PATH` (`cargo install satex-analyzer`).

## Settings

| Setting | Description |
| --- | --- |
| `satex.path` | Path to the `satex` binary. Empty: bundled binary, then `satex` on the `PATH`. User settings only. |
| `satex.statusBar.enabled` | Show satex in the status bar. |
| `satex.server.maxMemory` | Most memory one analysis may hold (`2GiB`). |
| `satex.server.maxCores` | CPU cores the server may use on Linux, through its cgroup (`2`, 0 for no cap). |
| `satex.server.niceness` | Scheduling priority of the server (`10`). Higher yields more to the editor. |
| `satex.server.cgroup` | On Linux, enforce the memory and core limits in a systemd user scope (`true`). Without a user bus (containers, WSL) `ulimit -d` caps the memory instead. |
| `satex.findings.whileAnalyzing` | What the Problems view shows while an analysis runs. `hide` (default): only findings no pass can change, the rest once the run ends, and the status bar says it runs. `keep`: the last complete findings. `mark`: those, saying they may change. |
| `satex.rootFile` | Root document, relative to the workspace folder. Empty: the active file. Used by the trace and compile commands. |
| `satex.trace.server` | `off`, `messages` or `verbose`: trace the language server communication. |

## When the server does not start

Every start problem shows a message with buttons that fix it; the output "satex" logs each step and what the server says.

- **No satex binary runs here.** The extension tries `satex.path`, the bundled binary, then `satex` on the PATH, and skips one that does not run. On a musl system (Alpine) or with an older C library than the release needs, build it there with `cargo install satex-analyzer` and set `satex.path`.
- **No hard memory cap.** In a container or WSL without a systemd user bus the server is capped with `ulimit -d` instead of a scope. Where neither works, its own budget `satex.server.maxMemory` still ends an analysis that grows past it.
- **The server keeps stopping.** It is started again three times with growing pauses, then stays off with a message; `satex: Restart Server` starts it again.
- **No TeX distribution.** satex reads the packages of TeX Live, MiKTeX or MacTeX; install one on the machine (or container) the extension runs on.

## Try it locally

`cargo vscode` from the repository builds satex, bundles it and installs the extension (needs Rust, Node and VS Code's `code`). Then reload the VS Code window.

Or from `editors/vscode`:

- `npm run package:local` builds the release binary (`cargo build --release`), bundles it under `bin/<platform>-<arch>/`, compiles and writes `satex-local.vsix`.
- `npm run try` does the same, then installs the VSIX with `code --install-extension --force`.
