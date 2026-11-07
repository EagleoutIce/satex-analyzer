# SaTeX for IntelliJ

IntelliJ Platform plugin for the SaTeX language server (`satex lsp`, stdio).

## Choice of LSP client

The plugin uses LSP4IJ (Red Hat, `com.redhat.devtools.lsp4ij`) as a dependency.
The platform's own LSP API is Ultimate-only (and in some IDEs paid-only), LSP4IJ also runs in Community editions.
LSP4IJ provides diagnostics, hover, completion, go to definition, code actions and document colors without more code.

## What it does

- Starts `satex lsp` (`--max-memory` from the settings) for `.tex .sty .cls .dtx .ins .ltx` and `.bib`.
- Registers two file types for these extensions. If another LaTeX plugin (TeXiFy) owns them, remove the overlap in Settings, Editor, File Types.
- Maps `satex.yaml` to the bundled `satex.schema.json` (copied from the repo root at build time).
- Settings: Tools, SaTeX. Path to the binary (empty: `satex` on the PATH), memory limit.

## Build

    ./gradlew buildPlugin --no-daemon

The zip lands in `build/distributions/`. Needs JDK 21. Test in a sandbox IDE with `./gradlew runIde`.

## Not covered (VS Code only)

Status bar, trace stepping, compile command, systemd cgroup wrapper, `satex/status` notifications.
