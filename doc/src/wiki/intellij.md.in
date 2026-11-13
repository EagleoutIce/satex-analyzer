<!-- generated from doc/src/wiki/intellij.md.in by satex-doc; do not edit -->

<p align="center"><a href="https://eagleoutice.github.io/satex-analyzer/"><img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wide.svg" alt="SaTeX: Static Analysis for (La)TeX and Friends" width="600"></a></p>

# IntelliJ plugin

The plugin in `editors/intellij/` runs `satex lsp` inside IntelliJ-based IDEs. It uses the LSP4IJ plugin as the language client, so it also works in Community editions.

It provides [diagnostics, quick fixes, hover, definitions, completion and document colors](lsp.md#capabilities). `satex.yaml` is checked against the bundled `satex.schema.json`.

## Install

1. Install LSP4IJ from the Marketplace.
2. Build the plugin with `./gradlew buildPlugin --no-daemon` in `editors/intellij/`.
3. In Settings, Plugins, use the gear, Install Plugin from Disk, and pick `build/distributions/satex-intellij-0.2.3.zip`.
4. Restart the IDE.

## Settings

Settings, Tools, SaTeX.

| Setting | Meaning |
| --- | --- |
| Path to satex | The binary. Empty uses `satex` on the `PATH`. |
| Memory limit | Passed as `--max-memory`. Empty uses satex's own limit. |

## Publishing

Not on the JetBrains Marketplace yet. Install from disk as above.
